# AURA status - 2026-10-26

What has been achieved, what was done and fixed, what is left, and what to aim for next. Written
for the product owner and for whoever picks the work up next. Branch
`feat/match-a-look-assessment-fixes`, pull request #73.

## Update - 2026-10-27

PR #73 was merged into `main` (`b0c20d4`, all CI checks green). This round is on branch
`fix/faces-and-in-app-coverage`.

**Fixed: the window filled its memory while you edited.** Running the 100 Evoto-style tools in the
window, the page process reached 4.4 GB after about 18 slider changes on CR3 files and stopped
answering clicks (two checks failed on timeouts, not on the tools). The cause: every full-quality
preview of a 30-megapixel photograph was turned into a 120 MB `data:` text URL, and the window
kept each one it had shown - measured, every new preview added 120 MB that a forced clean-up did not
return. Previews and the coverage, changes and clipping overlays are now blob images, made once per
preview, released when the preview is dropped or replaced, and built in under a second instead of
7-24 s. The window's security policy allows `blob:` images.

Rerun of the 100 Evoto-style tools on the 27 CR3 files with the fix: **108 checks, 93 pass, 15
warnings, 0 failures**. The page's memory now rises and falls (mean 3.7 GB, back down to 1.7 GB
repeatedly, peak 5.9 GB) instead of only rising. The warnings are face sliders that move a few
hundred pixels on that portrait (inner corners, nostrils, lashes, eyeliner wing), a beard control
on a photo without a beard, and three spot tools that found nothing where they were tried.

**Test fix: placing a light needs a light.** The in-app test moved the key light's direction and
height and the relight's direction while the light itself was off, and reported "no change". It
now switches them on first; all six checks pass, and the face and body section reads **228 checks,
163 pass, 51 skips (one-sided controls refusing negatives), 14 warnings, 0 failures**.

**Found, not yet fixed: two face finders disagree on small faces.** Auto advanced retouch, auto
edit, masks and culling use the learned detector, which also scans tiles of the 2048 px preview;
the portrait parse that the renderer uses (face and body sliders, group balance, group skin, the
portrait operators, `analyse_portrait`) uses the Haar detector on a 1024 px canvas and needs eye
evidence. On a stage photograph the first finds four people and the second one, so the group
controls treat four people as one. The fix that keeps every render re-creatable from its recipe:
when a face-dependent edit is saved, add the faces the learned detector found and the parse did not
as face boxes in the recipe (the same `hint:` form the "draw a missed face" tool already writes),
so the parse measures them like a drawn face. It changes what the people tools act on in every
group photograph, so it needs its own measured change, an ADR and a run over group photographs.

**Still high: peak memory.** A full-resolution preview still crosses from the engine to the window
as about 120 MB of base64 text. Sending pixels as binary would roughly halve the peak; it changes
the IPC contract (`ui/src/ipc/types.ts` is frozen), so it needs an ADR.

## 1. What has been achieved

- **Match a look copies the reference's colours.** A new goal, *Exact colours of the reference*,
  gives a photo the reference's own tone curve, palette (in its proportions), split tone and skin
  colour. On the 20-photo desktop test the median distance to the reference's colours fell from
  21.3 to **14.5** (Palette 15.8, Natural 18.4). On cool, pastel and teal-orange references it lands
  close: 12.9 to 4.6, 7.2 to 2.2, 10.3 to 4.1. On smooth test scenes it closes 96 % of the gap.
- **Your own edit is learned exactly** when the edited reference was made from a photo in the
  collection: the original is found in 19 of 20 cases and the result comes within a median 0.71 of
  the true look (from 7.6).
- **Safe by construction.** Every match is checked on the rendered pixels for new bands, clipped
  highlights, skin moved too far and colour regressions. Apply saves the strongest of 100/75/50/25 %
  that passes and says what it saved.
- **Every tool was run in the real app** (`scripts/test-everything-gui.py`, 670 checks over 13
  photographs including 3 RAW files, a colour chart and a portrait), and every failure it found has
  been fixed or explained (section 4).
