# AURA / Evoto deep inspection: continuation report

Date: 18 October 2026. This is a **new report** continuing [the first implementation audit](AURA-Evoto-audit-2026-10-18.md). It records additional static findings from installed Evoto 7.3.5, the reproducible analysis method, what AURA would need for equivalent behaviour, and the remaining evidence needed for an exact-output claim.

## What “same exact” can mean

Four different goals need separate acceptance criteria:

| Goal | How to establish it | Current status |
|---|---|---|
| Similar controls and workflow | Versioned inventory and repeatable UI interactions. | Partially documented; no complete local UI inventory. |
| Equivalent tasks | Each AURA feature processes the same input class and exposes the required protections and corrections. | Some native capabilities exist; important analysis/model gaps remain. |
| Comparable photographic quality | Blinded regional review and measured failures on a held-out corpus. | Not established. Four AURA regression portraits are insufficient. |
| Identical pixels for identical inputs/settings | Same source, geometry, profiles, algorithm/model behaviour and export transform, compared at native resolution. | Not established; private models and exact processing semantics remain unknown. |

Recovering a wrapper that loads a neural model is not recovering the model. A control named “skin smoothing” is not evidence that two products produce the same pixels. This report keeps those distinctions explicit. Full reverse engineering has not been completed; the final evidence summary below states the actual extent achieved.

The open IDE file `H:\editing to aura\AURA_Native_Edited_2026-10-17\Review.html` is an older AURA delivery review. It references 98 selected photographs and before/final previews. It is not an Evoto reference set, and its camera-preview comparison is not a same-domain RAW/RAW comparison. This audit does not overwrite that review or its photographs.

## 1. Source identity and scope

The static target is `F:\evoto\Evoto-worker.exe`, size 282,190,224 bytes, SHA-256 `99edcbb0df9b063ca6fae3bb9603b15dcc41994532cc3cde7434e66404078f86`, x64 PE32+ with preferred image base `0x140000000`. Addresses in this report are preferred static virtual addresses. They are **not** file offsets and are not necessarily live process addresses under ASLR.

The previous audit installed REA 6.5.0, Ghidra 12.1.4 and a verified Temurin JDK. REA remains installed. The continuation adds:

- A bounded read-only PE/string-reference scanner: `scripts/evoto-static-trace.py`.
- A Ghidra script that disassembles and attempts decompilation of explicitly selected function ranges: `scripts/ghidra/AuraTargetedTrace.java`.
- A persistent analysis project, so successful imports and targeted work can be resumed without rebuilding a whole-program inventory.
- A fresh REA PE-resource evidence record with a file-size budget appropriate to the actual worker.

No installed executable is patched. No licence/authentication bypass is attempted. No proprietary model weights, shader assets or recovered function bodies are added to AURA's renderer. Analysis artifacts are local evidence; independently specified AURA implementations must be validated separately.

## 2. Why the earlier analysis timed out, and the new approach

The prior REA/Ghidra whole-program attempts returned `provider_timeout` at startup deadlines of 330 and 450 seconds. Their diagnostics are retained in `.work-checks/evoto-audit-2026-10-18`. They did not produce a completed native function analysis.

This continuation reads the PE exception directory directly and finds **972,717 runtime-function/unwind ranges**. These can include function fragments and compiler/runtime helpers; this is not a count of source-level functions or product features. It explains why indiscriminate analysis is expensive. A read-only thread observation of our own Ghidra process showed the loader marking up runtime-function exception data and database references. This is evidence of analysis/import work, not a frozen Evoto application or proof of encrypted code.

The new scanner searches task-related ASCII literals only in selected data sections, identifies candidate x64 RIP-relative LEA encodings in executable sections, maps their destinations into matching strings, and associates each candidate with its enclosing unwind range. It does not pretend that a byte-pattern match alone proves an instruction. Validation then uses bounded disassembly/decompilation at the candidate range.

The initial scan found 466 matching strings; expanding the explicit search terms to under-eye and eyelid features produced **558 matching strings**, of which **402 have encoding candidates**, and **264 distinct bounded candidate ranges** after the scanner's size filters. The completed batches contain 13 initial ranges, 11 blemish/protection ranges, 3 callers and 6 eye-related ranges: **33 distinct selected ranges**. These counts describe the configured scan, not complete string/function coverage.

ASCII-only extraction omits UTF-16, packed text, indirect string references, computed dispatch and constants that have no matching feature name. A missing match does not mean a capability is absent. Runtime behaviour is not established by static reachability alone.

Ghidra rejected a project directory containing the `.work-checks` path component because of its project-path naming rules. The persistent project therefore resides at `C:\Users\Admin\AppData\Local\aura-analysis\evoto-2026-10-18\EvotoWorker735`. Scripts and evidence remain under the workspace audit directory. The source application and photographs are untouched.

## 3. New architectural evidence

The following are observations from the installed binary, not guesses based on public marketing. Their validation level is refined in the final analysis-results section.

### 3.1 Skin selection has several representations

Observed diagnostic/resource identifiers distinguish total skin masks, per-person instance masks, human-parsing skin masks, face-parsing masks, original face-skin masks and masks after expression changes. Additional strings refer to adapting body-mask dimensions to skin-mask dimensions and dedicated APIs for matte refinement.

This supports an architecture with **multiple semantically different mask inputs**. It does not establish the complete class taxonomy, probability thresholds, face-parser resolution or the exact rule used to combine masks.

