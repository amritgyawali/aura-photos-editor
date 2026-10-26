# Reference matching: first quality fixes after the deep assessment

Date: 2026-10-25. This follows the [360-case assessment](AURA-reference-look-colour-deep-assessment-2026-10-25.md). Its original images, measurements and failure report remain the before-change evidence.

**Final finding:** the rebuilt app improves the demonstrated sky result and
collection known-edit measurements, but it still cannot deliver a changed,
correct reference look for every tested photo. The original-output qualification
fails in Natural/Palette; detailed failures and implemented fixes are recorded
below.

## Changes

The native smoothness check now also measures perceptual colour steps in originally smooth regions across all brightness ranges. It retains the existing shadow guard and adds bounded OKLab checks over supported local tiles. A mostly unchanged image can no longer hide a banded bright sky or pale floor by averaging it into global support. Neighbourhood support excludes strong texture and real edges. The authored thresholds require both a perceptually large output step and strong amplification relative to the input; this remains a sampled guard, not an image-quality guarantee.

Single-photo reference uploads and edited-photo captures now measure native pixels instead of first resizing to 512 pixels. Display thumbnails remain bounded. Stored transfer measurements gain a separate reference-sampling version: old measurements remain readable, and reimport rebuilds their statistics and region descriptors from the original reference while preserving library identity and metadata. A preview cannot recover lost native colours; old references are not silently claimed to have native evidence. Captured-reference identities now hash the full edited image with a versioned native domain.

Matching retains a failed save even when the outer editor catches and displays its error. The matching panel therefore clears running progress, displays the refusal and releases controls for retry. The shared editor wrapper's other callers retain their error-handling behaviour. The same handling applies to one-click, explicit strength, live and collection operations.

Palette fitting is labelled `reference-palette-transport-v8`. Live matching and collection Sync refit older palette sessions rather than presenting their earlier analysis as the current fitter. All applied endpoints still pass native checks.

The first live retest showed that rejecting only the most extreme jumps still
allowed visibly banded softened endpoints. The perceptual gate was tightened and
the softened outputs were added to the regression review. A subsequent native
strength regression exposed independently backtracking 100% and 50% onto the same
operation. Distribution/registered candidates now qualify a safe full endpoint
before applying the requested amount and checking that actual result. The
existing half/full inequality assertion is retained and passes again.

## Validation

The main native workflow passes six active tests, including calibration, fine
striped colours, eight unrelated look families, strength/reset/replacement and
cancellation. The studio integration suite passes ten active tests, including
preview/export agreement, metadata/reopen, history, regional masks and manual
protection. Their optional external-photo/RAW tests are separate.

The focused colour unit run has 100 passes and one unchanged pre-existing Natural
batch-fidelity failure. It includes the optional archived-image regression: the
new guard rejects the recorded collection sky/floor and Palette floor artifacts,
accepts their authored smooth grades, and accepts the restrained Natural sky and
successful monochrome portrait. Synthetic checks retain real texture/object
edges and reject banding confined to a bright quadrant. A native fabric import
and legacy reimport regression passes.

All 62 tests across the matching panel, collection Sync and editor pass together
with one test worker. An earlier concurrent run hit the existing five-second
timeout in a history-navigation test during compilation; it passed in isolation
and in the final combined run without changing its assertion or timeout.
TypeScript/Vite production build, strict workspace Clippy, formatting, banned
patterns and the 353-command IPC surface check pass. Existing frontend bundle
size and GNU resource-manifest linker warnings remain.

## Final desktop assessment

Status: completed. Tested executable: `target/desktop-launch/debug/aura-desktop.exe`,
497,427,770 bytes, SHA-256 `11E175ED33BC0B0F6328049753A865E150F5C138223EA13EE5FECBAF6959C9C9`.
This is the final build containing the requested-strength endpoint correction.

[Final visual gallery](../../output/aura-main-match-twenty-2026-10-25/run-df0711e1/index.html),
[all desktop results](../../output/aura-main-match-twenty-2026-10-25/run-df0711e1/results.json),
[independent original-pixel qualification](../../output/aura-main-match-twenty-2026-10-25/run-df0711e1/original-output-verification.json),
[per-attempt original/tone/chroma CSV](../../output/aura-main-match-twenty-2026-10-25/run-df0711e1/original-output-verification.csv),
[before/after comparison](../../output/aura-main-match-twenty-2026-10-25/run-df0711e1/before-after-assessment.json).

