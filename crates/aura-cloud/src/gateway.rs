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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
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
    routes: Vec<Arc<Self>>,
    key_account: Option<String>,
    profile_id: Option<String>,
    image_capable: bool,
    cooldowns: parking_lot::Mutex<std::collections::BTreeMap<usize, u64>>,
    daily_order: parking_lot::Mutex<std::collections::BTreeMap<(String, time::Date), Vec<usize>>>,
    /// Makes call identifiers unique within a run without a random number.
    sequence: Arc<AtomicU64>,
}

/// How long a route that did not answer in time is left alone, and the longest a provider's
/// own pause is honoured for.
const UNAVAILABLE_REST_MS: u64 = 10 * 60 * 1000;
/// The rest after any other failure, and after a refusal that named no pause.
const DEFAULT_REST_MS: u64 = 60_000;
/// A pause shorter than this is treated as this long: the next photograph is seconds away.
const SHORTEST_REST_MS: u64 = 5_000;

/// How long a route rests after a failure, from the note its audit row carries.
///
/// A provider that did not answer in time is in the same state a minute later, and asking it
/// again for every photograph makes each one wait for the same silence. So is a model whose
/// answer could not be used even after the repair: that is the model, not the moment, and the
/// two round trips it took are the slowest way there is to reach the local answer. A provider
/// that refused for rate says when to come back: a per-minute allowance is free again in
/// seconds and a daily one is not, and both are its own words rather than a guess.
fn rest_after(note: Option<&str>) -> u64 {
    let note: serde_json::Value = note
        .and_then(|note| serde_json::from_str(note).ok())
        .unwrap_or_default();
    match note.get("errorCode").and_then(serde_json::Value::as_str) {
        Some("AURA-CLOUD-6003" | "AURA-CLOUD-6005") => UNAVAILABLE_REST_MS,
        Some("AURA-CLOUD-6004") => note
            .get("retryAfterMs")
            .and_then(serde_json::Value::as_u64)
            // One and two seconds are what the status mapping fills in when no pause was named.
            .filter(|pause| *pause > 2_000)
            .map_or(DEFAULT_REST_MS, |pause| {
                pause.clamp(SHORTEST_REST_MS, UNAVAILABLE_REST_MS)
            }),
        _ => DEFAULT_REST_MS,
    }
}

