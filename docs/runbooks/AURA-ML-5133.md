# AURA-ML-5133 - The camera matching policy table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

No camera at the wedding was matched, and the panel says the settings could not be loaded.

## What actually happened

`camera_match.toml` exists and did not validate. Five reasons, and the second and third are the ones
that matter:

1. The file will not parse as TOML.
2. **A bound is wider than the contract's own ceiling.**
3. **An evidence threshold is looser than the contract's own floor.**
4. The shooter share is outside 0.05 to 0.60, or the shooter cap is outside 0 to 0.30 stops.
5. A scene name is not one of the 23.

## Why this halts rather than falling back on the defaults

The same call phases 21, 22 and 25 made about their own tables, and it matters more here than in any
of the three: **a bound in this file governs every photograph a body shot.** A widened ceiling is
not a bolder edit on one frame, it is a systematic shift across four thousand of them that nobody
notices frame by frame. Falling back on the bundled table would run the pass under settings nobody
chose while a studio believed their own were in force.

## Evidence thresholds move the other way

`bounds` may only be **lowered**. `evidence.min_pairs` may only be **raised**, and
`evidence.max_gap_ms` and `scene.background_agreement` are the same shape.

The reason is that lowering an evidence threshold is a way of widening every bound at once without
touching one: a correction trusted on four pairs instead of twelve is a correction fitted to noise,
and it can reach any value inside the box.

## The seven ceilings

| Bound | Ceiling |
|---|---|
| `max_cct_k` | 900 |
| `max_tint` | 20 |
| `max_exposure_ev` | 0.60 |
| `max_channel_gain` | 0.10 |
| `max_saturation` | 12 |
| `max_contrast_shape` | 0.15 |
| `max_skin_uv` | 0.012 |

`shooter.share` is the one value bounded on both sides. Zero is a matching pass with the shooter
half switched off, which is a feature flag rather than a config value somebody sets by accident. One
erases a second photographer from their own work, which is section 12's second failure mode written
as a setting.

## Fixing it

The detail line names the offending key and its value. Restore the bundled table from
`crates/aura-brain-gallery/config/camera_match.toml`, or move the value in the permitted direction,
and bump `version` so every stored row is re-solved.
