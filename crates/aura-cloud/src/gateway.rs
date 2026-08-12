//! The gateway: one door, seven steps, and no way round it.
//!
//! ```text
//! caller
//!   |  CloudTask::<Name>{ typed input }
//!   v
//! 1 policy      offline mode, per-project switch, consent, region pin
//! 2 render      the task builds its prompt and its derivative images
//! 3 inspect     payload limits and the key-leak guard, enforced here
//! 4 cache       (task, version, prompt hash, image hashes, model)
//! 5 govern      tier choice, pre-call estimate, three ceilings
//! 6 call        retry, backoff, breaker; validate; one repair; validate
//! 7 settle      charge, audit row, cache write
//!   v
//! typed output OR the task's local fallback, flagged source='local_fallback'
//! ```
//!
//! ## Why the order differs from the phase document
//!
//! Section 3 of the phase document lists the cache lookup as step 2 and the
//! payload build as step 3. That order cannot be implemented, because the cache
//! key contains the content hashes of the images, and there are no content hashes
//! until the images have been built. Building first costs about ten milliseconds
//! of JPEG encoding on a cache hit; keying the cache on anything less than the
//! actual bytes would cost correctness, because two different frames would then
//! share an answer. The build happens first and the inversion is deliberate.
//!
//! ## Why `Scored` is a bound rather than a field
//!
//! Invariant 2: "every AI decision carries `confidence` (0-1) and `reasons[]`. A
//! decision without an explanation is a bug." [`crate::tasks::Scored`] is
//! required of every task's `Output`, so a task whose answer cannot explain
//! itself does not compile. That is a stronger guarantee than a reviewer checking
//! for it, and it costs one trait bound.
//!
//! ## Every path writes an audit row
//!
//! Including the ones where nothing was sent. A refusal, a cache hit and a
//! fallback each write a row with `source` and `fallback_reason` set, because
//! acceptance criterion 4 says *any* AI decision must be traceable, and the
//! decisions worth tracing are usually the ones that never reached a model.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::consent::{ConsentGate, DataClass, Permit};
use aura_core::errors::cloud::{cancelled, consent_missing, disabled, no_key, payload_refused};
use aura_core::progress::CancelToken;
use aura_core::{AuraError, AuraResult, ProjectId};

use crate::audit::{call_id, AuditSink, CallRecord};
use crate::budget::{CostGovernor, PRICE_TABLE_VERSION};
use crate::cache::{self, CacheEntry, ResponseCache};
use crate::contract::cloud::{CloudResult, CloudTask, PromptSpec, Source, UNKNOWN_ENUM_PENALTY};
use crate::fallback::{FallbackLedger, FallbackReason};
use crate::keys::{KeyStore, SecretKey};
use crate::payload::MAX_SHEET_LONG_EDGE;
use crate::provider::{CloudRequest, ProviderClient, RepairTurn};
use crate::repair::RepairOutcome;
use crate::schema::Schema;
use crate::tasks::Scored;

/// What is switched on for one installation and one project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudPolicy {
    /// The global switch. When true the crate is inert and no key is read.
    pub offline_studio_mode: bool,
    /// The per-project switch. Off for a new project, by design.
    pub project_enabled: bool,
    /// Blur faces in every derivative before it is uploaded.
    pub blur_faces: bool,
    /// Largest long edge any uploaded image may have.
    ///
    /// [`crate::payload::MAX_SHEET_LONG_EDGE`] rather than the 768 px crop limit,
    /// because a contact sheet is a grid of crops and is legitimately larger than
    /// one of them. The 768 px rule is enforced per tile, inside the builder.
    pub max_long_edge: u32,
    /// Largest total payload, in bytes.
    pub max_payload_bytes: usize,
}

impl Default for CloudPolicy {
    /// Everything off. A new installation makes no network calls until a human
    /// turns cloud AI on for a specific job.
    fn default() -> Self {
        Self {
            offline_studio_mode: false,
            project_enabled: false,
            blur_faces: false,
            max_long_edge: MAX_SHEET_LONG_EDGE,
            max_payload_bytes: crate::payload::MAX_PAYLOAD_BYTES,
        }
    }
}

