# AURA-ML-5019 - Not enough usable faces to group anybody

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence in an empty People panel, with the two things that would change it: analyse more photographs, or lower the face quality filter.

## What actually happened

Grouping needs at least `aura_people::api::MIN_VOTING_FACES` - four - faces that passed the quality gate. Below that, "clustering" is a description of two or three photographs, and a confident grouping of a wedding nobody has analysed yet is worse than an empty one.

Four ordinary causes, in the order they actually occur:

1. **The face pass has not run yet**, or has only run over one card. Check the panel's coverage line, or `SubjectHierarchy::coverage`.
2. **This wedding genuinely has few faces so far** - a morning of details, venue and dress frames.
3. **The quality gate excluded almost everything.** Two numbers do that: 48 source pixels and 0.4 fused usability. A ceremony shot entirely from the back of a large room can put every guest below the first.
4. **The recogniser is unavailable**, so no face has a template. Detection, landmarks, pose and bodies are all still stored - the boxes are there and the names are not. Look for `AURA-ML-5001` or `AURA-ML-5002` above.

## What AURA does automatically

Reports the code, returns an empty `SubjectHierarchy` with honest coverage, and continues. Every later phase reads `coverage` rather than assuming, so a cull that runs now weights by technical quality alone instead of by subject - and says so in its own reasons.

## Operator steps

1. Check coverage first. An unscanned wedding is not a grouping problem.
2. Look at the per-face reasons in the People panel's quality filter. Each excluded face carries its own sentence - "soft: sharpness 0.21", "38 px tall, below the 48 px identity gate" - and the dominant one tells you which of the four causes it is.
3. If the whole wedding is small-face, the tiled pass is what recovers it. Confirm it is firing: `face_scan.tiled` per frame, and `ScanReport::tile_ratio` for the pass. `should_tile` needs either several small detections on a wide lens, or bodies with no faces.
4. Lowering the gate is a legitimate answer for one specific wedding and a bad default. A lower gate admits faces that chain-merge, which is the failure the gate exists to prevent.

## Related

- Error registry: `crates/aura-core/errors.toml`
- Why the gate is where it is: `docs/model-cards/face_quality.md`
- Detection recall: `docs/model-cards/face_detect.md`
