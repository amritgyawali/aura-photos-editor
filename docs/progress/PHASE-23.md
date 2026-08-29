# Phase 23 progress - Geometry Suite: lens corrections, straightening AI and smart crop

One line per task group, in the order section 8 asks for them. Files touched, tests added,
benchmark delta.

## T1 - COL/SRC: the lens profile database and the corrections (section 8 step 1)

Files: `assets/lens_profiles/{profiles.toml,ATTRIBUTION.md}`, `crates/aura-geometry/src/profiles.rs`,
`crates/aura-geometry/src/lens.rs`, `crates/aura-render/src/geometry.rs`,
`crates/aura-render/shaders/geometry.wgsl`.
Fourteen profiles, resolved by lens id and focal length, behind a three-step chain: the file's own
embedded correction data first, then the database, then nothing. Distortion, vignette and lateral
chromatic aberration are applied in **linear light** before any creative operation, which is what
section 6.1 asks for and what phase 14's invariant 8 already guarantees for everything before the
output transform. **Every profile is a reference model for a class or family**, `lens_measured` is
0 on every row this build writes, and `ATTRIBUTION.md` says so. Condition C3.
Tests: profile resolution, the correction ordering, and
`the_shipped_lens_table_is_reachable_and_says_it_was_not_measured` in `boundaries.rs`.

## T2 - COL: the manual-lens distortion estimator (section 8 step 2)

Files: `crates/aura-geometry/src/lens.rs`.
When neither embedded data nor a profile exists, distortion is estimated from the frame's own
straight edges. **Distortion and nothing else**: a chromatic aberration or a vignette derived from
a single photograph would be a correction nobody measured, and migration 23 carries a CHECK that
refuses such a row rather than trusting the code not to write one.
Tests: the estimator on a painted barrel plate, and
`a_distortion_correction_never_leaves_an_undefined_pixel`.

## T3 - SRC: rotation with confidence gating and the crop it costs (section 8 step 3)

Files: `crates/aura-geometry/src/straighten.rs`, `crates/aura-core/src/contract/geometry.rs`
(`rotation_crop`).
The band is 0.2° to 8° above 0.70 horizon confidence, and its two ends mean different things:
below, the frame is already level; above, the tilt was a decision and is **left alone rather than
clamped**. The cost is paid before anything else - the angle is reduced until the rectangle it
implies cuts nobody and stays above the resolution floor, and abandoned if no angle works, with
both numbers stored. `rotation_crop` lives in the contract because the solver and the renderer must
agree about which pixels exist.
Tests: five straightening rows in `geometry_eval.rs`, plus
`the_rotation_crop_the_contract_computes_is_the_one_the_renderer_can_fill`.

## T4 - SRC: keystone with a measured stretch cap (section 8 step 4)

Files: `crates/aura-geometry/src/keystone.rs`.
Converging verticals are measured from a Sobel field, requiring a minimum vertical share of the
frame's structure before anything is attempted. `Keystone::stretch` is the **measured** ratio
between the two axis scales rather than a function of the sliders, because the cap is a statement
about what the correction did. Above 1.12 the correction is refused rather than clamped, and it is
skipped entirely when the crop it costs would break a safety rule.
Tests: `the_keystone_never_exceeds_the_cap_and_is_skipped_when_it_would_cut`, and
`a_frame_of_people_is_never_read_as_architecture`.

## T5 - MLL/SRC: crop candidates and the composition objective (section 8 step 5)

Files: `crates/aura-geometry/src/crop.rs`, `crates/aura-geometry/config/crop_rules.toml`.
Four terms - placement against the scene's own targets, balance about the rectangle's centre, edge
cleanliness, headroom - fused as a **geometric mean**, so no term can rescue another. The search is
bounded and runs inside whatever the rotation and the keystone left. 23 scene rows carry the
placement, the headroom target and the margin, with a written reason per row; **ten of them switch
automatic cropping off entirely**, which is the mitigation for hands never being protected on this
build (C4).
The objective is **authored rather than fitted**: section 9's DATA row asks for expert crops on
2,000 frames and this repository has none. Condition C2.
Tests: the objective's monotonicity, the margin, and
`a_crop_that_is_only_slightly_better_does_not_replace_what_was_shot`.

## T6 - SRC/QAL: the safety filter and the improvement margin (section 8 step 6)

