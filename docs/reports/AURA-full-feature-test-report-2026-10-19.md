# AURA full feature test report — 19 October 2026

## Verdict and scope

AURA was launched and tested through its actual Windows desktop/native bridge, a broad native workspace test run, the complete frontend suite and direct UI navigation. The audit found a reproducible multi-root import failure and a numeric-input error-recovery usability problem that need correction. Many rendering, editing, retouching, masking, history, colour-reference and local export paths passed their stated checks. This is not an all-features-perfect verdict, an Evoto parity claim or proof that every control combination, skin boundary and camera format works.

The inventory contains **325 distinct registered native commands** in 326 handler entries. `within_moment` appears twice. Commands are not equivalent to independent photographic features. Every command has a coverage entry below. The harness recorded 78 command names; the isolated UI audit ring recorded 88 names, with 132 distinct registered names in their union. A missing record is explicitly labelled and is not proof of dead code: the UI ring is finite, and some native bindings bypass the shared logger. Opening a panel or receiving a status response is not evidence that its full editing behaviour works.

The source tree already contained uncommitted development work. This audit did not fix product source or conceal failures by changing the build under test. Test harness inputs were corrected when they did not match the UI's actual types/units; the original refusals remain in the evidence. The executable identifies as AURA 1.0.0, debug with the packaged frontend and `custom-protocol`. It is the separate build produced in the preceding continuation. The frontend was rebuilt during this audit and produced the same `index-Bw500qHk.js` bundle.

## Results at a glance


| Layer | Observed result | Meaning |
|---|---|---|
| Native workspace | 4217 passed; 0 failed; 48 ignored across 273 completed result blocks | Default configured workspace tests; explicit ignores include media/environment, timing/debug and fixture-regeneration cases. See the reasons and native ledger. |
| Complete UI suite, initial | 787 / 788 passed; 1 failed | One high-operation-count retouch test exceeded its five-second timeout. |
| UI timeout isolated recheck | Passed | Same test passed alone; this does not erase the initial timing failure. |
| Complete UI suite, one worker | 788 / 788 passed; 0 failed | Reduced test contention; same product source and default test timeout. |
| Frontend production build | Passed | TypeScript and Vite; existing large-chunk warning. |
| Native desktop retouch | 31 processors exercised | Zero strength, restoration, Undo and Redo checked on an explicit synthetic chart. |
| Live desktop cases | 257 | Case results and their scope are individually recorded below. |
| GUI navigation | 26 observations | Six main workspaces, eighteen advanced groups and actual exposure/Undo interaction. |
| Local format exports | 3 written, 3 verified | JPEG, PNG and 16-bit TIFF independently reopened; embedded ICC recorded. |
| Actual Canon CR3 | Native-resolution render and enhancement passed | One real file, not qualification of all RAW cameras/codecs. |
| Original inputs | All six checksums unchanged | Includes the real CR3, portrait JPEG, chart variants and intentionally corrupt fixture. |
| Restart | True recipe; True reference library | Persistence of saved recipe identity and colour descriptors; not unsaved-draft recovery. |


## Critical finding: multiple-root import stops early

The test submitted six roots in one `start_ingest` call: a PNG chart, its byte-identical alias, a JPEG version, an invalid JPEG, a real two-person portrait JPEG and `989A2698.CR3`. Only the first PNG appeared. `ingest_progress` reported `known: true, done: 1, total: 1, running: false`; the separate portrait and RAW were absent. The database import journal still said `importing`, with no finish timestamp. A repeat fresh-project attempt reproduced the early stop. Importing the remaining roots individually worked.

This is a **high-priority functional failure** in this mixed multi-root scenario. The audit has not isolated whether duplication, per-root journaling, error propagation or their interaction is the primary cause. Reproduce it with the retained roots; inspect the worker error and journal transaction; ensure an error/cancellation finishes the journal and reaches the UI; and test that later valid roots are still processed or that a clear failure is reported. Do not describe the collection as fully imported merely because the progress counter says finished. The workaround used here is an individual-root import, not a product fix.

The corrupt JPEG was initially indexed as metadata-only input and was explicitly refused when native decoding was requested. The refusal created an `AURA-RAW-2001` problem naming that file, rather than returning photo pixels. That decode refusal is an expected protection, separate from the multi-root defect.

## Method and isolation

1. Started the audited desktop with a new `AURA_TEST_CATALOG`, a separate WebView profile and CDP port 9225. All audit decisions, masks and recipes were written to that disposable catalogue.
2. Ran `cargo test --workspace --locked --no-fail-fast -j2` and the complete Vitest suite. Preserved the original UI timeout and its follow-up runs. Production source was not edited to make tests pass.
3. Imported explicit fixtures, checked a native full-resolution CR3 decode, and exercised every listed native retouch tool through the real desktop IPC. Compared SHA-256 of the returned RGB bytes for Undo, Redo, zero amount and amount restoration.
4. Changed development controls individually, rendered, undid them and restored a named baseline. Tested point curves in the UI's integer 0–255 coordinates, all eight HSL bands, monochrome mixing, grading, geometry, colour spaces and local mask shapes.
5. Applied the real native automatic portrait workflow, requested all fourteen named image selections, ran local analysis/culling, checked reference matching and protected manual edits, and exported only into the audit directory.
6. Opened the main and advanced panels, made an actual keyboard adjustment on the exposure slider and used the visible Undo button. Captured screenshots and observable controls. File pickers and physical tablet input were not automated.
7. Restarted the audit process and verified recipe/reference persistence. Checked all six input file hashes after mutation tests. Existing delivery JPEGs were not replaced.

Case `PASS` means the specific stated assertion or API response succeeded. It does not imply a semantic mask is anatomically perfect, that a zero-count status proves successful processing, or that an artistic edit has passed professional acceptance. Timings were collected in a debug build while other tests/compilation ran and are not release benchmarks.


## Build identity

| File | Bytes | SHA-256 |
|---|---|---|
| aura-desktop.exe | 440184479 | 0e1ec26e4a4bdee2b6261441fdb7bcb8ac880d17177b7a3530c14c2f96fd8242 |
| WebView2Loader.dll | 160320 | 8427b1fc58ec707813e5c0a51eb5d69397bb333250a7b891be4d3b123f1e0f1c |

The running renderer reported the following capability identity. GPU availability here qualifies the configured output backend, not every image-processing stage:

```json
{
  "backend": "wgpu",
  "maxTexture": 9459,
  "precisionBits": 24,
  "maxWorkingBytes": 1073741824,
  "engine": "aura-render/1.0.0+profiles.1+portrait.1+display.1+raw.2+clear.enclosed-light-v2+matte.bounded-contour-v2-zero-cell-support+undereye.socket-preserving-v2-reference-consensus+boundary.inward-distance-v1+skin.robust-patch-v3-pixel-connectivity+wgpu.output.1.Vulkan.10de.1c82.560.94.NVIDIA GeForce GTX 1050 Ti.min262144",
  "degradation": null,
  "degradationMessage": null
}
```

## Input coverage and preservation

| Input | Before SHA-256 | Final unchanged |
|---|---|---|
| skin_chart.png | 2508632c67a679270632f58b2c1050e655d5491f0769dc7d7dbd4398ffaac498 | True |
| skin_chart_duplicate.png | 2508632c67a679270632f58b2c1050e655d5491f0769dc7d7dbd4398ffaac498 | True |
| skin_chart.jpg | 68cb48a29f24d9f75713a5c5771db847a629aa981ddcde6db3ea2ae74f0515f1 | True |
| broken.jpg | e4bfd5f0c91ba04cfeaddf9a63939c1307d74159a660ec67a102d2f76732d05f | True |
| 164A2098-original-fast.jpg | 2a8e2d1ea33d194079b94cf484396d0e0c3feb44b53af4c2499e063c649d4cb0 | True |
| 989A2698.CR3 | 562839abbb3ba6ef518cf49ea6c08c9bb7cc848d00d6eb74f630c9f4583e532f | True |


The real JPEG is a 1066 × 1600 native preview from prior local work, containing two people. It supports a practical workflow check but has relatively small faces, so it cannot establish fine pore or under-eye quality. The actual Canon CR3 was tested independently at its own sensor-derived output resolution. The small synthetic chart is 160 × 128 and deliberately contains colour, texture, dark bands, spots and highlights to exercise operators cheaply. It is not a skin-classification benchmark or an actual face.

Other proprietary RAW variants, calibrated colour-target photographs, different skin-tone cohorts, lens-profile bodies, physical monitors/printers and large cross-camera edited datasets were not all supplied. Codec unit/synthetic tests and one Canon file must not be presented as qualification of every CR2, ARW, NEF, RAF, RW2 or future camera.

## Retouch processors: individual results

Each processor was appended as one operation. The audit rendered its full-strength result, undid and redid it, set its amount to zero, set 50%, and restored its original full amount. Exact pixel equality was required for Undo, Redo, zero-to-baseline and restoration. A conditional detector is allowed to make no change when it finds no eligible defect. No synthetic test demonstrates that a brush is selecting the correct eyelid or jaw on every person.


| Processor | Visible effect on chart | Undo / Redo / zero / restore | Scope |
|---|---|---|---|
| acne_clear | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| heal | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| patch_heal | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| frequency_heal | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| clone | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| auto_blemish | No qualifying effect measured | Exact / exact / exact / exact | Synthetic operator fixture |
| frequency | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| texture_graft | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| skin_smooth | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| skin_uniformity | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| portrait_dodge_burn | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| micro_dodge_burn | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| dodge | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| burn | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| skin_color | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| color_match | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| mattify | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| under_eye | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| wrinkle | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| stretch_marks | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| teeth | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| eye_clean | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| eye_detail | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| red_eye | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| fabric | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| backdrop | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| glare | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| makeup | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| colorize | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| background_color | Yes | Exact / exact / exact / exact | Synthetic operator fixture |
| reshape | Yes | Exact / exact / exact / exact | Synthetic operator fixture |


Automatic spot cleanup (`auto_blemish`) produced no change on this chart. Its state/amount invariants passed, but effective spot removal is not established by that fixture. The acne, healing and frequency-healing operators did change chart pixels. Local eye/teeth/glare/colour operations changing a chart do not certify natural anatomy, detail recovery hidden by glare, or accurate makeup colours.

The actual two-person portrait planner detected and retouched two faces and stored 57 editable operations in the measured run, including 28 patch repairs. Its stored tool list contained **no frequency separation, dodge, burn, portrait dodge/burn or micro dodge/burn**. This checks the user's restriction on advanced operations for multi-person images. The real single-person RAW was also enhanced and rendered, with its stored operation report retained separately. No universal pore-removal or perfect-edge verdict is made.

## Development controls and editable strengths

The table records a representative changed value, not every possible slider position or simultaneous combination. Some parameters need an enabled parent effect (for example grain size with grain amount above zero) or matching coloured content to produce a visible difference. Lens profile switches can be accepted without a measured effect on an unprofiled chart. Those cases remain conditional rather than proving an active lens correction.


