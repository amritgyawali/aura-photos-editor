# Phase 30 - Delivery, Integrations, Learning Loop & Release Engineering

> **Single feature shipped by this phase:** Export and delivery (JPEG/TIFF/XMP, backup, client galleries), Lightroom and Photoshop integration, the learning loop that improves from every correction, and the release machinery that ships it all safely.
>
> **Mission:** Close the product: get finished work out of the app and into the client's hands, learn from every human correction, and make shipping updates a routine, reversible, well-tested event.

## 0. Phase card

| Field | Value |
|---|---|
| Phase | 30 of 30 |
| Epic | E6 - Curation & Delivery |
| Feature | Export and delivery (JPEG/TIFF/XMP, backup, client galleries), Lightroom and Photoshop integration, the learning loop that improves from every correction, and the release machinery that ships it all safely. |
| Depends on | Phases 01-29 |
| Unlocks | V1 launch and continuous improvement |
| Duration | 4 weeks |
| Primary owners | DevOps / Release Engineer, Tech Lead - Imaging Core (Rust), MLOps / Model Packaging Engineer, Product Manager Agent, Security & Privacy Engineer |
| Risk level | High - launch quality and data governance |
| Headline KPI | export 1,000 images (45 MP JPEG) <= 12 min on reference GPU; learning loop improves style match by >= 15 % after 3 corrected weddings; crash-free session rate >= 99.5 % |
| Competitor being beaten | Imagen cloud delivery; Aftershoot Lightroom integration; Pic-Time/ShootProof galleries |

## 1. Why this phase exists

A photographer's job ends when the client has the gallery, not when the pixels are finished. Export quality, naming, backup and gallery upload are the last mile that determines whether the product is actually used.

The learning loop is the compounding advantage: every correction a photographer makes should make their next wedding better, which turns usage into a moat that competitors cannot buy.

Release engineering is what keeps a complex AI application trustworthy over time: signed models, staged rollouts, crash reporting and instant rollback.

## 2. Scope contract

### 2.1 In scope

- Export engine: JPEG/TIFF/PNG with quality/resize/sharpen-for-output options, ICC embedding, metadata and copyright, file naming templates, folder structures, per-set exports (gallery, album, social, teaser, B&W).
- XMP/sidecar export for Lightroom hand-off, plus a Lightroom Classic plugin (import selection and recipes, round-trip) and a Photoshop plugin (open with masks and retouch layers where feasible).
- Backup: local/NAS/external destinations with verification hashes, plus optional cloud object storage; delivery bundle manifest with checksums.
- Client gallery integrations: Pic-Time / ShootProof / SmugMug / Google Drive / Dropbox style connectors via a pluggable provider interface, with upload resumption and per-set mapping.
- Learning loop: capture every user override (culling, parameters, masks, retouch strength, curation reorder), attribute it to the decision in the Phase 13 ledger, aggregate into preference updates, and retrain/adjust style profiles and ranker weights incrementally - locally, with an explicit review before adoption.
- Model and profile update channel: signed model packs, delta downloads, staged rollout, and one-click rollback.
- Release engineering: code signing and notarisation for Windows/macOS, installer, auto-update, crash reporting with opt-in, structured telemetry with consent, feature flags and kill switches.
- Licensing and entitlement: offline-tolerant licence checks, seat management, trial mode with clear limits.
- Support tooling: anonymised support bundles (Phase 13), diagnostics screen, and a reproducible-issue workflow.

### 2.2 Explicitly out of scope (do not build it here)

- Building a proprietary client gallery product (integrate, do not compete).
- Cross-machine distributed rendering (post-V1).
- Marketplace for third-party profiles (post-V1).

## 3. Architecture and data flow

