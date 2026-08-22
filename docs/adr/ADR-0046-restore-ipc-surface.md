# ADR-0046 - The restoration IPC surface

**Status:** accepted · **Date:** 2026-08-21 · **Phase:** 22 · **Supersedes:** nothing

The second of phase 22's two ADRs. [ADR-0045](ADR-0045-restoration-denoise-sharpen-and-identity.md)
covers the decisions; this covers the wire.

`crates/aura-app/src/contract/ipc.rs` is a frozen contract, so every shape here is in
`contracts.lock` and changing one later needs an amendment to this document and a re-lock, in that
order.

## 1. Context

Phase 22 produces a decision with three parts, two measured post-conditions and a per-face record
of something that did *not* happen. Section 4 asks for one panel: "Strength tiers, per-image
override, preview."

Three things make this surface different from phase 21's.

**The photographer chooses a tier, and a tier is a number in a way a switch is not.** Phase 21's
matrix was switches all the way down, and ADR-0044 section 4 could say "there is no strength field
anywhere". Here there is a genuine four-way choice, because "how much noise reduction" is a
question a photographer legitimately has an opinion about and "how much may AURA whiten teeth" is
not.

**The most important thing this surface carries is a refusal.** `restore_face` rows that were
skipped for identity drift are the phase's own guarantee working, and a panel that only listed
what happened would make a careful product look like a careless one.

**The panel shows a 100 % zoom preview.** Section 9 gives SFE three days for "Restore panel with
tiers, per-image override, 100 % zoom preview", and a preview is the one thing on this surface
that could carry pixels. It does not - see section 5.

## 2. Decision: seven commands, and what each of them may touch

| Command | Reads | Writes |
|---|---|---|
| `restore_status` | `v_restore_coverage`, `v_restore_identity` | nothing |
| `image_restore` | one `restore_plan` row and its faces | nothing |
| `restore_identity_refusals` | `v_restore_identity` and `restore_face` | nothing |
| `restore_review_queue` | `idx_restore_review` | nothing |
| `accept_restore` | one row | `reviewed` |
| `set_restore_override` | one row | the tier, two switches, `user_edited` |
| `restore_pass` | the pending set | every plan it makes |

Seven rather than phase 21's nine, and the two that are absent are absent for a reason.

**There is no `restore_matrix`.** Phase 21 had a per-project opt-in matrix because a studio has a
standing policy about whether it removes bra straps. There is no equivalent standing policy about
denoising: how much noise reduction a frame wants is a property of the frame, and the per-scene
ceilings that *are* a product decision live in `restore_profiles.toml` where a studio can lower
them once rather than per project.

**There is no `restore_preview`.** See section 5.

## 3. Decision: the tier is on the wire and no other number is

`SetRestoreOverrideInput` carries `denoise: Option<String>` - one of `off`, `light`, `standard`,
`strong` - and two booleans. It does not carry `luminance`, `colour`, `detail`, `amount`,
`kernel_sigma`, `skin_attenuation` or `strength`.

The line is between **which of four** and **how far each goes**. A photographer choosing
`standard` on a frame AURA put at `light` is making a judgement about their own photograph. A
photographer setting the luminance amount to 0.9 is overriding a decision that is conditioned on
the camera's noise model, and the number they set would mean something different on the next body
they shot with. `DenoiseSpec` is what the tier becomes under one sensor at one ISO, and it is
derived rather than chosen.

The two booleans are separate rather than one "restore" switch, because a photographer can want a
frame sharpened and want no model near anybody's face. Collapsing them would force them to choose.

**A tier a photographer chose is still clamped.** `set_restore_override` writes the tier and sets
`user_edited`; the *scene* ceiling and the *camera* ceiling still apply when the plan is next
made, and `restore_plan`'s CHECK refuses a row where an unmeasured model produced `strong`. A
studio may lower a ceiling in the config file and a photographer may not raise one from the panel -
phase 21's rule, and the reason it survives into a phase that does have a numeric choice is that
the choice is *inside* the bounds rather than over them.

## 4. Decision: the refusals are first-class, in three places

`RestoreFaceDto::skipped_because` carries the code, `RestorePlanDto::faces_skipped_identity`
carries the count, and `restore_identity_refusals` is a command of its own on the frozen surface.

Three places for one fact looks redundant and is not, because three different questions are being
asked. The panel asks "what happened to this face"; the frame badge asks "did anything get
declined here"; the delivery report and phase 27 ask "where in this wedding did this happen". A
number the third caller had to derive by opening four hundred plans is a number it would not ask
for.

