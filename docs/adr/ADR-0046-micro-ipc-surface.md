# ADR-0046 - The micro-retouch IPC surface

**Status:** accepted · **Date:** 2026-08-21 · **Phase:** 21 · **Supersedes:** nothing

The second of phase 21's two ADRs. [ADR-0045](ADR-0045-micro-retouch-and-cross-frame-borrowing.md)
covers the decisions; this covers the wire. For every phase since 15 that has been a short
document about shapes. This one is not, for a single reason: **this is the first surface in the
product that can carry pixels from one photograph into another**, and a delivery whose composites
are not on the wire is a delivery whose composites are not disclosed.

## 1. Context

Phase 20's surface was the first whose subject was a person rather than a photograph. This one
inherits that and adds three differences of its own:

- one operation - a glare repair - **composites two photographs**, and the product has promised
  in `docs/retouch-ethics.md` that no such thing is ever hidden;
- the phase's guarantee is three independent measurements rather than one, and phase 18's rule
  says two numbers that fail differently must not be collapsed into one;
- what a studio configures here is **which operations exist at all**, not how strong they are, so
  the shape of the configuration command is itself part of the promise.

## 2. Decision: nine commands, and what each of them may touch

| Command | Reads | Writes |
|---|---|---|
| `micro_status` | the project outline | nothing |
| `image_micro` | one plan | nothing |
| `micro_composites` | `v_micro_composites` | nothing |
| `micro_review_queue` | the weakest plans | nothing |
| `micro_matrix` | one project's switches | nothing |
| `set_micro_matrix` | one project's switches | one `micro_matrix` row |
| `accept_micro` | one plan | `reviewed` |
| `micro_pass` | proxies, regions, faces, scenes, moment siblings | every plan, then every recipe |
| `micro_reason_codes` | the frozen enum | nothing |

`micro_pass` is the only one that reaches a recipe, and it can reach exactly one field:
`recipe.retouch[]`, which phase 14 provisioned for "phases 20 and 21" when it froze the schema.
There is no path from this surface to the global exposure, to the curve, to a mask, to phase 19's
`recipe.masks[]` or to the restoration block. Phases 19 and 20 made the same choice about their
own arrays and the argument is unchanged: a boundary enforced by there being nowhere to write is
a boundary that survives the next person who adds a command.

Two commands are new in kind rather than in shape. `micro_composites` exists because a disclosure
that a caller has to assemble is a disclosure that two callers will assemble differently, and
`micro_reason_codes` is phase 13's rule - the panel's legend is built from the frozen enum, so it
cannot render a code no deciding path can emit.

## 3. Decision: a borrow's source is on the wire, in four places, and none of them is optional

`MicroOpDto::borrowedFrom` carries the photograph a glare repair took its pixels from.
`MicroPlanDto::borrowedFrom` carries the same ids collected per frame. `MicroStatusDto::borrows`
carries the count for the whole project. `micro_composites` returns the list.

That is deliberately four views of one fact, and it is not redundancy for its own sake. Each
answers a different question a photographer is actually asked: *what happened to this region*,
*was this photograph composited*, *does this gallery contain any composites at all*, and *show me
every one of them*. A single per-operation field would answer the first and force the panel to
compute the other three, and a panel that computes a disclosure is a panel that can compute it
wrongly.

The database is what makes the four agree. `micro_op` carries a trigger -
`micro_op_borrow_disclosed` - that aborts an insert of a borrow with no source, and
`v_micro_composites` is the one query all four numbers are derived from. The wire cannot express
an undisclosed composite because the storage cannot hold one.

`GlareMethod` being a two-variant enum rather than a nullable field is the same decision one layer
down, and ADR-0045 section 2 records it.

## 4. Decision: the matrix has switches and no strengths, and that is the whole shape

`MicroMatrixDto` and `SetMicroMatrixInput` carry booleans: five operator switches, five clothing
switches, and one for borrowing. There is no strength field, no ceiling field and no scale factor
anywhere on this surface, and there never will be.

