# What AURA will and will not do to people's appearance

This is a product policy, not a description of the current build. It is written first, before the
code it constrains, because the code exists to keep it - section 8 of the phase 21 plan puts
"publish this document and get sign-off" ahead of every implementation task, and that ordering is
the point.

Three audiences read this document. A **photographer** deciding whether to let software touch a
client's face. A **client** who wants to know what happened to their photographs. An **engineer**
about to add a feature, who needs to know which requests are out of scope permanently rather than
merely unbuilt.

---

## 1. The one sentence

**AURA does the small things a retoucher does without being asked, and nothing that changes who
somebody is.**

Every rule below follows from that, and where a rule is not obvious, the reason it exists is
written beside it.

---

## 2. What AURA will never do, in any version, for any customer

These are permanent. They are not defaults, not preferences, and not settings hidden behind an
advanced panel. Section 11 of `docs/plan/CLAUDE.md` has forbidden them since phase 01, and every
phase since has inherited the list:

| Never | Why |
|---|---|
| **Body reshaping** - slimming, waist narrowing, limb lengthening, jaw or nose reshaping | A photograph of a reshaped person is a photograph of somebody who was not there. |
| **Skin lightening or darkening toward a target** | There is no correct skin colour. A product with an ideal-skin constant is a product that lightens dark skin while believing it is correcting a cast. |
| **Face swapping or eye swapping between frames** | A composite portrait delivered as a photograph is a lie about a moment. |
| **Adding people, objects or expressions that were not there** | Same reason. |
| **Removing permanent features** - moles, freckles, birthmarks, scars, dimples | Those are identity, not defects. Phase 20 protects them; this phase inherits the protect set unchanged. |
| **Removing or altering tattoos** | The only *absolute* protection in the product. It cannot be switched off by a setting, a preset, an override or an API call. |
| **Whitening teeth or eyes cosmetically** | Bounded corrections toward a measured natural range are in scope. Cosmetic whitening is not, and section 4 is what keeps the two apart. |
| **Enlarging, reshaping or recolouring eyes** | Eye colour is identity. Eye size is anatomy. |
| **Inferring anything about a person** - gender, ethnicity, religion, age, attractiveness | Phase 06's rule, and the shapes have nowhere to put such a value. |

**These are enforced in code, not remembered.** There is no field in
`aura_core::contract::micro`, no column in migration 22 and no parameter on the IPC surface that
could express any of them, and `crates/aura-retouch/tests/boundaries.rs` fails the build if the
words appear in the crate. Adding one is a visible contract change requiring a CTO-role ADR - not
a commit.

---

## 3. What AURA does do

Six families of small fix, each bounded, each individually switchable by the studio:

| Family | What it does | What it explicitly does not do |
|---|---|---|
| **Hair** | Calms stray flyaway strands *against a clean background* by reducing their contrast against what is behind them | Never erases a strand, never edits inside the hair mass, never changes the shape of a hairline |
| **Teeth** | Evens the luminance across visible teeth and reduces a yellow cast *toward a measured natural range* | Never whitens past that range, never changes tooth shape, never touches gums or lips |
| **Eyes** | Reduces redness in the whites (colour only), adds a little local contrast in the iris | Never enlarges, never recolours, never brightens the whites past a cap, never removes a catchlight |
| **Clothing** | Removes lint, threads and small stains from fabric | Creases, wrinkles and visible straps are **off by default** and are a per-project choice |
| **Glare** | Reduces specular sheets on glasses, or reconstructs the destroyed area from a sibling frame | Never borrows an eye, never borrows an expression, never borrows anything that carried information |
| **Shine** | Evens shine and shadow on a nose, an ear or a neck | Never changes a shape |

Each is a **reduction**, not an erasure. That distinction is the difference between a photograph
that looks finished and one that looks worked on, and it is why every ceiling in this phase is low
enough that a photographer has to look for the change to see it.

---

## 4. Ceilings are code, not configuration

Every operation has a hard maximum. Those maxima live in
`crates/aura-core/src/contract/micro.rs` as constants, and
`crates/aura-retouch/config/micro_retouch.toml` may only set values **at or below** them.

A studio may make AURA gentler. **Nothing can make it stronger** - not a config file, not an
override, not an IPC call, not a preset. The matrix loader refuses a file that tries, with
`AURA-ML-5105`, and refuses it whole rather than partially.

