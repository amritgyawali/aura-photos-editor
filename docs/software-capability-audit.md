# AURA capability audit — 30 September 2026

**Verdict: AURA cannot yet edit every kind of photograph or replace a complete professional editing suite.** It can edit supported JPEG/PNG photos, automatically detect suitable portrait faces, perform restrained skin retouch, accept manual refinements, and export verified results. Significant format, history, export-metadata and advanced-feature gaps remain.

This is a broad audit of the current build, not proof of every possible photo, camera, control combination or commercial quality equivalence. No Retouch4me/SkinFiner installation was available for an A/B comparison.

## Build and method

- Source: `3a546d1127ed01ee094601eab084df6ccfcc65d2`; executable SHA-256: `6b230674c7fee706db1e153c6d39aa73be8c876b6dd2344c9298ca70b2075e10`.

- Windows 11 Home 10.0.26200; approximately 7.8 GiB RAM. Existing CPU debug desktop, WebView2 on CDP port 9223. Background work was preserved. Timings are not release benchmarks.

- 25 input files: eight distinct real Pexels photographs (five portraits, landscape, veterinary scene, sneaker product), plus 17 derived/format/corruption fixtures. The second RGB PNG is a byte-identical alias added to separate the same-stem TIFF/PNG grouping; do not count it as an independent photo. Same-stem companions and content deduplication explain why input file count differs from catalog-photo count.

- Actual desktop mouse and keyboard controls were exercised through Playwright/WebView2. Bulk imports, pixel comparisons, isolated format and processor probes used native IPC. This did not automate the Windows file picker, a physical pen/tablet, every PC application, or every keyboard key.

- Originals were opened read-only; edits live in test collections and exports. All 25 audit input SHA-256 hashes remained unchanged. Public photographs were downloaded for local testing; no personal photographs were uploaded.

## Verification totals

| Check | Result |
|---|---|
| UI suite | 553 passed across 57 files after lockfile dependency reinstall |
| TypeScript and Vite production build | Passed |
| Focused native integrations | 110 passed: retouch 29, RAW/color/container/PNG/tiers 68, export/watermark 13 |
| All 24 native retouch processors | Pixel changes and exact isolated Undo/Redo passed |
| Develop parameter probes | 32 executed; 30 changed pixels; lens distortion and CA switches had no effect on the unprofiled sample |
| JPEG/PNG/TIFF export | 3 files each, 9 written/read-back verified; 3 PNGs exactly matched full renderer; TIFF tags report 16 bits/channel |
| Resized watermark export | 1 verified PNG at 256 × 170 |
| Real GUI portrait export | 1 verified JPEG at 640 × 800 |
| Restart | Final recipe hash and pixel hash unchanged after forced restart |
| Full native/workspace gate | Not passed: rustc crashed during catalog compilation |

The focused native tests were compiled from current test sources against the existing desktop rlibs. Initial fallback harness time-crate mismatches were corrected by resolving the exact dependency fingerprint. Synthetic codec roundtrips do not qualify real camera files.

## What works in the tested scope

- Import and browse JPEG/PNG collections; skip repeat imports; render actual source pixels; preserve originals.

- Automatic light correction and YuNet face detection. All five real portraits eventually received three editable operations each. The composed two-person fixture detected both faces. Landscape, sneaker and veterinary scenes produced no suitable human face and skipped portrait retouch.

- Sampled skin texture smoothing, tone uniformity and local dodge/burn. Automatic masks are landmark-guided regions with sampled-color affinity and eye/mouth exclusions, not learned full skin segmentation.

- Exposure, temperature/tint, contrast, tonal sliders, RGB/point/parametric curves, HSL, grading, B&W, texture/clarity/dehaze, sharpening/noise sliders, grain, vignette, calibration adjustment, crop and rotation.

- Native manual heal/patch/clone, spot cleanup, frequency-based tone/detail processing, dodge/burn, skin color, eye/teeth adjustments, fine-line/shine reduction, fabric/backdrop and glare softening. Their names do not imply independent learned AI models.