A studio decides *which* small fixes it permits. How far each may go is bounded by
`aura_core::contract::micro`, which is a frozen contract, and lowered - never raised - by
`micro_retouch.toml`, whose loader refuses a file that exceeds a contract bound. If this surface
carried a strength then `docs/retouch-ethics.md` would be a description of the defaults rather
than a promise about the product, and the difference between those two is the entire subject of
this phase.

Borrowing is a switch of its own rather than a mode of the glare switch. They are separable wants:
a studio can reasonably want reflections calmed and want no composited pixels in anything it
delivers, and collapsing them would make the second unreachable without giving up the first.

A wrong-length list is refused rather than padded. A panel that sends four switches for five
operations has a bug, and defaulting the fifth is how a studio ends up with an operation running
that it believes it switched off - `AURA-ML-5104`, and `read_switches` is the one place it is
checked.

## 5. Decision: three measurements on the wire, with the sample count beside them

`NaturalnessReportDto` carries `catchlightRatio`, `hairEnergyRatio` and `teethExcursion`
separately, plus `measuredOn`, plus `withdrawn` per family.

Three rather than one is phase 18's two-numbers rule applied to three: the ratios fail
independently, they are fixed by different things, and a photographer whose complaint is "the
hairline looks chewed" needs to find a different number from the one whose complaint is "her teeth
look odd". A mean of the three would be a number nobody could act on.

`measuredOn` is phase 20's rule, kept: a ratio measured over eleven pixels is arithmetic rather
than evidence, and a panel that renders it to three decimal places without saying how many samples
it came from is inviting a decision it cannot support. The panel shows the ratios only when the
count is large enough to mean something.

`withdrawn` is a vector of three booleans and `families` is the vector of names beside it, so no
client hard-codes the order. Same shape as `operators` beside `allowed`, for the same reason - the
order is the contract's, and the wire says what it is rather than assuming both ends agree.

## 6. Decision: the operation shape is flat, with a `kind` string

`MicroOpDto` flattens five operators into one struct rather than a tagged union. The five differ
by one or two fields each, and a tagged union would make the panel's list rendering a five-arm
switch over shapes that are largely identical, plus five TypeScript interfaces that all have to be
kept in step with the same Rust enum.

The cost is that a teeth operation carries a zeroed `sclera` field, and that cost is real but
inert: nothing reads a field that its `kind` does not name, and `aura_core::contract::micro` still
holds the tagged version, which is where the guarantee lives. The wire is a rendering of the
contract rather than the contract.

The one field this flattening does *not* weaken is the disclosure. `borrowedFrom` is `None` for
every operator that is not a glare borrow, and the invariant that it is `Some` whenever `method` is
`borrow` is asserted in `micro_commands`, in the trigger and in
`crates/aura-core/tests/micro_contract.rs`.

## 7. Decision: no command returns pixels, and none returns a crop of anybody's face

Every rectangle on this surface is a `CropRectDto` - four normalised numbers - exactly as phases
09, 11, 13 and 20 do it. There is no image payload, no base64 field and no thumbnail anywhere in
these nine commands, so a support bundle assembled from them contains no pixels by construction
rather than by an exporter's care. Phase 13 wrote this rule about evidence and it holds here
unchanged, where the evidence is a region of somebody's mouth.

## 8. Consequences

- **A composite that reaches a client undisclosed is not reachable from this surface.** Four
  views, one view-backed source, one trigger.
- **A studio's configuration cannot loosen a guarantee**, because the wire has nowhere to put a
  number that would.
- **Phase 27 reads this surface rather than the tables.** It has to be able to say why a face looks
  worked on, and `image_micro` plus `micro_composites` is the whole answer.
- **The panel is a rendering of the contract**, so a change to `MicroOp` that the wire did not
  follow fails `cargo xtask contracts --check` at `ui/src/ipc/types.ts` rather than at runtime.
