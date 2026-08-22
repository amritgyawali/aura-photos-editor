# Model card - `face_recovery` `1.0.0`

| Field | Value |
|---|---|
| Name | `face_recovery` |
| Version | 1.0.0 |
| Task | Predict a high-frequency luminance correction for a slightly soft face |
| Class | `retouch` |
| Owner | MLL (ML Lead - Vision), with SRML and DATA |
| Licence | proprietary |
| Opset | 13 |
| Input | `crop [N,3,112,112]`, NCHW, `0..1`, **linear sRGB** |
| Output | `detail [N,1,112,112]`, unnormalised, to be **added** |
| Precision policy | int8 **forbidden**; fp16 and fp32 permitted |
| Stored version integer | 0 (`model_ver` on every `restore_plan` row, while the head is not consulted) |
| **Trained** | **No, and there is no measured fallback either. See "Training data".** |

## Purpose

Bring back detail in a face that is *slightly* soft, without changing whose face it is.

PHASE-22 section 6.3 is the shortest and strictest specification in this phase:

> Apply a face-prior restoration model only when the face is slightly soft (a narrow band of
> measured sharpness), never on heavily blurred faces where the model would hallucinate. Hard
> identity constraint: compute the Phase 06 face embedding before and after; if cosine distance
> exceeds a small threshold, reduce strength and retry, and if it still fails, skip and record the
> reason. This is the guarantee that the product never changes what someone looks like.

Its boundaries matter far more than its job, and there are four of them:

- **The band is checked before this head is consulted, not after.** `SOFT_FACE_LO` to
  `SOFT_FACE_HI`, and the floor is the one that matters. A heavily blurred face contains too
  little information to constrain a prior, so what a prior returns *is* the prior - a plausible
  face, which is to say somebody else's.
- **The output is a high-frequency residual, added to the face that is already there.** The low
  and mid bands never pass through this head, so it cannot move a feature, change a proportion or
  replace an expression. That is the first line of the identity guarantee, and it is a property of
  the output shape rather than of a threshold.
- **The strength is capped at 0.40 by the contract**, which section 5 of the phase document states
  twice.
- **Every recovery is measured on the rendered pixels and can only be refused.**
  `aura_restore::face_recovery::enforce` renders the plan through `aura_render::restore::apply`,
  crops the face, embeds it before and after through phase 06's recogniser, and compares. Above
  `MAX_IDENTITY_DRIFT` the strength drops and it renders again, at most three times; still above,
  and the face is **skipped**. There is deliberately no fourth outcome.

## Architecture

Two 3x3 convolutions at full resolution with a 1x1 head:

```
crop [N, 3, 112, 112]          the same 112 px two-point warp phases 06, 09 and 10 produce
  Conv 3x3 s1  ->  24          Relu
  Conv 3x3 s1  ->  24          Relu
  Conv 1x1     ->   1
detail [N, 1, 112, 112]
```

**112 px, the same crop the identity measurement uses.** Deliberately the same rather than larger:
the constraint embeds the crop before and after through phase 06's recogniser, and a recovery that
worked at a different scale from the measurement would be measured on a resample of itself.

**One channel, and it is luminance.** A chroma residual on a face is a colour change on a face -
a different operation with a much worse failure mode, and one the identity constraint has no way
to distinguish from the operation that was wanted.

**No pooling**, for the reason `denoise`'s architecture gives: the head has to write a value for
every pixel it read, and the documented opset subset (ADR-0007) has neither `Resize` nor
`ConvTranspose`. A real face prior needs a much larger receptive field than the five pixels this
gets, which is one of several reasons the trained version is a different architecture rather than
these weights filled in.

## Training data

**None. This head is an architecture fixture with deterministic pseudo-random weights.**

PHASE-22 section 9 asks DATA for a "soft-face labelled set" - faces at known degrees of softness,
with a sharp reference of the same person in the same light. There is no consented face data in
this repository at all; phase 06's condition C1 has been open since phase 06.

**`FACE_RECOVERY_HEAD_TRAINED` is false, and `aura_restore::face_recovery::solve` returns `None`
on every frame in this build.** No face in any wedding is recovered.

That is different from every other untrained head in this product, and the difference is
deliberate. Phases 15, 16 and 18 refuse to consult a placeholder and fall back on a reference
*solver*. Phase 20 could not do that and shipped a *measurement* instead. This phase ships a
**refusal**, because the measurement that would stand in for a face prior is unsharp masking on a
face - and that is not a weaker version of face recovery. It is a different operation, with a
worse result, wearing the same name. A photographer told "AURA improved this soft face" who
received a sharpened soft face has been lied to about the one thing this phase promised not to lie
about. ADR-0045 section 6 records the argument.

