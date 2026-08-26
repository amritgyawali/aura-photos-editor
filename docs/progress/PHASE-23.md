# Phase 23 progress - Geometry Suite

One line per task group: what was touched, what was tested, what moved.

| Task | Files | Tests | Notes |
|---|---|---|---|
| CTO/PM kickoff | `docs/plan/phases/PHASE-23-GEOMETRY-SUITE.md` read; ADR-0041 and ADR-0042 drafted | - | Sections 2, 5, 8 and 13 are the contract. |
| Freeze section 5 | `crates/aura-core/src/contract/geometry.rs` (1,660 lines) | 13 contract tests | `GeometryPlan`, `CropVariant`, `CropSafetyReport`, `ProtectedRegion`, 24 reason codes, `GeometryService`. Const assertion that `STRAIGHTEN_ACT_AT >= HORIZON_ACT_AT`. |
| Error registry | `crates/aura-core/errors.toml`, `errors/ml.rs`, six runbooks | registry test | AURA-ML-5090 to 5095. |
| Lens profiles | `assets/lens_profiles/{README,synthetic}.toml`, `profiles.rs` | 7 | Log-focal interpolation; a row with no `measured_by` is refused; a duplicate lens id is refused rather than resolved by directory order. |
| Crop rules | `crates/aura-geometry/config/crop_rules.toml`, `rules.rs` | 8 | 23 scene rows each with a written reason; the loader may only tighten. |
| Lens decision + estimator | `lens.rs` | 9 | Three routes, CA withheld on an estimate, the tracker and the trimmed fit. |
| Straightening | `straighten.rs` | 9 | The 0.70 gate, the band, and the reduce-or-skip solve. Aspect-preserving inscribed rectangle. |
| Keystone | `keystone.rs` | 8 | Refused past the cap rather than clamped; three verticals minimum; restricted Hough tracker. |
| Safety filter | `safety.rs` | 8 | Runs before the objective. Faces before hands before content before resolution. |
| Crop objective + search | `crop.rs` | 9 | Weighted geometric mean of four terms; bounded lattice; `STRADDLE_COST` at forty. |
| Aspect variants | `variants.rs` | 5 | Not subject to the improvement margin, subject to every safety rule. |
| Planner | `plan.rs` | 7 | Section 8's seven steps in section 8's order. Regions mapped through the lens first. |
| Migration 20 + store | `0020_geometry.sql`, `store.rs` | catalog suite | Two tables, one view, four indexes. `user_edited` checked inside the statement. |
| Service + pass | `api.rs`, `guard.rs`, `fixtures.rs` | 9 | Resumable; the work remaining is a query. |
| Section 10.1 gates | `tests/eval/geometry_eval.rs` | 23 | Seven section 10.1 gates plus determinism, the objective shape, the lens/region mapping and the decision/render parity. |
| Grep-as-a-test | `crates/aura-geometry/tests/no_render_calls.rs` | 2 | No renderer, no recipe write, no face detector, no pose model. |
| Shared optics maths | `crates/aura-raw/src/colour/lens.rs` | 6 | One implementation, reachable by the decision and the renderer. |
| Recipe amendment | `aura_recipe::Lens::coefficients`, golden re-blessed | 63 | ADR-0041 section 4. |
| Render application | `crates/aura-render/src/geometry.rs`, `shaders/geometry.wgsl` | 136 | `caps.geometry_models` true on the reference path; three entry points moved out of `colour.wgsl` and `spatial.wgsl`. |
| IPC + panel | `geometry_commands.rs`, `contract/ipc.rs`, `types.ts`, `GeometryPanel.tsx` | 8 UI, 300 UI total | Six commands; reverting is `set_framing`, not its own command. |
| Phase gate | `crates/aura-cli/src/phase23.rs` | exits 0 | Ten sections, and it prints what it does not prove. |
| Budgets | `crates/aura-perf/tests/geometry_budgets.rs`, `perf/budgets.toml` | 4 | 839 B/image measured against an 890 B budget, after two reductions. |
| Python gate | `ml/eval/crop_agreement.py` | self-test | The expert-crop arithmetic, against an authored answer. |
| Docs | `docs/geometry-and-cropping.md`, two ADRs, six runbooks, CHANGELOG | - | |

## Defects this phase found in its own work

1. **The edge tracker died at every line intersection** and found zero chains on a plate made
   of nothing but straight lines. A crossing is not an ending.
2. **Trimming the worst residuals kept the optical centre and threw away the evidence.** A
   robust fit must reject the chains no coefficient can straighten, not the chains with the
   largest residual.
3. **Re-acquiring after a gap with a one-pixel window flattened the curve at every crossing**,
   biasing the recovered coefficient low by about a sixth - self-consistently, so every chain
   agreed with every other chain about the wrong answer.
4. **The max-area inscribed rectangle changed the frame's shape with the angle.** Levelling a
   3:2 frame by two degrees delivered 1.72:1.
5. **The straddle penalty was an order of magnitude too small**, so the objective preferred
   slicing a bright window in half to leaving it whole.
6. **The forbidden-column scan matched substrings**, so `lens_profile` and `profile_ver` read
   as stored paths.
