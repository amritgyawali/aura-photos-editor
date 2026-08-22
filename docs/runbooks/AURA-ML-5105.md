# AURA-ML-5105 - A restoration profile or camera noise-model file was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

No noise reduction anywhere in the project, and a message saying the settings could not be
loaded.

## What actually happened

Two kinds of file can trigger this:

* `crates/aura-restore/config/restore_profiles.toml`, the 22 scene rows that cap how far each
  kind of photograph may be denoised and sharpened;
* any file under `crates/aura-restore/config/noise_models/`, one per camera body.

Both are loaded once at construction and both are refused rather than partially applied. The
loader checks that every ceiling is inside the bound the *code* owns - a config file may only
lower a ceiling, never raise one - and that every noise model passes `NoiseModel::problem`.

**Run-blocking rather than degraded, deliberately.** A missing scene row falls back to the
neutral row and reports `restore_tier_capped_by_scene`; that is a different situation. This code
means the table itself would not parse, and continuing would mean denoising against whatever
defaults happened to be compiled in.

## What to do

1. The message names the file and the row. A ceiling above the bound the contract owns is the
   commonest cause, and the message says which one.
2. Reinstall to restore the shipped files. They are compiled into the binary as a fallback, and
   the on-disk copies exist so that a studio can lower a ceiling.
3. `just phase-22-verify` loads both tables and fails on the same conditions.
