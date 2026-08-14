# AURA-ML-5024 - A scene profile or ritual taxonomy file was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The registered sentence. This one **stops**, and it is the only phase 07 code that does.

## What actually happened

`SceneProfileRegistry::load` or `Taxonomy::load` refused a config file. The loader is strict on purpose: these files decide how every photograph in every wedding is judged, and a half-loaded threshold table silently changes every downstream number.

The refusal reasons, all of which name the file and the offending key:

**Scene profiles** (`crates/aura-brain-wedding/config/scene_profiles.toml`)

1. **A profile with no `rationale`, or one shorter than nine characters.** Section 12's third failure mode is that this file becomes a dumping ground of magic numbers, so a value nobody can explain does not load. This is the most common cause and it is intentional friction.
2. `keeper_min > keeper_max`, or either outside `0..1`.
3. A weight outside `0..1`, or the three weights summing above 1.0 - which would let one scene's composite score exceed another's by construction.
4. An `editing_intent` that is not one of `airy`, `neutral`, `warm`, `moody`, `punchy`.
5. A `scene` key that is not one of the 22 in `SceneId`.

**Ritual taxonomies** (`crates/aura-brain-wedding/config/rituals/*.toml`)

6. **Two rites sharing an `id`, across any pair of files.** The id is authored rather than derived precisely so that inserting a rite does not renumber the rest; a duplicate defeats that and would relabel stored rows.
7. An `id` of 0, which is reserved for `RitualId::NONE`.
8. A missing `slug`, `tradition` or `title`.

## What AURA does automatically

Nothing. It refuses to load and leaves whatever was loaded before in place. A partially applied threshold table is worse than the previous one, and `Recovery::Halt` is registered for exactly that reason.

## Operator steps

1. The error's detail carries the file, the key and the rule. Fix that one thing first; the loader reports the first failure rather than all of them, because a config file with four problems usually has one cause.
2. `cargo test --package aura-brain-wedding profiles` runs the same loader over the shipped files. Run it before shipping a taxonomy edit; it is faster than finding out from a photographer.
3. If the shipped file is intact and a project override is the problem, delete the override rows: `DELETE FROM scene_profiles WHERE project_id = ? AND user_edited = 1`. The next pass reloads the defaults.
4. **Do not relax the rationale check to make this pass.** It is the only thing standing between this table and a hundred unexplained numbers.

## Related

- Error registry: `crates/aura-core/errors.toml`
- `docs/adr/ADR-0015-wedding-scene-taxonomy-and-story-segmentation.md` sections 3 and 7
- Adding a tradition: `docs/adding-a-tradition.md`
