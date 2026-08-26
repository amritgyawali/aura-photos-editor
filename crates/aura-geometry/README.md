# aura-geometry

Finishing the frame: what the optics did, how far the world is off level, whether the
architecture is square, and what may be removed from the edges.

Phase 23. Contract frozen in `aura-core/src/contract/geometry.rs`; decisions in
[ADR-0041](../../docs/adr/ADR-0041-geometry-lens-straightening-and-crop-safety.md) and
[ADR-0042](../../docs/adr/ADR-0042-geometry-ipc-surface.md).

## What this crate is for

It is the first crate in the product that decides to **remove** something from a photograph.
Twenty-two phases decided what is delivered, what it is of, whether it worked, how it should
look and how light moves inside it. None of them took anything away.

That asymmetry is the design brief. A wrong exposure looks wrong on the screen it was decided
on; a frame with somebody's hand missing from the edge looks like a frame, until it is printed.
So the headline behaviour is restraint - **seven photographs in ten are delivered exactly as
they were shot** - and eleven of the twenty-four reason codes describe something the product
declined to do.

## Layout

| Module | What it holds |
|---|---|
| `profiles` | The bundled lens table. Refuses a row with no attribution, and a duplicate lens id rather than resolving it by directory order. Interpolates a zoom in **log** focal length. |
| `rules` | `config/crop_rules.toml`. One row per scene, a written reason on each, and the loader may only make a safety rule **stricter**. |
| `lens` | Section 6.1's three routes, the edge-chain tracker and the filtered fit. Maps a protected region into the corrected frame. |
| `straighten` | The confidence gate, the angle band, and the *solve* - the rotation is reduced until its implied crop is safe, or abandoned. |
| `keystone` | A restricted Hough for near-vertical lines, the convergence ratio, and the cap it is refused past rather than clamped to. |
| `safety` | The hard filter. Runs **before** the objective. |
| `crop` | The composition objective and the bounded candidate lattice. |
| `variants` | The album, social and wide crops. Not subject to the improvement margin; subject to every safety rule. |
| `plan` | Section 8's seven steps, in section 8's order. |
| `store` | Migration 20. `user_edited` checked inside the statement; ordinal zero derived rather than written. |
| `api` | **Frozen service.** `Geometry` and the resumable `GeometryPass`. |
| `guard`, `errors` | The contract's predicates turned into AURA-ML-5090 to 5095. |
| `fixtures` | Synthetic frames whose geometry was painted into the pixels. |

## Four things to know before changing anything here

**A crop that cannot be proven safe is not a candidate.** `safety::filter` runs before
`crop::Objective` ever scores a rectangle. A filter applied afterwards invites exactly one
repair - nudge the winner until the face is back inside - and a nudged crop is a different
aspect ratio, a different resolution, or a fresh violation at the opposite edge. Nobody writes
a test for the nudge, because the nudge *is* the fix.

**This crate owns no renderer, no face detector and no pose model.**
`crates/aura-geometry/tests/no_render_calls.rs` is a grep that fails the build if one appears.
`ProtectedRegion` is the input port phases 06 and 11 fill; the optics maths lives in
`aura_raw::colour::lens` so the decision and the renderer cannot disagree about where a face
landed.

**The frame as shot is index zero of every plan, and it is not stored.** It is a pure function
of `rotate_deg` and the frame's aspect, regenerated on read - which is stronger than storing
it, because a stored row is a row somebody can delete.

**Every number in every gate is measured against synthetic frames.** There are no wedding
photographs and no expert crop labels in this repository. See conditions C1 to C4 in
`docs/progress/PHASE-23-EXIT.md` before quoting a result.

## Running the gates

```bash
cargo test -p aura-geometry                    # 85 unit tests, 23 gates, 2 greps
cargo run -p aura-cli -- verify --phase 23     # the assembly proof
python ml/eval/crop_agreement.py --self-test   # the expert-crop arithmetic
```
