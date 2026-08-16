# aura-brain-photo

Local, deterministic evidence about one photograph. The crate currently owns two passes:

* `integrity` measures subject-aware sharpness, motion, exposure, noise, and eye state;
* `composition` measures horizon, crop boundaries, placement, balance, and visible
  background competition, then emits bounded aesthetic evidence and crop hints.

Neither pass selects, rejects, edits, crops, straightens, or removes pixels. Consumers use
the frozen contracts in `aura-core`; they do not call analyser internals or query the
tables directly.

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
cargo test -p aura-brain-photo
cargo clippy -p aura-brain-photo --all-targets -- -D warnings
python ml/models/composition/eval_composition.py --self-test
python ml/models/composition/export.py --check
```

The Rust evaluation set is synthetic and authored. Its metrics prove deterministic
geometry and regression guards; they do not establish real-wedding model quality. See
`docs/progress/PHASE-11-EXIT.md` for the open evidence conditions.

