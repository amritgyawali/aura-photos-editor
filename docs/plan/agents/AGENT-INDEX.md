# Agent Instruction Index

**AURA - AURA Wedding AI**

Twenty-three agent roles. Each brief is a complete standing instruction set: ownership map, phase
assignments, non-negotiable rules, standard operating procedure, interfaces and handoffs, quality
gates owned, definition of done, anti-patterns, decision rights, toolbox, first week, and how the
role is measured.

## How to use these briefs

1. Read `12-ENGINEERING-CONSTITUTION.md` first. It is binding on every role and is not repeated in
   full inside each brief.
2. Read your own brief completely before writing any code.
3. Read the brief of any role you hand work to, so your output matches their acceptance criteria.
4. Read the phase file for the phase you are working on, then the contracts it defines.

When driving an AI coding agent, load in this order: `CLAUDE.md`, then the constitution, then the
agent brief, then the single phase file. Never load two phases at once.

## The roster

| Code | Role | Reports to | Holds a veto | Brief | Pages |
|---|---|---|---|---|---|
| `CTO` | Chief Architect / CTO Agent | Founder | Yes | [AGENT-CTO-CHIEF-ARCHITECT.md](./AGENT-CTO-CHIEF-ARCHITECT.md) | 6.4 |
| `PM` | Product Manager Agent | Founder | Yes | [AGENT-PM-PRODUCT-MANAGER.md](./AGENT-PM-PRODUCT-MANAGER.md) | 6.3 |
| `EM` | Engineering Manager / Delivery Lead Agent | Founder | Yes | [AGENT-EM-ENGINEERING-MANAGER.md](./AGENT-EM-ENGINEERING-MANAGER.md) | 6.2 |
| `TLC` | Tech Lead - Imaging Core (Rust) | `CTO` | Yes | [AGENT-TLC-TECH-LEAD-IMAGING-CORE.md](./AGENT-TLC-TECH-LEAD-IMAGING-CORE.md) | 6.1 |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | `TLC` | Yes | [AGENT-SRC-SENIOR-CORE-PIPELINE.md](./AGENT-SRC-SENIOR-CORE-PIPELINE.md) | 5.8 |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | `TLC` | Yes | [AGENT-SRG-SENIOR-GPU-RENDER.md](./AGENT-SRG-SENIOR-GPU-RENDER.md) | 5.9 |
| `MLL` | ML Lead - Vision | `CTO` | Yes | [AGENT-MLL-ML-LEAD-VISION.md](./AGENT-MLL-ML-LEAD-VISION.md) | 6.3 |
| `SRML` | Senior ML Engineer | `MLL` | No | [AGENT-SRML-SENIOR-ML-ENGINEER.md](./AGENT-SRML-SENIOR-ML-ENGINEER.md) | 5.7 |
| `MLR` | ML Research Engineer | `MLL` | No | [AGENT-MLR-ML-RESEARCH-ENGINEER.md](./AGENT-MLR-ML-RESEARCH-ENGINEER.md) | 5.7 |
| `MLOPS` | MLOps / Model Packaging Engineer | `MLL` | Yes | [AGENT-MLOPS-MLOPS-MODEL-PACKAGING.md](./AGENT-MLOPS-MLOPS-MODEL-PACKAGING.md) | 5.8 |
| `AGT` | AI Agent & Prompt Engineer | `MLL` | Yes | [AGENT-AGT-AI-AGENT-PROMPT-ENGINEER.md](./AGENT-AGT-AI-AGENT-PROMPT-ENGINEER.md) | 6.0 |
| `COL` | Colour Scientist | `CTO` | Yes | [AGENT-COL-COLOUR-SCIENTIST.md](./AGENT-COL-COLOUR-SCIENTIST.md) | 5.9 |
| `SFE` | Senior Frontend Engineer (Tauri + React) | `EM` | Yes | [AGENT-SFE-SENIOR-FRONTEND-ENGINEER.md](./AGENT-SFE-SENIOR-FRONTEND-ENGINEER.md) | 5.8 |
| `MFE` | Mid-Level Frontend Engineer | `SFE` | No | [AGENT-MFE-MID-LEVEL-FRONTEND-ENGINEER.md](./AGENT-MFE-MID-LEVEL-FRONTEND-ENGINEER.md) | 5.5 |
| `MBE` | Mid-Level Backend / Cloud Engineer | `EM` | No | [AGENT-MBE-MID-LEVEL-BACKEND-CLOUD.md](./AGENT-MBE-MID-LEVEL-BACKEND-CLOUD.md) | 5.6 |
| `DATA` | Data Engineer / Dataset Curator | `MLL` | Yes | [AGENT-DATA-DATA-ENGINEER-DATASET-CURATOR.md](./AGENT-DATA-DATA-ENGINEER-DATASET-CURATOR.md) | 5.7 |
| `QAL` | QA Lead - Automation | `EM` | Yes | [AGENT-QAL-QA-LEAD-AUTOMATION.md](./AGENT-QAL-QA-LEAD-AUTOMATION.md) | 5.7 |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | `QAL` | Yes | [AGENT-QAIQ-QA-ENGINEER-IMAGE-QUALITY.md](./AGENT-QAIQ-QA-ENGINEER-IMAGE-QUALITY.md) | 5.8 |
| `PERF` | Performance Engineer | `CTO` | Yes | [AGENT-PERF-PERFORMANCE-ENGINEER.md](./AGENT-PERF-PERFORMANCE-ENGINEER.md) | 5.7 |
| `DEVOPS` | DevOps / Release Engineer | `EM` | Yes | [AGENT-DEVOPS-DEVOPS-RELEASE-ENGINEER.md](./AGENT-DEVOPS-DEVOPS-RELEASE-ENGINEER.md) | 5.7 |
| `SEC` | Security & Privacy Engineer | `CTO` | Yes | [AGENT-SEC-SECURITY-PRIVACY-ENGINEER.md](./AGENT-SEC-SECURITY-PRIVACY-ENGINEER.md) | 5.8 |
| `UX` | UX / UI Designer | `PM` | Yes | [AGENT-UX-UX-UI-DESIGNER.md](./AGENT-UX-UX-UI-DESIGNER.md) | 5.8 |
| `DOC` | Technical Writer | `PM` | Yes | [AGENT-DOC-TECHNICAL-WRITER.md](./AGENT-DOC-TECHNICAL-WRITER.md) | 5.7 |

