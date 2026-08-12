# AI Model Stack

## Local models (ONNX, shipped and signed)

| Model | Job | Approx size | Runtime target | Quality gate |
| --- | --- | --- | --- | --- |
| `embed-clipish-512` | Perceptual embedding for similarity, clustering, uniqueness | 90 MB | 40 img/s GPU | dup recall 0.98 / precision 0.95 |
| `face-detect` | Face + landmark detection | 12 MB | 60 img/s | recall 0.97 at 24 px |
| `face-embed` | Identity embedding | 45 MB | 200 faces/s | identity F1 0.93 |
| `face-attr` | Eye state, gaze, expression, occlusion | 18 MB | 150 faces/s | blink F1 0.95 |
| `scene-wedding` | Scene class (17) + ritual + lighting + venue | 60 MB | 80 img/s | top-1 0.92 |
| `integrity` | Focus, motion, subject sharpness | 22 MB | 100 img/s | focus AUC 0.96 |
| `emotion-moment` | Emotion intensity, moment type, peak proximity | 40 MB | 60 img/s | pairwise agreement 0.80 |
| `aesthetic` | Composition and aesthetic scoring | 35 MB | 80 img/s | agreement 0.78 |
| `segment-14` | Semantic masks (skin, face, hair, clothing, sky, ...) | 75 MB | 20 img/s | mIoU 0.92 skin/face |
| `matting` | Alpha refinement for hair, veil, rim light | 30 MB | 25 img/s | no visible halo at 100 % |
| `blemish-detect` | Skin anomaly detection | 25 MB | 40 faces/s | recall 0.90 |
| `permanent-features` | Mole/freckle/scar/tattoo classification | 15 MB | 60 faces/s | false-removal 2 %, tattoos 0 % |
| `denoise-raw` | Noise-model-conditioned RAW denoise | 120 MB | 2.5 s per 45 MP | expert preference 80 % at ISO 6400+ |
| `face-recovery` | Gentle restoration of slightly soft faces | 80 MB | 0.6 s per face | identity distance under threshold, else skip |
| `distraction-detect` | Wedding distraction vocabulary | 35 MB | 40 img/s | precision 0.85 |
| `inpaint` (optional pack) | Local diffusion cleanup | 1.2 GB | 3 s per region | artefact-free 98 % |
| `artefact-check` | Detects failed inpainting | 20 MB | 100 regions/s | catches 95 % of known-bad |

Every model ships with a model card in `docs/model-cards/`, per-subgroup metrics (including skin-tone
buckets), a signed manifest entry in `models.lock`, and a documented fallback if it is unavailable.

## Learned-but-not-neural components

Exposure targets, white-balance priors, tone intent, colour behaviour, culling weights, coverage rules
and style deltas are **small fitted models** (regressions, robust estimators, lookup surfaces).
They train in minutes, are inspectable, deterministic, and can be shipped as config. Use a neural network
only where a fitted model genuinely cannot express the behaviour.

## Cloud reasoning (your API key)

Cloud AI is used for **judgement, not throughput**. Six governed tasks:

| Task | Phase | Trigger | Budget |
| --- | --- | --- | --- |
| Ritual and cultural scene disambiguation | 07 | Low-confidence segments only | 15 calls |
| Moment significance arbitration | 10 | Ambiguous emotional peaks | 20 calls |
| Ambiguous keep/reject arbitration | 12 | Near-threshold, must-have-coverage cases | 25 calls |
| Cleanup editorial judgement | 24 | Mid-confidence removal candidates | 20 calls |
| QC triage and remediation planning | 27 | Multi-symptom images | 40 calls |
| Album sequencing and captions | 29 | Once per album draft | 15 calls |

Global default cap: **75 calls and USD 1.50 per 3,000-image wedding**, cache hit rate 70 % or better.

**Rules, enforced in code:** strict JSON schema; temperature 0; derivative data only (thumbnails, crops,
statistics); never a RAW file; never the key in logs or prompts; per-project consent; every call recorded with
cost; and a complete local fallback so the pipeline never depends on the network. The model proposes;
deterministic code decides and executes.

## Training and deployment loop

```
PyTorch training (ml/)
  -> eval gate (per-subgroup metrics, model card)
  -> ONNX export + parity verification (max abs diff under tolerance)
  -> quantisation (int8/fp16) + re-verify
  -> sign manifest (ed25519) -> models.lock
  -> staged rollout with per-model rollback
```

A model that cannot demonstrate parity after export and quantisation does not ship. A model without a
card does not ship. A model whose subgroup metrics diverge by more than 10 % does not ship.