- **CI is green** on all checks for commit `7b3880a`: tests on Windows, macOS and Ubuntu,
  benchmarks, all thirty phase gates, desktop shell type-check, lints, UI tests and build, cargo
  deny and GitGuardian.

## 2. What was done (commits on the branch)

| Commit | What it did |
|---|---|
| `f29844c` | Match a look made visibly correct, safe and honest: learned originals (ADR-0161), scene-prior tone (ADR-0180), a reference of another scene matched as a look rather than repainted, a stretch limit against bands, honest refusals. |
| `4ebc598` | Measured what neutralising first and then copying could still gain (ADR-0181): white balance is already at its ideal; per-hue colour does not carry across scenes. |
| `4bf7401` | CI green: the two accuracy gates now test the identifiable half (colour); TLS dependencies upgraded for three advisories, X25519-only key exchange for the fourth. |
| `7b3880a` | The Exact goal (ADR-0182), the near-black band guard, Apply back-off, and the fix for CI jobs hanging six hours (the image-work slot is reentrant per thread). |
| this commit | The fixes from the full in-app test (section 3) and this report. |

## 3. What was fixed in this round

Found by running every tool and every value in the real window:

| Problem | Cause | Fix |
|---|---|---|
| Moving a colour band (e.g. orange luminance to -100) changed neutral greys by 34-55 levels. | A band applied in full to any pixel with saturation above 0.0001, and a grey card always carries a trace of cast. | The eight HSL bands fade in with saturation: nothing below 3 %, all of it from 15 %. CPU and shader changed together; engine string `+hsl.2`. Test: `hsl_barely_moves_a_nearly_neutral_pixel`. |
| On a photo with nobody in it, body slim/height/legs/torso and subject exposure/warmth/clarity changed 8-96 % of the pixels. | The learned subject selection finds the most prominent *thing* (here a pot of offerings) and was written into the body region, so the people tools warped and relit it. | On a frame with no face, the selection may separate the background only; every region that needs a face stays empty, as the portrait parse already guarantees. Engine string `+faceless.1`. Test: `a_subject_selection_is_never_a_person_on_a_frame_with_no_face`. |
| Auto advanced retouch reported its quality check as not passed on a wide stage photo. | It retouched four faces but checked only one: at the 1600 px check size the three seated guests' eyes were 13-18 px apart, below the 20 px the contour check needs. | When a retouched face is too small, quality control renders larger - a bigger screen render, or the full resolution an export is made at - until every face's eyes are 24 px apart. On that photo all four faces are now checked at 3600 x 1772 and the check passes (35 s instead of 33 s). Test: `quality_control_renders_large_enough_to_check_a_small_face`. |
| Test: the Auto portrait button reported "no change". | After the presets, the photo already carries a chosen retouch, so the button deliberately adds only the global enhance, which was already saved. | The test resets the photo first; the button then saves a change. |
| Test: dehaze reported "no change". | Interactive previews skip dehaze by design (and say so). | Dehaze is measured on the export render. |
| Test: "open the Retouch workspace" failed. | A loop reused the variable holding the project's name, so the library button could not be found. | Loop variable renamed. |
| Test: Auto retouch preset "Beauty & Fashion" matched two radios. | Auto advanced retouch offers a preset of the same name. | The test picks it inside the "Retouch preset" group. |
| Test: two retouch tools reported failing on the test spot. | Body-hair softening and shadow removal have nothing to act on in a patch of sky. | Listed as tools expected to find nothing there. |
| Test: the run crashed or saw a blank page. | Another AURA was on the same debugging port, and WebView2 swaps its process when it navigates. | Own port (9391); the test waits for the AURA page and reconnects. |

**Result of the full in-app run after the fixes:** 721 checks - 637 pass, 51 skips, 32 expected
warnings, 1 failure (the Beauty & Fashion test locator, fixed above; the auto-retouch group was then
run again and passed 34 of 34, including every preset, the strength sliders, the Auto portrait button
and Auto advanced retouch). By group: library 17/17, auto edit 16/16, develop 51 + 3 warnings, colour
values **122/122** (4 failures before), finish values 154 + 51 skips + 20 warnings, **0 failures**
(7 before), retouch tools 130 + 7 warnings, **0 failures** (2 before), masks 14/14, looks 4/4,
analysis 49/49, export 7/7, one-click 1/1, system 14/14, UI 26/26. No page errors; every original
byte-identical after the run.