- Painted retouch selections, erase, feathering, gradients/ranges/inversion in the native implementation, operation history, comparison, zoom/pan, 21 listed adaptive profiles, selected settings sync and snapshot saving.

## What is limited or unavailable

- **Formats:** HEIC/HEIF unsupported; WebP/BMP/GIF filtered by importer. Ordinary RGB/gray TIFF input failed despite TIFF output working. PNG transparency is composited onto white; 16-bit PNG input is reduced to 8-bit. CMYK JPEG opened, but no CMYK/ICC color-fidelity certification was performed.

- **RAW:** synthetic codec coverage only in this audit. CR3/CRX, compressed RAF, RW2 and other proprietary variants have documented restrictions/preview fallbacks. No promise of full-resolution sensor editing for every camera.

- **Retouch intelligence:** no reliable semantic skin/hair/teeth/sky/background models in the older placeholder pipelines; no identity recognition established; no automatic perfect removal of blemishes/permanent marks, reconstruction of hidden glare detail, or guaranteed flattering edit.

- **General editor:** no established full pixel-layer/Smart Object/text/vector document system, layered PSD interchange, liquify/content-aware scaling, panorama/HDR/focus merge, generative fill/expand, sky replacement, AI super-resolution, print layouts or soft proofing.

- **Performance/color:** CPU rendering; no active GPU backend. High-resolution wedding throughput, calibrated monitor/print accuracy, HDR display/export, multi-monitor DPI and 45–100 MP memory behavior remain unqualified.

## Confirmed failures and priorities

| Priority | Finding | Evidence and next action |
|---|---|---|
| High | Reset photo leaves native retouch | Fresh isolated photo: add Dodge, Reset photo, render. Reset pixels equal Dodge pixels, not original. Removing the extra recipe key is missed because changed_paths iterates only paths in the target recipe. Fix deleted-path detection and add an application-level regression. Source: `crates/aura-recipe/src/history.rs:326`. |
| High | Valid TIFF input rejected as damaged | Fresh single-file collections reject Pillow-valid RGB8 and grayscale16 TIFF with AURA-RAW-2002. TIFF export works. Add ordinary TIFF decoding and accurate unsupported-format reporting. Source: `crates/aura-raw/src/meta.rs`. |
| High | Requested sidecars not written | All three-format exports requested sidecar=true; each reported sidecars=0. ExportField initializes recipe_json and original_path to None. Connect persisted recipes/originals and verify sidecar contents. Source: `crates/aura-app/src/delivery_commands.rs:172`. |
| Medium | Original filenames lost on export | {original} produced image, image_2, image_3. ExportField queries original_name/camera_model while actual file names live in photo_file; errors/fallbacks lose naming data. Use the catalog primary-file relationship. Source: `crates/aura-app/src/delivery_commands.rs:178`. |
| Medium | Manual edits labelled automatic | The saved GUI Exposure, Orange, Crop and Native retouch history entries carry source=ai after an automatic pass. Manual protection still works. Fix provenance when committing user edits. Source: `crates/aura-app/src/native_retouch.rs`. |
| Medium | Portrait decode timeout under load | One 1200×1800 JPEG initially failed AURA-RAW-2004 during concurrent tests. Fresh isolated retry and face retouch passed. The error says retryable=false while asking the user to retry. Root cause not established. Source: `crates/aura-raw/src/timeout.rs`. |
| Medium | Window close did not exit promptly | CloseMainWindow returned true, but the audited process was still alive after 20 seconds. Only that process was stopped for restart. Saved recipe and pixels recovered exactly. Shutdown cause not established. Source: `restart-results.json`. |
| Build | Clean native test run blocked | cargo test -p aura-raw -p aura-export --lib crashed rustc compiling aura-catalog (0xc0000409), including a retry with incremental disabled. Focused integrations compiled against the existing desktop dependency graph and passed. This is not a green full-workspace gate. Source: `full-audit-native-tests-noincremental.log`. |
| Build | Local dependency corruption and advisories | Initial UI suite could not load mime-db/db.json. Preserved the error, restored with npm ci, then all 553 tests and production build passed. npm audit reports 5 development-tool findings: 3 moderate, 1 high, 1 critical. No exploitability assessment or dependency upgrade was made. Source: `full-audit-npm-security.json`. |

