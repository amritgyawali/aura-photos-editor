# AURA-ML-5107 - The artefact self-check made a restoration gentler, or withdrew one

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Some photographs denoised or sharpened less than others, carrying
`restore_tier_reduced_by_self_check`, `restore_amount_reduced_by_self_check` or
`restore_sharpen_withdrawn` in the panel, with the measured number beside them.

## What actually happened

**This is the mechanism working.** It is registered as an error so that it is visible, not
because something is wrong.

`selfcheck::enforce` applies the plan through the real renderer - the same code the delivered
JPEG goes through - and measures two quantities on the result:

| Measurement | Held to | What it protects |
|---|---|---|
| `texture_retention` | `MIN_TEXTURE_RETENTION` | fine structure in lace, fabric and hair |
| `ringing` | `MAX_RINGING` | edges, against the pale outline of over-deconvolution |

The two are fixed by two different parameters, which is why they are two numbers rather than
one: smearing is fixed by stepping the denoise tier down, and ringing by reducing the sharpen
amount. A single score would leave the automatic reduction with no way to know which lever to
pull. ADR-0045 section 2.1.

A measurement that misses its bound reduces its own parameter and is measured again, up to three
times. Sharpening that still rings after three attempts is **withdrawn**; denoising steps down a
tier at a time and reaches `Off` in the limit.

## What to do

1. Nothing, usually. `RestoreOutline::mean_texture_retention` and `mean_ringing` are the numbers
   to watch across a project.
2. A whole scene reducing is worth a look at that scene row in `restore_profiles.toml`.
3. A frame that matters and could not be sharpened is a frame to sharpen by hand, and the panel
   says what the measured ringing was, so that the choice is an informed one.