```text
finished gallery + curation sets
     |
  EXPORT: naming templates, ICC, metadata, per-set outputs, verification hashes
     |
     +--> local/NAS/cloud backup (manifest + checksums)
     +--> XMP sidecars -> Lightroom plugin round-trip -> Photoshop hand-off
     +--> client gallery providers (resumable upload, per-set mapping)
     |
  USER CORRECTIONS (culling, params, masks, retouch, curation)
     |
  attribute to decisions (P13 ledger) -> preference aggregation -> profile/ranker updates
     |
  review & adopt (A/B vs current) -> signed profile/model update -> staged rollout | rollback
```

## 4. Files and modules to create

| Path | Purpose |
|---|---|
| `crates/aura-export/src/{lib,jpeg,tiff,naming,metadata,sets,verify,manifest}.rs` | Export engine. |
| `crates/aura-delivery/src/{lib,backup,providers/*,resume,mapping}.rs` | Backup and gallery providers. |
| `crates/aura-learn/src/{lib,capture,attribute,aggregate,update,review,rollback}.rs` | Learning loop. |
| `plugins/lightroom/` and `plugins/photoshop/` | Integration plugins. |
| `ops/{release,sign,notarise,update,flags,crash}/` | Release engineering. |
| `docs/{delivery,learning-loop,release-process,privacy}.md` | Operational documentation. |

## 5. Interfaces, schemas and contracts (freeze before coding)

**Delivery and learning contracts (frozen)**

```rust
pub struct ExportJob {
    pub sets: Vec<ExportSet>,          // { name, images, format, quality, resize, sharpen, naming }
    pub destination: Destination,      // Folder | Nas | CloudBucket | Provider(ProviderId)
    pub metadata: MetadataPolicy,      // copyright, contact, keywords, strip_gps
    pub verify: bool,                  // hash-verify every written file
}

pub struct DeliveryManifest {
    pub project: ProjectId, pub created_at: Timestamp,
    pub files: Vec<(PathBuf, u64, String)>,   // path, bytes, hash
    pub sets: Vec<(String, u32)>,
    pub qc_report_path: Option<PathBuf>,
    pub cleanup_disclosures: Vec<(ImageId, String)>,
    pub engine_versions: Vec<(String, String)>,
}

pub struct Correction {
    pub decision_id: DecisionId, pub kind: DecisionKind,
    pub before_json: String, pub after_json: String,
    pub scene: SceneId, pub identity: Option<IdentityId>,
    pub magnitude: f32, pub created_at: Timestamp,
}

pub struct LearningUpdate {
    pub profile_id: ProfileId, pub from_version: u16, pub to_version: u16,
    pub corrections_used: u32,
    pub expected_improvement: f32,     // measured on held-out corrections
    pub diff_summary: Vec<String>,
    pub adopted: bool,
}
```

## 6. Algorithm, model and implementation design

### 6.1 Export that a professional can trust

- Verification is mandatory by default: every written file is re-read and hashed, and the manifest records it - photographers have lost galleries to silent write failures.
- Naming templates cover the real conventions (date, couple, chapter, sequence, camera, original name) with collision-safe suffixes.
- Output sharpening is resolution-aware and applied after resize; metadata policy can strip GPS while preserving copyright.
- Per-set exports mean gallery, album, social, teaser and B&W sets come out of one job with correct sizes and aspects.

### 6.2 Integrations without lock-in

- XMP sidecars are the universal path: any photographer can take AURA's culling and grading into Lightroom, which lowers adoption risk enormously.
- The Lightroom plugin imports selections, flags, colour labels and recipes, and can round-trip corrections back into AURA as learning-loop input.
- Gallery providers sit behind one trait with resumable uploads, per-set mapping and clear error surfaces; adding a provider must not touch core code.

### 6.3 The learning loop, done safely

