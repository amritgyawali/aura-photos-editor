# Automatic portrait editing validation — 2026-09-30

Implementation: ADR-0074. This is an offline face-guided retouch workflow, not a
commercial-retoucher parity claim or a semantic skin segmentation benchmark.

## Automated checks

- UI: 553 tests passed across 57 files; production TypeScript/Vite build passed.
- Native: 21 focused checks passed: Resize (1), automatic recipe integration (3),
  portrait/planner/enhancement checks (7), preview-service tests (10).
- Clippy completed for app, inference, vision and preview packages; style warnings
  remain. This does not claim a warning-free workspace-wide lint run.
- The native YuNet interpreter matched OpenCV DNN on identical input, with maximum
  absolute error below 0.000007 across all twelve output tensors. Independent
  detector probes found each of five real portrait faces and both faces in a
  composed two-person fixture. Black, grey and white frames produced no faces.

## Real photographs and editing

The desktop check uses the public Pexels photographs
[1239291](https://www.pexels.com/photo/1239291/),
[220453](https://www.pexels.com/photo/220453/),
[2379004](https://www.pexels.com/photo/2379004/),
[774909](https://www.pexels.com/photo/774909/) and
[8386841](https://www.pexels.com/photo/8386841/).
Source images are kept outside Git; their original SHA256 hashes are recorded by
the test runner.

`scripts/test-auto-portrait.py` exercises both Auto enhance and Auto portrait.
It checks face detection, visible pixel changes, editable operations, repeat runs
without duplicate history, exact undo/redo, manual protection, and portrait-only
changes confined to face regions. Five full-resolution PNG exports must match the
full renderer byte for byte. The `--resume` path also checks recipe hashes and
preview pixels after restarting the desktop, then exercises manual refinement
and undo through the actual UI.

The tests exposed and fixed two issues: raw inference floats caused duplicate
history despite identical canonical recipes, and fresh preview pixels differed
from their persisted JPEG representation. The latter also affected automatic
analysis after restart. Regression coverage now checks both preview tiers.

An early detached desktop test process ended without a diagnostic during export.
The subsequent monitored desktop run completed all five portraits and exports.
After a deliberate, clean restart, all five saved recipe hashes and preview pixel
hashes matched exactly. A second export matched the full renderer, and changing
an automatic step to 9% through the UI followed by Undo restored its saved stack.
The local evidence lives in `.work-checks/auto-portrait-review-final/results.json`
with before/after pictures and UI captures.

## Windows build

This machine uses Rust 1.97.1/MSVC and has limited free memory while other work is
running. MSVC rejected one generated archive with LNK1106; reducing local debug
information resolved the build. The verified desktop command is:

```powershell
$env:CARGO_INCREMENTAL='0'
cargo rustc --manifest-path ui/src-tauri/Cargo.toml --target-dir target/desktop-launch --features custom-protocol --bin aura-desktop --config profile.dev.package.aura-vision.debug=0 --config profile.dev.package.aura-brain-photo.debug=0 --config profile.dev.package.aura-people.debug=0 --config profile.dev.package.aura-app.debug=0 -j 1 -- -C debuginfo=0 -C link-arg=/DEBUG:NONE
```

Run `npm run build` in `ui` first. These are local build options, not a change to
release processing or an acceleration claim. Portrait detection remains limited
by face size, pose, occlusion and the suitability of the available skin sample.