## Step-by-step edit I performed

Source: [Pexels portrait 1239291](https://www.pexels.com/photo/1239291/). Separate collection: `Mouse and keyboard audit 18-37-26`.

1. Imported the source into an isolated collection through IPC, then selected it using the desktop collection and photo controls. Saved the initial full render.
2. Clicked **Auto enhance photo**: one face, three editable skin/light operations.
3. Typed **+0.35 EV** and **6000 K** using Ctrl+A and Enter.
4. Added a tone-curve point with the mouse; set orange saturation to **−12**.
5. Clicked **4:5** crop and saved **Audit natural portrait** as a named snapshot with Enter.
6. Moved the before/after divider with Home/ArrowRight and enabled clipping warnings.
7. Opened Retouch, selected Dodge, pressed **B**, increased brush size with **]**, painted with a mouse drag, pressed **E**, erased a dab, undid the stroke, and applied with **Enter**.
8. Tested **+**, arrow-key pan, **H**, **0**, split comparison, and **Ctrl+Z / Ctrl+Shift+Z** on the saved operation.
9. Entered an output path using keyboard controls, previewed export names, clicked **Export**, and verified the UI reported one file written and checked.
10. Restarted AURA and verified identical final recipe/pixel hashes.

The first exposure test used exact floating-point equality and falsely failed at `0.3499999940395355`. A dedicated mouse/keyboard repeat with 1e-5 tolerance passed; no application fix was needed. Test code now uses tolerance. Initial raw logs remain preserved.

The final edit is a workflow demonstration with a deliberately visible brush correction, not a best-quality retouch benchmark. The supplied portraits were already professionally lit; they do not prove acne/freckle discrimination, severe repair or fairness across a representative population.

## Reproduction details and ambiguous results resolved