The rebuilt native application received the same 20 targets, six unrelated
reference photographs and cyclic reference assignments as the original 60-case
run. All attempts request 80%; studio modes use Precise. Forty cases click the
actual Natural/Palette GUI; twenty use the collection reference IPC. The oracle
is not supplied to matching. Every workflow baseline is pixel-identical to its
recorded pre-fix counterpart. This is a final 60-attempt retest, not a repeat of
all 360 combinations on the new executable. Inputs remain JPEGs of at most 960
pixels on their long edge; high-resolution camera, RAW and HDR qualification is
outside this result.

| Workflow | Attempts | Final output differs from original | Material-change diagnostic | Reference stage changes pixels | Refused | Accepted unchanged reference stage |
|---|---:|---:|---:|---:|---:|---:|
| collection | 20 | 20 | 20 | 19 | 0 | 1 |
| natural | 20 | 10 | 10 | 10 | 8 | 2 |
| palette | 20 | 13 | 12 | 13 | 7 | 0 |

**The user's requirement does not pass across all modes/photos.** Seventeen of
60 final outputs are exactly the original: ten Natural and seven Palette.
Fifteen are native refusals; Natural also accepts two unchanged results. One
collection reference stage is unchanged even though automatic correction makes
its final output differ from the original. That gives eighteen attempts without
a reference-stage pixel change. A further changed Palette output (photo 11)
falls below the authored material-change diagnostic.

All twenty collection outputs pass that material-change diagnostic. Their
smallest mean RGB8 change from original is 1.871, and the smallest fraction of
changed pixels is 80.23%. This establishes changes in these files; it does not
establish that all twenty reproduce their reference looks.

| Workflow | Reference colour improved, before / after | Closer to known edit, before / after | Changed and close known edit, before / after | Median known-edit error, before / after |
|---|---:|---:|---:|---:|
| collection | 19 / 18 | 12 / 19 | 1 / 2 | 8.102 / 6.340 |
| natural | 15 / 10 | 14 / 10 | 2 / 2 | 6.086 / 6.267 |
| palette | 19 / 13 | 10 / 12 | 0 / 1 | 9.837 / 5.926 |

Known-edit closeness uses the previous authored mean CIE76 <=3 and p95 <=8
thresholds. The oracle uses encoded-sRGB blending, while AURA interpolates model
parameters; these definitions are not identical. Only five final changed,
accepted outputs pass both known-edit thresholds: two collection, two Natural
and one Palette. There are seven close outcomes if unchanged/refused cases are
included, which would overstate delivered matching.

Collection is closer to the declared edit on nineteen targets versus twelve
before the fixes. Palette's median error falls, but it now refuses seven
requests versus none before. Natural's refusals rise from three to eight, its
changed results fall from fifteen to ten, and its median error worsens slightly.
This is a safety/fidelity tradeoff, not an improvement on every measure.

All 60 native full previews equal export-purpose renders. All 60 delivered PNGs
are verified and equal their saved renders, including preserved originals after
refusal. Source and reference byte hashes remain unchanged, no page errors occur,
and one changed case per workflow passes exact native undo/redo. All eight live
Natural refusals clear the running progress message, preserve the saved recipe
and release the matching controls. These integrity checks pass independently of
the failed appearance requirement. The twelve final contact sheets were also
reviewed; selected sky/floor outputs were inspected at native scale.

### Native-scale visual review

The final collection output for the [desert sky](../../output/aura-main-match-twenty-2026-10-25/run-df0711e1/09.collection.teal-orange.matched.png)
is substantially smoother than the [earlier accepted banded result](../../output/aura-main-match-twenty-2026-10-25/run-988159e9/09.collection.teal-orange.matched.png).
Its known-edit mean error falls from 12.393 to 2.659. The final output is also
much more restrained; improvement must not be interpreted as successful transfer
of every reference colour. Against the unedited original, its mean RGB8 change
is 2.154 and all pixels differ, while its mean tone change is only 0.368 and
mean chromatic change is 1.994 in the report's CIE Lab measurements.

