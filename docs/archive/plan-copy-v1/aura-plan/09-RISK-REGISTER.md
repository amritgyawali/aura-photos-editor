# Risk Register

Severity: 1 (annoying) to 5 (project-ending). Owner is the role accountable for the mitigation.

| # | Risk | Sev | Owner | Mitigation | Trigger to act |
| --- | --- | --- | --- | --- | --- |
| R1 | Culling rejects a must-have moment | 5 | MLL | Coverage guard with hard rules, zero-missed gate, runner-up retention, QC replacement | Any fixture miss |
| R2 | Retouch produces plastic skin or removes permanent features | 5 | COL/MLL | Texture-retention floor in CI, permanent-feature classifier, conservative defaults | Gate breach or one field report |
| R3 | Generative cleanup damages a delivered photograph | 5 | SEC | Safety engine, size caps, denylists, artefact self-check, review-by-default | Any adversarial audit success |
| R4 | Two-hour autopilot run fails near the end | 4 | TLC | Per-stage checkpoints, resume, stage isolation, degraded completion, nightly long-run CI | Any resume failure |
| R5 | Performance budgets missed on real laptops | 4 | PERF | Tiered compute, hardware plan, adaptive batches, published hardware tiers | 20 % over budget |
| R6 | Face recovery or restoration changes identity | 5 | MLL | Embedding-distance constraint with skip-on-failure, 100 % CI gate | Any drift beyond threshold |
| R7 | Skin-tone bias in detection, retouch or grading | 5 | MLL/COL | Balanced labelling, per-bucket metrics, 10 % parity ship gate | Any bucket gap over 10 % |
| R8 | Cloud API key leak or cost blowout | 4 | SEC | Keychain-only storage, single gateway, hard budget caps, no key in logs or prompts | Any leak or cap breach |
| R9 | Privacy incident with client imagery | 5 | SEC | Local-first, derivative-only cloud data, per-project consent, no imagery in telemetry | Any unconsented transmission |
| R10 | Dataset acquisition stalls, models plateau | 5 | DATA/PM | Paid licensing early, opt-in with revenue share, learning loop, active labelling | Under 30 weddings by beta |
| R11 | Adobe or an incumbent ships the same automation | 4 | CTO/PM | Specialist depth, offline capability, dataset moat, faster iteration | Any credible announcement |
| R12 | Style learning bakes in a photographer's mistakes | 3 | MLL | Residual-on-baseline design, robust fitting, A/B before adoption | Complaint pattern |
| R13 | Gallery consistency flattens intentional mood | 4 | COL/PM | Damping under 1.0, hard bounds, change-point detection, labelled transition fixtures | Audit finding |
| R14 | QC agent creates new problems while fixing old ones | 4 | QAL | Improvement verification, automatic revert, no-regression checks, bounded rounds | Any regression in fixtures |
| R15 | Model artefacts bloat the installer | 2 | DEVOPS | Optional packs, delta updates, quantisation | Installer over 900 MB |
| R16 | Lightroom/Photoshop plugin breakage | 3 | MBE | Version detection, XMP-only degradation, compatibility matrix in CI | Any host update |
| R17 | Scope creep delays V1 indefinitely | 4 | PM/EM | Version milestones, ship V1 at Phase 17 plus 28 plus 30, phase gate discipline | Two phases over estimate |
| R18 | Determinism loss makes support impossible | 3 | TLC | Seeded everything, sorted iteration, golden tests, support bundles | Any non-reproducible report |
| R19 | Photographer trust collapse from a public AI failure | 5 | PM/CTO | Explainability, disclosure, refusal list, conservative autonomy, fast rollback | Any viral complaint |
| R20 | Key-person dependency in colour science | 3 | EM | Document COL methodology, pair reviews, recorded measurement procedures | Single-owner area |

## Standing rules that retire whole classes of risk

1. RAW files are read-only. Data loss risk becomes cache-corruption risk, which is recoverable.
2. Cloud is optional everywhere. Vendor outage becomes a slower run, not a broken product.
3. Every AI stage has a kill switch. A bad model becomes a config change, not an emergency release.
4. Every decision is in the ledger. "We cannot reproduce it" stops being a support outcome.
