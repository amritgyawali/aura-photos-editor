# ADR-0103: ONNX Runtime on the graphics card, for learned masking selections

Status: accepted. This amends ADR-0007 for the models it names, and nothing else.
Date: 2026-10-08

## Problem

ADR-0007 kept ONNX Runtime out of the product. Every model ran on the bundled pure-Rust
interpreter, which is deterministic, auditable, and has no native dependency. That is right for
the small heads AURA trains. It is too slow for the networks a Lightroom-class mask needs:

| Need | Network |
|---|---|
| Subject at 1024 pixels | IS-Net |
| Sky with its clouds | U²-Net |
| An object inside a drawn box | Segment Anything |

On the interpreter each of these takes seconds to minutes per image, and the interpreter cannot
run Segment Anything at all, because it has no `Resize`. ADR-0102 shipped masking without them,
and the photographer chose to allow ONNX Runtime and the graphics card.

## Decision

### The runtime (`aura_infer::accelerated`)

ONNX Runtime **1.24.4 with DirectML**, through the `ort` crate (2.0.0-rc.12, API 24).

- **Loaded at run time, never linked.** The crate is built with `load-dynamic`.
  `onnxruntime.dll`, `onnxruntime_providers_shared.dll` and `DirectML.dll` ship beside
  `aura-desktop.exe`. There is no link-time dependency and no build-time download, so the build
  on this machine (GNU toolchain, no MSVC) is unchanged. A machine without the DLLs gets an `Err`
  with the reason, and every caller falls back.
- **Graphics card first.** Each session is built with the DirectML execution provider, and ONNX
  Runtime places on the processor whatever DirectML cannot run. When a run fails on the card,
  most often from running out of video memory, it is repeated once on the processor, which the
  model then keeps. `AURA_ORT_CPU` forces the processor.
- **Verified before it is opened.** Each model is pinned by SHA-256 (`aura_vision::ai`) and
  checked on load. A truncated download or a damaged disk is a refusal with a reason, never a
  session that returns noise. This matters on this machine, whose D: drive has reported NTFS
  corruption.
- **Still in one crate.** `scripts/check-banned.sh` already forbids `ort::` outside `aura-infer`,
  and that stays true.

The interpreter keeps every model it ran before. ADR-0007's argument still holds for the heads
AURA trains, and nothing in this ADR moves them.

### The models (`aura_vision::ai`)

| Selection | Model | Licence | Size | Warm run here |
|---|---|---|---|---|
| Subject, background | IS-Net general use (rembg) | Apache-2.0 | 178 MB | 0.33 s |
| Sky | SkySeg, a U²-Net | MIT | 176 MB | 0.08 s |
| Objects (drawn box) | SAM 2.1 Hiera-Tiny, encoder + decoder | Apache-2.0 | 109 + 16 MB | 0.43 s |

"Here" is the reference laptop: a GTX 1650 Max-Q with 4 GB, 8 GB of RAM, DirectML. A cold load
takes 5-10 s per model, for the SHA-256 check and the graph compile. The models load in a
background thread when the window opens (`warm_up`), so the first click does not wait.

**BiRefNet-lite was the first choice for the subject, and it does not fit this machine.** Its
deformable convolutions expand into intermediate buffers of most of a gigabyte (822 MB for one
`Mul`). It ran out of memory on the card, and then on the processor with 4.7 GB of virtual memory
free; the fp16 export failed the same way. IS-Net has no deformable operators and runs in a third
of a second.

### What each selection does, and where it falls back

- **Subject and background** use IS-Net: the main subject, which may be a person, a bouquet or a
  car. Without it, they come from the person segmenter (ADR-0082) as before.
- **Sky** uses SkySeg, which gets clouds and sunsets right. Its answer is accepted only when it
  reaches the top of the frame and does not run down to the bottom (`plausible_sky`): the model
  also calls a plain studio backdrop and an out-of-focus background "sky", and both of those
  reach the bottom of the frame. The subject is taken out of the sky. Without the model, the
  measured horizon detector (ADR-0102) is used.
- **Objects** is a new tool. The photographer drags a box; SAM 2.1 returns the object, clipped
  to the box. Without the model, the tool says it is not available.
- **People and face parts** still come from the person segmenter and the portrait parse. The
  person segmenter is better at telling a person's hair from their skin than a salient-object
  model is.

All of these are stored as 512-cell mattes (raised from 256, `MAX_MATTE_CELLS`) and refined
against the photograph at render resolution, as in ADR-0102.

### Distribution

The DLLs and models total about 600 MB and are not committed. `scripts/fetch-ai-models.sh <dir>`
downloads them, checks every hash, and lays out `<dir>` to be copied beside the executable: the
DLLs next to it, and `models/` beside it. Model cards: `docs/model-cards/isnet_general.md`,
`skyseg.md` and `sam21_tiny.md`.

DirectML's licence allows redistribution inside an application. It also says the component may
send usage data to Microsoft; that goes in `docs/privacy.md`, beside the cloud policy.

## Consequences

- Masking now matches Lightroom's selections, Objects included, on a laptop GPU, in well under a
  second per click.
- The product has a native dependency for the first time. It is optional, verified, and
  isolated in one crate.
- Results from the card and the processor can differ by rounding. These are selections a
  photographer looks at, not decisions in the ledger, so nothing is recorded per device.
- Every model here was judged by eye on public photographs, not by a gate (see the cards). The
  per-skin-tone measurement the Constitution asks of anything touching skin does not apply to a
  selection, but it is not a claim of parity either.
