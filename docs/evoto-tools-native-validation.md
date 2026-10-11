# Explicit color, proportions and provider links: native validation

Validated on Windows on 2026-10-11 in `C:\Users\amrit\aura-eye-protection`,
based on `55305c0` plus the changes delivered with this document. The initial
`D:\aura photos editor` checkout was not used as the implementation source.

## Actual build and photo path

The desktop build used Rust 1.97.1, `x86_64-pc-windows-gnu`, `--locked`,
`--features custom-protocol`, and one Cargo worker. Its executable SHA-256 was
`39102af303caa6844e652a2c26f6df75af4f5df43f52a5b045619271090c575a`.
The GNU linker reported an existing manifest-merge warning; launch and the native
workflow below succeeded. The old working app was left open during launch and
native testing; no close was sent to that process. It later disappeared from the
process list during verification. A read-only check confirmed all six previously
saved recipe hashes remained in the working catalogue. The old unsaved UI draft
cannot be verified or reported as preserved. The new app used its own catalogue,
WebView profile and debugging port 9362.

A real 1333 x 2000 portrait was imported through the interface. Source SHA-256:
`577448e4d45a39cfc6ae254f30a23f70eeca9165a8d5091250282d1ab8810663`.
Normal automatic preparation completed before the new manual-tool checks.

## Saved tools, history and full-resolution output

- Hair color: chosen sRGB `#6d3b9e`, strength 65%, neutral brightness, ellipse
  `[0.23, 0.48, 0.035, 0.19]`; 51,880 changed pixels.
- Solid background color: `#d7e7ec`, strength 65%, ellipse
  `[0.08, 0.5, 0.065, 0.35]`; 187,218 changed pixels.
- Local proportions: width -0.35, height zero, strength 65%, ellipse
  `[0.65, 0.75, 0.15, 0.15]`; 106,221 changed pixels.

Every tool had zero changed pixels outside its selection. Actual native undo and
redo restored exact saved recipe hashes and full-resolution RGB pixels. The saved
final render matched the captured final native PNG exactly. Face rectangle
`x=370..879, y=320..949`, including eyes, nose and lips, matched the prepared
baseline exactly after all three new edits. This is protection evidence for these
manual edits, not proof that automatic cleanup preserved every possible feature.

The first export produced a verified delivery manifest. The original harness
then failed because it searched the destination root rather than the `gallery`
subfolder. The harness now follows the manifest's checked relative file path.
A separate actual export/readback check then passed: one 1333 x 2000 JPEG,
539,454 bytes, verified manifest, original and imported-copy hashes unchanged,
saved recipe unchanged by export, and no captured interface exceptions.
Final recipe hash:
`2ee63a9d32c9279934e57f13c60f9431c4d484042c26620aa72adc6a5c533a01`.

The full image and face-detail crop were inspected. The selected hair strand has
the chosen tint with visible texture. The elliptical background test demonstrates
local compositing, not a finished whole-background replacement. The local torso
change rendered without an obvious duplicated edge in this example. Some facial
marks and pores remain after normal preparation. Professional or Evoto-equivalent
quality and complete blemish removal are **not established**.

## Provider browser checks

The actual native command opened OpenAI's API-key dashboard and Claude's key
dashboard. Unknown provider identities were refused. The cloud state before and
after opening links matched exactly. No credentials were entered and no paid
provider request was made. Saving/checking an API key remains necessary; dashboard
visits do not implement ChatGPT subscription OAuth or authenticate AURA.

## Automated checks and limits

All 742 interface tests in 86 files, frontend production build/typecheck,
formatting, banned-pattern checks and the 315-command IPC surface check passed.
The 83 frozen contracts passed. The final GNU serial library run passed 446 tests
(app 178, recipe 70, renderer 198; two app tests ignored). All 21 native-retouch
integration tests and strict affected-package/all-target Clippy passed. The final
run used temporary files on D: and one test thread after E: filesystem/history
tests became very slow; the interrupted earlier run is not counted as a pass.

Earlier MSVC attempts encountered missing/corrupt dependency metadata and compiler
cache crashes. A fresh GNU build and then the working GNU cache completed the
native application. These failures are retained as environment evidence and are
not represented as successful MSVC validation.

Local photos, exports, profiles, catalogues, raw logs and executables are excluded
from Git. Evidence is under `E:\aura-native-evoto-20261011\native-validation`.

After validation, a backup copy of the previous local executable on D: did not
match its source hash, so `app/aura-desktop.exe` was left untouched. The validated
executable remains in the working C: build cache. A separate SQLite snapshot of
the working catalogue passed integrity checks and matched all six saved recipes.
The updated native app was then launched with that working catalogue/profile on
port 9359; its native recipe reads matched all six saved hashes. The local
`output/evoto-feature-work/Open updated AURA working catalog.ps1` launcher checks
the validated executable hash before opening this catalogue. This recovery does
not establish preservation of the old unsaved UI draft.

See [the workflow and gaps](evoto-style-workflows.md) and
[the published route inventory](evoto-feature-inventory.json). All commercial
features, varied-photo quality, provider responses and cross-platform CI remain
outside the claims established by this run.
