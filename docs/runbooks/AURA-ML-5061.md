# AURA-ML-5061 - An exposure or white-balance override was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

The slider snaps back. Nothing was recorded and nothing was rendered.

## Common causes

* The photograph has no tone estimate yet, so there is nothing to override.
* The override was empty - all three of exposure, temperature and tint were absent.
* A value was outside its documented range: exposure outside -5..5 stops, temperature
  outside 2000..50000 K, tint outside -150..150.

## Operator steps

1. Confirm the photograph has been through the tone pass (`ToneService::of_image`).
2. Re-read the estimate and redraw the panel. Every refusal case is answered by that, which
   is why the recovery is `ask_user` rather than a retry.

Recording the override and applying it to the pixels are **two writes**, deliberately: this
table records the disagreement, and `aura_recipe::schema::merge` moves the pixels. A refusal
here means neither happened.
