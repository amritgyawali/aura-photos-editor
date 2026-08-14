# ADR-0013 - People intelligence and the biometric store

**Status:** accepted
**Date:** 2026-08-13
**Phase:** 06 - Face Detection, Recognition & People Intelligence
**Deciders:** CTO, ML Lead (Vision), Senior Engineer (Core Pipeline), Security & Privacy Engineer
**Supersedes:** nothing. **Amended by:** nothing yet.

## Context

Phase 06 ships one feature: the app learns who matters at this wedding. Doing that means
storing biometric data, which is a regulated category in most of the markets this product
sells into, and it means making a decision - who the couple are - that every later phase
weights by. Both halves have failure modes that are quiet, and this ADR records the
decisions that make them loud.

Nine questions had to be answered before code. They are answered in order below, with the
alternative that was rejected and why.

---

## 1. Where the frozen contract lives

**Decision.** The section 5 contract - `Role`, `FaceRef`, `SubjectHierarchy`,
`ImageSubjects`, `PeopleService` - lives in `crates/aura-core/src/contract/people.rs`,
not in `aura-people`.

**Why.** Look at the list of phases that consume it: 09 (blink), 10 (emotion), 11
(composition), 12 (culling and coverage), 13 (the ledger), 18 to 22 (masks and retouch),
25 (gallery), 27 (QC), 29 (curation). Every one of them needs the vocabulary - "is this
the bride" - and none of them needs the biometric store. Putting `Role` in `aura-people`
would make nine phases link the crate that holds the templates in order to read an enum.

`aura-core` still depends on no other workspace crate; a test asserts it.

**Rejected.** Re-exporting from `aura-people`. It looks identical from a caller's side and
is not: the dependency edge is what matters, and a re-export creates one.

---

## 2. Three spellings that differ from the phase document

Section 5 freezes the interfaces. Three of them are written differently in code, and each
difference is forced rather than preferred.

| Phase document | Code | Why |
|---|---|---|
| `fn hierarchy(&self, project) -> SubjectHierarchy` | returns `AuraResult<SubjectHierarchy>` | Both `hierarchy` and `subjects` read the catalog, and a catalog read can fail. Returning a default on `AURA-DB-3006` would make "this wedding has no identified people" and "the database could not be read" indistinguishable - the silent failure invariant 9 forbids. |
| `weights: HashMap<IdentityId, f32>` | `BTreeMap` | `HashMap::new` is banned by `scripts/check-banned.sh`, and the reason applies here more than anywhere: these weights are iterated when a prominence score is computed, and invariant 4 requires two machines to produce the same recipe. |
| `ImageId` | `PhotoId`, aliased | One type, aliased rather than duplicated, exactly as `aura_index` does it - so no conversion between a similarity query and a subject query can disagree. |

Two ids were added to the frozen `aura-core/src/contract/ids.rs`: `FaceId` (`fce_`) and
`IdentityId` (`idt_`). Section 5's signatures use both, and the `typed_id!` macro is
additive - a new entry changes no existing behaviour. `contracts.lock` was re-locked.

---

## 3. What migration 6 stores beyond section 5's schema

Section 5 freezes `faces` and `identities`. Both are copied column for column. Five things
are added, and each is here because something in sections 6, 10 or 13 cannot be satisfied
without it.

| Addition | Why |
|---|---|
| `project_id` on `faces` and `person_boxes` | Section 2.2 forbids cross-wedding identity persistence. A `WHERE project_id = ?` that a caller cannot forget is worth one redundant text column per face. |
| `face_scan` | **The resumability ledger.** Phase 05 could derive its remaining work from the presence of an `embeddings` row, because every photograph yields exactly one vector. A face pass cannot: "no faces in this frame" is a legitimate and common result, and inferring work-remaining from missing `faces` rows would re-scan every landscape for ever. |
| `face_vault` | Which credential-store account sealed this project, and whether it has been erased. Erasure records that it happened, because "there are no faces" and "the faces were deliberately destroyed" are different answers to a client who asks. |
| `identity_links` | The append-only journal of every people decision a human made. It is what makes merge, split and rename undoable *and* what makes them survive a re-analysis - see section 7 below. |
| `sub_centroids`, `variance` on `identities` | Section 6.2's outfit-and-hairstyle case: an identity whose members span two looks has a main centroid that matches neither. |

---

## 4. The envelope: BLAKE3 rather than `chacha20poly1305`

**Decision.** Templates, centroids and aligned crops are sealed with encrypt-then-MAC
built on BLAKE3: an XOF keystream under an encryption key, a keyed hash as the tag under a
separate MAC key, and a **synthetic nonce** derived from the record kind, the record's own
identifier and the plaintext.

**Why not an AEAD crate.** It would be one line in `Cargo.toml` and it is the conventional
answer. Two reasons it is not the answer here:

