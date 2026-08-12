# AURA Wedding AI

> Shoot the wedding. Import the RAWs. Click once. Deliver.

The autonomous AI post-production system for wedding photographers.
Import 1,000-4,000 RAW files, press one button, and receive a culled, edited, retouched,
quality-checked, consistently graded, export-ready wedding gallery.

## What this bundle is

A complete, executable plan: strategy, architecture, data model, model stack, QA strategy,
coding standards, dataset plan, business plan, risk register, and **30 phase documents of roughly
three pages each**, every one written so a coding agent can implement it without guessing.

## Read in this order

| File | Purpose |
| --- | --- |
| `CLAUDE.md` | **Start here if you are a coding agent.** Operating rules, invariants, phase ritual. |
| `00-PRODUCT-STRATEGY.md` | What we are building, for whom, why it wins, what we refuse to build. |
| `01-ARCHITECTURE.md` | Process model, crate layout, pipeline, threading, IPC, hardware strategy. |
| `02-AI-MODEL-STACK.md` | Every model, its job, size, runtime, gate, and the cloud reasoning policy. |
| `03-DATA-MODEL.md` | SQLite schema, cache layout, recipe JSON, ledger, migrations. |
| `04-ROADMAP-30-PHASES.md` | The 30 phases, epics and version milestones. |
| `AGENT-TEAM.md` | The 23-role virtual IT company and how to role-play it. |
| `05-QA-STRATEGY.md` | Test pyramid, golden images, perceptual gates, chaos tests, release gates. |
| `06-CODING-STANDARDS.md` | Rust/TypeScript/Python standards, error handling, review checklist. |
| `07-DATASET-AND-TRAINING.md` | The Wedding Intelligence Dataset - the real moat. |
| `08-BUSINESS-GTM.md` | Pricing, positioning, launch, unit economics. |
| `09-RISK-REGISTER.md` | What can kill this project and what we do about it. |
| `10-GLOSSARY.md` | Shared vocabulary. Use these exact terms in code. |
| `11-ZERO-BUDGET-BUILD.md` | How to build and sell this for ~USD 0, and exactly when to start spending. |
| `phases/PHASE-01..30-*.md` | The build itself. One feature per phase, fully specified. |
| `WEDDING-AI-MASTER-PLAN.md` | Everything concatenated into one file. |

## Technology stack (decided)

| Layer | Choice | Why |
| --- | --- | --- |
| Desktop shell | Tauri 2 + React 18 + TypeScript + Vite | Native performance, small binary, Rust core in the same process tree. |
| Core engine | Rust (2021, stable) | Memory safety with C-like speed; excellent concurrency for 4,000-file pipelines. |
| RAW decode | LibRaw (via Rust FFI) | Broadest camera support (CR2/CR3, NEF, ARW, RAF, DNG, ORF). |
| GPU render | wgpu (Vulkan/Metal/DX12) + WGSL | One shader codebase for Windows and macOS. |
| Inference | ONNX Runtime (TensorRT, CUDA, DirectML, CoreML, CPU) | One model artefact, every hardware target. |
| Classical CV | OpenCV (narrow, isolated usage) | Proven primitives where a model is unnecessary. |
| Catalog | SQLite (WAL) + JSON sidecars + XMP | Zero-admin, transactional, portable, interoperable. |
| Training | Python 3.11 + PyTorch 2 | Standard research stack; exports to ONNX. |
| Cloud reasoning | Bring-your-own API key, governed gateway | Your key, your budget, your data, with hard caps and offline fallback. |

## Non-negotiable promises

1. RAW files are never modified. Every edit is a recipe.
2. Client imagery never leaves the machine without explicit per-project consent.
3. Every automated decision is explainable, confidence-scored and reversible.
4. The product works fully offline. Cloud AI is an accelerator, never a dependency.
5. Nothing the product does may change what a person looks like.


---

## Governance and agent instructions

The documents below are the operating law of this project and the standing instructions for every
team member. They are binding on humans and on AI coding agents equally.

| Document | What it is | When to read it |
|---|---|---|
| [12-ENGINEERING-CONSTITUTION.md](./12-ENGINEERING-CONSTITUTION.md) | The Ten Laws plus 24 articles of binding engineering rules, and the enforcement list of what blocks a merge and what blocks a release. | Before writing a single line of code, and again before every release. |
| [agents/AGENT-INDEX.md](./agents/AGENT-INDEX.md) | The 23-role roster, discipline groupings and every standing veto. | To find your brief, or the brief of whoever you hand work to. |

### Reading order for an AI coding agent

1. `CLAUDE.md`
2. `12-ENGINEERING-CONSTITUTION.md`
3. Your own agent brief from the table below
4. The single phase file you are implementing, and nothing else

Never load two phase files into one session. Context degrades, invariants get forgotten, and the
resulting change becomes impossible to review.

### The 23 agent briefs

