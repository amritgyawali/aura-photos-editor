# ADR-0114: Internal AI routes, bounded skin advice and residual review

Accepted 2026-10-11.

The user supplies an assignment-format AI configuration file. Parse it as data,
validate every profile before importing, and store each key in the existing OS
credential store. Accounts include the profile identifier and endpoint digest:
rotating an endpoint cannot send its new credential through an old cached route.
Store only endpoint, model, capability, pricing and discovery status in the local
catalogue. Neither the file contents nor keys belong in the repository or logs.

Internal routes use the existing governed gateway. Policy, derived-image consent,
payload limits, budgets, typed validation, cancellation and local fallback apply
before and during routing. Each provider has its own key, circuit breaker and
audit identity. Never send an image to a model with unconfirmed image support.
Keep configured text models; optionally discover a separately labelled vision
model from the same provider, requiring published pricing and bounded rates.
An unaffordable model does not hide a cheaper eligible fallback. Limit attempts
to three providers per task; each may make one schema-repair request. Refused
keys and temporary failures stop repeat calls through breakers and cooldowns.
An explicitly free configured model permits only free discovered alternatives.

Charge known token usage even when a paid completion fails validation. Its audit
row records the failed cloud attempt; the separate local answer remains free.
Price the repair again after charging the first response. Token-price estimates
are not invoice reconciliation, and unknown pricing is not assumed to be zero.
Audit identifiers include project and clock context, so restarting the gateway
cannot overwrite an earlier request's audit record. Feedback identifiers are
stable for the rated call and remain scoped to its project.

The configuration activates routing once when no privacy choice exists. Explicit
privacy settings persist across restarts and refreshes. A requested AI edit can
grant derived-image consent for an untouched project; existing privacy choices,
expiry and revocation remain authoritative. Enabling the project's AI setting
records its actual consent ledger, fixing the previously inert toggle. Full
originals and shared model-training permission are never granted by import.

Per-project ranking is reviewed once per UTC day on use, using the last 256 local
audit records. Typed-response success and latency measure reliability. Explicit
useful/needs-improvement ratings measure user preference; repeated ratings of
one call replace the same feedback record. No background source-code rewriting,
model fine-tuning, remote dataset contribution or guaranteed quality increase is
performed. AURA must be running for this review to happen.

Vision skin advice produces validated bounded parameters, never pixels, masks,
geometry, commands or generated anatomy. Native segmentation and healing apply
the parameters, preserving measured feature exclusions. Custom per-photo retouch
preferences and manual edits take precedence. The fixed 600-photo AI cutoff is
removed; project/month budgets and cancellation govern every eligible photo.
Local one-click editing and vision-guided retouch share enabled face/body healing
defaults. The report also records explicitly supplied options when no face or
operation is found, so a face-only choice cannot be mistaken for untouched defaults.
Repeated AI recommendations with no changed editable fields preserve the saved
recipe and undo history instead of saving audit-only provenance changes.

Native healing edits gain optional `repairPasses` in 1..4 for AcneClear and
FrequencyHeal. Omitted values retain the original two-pass rendering and JSON.
The new detailed preset opts into four passes. The read-only residual review
renders the saved retouch view, checks its recipe hash before and after measuring,
and lists candidate regions inside the selected skin matte or brush. Choosing a
candidate prepares a brush; preview and Apply remain explicit. Zero candidates
is not a guarantee of complete removal. Existing eye/nose protections remain.

Frozen IPC types and all 83 locked contracts remain unchanged. New commands and
types extend the non-frozen native tool surface. Browser connection buttons are
replaced by internal status; advanced manual setup remains available.
