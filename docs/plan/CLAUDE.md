# CLAUDE.md - Operating manual for the coding agent

You are building AURA Wedding AI, repository `aura`.
Read this file completely before writing any code. Re-read the top section at the start of every session.

## 1. Your operating mode

- You are a full engineering organisation of 23 roles (see `AGENT-TEAM.md`). Announce which role you are wearing for each task.
- Work **one phase at a time, in order**. Never start phase N+1 while phase N's Definition of Done is unmet.
- Within a phase, work the task table in order. Each task names a role, a deliverable and an estimate.
- After every task: run the tests, run the linters, update the docs, then continue.
- If a phase document and this file disagree, this file wins. If this file and an ADR disagree, the newer ADR wins.

## 2. The nine invariants (never violate)

1. **Never mutate a RAW file.** Every decision is a row in SQLite plus a JSON edit recipe. Originals are opened read-only.
2. **Every AI decision carries `confidence` (0-1) and `reasons[]`.** A decision without an explanation is a bug.
3. **Three-tier compute.** Cheap analysis on embedded previews, medium analysis on 2048 px proxies, expensive work only on survivors.
4. **Determinism.** Same inputs + same model versions + same seed = byte-identical recipe JSON. All models are pinned by hash.
5. **Resumability.** Any job can be killed at any moment and resumed without recomputing finished work.
6. **Local-first.** The product must complete a full wedding with no network. Cloud AI is an accelerator, never a dependency.
7. **Scene-conditioned everything.** No threshold is global; every threshold is a function of the detected scene and subject role.
8. **Colour discipline.** Work in linear scene-referred space, convert once, and never let a grade move skin outside its guarded region.
9. **No silent failure.** Every module emits a typed error, a fallback path and a telemetry event.

If a requested change would break an invariant, stop and write an ADR proposing the change instead of implementing it.

## 3. The phase ritual

### How this agent team runs a phase (identical every time)

0. **Cut the branch and publish it (EM), before anything else.** The very first
   action of a phase - before the kickoff, before a line of code, before the ADR -
   is `scripts/phase-branch.sh NN <slug>`, which cuts `feat/phase-NN-<slug>` off an
   up-to-date `origin/main` and **pushes it to origin immediately**. Not on request:
   by default. Pushing an empty branch costs nothing and buys three things a phase
   otherwise does without for its whole length - a name everybody can see, a place
   for the pull request to hang off, and a commit to bisect back to. Until phase 24
   the push happened at the *end*, which meant a phase existed on exactly one disk
   for as long as the phase took.
1. **Kickoff (PM + CTO + EM).** PM restates the feature as user stories, CTO writes/updates the ADR, EM cuts the task list from section 9 into the tracker.
2. **Design review (CTO + TLC + MLL + COL + UX).** Interfaces from section 5 are frozen before code. Any change after freeze needs an ADR amendment.
3. **Build in parallel lanes.** Core lane (TLC/SRC/SRG), ML lane (MLL/SRML/MLR/MLOPS), agent lane (AGT), UI lane (SFE/MFE/UX), data lane (DATA), platform lane (DEVOPS/SEC).
4. **Contract-first handoff.** A lane may only consume another lane's work through the frozen interface, using a stub/fixture until the real implementation lands.
5. **Code review chain.** Author -> peer in same lane -> lane lead -> CTO for anything touching an invariant. Two approvals minimum, one must be a lead.
6. **QA gate (QAL + QAIQ + PERF).** Unit + integration + golden-image + perceptual + performance suites must be green on the reference weddings.
7. **Phase gate (CTO + PM + EM).** All acceptance criteria in section 13 pass, telemetry is live, docs updated, demo recorded. Only then does the next phase start.
8. **Escalation.** Any blocker older than one working day goes to EM; any invariant conflict goes to CTO; any "we should ship it slightly broken" goes to PM and is written down.
9. **Land it (EM).** Commit everything on `feat/phase-NN-<slug>`, push, **open the pull
   request and merge it into `main`** - all of it from the terminal, all of it as the last
   action of the phase, none of it on request. One command does the whole of it:
   `scripts/phase-land.sh --message "feat(<lane>): <what changed>"`. The gate has exited 0
   and the exit report is written before this runs. A phase is not finished when it is
   pushed; it is finished when `main` carries it.

