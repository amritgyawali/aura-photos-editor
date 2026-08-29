# ADR-0047 - Geometry: a crop that must earn its place, a rotation that pays for itself in pixels, and a safety filter that runs before the score

**Status:** accepted · **Date:** 2026-08-28 · **Phase:** 23 · **Supersedes:** nothing

Phase 23 section 4 asks for no ADR by name. It needs two anyway, and this is the first. Section 5
freezes a `GeometryPlan` whose six supporting types it does not define; section 6.2 states a
rotation rule whose cost - "rotation implies cropping" - has to be paid before the crop search
runs rather than after it; section 6.3 names four hard constraints of which two cannot be filled
on this build; and section 4's file list names no table while the phase's headline promise is one
nobody can audit without one. The second document is
[ADR-0048](ADR-0048-geometry-ipc-surface.md), which covers the wire.

The ADR numbering in this repository is sequential across the whole project rather than aligned to
phase numbers.

## 1. Context

Twenty-two phases have decided what a photograph is, how it should look, and what should be
repaired in it. This is the first that decides **which pixels exist**.

That is a different kind of decision and section 1 of the phase document says why:

> Smart crop is where automation is most dangerous, so a subject-aware, conservative,
> always-reversible crop is a trust feature as much as a quality feature.

Three properties separate this phase from every previous one.

**A wrong answer removes information that the delivered file no longer contains.** Phase 22 could
denoise too hard and a photographer could see it; the original was still there. A crop is the same
in principle - nothing on disk moves, and the recipe is reversible - and completely different in
practice, because the failure is invisible in the only artefact anybody looks at. A gallery of
four hundred JPEGs where six have a hand cut off at the wrist does not look like a bug. It looks
like the photographer.

**The two operations pay for each other.** Levelling a horizon costs a crop, because the largest
upright rectangle inside a rotated frame is smaller than the frame. So does a keystone. So the
question "how far may this be rotated" cannot be answered without knowing what the crop it costs
would cut, and the crop search cannot start until both costs are known.

**Most of the work is deciding not to act.** Section 10.1 requires that at least 70 % of frames
keep their original framing. That is not a conservative default that could be tuned upward later;
it is the phase's own definition of correct behaviour, and a build that cropped 60 % of a wedding
would be failing rather than being aggressive.

## 2. Decision: geometry occupies one stage, and it is the last one

Phase 14 froze a 23-stage render graph with `Stage::Geometry` at index 21, after
`Stage::Sharpen`. Phase 22 already split itself across two stages when one was not enough
(ADR-0045 section 2). This phase does not, and the reason is worth recording because the
temptation runs the other way.

Lens correction is an optical operation and belongs in the sensor domain, beside denoise at index
6. Distortion, vignette and lateral chromatic aberration are all properties of the lens rather
than of the photograph, and correcting them after tone has been shaped means correcting them in a
space where the vignette is no longer multiplicative.

**It is still one stage, at index 21**, and the argument that wins is resampling. A distortion
correction resamples. A rotation resamples. A keystone resamples. A crop does not, but it decides
what the other three have to cover. Splitting the lens half to index 6 would mean **two
resampling passes over the frame** - and section 12's fourth failure mode is "resampling softens
images", whose mitigation is "geometry applied once in the render graph rather than repeatedly".