| Path | Tested value | Pixel effect | Undo exact |
|---|---|---|---|
| global.temperature | 7000.0 | Yes | True |
| global.tint | 45.0 | Yes | True |
| global.exposure | 1.5 | Yes | True |
| global.contrast | 30.0 | Yes | True |
| global.highlights | 30.0 | Yes | True |
| global.shadows | 30.0 | Yes | True |
| global.whites | 30.0 | Yes | True |
| global.blacks | 30.0 | Yes | True |
| global.texture | 30.0 | Yes | True |
| global.clarity | 30.0 | Yes | True |
| global.dehaze | 30.0 | Yes | True |
| global.vibrance | 30.0 | Yes | True |
| global.saturation | 30.0 | Yes | True |
| global.parametric.highlights | 30.0 | Yes | True |
| global.parametric.lights | 30.0 | Yes | True |
| global.parametric.darks | 30.0 | Yes | True |
| global.parametric.shadows | 30.0 | Yes | True |
| global.parametric.shadow_split | 28.0 | Not measured on this context | True |
| global.parametric.midtone_split | 53.0 | Not measured on this context | True |
| global.parametric.highlight_split | 78.0 | Not measured on this context | True |
| global.sharpen.radius | 1.375 | Not measured on this context | True |
| global.sharpen.detail | 40.0 | Not measured on this context | True |
| global.sharpen.masking | 15.0 | Not measured on this context | True |
| global.noise.luminance | 15.0 | Yes | True |
| global.noise.detail | 65.0 | Not measured on this context | True |
| global.noise.colour | 15.0 | Yes | True |
| global.effects.vignette.amount | 30.0 | Yes | True |
| global.effects.vignette.midpoint | 65.0 | Not measured on this context | True |
| global.effects.vignette.roundness | 30.0 | Not measured on this context | True |
| global.effects.vignette.feather | 65.0 | Not measured on this context | True |
| global.effects.vignette.highlights | 15.0 | Not measured on this context | True |
| global.effects.grain.amount | 15.0 | Yes | True |
| global.effects.grain.size | 40.0 | Not measured on this context | True |
| global.effects.grain.roughness | 65.0 | Not measured on this context | True |
| global.calibration.shadows_tint | 30.0 | Yes | True |
| global.calibration.red_hue | 30.0 | Yes | True |
| global.calibration.red_saturation | 30.0 | Yes | True |
| global.calibration.green_hue | 30.0 | Yes | True |
| global.calibration.green_saturation | 30.0 | Yes | True |
| global.calibration.blue_hue | 30.0 | Yes | True |
| global.calibration.blue_saturation | 30.0 | Yes | True |
| global.colour_grade.blending | 65.0 | Not measured on this context | True |
| global.colour_grade.balance | 30.0 | Not measured on this context | True |
| lens.vignette | 15.0 | Yes | True |
| geometry.rotate | 5 | Yes | True |
| global.hsl.red.h | 20 | Yes | True |
| global.hsl.red.s | 20 | Yes | True |
| global.hsl.red.l | 20 | Yes | True |
| global.hsl.orange.h | 20 | Yes | True |
| global.hsl.orange.s | 20 | Yes | True |
| global.hsl.orange.l | 20 | Yes | True |
| global.hsl.yellow.h | 20 | Yes | True |
| global.hsl.yellow.s | 20 | Yes | True |
| global.hsl.yellow.l | 20 | Yes | True |
| global.hsl.green.h | 20 | Not measured on this context | True |
| global.hsl.green.s | 20 | Not measured on this context | True |
| global.hsl.green.l | 20 | Not measured on this context | True |
| global.hsl.aqua.h | 20 | Not measured on this context | True |
| global.hsl.aqua.s | 20 | Not measured on this context | True |
| global.hsl.aqua.l | 20 | Not measured on this context | True |
| global.hsl.blue.h | 20 | Yes | True |
| global.hsl.blue.s | 20 | Yes | True |
| global.hsl.blue.l | 20 | Yes | True |
| global.hsl.purple.h | 20 | Yes | True |
| global.hsl.purple.s | 20 | Yes | True |
| global.hsl.purple.l | 20 | Yes | True |
| global.hsl.magenta.h | 20 | Yes | True |
| global.hsl.magenta.s | 20 | Yes | True |
| global.hsl.magenta.l | 20 | Yes | True |
| global.colour_grade.shadows.hue | 35 | Not measured on this context | True |
| global.colour_grade.shadows.saturation | 25 | Yes | True |
| global.colour_grade.shadows.luminance | 20 | Yes | True |
| global.colour_grade.midtones.hue | 35 | Not measured on this context | True |
| global.colour_grade.midtones.saturation | 25 | Yes | True |
| global.colour_grade.midtones.luminance | 20 | Yes | True |
| global.colour_grade.highlights.hue | 35 | Not measured on this context | True |
| global.colour_grade.highlights.saturation | 25 | Yes | True |
| global.colour_grade.highlights.luminance | 20 | Yes | True |
| global.colour_grade.global.hue | 35 | Not measured on this context | True |
| global.colour_grade.global.saturation | 25 | Yes | True |
| global.colour_grade.global.luminance | 20 | Yes | True |
| lens.distortion | true | Not measured on this context | True |
| lens.ca | true | Not measured on this context | True |
| geometry.crop | [0.1, 0.1, 0.9, 0.9] | Yes | True |
| global.sharpen.amount | 23 | Yes | True |
| global.curve.points | [[0, 0], [128, 152], [255, 255]] | Yes | True |
| global.channel_curves.red.points | [[0, 0], [128, 152], [255, 255]] | Yes | True |
| global.channel_curves.green.points | [[0, 0], [128, 152], [255, 255]] | Yes | True |
| global.channel_curves.blue.points | [[0, 0], [128, 152], [255, 255]] | Yes | True |


The initial harness supplied sharpening amount 22.5, normalized 0–1 curve coordinates, and sync group `light`. The product contracts use an integer sharpening amount, 0–255 point curves and sync group `tone`. These malformed native requests were correctly refused. Corrected representative inputs passed. The raw failed harness rows remain visible with that assessment, so they are not misreported as broken valid slider values. All nine valid settings-sync groups subsequently returned successfully; that is a wire/processing check, not proof that every neutral source group causes visible change.

The actual GUI exposure adjustment changed the saved value from approximately 0.4000 to 0.4500; the visible Undo button restored approximately 0.4000. Native retouch strengths were tested at 0%, 50% and 100%, with restoration from the same operation rather than multiplication of the prior amount.

### Numeric-input recovery finding

A later **actual GUI** test entered 22.5 into the sharpening number field, whose step is 1. The native write was safely refused and the saved recipe hash stayed unchanged. However, the entire edit control area became disabled with the generic message “One edit could not be read… Retry preview”; typing a valid correction was blocked. Pressing **Retry preview** restored the controls. The valid integer 23 then saved, and Undo restored the original value 15 and the original recipe hash.

This is a medium-priority input-validation/recovery usability issue, separate from correct native rejection. Validate integral-step input before invoking the native write, identify the invalid field clearly and keep valid controls available. The protection prevented data corruption, but the user should not have to reload a preview to correct a number. The recovery result is retained in the GUI ledger as `LIMITED`.

## Masks, skin selection and protection

Linear, radial, brush and luminance-range local masks were created, given exposure adjustments, rendered and reduced to zero amount. Their local edits visibly changed the chart, and zero amount restored exact baseline pixels. The full native suite includes dedicated selection/contour/under-eye invariants; those tests are listed in the native ledger.

All fourteen named image selections were requested on the actual two-person image. Results below distinguish a refusal/no-region response from a saved selection. A saved mask ID establishes a functioning selection path, not dense anatomical correctness. Facial subregion selections and segmenter masks have different evidence and resolution limits; use their native overlays and visual review before retouching close to eyes, nose, mouth, hairline or jaw.


| Selection | Saved mask | Message |
|---|---|---|
| subject | mask-22a47a5f1d654a209015eaf929cccdf9 | No additional message |
| background | mask-a074a94387c945a7aa27cee2a0d0d2f4 | No additional message |
| sky | None | No sky found: no open sky at the top of the frame. |
| face_skin | mask-d28e5edd753145ad8c654ba6114e44c5 | No additional message |
| body_skin | mask-57b7116b44d14d13aec193c5eabffafb | No additional message |
| hair | mask-c3b4a29c58e249c3b5f0c1731fdf9c1f | No additional message |
| clothes | mask-e124f1abef98431c804b78dab88a6c42 | No additional message |
| eyes | mask-32fc396d481646119176d3fff240f71f | No additional message |
| iris | mask-741cb15070b64386a2d709d761c7648a | No additional message |
| sclera | mask-a17e7c5b6c934d478dac8cb5d3a15d0d | No additional message |
| eyebrows | mask-66122278890346bdad6d3d0c8c5ab08c | No additional message |
| lips | mask-80707de1dd5f4e2086601a63046e68ab | No additional message |
| teeth | mask-2a3ea586e8174f18b8de7db96b03c8f1 | No additional message |
| facial_hair | mask-cecef36624fb4ba7ab3306df124f8f49 | No additional message |


The scene has no open sky, and the sky selection correctly returned no mask with an explanatory message. It is not a successful sky segmentation sample. The preceding pixel-connectivity regression fix is active in the renderer identity. Its tests address thin gaps in explicitly sampled connected-skin mode; it is not a replacement for all automatic semantic masks.

## Reference colour matching, profiles and manual protection

A previously edited **AURA** JPEG, `989A2699_AURA_FINAL.jpg` at 4480 × 6720, was imported as a reference. It was not an Evoto export. Its supported ICC profile was converted and the descriptor library survived restart. A different real portrait was analysed and each amount/tone/colour/warmth axis was applied through native-resolution validation. Removing the matched treatment succeeded. A stale session was subsequently refused after a newer manual exposure change, and the recipe hash remained unchanged by that refused application.

These checks establish the fitted workflow and manual protection in this scenario. They do not recover exact original editing settings from a rendered reference, copy its facial retouch, or demonstrate Evoto-equivalent colour. The measured appearance distance is an internal fit statistic. The older same-photo paired-reference and multi-reference folder-style workflows have separate data requirements; their unit/integration/UI results are recorded separately and are not relabelled as this single-reference test. Personal style training and robust cross-camera style generalization still need appropriate edited/original datasets.


| Reference step | Result | Before / after distance | Protected fields |
|---|---|---|---|
| Portrait colour-reference fitting | PASS | 21.23995 / 17.8372 | 31 |
| Colour-reference strength: amount | PASS | 21.152925 / 19.4034 | 0 |
| Colour-reference strength: tone | PASS | 21.152925 / 20.10913 | 0 |
| Colour-reference strength: color | PASS | 21.152925 / 19.475857 | 0 |
| Colour-reference strength: whiteBalance | PASS | 21.152925 / 19.517956 | 0 |


## Culling, analysis, story, cameras, gallery and quality control

The audit ran local embedding, people grouping, scene classification, moment grouping, integrity, emotion and composition passes, then rebuilt the similarity index. A first cull on unanalysed inputs returned zero eligible photos; resize and keep/reject operations were refused. Repeating the cull after analysis allowed resize and all three keep/reject/clear actions. These earlier refusals demonstrate a prerequisite rather than successful cull manipulation; the user flow should make missing analysis clear.

Camera matching ran on a one-camera collection and correctly reported that there was nothing to match. It used no matched pairs, so it does not establish successful cross-camera calibration. Gallery consistency and an inspect-only QC pass returned reports; empty/limited measurements are not a calibrated photo-quality approval. Full story, identity, album, learning and generative-quality acceptance requires representative additional data. Their registration, GUI reachability and automated test evidence are retained in the ledgers.

## Export, metadata, persistence and cache

The audit named, wrote, read back, hashed and sealed three local exports of the test chart: JPEG, PNG and 16-bit TIFF. A separate image reader reopened all three, checked dimensions and embedded ICC data, and read TIFF BitsPerSample. These outputs are under the audit directory; existing customer delivery files were not overwritten. Native tests cover further export, watermark, metadata and delivery paths, but this desktop scenario was not a gallery upload, print workflow or full multi-file production delivery.


| File | Dimensions | ICC bytes | Bits per sample | SHA-256 |
|---|---|---|---|---|
| audit_skin_chart_0001.jpg | [160, 128] | 6632 | 8-bit raster | 6ea8d5c3172d78cf8302802168209713174c1a1d805565e815b8600de84e5f29 |
| audit_skin_chart_0001.png | [160, 128] | 0 | 8-bit raster | 5a3857df466d9b60df9d637ee4fe02eb2da61871d9439e4c5e9c0d2ac5bd13f4 |
| audit_skin_chart_0001.tif | [160, 128] | 6632 | [16, 16, 16] | 3f3ec5793bdf4b46503c742afa5522d9981efc29d7b98c5424727b36842f0a08 |