| Evidence | Static location / relevant range | What it supports |
|---|---|---|
| `HAVE_FACE_PARSING_MASK_TEXTURE` | Literal `0x14cada4e0`; candidate range `0x143a8d810`–`0x143a8dc34` | A face-parsing texture concept exists in a renderer-related path. |
| Adult cache checks mention landmarks, instance masks and `FaceParsingV2` | Range `0x147a9b0d0`–`0x147a9c635` | Adult blemish setup depends on more than a single skin-colour mask. |
| Human-parsing skin-mask diagnostic | Included in `static-trace.json` | Human parsing is used as a distinct source of skin-mask information. |
| Skin-mask update after smile/close-mouth diagnostics | Included in `static-trace.json` | Geometry/expression edits have mask-update dependencies. Exact dispatch/order remains unknown. |

**AURA implication:** unify person ownership, fine face labels, body labels, protected anatomy, artist masks and geometry transforms in one analysis contract. Each operation should request the representations it needs. A generic six-class selfie mask is useful but cannot alone provide every eyelid, nostril, lip-border and makeup distinction.

### 3.2 Adult blemish processing distinguishes several inference tasks

The installed worker contains separate function-linked diagnostics for blemish inference, nevus inference, freckle-mask inference and acne-mask inference:

| Routine evidence | Candidate range | Diagnostic input-channel check |
|---|---|---:|
| Blemish inference | `0x147a9a8d0`–`0x147a9abb7` | 3 |
| Nevus inference | `0x147a9adf0`–`0x147a9b0c1` | 3 |
| Freckle-mask inference | `0x147a9abc0`–`0x147a9ade4` | 4 |
| Acne-mask inference | `0x147a9a560`–`0x147a9a8c4` | 3 |

The channel counts were subsequently confirmed against the decoded routines and their branches: the shape access checks a dimension against 3 or 4 and the failure branch references the corresponding diagnostic. This supports the declared channel-count contract, but does not establish every accepted tensor layout/rank, preprocessing normalization or fourth-channel meaning. It does not reveal training labels or model accuracy. A routine named nevus does not prove all moles are always preserved; behaviour also depends on settings and composition.

The recovered wrappers select different model-handle locations relative to the pipeline object: blemish `+0x28`, acne mask `+0x30`, freckle mask `+0x38`, nevus `+0x40`. They share certain local inference helper addresses. This is static evidence of distinct task handles through common plumbing, not proof of four independent network architectures, particular training, or a reusable proprietary model.

There are also distinct post-processing ranges for the blemish result, freckle mask and acne mask. Input/cache validation mentions total skin masks, per-face landmarks, instance masks, face parsing and valid preview/export regions of interest. This gives a much more concrete target than treating every compact dark patch as acne.

**AURA implication:** retain measured candidate detection as a fallback, but add a validated, rights-cleared task model or models that distinguish temporary defects from permanent or cosmetic details. Keep defect proposals separate from protection masks and repair pixels. Do not implement the freckle task by guessing that the fourth channel is a skin mask merely because that would be convenient.

### 3.3 Feature protection is an explicit operation in at least one pipeline

A newborn-specific routine identified by `faceFeatureProtect` has a candidate range `0x147aa29a0`–`0x147aa3a08`. Its diagnostics separately require `FaceParsingV2` and another face-parsing representation.

This is evidence of an explicit feature-protection dependency in that **newborn pipeline**. It must not be generalized into a proven identical adult pipeline without tracing the adult path. The exact protected labels, dilation/feathering, fallback rules and blend equations remain unknown.

**AURA implication:** make feature protection an explicit, operation-aware stage with provenance and uncertainty. Eyes, nose, mouth, facial hair, makeup and ceremonial pigment need different rules. An under-eye-lightening tool needs some skin below the eye, while a blemish-repair tool must not reconstruct the eyelid or socket shadow.

### 3.4 Nose cleanup has a separate two-model loading contract

The nose-flaw pipeline has diagnostics requiring exactly two files or buffers, identified as G1 and G2, and independent load-failure checks. Candidate buffer-loading range: `0x147ae70a0`–`0x147ae781d`; candidate file-loading range: `0x147ae7820`–`0x147ae82db`.

Validation/preprocessing ranges check face enablement, serialized export data, instance-mask validity and region validity. This supports a dedicated nose task with explicit preconditions. It does not establish that G1 and G2 are generators, segmentation/refinement stages or any particular architecture. Their names are insufficient to identify a training method.

**AURA implication:** nose blemishes cannot safely be handled by a broad cheek smoothing mask. Detect local defects while preserving nostril openings, nose-wing contours and illumination. The two-model observation is an investigative lead; independently copying that model count is not a proven path to equivalent quality.

### 3.5 Makeup protection participates in face composition

CPU face-composition diagnostics refer separately to missing makeup-protection images and missing makeup-protection masks. Candidate ranges include `0x143ac9d80`–`0x143acb7c2`, `0x143acb7d0`–`0x143acdef0` and `0x143ad1af0`–`0x143ad4a39`.

This suggests that protection affects result composition and is not merely a label shown in the UI. Exact blend order, protected image content, CPU/GPU agreement and activation conditions still require tracing.

