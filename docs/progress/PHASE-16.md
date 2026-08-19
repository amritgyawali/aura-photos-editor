# PHASE-16 progress - Tone AI, Adaptive Curves, HSL AI & Skin-Tone Protection

One line per task group, in the order section 8 lists them. Branch
`feat/phase-16-tone-curves-colour-ai`.

| # | Task | Files | Tests | Notes |
|---|---|---|---|---|
| T0 | Kickoff and ADRs | `docs/adr/ADR-0033-tone-curves-hsl-and-skin-protection.md`, `docs/adr/ADR-0034-colour-ipc-surface.md` | - | Seven decisions recorded before code, five of them about what the phase refuses to do |
| T1 | Freeze the section 5 contract | `crates/aura-core/src/contract/colour.rs` | `cargo xtask contracts` | `ColourDecision`, `ToneCurve`, `HslAdjustments`, `SkinGuardReport`, `ColourVariant`, 29 reason codes, `ColourService`. No ideal-skin field anywhere |
| T2 | Error registry and runbooks | `crates/aura-core/errors.toml`, `docs/runbooks/AURA-ML-5066..5071.md` | - | Six codes. `AURA-ML-5069` is the first in the product that fires because a *guarantee* could not be met |
| T3 | The shared interpolation | `crates/aura-raw/src/colour/curve.rs`, `crates/aura-render/src/tonemap.rs` | 3 unit | Fritsch-Carlson moved to `aura-raw` when the curve fitter became its second consumer. Arithmetic unchanged; phase 14's golden suite is the guard |
| T4 | The intent table | `crates/aura-brain-photo/config/tone_intent.toml`, `src/colour/intent.rs`, `tests/tone_intent.rs` | 6 unit + 14 integration | 22 argued-over scene rows plus a neutral one, a written reason each. A row cannot raise the subtlety or clipping ceiling |
| T5 | The tone solver | `src/colour/tone.rs` | 7 unit | Five parameters from the histogram, the subject spread and phase 09's noise headroom. The head is registered and never consulted |
| T6 | The curve fitter | `src/colour/curve.rs` | 9 unit | Section 6.1's three constraints, applied by **bounding the gain** rather than clamping the nodes - clamping produced flat bands, which is a posterised band and new clipping in one move |
| T7 | Content and harmony | `src/colour/content.rs`, `src/colour/harmony.rs` | 12 unit | Six content bands inferred from hue, saturation, luminance and position; five goals, one of them a prohibition |
| T8 | The eight bands | `src/colour/hsl.rs` | 7 unit | The unit conversion the renderer defines, the per-scene cap, and the cheap half of the skin defence |
| T9 | The skin guard | `src/colour/skin_guard.rs` | 8 unit | The guarantee, measured **through the real renderer** on this frame's own skin after the tone half. Three re-solves, then withdrawal |
| T10 | The clipping and subtlety guards | `src/colour/clip_guard.rs` | 6 unit | Reductions only. The parameter to reduce is **measured** rather than assumed - a fixed order spent every attempt on a white point that was not the cause |
| T11 | The analyser and the codec | `src/colour/analyse.rs`, `src/colour/codec.rs` | 8 unit | One decode, one grade, three guarded alternatives. Reasons ranked by informativeness, not alphabetically |
| T12 | Migration 16 and the store | `crates/aura-catalog/migrations/0016_colour.sql`, `src/colour/store.rs` | gate | One table, one view, three indexes. No skin-target column and nowhere to put one |
| T13 | The frozen service and the pass | `src/colour/api.rs` | gate | `ColourService` plus the resumable walk. Section 11's three telemetry events plus a fourth |
| T14 | Fixtures | `src/colour/fixtures.rs` | 27 eval | 22 synthetic frames under a **neutral** light, with the subject contrast painted in encoded units |
| T15 | The model | `crates/aura-infer/src/onnx/fixtures.rs`, `xtask/src/models.rs`, `models/models.lock`, `docs/model-cards/tone_model.md` | `cargo xtask models` | `tone_model` 1.0.0, signed, carded, int8 forbidden, **never consulted** |
| T16 | The evaluation harness | `tests/eval/colour_eval.rs`, `ml/models/colour/{train_tone,eval_colour,export}.py` | 27 eval + 8 self-test | Section 10.1's six gates plus the harness's own falsification checks |
| T17 | The IPC surface | `crates/aura-app/src/contract/ipc.rs`, `src/colour_commands.rs`, `ui/src-tauri/src/main.rs`, `ui/src/ipc/{types,client}.ts` | 9 unit | Seven commands, thirteen shapes, nine writable recipe paths and none of them a white balance |
| T18 | The panels | `ui/src/components/develop/{TonePanel,CurveEditor,HslPanel}.tsx` + test | 16 vitest | The guarantee as a measurement, the curve over the identity, the protected-skin indicator |
| T19 | The gate | `crates/aura-cli/src/phase16.rs` | `aura-cli verify --phase 16` | Fourteen checks, exits 0 |
| T20 | Docs and re-lock | `docs/tone-and-colour.md`, `docs/skin-fairness.md`, `contracts.lock`, `CLAUDE.md` | `cargo xtask contracts --check` | Migrations 15 and 16 added to the frozen set; 15 had been omitted by oversight |

## Three things that changed direction during the phase

**The skin guarantee's baseline moved.** It was measured against the raw pixel first, and
that made every correctly graded frame a violation - chroma in CIE `LCh` scales with
lightness, so opening a shadow *must* change it. The baseline is the frame's own skin after
the tone half, which is what section 6.3's own wording ("all HSL, vibrance and saturation
operations") asks for.

**The curve fitter stopped clamping.** Clamping a node that wanted to sit above white produced
a flat top: a posterised band, new clipping and a failed property test, all from one line. The
fit now solves for the interval of gains that keeps every node inside its bounds, which is a
gentler curve that is still a curve.

**The clipping guard stopped guessing.** A fixed order - whites, then contrast, then curve -
spent all four of its attempts halving a white point on the one fixture where the *curve* was
the cause. It now tries each candidate, measures, and reduces the one that helps.