- Capture: every override is written as a `Correction` attributed to the originating decision, with scene and identity context.
- Aggregate: group corrections by (decision kind, scene bucket, identity role) and compute robust central tendencies; require a minimum count before acting, and discard outliers.
- Update: adjust the Phase 17 style deltas, Phase 10/11 ranker weights and Phase 12 threshold offsets incrementally - never a full retrain in the background without consent.
- Verify before adopting: measure expected improvement on held-out corrections and show an A/B comparison; the user adopts explicitly, and one click rolls back.
- All learning is local by default; contributing anonymised data to the Wedding Intelligence Dataset is strictly opt-in per project with a clear consent record.

### 6.4 Release engineering

- Signed and notarised installers, staged rollout by percentage, feature flags with kill switches for every AI stage, and a documented rollback within one release cycle.
- Model packs are versioned, signed and delta-updated; a model rollback must be possible without downgrading the app.
- Crash reporting and structured telemetry are opt-in, contain no image content, and are documented in the privacy page.
- Nightly long-run CI on real GPU hardware plus the full golden/eval suite gates every release; the release checklist is owned by EM and signed by CTO.

## 7. Cloud AI usage (bring-your-own API key)

No cloud AI call in this phase. The phase must work with the network cable unplugged; the Cloud AI Gateway from Phase 04 stays idle here.

## 8. Implementation order (execute literally, in this order)

1. Implement the export engine with naming, metadata, ICC, per-set outputs and verification.
2. Implement backup destinations with manifests and checksum verification.
3. Implement the provider interface and two gallery providers with resumable upload.
4. Ship XMP sidecar export and the Lightroom plugin with round-trip.
5. Ship the Photoshop hand-off (masks and retouch layers where feasible).
6. Implement correction capture and attribution to ledger decisions.
7. Implement aggregation, incremental updates, held-out verification and A/B review.
8. Implement signed model/profile update channel with staged rollout and rollback.
9. Implement licensing, crash reporting, telemetry consent and feature flags.
10. Build the release pipeline: signing, notarisation, installers, auto-update, nightly long-run CI.
11. Run a closed beta with 20 photographers; triage, fix, and gate V1 on the exit criteria.

## 9. Team assignment - who does exactly what

| Code | Agent role | Task | Deliverable | Est. |
|---|---|---|---|---|
| `DEVOPS` | DevOps / Release Engineer | Release pipeline, signing, notarisation, installers, auto-update, staged rollout, rollback | Release machinery | 12 d |
| `TLC` | Tech Lead - Imaging Core (Rust) | Export engine architecture, provider trait design, plugin boundaries | Architecture + review | 5 d |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | Export implementation, naming, metadata, verification, manifests | `aura-export` | 8 d |
| `MBE` | Mid-Level Backend / Cloud Engineer | Backup destinations, provider implementations, resumable upload, error surfaces | `aura-delivery` | 8 d |
| `MLOPS` | MLOps / Model Packaging Engineer | Learning loop: capture, aggregation, incremental updates, A/B verification, rollback | `aura-learn` | 9 d |
| `MLL` | ML Lead - Vision | Define which parameters may be learned, robustness rules and improvement metrics | Learning spec | 4 d |
| `SFE` | Senior Frontend Engineer (Tauri + React) | Export dialog, delivery screen, provider setup, learning-review UI, diagnostics screen | UI shipped | 8 d |
| `MFE` | Mid-Level Frontend Engineer | Naming template editor, per-set configuration, upload progress, rollback dialog | UI panels | 5 d |
| `SEC` | Security & Privacy Engineer | Licence security, telemetry/consent review, provider credential storage, privacy page sign-off | Security sign-off | 5 d |
| `PM` | Product Manager Agent | Own V1 exit criteria, pricing/licensing model, beta programme and launch messaging | Launch plan | 6 d |
| `QAL` | QA Lead - Automation | Export fidelity tests, verification tests, provider mocks, learning-loop regression gates | CI gates | 7 d |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | Closed beta triage with 20 photographers; own the launch bug bar | Beta report | 10 d |
| `EM` | Engineering Manager / Delivery Lead Agent | Own the release checklist, beta logistics, cross-team burndown to launch | Launch readiness | 8 d |
| `PERF` | Performance Engineer | Export throughput tuning; upload concurrency; long-run stability | Benchmark | 4 d |
| `DOC` | Technical Writer | Delivery guide, plugin docs, learning-loop explainer, privacy page, release notes process | Docs merged | 6 d |
| `CTO` | Chief Architect / CTO Agent | Sign the V1 release gate; approve rollback criteria and the post-launch on-call plan | Release sign-off | 2 d |

