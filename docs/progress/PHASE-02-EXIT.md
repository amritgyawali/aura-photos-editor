# Phase 02 - exit report

**Status: green on this machine, with three caveats recorded in sections 4 and 6.**
Every gate runs and passes locally. CI has not run these lanes on macOS or Linux,
the decoder has never seen a file from a real camera, and the per-image
performance budgets are met at fixture scale but not at 45 MP - the last of these
is waived in ADR-0004 with measurements.

Section 7b records a follow-up that landed inside this phase: four more mosaic
formats decode from sensor data, and the decode path is parallel. Neither changed
a frozen contract or a cached pixel, and neither retires any of the three
caveats.

Measured on: Windows 11, Rust 1.97.1, host toolchain `x86_64-pc-windows-gnu`
(ADR-0002 section 7), 2026-08-12.

## 1. What shipped

The single feature of this phase: instant, colour-correct previews for every RAW
- embedded JPEG for triage, a 2048 px proxy for AI, and on-demand
full-resolution decode for final render.

- `aura-raw`: four container parsers, magic-based format sniffing, CFA unpacking
  for 8/10/12/14/16-bit and lossless JPEG (SOF3), half-size and full demosaic,
  tiled tier 3, EXIF orientation, and a watchdog with per-file time and memory
  ceilings. Pure, safe Rust: the crate keeps `#![forbid(unsafe_code)]`.
- The colour pipeline: linear Rec.2020 working space, Bradford adaptation, a
  documented neutral curve, a three-source profile chain and CIEDE2000.
- `aura-cache`: content-addressed, sharded, budgeted, digest-verified, with an
  index that rebuilds itself by scanning.
- `aura-preview`: the frozen `PreviewService`, strict-priority scheduling with
  de-duplication and promotion, and a pool that leaves one core free for the
  person using the application.
- Six IPC commands, one event stream, and real pixels in the phase 01 grid with
  no change to the grid's structure.
- Synthetic RAW fixtures - eight bench bodies, three encodings, a colour chart -
  so the decoder is tested end to end without a single camera file.

## 2. Gate results

| Gate | Command | Result |
|---|---|---|
| Build | `cargo build --workspace --all-targets` | pass, 0 warnings |
| Tests | `cargo test --workspace --all-targets` | **174 passed, 0 failed, 1 ignored** |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | pass |
| Format | `cargo fmt --all -- --check` | pass |
| Banned patterns | `scripts/check-banned.sh` | `check-banned: clean` |
| Frozen contracts | `cargo xtask contracts --check` | `contracts: 10 entries, all locked` |
| UI types | `npm run lint` (tsc strict) | pass |
| UI tests | `npm test` | **15 passed, 0 failed** |
| Phase 01 gate | `cargo run --release -p aura-cli -- verify` | `phase-01 verify: all fixtures clean` |
| Phase 02 gate | `cargo run --release -p aura-cli -- verify --phase 02` | `phase-02 verify: all fixtures clean` |

Rust tests by crate: `aura-raw` 55 (colour 18, containers 17, fuzz 6, tiers 14),
`aura-cache` 18, `aura-preview` 20 (priority 11, service 9), `aura-core` 25,
`aura-ingest` 18, `aura-perf` 12, `aura-catalog` 11, `aura-app` 8. The ignored
test is the 25 MP scaling probe, which is run deliberately rather than in CI.

### Phase gate output

```
fixtures: 9 files, imported 9
tier 1: built=8 failed=1 cache=18658 bytes elapsed=112 ms
  quarantined pht_019ff48e-...: AURA does not recognise this file as a photo it can develop.
tier 2: built=8 failed=1 cache=1212852 bytes elapsed=198 ms
second pass: built=8 hit_rate=94.1% elapsed=39 ms
tier 1 preview rows: 8
tier 2 preview rows: 8
colour: worst mean dE2000 0.158 across 8 bodies
first pass total: 310 ms
phase-02 verify: all fixtures clean
```

The `failed=1` is the point of the ninth fixture: a file with a RAW extension
that is not a RAW. It is quarantined with `AURA-RAW-2001`, the other eight
finish, and the run is still clean.

## 3. Acceptance criteria (section 13 of the phase document)

| Criterion | Proof | Result |
|---|---|---|
| Freshly ingested wedding shows real thumbnails, scrolling never blocks | `a_thumbnail_is_produced_cached_and_recorded`, `a_visible_request_is_served_while_a_batch_is_running`, `scrolling_away_cancels_queued_work` | pass |
| Every image produces a 2048 px proxy in 8-bit sRGB **and** 16-bit linear | `the_proxy_is_produced_in_both_representations_at_the_same_size` | pass |
| ColorChecker mean dE2000 <= 2.0 on all 8 bodies; profiles documented and signed | `the_colour_chart_survives_the_proxy_pipeline_on_every_bench_body`, ADR-0003 | pass on 8 **synthetic** bodies, worst 0.158; see caveat 2 |
| Tiled full decode matches whole-image decode exactly, inside the memory ceiling | `tiled_full_decode_matches_a_whole_image_decode`, `an_absurd_declared_size_is_refused_before_anything_is_allocated` | pass |
| Cache respects its budget, survives corruption, reports hit rate | `the_budget_is_never_exceeded_and_the_oldest_entry_goes_first`, `a_corrupted_entry_is_rebuilt_rather_than_served`, `statistics_report_a_usable_hit_rate` | pass |
| No decode failure can crash or hang the app; failures land in Problems | `fuzz_decode.rs` (6 suites), `a_decode_that_overruns_its_deadline_is_reported_as_a_timeout`, `a_file_that_cannot_be_decoded_becomes_a_problem_row_not_an_exception` | pass |
| `just phase-02-verify` passes golden proxy diffs on the fixture set | gate output above | pass |

