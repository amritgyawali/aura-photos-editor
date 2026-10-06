# Professional retouch validation - 2026-10-06

What was built, what was run, and what the result looked like. The decisions are in
[ADR-0090](adr/ADR-0090-frequency-healing-and-the-texture-graft.md).

## What was run

One photograph: the 1024 x 1536 **synthetic** acne portrait earlier retouch work used
(a generated image of an adult, not a photograph of a person; SHA-256
`c11925539965ac39f67a01b46f5275f6cac1f25e7a2f079767ea4f97a34ddd9a`). It has dense
inflamed and dark marks on the forehead, both cheeks and the chin, an oily highlight on
the forehead and beside the nose, and one side of the face in shadow.

### In the desktop application

A debug build of the native Windows shell with the production UI bundled
(`--features custom-protocol`), started with an isolated catalog and an isolated WebView
profile. `scripts/verify-professional-retouch.py` pressed the application's own controls
and then read back what the application saved; it edits no pixels.

| Run | What the controls did | Operations saved | Retouch | Export |
|---|---|---|---|---|
| 1 | New collection, import, the import's automatic edit, **Retouch**, **Professional retouch**, **Auto retouch: Face**, export with verification | 237 | 18.2 s | 11.8 s |
| 2 | **Reset photo**, then the same preset and export: the retouch on the photograph as taken | 237 | 18.1 s | 11.3 s |
| 3 | Run 2 again | 237 | 17.9 s | 11.3 s |

Times are single runs on this 8 GB laptop with other programs open, not a benchmark.

The saved stack: 1 frequency heal, 1 skin smoothing, 1 tone evening, 1 skin dodge and
burn, 3 micro dodge and burn, 220 texture-matched spot repairs, the frequency-separation
surface finish, hair detail, 1 texture restore, and the line, eye and under-eye steps the
face measured for. The script asserts that frequency healing is the face's first automatic
operation and the texture restore comes after the finish; that healing and the finish
share the feature-protected surface selection; and that the restore runs over the
segmented face skin.

- Every export was read back and verified by the application (`verified: true`, one file).
- The original file's hash was the same after every run.
- Runs 2 and 3 saved the same recipe (hash `861e0664...3de1`), and their exported JPEGs
  are **byte-identical** (429,648 bytes, BLAKE3 `c649bc38...3bb3`).
- Run 1's export (491,063 bytes) includes the import's automatic edit, which on this
  portrait raised exposure by 0.56 EV and so lightens the whole photograph, skin included.
  That edit is earlier behaviour and not part of this change; run 2 exists so the retouch
  can be judged without it.

Evidence is under `output/professional-retouch/` (ignored by git): the original, both
exports, a side-by-side, the face at 100 % and the nose, both cheeks and the forehead at
200 %, the saved recipe, both `verification.json` files and two screenshots of the
application.

## The first version, and what was wrong with it

An earlier build of this branch was run the same way and shown to the person who asked
for the work. They said it looked like a cartoon: no detail on the nose and the face
blurred all over. At 100 % it did: smooth skin with a borrowed oily-ridge pattern laid
over it, a brown blotch beside the inner corner of one eye, and a nose with its pores
gone. ADR-0090 records the four causes and the fixes; in short, the texture step now puts
back each pixel's own photographed detail instead of borrowed tiles, a shadow is no longer
taken for a mark, and the preset smooths far less.

## What it looks like now

Looked at by the author of the change at 100 % and 200 %, against the original. One
person's inspection of one image; nobody else has judged this version.

- The inflamed and dark marks on the forehead, both cheeks and the chin are gone at 100 %.
- The skin has its own pores everywhere, at the place they were photographed: on the
  cheeks, the forehead, the nose bridge and the sides of the nose. It no longer reads as
  smoothed.
- The nose keeps its shape, its shading and its nostril edges. The highlight on its tip is
  softer than in the original, because glints are limited.
- The shadow beside the inner corner of each eye is untouched.

**Still visible:**

- One small white spot on the forehead. It is not red, so it is kept by rule: a compact
  bright spot may be a piercing.
- At 200 %, faint traces of a few marks on the shadowed cheek, and the original pitting of
  the skin beside the nose. The restore puts back the photograph's own detail, so texture a
  person would rather lose is limited to the skin's ordinary range but not removed.
- 220 spot repairs are still planned after frequency healing, the whole budget, on this
  portrait.

## Automated checks run locally

- `cargo test -p aura-render`: 290 tests passed, including the 7 painted-fixture tests in
  `skin_finish.rs` and the 5 plane tests. On those fixtures a mark 38 % darker than the
  skin around it is rebuilt to within 1.0 to 1.3 % of it.
- `cargo test -p aura-recipe -p aura-app --lib` and the retouch integration tests
  (`native_skin_workflow`, `automatic_portrait`, `studio_tools`, `photo_pixels`,
  `portrait_workflow`): passed.
- `cargo clippy -p aura-recipe -p aura-render -p aura-app --all-targets -- -D warnings`:
  clean. `cargo fmt --all -- --check` and `scripts/check-banned.sh`: clean.
- UI: `tsc --noEmit` clean; the full suite (686 tests) passed before the final preset
  change, and the three retouch test files (40 tests) passed after it; production build
  succeeded.

`cargo test --workspace --all-targets`, the phase gates and the three-platform matrix
were left to CI on the pull request.

## What this does not show

- Anything about a photograph of a real person. The portrait is generated, and the
  fixtures are painted.
- Any skin tone but the one in the portrait, on real pixels. The painted fixtures cover
  three tones and two exposures and prove the arithmetic is ratio-based; they are not a
  study.
- That a retoucher would agree with the result, or that every mark on every face is
  found. A mark is a measurement here, not a diagnosis.
- Behaviour on a full-size camera file. The portrait is 1.6 megapixels; a preview and a
  full-size export of a larger file measure different pixels and can differ in which
  faint marks are rebuilt.

## Reproducing it

```powershell
# 1. Bundle the UI and build the shell (GNU toolchain, a target directory without spaces).
cd ui; npm ci; npm run build; cd ..
cargo build --manifest-path ui/src-tauri/Cargo.toml --features custom-protocol

# 2. Start it isolated, with a debugging port.
$env:AURA_TEST_CATALOG = 'C:\aura-check\catalog\catalog.sqlite'
$env:WEBVIEW2_USER_DATA_FOLDER = 'C:\aura-check\webview'
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9337'
& <target>\debug\aura-desktop.exe

# 3. Drive it.
python scripts/verify-professional-retouch.py PORTRAIT C:\aura-check\out
python scripts/verify-professional-retouch.py PORTRAIT C:\aura-check\out-retouch-only --resume --retouch-only
```