- Start the existing desktop with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`. Run `scripts/prepare-audit-fixtures.py`, `test-capability-audit.py`, `test-audit-gui.py`, `test-audit-followup.py`, `test-audit-isolated.py`, then `test-audit-export-ui.py`. Tests create separate collections and write under `.work-checks/full-audit/`. Preserve results before a new run; `--resume` retains completed capability cases.

- The initial same-stem `test-rgb.png`/`test-rgb.tif` pair selected TIFF as primary. A duplicate PNG alias later altered catalog association, making one TIFF-labelled retry appear to pass. **Fresh single-file TIFF collections reproduced the rejection**; the ambiguous retry is not treated as TIFF support. The fixture generator now gives the opaque PNG a distinct stem.

- Initial 23 tool-history failures followed a retouch-only Reset that did not clear the prior effect. A second pass using explicit stack clearing showed all 24 processors and ordinary Undo/Redo working. A fresh-photo reset reproduction confirms the independent Reset defect.

- Extreme exposure 99999 was accepted but correctly clamped to +5 EV; this is not an out-of-range execution bug.

- Corrupt JPEG rejection, unchanged flat black/one-pixel images and no face on a 90-degree EXIF-rotated face are bounded/expected behaviors, not counted as universal editing failures.

- Scope excludes clean full-workspace CI, real-camera RAW certification, competitor A/B testing, accessibility with a screen reader, cloud/provider delivery, paid APIs, huge catalogs, power-loss durability and every tool combination. Existing tests cover parts of these modules; that is not equivalent to end-to-end qualification.

## All 100 requested feature families

Statuses are scope labels, not a percentage-complete score. “Tested with limits” means the stated sample or focused check passed. “Partial / limited” can include working components. “Unavailable / not established” means the complete workflow is not supported by the inspected implementation/documentation. The original roadmap statuses are historical; this table is the current audit.

| # | Feature | Current assessment | Limits / evidence |
|---|---|---|---|
| 1 | File and folder import | Tested with limits | File/folder IPC import and desktop collection selection tested. Windows file picker was not automated. Same-directory stems are grouped as one photo. `crates/aura-ingest/src/import.rs` |
| 2 | RAW decoding and demosaicing | Partial / limited | 81 RAW/export integration tests include synthetic RAW fixtures. No real camera RAW file was qualified; proprietary compression support varies. `docs/camera-support.md` |
| 3 | EXIF and camera/lens metadata | Partial / limited | EXIF orientation 6 swapped dimensions correctly. Real MakerNotes and all orientation values were not tested. `crates/aura-ingest/src/import.rs` |
| 4 | Ratings, flags and color labels | Partial / limited | Catalog and UI foundations; not exercised through a complete tagging workflow in this audit. `crates/aura-catalog/migrations/0001_init.sql` |
| 5 | Keywords and searchable metadata | Partial / limited | Metadata storage exists; comprehensive search and keyword editing not established here. `crates/aura-catalog/migrations/0001_init.sql` |
| 6 | Collections and rule-based albums | Partial / limited | New collections and navigation tested; smart/rule-based albums not established. `crates/aura-catalog/migrations/0001_init.sql` |
| 7 | Duplicate detection and skip-on-import | Tested with limits | Repeated import reported skipped existing files. Same-stem derivative grouping can hide an individually editable PNG behind a TIFF. `crates/aura-ingest/src/import.rs` |
| 8 | Face recognition and people albums | Partial / limited | Real YuNet face detection in auto retouch; this is not identity recognition. Older people-pipeline detector is a placeholder. `docs/retouch.md` |
| 9 | Tethered capture and live view | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/lightroom-parity.md` |
| 10 | Offline editable proxy previews | Partial / limited | Cached previews exist; editing with an original disconnected was not tested. `crates/aura-preview/src/lib.rs` |
| 11 | Exposure compensation | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 12 | White balance temperature and tint | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 13 | Global contrast | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 14 | Highlight recovery | Partial / limited | Highlight slider changes pixels. It cannot recover detail already clipped out of a JPEG. `crates/aura-render/src/cpu.rs` |
| 15 | Shadow recovery | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 16 | White and black point controls | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 17 | Levels and midpoint gamma | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `ui/src/components/develop/LightroomPanel.tsx` |
| 18 | RGB and luminance histogram | Tested with limits | Histogram visible in the live desktop; unit test passed. No instrumented colorimetry validation. `ui/src/components/develop/Histogram.tsx` |
| 19 | Highlight and shadow clipping overlay | Tested with limits | Live clipping overlay toggled. Preview clipping is not sensor-level RAW clipping. `ui/src/components/develop/Histogram.tsx` |
| 20 | One-click automatic tone enhancement | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `crates/aura-app/src/photo_enhance.rs` |
| 21 | Point and individual RGB tone curves | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 22 | Parametric tone curves | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 23 | HSL color mixer | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 24 | Targeted point-color editing | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `ui/src/components/develop/LightroomPanel.tsx` |
| 25 | Shadow/midtone/highlight color grading | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 26 | Black-and-white channel mixing | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 27 | Vibrance and saturation | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 28 | Camera calibration and matching profiles | Partial / limited | Calibration sliders work; measured camera matching profiles remain incomplete. `docs/colour-management.md` |
| 29 | ICC input/display/output color management | Partial / limited | Output/profile infrastructure exists. No calibrated monitor, wide-gamut roundtrip or print proof certification. `docs/colour-management.md` |
| 30 | Importable 3D LUT looks | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `crates/aura-render/src/creative.rs` |
| 31 | Crop and aspect-ratio presets | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 32 | Straighten and horizon correction | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `ui/src/components/develop/LightroomPanel.tsx` |
| 33 | Rotate and mirror | Partial / limited | Small-angle rotation tested; full rotate/mirror workflow not fully exercised. `crates/aura-recipe/src/contract/recipe.rs` |
| 34 | Perspective and keystone correction | Partial / limited | Perspective implementation exists; not exercised through the desktop in this audit. `crates/aura-render/src/cpu.rs` |
| 35 | Lens distortion correction | Partial / limited | Switch saved but changed no pixels on the sample without a calibrated lens profile. `docs/lightroom-parity.md` |
| 36 | Chromatic aberration and defringing | Partial / limited | Switch saved but changed no pixels on the sample; no measured chromatic-aberration chart tested. `docs/lightroom-parity.md` |
| 37 | Lens vignetting compensation | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `crates/aura-render/src/cpu.rs` |
| 38 | High-quality image resampling | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/delivery.md` |
| 39 | Content-aware scaling | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `crates/aura-recipe/src/contract/recipe.rs` |
| 40 | Liquify and mesh warping | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `crates/aura-recipe/src/contract/recipe.rs` |
| 41 | Local adjustment brush | Tested with limits | Actual mouse brush, erase, stroke undo and Enter-to-apply tested in native retouch. `ui/src/components/develop/MaskPanel.tsx` |
| 42 | Linear gradient masks | Tested with limits | Native retouch gradient implementation and focused tests; not general adjustment-layer gradients. `docs/lightroom-parity.md` |
| 43 | Radial gradient masks | Tested with limits | Feathered retouch ellipse; not a full layer-based radial-adjustment system. `docs/lightroom-parity.md` |
| 44 | Luminance-range masks | Tested with limits | Native retouch luminance gate; covered by renderer/UI tests, not a new real-photo GUI range-mask session. `ui/src/components/develop/MaskPanel.tsx` |
| 45 | Color-range masks | Partial / limited | Sampled skin/color-affinity selection; not unrestricted semantic/color selection. `docs/masks.md` |
| 46 | Automatic subject/background selection | Partial / limited | Semantic segmentation model is untrained. No reliable automatic subject/background masking claim. `docs/lightroom-parity.md` |
| 47 | Automatic sky selection | Partial / limited | Semantic segmentation model is untrained. No reliable automatic sky mask claim. `docs/lightroom-parity.md` |
| 48 | Face, skin, hair and eye masks | Partial / limited | Automatic face boxes/landmarks and cheek/forehead sampled-color masks work. No learned skin/hair/eye segmentation. `docs/retouch.md` |
| 49 | Mask feathering and edge refinement | Tested with limits | Feather/edge-protection processors have focused checks; no perfect hair-edge guarantee. `ui/src/components/develop/MaskPanel.tsx` |
| 50 | Mask add, subtract, intersect and invert | Partial / limited | Brush add/erase, inversion and range intersections exist. Arbitrary layer-mask Boolean workflows are incomplete. `ui/src/components/develop/MaskPanel.tsx` |
| 51 | Spot healing and blemish repair | Tested with limits | Heal, patch heal and local spot cleanup change pixels. Spot heuristics cannot decide whether a mark is permanent. `docs/retouch.md` |
| 52 | Clone stamp | Tested with limits | Sampled clone renders and passes exact isolated Undo/Redo. `docs/retouch.md` |
| 53 | Content-aware object removal | Partial / limited | Small-area heal/patch tools work; general semantic/generative object removal is not production-ready. `docs/lightroom-parity.md` |
| 54 | Red-eye correction | Tested with limits | Manual red-eye tool renders; no diverse flash-red-eye perceptual benchmark. `docs/retouch.md` |
| 55 | Texture-preserving skin smoothing | Tested with limits | Sampled smoothing and auto face-guided texture pass work. Not commercial quality parity. `docs/retouch.md` |
| 56 | Frequency separation | Partial / limited | Frequency-based tone/detail processor works; separate editable high/low pixel layers are unavailable. `docs/retouch.md` |
| 57 | Dodge and burn / local relighting | Tested with limits | Manual dodge/burn and automatic sampled-skin light balancing work. `docs/lightroom-parity.md` |
| 58 | Eye and teeth enhancement | Tested with limits | Manual eye/teeth processors work; no automatic semantic sclera/teeth targeting. `docs/retouch.md` |
| 59 | Skin-tone uniformity | Tested with limits | Sampled skin tone uniformity works; similar-colored non-skin pixels can match. `docs/retouch.md` |
| 60 | Depth-aware background blur | Partial / limited | No production depth map/background blur workflow established. `crates/aura-recipe/src/contract/recipe.rs` |
| 61 | Pixel layers and layer groups | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `crates/aura-recipe/src/contract/recipe.rs` |
| 62 | Adjustment layers | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `crates/aura-recipe/src/contract/recipe.rs` |
| 63 | Per-layer masks | Partial / limited | Retouch selections are per operation; general pixel-layer masks unavailable. `crates/aura-recipe/src/contract/recipe.rs` |
| 64 | Layer blend modes and opacity | Partial / limited | Retouch strength exists; general layer blend modes unavailable. `crates/aura-recipe/src/contract/recipe.rs` |
| 65 | Smart Objects / linked image objects | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `crates/aura-recipe/src/contract/recipe.rs` |
| 66 | Reorderable nondestructive filter stack | Tested with limits | Native retouch operation stack exists; not a universal smart-filter/layer stack. `crates/aura-render/src/cpu.rs` |
| 67 | Editable text layers | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `crates/aura-recipe/src/contract/recipe.rs` |
| 68 | Vector shapes and editable paths | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `crates/aura-recipe/src/contract/recipe.rs` |
| 69 | Automatic layer alignment and blending | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `crates/aura-recipe/src/contract/recipe.rs` |
| 70 | Editable layered document interchange | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `crates/aura-recipe/src/contract/recipe.rs` |
| 71 | Bracketed HDR merge | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/lightroom-parity.md` |
| 72 | Panorama stitching | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/lightroom-parity.md` |
| 73 | Focus stacking | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/lightroom-parity.md` |
| 74 | Multi-frame noise reduction | Partial / limited | No multi-frame noise-reduction workflow established. `docs/restoration.md` |
| 75 | AI denoising | Partial / limited | Classical noise sliders work. Learned denoise and face-recovery heads remain untrained. `docs/restoration.md` |
| 76 | Sharpening, texture, clarity and dehaze | Tested with limits | Sharpening, texture, clarity and dehaze changed sample pixels; quality is not benchmarked against competitors. `ui/src/components/develop/LightroomPanel.tsx` |
| 77 | AI enlargement / super-resolution | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/restoration.md` |
| 78 | Sky replacement | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/lightroom-parity.md` |
| 79 | Generative fill | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/lightroom-parity.md` |
| 80 | Generative canvas expansion | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/lightroom-parity.md` |
| 81 | Presets and adjustable preset strength | Tested with limits | 21 profiles listed; first profile at 65% applied repeatably. Not all 21 profiles visually graded. `ui/src/components/develop/LightroomPanel.tsx` |
| 82 | Copy, paste and synchronize edits | Tested with limits | Selected tone settings synced to one target with manual protection; full collection stress sync not tested. `ui/src/components/develop/LightroomPanel.tsx` |
| 83 | Reference-image look matching | Partial / limited | Reference-style analysis/matching implementation and UI tests exist; no new end-to-end reference set audit. `ui/src/components/look/MatchLookPanel.tsx` |
| 84 | Nondestructive history and Undo/Redo | Known failure | Normal isolated Undo/Redo passed for all 24 tools and keyboard workflow. Reset photo fails on retouch-only edits. `crates/aura-app/src/develop_commands.rs` |
| 85 | Virtual copies and named snapshots | Partial / limited | Named snapshot saved through keyboard. Restore and virtual-copy workflows not fully exercised. `crates/aura-app/src/develop_commands.rs` |
| 86 | Before/after comparison and zoom | Tested with limits | Live compare divider, zoom, pan, fit and before/after tested. `ui/src/components/develop/PhotoStudio.tsx` |
| 87 | Macros, actions and plug-in scripting | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `ui/src/components/autopilot/prepareCollection.ts` |
| 88 | Batch editing and export queues | Partial / limited | Multi-photo IPC processing and export tested. Thousands-of-images queue/cancel/resume not qualified. `ui/src/components/autopilot/prepareCollection.ts` |
| 89 | Film grain and analog looks | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `crates/aura-render/src/creative.rs` |
| 90 | Creative post-crop vignette | Tested with limits | Sample execution or focused checks passed; not universal format or visual-quality proof. `crates/aura-render/src/creative.rs` |
| 91 | JPEG/PNG/TIFF export and bit-depth options | Partial / limited | JPEG/PNG and true 16-bit TIFF export verified. Ordinary RGB/gray TIFF input fails; PNG alpha/16-bit precision not retained. `crates/aura-export/src/api.rs` |
| 92 | Export resizing and output sharpening | Tested with limits | 256-pixel long-edge resize verified; output sharpening exercised by export integration tests. `docs/delivery.md` |
| 93 | Batch naming and metadata/privacy controls | Known failure | Original-name template produced image/image_2/image_3. Privacy policy checks are structural, not forensic real-camera metadata qualification. `docs/delivery.md` |
| 94 | Text and image watermarks | Tested with limits | Text/logo watermark UI and native compositing exist; synthetic RGBA watermark plus 256-pixel export verified. `docs/delivery.md` |
| 95 | Soft proofing and gamut warnings | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/lightroom-parity.md` |
| 96 | Print layouts and contact sheets | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/delivery.md` |
| 97 | XMP sidecars and edit interchange | Known failure | Requested export sidecars were absent. Export field supplies recipe_json=None; XMP serialization elsewhere does not prove working interchange. `crates/aura-recipe/src/xmp.rs` |
| 98 | GPU-accelerated rendering | Partial / limited | CPU renderer used. GPU shaders/ports do not constitute an active GPU rendering backend. `crates/aura-render/src/lib.rs` |
| 99 | Tiled rendering and multiresolution caching | Tested with limits | Preview/cache implementation and earlier cache tests; final edited recipe/pixels persisted through forced restart. `crates/aura-preview/src/lib.rs` |
| 100 | HDR display editing and HDR export | Unavailable / not established | Current source/documentation does not establish this complete desktop workflow. `docs/colour-management.md` |

