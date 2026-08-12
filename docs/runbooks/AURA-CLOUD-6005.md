# AURA-CLOUD-6005 - The response did not match the task schema after one repair

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and a `local_fallback` decision for that step only.

## What actually happened

The provider returned JSON that failed the task's JSON Schema. The gateway sent exactly one repair message containing the validator's error and the original response; the second answer failed too. Common shapes: prose wrapped around the JSON, a markdown fence, a truncated object because the token ceiling was hit, or an invented field.

## What AURA does automatically

One repair attempt, then the local fallback, then an audit row with `status='schema_invalid'` and `retry_count=1`. **It never panics and never writes a partial value.** Fields the schema does not define are dropped rather than failing the whole response; unknown enum values map to `unknown` and lose 0.20 confidence.

## Operator steps

1. Read the audit row's `prompt_hash` and find the cassette or the recorded response for that call.
2. A truncated object means `max_tokens` for that task is too low. It is a per-task constant, and raising it requires a task version bump so the cache does not serve answers made under the old ceiling.
3. A model that consistently wraps JSON in prose is the wrong tier for the task; raise the task's minimum tier.
4. If the schema itself is wrong, changing it means a task version bump plus a cassette re-record plus a CHANGELOG entry. Never edit a shipped schema in place.

## Related

- Validator: `crates/aura-cloud/src/schema.rs`
- Repair loop: `crates/aura-cloud/src/repair.rs`
