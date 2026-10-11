# ADR-0108: Portrait finishing tools (reshape, liquify, background, makeup) and connecting Claude or ChatGPT from the app

Status: accepted.
Date: 2026-10-11

## Problem

The owner asked for AURA to edit "like Evoto" and to carry every tool Evoto has. Most of Evoto's
portrait work was already in AURA - skin smoothing and evening, blemish and acne clearing, wrinkle,
under-eye, shine, teeth, eye redness and detail, red-eye, flyaways, fabric creases, backdrop
cleanup, glasses glare, dodge and burn, makeup tint, AI masks, presets, sync and batch editing.
What was missing falls into four groups, and three of them are things `docs/retouch-ethics.md`
section 2 listed as permanently out of scope:

1. **Face and body reshaping, and liquify.** Evoto's face slim, jaw, chin, forehead, eyes, nose,
   mouth, smile and head size; body slim, waist, hips, arms, shoulders, legs and neck; a push /
   enlarge / shrink / restore liquify brush.
2. **Background replacement.** A solid colour (headshots, ID, e-commerce), a gradient backdrop, a
   blur, and sky replacement.
3. **Feature colour and makeup.** Hair colour, eye colour, lipstick, blush, eyeshadow, brow colour.
4. **"Connect Claude" and "Connect ChatGPT"** buttons that open the provider in a browser.

## Decision

### 1. The tools exist, and they are the photographer's alone

The owner's request is a product decision and it reverses part of ADR-0045's and phase 21's
policy for **manual** editing. It does not reverse it for automation, and that line is enforced:

- Nothing here runs unless a person moves a control. There is no automatic reshape, no automatic
  background swap, no automatic recolour. `crates/aura-app/tests/no_automatic_reshape.rs` fails the
  build if any module other than the panel's command names the extension, so Auto enhance, Auto
  retouch, the advanced workflow and the unattended run cannot reach it.
- `aura-retouch` and the frozen `micro` contract are untouched. Their boundary greps still forbid
  reshaping inside the automatic retouch crates, and they still pass.
- **Skin tone is still never changed.** There is no skin-colour control in this extension and the
  makeup multiply for blush is confined to cheek spots measured from the face. ADR-0101's and phase
  15's rule - no constant a person's skin could be compared against - holds.
- Tattoo, mole and scar protection are unaffected: these tools move or recolour, they never heal.

### 2. Where it lives

`aura_recipe::studio_finish` is a recipe *extension* (`studio_finish_v1`), not a frozen field -
ADR-0102's reason: the frozen shape has nowhere to put a displacement or a stroke. An absent key and
a key of zeroes render the same photograph, and writing the identity removes the key so an untouched
recipe stays byte-identical. `Validation::check` reads it, so an out-of-range value is refused on
every save.

`aura_render::studio_finish` renders it after the retouch stack and before the Studio's local
masks, on the whole frame:

1. colours, through the portrait parse's hair, iris, lips and brow regions; blush and eyeshadow as a
   soft multiply placed from the face landmarks;
2. background, through the parse's background (or sky) region, feathered;
3. shape: every face slider, every body slider and every liquify stroke adds into **one**
   displacement field and the frame is resampled through it once.

The regions and landmarks are re-derived from the pixels, as the portrait operators' are (ADR-0065),
so the four values a delivered file is re-created from still decide every pixel. Each displacement
primitive is bounded so a single one cannot fold (a move at most two fifths of its radius, a scale
inside -0.5..0.5). A frame carrying a finish is rendered whole rather than tiled, and the extension
is excluded from the retouch checkpoint key so moving a slider does not re-run the retouch stack.

Picked colours are stored as sRGB and converted through the inverse of the output roll-off, so a
chosen white exports as that white.

### 3. Honest limits

- Landmarks come from the Haar parse (ADR-0065): eyes, nose and mouth are measured, the jaw, chin
  and forehead are placed relative to them. A face the parse misses is not reshaped; drawing a face
  box in Portrait retouch fixes that.
- Body tools use the parse's body region and work on the largest person. A strong change bends the
  background near the body, as liquify does in every tool.
- Background replacement is only as good as the parse's background plane. It is a colour model
  seeded beside the person, not a trained matting network, so hair edges are feathered rather than
  cut strand by strand. The AI subject matte from ADR-0103 is not yet wired into this path.
- Not built: generative background extension, AI headshot generation, tethered shooting, and any
  skin-tone changer. The last is deliberate and stays out.

### 4. Connecting Claude or ChatGPT

Neither Anthropic nor OpenAI lets a third-party desktop application sign a user in and receive an
API key; a key is created by the account holder on the vendor's own page. So "Connect Claude" and
"Connect ChatGPT" open that page - `console.anthropic.com/settings/keys` and
`platform.openai.com/api-keys` - in the default browser, select the provider in the panel, and the
photographer pastes the key they made into the key field, which stores it in the operating system's
credential store exactly as before. A ChatGPT Plus subscription is not an API key and does not pay
for API calls; the OpenAI page is where API billing is set up.

`aura_cloud::browser` opens **only an address from the catalogue** and only an `https://` one, never
a string from the window, so the IPC command cannot be used to launch an arbitrary program or page.
On Windows it calls `rundll32 url.dll,FileProtocolHandler` rather than `cmd /c start`, so no shell
parses the address. A browser that will not start is reported with the address to open by hand.

## Consequences

- `docs/retouch-ethics.md` and `docs/portrait-retouch.md` now say that reshaping, background and
  colour tools exist as manual tools and are never automatic.
- `docs/evoto-parity.md` maps every Evoto tool to where it is in AURA, and says what is not there.
- Three IPC commands: `studio_finish`, `save_studio_finish`, `open_provider_page`.
