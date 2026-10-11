# Paired reference validation

## Residual dark-mark correction

The second compact-repair pass previously admitted only inflamed marks and
targeted redness, even when the photographer explicitly enabled Remove dark
marks. It now honors that choice. Neutral candidates still have to pass the
planner's compact-shape, surrounding-light, wider-context and clean-donor checks.
Default dark-mark preservation, eye/nose guards and the exclusion of previous
repair centres remain in place. The total per-face repair limit is applied after
that exclusion so discarded candidates do not consume the remaining allowance.
The planner version is `measured-features-v21`; existing saved recipes are not
rewritten.

Cleanup passes now also bound compact spot planning: one keeps the first pass,
two retains the usual one residual check, and four permits three residual checks.
Each check renders all preceding feature-protected repairs, excludes their
footprints and searches the remaining skin. Checks stop early when no eligible
candidate remains or the per-face limit is reached.

The corrected Windows executable has SHA256
`7FBE5CA56AC4A6B0315C9F5B3F7AEFAE4B69678F4AB29BA145325C30E5E95BEA`.
The same offline three-face input produced 1024 operations, including 1005 compact
repairs, compared with the baseline's 631 operations and 612 compact repairs.
Retouch took 25.50 seconds and verified JPEG export/readback took 14.40 seconds.
The lossless PNG again matched the full native view/render, all 12 measured
eye/nostril regions remained pixel-identical, and both public inputs stayed
unchanged. Evidence is in `output/internal-ai-20261011/residual-final-beauty`.
The grouped photo reached the existing 1024-operation recipe limit: the report
discloses that only 147 of the third face's proposed spot repairs were retained.
The additional candidates were not saved; the limit was not raised or hidden.

A separate 2048 x 3072 heavy-acne portrait produced 768 operations, including
759 compact repairs. Retouch took 23.20 seconds and verified JPEG export/readback
took 27.20 seconds. The lossless export matched the native full render/view, the
original stayed unchanged, and four eye/nostril regions stayed pixel-identical.
Both the whole portrait and an unscaled 869 x 652 detail crop were inspected.
The result is cleaner around the cheeks and nose while retaining opening shape,
lashes, eyes and local lighting. Residual forehead/nose redness remains.
Evidence is in `output/internal-ai-20261011/residual-final-acne` and
`residual-final-acne-full`.

The matching 1066 x 1600 selection-grid diagnostic passed separately. Nose skin
was 72.91% selected; wing changes were confined to saved compact repairs, with
zero broad selection there. The inspected contour rectangle had 0.7866% maximum
relative coarse change, below the unchanged 1% gate, and broad rendered pixels
there were unchanged. Visible body skin was selected and changed, with no body
coverage inside detected faces. Diagnostic writes were undone to the exact
saved recipe hash. Evidence is in `output/internal-ai-20261011/residual-final-protected`.

The corrected executable is installed as `app/aura-desktop.exe` and opens the
working catalogue with all 13 saved recipe hashes intact, 16 configured AI routes
and no observed page errors. The prior C0 executable is retained locally. The
working catalogue and its C: backup pass SQLite integrity and saved-recipe checks.

All three corrected face crops were visually inspected. Additional cheek marks
are reduced without visible loss of the nose contour, eyes, teeth or expression.
Some under-eye marks and uneven tone remain; the published reference is still
more even there. The increase in repair count does not establish full removal,
accuracy on arbitrary photographs or superiority. Application tests passed:
181 tests, with two optional local-image diagnostics ignored. Strict workspace
Clippy, formatting and the banned-code check passed.

## Baseline native result on 2026-10-11

The Windows executable with SHA256
`C0ACC1D705C064AB989427E782289B94D8FA711BC3796C145593B5E99A229764`
processed the public beauty before image at 2000 x 1200 in an isolated catalogue,
with offline privacy enabled before import. Three faces produced 631 saved native
operations, including 612 compact repairs. Retouch took 18.81 seconds; the JPEG
export/readback took 12.59 seconds. The additional lossless PNG matched the full
native view/render. All 12 measured eye/nostril regions remained pixel-identical,
and both public input hashes stayed unchanged.
This baseline executable is retained locally for recovery. The working catalogue
and its C: backup pass integrity checks with all 13 saved recipe hashes unchanged.

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