The saved recipe hash and reference library matched after restarting the audit desktop. Unsaved-draft recovery was not tested. The preview cache budget was temporarily changed for the disposable project, its cache purged and its previous budget restored. This was not a purge of the user's production collection or a hardware stress/performance qualification.

## GUI navigation and visible controls

All six workspaces and eighteen advanced groups opened. Opening a status-only panel is recorded as reachability, not feature completion. The inspector captured up to 400 visible controls per observation, including labels, disabled state, ranges and steps. The high-operation retouch panel reached that capture cap; consult the source inventory and native tool table rather than interpreting 400 as its complete control count. The initial GUI script used a `button` selector for a filmstrip item, whose accessible role is `option`; correcting that test selector allowed the slider/Undo test to pass. It was a harness issue, not a missing filmstrip feature.


| Panel / check | Observation | Visible alerts |
|---|---|---|
| Workspace: Start | OPENED | 0 |
| Workspace: Photos | OPENED | 0 |
| Workspace: Auto edit | OPENED | 0 |
| Workspace: Match a look | OPENED | 0 |
| Workspace: Export | OPENED | 0 |
| Workspace: Advanced | OPENED | 0 |
| Advanced: Photo measurements | OPENED | 0 |
| Advanced: People | OPENED | 0 |
| Advanced: Story and moments | OPENED | 0 |
| Advanced: Camera matching | OPENED | 0 |
| Advanced: Cull and selection | OPENED | 0 |
| Advanced: Photo evidence | OPENED | 0 |
| Advanced: Detailed development | OPENED | 0 |
| Advanced: Portrait regions | OPENED | 0 |
| Advanced: Precision retouch, hair color and proportions | OPENED | 0 |
| Advanced: Object cleanup | OPENED | 0 |
| Advanced: Learn a style | OPENED | 0 |
| Advanced: Complete collection workflow | OPENED | 0 |
| Advanced: Provider catalogue | OPENED | 0 |
| Advanced: Activity log | OPENED | 0 |
| Advanced: Quality review | OPENED | 0 |
| Advanced: Gallery consistency | OPENED | 0 |
| Advanced: Albums & curation | OPENED | 0 |
| Advanced: Performance & storage | OPENED | 0 |
| Actual UI exposure slider and Undo | PASS | See interaction values |
| Integer numeric entry requires preview retry after invalid fraction | LIMITED | See interaction values |


## Priority work and limits

1. **High — mixed multi-root import:** repair the early termination and inconsistent completed-progress/uncompleted-journal states. Add the retained duplicate/corrupt/later-valid-root scenario to the native and UI workflow tests.
2. **Medium — numeric-input recovery and high-operation UI timing:** validate integral numeric fields before writes and avoid disabling the entire editor after a user-correctable invalid value. Separately investigate rendering/interaction with hundreds of saved repairs. Preserve the initial five-second timeout, isolated recheck and serial full-suite results. Test realistic large histories under measured hardware conditions; do not simply raise all test timeouts and declare the UI fast.
3. **Acceptance gap — skin and anatomical edges:** use representative close portraits, different tones, makeup, facial hair, occlusion, ceremonial marks, neck shadows and native-scale reviews. Exact mask ground truth and professional visual acceptance were not supplied by one chart or one couple portrait.
4. **Acceptance gap — broader formats, hardware and outputs:** qualify real files per camera, ICC round trips, mixed lighting, calibrated colour/print, different GPUs and large image/batch memory behaviour. Source support and synthetic codec tests are narrower claims.
5. **Coverage gap — externally dependent features:** paid AI provider calls, publishing/upload, purchases/activation, physical tablet drivers, multi-camera calibration and learned-style training on an approved dataset were not executed as real external workflows. Local mocks, guards and contracts do not turn those into desktop end-to-end passes.
6. **Maintenance — command registration:** remove the duplicate `within_moment` handler entry after review. This audit found duplicate registration, not an observed failure to invoke its existing behaviour.

The report distinguishes corrections to audit inputs, expected safety refusals, missing prerequisites, genuinely failing behaviour and untested acceptance. It should be used as a measured test record and a work list, not a claim that every implemented name provides commercial parity or perfect results.

## Reproduction and evidence

Evidence root: `.work-checks/full-feature-audit-2026-10-19/`.

- `runtime-audit.py`: fixture creation, mixed import, real RAW rendering and 31 processor invariants.
- `runtime-workflows.py`: development probes, local and named masks, real portrait, status surfaces and exports.
- `runtime-extended.py` and `runtime-final.py`: corrected valid UI units, B&W, settings sync, prerequisites, reference strengths, stale protection, real RAW enhancement and cache checks.
- `gui-audit.py`, `gui-results.json` and `gui-*.png`: navigation, visible controls and interaction evidence. The successful resumed filmstrip/slider interaction is recorded in `gui-results.json`.
- `runtime-calls.jsonl`: individual native IPC arguments, results and elapsed times. `runtime-results.json` retains complete returned recipe/report evidence; it can be large.
- `input-hashes-before.json`, `input-hashes-final.json`: source preservation evidence.
- `real-portrait-before.png`, `real-portrait-after.png`, `real-portrait-operation-audit.json`, `raw-portrait-operation-audit.json`: actual portrait artifacts and stored operation metadata.
- `native-workspace-tests.log`, `ui-tests.json`, `ui-timeout-recheck.json`, `ui-tests-serial.json`, `ui-build.log`: automated test/build evidence.
- `restart-verification.json`, `before-restart.json`: persistence evidence.

Full machine-readable result: [AURA-full-feature-test-report-2026-10-19.json](AURA-full-feature-test-report-2026-10-19.json). Complete test ledgers: [native tests](AURA-full-feature-test-report-2026-10-19-native-tests.csv), [UI tests](AURA-full-feature-test-report-2026-10-19-ui-tests.csv), [registered commands](AURA-full-feature-test-report-2026-10-19-commands.csv). [Function-by-function details](AURA-full-feature-test-report-2026-10-19-function-details.md) include native signatures, source locations, binding documentation, invocation examples and explicit remaining coverage. The HTML companion contains searchable detailed runtime, test and command records.

## Every desktop case: outcome ledger


