# Real photograph workflow verification

Updated 2026-09-22. Complements the limitations recorded in PHASE-01-30-REVIEW.md.

The desktop workflow now connects file selection to background import, pixel
analysis, reversible automatic adjustments and verified JPEG delivery. The native
shell exposes both `automatic_start` and `photo_analysis`; a frontend integration
check covers the complete literal IPC command surface.

Automatic processing respects saved cloud policy and project consent. Without an
available vision provider, it applies measured local adjustments and labels them
as local. Unavailable learned analysis preserves framing and retains readable
photographs. Cloud/cache counts, failed edits and local adjustments are reported
separately. Cancellation remains pending until active work finishes, and terminal
status is published after the run report has been written.

## Automated verification

- 548 UI tests passed together across 57 test files after merge recovery.
- 21 decoder unit tests passed, including transparent PNG decoding at thumbnail,
  proxy and full-resolution tiers. The proxy check explicitly starts without a
  cached thumbnail and checks alpha compositing and the JPEG cache output.
- 23 cloud-library unit tests passed in the earlier verification, including
  bounded automatic adjustment validation.
- 21 import integration tests passed, including the regression for PNG folder
  scanning and individual file selection. PNG was previously classified and
  decodable but omitted from the default scanner allowlist. New regressions cover
  importing the same folder and camera into separate projects and preserving
  existing source-root IDs on reimport.
- Two app integration tests passed: actual JPEG import/render/edit/export with
  unchanged originals and protected manual exposure; complete offline delivery,
  report persistence and cancellation of a waiting import.
- TypeScript/Vite production build, Rust formatting, frozen contract hashes and
  whitespace checks passed.

The existing build caches contained corrupt artifacts. Locked JavaScript
dependencies were restored, Rust incremental compilation was disabled, and
damaged native dependency archives were rebuilt. The Windows launcher builds with
one worker to fit this machine's memory.

## Merge recovery and repeated imports

All 17 unmerged paths from applying the feature-branch stash onto the older main
checkout were resolved. The feature's committed dependencies were restored along
with its stashed changes, including provider setup, TLS, real image decoding and
rendering. The stash remains available, and a local recovery archive preserves
the pre-resolution files and diffs under `.work-checks/`.

Source-root and camera primary keys now include the project ID. Previously a
second project using the same folder failed with `AURA-DB-3006`; a camera serial
could cause the same collision. Existing scoped upserts preserve legacy row IDs.
Automatic-workflow errors now expose the specific actionable message instead of
replacing it with an unrelated generic rendering error.

The desktop test also exposed a separate PNG proxy failure: tier 2 unconditionally
used the JPEG decoder, despite thumbnails and full-resolution PNG exports working.
The proxy now chooses the PNG decoder for PNG sources before the existing colour
conversion. The regression that claimed all preview tiers now actually exercises
tier 2 as well as thumbnails and full resolution.

## Desktop verification on 2026-09-22

The Windows executable rebuilt successfully and was launched through
`scripts/start-aura.ps1`. The interface imported the same sample folder used by
earlier projects, containing one JPEG and one PNG. The run completed with two
analyzed photographs, two local edits, zero failed edits and two verified exports.
Both exported JPEGs decoded at 640 x 480 with non-flat pixels. Original SHA-256
hashes were unchanged, and the run, analysis and edit reports were present.

Both library thumbnails loaded. Develop displayed the PNG at 640 x 480, and
switching between original and edited previews worked with different image data
and no interface alerts. Native full-resolution rendering also succeeded for
both formats. The application was left open and responding in Develop.

Evidence is retained locally in `.work-checks/runtime-smoke-result.json`,
`.work-checks/aura-verified-2026-09-22.png`, and the build/test logs in that folder.
The verified run used local enhancement, not a paid cloud request. Optional
learned analysis remained unavailable; the run explicitly retained both images
and preserved their original framing.

## Usage and limits

See [the quickstart](../photo-editing-quickstart.md). `Start AURA.cmd` launches the
bundled desktop interface; no Vite server is needed. Select photos or a folder to
start automatically, then use Develop to compare and refine the results.

JPEG and PNG are the verified formats. Camera RAW support depends on the encoding.
Advanced local face, subject and retouch models retain the phase review's limits.
The current checks do not certify unattended professional delivery or a live paid
provider round trip; cloud edits require a configured vision provider and consent.
