# ADR-0038 - The mask IPC surface

- **Status:** accepted
- **Date:** 2026-08-20
- **Phase:** 18 - Local Mask AI: Automatic Semantic Masking
- **Extends:** ADR-0030 (develop IPC surface), ADR-0032 (tone IPC surface), ADR-0034 (colour
  IPC surface), ADR-0036 (style IPC surface)
- **Deciders:** CTO, Senior Frontend Engineer, ML Lead - Vision, Senior Engineer - GPU &
  Render, Product Manager

## Context

Phase 18 adds eight commands and nine wire shapes.
`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are frozen contracts checked
by `cargo xtask contracts --check`, so every addition needs an ADR and a re-lock, in that
order.

This is the first surface in the product that has to move **a picture of a region** to the
panel. Every surface since phase 09 has carried numbers, verdicts and crop rectangles; a mask
overlay is none of those, and getting its representation wrong is either a panel that cannot
draw a soft edge or an IPC call that ships a megabyte per photograph. Five decisions follow.

## Decision 1 - The overlay crosses the wire as a quarter-resolution 8-bit alpha plane, base64, and never as a full-resolution image

`MaskOverlayDto` carries `width`, `height`, `alphaBase64` and the `level` the plane was
resolved at, and the plane is at most `OVERLAY_MAX_EDGE` on its long edge.

The panel draws an overlay on a preview that is itself a proxy, so a full-resolution plane is
detail nobody can see costing bytes everybody pays. A 512 px long edge is 512 × 341 = 175 KB
raw and about 20 KB after the run-length the payload already carries, which is inside the
50 ms budget every command in this product has.

The alternative - sending a PNG - was rejected because it puts an encoder on the command path
and because the panel wants the alpha values themselves for the brush, not an image of them.

## Decision 2 - `confidence` and `edgeQuality` are separate fields and the panel renders both

`MaskDto` never collapses the two into one "quality" number. ADR-0037 decision 6 has the
argument; the consequence here is that `MaskPanel.tsx` shows two bars and a sentence naming
which of the two is limiting what may be done, and `MaskPanel.test.tsx` asserts that a mask
below `AGGRESSIVE_FLOOR` renders the sentence rather than only a colour.

## Decision 3 - `allowance` is computed once, in Rust, and sent

`MaskDto::allowance` is the `[0, 1]` strength ceiling phases 19 to 24 multiply by. It is on
the wire even though the panel could compute it from the two quality numbers, because two
implementations of a gating rule is two answers to "may this mask carry skin smoothing", and
the one in TypeScript is the one nobody tests against a fixture.

## Decision 4 - Editing a mask is one command with an explicit op, not a stream of brush points

`edit_mask` takes a `MaskOpDto` - `union`, `intersect`, `subtract`, `feather`, `grow`,
`shrink`, `invert` - with either another mask id or an explicit stroke plane as the operand.
The panel accumulates a stroke locally and sends it once on pointer-up.

A per-point command would be a command per animation frame, which breaks the 50 ms rule by
volume rather than by latency, and it would make undo a replay of two hundred rows. The
existing `image_history` from phase 14 already gives the photographer one undo step per
deliberate edit, which is what a photographer means by one.

`edit_mask` sets `userEdited` and there is no argument that clears it. The one thing that
clears it is `regenerate_mask`, which is a separate command with its own confirmation, and
ADR-0037 decision 7 is why.

## Decision 5 - `mask_status` reports `selected` and `masked` as two numbers, not a ratio

The denominator argument is ADR-0037 decision 8. On the wire it means the panel can say "312
of 340 selected frames have masks" rather than "92 %", and a photographer who is looking at
a project where phase 12 has not run yet sees `selected: 0` rather than a coverage figure
computed against a denominator that does not exist.

## Consequences

- Nine new shapes in `ipc.rs` and nine in `types.ts`; `contracts.lock` is re-locked in the
  same commit.
- No command on this surface applies a mask to pixels. Section 2.2 of the phase document puts
  that in phases 19 to 24, and there is no `apply_mask` here to be tempted by.
- The overlay path is the only place in the product where a plane of pixel-shaped data
  reaches the panel. It is derived data about a region, not an image of the photograph, and
  `MaskOverlayDto` has no field that could hold one.
