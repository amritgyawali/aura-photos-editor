# Model card - `permanent_features` `1.0.0`

| Field | Value |
|---|---|
| Name | `permanent_features` |
| Version | 1.0.0 |
| Task | Say what kind of permanent feature one mark on a face is |
| Class | `retouch` |
| Owner | MLL (ML Lead - Vision), with SRML, PM and DATA |
| Licence | proprietary |
| Opset | 13 |
| Input | `patch [N,3,64,64]`, NCHW, `0..1`, **linear sRGB** |
| Output | `kinds [N,6]`, unnormalised logits over `ProtectedKind` |
| Precision policy | int8 **forbidden**; fp16 and fp32 permitted |
| Stored version integer | 100 (`model_ver` on every `retouch_plan` row) |
| File size | 31,973 bytes per variant |
| **Trained** | **No. See "Training data".** |

## Purpose

Name what a mark is, so the product can protect it as what it is.

PHASE-20 section 2.1: "freckles, moles, birthmarks, scars, tattoos and dimples explicitly
detected and protected, with a user toggle per identity". The six classes are
`aura_core::contract::retouch::ProtectedKind`, in that order, and the head predicts all six -
because a kind nothing can name is a kind nothing can protect specifically, and one of the six is
the kind this product will never alter under any setting.

The classes are not equal:

| Class | What follows from it |
|---|---|
| `mole`, `freckle`, `birthmark`, `scar`, `dimple` | protected by default; a photographer may clear the protection |
| `tattoo` | **absolute**. `ProtectedKind::is_absolute` is true, `RetouchService::set_protection` refuses to clear it, and migration 21 carries a trigger that aborts the delete |

## Architecture

```
patch [N, 3, 64, 64]
  -> Conv 24x3   3x3 s2 -> Relu        # 64 -> 32
  -> MaxPool 2                          # 32 -> 16
  -> Conv 32x24  3x3    -> Relu
  -> GlobalAveragePool -> Flatten
  -> Gemm 6x32                          # six class logits
kinds [N, 6]
```

A patch rather than a face, because the question is about one mark. Global pooling, because the
answer is a property of the whole patch and its position inside it is already known - the patch
was cut around a candidate the detector located.

## Training data

**None. This model is a signed placeholder with deterministic weights.**

Section 8 step 2 asks for labelled permanent-feature data across skin tones, with consent, and
there is none in this repository. `aura_retouch::ops::PERMANENT_HEAD_TRAINED` is `false`, so
**this head is never consulted**.

What decides permanence in this build is section 6.1's own mechanism, and it is the stronger one:
**cross-frame evidence**. A mark at the same place on the same person's face across at least four
frames spanning at least forty-five minutes is permanent, measured in face-normalised coordinates
so that a person moving, turning or tilting their head does not move the mark. That is arithmetic
over a gallery rather than a network, it is unique to a gallery-aware product, and it is what
`aura_retouch::permanent::accumulate` implements.

Its limit on this build is inherited rather than its own: phase 06's face detector is a
placeholder, so there are no identities and no landmarks, so there is no correspondence to
accumulate. Condition C1 in `docs/progress/PHASE-20-EXIT.md`.

## Latency

Not measured; the rows are left empty rather than estimated.

| Machine | Batch 1 | Batch 32 |
|---|---|---|
| RTX 4070 laptop | | |
| M3 Pro MacBook | | |
| Intel iGPU desktop | | |

## Quality gate

Section 10.1's permanent-feature rows: false removal at or below two per cent, and zero for
tattoos. `tests/eval/retouch_eval.rs` gate 2 and gate 5 measure both against synthetic faces -
moles, a freckle field and a tattoo painted into the pixels - and gate 5 additionally asserts that
an absolute protection cannot be cleared through the service.

## Ethical and fairness notes

The class this head predicts decides whether somebody's face is altered, so the whole design is
tilted toward protecting:

- **`PERMANENT_FLOOR` is lower than `TEMPORARY_FLOOR`** (0.55 against 0.75). It is easier to
  become protected than to become removable, deliberately.
- **A tattoo cannot be unprotected by any setting**, and the refusal is enforced in three places:
  the type, the service and a database trigger. Section 11 of `docs/plan/CLAUDE.md` forbids
  operations that change a person's identity, permanently.
- **The head is unwilling to call anything a tattoo from one frame.** In the measured fallback,
  only size does it, and only well past the size at which this phase would have touched a mark
  anyway - because a wrong `tattoo` label is harmless and a missed one is not.

**No per-skin-tone metrics are published.** Freckle density, mole contrast against skin and scar
appearance all vary with skin tone, and a claim about parity here would need the corpus section 9
asks for. Condition C2 in the exit report.

## Known failure modes

- A cluster of freckles is reported as one feature rather than several when they sit within
  `SAME_FEATURE_RADIUS` of each other. The consequence is that they are protected together, which
  is the safe direction.
- Deliberate face paint - mehndi, sindoor, tilak, festival colour - is not in the vocabulary. The
  scene rows in `retouch_presets.toml` switch tone evening off for `ritual` frames for exactly
  this reason, which protects it by not operating rather than by classifying it.
- A birthmark and a large bruise look alike from one frame. Cross-frame evidence separates them:
  a bruise fades over a day and a birthmark does not.

## Fallback

`aura_retouch::permanent::classify`, which reads the detector's own colour and size measurements,
plus the cross-frame accumulation described above. Uncertainty leaves the mark alone.

## Rollback

As `blemish_detector`: `models.lock` pins the sha256, the manifest is signed, and a rollback bumps
`model_ver`, raises `AURA-ML-5096` and re-plans in the background. **A rollback does not clear the
protect set** - `retouch_protected` rows written by a photographer survive every re-analysis.

## Related

- `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md`
- `docs/model-cards/blemish_detector.md`
- `docs/retouch.md`