1. It brings six crates into a workspace that has no cipher dependency at all, and every
   one becomes a supply-chain surface for the most sensitive data the product holds.
2. BLAKE3 is already a workspace dependency - content addressing, migration hashes - and
   both of these uses, XOF-as-keystream and keyed-hash-as-MAC, are documented by its
   authors rather than invented here.

**What is not claimed:** that this is better than an AEAD from a cipher crate. It is a
composition of two documented primitive uses, chosen to avoid a dependency.
`ENVELOPE_VER` is in the header and `Vault::open` refuses a version it does not implement,
so moving to `chacha20poly1305` later is a version bump and a re-seal rather than a
rewrite.

**Why the nonce depends on the plaintext.** A stream cipher that reuses a (key, nonce)
pair across two plaintexts leaks their XOR, and that is not a theoretical risk here - it is
the *expected* behaviour of this product. A face id is derived from the photograph and the
box, so re-scanning after a recogniser upgrade writes a **different template under the same
id**. Deriving the nonce from the plaintext as well makes reuse impossible by
construction, and keeps sealing deterministic, which invariant 4 requires. The cost is
that identical plaintexts produce identical ciphertexts; for 512 half-precision floats
that is a non-event.

**What is not sealed:** boxes, landmarks, pose angles, quality scores, counts and
timestamps. Five normalised points inside a box cannot be matched against a person, and
they are needed in the clear by the People panel, the prominence scoring and every support
investigation. Encrypting them would put the panel behind the keychain for no gain.

**Crops are JPEG-encoded before sealing.** A raw 112 px crop is 37,632 bytes, which is 90 MB
per 1,000 images on its own - three and a half times section 11's whole 25 MB budget. At
quality 82 it is about 3.6 KB. The template is computed from the *raw* crop before the
encode, so no measurement depends on the compression.

---

## 5. The chain-merge guard is cohesion, not rank order

**Decision.** Section 6.2 asks for "rank-order verification (mutual nearest neighbour)".
Mutual nearest neighbour is satisfied **by construction** - the loop merges the pair with
the globally smallest average-linkage distance, and if `d(A,B)` is the global minimum then
each is the other's nearest. The guard that actually refuses merges is **relative
cohesion**: the gap between two clusters must be at most `max_cohesion_ratio` (2.2) times
the internal spread of the more spread of the two.

**Why the change.** Because mutual-NN, implemented literally, refuses nothing. It is a
property the selection rule already guarantees, so a rank test on top of it is dead code -
and the chain merge it is supposed to prevent still happens: two sisters' clusters are
mutually nearest and sit below any threshold loose enough to also group one person across
twelve hours of changing light.

Cohesion asks a different question, and the numbers separate the two cases. Calibrated
against real recogniser behaviour: two *looks* of one person sit about 1.5 to 1.8 times
their own internal spread apart; two *siblings* sit at three times it or more, even when
the absolute distance is comfortably under the threshold.
`crates/aura-vision/tests/face_cluster.rs` constructs both cases at those separations and
asserts the outcome, and `cohesion_verification_actually_refuses_something` fails if the
guard becomes inert.

**Also decided here:** average linkage is computed **exactly** from running sums, using the
identity that for unit vectors the mean pairwise cosine distance between two clusters is
one minus the dot product of their unnormalised means. That makes exact average linkage
cost one dot product per cluster pair rather than `|A| x |B|`, which is what makes section
11's twelve-second budget reachable without approximating the linkage.

**And:** the quadratic pass is capped at `MAX_SKELETON` = 4,096 of the highest-quality
voting faces. That is section 6.2's own two-pass strategy, and it is what bounds the cost
independently of project size.

---

## 6. The GPU throughput budgets are waived

**Decision.** Section 11's "4,000 images in <= 240 s on an RTX 4070" and "<= 480 s on an
M3 Pro" are **waived**, with an expiry condition. The processor-path row
`[stage.face_scan_image_cpu]` in `perf/budgets.toml` replaces them.

**Why.** This build has no GPU backend (ADR-0007), so neither number can be measured on
any machine that exists here. A green test that measures nothing is worse than an honest
waiver.

**Measured instead**, release, `1.97.1-x86_64-pc-windows-gnu`, Intel i5-10300H:

| Stage | Measured | Budget |
|---|---|---|
| 640 px detector pass | 148 ms | - |
| Recogniser, per face | 9.1 ms | - |
| Quality head, per face | 3.8 ms | - |
| One frame at 2.4 faces | ~180 ms | 420 ms |
| Clustering, 4,096-face skeleton | 2.1 s (debug) | 12 s |
| Face store per 1,000 images | 12.8 MB | 25 MB |
| People panel open | 0 ms | 300 ms |

At 180 ms per frame a 4,000-image wedding is about twelve minutes without tiling.

