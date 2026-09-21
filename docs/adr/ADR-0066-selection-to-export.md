# ADR-0066: Selection-to-export workflow

Accepted 2026-09-19. Extends ADR-0065 for the current desktop workflow.

Selecting photographs can create a project, import the files, measure the actual
pixels, apply reversible per-photo adjustments, and export verified JPEGs to a
unique folder under Pictures/AURA Exports. The output path and progress remain
visible in the application. The existing Finish everything control can choose a
different destination.

Provider use respects the saved cloud switch, offline mode, face-blur requirement,
project consent and spending governor. Selection does not grant new cloud consent.
Local enhancement is labeled as such. Advanced learned analysis may be unavailable;
in that case the application keeps every readable photograph and preserves framing.
The 600-call ceiling limits external AI use, not the number of photographs edited
or exported. Remaining images receive local adjustments.

The additive IPC types AutomaticStartInput, AutomaticStartDto and PhotoAnalysisDto
connect this workflow to the UI. OneClickStatusDto reports measured photographs,
failed edits and actual cloud/local counts. Its status can also be cancelling or
completed_with_issues. Cancellation remains pending while an active operation
finishes; another automatic run cannot start until the worker has exited. The
export service currently finishes an active export before acknowledging Stop.

Reports are saved beside exports. In-process jobs survive UI navigation/reload,
but do not resume automatically after the native process exits. Originals remain
read-only and human-edited recipe fields stay protected.

The affected surfaces are `crates/aura-app/src/contract/ipc.rs`, the app command
exports, `ui/src/ipc/types.ts` and `client.ts`, the Tauri command registry, and the
welcome/import/automatic-progress panels. `ui/src/nativeBindings.test.ts` checks
all literal application IPC calls against that registry. The app integration
tests exercise real pixels, protected manual edits, delivery and cancellation.
