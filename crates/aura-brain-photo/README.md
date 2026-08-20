# aura-brain-photo

Local, deterministic judgement about one photograph. The crate owns four passes:

* `integrity` (PHASE-09) measures subject-aware sharpness, motion, exposure, noise, and eye
  state;
* `composition` (PHASE-11) measures horizon, crop boundaries, placement, balance, and visible
  background competition, then emits bounded aesthetic evidence and crop hints;
* `tone` (PHASE-15) decides what colour the light was and how bright the people in it should
  be;
* `local` (PHASE-19) decides how the light inside one photograph should be shaped: which faces
  to lift, how far to separate the subject from what is behind it, where to deepen form and
  where to take shine off.

The first two describe a photograph and act on nothing. The second two decide, and reach the
pixels only through `aura_recipe::schema::merge` - this crate depends on neither `aura-recipe`
nor `aura-render`, which is what keeps a decision from becoming an edit without passing through
the one function that honours `user_edited_fields`.

Consumers use the frozen contracts in `aura-core`; they do not call analyser internals or query
the tables directly.

## Local light data flow

`LocalPass` requests one 2048 px proxy, reads people, scene, noise and framing through frozen
services, reads phase 18's masks through `MaskField` - and gates every operation that has none.
`Analyser` measures the frame once, solves every face together, pairs the subject enhancement
with a matching background reduction, separates three frequency bands and returns two, places
the shaping zones and finds the specular sheen, then spends all of it against one per-image
perceptual allowance. `LocalStore` writes the versioned plan.

**This crate contains no mask generator and no fallback that draws one.** Phase 18 owns masks;
a rectangle's edge does not follow a person, and an edit through one leaves the bright rim the
phase exists to avoid. See `docs/adr/ADR-0033-local-light-sculpting.md` section 4.

Policy is in `config/local_light.toml`, with a written reason on every row. Changing a strength
bumps `policy_ver`; changing arithmetic bumps `ANALYSIS_VER`; changing how shaping zones become
a grid bumps `SHAPING_VER` - and that last one moves delivered pixels without moving a stored
number, which is why it exists. The public vocabulary is documented in `docs/local-light.md`.

## Composition data flow

`CompositionPass` requests one 2048 px proxy, reads optional people and scene context
through frozen services, runs `Analyser`, stores the versioned `CompositionResult`, and
refreshes relative-within-moment ranks after siblings are available. Missing geometry,
scene rules, horizons, and trained aesthetic provenance remain explicit caveats.

Rules are in `config/composition_rules.toml`. Changing a band or penalty bumps
`rules_ver`; changing arithmetic bumps `ANALYSIS_VER`; changing trained model behaviour
bumps `MODEL_VER`. The public vocabulary and overlay semantics are documented in
`docs/composition-and-framing.md`.

## Verification

```text
cargo test -p aura-brain-photo --test composition_eval -- --nocapture
cargo test -p aura-brain-photo --test local_eval -- --nocapture
cargo run --release --package aura-cli -- verify --phase 19 --work target/phase19-verify
cargo test -p aura-brain-photo
cargo clippy -p aura-brain-photo --all-targets -- -D warnings
python ml/models/composition/eval_composition.py --self-test
python ml/models/composition/export.py --check
```

The Rust evaluation set is synthetic and authored. Its metrics prove deterministic
geometry and regression guards; they do not establish real-wedding model quality. See
`docs/progress/PHASE-11-EXIT.md` for the open evidence conditions.

