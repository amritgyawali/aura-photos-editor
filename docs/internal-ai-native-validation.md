# Internal AI and skin repair validation — 2026-10-11

Implemented and tested in the readable `C:\Users\amrit\aura-eye-protection`
checkout on `fix/eye-protection-and-retouch-coverage`. The supplied D: checkout
and some D: evidence destinations returned filesystem errors or failed copy
readback. Final native evidence and application backups are on C:.

The tested Windows custom-protocol executable has SHA256
`FC5CF4F70A5847712B777859170178BAA216696D1FC792F4210CA0957CC1C2E2`.
The standard `app/aura-desktop.exe` is updated from that verified executable.
Its prior version is retained in the ignored C: validation directory. The four
tested runtime DLLs accompany it. The GNU linker emits a resource-manifest
warning; the resulting app launched and completed the native checks below.

## Automated checks

- UI: 744 tests in 87 files; TypeScript checking and production bundle passed.
- Latest application library: 179 passed, 2 ignored.
- The offline one-click integration test also passes with the new face/body
  defaults and explicit custom preferences. Explicit IPC return types satisfy
  the older phase-gate parser; the rebuilt production JavaScript is byte-identical
  to the native-tested bundle. These compatibility fixes do not change runtime code.
- Cloud library: 35 passed; gateway integration: 17 passed.
- Recipe and renderer libraries: 70 and 198 passed. Additional native healing
  and residual-review integration fixtures passed, including default two-pass
  compatibility, improved repair across passes, and unchanged excluded pixels.
- Strict workspace Clippy across all targets, formatting and banned-pattern
  checks passed. All 318 IPC names agree; all 83 contracts remain locked.
- Python native-driving harnesses compile. Staged source is compared against
  the supplied credential values before delivery; keys and photos are excluded.

## Native AI and batch checks

The supplied file imported 11 original profiles plus 5 vision alternatives.
Import/discovery success is not proof of successful inference. Native Gemini
3.8 calls succeeded earlier in this validation, and Gemini 3.6 fallback
successfully graded photos and returned bounded skin advice. The final executable
also completed fresh Gemini 3.6 calls in the recovery batch. Other configured
services returned authentication, availability, quota or unknown-price states;
they are not described as fully connected.

The single-photo harness passed exact-pixel grade undo/redo, unchanged-repeat
recipe/history checks, replacement of an existing feedback rating, full-resolution
read-only review, stale-review rejection, and offline refresh remaining offline.
Fresh skin advice used face/body scope with body blemish strength enabled.

Both three-photo batches completed with zero failed edits and three exports
written/read back. The offline batch used local editing and made no cloud image
requests. The online recovery batch reported an intentionally invalid
configuration refresh and continued with saved providers. Both retained the
first photo's distinct saved settings and preserved source/copy SHA256 hashes.
The invalid fixture was confined to the test catalogue; the supplied file was
not rewritten.

The body-only offline test produced native healing without a face, preserved
the existing global grade and passed lossless export. Early cancellation wrote
zero exports and left the saved recipe unchanged. An explicitly chosen
default-valued face-only preference also survived subsequent automatic editing
when no face was detected.

## Native detailed retouch exports

Each input was imported, reset to its original global grade, processed through
the studio's **Detailed skin repair and review** preset and exported by AURA.
The preset used four repair passes. JPEG readback passed. The additional PNG
exports matched both the full native render and retouch view byte-for-byte.
Source and imported-copy hashes stayed unchanged.

| Input | Full dimensions | Saved operations | Compact repairs | Protected full-resolution regions |
| --- | --- | --- | --- | --- |
| Heavy-acne portrait | 2048 × 3072 | 490 | 482 | Both eyes and both nostrils: zero change |
| Second portrait | 1333 × 2000 | 254 | 248 | Both eyes and both nostrils: zero change |
| Body-only crop | 1333 × 1050 | 1 | Whole-mask frequency healing | 1,261,386 pixels outside selection unchanged |

The body crop selected 138,264 pixels. Detailed healing changed 24,909 selected
pixels; restrained offline defaults changed 9,952. Clothing, jewelry, hair and
background outside the selection remained unchanged in the lossless output.

At the first portrait's explicitly matched 1066 × 1600 inspection grid, nose
skin coverage was 70.98%. Nostril wings were excluded from broad finishing;
their changes were confined to compact saved repairs. The independently inspected
nose structure rectangle changed in 60 pixels, with maximum channel delta 7;
coarse relative change was 0.5864%, below the 1% acceptance limit. Broad rendered
pixels there remained identical. Body selection excluded detected faces. The
temporary diagnostic edits were undone to the exact baseline recipe hash.

The real native review UI also passed: checking and preparing a candidate did
not save an edit, a dirty draft prevented replacing it with another candidate,
Apply saved a focused four-pass repair, and Undo restored the exact recipe hash.

## Visual verdict and limits

The full face and body comparisons were inspected. The portraits are visibly
cleaner, with eye makeup, lips, nose contours, complexion and lighting recognizable.
Residual redness, small marks and texture remain, particularly in the heavy-acne
portrait. This is not a 100% removal verdict or proof of superiority to Evoto or
Retouch4me. No matched Evoto outputs were available for these source photos.
Full Evoto parity remains unverified; its public examples do not disclose a
unique algorithm. Broad skin tones, poses, occlusions, hands and legs still need
a larger real-photo dataset. The body fixture covers available neck/arm/hand skin.

Daily learning ranks routes from bounded local reliability and user ratings;
it is not foundation-model training or autonomous code improvement. Unknown
prices remain blocked. Spending estimates are not provider invoice reconciliation.

Evidence is intentionally ignored at `output/internal-ai-20261011`: `live-ai-final`,
`local-body-final`, `local-body-full`, `batch-offline-final`, `batch-recovery-final`,
`detailed-{first,second,body}`, their `-full` counterparts,
`detailed-first-protected`, and `review-ui`. The working catalogue and its C:
backup pass integrity checks with all 13 saved recipe hashes matching the baseline.

See [the workflow](internal-ai-workflow.md) for controls and
[ADR-0114](adr/ADR-0114-internal-ai-routing-and-residual-review.md) for implementation.
