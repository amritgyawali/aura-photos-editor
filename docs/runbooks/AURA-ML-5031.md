# AURA-ML-5031 - A moment profile file was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, and a product that has **not** changed anything. This is the one phase 08 code that halts.

## Why it halts, when nothing else in this phase does

For `AURA-ML-5024`'s reason, one phase on. `moment_profiles.toml` is a threshold table: it decides which frames belong to the same moment, in every scene, for every wedding on this machine. A **half-loaded** one silently changes that for some scenes and not others, and it does so without anybody noticing - which is exactly the class of failure invariant 9 exists to forbid.

Every other failure in this phase degrades into a wedding that is still usable: a frame with no moment, a grouping nobody trusts, a refused edit. A silently altered threshold table is different in kind, so the loader refuses and leaves the previous table in place.

## What actually happened

One of six rules refused, and the detail names the file, the key and the rule in that order - which is the order somebody fixes them in.

1. **"has no rationale"** - the rule with the most friction and the most value. A threshold nobody can explain is a magic number, and somebody who cannot write a sentence saying why a photographer would agree with 0.60 has not finished deciding it. Nine characters minimum, the same floor `scene_profiles.toml` uses.
2. **"is not valid TOML"**.
3. **"has an edge_threshold outside 0..1"**.
4. **"has edge_threshold = X, above the 0.85 two frames with no shared faces can reach; nothing in this scene would ever group"** - the interesting one. The four weights are 0.55, 0.20, 0.15 and 0.10, and the identity term is zero for any pair with no assigned faces. So a threshold above 0.85 disables grouping entirely for detail shots, venue shots and every wedding where the face pass has not run - silently, and only for some scenes. The loader refuses rather than survives it.
5. **"has a window_scale outside 0.25..4.0"**.
6. **"has a max_group outside 2..500"**.

## What AURA does automatically

**For an installation override** at `<catalog>/config/moment_profiles.toml`: falls back to the shipped baseline, keeps the refusal for the Problems list, and carries on. The baseline is known good and falling back to it keeps the product open.

**For the embedded baseline**: halts. A build whose own threshold table will not parse is a broken build, and it must not open a project - which is why `crates/aura-brain-wedding/tests/config.rs` loads it in CI rather than only at run time.

## Operator steps

1. Read the file, the key and the rule from the message.
2. For an installation override, the fastest fix is to delete it: `<catalog>/config/moment_profiles.toml`. The shipped baseline takes over and the product is immediately correct, if untuned.
3. For rule 1, write the sentence. It is expected to say *why a photographer would agree with the number*, not to restate it. "Dance floor frames are visually similar for minutes on end" is a rationale; "looser threshold for dance floor" is not.
4. `just phase-08-verify` loads the table as its second check, before anything is built on it.

## When this is not the problem

A photographer whose grouping is merely *wrong* is not hitting this: the table loaded, and the numbers in it are the argument. `AURA-ML-5030` is that conversation.

## Related

* `AURA-ML-5024` - the same rule for scene profiles and ritual taxonomies, and the code this one is modelled on.
* `docs/adr/ADR-0017-burst-grouping-and-duplicate-policy.md` section 4 - why the grouping thresholds are a second file rather than three more fields on the frozen `SceneProfile`.
