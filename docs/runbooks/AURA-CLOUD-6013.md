# AURA-CLOUD-6013 - Cloud work cancelled by the photographer

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. Everything already decided is saved.

## What actually happened

The cancellation token was signalled - the user pressed Stop, closed the project, or a higher-priority job pre-empted the batch.

## What AURA does automatically

Checks the token before each provider call and before each agent step, so a cancel takes effect within one call rather than at the end of the run. An in-flight call is allowed to finish and its result is cached, because throwing away an answer the user has already paid for helps nobody.

## Operator steps

Normal operation. Investigate only if a cancel takes longer than one provider call to take effect, which would mean a loop is not checking the token.

## Related

- Agent loop: `crates/aura-cloud/src/agent/`