### 9.1 Handoff chain for this phase

```text
TLC architecture -> SRC export engine + MBE delivery/providers -> LR/PS plugins
                                |
                                v
          MLL learning spec -> MLOPS learning loop -> SFE/MFE delivery + review UI
                                |
     DEVOPS release machinery + SEC privacy/licence review -> EM/QAIQ closed beta -> CTO V1 gate
```

### How this agent team runs a phase (identical every time)

1. **Kickoff (PM + CTO + EM).** PM restates the feature as user stories, CTO writes/updates the ADR, EM cuts the task list from section 9 into the tracker.
2. **Design review (CTO + TLC + MLL + COL + UX).** Interfaces from section 5 are frozen before code. Any change after freeze needs an ADR amendment.
3. **Build in parallel lanes.** Core lane (TLC/SRC/SRG), ML lane (MLL/SRML/MLR/MLOPS), agent lane (AGT), UI lane (SFE/MFE/UX), data lane (DATA), platform lane (DEVOPS/SEC).
4. **Contract-first handoff.** A lane may only consume another lane's work through the frozen interface, using a stub/fixture until the real implementation lands.
5. **Code review chain.** Author -> peer in same lane -> lane lead -> CTO for anything touching an invariant. Two approvals minimum, one must be a lead.
6. **QA gate (QAL + QAIQ + PERF).** Unit + integration + golden-image + perceptual + performance suites must be green on the reference weddings.
7. **Phase gate (CTO + PM + EM).** All acceptance criteria in section 13 pass, telemetry is live, docs updated, demo recorded. Only then does the next phase start.
8. **Escalation.** Any blocker older than one working day goes to EM; any invariant conflict goes to CTO; any "we should ship it slightly broken" goes to PM and is written down.

### Branch, commit and PR rules

- Branch: `feat/phase-NN-<slug>`; one PR per task group, never one giant PR per phase.
- Conventional Commits (`feat(core): ...`, `fix(ml): ...`, `perf(render): ...`, `test(qa): ...`, `docs: ...`).
- Every PR states: what changed, which acceptance criterion it advances, benchmark delta, and screenshots or golden-image diffs when pixels change.
- CI must be green: `fmt`, `clippy -D warnings`, `cargo test`, `pytest`, `vitest`, golden-image diff, benchmark regression guard (<= 5 % slower), model-hash check.


## 10. Test plan

### 10.1 Phase-specific tests

- Export fidelity: rendered JPEG/TIFF match the reference render within a perceptual tolerance; ICC and metadata correct.
- Verification catches a deliberately corrupted write and fails the job with a clear error.
- Naming templates produce collision-free names across 4,000 files, including duplicate original names from two cameras.
- XMP round-trip: Lightroom shows AURA's selections and grading; corrections made there return as learning-loop input.
- Provider uploads resume correctly after a network drop; per-set mapping is respected.
- Learning loop improves style match by >= 15 % after 3 corrected weddings on held-out corrections, and rollback restores the previous profile exactly.
- No learning update is adopted without explicit user action; opt-in dataset contribution is off by default and recorded with consent.
- Signed model packs verify; tampered packs are rejected; model rollback works without downgrading the app.
- Installers are signed and notarised; auto-update applies and can be rolled back; kill switches disable each AI stage.
- Crash-free session rate >= 99.5 % across the closed beta.

