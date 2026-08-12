# AURA-CLOUD-6003 - The AI provider could not be reached

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, once per run rather than once per call, and `local_fallback` decisions in the Explain panel.

## What actually happened

DNS, TCP connect, or the read deadline elapsed. Hotel wifi, a captive portal, a corporate proxy, a VPN split-tunnel, or the provider being genuinely down all land here.

## What AURA does automatically

Bounded retries with exponential backoff and jitter, then the circuit breaker opens and the rest of the run goes local without further waiting. **No pipeline stage blocks on a cloud result**, so the total wall clock rises by at most the budgeted 3 %.

## Operator steps

1. Establish whether anything else on the machine can reach the internet. A captive portal that has not been accepted is the single most common cause on location.
2. If this build is reaching an `http://` OpenAI-compatible endpoint, confirm the server is running and that the host and port in Settings match.
3. Public HTTPS endpoints are not reachable by this build at all - see the TLS waiver in `docs/adr/ADR-0009-cloud-ai-policy.md`. That presents as this code and is expected, not a defect.
4. Nothing needs to be re-run by hand. Re-running the wedding with the network up replaces `local_fallback` decisions with cloud ones, and the response cache makes the second pass nearly free.

## Related

- Policy ADR: `docs/adr/ADR-0009-cloud-ai-policy.md`
