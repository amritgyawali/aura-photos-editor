# Runbook - a model update failed

**The photographer keeps working.** That is the design: a new model version is
`pending` until it has completed one real inference, and the previous version
stays on disk until then. Nothing below is an emergency for the customer; several
of them are urgent for us.

## Decide which failure it is

| Code | What it means | Who fixes it |
|---|---|---|
| `AURA-ML-5002` | The signature over `models.lock` did not verify | Security. Treat as an incident until proven otherwise |
| `AURA-ML-5003` | A file's digest does not match the signed manifest | The machine: re-install the pack |
| `AURA-ML-5004` | The transfer ended early | Nobody: it resumes |
| `AURA-ML-5005` | A model has no model card | The release: it skipped its gate |
| `AURA-ML-5009` | A new version failed its first real use and rolled back | The release: pull the version |
| `AURA-ML-5010` | The bytes verified and the parser still refused them | The artefact: re-export |
| `AURA-ML-5012` | A delta could not be applied | Nobody: the full file is fetched |

## The order the chain runs in

Signature, then digest, then model card, then load. It is fixed, it is offline,
and the first failure stops the chain. If you are looking at a digest failure,
the signature already passed - so the manifest is trustworthy and the *file* is
the problem. If you are looking at a signature failure, nothing after it has been
checked at all.

## What is on disk during an update

```text
models/
  scene_1.4.2.fp32.onnx           the live file
  scene_1.4.2.fp32.onnx.part      an in-flight transfer, resumable
  scene_1.4.2.fp32.onnx.previous  the version being kept until the new one proves itself
  installed.json                  which version is active, pending or rejected
```

A file at its final name has been verified. Nothing unverified ever exists under
a name the loader would open - transfers land in `.part`, are verified whole, and
are then renamed.

## Standard recovery

1. **Re-install the pack from Settings.** This fixes every single-machine
   corruption and every interrupted transfer.
2. **If it fails twice on one machine**, look at the disk and at security
   software: an antivirus that quarantines part of a file presents exactly as a
   digest mismatch.
3. **If it fails on two machines**, the published pack is wrong. Pull it from
   distribution, and escalate to MLOPS - and to SEC if the failure was a
   signature.
4. **Never re-sign on a customer's machine.** The release key is offline for a
   reason, and a signature made anywhere else proves nothing.

## Rollback

Automatic, and already done by the time anybody reads the error: the previous
files are renamed back, the failed version is marked rejected in `installed.json`
so it is not retried on every launch, and `model.update` records `ok = false`.

Re-installing the same version clears the rejection, which is what an operator
expects after a fix has been published.

## What to attach to a report

- The error code and the model name from the log line.
- `installed.json` (it contains no paths and no personal data).
- Whether a previous version was present - "rolled back to nothing" and "rolled
  back to 1.4.1" are different conversations.

## Related

- Error codes: `AURA-ML-5002` through `AURA-ML-5012`
- Adding or publishing a model: `docs/runbooks/adding-a-model.md`
- Runtime ADR: `docs/adr/ADR-0007-inference-runtime.md`
