# Reference matching: first quality fixes after the deep assessment

Date: 2026-10-25. This follows the [360-case assessment](AURA-reference-look-colour-deep-assessment-2026-10-25.md). Its original images, measurements and failure report remain the before-change evidence.

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

The desktop has rebuilt successfully. The live 20-photo before/after retest and
final artifact details will be recorded below after they finish.

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

## Scope

These changes address the demonstrated bright-gradient protection, stale progress and native-reference measurement gaps. They do not implement semantic scene matching, a trained reference-conditioned model, full-precision analysis of high-bit-depth references, or exact recovery of arbitrary photographic styles. A guarded or attenuated result is not proof of a close style match. The existing unrelated-scene Natural fidelity failures remain separate qualification limits.