**AURA implication:** protection should constrain final corrections and donor selection as well as candidate detection. A defect detector that avoids makeup can still damage makeup if a later smoothing/unification pass crosses it. Verify the combined stack against the intended protection, not only each planner in isolation.

### 3.6 Smoothing depends on masks and multiple image bands

The worker contains a face high/low-frequency smoothing render diagnostic for a missing skin mask at `0x146b5c0b0`–`0x146b5c27f`. Other strings identify Gaussian/median/skin-mask/adjustment-LUT texture inputs and shader names for HSV/adaptive skin masks.

These show that mask-based and frequency-related processing concepts are present. They do not prove that a named Gaussian filter is the final production smoothing operator on every photo, nor identify kernel sizes, model outputs, colour space or processing order. Different paths may be legacy, fallback, preview or platform-specific implementations.

**AURA implication:** retain selected-pixel normalized filtering and bounded correction, but validate band scales, source shading, semantic barriers and native-resolution edges. Do not interpret broad low-frequency differences as unwanted skin texture near a jaw or eyelid.

### 3.7 White balance has RAW/JPEG-specific AI dependencies

A candidate range `0x143749f60`–`0x14374b492` references both JPEG and RAW auto-WB diagnostics and distinguishes `AutoWBPipeline` from `AutoWBPipeline_RAW`. It also checks a state described as processed AI data. Separate candidate loader ranges `0x1436c84a0`–`0x1436c87f5` and `0x1436c8810`–`0x1436c8b65` mention loading those respective models from files.

This supports domain-specific inference components and prerequisite state. It does not reveal exact WB gains, training data, illuminant priors, camera profiles, mixed-light treatment or whether a particular automatic result was used for the selected photograph.

**AURA implication:** technical RAW WB and finished-JPEG cast correction should have separate data-domain contracts. A correction inferred from display pixels cannot simply be treated as camera-native gain ratios. Validate source camera transform, as-shot gains, adaptation, tone mapping and output colour management independently from any aesthetic reference matching.

### 3.8 Matte refinement tracks several persistent states

Strings distinguish base-edge masks, applied masks, result masks, source/result signatures, result image identity, edit state and pending automatic application. A result-creation diagnostic explicitly mentions retaining an old cache if a background-edge operation fails.

These are strong leads for cache/provenance/state-management work. The surrounding matte-refinement feature may concern subject/background edges; the name must not be used as proof that it is Evoto's face-skin edge-retouch algorithm. Exact cache-validity checks and UI transitions remain to be traced.

**AURA implication:** every result/mask/QC verdict must be bound to the exact source, geometry, analysis model and recipe revision. A stale result must not become a current successful edit. Background-edge refinement and internal facial-anatomy protection should be specified separately.

### 3.9 Caller tracing confirms conditional task composition

Three additional ranges were followed from direct-call candidates and successfully decoded/decompiled:

| Range | Observed static relationship |
|---|---|
| `0x147a99e40`–`0x147a9a557` | Adult inference dispatcher calls the blemish/nevus, freckle-mask and acne-mask wrappers under separate flags. It has a synchronous branch and another branch involving further dispatch helpers. The full scheduling semantics are not recovered. |
| `0x147a9ea30`–`0x147a9f46d` | An adult orchestration path invokes the dispatcher, then conditionally calls blemish, acne-mask and freckle-mask postprocessors. |
| `0x147aa1f00`–`0x147aa20e5` | A newborn result path calls the parsing-dependent feature-protection routine before a subsequent result-combination helper. The complete helper semantics remain unresolved. |

All **14** direct-call candidates emitted for these selected defect/protection targets were confirmed against Ghidra CALL references. This establishes these specific static edges, not the global order of all Evoto effects or dynamic activation for an arbitrary photograph.

**AURA implication:** independent task controls should preserve their inference dependencies and protection/composition order. A single universal removal strength cannot reproduce separately gated semantic tasks. Model inputs, outputs and cache validity must remain compatible when one task is disabled or reused.

### 3.10 Dark circles and eye bags are distinct pipeline concepts

Additional search terms found `DarkCirclesV1Pipeline` and `EyebagV1Pipeline`, with associated factory/registration ranges `0x146b698f0`–`0x146b699b7` and `0x146ba0840`–`0x146ba0907`. These short ranges are **registration wrappers**, not the full image-processing algorithm.

For the dark-circle class, the scanner validated an MSVC x64 complete-object locator by its signature, type-descriptor RVA and self-RVA. It found a corresponding table at `0x14cfc8230`, with locator `0x14d7a61a8` and type descriptor RVA `0xe738290`. Four bounded method ranges were decoded/decompiled: `0x146b37db0`, `0x147b0fcc0`, `0x147b0faa0`, `0x147b13bd0`. A further slot at `0x147b13bc0` has no matching unwind range in this scan and was not silently promoted to a recovered function.

The decoded methods delegate to additional helpers and, in one path, iterate per-result/per-instance objects and set success from a return value. This does not yet reveal the actual dark-circle correction equation, segmentation labels, network weights or lower-eyelid protection curve. A vtable association is stronger than matching a display label, but it is not full semantic recovery. The corresponding eye-bag vtable was not established by this RTTI scan.

**AURA implication:** treat pigmentation/dark-circle correction and eye-bag/lid-volume correction as distinct tasks. The separate pipeline evidence is consistent with that design; it does not authorize guessing a private algorithm or claiming the existing AURA scalar lift is identical.

