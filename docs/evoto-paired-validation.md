# Paired reference validation

## Moderate redness beside uneven light

Planner `measured-features-v23` lets the existing local pigment fallback consider
a redness peak above 0.04 instead of 0.06. Native traces found moderate nose marks
with whole-component redness above 0.025 and clean enclosing skin that the old
peak threshold rejected. The independent requirements remain: component mean
above 0.025, centre redness exceeding the healthy-ring median by 0.025, a complete
selected ring with at least ten healthy samples and two in every quadrant, safe
skin coverage and a clean donor. This path applies only when surrounding light
does not support texture replacement. It corrects local chroma at unchanged
working-space luminance; it does not rebuild that shadow's texture.

The regression case fails with the old peak threshold. It now checks an actual
rendered colour correction, luminance preserved within 0.000001, exact pixels
outside the repair, a clean shadow that receives no pigment correction, and an
inflamed centre without enclosing selected healthy skin that is rejected.
Application library: 184 passed, two optional image diagnostics ignored. Strict
workspace Clippy, formatting and banned-code checks pass. Existing saved recipes
are not rewritten; repair budgets and prior-footprint protection remain intact.

Native Windows validation on 2026-10-12 used executable SHA256
`07169C7C8E6EB4FA57A7B4DFFC3CC648396BD6865469334F2778D614C7471E1B`,
with offline privacy enabled in an isolated catalogue before import:

| Input and preset | Operations | Texture repairs | Local pigment repairs | Under-eye corrections | Unchanged full-resolution feature regions |
| --- | ---: | ---: | ---: | ---: | ---: |
| 2048 x 3072 heavy acne, Portrait skin finish | 780 | 757 | 7 | 2 | 4 |
| 2000 x 1200 public beauty, Portrait skin finish | 1024 | 977 | 3 | 4 | 12 |
| Heavy acne, Acne only | 638 | 635 | 1 | 0 | 4 |

Each verified JPEG was read back. Each additional lossless PNG matched both the
native full render and native retouch view; all original and reference hashes
stayed unchanged. All three public face crops, the heavy portrait and unscaled
eye/nose/cheek crops for both presets were inspected. No new hard repair edges or
obvious anatomy changes were seen. In the public group, the retained per-face
spot counts are 279, 499 and 202. The middle face discloses 499 of 581 proposals;
the recipe remains capped at 1024 operations.

Compared with the same preset in A741, the traced 42 x 32 nose region has a
4.09% lower mean normalized red signal. The measured forehead and glabella
regions are unchanged. This is a modest local improvement, not a measurement
of total acne removal. Blackheads, forehead marks, protected marks and some
uneven tone remain, and the published Evoto result still differs.

The acne-only native coverage test selected 70.72% of its landmark nose sample.
Eye regions and nostril openings had zero changed or selected pixels. Nose-wing
changes were confined to saved compact repairs, with no broad selection there.
The independently inspected contour rectangle had a maximum coarse relative
change of 0.4002%, below the existing 1% limit; its broad render stayed unchanged.
Visible body skin selected 88,192 pixels and changed 22,151 in the 1066 x 1600
inspection plane, with face holes retained. Diagnostic writes were undone to
the exact saved recipe. Hands and legs are absent from these photographs.

Native UI Undo/Redo restored the exact recipes and native pixels for both
finishing photographs, with no observed page errors. The initial history
harness was interrupted after its CDP client stopped responding; the repeated
check after a graceful test restart passed. A separate normal restart restored
the exact saved recipes and full-resolution RGB hashes for all three workflows.
Evidence: `pigment-history.json` and `pigment-restart/verification.json`.

The validated 07169 executable is installed at `app/aura-desktop.exe`, with the
previous A741 executable retained for recovery. The working launcher pins its
tested hash. The working catalogue and C: backup retain all 13 original saved
recipe hashes and pass integrity checks; installation first checked for an
unsaved retouch draft and closed the owned sessions normally.

Ignored evidence is in `output/internal-ai-20261011/pigment-acne`,
`pigment-acne-full`, `pigment-beauty`, `pigment-acne-only`,
`pigment-acne-only-full` and `pigment-protected`. Reference images and generated
exports are not bundled or committed.

## Portrait finishing and group repair allocation

