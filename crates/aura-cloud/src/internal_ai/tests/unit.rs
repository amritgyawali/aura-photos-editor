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
        let asked = String::from_utf8_lossy(&request.body).to_string();
        // A service that is silent the first time it is asked, and one that stays silent.
        let earlier = self
            .calls
            .lock()
            .iter()
            .filter(|call| call.method == "POST" && call.url == request.url)
            .count();
        if request.method == "POST"
            && (request.url.contains("silent.example")
                || (request.url.contains("slow-once.example") && earlier == 1))
        {
            return Err(aura_core::errors::cloud::unreachable(
                "test",
                "no answer".to_string(),
            ));
        }
        let said = |text: &str| {
            serde_json::to_vec(&serde_json::json!({"model":"vision-model","choices":[{"message":{"content":text},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":4}})).unwrap()
        };
        // The check picture: most models read it, one cannot see, one is over its quota.
        let busy = request.url.contains("busy.example") || asked.contains("\"a-vision\"");
        if request.method == "POST" && !fail && (busy || asked.contains("left half")) {
            return Ok(HttpResponse {
                status: if busy { 429 } else { 200 },
                headers: vec![],
                body: if request.url.contains("blind.example") {
                    said("Red and green.")
                } else {
                    said("Yellow, blue.")
                },
            });
        }
        let body = if fail {
            b"secret-echo-that-must-not-escape".to_vec()
        } else if request.method == "GET" && request.url.contains("two.example") {
            serde_json::to_vec(&serde_json::json!({"data":[
                {"id":"text-model","pricing":{"prompt":"0.000001","completion":"0.000002"}},
                {"id":"a-vision","architecture":{"input_modalities":["text","image"]},"pricing":{"prompt":"0.000001","completion":"0.000002"}},
                {"id":"b-vision","architecture":{"input_modalities":["text","image"]},"pricing":{"prompt":"0.000001","completion":"0.000002"}}]})).unwrap()
        } else if request.method == "GET" && request.url.contains("integrate.api.nvidia.com") {
            serde_json::to_vec(&serde_json::json!({"data":[{"id":"nvidia/text-model"},{"id":"meta/llama-3.2-90b-vision-instruct"},{"id":"meta/llama-3.2-11b-vision-instruct"}]})).unwrap()
        } else if request.method == "GET" && request.url.contains("api.groq.com") {
            serde_json::to_vec(&serde_json::json!({"data":[{"id":"openai/gpt-oss-120b"},{"id":"qwen/qwen3.8-27b"}]})).unwrap()
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
fn a_photo_route_is_offered_only_after_it_read_the_check_picture() {
    let keys = MemoryKeyStore::new();
    let transport = FakeTransport::default();
    let profiles = import(
        "SEES_API_KEY=test-key-one\nSEES_BASE_URL=https://good.example/v1\nSEES_MODEL=vision-model\n\
         BLIND_API_KEY=test-key-two\nBLIND_BASE_URL=https://blind.example/v1\nBLIND_MODEL=vision-model\n\
         BUSY_API_KEY=test-key-three\nBUSY_BASE_URL=https://busy.example/v1\nBUSY_MODEL=vision-model",
        &keys,
        &transport,
    )
    .unwrap();
    let status = |id: &str| profiles.iter().find(|p| p.id == id).unwrap();
    assert!(status("SEES").available() && status("SEES").vision);
    // Listed as image-capable and priced, and still not a route: it did not read the picture.
    assert_eq!(status("BLIND").status, "check_image_not_read");
    assert_eq!(status("BUSY").status, "check_http_429");
    assert!(!status("BLIND").available() && !status("BUSY").available());
    // The picture is generated here: every check carried the same small image and no photograph.
    let calls = transport.calls.lock();
    let checks: Vec<_> = calls.iter().filter(|c| c.method == "POST").collect();
    assert_eq!(checks.len(), 3);
    assert!(checks
        .iter()
        .all(|c| String::from_utf8_lossy(&c.body).contains("data:image/jpeg;base64,")));
    assert!(saw_the_card("Yellow, then blue") && !saw_the_card("blue and yellow"));
    assert!(!saw_the_card("I cannot see images"));
}
#[test]
fn a_check_that_met_silence_is_tried_once_more_and_a_refusal_is_not() {
    let keys = MemoryKeyStore::new();
    let transport = FakeTransport::default();
    let profiles = import(
        "ONCE_API_KEY=test-key-one
ONCE_BASE_URL=https://slow-once.example/v1
ONCE_MODEL=vision-model
         NEVER_API_KEY=test-key-two
NEVER_BASE_URL=https://silent.example/v1
NEVER_MODEL=vision-model
         BUSY_API_KEY=test-key-three
BUSY_BASE_URL=https://busy.example/v1
BUSY_MODEL=vision-model",
        &keys,
        &transport,
    )
    .unwrap();
    let route = |id: &str| profiles.iter().find(|p| p.id == id).unwrap();
    assert!(route("ONCE").available());
    assert_eq!(route("NEVER").status, "check_no_answer_in_time");
    assert_eq!(route("BUSY").status, "check_http_429");
    let asked = |host: &str| {
        transport
            .calls
            .lock()
            .iter()
            .filter(|call| call.method == "POST" && call.url.contains(host))
            .count()
    };
    assert_eq!(
        (
            asked("slow-once.example"),
            asked("silent.example"),
            asked("busy.example")
        ),
        (2, 2, 1)
    );
}
#[test]
fn the_first_alternative_that_reads_the_picture_becomes_the_photo_route() {
    let keys = MemoryKeyStore::new();
    let profiles = import(
        "TWO_API_KEY=test-key-one\nTWO_BASE_URL=https://two.example/v1\nTWO_MODEL=text-model",
        &keys,
        &FakeTransport::default(),
    )
    .unwrap();
    let text = profiles.iter().find(|p| p.id == "TWO").unwrap();
    let photo = profiles.iter().find(|p| p.id == "TWO:vision").unwrap();
    assert!(text.available() && !text.vision);
    // `a-vision` sorts first and is over its quota; `b-vision` answered.
    assert_eq!(photo.model, "b-vision");
    assert!(photo.available() && photo.vision);
    assert_eq!(profiles.len(), 2);
}
#[test]
fn catalogues_that_omit_modalities_still_yield_their_documented_image_models() {
    let keys = MemoryKeyStore::new();
    let profiles = import(
        "NIM_API_KEY=test-key-one\nNIM_BASE_URL=https://integrate.api.nvidia.com/v1\nNIM_MODEL=nvidia/text-model\n\
         GROQ_API_KEY=test-key-two\nGROQ_BASE_URL=https://api.groq.com/openai/v1\nGROQ_MODEL=openai/gpt-oss-120b",
        &keys,
        &FakeTransport::default(),
    )
    .unwrap();
    let route = |id: &str| profiles.iter().find(|p| p.id == id).unwrap();
    // The smaller model is asked first: it answers in seconds where the larger timed out.
    assert_eq!(
        route("NIM:vision").model,
        "meta/llama-3.2-11b-vision-instruct"
    );
    assert_eq!(route("GROQ:vision").model, "qwen/qwen3.8-27b");
    assert!(route("NIM:vision").available() && route("GROQ:vision").available());
    assert_eq!(route("GROQ:vision").input_usd_per_mtok, Some(0.80));
    assert!(!route("GROQ").vision && !route("NIM").vision);
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
fn a_route_rests_for_as_long_as_its_failure_says() {
    let note = |code: &str, pause: Option<u64>| {
        serde_json::json!({ "errorCode": code, "retryAfterMs": pause }).to_string()
    };
    let rest = |code: &str, pause: Option<u64>| {
        crate::gateway::rest_after_for_tests(Some(&note(code, pause)))
    };
    // No answer in time: the same a minute later.
    assert_eq!(rest("AURA-CLOUD-6003", None), 600_000);
    // A per-minute allowance says when it is free again, and that is when it is asked again.
    assert_eq!(rest("AURA-CLOUD-6004", Some(18_100)), 18_100);
    assert_eq!(rest("AURA-CLOUD-6004", Some(3_000)), 5_000);
    // A daily allowance asks for hours; the day's import check will have dropped it by then.
    assert_eq!(rest("AURA-CLOUD-6004", Some(40_000_000)), 600_000);
    // The one or two seconds filled in when no pause was named are not a pause.
    assert_eq!(rest("AURA-CLOUD-6004", Some(1_000)), 60_000);
    // An answer that could not be used after the repair: the model will not learn in a minute.
    assert_eq!(rest("AURA-CLOUD-6005", None), 600_000);
    assert_eq!(rest("AURA-CLOUD-6002", None), 60_000);
    assert_eq!(crate::gateway::rest_after_for_tests(None), 60_000);
}
#[test]
fn a_provider_that_names_no_pause_rests_a_minute_and_is_then_asked_again() {
    let transport = Arc::new(FakeTransport::default());
    let clock = FixedClock::at(time::OffsetDateTime::UNIX_EPOCH);
    let cloud = gateway(
        &[
            profile("BUSY", "busy.example", true),
            profile("GOOD", "good.example", true),
        ],
        CloudPolicy::enabled(),
        true,
        transport.clone(),
        Arc::new(MemoryAudit::new()),
        clock.clone(),
    );
    let project = project();
    let cancel = CancelToken::new();
    let ask = |photo: &str| {
        cloud
            .run(
                &Task { images: true },
                &photo.to_string(),
                &CallContext {
                    project: &project,
                    decision_ref: None,
                    cancel: &cancel,
                },
            )
            .unwrap()
            .source
    };
    let to = |host: &str| {
        transport
            .calls
            .lock()
            .iter()
            .filter(|call| call.url.contains(host))
            .count()
    };
    assert_eq!(ask("one"), Source::Cloud);
    assert_eq!((to("busy.example"), to("good.example")), (1, 1));
    // Half a minute later it is still resting: the second photograph does not wait for it.
    clock.advance_ms(30_000);
    assert_eq!(ask("two"), Source::Cloud);
    assert_eq!((to("busy.example"), to("good.example")), (1, 2));
    clock.advance_ms(31_000);
    assert_eq!(ask("three"), Source::Cloud);
    assert_eq!((to("busy.example"), to("good.example")), (2, 3));
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