## 4. A cautious architecture reconstruction

The evidence supports the following **partial dependency map**, with unknown edges deliberately marked. It is not a recovered global execution order:

```text
source image / RAW metadata
    |-- domain-specific RAW/JPEG auto-WB model loading and prerequisite state
    |
    |-- person/instance masks
    |-- face landmarks
    |-- total/body/face skin representations
    |-- face parsing and FaceParsingV2
          |
          |-- adult blemish setup / inference / postprocessing
          |      |-- blemish
          |      |-- nevus
          |      |-- freckle mask
          |      `-- acne mask
          |
          |-- newborn feature-protection path
          `-- dedicated nose-flaw path (G1/G2 loading)

effect results + protection masks/images
    `-- face composition (observed makeup-protection dependencies)

separate matte-refinement state
    `-- base/applied/result masks + source/result signatures

exact shared inference backbone, trained weights, blend equations,
complete stage ordering and all CPU/GPU branches: UNKNOWN
```

The independently designed AURA architecture should preserve these useful separations without copying undocumented private structures or assuming equivalent quality from similar names.

## 5. What AURA lacks relative to the new evidence

| Area | AURA source status | Practical gap / next requirement |
|---|---|---|
| Face geometry | Automatic path starts from five YuNet landmarks with additional image-derived guards. | Dense facial geometry and visibility/pose uncertainty, not only wider fixed exclusion shapes. |
| Fine semantics | Selfie Multiclass is real six-class segmentation. | Validated eyelid/lip/nostril/makeup/facial-hair labels and high-resolution boundaries. |
| Defect categories | Main cleanup uses measured heuristics; dedicated registered blemish/permanent heads are untrained. | Validated temporary-defect/permanent-mark understanding and a clear uncertain-mark policy. |
| Nose defects | Recent guards preserve nose openings and contour structure. | Local defect understanding with strict anatomy preservation and native-resolution acceptance checks. |
| Eye corrections | Current bounded under-eye rendering preserves more structure than older output. | Separate lid/socket/pigment/makeup treatment; continuity and protected-feature validation on new poses/lighting. |
| Makeup/ceremonial marks | Dedicated image-derived protections exist, including recent bindi growth/protection logic. | Calibrated semantics plus final-composition constraints; avoid both missing adjacent skin and growing skin over pigment. |
| Consistent masks | Several automatic/legacy render paths derive regions differently. | One shared, revisioned analysis and coordinate contract. |
| White balance | Actual native colour pipeline and conservative measured estimators exist. | Calibrated RAW/JPEG-domain behaviour, mixed-light handling and representative camera/colour validation. |
| QC | Previous audit added honest advanced-QC status and contour warnings. | All-path, revision-bound native-resolution QC and explicit failure handling; no universal “perfect” verdict. |
| Reference matching | Existing same-photo matcher searches bounded global controls and caps 16 MP. | Memory-bounded support for these 30 MP files plus a separate semantic cross-photo style contract. |

## 6. Independent implementation specification

The following is proposed AURA logic, **not recovered Evoto source**.

### 6.1 Analysis artifact

Store source hash, orientation, camera/profile provenance, model IDs/hashes, coordinate transforms, person IDs, face/part probability maps, confidence calibration version, artist protection and per-operation coverage. Store uncertainty rather than converting all outputs to a binary mask.

Every consumer must use the same analysis domain. Do not let a panel show a selfie-segmenter mask while a later renderer silently derives a different Haar/colour region. Geometry changes require valid remapping or recomputation. Cache keys must include all relevant transforms and revisions.

### 6.2 Operation-aware selection

For a proposed correction, combine semantic support, person ownership, local confidence, artist selection and operation-specific protection. A skin-smoothing operation must exclude eye/mouth/nose openings; eye-specific work requests its own regions. A dark under-eye pixel is not automatically non-skin. A tiny uncertain strip should receive reduced work or a reported skip.

Keep protected holes separate from exterior background. Use boundary trimaps and native-pixel evidence for uncertain edges; refine without indiscriminate mask growth. If skin and background have similar colours, colour guidance alone is insufficient and uncertainty must remain visible.

### 6.3 Separate proposals, protections and repairs

The detector proposes defects with category/confidence, not delivered pixels. The protection stage decides what must remain. The repair stage searches safe same-person donors or solves a bounded reconstruction. The compositor applies the allowed delta. The verifier inspects the result at the actual required scale. Report all five stages.

Do not classify scars, moles, freckles, religious pigment or makeup as temporary solely because they are compact or dark. Cross-frame evidence can help, but only with verified same-person alignment. User-protected marks take priority over cosmetic strength settings.

### 6.4 Local repair and tone handling

Use comparable anatomical regions and illumination for donors. Reject donors crossing coherent contours or carrying other defects. Fit low-frequency tone/chroma separately from high-frequency texture; avoid cloned pattern repetition. Work at scales tied to face size and output pixels, with sufficient composed tile halos.

For under-eye darkness, estimate local illumination and pigment residuals separately from lid/socket shape. Solve one continuous, bounded exposure/chroma delta field. Do not fill the entire area to a cheek-average colour. Validate along the whole transition, per eye, including closed/turned eyes and makeup.

For nose/chin/neck, preserve coherent gradient orientation and contour position. A source shadow between chin and neck is not an automatic repair target. Reducing a detected blemish must not generate a bright rim or erase the nostril boundary.

