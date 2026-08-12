# Roadmap: 30 Phases

Each phase delivers exactly one powerful feature, end to end: schema, contracts, algorithms, UI, tests,
performance budgets, telemetry and acceptance criteria. Phases are sequential by dependency, not by convenience.

| # | Phase | Epic | Duration | Primary owners | Risk |
| --- | --- | --- | --- | --- | --- |
| 01 | [Project Foundation, Catalog & Wedding Project Ingest](phases/PHASE-01-FOUNDATION-CATALOG-INGEST.md) | E1 - Foundation | 2 weeks | CTO, TLC, SRC, SFE, DEVOPS | Medium - foundational mistakes are expensive later |
| 02 | [RAW Decode Engine & Three-Tier Preview Pyramid](phases/PHASE-02-RAW-DECODE-PREVIEW-PYRAMID.md) | E1 - Foundation | 2.5 weeks | TLC, SRC, COL, PERF | High - this is the throughput backbone of the product |
| 03 | [Inference Runtime Layer & Signed Model Package Manager](phases/PHASE-03-INFERENCE-RUNTIME-MODEL-REGISTRY.md) | E1 - Foundation | 2 weeks | MLL, MLOPS, SRG, SEC | High - hardware diversity is where desktop AI products die |
| 04 | [Cloud AI Gateway & Agentic Reasoning Runtime (bring-your-own key)](phases/PHASE-04-CLOUD-AI-GATEWAY.md) | E1 - Foundation | 2 weeks | AGT, CTO, SEC, MBE | High - cost, privacy and non-determinism all live here |
| 05 | [Perceptual Embeddings & Wedding Similarity Index](phases/PHASE-05-EMBEDDINGS-SIMILARITY-INDEX.md) | E2 - Wedding Brain | 1.5 weeks | MLL, SRML, SRC | Medium |
| 06 | [Face Detection, Recognition & People Intelligence](phases/PHASE-06-PEOPLE-INTELLIGENCE.md) | E2 - Wedding Brain | 2.5 weeks | MLL, SRML, SRC, SEC | High - accuracy and privacy both matter |
| 07 | [Wedding Scene AI & Story Timeline Segmentation](phases/PHASE-07-WEDDING-SCENE-STORY-AI.md) | E2 - Wedding Brain | 2.5 weeks | MLL, SRML, AGT, SRC | High - this is the core differentiator |
| 08 | [Smart Burst Grouping & Duplicate Detection](phases/PHASE-08-BURST-GROUPING-DUPLICATES.md) | E2 - Wedding Brain | 1.5 weeks | MLL, SRC, MLR | Medium |
| 09 | [Frame Integrity AI: Focus, Motion, Exposure, Noise & Eye State](phases/PHASE-09-FRAME-INTEGRITY-AI.md) | E2 - Wedding Brain | 2.5 weeks | MLL, SRML, COL, SRC | High - false rejections destroy trust instantly |
| 10 | [Expression, Emotion & Moment Ranking AI](phases/PHASE-10-EMOTION-MOMENT-AI.md) | E2 - Wedding Brain | 2.5 weeks | MLL, SRML, AGT, MLR | High - subjective and culturally sensitive |
| 11 | [Composition & Aesthetic AI](phases/PHASE-11-COMPOSITION-AESTHETIC-AI.md) | E2 - Wedding Brain | 2 weeks | MLL, SRML, MLR | Medium-High - aesthetics are subjective |
| 12 | [Autonomous Culling Engine, Story Coverage Guard & Gallery Sizing](phases/PHASE-12-CULLING-ENGINE-COVERAGE.md) | E2 - Wedding Brain | 3 weeks | MLL, SRC, PM, MLR | Critical - this is the product |
| 13 | [Explain My Edit, Confidence Calibration & Decision Ledger](phases/PHASE-13-EXPLAINABILITY-CONFIDENCE-LEDGER.md) | E2 - Wedding Brain | 2 weeks | MLL, AGT, SFE, SRC | Medium - but critical for adoption |
| 14 | [Non-Destructive Edit Recipe & GPU Develop Engine](phases/PHASE-14-DEVELOP-ENGINE-EDIT-RECIPE.md) | E3 - Photo Brain | 3 weeks | COL, SRG, TLC, PERF | High - correctness here is load-bearing for everything visual |
| 15 | [Exposure AI & White Balance AI (mixed lighting mastery)](phases/PHASE-15-EXPOSURE-WHITE-BALANCE-AI.md) | E3 - Photo Brain | 2.5 weeks | COL, MLL, SRML | High - the most visible AI decision in the product |
| 16 | [Tone AI, Adaptive Curves, HSL AI & Skin-Tone Protection](phases/PHASE-16-TONE-CURVES-COLOUR-AI.md) | E3 - Photo Brain | 2 weeks | COL, MLL, SRML | Medium-High |
| 17 | [Style Learning: Scene-Conditional Personal AI Profiles ("Teach My AI")](phases/PHASE-17-STYLE-LEARNING-PERSONAL-AI.md) | E3 - Photo Brain | 3 weeks | MLL, SRML, MLOPS, COL | High - the strongest retention feature in the product |
| 18 | [Local Mask AI: Automatic Semantic Masking](phases/PHASE-18-LOCAL-MASK-AI.md) | E3 - Photo Brain | 2.5 weeks | MLL, SRML, SRG | High - mask quality is visible in every retouch |
| 19 | [Local Light Sculpting: Face Lighting, Subject Enhancement, Background Balancing & Dodge/Burn AI](phases/PHASE-19-LOCAL-LIGHT-SCULPTING.md) | E3 - Photo Brain | 2 weeks | COL, MLL, SRG | Medium-High - subtlety is the whole point |
| 20 | [Portrait Retouch AI with Natural Texture Protection](phases/PHASE-20-PORTRAIT-RETOUCH-AI.md) | E4 - Retouch & Restoration | 3 weeks | MLL, SRML, COL, SRG | High - the most scrutinised output in the product |
| 21 | [Micro-Retouch Suite: Hair, Teeth, Eyes, Clothing & Glare](phases/PHASE-21-MICRO-RETOUCH-SUITE.md) | E4 - Retouch & Restoration | 2.5 weeks | MLL, SRML, SRG, COL | Medium-High - subtlety and 'uncanny' risk |
| 22 | [Restoration Stack: Scene-Aware Denoise, Selective Sharpen & Face Recovery](phases/PHASE-22-RESTORATION-STACK.md) | E4 - Retouch & Restoration | 3 weeks | COL, MLL, SRML, PERF | High - heavy compute and easy to overdo |
| 23 | [Geometry Suite: Lens Corrections, Straightening AI & Smart Crop](phases/PHASE-23-GEOMETRY-SUITE.md) | E4 - Retouch & Restoration | 1.5 weeks | COL, SRC, MLL | Medium |
| 24 | [Generative Cleanup & Distraction Removal (safe by construction)](phases/PHASE-24-GENERATIVE-CLEANUP.md) | E4 - Retouch & Restoration | 3 weeks | MLL, SRML, SRG, SEC | High - generative output is the easiest way to destroy trust |
| 25 | [Gallery Intelligence Engine: Cross-Photo Colour, Skin & Scene Consistency](phases/PHASE-25-GALLERY-INTELLIGENCE-ENGINE.md) | E5 - Gallery Brain & Autonomy | 3 weeks | COL, MLL, SRC, TLC | High - the marquee differentiator |
| 26 | [Multi-Camera & Second-Shooter Matching](phases/PHASE-26-MULTI-CAMERA-SHOOTER-MATCHING.md) | E5 - Gallery Brain & Autonomy | 2 weeks | COL, MLL, SRC | Medium-High |
| 27 | [AI Quality-Control Agent & Automatic Re-Edit Loop](phases/PHASE-27-AI-QC-AGENT.md) | E5 - Gallery Brain & Autonomy | 3 weeks | AGT, MLL, QAL, SRC | High - it is the last line of defence |
| 28 | [Zero-Touch Wedding Autopilot Orchestrator](phases/PHASE-28-ZERO-TOUCH-AUTOPILOT.md) | E5 - Gallery Brain & Autonomy | 3 weeks | TLC, EM, PERF, SRC | Critical - it is the product's headline |
| 29 | [Curation Intelligence: B&W Selection, Hero Photos, Album Story & Social Picks](phases/PHASE-29-CURATION-INTELLIGENCE.md) | E6 - Curation & Delivery | 2.5 weeks | MLL, AGT, PM, SRC | Medium |
| 30 | [Delivery, Integrations, Learning Loop & Release Engineering](phases/PHASE-30-DELIVERY-INTEGRATIONS-LEARNING-LOOP.md) | E6 - Curation & Delivery | 4 weeks | DEVOPS, TLC, MLOPS, PM, SEC | High - launch quality and data governance |