`RestoreFaceDto::identity_drift` is on the wire **whether the face was kept or skipped**, which is
what lets the panel show a measured distance beside the sentence rather than a bare refusal. Phase
21 put its three naturalness measurements on the wire for the same reason, with the sample count
beside them so a ratio over eleven samples is not printed to three decimal places.

## 5. Decision: no command returns pixels, and the 100 % preview is the panel's own

Section 9's SFE row asks for a "100 % zoom preview", which is the one thing on this surface that
could plausibly justify returning image data. It does not.

The panel renders the preview from the pixels it already has - the same `render` command phases 14
to 21 all use, at the region and zoom the photographer is looking at. What this surface adds is
the *parameters* that render was made with, so the panel can offer a before-and-after by asking
for the same region twice with `restoration` set and unset.

The reason is phase 13's and it has not changed: evidence can never be a pixel. A surface that
returned crops would be a surface a support bundle could accidentally include, and
`docs/plan/CLAUDE.md` section 9's rule about what may leave the device would then depend on
what a serialiser happened to skip.

## 6. Decision: the pass takes an occasion, and there is no interactive one

`RestorePassInput::when` is `export` or `background`. There is no third value, `RestoreWhen` has no
third variant, and `graph::plan` refuses `Stage::Restoration` on the interactive path
independently.

Three layers for one rule looks excessive until you notice they fail differently: the type stops a
caller *asking*, the graph stops a render *doing it anyway*, and the wire stops a future IPC
client inventing a value. Section 6.4 is one sentence - "Restoration never runs on the interactive
path" - and it is the sentence that keeps the editor responsive on a 4,000-image wedding.

The pass also takes `enabled`, hard rule 8's kill switch, and a disabled pass **still writes a
plan per frame** - one that does nothing. A frame with no plan and a frame the studio switched off
look identical in a coverage report otherwise.

## 7. Decision: the status carries the unmeasured cameras by name

`RestoreStatusDto::unmeasured_cameras` is a list of strings, and it is the field on this surface
most likely to be deleted by somebody tidying up.

It is there because the condition it reports is the one a photographer can actually act on. Every
noise model in this build is synthetic, so every wedding will show every body it was shot on - and
when a photographed reference for one body arrives, that body drops off the list and its frames
become eligible for `DenoiseTier::Strong`. A studio that sees its main camera on that list knows
why its dance-floor frames are capped, which is a much better experience than a tier that is
quietly one step lower than it could be.

`RestoreStatusDto::sharpen_refusals` is a histogram for the same reason: "AURA sharpened nothing
in this wedding" has six causes and five of them are somebody else's bug.

## 8. Consequences

- The develop panel gains a fourth tab beside Basic, Tone and HSL, and a sixth retouch-family
  panel beside Mask, Local, Retouch and Micro-Retouch.
- Phase 23's geometry surface must not add a second sharpening control. `Stage::Sharpen`'s
  position is the enforcement and `docs/restoration.md` says so to a photographer.
- Phase 27's QC agent reads `restore_identity_refusals` and `image_restore` and needs no new
  command.
- Phase 30's learning loop reads `user_edited` plans to find out where a photographer disagreed
  with a tier. The row keeps AURA's own numbers, so the disagreement is readable - phase 15's rule.
- `ui/src/ipc/client.ts` still stops at phase 19, so these commands are reachable from the Tauri
  shell and not yet from a typed client method. Phases 20 and 21 are in the same state and the
  phase 22 exit report carries it as condition C5.

## 9. What was considered and rejected

**A per-project restoration matrix, like phase 21's.** Rejected; see section 2.

**A single `restore` boolean instead of two.** Rejected; see section 3.

**Returning the before-and-after crop from `image_restore`.** Rejected; see section 5.

**Putting `DenoiseSpec` on the override so a photographer could set the amounts.** Rejected. The
amounts are what a tier *becomes* under one sensor at one ISO; a photographer who set them would
be setting a number that means something different on their other body, and the panel would have
to explain a photon transfer curve to justify the units.

**Reporting one "restoration quality" number instead of `texture_retention` and `ringing`.**
Rejected for the reason ADR-0045 section 2.1 gives: the two are fixed by two different levers, and
a photographer whose complaint is that an edge looks crunchy needs to see the ringing figure
rather than a score that averaged it with something else.