### 6.5 Colour and white balance

Keep source RAW gains, camera transform, scene-linear adjustments, local illumination corrections, aesthetic style and output transform explicit. Estimate WB with calibrated uncertainty and multiple cues; retain as-shot intent when evidence is insufficient. A single global temperature/tint pair cannot generally neutralize spatially different lights.

Never define correctness as moving all subjects to a fixed skin colour. Use neutral targets and chart captures for technical calibration; use approved references and professional judgement for the intended style. A reference with warm skin or coloured lighting may be deliberate.

### 6.6 Delivery gate

Require a successful current-revision render, measured region coverage, feature/edge checks, texture/colour limits and file-integrity checks. Failed edits must not silently be represented as final approved work merely because an older recipe can still render. A current missing measurement should show `incomplete`; an older pass must not transfer to a changed recipe.

## 7. Exact-output comparison protocol

No exact-output claim is meaningful until a controlled reference exists. For each reference image record Evoto version, original hash, camera profile, all relevant slider/mask settings, crop/orientation, enabled AI features, export dimensions, bit depth, colour space and output sharpening. Use authorized exports; this audit does not automatically spend account credits.

For each AURA candidate preserve those observable conditions and report technical baseline, retouch-only difference and final delivery separately. Compare eyes, nostrils, lip border, tika, hairline, nose wing, chin/neck and body skin at native resolution. A small whole-image error can hide a severe local artifact.

Before adding a new model, verify weight/code/dataset rights separately and evaluate it on held-out identities and capture sessions. Calibrate detection/protection quality across complexions, makeup, lighting, camera codecs, expression, occlusion and skin-region width. Then evaluate speed/memory/cancellation on actual 30 MP files. A successful adapter build is not a trained or validated feature.

## 8. Reproducibility and evidence locations

All continuation evidence is under `.work-checks/evoto-deep-2026-10-18` unless otherwise noted:

| Artifact | Purpose |
|---|---|
| `static-trace.json` | Source hash, preferred base, bounded feature strings, encoding candidates and associated unwind ranges. |
| `target-functions.tsv` | First 13 explicitly selected ranges. |
| `blemish-functions.tsv` | Additional 11 defect/protection ranges. |
| `all-candidate-functions.tsv` | Candidate range inventory under the scanner's size constraints. |
| `worker-resources.evidence.json` | Fresh successful REA PE-resource inspection; not evidence of ML-model extraction. |
| `ghidra-persistent-launch.log` / `ghidra-persistent.log` | Persistent import and first targeted-script logs. |
| `targeted-decompilation.json` | First targeted results, if successfully produced; status summarized below. |
| `blemish-decompilation.json` / `caller-decompilation.json` / `eye-decompilation.json` | Completed additional targeted disassembly/pseudocode batches. |
| `validated-feature-links.json` | Cross-checks candidate literals against decoded instructions/operands and caller candidates against CALL references. |
| `eye-rtti-vtables.json` | Validated locator structure and candidate dark-circle virtual table/slots; not a full class/source reconstruction. |
| `ghidra-threads.txt` | Read-only diagnostic of our analysis process, showing the import workload. |
| Persistent Ghidra project outside `.work-checks` | Resumable imported program; avoids discarding completed analysis on each CLI exit. |

Run the static scan:

```powershell
python scripts/evoto-static-trace.py --binary 'F:\evoto\Evoto-worker.exe' --output .work-checks/evoto-deep-2026-10-18
```

The Ghidra script uses a 24-range cap, 16,000-byte range cap, per-function decompiler deadline and incremental JSON writing. It records disassembly/decompiler success separately. It does not claim a timeout is a successfully recovered function. The project uses `-noanalysis` for bounded targeted work; this deliberately limits whole-program type, call-graph and switch recovery.

Reprocess the saved project and validate the results:

```powershell
./scripts/trace-evoto-targets.ps1 `
  -GhidraRoot '.work-checks/evoto-audit-2026-10-18/analysis-tools/ghidra_12.1.4_PUBLIC' `
  -JdkRoot '.work-checks/evoto-audit-2026-10-18/analysis-tools/jdk-21.0.12.1+1' `
  -ProjectRoot 'C:\Users\Admin\AppData\Local\aura-analysis\evoto-2026-10-18' `
  -EvidenceRoot '.work-checks/evoto-deep-2026-10-18'
python scripts/summarize-evoto-traces.py .work-checks/evoto-deep-2026-10-18
python -m unittest discover -s scripts/tests -p 'test_evoto_static_trace.py' -v
```

For a new project, create the chosen non-dot-prefixed directory first and use `-ImportWorker` once. Use the normal processing mode on subsequent runs. Do not start concurrent writers against the same Ghidra project. PowerShell execution policy must permit the independently reviewed local helper; the session used a process-local invocation without changing machine policy.

## 9. Coverage and remaining unknowns

Even successful targeted decompilation covers a tiny portion of this binary. Required further work includes exact inference tensor normalization/layout, model identities and output contracts, part-label mappings, enabled/disabled paths, complete composition order, blend equations, native-resolution/preview differences, GPU kernels, protection settings and failure/cache behaviour.

The neural weights, training corpus and training recipe remain unknown. Wrapper recovery can reveal inputs, dispatch and safeguards while still leaving most photographic behaviour in private weights. Public model alternatives or independently trained models are possible AURA development options, but they are not an “exact recovered Evoto model.”

