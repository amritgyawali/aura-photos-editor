# Adaptive portrait retouch

The portrait pass now tries upright detection first, then quarter-turn views if
no face was found. Sideways and upside-down detections are mapped back to the
original photograph before planning edits. The source image is never rotated by
this analysis. Small, cropped, occluded or mixed-orientation groups remain limited;
the fallback stops at the first orientation with confident faces.

For each suitable face, AURA compares cheek/forehead patches and chooses a
representative low-variation color sample. Color outliers, clipped samples and
very dark/noisy patches are rejected. Texture, tone and local-light strengths
are bounded and depend on measured variation within that face. Low signal gets
gentler correction. Eye/mouth exclusions and fine-detail preservation remain.
This is geometric and sampled-color targeting, not learned semantic segmentation.

In Develop or Retouch, expand **Automatic decisions by face** to see the reason,
detection confidence and three applied strengths for each face. Detection
confidence does not measure beauty or edit quality. Every operation remains
editable, and manual retouch stacks remain protected from automatic replacement.

**Reset photo** now detects removed optional recipe fields, including native
retouch. Reset and restoration of a snapshot without retouch can clear that stack;
Undo restores the prior version. Manual merges and snapshot restoration carry
user provenance rather than inheriting the preceding automatic pass's label.

The pinned YuNet weights are unchanged. Detection policy is
`yunet-2023mar-aura320-rotation-v2`; planner policy is `sample-consensus-v3`.
No cloud service, account or model download is needed. No-face images can take
longer because of bounded orientation fallbacks. `AURA_DISABLE_AUTO_PORTRAIT=1`
still disables automatic portrait analysis.

## Validation on 2026-09-30

- UI production build passed; 556 tests across 58 files passed.
- 64 recipe tests and 12 focused portrait/detector/exposure tests passed. These
  used direct-source Rust test harnesses against the desktop dependencies because
  the machine's existing debug archives caused native linker failures.
- The desktop build passed after rebuilding affected internal packages with
  debug information disabled. This is a local build workaround, not a claim that
  the toolchain/archive issue is resolved.
- A native detector probe found all five public portrait fixtures, both faces in
  a composed two-face fixture, and the 90/180/270-degree variants. Landscape and
  sneaker negatives produced no detections. Debug detection took about 2 seconds
  upright and up to 9 seconds for fallback/negative cases on this busy PC; these
  are observations, not release-performance benchmarks.

- Rebuilt-desktop checks passed for one real portrait and its three quarter-turn
  variants: three effective retouch operations per face, repeat-pass idempotence,
  exact pixel restoration on Reset/Undo/Redo, pre-retouch snapshot restoration,
  manual provenance and protection. The explanation disclosure, Reset and Undo
  were also exercised through the actual WebView interface.

- All five real portraits passed the preview/full-render regression, repeat-pass
  idempotence, Undo/Redo and manual protection. Five PNG exports verified and
  matched full-render pixels exactly. Actual Retouch controls, selection-mask
  preview, keyboard strength adjustment and Undo passed. Original-file hashes
  remained unchanged. This PNG check does not establish sidecar support.

The compact results are committed in
[`audits/2026-09-30-adaptive-portrait-results.json`](audits/2026-09-30-adaptive-portrait-results.json).
These fixtures verify the tested behavior, not professional retouch quality on
every image or parity with another editor.
The audit in `software-capability-audit.md` describes the earlier `3a546d1` build;
its other format, export and missing-feature findings are not fixed by this work.

To repeat desktop checks with WebView2 debugging enabled on port 9223:

```powershell
python scripts/test-portrait-v2.py
python scripts/test-auto-portrait.py --photos .work-checks/portrait-review/originals --output .work-checks/portrait-v2-five-real
```

Both commands create isolated test collections and require the five public
portrait fixtures, Pillow and Playwright. They do not modify the source files.
Local evidence includes `.work-checks/portrait-v2/results.json`,
`.work-checks/portrait-v2/face-decisions.png` and the five-portrait output folder.
