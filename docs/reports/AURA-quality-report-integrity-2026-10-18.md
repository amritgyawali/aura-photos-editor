# AURA: quality reports tied to the edit they measured

This implementation addresses a concrete reliability problem found after the regional retouch work. It does not claim that AURA is perfect, that every skin pixel is detected, or that output matches Evoto.

## Findings

The advanced retouch report was saved in the recipe but did not identify the parameter state it measured. Later changes could leave a historical pass visible beside a different edit. The component also preferred its in-memory run result over a newer recipe and could accept a delayed result after switching photos. Finally, summary text could claim that checks passed despite a nested regional warning, and a saved `passed` boolean was not independently checked against the numeric limits.

## Changes

The native report version is now `auto-advanced-v4-revision-bound-qc`. Every new report stores the checked photo ID and a JSON-string snapshot of the recipe parameters. The snapshot includes image identity, global corrections, geometry, retouch operations, masks and extension data. It excludes the report itself and edit provenance. Excluding itself prevents recursive growth; excluding provenance avoids invalidating a check solely because history metadata changed.

The first real-RAW binding test caught a flaw in storing the snapshot as a nested object: the recipe merge flattens parameter paths, which removed empty containers inside that object. The snapshot is now an opaque string so the merge preserves it exactly. An additional unit regression covers the actual merge, not only serialization, and the native integration checks the final saved recipe after both runs. The initial failed evidence is retained in `revision-qc-native.log` and `revision-qc-native-diagnostic.log`.

Snapshot numbers use the existing recipe canonical precision of six decimal places. The interface compares values structurally: object-key ordering is irrelevant, while array order is preserved, including the retouch stack. The comparison does not change the recipe. A manual exposure, rotation, mask or retouch change invalidates the historical result; returning to exactly the checked parameters can make it applicable again.

New UI status distinguishes `current`, `changed` and `unbound`. Older reports remain readable but cannot verify current settings without a saved comparison. Changed reports show an explicit instruction to run the checks again. Historical measurements remain available for inspection rather than being silently rewritten or deleted.

The component now reads completed results through the same validation path as saved recipes. A new recipe body clears the cached result. Run tokens and current-photo checks prevent an earlier asynchronous result or progress event from appearing on another photo, including navigation away and back while an older run is pending. These UI changes do not cancel a native job that is already running.

A saved pass must also satisfy the existing native limits: texture retention at least 0.6, nonnegative skin drift no greater than 0.012, valid clipping fractions and clipping increase no greater than 0.005. All required regional measurements must be present and free of review warnings. A saved boolean cannot override contradictory measurements. Summary text claiming a pass is replaced when the actual validated status does not pass.

## Files

- `crates/aura-app/src/advanced_retouch.rs`: snapshot creation, report fields/version, save/load regression coverage.
- `ui/src/ipc/advancedRetouch.ts`: structural comparison, status, numeric checks and consistent summary.
- `ui/src/components/develop/AdvancedRetouch.tsx`: current-recipe display and asynchronous result isolation.
- `ui/src/components/develop/AdvancedRetouch.test.tsx`: changed settings, older reports, photo identity, navigation races, measurement contradictions and regional warnings.
- `crates/aura-app/tests/advanced_retouch_photos.rs`: actual saved-recipe binding assertions after first and repeated native advanced-retouch runs.

## Practical limits

The snapshot adds storage approximately proportional to the recipe parameters. It is deliberate evidence for an exact structural comparison, not a new image or model. It does not expand recursively on repeated runs. Changes to extension metadata can conservatively invalidate a result even when they do not alter pixels.

This is a recipe/photo-identity consistency check, not a fresh source-file hash or a renderer/model-build check. It is not a cryptographic attestation and does not detect a user manually forging both a report and its snapshot. The existing one-click export hash check remains a separate mechanism. Manual export behaviour is unchanged.

This continuation does not change retouch pixels, skin segmentation, white balance or colour correction. It fixes whether a quality result can truthfully be shown as applying to the current edit. Dense feature parsing, representative visual calibration and broader native-resolution inspection remain outstanding requirements.

## Validation

Tests use an isolated catalog; the production catalog and wedding deliveries are not edited by these checks.

- Application library: 253 passed, 2 intentionally ignored. Includes canonical save/load, metadata exclusion, visual-change invalidation, non-recursive storage and preservation through the actual recipe merge.
- One-click delivery integration: 1 passed. Existing failure withholding and cancellation remain tested.
- UI: 43 passed across `AdvancedRetouch.test.tsx` and `NativeRetouchWorkspace.test.tsx`. Includes cross-photo late completion, immediate stale display after edits, contradictory pass summaries, numeric limits and older unbound reports.
- Frontend production build: passed TypeScript and Vite; the existing large-bundle warning remains.
- Focused Clippy: `cargo clippy --locked --no-deps -p aura-app --lib -j 2` exited 0. Existing warnings remain; this is not a dependency-wide clean-lint claim.
- `git diff --check`: passed, with existing CRLF conversion notices on unrelated UI files.

The initial new unit fixture used a sampled smoothing operation without its required source. The fixture was corrected to a valid manual dodge operation. The initial native failure exposed the nested-object merge issue described above; these failures were corrected rather than treating their results as success.

Final unit/integration logs: `.work-checks/revision-qc-final-tests.log` and `revision-qc-final-clippy.log`. UI test/build output was recorded in the execution session.

The ignored native integration test was explicitly run on `H:\editing to aura\989A2699.CR3` with an isolated catalog. It passed both complete advanced-retouch runs, including saved recipe/photo binding assertions after each run. Test duration was 54.10 seconds while other build work was running. The first run's QC measured 1066 x 1600 pixels: texture retention 0.9688272, skin drift 0.0005463353, no clipping and no regional review warnings. These are screen-resolution heuristic measurements, not native-size visual certification. Full-resolution export was not requested by this test invocation, and no wedding delivery was replaced.

The final runs total 298 passing tests: 253 application tests, one one-click test, 43 UI tests and one native integration test. Final native evidence is `.work-checks/revision-qc-native-final.log` and `revision-qc/binding-comparison-final.json`. The latter contains the final saved parameter comparison from the repeat run.

The updated desktop build completed successfully in 2m 35s. The existing MinGW multiple-manifest resource warning remains. Build log: `.work-checks/revision-qc-desktop-final.log`. The application was relaunched from `target/desktop-launch/debug/aura-desktop.exe`; its read-only IPC smoke result is recorded in `.work-checks/revision-qc/desktop-smoke.json`.