### 10.2 Standing test matrix (applies to every phase)

| Layer | What it proves |
|---|---|
| Unit | Pure functions, thresholds, scoring maths, serialisation round-trips, error taxonomy. |
| Property/fuzz | Corrupt RAWs, truncated previews, absurd EXIF, 0-face and 60-face frames, 1-image and 6,000-image projects. |
| Golden image | Frozen fixture set rendered and compared pixel-wise; dE2000 mean <= 0.5, max <= 2.0 unless intentionally changed and re-blessed. |
| Perceptual (human) | QAIQ blind A/B against the previous build and against the named competitor for this feature; >= 60 % preference required. |
| Performance | Throughput, wall clock, peak RAM, peak VRAM on the three reference machines. |
| Resume/kill | Kill the process at 10 %, 50 %, 90 %; restart must continue without recomputation or corruption. |
| Regression | Full previous-phase suite must stay green; no acceptance criterion from an earlier phase may regress. |

Reference machines: RTX 4070 laptop (Win 11, 32 GB), M3 Pro MacBook (18 GB), Intel iGPU desktop (Win 11, 16 GB, DirectML fallback).

## 11. Performance budget and telemetry

| Metric | Budget |
|---|---|
| Export 1,000 images (45 MP JPEG, GPU) | <= 12 min |
| Export throughput | >= 1.4 images/s sustained |
| Hash verification overhead | <= 8 % of export time |
| Upload 1,000 images (100 Mbps) | <= 35 min with resumption |
| Learning update computation | <= 90 s per wedding of corrections |

Telemetry events (local-first, opt-in aggregation):

- `export.job` {sets, images, format, ms, verified, destination_kind}
- `delivery.upload` {provider, images, bytes, ms, resumes}
- `learn.corrections` {kind, count, mean_magnitude}
- `learn.update` {profile, expected_improvement, adopted}
- `release.update` {from_version, to_version, channel, rolled_back}

## 12. Failure modes and mitigations

| Failure mode | Mitigation |
|---|---|
| Silent export corruption | Mandatory hash verification, delivery manifest, and a failing job rather than a bad gallery. |
| Learning loop degrades quality over time | Held-out verification, explicit adoption, A/B comparison, versioned profiles, and one-click rollback. |
| Privacy backlash over telemetry or data collection | Opt-in only, no image content in telemetry, per-project consent for dataset contribution, and a plain-language privacy page. |
| Plugin breakage on Lightroom/Photoshop updates | Version detection, graceful degradation to XMP-only, and a compatibility matrix in CI. |
| Launch quality risk from 30 phases of integration | Closed beta with 20 photographers, published exit criteria, nightly long-run CI, feature flags and staged rollout. |
| Provider API changes | Single provider trait, contract tests against mocks and live sandboxes, and clear error surfaces. |

## 13. Acceptance criteria

- [ ] Finished galleries export in every required format and set, verified by checksums, with a delivery manifest.
- [ ] Lightroom and Photoshop users can adopt AURA without abandoning their workflow.
- [ ] Client galleries and backups upload automatically with resumption.
- [ ] Every correction a photographer makes measurably improves their next wedding, only after they approve the update.
- [ ] Releases are signed, staged, flag-controlled and reversible; models can roll back independently.
- [ ] V1 exit criteria are met and signed off by the CTO after the closed beta.

## 14. Definition of Done (phase gate)

- [ ] All acceptance criteria in section 13 verified by QA on the three reference weddings (indoor Hindu night ceremony, outdoor daylight Christian wedding, mixed-light Nepali reception).
- [ ] Unit, integration, golden-image, perceptual and performance suites green in CI on Windows (NVIDIA), Windows (integrated/DirectML) and macOS (Apple Silicon).
- [ ] Performance budget in section 11 met or a signed waiver from PERF + CTO recorded in the ADR.
- [ ] Telemetry events from section 11 visible in the local metrics dashboard and in the opt-in aggregate pipeline.
- [ ] Every new AI decision surface returns `confidence` + `reasons[]` and is rendered in the Explain panel.
- [ ] Docs updated: module README, model card (if a model shipped), in-app help string, CHANGELOG entry.
- [ ] Rollback path exists: feature flag off, previous model version pinnable, catalog migration reversible.
- [ ] Demo recording of the feature running on a real 3,000-image wedding attached to the phase gate.