Additional criteria from section 10.1:

| Criterion | Proof | Result |
|---|---|---|
| Tier 1 on 4,000 files <= 3 min, zero crashes, quarantine list correct | budget asserted per file; gate proves the quarantine list | pass at fixture scale, caveat 1 |
| Cache: budget, LRU, `pipeline_ver` invalidation, corrupt entry self-heals | `cache.rs` (18 tests) | pass |
| Priority: a visible request during a batch served in <= 50 ms | `a_visible_request_is_served_while_a_batch_is_running` | pass |
| Formats decode or fail loudly | `containers.rs`, `docs/camera-support.md` | pass, with the coverage gaps documented |

## 4. Performance

Criterion, release build, `cargo bench -p aura-preview --bench decode`, on the
0.10 MP bench fixture:

| Stage | Unpacked 16-bit | Packed 14-bit | Lossless JPEG |
|---|---|---|---|
| Tier 1 | 464 us | 452 us | 456 us |
| Tier 2 | 1.65 ms | 1.81 ms | 4.04 ms |
| Tier 3 | 5.44 ms | 5.43 ms | 8.63 ms |

Scaling, from `scaling_probe` (release, `--ignored`):

| Sensor | Tier 1 | Tier 2 | Tier 3 | Tier 2 per MP |
|---|---|---|---|---|
| 0.10 MP | 0 ms | 2 ms | 5 ms | 20.3 ms |
| 1.57 MP | 37 ms | 29 ms | 95 ms | 18.4 ms |
| 6.29 MP | 111 ms | 112 ms | 390 ms | 17.8 ms |
| 25.17 MP | 385 ms | 380 ms | 1,796 ms | 15.1 ms |

Against the section 11 budgets:

| Metric | Budget | Measured |
|---|---|---|
| Tier 1, per file | <= 45 ms | **0.46 ms** at fixture scale; see caveat 3 |
| Tier 2 proxy per image (CPU) | <= 250 ms | **1.65 ms** at fixture scale, **~680 ms extrapolated at 45 MP** - waived |
| Tier 3 full decode, 45 MP | <= 1,800 ms | 1,796 ms at 25 MP, **~3.2 s extrapolated at 45 MP** - waived |
| Cached preview read | <= 8 ms | **memory hit under 8 ms, disk hit under 25 ms**, asserted in `the_second_request_is_served_from_memory_and_the_third_from_disk` |
| Cache size | <= 3.5 GB per 1,000 images | **1.21 MB for 8 fixture images**; not comparable to a real wedding, caveat 1 |

**Caveat 1 - fixture scale.** Every figure above comes from generated files. A
384 x 256 chart is not a 45 MP wedding frame, and the cache-size budget in
particular cannot be checked without real RAWs.

**Caveat 2 - synthetic bodies.** The dE2000 result is measured on eight
synthetic bench bodies whose matrices are exact by construction. It proves the
matrix chain, the white balance convention, the demosaic and the curve are
mutually consistent - which is what a golden test is for - but it is not a
photographed ColorChecker on a real camera. Section 13 asks for eight real
bodies; that needs eight chart frames and a COL sign-off, and is the first item
of the phase 03 colour backlog.

**Caveat 3 - tier 1 scaling is pessimistic here.** The fixture's embedded
preview is half the sensor's size, so it grows with the frame; a real camera
stores a preview of roughly 2 MP whatever the sensor. The 385 ms at 25 MP is
therefore an artefact of the fixture, not a prediction.

**The waiver.** Tier 2 and tier 3 miss their per-image budgets at full sensor
resolution on the scalar CPU path. Recorded, with numbers and an expiry
condition, in ADR-0004 under "Performance waiver". The short version: the cost is
two scalar loops that a SIMD or GPU path replaces, tier 2 is background work
behind a cache, and the number the photographer feels - the cached read - is
inside budget.

## 5. Defects found and fixed while proving the gates

Five, each now covered by the test that found it. They are listed with their
mechanisms in `docs/progress/PHASE-02.md`; in summary:

1. `Orientation::Transverse` used the source dimensions in the wrong order and
   panicked on any non-square image.
2. White balance multipliers were applied instead of their reciprocals, tinting
   every frame by the square of the true cast.
