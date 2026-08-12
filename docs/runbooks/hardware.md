# Runbook - "AURA is not using my graphics card"

**Read this first, then the code for the specific error.** The one-line answer is
usually in Settings > Hardware, and the panel is designed to give it without
anybody reading a log file.

## What to look at, in order

1. **Settings > Hardware, first line.** It names the provider that is answering:
   "Processor", "NVIDIA (CUDA)", "Windows graphics (DirectML)" and so on.
2. **The unavailable list.** Every provider that is not being used appears with a
   reason. Today, on every machine, that reason is "not compiled into this
   build": no GPU backend is compiled in - see
   `docs/adr/ADR-0007-inference-runtime.md`. That is expected and is not a
   defect report.
3. **The set-aside list.** A provider here failed its own check on this machine:
   `crash`, `mismatch` or `timeout`. Follow `AURA-GPU-4002.md`.
4. **"safe settings; this computer was not measured".** The probe was abandoned.
   Follow `AURA-GPU-4004.md`.

## What the probe actually does

On first run, and whenever "Re-check hardware" is pressed:

1. enumerate the providers in preference order - TensorRT, CUDA, DirectML,
   Core ML, processor;
2. run a small reference model twenty times on each candidate;
3. compare its output against the processor's within 1e-3;
4. take the median time as the score, and set aside anything that crashed,
   disagreed or hung;
5. write `hardware_plan.json` beside the catalog.

The whole thing has a fifteen-second ceiling. Past that it is abandoned and the
conservative plan is used *without being saved*, so the next launch measures
again rather than inheriting a pessimistic guess.

## The plan file

`hardware_plan.json`, in the `hardware` directory beside the catalog. It holds
the provider order, the memory budget, the batch sizes, the probe scores, the
set-aside list and any user override.

It is a cache of measurements and never a source of truth. Deleting it is always
safe: the next launch measures again. That is also the fix for almost every
strange hardware state, and it is what "Re-check hardware" does without needing
a file manager.

Never hand-edit it on a customer's machine. A plan written by hand has no probe
behind it, and the first thing anybody will do with the resulting bug report is
ask what the probe measured.

## Overrides

The panel lets a user choose a provider. The choice is honoured - unless that
provider was set aside for producing wrong numbers, which is a correctness
question rather than a preference - and it is recorded, so a crash report from an
overridden machine is not mistaken for a defect in the negotiation.

## What "slow" usually means today

This build has no GPU backend, so every machine runs the processor path. Before
escalating a "slow AI" report:

1. run `just bench-models` and attach the table;
2. confirm from Settings whether a batch job is running (contention shows up as
   queue time, not latency - see `AURA-ML-5008.md`);
3. compare against the model card's latency table for this machine class.

## Related

- Error codes: `AURA-GPU-4001` through `AURA-GPU-4005`
- Runtime ADR: `docs/adr/ADR-0007-inference-runtime.md`
- IPC surface for the panel: `docs/adr/ADR-0008-inference-ipc-surface.md`
