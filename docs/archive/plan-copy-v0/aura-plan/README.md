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