What makes the ordering safe rather than merely cheap is that vignette correction is applied in
**linear light**, as section 6.1 requires, and everything before the output transform is linear
(phase 14's invariant 8). A vignette correction at index 21 in linear light and one at index 6 in
linear light differ by the tone curve's effect on a multiplicative gain - which is why the
correction is a gain rather than an offset, and why `MAX_VIGNETTE` bounds it.

**Consequence for phase 22 and for anything after it.** Geometry resamples after sharpening,
which is correct, and it means nothing later may sharpen again: a resample after a sharpen
softens the sharpened edges slightly, and a second sharpen to compensate is the halo generator
phase 22 spent four preconditions avoiding. Phase 22's exit report states the rule and this phase
keeps it - there is no sharpening control anywhere in `aura-geometry`, and
`crates/aura-geometry/tests/boundaries.rs` fails the build if one appears.

## 3. Decision: the safety filter runs before the score, not as a penalty in it

Section 6.3 lists four hard constraints and then a scoring objective. The obvious implementation
folds the constraints into the objective as large negative terms, and it is wrong.

A penalty is a trade. A rectangle that cuts a hand and is otherwise excellent can outscore a
rectangle that cuts nothing and is merely good, for any finite penalty, on some frame in some
wedding - and the frames where that happens are exactly the frames where the composition
objective is most confident, which is to say the couple portraits. Section 12's first failure
mode is "auto-crop cuts something important" and its mitigation is "hard safety constraints with
a **zero-tolerance** CI gate".

So `safety::check` runs first and returns a boolean, `crop::search` never scores a rectangle that
failed it, and the objective has no term for protected content at all. There is no weight anybody
could tune to trade a face against a better composition, because there is no weight.

Three consequences follow, and all three are deliberate:

* **A refused variant is stored rather than dropped.** `geometry_crop` keeps it with `safe = 0`
  and the code that refused it. Phase 17 established that a rejection is written when the failure
  *is* the evidence; here "why is there no square crop of this photograph" is a question the panel
  has to answer and cannot answer from an absence.
* **The delivered rectangle is checked twice.** Once by the filter and once by the database:
  `geometry_primary_is_safe_insert` and `geometry_primary_is_safe_update` abort any statement that
  would leave `primary_crop` addressing an unsafe row. The contract says the same thing and the
  type system cannot, because an index is an integer and the row it addresses is in another table.
* **The safety report stores its denominator.** `considered` beside `at_risk`. Section 10.1's
  gate - zero auto-crops cut a detected face - over a wedding whose detector found no faces is
  arithmetic, and phase 21's rule says a number that could be either must carry the count that
  says which.

## 4. Decision: the rotation band is a band, and its cost is paid before the crop search

Section 6.2 gives three numbers: rotate only above 0.70 horizon confidence, only between 0.2 and
8 degrees, and cap the keystone stretch. All three are in the contract as constants and none is in
a config file that could raise them.

The band's two ends mean different things and it is worth being explicit, because a future reader
will otherwise read them as one tolerance:

* **Below `ROTATE_MIN_DEG` (0.2°) the frame is already level.** Rotating it would resample the
  whole photograph to move a horizon by a quarter of a pixel at the frame edge, which is a cost
  with no benefit. The reason code is `geometry_tilt_negligible` and it is a refusal.
* **Above `ROTATE_MAX_DEG` (8°) the tilt is a decision somebody made.** A twenty-degree Dutch
  angle is not a mistake, and the failure mode of treating it as one is section 12's second:
  "straightening ruins intentional tilts". The frame is **left alone rather than clamped to
  eight**, which is the distinction the schema's CHECK is written around: the bound is on the
  applied angle, and there is no path that applies eight degrees to a frame that wanted twenty.

The cost is paid up front. `straighten::solve` computes `rotation_crop` for the angle it wants,
checks the resulting rectangle against the protected regions and the resolution floor, and
**reduces the angle until it passes or abandons it**. The keystone does the same. The two bites
are then intersected, and the crop search runs inside what is left of both.

`rotation_crop` itself lives in `aura_core::contract::geometry` rather than in the solver, and it
is the only function in that file that computes anything. Two crates need it - the solver to know
what an angle costs before committing to it, and the renderer to know what rectangle to deliver -
and two implementations of "the largest upright rectangle inside a rotated frame" is two answers
to which pixels exist.

## 5. Decision: an unidentified subject stops the crop search rather than defaulting it

This is the decision most likely to be re-argued, because the code that implements it looks like a
missing feature.

The crop objective is the geometric mean of four terms: placement, balance, edge cleanliness and
headroom. Three of the four are properties of the *frame* and can be measured over any
photograph. The fourth - placement - is a measurement of where **the subject** sits, and the
subject comes from phase 06's faces or phase 11's crop hint.

When neither is present, `subject_of` falls back on the frame's own energy centroid. That is a
real measurement and it is a measurement of the wrong thing: it says where the detail is, not what
the photograph is about, and on a reception frame the detail is often a chandelier.

Two options, and the first is what most products do:

1. **Search anyway.** Three of four terms still mean something, so the answer is not nonsense - it
   is a rectangle optimised toward a bright object, compared against `MIN_IMPROVEMENT` as though
   the comparison meant something, and delivered as a considered decision.
2. **Do not search.** Record `geometry_crop_kept_original`, deliver the frame as shot.

The second ships. Phase 19's rule - a phase that consumes another phase's output owns no fallback
for it - and phase 22's - a repair that cannot be measured is not performed - are the same rule
seen from two sides, and this is the third time it has decided a design.

**The aspect variants are still generated**, and that is not an inconsistency. A variant is an
option phase 29 may take rather than a decision about the delivery, and a centred 4:5 over an
unidentified subject is a reasonable option and a bad delivery.

On this build the consequence is large and is stated in the exit report: phase 06's detector is a
placeholder, so `projected` is empty on every real photograph, so no frame is auto-cropped unless
phase 11 supplied a hint. The fixture wedding measures 0.83 conservatism because its frames carry
painted regions; a real wedding on this build would measure 1.00.

## 6. Decision: the improvement margin is per-scene, and the scene may only raise it

`MIN_IMPROVEMENT` is 0.06 in the contract. `crop_rules.toml` carries a per-scene margin and the
loader refuses a file whose margin is *below* the contract's - the same direction phase 22's
profile loader enforces, and for the same reason: a studio may tighten a rule and may not loosen
one.

Ten of the 23 scene rows switch automatic cropping off entirely. That is not a hedge; it is where
the phase's danger is concentrated. The scenes with joined hands, rings, garlands and children's
hands at the edge of frame are exactly the scenes where `ProtectedContent::Hands` would be doing
the protecting - and phase 11's keypoint head is a placeholder, so hands are never in the
protected set on this build. Switching cropping off in those scenes is the mitigation for a gap
that is otherwise silent, and it is written in the config file with the reason on the row.

## 7. Decision: no lens profile in this repository is measured, and the schema says so on every row

`assets/lens_profiles/profiles.toml` has fourteen rows. Every one is a reference model for a lens
class or family - a plausible distortion polynomial for a 35 mm prime rather than a measurement of
one - and `assets/lens_profiles/ATTRIBUTION.md` says so.

`geometry_plan.lens_measured` is `0` on every row this build writes. It is the same column phase
22 added for its noise models and it exists for the same reason: **the day a measured profile
arrives, the frames corrected through a reference one have to be findable.** Without the column
they are indistinguishable from correctly corrected frames, and the only remedy is to re-plan
every wedding in the catalog.

The correction is applied anyway, rather than refused, and this differs from phase 22's face
recovery - which refuses entirely when its head is untrained. The difference is what the fallback
*is*. Phase 22's fallback for a face prior was unsharp masking on a face, which is a different
operation with the same name. Here the fallback is a distortion polynomial that is approximately
right for the lens class, whose failure mode is a residual barrel of a fraction of a percent -
visible to a measurement and not to a photographer. ADR-0045 section 6's test applies: a
measurement whose failure mode is doing too little ships; a guess whose failure mode is confident
invention does not.

## 8. Decision: two version columns rather than three

Every decision phase since 06 has carried three. This one carries two - `analysis_ver` and
`profile_ver` - because **this phase ships no model**, the third since phase 08 and for the same
reason phase 17 shipped none: there is nothing to train. The horizon comes from phase 11, the
faces from phase 06, and everything this phase does with them is arithmetic with a documented
form.

The two invalidate different things and that is why they are two:

* `analysis_ver` moves when the band, the solver, the objective, the margin or the filter changes.
  Every field on the plan is stale.
* `profile_ver` moves when `profiles.toml` or `crop_rules.toml` changes. The lens correction and
  the crop are stale; the rotation is not.

`AURA-ML-5109` carries both numbers, and phase 15's merge lesson applies: a version column counts
*measurements* rather than commits, so two branches that each invalidate the same column produce a
third number rather than either of theirs.

## 9. Decision: migration 23 uses a deferred foreign key, the only one in the product

`geometry_crop.photo_id` references `geometry_plan.photo_id` and is declared
`DEFERRABLE INITIALLY DEFERRED`.

The write order is variants first, plan second, because `geometry_primary_is_safe_insert` reads
`geometry_crop` to decide whether the incoming plan's `primary_crop` addresses a safe row. A plan
written before its own variants would be checked against **the previous version of that
photograph's rectangles**, which is a check that passes for the wrong reason.

An immediate foreign key refuses the first variant of every photograph, because at that moment no
plan row exists. That is exactly what happened: the first run of the phase gate wrote zero plans
and reported `FOREIGN KEY constraint failed` twenty-four times, and the store's own comment
already described the ordering the constraint forbade.

Three options were considered. Dropping the constraint loses the cascade and permits orphan
variants. Writing the plan first with `primary_crop = 0` and updating it afterwards needs a third
trigger and makes the delivered index momentarily wrong. Deferring the check moves it to COMMIT,
by which point both rows exist, and a transaction that wrote variants and then failed to write
their plan still aborts. The third ships.

**It is worth flagging for later phases**: SQLite enforces a deferred constraint only inside an
explicit transaction, and `aura_catalog::Writer::transact` is one. A caller that wrote a variant
outside a transaction would find the constraint checked immediately.

## 10. What this phase deliberately does not build

**No scale, no fill, no panorama, no upscale.** Section 2.2 puts generative fill in phase 24 and
panoramas out of scope for V1. There is no column in migration 23 that could carry an upscale, a
synthesised corner or a second photograph's id; there is no field on the IPC surface; and
`crates/aura-geometry/tests/boundaries.rs` - the sixth grep-as-a-test in the repository - fails
the build if the words appear in the crate. A schema with no column for either makes adding one a
visible contract change.

**No cloud call.** Section 7: "The phase must work with the network cable unplugged." Nothing in
`aura-geometry` can reach the gateway and the boundaries test checks it.

**No second horizon detector.** Phase 11 owns the horizon and this phase reads it. Without it
`Horizon::default` has `present = false`, every frame records `geometry_horizon_absent`, and
nothing is rotated - a refusal rather than a guess, because a rotation derived from something
other than a measured horizon is a tilt somebody has to undo.

## 11. Consequences

**Good.** The safety promise is a database property rather than a code path. The conservatism
requirement is a stored number a support case can query. A refusal carries the angle that was
wanted beside the angle that survived, so a photographer can see the reasoning rather than only
the result. Geometry resamples once.

**Bad.** Two of the five `ProtectedContent` kinds are never filled on this build, so the safety
filter protects less than the contract describes. The crop objective is authored rather than
fitted against expert crops, because section 9's DATA row asks for 2,000 labelled frames and this
repository has none. Every lens profile is a reference model. All three are conditions in
`docs/progress/PHASE-23-EXIT.md`.

**Neutral.** The deferred foreign key is a small novelty in a schema style that has been uniform
for twenty-two migrations. It is documented in the migration itself as well as here, because the
next person to write a two-table decision schema will copy this one.