**Expiry.** The waiver ends when a GPU backend lands. The row is reinstated with the
numbers the phase document asks for, and this ADR is amended rather than replaced.

---

## 7. How a photographer's decision survives a re-analysis

**Decision.** Decisions are replayed by **face set**, not by identity id. Every journal
entry in `identity_links` carries the faces the identity held at the time, and
`identity_holding` finds whichever new identity holds a **strict majority** of them.

**Why.** Clustering after a model change produces new identity ids - it *is* a different
grouping, and pretending the old ids survived would attach a name to a person who is now
three people. A decision whose faces have scattered evenly is applied to none of them and
logged as orphaned, which is the honest outcome: the product does not know which of the
four the photographer meant.

**Ordering, which is the part that is easy to get wrong.** `regroup` replays decisions
*before* it builds the co-occurrence graph. A photographer who merged two identities has
said those faces are one person; a graph built before the merge would count that person as
appearing with themselves, and roles inferred from it would place them in the couple.

---

## 8. Roles never infer anything about a person

**Decision.** Automation assigns `Role::Couple` to both members of the pair the evidence
identifies. `Role::Bride` and `Role::Groom` are reachable **only** through
`PeopleService::set_role`, which sets `user_locked`.

**Why.** The evidence identifies a pair. Which of two people is the bride is not a
photographic fact, the couple may be same-sex, and there is no version of guessing that is
acceptable in this product. `neither_half_of_the_couple_is_ever_called_the_bride` asserts
it so a later refactor cannot quietly add a heuristic.

**And the confidence is capped while scenes are missing.** Phase 07 assigns scene labels;
until it does, the portrait and ceremony terms contribute nothing and the decision rests
on co-occurrence and frequency - which is weaker in a specific, predictable way: a bride
and her mother are constantly co-occurrent. `SCENELESS_CONFIDENCE_CEILING` is 0.62,
deliberately below the 0.90 line phase 04's rule draws, so the optional cloud hint is
*allowed* to help while scenes are missing and is locked out once they exist.

---

## 9. Files that are not where section 15 says

Section 15 lists the areas the phase may create. Four additions, all recorded here:

| File | Why |
|---|---|
| `crates/aura-vision/src/face/fixtures.rs` | The synthetic faces and the reference detector that section 10.1's recall, small-face and bokeh gates are measured against. Without it those three numbers cannot be stated at all. |
| `crates/aura-vision/src/face/redact.rs` | The background and face blur section 7 requires before a thumbnail may leave the machine. A privacy control belongs in code, not in a prompt. |
| `crates/aura-people/src/{errors,vault,store,scan}.rs` | Section 4 lists `{lib,timeline,graph,importance,api}`. The store, the vault and the project walk are what those five sit on; putting them inside `api.rs` would have produced a two-thousand-line file. |
| `crates/aura-cloud/src/couple_hint.rs` | Section 7's task. `tasks.rs` is phase 04's reference task and is left alone. |

The UI lives at `ui/src/components/people/` rather than `apps/desktop/src/routes/people/`,
matching the repository's actual layout as phase 05's `SimilarPanel` does.

---

## Consequences

**Good.**

- The two most sensitive capabilities in the product - biometric storage and network
  access - are in different dependency graphs, and `check-banned.sh` keeps them there.
- A catalog copied off the machine without its keychain entry has no biometric data in it.
- Every gate in section 10.1 is measured today against ground truth with a known answer,
  so the harness that will measure a trained model has already been run.
- Erasure is verified rather than assumed, and it keeps culling and edit decisions intact.

**Bad.**

- The three shipped models are placeholders. Detection recall and clustering F1 are real
  numbers about the *algorithms* and say nothing about the *weights*. Condition C1.
- The quality head's trust weight is 0.0, so the gate is four measured factors. Condition
  C2.
- No demographic analysis is published, because the fixtures use one skin tone and a
  fairness number computed from them would describe a renderer. Condition C5.
- Rolling an envelope rather than taking an AEAD crate is a decision a future reviewer may
  reverse. The version byte is there so they can.

**Ugly.**

- `AURA-SEC-9004` is designed to be unreachable. Code that exists to never run is code
  nobody exercises in the field, which is why `crates/aura-people/tests/privacy.rs` fires
  it deliberately.

## References

- Phase document: `docs/plan/phases/PHASE-06-PEOPLE-INTELLIGENCE.md`
- IPC surface: `docs/adr/ADR-0014-people-ipc-surface.md`
- Inference runtime and the operator subset: `docs/adr/ADR-0007-inference-runtime.md`
- Cloud policy: `docs/adr/ADR-0009-cloud-ai-policy.md`
- Model cards: `docs/model-cards/face_detect.md`, `face_embed.md`, `face_quality.md`
- Exit report and its conditions: `docs/progress/PHASE-06-EXIT.md`