Files: `crates/aura-geometry/src/safety.rs`, `crates/aura-geometry/src/decide.rs`,
`crates/aura-catalog/migrations/0023_geometry.sql`.
The filter runs **before** the score and returns a boolean; the objective has no term for protected
content, so there is no weight anybody could tune to trade a face against a better composition.
Above it sits the per-scene improvement margin, which the config file may only raise. The delivered
rectangle is checked twice - once by the filter and once by two database triggers that abort any
statement leaving `primary_crop` addressing an unsafe row.
The safety report stores its denominator: `considered` beside `at_risk`, because zero cut over zero
checked is arithmetic. **On this build it is always zero** - phase 06's detector is a placeholder,
which also means the crop search does not run at all on a real photograph. Condition C1.
Tests: `no_delivered_crop_cuts_a_protected_region`,
`a_crop_is_refused_rather_than_scored_when_it_would_cut_somebody`,
`no_delivered_crop_falls_below_the_resolution_floor`, and
`the_floor_is_on_the_long_edge_rather_than_on_the_area`.

## T7 - SRC: aspect variants and the store (section 8 step 7)

Files: `crates/aura-geometry/src/variants.rs`, `crates/aura-geometry/src/store.rs`,
`crates/aura-catalog/migrations/0023_geometry.sql`.
Four aspects generated per frame beside the delivered rectangle, bounded at five variants total by
the contract and again by a CHECK. A variant that could not be made safely is **stored with the
code that refused it** rather than dropped, because "why is there no square crop of this
photograph" is a question the panel has to answer.
The write order is variants first, plan second - the insert trigger reads the variants - which is
why `geometry_crop.photo_id` is `DEFERRABLE INITIALLY DEFERRED`, the only deferred foreign key in
the product. The first gate run found the contradiction the hard way: zero plans written and
`FOREIGN KEY constraint failed` twenty-four times. ADR-0047 section 9.
Storage measured at 1,088 B/image against a 1,400 B budget.
Tests: the store round trip, both triggers, the user-edited carry-forward, and
`crates/aura-perf/tests/geometry_budgets.rs`.

## T8 - SFE: the Framing panel and the IPC surface (section 8 step 8)

Files: `crates/aura-app/src/geometry_commands.rs`, `crates/aura-app/src/contract/ipc.rs`,
`crates/aura-app/src/state.rs`, `ui/src/ipc/{types,client}.ts`,
`ui/src/components/develop/GeometryPanel.tsx`, `ui/src-tauri/src/main.rs`.
Nine commands (ADR-0048). The **revert is its own command and its own button**, rendered on every
plan whether or not anything was changed, because "original framing is always one click away" is an
acceptance criterion and a flag inside a seven-field payload is a click every caller has to
assemble correctly. The panel shows what was left alone as prominently as what was done, draws what
a crop was not allowed to cut, names a reference lens profile as a reference profile, and prints
the safety count **with its denominator** - in words when the denominator is zero.
Phase 22's seven restoration commands were registered in the shell at the same time; they had never
been wired in, so that half of phase 22's C5 is closed. 92 IPC commands are now registered in
`ui/src-tauri/src/main.rs`. The shell's own Rust cannot be compiled on this machine - `dlltool` is
absent - so the registration is verified by a symbol cross-check: every `aura_app::` call resolves
to an exported function, every imported IPC type is defined, and every registered handler name is
declared. Condition C7.
Tests: 10 in `GeometryPanel.test.tsx`, including that an unsafe variant cannot be delivered and
that a zero over no denominator is reported as meaningless.

## T9 - QAL/QAIQ: validation and the safety audit (section 8 step 9)

Files: `tests/eval/geometry_eval.rs` (18 tests), `crates/aura-geometry/tests/boundaries.rs`
(6 checks), `crates/aura-cli/src/phase23.rs`, `justfile`.
Section 10.1's rows as executable gates over architecture, portrait, group and detail fixtures
whose answers are painted in. The boundaries test is the sixth grep-as-a-test in the repository and
checks five architectural properties plus the lens table: no recipe write, no socket, no upscale or
fill, no face detector of its own, and no model.
**The 300-crop perceptual audit did not happen** - section 9 gives QAIQ three days for it and there
are no real photographs here - and `the_two_rows_this_harness_cannot_measure` prints that on every
run rather than leaving the suite silent about it. Condition C2.

## T10 - not in section 8: the two defects the gate found

Both are in `docs/progress/PHASE-23-EXIT.md` section 6 and ADR-0047 section 9, and both are the
kind that ship silently: an ordering constraint contradicting a referential one, and a refusal
raised with a run-blocking code whose runbook tells a photographer their installation is broken.