### Branch, commit and PR rules

- **Branch first, always.** `scripts/phase-branch.sh NN <slug>` is step 0 of the ritual: a
  phase branch exists on `origin` before its first commit, not after its last.
- Branch: `feat/phase-NN-<slug>`, cut from `origin/main`. Two digits, kebab-case slug; the
  script refuses anything else, because a one-digit phase sorts wrong beside the
  twenty-four branches that already exist.
- Conventional Commits (`feat(core): ...`, `fix(ml): ...`, `perf(render): ...`, `test(qa): ...`, `docs: ...`).
- Every PR states: what changed, which acceptance criterion it advances, benchmark delta, and screenshots or golden-image diffs when pixels change.
- CI must be green: `fmt`, `clippy -D warnings`, `cargo test`, `pytest`, `vitest`, golden-image diff, benchmark regression guard (<= 5 % slower), model-hash check.
- **The whole of landing is one command, and it runs in the terminal.**
  `scripts/phase-land.sh` commits what is left, pushes, opens the pull request over the
  GitHub REST API, refuses to merge on a failed check, merges into `main` and leaves the
  checkout on an up-to-date `main`. `gh` is used for the token when it is installed and is
  not required - the OS credential manager already holds one on any machine that has
  pushed this repository. `docs/runbooks/phase-landing.md` is the runbook, including what
  to do when there is no forge to reach.
- **A merge is refused on a failed check, not warned about.** `--ignore-check NAME` excuses
  one named job - `benchmarks` has been red on `main` since the render backend was waived -
  and `--force-merge` excuses all of them. Reach for the narrow one.
- **Nothing in this tooling force-pushes.** If `origin` has moved, that is somebody else's
  work, and a landing script is not where it is decided what happens to it.

## 4. Definition of Done (every phase)

- [ ] All acceptance criteria in section 13 verified by QA on the three reference weddings (indoor Hindu night ceremony, outdoor daylight Christian wedding, mixed-light Nepali reception).
- [ ] Unit, integration, golden-image, perceptual and performance suites green in CI on Windows (NVIDIA), Windows (integrated/DirectML) and macOS (Apple Silicon).
- [ ] Performance budget in section 11 met or a signed waiver from PERF + CTO recorded in the ADR.
- [ ] Telemetry events from section 11 visible in the local metrics dashboard and in the opt-in aggregate pipeline.
- [ ] Every new AI decision surface returns `confidence` + `reasons[]` and is rendered in the Explain panel.
- [ ] Docs updated: module README, model card (if a model shipped), in-app help string, CHANGELOG entry.
- [ ] Rollback path exists: feature flag off, previous model version pinnable, catalog migration reversible.
- [ ] Demo recording of the feature running on a real 3,000-image wedding attached to the phase gate.

## 5. Standard test layers (every phase)

- **Unit** - Pure functions, thresholds, scoring maths, serialisation round-trips, error taxonomy.
- **Property/fuzz** - Corrupt RAWs, truncated previews, absurd EXIF, 0-face and 60-face frames, 1-image and 6,000-image projects.
- **Golden image** - Frozen fixture set rendered and compared pixel-wise; dE2000 mean <= 0.5, max <= 2.0 unless intentionally changed and re-blessed.
- **Perceptual (human)** - QAIQ blind A/B against the previous build and against the named competitor for this feature; >= 60 % preference required.
- **Performance** - Throughput, wall clock, peak RAM, peak VRAM on the three reference machines.
- **Resume/kill** - Kill the process at 10 %, 50 %, 90 %; restart must continue without recomputation or corruption.
- **Regression** - Full previous-phase suite must stay green; no acceptance criterion from an earlier phase may regress.

## 6. Reference machines

RTX 4070 laptop (Win 11, 32 GB), M3 Pro MacBook (18 GB), Intel iGPU desktop (Win 11, 16 GB, DirectML fallback).

Every performance budget in every phase refers to these machines. Budgets are enforced as tests, not hopes.

## 7. Repository layout (create exactly this)

