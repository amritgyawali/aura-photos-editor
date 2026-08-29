# AURA-ML-5112 - A lens profile database or crop rule file was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing is straightened, cropped or lens-corrected anywhere in the project, and a message saying
the settings could not be loaded. No photograph is altered.

## What actually happened

Two kinds of file can trigger this:

* `crates/aura-geometry/config/crop_rules.toml`, the 23 scene rows that decide whether a scene may
  be cropped automatically at all and how tightly;
* `assets/lens_profiles/profiles.toml`, the lens profile database.

Both are loaded once at construction and both are refused rather than partially applied. The
loader checks that **a file may only tighten a bound the code owns, never loosen one**: the
resolution floor, the improvement margin, the safety margin and the rotation ceiling all live in
`aura_core::contract::geometry` as constants, and a row that raises any of them is a row that
would let a studio quietly switch off the guarantee in `docs/geometry.md`.

**Run-blocking rather than degraded, deliberately.** A scene with no row falls back to the neutral
row, which forbids automatic cropping entirely - that is a different and safe situation. This code
means the table would not parse or would have widened a bound, and continuing would mean cropping
against whatever defaults happened to be compiled in.

## What to do

1. The message names the file, the key and the rule that refused it.
2. Reinstall to restore the shipped files. They are compiled into the binary as a fallback, and
   the on-disk copies exist so a studio can *tighten* a rule.
3. `just phase-23-verify` loads both tables and fails on the same conditions, including four
   explicit checks that a loosened bound is refused.