In the same sky region used by the original assessment (rows 5 through 45% of
height, excluding five columns at each side), the 451,012 adjacent-pixel CIE76
steps have p99 6.844 before the fix versus 2.000 after it, and maximum 21.527
versus 5.086. Pairs whose source step is at most 1 but whose output step exceeds
3 fall from 7,149 to zero in this region. These independent local measurements
support the visual improvement without certifying all other regions or photos.

The [pale-floor portrait](../../output/aura-main-match-twenty-2026-10-25/run-df0711e1/20.collection.cool-cyan.matched.png)
still shows pale cyan/green patches at native scale. The known-edit error falls
from 19.863 to 7.843, but the result is not artifact-free and does not reproduce
the declared cool treatment closely. The sampled smoothness guard reduces or
rejects demonstrated failures; this surviving floor defect is evidence that it
does not comprehensively solve them. A lower global distance is insufficient.

## Original-versus-output requirement

The user additionally requested verification that the resulting photograph's
colour/tone/look differs from the original. The live harness now saves an
unedited native render of every imported target before automatic correction or
reference matching. It measures exact pixel equality, fraction of changed pixels,
mean RGB8 change, CIE76 mean/p95, and separate tone/chromatic changes against that
original. It also retains the separate comparison against the reference stage's
baseline. Changes to metadata or a recipe alone do not qualify.

Workflow/file-integrity checks and appearance-delivery requirements are reported
separately. An unchanged or refused result is recorded as a failure to deliver
the requested transformation; it is not omitted or counted as a successful
match. Automatic correction can make a collection output differ from its
original even when the reference stage itself makes no further change. Neither
a pixel difference nor an automatic correction proves a close reference look.

The independent [saved-pixel verification command](../../scripts/report-original-match-verification.py)
reopens the original and matched PNGs, recomputes exact equality, mean RGB8 change
and changed-pixel fraction, and compares them with the desktop harness's recorded
measurements. It writes a JSON qualification and a CSV containing every attempt,
including refusals. `--require-changes` returns exit code 1 when any original is
unchanged, any reference stage is unchanged, or any requested match was refused.
This deliberately distinguishes a completed test run from a passing product
requirement. Numerical agreement allows float32 accumulation precision for mean
RGB; exact pixel equality is checked without tolerance.

The additional "materially changed" diagnostic requires mean RGB8 change of at
least 0.5 and mean CIE76 of at least 0.5 against the original. These are authored
screening thresholds, not proof that a viewer perceives the intended look. Tone
and chromatic changes are recorded separately in the CSV. A monochrome source or
reference can legitimately change tone without introducing new chroma.

## Per-photo original-output result

"Different" means saved output pixels differ from the unedited native original;
it does not mean a correct reference match. "Refused" preserves the original.
"Unchanged" is an accepted studio result with exactly the original pixels.
"Correction only" means collection automatic correction changes the original
but its reference stage adds nothing. The CSV above contains exact change
magnitudes, tone/chroma measurements and full refusal messages for every row.

| Photo | Target subject | Assigned reference look | Collection | Natural | Palette |
|---:|---|---|---|---|---|
| 01 | Woman on beach at night | warm-matte | Different | Different | Different |
| 02 | Illuminated coastal city | cool-cyan | Different | Different | Different |
| 03 | Rustic cafe and red flowers | teal-orange | Different | Refused | Refused |
| 04 | Man on beach at night | pastel | Different | Refused | Refused |
| 05 | Urban beach at dusk | warm-punch | Different | Different | Different |
| 06 | Red flowers in snow | monochrome | Different | Refused | Refused |
| 07 | Mossy forest waterfall | warm-matte | Different | Different | Different |
| 08 | Autumn forest waterfall | cool-cyan | Different | Different | Different |
| 09 | Desert dunes and blue sky | teal-orange | Different | Different | Different |
| 10 | Senior man outdoors | pastel | Different | Refused | Refused |
| 11 | Bearded man in dramatic shadows | warm-punch | Different | Refused | Different, weak |
| 12 | Thoughtful grey-haired senior | monochrome | Different | Different | Different |
| 13 | Monochrome senior portrait | warm-matte | Different | Refused | Different |
| 14 | Bright fruit bowl | cool-cyan | Different | Different | Different |
| 15 | Figs and grapes in dark setting | teal-orange | Different | Refused | Refused |
| 16 | Woman in white corset in studio | pastel | Different | Different | Refused |
| 17 | Dim studio fashion portrait | warm-punch | Correction only | Unchanged | Refused |
| 18 | Dramatic red studio portrait | monochrome | Different | Different | Different |
| 19 | Studio portrait with natural hair | warm-matte | Different | Unchanged | Different |
| 20 | Woman in pastel pink dress | cool-cyan | Different | Refused | Different |