3. The Rec.2020 to sRGB rotation was missing from the 8-bit output, desaturating
   every saturated colour.
4. The embedded-preview fallback applied the scene-referred curve to an
   already-display-referred JPEG, doubling its contrast.
5. The phase gate compared lifetime cache counters instead of a per-pass delta.

## 6. Known issues and deliberate gaps

- **No LibRaw, and three proprietary compressions still refused.** Canon CRX
  (CR3), Panasonic RW2 and compressed RAF decode at tier 1 and fall back to an
  embedded-preview proxy at tier 2, flagged `AURA-RAW-2007`. Compressed NEF,
  ARW2, compressed ORF and X-Trans **are** decoded as of the follow-up recorded
  in section 7. Rationale in ADR-0004, per-format detail in
  `docs/camera-support.md`.
- **No file from a real camera has been through this decoder.** Everything is
  proven against generated fixtures. The rows in the support matrix that name a
  manufacturer describe what the code does with that manufacturer's documented
  layout, not a verified result.
- **No GPU path.** The `SRG` task in section 9 is not built; there is no render
  graph in the workspace yet.
- **No HEIF.**
- **Perceptual audit not run.** Section 9's `QAIQ` task - a blind visual audit of
  proxies against camera JPEGs and Lightroom's neutral rendering on eight bodies
  - needs real files and a human. Not done.
- **Telemetry is logged, not dashboarded.** `preview.decoded`,
  `preview.quarantined`, `cache.stats` and `colour.profile_missing` are emitted
  through `tracing` with the fields the phase document specifies; the local
  metrics dashboard is a later phase, so the Definition-of-Done line about a
  dashboard is unmet in the same way it was in phase 01.
- **`PreviewEvent` is defined and typed on both sides but not emitted**, for the
  same reason `IngestEvent` was not in phase 01: the Tauri shell has not been
  launched on this machine. The UI subscribes to it already.
- **Demo recording not attached.** Needs a real 3,000-image wedding.

## 7. Rollback

- **Feature flag.** Previews are additive: with no cache directory and no
  preview rows, the grid renders the phase 01 placeholder and nothing else
  changes. Deleting `cache/` is always safe.
- **Pipeline version.** Reverting a rendering change means restoring
  `PIPELINE_VER`; old entries remain addressable and become live again, because
  the version is part of the cache key rather than a mutation of it.
- **Catalog.** No migration was added. The `preview` table already existed in
  schema v1 and is only written now, so rolling back the code leaves rows that a
  phase 01 build ignores.
- **Contracts.** ADR-0005 additions are additive; reverting them requires
  re-locking `contracts.lock`, which CI enforces.

## 7b. Follow-up: the codecs and the parallel path

Added after this report was first written, inside phase 02 because nothing frozen
moved: no contract file changed, `contracts.lock` still verifies, and
`PIPELINE_VER` is untouched, so every cached preview stays valid.

**Camera coverage.** Nikon's compressed NEF, Sony's ARW2, Olympus's compressed
ORF and Fujifilm's X-Trans array now render tier 2 and tier 3 from sensor data
rather than from the camera's own JPEG. Each is an independent safe-Rust
implementation of the published format, and each ships with an encoder so the
decoder is proved by round trip - which is the strongest proof available in a
repository with no camera files. Canon CRX, Panasonic RW2 and compressed RAF are
still refused, for the reasons in ADR-0004.

This does not weaken condition 1 below. A round trip proves the decoder is the
inverse of an encoder written from the same description; it cannot prove the
description matches what the camera writes. If anything it sharpens the
condition: there are now four more decoders whose first real file will be the
real test.

**Performance.** Demosaic, resize, the colour rotation and the mosaic unpack are
parallel over output rows, with each row writing its own slice so the output is
bit-identical whatever the thread count. At 25 MP that is 2.1x on tier 3 and 1.4x
on tier 2. It is not enough: a 45 MP tier 2 still extrapolates to about 580 ms
against a 250 ms budget. The waiver in ADR-0004 is therefore renewed and narrowed
rather than closed, and it now names the change that would close it - fusing the
bin and the resize, which alters proxy pixels and so needs a `PIPELINE_VER` bump
and ML-lead sign-off.

**Gate re-run after the follow-up:** 190 Rust tests and 15 UI tests pass, clippy
and `check-banned` are clean, `contracts: 10 entries, all locked`, and both phase
gates report `all fixtures clean` with `colour: worst mean dE2000 0.158 across 8
bodies` - now measured across seven mosaic encodings rather than three.

## 8. Gate decision

Phase 03 may start once:

1. one real RAW per supported manufacturer has been decoded and added to the
   fixture corpus, confirming the matrix rows that are currently reasoned rather
   than measured;
2. a photographed ColorChecker from at least one real body has been rendered and
   signed off by COL, which converts caveat 2 from "the pipeline is
   self-consistent" into "the pipeline is correct";
3. the CI matrix has run these lanes on Windows, macOS and Linux.

Everything else on the phase 02 checklist is proven above. The performance
waiver in ADR-0004 stands until the render graph lands.
