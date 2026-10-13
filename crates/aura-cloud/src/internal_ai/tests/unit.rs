//! Synthetic fixtures only; this module is compiled exclusively under cfg(test).
use super::*;
use crate::audit::{AuditSink, MemoryAudit};
use crate::budget::{CostGovernor, MemoryBudget};
use crate::cache::MemoryCache;
use crate::contract::cloud::{CloudTask, ImagePart, PromptSpec, Source, Validate};
use crate::gateway::{CallContext, CloudAiGateway, CloudPolicy};
use crate::keys::MemoryKeyStore;
use crate::tasks::Scored;
use aura_core::clock::FixedClock;
use aura_core::progress::CancelToken;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Default)]
struct FakeTransport {
    calls: parking_lot::Mutex<Vec<HttpRequest>>,
    failed: AtomicUsize,
}
impl Transport for FakeTransport {
    fn send(&self, request: &HttpRequest, _: Duration) -> AuraResult<HttpResponse> {
        self.calls.lock().push(request.clone());
        let fail = request.url.contains("bad.example");
        if fail {
            self.failed.fetch_add(1, Ordering::Relaxed);
        }
        let body = if fail {
            b"secret-echo-that-must-not-escape".to_vec()
        } else if request.method == "GET"
            && request.url.contains("generativelanguage.googleapis.com")
        {
            serde_json::to_vec(&serde_json::json!({"data":[{"id":"models/gemini-3.8-flash"},{"id":"models/gemini-3.6-flash"},{"id":"models/gemini-3.8-flash-tts"}]})).unwrap()
        } else if request.method == "GET" {
            serde_json::to_vec(&serde_json::json!({"data":[{"id":"vision-model","architecture":{"input_modalities":["text","image"]},"pricing":{"prompt":"0.000001","completion":"0.000002"}}]})).unwrap()
        } else if request.url.contains("invalid.example") {
            serde_json::to_vec(&serde_json::json!({"model":"vision-model","choices":[{"message":{"content":"This answer has no valid JSON"},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":20}})).unwrap()
        } else {
            serde_json::to_vec(&serde_json::json!({"model":"vision-model","choices":[{"message":{"content":"{\"confidence\":0.8,\"reasons\":[\"Measured result\"]}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":20}})).unwrap()
        };
        Ok(HttpResponse {
            status: if fail { 401 } else { 200 },
            headers: vec![],
            body,
        })
    }
    fn name(&self) -> &'static str {
        "cassette"
    }
}

#[test]
fn invalid_billed_completions_count_against_the_budget_without_billing_the_local_answer() {
    let transport = Arc::new(FakeTransport::default());
    let audit = Arc::new(MemoryAudit::new());
    let cloud = gateway(
        &[profile("INVALID", "invalid.example", true)],
        CloudPolicy::enabled(),
        true,
        transport.clone(),
        audit.clone(),
        FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
    );
    let project = project();
    let cancel = CancelToken::new();
    let result = cloud
        .run(
            &Task { images: true },
            &"photo".to_string(),
            &CallContext {
                project: &project,
                decision_ref: None,
                cancel: &cancel,
            },
        )
        .unwrap();
    assert_eq!(result.source, Source::LocalFallback);
    assert_eq!(transport.calls.lock().len(), 2);
    assert!(result.cost_usd.abs() < f32::EPSILON);
    let spent = audit.spent_usd(&project.to_string()).unwrap();
    assert!(spent > 0.0);
    assert!(
        (spent
            - cloud
                .governor()
                .store()
                .state(&project.to_string())
                .unwrap()
                .spent_usd)
            .abs()
            < 1e-9
    );
    let rows = audit.recent(&project.to_string(), 20).unwrap();
    assert!(rows
        .iter()
        .any(|r| r.status == "invalid_response" && r.tokens_in == 200));
}
#[derive(Debug, serde::Deserialize)]
struct Answer {
    confidence: f32,
    reasons: Vec<String>,
}
impl Validate for Answer {
    fn validate(&self) -> Result<(), String> {
        Ok(())
    }
}
impl Scored for Answer {
    fn confidence(&self) -> f32 {
        self.confidence
    }
    fn reasons(&self) -> &[String] {
        &self.reasons
    }
}
struct Task {
    images: bool,
}
impl CloudTask for Task {
    const NAME: &'static str = "internal_route_test";
    const VERSION: u16 = 1;
    type Input = String;
    type Output = Answer;
    fn prompt(&self, input: &String) -> PromptSpec {
        let prompt =
            PromptSpec::new("Return JSON with confidence and reasons", input).with_max_tokens(100);
        if self.images {
            prompt.with_images(vec![ImagePart {
                bytes: vec![1, 2, 3],
                media_type: "image/jpeg".to_string(),
                width: 16,
                height: 16,
                content_hash: "image-hash".to_string(),
            }])
        } else {
            prompt
        }
    }
    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","required":["confidence","reasons"],"properties":{"confidence":{"type":"number"},"reasons":{"type":"array","items":{"type":"string"}}}}"#
    }
    fn local_fallback(&self, _: &String) -> AuraResult<Answer> {
        Ok(Answer {
            confidence: 0.2,
            reasons: vec!["Local".to_string()],
        })
    }
}
fn profile(id: &str, host: &str, vision: bool) -> Profile {
    Profile {
        id: id.to_string(),
        key_profile: None,
        endpoint: format!("https://{host}/v1"),
        model: format!("vision-model-{id}"),
        vision,
        input_usd_per_mtok: Some(1.0),
        output_usd_per_mtok: Some(2.0),
        status: "catalogue_ready".to_string(),
    }
}
fn gateway(
    profiles: &[Profile],
    policy: CloudPolicy,
    allow: bool,
    transport: Arc<FakeTransport>,
    audit: Arc<MemoryAudit>,
    clock: Arc<FixedClock>,
) -> CloudAiGateway {
    let keys = Arc::new(MemoryKeyStore::new());
    for p in profiles {
        keys.store(&p.account(), &SecretKey::new(format!("test-key-{}", p.id)))
            .unwrap();
    }
    let consent: Arc<dyn aura_core::contract::consent::ConsentGate> = if allow {
        Arc::new(aura_catalog::consent::AlwaysConsent)
    } else {
        Arc::new(aura_catalog::consent::NeverConsent)
    };
    let wire: Arc<dyn Transport> = transport;
    CloudAiGateway::new(
        profile("legacy", "unused.example", true).client(Arc::clone(&wire)),
        keys,
        Arc::new(MemoryCache::new()),
        audit,
        Arc::new(CostGovernor::new(Arc::new(MemoryBudget::new()))),
        consent,
        clock,
        policy,
    )
    .with_internal_routes(profiles, &wire)
}
fn project() -> aura_core::ProjectId {
    aura_core::ProjectId::from_uuid(uuid::Uuid::nil())
}

#[test]
fn models_above_the_task_ceiling_do_not_hide_an_affordable_fallback() {
    let mut expensive = profile("EXPENSIVE", "expensive.example", true);
    expensive.output_usd_per_mtok = Some(1000.0);
    let profiles = vec![
        expensive.clone(),
        expensive.clone(),
        expensive,
        profile("GOOD", "good.example", true),
    ];
    let transport = Arc::new(FakeTransport::default());
    let cloud = gateway(
        &profiles,
        CloudPolicy::enabled(),
        true,
        transport.clone(),
        Arc::new(MemoryAudit::new()),
        FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
    );
    let project = project();
    let cancel = CancelToken::new();
    let answer = cloud
        .run(
            &Task { images: true },
            &"photo".to_string(),
            &CallContext {
                project: &project,
                decision_ref: None,
                cancel: &cancel,
            },
        )
        .unwrap();
    assert_eq!(answer.source, Source::Cloud);
    assert_eq!(transport.calls.lock().len(), 1);
}

#[test]
fn file_parsing_is_data_only_and_errors_never_quote_values() {
    let data="FIRST_API_KEY=secret-one\nFIRST_BASE_URL=https://same.example/v1\nFIRST_MODEL=vision-model\nSECOND_API_KEY=secret-two\nSECOND_BASE_URL=https://same.example/v1\nSECOND_MODEL=other";
    let parsed = parse(data).unwrap();
    assert_ne!(parsed[0].0.account(), parsed[1].0.account());
    let encoded = serde_json::to_string(&parsed[0].0).unwrap();
    assert!(!encoded.contains("secret"));
    for invalid in [
        format!("{data}\nFIRST_MODEL=secret-value"),
        data.replace(
            "https://same.example/v1",
            "https://same.example/v1?key=secret-value",
        ),
        data.replace("vision-model", "$(secret-value)"),
        "# heading\nexecute shell secret-value".to_string(),
    ] {
        let error = parse(&invalid).unwrap_err();
        assert!(!format!("{error:?}").contains("secret-value"));
    }
}

#[test]
fn long_uppercase_vendor_model_names_are_valid_but_key_shaped_names_are_not() {
    let config="WANDB_API_KEY=test-secret-one\nWANDB_BASE_URL=https://api.inference.wandb.ai/v1\nWANDB_MODEL=nvidia/NVIDIA-Nemotron-3-Ultra-550B-A55B";
    assert!(parse(config).is_ok());
    assert!(
        parse(&config.replace("nvidia/NVIDIA-Nemotron-3-Ultra-550B-A55B", "sk-private-key"))
            .is_err()
    );
    let mut rotated = parse(config).unwrap().remove(0).0;
    let first = rotated.account();
    rotated.endpoint = "https://other.example/v1".to_string();
    assert_ne!(first, rotated.account());
}

#[test]
fn google_catalogue_model_namespace_matches_its_chat_model_name() {
    let mut google = profile("GOOGLE_AISTUDIO", "unused.example", false);
    google.endpoint = "https://generativelanguage.googleapis.com/v1beta/openai".to_string();
    google.model = "gemini-3.8-flash".to_string();
    google.input_usd_per_mtok = None;
    google.output_usd_per_mtok = None;
    apply_catalogue(
        &mut google,
        &serde_json::json!({"data":[{"id":"models/gemini-3.8-flash"}]}),
    );
    assert!(google.vision && google.available());
}
#[test]
fn a_busy_google_image_model_retains_a_distinct_documented_image_fallback() {
    let keys = MemoryKeyStore::new();
    let profiles = import(
        "GOOGLE_API_KEY=test-google-key\nGOOGLE_BASE_URL=https://generativelanguage.googleapis.com/v1beta/openai\nGOOGLE_MODEL=gemini-3.8-flash",
        &keys,
        &FakeTransport::default(),
    )
    .unwrap();
    assert_eq!(profiles.len(), 2);
    let primary = profiles.iter().find(|p| p.id == "GOOGLE").unwrap();
    let alternative = profiles.iter().find(|p| p.id == "GOOGLE:vision").unwrap();
    assert_eq!(primary.model, "gemini-3.8-flash");
    assert_eq!(alternative.model, "gemini-3.6-flash");
    assert!(primary.vision && alternative.vision && alternative.available());
    assert_eq!(primary.account(), alternative.account());
    assert_eq!(alternative.input_usd_per_mtok, Some(1.5));
    assert_eq!(alternative.output_usd_per_mtok, Some(7.5));
}
#[test]
fn importing_stores_keys_separately_and_discovers_capabilities_and_prices() {
    let keys = MemoryKeyStore::new();
    let transport = FakeTransport::default();
    let profiles=import("FIRST_API_KEY=test-key-one\nFIRST_BASE_URL=https://good.example/custom/openai\nFIRST_MODEL=vision-model",&keys,&transport).unwrap();
    assert_eq!(profiles.len(), 1);
    assert!(profiles[0].available() && profiles[0].vision);
    assert_eq!(
        keys.load(&profiles[0].account()).unwrap().unwrap().expose(),
        "test-key-one"
    );
    let request = profiles[0]
        .client(Arc::new(FakeTransport::default()))
        .provider()
        .probe(&SecretKey::new("test".to_string()))
        .unwrap();
    assert_eq!(
        request.url,
        "https://good.example/custom/openai/chat/completions"
    );
}
#[test]
fn authentication_failure_uses_the_next_models_own_key_and_audit_identity() {
    let transport = Arc::new(FakeTransport::default());
    let audit = Arc::new(MemoryAudit::new());
    let clock = FixedClock::at(time::OffsetDateTime::UNIX_EPOCH);
    let cloud = gateway(
        &[
            profile("BAD", "bad.example", true),
            profile("GOOD", "good.example", true),
        ],
        CloudPolicy::enabled(),
        true,
        transport.clone(),
        audit.clone(),
        clock,
    );
    let project = project();
    let cancel = CancelToken::new();
    let result = cloud
        .run(
            &Task { images: true },
            &"photo one".to_string(),
            &CallContext {
                project: &project,
                decision_ref: None,
                cancel: &cancel,
            },
        )
        .unwrap();
    assert_eq!(result.source, Source::Cloud);
    let calls = transport.calls.lock();
    assert_eq!(calls.len(), 2);
    assert!(calls[0]
        .headers
        .iter()
        .any(|(_, v)| v == "Bearer test-key-BAD"));
    assert!(calls[1]
        .headers
        .iter()
        .any(|(_, v)| v == "Bearer test-key-GOOD"));
    let rows = audit.recent(&project.to_string(), 10).unwrap();
    assert!(rows
        .iter()
        .any(|r| r.provider == "BAD" && r.source == Source::LocalFallback));
    assert!(rows
        .iter()
        .any(|r| r.provider == "GOOD" && r.source == Source::Cloud));
    let request = calls[0].clone();
    drop(calls);
    let safe = SafeTransport(transport.clone());
    let reply = safe.send(&request, Duration::from_secs(1)).unwrap();
    assert!(reply.body.is_empty());
}
#[test]
fn image_requests_skip_text_models_and_policy_or_consent_prevents_all_network() {
    for (policy, allow, expected) in [
        (CloudPolicy::enabled(), true, 1),
        (CloudPolicy::default(), true, 0),
        (CloudPolicy::enabled(), false, 0),
    ] {
        let transport = Arc::new(FakeTransport::default());
        let cloud = gateway(
            &[
                profile("TEXT", "text.example", false),
                profile("VISION", "vision.example", true),
            ],
            policy,
            allow,
            transport.clone(),
            Arc::new(MemoryAudit::new()),
            FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
        );
        let project = project();
        let cancel = CancelToken::new();
        cloud
            .run(
                &Task { images: true },
                &"photo".to_string(),
                &CallContext {
                    project: &project,
                    decision_ref: None,
                    cancel: &cancel,
                },
            )
            .unwrap();
        assert_eq!(transport.calls.lock().len(), expected);
        assert!(transport
            .calls
            .lock()
            .iter()
            .all(|r| !r.url.contains("text.example")));
    }
}
#[test]
fn fallback_is_bounded_to_three_routes_and_cancel_never_calls_a_provider() {
    let transport = Arc::new(FakeTransport::default());
    let profiles: Vec<_> = (0..8)
        .map(|i| profile(&format!("BAD{i}"), "bad.example", true))
        .collect();
    let cloud = gateway(
        &profiles,
        CloudPolicy::enabled(),
        true,
        transport.clone(),
        Arc::new(MemoryAudit::new()),
        FixedClock::at(time::OffsetDateTime::UNIX_EPOCH),
    );
    let project = project();
    let cancel = CancelToken::new();
    assert_eq!(
        cloud
            .run(
                &Task { images: true },
                &"photo".to_string(),
                &CallContext {
                    project: &project,
                    decision_ref: None,
                    cancel: &cancel
                }
            )
            .unwrap()
            .source,
        Source::LocalFallback
    );
    assert_eq!(transport.calls.lock().len(), 3);
    cancel.cancel();
    assert!(cloud
        .run(
            &Task { images: true },
            &"another".to_string(),
            &CallContext {
                project: &project,
                decision_ref: None,
                cancel: &cancel
            }
        )
        .is_err());
    assert_eq!(transport.calls.lock().len(), 3);
}
#[test]
fn explicit_quality_feedback_changes_daily_order_but_not_within_the_same_day() {
    let audit = Arc::new(MemoryAudit::new());
    let clock = FixedClock::at(time::OffsetDateTime::UNIX_EPOCH);
    let cloud = gateway(
        &[
            profile("FIRST", "first.example", true),
            profile("SECOND", "second.example", true),
        ],
        CloudPolicy::enabled(),
        true,
        Arc::new(FakeTransport::default()),
        audit.clone(),
        clock.clone(),
    );
    let project = project().to_string();
    assert_eq!(cloud.ranked_routes(&project), vec![0, 1]);
    let mut record = crate::audit::CallRecord::local(
        uuid::Uuid::nil(),
        "internal_ai_feedback",
        1,
        "user_feedback",
    )
    .in_project(&project);
    record.provider = "SECOND".to_string();
    record.status = "accepted".to_string();
    audit.record(&record);
    assert_eq!(cloud.ranked_routes(&project), vec![0, 1]);
    clock.advance_ms(24 * 60 * 60 * 1000);
    assert_eq!(cloud.ranked_routes(&project), vec![1, 0]);
}