| Step | Case | Raw outcome | Assessment |
|---|---|---|---|
| 1 | Desktop startup and native renderer capability | PASS | PASS |
| 2 | Multiple-root import with duplicate and corrupt inputs | FAIL | FAIL |
| 3 | Individual-root import workaround | PASS | PASS |
| 4 | Native full-resolution raster rendering | PASS | PASS |
| 5 | Native full-resolution CR3 rendering | PASS | PASS |
| 6 | Native retouch processor: acne_clear | PASS | PASS |
| 7 | Native retouch processor: heal | PASS | PASS |
| 8 | Native retouch processor: patch_heal | PASS | PASS |
| 9 | Native retouch processor: frequency_heal | PASS | PASS |
| 10 | Native retouch processor: clone | PASS | PASS |
| 11 | Native retouch processor: auto_blemish | PASS | PASS |
| 12 | Native retouch processor: frequency | PASS | PASS |
| 13 | Native retouch processor: texture_graft | PASS | PASS |
| 14 | Native retouch processor: skin_smooth | PASS | PASS |
| 15 | Native retouch processor: skin_uniformity | PASS | PASS |
| 16 | Native retouch processor: portrait_dodge_burn | PASS | PASS |
| 17 | Native retouch processor: micro_dodge_burn | PASS | PASS |
| 18 | Native retouch processor: dodge | PASS | PASS |
| 19 | Native retouch processor: burn | PASS | PASS |
| 20 | Native retouch processor: skin_color | PASS | PASS |
| 21 | Native retouch processor: color_match | PASS | PASS |
| 22 | Native retouch processor: mattify | PASS | PASS |
| 23 | Native retouch processor: under_eye | PASS | PASS |
| 24 | Native retouch processor: wrinkle | PASS | PASS |
| 25 | Native retouch processor: stretch_marks | PASS | PASS |
| 26 | Native retouch processor: teeth | PASS | PASS |
| 27 | Native retouch processor: eye_clean | PASS | PASS |
| 28 | Native retouch processor: eye_detail | PASS | PASS |
| 29 | Native retouch processor: red_eye | PASS | PASS |
| 30 | Native retouch processor: fabric | PASS | PASS |
| 31 | Native retouch processor: backdrop | PASS | PASS |
| 32 | Native retouch processor: glare | PASS | PASS |
| 33 | Native retouch processor: makeup | PASS | PASS |
| 34 | Native retouch processor: colorize | PASS | PASS |
| 35 | Native retouch processor: background_color | PASS | PASS |
| 36 | Native retouch processor: reshape | PASS | PASS |
| 37 | Final activity status | PASS | PASS |
| 38 | Input originals preserved | PASS | PASS |
| 39 | Reset original removes saved native retouch | PASS | PASS |
| 40 | Develop control: global.temperature | PASS | PASS |
| 41 | Develop control: global.tint | PASS | PASS |
| 42 | Develop control: global.exposure | PASS | PASS |
| 43 | Develop control: global.contrast | PASS | PASS |
| 44 | Develop control: global.highlights | PASS | PASS |
| 45 | Develop control: global.shadows | PASS | PASS |
| 46 | Develop control: global.whites | PASS | PASS |
| 47 | Develop control: global.blacks | PASS | PASS |
| 48 | Develop control: global.texture | PASS | PASS |
| 49 | Develop control: global.clarity | PASS | PASS |
| 50 | Develop control: global.dehaze | PASS | PASS |
| 51 | Develop control: global.vibrance | PASS | PASS |
| 52 | Develop control: global.saturation | PASS | PASS |
| 53 | Develop control: global.parametric.highlights | PASS | PASS |
| 54 | Develop control: global.parametric.lights | PASS | PASS |
| 55 | Develop control: global.parametric.darks | PASS | PASS |
| 56 | Develop control: global.parametric.shadows | PASS | PASS |
| 57 | Develop control: global.parametric.shadow_split | PASS | PASS |
| 58 | Develop control: global.parametric.midtone_split | PASS | PASS |
| 59 | Develop control: global.parametric.highlight_split | PASS | PASS |
| 60 | Develop control: global.sharpen.amount | FAIL | Expected input refusal; valid UI-unit probe subsequently passed |
| 61 | Develop control: global.sharpen.radius | PASS | PASS |
| 62 | Develop control: global.sharpen.detail | PASS | PASS |
| 63 | Develop control: global.sharpen.masking | PASS | PASS |
| 64 | Develop control: global.noise.luminance | PASS | PASS |
| 65 | Develop control: global.noise.detail | PASS | PASS |
| 66 | Develop control: global.noise.colour | PASS | PASS |
| 67 | Develop control: global.effects.vignette.amount | PASS | PASS |
| 68 | Develop control: global.effects.vignette.midpoint | PASS | PASS |
| 69 | Develop control: global.effects.vignette.roundness | PASS | PASS |
| 70 | Develop control: global.effects.vignette.feather | PASS | PASS |
| 71 | Develop control: global.effects.vignette.highlights | PASS | PASS |
| 72 | Develop control: global.effects.grain.amount | PASS | PASS |
| 73 | Develop control: global.effects.grain.size | PASS | PASS |
| 74 | Develop control: global.effects.grain.roughness | PASS | PASS |
| 75 | Develop control: global.calibration.shadows_tint | PASS | PASS |
| 76 | Develop control: global.calibration.red_hue | PASS | PASS |
| 77 | Develop control: global.calibration.red_saturation | PASS | PASS |
| 78 | Develop control: global.calibration.green_hue | PASS | PASS |
| 79 | Develop control: global.calibration.green_saturation | PASS | PASS |
| 80 | Develop control: global.calibration.blue_hue | PASS | PASS |
| 81 | Develop control: global.calibration.blue_saturation | PASS | PASS |
| 82 | Develop control: global.colour_grade.blending | PASS | PASS |
| 83 | Develop control: global.colour_grade.balance | PASS | PASS |
| 84 | Develop control: lens.vignette | PASS | PASS |
| 85 | Develop control: geometry.rotate | PASS | PASS |
| 86 | Develop control: global.hsl.red.h | PASS | PASS |
| 87 | Develop control: global.hsl.red.s | PASS | PASS |
| 88 | Develop control: global.hsl.red.l | PASS | PASS |
| 89 | Develop control: global.hsl.orange.h | PASS | PASS |
| 90 | Develop control: global.hsl.orange.s | PASS | PASS |
| 91 | Develop control: global.hsl.orange.l | PASS | PASS |
| 92 | Develop control: global.hsl.yellow.h | PASS | PASS |
| 93 | Develop control: global.hsl.yellow.s | PASS | PASS |
| 94 | Develop control: global.hsl.yellow.l | PASS | PASS |
| 95 | Develop control: global.hsl.green.h | PASS | PASS |
| 96 | Develop control: global.hsl.green.s | PASS | PASS |
| 97 | Develop control: global.hsl.green.l | PASS | PASS |
| 98 | Develop control: global.hsl.aqua.h | PASS | PASS |
| 99 | Develop control: global.hsl.aqua.s | PASS | PASS |
| 100 | Develop control: global.hsl.aqua.l | PASS | PASS |
| 101 | Develop control: global.hsl.blue.h | PASS | PASS |
| 102 | Develop control: global.hsl.blue.s | PASS | PASS |
| 103 | Develop control: global.hsl.blue.l | PASS | PASS |
| 104 | Develop control: global.hsl.purple.h | PASS | PASS |
| 105 | Develop control: global.hsl.purple.s | PASS | PASS |
| 106 | Develop control: global.hsl.purple.l | PASS | PASS |
| 107 | Develop control: global.hsl.magenta.h | PASS | PASS |
| 108 | Develop control: global.hsl.magenta.s | PASS | PASS |
| 109 | Develop control: global.hsl.magenta.l | PASS | PASS |
| 110 | Develop control: global.colour_grade.shadows.hue | PASS | PASS |
| 111 | Develop control: global.colour_grade.shadows.saturation | PASS | PASS |
| 112 | Develop control: global.colour_grade.shadows.luminance | PASS | PASS |
| 113 | Develop control: global.colour_grade.midtones.hue | PASS | PASS |
| 114 | Develop control: global.colour_grade.midtones.saturation | PASS | PASS |
| 115 | Develop control: global.colour_grade.midtones.luminance | PASS | PASS |
| 116 | Develop control: global.colour_grade.highlights.hue | PASS | PASS |
| 117 | Develop control: global.colour_grade.highlights.saturation | PASS | PASS |
| 118 | Develop control: global.colour_grade.highlights.luminance | PASS | PASS |
| 119 | Develop control: global.colour_grade.global.hue | PASS | PASS |
| 120 | Develop control: global.colour_grade.global.saturation | PASS | PASS |
| 121 | Develop control: global.colour_grade.global.luminance | PASS | PASS |
| 122 | Develop control: lens.distortion | PASS | PASS |
| 123 | Develop control: lens.ca | PASS | PASS |
| 124 | Develop control: geometry.crop | PASS | PASS |
| 125 | Develop control: global.curve.points | FAIL | Expected input refusal; valid UI-unit probe subsequently passed |
| 126 | Develop control: global.channel_curves.red.points | FAIL | Expected input refusal; valid UI-unit probe subsequently passed |
| 127 | Develop control: global.channel_curves.green.points | FAIL | Expected input refusal; valid UI-unit probe subsequently passed |
| 128 | Develop control: global.channel_curves.blue.points | FAIL | Expected input refusal; valid UI-unit probe subsequently passed |
| 129 | Snapshot restoration recovers identical rendered pixels | PASS | PASS |
| 130 | Output colour space: srgb | PASS | PASS |
| 131 | Output colour space: adobe_rgb | PASS | PASS |
| 132 | Output colour space: display_p3 | PASS | PASS |
| 133 | Local mask: linear | PASS | PASS |
| 134 | Local mask: radial | PASS | PASS |
| 135 | Local mask: brush | PASS | PASS |
| 136 | Local mask: luminance | PASS | PASS |
| 137 | Real portrait original render | PASS | PASS |
| 138 | Automatic enhancement of a real two-person portrait | PASS | PASS |
| 139 | Real portrait native edited render | PASS | PASS |
| 140 | Image selection: subject | PASS | PASS |
| 141 | Image selection: background | PASS | PASS |
| 142 | Image selection: sky | PASS | PASS |
| 143 | Image selection: face_skin | PASS | PASS |
| 144 | Image selection: body_skin | PASS | PASS |
| 145 | Image selection: hair | PASS | PASS |
| 146 | Image selection: clothes | PASS | PASS |
| 147 | Image selection: eyes | PASS | PASS |
| 148 | Image selection: iris | PASS | PASS |
| 149 | Image selection: sclera | PASS | PASS |
| 150 | Image selection: eyebrows | PASS | PASS |
| 151 | Image selection: lips | PASS | PASS |
| 152 | Image selection: teeth | PASS | PASS |
| 153 | Image selection: facial_hair | PASS | PASS |
| 154 | Feature status: activity_status | PASS | PASS |
| 155 | Feature status: licence_status | PASS | PASS |
| 156 | Feature status: composition_status | PASS | PASS |
| 157 | Feature status: cull_status | PASS | PASS |
| 158 | Feature status: ledger_status | PASS | PASS |
| 159 | Feature status: tone_status | PASS | PASS |
| 160 | Feature status: colour_status | PASS | PASS |
| 161 | Feature status: look_status | PASS | PASS |
| 162 | Feature status: style_status | PASS | PASS |
| 163 | Feature status: local_status | PASS | PASS |
| 164 | Feature status: retouch_status | PASS | PASS |
| 165 | Feature status: micro_status | PASS | PASS |
| 166 | Feature status: restore_status | PASS | PASS |
| 167 | Feature status: geometry_status | PASS | PASS |
| 168 | Feature status: cleanup_status | PASS | PASS |
| 169 | Feature status: gallery_status | PASS | PASS |
| 170 | Feature status: camera_status | PASS | PASS |
| 171 | Feature status: qc_status | PASS | PASS |
| 172 | Feature status: develop_status | PASS | PASS |
| 173 | Feature status: emotion_status | PASS | PASS |
| 174 | Feature status: index_status | PASS | PASS |
| 175 | Feature status: integrity_status | PASS | PASS |
| 176 | Feature status: mask_status | PASS | PASS |
| 177 | Feature status: moment_status | PASS | PASS |
| 178 | Feature status: people_status | PASS | PASS |
| 179 | Feature status: story_status | PASS | PASS |
| 180 | Feature status: curate_status | PASS | PASS |
| 181 | Feature status: export_status | PASS | PASS |
| 182 | Feature status: delivery_status | PASS | PASS |
| 183 | Feature status: learn_status | PASS | PASS |
| 184 | Adaptive edit-profile catalogue | PASS | PASS |
| 185 | Export naming preview | PASS | PASS |
| 186 | JPEG, PNG and 16-bit TIFF verified export | PASS | PASS |
| 187 | Export manifest | PASS | PASS |
| 188 | Independent exported-file read-back | PASS | PASS |
| 189 | Correct UI-unit probe: global.sharpen.amount | PASS | PASS |
| 190 | Correct UI-unit probe: global.curve.points | PASS | PASS |
| 191 | Correct UI-unit probe: global.channel_curves.red.points | PASS | PASS |
| 192 | Correct UI-unit probe: global.channel_curves.green.points | PASS | PASS |
| 193 | Correct UI-unit probe: global.channel_curves.blue.points | PASS | PASS |
| 194 | Monochrome mixer: red | PASS | PASS |
| 195 | Monochrome mixer: orange | PASS | PASS |
| 196 | Monochrome mixer: yellow | PASS | PASS |
| 197 | Monochrome mixer: green | PASS | PASS |
| 198 | Monochrome mixer: aqua | PASS | PASS |
| 199 | Monochrome mixer: blue | PASS | PASS |
| 200 | Monochrome mixer: purple | PASS | PASS |
| 201 | Monochrome mixer: magenta | PASS | PASS |
| 202 | Corrupt image decode refusal | PASS | PASS |
| 203 | Problems surfaced after a corrupt decode | PASS | PASS |
| 204 | Selected exposure settings sync | FAIL | Expected input refusal; valid UI-unit probe subsequently passed |
| 205 | Adaptive profile application | PASS | PASS |
| 206 | Collection culling pass | PASS | PASS |
| 207 | Gallery size control | FAIL | Missing analysis prerequisite; subsequent analysed test passed |
| 208 | Manual cull override: keep | FAIL | Missing analysis prerequisite; subsequent analysed test passed |
| 209 | Manual cull override: reject | FAIL | Missing analysis prerequisite; subsequent analysed test passed |
| 210 | Manual cull override: clear | FAIL | Missing analysis prerequisite; subsequent analysed test passed |
| 211 | Camera matching pass with limited input cameras | PASS | PASS |
| 212 | Gallery consistency pass | PASS | PASS |
| 213 | Inspect-only quality-control pass | PASS | PASS |
| 214 | Quality-control Markdown report | PASS | PASS |
| 215 | Edited image colour-reference import | PASS | PASS |
| 216 | Portrait colour-reference fitting | PASS | PASS |
| 217 | Colour-reference strength: amount | PASS | PASS |
| 218 | Colour-reference strength: tone | PASS | PASS |
| 219 | Colour-reference strength: color | PASS | PASS |
| 220 | Colour-reference strength: whiteBalance | PASS | PASS |
| 221 | Remove matched colour treatment | PASS | PASS |
| 222 | Saved reference library read-back | PASS | PASS |
| 223 | External services, billing and physical hardware | BLOCKED | BLOCKED |
| 224 | Final desktop activity | PASS | PASS |
| 225 | Sync valid settings group: tone | PASS | PASS |
| 226 | Sync valid settings group: white_balance | PASS | PASS |
| 227 | Sync valid settings group: curves | PASS | PASS |
| 228 | Sync valid settings group: color | PASS | PASS |
| 229 | Sync valid settings group: detail | PASS | PASS |
| 230 | Sync valid settings group: effects | PASS | PASS |
| 231 | Sync valid settings group: calibration | PASS | PASS |
| 232 | Sync valid settings group: lens | PASS | PASS |
| 233 | Sync valid settings group: geometry | PASS | PASS |
| 234 | Runtime availability: list_models | PASS | PASS |
| 235 | Runtime availability: infer_stats | PASS | PASS |
| 236 | Runtime availability: hardware_plan | PASS | PASS |
| 237 | Runtime availability: mask_kinds | PASS | PASS |
| 238 | Local analysis stage: embed_project | PASS | PASS |
| 239 | Local analysis stage: group_people | PASS | PASS |
| 240 | Local analysis stage: classify_scenes | PASS | PASS |
| 241 | Local analysis stage: group_moments | PASS | PASS |
| 242 | Local analysis stage: analyse_integrity | PASS | PASS |
| 243 | Local analysis stage: score_emotion | PASS | PASS |
| 244 | Local analysis stage: analyse_composition | PASS | PASS |
| 245 | Build similarity index | PASS | PASS |
| 246 | Cull after local analysis | PASS | PASS |
| 247 | Gallery resize after local analysis | PASS | PASS |
| 248 | Cull override after analysis: keep | PASS | PASS |
| 249 | Cull override after analysis: reject | PASS | PASS |
| 250 | Cull override after analysis: clear | PASS | PASS |
| 251 | Stale colour-match session preserves newer manual edits | PASS | PASS |
| 252 | Two-person automatic portrait respects advanced-retouch restriction | PASS | PASS |
| 253 | Automatic single-person Canon RAW enhancement | PASS | PASS |
| 254 | Edited Canon RAW native-resolution render | PASS | PASS |
| 255 | Audit-project preview cache budget and purge | PASS | PASS |
| 256 | Final input checksum verification | PASS | PASS |
| 257 | Final idle runtime | PASS | PASS |