## All 24 native retouch processors

| Processor | Pixel effect | Exact isolated Undo/Redo |
|---|---|---|
| heal | True | True |
| patch_heal | True | True |
| clone | True | True |
| auto_blemish | True | True |
| frequency | True | True |
| skin_smooth | True | True |
| skin_uniformity | True | True |
| portrait_dodge_burn | True | True |
| micro_dodge_burn | True | True |
| dodge | True | True |
| burn | True | True |
| skin_color | True | True |
| color_match | True | True |
| mattify | True | True |
| under_eye | True | True |
| wrinkle | True | True |
| teeth | True | True |
| eye_clean | True | True |
| eye_detail | True | True |
| red_eye | True | True |
| fabric | True | True |
| backdrop | True | True |
| glare | True | True |
| makeup | True | True |

## Files and evidence

- [Machine-readable report](audits/2026-09-30-capability-results.json) includes per-case results, input hashes, model reports, GUI history and source links.
- [Interactive local report](../.work-checks/full-audit/report.html) contains before/after images and searchable feature/issue tables.
- [Edited PNG](../.work-checks/full-audit/gui/final.png) and [exported JPEG](../.work-checks/full-audit/gui/export/gallery/mouse-and-keyboard-audit-18-37-26_0001.jpg).
- Local raw logs and screenshots: `.work-checks/full-audit*`; input sources are individually attributed in `input-manifest.json`. Downloaded photographs and large screenshots are local evidence, not committed source assets.

## Recommended completion order

1. Fix retouch reset and manual provenance; verify reset/snapshot/history transitions with removed recipe fields.
2. Fix real catalog-to-export filename/recipe wiring; test exported sidecars and roundtrip interchange.
3. Implement ordinary TIFF input and accurately disclose unsupported/precision-losing formats.
4. Resolve shutdown/decode stalls and the local native compiler failure; run clean full CI and representative high-resolution workloads.
5. Train/validate semantic models and integrate one reliable face pipeline across the product; benchmark diverse real images with human review.
6. Treat layers, merging, generative tools and other absent workflows as separate implementation projects.

No application behavior was changed as part of this audit. Dependency restoration, test harnesses, evidence and reporting are the changes. The failures above remain open.