Inherited invariants that this phase must not break:

- **Never mutate a RAW file.** Every decision is a row in SQLite plus a JSON edit recipe. Originals are opened read-only.
- **Every AI decision carries `confidence` (0-1) and `reasons[]`.** A decision without an explanation is a bug.
- **Three-tier compute.** Cheap analysis on embedded previews, medium analysis on 2048 px proxies, expensive work only on survivors.
- **Determinism.** Same inputs + same model versions + same seed = byte-identical recipe JSON. All models are pinned by hash.
- **Resumability.** Any job can be killed at any moment and resumed without recomputing finished work.
- **Local-first.** The product must complete a full wedding with no network. Cloud AI is an accelerator, never a dependency.
- **Scene-conditioned everything.** No threshold is global; every threshold is a function of the detected scene and subject role.
- **Colour discipline.** Work in linear scene-referred space, convert once, and never let a grade move skin outside its guarded region.
- **No silent failure.** Every module emits a typed error, a fallback path and a telemetry event.

## 15. Claude Code execution prompt (copy-paste this)

```text
You are the engineering team of AURA Wedding AI working as autonomous agents. Execute PHASE 30 - Delivery, Integrations, Learning Loop & Release Engineering.

Read first (in this order):
  docs/CLAUDE.md
  docs/01-ARCHITECTURE.md
  docs/03-DATA-MODEL.md
  docs/phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md   <- this phase, obey sections 2, 5, 8, 13

Goal: ship exactly one feature - Export and delivery (JPEG/TIFF/XMP, backup, client galleries), Lightroom and Photoshop integration, the learning loop that improves from every correction, and the release machinery that ships it all safely.

Rules:
  - Do not start Phase 31. Do not implement anything listed in section 2.2.
  - Freeze the interfaces in section 5 first; write them as code with doc comments, then implement.
  - Follow the invariants in section 14. Never write to a RAW file. Every decision needs confidence + reasons.
  - Create/modify only these areas: `crates/aura-export/src/{lib,jpeg,tiff,naming,metadata,sets,verify,manifest}.rs`, `crates/aura-delivery/src/{lib,backup,providers/*,resume,mapping}.rs`, `crates/aura-learn/src/{lib,capture,attribute,aggregate,update,review,rollback}.rs`, `plugins/lightroom/` and `plugins/photoshop/`, `ops/{release,sign,notarise,update,flags,crash}/`, `docs/{delivery,learning-loop,release-process,privacy}.md`
  - Work task by task using section 9. For each task: write the test first, implement, run the suite, commit with Conventional Commits.
  - Use branch feat/phase-30-delivery-integrations-learning-loop and open one PR per task group with docs/templates/PR.md.
  - After every task, append a line to docs/progress/PHASE-30.md: task id, files touched, tests added, benchmark delta.

Stop conditions:
  - Every checkbox in section 13 is proven by a passing automated test or a recorded QA result.
  - `just phase-30-verify` exits 0 on the reference wedding fixtures.
  - If a section-5 interface must change, stop, write docs/adr/ADR-30-<slug>.md, and continue only after recording the decision.

Deliver at the end: the phase exit report (docs/progress/PHASE-30-EXIT.md) containing acceptance-criteria evidence, benchmark table, known issues and the rollback switch.
```

---

*Phase 30 of 30 - Delivery, Integrations, Learning Loop & Release Engineering - part of the AURA Wedding AI master build plan.*
