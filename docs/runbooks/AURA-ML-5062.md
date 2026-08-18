# AURA-ML-5062 - One photograph's exposure and white balance could not be estimated

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

One photograph keeps the camera's own as-shot settings. The rest of the wedding is
unaffected.

## What actually happened

The proxy would not decode, or the analyser failed on it. **No row is written**, which is
the whole point of the code: a frame stored with a neutral estimate would read to phases 16,
17, 25 and 27 as "AURA decided this photograph needed nothing", and all four act on that.
The absence of a row means nobody looked, and the next pass tries again.

## Operator steps

1. Read the `photo` context field and open that file.
2. A decode failure is a phase 02 problem, not a phase 15 one - check
   `docs/runbooks/previews.md` first.
3. Re-run the tone pass. It is resumable and will retry only the frames with no row.
