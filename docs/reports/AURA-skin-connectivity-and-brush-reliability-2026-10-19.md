# AURA skin connectivity and brush reliability — 2026-10-19

This continuation improves the sampled-skin tools and manual brush. It does not claim complete Evoto parity or perfect retouching on every photograph. The previous feature inventory and reference-colour implementation remain documented in [the Evoto feature report](AURA-Evoto-8-features-and-colour-matching-2026-10-18.md).

## Reproduced selection failure

Sampled skin selection used a coarse grid of weighted cell means, with a flood fill constrained by colour and relative brightness differences. A narrow dark line could occupy only a small fraction of one cell, so its average still resembled skin. A one-pixel protected strip could disappear from the connectivity calculation because excluded pixels contributed no weight. In both cases the coarse traversal could enter a separate surface with similar skin colour.

Two new regression tests were run before the fix. Both failed: a one-pixel dark contour was selected, and the surface across an authored protected strip remained selected. The synthetic scene is 1440 × 64 pixels, giving a six-pixel coarse cell. Its narrow barrier lies inside a cell rather than on the grid boundary. These tests reproduce the computational failure; they do not establish that this was the only cause of every earlier nose/chin artifact.

## Implemented correction

`crates/aura-render/src/retouch_skin/connectivity.rs` performs an additional four-neighbour traversal on the original working pixels inside the authored selection and the coarse pass's accepted domain. Candidates must have finite nonnegative channels, positive coverage and positive affinity to the sampled skin. Traversal retains the existing relative-luminance and chromaticity edge thresholds. It never crosses a zero-coverage pixel, a rejected colour or a sufficiently strong local edge.

The seed is the nearest actual eligible pixel inside the coarse pass's selected seed cell. A mixed cell no longer acts as one connected surface. The original coarse selection remains an upper bound; the new pass can only remove eligibility. Existing inward feathering and selection restrictions still apply. Both the selection overlay and the renderer use the same connectivity calculation, so excluded surfaces are also excluded from the smoothing and tone estimates.

The extra state uses one byte per pixel in the bounded coverage rectangle, plus a breadth-first frontier. Work is linear in candidate pixels and their neighbour edges; large selections have additional processing and allocation costs. This is not a benchmark of production full-resolution batch throughput. The existing coarse seed search and thresholds remain, so the change is conservative and cannot recover skin rejected by that first pass.

The persistent renderer identity now includes `robust-patch-v3-pixel-connectivity`. Cached renders produced under the previous skin processor cannot qualify as current renders. No recipe schema migration is required. Existing parameter values, source coordinates and authored masks remain unchanged; connected sampled-skin operations can render more conservatively under the new processor.

This applies when **Only skin connected to the sample** is enabled for sampled smoothing, colour evening or portrait dodge and burn. It does not replace semantic face/body parsing, change every automatic portrait mask, or activate the setting on old operations. Identical touching surfaces without a measurable edge still need a tighter ellipse, brush mask or semantic selection. The UI now describes eligible pixels and removes the previous absolute claim that a same-coloured background can never be selected.

## Brush improvements

`ui/src/components/develop/RetouchCanvas.tsx` and `brushPath.ts` now:

- Cancel an active gesture when the app loses focus, relevant interaction settings change, or a move arrives after the pointer buttons were released outside the WebView. An abandoned gesture is not committed to the draft.
- Consume available browser-coalesced pointer samples while retaining pen pressure and committing one stroke on release, rather than saving edits on every movement.
- Replace only geometrically redundant straight samples with matching interpolated pressure. Corners, reversals and pressure changes remain. A test covering 5,000 straight samples retains the correct endpoints without exhausting the 1,024-point bound. This is not arbitrary smoothing or lossy curve simplification.
- Report the point-limit notice once per gesture. Complex paths still retain the bounded point limit; users can release and start another stroke. Existing overall stroke/mask limits remain.

These changes prevent further extension of a stale gesture. They do not silently apply, discard or reconstruct an already-open draft in a different running process.

## Verification

| Check | Result |
| --- | --- |
| Pre-fix repro | Both new narrow-barrier tests failed against the prior renderer. |
| Sampled-skin integration suite | **14 passed**. Includes horizontal and vertical narrow barriers, exposure scales, an actual one-pixel open route, a render where the sampled side changes while the disconnected surface remains byte-for-byte unchanged, authored holes, colour/brightness boundaries, cache distinctions and existing tool behaviour. |
| Existing under-eye unit suite | **12 passed**. Covers preserved socket gradients, pore contrast, lashes, highlight protection, bounded lightening, contaminated references, different exposures and strength scaling. The under-eye algorithm itself was not changed in this continuation. |
| Focused frontend suites | **51 passed across 3 files**: native retouch workspace (38), canvas (10) and brush path (3). New interaction tests cover focus loss, missed release, coalesced samples and retained pressure. |
| Frontend build | TypeScript and Vite passed. Existing large-bundle warning remains. |

Evidence is in `.work-checks/skin-connectivity-2026-10-19/`, including `sampled-skin.log`, `under-eye-regression.log`, `brush-ui.log` and `ui-build.log`. Desktop build and runtime evidence are recorded below.

## Session preservation and limits

The existing running AURA session reported an **unsaved brush draft** and a stroke-limit notice. This task did not terminate that process or apply or discard its draft. Verification must not apply or discard the photographer's draft merely to replace the executable. A separately named desktop candidate is built under `target/aura-skin-connectivity-2026-10-19/` for verification and subsequent use after that draft is saved or discarded by the photographer.

No existing final JPEG delivery or original RAW was rewritten for this change. New tests validate specific invariants and failure cases, not universal skin-classification accuracy, every anatomical edge, artistic perfection or identical Evoto outputs. Wider automatic portrait-quality improvements still require representative reviewed images, dense anatomical masks and independently validated defect detection.

<!-- DESKTOP-VERIFICATION -->

## Desktop candidate verification

The debug desktop build with the packaged frontend succeeded. `cargo rustc` used a separate output path so it could not replace the executable serving the open draft. The compiler emitted the expected custom-output filename warnings and the existing Windows resource multiple-manifest linker warning. An initial launch revealed that the separate directory needed `WebView2Loader.dll`; the existing loader was copied alongside the executable, and the subsequent launch passed.

Ready-to-open build: `target/aura-skin-connectivity-2026-10-19/aura-desktop.exe`.

The final candidate was launched against a SQLite backup in a separate WebView profile. The live page loaded the expected `index-Bw500qHk.js` bundle, exposed the real Tauri bridge and the colour-reference command, reported zero busy image workers and no error alerts. The candidate verification process was then closed. The original process was not terminated by this task. At the final process check it was no longer running; its exit cause and draft recovery were not established. The normal catalogue and existing exports were not replaced by the isolated verification catalogue. The new executable is ready to open for normal editing.

**27 focused native checks and 51 frontend tests passed.** The renderer cache-version test passed in addition to the skin and under-eye suites. This was not the full workspace test suite. The runtime smoke check establishes packaging and bridge startup; it is not a new final-photo review or proof of perfect retouching.

Executable SHA-256: `0e1ec26e4a4bdee2b6261441fdb7bcb8ac880d17177b7a3530c14c2f96fd8242`.

`verification.json`, `desktop-build.log`, `desktop-launch.json`, `desktop-smoke.json` and `engine-cache-version.log` under the evidence directory record the final artifact and checks.