## Every registered command: coverage ledger

The harness counts include successful responses and refused requests; neither alone proves photographic quality. A zero means no direct invocation was recorded by that harness. UI-ring response counts are separate, finite and incomplete for bypassing bindings. Automated tests and source bindings are in the companion data.


| Command | Registrations | Harness calls | Harness OK / refused | GUI-ring OK | Coverage |
|---|---|---|---|---|---|
| ui_audit | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| automatic_start | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| photo_analysis | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| photo_auto_edit | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| one_click_finish | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| one_click_status | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| one_click_cancel | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| activity_status | 1 | 6 | 6 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| analyse_portrait | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| portrait_retouch | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| set_portrait_retouch | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| auto_portrait_retouch | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| sync_settings | 1 | 10 | 9 / 1 | 0 | Harness-invoked: inspect case outcome and scope |
| pick_white_balance | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| native_retouch_edit | 1 | 159 | 159 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| native_retouch_blemish_review | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| native_retouch_selection_preview | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| native_retouch_saved_selection | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| local_masks | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| create_local_mask | 1 | 18 | 18 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| save_local_masks | 1 | 13 | 13 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| local_mask_coverage | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| native_retouch_preview | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| photo_original | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| live_preview | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| native_retouch_draft_preview | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| list_edit_profiles | 1 | 2 | 2 / 0 | 10 | Harness-invoked: inspect case outcome and scope |
| learn_lightroom_style | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| licence_status | 1 | 1 | 1 / 0 | 4 | Harness-invoked: inspect case outcome and scope |
| activate_licence | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| deactivate_licence | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| refresh_licence | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| delete_personal_style | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| apply_edit_profile | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| preview_edit_profile | 1 | 0 | 0 / 0 | 48 | GUI-wrapper invocation observed; inspect panel scope |
| analyse_reference_style | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| apply_reference_style | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| match_paired_reference | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| import_color_reference | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| list_color_references | 1 | 2 | 2 / 0 | 12 | Harness-invoked: inspect case outcome and scope |
| prepare_color_match | 1 | 2 | 2 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| apply_color_match | 1 | 6 | 5 / 1 | 0 | Harness-invoked: inspect case outcome and scope |
| color_match_status | 1 | 0 | 0 / 0 | 12 | GUI-wrapper invocation observed; inspect panel scope |
| enhance_photo | 1 | 2 | 2 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| enhance_portrait | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| auto_retouch | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| auto_advanced_retouch | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| create_project | 1 | 2 | 2 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| list_projects | 1 | 0 | 0 / 0 | 10 | GUI-wrapper invocation observed; inspect panel scope |
| start_ingest | 1 | 7 | 7 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| cancel_job | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| ingest_progress | 1 | 14 | 14 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| list_images | 1 | 3 | 3 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| set_camera_label | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| list_problems | 1 | 4 | 4 / 0 | 2 | Harness-invoked: inspect case outcome and scope |
| composition_status | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| image_composition | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| flagged_composition | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| dismiss_composition_flag | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| analyse_composition | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| cull_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| gallery | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| image_decision | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| cull_project | 1 | 2 | 2 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| resize_gallery | 1 | 2 | 1 / 1 | 0 | Harness-invoked: inspect case outcome and scope |
| set_cull_mode | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| override_decision | 1 | 6 | 3 / 3 | 0 | Harness-invoked: inspect case outcome and scope |
| explain_image | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| decision_history | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| decision_by_id | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| ledger_status | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| review_queue | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| record_decisions | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| export_support_bundle | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| compact_ledger | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| tone_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| image_tone | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| tone_review_queue | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| reference_frames | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| accept_tone | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_tone_override | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| estimate_tone | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| colour_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| image_colour | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| colour_review_queue | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| accept_colour | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_colour_override | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| select_colour_variant | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| estimate_colour | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| look_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| list_looks | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| parse_reference | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| look_buckets | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| look_match_report | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| measure_look | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| select_look | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_look_strength | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| rename_look | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| forget_look | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| style_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| list_profiles | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| profile_report | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| profile_pairs | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| scan_archive | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| train_profile | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| adopt_profile | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| compare_profiles | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| export_profile | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| import_profile | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_project_profile | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| local_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| image_local | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| local_review_queue | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| accept_local | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_local_strength | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| sculpt_local | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| retouch_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| image_retouch | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| protected_features | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| retouch_review_queue | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| accept_retouch | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_retouch | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_protection | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| retouch_pass | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| micro_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| image_micro | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| micro_composites | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| micro_review_queue | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| micro_matrix | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| set_micro_matrix | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| accept_micro | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| micro_pass | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| micro_reason_codes | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| restore_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| image_restore | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| restore_identity_refusals | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| restore_review_queue | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| accept_restore | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_restore_override | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| restore_pass | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| restore_reason_codes | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| geometry_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| image_geometry | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| geometry_review_queue | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| accept_geometry | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| cleanup_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| image_cleanup | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| cleanup_blocked | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| cleanup_disclosures | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| cleanup_pass | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| decide_cleanup | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| disable_cleanup | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| manual_remove | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| cleanup_reason_codes | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| gallery_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| gallery_nodes | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| gallery_node_strip | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| image_gallery | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| gallery_outliers | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| gallery_pass | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| pin_gallery_anchor | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_gallery_override | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| disable_gallery | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| gallery_reason_codes | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| camera_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| camera_transforms | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| camera_fingerprints | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| camera_reports | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| camera_pairs | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| camera_shooter_bias | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| camera_pass | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| set_camera_reference | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| disable_camera | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_camera_override | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| camera_reason_codes | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| qc_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| qc_report | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| qc_report_markdown | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| qc_queue | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| qc_queue_grouped | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| qc_tickets | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| qc_rounds | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| qc_run | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| qc_decide | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| qc_decide_bulk | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_framing | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| plan_geometry | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| analyse_integrity | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| build_index | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| cancel_previews | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| ai_setup_status | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| check_ai_key | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| classify_scenes | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| clear_ai_key | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| cloud_cache_stats | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| cloud_calls | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| cloud_spend | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| cloud_status | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| internal_ai_status | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| internal_ai_feedback | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| develop_status | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| dismiss_flag | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| edit_mask | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| embed_project | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| emotion_status | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| ensure_masks | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| erase_biometrics | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| find_similar | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| flagged_images | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| get_preview | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| group_moments | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| group_people | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| hardware_plan | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| history_step | 1 | 160 | 160 / 0 | 2 | Harness-invoked: inspect case outcome and scope |
| identity_cover | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| image_subjects | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| identity_timelines | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| image_descriptors | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| image_emotion | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| image_history | 1 | 0 | 0 / 0 | 12 | GUI-wrapper invocation observed; inspect panel scope |
| image_integrity | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| image_masks | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| image_recipe | 1 | 4 | 4 / 0 | 12 | Harness-invoked: inspect case outcome and scope |
| image_scene | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| index_status | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| infer_stats | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| integrity_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| list_identities | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| list_models | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| list_moments | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| lock_moment | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| mask_allowance | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| mask_kinds | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| mask_overlay | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| mask_status | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| merge_chapters | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| merge_identities | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| merge_moments | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| moment_duplicates | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| moment_of_image | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| moment_peak | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| moment_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| within_moment | 2 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| move_chapter_boundary | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| people_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| prefer_frame | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| prefetch_previews | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| preview_problems | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| preview_stats | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| purge_cache | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| purge_cloud_cache | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| ranked_by_emotion | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| reactions_of | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| recheck_hardware | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| regenerate_mask | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| rename_identity | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| render_caps | 1 | 2 | 2 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| render_image | 1 | 448 | 447 / 1 | 0 | Harness-invoked: inspect case outcome and scope |
| scan_faces | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| scene_profiles | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| score_emotion | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| segment_story | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| list_ai_providers | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| open_ai_provider_page | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| save_ai_setup | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_ai_key | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| skip_ai_setup | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_cache_budget | 1 | 2 | 2 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| set_chapter | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_cloud_budget | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_cloud_privacy | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_execution_provider | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_identity_importance | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_identity_role | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_keep_hint | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_moment_peak | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| set_param | 1 | 107 | 102 / 5 | 2 | Harness-invoked: inspect case outcome and scope |
| snapshot | 1 | 97 | 97 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| split_chapter | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| split_identity | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| split_moment | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| story_outline | 1 | 0 | 0 / 0 | 2 | GUI-wrapper invocation observed; inspect panel scope |
| story_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| undo_moment_edit | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| warmup_models | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| autopilot_status | 1 | 0 | 0 / 0 | 2 | GUI-wrapper invocation observed; inspect panel scope |
| autopilot_preflight | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| autopilot_start | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| autopilot_progress | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| autopilot_cancel | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| autopilot_stages | 1 | 0 | 0 / 0 | 2 | GUI-wrapper invocation observed; inspect panel scope |
| autopilot_summary | 1 | 0 | 0 / 0 | 2 | GUI-wrapper invocation observed; inspect panel scope |
| autopilot_events | 1 | 0 | 0 / 0 | 2 | GUI-wrapper invocation observed; inspect panel scope |
| autopilot_set_settings | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| curate_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| curate_project | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| curate_bw | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| curate_heroes | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| curate_album | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| curate_spread | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| curate_social | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| curate_teaser | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| curate_set_order | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| curate_decide | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| curate_export | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| export_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| export_presets | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| export_preview_names | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| export_run | 1 | 1 | 1 / 0 | 0 | Harness-invoked: inspect case outcome and scope |
| export_run_watermarked | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| export_files | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| export_manifest | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| delivery_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| delivery_providers | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| delivery_backup | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| delivery_upload | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| delivery_items | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| learn_status | 1 | 1 | 1 / 0 | 1 | Harness-invoked: inspect case outcome and scope |
| learn_buckets | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| learn_compare | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| learn_adopt | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| learn_roll_back | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| learn_consent | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |
| learn_set_consent | 1 | 0 | 0 / 0 | 0 | Registered; no invocation captured by the harness or UI ring log |
| diagnostics_report | 1 | 0 | 0 / 0 | 1 | GUI-wrapper invocation observed; inspect panel scope |


## Explicitly ignored native cases

These were not executed by the default workspace run. Some need photographs, paired datasets, hardware, network credentials or deliberate timing runs. Debug/experimental cases and a frozen-document re-blessing case are also ignored; running them indiscriminately would not be a valid product audit. A missing reason is labelled rather than inferred. Direct desktop scenarios above provide additional real-photo evidence but do not change these tests to passed.