impl CloudPolicy {
    /// A policy with cloud AI enabled for the project.
    #[must_use]
    pub const fn enabled() -> Self {
        Self {
            offline_studio_mode: false,
            project_enabled: true,
            blur_faces: false,
            max_long_edge: MAX_SHEET_LONG_EDGE,
            max_payload_bytes: crate::payload::MAX_PAYLOAD_BYTES,
        }
    }

    /// Why this policy forbids a call, or `None` when it does not.
    #[must_use]
    pub const fn refusal(&self) -> Option<&'static str> {
        if self.offline_studio_mode {
            return Some("offline_studio_mode");
        }
        if !self.project_enabled {
            return Some("project_switch_off");
        }
        None
    }
}

/// One call's context: whose wedding, which decision, and the stop button.
#[derive(Debug, Clone, Copy)]
pub struct CallContext<'a> {
    /// The project the decision belongs to.
    pub project: &'a ProjectId,
    /// The decision row this call will justify, when the caller has one.
    pub decision_ref: Option<&'a str>,
    /// Checked before the provider call and before each agent step.
    pub cancel: &'a CancelToken,
}

/// The governed door to every cloud model.
#[derive(Debug)]
pub struct CloudAiGateway {
    client: Arc<ProviderClient>,
    keys: Arc<dyn KeyStore>,
    cache: Arc<dyn ResponseCache>,
    audit: Arc<dyn AuditSink>,
    governor: Arc<CostGovernor>,
    consent: Arc<dyn ConsentGate>,
    ledger: Arc<FallbackLedger>,
    clock: Arc<dyn Clock>,
    policy: CloudPolicy,
    /// Makes call identifiers unique within a run without a random number.
    sequence: AtomicU64,
}

