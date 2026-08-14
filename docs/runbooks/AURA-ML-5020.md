# AURA-ML-5020 - The couple could not be identified with confidence

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, as a prompt in the People panel offering the top two candidate pairs and a one-click confirmation. It is also the prompt Autopilot shows before its first run, because section 12 of the phase document names a wrong couple as the failure that poisons every later decision.

## What actually happened

`aura_vision::face::roles::infer_roles` scores every pair of identities on four terms - co-occurrence in couple portraits, centrality during the ceremony, raw mutual co-occurrence, and how much of a photographic subject each of the two is. The top two pairs scored within `COUPLE_AMBIGUOUS_MARGIN` (0.05) of each other.

The usual cause today is that **there are no scene labels**. Phase 07 assigns them; until it does, the portrait and ceremony terms contribute nothing and the decision rests on co-occurrence and frequency alone. That fallback is weaker in a specific and predictable way: a bride and her mother appear together in getting-ready frames constantly, so they can outscore a bride and a groom who were photographed separately all morning.

That is why the confidence is capped at `SCENELESS_CONFIDENCE_CEILING` (0.62) whenever no scene is known, and why the reason string says so rather than letting a 0.9 look earned.

## What AURA does automatically

Keeps the best guess, assigns `Role::Couple` to both members, leaves `SubjectHierarchy::couple_unconfirmed` true, and asks. Nothing irreversible - a delivery, a gallery, an automatic cull - runs on an unconfirmed couple.

If cloud reasoning is enabled for people and the project has consent, the optional couple hint from section 7 may raise confidence. It may **never** override a `user_locked` identity, and it is capped at two calls per project.

## Operator steps

1. Confirm in the People panel. One click sets `bride` or `groom`, sets `user_locked`, and writes a journal entry - so the decision survives a full re-analysis and is never overwritten by automation.
2. **AURA never guesses which of two people is the bride.** The evidence identifies a pair, the couple may be same-sex, and that assignment is a human's. If the panel offers a pair and you are only sure of one of them, set the one you are sure of.
3. If the pair itself is wrong, the cause is usually a merge or a split that has not been made yet. Fix the identities first, then confirm - the roles are re-inferred from the corrected graph.
4. If it recurs across many weddings once phase 07 is in, that is a real finding about the couple scoring weights rather than an operator problem. The weights are constants in `aura_vision::face::roles`.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Cloud policy: `docs/adr/ADR-0009-cloud-ai-policy.md`
- How a people decision survives a re-analysis: `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md`