| Code | Role | Brief |
|---|---|---|
| `CTO` | Chief Architect / CTO Agent | [AGENT-CTO-CHIEF-ARCHITECT.md](./agents/AGENT-CTO-CHIEF-ARCHITECT.md) |
| `PM` | Product Manager Agent | [AGENT-PM-PRODUCT-MANAGER.md](./agents/AGENT-PM-PRODUCT-MANAGER.md) |
| `EM` | Engineering Manager / Delivery Lead Agent | [AGENT-EM-ENGINEERING-MANAGER.md](./agents/AGENT-EM-ENGINEERING-MANAGER.md) |
| `TLC` | Tech Lead - Imaging Core (Rust) | [AGENT-TLC-TECH-LEAD-IMAGING-CORE.md](./agents/AGENT-TLC-TECH-LEAD-IMAGING-CORE.md) |
| `SRC` | Senior Engineer - Core Pipeline (Rust) | [AGENT-SRC-SENIOR-CORE-PIPELINE.md](./agents/AGENT-SRC-SENIOR-CORE-PIPELINE.md) |
| `SRG` | Senior Engineer - GPU & Render (Rust / wgpu / CUDA) | [AGENT-SRG-SENIOR-GPU-RENDER.md](./agents/AGENT-SRG-SENIOR-GPU-RENDER.md) |
| `MLL` | ML Lead - Vision | [AGENT-MLL-ML-LEAD-VISION.md](./agents/AGENT-MLL-ML-LEAD-VISION.md) |
| `SRML` | Senior ML Engineer | [AGENT-SRML-SENIOR-ML-ENGINEER.md](./agents/AGENT-SRML-SENIOR-ML-ENGINEER.md) |
| `MLR` | ML Research Engineer | [AGENT-MLR-ML-RESEARCH-ENGINEER.md](./agents/AGENT-MLR-ML-RESEARCH-ENGINEER.md) |
| `MLOPS` | MLOps / Model Packaging Engineer | [AGENT-MLOPS-MLOPS-MODEL-PACKAGING.md](./agents/AGENT-MLOPS-MLOPS-MODEL-PACKAGING.md) |
| `AGT` | AI Agent & Prompt Engineer | [AGENT-AGT-AI-AGENT-PROMPT-ENGINEER.md](./agents/AGENT-AGT-AI-AGENT-PROMPT-ENGINEER.md) |
| `COL` | Colour Scientist | [AGENT-COL-COLOUR-SCIENTIST.md](./agents/AGENT-COL-COLOUR-SCIENTIST.md) |
| `SFE` | Senior Frontend Engineer (Tauri + React) | [AGENT-SFE-SENIOR-FRONTEND-ENGINEER.md](./agents/AGENT-SFE-SENIOR-FRONTEND-ENGINEER.md) |
| `MFE` | Mid-Level Frontend Engineer | [AGENT-MFE-MID-LEVEL-FRONTEND-ENGINEER.md](./agents/AGENT-MFE-MID-LEVEL-FRONTEND-ENGINEER.md) |
| `MBE` | Mid-Level Backend / Cloud Engineer | [AGENT-MBE-MID-LEVEL-BACKEND-CLOUD.md](./agents/AGENT-MBE-MID-LEVEL-BACKEND-CLOUD.md) |
| `DATA` | Data Engineer / Dataset Curator | [AGENT-DATA-DATA-ENGINEER-DATASET-CURATOR.md](./agents/AGENT-DATA-DATA-ENGINEER-DATASET-CURATOR.md) |
| `QAL` | QA Lead - Automation | [AGENT-QAL-QA-LEAD-AUTOMATION.md](./agents/AGENT-QAL-QA-LEAD-AUTOMATION.md) |
| `QAIQ` | QA Engineer - Image Quality (perceptual) | [AGENT-QAIQ-QA-ENGINEER-IMAGE-QUALITY.md](./agents/AGENT-QAIQ-QA-ENGINEER-IMAGE-QUALITY.md) |
| `PERF` | Performance Engineer | [AGENT-PERF-PERFORMANCE-ENGINEER.md](./agents/AGENT-PERF-PERFORMANCE-ENGINEER.md) |
| `DEVOPS` | DevOps / Release Engineer | [AGENT-DEVOPS-DEVOPS-RELEASE-ENGINEER.md](./agents/AGENT-DEVOPS-DEVOPS-RELEASE-ENGINEER.md) |
| `SEC` | Security & Privacy Engineer | [AGENT-SEC-SECURITY-PRIVACY-ENGINEER.md](./agents/AGENT-SEC-SECURITY-PRIVACY-ENGINEER.md) |
| `UX` | UX / UI Designer | [AGENT-UX-UX-UI-DESIGNER.md](./agents/AGENT-UX-UX-UI-DESIGNER.md) |
| `DOC` | Technical Writer | [AGENT-DOC-TECHNICAL-WRITER.md](./agents/AGENT-DOC-TECHNICAL-WRITER.md) |
