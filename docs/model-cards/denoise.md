# Model card - `denoise` `1.0.0`

| Field | Value |
|---|---|
| Name | `denoise` |
| Version | 1.0.0 |
| Task | Predict what should be subtracted from a noisy tile, given the sensor's own noise model |
| Class | `retouch` |
| Owner | MLL (ML Lead - Vision), with COL, SRML and DATA |
| Licence | proprietary |
| Opset | 13 |
| Input | `tile [N,4,128,128]`, NCHW, `0..1`, **linear sRGB + noise plane** |
| Output | `residual [N,3,128,128]`, unnormalised, to be **subtracted** |
| Precision policy | int8 **forbidden**; fp16 and fp32 permitted |
| Stored version integer | 0 (`model_ver` on every `restore_plan` row, while the head is not consulted) |
| **Trained** | **No. See "Training data".** |

## Purpose

Remove the noise a wedding reception actually produces, and nothing else.

PHASE-22 section 6.1: "Condition the denoiser on the measured per-camera noise model (read noise,
shot noise slope per ISO) so it removes the right amount rather than a learned average." That
sentence is the whole design of this head, and the fourth input plane is where it lives.

Its boundaries matter more than its job:

- **It never decides how much to remove.** `aura_restore::denoise::choose` picks one of four tiers
  from phase 09's measured `noise_sigma_rel` against the scene's own ceiling, and
  `aura_restore::selfcheck::enforce` steps that tier *down* if the rendered result lost more than
  `MIN_TEXTURE_RETENTION` of its high-band energy. Nothing this head emits can raise a strength.
- **It emits a residual rather than a clean image.** A network that outputs the photograph has to
  reproduce the photograph, so its errors are errors *in* the photograph. A network that outputs
  what should be subtracted starts from the identity, and its errors are errors in the correction.
  The failure mode of the second is leaving noise behind; the failure mode of the first is
  inventing texture, which is what this phase exists not to do.
- **It never sees a whole photograph.** The input is a 128 px tile with the halo phase 14's tiler
  decodes around it.

## Architecture

Three 3x3 convolutions at full resolution with a 1x1 head:

```
tile [N, 4, 128, 128]          three colour planes and one predicted-sigma plane
  Conv 3x3 s1  ->  32          Relu
  Conv 3x3 s1  ->  32          Relu
  Conv 3x3 s1  ->  32          Relu
  Conv 1x1     ->   3
residual [N, 3, 128, 128]
```

**No pooling anywhere.** Every other detection head in this product pools its way down to a
decision grid, because it is answering a question about a region. This one has to write a value
for every pixel it read, and the documented opset subset (ADR-0007) has neither `Resize` nor
`ConvTranspose`, so a pooled trunk could not get back. The cost is that the receptive field is
seven pixels, which is enough for the shot noise this head is mostly about and is not enough for
the low-frequency chroma blotching a real denoiser also has to remove - a trained version needs a
multi-scale design and the opset subset needs to grow first.

**The fourth plane is the conditioning.** It carries `NoiseModel::sigma_at` evaluated at each
pixel's own signal level, and it is a plane rather than a scalar because the predicted sigma is
signal-dependent: shot noise grows with the square root of the signal, so a shadow and a highlight
in the same frame have different sigmas. A scalar would force the network to learn that
relationship from the pixels it is trying to denoise. Phase 18's matting head makes the same
distinction for its trimap, and the manifest says `linear_srgb+noise` for the same reason it says
`linear_srgb+trimap` there.

## Training data

**None. This head is an architecture fixture with deterministic pseudo-random weights.**

PHASE-22 section 8 asks for paired noisy/clean captures across the top twenty camera bodies and
six ISO steps - a bracketed low-ISO reference against a high-ISO capture of the same scene, on the
same tripod, in the same light. There are no camera files in this repository at all; phase 02's
first exit condition has been open since phase 03 and `docs/adr/ADR-0006-phase-02-waiver.md`
records it.

Synthetic noise alone was considered and rejected. The noise models in
`crates/aura-restore/config/noise_models/` are themselves derived from published specifications
rather than measured, so a network trained on noise synthesised from them would be a network
trained to invert a function this repository wrote. Its gates would measure the round trip.
ADR-0045 section 10 records the argument.

`aura_restore::decide::MODEL_VER` is therefore `0` and **this head is never consulted**. What runs
is the noise-model-conditioned edge-preserving filter in `aura_render::restore::denoise`, which is
a measurement rather than a model: it compares each local step against the sensor's own predicted
sigma and blends toward the neighbourhood only where the step is smaller than the sensor could
have produced by noise. ADR-0045 section 6 records why that is worth shipping - its failure mode
is leaving noise behind, which a photographer can see and correct.

