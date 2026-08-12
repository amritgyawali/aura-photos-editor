# Data Model

## Storage layout

```
<project>.aura/
  catalog.sqlite           # WAL mode, the single source of truth
  catalog.sqlite-wal
  recipes/<image_id>.json   # optional export form; canonical copy lives in SQLite
  cache/<hh>/<hash>.jpg     # tier-1 preview
  cache/<hh>/<hash>.proxy   # tier-2 proxy (2048 px)
  cache/<hh>/<hash>.meta    # decode metadata + pipeline_ver
  masks/<hh>/<hash>.masks   # compressed mask payloads
  models.lock               # pinned model versions + signatures
  hardware_plan.json        # probed execution providers and budgets
  ledger/                   # decision ledger segments
  exports/                  # delivery manifests
```

The cache is content-addressed by `content_hash` (xxhash3-128) plus `pipeline_ver`, so changing a decode
or preview algorithm invalidates exactly what it should and nothing more.

## Core tables

| Table | Purpose | Key columns |
| --- | --- | --- |
| `projects` | One wedding | id, name, created_at, engine_ver, consent_flags |
| `cameras` | Bodies and profiles seen in the project | id, make, model, serial, clock_offset_ms |
| `images` | One row per file | id, path, content_hash, camera_id, exif_ts, timeline_ts, width, height, iso, aperture, shutter, focal, flash, orientation |
| `ingest_journal` | Crash-safe ingest progress | id, image_id, stage, status, ts |
| `embeddings` | Perceptual vectors | image_id, vec(512 fp16), dhash, hsv_hist, luma_stats |
| `faces` | Detections | id, image_id, box, landmarks, quality, eye_state, expression, embedding |
| `identities` | Recurring people | id, label, role, face_count, prominence, user_locked |
| `scenes` | Story segmentation | segment_id, image_id, scene_class, ritual, lighting, confidence |
| `moments` | Burst and moment grouping | moment_id, image_id, duplicate_set, relation |
| `integrity` | Technical quality | image_id, focus, motion, clipping, noise_sigma_rel, flags |
| `emotion` | Emotional evidence | image_id, intensity, moment_type, peak_proximity, reaction_of |
| `composition` | Aesthetic evidence | image_id, score, horizon_deg, headroom, balance, crop_hint |
| `selection` | Culling result | image_id, keep_score, selected, rank_in_moment, runner_up_of |
| `recipes` | Edit recipes | image_id, version, json, user_edited_fields, engine_ver |
| `masks` | Mask payloads | id, image_id, kind, identity_id, payload, feather, confidence, user_edited |
| `profiles` | Style profiles | id, name, version, tree_json, diagnostics_json |
| `camera_transforms` | Matching results | camera_id, flash, reference, transform_json, evidence_pairs |
| `scene_nodes` | Gallery tree | id, parent_id, segment_id, anchors_json, target_json |
| `qc_tickets` | QC findings | id, image_id, category, diagnosis, deviation, remedy, status, round |
| `decisions` | The ledger | id, image_id, kind, value_json, reasons_json, confidence, autonomy, actor, ts |
| `corrections` | Learning loop input | id, decision_id, before_json, after_json, magnitude, ts |
| `cloud_calls` | Cloud audit | id, task, tokens_in, tokens_out, cost_usd, latency_ms, cache_status |
| `cloud_budget` | Caps and spend | project_id, cap_usd, spent_usd, cap_calls, used_calls |

## Edit recipe (schema v1, the heart of the product)

```json
{
  "schema": 1,
  "image_id": "...",
  "global": {
    "exposure": 0.31, "temperature": 4930, "tint": 8, "contrast": 11,
    "highlights": -31, "shadows": 22, "whites": 7, "blacks": -9,
    "vibrance": 6, "saturation": 0, "curve": [[0,0],[64,60],[128,132],[255,255]],
    "hsl": { "orange": { "h": -2, "s": -4, "l": 3 } }
  },
  "lens": { "distortion": true, "vignette": 0.8, "ca": true, "profile": "sony-fe-35-1.4" },
  "geometry": { "rotate": -0.6, "crop": [0.02,0.0,0.98,1.0], "aspect": "original" },
  "masks": [ { "id": "m1", "kind": "face", "identity": "bride", "exposure": 0.22, "shadows": 12 } ],
  "retouch": { "preset": "natural", "ops": [ { "op": "blemish", "box": [0.41,0.28,0.43,0.30] } ] },
  "restoration": { "denoise": "standard", "sharpen": 0.35, "face_recovery": 0.2 },
  "bw": null,
  "provenance": {
    "scene": "indoor_ceremony", "confidence": 0.982, "engine_ver": "1.0.0",
    "models": { "scene-wedding": "1.2.0" },
    "user_edited_fields": ["global.exposure"],
    "cleanup_disclosures": []
  }
}
```

`user_edited_fields` is sacred: automation must never overwrite a field listed there.

## Migrations

Forward-only, numbered, one file per phase that changes the schema (`0001_init.sql` ... `0030_delivery.sql`).
Every migration has an up-test on a populated fixture database. A release may never require a manual database step.

## Ledger

Append-only decision records: what was decided, the value, structured reasons, confidence, autonomy band,
actor (model, rule, cloud, user) and timestamp. Budget: 6 MB per 1,000 images. The ledger powers
"Explain My Edit", QC diagnosis, the learning loop and support bundles.
