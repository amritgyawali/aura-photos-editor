# The AURA edit recipe, schema v1

Every edit AURA makes is a JSON document beside your photograph. Your RAW files are never
modified - not by an automated pass, not by a retouch, not by an export. This page is the
published shape of that document, for anyone writing a tool that reads or writes one.

The authoritative definition is `crates/aura-recipe/src/contract/recipe.rs`. It is a frozen
contract: changing it requires an ADR and a re-lock of `contracts.lock`.

## The document

```json
{
  "schema": 1,
  "engine": "aura-render/1.0.0",
  "image": { "content_hash": "a3f2...", "camera": "ILCE-7M4", "profile": "adobe_standard" },
  "global": {
    "exposure": 0.31, "contrast": 11,
    "temperature": 4930, "tint": 8,
    "highlights": -31, "shadows": 22, "whites": 7, "blacks": -9,
    "clarity": 6, "texture": 4, "dehaze": 0,
    "vibrance": 8, "saturation": 0,
    "curve": { "points": [[0,0],[64,58],[128,132],[255,255]] },
    "hsl": { "orange": { "h": -2, "s": -6, "l": 4 } },
    "sharpen": { "amount": 40, "radius": 0.8, "detail": 25, "masking": 30 },
    "noise": { "luminance": 22, "colour": 30, "detail": 50, "model": "scene_aware_v1" }
  },
  "lens": { "distortion": true, "vignette": 60, "ca": true, "profile": "FE 35mm F1.4 GM" },
  "geometry": { "rotate": -0.6, "crop": [0.02,0.01,0.97,0.98], "perspective": null },
  "masks": [
    { "id": "m1", "kind": "face", "target": "identity:bride", "feather": 0.35,
      "params": { "exposure": 0.18, "shadows": 8, "clarity": -4, "temperature": -60 } },
    { "id": "m2", "kind": "background", "invert_of": "subject", "feather": 0.5,
      "params": { "exposure": -0.22, "saturation": -6 } }
  ],
  "retouch": [ { "op": "skin_smooth", "strength": 0.35, "protect_texture": 0.8, "mask": "skin" },
               { "op": "glare", "strength": 1.0, "protect_texture": 0.0, "mask": "eyes",
                 "borrowed_from": "pht_9f31" } ],
  "restoration": { "denoise": "auto", "face_recovery": 20, "deblur": 0 },
  "bw": null,
  "provenance": {
    "scene": "indoor_ceremony", "style_profile": "amrit_v3",
    "confidence": 0.982, "decision_id": "d_8f21", "source": "ai",
    "user_edited_fields": []
  }
}
```

## Ranges

| Field | Range | Meaning |
|---|---|---|
| `global.exposure` | `-5.0 … 5.0` | Stops. |
| `global.contrast` | `-100 … 100` | An S-curve about middle grey. |
| `global.temperature` | `2000 … 50000` | Kelvin. 5500 is neutral. |
| `global.tint` | `-150 … 150` | Positive is magenta, as in Lightroom. |
| `global.highlights`, `shadows`, `whites`, `blacks` | `-100 … 100` | Four overlapping tone bands. |
| `global.clarity`, `texture` | `-100 … 100` | Local contrast at a coarse and a fine radius. |
| `global.dehaze` | `-100 … 100` | Lifts or restores the frame's own black floor. |
| `global.vibrance`, `saturation` | `-100 … 100` | Weighted and flat saturation. |
| `global.curve.points` | `[[0,0] … [255,255]]` | At least two points, x strictly increasing, spanning 0 to 255. |
| `global.hsl.<band>` | `-100 … 100` each | Bands: red, orange, yellow, green, aqua, blue, purple, magenta. |
| `global.sharpen.amount` | `0 … 150` | |
| `global.sharpen.radius` | `0.5 … 3.0` | Pixels. |
| `global.sharpen.detail`, `masking` | `0 … 100` | |
| `global.noise.*` | `0 … 100` | |
| `lens.vignette` | `0 … 100` | Correction strength, not an effect. |
| `geometry.rotate` | `-45.0 … 45.0` | Degrees, **positive clockwise**. |
| `geometry.crop` | `[left, top, right, bottom]` in `0 … 1` | `right > left`, `bottom > top`. |
| `masks[].feather` | `0.0 … 1.0` | |
| `retouch[].strength`, `protect_texture` | `0.0 … 1.0` | |
| `retouch[].borrowed_from` | a photo id, or absent | **The disclosure.** Present only on an operation whose pixels came from another photograph. |
| `provenance.confidence` | `0.0 … 1.0` | |

**`borrowed_from` is how a delivered file says it is a composite.** Added by phase 21 and
optional, so a recipe written by any earlier build reads unchanged. It is in the recipe rather
than only in the catalog because a delivered file has to be re-creatable from the RAW hash, the
recipe, the engine string and the output spec - and a composite whose source is in none of those
four cannot be re-created or audited. AURA never composites two photographs without writing it
here; `docs/retouch-ethics.md` section 5 lists the four other places the same fact appears.

**A value out of range is clamped, not refused.** An exposure of +9 renders at +5. A *shape*
that has no correct interpretation is refused with `AURA-RENDER-8002`: a curve that goes
backwards, a crop with no area, two masks sharing an id, a retouch operation naming a mask
that is not in the document.

## Canonical form

The bytes on disk are the canonical form, and the recipe's identity is BLAKE3 over them.
Three rules:

1. Keys sorted byte-wise.
2. No whitespace between tokens.
3. Every non-integral number written with exactly six decimal places. Integral values keep
   their integer spelling, and negative zero is written as zero.

That third rule is why a parameter is effectively quantised to a millionth - four orders of
magnitude finer than any control in the product - and why a document written on one machine
hashes identically on another.

## `user_edited_fields`

The list of dotted paths a person has touched, sorted. **No automated pass may change a path
in this list.** Not an AI pass, not a preset, not the QC agent. The rule is enforced in
`aura_recipe::schema::merge`, which is the only function in the product that writes one
recipe into another, and there is no argument that switches it off.

The only way a path leaves the list is a photographer choosing "reset to AI suggestion",
which hands that field back deliberately.

## Versioning

`schema` is 1 today. Three promises hold for every future version:

- **A field is never removed.** A v2 that stops using a parameter keeps reading it.
- **An unknown field is preserved.** An older build that opens a newer document renders what
  it understands, warns with `AURA-RENDER-8003`, and writes the rest back untouched. Opening
  a project in an older version cannot destroy work done in a newer one.
- **A migration is tested against a frozen document**, not against itself.
  `crates/aura-recipe/tests/fixtures/recipe_v1_golden.json` is that document.

## Interchange

Two files sit beside your RAW:

- `IMG_0042.xmp` - the twenty-four parameters Adobe's `crs:` namespace defines with the
  meaning we mean. A Lightroom user who opens the folder gets an edit that works.
- `IMG_0042.aura.json` - the whole recipe, canonical. Masks, retouch, restoration and
  provenance have no `crs:` equivalent that means the same thing, and writing an
  approximation would be worse than writing nothing.

If both exist, the AURA sidecar wins and the XMP is compared against it. A difference means
you edited in Lightroom, and those fields become yours - protected from every automated pass
from then on.

## What a recipe cannot say

There is no destination in this document. No output path, no filename, no format, no
"deleted" flag. A recipe describes an edit; it cannot perform one, and it cannot name a file
to perform it on. That is invariant 1 as a property of the shape rather than a rule somebody
has to remember.