The adjustable `Portrait skin finish` preset combines four-pass face/body
healing with measured skin-tone, light, redness, shine and dark-circle correction.
It retains fine texture, eye/nose guards and neutral global grading. Iris,
eye-white, teeth, contour and expression changes remain disabled. The under-eye
correction compares each person's shadow with their own cheek; it does not
replace any pixels with the published reference.

Compact repairs now share the remaining 1024-operation allowance across faces.
Required finishing and existing manual/scene operations are reserved first.
Smaller repair plans receive their full allowance; larger plans share the
remaining slots equally, with stable face order breaking ties. Detector priority
within each face is preserved. Retained proposals are trimmed after each person
to bound memory, and the report counts saved repairs and discloses discarded
proposals. No recipe limit or feature guard was relaxed. Planner version:
`measured-features-v22`; previously saved recipes remain unchanged.

The paired harness accepts `--portrait-finish` to exercise the native UI preset.
This explicit mode checks the asymmetric lid cores separately from deliberately
corrected under-eye skin. Skin-only comparisons retain the broader eye/socket
checks. Nostril checks, source hashes, unchanged recipes and exact full-resolution
export/render/view equality remain required in both modes.

Native Windows build SHA256:
`A741A74C8FDF7F92CB4F01EED175CD3088F2E98648299E7C5DB11847D3025B03`.
The same public 2000 x 1200 beauty input produced 1024 operations: 979 compact
texture repairs, one localized pigment correction and four measured under-eye
corrections, alongside native skin finishing. The three faces retained 278,
501 and 201 spot operations. The last face now keeps its full proposed plan;
the middle face's report explicitly discloses 501 of 581 retained proposals.
Retouch took 36.28 seconds and verified JPEG export/readback took 28.92 seconds.
Lossless PNG matched the full native render/view, both public inputs stayed
unchanged, and all 12 measured lid/nostril regions were pixel-identical.
Evidence: `output/internal-ai-20261011/portrait-finish-beauty`.

The separate 2048 x 3072 heavy-acne portrait produced 778 operations, including
759 texture repairs and two under-eye corrections. Retouch took 35.47 seconds;
verified JPEG export/readback took 33.79 seconds. The original stayed unchanged,
lossless PNG matched the native full render/view, and all four measured
lid/nostril regions were pixel-identical. The full portrait and an unscaled
920 x 700 eye/nose/cheek crop were inspected. Evidence:
`output/internal-ai-20261011/portrait-finish-acne` and `portrait-finish-acne-full`.

All three paired face crops were inspected at their delivered dimensions.
Cheek/forehead marks and measured under-eye shadows are reduced, and the last
face receives additional cleanup without changing its expression, eyes or nose
shape. No obvious hard repair boundaries were seen in these crops. Protected
marks, under-eye lines and some uneven tone remain; the heavy-acne photograph
still has forehead/nose redness. The published Evoto result is different.
These observations do not establish exact matching, complete removal, universal
accuracy, full feature parity or superiority.

Application library: 183 passed, two optional diagnostics ignored. UI: 745
passed across 87 files with one worker; a repair-list test timed out under
concurrent builds, then passed alone and in the serial suite. UI build, strict
workspace Clippy, format, Python syntax and banned checks passed.

Native UI Undo/Redo restored exact recipe bodies, recipe hashes and rendered
pixels for both finishing test photos, with no observed page errors. A normal
restart restored the same recipe hashes and full-resolution RGB hashes for
both photos. Evidence: `output/internal-ai-20261011/portrait-finish-history.json`
and `portrait-finish-restart/verification.json`.

The validated A741 executable is installed as `app/aura-desktop.exe` and opens
the working catalogue with all 13 original saved recipe hashes intact, 16
configured AI routes and no observed page errors. The prior 7FB executable is
retained locally for recovery. The working catalogue and its C: backup pass
SQLite integrity checks. The launcher pins the validated executable hash.

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

At this earlier validation, the 7FB executable was installed and opened the
working catalogue with all 13 saved recipe hashes intact, 16 configured AI routes
and no observed page errors. The prior 7FB executable is retained locally. The
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
This baseline is historical evidence; the newer 7FB executable is retained for recovery. The working catalogue
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