## Latency

Not measured. This build links no `wgpu` backend (ADR-0029 section 4), so section 11's 2.5 s
figure for a 45 MP frame on an RTX 4070 has no path to be measured on and is waived in the phase
22 exit report along with the three other device rows.

The processor reference path is budgeted in `perf/budgets.toml` under
`stage.restore_plan_frame`, and that budget is about producing a *decision* on a 2048 px proxy -
including up to four renders through the self-check - rather than about denoising a 45 MP frame.
Conflating the two would be a budget that looks strict and measures the wrong thing.

## Quality gate

Section 10.1's first row: "denoise PSNR/SSIM beats bilinear baseline decisively".
`tests/eval/restore_eval.rs` measures both against the clean plate the fixture noise was added to,
and against `fixtures::bilinear_baseline`, which is the plain box blur that "bilinear" means as a
denoiser. The reference path clears it.

Everything else in that row is **unmeasured**: expert preference at ISO >= 6400, chroma detail on
fabric, and the competitive study against DxO DeepPRIME, Topaz Photo AI and Lightroom AI Denoise
all need real photographs. Conditions C1 and C4 of `docs/progress/PHASE-22-EXIT.md`.

## Ethical and fairness notes

The denoiser has no per-person, per-ethnicity or per-age behaviour and no way to acquire one: it
reads a tile and a predicted sigma, and neither carries an identity. There is no skin-tone target
anywhere in this phase - phase 15's rule, and the phase gate scans migration 22 for a constant
that would break it.

There is one fairness consideration and it is worth stating rather than assuming away. Denoising
removes *texture*, and skin texture varies between people; a denoiser tuned on one population's
skin will smooth another's differently. The mitigation here is structural rather than promised:
the tier is chosen from a measured noise figure rather than from anything about the subject, and
`selfcheck::enforce` measures the **frame's own** high-band energy before and after and steps the
tier down if too much of it went. A frame whose texture is more fragile is a frame the guard
protects harder, without anybody having to classify whose skin it is.

The face is deliberately excluded from that texture measurement, for a different reason: face
recovery *raises* high-band energy inside a face, and a whole-frame ratio would let a strong
recovery hide a strong smear elsewhere. `docs/skin-fairness.md` carries this alongside the other
phases that touch skin.

## Known failure modes

- **Low-frequency chroma blotching survives.** The seven-pixel receptive field cannot see it. The
  reference filter's wide chroma radius handles it better than this head would.
- **Fine repeating texture reads as noise.** Lace, a herringbone weave and a beaded veil all have
  energy at the scale shot noise lives at, and this is the failure the smearing bound in
  `selfcheck` exists to catch on the rendered result rather than in the head.
- **An unmeasured camera's model is systematically wrong for that body**, in the same direction
  for every frame from it. `NoiseModel::tier_ceiling` caps such a body at `DenoiseTier::Standard`
  and `restore_plan.denoise_measured` records it per row; ADR-0045 section 3 has the asymmetry.
- **A frame with no phase 09 verdict is not denoised at all.** The plan records
  `restore_no_noise_reading` rather than guessing.

## Fallback

`aura_render::restore::denoise`, which is what runs today: a noise-model-conditioned
edge-preserving blend over separated luminance and chroma planes, with the chroma radius
`CHROMA_RADIUS_RATIO` times the luminance radius. It is deterministic, it has no weights, and it
does nothing at all when no sigma is available - `RestoreApplied::unconditioned` counts that case,
because a denoiser that does not know how much noise to expect is a blur.

## Rollback

`models.lock` pins the digest and `models/manifest.sig` signs the manifest. A version that fails
its first real use is rolled back automatically by `aura-models` and recorded as rejected; the
photographer keeps the quality they had that morning. Because `MODEL_VER` is `0` and nothing
consults this head, a rollback of this entry changes no stored decision in this build.

## Related

- `docs/adr/ADR-0045-restoration-denoise-sharpen-and-identity.md` - sections 3, 6 and 10.
- `docs/restoration.md` - what this does, in the product's own words.
- `docs/model-cards/face_recovery.md` - the other head this phase registers, and the one with no
  measured fallback at all.
- `crates/aura-restore/config/noise_models/` - the twenty bodies, and the `measured` flag that is
  false on all of them.
