# AURA-RENDER-8004 - The XMP sidecar could not be read

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing changes about the photograph. A note says the Lightroom settings file next to it
could not be read and AURA used its own copy.

## What it means

`<image>.xmp` exists but does not parse, or carries no `crs:` block. AURA's own
`<image>.aura.json` is the source of truth for every edit this product made, so the render
is complete and correct; what is lost is any edit made *in Lightroom* since the last time
AURA read the file.

## Operator steps

1. Open the XMP in a text editor. It is XML; a truncated file is the common cause, usually
   from a sync client copying it mid-write.
2. If the photographer has been editing in Lightroom, ask them to re-save metadata to the
   file from Lightroom, which rewrites the XMP.
3. `aura_recipe::xmp::reconcile` is what compares the two documents when both are readable;
   fields where they disagree are treated as user edits and protected from then on.
4. AURA never writes over an XMP it could not read. The file on disk is exactly as it was.
