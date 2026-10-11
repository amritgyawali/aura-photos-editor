# Paired reference validation

## Native result on 2026-10-11

The Windows executable with SHA256
`C0ACC1D705C064AB989427E782289B94D8FA711BC3796C145593B5E99A229764`
processed the public beauty before image at 2000 x 1200 in an isolated catalogue,
with offline privacy enabled before import. Three faces produced 631 saved native
operations, including 612 compact repairs. Retouch took 18.81 seconds; the JPEG
export/readback took 12.59 seconds. The additional lossless PNG matched the full
native view/render. All 12 measured eye/nostril regions remained pixel-identical,
and both public input hashes stayed unchanged.
This validated executable is installed as `app/aura-desktop.exe`; the previous
validated executable is retained locally for recovery. The working catalogue and
its C: backup pass integrity checks with all 13 saved recipe hashes unchanged.

All three face crops were inspected at their delivered dimensions beside the
published after image. AURA reduced forehead and cheek marks while retaining
eyes, teeth, expression and nose contours. Residual cheek marks remain, and the
published Evoto example has more even cheek and under-eye tone and fewer visible
marks. This comparison does not establish complete removal or superior quality.
The published after also includes finishing beyond AURA's skin-only preset.

The rebuilt native review UI passed checking/preparation without saving, draft
protection, saving a focused four-pass repair, and Undo to the exact recipe hash.
The native color, solid-background and local-proportion tools also passed
full-resolution selection-boundary checks, exact-pixel Undo/Redo, verified export
and unchanged original hashes on a separate portrait.
Automated checks passed: 199 renderer library tests, four healing/review tests,
four color-discipline tests, review UI tests, strict workspace Clippy, formatting,
banned checks, all 318 IPC names and all 83 locked contracts.

Ignored evidence is in `output/internal-ai-20261011/paired-beauty` and
`quality-review-ui`, with the color/proportion checks in `quality-color-shape`.
These local reference images are not bundled or published.

## Reproduce the comparison

`scripts/verify-evoto-reference.py` runs a published before image through the
native detailed skin preset, exports lossless PNG, checks source hashes and
protected eye/nostril regions, and writes an Original / AURA / published Evoto
comparison without rescaling. Run it against an isolated catalogue and WebView:

```powershell
python scripts/verify-evoto-reference.py before.webp after.webp output/paired-comparison --collection "Paired reference verification" --port 9363
```

The input WebP is decoded into an identical-pixel PNG because AURA's importer
does not accept WebP. The public reference files remain unchanged and outside
application assets. The comparison report distinguishes native workflow
verification from removal, feature-parity and superiority claims.

The current review retains the largest 256 candidates throughout the whole
selected area. Earlier code retained the first 256 before sorting, so small
marks near the top could hide a larger mark lower down. The regression fixture
contains 300 small marks followed by one larger mark; the larger mark now appears
first, total area/pixel counts remain complete, and ties retain deterministic
scan order. Review still never saves an edit or establishes complete removal.

Selected-color conversion and luminance now use the shared `aura-raw` working
space. Their arithmetic order is retained, while the renderer no longer reaches
into working-space primaries directly. The color-discipline integration gate
passes without an exemption or a weakened assertion.

The published [Evoto homepage](https://www.evoto.ai/) and
[portrait tools](https://www.evoto.ai/features/portrait-retouching) provide useful
reference targets. Their marketing exports do not include camera originals,
editing settings, model weights or a complete specification. Complete removal
requires independently annotated defects and review at full resolution across
a varied dataset. Superior quality requires a defined, matched comparison and
independent assessment. The [feature inventory](evoto-feature-inventory.json)
still contains absent or unverified workflows; full parity is not established.