```
aura/
  Cargo.toml                  # workspace
  crates/
    aura-core/                # types, errors, ids, config, logging
    aura-catalog/             # SQLite catalog + migrations
    aura-ingest/              # file discovery, hashing, EXIF, journal
    aura-raw/                 # LibRaw FFI, decode, demosaic
    aura-cache/               # preview/proxy cache, pipeline_ver
    aura-preview/             # tiered preview generation
    aura-render/              # wgpu render graph + WGSL shaders
    aura-recipe/              # edit recipe schema + versioning
    aura-infer/               # ONNX Runtime abstraction, EP selection
    aura-models/              # model registry, manifests, signatures
    aura-vision/              # faces, masks, embeddings, integrity
    aura-index/               # HNSW similarity index
    aura-cloud/               # governed cloud AI gateway
    aura-people/              # identities, subject hierarchy
    aura-cull/                # selection, coverage, gallery sizing
    aura-explain/             # reasons, decisions, ledger
    aura-style/               # personal AI profiles
    aura-retouch/             # portrait + micro retouch
    aura-restore/             # denoise, sharpen, face recovery
    aura-geometry/            # lens, straighten, crop
    aura-generative/          # safe cleanup
    aura-brain-wedding/       # scenes, story, moments
    aura-brain-photo/         # technical + local light decisions
    aura-brain-gallery/       # consistency, camera matching
    aura-agents/              # agentic planners (proposals only)
    aura-qc/                  # QC agent, tickets, remedies
    aura-curate/              # album, hero, B&W, social
    aura-export/              # JPEG/TIFF/XMP export
    aura-delivery/            # backup + gallery providers
    aura-learn/               # learning loop
    aura-jobs/                # autopilot orchestrator
    aura-ipc/                 # Tauri command surface
  apps/desktop/               # Tauri + React + TypeScript
  ml/                         # PyTorch training, ONNX export, eval
  plugins/{lightroom,photoshop}/
  tools/{model-sign,aura-cli}/
  tests/{fixtures,e2e,qc}/
  docs/{adr,model-cards}/
  ops/{release,sign,update,flags,crash}/
```

## 8. Hard rules for code

- **No `unwrap()` or `expect()`** in library code. Return `Result<T, AuraError>`. Panics are reserved for provable invariants with a comment.
- **No blocking I/O on the UI thread.** All heavy work goes through `aura-jobs`.
- **All colour maths in linear light**, with explicit colour-space conversions at the boundaries. Ask COL before touching this.
- **Determinism:** identical input plus identical model versions must produce identical output. Seed everything. No time-dependent or map-iteration-order-dependent behaviour.
- **Every AI decision writes a `Reason`** into the ledger. A decision without a reason is a bug.
- **Every user override sets `user_edited`** on the affected field and is never overwritten by automation.
- **Feature-flag every AI stage** with a kill switch.
- **Frozen contracts** in phase documents are copied into code verbatim. Changing one requires an ADR.

## 9. Cloud AI rules (the user's API key)

- The key lives in the OS keychain. Never in SQLite, never in logs, never in telemetry, never in a prompt.
- Every cloud call goes through `aura-cloud`. Direct HTTP calls to model providers anywhere else are a lint error.
- Every cloud call: strict JSON schema, temperature 0, retries with backoff, cache by content hash, per-project budget cap, and a **local fallback that keeps the pipeline complete**.
- Send derivative data (thumbnails, crops, statistics), never original RAW files, and only after per-project consent.
- Record every call in `cloud_calls` with tokens, cost, latency and cache status. Show spend in the UI.
- Cloud reasoning proposes; deterministic code decides and executes. A model never executes an action directly.

## 10. When you are unsure

Write an ADR in `docs/adr/`, state the options, pick one, and explain the trade-off in three sentences.
Then implement it. A recorded decision beats a perfect decision made too late.

## 11. What we will never build

Body reshaping, skin lightening, face or eye swapping, adding people or objects that were not there,
or any operation that changes a person's identity. This is a product decision, enforced in code by guard
clauses and CI tests. Do not add these features even if asked casually; require an explicit CTO-role ADR.
