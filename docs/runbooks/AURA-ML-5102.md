# AURA-ML-5102 - Stored restoration plans came from different heads, arithmetic or profile tables

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

A progress line saying AURA is re-checking the wedding, and nothing else. Existing edits stay.

## What actually happened

Three version numbers key every stored `restore_plan` row:

| Column | Invalidates |
|---|---|
| `model_ver` | every learned decision - the denoiser and the face-recovery head |
| `analysis_ver` | the tier arithmetic, the kernel estimate, the self-check measurements |
| `profile_ver` | the scene ceilings and the per-camera noise models |

A build whose numbers differ from the stored ones is a build that would be comparing its own
tier against a tier chosen under different rules. Phase 05 wrote the rule and this is the ninth
code enforcing it: a comparison across a version boundary returns a plausible number that means
nothing, and it must never happen silently.

`RestoreStore::pending` is a query over exactly these three columns, so the re-check is the
ordinary resumable pass rather than a special path. Rows a photographer has edited keep
`user_edited = 1` and are not overwritten.

## What to do

1. Nothing. Let the background pass finish.
2. If it does not finish, `aura-cli verify --phase 22` reports the stored versions and the
   running ones side by side.
3. If the drift is `profile_ver` alone, the change was to `restore_profiles.toml` or to a file
   under `config/noise_models/`. Both are editable and both bump the version deliberately.