This asymmetry is the whole design. A ceiling a text file can raise is a description of the
defaults; a ceiling only a signed release can raise is a promise. CI attempts to exceed each one
and asserts the refusal.

---

## 5. Cross-frame borrowing, and the rule that bounds it

Phase 21 can reconstruct a small destroyed region from a **sibling frame in the same moment** -
the same instant, the same face, a different exposure of it. This is the most powerful thing in
the phase, and it is the one that could most easily become a composite nobody disclosed.

The rule that bounds it:

> **You may only borrow pixels that carry no information.**

In practice, a borrow is permitted only when all of these hold, and the code checks each one:

1. The region is **small** - bounded as a fraction of the frame.
2. The region is **genuinely destroyed** - a majority of it is blown specular in the target frame.
   A glasses reflection that has clipped the sensor carries no eye; a soft sheen carries one, and
   the soft sheen case is reduced rather than borrowed.
3. The sibling **aligns** - a measured alignment score above a floor. A borrow that does not align
   is refused rather than blended.
4. The sibling is in the **same moment**, so it is the same instant of the same event.
5. The borrow happens **only for glare**. There is no code path from any other operation to a
   borrow, and no way to request one.

And whenever it happens, it is **disclosed**:

- the operation records the source photograph's identifier in the recipe;
- the plan carries a reason code saying it happened;
- the row in `micro_op` names the source;
- the Micro-Retouch panel shows it as a borrowed region;
- the delivery report lists every frame with a borrowed region in it.

A composite that is recorded, shown and reported in five places is not hidden. A composite
recorded in none of them is what this rule exists to make impossible.

**What is deliberately not built:** replacing a closed eye with an open one from another frame.
It is technically the same machinery and it is excluded on purpose. A closed eye carries
information - it says the person blinked - so borrowing over it is a change to what happened
rather than a repair of what was destroyed. Phase 21 section 2.2 excludes it, and adding it needs
a new ADR arguing against this paragraph rather than a flag.

---

## 6. The studio decides, and the client is told

**Opt-in matrix.** Every family above can be switched off per project. Two of them - visible
straps and creases - are switched off by default and have to be turned on deliberately, because
removing a crease is the one thing on the list a client might reasonably not want done.

**Delivery report.** Every project can produce a list of which operations ran, how many times, and
on which frames pixels were borrowed. A studio that needs to answer "what did you do to these
photographs" has a document rather than a memory.

**Nothing here is irreversible.** Invariant 1: originals are opened read-only and every decision
is a row and a recipe entry. Switching a family off and re-exporting returns the original
photograph exactly.

---

## 7. What the current build actually does

The policy above is what the product is for. What this repository can prove today is narrower, and
saying so is part of keeping the policy honest:

- **The three detection heads shipped with this phase are untrained placeholders and none of them
  is consulted.** What runs instead is measurement - contrast structure for flyaways, specular
  saturation for glare, small-anomaly detection for lint - whose failure mode is finding fewer
  things rather than confidently wrong ones.
- **Phase 06's face detector is a placeholder**, so on a real photograph there are no faces, no
  eye landmarks and therefore nothing for the teeth, eye or glare families to act on.
- **Phase 18's mask generator is not wired into this pass**, so on this build every operation is
  gated and nothing is edited.
- **The naturalness audit of 400 frames judged by retouchers has not happened.** The headline KPI
  of this phase is unmeasured.
- **No per-skin-tone and no per-hair-type study has been run.** The mechanisms are relative to the
  frame's own measurements by construction - see `docs/skin-fairness.md` - and no per-bucket
  number is published or should be inferred.

Everything in sections 2 to 6 is enforced today. Everything in section 3 is *measured* today only
against synthetic frames whose answers were painted into the pixels.
`docs/progress/PHASE-21-EXIT.md` carries all of it as conditions.

---

## 8. Changing this document

A change to sections 2, 4 or 5 requires an ADR and sign-off from the PM and CTO roles, in that
order, before any code moves. A change that widens what AURA may do to a person is not a
refactor.

The corresponding code lives in:

- `crates/aura-core/src/contract/micro.rs` - the ceilings and the shapes
- `crates/aura-retouch/src/micro/guard.rs` - the enforcement
- `crates/aura-retouch/config/micro_retouch.toml` - what a studio may choose
- `crates/aura-retouch/tests/boundaries.rs` - the grep that fails the build
- `crates/aura-cli/src/phase21.rs` - the gate that attempts each ceiling and asserts refusal