## Epics

| Epic | Phases | Outcome |
| --- | --- | --- |
| E1 Foundation | 01-04 | A desktop app that ingests 4,000 RAWs fast, renders previews on the GPU, runs local models on any hardware, and can call a cloud model safely. |
| E2 Wedding Brain | 05-13 | The app understands the wedding: people, scenes, story, moments, technical quality, and culls autonomously with explanations. |
| E3 Photo Brain / Develop | 14-19 | A non-destructive develop engine that gets exposure, white balance, tone, colour and local light right, in the photographer's own style. |
| E4 Retouch & Restoration | 20-24 | Portrait retouching, micro-retouching, denoise/sharpen/face recovery, geometry and safe generative cleanup. |
| E5 Gallery Brain & Autonomy | 25-28 | Gallery-wide consistency, multi-camera matching, an autonomous QC agent, and one-button Zero-Touch delivery. |
| E6 Curation & Delivery | 29-30 | Album, hero, B&W and social curation, then export, integrations, the learning loop and release engineering. |

## Version milestones

- **V1 Wedding Autopilot Core** - Phases 01-17 plus 28 and 30 (culling, grading, style, autopilot, export). This is the sellable product.
- **V2 Portrait Intelligence** - Phases 18-23 (masks, local light, retouch, micro-retouch, restoration, geometry).
- **V3 Gallery Intelligence** - Phases 25-27 (consistency, camera matching, QC agent).
- **V4 Advanced AI** - Phases 24 and 29 plus cloud scale-out and studio/team workflow.

Ship V1 before starting V2. A shipped culling-and-grading autopilot beats an unfinished everything-machine.
