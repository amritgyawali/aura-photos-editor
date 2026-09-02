# Phase 29 progress - Curation Intelligence

One line per task, in the order section 9 asks for them.

| Task | Files touched | Tests added | Note |
|---|---|---|---|
| Step 0 - branch | - | - | `claude/phase-29-420bbm`, cut and pushed before any code |
| CTO - ADR | `docs/adr/ADR-0059-curation-selection-and-album-composition.md` | - | Thirteen sections; curation owns no output, the mix is solved against a measured skin band, coverage is a filter, chapter order is inviolable, the cloud can only be agreed with |
| CTO - ADR | `docs/adr/ADR-0060-curate-ipc-surface.md` | - | Eleven commands; no `curate_apply`, no strength field, four shapes that carry more than they look like they need |
| TLC - freeze | `crates/aura-core/src/contract/curate.rs` | 27 contract tests | `BwMix`, `BwPick`, `HeroPick`, `SpreadPair`, `Spread`, `AlbumPlan`, `SocialSets`, `TeaserPick`, `CurationResult`, 39 `CurateCode`s in five groups, `CurationOutline`, `CurateOverride`, `CurateService`; `SpreadId` is the eighteenth typed id |
| SRC - migration | `crates/aura-catalog/migrations/0029_curation.sql` | catalog suite | Eleven tables, two views, five triggers; the near-duplicate ceiling and the caption bound in the schema, and no free-text column |
| PM - policy | `crates/aura-curate/config/curation.toml`, `src/policy.rs` | policy tests | Album sizes, rhythm patterns, hero and monochrome weights, social formats - a written reason per row; five bounds the code owns that a studio may only tighten, and a scan that refuses a key naming a skin target |
| SRC - readings | `src/read.rs` | unit | `Field`, the one way this crate learns anything; every reading an `Option`, and `Facing` measured from phase 06's eye landmarks rather than assumed |
| SRC - monochrome | `src/bw.rs` | unit | Eight bands out of phase 05's histogram, four measured terms, and a solver that spreads the collapsed set against itself rather than away from the mean |
| SRC - heroes | `src/hero.rs`, `src/explain.rs` | unit | An arithmetic blend under a technical veto, three diversity constraints, and the binding one recorded on every pick |
| SRC - album | `src/album.rs`, `src/spread.rs` | unit | Coverage as a filter before any score, largest-remainder chapter quotas, a bounded pairing look-ahead, and an order a photographer set that no pass overwrites |
| SRC - social | `src/social.rs`, `src/teaser.rs` | unit | Three sets by slot with a legibility term; a teaser across six named parts of the day |
| AGT - captions | `src/caption.rs` | unit | A closed vocabulary built from this wedding's own labels; the same grounding check for a template and for a model, and no gendered role word in either |
| AGT - cloud | `src/sequence.rs` | unit | `AlbumSequencing` at the reasoning tier; every move checked against the local objective, and a chapter-crossing move refused rather than nudged |
| SRC - export | `src/export.rs` | unit | Twelve hand-written specifications; the format is published rather than a consequence of Rust field names |
| SRC - store | `src/store.rs`, `src/api.rs` | 12 integration | One transaction per pass; `album_order` kept apart from the spreads, which is what makes a reorder survive a re-curation |
| QAL - fixtures | `src/fixtures.rs` | - | A whole synthetic wedding with no photograph in it; two shapes - `as_shipped` reproduces this build, `complete` exercises what face detection would unlock |
| QAL - grep test | `crates/aura-curate/tests/no_outputs.rs` | 9 | The ninth grep-as-a-test: no recipe write, no file, no socket, no photograph opened, no second similarity index, no skin target, and only one module may name `album_order` |
| QAL - gates | `tests/eval/curate_eval.rs` | 25 | Section 10.1's seven rows, on seven seeds and both fixture shapes; the three that need photographers are reported rather than asserted |
| MLL - training | `ml/models/curate/train_hero.py`, `train_bw.py` | 2 self-tests | Both procedures exercised end to end on synthetic archives; both refuse real input, because there is none |
| QAIQ - study | `ml/models/curate/eval_curate.py` | 1 self-test | The three headline numbers from a real catalog; a project nobody reviewed reports nothing rather than unanimity |
| SFE - IPC | `crates/aura-app/src/curate_commands.rs`, `contract/ipc.rs` | - | Eleven commands and the `Field` port assembled from eight frozen services |
| SFE - state | `crates/aura-app/src/state.rs` | - | The readings gathered once per pass; `band_of_uv` turns phase 15's skin locus into the band the mix must not move |
| SFE - shell | `ui/src-tauri/src/main.rs`, `ui/src/ipc/{client,types}.ts` | - | 240 handlers, 240 registered, 240 client wrappers - asserted by the gate |
| SFE/MFE - panels | `ui/src/components/curate/` - six components | 21 vitest | Hero grid, monochrome picks, spread view, social sets, album builder, and the container; mounted in `App.tsx` |
| PERF - budgets | `crates/aura-perf/tests/curate_budgets.rs`, `perf/budgets.toml` | 5 | All three of section 11's rows met, none waived; the store's shape asserted as well as its size |
| CTO - gate | `crates/aura-cli/src/phase29.rs`, `main.rs`, `justfile` | - | Seven sections plus the IPC parity count; exits 0, and prints the five conditions on every run |
| DOC - docs | `docs/curation.md` | - | What it proposes, what it will never do, and the three things it cannot judge on this build |
| EM - registry | `crates/aura-core/errors.toml`, `docs/runbooks/AURA-ML-514{2..5}.md` | registry test | Four codes, four runbooks |
| EM - lock | `xtask/src/main.rs`, `contracts.lock` | contract check | Migration 29 added to `EXTRA_CONTRACTS`; 78 entries locked |

## Benchmark deltas

| Metric | Budget | Measured |
|---|---|---|
| Full curation, 600 frames | 12,000 ms at this size | 446 ms |
| Album re-composition after a drag | 1,500 ms | 375 ms |
| Monochrome mix, per image | 25 ms | 0.01 ms |
| Store, per selected image (600-frame gallery) | 4,000 B | 211 B |
| Store, per selected image (smallest gallery) | 4,000 B | 2,439 B |

**All three of section 11's rows are met and none is waived** — the first phase since 20 where that
is true. The reason is structural rather than an achievement: nothing in this phase opens a
photograph, so a whole wedding's curation is arithmetic over rows phases 05 to 28 already wrote.
`crates/aura-curate/tests/no_outputs.rs` is what keeps it that way.

The storage figure **falls** as a wedding grows, which is the opposite of every migration from 09
to 20, because the album, the portfolio and the captions are capped by the contract and the gallery
is not. The budget is therefore set at the small end where the figure is worst, and the bound is
asserted as well as the number: doubling the gallery multiplies the store by 1.62x rather than 2x,
and the difference is entirely `curate_bw`.

## What did not happen

Section 10.1's first three rows — hero agreement at 0.75, an album photographers reorder by under
15 %, monochrome picks accepted at 70 % — are **studies**. They need sixty consented weddings and a
room of photographers, and neither exists in this repository. `ml/models/curate/eval_curate.py` is
what runs them the day one does; until then they are condition C4 of the exit report rather than
numbers.

Three attempts were made to find an arithmetic proxy for the second and third of those, and all
three were wrong. The failed attempts are recorded in the exit report's section 8 rather than
deleted, because each of them looked reasonable and each would have shipped a green gate that
measured nothing.