Background generative tools, culling, tethering, presets, cloud/session behaviour and all 100 named resource modules are separate coverage items. The appendix lists the installed module names; a name's presence is not a completed reverse-engineering result or an AURA feature.

## 10. Final analysis and validation results

| Check | Observed result |
|---|---|
| Persistent Ghidra import/save | Successful; project retained for continuation. |
| Targeted disassembly | 33 selected ranges decoded; 16,093 instructions enumerated across them. |
| Targeted decompilation | Ghidra reported completed pseudocode for all 33 ranges, with no per-function failure message in these results. This is not proof of correct original types or complete semantics. |
| Literal-reference cross-check | 74 of 74 emitted candidate links within the selected ranges confirmed against decoded references or absolute LEA operand representations. |
| Direct-call cross-check | 14 of 14 selected direct-call candidates confirmed against decoded CALL references. |
| REA PE-resource inspection | Successful evidence envelope for the actual worker hash. Resource inventory is not model extraction. |
| Synthetic analysis tests | 4 passed: offset/VA/range mapping, read-only candidate scan, rejection of incorrect decoded targets/mixed binary hashes, and rejection of x86 as x64. |
| Photo edits/exports in this continuation | None. Existing originals, approved recipes and delivered photos were not reprocessed by these analysis tools. |

The scanner and summarizer deliberately retain **candidate** versus **validated** status. Ghidra's LEA address operands on this language are not always present in the automatic reference list; the cross-check therefore also records Ghidra's decoded absolute operand representation. That is a decoded-instruction check, not proof of reachable/runtime execution. No failed or unvalidated item was converted into a success merely to increase coverage.

The first report's tested AURA QC changes remain distinct from this continuation's analysis helpers. This continuation does **not** integrate new trained skin/defect/WB models, copy private code into AURA, or demonstrate exact Evoto output. It makes further investigation reproducible and provides more specific, function-linked implementation requirements. Full reverse engineering, trained-model equivalence and broad photographic-quality parity remain unfinished.


## Appendix A. Complete observed effect-module directory inventory

All 100 names below are installation observations. Their algorithms/weights and complete run-time wiring have not all been reconstructed. They are not a claim of implemented AURA parity.