## 4. Warnings that are expected (not defects)

- **Dehaze in the interactive preview**: skipped on purpose for speed; the full-quality preview and
  every export apply it.
- **Lens distortion and chromatic aberration**: no lens profile matches these test photos.
- **Light direction/height, relight direction, backlight**: need the Light or Relight amount above
  zero first; they shape a light that is not switched on.
- **Group balance and group skin**: need two or more people; the test portrait has one.
- **Waist, arms, belly**: not visible in the test portrait's framing.
- **Teeth evening, beard define, eyeliner wing, red recovery, eye symmetry**: nothing for them to act
  on in this portrait (closed mouth, no beard, symmetric eyes, no red clothing).
- **Spot tools on a patch of sky** (red eye, eye clean, blemish, defringe, frequency heal, body hair,
  shadow removal): ran correctly and found nothing to change.
- **51 skips**: one-sided controls (0 to 100) correctly refuse a negative value and change nothing.
- **Autopilot waits**: nothing in this build is calibrated, so it holds decisions for review.

## 5. What is remaining to fix

1. **Exact is not a 100 % copy on every photo, and cannot be safely.** Where the reference's colours
   would clip a bright backdrop, move skin past its allowance, or lift a near-black JPEG backdrop into
   compression blocks, the checks stop it short and the panel says so (example: a dark studio portrait
   keeps 6 % of a warm-punch look; a monochrome reference on a flat photo is refused).
2. **Tone from a single reference of another scene is not identifiable.** One reference cannot tell
   "this look is contrasty" from "this scene is contrasty"; colour is matched, tone is estimated.
3. **The models are placeholders.** Every learned head (faces beyond the Haar detector, scenes,
   emotion, aesthetics, matting) is untrained; quality numbers come from fixtures this repository
   authored. Body tools need a detected face to know where a person is - a person seen from behind
   needs a face box drawn over their head.
4. **Codex's uncommitted work in the main checkout** (`feat/reference-color-workspace`) touches the
   same colour-match files and the same test script. It has to be reconciled with this branch before
   either is merged on top of the other.
5. **No GPU backend, no network transport, nothing signed**: unchanged from the project's state
   (see `CLAUDE.md`).

## 6. What needs to be done next

1. ~~Merge PR #73 once CI is green on this commit.~~ Done 2026-10-26 (`b0c20d4`).
2. Reconcile Codex's main-tree colour-match changes with this branch (rebase whichever lands second
   and rerun `scripts/test-everything-gui.py` plus the 20-photo match test).
3. ~~Run `scripts/test-evoto100-gui.py`~~ done 2026-10-27 (108 checks, 0 failures after the memory
   fix). `scripts/test-effects-gui.py` is still to run.
4. Add a group photograph, a full-length portrait and a photo with a beard and an open smile to the
   in-app test inputs, so the warnings in section 4 become real checks.
5. Test a person seen from behind with a drawn face box, to confirm the body tools follow it.
6. Make the two face finders agree (see the 2026-10-27 update): an ADR, the face boxes saved into
   the recipe, and a run over group photographs.
7. Send previews to the window as binary rather than base64 text (an IPC contract change, ADR first).

## 7. What to achieve next

- **A photographer's blind comparison** of Exact, Palette and Natural against the reference: the
  numbers say Exact is closest; only people can say it looks right.
- **Real training data** for the placeholder heads, starting with face and person detection, which
  every people tool depends on.
- **Speed**: a 1,000-photo folder takes about two hours; the target is under one.
- **Real camera files and a photographed colour chart**, which reopen phase 02's colour criteria.

The in-app results are written to `.work-checks/everything/results.json` by
`scripts/test-everything-gui.py --port 9391` (use a port no other AURA is using).