| Package | Test | Declared reason |
|---|---|---|
| aura-app | personal_style::tests::learns_from_a_real_catalogue | needs AURA_LRCAT: a Lightroom Classic catalogue; prints what was learned |
| aura-app | portrait_features::deep_blemish::diagnostics::inspect_real_photo_spot_proposals | needs AURA_SPOT_INPUT JSON and matching .rgb; prints native proposals for review |
| aura-app | every_photograph_gets_its_own_settings | needs AURA_ADAPTIVE_PHOTOS; prints what each photograph was given |
| aura-app | every_stage_runs_in_order_on_real_photographs | requires AURA_ADVANCED_PHOTOS pointing at real portraits |
| aura-app | retouches_real_photographs | needs AURA_SKIN_PHOTOS; writes before/after images for a person to look at |
| aura-app | where_the_time_goes | needs AURA_PROFILE_DIR, a folder of photographs |
| aura-app | isolate_chin_neck_repairs | requires local chin/neck review inputs |
| aura-app | native_raw_reference_match_does_not_touch_originals | requires AURA_COLOR_MATCH_RAW and AURA_COLOR_MATCH_REFERENCE; isolated native RAW verification |
| aura-app | the_full_quality_preview_is_the_original_size_and_cached | requires AURA_PREVIEW_PHOTO pointing at a real photograph |
| aura-app | ai_selections_on_real_photographs | needs AURA_MASK_PHOTOS: photographs separated by ';'; writes overlays to AURA_MASK_OUT |
| aura-app | manual_skin_selection_survives_repeated_native_retouch | requires AURA_SKIN_PORTRAIT pointing at a real portrait |
| aura-app | native_raw_finish_preserves_originals_and_obeys_portrait_policy | requires AURA_POLISHED_PHOTOS and real photographs |
| aura-app | inspect_full_resolution_native_contours_without_claiming_visual_parity | requires aligned native render pairs in AURA_CONTOUR_REVIEW_ROOT |
| aura-app | learn_a_profile_from_raw_and_retoucher_pairs | needs downloaded FiveK pairs; see the module documentation |
| aura-app | print_dng_meta | diagnostic: prints what the decoder read from one DNG |
| aura-app | edits_reference_pairs | needs AURA_PAIR_DIR; writes renders for a person to compare |
| aura-app | render_saved_recipes_for_review | requires AURA_SAVED_REVIEW_ROOT and recorded native recipes |
| aura-app | scenes_are_read_before_they_are_corrected | needs AURA_SCENE_PHOTOS; prints what each photograph was given |
| aura-app | adaptive_versus_constant | experiment |
| aura-cloud | the_configured_provider_answers_a_real_auto_edit_task | live network; needs AURA_LIVE_AI_KEY |
| aura-portrait | dump_overlays_for_ui_preview | writes overlay planes for a UI preview to AURA_PORTRAIT_EVAL_OUT |
| aura-portrait | dump_painted_fixtures | writes painted fixtures to AURA_PORTRAIT_EVAL_OUT |
| aura-portrait | faces_on_colour_frames | needs AURA_PORTRAIT_EVAL_DIR |
| aura-portrait | frontal_cascade_on_grey_frames | needs AURA_PORTRAIT_EVAL_DIR |
| aura-portrait | readings_on_colour_frames | needs AURA_PORTRAIT_EVAL_DIR |
| aura-portrait | regions_on_colour_frames | needs AURA_PORTRAIT_EVAL_DIR |
| aura-portrait | timing_breakdown | needs AURA_PORTRAIT_EVAL_DIR |
| aura-raw | probe_files | needs AURA_PROBE_FILE or AURA_PROBE_LIST |
| aura-raw | proxy_timing | needs AURA_PROBE_FILE |
| aura-raw | every_file_decodes_exactly_like_the_reference | needs AURA_REF_DIR from scripts/raw-reference-dump.py |
| aura-raw | decode_cost_per_megapixel | generates frames up to 25 MP; run it deliberately, not in CI |
| aura-raw | debug_sony | debug |
| aura-recipe | bless_the_frozen_v1_document | re-blesses the frozen v1 document; run deliberately |
| aura-render | bands::tests::pass_timing | timing, by hand |
| aura-render | retouch_clear::tests::timing | timing, by hand |
| aura-render | brushes_a_real_photograph | needs AURA_BRUSH_PHOTO; writes an image for a person to look at |
| aura-render | a_face_sized_field_takes_seconds_not_minutes | timing: run by hand with --release |
| aura-render | the_explicit_disable_switch_uses_identical_cpu_pixels_and_cache_identity | run with AURA_DISABLE_GPU=1 in a separate process |
| aura-render | benchmark_output_including_all_transfers | requires a real GPU; reports upload, compute and readback together |
| aura-render | enabled_engine_uses_gpu_and_separates_its_cache_from_the_reference | requires a real GPU |
| aura-render | hardware_output_matches_all_spaces_depths_hdr_and_stripe_origins | requires a real GPU; does not accept software adapters |
| aura-render | natural_retouch_before_and_after | needs AURA_PORTRAIT_EVAL_DIR and AURA_PORTRAIT_EVAL_OUT |
| aura-render | time_every_retouch_operation | requires AURA_PROFILE_* pointing at a real photograph and recipe |
| aura-render | finishes_real_photographs | needs AURA_SKIN_PHOTOS; writes images for a person to look at |
| aura-vision | skin::tests::segmentation_timing | timing, by hand |
| aura-vision | segments_real_photographs | needs AURA_SKIN_PHOTOS; writes overlays for a person to look at |
| aura-vision | quarter_turns_preserve_face_and_body_selection | requires AURA_SKIN_RGB=path to packed RGB and AURA_SKIN_SIZE=WxH |
| aura-cloud | crates\aura-cloud\src\contract\cloud.rs - contract::cloud::CloudTask (line 240) | No reason printed; inspect source |


## Native suite ledger

