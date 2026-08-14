# AURA-ML-5023 - No scene profile, so neutral tolerances were used

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence, once per missing scene per project. Nothing stops.

## What actually happened

`StoryService::profile` was asked for a scene that has no row in `scene_profiles` for this project, and returned `SceneProfile::neutral`.

The function is deliberately **infallible**. A scene that cannot be graded takes a wedding out of the product; a scene graded neutrally does not. So the missing row is a warning with a substitution, not a refusal.

Three ordinary causes:

1. **A scene was added to `SceneId` without a profile row.** The most likely one, and a test in `crates/aura-brain-wedding/tests/profiles.rs` exists to catch it before a release: it asserts the registry is total over `SceneId::ALL` minus `Unknown`.
2. **The project's profiles were never loaded.** `scene_profiles` is populated per project on the first classification pass. A catalog migrated from schema 6 that has not been re-analysed has an empty table.
3. **A photographer deleted an override row by hand.**

## What AURA does automatically

Substitutes `SceneProfile::neutral` - keeper band 0.20 to 0.60, noise 0.50, blur 0.35, weights 0.35 / 0.30 / 0.25, `EditIntent::Neutral`, `must_cover` false - logs this code once per scene per project, and continues. Every later phase reads the substituted profile like any other, so the wedding is judged consistently, just not specifically.

## Operator steps

1. `SELECT scene FROM scene_profiles WHERE project_id = ?` and compare with `SceneId::ALL`. The gap is the answer.
2. If the whole table is empty, re-run classification for the project; loading is idempotent and does not overwrite `user_edited = 1` rows.
3. If one scene is missing from the shipped file, that is a bug in `crates/aura-brain-wedding/config/scene_profiles.toml` - add the row **with a rationale**, or the loader will refuse it with `AURA-ML-5024`.
4. Neutral is not a good long-term answer for `dance_floor` or `family_portrait`; those two are where the difference between neutral and tuned is largest, and a wedding culled with neutral dance-floor tolerances loses frames it should keep.

## Related

- Error registry: `crates/aura-core/errors.toml`
- The shipped profiles and their rationales: `crates/aura-brain-wedding/config/scene_profiles.toml`
- Why every value needs a rationale: `docs/adr/ADR-0015-wedding-scene-taxonomy-and-story-segmentation.md` section 7