/// [`rest_after`], for the routing tests beside the import.
#[cfg(test)]
pub(crate) fn rest_after_for_tests(note: Option<&str>) -> u64 {
    rest_after(note)
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
            routes: Vec::new(),
            key_account: None,
            profile_id: None,
            image_capable: false,
            cooldowns: parking_lot::Mutex::new(std::collections::BTreeMap::new()),
            daily_order: parking_lot::Mutex::new(std::collections::BTreeMap::new()),
            sequence: Arc::new(AtomicU64::new(0)),
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

    /// Add file-configured routes. Each keeps its own key, breaker, prices and
    /// audit identity; every attempt still passes this gateway's policy gates.
    #[must_use]
    pub fn with_internal_routes(
        mut self,
        profiles: &[crate::internal_ai::Profile],
        transport: &Arc<dyn crate::provider::Transport>,
    ) -> Self {
        for profile in profiles.iter().filter(|p| p.available()) {
            let mut route = Self::new(
                profile.client(Arc::clone(transport)),
                Arc::clone(&self.keys),
                Arc::clone(&self.cache),
                Arc::clone(&self.audit),
                Arc::clone(&self.governor),
                Arc::clone(&self.consent),
                Arc::clone(&self.clock),
                self.policy,
            );
            route.key_account = Some(profile.account());
            // Capability and quality prior are kept with the profile, not inferred
            // from a successful text-only completion.
            route.profile_id = Some(profile.id.clone());
            route.image_capable = profile.vision;
            route.sequence = Arc::clone(&self.sequence);
            route.ledger = Arc::clone(&self.ledger);
            self.routes.push(Arc::new(route));
        }
        if let Some(primary) = self
            .routes
            .iter()
            .find(|route| route.image_capable)
            .or_else(|| self.routes.first())
        {
            self.client = Arc::clone(&primary.client);
            self.key_account.clone_from(&primary.key_account);
        }
        self
    }

    /// The selected route's secure account, including internally configured keys.
    #[must_use]
    pub fn credential_account(&self) -> String {
        self.key_account
            .clone()
            .unwrap_or_else(|| self.client.provider().kind().account())
    }

    /// Refresh per-project ranking once each UTC day from persisted outcomes.
    /// Only explicit accepted/rejected feedback is used as quality evidence;
    /// typed response success and latency are reliability evidence.
    pub(crate) fn ranked_routes(&self, project: &str) -> Vec<usize> {
        let day = self.clock.now_utc().date();
        let mut orders = self.daily_order.lock();
        let key = (project.to_string(), day);
        if let Some(order) = orders.get(&key) {
            return order.clone();
        }
        orders.retain(|(_, recorded), _| *recorded == day);
        let rows = self.audit.recent(project, 256).unwrap_or_default();
        let mut scored: Vec<(usize, f64)> = self
            .routes
            .iter()
            .enumerate()
            .map(|(index, route)| {
                let mut score = (self.routes.len().saturating_sub(index)) as f64;
                for row in rows
                    .iter()
                    .filter(|row| Some(&row.provider) == route.profile_id.as_ref())
                {
                    score += match row.status.as_str() {
                        "accepted" => 3.0,
                        "rejected" => -5.0,
                        "ok" | "repaired" => 0.25 - (row.latency_ms as f64 / 60_000.0).min(0.2),
                        "schema_invalid" => -1.0,
                        "fallback" if row.fallback_reason.as_deref() == Some("provider_failed") => {
                            -1.0
                        }
                        _ => 0.0,
                    };
                }
                (index, score.clamp(-100.0, 100.0))
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        let order: Vec<_> = scored.into_iter().map(|(index, _)| index).collect();
        orders.insert(key, order.clone());
        order
    }

    /// True when this installation has suitable internally managed routes.
    #[must_use]
    pub fn has_internal_routes(&self) -> bool {
        !self.routes.is_empty()
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
    #[allow(clippy::too_many_lines)] // The ordered policy, routing and settlement gates stay visible together.
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

        if !self.routes.is_empty() {
            let mut last = None;
            let mut attempted = 0;
            for index in self.ranked_routes(&project) {
                let Some(route) = self.routes.get(index) else {
                    continue;
                };
                // The import stores verified capability in a private routing
                // marker. Text-only routes never receive an image payload.
                if !prompt.images.is_empty() && !route.image_capable {
                    continue;
                }
                if route.client.breaker().is_open() {
                    continue;
                }
                if self
                    .cooldowns
                    .lock()
                    .get(&index)
                    .is_some_and(|until| *until > self.clock.monotonic_ms())
                {
                    continue;
                }
                if attempted >= 3 {
                    break;
                }
                let result = route.run(task, input, context)?;
                if result.source != Source::LocalFallback {
                    return Ok(result);
                }
                let row = self
                    .audit
                    .recent(&project, 32)
                    .ok()
                    .and_then(|rows| rows.into_iter().find(|row| row.id == result.call_id));
                let rest = rest_after(row.as_ref().and_then(|row| row.scratchpad.as_deref()));
                let reason = row.and_then(|row| row.fallback_reason);
                let retryable = reason.as_deref().is_some_and(|reason| {
                    matches!(
                        reason,
                        "provider_failed" | "schema_invalid" | "no_key" | "cost_ceiling"
                    )
                });
                // A route refused before any call does not consume one of the
                // three provider attempts; continue to an affordable model.
                if reason.as_deref() != Some("cost_ceiling") {
                    attempted += 1;
                }
                if retryable && reason.as_deref() != Some("cost_ceiling") {
                    self.cooldowns
                        .lock()
                        .insert(index, self.clock.monotonic_ms().saturating_add(rest));
                }
                last = Some(result);
                if !retryable {
                    break;
                }
            }
            if let Some(result) = last {
                return Ok(result);
            }
            return self.fall_back(
                task,
                input,
                &project,
                context,
                &no_key("No eligible internal AI route"),
                started_ms,
            );
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
        // Keep billing/feedback rows distinct after an application restart.
        // Wall-clock identity does not affect the deterministic response cache.
        let audit_key = format!(
            "{key}:{project}:{}",
            self.clock.now_utc().unix_timestamp_nanos()
        );
        let id = call_id(&audit_key, self.sequence.fetch_add(1, Ordering::Relaxed));

        // 4. Cache.
        if let Some(hit) = self.try_cache::<T>(
            &key, &schema, &prompt, &plan, id, &project, context, started_ms,
        ) {
            return Ok(hit);
        }

        // The key is read as late as possible, so a run that was going to be
        // refused, cached or over budget never touches the credential store.
        // A provider whose row says no key is required gets an empty secret: the
        // builders already suppress the credential header for one, and a local
        // server with no key at all would otherwise read as "no key stored" and
        // degrade to the fallback for ever.
        let account = self
            .key_account
            .clone()
            .unwrap_or_else(|| config.kind.account());
        let requires_key = crate::catalog::spec(config.kind).requires_key;
        let stored = self.keys.load(&account);
        let secret = match (requires_key, stored) {
            (_, Ok(Some(secret))) => secret,
            (false, Ok(None)) => SecretKey::new(String::new()),
            (true, Ok(None)) => {
                return self.fall_back(
                    task,
                    input,
                    &project,
                    context,
                    &no_key(config.kind.as_str()),
                    started_ms,
                );
            }
            (_, Err(err)) => {
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
        let mut usage = AttemptUsage::default();
        let attempt = self.ask::<T>(
            &prompt,
            &plan.model,
            &secret,
            &schema,
            &project,
            &mut usage,
            f64::from(task.max_cost_usd()),
        );

        let (validated, response, outcome) = match attempt {
            Ok(complete) => complete,
            Err(err) => {
                if self.profile_id.is_some() && (usage.tokens_in > 0 || usage.tokens_out > 0) {
                    // A provider can bill a completion that fails schema validation.
                    // Record that spend separately from the zero-cost local answer.
                    let mut billed = CallRecord::local(
                        id,
                        T::NAME,
                        T::VERSION,
                        FallbackReason::of(&err).as_str(),
                    )
                    .in_project(&project);
                    billed.provider = self.profile_id.clone().unwrap_or_default();
                    billed.model.clone_from(&plan.model.model);
                    billed.source = Source::Cloud;
                    billed.status = "invalid_response".to_string();
                    billed.tokens_in = usage.tokens_in;
                    billed.tokens_out = usage.tokens_out;
                    billed.cost_usd = usage.cost;
                    billed.prompt_hash = prompt.prompt_hash();
                    billed.image_hashes = image_hashes(&prompt);
                    billed.transport = self.client.transport_name().to_string();
                    billed.latency_ms = self.clock.monotonic_ms().saturating_sub(started_ms);
                    billed.decision_ref = context.decision_ref.map(ToString::to_string);
                    self.audit.record(&billed);
                }
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
        if let Err(err) = if self.profile_id.is_none() {
            self.governor.charge(project, cost, plan.downgraded)
        } else {
            Ok(())
        } {
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
                provider: self
                    .profile_id
                    .clone()
                    .unwrap_or_else(|| self.client.provider().kind().as_str().to_string()),
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
    #[allow(clippy::too_many_arguments)]
    fn ask<T>(
        &self,
        prompt: &PromptSpec,
        alias: &crate::provider::ModelAlias,
        secret: &SecretKey,
        schema: &Schema,
        project: &str,
        usage: &mut AttemptUsage,
        max_cost: f64,
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
                model: alias.model.clone(),
                prompt: prompt.clone(),
                repair: None,
            },
            secret,
        )?;
        self.charge_attempt(&first, alias, project, usage)?;

        match crate::validate::parse::<T::Output>(T::NAME, schema, &first.text) {
            Ok(validated) => Ok((validated, first, RepairOutcome::NotNeeded)),
            Err(complaint) => {
                if self.profile_id.is_some() {
                    let mut repair_prompt = prompt.clone();
                    repair_prompt
                        .user
                        .push_str(&crate::provider::truncate(&first.text, 4_000));
                    repair_prompt.user.push_str(&complaint.detail);
                    self.governor.plan(
                        T::NAME,
                        project,
                        self.client.provider().config(),
                        &repair_prompt,
                        max_cost,
                    )?;
                }
                tracing::info!(
                    target: "cloud.repair",
                    task = T::NAME,
                    "the first answer did not validate; sending the one repair message"
                );
                let second = self.client.call(
                    &CloudRequest {
                        model: alias.model.clone(),
                        prompt: prompt.clone(),
                        repair: Some(RepairTurn {
                            previous: crate::provider::truncate(&first.text, 4_000),
                            complaint: complaint.detail.clone(),
                        }),
                    },
                    secret,
                )?;
                self.charge_attempt(&second, alias, project, usage)?;
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

    fn charge_attempt(
        &self,
        response: &crate::provider::CloudResponse,
        alias: &crate::provider::ModelAlias,
        project: &str,
        usage: &mut AttemptUsage,
    ) -> AuraResult<()> {
        if self.profile_id.is_none() {
            return Ok(());
        }
        usage.tokens_in = usage.tokens_in.saturating_add(response.tokens_in);
        usage.tokens_out = usage.tokens_out.saturating_add(response.tokens_out);
        let cost = alias.price(response.tokens_in, response.tokens_out);
        usage.cost += cost;
        self.governor.charge(project, cost, false).map_err(|_| {
            payload_refused("Could not record AI spending; stopping provider requests")
        })
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
            &format!(
                "{}:{}:{}:{project}:{}",
                T::NAME,
                T::VERSION,
                reason.as_str(),
                self.clock.now_utc().unix_timestamp_nanos()
            ),
            self.sequence.fetch_add(1, Ordering::Relaxed),
        );

        let mut record =
            CallRecord::local(id, T::NAME, T::VERSION, reason.as_str()).in_project(project);
        record.confidence = confidence;
        if let Some(profile) = &self.profile_id {
            record.provider.clone_from(profile);
            let pause = cause
                .context
                .iter()
                .find(|(key, _)| key == "retry_after_ms")
                .and_then(|(_, value)| value.parse::<u64>().ok());
            // Both values are a registry code and a number: nothing here needs escaping.
            record.scratchpad = Some(format!(
                "{{\"errorCode\":\"{}\",\"retryAfterMs\":{}}}",
                cause.code.0,
                pause.map_or_else(|| "null".to_owned(), |ms| ms.to_string())
            ));
        }
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

#[derive(Debug, Default)]
struct AttemptUsage {
    tokens_in: u32,
    tokens_out: u32,
    cost: f64,
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
