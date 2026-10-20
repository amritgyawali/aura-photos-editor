# AURA optical and colour tools — 19 October 2026

Four additional native retouch tools are implemented. Open **Auto edit → choose
a photo → Retouch**, then choose a tool or search its name. Each uses the existing
editable operation stack, strength, selections, preview, Undo/Redo and export.

| Tool | Controls | Behaviour and limits |
|---|---|---|
| Directional motion blur | Travel 0.05–5% of the short edge; angle −180–180° | Integrates a straight motion trail using selected pixels only. Adds motion; does not correct camera shake. |
| Radial zoom blur | Travel 0.5–50%; centre X/Y or click on the photo; reset to image centre | Motion increases with distance from the centre. The exact centre remains fixed. Manual focus positioning; no inferred depth. |
| Colour spill suppression | Spill colour, hue selectivity and strength | Reduces chroma aligned with the chosen spill hue while preserving linear luminance. Rejects grey/white/black samples. Protect legitimate colours with a mask; no semantic distinction between a colour cast and an object of that colour. |
| Highlight compression | Threshold 0–4 stops above middle grey; compression and strength | Smoothly compresses luminance above the threshold while preserving RGB proportions. Lower tones remain unchanged. Cannot recover detail already clipped in the source. |

Use a painted mask, ellipse, gradient, luminance range or a previously detected
region. The blur filters exclude protected source pixels from their sampling,
including at coarse pyramid levels. A detected-region selection preserves the
zoom centre chosen for the current photo. The centre picker leaves Hand mode and
read-only comparison/coverage views before accepting a position.

Presets save tool parameters and the spill hue, but omit image-specific centres,
sources and masks. Loading a preset retains the current photo's selection and
coordinates; review the centre before applying a zoom preset.

## Implementation

- `aura-recipe` validates four explicit operation variants; malformed colour
  targets and out-of-range coordinates are rejected before saving.
- `aura-render` computes effects in linear Rec.2020. Blur uses immutable,
  coverage-premultiplied pixels, a normalized line integral and at most 65 taps.
  A mip pyramid prefilters longer trails to avoid sparse repeated copies.
- Despill uses a hue-aligned chroma projection, smooth selectivity and a uniform
  correction limit to avoid creating negative channels from nonnegative inputs.
- Highlight compression uses a continuous, monotonic shoulder above its threshold.
  Disabled operations, zero strength and protected pixels remain unchanged.
- Rows render independently with a fixed sample order. Storage scales with the
  selected rectangle and its pyramid. Full-resolution speed and memory budgets
  on all reference machines remain unqualified.
- All four tools are explicit manual operations, excluded from automatic portrait
  stage assignment. No new dependency, model, network call, schema migration or
  frozen-contract change is needed. Originals remain read-only.

Design: [ADR-0128](../adr/ADR-0128-selected-optical-and-colour-tools.md).
Older binaries cannot decode these four new variants. Remove the new steps using
the current binary before reopening affected recipes in an older build.

## Validation

| Check | Measured result |
|---|---|
| Full UI suite | 818 passed in 96 files |
| Final targeted editor tests | 84 passed in four files, including click-to-position after Hand/coverage mode |
| Full recipe and renderer suites | 475 passed, 12 ignored |
| New native optical/colour signal tests | 10 passed, included in the full native count |
| Native integration | Both material and optical workflows passed: import, save, actual pixel changes, Undo/Redo, invalid-operation rejection, verified JPEG export and source-byte preservation |
| Frontend production build | Passed; existing bundle-size warning remains |
| Frozen contracts | All 83 entries locked; no relock |
| Bundled desktop build | Passed with `custom-protocol`; existing multiple-manifest linker warning remains |
| Rebuilt desktop app | Eight checks passed, including all four tools, centre picking, neutral-spill rejection, Undo/Redo and source preservation |
| Strict local renderer/recipe lint | Recipe passes; renderer blocked by two existing findings in skin connectivity and under-eye rendering |

The full UI run preceded the last centre-picker refinement; all four affected
editor test files and the production build were rerun afterward. Native tests
ran after the final colour-validation helper refactor. The twelve ignored native
tests require opt-in fixtures, performance runs or hardware and are not passes.

Signal tests check blur direction and energy, fixed zoom centre, long-trail
prefiltering, excluded-source protection at coarse levels, constant-colour and
soft-mask behaviour, despill luminance/hue/gamut, monotonic highlight compression,
recipe round trips, zero/disabled identity, erasure and determinism.

Strict dependency-inclusive Clippy additionally reports existing RAW decoder
findings. The local `--lib --no-deps` check reports only
`retouch_skin/connectivity.rs:37` (`needless_pass_by_value`) and
`retouch_undereye.rs:152` (`too_many_lines`); no finding remains in the new optical
renderer or recipe validation. No lint rule was disabled to obtain that result.

Logs are under `.work-checks/aura-optics-*`.

The rebuilt executable was launched with a fresh isolated catalogue and a
synthetic fixture containing dark, textured, bright and green regions. Each tool
was authored through the WebView controls and changed native rendered pixels.
The zoom picker was exercised after Hand mode, with numeric bounds checked too.
The spill-colour input received DOM change events because the OS colour dialog
is outside browser automation; rejection and native serialization were checked.

Four Undo operations restored the exact initial recipe hash and rendered pixels;
four Redo operations restored the exact edited hash and pixels. Source PNG
SHA-256 matched before and after. Screenshots, before/after renders and structured
results are under `.work-checks/optical-effects-2026-10-19/desktop-201501/`.

The updated executable is `target/desktop-launch/debug/aura-desktop.exe`.
Open **Start AURA.cmd** to use it. The isolated verification app was closed.

## Coverage

The [feature ledger](AURA-feature-coverage-2026-10-19.json) records the motion-blur
route as a manual partial implementation. [Evoto's public motion-blur page](https://www.evoto.ai/features/motion-blur-effect)
also describes automatic subject separation; that broader workflow has not been
established by this addition. Zoom blur, despill and highlight compression are
recorded as additional AURA capabilities without claiming direct Evoto parity.

Synthetic signal tests and desktop workflow checks establish implementation
behaviour. They do not establish photographic preference over Evoto or complete
coverage of its tools. Representative photographs and independent visual review
remain necessary for those claims.
