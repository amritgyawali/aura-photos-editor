# Architecture

## Principles

1. **Pipeline of specialists, not one giant model.** Each stage is independently improvable, testable and replaceable.
2. **Three-tier compute.** Cheap analysis on embedded previews, medium analysis on proxies, expensive work only on final selects.
3. **Local first, cloud optional.** Every stage has a complete local path. Cloud adds judgement and heavy generation.
4. **Non-destructive by construction.** The RAW file is read-only. The recipe is the truth.
5. **Explainability is data, not text generation.** Reasons are structured records emitted by the deciding code.
6. **Determinism.** Same inputs plus same model versions equals same outputs, always.

## Process and thread model

```
Tauri main process (Rust)
  |- UI (WebView: React + TypeScript)          <- IPC commands + event stream
  |- Job runtime (tokio multi-thread)
  |    |- CPU pool (rayon)      decode, hashing, EXIF, classical CV
  |    |- GPU queue (wgpu)      previews, render, masks, restoration
  |    |- Infer pool (ORT)      batched model inference, EP-aware
  |    +- Cloud queue           rate-limited, budgeted, cached
  |- Catalog (SQLite WAL, single writer, many readers)
  +- Cache (content-addressed files on disk)
```

One writer to SQLite, serialised through a channel. Long jobs never hold a transaction open.
The UI never blocks: it subscribes to progress events and reads snapshots.

## The pipeline

```
RAW files
  -> ingest (discover, hash, EXIF, timeline)
  -> previews (tier 1 embedded JPEG, tier 2 proxy 2048, tier 3 full)
  -> embeddings + similarity index
  -> faces + identities + subject hierarchy
  -> scenes + story segmentation + rituals
  -> bursts + duplicates + moments
  -> frame integrity (focus, motion, exposure, noise, eyes)
  -> emotion + moment ranking
  -> composition + aesthetics
  -> CULL (keep scores, coverage guard, gallery sizing)
  ============ selected frames only from here ============
  -> masks (semantic, identity-scoped, matted)
  -> exposure + white balance
  -> tone + curves + HSL + skin guard
  -> style profile deltas
  -> local light sculpting
  -> portrait retouch + micro retouch
  -> restoration (denoise, sharpen, face recovery)
  -> geometry (lens, straighten, crop)
  -> generative cleanup (safe, reviewed)
  -> camera + shooter matching
  -> gallery consistency (reference frames, skin, scene)
  -> QC agent (inspect, fix, replace, escalate)
  -> curation (B&W, heroes, album, social)
  -> export + delivery + learning loop
```

## Why the three-tier compute model matters

| Tier | Input | Work | Volume |
| --- | --- | --- | --- |
| 1 | Embedded RAW preview | Embeddings, faces, scene, integrity, emotion, composition, culling | All 4,000 |
| 2 | Proxy 2048 px | Masks, WB/exposure/tone/colour, style, local light, retouch decisions | ~1,000-1,800 selected |
| 3 | Full resolution | Final render, retouch pixels, restoration, cleanup, export | ~1,000 final |

Spending five seconds retouching a frame the culler will reject is the single most common way these
pipelines become slow. Tiering is not an optimisation; it is the architecture.

## Hardware strategy

At first run, `aura-infer` probes available execution providers and writes `hardware_plan.json`:
provider order (TensorRT, CUDA, DirectML, CoreML, CPU), VRAM budget, default batch sizes and probe timings.
Every later stage reads that plan instead of guessing. Under VRAM pressure, batches shrink before anything fails.

## IPC surface

Typed Tauri commands, one module per domain, mirrored by generated TypeScript types.
No command may take longer than 50 ms: anything heavier returns a job handle and streams progress events.

## Failure philosophy

- Every stage is resumable and idempotent.
- Optional stages may fail without failing the wedding; the run reports `CompletedDegraded` with specifics.
- Any stage can be disabled by a feature flag, and the pipeline must still produce a deliverable gallery.