| Package | Target | Passed | Failed | Ignored |
|---|---|---|---|---|
| aura-app | unittests src\lib.rs | 261 | 0 | 2 |
| aura-app | tests\adaptive_retouch_photos.rs | 0 | 0 | 1 |
| aura-app | tests\advanced_retouch_photos.rs | 0 | 0 | 1 |
| aura-app | tests\ai_setup.rs | 11 | 0 | 0 |
| aura-app | tests\auto_retouch_photos.rs | 0 | 0 | 1 |
| aura-app | tests\automatic_portrait.rs | 3 | 0 | 0 |
| aura-app | tests\batch_profile.rs | 0 | 0 | 1 |
| aura-app | tests\chin_neck_review.rs | 0 | 0 | 1 |
| aura-app | tests\color_reference_match.rs | 1 | 0 | 1 |
| aura-app | tests\full_quality_preview.rs | 0 | 0 | 1 |
| aura-app | tests\index_contract.rs | 9 | 0 | 0 |
| aura-app | tests\infer_contract.rs | 6 | 0 | 0 |
| aura-app | tests\ipc_contract.rs | 8 | 0 | 0 |
| aura-app | tests\licensing.rs | 3 | 0 | 0 |
| aura-app | tests\local_masks.rs | 1 | 0 | 1 |
| aura-app | tests\native_skin_workflow.rs | 0 | 0 | 1 |
| aura-app | tests\one_click.rs | 1 | 0 | 0 |
| aura-app | tests\photo_pixels.rs | 3 | 0 | 0 |
| aura-app | tests\photo_workflow.rs | 2 | 0 | 0 |
| aura-app | tests\polished_wedding_photos.rs | 0 | 0 | 1 |
| aura-app | tests\portrait_contour_review.rs | 0 | 0 | 1 |
| aura-app | tests\portrait_exposure.rs | 3 | 0 | 0 |
| aura-app | tests\portrait_workflow.rs | 2 | 0 | 0 |
| aura-app | tests\preview_proxy.rs | 1 | 0 | 0 |
| aura-app | tests\profile_fit.rs | 0 | 0 | 2 |
| aura-app | tests\reference_pairs.rs | 0 | 0 | 1 |
| aura-app | tests\saved_batch_review.rs | 0 | 0 | 1 |
| aura-app | tests\scene_photos.rs | 0 | 0 | 1 |
| aura-app | tests\studio_tools.rs | 2 | 0 | 0 |
| aura-app | tests\style_adaptive.rs | 0 | 0 | 1 |
| aura-brain-gallery | unittests src\lib.rs | 167 | 0 | 0 |
| aura-brain-gallery | ..\..\tests\eval\camera_eval.rs | 17 | 0 | 0 |
| aura-brain-gallery | ..\..\tests\eval\consistency_eval.rs | 10 | 0 | 0 |
| aura-brain-gallery | tests\no_recipe_writes.rs | 6 | 0 | 0 |
| aura-brain-gallery | tests\store_and_pins.rs | 13 | 0 | 0 |
| aura-brain-photo | unittests src\lib.rs | 163 | 0 | 0 |
| aura-brain-photo | tests\calibration.rs | 11 | 0 | 0 |
| aura-brain-photo | ..\..\tests\eval\colour_eval.rs | 27 | 0 | 0 |
| aura-brain-photo | ..\..\tests\eval\composition_eval.rs | 37 | 0 | 0 |
| aura-brain-photo | tests\composition_rules.rs | 1 | 0 | 0 |
| aura-brain-photo | ..\..\tests\eval\integrity_eval.rs | 26 | 0 | 0 |
| aura-brain-photo | ..\..\tests\eval\local_eval.rs | 38 | 0 | 0 |
| aura-brain-photo | tests\no_recipe_writes.rs | 2 | 0 | 0 |
| aura-brain-photo | ..\..\tests\eval\tone_eval.rs | 24 | 0 | 0 |
| aura-brain-photo | tests\tone_intent.rs | 14 | 0 | 0 |
| aura-brain-wedding | unittests src\lib.rs | 0 | 0 | 0 |
| aura-brain-wedding | ..\..\tests\eval\burst_eval.rs | 16 | 0 | 0 |
| aura-brain-wedding | tests\config.rs | 36 | 0 | 0 |
| aura-brain-wedding | ..\..\tests\eval\emotion_eval.rs | 38 | 0 | 0 |
| aura-brain-wedding | tests\moments.rs | 51 | 0 | 0 |
| aura-brain-wedding | ..\..\tests\eval\scene_eval.rs | 18 | 0 | 0 |
| aura-cache | unittests src\lib.rs | 0 | 0 | 0 |
| aura-cache | tests\cache.rs | 18 | 0 | 0 |
| aura-catalog | unittests src\lib.rs | 1 | 0 | 0 |
| aura-catalog | tests\catalog.rs | 12 | 0 | 0 |
| aura-cli | unittests src\main.rs | 0 | 0 | 0 |
| aura-cloud | unittests src\lib.rs | 41 | 0 | 0 |
| aura-cloud | tests\agent.rs | 12 | 0 | 0 |
| aura-cloud | tests\budget.rs | 14 | 0 | 0 |
| aura-cloud | tests\couple_hint.rs | 19 | 0 | 0 |
| aura-cloud | tests\explain_summary.rs | 5 | 0 | 0 |
| aura-cloud | tests\gateway.rs | 17 | 0 | 0 |
| aura-cloud | tests\http.rs | 14 | 0 | 0 |
| aura-cloud | tests\keys.rs | 14 | 0 | 0 |
| aura-cloud | tests\live_provider.rs | 0 | 0 | 1 |
| aura-cloud | tests\privacy.rs | 14 | 0 | 0 |
| aura-cloud | tests\schema.rs | 16 | 0 | 0 |
| aura-core | unittests src\lib.rs | 112 | 0 | 0 |
| aura-core | tests\clock_and_paths.rs | 7 | 0 | 0 |
| aura-core | tests\composition_contract.rs | 8 | 0 | 0 |
| aura-core | tests\consent.rs | 5 | 0 | 0 |
| aura-core | tests\error_registry.rs | 3 | 0 | 0 |
| aura-core | tests\ids.rs | 5 | 0 | 0 |
| aura-core | tests\integrity_contract.rs | 13 | 0 | 0 |
| aura-core | tests\local_contract.rs | 24 | 0 | 0 |
| aura-core | tests\no_upward_deps.rs | 23 | 0 | 0 |
| aura-core | tests\no_upward_deps.rs | 1 | 0 | 0 |
| aura-core | tests\progress.rs | 4 | 0 | 0 |
| aura-core | tests\restore_contract.rs | 27 | 0 | 0 |
| aura-cull | unittests src\lib.rs | 37 | 0 | 0 |
| aura-cull | ..\..\tests\eval\cull_eval.rs | 24 | 0 | 0 |
| aura-curate | unittests src\lib.rs | 136 | 0 | 0 |
| aura-curate | ..\..\tests\eval\curate_eval.rs | 25 | 0 | 0 |
| aura-curate | tests\no_outputs.rs | 9 | 0 | 0 |
| aura-curate | tests\store_round_trip.rs | 12 | 0 | 0 |
| aura-delivery | unittests src\lib.rs | 26 | 0 | 0 |
| aura-delivery | tests\delivery_pass.rs | 8 | 0 | 0 |
| aura-explain | unittests src\lib.rs | 41 | 0 | 0 |
| aura-explain | ..\..\tests\eval\explain_eval.rs | 30 | 0 | 0 |
| aura-export | unittests src\lib.rs | 67 | 0 | 0 |
| aura-export | ..\..\tests\eval\delivery_eval.rs | 21 | 0 | 0 |
| aura-export | tests\export_pass.rs | 12 | 0 | 0 |
| aura-export | tests\watermark.rs | 4 | 0 | 0 |
| aura-generative | unittests src\lib.rs | 117 | 0 | 0 |
| aura-generative | ..\..\tests\eval\cleanup_eval.rs | 26 | 0 | 0 |
| aura-generative | tests\one_choke_point.rs | 6 | 0 | 0 |
| aura-generative | tests\store_and_triggers.rs | 12 | 0 | 0 |
| aura-geometry | unittests src\lib.rs | 85 | 0 | 0 |
| aura-geometry | ..\..\tests\eval\geometry_eval.rs | 23 | 0 | 0 |
| aura-geometry | tests\no_render_calls.rs | 2 | 0 | 0 |
| aura-index | unittests src\lib.rs | 0 | 0 | 0 |
| aura-index | tests\hnsw.rs | 23 | 0 | 0 |
| aura-index | tests\snapshot.rs | 18 | 0 | 0 |
| aura-index | tests\store.rs | 11 | 0 | 0 |
| aura-infer | unittests src\lib.rs | 7 | 0 | 0 |
| aura-infer | tests\hardware.rs | 7 | 0 | 0 |
| aura-infer | tests\onnx_subset.rs | 11 | 0 | 0 |
| aura-infer | tests\parity.rs | 8 | 0 | 0 |
| aura-infer | tests\plan_file.rs | 8 | 0 | 0 |
| aura-infer | tests\precision.rs | 7 | 0 | 0 |
| aura-infer | tests\runtime.rs | 11 | 0 | 0 |
| aura-infer | tests\scheduler.rs | 7 | 0 | 0 |
| aura-infer | tests\wire.rs | 8 | 0 | 0 |
| aura-ingest | unittests src\lib.rs | 0 | 0 | 0 |
| aura-ingest | tests\ingest.rs | 25 | 0 | 0 |
| aura-jobs | unittests src\lib.rs | 115 | 0 | 0 |
| aura-jobs | ..\..\tests\e2e\autopilot_chaos.rs | 12 | 0 | 0 |
| aura-jobs | ..\..\tests\e2e\autopilot_run.rs | 21 | 0 | 0 |
| aura-jobs | tests\jobs.rs | 7 | 0 | 0 |
| aura-jobs | tests\no_decisions.rs | 11 | 0 | 0 |
| aura-learn | unittests src\lib.rs | 17 | 0 | 0 |
| aura-learn | tests\learning_loop.rs | 11 | 0 | 0 |
| aura-learn | tests\no_guarantee_learning.rs | 5 | 0 | 0 |
| aura-licence | unittests src\lib.rs | 5 | 0 | 0 |
| aura-look | unittests src\lib.rs | 0 | 0 | 0 |
| aura-look | ..\..\tests\eval\look_eval.rs | 12 | 0 | 0 |
| aura-look | tests\no_network.rs | 2 | 0 | 0 |
| aura-look | tests\no_recipe_writes.rs | 4 | 0 | 0 |
| aura-look | tests\render_match.rs | 3 | 0 | 0 |
| aura-look | tests\source.rs | 14 | 0 | 0 |
| aura-look | tests\store.rs | 9 | 0 | 0 |
| aura-models | unittests src\lib.rs | 0 | 0 | 0 |
| aura-models | tests\delta.rs | 6 | 0 | 0 |
| aura-models | tests\install_state.rs | 5 | 0 | 0 |
| aura-models | tests\registry.rs | 12 | 0 | 0 |
| aura-models | tests\verify.rs | 8 | 0 | 0 |
| aura-people | unittests src\lib.rs | 0 | 0 | 0 |
| aura-people | ..\..\tests\eval\identity_eval.rs | 9 | 0 | 0 |
| aura-people | tests\people.rs | 14 | 0 | 0 |
| aura-people | tests\privacy.rs | 21 | 0 | 0 |
| aura-perf | unittests src\lib.rs | 0 | 0 | 0 |
| aura-perf | tests\autopilot_budgets.rs | 3 | 0 | 0 |
| aura-perf | tests\budgets.rs | 11 | 0 | 0 |
| aura-perf | tests\camera_budgets.rs | 4 | 0 | 0 |
| aura-perf | tests\cleanup_budgets.rs | 6 | 0 | 0 |
| aura-perf | tests\cloud_budgets.rs | 5 | 0 | 0 |
| aura-perf | tests\composition_budgets.rs | 2 | 0 | 0 |
| aura-perf | tests\cull_budgets.rs | 2 | 0 | 0 |
| aura-perf | tests\curate_budgets.rs | 5 | 0 | 0 |
| aura-perf | tests\delivery_budgets.rs | 5 | 0 | 0 |
| aura-perf | tests\develop_budgets.rs | 5 | 0 | 0 |
| aura-perf | tests\emotion_budgets.rs | 4 | 0 | 0 |
| aura-perf | tests\gallery_budgets.rs | 4 | 0 | 0 |
| aura-perf | tests\geometry_budgets.rs | 4 | 0 | 0 |
| aura-perf | tests\index_budgets.rs | 7 | 0 | 0 |
| aura-perf | tests\infer_budgets.rs | 7 | 0 | 0 |
| aura-perf | tests\integrity_budgets.rs | 3 | 0 | 0 |
| aura-perf | tests\ledger_budgets.rs | 4 | 0 | 0 |
| aura-perf | tests\local_budgets.rs | 2 | 0 | 0 |
| aura-perf | tests\micro_budgets.rs | 3 | 0 | 0 |
| aura-perf | tests\moment_budgets.rs | 4 | 0 | 0 |
| aura-perf | tests\people_budgets.rs | 4 | 0 | 0 |
| aura-perf | tests\preview_budgets.rs | 6 | 0 | 0 |
| aura-perf | tests\qc_budgets.rs | 6 | 0 | 0 |
| aura-perf | tests\retouch_budgets.rs | 2 | 0 | 0 |
| aura-perf | tests\scene_budgets.rs | 6 | 0 | 0 |
| aura-perf | tests\tone_budgets.rs | 2 | 0 | 0 |
| aura-portrait | unittests src\lib.rs | 37 | 0 | 0 |
| aura-portrait | tests\local_eval.rs | 0 | 0 | 7 |
| aura-preview | unittests src\lib.rs | 0 | 0 | 0 |
| aura-preview | tests\priority.rs | 11 | 0 | 0 |
| aura-preview | tests\service.rs | 10 | 0 | 0 |
| aura-qc | unittests src\lib.rs | 227 | 0 | 0 |
| aura-qc | tests\no_pixel_ops.rs | 7 | 0 | 0 |
| aura-qc | ..\..\tests\eval\qc_eval.rs | 12 | 0 | 0 |
| aura-raw | unittests src\lib.rs | 39 | 0 | 0 |
| aura-raw | tests\codecs.rs | 15 | 0 | 0 |
| aura-raw | tests\colour_maths.rs | 18 | 0 | 0 |
| aura-raw | tests\containers.rs | 18 | 0 | 0 |
| aura-raw | tests\full_decode.rs | 1 | 0 | 0 |
| aura-raw | tests\fuzz_decode.rs | 6 | 0 | 0 |
| aura-raw | tests\png_photos.rs | 3 | 0 | 0 |
| aura-raw | tests\probe_file.rs | 0 | 0 | 1 |
| aura-raw | tests\proxy_timing.rs | 0 | 0 | 1 |
| aura-raw | tests\reference_decode.rs | 0 | 0 | 1 |
| aura-raw | tests\scaling_probe.rs | 0 | 0 | 1 |
| aura-raw | tests\tiers.rs | 16 | 0 | 0 |
| aura-raw | tests\zz_debug.rs | 0 | 0 | 1 |
| aura-recipe | unittests src\lib.rs | 71 | 0 | 0 |
| aura-recipe | tests\migration.rs | 6 | 0 | 1 |
| aura-render | unittests src\lib.rs | 224 | 0 | 2 |
| aura-render | tests\acne_brush_photo.rs | 0 | 0 | 1 |
| aura-render | tests\acne_clear.rs | 11 | 0 | 1 |
| aura-render | tests\blemish_review.rs | 4 | 0 | 0 |
| aura-render | tests\colour_discipline.rs | 4 | 0 | 0 |
| aura-render | tests\gpu_fallback.rs | 0 | 0 | 1 |
| aura-render | tests\gpu_output.rs | 0 | 0 | 3 |
| aura-render | tests\native_retouch.rs | 22 | 0 | 0 |
| aura-render | tests\parity.rs | 7 | 0 | 0 |
| aura-render | tests\patch_heal.rs | 14 | 0 | 0 |
| aura-render | tests\portrait_local.rs | 0 | 0 | 1 |
| aura-render | tests\portrait_retouch.rs | 10 | 0 | 0 |
| aura-render | ..\..\tests\eval\render_eval.rs | 18 | 0 | 0 |
| aura-render | tests\restoration_order.rs | 10 | 0 | 0 |
| aura-render | tests\retouch_checkpoints.rs | 2 | 0 | 0 |
| aura-render | tests\retouch_profile.rs | 0 | 0 | 1 |
| aura-render | tests\sampled_skin.rs | 14 | 0 | 0 |
| aura-render | tests\shader_parity.rs | 10 | 0 | 0 |
| aura-render | tests\skin_finish.rs | 12 | 0 | 0 |
| aura-render | tests\skin_finish_photos.rs | 0 | 0 | 1 |
| aura-render | tests\stage_order.rs | 9 | 0 | 0 |
| aura-render | tests\stretch_marks.rs | 3 | 0 | 0 |
| aura-render | tests\tiling.rs | 4 | 0 | 0 |
| aura-restore | unittests src\lib.rs | 72 | 0 | 0 |
| aura-restore | tests\boundaries.rs | 4 | 0 | 0 |
| aura-restore | ..\..\tests\eval\restore_eval.rs | 17 | 0 | 0 |
| aura-retouch | unittests src\lib.rs | 107 | 0 | 0 |
| aura-retouch | tests\boundaries.rs | 3 | 0 | 0 |
| aura-retouch | ..\..\tests\eval\micro_eval.rs | 10 | 0 | 0 |
| aura-retouch | ..\..\tests\eval\retouch_eval.rs | 13 | 0 | 0 |
| aura-style | unittests src\lib.rs | 92 | 0 | 0 |
| aura-style | tests\no_network.rs | 3 | 0 | 0 |
| aura-style | ..\..\tests\eval\style_eval.rs | 19 | 0 | 0 |
| aura-vision | unittests src\lib.rs | 72 | 0 | 1 |
| aura-vision | tests\descriptors.rs | 23 | 0 | 0 |
| aura-vision | tests\embedding.rs | 13 | 0 | 0 |
| aura-vision | ..\..\tests\eval\embedding_eval.rs | 9 | 0 | 0 |
| aura-vision | tests\face_cluster.rs | 18 | 0 | 0 |
| aura-vision | tests\face_geometry.rs | 28 | 0 | 0 |
| aura-vision | tests\face_quality.rs | 17 | 0 | 0 |
| aura-vision | tests\face_roles.rs | 24 | 0 | 0 |
| aura-vision | ..\..\tests\eval\mask_eval.rs | 22 | 0 | 0 |
| aura-vision | tests\no_template_writes.rs | 2 | 0 | 0 |
| aura-vision | tests\skin_photos.rs | 0 | 0 | 1 |
| aura-vision | tests\skin_rotation.rs | 0 | 0 | 1 |
| licence-issue | unittests src\main.rs | 0 | 0 | 0 |
| model-sign | unittests src\main.rs | 0 | 0 | 0 |
| xtask | unittests src\main.rs | 0 | 0 | 0 |
| aura-app | doctests | 0 | 0 | 0 |
| aura-brain-gallery | doctests | 0 | 0 | 0 |
| aura-brain-photo | doctests | 0 | 0 | 0 |
| aura-brain-wedding | doctests | 0 | 0 | 0 |
| aura-cache | doctests | 0 | 0 | 0 |
| aura-catalog | doctests | 0 | 0 | 0 |
| aura-cloud | doctests | 0 | 0 | 1 |
| aura-core | doctests | 0 | 0 | 0 |
| aura-cull | doctests | 0 | 0 | 0 |
| aura-curate | doctests | 0 | 0 | 0 |
| aura-delivery | doctests | 0 | 0 | 0 |
| aura-explain | doctests | 0 | 0 | 0 |
| aura-export | doctests | 0 | 0 | 0 |
| aura-generative | doctests | 0 | 0 | 0 |
| aura-geometry | doctests | 0 | 0 | 0 |
| aura-index | doctests | 0 | 0 | 0 |
| aura-infer | doctests | 0 | 0 | 0 |
| aura-ingest | doctests | 0 | 0 | 0 |
| aura-jobs | doctests | 0 | 0 | 0 |
| aura-learn | doctests | 0 | 0 | 0 |
| aura-licence | doctests | 0 | 0 | 0 |
| aura-look | doctests | 0 | 0 | 0 |
| aura-models | doctests | 0 | 0 | 0 |
| aura-people | doctests | 0 | 0 | 0 |
| aura-perf | doctests | 0 | 0 | 0 |
| aura-portrait | doctests | 0 | 0 | 0 |
| aura-preview | doctests | 0 | 0 | 0 |
| aura-qc | doctests | 0 | 0 | 0 |
| aura-raw | doctests | 0 | 0 | 0 |
| aura-recipe | doctests | 0 | 0 | 0 |
| aura-render | doctests | 0 | 0 | 0 |
| aura-restore | doctests | 0 | 0 | 0 |
| aura-retouch | doctests | 0 | 0 | 0 |
| aura-style | doctests | 0 | 0 | 0 |
| aura-vision | doctests | 0 | 0 | 0 |


