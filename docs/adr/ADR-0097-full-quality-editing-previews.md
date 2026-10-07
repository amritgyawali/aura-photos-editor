# ADR-0097: Full-quality editing previews, rendered once and kept

Status: accepted
Date: 2026-10-07

Numbered 0097 because 0095 and 0096 are taken by uncommitted work in another checkout
(disposable editing previews, complexion-faithful automatic retouch) that has not landed yet.

## Problem

A photographer asked for the editor to show the full-quality original rather than an optimised
image, to load fast every time, and to still be there after visiting another section.

What the editor showed was a screen-sized render made from the 2048-pixel proxy - itself a JPEG
re-encode of the original - and the Original view was that proxy. Every visit rendered again:
the Photo Studio unmounted when another section opened, nothing on the window side was kept,
and the renderer kept nothing either. A retouched 6 MP portrait took 18 s at screen size and
91 s at full resolution on the reference laptop, every time.

## Decision

### The original's own resolution (`crates/aura-app/src/preview_render.rs`)

The Studio's edited and original views and the Retouch view's after and before are rendered at
`RenderLevel::Full`, from the original file. A full-resolution preview runs every stage, as an
export does (`RenderPurpose::Export`), so what is shown is what will be delivered - apart from
the crop and effects the Retouch view leaves off. The Original view is the neutral recipe
rendered the same way (`photo_original`), not the proxy.

A quick screen-sized look (1600 px, from the proxy) is still made, for the seconds before the
full-quality preview is ready and for live drafts while a brush is moving; it is replaced as soon
as the full one arrives, and the status line says which one is on screen.

### Rendered once (memory and disk)

Every editing preview goes through one function that keeps finished previews in memory (384 MiB,
least recently used out) and on disk (`cache/edited-previews-v1`, 3 GiB, least recently used
out, written beside its final name and renamed into place). It never takes the last 2 GiB of
the disk it is on: when that disk is nearly full, previews are kept in memory only. The key is the photograph, the
original's content hash from the catalog, the level, the purpose and the renderer's own hash of
the request (canonical recipe, engine, output). A changed edit, original or engine is a new key,
so nothing stale is shown; a truncated file is a miss. Unsaved drafts are held in memory only.
**Clear cache** clears both. Delivery and analysis never use this cache.

### Kept in the window

`ui/src/state/previewCache.ts` keeps the previews the window has received for the session
(640 M characters, counting the displayable copy), keyed by project, photograph, view and recipe
hash. `useProgressivePreview` shows a cached full-quality preview at once and asks for nothing;
otherwise it asks for the quick look and the full one together, shows the quick one, then the
full one. While a new version loads, the last picture of the same photograph stays on screen;
another photograph's never does. A payload that is not exactly the pixels it claims is shown
once and never kept. The Photo Studio now stays mounted while another section is open, so the
photograph, an open retouch and the zoom are exactly where they were on return.

Edits wait only for the quick look of a new version; the full-quality one follows without
blocking. Coverage overlays are measured at the quick size and drawn over the full-quality
picture by sampling the coverage cell each pixel falls in.

### Faster rendering

Making full resolution the default exposed two costs in the retouch renderer:

- `bands::blur`, used by every frequency-separation tool, summed every sample of every window,
  so its cost grew with the radius - and radii are several times larger at full resolution. It
  now keeps a running sum, as its own comment always said it did.
- Blurs, local percentiles, the skin filters' box means, acne clear's per-cell work and the
  generic tools' per-pixel loop now run across the processor's cores. Each output is computed
  by one task in a fixed order and the division of work depends only on the frame and radius,
  never on the number of cores, so a render is still the same on every machine.

Measured (`crates/aura-app/tests/full_quality_preview.rs`, development build, 8 threads, after
Auto advanced retouch):

| | Before | After |
|---|---:|---:|
| 6 MP portrait, quick look | 18.1 s | 4.8 s |
| 6 MP portrait, full quality, first time | 91.4 s | 17.4 s |
| 6 MP portrait, full quality, again (memory) | 91.4 s | 0.06 s |
| 6 MP portrait, full quality, after restart (disk) | 91.4 s | 0.07 s |
| 28 MP camera JPEG, full quality, first time | - | 59.4 s |
| 28 MP camera JPEG, again / after restart | - | 0.2 s / 0.3 s |

The window adds the transfer and decoding of the pixels, roughly a second for a 28 MP image,
and nothing when the preview is already held in the window.

## Consequences

- What is on screen is the original's resolution and the delivered rendering; zooming in shows
  real detail rather than an enlarged proxy.
- The first full-quality preview of a new edit still takes time - most of it acne clear and the
  frequency-separation tools at full resolution - and a 28 MP retouched frame about a minute on
  this laptop. The quick look covers that time; every later view is immediate.
- Memory: a 28 MP preview is about 86 MB of pixels in the app and about 230 MB in the window;
  both caches are bounded and evict the least recently used.
- Live drafts stay at the quick size; once applied, the saved result is shown at full quality.