| # | Module | Verified scope |
|---:|---|---|
| 1 | AIReshape | Directory present; implementation coverage varies/unknown. |
| 2 | AIShine | Directory present; implementation coverage varies/unknown. |
| 3 | AISkinUnify | Directory present; implementation coverage varies/unknown. |
| 4 | BeardProtect | Directory present; implementation coverage varies/unknown. |
| 5 | BgBalance | Directory present; implementation coverage varies/unknown. |
| 6 | BgClean | Directory present; implementation coverage varies/unknown. |
| 7 | BgDeMoire | Directory present; implementation coverage varies/unknown. |
| 8 | BgEnhance | Directory present; implementation coverage varies/unknown. |
| 9 | BgFloorReflection | Directory present; implementation coverage varies/unknown. |
| 10 | BgFloorReflectionPrepare | Directory present; implementation coverage varies/unknown. |
| 11 | BgMistake | Directory present; implementation coverage varies/unknown. |
| 12 | BgReplaceCustomColor | Directory present; implementation coverage varies/unknown. |
| 13 | BgShadow | Directory present; implementation coverage varies/unknown. |
| 14 | BgShadowRetain | Directory present; implementation coverage varies/unknown. |
| 15 | BgShadowRetainPostProcess | Directory present; implementation coverage varies/unknown. |
| 16 | BodyAcne | Directory present; implementation coverage varies/unknown. |
| 17 | BodyDermabrasionHLF | Directory present; implementation coverage varies/unknown. |
| 18 | BodyPortraiture | Directory present; implementation coverage varies/unknown. |
| 19 | BodySkinUnify | Directory present; implementation coverage varies/unknown. |
| 20 | BrightEye | Directory present; implementation coverage varies/unknown. |
| 21 | CloseMouth | Directory present; implementation coverage varies/unknown. |
| 22 | ClothesBlemishRemoval | Directory present; implementation coverage varies/unknown. |
| 23 | ClothesCrease | Directory present; implementation coverage varies/unknown. |
| 24 | ClothesEdgeSmooth | Directory present; implementation coverage varies/unknown. |
| 25 | CollarBoneEnhance | Directory present; implementation coverage varies/unknown. |
| 26 | CustomizeWaterMark | Directory present; implementation coverage varies/unknown. |
| 27 | DeTattoo | Directory present; implementation coverage varies/unknown. |
| 28 | Debug | Directory present; implementation coverage varies/unknown. |
| 29 | Degreasing | Directory present; implementation coverage varies/unknown. |
| 30 | DoubleChin | Directory present; implementation coverage varies/unknown. |
| 31 | Edit | Directory present; implementation coverage varies/unknown. |
| 32 | EyeRemoveRedBlood | Directory present; implementation coverage varies/unknown. |
| 33 | EyeWhiten | Directory present; implementation coverage varies/unknown. |
| 34 | EyelidGenerator | Directory present; implementation coverage varies/unknown. |
| 35 | EyesSize | Directory present; implementation coverage varies/unknown. |
| 36 | FaceAcne | Directory present; implementation coverage varies/unknown. |
| 37 | FaceAcneBaby | Directory present; implementation coverage varies/unknown. |
| 38 | FaceDermabrasionHLF | Directory present; implementation coverage varies/unknown. |
| 39 | FacePlump | Directory present; implementation coverage varies/unknown. |
| 40 | FacePortraiture | Directory present; implementation coverage varies/unknown. |
| 41 | FaceShaping3D | Directory present; implementation coverage varies/unknown. |
| 42 | FaceStereo | Directory present; implementation coverage varies/unknown. |
| 43 | FaceSurgery | Directory present; implementation coverage varies/unknown. |
| 44 | Filters | Directory present; implementation coverage varies/unknown. |
| 45 | GlassReflectionRemoval | Directory present; implementation coverage varies/unknown. |
| 46 | GrowthLine | Directory present; implementation coverage varies/unknown. |
| 47 | HairColorAdjust | Directory present; implementation coverage varies/unknown. |
| 48 | HairColorUnify | Directory present; implementation coverage varies/unknown. |
| 49 | HairGloss | Directory present; implementation coverage varies/unknown. |
| 50 | HairSeam | Directory present; implementation coverage varies/unknown. |
| 51 | HairSmooth | Directory present; implementation coverage varies/unknown. |
| 52 | Inpaint | Directory present; implementation coverage varies/unknown. |
| 53 | IrisEditing | Directory present; implementation coverage varies/unknown. |
| 54 | IrisEditingRedMask | Directory present; implementation coverage varies/unknown. |
| 55 | LensBlur | Directory present; implementation coverage varies/unknown. |
| 56 | LensCorrect | Directory present; implementation coverage varies/unknown. |
| 57 | LipWrinkleRepair | Directory present; implementation coverage varies/unknown. |
| 58 | Liquify | Directory present; implementation coverage varies/unknown. |
| 59 | MakeupEnhance | Directory present; implementation coverage varies/unknown. |
| 60 | MakeupProtect | Directory present; implementation coverage varies/unknown. |
| 61 | Makeups | Directory present; implementation coverage varies/unknown. |
| 62 | MatteRefinement | Directory present; implementation coverage varies/unknown. |
| 63 | MatteRefinementMix | Directory present; implementation coverage varies/unknown. |
| 64 | MultiplePeopleAISkinUnify | Directory present; implementation coverage varies/unknown. |
| 65 | NeckLines | Directory present; implementation coverage varies/unknown. |
| 66 | NeutralGray | Directory present; implementation coverage varies/unknown. |
| 67 | NeutralGrayBodyEven | Directory present; implementation coverage varies/unknown. |
| 68 | NeutralGrayDB | Directory present; implementation coverage varies/unknown. |
| 69 | NeutralGrayHand | Directory present; implementation coverage varies/unknown. |
| 70 | NoseFlaw | Directory present; implementation coverage varies/unknown. |
| 71 | PerspectiveTransform | Directory present; implementation coverage varies/unknown. |
| 72 | PetBrightEye | Directory present; implementation coverage varies/unknown. |
| 73 | PoseRefine | Directory present; implementation coverage varies/unknown. |
| 74 | PostCropVignetting | Directory present; implementation coverage varies/unknown. |
| 75 | PursedLipsSmile | Directory present; implementation coverage varies/unknown. |
| 76 | RealGrain | Directory present; implementation coverage varies/unknown. |
| 77 | RedEyeReduction | Directory present; implementation coverage varies/unknown. |
| 78 | RemoveBlueVeins | Directory present; implementation coverage varies/unknown. |
| 79 | RemoveDoubleEyelid | Directory present; implementation coverage varies/unknown. |
| 80 | RemoveEyeBag | Directory present; implementation coverage varies/unknown. |
| 81 | RemoveGingiva | Directory present; implementation coverage varies/unknown. |
| 82 | RemoveHair | Directory present; implementation coverage varies/unknown. |
| 83 | RemovePetHair | Directory present; implementation coverage varies/unknown. |
| 84 | RemoveStubble | Directory present; implementation coverage varies/unknown. |
| 85 | RemoveTanLines | Directory present; implementation coverage varies/unknown. |
| 86 | RemoveWhiteHair | Directory present; implementation coverage varies/unknown. |
| 87 | RepairBreast | Directory present; implementation coverage varies/unknown. |
| 88 | SkinAdjust | Directory present; implementation coverage varies/unknown. |
| 89 | SkinLighten | Directory present; implementation coverage varies/unknown. |
| 90 | SkinTexture | Directory present; implementation coverage varies/unknown. |
| 91 | SkinUnifyColorPicker | Directory present; implementation coverage varies/unknown. |
| 92 | SmileMouth | Directory present; implementation coverage varies/unknown. |
| 93 | SmileMouth3D | Directory present; implementation coverage varies/unknown. |
| 94 | StretchMark | Directory present; implementation coverage varies/unknown. |
| 95 | TeethWhitenV3 | Directory present; implementation coverage varies/unknown. |
| 96 | ToothBeauty | Directory present; implementation coverage varies/unknown. |
| 97 | ToothRepairing | Directory present; implementation coverage varies/unknown. |
| 98 | WHairSkinCpu | Directory present; implementation coverage varies/unknown. |
| 99 | Wrinkle | Directory present; implementation coverage varies/unknown. |
| 100 | WrinkleV1 | Directory present; implementation coverage varies/unknown. |

## Appendix B. Completed targeted function-range ledger

