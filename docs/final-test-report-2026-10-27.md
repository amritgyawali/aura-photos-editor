# AURA final test - every tool, 2026-10-27

Tested on `feat/match-a-look-full-look` at `4d181f2`, in the real AURA desktop window (debug build of
that commit with the UI embedded, ONNX Runtime and the learned models attached as the installed app
has them). Every window run used an isolated test catalogue; the photographer's own catalogue was
never opened, and every original file was byte-identical afterwards.

## 1. The verdict

AURA's tools **run, are reversible and are safe**: 137 of 137 hand-placed retouch tools and effects,
108 of 108 Portrait-studio sliders, 54 Develop checks and 122 colour checks ran without an error;
strength 0 is an exact no-op for every tool; Undo and Redo restore every edit exactly; not one pixel
of any tool's change landed beyond 1.6 x its selection; sliders built for people leave a photograph with nobody in it untouched; and
the window showed no page error in 721 checks.

What is **not** yet right falls in three groups:

1. **Defects a photographer will see** - thirteen, four of them high, listed in section 3 with the
   cause in the code and a fix. The worst: teeth whitening turns lips magenta, sky selection picks
   trees and banners, the editing preview never shows Dehaze, and one-click finishing withholds crowd
   photographs whose small background faces cannot be measured.
2. **Features that are placeholders by design** - People, Emotion, scene classes, Autopilot,
   restoration, object cleanup, gallery and camera matching run end to end and decide nothing on a
   real wedding, because their models are untrained. The most useful single change in the product
   is in section 4: the face detector that works (portrait retouch's) is not the one those features
   use.
3. **Things this machine could not prove** - export through the window (the trial has expired),
   and the positive case of tools whose defect never appeared in the test photographs (beards,
   open smiles, red-eye). Section 6 lists them.

| Area | Works | Works, nothing to fix there | Works, with an issue | Defect | Not proven / placeholder |
|---|---|---|---|---|---|
| Retouch tools and effects (137) | 113 | 13 | 8 | 3 | 0 |
| Portrait studio sliders (108) | 101 | 0 | 2 | 1 | 4 |

## 2. How it was tested

| Run | What it did | Result |
|---|---|---|
| UI unit tests (`vitest`) | every component, 123 files | **1,158 / 1,158 pass** |
| Rust workspace tests | 238 test binaries, all crates except `aura-perf` | **4,743 pass, 0 fail**, 61 ignored (real-photo harnesses that need inputs) |
| Performance budgets (`aura-perf`, `AURA_PERF_HOST_SCALE=6`) | every wall-clock and storage budget, 26 test binaries | **116 / 116 pass** |
| Phase gates (`scripts/check-phase-gates.sh`) | all 31 phase gates (30 planned phases and phase 31) | **31 / 31 gates pass** |
| Whole-product window test (`scripts/test-everything-gui.py`) | 13 photographs (JPEG, PNG, 2 Canon CR2, 1 DNG), every workspace, every slider at both ends, every tool, every analysis pass, export, one-click, Autopilot | **721 checks: 639 pass, 26 warn, 5 fail, 51 skip** |
| Deep per-tool test (new, real wedding portraits) | 8 Canon CR3 wedding frames; each of 137 tools placed **on the feature it is for** (eye, lash line, brow, mouth, cheek, under-eye, face, body, background) at strength 0, 0.6 and 1.0; each of 108 Portrait-studio sliders at its top value on a 34 %-of-frame face | 137 / 137 tools and 108 / 108 sliders ran; findings below |
| Retest | every tool that did nothing, re-placed where it has something to act on (skin sample, brightest point, tinted backdrop, open eyes and mouths) | 3 defects confirmed (teeth whitening, anamorphic streaks, eyeliner wing), 5 tools confirmed too weak (three sampled-skin tools, sun rays, god rays), 7 cleared (colour cast, auto levels, bokeh, sparkle, highlight glow, catchlight, studio catchlights) |
| AI-selection check (new) | 12 selections on 4 CR3 frames and an indoor stage photo, drawn as overlays and judged by eye | sky defect confirmed; person selections accurate |
| Face detection check (new) | 8 CR3 frames against what is in them | 14 of about 18 visible faces found, 1 false face, 1 doubtful, 1.2-2.3 s each |

Evidence: `.work-checks/final-2026-10-27/` (logs, the window run's `everything-full/results.json`),
`H:/aura-final-deep/` (per-tool crops, `deep.json`), `H:/aura-final-retest/`, `H:/aura-final-masks/`.
The deep, retest and mask scripts are in this session's scratchpad.

**Caveats.** The window run's Develop timings were taken while the Rust tests compiled with four
jobs. The window harness's "bride" photographs turned out to be a bird, dolphins, hands and a
lighthouse - only one has a detected face, and it is 0.87 % of the frame - so its 225 Portrait-studio
checks prove the controls run and undo, not how they look. The deep test on real wedding portraits
exists to close that gap.

## 3. Defects - what to fix, in order

| # | Severity | Tool | Expected | Actual (measured) | Cause | Fix |
|---|---|---|---|---|---|---|
| 1 | High | **One-click "Finish a folder"** | every edited photograph is delivered | 3 of 13 photographs **withheld from delivery** - the Kerala temple crowd, a night procession, a stage panel - with "Face 2 has insufficient geometry or resolution for the contour check" | `crates/aura-app/src/portrait_preflight.rs:98-126` requires a complete contour reading for **every saved face**, whether or not anything was done to it | require readings only for faces with an enabled `auto-portrait-v1-{face}-` operation; record the others as "not retouched, not checked" |
| 2 | High | **Dehaze** (Develop) | the editing preview shows it | **no change in the preview at any zoom**; the exported photo moves 16.8 levels | `PhotoStudio.tsx:134-135` renders with `purpose: 'interactive'`; `aura-render/src/graph.rs:439` skips Dehaze for that purpose; the returned skip reason is never shown | include Dehaze in the settled (full-quality) preview - ADR-0126 made it a 512 px dark-channel base, so the original cost argument no longer holds - or at least show "Dehaze appears in the export" |
| 3 | High | **Natural teeth whitening** (Retouch) | whiter teeth; lips untouched | on three closed or smiling mouths **the lips turned magenta** (blue +9 to +15 levels, red unchanged) | `crates/aura-render/src/retouch_tools.rs:747-756` adds 0.7 x the yellow excess to blue on every pixel in the ellipse | weight each pixel by tooth likelihood (bright for the mouth, low saturation, not lip-red); never push blue past neutral; reuse the automatic retouch's closed-mouth rule |
| 4 | High | **AI selection: Sky** (Masks) | sky, or nothing | **33 % of a portrait with no sky** (the dark trees behind the bride) and **24.5 % of an indoor stage** (a white banner) | sky accepted without a plausibility check | require sky to be bright, low in texture and connected to the top edge; refuse below a confidence; say "no sky found" (it already says nothing when it finds none) |
| 5 | Medium | **Flash red-eye correction** | only a red pupil changes | red-brown eyeshadow on a closed eye turned **olive-green** | the gate at `retouch_tools.rs:769` (`B >= 0.5 G`) admits eyeshadow and skin in shadow | require a dark, saturated, roughly round component (`B >= 0.8 G`, luma below the iris median); fade out beyond it |
| 6 | Medium | **Anamorphic streaks** (Effects) | horizontal streaks from lights | **0 pixels changed**, even centred on the brightest point of two photos, at default settings | `crates/aura-render/src/retouch_effects/lens.rs:121` threshold `0.5 + (1 - tone) x 1.5` is 1.25 at the default tone - above any display-referred pixel; the unit test uses a value of 10 | map tone to about 0.95 down to 0.35 linear and add a test on a real photograph. Check **Sun rays** and **God rays** for the same threshold: both moved under 1 level at a bright point |
| 7 | Medium | **Eyeliner wing** (Portrait studio) | a wing at the outer corner | **0 pixels** at 100 on three photographs, two with open eyes (Eyeliner itself: 437-1,037 px on the same faces) | the wing's placement gate never passes on these faces | find why the wing is never placed; add a real-photo case to the evoto100 harness |
| 8 | Medium | **Skin smoothing / Even sampled skin / Skin dodge & burn** | visible smoothing at 60 % | with a correct skin sample: 28-155 pixels, **0.3-0.7 levels** - invisible | the default sample tolerance (0.08) admits almost none of real, shaded skin | set the tolerance from the sample's own spread, or default it much higher; show how much of the selection matched |
| 9 | Medium | **Face detection** (portrait) | every face, nothing else | the groom in sunglasses missed; one **false face on a white saree** (989A3150) | Haar eye evidence fails behind sunglasses; the fabric passed the frontal cascade | require skin evidence inside an accepted box; for sunglasses, accept a frontal face with skin, mouth and nose but no eyes. A false face is worse than a miss - automatic retouch would smooth the fabric |
| 10 | Low | **Sclera redness reduction** | only the whites of the eyes | eyeshadow inside the ellipse shifts slightly green | no sclera gate | intersect with the sclera region the portrait parse already finds |
| 11 | Low | **Camera names** | "Canon EOS 5D Mark IV" | "Canon **Canon** EOS 5D Mark IV", "Canon Canon EOS 20D" in integrity and camera matching | make prepended to a model string that already contains it | drop the make when the model already starts with it |
| 12 | Low | **HSL magenta and blue** | each band about as strong as the others | magenta hue moves only +/-6-8 deg (red: +/-45 deg); magenta saturation +/-0.12-0.15; blue saturation +100 only +0.13 | band widths or gains | compare band responses on a chart and even them out |
| 13 | Low | **Lens distortion / chromatic aberration** switches | something happens, or a reason | silent no-op on a JPEG with no lens profile | nothing to apply, and nothing says so | show "no profile for this lens" beside the switch |

Smaller usability points found on the way: **Colourise** recolours skin inside the ellipse (add a
"protect skin" default); **Solid background colour** without a selection paints a translucent blob (it
should ask for a background selection first); **Light direction / height** and **Relight direction**
do nothing until Key light / Relight is on (grey them out until then); **Hair definition** and **Hair
edge** are close to inert at 100; **Auto edit** that judges a photo already balanced changes nothing
and should say so.

## 4. The structural finding - two face detectors

Portrait retouch uses `aura-portrait`'s detector, which works on real photographs (section 5.7). People,
Emotion, Cull and Curation use phase 06's `scan_faces`, whose model is a placeholder: on the 13 test
photographs it produced **832 "faces"** (64 per photo, including the bird and the dolphins), all gated
out, so **0 identities**, an emotion score computed over boxes that are not faces, and a cull and
curation that cannot reason about people. Feeding `aura-portrait`'s faces into `PeopleService` would
make People, Emotion, the couple's coverage in the cull and the album's people terms work on a real
wedding without training anything - the single change that would bring the most of AURA to life.

The other analysis passes behave as their exit reports say: scene classes are all `unknown` (so
exposure targets, tone intent and composition rules fall back to neutral), composition finds a
"horizon" in every photo, integrity calls 12 of 13 sharp photos soft, restoration and cleanup act on
nothing, and Autopilot completes "degraded" with every deciding step - export included - waiting for a
person (selected 0, exported 0). None of that is a crash; all of it means the button does nothing a
photographer can see, and the Autopilot panel should say so before it is pressed.

## 5. Every tool, one by one

Columns: what the whole-product window run found (tools placed at one fixed spot of one photograph);
what the deep run found with the tool on its proper target in a real wedding portrait (pixels changed,
mean change inside the selection, whether strength 0 was an exact no-op, share of the change outside
1.6 x the selection, time to save and re-render a 2048 px preview); the verdict; and, where there is
one, the expected-versus-actual and the fix.

### 5.1 Retouch tools and effects (137)

| # | Tool | Group | Window run (fixed spot) | Real portrait, on its target | Verdict | Expected vs actual, fix |
|---|---|---|---|---|---|---|
| 1 | Local saturation & vibrance (`color_saturation`) | Color | PASS: 41107 pixels changed | face: 151,233 px changed, mean 4.88 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 2 | Acne & blemish clear · brush (`acne_clear`) | Repair | PASS: 1010 pixels changed | cheek: 1,047 px changed, mean 2.96 levels in the selection; 0 % = no-op yes; leak 0%; 2.36 s | Works |  |
| 3 | Heal blemish / flyaway / lint (`heal`) | Repair | PASS: 30999 pixels changed | cheek: 2,043 px changed, mean 13.66 levels in the selection; 0 % = no-op yes; leak 0%; 2.19 s | Works |  |
| 4 | Texture-aware patch heal (`patch_heal`) | Repair | PASS: 38969 pixels changed | cheek: 1,631 px changed, mean 7.49 levels in the selection; 0 % = no-op yes; leak 0%; 2.08 s | Works |  |
| 5 | Frequency healing · marks (`frequency_heal`) | Repair | WARN | cheek: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 2.16 s | Works (nothing to fix here) | Found no marks on clean, made-up bridal skin - correct. Positive case proven only by unit tests on painted marks in this run. |
| 6 | Clone stamp (`clone`) | Repair | PASS: 29219 pixels changed | cheek: 1,772 px changed, mean 8.82 levels in the selection; 0 % = no-op yes; leak 0%; 2.02 s | Works |  |
| 7 | Auto spot cleanup (`auto_blemish`) | Skin | WARN | cheek: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works (nothing to fix here) | No dark spots on this skin - correct. Positive case covered by unit tests only. |
| 8 | Frequency separation (`frequency`) | Skin | PASS: 15769 pixels changed | cheek: 756 px changed, mean 1.84 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 9 | Restore skin texture (`texture_graft`) | Skin | PASS: 6258 pixels changed | cheek: 558 px changed, mean 1.65 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works |  |
| 10 | Skin smoothing · protect detail (`skin_smooth`) | Sampled skin | PASS: 2870 pixels changed | cheek: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 2.06 s | Works, with an issue | Needs a skin sample (the UI asks for one). With a proper sample on the cheek it changed only 155 pixels, 0.7 levels mean at 60 % - invisible. The default tolerance (0.08) admits almost none of real, shaded skin. Fix: measure tolerance from the sample's own spread, or default it higher. |
| 11 | Even sampled skin tone (`skin_uniformity`) | Sampled skin | PASS: 3460 pixels changed | cheek: 0 px changed, mean 0.01 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works, with an issue | Same as skin smoothing: with a sample, 45 pixels and 0.6 levels at 60 % - invisible on real skin. |
| 12 | Skin dodge and burn · protect edges (`portrait_dodge_burn`) | Sampled skin | PASS: 2257 pixels changed | cheek: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 1.92 s | Works, with an issue | Same as skin smoothing: with a sample, 28 pixels and 0.3 levels at 60 %. |
| 13 | Micro dodge and burn (`micro_dodge_burn`) | Light | PASS: 14135 pixels changed | cheek: 780 px changed, mean 2.06 levels in the selection; 0 % = no-op yes; leak 0%; 2.08 s | Works |  |
| 14 | Dodge / highlight sculpting (`dodge`) | Light | PASS: 40385 pixels changed | cheek: 2,107 px changed, mean 14.17 levels in the selection; 0 % = no-op yes; leak 0%; 2.05 s | Works |  |
| 15 | Burn / shadow sculpting (`burn`) | Light | PASS: 38433 pixels changed | cheek: 1,958 px changed, mean 9.76 levels in the selection; 0 % = no-op yes; leak 0%; 2.09 s | Works |  |
| 16 | Skin color correction (`skin_color`) | Color | PASS: 37366 pixels changed | cheek: 827 px changed, mean 1.71 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 17 | Match complexion to sample (`color_match`) | Color | PASS: 18040 pixels changed | cheek: 1,949 px changed, mean 8.68 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 18 | Reduce oily shine (`mattify`) | Skin | PASS: 6213 pixels changed | cheek: 165 px changed, mean 0.75 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 19 | Under-eye shadow lift (`under_eye`) | Skin | PASS: 423 pixels changed | under: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works (nothing to fix here) | Eyes closed and the under-eye already even - nothing to lift. Positive case not exercised on a real photo in this run. |
| 20 | Soften fine lines (`wrinkle`) | Skin | PASS: 17640 pixels changed | cheek: 818 px changed, mean 1.96 levels in the selection; 0 % = no-op yes; leak 0%; 2.08 s | Works |  |
| 21 | Stretch-mark softening (`stretch_marks`) | Body | PASS: 4442 pixels changed | body: 20,236 px changed, mean 0.5 levels in the selection; 0 % = no-op yes; leak 0%; 2.28 s | Works |  |
| 22 | Natural teeth whitening (`teeth`) | Details | PASS: 23310 pixels changed | mouth: 3,203 px changed, mean 13.21 levels in the selection; 0 % = no-op yes; leak 0%; 1.92 s | **Defect** | Expected: whiter teeth, lips untouched. Actual: on three closed or smiling mouths the lips turned magenta (blue +9 to +15 levels, red unchanged). Cause: retouch_tools.rs:747 adds 0.7 x the yellow excess to blue on every pixel in the ellipse. Fix: weight each pixel by tooth likelihood (bright relative to the mouth, low saturation, not lip-red) and never push blue past neutral. |
| 23 | Eye redness reduction (`eye_clean`) | Details | WARN | eye: 2,198 px changed, mean 5.75 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works, with an issue | Expected: only the whites of the eye lose redness. Actual: eyeshadow inside the ellipse shifts slightly green. Fix: weight by sclera likelihood (bright, low saturation) or intersect with the sclera region the portrait parse already finds. |
| 24 | Iris and eyelash detail (`eye_detail`) | Details | PASS: 12669 pixels changed | eye: 1,912 px changed, mean 7.38 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 25 | Flash red-eye correction (`red_eye`) | Details | WARN | eye: 1,857 px changed, mean 8.84 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | **Defect** | Expected: only a red pupil changes. Actual: red-brown eyeshadow on a closed eye turned olive-green. Cause: the gate at retouch_tools.rs:769 (B >= 0.5 G) admits eyeshadow and skin shadow. Fix: require a dark, saturated, round component (B >= 0.8 G, luma below the iris median) and fade to nothing outside it. |
| 26 | Fabric crease softening (`fabric`) | Cleanup | PASS: 19997 pixels changed | body: 159,103 px changed, mean 6.29 levels in the selection; 0 % = no-op yes; leak 0%; 2.2 s | Works |  |
| 27 | Fabric moiré reduction (`fabric_moire`) | Clothing | PASS: 13151 pixels changed | body: 119,292 px changed, mean 4.37 levels in the selection; 0 % = no-op yes; leak 0%; 2.08 s | Works |  |
| 28 | Fabric stain correction (`fabric_stain`) | Clothing | PASS: 31172 pixels changed | body: 230,047 px changed, mean 9.14 levels in the selection; 0 % = no-op yes; leak 0%; 2.05 s | Works |  |
| 29 | Shadow balancing (`shadow_balance`) | Light | PASS: 10778 pixels changed | face: 125,041 px changed, mean 7.99 levels in the selection; 0 % = no-op yes; leak 0%; 2.02 s | Works |  |
| 30 | Background blur (`background_blur`) | Background | PASS: 15822 pixels changed | background: 16,818 px changed, mean 1.34 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 31 | Highlight glow (`highlight_glow`) | Effects | PASS: 1439 pixels changed | face: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works | No change on a face (no highlight above the threshold); at the brightest point it acts (985 to 12,970 px). |
| 32 | Directional motion blur (`motion_blur`) | Effects | PASS: 5471 pixels changed | face: 28,363 px changed, mean 1.32 levels in the selection; 0 % = no-op yes; leak 0%; 2.02 s | Works |  |
| 33 | Radial zoom blur (`zoom_blur`) | Effects | PASS: 2655 pixels changed | face: 83,373 px changed, mean 3.87 levels in the selection; 0 % = no-op yes; leak 0%; 2.08 s | Works |  |
| 34 | Color spill suppression (`color_despill`) | Color | PASS: 43804 pixels changed | background: 2,885 px changed, mean 0.35 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 35 | Highlight compression (`highlight_compress`) | Light | PASS: 1659 pixels changed | face: 833 px changed, mean 0.04 levels in the selection; 0 % = no-op yes; leak 0%; 2.05 s | Works |  |
| 36 | Backdrop smoothing (`backdrop`) | Cleanup | PASS: 26575 pixels changed | background: 49,215 px changed, mean 3.75 levels in the selection; 0 % = no-op yes; leak 0%; 2.08 s | Works |  |
| 37 | Defringe · purple and green edges (`defringe`) | Cleanup | WARN | face: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works (nothing to fix here) | No coloured fringes on this frame - correct restraint. |
| 38 | Glare softening (`glare`) | Details | PASS: 6213 pixels changed | eye: 0 px changed, mean 0.11 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works (nothing to fix here) | No glare in the eye - correct. |
| 39 | Local cosmetic tint (`makeup`) | Color | PASS: 37366 pixels changed | cheek: 827 px changed, mean 1.71 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works | Subtle warm tint as described. |
| 40 | Hair, clothes & makeup color (`colorize`) | Color | PASS: 45834 pixels changed | body: 271,591 px changed, mean 28.04 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works, with an issue | Works, but recolours everything in the ellipse - on a torso it recoloured hands and bangles with the saree. Add 'protect skin' on by default. |
| 41 | Solid background color (`background_color`) | Cleanup | PASS: 44539 pixels changed | background: 86,486 px changed, mean 24.47 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works, with an issue | Without a background selection it paints a translucent, feathered blob, not a solid background. The panel should require a background mask (one click from the AI selection) before it applies. |
| 42 | Local face & body proportions (`reshape`) | Shape | PASS: 9084 pixels changed | face: 82,963 px changed, mean 3.23 levels in the selection; 0 % = no-op yes; leak 0%; 1.92 s | Works |  |
| 43 | Liquify · push (`liquify_push`) | Liquify | PASS: 26034 pixels changed | face: 156,027 px changed, mean 19.5 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works |  |
| 44 | Liquify · enlarge (eyes, hair volume) (`liquify_bloat`) | Liquify | PASS: 22274 pixels changed | face: 154,716 px changed, mean 13.44 levels in the selection; 0 % = no-op yes; leak 0%; 1.94 s | Works |  |
| 45 | Liquify · shrink (nose, waist) (`liquify_pucker`) | Liquify | PASS: 22474 pixels changed | face: 154,666 px changed, mean 13.61 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 46 | Liquify · twirl (`liquify_twirl`) | Liquify | PASS: 28307 pixels changed | face: 168,173 px changed, mean 19.31 levels in the selection; 0 % = no-op yes; leak 0%; 2.08 s | Works |  |
| 47 | Spin blur (`spin_blur`) | Blur & optics | PASS: 8942 pixels changed | face: 155,623 px changed, mean 9.5 levels in the selection; 0 % = no-op yes; leak 0%; 2.02 s | Works |  |
| 48 | Pixelate (privacy) (`pixelate`) | Blur & optics | PASS: 9385 pixels changed | face: 68,357 px changed, mean 2.73 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 49 | Blur (privacy / soften) (`privacy_blur`) | Blur & optics | PASS: 16515 pixels changed | face: 98,745 px changed, mean 4.43 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 50 | Soft focus · diffusion filter (`soft_focus`) | Blur & optics | PASS: 13513 pixels changed | face: 63,674 px changed, mean 3.48 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works |  |
| 51 | Orton glow · dreamy (`orton_glow`) | Blur & optics | PASS: 41793 pixels changed | face: 192,173 px changed, mean 23.13 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 52 | Sharpen (local) (`local_sharpen`) | Blur & optics | PASS: 14533 pixels changed | face: 107,430 px changed, mean 6.39 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 53 | Noise reduction (local) (`local_denoise`) | Blur & optics | PASS: 13714 pixels changed | face: 45,313 px changed, mean 2.03 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 54 | Hue shift (`hue_shift`) | Colour | PASS: 44507 pixels changed | body: 251,013 px changed, mean 16.87 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 55 | Invert colours (`invert_colors`) | Colour | PASS: 44920 pixels changed | face: 212,010 px changed, mean 46.78 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 56 | Green screen · replace colour (`chroma_key`) | Colour | PASS: 44694 pixels changed | background: 26,327 px changed, mean 1.6 levels in the selection; 0 % = no-op yes; leak 0%; 3.23 s | Works |  |
| 57 | Reduce visible veins (`vein_reduce`) | Skin | PASS: 8644 pixels changed | cheek: 681 px changed, mean 2.35 levels in the selection; 0 % = no-op yes; leak 0%; 2.02 s | Works |  |
| 58 | Unify colour (hair, clothes) (`color_unify`) | Colour | PASS: 38112 pixels changed | body: 227,939 px changed, mean 12.87 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 59 | Local white balance (`local_white_balance`) | Colour | PASS: 44315 pixels changed | background: 90,931 px changed, mean 27.07 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works |  |
| 60 | Light leak (`light_leak`) | Light & effects | PASS: 42863 pixels changed | face: 55,414 px changed, mean 1.6 levels in the selection; 0 % = no-op yes; leak 0%; 2.02 s | Works |  |
| 61 | Glitch (`glitch`) | Light & effects | PASS: 28158 pixels changed | face: 142,087 px changed, mean 7.94 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works |  |
| 62 | Haze · fog (`haze`) | Light & effects | PASS: 43724 pixels changed | background: 80,952 px changed, mean 13.65 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works |  |
| 63 | Sun rays · starburst (`starburst`) | Light & effects | PASS: 11076 pixels changed | face: 0 px changed, mean 0.02 levels in the selection; 0 % = no-op yes; leak 0%; 2.05 s | Works, with an issue | Expected: visible rays from a light. Actual: under 1 level of mean change even at the brightest point of two photos. Probably the same scene-referred threshold as the streak tool; check and rescale for display-referred photographs. |
| 64 | Film halation (`halation`) | Light & effects | PASS: 4433 pixels changed | face: 224 px changed, mean 0.02 levels in the selection; 0 % = no-op yes; leak 0%; 2.08 s | Works |  |
| 65 | Add catchlight (`catchlight`) | Details | PASS: 45776 pixels changed | eye: 2,378 px changed, mean 15.7 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works | Acts on open eyes (+25 to +40 levels in a small disc). The light takes the chosen light colour; the brown disc first seen came from the test's colour input, not the tool. Improvement: a slight radial falloff instead of a flat fill would read more like a reflection. |
| 66 | Pet eye / eye glow fix (`eye_glow_fix`) | Details | PASS: 33699 pixels changed | eye: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 2.12 s | Works (nothing to fix here) | No glowing pupil - correct; its own gate protects skin (better than red-eye's). |
| 67 | Lip gloss (`lip_gloss`) | Details | PASS: 3011 pixels changed | mouth: 376 px changed, mean 1.12 levels in the selection; 0 % = no-op yes; leak 0%; 2.06 s | Works |  |
| 68 | Add freckles (`freckles`) | Skin | PASS: 1200 pixels changed | cheek: 54 px changed, mean 0.15 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works |  |
| 69 | Floor reflection (`floor_reflection`) | Scene | PASS: 23968 pixels changed | face: 170,424 px changed, mean 10.92 levels in the selection; 0 % = no-op yes; leak 0%; 2.11 s | Works |  |
| 70 | Drop shadow (`drop_shadow`) | Scene | PASS: 32454 pixels changed | face: 109,728 px changed, mean 4.3 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 71 | Dust & scratches (`dust_scratches`) | Fix & clean up | PASS: 561 pixels changed | background: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 2.02 s | Works (nothing to fix here) | A clean CR3 background has no dust - correct. |
| 72 | JPEG artifact cleanup (`jpeg_cleanup`) | Fix & clean up | PASS: 66 pixels changed | background: 0 px changed, mean 0.08 levels in the selection; 0 % = no-op yes; leak 0%; 2.2 s | Works (nothing to fix here) | A RAW has no JPEG blocks - correct. |
| 73 | Remove banding (`deband`) | Fix & clean up | PASS: 496 pixels changed | background: 0 px changed, mean 0.02 levels in the selection; 0 % = no-op yes; leak 0%; 2.58 s | Works (nothing to fix here) | No banding in this background - correct. |
| 74 | Auto levels (selection) (`auto_levels`) | Fix & clean up | PASS: 40984 pixels changed | background: 68,094 px changed, mean 8.08 levels in the selection; 0 % = no-op yes; leak 0%; 2.08 s | Works | Also verified on the stage backdrop (18.7 levels mean). |
| 75 | Remove colour cast (selection) (`auto_neutral`) | Fix & clean up | PASS: 43981 pixels changed | background: 0 px changed, mean 0.4 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works | No change on a neutral background; on the red stage backdrop it removed the cast (20 levels mean). |
| 76 | Even out uneven lighting (`flatten_light`) | Fix & clean up | PASS: 27946 pixels changed | background: 52,758 px changed, mean 3.4 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works |  |
| 77 | HDR detail · structure (`hdr_detail`) | Fix & clean up | PASS: 28367 pixels changed | face: 121,105 px changed, mean 6.2 levels in the selection; 0 % = no-op yes; leak 0%; 2.02 s | Works |  |
| 78 | Bokeh highlights (`bokeh_highlights`) | Creative looks | PASS: 1176 pixels changed | background: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 1.94 s | Works | Nothing on a dull background; on bright points it makes discs (5,418 to 20,722 px, up to 8 levels). |
| 79 | Tilt-shift · miniature (`tilt_shift`) | Creative looks | PASS: 38055 pixels changed | face: 120,738 px changed, mean 2.63 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works |  |
| 80 | Duotone (`duotone`) | Creative looks | PASS: 42151 pixels changed | face: 196,041 px changed, mean 24.52 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 81 | Posterize (`posterize`) | Creative looks | PASS: 40876 pixels changed | face: 136,864 px changed, mean 4.35 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 82 | Halftone print (`halftone`) | Creative looks | PASS: 44820 pixels changed | face: 197,096 px changed, mean 36.72 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 83 | Pencil sketch (`pencil_sketch`) | Creative looks | PASS: 44646 pixels changed | face: 213,462 px changed, mean 53.35 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 84 | Oil paint (`oil_paint`) | Creative looks | PASS: 9447 pixels changed | face: 80,046 px changed, mean 2.83 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works | Mild at 60 % (looks like smoothing more than brush strokes); stronger at 100 %. |
| 85 | Cross-process (`cross_process`) | Creative looks | PASS: 41402 pixels changed | face: 159,581 px changed, mean 6.47 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 86 | Infrared (`infrared`) | Creative looks | PASS: 44177 pixels changed | face: 193,003 px changed, mean 22.92 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 87 | Mirror symmetry (`mirror_symmetry`) | Creative looks | PASS: 11926 pixels changed | face: 96,653 px changed, mean 7.71 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 88 | Lens flare (`lens_flare`) | Creative looks | PASS: 39783 pixels changed | face: 15,819 px changed, mean 1.05 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works | Faint on a face with no light source; place it on a light. |
| 89 | God rays (`god_rays`) | Creative looks | PASS: 1527 pixels changed | face: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 2.09 s | Works, with an issue | Very weak on real photographs (under 1 level mean at the brightest point). Rescale its threshold as for the streak tool. |
| 90 | Snow & rain (`weather`) | Creative looks | PASS: 696 pixels changed | face: 3,434 px changed, mean 0.23 levels in the selection; 0 % = no-op yes; leak 0%; 1.94 s | Works |  |
| 91 | Split tone (`split_tone`) | Colour looks | PASS: 39426 pixels changed | face: 145,574 px changed, mean 4.31 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works |  |
| 92 | Film fade · matte (`film_fade`) | Colour looks | PASS: 42581 pixels changed | face: 184,265 px changed, mean 10.68 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 93 | Bleach bypass (`bleach_bypass`) | Colour looks | PASS: 43245 pixels changed | face: 181,991 px changed, mean 13.41 levels in the selection; 0 % = no-op yes; leak 0%; 2.2 s | Works |  |
| 94 | Teal & orange (`teal_orange`) | Colour looks | PASS: 23725 pixels changed | face: 247 px changed, mean 0.4 levels in the selection; 0 % = no-op yes; leak 0%; 1.92 s | Works |  |
| 95 | Colour splash (`colour_splash`) | Colour looks | PASS: 1 pixels changed | face: 64,673 px changed, mean 4.24 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works |  |
| 96 | Restore blue sky (`sky_blue`) | Fix & clean up | PASS: 71 pixels changed | background: 68,328 px changed, mean 8.33 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works |  |
| 97 | Spotlight (`spotlight`) | Light & effects | PASS: 34550 pixels changed | face: 178,000 px changed, mean 15.91 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 98 | Rim light (`rim_light`) | Light & effects | PASS: 17453 pixels changed | face: 33,336 px changed, mean 0.82 levels in the selection; 0 % = no-op yes; leak 0%; 2.05 s | Works |  |
| 99 | Chromatic aberration (creative) (`chromatic_aberration`) | Light & effects | PASS: 6754 pixels changed | face: 32,647 px changed, mean 1.28 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 100 | Fisheye (`fisheye`) | Liquify | PASS: 20826 pixels changed | face: 163,526 px changed, mean 13.52 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 101 | Anamorphic streaks (`anamorphic_streak`) | Light & effects | PASS: 2764 pixels changed | face: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | **Defect** | Expected: horizontal streaks from bright points. Actual: 0 pixels changed even centred on the brightest point of two photos. Cause: lens.rs:121 threshold = 0.5 + (1 - tone) x 1.5 in linear light, 1.25 at the UI default tone 0.5 - above any display-referred pixel. The unit test passes only with a value of 10. Fix: map tone to about 0.95 down to 0.35 linear, and test on a real photograph. |
| 102 | Kaleidoscope (`kaleidoscope`) | Creative looks | PASS: 25560 pixels changed | face: 165,541 px changed, mean 12.81 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 103 | Watercolour (`watercolor`) | Creative looks | PASS: 42522 pixels changed | face: 188,154 px changed, mean 13.36 levels in the selection; 0 % = no-op yes; leak 0%; 2.11 s | Works |  |
| 104 | Stained glass (`stained_glass`) | Creative looks | PASS: 32015 pixels changed | face: 126,867 px changed, mean 7.79 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 105 | Cartoon (`cartoon`) | Creative looks | PASS: 36177 pixels changed | face: 134,970 px changed, mean 6.32 levels in the selection; 0 % = no-op yes; leak 0%; 2.09 s | Works |  |
| 106 | Neon edges (`neon_edges`) | Creative looks | PASS: 40820 pixels changed | face: 182,591 px changed, mean 20.53 levels in the selection; 0 % = no-op yes; leak 0%; 2.12 s | Works |  |
| 107 | Thermal camera (`thermal`) | Creative looks | PASS: 45610 pixels changed | face: 208,107 px changed, mean 41.41 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 108 | Repair blown highlights (`clipped_highlight_repair`) | Fix & clean up | PASS: 135 pixels changed | cheek: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works (nothing to fix here) | Nothing clipped on the cheek - correct. |
| 109 | Dehaze (selection) (`local_dehaze`) | Fix & clean up | PASS: 43910 pixels changed | background: 72,317 px changed, mean 6.75 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works |  |
| 110 | Limbal ring (`limbal_ring`) | Details | PASS: 16797 pixels changed | eye: 663 px changed, mean 1.1 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 111 | Darken lashes & liner (`lash_darken`) | Details | PASS: 3598 pixels changed | lash: 621 px changed, mean 2.99 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works |  |
| 112 | Fill thin hair or brows (`hair_fill`) | Details | PASS: 10533 pixels changed | brow: 1,126 px changed, mean 4.23 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works |  |
| 113 | Match grain (`match_grain`) | Skin | PASS: 345 pixels changed | cheek: 0 px changed, mean 0.22 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works (nothing to fix here) | Untouched skin already has its own grain - correct. |
| 114 | Channel swap (`channel_swap`) | Colour looks | PASS: 45036 pixels changed | face: 196,896 px changed, mean 27.96 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works |  |
| 115 | Sunset glow (`sunset_glow`) | Light & effects | PASS: 33536 pixels changed | face: 40,568 px changed, mean 1.23 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 116 | Day to night (`day_to_night`) | Scene | PASS: 40531 pixels changed | background: 84,277 px changed, mean 16.76 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works |  |
| 117 | Lomo (`lomo`) | Colour looks | PASS: 41684 pixels changed | face: 173,083 px changed, mean 8.92 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 118 | Instant film (`instant_film`) | Colour looks | PASS: 35384 pixels changed | face: 154,967 px changed, mean 5.29 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 119 | Pop art (`pop_art`) | Creative looks | PASS: 44630 pixels changed | face: 203,522 px changed, mean 36.94 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 120 | Iris blur (`iris_blur`) | Blur & optics | PASS: 9276 pixels changed | face: 30,422 px changed, mean 1.52 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 121 | Double exposure echo (`echo_double`) | Creative looks | PASS: 40247 pixels changed | face: 167,481 px changed, mean 11.4 levels in the selection; 0 % = no-op yes; leak 0%; 2.12 s | Works |  |
| 122 | Ripple (`ripple`) | Liquify | PASS: 9890 pixels changed | face: 51,797 px changed, mean 2.08 levels in the selection; 0 % = no-op yes; leak 0%; 2.02 s | Works |  |
| 123 | Frosted glass (`frosted_glass`) | Blur & optics | PASS: 14185 pixels changed | face: 76,247 px changed, mean 3.36 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 124 | Pixel sort (`pixel_sort`) | Creative looks | PASS: 13071 pixels changed | face: 98,966 px changed, mean 7.96 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 125 | VHS tape (`vhs_retro`) | Creative looks | PASS: 40809 pixels changed | face: 151,567 px changed, mean 7.03 levels in the selection; 0 % = no-op yes; leak 0%; 2.02 s | Works |  |
| 126 | Emboss (`emboss`) | Creative looks | PASS: 42424 pixels changed | face: 191,478 px changed, mean 15.94 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 127 | Crosshatch (`crosshatch`) | Creative looks | PASS: 44638 pixels changed | face: 207,747 px changed, mean 50.68 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 128 | Sparkle (`sparkle`) | Light & effects | PASS: 425 pixels changed | face: 369 px changed, mean 0.04 levels in the selection; 0 % = no-op yes; leak 0%; 2.17 s | Works | Also verified on the brightest points of two photos. |
| 129 | Fairy lights (`fairy_lights`) | Light & effects | PASS: 4737 pixels changed | background: 9,134 px changed, mean 0.9 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works |  |
| 130 | Rainbow prism (`rainbow_prism`) | Light & effects | PASS: 10284 pixels changed | face: 20,701 px changed, mean 2.2 levels in the selection; 0 % = no-op yes; leak 0%; 1.95 s | Works |  |
| 131 | Soften body hair (`body_hair_soften`) | Skin | WARN | body: 11,485 px changed, mean 0.81 levels in the selection; 0 % = no-op yes; leak 0%; 2.03 s | Works |  |
| 132 | Smooth cellulite (`cellulite`) | Skin | PASS: 6340 pixels changed | body: 8,941 px changed, mean 0.21 levels in the selection; 0 % = no-op yes; leak 0%; 2.0 s | Works |  |
| 133 | Even tan lines (`tan_line`) | Skin | PASS: 37823 pixels changed | body: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works (nothing to fix here) | No tan line in the selection - correct. |
| 134 | Rejuvenate hands (`hand_rejuvenate`) | Skin | PASS: 8379 pixels changed | body: 22,052 px changed, mean 0.86 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 135 | Even dark knuckles & elbows (`knuckle_even`) | Skin | PASS: 5276 pixels changed | body: 21,298 px changed, mean 0.59 levels in the selection; 0 % = no-op yes; leak 0%; 2.08 s | Works |  |
| 136 | Remove glasses reflection colour (`lens_reflection`) | Details | PASS: 20559 pixels changed | eye: 1,037 px changed, mean 2.02 levels in the selection; 0 % = no-op yes; leak 0%; 1.98 s | Works |  |
| 137 | Remove a cast shadow (`shadow_remove`) | Fix & clean up | WARN | body: 0 px changed, mean 0.0 levels in the selection; 0 % = no-op yes; leak 0%; 1.97 s | Works (nothing to fix here) | No cast shadow in the selection - correct. |

### 5.2 Portrait studio sliders (108)

Every slider was also run at both ends in the window run on a photograph with a small face; there,
all two-sided sliders passed with exact Undo, 51 one-sided sliders (0-100) correctly refused a
negative value without changing anything, and the nine "no person" checks left a photo with nobody in
it untouched.

| # | Slider | Window run (both ends, small face) | Real portrait at the top value | Verdict | Expected vs actual, fix |
|---|---|---|---|---|---|
| 1 | `face.slim` | -100 PASS, +100 PASS | 2.94% of frame changed, 0% of it outside the head area; 2.66 s | Works |  |
| 2 | `face.jaw` | -100 PASS, +100 PASS | 1.06% of frame changed, 0% of it outside the head area; 2.0 s | Works |  |
| 3 | `face.chin` | -100 PASS, +100 PASS | 0.65% of frame changed, 0% of it outside the head area; 2.03 s | Works |  |
| 4 | `face.forehead` | -100 PASS, +100 PASS | 1.74% of frame changed, 0% of it outside the head area; 2.0 s | Works |  |
| 5 | `face.cheekbones` | -100 PASS, +100 PASS | 0.89% of frame changed, 0% of it outside the head area; 2.03 s | Works |  |
| 6 | `face.eyeSize` | -100 PASS, +100 PASS | 0.79% of frame changed, 0% of it outside the head area; 2.11 s | Works |  |
| 7 | `face.eyeDistance` | -100 PASS, +100 PASS | 0.67% of frame changed, 0% of it outside the head area; 2.05 s | Works |  |
| 8 | `face.noseWidth` | -100 PASS, +100 PASS | 0.20% of frame changed, 0% of it outside the head area; 2.0 s | Works |  |
| 9 | `face.noseLength` | -100 PASS, +100 PASS | 0.27% of frame changed, 0% of it outside the head area; 2.14 s | Works |  |
| 10 | `face.mouthWidth` | -100 PASS, +100 PASS | 0.34% of frame changed, 0% of it outside the head area; 2.06 s | Works |  |
| 11 | `face.lips` | -100 PASS, +100 PASS | 0.22% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 12 | `face.smile` | -100 PASS, +100 PASS | 0.21% of frame changed, 0% of it outside the head area; 2.05 s | Works |  |
| 13 | `face.headSize` | -100 PASS, +100 PASS | 6.86% of frame changed, 0% of it outside the head area; 2.05 s | Works |  |
| 14 | `face.eyeHeight` | -100 PASS, +100 PASS | 0.60% of frame changed, 0% of it outside the head area; 2.05 s | Works |  |
| 15 | `face.eyeTilt` | -100 PASS, +100 PASS | 0.42% of frame changed, 0% of it outside the head area; 2.0 s | Works |  |
| 16 | `face.innerCorner` | -100 PASS, +100 PASS | 0.06% of frame changed, 0% of it outside the head area; 2.0 s | Works |  |
| 17 | `face.outerCorner` | -100 PASS, +100 PASS | 0.07% of frame changed, 0% of it outside the head area; 2.05 s | Works |  |
| 18 | `face.eyeSymmetry` | -100 SKIP, +100 WARN | 0.04% of frame changed, 0% of it outside the head area; 1.98 s | Works |  |
| 19 | `face.noseBridge` | -100 PASS, +100 PASS | 0.11% of frame changed, 0% of it outside the head area; 2.05 s | Works |  |
| 20 | `face.noseTip` | -100 PASS, +100 PASS | 0.04% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 21 | `face.noseLift` | -100 PASS, +100 PASS | 0.08% of frame changed, 0% of it outside the head area; 2.0 s | Works |  |
| 22 | `face.nostrils` | -100 PASS, +100 PASS | 0.04% of frame changed, 0% of it outside the head area; 2.09 s | Works |  |
| 23 | `face.cheekApples` | -100 PASS, +100 PASS | 0.54% of frame changed, 0% of it outside the head area; 2.06 s | Works |  |
| 24 | `face.upperLip` | -100 PASS, +100 PASS | 0.13% of frame changed, 0% of it outside the head area; 2.06 s | Works |  |
| 25 | `face.lowerLip` | -100 PASS, +100 PASS | 0.12% of frame changed, 0% of it outside the head area; 2.0 s | Works |  |
| 26 | `face.mouthHeight` | -100 PASS, +100 PASS | 0.27% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 27 | `face.browHeight` | -100 PASS, +100 PASS | 0.36% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 28 | `face.browTilt` | -100 PASS, +100 PASS | 0.13% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 29 | `face.browThickness` | -100 PASS, +100 PASS | 0.14% of frame changed, 0% of it outside the head area; 2.05 s | Works |  |
| 30 | `face.temples` | -100 PASS, +100 PASS | 0.58% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 31 | `face.faceLength` | -100 PASS, +100 PASS | 1.00% of frame changed, 0% of it outside the head area; 2.03 s | Works |  |
| 32 | `face.chinWidth` | -100 PASS, +100 PASS | 0.15% of frame changed, 0% of it outside the head area; 2.06 s | Works |  |
| 33 | `face.hairVolume` | -100 PASS, +100 PASS | 2.73% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 34 | `body.slim` | -100 PASS, +100 PASS | 61.90% of frame changed, 71% of it outside the head area; 2.61 s | Works |  |
| 35 | `body.waist` | -100 PASS, +100 PASS | 7.40% of frame changed, 100% of it outside the head area; 2.42 s | Works |  |
| 36 | `body.hips` | -100 PASS, +100 PASS | 7.32% of frame changed, 100% of it outside the head area; 2.33 s | Works |  |
| 37 | `body.arms` | -100 PASS, +100 PASS | 4.55% of frame changed, 100% of it outside the head area; 2.31 s | Works |  |
| 38 | `body.shoulders` | -100 PASS, +100 PASS | 5.04% of frame changed, 72% of it outside the head area; 2.44 s | Works |  |
| 39 | `body.legs` | -100 PASS, +100 PASS | 23.14% of frame changed, 100% of it outside the head area; 2.38 s | Works |  |
| 40 | `body.neck` | -100 PASS, +100 PASS | 7.33% of frame changed, 0% of it outside the head area; 2.38 s | Works |  |
| 41 | `body.thighs` | -100 PASS, +100 PASS | 6.16% of frame changed, 100% of it outside the head area; 2.34 s | Works |  |
| 42 | `body.calves` | -100 PASS, +100 PASS | 3.75% of frame changed, 100% of it outside the head area; 2.36 s | Works |  |
| 43 | `body.belly` | -100 PASS, +100 PASS | 5.77% of frame changed, 100% of it outside the head area; 2.38 s | Works |  |
| 44 | `body.hipLift` | -100 PASS, +100 PASS | 7.50% of frame changed, 100% of it outside the head area; 2.42 s | Works |  |
| 45 | `body.shoulderSlope` | -100 PASS, +100 PASS | 2.95% of frame changed, 69% of it outside the head area; 2.38 s | Works |  |
| 46 | `body.torso` | -100 PASS, +100 PASS | 43.13% of frame changed, 95% of it outside the head area; 2.41 s | Works |  |
| 47 | `body.height` | -100 PASS, +100 PASS | 44.26% of frame changed, 94% of it outside the head area; 2.36 s | Works |  |
| 48 | `body.neckWidth` | -100 PASS, +100 PASS | 0.40% of frame changed, 0% of it outside the head area; 2.34 s | Works |  |
| 49 | `body.doubleChin` | -100 SKIP, +100 PASS | 0.23% of frame changed, 0% of it outside the head area; 2.36 s | Works |  |
| 50 | `body.headLevel` | -100 SKIP, +100 PASS | 9.17% of frame changed, 0% of it outside the head area; 2.45 s | Works |  |
| 51 | `skin.aegyoSal` | -100 SKIP, +100 PASS | 0.10% of frame changed, 0% of it outside the head area; 2.14 s | Works |  |
| 52 | `skin.lipLines` | -100 SKIP, +100 PASS | 0.06% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 53 | `skin.glow` | -100 SKIP, +100 PASS | 0.58% of frame changed, 0% of it outside the head area; 2.06 s | Works |  |
| 54 | `skin.stubble` | -100 SKIP, +100 PASS | 0.05% of frame changed, 0% of it outside the head area; 2.05 s | Works |  |
| 55 | `skin.rosy` | -100 SKIP, +100 PASS | 2.48% of frame changed, 0% of it outside the head area; 2.03 s | Works |  |
| 56 | `skin.teethEvening` | -100 SKIP, +100 WARN | 0.01% of frame changed, 0% of it outside the head area; 2.03 s | Runs; not proven | 0 to 21 px: none of the test faces shows teeth. Re-test on an open smile. |
| 57 | `skin.featureSharpen` | -100 SKIP, +100 PASS | 0.38% of frame changed, 0% of it outside the head area; 2.09 s | Works |  |
| 58 | `skin.beardDefine` | -100 SKIP, +100 WARN | 0.00% of frame changed, 0% of it outside the head area; 2.03 s | Runs; not proven | No beard in any test photo. |
| 59 | `skin.colourSpill` | -100 SKIP, +100 PASS | 1.02% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 60 | `skin.pores` | -100 SKIP, +100 PASS | 0.02% of frame changed, 0% of it outside the head area; 2.0 s | Works |  |
| 61 | `makeup.eyeliner` | -100 SKIP, +100 PASS | 0.03% of frame changed, 0% of it outside the head area; 2.0 s | Works |  |
| 62 | `makeup.eyelinerWing` | -100 SKIP, +100 WARN | 0.00% of frame changed, 0% of it outside the head area; 2.0 s | **Defect** | Expected: a winged liner at the outer corner. Actual: 0 pixels changed at 100 on three photographs, two with open eyes (Eyeliner itself changes 437 to 1,037 px on the same faces). Investigate the wing's placement gate. |
| 63 | `makeup.lashes` | -100 SKIP, +100 PASS | 0.03% of frame changed, 0% of it outside the head area; 2.03 s | Works |  |
| 64 | `makeup.lowerLashes` | -100 SKIP, +100 PASS | 0.03% of frame changed, 0% of it outside the head area; 2.09 s | Works |  |
| 65 | `makeup.lipLiner` | -100 SKIP, +100 PASS | 0.07% of frame changed, 0% of it outside the head area; 2.0 s | Works |  |
| 66 | `makeup.lipOmbre` | -100 SKIP, +100 PASS | 0.14% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 67 | `makeup.highlighter` | -100 SKIP, +100 PASS | 0.11% of frame changed, 0% of it outside the head area; 1.98 s | Works |  |
| 68 | `makeup.bronzer` | -100 SKIP, +100 PASS | 0.52% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 69 | `makeup.enhance` | -100 SKIP, +100 PASS | 0.17% of frame changed, 0% of it outside the head area; 2.05 s | Works |  |
| 70 | `makeup.browFill` | -100 SKIP, +100 PASS | 0.14% of frame changed, 0% of it outside the head area; 2.03 s | Works |  |
| 71 | `hair.gloss` | -100 SKIP, +100 PASS | 1.77% of frame changed, 0% of it outside the head area; 2.06 s | Works |  |
| 72 | `hair.parting` | -100 SKIP, +100 PASS | 0.05% of frame changed, 0% of it outside the head area; 2.05 s | Works | Small by nature (0.05 %). |
| 73 | `hair.richness` | -100 SKIP, +100 PASS | 2.19% of frame changed, 0% of it outside the head area; 1.97 s | Works |  |
| 74 | `hair.edge` | -100 SKIP, +100 PASS | 0.00% of frame changed, 0% of it outside the head area; 2.55 s | Works, with an issue | Effectively inert at 100 (0.003 % of the frame). |
| 75 | `hair.definition` | -100 SKIP, +100 PASS | 1.03% of frame changed, 99% of it outside the head area; 2.08 s | Works, with an issue | Changes 1 % of the frame, almost all of it away from the head (hair strands on the shoulders and veil); no visible change on the hair itself at 100. |
| 76 | `attire.gold` | -100 SKIP, +100 PASS | 3.54% of frame changed, 46% of it outside the head area; 2.11 s | Works |  |
| 77 | `attire.redRecovery` | -100 SKIP, +100 WARN | 0.37% of frame changed, 6% of it outside the head area; 2.11 s | Works |  |
| 78 | `attire.veilClarity` | -100 SKIP, +100 PASS | 2.97% of frame changed, 0% of it outside the head area; 2.02 s | Works |  |
| 79 | `attire.embroidery` | -100 SKIP, +100 PASS | 10.92% of frame changed, 83% of it outside the head area; 1.98 s | Works |  |
| 80 | `attire.garmentEdge` | -100 SKIP, +100 PASS | 1.66% of frame changed, 90% of it outside the head area; 2.02 s | Works |  |
| 81 | `attire.mehndi` | -100 SKIP, +100 PASS | 1.03% of frame changed, 95% of it outside the head area; 2.14 s | Works |  |
| 82 | `attire.whiteGarment` | -100 SKIP, +100 PASS | 0.76% of frame changed, 85% of it outside the head area; 2.03 s | Works |  |
| 83 | `attire.darkGarment` | -100 SKIP, +100 PASS | 0.02% of frame changed, 100% of it outside the head area; 2.05 s | Works | No dark garment in the frame - tiny change is correct. |
| 84 | `attire.outfitColour` | -100 SKIP, +100 PASS | 22.99% of frame changed, 82% of it outside the head area; 1.98 s | Works |  |
| 85 | `scene.backdropGlow` | -100 SKIP, +100 PASS | 13.40% of frame changed, 40% of it outside the head area; 2.52 s | Works |  |
| 86 | `scene.saturation` | -100 PASS, +100 PASS | 49.73% of frame changed, 83% of it outside the head area; 2.31 s | Works |  |
| 87 | `scene.brightness` | -100 PASS, +100 PASS | 50.64% of frame changed, 83% of it outside the head area; 2.31 s | Works |  |
| 88 | `scene.warmth` | -100 PASS, +100 PASS | 49.53% of frame changed, 84% of it outside the head area; 2.3 s | Works |  |
| 89 | `scene.depthBlur` | -100 SKIP, +100 PASS | 43.53% of frame changed, 85% of it outside the head area; 2.69 s | Works |  |
| 90 | `scene.denoise` | -100 SKIP, +100 PASS | 0.09% of frame changed, 73% of it outside the head area; 2.48 s | Works |  |
| 91 | `scene.lightWrap` | -100 SKIP, +100 PASS | 6.26% of frame changed, 57% of it outside the head area; 2.44 s | Works |  |
| 92 | `subject.exposure` | -100 PASS, +100 PASS | 52.16% of frame changed, 65% of it outside the head area; 2.34 s | Works |  |
| 93 | `subject.warmth` | -100 PASS, +100 PASS | 51.23% of frame changed, 65% of it outside the head area; 2.28 s | Works |  |
| 94 | `subject.clarity` | -100 PASS, +100 PASS | 44.05% of frame changed, 65% of it outside the head area; 2.36 s | Works |  |
| 95 | `subject.groupBalance` | -100 SKIP, +100 WARN | 0.00% of frame changed, 0% of it outside the head area; 1.95 s | Runs; not proven | A one-person portrait; needs a group photo with uneven faces. |
| 96 | `subject.groupSkin` | -100 SKIP, +100 WARN | 0.00% of frame changed, 0% of it outside the head area; 1.97 s | Runs; not proven | As group balance. |
| 97 | `subject.relight` | -100 SKIP, +100 PASS | 7.12% of frame changed, 58% of it outside the head area; 2.48 s | Works |  |
| 98 | `subject.relightDirection` | -100 WARN, +100 WARN | 7.12% of frame changed, 58% of it outside the head area; 2.33 s (measured against subject.relight=100) | Works | Works with Relight on (7.1 %); grey it out while Relight is 0. |
| 99 | `subject.harshShadows` | -100 SKIP, +100 PASS | 1.24% of frame changed, 0% of it outside the head area; 1.98 s | Works |  |
| 100 | `subject.backlight` | -100 SKIP, +100 WARN | 49.78% of frame changed, 65% of it outside the head area; 2.31 s | Works |  |
| 101 | `subject.faceVignette` | -100 SKIP, +100 PASS | 62.97% of frame changed, 90% of it outside the head area; 2.38 s | Works |  |
| 102 | `subject.faceBrightness` | -100 PASS, +100 PASS | 5.16% of frame changed, 0% of it outside the head area; 1.97 s | Works |  |
| 103 | `light.key` | -100 SKIP, +100 PASS | 5.63% of frame changed, 0% of it outside the head area; 1.98 s | Works |  |
| 104 | `light.direction` | -180 WARN, +180 WARN | 5.77% of frame changed, 0% of it outside the head area; 1.94 s (measured against light.key=100) | Works | Does nothing alone (the window run's warning); with Key light on it moves the light (5.8 % of the frame). |
| 105 | `light.height` | -30 WARN, +80 WARN | 5.23% of frame changed, 0% of it outside the head area; 2.08 s (measured against light.key=100) | Works | As Direction: works once Key light is on. The panel should grey it out while Key light is 0. |
| 106 | `light.fill` | -100 SKIP, +100 PASS | 0.75% of frame changed, 0% of it outside the head area; 2.06 s | Works |  |
| 107 | `light.catchlights` | -100 SKIP, +100 PASS | 0.00% of frame changed, 0% of it outside the head area; 1.98 s | Works | Nothing on the downcast eyes here (correct - no iris to reflect in); 38 to 107 px on open eyes in the retest. |
| 108 | `light.glow` | -100 SKIP, +100 PASS | 35.27% of frame changed, 70% of it outside the head area; 2.11 s | Works |  |

### 5.3 Develop controls, edit profiles and white balance

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | slider global.exposure | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 2 | slider global.temperature | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 3 | slider global.tint | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 4 | slider global.contrast | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 5 | slider global.highlights | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 6 | slider global.shadows | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 7 | slider global.whites | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 8 | slider global.blacks | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 9 | slider global.texture | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 10 | slider global.clarity | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 11 | slider global.dehaze | WARN | no visible change at this value | **Defect** | Expected: the editing preview shows dehaze. Actual: no change in the preview at any zoom; the export changes by 16.8 levels. Cause: PhotoStudio renders with purpose 'interactive' and graph.rs:439 skips Dehaze for that purpose; the skip reason is never shown. Fix: render dehaze in the settled preview (it is a 512 px dark-channel base, ADR-0126) or at least show 'Dehaze is applied in the export'. |
| 12 | slider global.vibrance | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 13 | slider global.saturation | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 14 | slider global.hsl.red.h | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 15 | slider global.hsl.orange.s | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 16 | slider global.hsl.blue.l | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 17 | slider global.curve.points | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 18 | slider global.channel_curves.red.points | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 19 | slider global.parametric.lights | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 20 | slider global.sharpen.amount | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 21 | slider global.noise.luminance | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 22 | slider global.noise.colour | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 23 | slider global.effects.vignette.amount | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 24 | slider global.effects.grain.amount | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 25 | slider global.calibration.red_hue | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 26 | slider global.colour_grade.shadows.saturation | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 27 | slider bw | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 28 | slider geometry.rotate | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 29 | slider geometry.crop | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 30 | slider lens.distortion | WARN | no visible change at this value | Works, with an issue | Silent no-op on a JPEG with no lens profile and no measured coefficients. Show 'no profile for this lens' beside the switch. |
| 31 | slider lens.ca | WARN | no visible change at this value | Works, with an issue | As distortion: nothing measured on this frame, nothing changes, nothing is said. |
| 32 | slider lens.vignette | PASS | changes the photograph; Undo restores it exactly | Works |  |
| 33 | out-of-range value refused | PASS | clamped to the allowed range | Works |  |
| 34 | dehaze in the export render | PASS | mean change 16.78 levels in the export render | Works |  |
| 35 | snapshots | PASS | take and restore | Works |  |
| 36 | white balance picker | PASS | neutral spot accepted (8930adca61); coloured spot refused | Works |  |
| 37 | edit profile: True Natural | PASS | 33 fields; Colour boost halved because the photograph is already vivid. | Works |  |
| 38 | edit profile: Light & Airy | PASS | 57 fields; Colour boost halved because the photograph is already vivid. | Works |  |
| 39 | edit profile: Dark & Moody | PASS | 57 fields;  | Works |  |
| 40 | edit profile: Cinematic Teal & Orange | PASS | 58 fields; Colour boost halved because the photograph is already vivid. | Works |  |
| 41 | edit profile: Portra Film | PASS | 57 fields;  | Works |  |
| 42 | edit profile: Vintage Fade | PASS | 56 fields;  | Works |  |
| 43 | edit profile: Golden Hour Glow | PASS | 50 fields; Colour boost halved because the photograph is already vivid. | Works |  |
| 44 | edit profile: Classic Monochrome | PASS | 65 fields;  | Works |  |
| 45 | edit profile: Film Noir | PASS | 51 fields;  | Works |  |
| 46 | edit profile: Vivid Landscape | PASS | 43 fields; Colour boost halved because the photograph is already vivid. | Works |  |
| 47 | edit profile: Soft Portrait | PASS | 39 fields; Colour boost halved because the photograph is already vivid. | Works |  |
| 48 | edit profile: Neon Night | PASS | 52 fields; Cooling reduced to 20% because the light is already cool., Colour boost halved because the photograph is already vivid. | Works |  |
| 49 | edit profile: Editorial Matte | PASS | 52 fields; Colour boost halved because the photograph is already vivid. | Works |  |
| 50 | edit profile: Fresh & Crisp | PASS | 44 fields; Colour boost halved because the photograph is already vivid. | Works |  |
| 51 | edit profile: Romantic Wedding | PASS | 49 fields; Colour boost halved because the photograph is already vivid. | Works |  |
| 52 | edit profile: Nordic Cool | PASS | 54 fields; Cooling reduced to 20% because the light is already cool., Colour boost halved because the photograph is already vivid. | Works |  |
| 53 | profile preview | PASS | 10287 | Works |  |
| 54 | sync settings to another photo | PASS | synced 1 | Works |  |

### 5.4 Colour accuracy checks (HSL, curves, grading, calibration) on a colour chart

Each check measures the right patch moving the right way, the opposite band and the greys **not**
moving, and Undo restoring the chart exactly.

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | HSL red h -100 | PASS | hue -40.7 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 2 | HSL red h +100 | PASS | hue +48.5 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 3 | HSL red s -100 | PASS | saturation -0.643; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 4 | HSL red s +100 | PASS | saturation +0.299; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 5 | HSL red l -100 | PASS | luma -25.3; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 6 | HSL red l +100 | PASS | luma +18.8; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 7 | HSL orange h -100 | PASS | hue -45.0 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 8 | HSL orange h +100 | PASS | hue +28.8 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 9 | HSL orange s -100 | PASS | saturation -0.485; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 10 | HSL orange s +100 | PASS | saturation +0.300; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 11 | HSL orange l -100 | PASS | luma -26.4; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 12 | HSL orange l +100 | PASS | luma +21.2; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 13 | HSL yellow h -100 | PASS | hue -11.7 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 14 | HSL yellow h +100 | PASS | hue +14.0 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 15 | HSL yellow s -100 | PASS | saturation -0.326; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 16 | HSL yellow s +100 | PASS | saturation +0.292; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 17 | HSL yellow l -100 | PASS | luma -14.2; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 18 | HSL yellow l +100 | PASS | luma +11.8; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 19 | HSL green h -100 | PASS | hue -21.4 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 20 | HSL green h +100 | PASS | hue +18.0 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 21 | HSL green s -100 | PASS | saturation -0.196; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 22 | HSL green s +100 | PASS | saturation +0.299; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 23 | HSL green l -100 | PASS | luma -7.4; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 24 | HSL green l +100 | PASS | luma +7.0; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 25 | HSL aqua h -100 | PASS | hue -19.1 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 26 | HSL aqua h +100 | PASS | hue +34.2 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 27 | HSL aqua s -100 | PASS | saturation -0.687; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 28 | HSL aqua s +100 | PASS | saturation +0.294; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 29 | HSL aqua l -100 | PASS | luma -44.9; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 30 | HSL aqua l +100 | PASS | luma +30.9; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 31 | HSL blue h -100 | PASS | hue -44.5 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 32 | HSL blue h +100 | PASS | hue +52.5 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 33 | HSL blue s -100 | PASS | saturation -0.261; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 34 | HSL blue s +100 | PASS | saturation +0.132; opposite band 0.0, greys 0.0 levels; Undo exact | Works, with an issue | +0.13 against about +0.30 for other bands. |
| 35 | HSL blue l -100 | PASS | luma -13.5; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 36 | HSL blue l +100 | PASS | luma +11.0; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 37 | HSL purple h -100 | PASS | hue -68.9 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 38 | HSL purple h +100 | PASS | hue +33.9 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 39 | HSL purple s -100 | PASS | saturation -0.372; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 40 | HSL purple s +100 | PASS | saturation +0.304; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 41 | HSL purple l -100 | PASS | luma -17.9; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 42 | HSL purple l +100 | PASS | luma +14.2; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 43 | HSL magenta h -100 | PASS | hue -8.3 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works, with an issue | Only -8 deg (red band moves +/-45 deg). Magenta hue and saturation (+/-0.12 to 0.15) and blue saturation +100 (+0.13) are much weaker than the other bands. |
| 44 | HSL magenta h +100 | PASS | hue +6.1 deg; opposite band 0.0, greys 0.0 levels; Undo exact | Works, with an issue | Only +6 deg - see above. |
| 45 | HSL magenta s -100 | PASS | saturation -0.117; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 46 | HSL magenta s +100 | PASS | saturation +0.153; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 47 | HSL magenta l -100 | PASS | luma -5.2; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 48 | HSL magenta l +100 | PASS | luma +4.8; opposite band 0.0, greys 0.0 levels; Undo exact | Works |  |
| 49 | temperature 2000 K | PASS | cooler; Undo exact | Works |  |
| 50 | temperature 50000 K | PASS | warmer; Undo exact | Works |  |
| 51 | tint -150 | PASS | greener; Undo exact | Works |  |
| 52 | tint +150 | PASS | more magenta; Undo exact | Works |  |
| 53 | saturation -100 | PASS | grey; Undo exact | Works |  |
| 54 | saturation +100 | PASS | more saturated; Undo exact | Works |  |
| 55 | vibrance -100 | PASS | less saturated; Undo exact | Works |  |
| 56 | vibrance +100 | PASS | more saturated; Undo exact | Works |  |
| 57 | exposure -5 | PASS | darker; Undo exact | Works |  |
| 58 | exposure +5 | PASS | brighter; Undo exact | Works |  |
| 59 | contrast -100 | PASS | flatter; Undo exact | Works |  |
| 60 | contrast +100 | PASS | punchier; Undo exact | Works |  |
| 61 | highlights -100 | PASS | brights darker; Undo exact | Works |  |
| 62 | highlights +100 | PASS | brights brighter; Undo exact | Works |  |
| 63 | shadows -100 | PASS | darks darker; Undo exact | Works |  |
| 64 | shadows +100 | PASS | darks brighter; Undo exact | Works |  |
| 65 | whites -100 | PASS | whites darker; Undo exact | Works |  |
| 66 | whites +100 | PASS | whites brighter; Undo exact | Works |  |
| 67 | blacks -100 | PASS | blacks darker; Undo exact | Works |  |
| 68 | blacks +100 | PASS | blacks brighter; Undo exact | Works |  |
| 69 | red curve lifted | PASS | redder greys; Undo exact | Works |  |
| 70 | green curve lifted | PASS | greener greys; Undo exact | Works |  |
| 71 | blue curve lifted | PASS | bluer greys; Undo exact | Works |  |
| 72 | tone curve lifted | PASS | brighter mid greys; Undo exact | Works |  |
| 73 | grade shadows blue | PASS | darks bluer than brights; Undo exact | Works |  |
| 74 | grade highlights orange | PASS | brights warmer than darks; Undo exact | Works |  |
| 75 | grade midtones green | PASS | mid greys greener; Undo exact | Works |  |
| 76 | grade global magenta | PASS | greys more magenta; Undo exact | Works |  |
| 77 | grade shadows luminance -100 | PASS | darker; Undo exact | Works |  |
| 78 | grade shadows luminance +100 | PASS | brighter; Undo exact | Works |  |
| 79 | grade midtones luminance -100 | PASS | darker; Undo exact | Works |  |
| 80 | grade midtones luminance +100 | PASS | brighter; Undo exact | Works |  |
| 81 | grade highlights luminance -100 | PASS | darker; Undo exact | Works |  |
| 82 | grade highlights luminance +100 | PASS | brighter; Undo exact | Works |  |
| 83 | grade global luminance -100 | PASS | darker; Undo exact | Works |  |
| 84 | grade global luminance +100 | PASS | brighter; Undo exact | Works |  |
| 85 | global.calibration.shadows_tint -100 | PASS | acts; Undo exact | Works |  |
| 86 | global.calibration.shadows_tint +100 | PASS | acts; Undo exact | Works |  |
| 87 | global.calibration.red_hue -100 | PASS | acts; Undo exact | Works |  |
| 88 | global.calibration.red_hue +100 | PASS | acts; Undo exact | Works |  |
| 89 | global.calibration.red_saturation -100 | PASS | acts; Undo exact | Works |  |
| 90 | global.calibration.red_saturation +100 | PASS | acts; Undo exact | Works |  |
| 91 | global.calibration.green_hue -100 | PASS | acts; Undo exact | Works |  |
| 92 | global.calibration.green_hue +100 | PASS | acts; Undo exact | Works |  |
| 93 | global.calibration.green_saturation -100 | PASS | acts; Undo exact | Works |  |
| 94 | global.calibration.green_saturation +100 | PASS | acts; Undo exact | Works |  |
| 95 | global.calibration.blue_hue -100 | PASS | acts; Undo exact | Works |  |
| 96 | global.calibration.blue_hue +100 | PASS | acts; Undo exact | Works |  |
| 97 | global.calibration.blue_saturation -100 | PASS | acts; Undo exact | Works |  |
| 98 | global.calibration.blue_saturation +100 | PASS | acts; Undo exact | Works |  |
| 99 | global.parametric.highlights -100 | PASS | acts; Undo exact | Works |  |
| 100 | global.parametric.highlights +100 | PASS | acts; Undo exact | Works |  |
| 101 | global.parametric.lights -100 | PASS | acts; Undo exact | Works |  |
| 102 | global.parametric.lights +100 | PASS | acts; Undo exact | Works |  |
| 103 | global.parametric.darks -100 | PASS | acts; Undo exact | Works |  |
| 104 | global.parametric.darks +100 | PASS | acts; Undo exact | Works |  |
| 105 | global.parametric.shadows -100 | PASS | acts; Undo exact | Works |  |
| 106 | global.parametric.shadows +100 | PASS | acts; Undo exact | Works |  |
| 107 | global.clarity -100 | PASS | acts; Undo exact | Works |  |
| 108 | global.clarity +100 | PASS | acts; Undo exact | Works |  |
| 109 | global.texture -100 | PASS | acts; Undo exact | Works |  |
| 110 | global.texture +100 | PASS | acts; Undo exact | Works |  |
| 111 | global.dehaze -100 | PASS | acts; Undo exact | Works |  |
| 112 | global.dehaze +100 | PASS | acts; Undo exact | Works |  |
| 113 | global.sharpen.amount +150 | PASS | acts; Undo exact | Works |  |
| 114 | global.noise.luminance +100 | PASS | acts; Undo exact | Works |  |
| 115 | global.noise.colour +100 | PASS | acts; Undo exact | Works |  |
| 116 | global.effects.vignette.amount -100 | PASS | acts; Undo exact | Works |  |
| 117 | global.effects.vignette.amount +100 | PASS | acts; Undo exact | Works |  |
| 118 | global.effects.grain.amount +100 | PASS | acts; Undo exact | Works |  |
| 119 | grade blending 0 | PASS | changes an active grade; Undo exact | Works |  |
| 120 | grade blending 100 | PASS | changes an active grade; Undo exact | Works |  |
| 121 | grade balance -100 | PASS | changes an active grade; Undo exact | Works |  |
| 122 | grade balance +100 | PASS | changes an active grade; Undo exact | Works |  |

### 5.5 Auto edit

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | Auto edit: a0541-050618_215910__MG_2944.dng | PASS | mean change 9.72 levels; {'recipe': {'photoId': 'pht_01a17c4d-2924-7d33-8f6b-edb2cead8323', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 2 | Auto edit: IWT_6b6232015e.jpg | WARN | Auto edit left this photograph exactly as it was | Works | No change was a decision: the AI route judged it balanced, and the white drape measures neutral (R 191, G 188, B 192). The panel should say 'already balanced - nothing changed' so it never looks like a failure. |
| 3 | Auto edit: IWT_83fee8abf8.jpg | PASS | mean change 6.72 levels; {'recipe': {'photoId': 'pht_01a17c4d-2926-72a0-bbc7-70958ef7e09a', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 4 | Auto edit: IWT_43200f1414.jpg | PASS | mean change 14.45 levels; {'recipe': {'photoId': 'pht_01a17c4d-2925-7e32-a374-f672ff61ccc6', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 5 | Auto edit: 164A2088.CR2 | PASS | mean change 3.54 levels; {'recipe': {'photoId': 'pht_01a17c4d-2923-7042-9684-a40f248e9195', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 6 | Auto edit: 164A2089.CR2 | PASS | mean change 8.56 levels; {'recipe': {'photoId': 'pht_01a17c4d-2924-7d33-8f6b-ed96430a8493', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 7 | Auto edit: IWT_04d24a6549.jpg | PASS | mean change 3.53 levels; {'recipe': {'photoId': 'pht_01a17c4d-2925-7e32-a374-f63a18bdfca8', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 8 | Auto edit: IWT_198817ff3d.jpg | PASS | mean change 2.00 levels; {'recipe': {'photoId': 'pht_01a17c4d-2925-7e32-a374-f656165539cb', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 9 | Auto edit: IWT_5469f35d26.jpg | PASS | mean change 0.87 levels; {'recipe': {'photoId': 'pht_01a17c4d-2926-72a0-bbc7-7054f720e720', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 10 | Auto edit: IWT_aeb96ff84f.jpg | PASS | mean change 4.73 levels; {'recipe': {'photoId': 'pht_01a17c4d-2927-79b0-bcfd-da1d08308b61', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 11 | Auto edit: IWT_e734beadc7.jpg | PASS | mean change 0.31 levels; {'recipe': {'photoId': 'pht_01a17c4d-2927-79b0-bcfd-da3d17be9eab', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 12 | Auto edit: bride-as-png.png | PASS | mean change 14.44 levels; {'recipe': {'photoId': 'pht_01a17c4d-2925-7e32-a374-f5f6dfaf359f', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 13 | Auto edit: colour-chart.png | PASS | mean change 0.58 levels; {'recipe': {'photoId': 'pht_01a17c4d-2925-7e32-a374-f6136bf8f004', 'body': '{"bw":null,"engine":"aura-render/1.0.0","geo | Works |  |
| 14 | photo analysis | PASS | {'photoId': 'pht_01a17c4d-2925-7e32-a374-f5f6dfaf359f', 'readings': {'image_hash': '', 'median': 123, 'high': 143, 'low': 113, 'neutral_rgb': [181, 189, 204], 'neutral_pi | Works |  |
| 15 | enhance photo | PASS | b09c322be55a | Works |  |
| 16 | enhance portrait | PASS | ba41b2830ac5 | Works |  |

AI route: Auto edit tried the configured provider four times in this run and fell back to the local
model each time (`provider_failed`); answers it had cached earlier (Groq `qwen/qwen3.8-27b`) were
reused. The fallback is graceful; the live route needs a working key.

### 5.6 Automatic retouch (22 presets, strength sliders, Auto portrait, Auto advanced retouch)

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | a face is found on at least one photograph | PASS | 1 of 10 photographs have a detected face: ['IWT_aeb96ff84f.jpg'] | Works |  |
| 2 | preset: Natural | PASS | 25 automatic operations | Works |  |
| 3 | preset: Acne · keep freckles | PASS | 12 automatic operations | Works |  |
| 4 | preset: Acne & freckles | PASS | 14 automatic operations | Works |  |
| 5 | preset: Polished wedding | PASS | 29 automatic operations | Works |  |
| 6 | preset: Bright and polished | PASS | 45 automatic operations | Works |  |
| 7 | preset: Acne only · preserve detail | PASS | 14 automatic operations | Works |  |
| 8 | preset: Detailed skin repair and review | PASS | 22 automatic operations | Works |  |
| 9 | preset: Portrait skin finish | PASS | 38 automatic operations | Works |  |
| 10 | preset: Smooth portrait finish | PASS | 40 automatic operations | Works |  |
| 11 | preset: Body skin finish · stretch marks | PASS | 40 automatic operations | Works |  |
| 12 | preset: Skin cleanup · refine pores | PASS | 23 automatic operations | Works |  |
| 13 | preset: Deep acne cleanup | PASS | 39 automatic operations | Works |  |
| 14 | preset: Professional retouch | PASS | 39 automatic operations | Works |  |
| 15 | preset: Subtle | PASS | 21 automatic operations | Works |  |
| 16 | preset: Soft glow | PASS | 25 automatic operations | Works |  |
| 17 | preset: Polished beauty | PASS | 31 automatic operations | Works |  |
| 18 | preset: Bridal | PASS | 32 automatic operations | Works |  |
| 19 | preset: Groom & men | PASS | 21 automatic operations | Works |  |
| 20 | preset: Editorial | PASS | 26 automatic operations | Works |  |
| 21 | preset: Body focus | PASS | 30 automatic operations | Works |  |
| 22 | preset: Studio clean | PASS | 28 automatic operations | Works |  |
| 23 | preset: Beauty & Fashion (Evoto-style) | PASS | 37 automatic operations | Works |  |
| 24 | retouch strength sliders | PASS | 8 sliders; moved, saved and reset | Works |  |
| 25 | Auto portrait button | PASS | saved | Works |  |
| 26 | Auto advanced retouch (18 stages) | PASS | 18 stages, 15 applied; quality passed=True | Works |  |
| 27 | analyse portrait | PASS | {'photoId': 'pht_01a17c4d-2927-79b0-bcfd-da1d08308b61', 'overlayWidth': 320, 'overlayHeight': 158, 'faces': [{'bbox': [0.123523004, 0.32433596, 0.065454, 0.1329 | Works |  |
| 28 | portrait retouch style: natural | PASS | {'photoId': 'pht_01a17c4d-2927-79b0-bcfd-da1d08308b61', 'ops': [{'op': 'skin_smooth', 'strength': 0.29}, {'op': 'skin_even', 'strength': 0.16}, {'op': 'blemish_ | Works |  |
| 29 | portrait retouch style: soft | PASS | {'photoId': 'pht_01a17c4d-2927-79b0-bcfd-da1d08308b61', 'ops': [{'op': 'skin_smooth', 'strength': 0.18}, {'op': 'skin_even', 'strength': 0.09}, {'op': 'blemish_ | Works |  |
| 30 | portrait retouch style: polished | PASS | {'photoId': 'pht_01a17c4d-2927-79b0-bcfd-da1d08308b61', 'ops': [{'op': 'skin_smooth', 'strength': 0.41}, {'op': 'skin_even', 'strength': 0.22}, {'op': 'blemish_ | Works |  |
| 31 | retouch strength sync | PASS | {'sliders': [{'category': 'blemishes', 'operations': 4, 'factor': 1, 'headroom': 1}, {'category': 'smoothing', 'operations': 5, 'factor': 1, 'headroom': 1.75713 | Works |  |

### 5.7 Face detection on real wedding photographs

| Photograph | In the frame | Found | Time |
|---|---|---|---|
| 989A2698 | the bride, close | 1, correct | 2.3 s |
| 989A2706 | the bride, veil | 1, correct | 1.3 s |
| 989A2742 | the bride, seated | 1, correct | 1.5 s |
| 989A2746 | the bride, close | 1, correct | 1.6 s |
| 989A2865 | the bride, small, full length on a stage | 1, correct | 1.2 s |
| 989A2925 | bride and an older man | 2, correct (one found by the tilted pass) | 1.4 s |
| 989A3150 | couple seated, three standing | 4: bride and two standing found; **groom in sunglasses and the cropped man missed; one false face on the white saree** | 1.5 s |
| 989A3839 | about six women | 5: four correct, one doubtful | 1.6 s |

Portrait orientation from the camera was applied correctly on every CR3.

### 5.8 AI selections (masks)

| Selection | On real portraits | Verdict |
|---|---|---|
| Subject | the whole bride, cleanly, 0.4-0.7 s | Works; on a stage with five people it took only the speaker |
| Background | the exact complement of the subject | Works |
| Sky | **33 % of a frame with no sky (trees); 24.5 % of an indoor stage (a banner)**; correctly nothing on three frames | **Defect** (section 3, #4) |
| Face skin, body skin | face; hands and arms | Works (1-2.6 s for face skin) |
| Hair | the hair under the tiara and veil | Works |
| Clothes | the whole saree and blouse | Works |
| Eyes, irises, whites, eyebrows, lips | small, placed on the features | Works on large faces; near zero on small ones, as expected |

Window-run rows:

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | mask kinds | PASS | ['skin', 'face', 'eyes', 'sclera', 'iris', 'teeth', 'lips', 'eyebrows', 'hair', 'facial_hair', 'clothing', 'dress', 'background', 'sky', 'subject', 'greenery', 'water', ' | Works |  |
| 2 | ensure masks | PASS | 20 masks | Works |  |
| 3 | AI selection: subject | PASS | 1 mask(s); 12.8% of the photo selected | Works, with an issue | Accurate on a single bride; on a stage with five people it selected only the speaker. |
| 4 | AI selection: background | PASS | 2 mask(s); 87.2% of the photo selected | Works |  |
| 5 | AI selection: sky | PASS | 3 mask(s); 24.5% of the photo selected | **Defect** | Selected 24.5 % of an indoor stage photo (the white banner) and, in the mask check, 33 % of a portrait with no sky (dark trees). Fix: require sky to be bright and connected to the top edge, and refuse below a confidence. |
| 6 | AI selection: face_skin | PASS | 4 mask(s); 0.7% of the photo selected | Works |  |
| 7 | AI selection: body_skin | PASS | 5 mask(s); 0.3% of the photo selected | Works |  |
| 8 | AI selection: hair | PASS | 6 mask(s); 0.4% of the photo selected | Works |  |
| 9 | AI selection: clothes | PASS | 7 mask(s); 3.8% of the photo selected | Works |  |
| 10 | AI selection: eyes | PASS | 8 mask(s); 0.0% of the photo selected | Works | Tiny face here; on real portraits eye, iris, brow and lip selections are small but placed. |
| 11 | AI selection: iris | PASS | 9 mask(s); 0.0% of the photo selected | Works |  |
| 12 | AI selection: sclera | PASS | 10 mask(s); 0.0% of the photo selected | Works |  |
| 13 | AI selection: eyebrows | PASS | 11 mask(s); 0.0% of the photo selected | Works |  |
| 14 | AI selection: lips | PASS | 12 mask(s); 0.0% of the photo selected | Works |  |

### 5.9 Looks - Match a look, reference looks, paired match

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | match a finished version of the same photo | PASS | error 8.67 -> 4.34 | Works |  |
| 2 | reference look from a folder of photos | PASS | analysed 9bd4222e-0 and applied | Works |  |
| 3 | match a look (lighting buckets) | PASS | {'profile': 'prf_01a17c7e-d1f7-7692-a1f3-bb8a700fff0e', 'cancelled': False, 'found': 8, 'measured': 8, 'refused': 0, 'baselineFrames': 0, 'buckets': 2, 'matched': None, ' | Works |  |
| 4 | list looks | PASS | 1 look(s) | Works |  |

### 5.10 Library, import and previews

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | import every format | PASS | 13 photographs, including 3 RAW | Works |  |
| 2 | previews and render: a0541-050618_215910__MG_2944.dng | PASS | thumbnail ok, original 1600x1067, proxy 1752x1168 | Works |  |
| 3 | previews and render: IWT_6b6232015e.jpg | PASS | thumbnail ok, original 1600x1200, proxy 2048x1536 | Works |  |
| 4 | previews and render: IWT_83fee8abf8.jpg | PASS | thumbnail ok, original 1094x1600, proxy 1400x2048 | Works |  |
| 5 | previews and render: IWT_43200f1414.jpg | PASS | thumbnail ok, original 1600x1200, proxy 2048x1536 | Works |  |
| 6 | previews and render: 164A2088.CR2 | PASS | thumbnail ok, original 1066x1600, proxy 1365x2048 | Works |  |
| 7 | previews and render: 164A2089.CR2 | PASS | thumbnail ok, original 1066x1600, proxy 1365x2048 | Works |  |
| 8 | previews and render: IWT_04d24a6549.jpg | PASS | thumbnail ok, original 1200x1600, proxy 1536x2048 | Works |  |
| 9 | previews and render: IWT_198817ff3d.jpg | PASS | thumbnail ok, original 1600x1066, proxy 2048x1365 | Works |  |
| 10 | previews and render: IWT_5469f35d26.jpg | PASS | thumbnail ok, original 1600x1066, proxy 2048x1364 | Works |  |
| 11 | previews and render: IWT_aeb96ff84f.jpg | PASS | thumbnail ok, original 1600x788, proxy 2048x1008 | Works |  |
| 12 | previews and render: IWT_e734beadc7.jpg | PASS | thumbnail ok, original 1600x1068, proxy 2048x1367 | Works |  |
| 13 | previews and render: bride-as-png.png | PASS | thumbnail ok, original 1600x1200, proxy 2048x1536 | Works |  |
| 14 | previews and render: colour-chart.png | PASS | thumbnail ok, original 1600x800, proxy 1600x800 | Works |  |
| 15 | prefetch previews | PASS | 0 | Works |  |
| 16 | descriptors before analysis | PASS | refused before analysis, with an accurate message | Works |  |
| 17 | list collections | PASS | 1 collection(s) | Works |  |

CR3 (Canon's current RAW) imports, orients and renders - eight 30-39 MB files in this run. The DNG
from an 8-megapixel camera renders a 1,752 px preview, which is its half-size decode, not a sizing
fault.

### 5.11 Analysis passes (phases 05-27)

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | estimate_tone | PASS | {'estimated': 13, 'failed': 0, 'faceAnchored': 0, 'mixedLight': 3, 'colouredLight': 1, 'lowConfidence': 7, 'loci': 0, 'segmentsAnchored': 0, 'meanEv': 0, 'meanCct': 8217. | Works |  |
| 2 | estimate_colour | PASS | {'decided': 13, 'failed': 0, 'skinMeasured': 0, 'skinGuardTriggered': 0, 'skinGuardWithdrew': 0, 'clipGuardResolved': 7, 'subtletyCapped': 0, 'lowConfidence': 0, 'meanCon | Works |  |
| 3 | analyse_integrity | PASS | {'scored': 13, 'failed': 0, 'faces': 0, 'closed': 0, 'closedOk': 0, 'meanScore': 0.61496586, 'uncalibrated': ['Apple iPhone 7', 'Canon Canon EOS 20D', 'Canon Canon EOS 5D | Placeholder (by design) | 'Subject soft' on 12 of 13 photos, most of them sharp; camera names doubled ('Canon Canon EOS 5D Mark IV'). |
| 4 | analyse_composition | PASS | {'scored': 13, 'failed': 0, 'keypointSubjects': 0, 'cut': 0, 'intentionalTilts': 0, 'horizons': 13, 'meanAbsTilt': 13.917692, 'flagCounts': [6, 0, 12, 0, 0, 0, 0, 0, 0, 0 | Placeholder (by design) | A 'horizon' found in all 13 photos (bird close-up, top-down dolphins); 6 flagged tilted, mean 13.9 deg - likely false. |
| 5 | embed_project | PASS | {'embedded': 13, 'failed': 0, 'remaining': 0, 'elapsedMs': 500, 'batches': 2, 'cancelled': False} | Works |  |
| 6 | classify_scenes | PASS | {'photos': 13, 'classified': 13, 'coverage': 1, 'chapters': 0, 'needsReview': 0, 'locked': 0, 'penalty': 0, 'gapsOnly': False, 'sceneVer': 100, 'taxonomyVer': 1, 'rituals | Placeholder (by design) | Every photo's scene is 'unknown', so exposure targets, tone intent, composition rules and local-light policy all fall back to neutral. |
| 7 | scan_faces | PASS | {'scanned': 13, 'faces': 832, 'voting': 0, 'gated': 832, 'personBoxes': 832, 'headless': 1, 'failed': 0, 'remaining': 0, 'tiledFrames': 11, 'tileRatio': 0.84615386, 'elap | Placeholder (by design) | 832 'faces' on 13 photos (64 each, including a bird and dolphins), all gated - the phase 06 placeholder detector. The portrait detector that does work (Haar, aura-portrait) is not used here. Fix: feed aura-portrait's faces into PeopleService. |
| 8 | group_people | PASS | {'identities': 0, 'assigned': 0, 'unassigned': 832, 'threshold': 0.55, 'refusedMerges': 0, 'subClustered': 0, 'couple': [], 'coupleConfidence': 0, 'coupleAmbiguous': Fals | Placeholder (by design) | 0 identities, because 0 voting faces. |
| 9 | group_moments | PASS | {'photos': 13, 'groupable': 13, 'grouped': 6, 'coverage': 0.461, 'moments': 5, 'locked': 0, 'meanSize': 1.2, 'bursts': 5, 'duplicates': [0, 0, 0], 'medianIntervalMs': 141 | Works |  |
| 10 | score_emotion | PASS | {'scored': 13, 'failed': 0, 'faces': 832, 'moments': 5, 'peaked': 0, 'links': 5, 'meanScore': 0.9162264, 'elapsedMs': 3142, 'cancelled': False} | Placeholder (by design) | Scores computed over the 832 gated placeholder boxes (mean 0.92) - meaningless until real faces arrive. |
| 11 | cull_project | PASS | {'photos': 13, 'eligible': 13, 'selected': 4, 'vetoCounts': [1, 1, 0], 'vetoNames': ['veto_out_of_focus', 'veto_exposure_lost', 'veto_eyes_closed'], 'swaps': 0, 'coverage | Works |  |
| 12 | curate_project | PASS | {'photos': 13, 'selected': 4, 'curated': 4, 'coverage': 1, 'bwOffered': 0, 'bwAccepted': 0, 'bwRejected': 0, 'heroes': 4, 'chaptersCovered': 1, 'spreads': 3, 'albumSize': | Works |  |
| 13 | sculpt_local | PASS | {'planned': 13, 'failed': 0, 'actedOn': 0, 'opCounts': [0, 0, 0, 0, 0, 0], 'gated': 52, 'fullyMasked': 0, 'groupSolved': 13, 'shineReduced': 0, 'lowConfidence': 0, 'meanB | Placeholder (by design) | Planned 13, acted on 0, 52 operations gated - masks are not wired into the local pass. |
| 14 | retouch_pass | PASS | {'planned': 13, 'failed': 0, 'actedOn': 0, 'maskCovered': 0, 'blemishes': 0, 'textureResolved': 0, 'textureWithdrawn': 0, 'protected': 0, 'lowConfidence': 13, 'meanBandRa | Placeholder (by design) | Acted on 0 (low confidence on all 13). The manual and Auto retouch paths are what work. |
| 15 | micro_pass | PASS | {'planned': 13, 'failed': 0, 'actedOn': 0, 'regionCovered': 0, 'ops': [0, 0, 0, 0, 0], 'borrows': 0, 'meanAlignment': 0, 'withdrawn': [0, 0, 0], 'resolved': 0, 'lowConfid | Placeholder (by design) | Acted on 0. |
| 16 | restore_pass | PASS | {'planned': 13, 'failed': 0, 'actedOn': 0, 'regionCovered': 0, 'tiers': [13, 0, 0, 0], 'sharpened': 0, 'facesRecovered': 0, 'facesSkippedIdentity': 0, 'reduced': 0, 'lowC | Placeholder (by design) | Planned 13, acted on 0 - the untrained restoration head refuses rather than guesses. |
| 17 | plan_geometry | PASS | {'planned': 13, 'failed': 0, 'keptOriginal': 1, 'recipesWritten': 0, 'recipesProtected': 0, 'elapsedMs': 44, 'cancelled': False} | Works |  |
| 18 | cleanup_pass | PASS | {'examined': 13, 'withProposals': 0, 'proposals': 0, 'blocked': [9, 0, 0, 0, 0], 'reverted': 0, 'judged': 0, 'declined': 0, 'failed': 0, 'cancelled': False, 'elapsedMs':  | Placeholder (by design) | 0 proposals by design: the distraction detector is untrained, so everything is refused. |
| 19 | gallery_pass | PASS | {'nodes': 0, 'anchored': 0, 'split': 0, 'normalised': 0, 'outliers': 0, 'skinTargets': 0, 'spreadBeforeCct': 0, 'spreadAfterCct': 0, 'spreadBeforeEv': 0, 'spreadAfterEv': | Placeholder (by design) | 0 nodes, so nothing normalised - there are no chapters without scene classes. |
| 20 | camera_pass | PASS | {'cameras': 6, 'referenceId': '', 'referenceSource': 'primary_shooter', 'pairs': 0, 'pairsRejected': 0, 'heldoutPairs': 0, 'solved': 0, 'blended': 0, 'baselineOnly': 10,  | Placeholder (by design) | 6 cameras, 0 fingerprinted; every correction from a fabricated brand baseline. |
| 21 | qc_run | PASS | {'images': 4, 'imagesUnreached': 0, 'complete': True, 'checksRun': 21, 'skipped': 16, 'byCategory': [{'category': 'consistency', 'found': 0, 'fixed': 0, 'escalated': 0, ' | Works | Runs: 21 checks, 16 skipped (completeness 0.57), 5 open tickets. It says what it skipped, which is correct. |
| 22 | descriptors after analysis | PASS | {'photoId': 'pht_01a17c4d-2925-7e32-a374-f5f6dfaf359f', 'dhashHex': '645c569bd5d5a0e8', 'lumaMean': 0.49146098, 'lumaP1': 0.42745098, 'lumaP50': 0.48235294, 'lu | Works |  |
| 23 | build similarity index | PASS | {'vectors': 13, 'photos': 13, 'coverage': 1, 'filterable': 6, 'modelVer': 100, 'staleModelVersions': [], 'buildMs': 1, 'fromSnapshot': False} | Works |  |
| 24 | find similar | PASS | {'photoId': 'pht_01a17c4d-2925-7e32-a374-f5f6dfaf359f', 'neighbours': [{'photoId': 'pht_01a17c4d-2925-7e32-a374-f672ff61ccc6', 'distance': 5.4836273e-06, 'similarity': 0. | Works |  |
| 25 | story outline | PASS | {'chapters': [{'segmentId': 'seg_01a17c7f-9a82-70c1-a8ba-ccffb5f2fb3c', 'ordinal': 0, 'chapter': 'details', 'label': None, 'title': 'Details', 'startMs': 0, 'endMs': 0, ' | Works |  |
| 26 | tone_status | PASS | {'photos': 13, 'estimated': 13, 'coverage': 1, 'faceAnchored': 0, 'skinConstrained': 0, 'mixedLight': 3, 'colouredLight': 1, 'needsReview': 7, 'userEdited': 0, 'meanEv':  | Works |  |
| 27 | colour_status | PASS | {'photos': 13, 'decided': 13, 'coverage': 1, 'skinMeasured': 0, 'skinGuardTriggered': 0, 'clipGuardResolved': 7, 'subtletyCapped': 0, 'needsReview': 0, 'userEdited': 0, ' | Works |  |
| 28 | integrity_status | PASS | {'photos': 13, 'scored': 13, 'coverage': 1, 'subjectAware': 0, 'reviewed': 0, 'flagCounts': [12, 0, 1, 1, 0, 4, 0, 1, 0, 0, 0, 0, 13, 4], 'flagNames': ['subject_soft', 'c | Works |  |
| 29 | composition_status | PASS | {'photos': 13, 'scored': 13, 'coverage': 1, 'keypointAware': 0, 'flagCounts': [6, 0, 12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 13], 'flagNames': ['horizon_tilted', 'intenti | Works |  |
| 30 | people_status | PASS | {'photos': 13, 'scanned': 13, 'coverage': 1, 'faces': 832, 'votingFaces': 0, 'identities': 0, 'tiledFrames': 11, 'coupleUnconfirmed': True, 'staleVersions': [], 'erased': | Works |  |
| 31 | moment_status | PASS | {'photos': 13, 'groupable': 13, 'grouped': 6, 'coverage': 0.461, 'moments': 5, 'locked': 0, 'meanSize': 1.2, 'bursts': 5, 'duplicates': [0, 0, 0], 'medianIntervalMs': 141 | Works |  |
| 32 | emotion_status | PASS | {'photos': 13, 'scored': 13, 'coverage': 1, 'faceAware': 1, 'moments': 5, 'peaked': 0, 'peakRate': 0, 'links': 5, 'interactionCounts': [7, 0, 0, 0, 13, 0, 13, 0, 6], 'int | Works |  |
| 33 | cull_status | PASS | {'photos': 13, 'eligible': 13, 'selected': 4, 'coverage': 1, 'emotionAware': 1, 'compositionAware': 1, 'grouped': 0.461, 'covered': 1, 'coveredWeak': 0, 'missing': 11, 'u | Works |  |
| 34 | curate_status | PASS | {'photos': 13, 'selected': 4, 'curated': 4, 'coverage': 1, 'bwOffered': 0, 'bwAccepted': 0, 'bwRejected': 0, 'heroes': 4, 'chaptersCovered': 1, 'spreads': 3, 'albumSize': | Works |  |
| 35 | gallery_status | PASS | {'photos': 13, 'normalised': 0, 'coverage': 0, 'nodes': 0, 'anchoredNodes': 0, 'splitNodes': 0, 'pinnedAnchors': 0, 'bounded': 0, 'moodPreserved': 0, 'userEdited': 0, 'ou | Works |  |
| 36 | camera_status | PASS | {'photos': 13, 'matched': 12, 'coverage': 0.9230769, 'cameras': 6, 'fingerprinted': 0, 'solvedFromPairs': 0, 'blended': 0, 'baselineOnly': 12, 'pairs': 0, 'pairsRejected' | Works |  |
| 37 | qc_status | PASS | {'selected': 4, 'checked': 4, 'coverage': 1, 'inspections': 21, 'inspectionsSkipped': 16, 'completeness': 0.567, 'open': 5, 'accepted': 0, 'dismissed': 0, 'falseTicketRat | Works |  |
| 38 | restore_status | PASS | {'photos': 13, 'planned': 13, 'coverage': 1, 'actedOn': 0, 'regionCovered': 0, 'tiers': [13, 0, 0, 0], 'tierNames': ['off', 'light', 'standard', 'strong'], 'sharpened': 0 | Works |  |
| 39 | geometry_status | PASS | {'photos': 13, 'planned': 13, 'coverage': 1, 'keptOriginal': 1, 'profileCovered': 0, 'levelled': 0, 'meanRotateDeg': 0, 'keystoned': 0, 'variantCounts': [0, 0, 0, 0, 0],  | Works |  |
| 40 | cleanup_status | PASS | {'photos': 13, 'examined': 13, 'coverage': 1, 'withProposals': 0, 'applied': 0, 'blocked': [9, 0, 0, 0, 0], 'checkNames': ['size_cap', 'denylist', 'identity_protect', 'st | Works |  |
| 41 | micro_status | PASS | {'photos': 13, 'planned': 13, 'coverage': 1, 'actedOn': 0, 'regionCovered': 0, 'opCounts': [0, 0, 0, 0, 0], 'operators': ['flyaway', 'teeth', 'eyes', 'clothing', 'glare'] | Works |  |
| 42 | retouch_status | PASS | {'photos': 13, 'planned': 13, 'coverage': 1, 'actedOn': 0, 'maskCovered': 0, 'blemishesRemoved': 0, 'anomaliesLeft': 0, 'protectedCounts': [0, 0, 0, 0, 0, 0], 'protectedK | Works |  |
| 43 | local_status | PASS | {'photos': 13, 'planned': 13, 'coverage': 1, 'actedOn': 0, 'maskCovered': 0, 'opCounts': [0, 0, 0, 0, 0, 0], 'opNames': ['face_light', 'subject_enhance', 'background_bala | Works |  |
| 44 | story_status | PASS | {'photos': 13, 'classified': 13, 'coverage': 1, 'chapters': 6, 'needsReview': 6, 'locked': 0, 'penalty': 0, 'gapsOnly': True, 'sceneVer': 100, 'taxonomyVer': 1, 'ritualsK | Works |  |
| 45 | index_status | PASS | {'vectors': 13, 'photos': 13, 'coverage': 1, 'filterable': 6, 'modelVer': 100, 'staleModelVersions': [], 'buildMs': 1, 'fromSnapshot': False} | Works |  |
| 46 | develop_status | PASS | {'images': 13, 'withRecipe': 13, 'fromAi': 13, 'fromUser': 0, 'touchedByHand': 2, 'sidecarBehind': 13} | Works |  |
| 47 | look_status | PASS | {'profiles': 1, 'selected': None, 'selectedName': '', 'selectedOrigin': '', 'strength': 1, 'appliable': 13, 'photographs': 13, 'baselineCoverage': 1, 'networkTransportAva | Works |  |
| 48 | style_status | PASS | {'profiles': 0, 'active': None, 'activeName': '', 'activeVersion': 0, 'trainedPairs': 0, 'strength': 0, 'overallDe00': 0, 'chapterOverrides': [], 'levelCounts': [], 'buck | Works |  |
| 49 | learn_status | PASS | {'corrections': 0, 'projects': 0, 'buckets': 0, 'actionableBuckets': 0, 'unattributed': 0, 'attributionRate': 0, 'updates': 0, 'adopted': 0, 'consentedProjects': 0, 'cont | Works |  |

### 5.12 Export, one-click finishing and Autopilot

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | export jpeg 8-bit | FAIL | Error: Page.evaluate: Error: {"code":"AURA-REL-12005","message":"Your free trial has ended. Editing still works; enter a licence key to export finished photographs.","ret | Blocked on this machine | Refused by the expired trial (AURA-REL-12005) - the refusal itself is correct and clear. Export's writers, read-back and manifest pass in the Rust suites and the phase 30 gate. |
| 2 | export png 8-bit | FAIL | Error: Page.evaluate: Error: {"code":"AURA-REL-12005","message":"Your free trial has ended. Editing still works; enter a licence key to export finished photographs.","ret | Blocked on this machine | Trial expired; see above. |
| 3 | export tiff 16-bit | FAIL | Error: Page.evaluate: Error: {"code":"AURA-REL-12005","message":"Your free trial has ended. Editing still works; enter a licence key to export finished photographs.","ret | Blocked on this machine | Trial expired; see above. |
| 4 | watermarked export | FAIL | Error: Page.evaluate: Error: {"code":"AURA-REL-12005","message":"Your free trial has ended. Editing still works; enter a licence key to export finished photographs.","ret | Blocked on this machine | Trial expired; see above. |
| 5 | delivery manifest | PASS | None | Works |  |
| 6 | delivery status | PASS | {'files': 0, 'backups': 0, 'backedUp': 0, 'diverged': 0, 'providers': 0, 'uploaded': 0, 'outstanding': 0, 'refused': 0, 'resumes': 0, 'unmappedSets': 0, 'bytesSent': 0, ' | Works |  |
| 7 | export presets | PASS | 6 presets | Works |  |

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | finish a whole folder (edit, retouch, cull, export) | FAIL | AssertionError: failed: {'analyzed': 13, 'failedEdits': 3, 'jobId': 'oneclick-c4ec90fd-4dd1-4492-8aa6-e642005c9351', 'status': 'failed', 'phase': 'export', 'phaseLabel':  | **Defect** | Two problems. (1) Export refused by the expired trial. (2) Independent of that: 3 of 13 photos were withheld from delivery because a small background face could not be contour-checked (portrait_preflight.rs:98-126 requires a reading for every saved face, retouched or not). A crowd shot is a normal wedding photo. Fix: require readings only for faces with an enabled auto-portrait-v1-{face}- operation. |

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | pre-flight | PASS | {'verdict': 'warn', 'permitsStart': True, 'images': 13, 'estimatedOutputBytes': 156000000, 'estimatedMs': 13084, 'rows': [{'check': 'project_integrity', 'title': 'The wed | Works |  |
| 2 | run Autopilot on the wedding | WARN | completed_degraded: selected 0, exported 0, 1169 ms; degraded: [['cull', 'This step is waiting for you, because AURA is not confident enough to do it on its own'], ['tone | Placeholder (by design) | Completes 'degraded': 13 of its steps, export included, wait for a person because nothing is calibrated - so it selects 0 and exports 0. Documented (ADR-0057), but the button does nothing a photographer can see; the panel should say so before it is pressed. |

### 5.13 Settings, system and every workspace and Advanced section in the window

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | hardware_plan | PASS | {'gpu': None, 'epOrder': ['cpu'], 'selectedEp': 'cpu', 'overrideEp': None, 'unavailable': [{'ep': 'tensorrt', 'reason': 'not compiled into this build'}, {'ep': 'cuda', 'r | Works |  |
| 2 | render_caps | PASS | {'backend': 'wgpu', 'maxTexture': 9459, 'precisionBits': 24, 'maxWorkingBytes': 1073741824, 'engine': 'aura-render/1.0.0+profiles.1+portrait.4+display.1+local.1+raw.2+cle | Works |  |
| 3 | licence_status | PASS | {'state': 'trial_ended', 'mayExport': False, 'name': None, 'email': None, 'edition': None, 'expires': None, 'daysLeft': None, 'trialEnds': '2026-10-26', 'message': 'Your  | Works |  |
| 4 | list_ai_providers | PASS | [{'id': 'anthropic', 'label': 'Anthropic (Claude)', 'blurb': 'Strong at reading a scene and explaining why. The default this product was built against.', 'wire': 'anthrop | Works |  |
| 5 | ai_setup_status | PASS | {'completed': True, 'skipped': False, 'provider': 'anthropic', 'endpoint': None, 'models': ['', '', ''], 'keyedProviders': [], 'schemes': ['http', 'https'], 'offlineStudi | Works |  |
| 6 | internal_ai_status | PASS | {'profiles': [{'id': 'GOOGLE_AISTUDIO', 'endpoint': 'https://generativelanguage.googleapis.com/v1beta/openai', 'model': 'gemini-3.8-flash', 'vision': True, 'inputUsdPerMt | Works |  |
| 7 | cloud_status | PASS | {'provider': 'compat', 'endpoint': 'https://api.groq.com/openai/v1', 'keyPresent': True, 'keyFingerprint': 'gsk_...M2n2', 'keyStore': 'dpapi', 'offlineStudioMode': False, | Works |  |
| 8 | list_models | PASS | [{'name': 'aura_tiny_embedding', 'version': '1.0.0', 'task': 'embedding', 'activeVersion': None, 'pendingVersion': None, 'rejectedVersions': [], 'modelCard': 'docs/model- | Works |  |
| 9 | infer_stats | PASS | {'residentSessions': 8, 'poolHits': 84, 'poolLoads': 9, 'requests': 377, 'downshifts': 0, 'meanOverheadMs': 0.0002970822, 'peakMemoryMb': 104} | Works |  |
| 10 | diagnostics_report | PASS | {'appVersion': '1.0.0', 'schemaVersion': 31, 'renderBackend': 'wgpu', 'renderDegradation': None, 'modelSet': 'a7541dfc1e2bd01a5234513d9ba0e15ec3450fe7005fa1f5194e720999ee | Works |  |
| 11 | preview_stats | PASS | {'bytesUsed': 216466157, 'budgetBytes': 42949672960, 'entries': 52, 'hits': 45, 'misses': 39, 'evictions': 0, 'hitRate': 0.5357142857142857} | Works |  |
| 12 | list_problems | PASS | [] | Works |  |
| 13 | activity_status | PASS | {'items': [], 'imageWorkersBusy': 0, 'imageWorkers': 4} | Works |  |
| 14 | cloud_spend | PASS | {'capUsd': 1.5, 'spentUsd': 0.027776799999999997, 'monthCapUsd': 30, 'monthSpentUsd': 0.027776799999999997, 'calls': 11, 'downgrades': 0, 'fallbacks': 25, 'cacheHitRate': | Works |  |

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | workspace: Start | PASS | opens without errors | Works |  |
| 2 | workspace: Photos | PASS | opens without errors | Works |  |
| 3 | workspace: Auto edit | PASS | opens without errors | Works |  |
| 4 | workspace: Match a look | PASS | opens without errors | Works |  |
| 5 | workspace: Export | PASS | opens without errors | Works |  |
| 6 | workspace: Advanced | PASS | opens without errors | Works |  |
| 7 | Advanced: Photo measurements | PASS | opens without errors | Works |  |
| 8 | Advanced: People | PASS | opens without errors | Works |  |
| 9 | Advanced: Story and moments | PASS | opens without errors | Works |  |
| 10 | Advanced: Camera matching | PASS | opens without errors | Works |  |
| 11 | Advanced: Cull and selection | PASS | opens without errors | Works |  |
| 12 | Advanced: Photo evidence | PASS | opens without errors | Works |  |
| 13 | Advanced: Detailed development | PASS | opens without errors | Works |  |
| 14 | Advanced: Portrait regions | PASS | opens without errors | Works |  |
| 15 | Advanced: Precision retouch, hair color and proportions | PASS | opens without errors | Works |  |
| 16 | Advanced: Object cleanup | PASS | opens without errors | Works |  |
| 17 | Advanced: Learn a style | PASS | opens without errors | Works |  |
| 18 | Advanced: Complete collection workflow | PASS | opens without errors | Works |  |
| 19 | Advanced: Provider catalogue | PASS | opens without errors | Works |  |
| 20 | Advanced: Activity log | PASS | opens without errors | Works |  |
| 21 | Advanced: Licence | PASS | opens without errors | Works |  |
| 22 | Advanced: Quality review | PASS | opens without errors | Works |  |
| 23 | Advanced: Gallery consistency | PASS | opens without errors | Works |  |
| 24 | Advanced: Albums & curation | PASS | opens without errors | Works |  |
| 25 | Advanced: AI provider | PASS | opens without errors | Works |  |
| 26 | Advanced: Performance & storage | PASS | opens without errors | Works |  |

| # | Check | Result | Detail | Verdict | Notes |
|---|---|---|---|---|---|
| 1 | originals never modified | PASS | all 13 originals byte-identical | Works |  |
| 2 | no page errors | PASS | none | Works |  |

## 6. Not proven by this run

- **Export through the window.** The trial on this machine ended, so every export, one-click finish
  and watermarked export was refused with `AURA-REL-12005` - correctly. The writers, read-back
  verification and manifest pass in the Rust suites and the phase 30 gate; a licence key (or a
  debug-only test key) is needed to prove the window path. Recommendation: let a debug build with
  `AURA_TEST_CATALOG` accept a test-only key, so the harness can export.
- **The positive case of tools that found nothing**: beards, open smiles (teeth evening), red-eye,
  lens fringing, dust, JPEG blocks, banding, tan lines, cast shadows, group balance. Unit tests cover
  them on painted defects; a real photo with each defect is still missing from the test set.
- **Quality judged by photographers.** Every pixel number here says a tool did something, where,
  and how much - not that a retoucher would keep it. The visual review above caught the colour
  defects; a side-by-side review of the 245 crops by a photographer is the remaining step.
- **Live AI providers.** Only cached answers were used; the configured providers failed in this run.

## 7. What to add

From this test, the parity documents and what a wedding photographer will look for first:

1. **Make people-aware features real**: feed the working portrait detector into People, Emotion,
   Cull and Curation (section 4); add face recognition so the same person is retouched the same way
   across a wedding.
2. **A test licence for debug builds**, so export and one-click can be proven on every run.
3. **Better test photographs** for `test-everything-gui.py`: real portraits with open smiles, a
   beard, a group, red-eye, a sky - the current "bride" set has no usable face.
4. **Multi-person subject selection** (every person, or pick one) and **per-person masks**.
5. **Content-aware remove and generative fill / background extend** on real photographs - currently
   refused until a distraction detector is trained.
6. **HDR merge, panorama merge and soft proofing** (listed as missing in `docs/lightroom-parity.md`).
7. **GPU rendering** - the shaders exist and are checked; no backend is linked, so every edit renders
   on the CPU (about 2 s per tool edit at preview size here).
8. **Measured camera and lens profiles** - every camera renders through the neutral reference profile,
   and every bundled lens profile and brand baseline is fabricated.
9. **Tethered shooting** and **glasses-reflection removal / missing-tooth repair** (Evoto modules
   marked "model" in `docs/evoto-module-map.md`).
10. **Clear "nothing to do" messages** wherever a tool or Auto edit decides to change nothing, and a
    disabled state for controls that depend on another (light direction, relight direction).