## Discipline groupings

- **Leadership and delivery:** `CTO`, `PM`, `EM`
- **Imaging core, Rust:** `TLC`, `SRC`, `SRG`
- **Machine learning:** `MLL`, `SRML`, `MLR`, `MLOPS`, `AGT`
- **Colour and perception:** `COL`, `QAIQ`
- **Interface and cloud:** `SFE`, `MFE`, `MBE`, `UX`
- **Data, quality and platform:** `DATA`, `QAL`, `PERF`, `DEVOPS`, `SEC`, `DOC`

## Standing vetoes

A veto is technical, written, and cannot be overruled by schedule pressure. It is satisfied, not
negotiated away.

- `COL` on skin and colour rendering, and on render node order.
- `SEC` on secrets, network egress, licence compatibility and client data.
- `QAIQ` on perceptual regressions: plastic skin, halos, banding, face and hand artefacts.
- `QAL` on missing or flaky tests, and on an incomplete release verification matrix.
- `PERF` on unjustified performance-budget regressions.
- `DEVOPS` on unsigned artefacts and untested rollback.
- `DATA` on unlicensed data and on wedding-level split leakage.
- `TLC` on non-cancellable, non-resumable or corrupting changes.
- `MLL` and `MLOPS` on models without a card, calibration, fallback or signature.
- `CTO` on dependency cycles, the offline guarantee, unversioned formats and destructive edits.
- `PM` on overclaiming and on automation without an override.
- `UX` on automated decisions presented without a reason and a reversible override.
- `DOC` on shipping undocumented user-facing behaviour.

## Total

23 briefs, 74109 words, 134.7 pages at 550 words per page, plus a 12.3 page constitution.
