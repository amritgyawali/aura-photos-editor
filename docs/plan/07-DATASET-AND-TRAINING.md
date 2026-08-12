# The Wedding Intelligence Dataset

The user interface can be copied in six months. The dataset cannot. This document is the moat plan.

## What one labelled wedding contains

```
RAW originals
+ photographer's final edits
+ edit parameters (XMP or fitted recipe)
+ scene and ritual classification per frame
+ selected / rejected status with reasons
+ burst and duplicate groupings
+ technical quality labels (focus, motion, eye state)
+ emotional and moment labels with pairwise rankings
+ identity labels with roles (bride, groom, family, VIP, guest)
+ retouch labels (temporary versus permanent features)
+ album sequence and hero picks
+ consent record and licence terms
```

## Growth plan

| Stage | Weddings | Source | Purpose |
| --- | --- | --- | --- |
| Seed | 30 | Paid licensing from 6-10 photographers across traditions | Bootstrap every model to gate level |
| Beta | 200 | Closed beta, opt-in contribution with revenue share or free licence | Generalisation across gear and lighting |
| V1 | 1,000 | Opt-in from paying customers plus continued licensing | Scene-conditional depth |
| Scale | 10,000 (30 M frames) | Opt-in at scale | Cultural and regional coverage no competitor can match |

Deliberate diversity targets: Hindu, Nepali, Muslim, Christian, Sikh and civil ceremonies; day and night;
indoor and outdoor; flash and natural light; five skin-tone buckets with balanced representation;
Sony, Canon, Nikon, Fujifilm bodies; single and multi-shooter teams.

## Consent and ethics (non-negotiable)

- Contribution is **opt-in per project**, never per account, never by default.
- Photographers must warrant that they hold the rights and that couples have consented.
- Data is stored encrypted with access logging; identity labels are pseudonymous; withdrawal deletes
  contributed data and is honoured within 30 days.
- Models trained on contributed data are documented; contributors get free licence terms or revenue share.
- No face recognition data ever leaves a customer machine as part of telemetry.

## Labelling operations

- Two independent labellers per subjective task (emotion, aesthetics, keep/reject) plus adjudication;
  publish inter-annotator agreement with every dataset version.
- Pairwise comparisons rather than absolute scores for taste-based labels - people are consistent about
  "A is better than B" and inconsistent about "this is a 7".
- Active learning: prioritise labelling frames where the model is least confident or where users override most.
- Every dataset version is hashed, immutable and referenced in the model card of anything trained on it.

## Training discipline

- Split by **wedding**, never by frame, or the model will memorise weddings and report fantasy metrics.
- Always report per-subgroup metrics (skin tone, tradition, lighting, camera brand). A 10 % subgroup gap blocks release.
- Every experiment records seed, config, dataset hash, hardware and metrics. Unreproducible results do not count.
- The learning loop (Phase 30) is the long-term engine: every correction a photographer makes is a free,
  perfectly targeted label. Capture it, verify it, and adopt it only with the user's approval.