Every face still gets a row in `restore_face`, carrying
`RestoreCode::RecoveryHeadUntrained` - so a photographer can see that AURA looked and why it did
nothing, rather than seeing nothing at all. Phase 20's rule: what was left alone is shown as
prominently as what was done.

## Latency

Not measured, for the reason `denoise`'s is not: this build links no `wgpu` backend, so section
11's 1.2 s "sharpen + face recovery on 45 MP" row has no path to be measured on and is waived in
the phase 22 exit report.

The identity constraint's own cost is bounded rather than budgeted: at most `MAX_RESOLVES + 1`
renders and embeddings per face, and at most `MAX_RECOVERED_FACES` faces per frame. A frame with
more faces than that is a group shot, where no individual face is large enough to be inside the
band anyway.

## Quality gate

Section 10.1's second row: "Identity preservation: face embedding distance after face recovery
below threshold on 100 % of fixtures, or the operation is skipped."

That gate is **a query rather than a test result**, which is the point of storing the distance on
every row that reached a render:

```sql
SELECT MAX(identity_drift) FROM restore_face WHERE skipped = 0;
```

`v_restore_identity` exposes it per project, `RestoreService::identity_refusals` lists the frames
it fired on, and migration 22 refuses a row that would break it - a `restore_face` row with
`skipped = 0` and a drift above the ceiling does not insert, and a trigger aborts the UPDATE that
would un-skip one.

`tests/eval/restore_eval.rs` exercises the constraint end to end against a probe whose response to
the operator is measurable and monotone. **That is not phase 06's recogniser**, which is itself an
untrained placeholder: the test proves the constraint refuses what it should refuse, and says
nothing about whether a real embedding would notice a real identity change. Conditions C1 and C2 of
`docs/progress/PHASE-22-EXIT.md`.

## Ethical and fairness notes

This is the head in the whole product that is closest to the line section 11 of
`docs/plan/CLAUDE.md` draws: "Body reshaping, skin lightening, face or eye swapping, adding people
or objects that were not there, or any operation that changes a person's identity." A face-prior
model that ran on a face too blurred to constrain it would cross that line by accident rather than
by intent, and it would do it in a way nobody could see by looking at the photograph.

Four things stand between this head and that outcome, and none of them is a promise:

1. `SOFT_FACE_LO` is checked before the head is consulted.
2. The output cannot carry a low or mid band, so it cannot move a feature.
3. The strength is capped at 0.40 by the contract, which no config file may raise.
4. The identity distance is measured **on the rendered pixels** and the only outcomes are reduce
   and skip.

There is no per-skin-tone parity study, and none can be run here: the constraint is measured with
an untrained recogniser on synthetic faces. A recogniser with different accuracy across skin tones
would make this constraint *differently strict* for different people, which is a real risk and is
recorded as condition C2 rather than argued away. `docs/skin-fairness.md` says so in the product's
own words.

## Known failure modes

- **It does nothing, on every frame, in this build.** That is the intended behaviour of an
  untrained face prior and it is stated in the panel and in the plan rather than being silent.
- **A five-pixel receptive field cannot recover a face.** The trained version is a different
  architecture; these weights are a shape, not a starting point.
- **The band uses a per-face sharpness from phase 06**, whose detector is itself a placeholder. On
  a real photograph there are no faces to be in the band at all.
- **A face that cannot be embedded is skipped**, not recovered without a measurement. A guarantee
  that cannot be measured is a guarantee that cannot be kept.

## Fallback

**There is none, and that is the decision.** See "Training data" and ADR-0045 section 6. When no
identity probe is supplied, `aura_restore::decide::Analyser::plan` skips every face rather than
recovering any without a measurement.

## Rollback

`models.lock` pins the digest and `models/manifest.sig` signs the manifest. Because `MODEL_VER` is
`0` and nothing consults this head, a rollback of this entry changes no stored decision in this
build. When a trained version arrives, `MODEL_VER` bumps, `AURA-ML-5102` fires and every stored
plan is re-made - which is correct, because a plan made without a face prior is not comparable
with one made with it.

## Related

- `docs/adr/ADR-0045-restoration-denoise-sharpen-and-identity.md` - sections 5 and 6.
- `docs/restoration.md` - the identity guarantee, in the product's own words.
- `docs/runbooks/AURA-ML-5108.md` - what a photographer sees when a face is declined.
- `docs/retouch-ethics.md` - the wider promise this head sits inside.