impl CloudAiGateway {
    /// Assemble a gateway from its parts.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        client: Arc<ProviderClient>,
        keys: Arc<dyn KeyStore>,
        cache: Arc<dyn ResponseCache>,
        audit: Arc<dyn AuditSink>,
        governor: Arc<CostGovernor>,
        consent: Arc<dyn ConsentGate>,
        clock: Arc<dyn Clock>,
        policy: CloudPolicy,
    ) -> Self {
        Self {
            client,
            keys,
            cache,
            audit,
            governor,
            consent,
            ledger: Arc::new(FallbackLedger::new()),
            clock,
            policy,
            sequence: AtomicU64::new(0),
        }
    }

    /// Replace the policy, for the settings panel's switches.
    #[must_use]
    pub const fn with_policy(mut self, policy: CloudPolicy) -> Self {
        self.policy = policy;
        self
    }

    /// The policy in force.
    #[must_use]
    pub const fn policy(&self) -> CloudPolicy {
        self.policy
    }

    /// What fell back, and how often.
    #[must_use]
    pub fn ledger(&self) -> &Arc<FallbackLedger> {
        &self.ledger
    }

    /// The cost governor, for the spend meter.
    #[must_use]
    pub fn governor(&self) -> &Arc<CostGovernor> {
        &self.governor
    }

    /// The audit trail, for the viewer.
    #[must_use]
    pub fn audit(&self) -> &Arc<dyn AuditSink> {
        &self.audit
    }

    /// The response cache, for the settings panel.
    #[must_use]
    pub fn cache(&self) -> &Arc<dyn ResponseCache> {
        &self.cache
    }

    /// The provider client, for the Check button and the breaker state.
    #[must_use]
    pub fn client(&self) -> &Arc<ProviderClient> {
        &self.client
    }

    /// Ask one task one question.
    ///
    /// This never fails for an ordinary cloud reason. Every refusal, outage,
    /// budget stop and malformed answer becomes the task's local fallback with
    /// `source = local_fallback`. It returns an error only when the *local
    /// fallback itself* fails, which means the input was unusable, and when the
    /// photographer cancelled.
    ///
    /// # Errors
    ///
    /// Whatever `local_fallback` raised, or `AURA-CLOUD-6013` on cancellation.
    pub fn run<T>(
        &self,
        task: &T,
        input: &T::Input,
        context: &CallContext<'_>,
    ) -> AuraResult<CloudResult<T::Output>>
    where
        T: CloudTask,
        T::Output: Scored,
    {
        let started_ms = self.clock.monotonic_ms();
        let project = context.project.to_string();

        if context.cancel.is_cancelled() {
            return Err(cancelled(format!("{} was stopped before it ran", T::NAME)));
        }

        // 1. Policy. Nothing is built, hashed or read before this passes.
        if let Some(reason) = self.policy.refusal() {
            return self.fall_back(
                task,
                input,
                &project,
                context,
                &disabled(reason),
                started_ms,
            );
        }
        if let Err(err) = self.check_consent(context.project) {
            return self.fall_back(task, input, &project, context, &err, started_ms);
        }

        // 2. Render. The task builds its own prompt and its own derivatives.
        let prompt = task.prompt(input);

        // 3. Inspect. The limits are enforced here as well as in the builder,
        //    because a task could construct an `ImagePart` by hand.
        if let Err(err) = self.inspect(&prompt) {
            return self.fall_back(task, input, &project, context, &err, started_ms);
        }

        let schema = match Schema::parse(task.output_schema()) {
            Ok(schema) => schema,
            // An unparseable schema is a programming error in the task, caught by
            // that task's own tests. At runtime it degrades like anything else.
            Err(err) => {
                return self.fall_back(task, input, &project, context, &err, started_ms);
            }
        };

        // 5 (before 4, because the cache key contains the model). Choose a tier.
        let config = self.client.provider().config().clone();
        let plan = match self.governor.plan(
            T::NAME,
            &project,
            &config,
            &prompt,
            f64::from(task.max_cost_usd()),
        ) {
            Ok(plan) => plan,
            Err(err) => {
                return self.fall_back(task, input, &project, context, &err, started_ms);
            }
        };

        let key = cache::key(T::NAME, T::VERSION, &prompt, &plan.model.model);
        let id = call_id(&key, self.sequence.fetch_add(1, Ordering::Relaxed));

        // 4. Cache.
        if let Some(hit) = self.try_cache::<T>(
            &key, &schema, &prompt, &plan, id, &project, context, started_ms,
        ) {
            return Ok(hit);
        }

        // The key is read as late as possible, so a run that was going to be
        // refused, cached or over budget never touches the credential store.
        let account = config.kind.account();
        let secret = match self.keys.load(&account) {
            Ok(Some(secret)) => secret,
            Ok(None) => {
                return self.fall_back(
                    task,
                    input,
                    &project,
                    context,
                    &no_key(config.kind.as_str()),
                    started_ms,
                );
            }
            Err(err) => {
                return self.fall_back(task, input, &project, context, &err, started_ms);
            }
        };

        if context.cancel.is_cancelled() {
            return Err(cancelled(format!(
                "{} was stopped before its call",
                T::NAME
            )));
        }

        // 6. Call, validate, repair once, validate again.
        let attempt = self.ask::<T>(&prompt, &plan.model.model, &secret, &schema);

        let (validated, response, outcome) = match attempt {
            Ok(complete) => complete,
            Err(err) => {
                return self.fall_back(task, input, &project, context, &err, started_ms);
            }
        };

        // 7. Settle: charge what it really cost, write the row, fill the cache.
        Ok(self.settle::<T>(
            &Settlement {
                key: &key,
                prompt: &prompt,
                plan: &plan,
                id,
                project: &project,
                context,
                started_ms,
                outcome,
            },
            validated,
            response,
        ))
    }

    /// Everything that happens after a good answer: charge, cache, record.
    ///
    /// Every failure in here is logged and swallowed. The photographer has an
    /// answer they have already paid for; losing it because a counter could not
    /// be written would be the worst possible trade.
    fn settle<T>(
        &self,
        settlement: &Settlement<'_>,
        validated: crate::validate::Validated<T::Output>,
        response: crate::provider::CloudResponse,
    ) -> CloudResult<T::Output>
    where
        T: CloudTask,
        T::Output: Scored,
    {
        let Settlement {
            key,
            prompt,
            plan,
            id,
            project,
            context,
            started_ms,
            outcome,
        } = *settlement;

        let cost = plan.model.price(response.tokens_in, response.tokens_out);
        if let Err(err) = self.governor.charge(project, cost, plan.downgraded) {
            tracing::warn!(
                target: "cloud.budget",
                code = err.code.0,
                "a charge could not be recorded; the spend meter may under-report"
            );
        }
        self.ledger.note_cloud();

        // Section 6.2: an unrecognised enum member is kept and discounted.
        let confidence = (validated.value.confidence()
            - UNKNOWN_ENUM_PENALTY * validated.coerced as f32)
            .clamp(0.0, 1.0);

        if let Err(err) = self.cache.put(&CacheEntry {
            key,
            task: T::NAME,
            task_version: T::VERSION,
            model: &plan.model.model,
            response_json: &response.text,
        }) {
            tracing::warn!(
                target: "cloud.cache",
                code = err.code.0,
                "a response could not be cached; the next run will pay for it again"
            );
        }

        let latency_ms = self.clock.monotonic_ms().saturating_sub(started_ms);
        self.audit.record(
            &CallRecord {
                id,
                project_id: Some(project.to_string()),
                task: T::NAME.to_string(),
                task_version: T::VERSION,
                model: response.model.clone(),
                provider: self.client.provider().kind().as_str().to_string(),
                transport: self.client.transport_name().to_string(),
                source: Source::Cloud,
                tier: Some(plan.tier.as_str().to_string()),
                prompt_hash: prompt.prompt_hash(),
                image_hashes: image_hashes(prompt),
                tokens_in: response.tokens_in,
                tokens_out: response.tokens_out,
                cost_usd: cost,
                latency_ms,
                status: outcome.as_str().to_string(),
                retry_count: outcome.retry_count(),
                fallback_reason: None,
                price_table: PRICE_TABLE_VERSION,
                pruned_fields: u32::try_from(validated.pruned).unwrap_or(u32::MAX),
                coerced_enums: u32::try_from(validated.coerced).unwrap_or(u32::MAX),
                confidence,
                decision_ref: context.decision_ref.map(ToString::to_string),
                scratchpad: None,
            }
            .in_project(project),
        );

        tracing::info!(
            target: "cloud.call",
            task = T::NAME,
            task_version = T::VERSION,
            model = response.model.as_str(),
            tokens_in = response.tokens_in,
            tokens_out = response.tokens_out,
            cost_usd = cost,
            latency_ms,
            status = outcome.as_str(),
            retries = outcome.retry_count(),
            "cloud call complete"
        );

        CloudResult {
            value: validated.value,
            source: Source::Cloud,
            confidence,
            model: response.model,
            tokens_in: response.tokens_in,
            tokens_out: response.tokens_out,
            cost_usd: cost as f32,
            call_id: id,
        }
    }

    /// Step 4: a validated answer already in the cache, or nothing.
    ///
    /// A stored entry that no longer validates means the task's schema moved
    /// without its `VERSION` moving. It is dropped and the question re-asked
    /// rather than an answer the current code cannot read being served.
    #[allow(clippy::too_many_arguments)]
    fn try_cache<T>(
        &self,
        key: &str,
        schema: &Schema,
        prompt: &PromptSpec,
        plan: &crate::budget::Plan,
        id: uuid::Uuid,
        project: &str,
        context: &CallContext<'_>,
        started_ms: u64,
    ) -> Option<CloudResult<T::Output>>
    where
        T: CloudTask,
        T::Output: Scored,
    {
        let stored = self.cache.get(key).ok().flatten()?;
        let Ok(validated) = crate::validate::parse::<T::Output>(T::NAME, schema, &stored) else {
            tracing::warn!(
                target: "cloud.cache",
                task = T::NAME,
                "a cached response no longer matches the task schema; re-asking"
            );
            return None;
        };

        self.ledger.note_cache();
        let confidence = validated.value.confidence();
        let provider = self.client.provider().kind().as_str().to_string();
        self.audit.record(
            &CallRecord {
                id,
                project_id: Some(project.to_string()),
                task: T::NAME.to_string(),
                task_version: T::VERSION,
                model: plan.model.model.clone(),
                provider,
                transport: self.client.transport_name().to_string(),
                source: Source::Cache,
                tier: Some(plan.tier.as_str().to_string()),
                prompt_hash: prompt.prompt_hash(),
                image_hashes: image_hashes(prompt),
                tokens_in: 0,
                tokens_out: 0,
                cost_usd: 0.0,
                latency_ms: self.clock.monotonic_ms().saturating_sub(started_ms),
                status: "cache".to_string(),
                retry_count: 0,
                fallback_reason: None,
                price_table: PRICE_TABLE_VERSION,
                pruned_fields: 0,
                coerced_enums: 0,
                confidence,
                decision_ref: context.decision_ref.map(ToString::to_string),
                scratchpad: None,
            }
            .in_project(project),
        );

        Some(CloudResult {
            value: validated.value,
            source: Source::Cache,
            confidence,
            model: plan.model.model.clone(),
            tokens_in: 0,
            tokens_out: 0,
            cost_usd: 0.0,
            call_id: id,
        })
    }

    /// Prove a key works, for the Check button in Settings.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6002` when the provider rejects it, `AURA-CLOUD-6003` when the
    /// provider cannot be reached, `AURA-CLOUD-6001` when there is no key.
    pub fn validate_key(&self, secret: &SecretKey) -> AuraResult<String> {
        let provider = self.client.provider().clone();
        let request = provider.probe(secret)?;
        let response = self
            .client
            .transport_send(&request, std::time::Duration::from_secs(15))?;
        if (200..300).contains(&response.status) {
            let parsed = provider.parse(&response)?;
            self.client.breaker().reset();
            return Ok(parsed.model);
        }
        Err(crate::provider::map_status(provider.kind(), &response))
    }

    /// Consent, per class, for the two classes any task in this phase can need.
    fn check_consent(&self, project: &ProjectId) -> AuraResult<()> {
        for class in [DataClass::C1Operational, DataClass::C2Identifying] {
            if let Permit::Denied { reason } = self.consent.may_egress(*project, class) {
                return Err(consent_missing(&format!("{class:?}")).with_context("why", reason));
            }
        }
        Ok(())
    }

    /// The payload limits and the key-leak guard.
    fn inspect(&self, prompt: &PromptSpec) -> AuraResult<()> {
        // A credential inside a prompt is a credential uploaded to a third party.
        // Nothing in this crate builds one, which is exactly why it is checked.
        if crate::redact::looks_like_a_key(&prompt.system)
            || crate::redact::looks_like_a_key(&prompt.user)
        {
            return Err(payload_refused(
                "the rendered prompt contains something key-shaped; refusing to upload it",
            ));
        }

        let mut total = prompt.system.len() + prompt.user.len();
        for image in &prompt.images {
            if image.width.max(image.height) > self.policy.max_long_edge {
                return Err(payload_refused(format!(
                    "an image is {}x{}, over the {} px limit",
                    image.width, image.height, self.policy.max_long_edge
                )));
            }
            if image.media_type != "image/jpeg" && image.media_type != "image/png" {
                return Err(payload_refused(format!(
                    "{} is not an uploadable derivative format",
                    image.media_type
                )));
            }
            total += image.bytes.len();
        }
        if total > self.policy.max_payload_bytes {
            return Err(payload_refused(format!(
                "the payload is {total} bytes, over the {} byte ceiling",
                self.policy.max_payload_bytes
            )));
        }
        Ok(())
    }

    /// One call, one repair, and the outcome for the audit row.
    fn ask<T>(
        &self,
        prompt: &PromptSpec,
        model: &str,
        secret: &SecretKey,
        schema: &Schema,
    ) -> AuraResult<(
        crate::validate::Validated<T::Output>,
        crate::provider::CloudResponse,
        RepairOutcome,
    )>
    where
        T: CloudTask,
        T::Output: Scored,
    {
        let first = self.client.call(
            &CloudRequest {
                model: model.to_string(),
                prompt: prompt.clone(),
                repair: None,
            },
            secret,
        )?;

        match crate::validate::parse::<T::Output>(T::NAME, schema, &first.text) {
            Ok(validated) => Ok((validated, first, RepairOutcome::NotNeeded)),
            Err(complaint) => {
                tracing::info!(
                    target: "cloud.repair",
                    task = T::NAME,
                    "the first answer did not validate; sending the one repair message"
                );
                let second = self.client.call(
                    &CloudRequest {
                        model: model.to_string(),
                        prompt: prompt.clone(),
                        repair: Some(RepairTurn {
                            previous: crate::provider::truncate(&first.text, 4_000),
                            complaint: complaint.detail.clone(),
                        }),
                    },
                    secret,
                )?;
                let validated = crate::validate::parse::<T::Output>(T::NAME, schema, &second.text)?;
                // Tokens from both attempts are billed, so both are charged.
                let combined = crate::provider::CloudResponse {
                    tokens_in: first.tokens_in.saturating_add(second.tokens_in),
                    tokens_out: first.tokens_out.saturating_add(second.tokens_out),
                    ..second
                };
                Ok((validated, combined, RepairOutcome::Repaired))
            }
        }
    }

    /// The local answer, plus the row that records why it was needed.
    fn fall_back<T>(
        &self,
        task: &T,
        input: &T::Input,
        project: &str,
        context: &CallContext<'_>,
        cause: &AuraError,
        started_ms: u64,
    ) -> AuraResult<CloudResult<T::Output>>
    where
        T: CloudTask,
        T::Output: Scored,
    {
        let reason = FallbackReason::of(cause);
        self.ledger.note_fallback(T::NAME, reason);

        let value = task.local_fallback(input)?;
        let confidence = value.confidence();
        let id = call_id(
            &format!("{}:{}:{}", T::NAME, T::VERSION, reason.as_str()),
            self.sequence.fetch_add(1, Ordering::Relaxed),
        );

        let mut record =
            CallRecord::local(id, T::NAME, T::VERSION, reason.as_str()).in_project(project);
        record.confidence = confidence;
        record.latency_ms = self.clock.monotonic_ms().saturating_sub(started_ms);
        record.status = if reason == FallbackReason::SchemaInvalid {
            "schema_invalid".to_string()
        } else {
            "fallback".to_string()
        };
        record.retry_count = u32::from(reason == FallbackReason::SchemaInvalid);
        if let Some(decision) = context.decision_ref {
            record = record.for_decision(decision);
        }
        self.audit.record(&record);

        Ok(CloudResult {
            value,
            source: Source::LocalFallback,
            confidence,
            model: "local".to_string(),
            tokens_in: 0,
            tokens_out: 0,
            cost_usd: 0.0,
            call_id: id,
        })
    }
}

/// Everything step 7 needs, gathered so the settle function takes three
/// arguments rather than eleven.
#[derive(Debug, Clone, Copy)]
struct Settlement<'a> {
    key: &'a str,
    prompt: &'a PromptSpec,
    plan: &'a crate::budget::Plan,
    id: uuid::Uuid,
    project: &'a str,
    context: &'a CallContext<'a>,
    started_ms: u64,
    outcome: RepairOutcome,
}

/// The image content hashes of a prompt, in the order they are sent.
fn image_hashes(prompt: &PromptSpec) -> Vec<String> {
    prompt
        .images
        .iter()
        .map(|image| image.content_hash.clone())
        .collect()
}