## UI file ledger

| File | Passed | Failed | Duration ms |
|---|---|---|---|
| ui/src/App.test.tsx | 1 | 0 | 175.5 |
| ui/src/nativeBindings.test.ts | 1 | 0 | 2.3 |
| ui/src/reachability.test.ts | 3 | 0 | 6.6 |
| ui/src/audit/log.test.ts | 6 | 0 | 7.0 |
| ui/src/components/AiKeysPanel.test.tsx | 13 | 0 | 5.2 |
| ui/src/components/AiSetup.test.tsx | 22 | 0 | 8.1 |
| ui/src/components/HardwarePanel.test.tsx | 5 | 0 | 3.2 |
| ui/src/components/ImportWizard.test.tsx | 2 | 0 | 163.7 |
| ui/src/components/InternalAiStatus.test.tsx | 3 | 0 | 110.0 |
| ui/src/components/LicencePanel.test.tsx | 4 | 0 | 155.4 |
| ui/src/components/LogPanel.test.tsx | 3 | 0 | 142.5 |
| ui/src/components/SimilarPanel.test.tsx | 17 | 0 | 5.7 |
| ui/src/ipc/retouchPreferences.test.ts | 9 | 0 | 7.6 |
| ui/src/stores/thumbnailStore.test.ts | 8 | 0 | 34.3 |
| ui/src/state/previewCache.test.tsx | 6 | 0 | 402.1 |
| ui/src/state/store.test.ts | 6 | 0 | 7.2 |
| ui/src/components/autopilot/Autopilot.test.tsx | 9 | 0 | 214.0 |
| ui/src/components/autopilot/AutopilotPanel.test.tsx | 7 | 0 | 290.8 |
| ui/src/components/autopilot/BatchPhotoPicker.test.tsx | 3 | 0 | 680.2 |
| ui/src/components/autopilot/PreflightDialog.test.tsx | 7 | 0 | 48.1 |
| ui/src/components/autopilot/prepareCollection.test.ts | 10 | 0 | 69.3 |
| ui/src/components/autopilot/ProgressPanel.test.tsx | 9 | 0 | 90.3 |
| ui/src/components/autopilot/RunSummary.test.tsx | 8 | 0 | 62.6 |
| ui/src/components/autopilot/StageList.test.tsx | 9 | 0 | 216.3 |
| ui/src/components/camera/CameraMatchView.test.tsx | 8 | 0 | 119.4 |
| ui/src/components/cleanup/CleanupPanel.test.tsx | 5 | 0 | 50.9 |
| ui/src/components/cleanup/ProposalQueue.test.tsx | 7 | 0 | 60.7 |
| ui/src/components/delivery/Delivery.test.tsx | 18 | 0 | 99.8 |
| ui/src/components/delivery/WatermarkPanel.test.tsx | 2 | 0 | 116.1 |
| ui/src/components/curate/AlbumBuilder.test.tsx | 6 | 0 | 74.4 |
| ui/src/components/curate/HeroGrid.test.tsx | 5 | 0 | 61.1 |
| ui/src/components/curate/SocialSets.test.tsx | 5 | 0 | 54.7 |
| ui/src/components/curate/SpreadView.test.tsx | 5 | 0 | 47.4 |
| ui/src/components/cull/CoveragePanel.test.tsx | 16 | 0 | 52.5 |
| ui/src/components/develop/AdvancedRetouch.test.tsx | 7 | 0 | 498.0 |
| ui/src/components/develop/AutoRetouchSettings.test.tsx | 20 | 0 | 1849.0 |
| ui/src/components/develop/BasicPanel.test.tsx | 16 | 0 | 178.6 |
| ui/src/components/develop/batchRetouch.test.ts | 2 | 0 | 4.2 |
| ui/src/components/develop/BlemishReview.test.tsx | 4 | 0 | 148.4 |
| ui/src/components/develop/brushPath.test.ts | 3 | 0 | 6.5 |
| ui/src/components/develop/DevelopPanel.test.tsx | 7 | 0 | 142.1 |
| ui/src/components/develop/DevelopWorkspace.test.tsx | 10 | 0 | 278.2 |
| ui/src/components/develop/GeometryPanel.test.tsx | 8 | 0 | 118.6 |
| ui/src/components/develop/Histogram.test.ts | 2 | 0 | 3.4 |
| ui/src/components/develop/LightroomPanel.test.tsx | 7 | 0 | 526.6 |
| ui/src/components/develop/LocalPanel.test.tsx | 8 | 0 | 93.7 |
| ui/src/components/develop/MaskPanel.test.tsx | 8 | 0 | 168.5 |
| ui/src/components/develop/MasksPanel.test.tsx | 4 | 0 | 120.7 |
| ui/src/components/develop/MicroRetouchPanel.test.tsx | 12 | 0 | 142.5 |
| ui/src/components/develop/NativeRetouchWorkspace.test.tsx | 38 | 0 | 15530.2 |
| ui/src/components/develop/PhotoStudio.test.tsx | 8 | 0 | 1375.8 |
| ui/src/components/develop/PortraitAutoReport.test.tsx | 4 | 0 | 48.2 |
| ui/src/components/develop/portraitOverlay.test.ts | 5 | 0 | 3.9 |
| ui/src/components/develop/PortraitRetouch.test.tsx | 6 | 0 | 157.4 |
| ui/src/components/develop/previewTools.test.ts | 2 | 0 | 3.5 |
| ui/src/components/develop/RestorePanel.test.tsx | 12 | 0 | 225.2 |
| ui/src/components/develop/RetouchCanvas.test.tsx | 10 | 0 | 193.7 |
| ui/src/components/develop/retouchColor.test.ts | 1 | 0 | 2.4 |
| ui/src/components/develop/RetouchPanel.test.tsx | 10 | 0 | 96.1 |
| ui/src/components/develop/RetouchPresets.test.tsx | 7 | 0 | 105.6 |
| ui/src/components/develop/RetouchSkinSelection.test.tsx | 6 | 0 | 130.8 |
| ui/src/components/develop/rgbImage.test.ts | 5 | 0 | 4.2 |
| ui/src/components/develop/SavedRetouchStrength.test.tsx | 2 | 0 | 126.1 |
| ui/src/components/develop/SyncSettingsPanel.test.tsx | 1 | 0 | 84.3 |
| ui/src/components/develop/TonePanel.test.tsx | 16 | 0 | 140.5 |
| ui/src/components/develop/useRetouchDraftPreview.test.tsx | 4 | 0 | 34.0 |
| ui/src/components/develop/useSavedRetouchCoverage.test.ts | 2 | 0 | 95.1 |
| ui/src/components/explain/CompositionCard.test.tsx | 22 | 0 | 283.5 |
| ui/src/components/explain/EmotionCard.test.tsx | 12 | 0 | 18.1 |
| ui/src/components/explain/ExplainPanel.test.tsx | 15 | 0 | 219.7 |
| ui/src/components/explain/Inspector.test.tsx | 4 | 0 | 120.7 |
| ui/src/components/explain/IntegrityCard.test.tsx | 26 | 0 | 10.9 |
| ui/src/components/gallery/ConsistencyView.test.tsx | 15 | 0 | 200.0 |
| ui/src/components/grid/MomentStack.test.tsx | 24 | 0 | 9.7 |
| ui/src/components/grid/VirtualGrid.test.tsx | 3 | 0 | 117.3 |
| ui/src/components/look/applyLook.test.ts | 5 | 0 | 5.4 |
| ui/src/components/look/ColorMatchPanel.test.tsx | 4 | 0 | 289.0 |
| ui/src/components/look/InstagramStyle.test.tsx | 5 | 0 | 189.0 |
| ui/src/components/look/matchColorCollection.test.ts | 3 | 0 | 6.0 |
| ui/src/components/look/MatchLookPanel.test.tsx | 21 | 0 | 6.2 |
| ui/src/components/look/PairedReference.test.tsx | 4 | 0 | 232.5 |
| ui/src/components/people/PeoplePanel.test.tsx | 32 | 0 | 7.3 |
| ui/src/components/profiles/PersonalStyle.test.tsx | 2 | 0 | 140.3 |
| ui/src/components/profiles/ProfileGallery.test.tsx | 4 | 0 | 431.6 |
| ui/src/components/qc/QcReport.test.tsx | 7 | 0 | 170.1 |
| ui/src/components/qc/TicketQueue.test.tsx | 5 | 0 | 146.6 |
| ui/src/components/story/StoryPanel.test.tsx | 7 | 0 | 172.1 |
| ui/src/components/story/Timeline.test.tsx | 23 | 0 | 7.2 |
| ui/src/components/style/StylePanel.test.tsx | 4 | 0 | 240.2 |
| ui/src/components/style/StylePanels.test.tsx | 17 | 0 | 202.3 |
| ui/src/components/workflow/AutomaticProgress.test.tsx | 5 | 0 | 122.5 |
| ui/src/components/workflow/FinishFolder.test.tsx | 5 | 0 | 189.6 |
| ui/src/components/workflow/OneClickRunner.test.tsx | 5 | 0 | 182.7 |
| ui/src/components/workflow/StatusBar.test.tsx | 4 | 0 | 91.4 |