Natural's unchanged originals are photos 03, 04, 06, 10, 11, 13, 15, 17, 19 and
20; photos 17 and 19 are accepted no-ops, the others are refusals. Palette's
unchanged originals are 03, 04, 06, 10, 15, 16 and 17; all seven are refusals.
Collection photo 17 is the correction-only case. These are exact combinations,
not claims that an entire genre always fails. For example, monochrome transfer
works closely on photo 12 while the monochrome request on red flowers (06)
fails the studio guards. Multiple reference subjects per identical grade are
still needed to separate photographic subject from style.

## Reopened Your photograph preview

The [reopen verification](../../scripts/test-match-preview-reopen.py) launches a
fresh application process against this isolated catalogue and checks all twenty
last-saved states, which are Palette outcomes. It reads actual pixels from the
visible Edited image in the "Your photograph" pane through a browser canvas,
then switches to Original and checks that image as well. All twenty Edited
panes exactly equal their recorded saved native output; all twenty Original
panes exactly equal their unedited native render. Every recipe hash remains
unchanged. No matching or recipe writes are performed.

[Reopened pane measurements](../../output/aura-main-match-twenty-2026-10-25/run-df0711e1/preview-reopen.json),
[reopened application screenshot](../../output/aura-main-match-twenty-2026-10-25/run-df0711e1/preview-reopened.png).

This rules out a stale original preview for those twenty final states. Seven
Palette outputs display the original because their requested match was refused;
the GUI is showing the saved outcome correctly. It does not qualify all
intermediate collection/Natural on-screen states after reopening.

## Reproduce the qualification

Run a new full desktop assessment with
`python scripts/test-main-match-twenty.py`; it prints the new isolated run path.
Use that path for the following verification commands. The recorded run can also
be rechecked directly:

```powershell
python scripts/report-original-match-verification.py output/aura-main-match-twenty-2026-10-25/run-df0711e1 --require-changes
python scripts/test-match-preview-reopen.py output/aura-main-match-twenty-2026-10-25/run-df0711e1
```

The first command returns **1**, as expected for the demonstrated product
requirement failure, after successfully checking the saved pixel evidence and
writing its JSON/CSV. The second returns **0** after verifying the twenty
reopened Edited/Original pane pairs. Desktop integrity and native workflow
integration checks pass; the unchanged Natural batch-fidelity unit failure and
the appearance-delivery failures remain open. This is not an all-green matching
qualification.

## Remaining implementation work

The tighter guard can reject an unsafe transfer without supplying an acceptable
replacement. The next fitting change should constrain the transfer's local
slope/Jacobian and regularize quantile/LUT transitions during fitting, then try
bounded reference-driven candidates against the actual requested-strength
render. Merely relaxing the new guard would restore demonstrated artifacts.
This is proposed work, not an implemented fallback in this build.

The two single-photo engines still differ from the collection selector. They
need a shared native candidate review and explicit outcomes for applied,
already-close, unsupported and refused results. A no-op should explain its
measured reason and should not imply that a visible reference transformation was
delivered. An arbitrary colour offset would satisfy pixel inequality while
failing the user's actual goal, so it is not used to manufacture success.

Unrelated-scene matching also needs subject/region correspondence and separate
intent controls for preserving object colours versus transferring a reference
palette. A forest's green coverage cannot identify the grading operation to use
on a studio floor. Qualification should use several unrelated reference subjects
per identical authored grade, held-out targets, naturalness review and spatial
artifact checks. The original report details this larger architecture and the
required evaluation changes.

## Scope

These changes address the demonstrated bright-gradient protection, stale progress and native-reference measurement gaps. They do not implement semantic scene matching, a trained reference-conditioned model, full-precision analysis of high-bit-depth references, or exact recovery of arbitrary photographic styles. A guarded or attenuated result is not proof of a close style match. The existing unrelated-scene Natural fidelity failures remain separate qualification limits.