This is a ledger of selected static ranges, not a complete source-function inventory. Pseudocode completion does not resolve every private type, helper or model. Counts below come from the saved Ghidra result and independent reference cross-check.

| Static start | Label evidence | Instructions | Confirmed literal links | Pseudocode produced |
|---|---|---:|---:|---|
| `0x143748ba0` | RawProcessor::_GetAutoWhiteBalance: hasProcessedAiData is false | 1363 | 4 | Yes |
| `0x143749f60` | RawProcessor::_GetAutoWhiteBalance: hasProcessedAiData is false | 1459 | 9 | Yes |
| `0x146b5c0b0` | EffectFaceDermabrasionHLF::Render GetSkinMask failed. | 127 | 1 | Yes |
| `0x147ae70a0` | NoseFlawRemovalPipeline Failed to load G1 model from buffer | 459 | 6 | Yes |
| `0x147ae5920` | NoseFlawRemovalPipeline::check_data_valid::Instance mask is not valid, skip face_id=%d | 576 | 4 | Yes |
| `0x143ac9d80` | Export CPU composition_face: makeupProtect image is empty | 1678 | 1 | Yes |
| `0x143acb7d0` | CPU composition_face: makeupProtect mask is empty | 2192 | 1 | Yes |
| `0x143a8d810` | HAVE_FACE_PARSING_MASK_TEXTURE | 253 | 2 | Yes |
| `0x143a3f2b0` | EffectMatteRefinement::EnsureResultImage: removeBGEdge failed, keep old cache. | 558 | 2 | Yes |
| `0x146b623b0` | SkinUnifyPipeline | 39 | 1 | Yes |
| `0x143a4f660` | EffectMatteRefinementMix::Render: invalid result texture. | 499 | 2 | Yes |
| `0x1436c84a0` | failed to load AutoWBPipeline model from file %s. | 205 | 1 | Yes |
| `0x1436c8810` | failed to load AutoWBPipeline_RAW model from file %s. | 205 | 1 | Yes |
| `0x147a9f8c0` | AdultFaceBlemishPipeline::Total Skin Mask data is invalid, return invalid input | 309 | 2 | Yes |
| `0x147a9fd80` | AdultFaceBlemishPipeline::Total Skin Mask data is invalid, return invalid input | 364 | 2 | Yes |
| `0x147a9b0d0` | AdultFaceBlemishPipeline::initCache::Face(%d) Face Landmark data is invalid, skip | 1149 | 13 | Yes |
| `0x147a9a8d0` | AdultFaceBlemishPipeline::inferenceBlemish::input tensor channel isn't 3 | 178 | 2 | Yes |
| `0x147a9adf0` | AdultFaceBlemishPipeline::inferenceNevus::input tensor channel isn't 3 | 174 | 2 | Yes |
| `0x147a9abc0` | AdultFaceBlemishPipeline::inferenceFreckleMask::input tensor channel isn't 4 | 126 | 2 | Yes |
| `0x147a9a560` | AdultFaceBlemishPipeline::inferenceAcneMask::input tensor channel isn't 3 | 206 | 2 | Yes |
| `0x147a9d7e0` | AdultFaceBlemishPipeline::postProcessBlemish:: blemish_output valid. | 481 | 2 | Yes |
| `0x147a9e050` | AdultFaceBlemishPipeline::postProcessFreckleMask:: m_blemish_data_valid is false. | 531 | 4 | Yes |
| `0x147a9d180` | AdultFaceBlemishPipeline::postProcessAcneMask:: acne_mask_output valid. | 271 | 2 | Yes |
| `0x147aa29a0` | NewBornFaceBlemishPipelineV2::faceFeatureProtect::Face(%d) FaceParsingV2 data is invalid, skip | 986 | 4 | Yes |
| `0x147a99e40` | caller candidate of AdultFaceBlemishPipeline::inferenceBlemish::input tensor channel isn't 3 | 416 | 0 | Yes |
| `0x147a9ea30` | caller candidate of AdultFaceBlemishPipeline::postProcessBlemish:: blemish_output valid. | 647 | 0 | Yes |
| `0x147aa1f00` | caller candidate of NewBornFaceBlemishPipelineV2::faceFeatureProtect::Face(%d) FaceParsingV2 data is invalid, skip | 115 | 0 | Yes |
| `0x146b37db0` | .?AVDarkCirclesV1Pipeline@TSAI@@ vtable slot 0 | 38 | 0 | Yes |
| `0x147b0fcc0` | .?AVDarkCirclesV1Pipeline@TSAI@@ vtable slot 1 | 180 | 0 | Yes |
| `0x147b0faa0` | .?AVDarkCirclesV1Pipeline@TSAI@@ vtable slot 2 | 149 | 0 | Yes |
| `0x147b13bd0` | .?AVDarkCirclesV1Pipeline@TSAI@@ vtable slot 3 | 82 | 0 | Yes |
| `0x146b698f0` | DarkCirclesV1Pipeline | 39 | 1 | Yes |
| `0x146ba0840` | EyebagV1Pipeline | 39 | 1 | Yes |

Final source-integrity check: both installed executable SHA-256 values match the initial audit. The continuation did not patch the installed Evoto binaries. Hashes of the report, analysis helpers and retained JSON/TSV evidence are in `.work-checks/evoto-deep-2026-10-18/evidence-manifest.json`.
