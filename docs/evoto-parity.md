# Evoto parity

Every tool Evoto offers, where the same thing lives in AURA, and whether AURA does it
automatically. Where AURA does not have something, this page says so rather than implying it.

**Where to find things.** *Studio* is the Photo editor (Develop). *Retouch* is the **Retouch**
button inside it. *Portrait studio* is the **Portrait studio: face, body, background, makeup**
section in the Studio's right-hand panel (ADR-0108). *Automatic* means Auto enhance or Auto
advanced retouch sets it without anybody touching a control.

## Evoto's homepage before/after, measured

Evoto's homepage shows each use case as a before/after slider. The *Beauty & Fashion* card ("High-end
skin work, clothing wrinkle removal, and hair refinement handled in a single automated pass") was
measured pixel by pixel across its divider (ADR-0109):

| What changes | Measured | AURA |
|---|---|---|
| Grey studio backdrop | sRGB 206 → about 234, the same ×1.13 on R, G and B; fall-off kept; not clipped | **Bright backdrop** lift: 206 → 232.5 on a painted studio portrait, neutral (`crates/aura-app/tests/beauty_fashion.rs`) |
| Stray hairs over the backdrop | Present before, gone after; soft wisps at the hair's edge kept | Hair stage fades separate strays, keeps the soft outline |
| Clothing creases | Strongest crease detail about 18 % lower; weave kept | Fabric creases softened (0.5 in the preset) |
| Skin | Pores kept; each person's own tone kept | Skin stages; skin tone never moved |

Run it with **Retouch → Auto advanced retouch → Beauty & Fashion (Evoto-style)**, or pick the
*Beauty & Fashion (Evoto-style)* preset in the automatic retouch settings. The other homepage cards
could not be measured from here (the site is unreachable from the build environment); send
screenshots of them and each one gets the same treatment.

## Skin and face retouching

| Evoto | AURA | Automatic |
|---|---|---|
| Skin smoothing | Retouch → Skin smoothing · protect detail; frequency separation | Yes, sized per face |
| Remove acne / blemishes / spots | Retouch → Acne & blemish clear, Auto spot cleanup, Frequency healing | Yes, temporary marks only |
| Keep moles, freckles, birthmarks | Protected by default; heal one by hand if you choose | Yes (protection) |
| Wrinkle removal (forehead, crow's feet, smile lines, neck lines) | Retouch → Soften fine lines | Advanced workflow |
| Eye bags / dark circles | Retouch → Under-eye shadow lift (measured against the cheek) | Yes |
| Even skin tone / uniformity | Retouch → Even sampled skin tone, Match complexion to sample | Yes |
| Skin colour correction (redness, yellowness) | Retouch → Skin color correction | Redness only |
| Remove oily shine | Retouch → Reduce oily shine | Yes |
| Restore skin texture / pores | Retouch → Restore skin texture | Yes |
| Body skin smoothing | Retouch on the body-skin selection | Advanced workflow |
| Face lighting / 3D contour, dodge and burn | Retouch → Dodge, Burn, Micro dodge & burn, Skin dodge and burn | Advanced workflow |
| Skin tone changer (lighten / darken) | **Not provided, deliberately.** AURA never moves a person's skin tone. | - |

## Eyes, teeth, lips, hair

| Evoto | AURA | Automatic |
|---|---|---|
| Teeth whitening | Retouch → Natural teeth whitening | Yes, when the mouth is open |
| Eye brightening, red veins | Retouch → Eye redness reduction, Iris and eyelash detail | Yes |
| Red-eye | Retouch → Flash red-eye correction | Yes |
| Eye colour | Portrait studio → Makeup & colour → Eye colour | No - manual only |
| Lipstick | Portrait studio → Lipstick | No - manual only |
| Blush, eyeshadow, brow colour | Portrait studio → Makeup & colour | No - manual only |
| Hair colour | Portrait studio → Hair colour | No - manual only |
| Flyaway hair removal | Retouch → Heal blemish / flyaway / lint | Advanced workflow |
| Hair definition | Portrait retouch → Hair define | Yes |

## Face and body reshaping

All manual. No automatic pass ever reshapes anybody (`crates/aura-app/tests/no_automatic_reshape.rs`).

| Evoto | AURA |
|---|---|
| Face slim, jawline, chin, forehead, cheekbones | Portrait studio → Face |
| Eye size, eye distance | Portrait studio → Face |
| Nose slim, nose length | Portrait studio → Face |
| Mouth width, lip fullness, smile | Portrait studio → Face |
| Head size | Portrait studio → Face |
| Body slim, waist, hips, arms, shoulders | Portrait studio → Body |
| Leg lengthening, neck lengthening | Portrait studio → Body |
| Liquify: push, bloat, pinch, restore | Portrait studio → Liquify (Push, Enlarge, Shrink, Restore) |
| Double chin | Portrait studio → Face → Jawline and Chin length |

## Background, clothing, cleanup

| Evoto | AURA | Automatic |
|---|---|---|
| Background replacement (colour, white for ID / e-commerce) | Portrait studio → Background → Solid colour, or the *Clean white backdrop* preset | No |
| Gradient studio backdrop | Portrait studio → Background → Gradient, or *Studio grey gradient* | No |
| Background blur | Portrait studio → Background → Blur; Portrait retouch → Background blur | No |
| Sky replacement | Portrait studio → Background → Replace sky (*Blue sky*, *Sunset sky*) | No |
| Backdrop cleanup (seams, wrinkles, dirt) | Retouch → Clean backdrop / Backdrop smoothing | Advanced workflow |
| Clothing wrinkle removal | Retouch → Fabric crease softening | Advanced workflow |
| Lint, threads, stains | Retouch → Heal blemish / flyaway / lint | Advanced workflow |
| Glasses glare removal | Retouch → Glare softening | Advanced workflow |
| Object / passer-by removal | Cleanup → Manual remove; Retouch → Clone stamp, Texture-aware patch heal | **Refuses** unclassified removals on real photos (ADR-0049) |
| Background extend (generative) | **Not in AURA.** | - |

## Colour, presets and workflow

| Evoto | AURA | Automatic |
|---|---|---|
| White balance, exposure, contrast, highlights, shadows | Studio → Basic, white-balance picker | Yes |
| HSL, colour grading, curves, calibration, grain, noise reduction | Studio (see `docs/lightroom-parity.md`) | From profiles |
| AI colour match / match a reference look | Match a look (ADR-0063), Personal style from Lightroom (ADR-0104) | Yes |
| Presets / looks, save your own | 21 edit profiles, Develop → Presets, finishing presets in Portrait studio | Yes |
| Batch editing, sync settings | Sync settings to all photos, Auto edit all photos, Autopilot | Yes |
| Consistent results for the same person across a batch | Per-person retouch strength (phase 20), gallery consistency (phase 25) | Yes |
| AI culling | Cull (phase 12) | Yes |
| Masks: subject, sky, background, people, brush, gradients | Studio → Masking (ADR-0102, ADR-0103) | AI selections on request |
| Export presets, watermark | Delivery (phase 30), watermark panel | Repeats the last export |
| Tethered shooting | **Not in AURA.** | - |
| AI headshot / ID photo generation | **Not in AURA** - a white backdrop plus a crop does the ID layout; nothing is generated. | - |

## AI accounts

| Evoto | AURA |
|---|---|
| (Evoto uses its own cloud) | Bring your own AI: **Connect Claude** and **Connect ChatGPT** in Settings → AI keys and on the first-run screen open the provider's key page in your browser; paste the key you create there. Nineteen providers in all - `docs/using-your-own-ai-key.md`. |
