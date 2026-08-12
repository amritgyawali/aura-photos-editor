# AURA-ML-5005 - Model has no model card

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. In practice a customer should never see it: the same check runs in CI and blocks the release.

## What actually happened

Every entry in `models.lock` names a `model_card` path, and Article VI rule M1 makes the card mandatory - purpose, architecture, training data with licences, latency on the reference machines, quality gate, known failure modes and fallback behaviour. The named card is missing or empty.

## What AURA does automatically

The model is refused before load and `model.rejected` is emitted with `reason = "card"`. This is deliberately a hard refusal: an undocumented model is one nobody can reason about when it misbehaves on a stranger's wedding.

## Operator steps

1. Developers: write the card from `docs/model-cards/TEMPLATE.md`, then re-run `cargo run -p xtask -- models --check`.
2. Never work around this by removing the `model_card` field. The field is required by the schema and the check will fail on the missing field instead, which is the same refusal with a worse message.
3. If a shipped pack triggers it, the release skipped its gate: pull the pack and escalate to MLOPS.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Model card template: `docs/model-cards/TEMPLATE.md`
- Adding a model: `docs/runbooks/adding-a-model.md`
