# Evoto-style workflows in AURA

This describes available software workflows, not shared proprietary technology or
a demonstrated quality match. On 2026-10-11 the six public homepage before/after
pairs were downloaded and visually inspected as reference targets. A paired
native AURA benchmark has not established quality equivalence with those
examples. The images do not disclose Evoto's proprietary algorithms.

## Delivered workflows

- Paired finished-reference matching: choose a JPEG/PNG export of the same photo
  in the studio to fit nine native tone/colour settings. AURA keeps saved healing
  and manual settings, measures improvement at full resolution before saving,
  and reports strict equality separately. See the
  [workflow and limits](paired-reference-matching.md).
- Face and visible body skin: independently measured skin selections, localized
  blemish repair, pore refinement, texture restoration, skin tone and light controls.
  Available skin can include nose, forehead, cheeks, neck, hands and other body
  regions; segmentation is not a universal skin-coverage guarantee.
- Portrait skin finish: detailed face/body healing with measured tone, redness,
  shine and under-eye correction, while preserving iris, teeth and face shape.
  Compact repair slots are shared across detected faces. See the
  [paired native validation](evoto-paired-validation.md) for evidence and limits.
- Manual cleanup: painted Acne Clear blemish brush, patch healing and donor
  controls, cloning, editable selections, and saved before/after coverage.
- Portrait detail: measured dark-circle correction, eye and teeth controls,
  makeup, and protections for eyes, nostril openings and nose structure.
- Advanced retouch: eighteen reported stages, including hair, backdrop and
  clothing cleanup, skin correction, dodge and burn, grading and quality checks.
  Stages can be unchanged, not applicable or protected rather than inventing edits.
- Masked adjustments: subject, background, sky, face/body skin, hair, clothes and
  facial regions, with brush, gradients, intersection/subtraction and local sliders.
  Learned selections depend on the documented optional model files being installed.
- Collection cleanup preset synchronization: **Apply cleanup settings to this
  collection** in Retouch. Each image receives fresh native analysis. Outcomes are
  reported individually, and **Stop after current photo** prevents another start.
  It preserves existing global grading and manual repairs; it does not copy coordinates.
- Independent collection editing, reference looks, personal styles from Lightroom,
  full-quality previews, reversible history and verified full-resolution exports.
- Grayscale and very high/low contrast inspection. These views aid review; they
  do not change the saved recipe or export color.
- **Hair, clothes & makeup color**: choose an arbitrary sRGB color in native
  Retouch. The renderer converts it to linear Rec.2020 and transfers chroma while
  preserving source luminance and fine detail. Use Color brightness to lighten or
  darken the choice by up to four stops, including darkening gray/white hair; its
  neutral position preserves source luminance. Choose an available saved hair,
  clothing or facial-region selection, or paint a new selection. No new semantic
  region detector is supplied by this control.
- **Solid background color**: composite a chosen color only inside the saved or
  painted background selection. Strength and feather control blending; the
  subject is protected by the chosen mask, which must be reviewed.
- **Local face & body proportions**: explicit ellipse-based width/height warp,
  bounded to 25% at full control strength and tapering at the ellipse boundary.
  Works on a chosen face, nose, waist, arm or leg region without running a model.
  This changes photographed structure and is never added by automatic skin cleanup.
- **Internal AI setup**: Advanced > AI provider shows routes imported from the
  local configuration file, refresh controls and fallback status. Browser key-page
  buttons have been replaced. Credentials remain in the operating-system secure
  store; API access and chat subscriptions are separate.

## Gaps and quality acceptance

The full catalogue is not implemented. The chosen-color and local proportion
tools above are manual native tools, not claims of Evoto's anatomical automation.
Image background replacement, transparent export/contact-shadow synthesis, eye
opening, expression synthesis, complete makeup landmarks, generated hair filling,
pet retouching, cloud galleries and video retouch remain incomplete or absent.
Optional cloud/model flows need their own configured runtime and real-photo
validation. See [the route inventory](evoto-feature-inventory.json), which records
all 143 published catalogue routes, including overlaps, use cases and video pages.

ChatGPT subscription sign-in is not implemented by importing API keys. The
[official documentation](https://developers.openai.com/siwc/request-client-id)
describes commercial client registration; its
[open-source integration](https://developers.openai.com/siwc/token-sharing-open-source)
has a distinct PKCE/loopback, token-validation and Responses API flow. AURA's
current provider gateway uses API keys and must not label a dashboard visit or
a pasted chat-session token as that sign-in flow. Claude is connected through its
[documented API-key authentication](https://platform.claude.com/docs/en/api/overview).

## Homepage comparison observations

The [public homepage](https://www.evoto.ai/) supplies six example pairs at
`res.evoto.ai/ui/www/images/pages/homeV3/section04-<category>-{before,after}.webp`.
They were inspected at their delivered 2000x1200 size. These are marketing exports,
not lossless source photographs or disclosures of model weights, masks or recipes.

- Beauty: facial spots and uneven color are reduced across three different skin
  tones; eyes, teeth, hair and face contours remain identifiable.
- Headshots: local skin contrast and under-eye shadows are softened; beard/hair,
  clothing and existing expressions remain important acceptance targets.
- Wedding: global color and lighting change along with facial cleanup; skin-only
  retouch cannot reproduce the complete example grade by itself.
- Events: strong colored ambient light remains while face/body light and color
  become more even. A universal neutral-white-balance pass would lose that intent.
- Maternity: abdominal stretch marks are reduced with the broad lighting and
  belly shape maintained; a compact-acne detector alone does not cover long marks.
- Newborn: clustered cheek redness and forehead/chin marks are reduced; eyelids,
  nostrils, lips and fine hair need separate protection.

These observations inform measurable acceptance tests. They cannot identify a
unique algorithm, guarantee identical output, or certify AURA's current quality.
Reference images remain local evidence and are not included in application assets.

The heavy-acne validation portrait still has residual marks and visible pores.
The saved planner report and a verified export establish execution and integrity;
they cannot establish complete defect removal or professional visual quality.
Any stronger quality claim requires paired native exports at original resolution,
original-hash checks, region-level eye/nose inspection, a varied face/body dataset
and actual reference outputs. See `retouch-recovery-validation.md` for measured
results and limitations.

Official product references used to identify workflow gaps:
[portrait retouching](https://www.evoto.ai/features/portrait-retouching),
[batch edits](https://www.evoto.ai/features/batch-edits), and
[background removal](https://www.evoto.ai/features/ai-background-remover).

The manual color/proportion tools and the earlier browser setup were run in the
native desktop app. See [their dated validation](evoto-tools-native-validation.md)
for measured boundaries, history, full-resolution export, provider setup and limits.
The browser setup has since been replaced by internal routing.

Internal provider routing now replaces the browser connection buttons. The
configured local file, fallback, daily ratings, bounded native skin advice,
four-pass healing and read-only residual review are documented in
[Internal AI and native skin repair](internal-ai-workflow.md). These extensions
do not establish complete Evoto parity or guaranteed complete blemish removal.
