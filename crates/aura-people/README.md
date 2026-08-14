# `aura-people`

Who matters at this wedding, and the encrypted store that knows.

## What it is for

Phase 06 ships one feature: a subject hierarchy. Every later decision in this product needs
to know that sharpness on the bride's face outranks sharpness on a stranger's elbow, and
nine phases - 09 to 13, 18 to 22, 25, 27 and 29 - read the answer from here.

This crate owns the durable half: the identities, the co-occurrence graph, the roles, the
timelines, the importance model, and the sealed biometric store they are derived from. The
per-frame half - detection, alignment, quality, templates, clustering arithmetic - is
`aura_vision::face`.

## The split, and why it is structural

`aura-vision` has no catalog dependency, so it **cannot** persist a template even by
mistake. `aura-people` has no socket dependency, so it cannot upload one. The two most
sensitive capabilities in the product - biometric storage and network access - are in
different dependency graphs, and `scripts/check-banned.sh` keeps them there.

The one thing this crate needs from the operating system's credential store is a port,
`BiometricKeyStore`, implemented by the application layer over `aura_cloud::keys`. That is
why linking the crate that can reach the network is not necessary to hold the key.

## The five guarantees

1. **Templates are sealed.** `vault` is the only way in or out, the key lives in the OS
   credential store, and `faces.embed` never holds a vector. A catalog copied off the
   machine without the keychain entry has no biometric data in it.
2. **Nothing crosses a wedding.** Every query is scoped by `project_id`; section 2.2's
   prohibition is enforced by the schema and by `PeopleStore::assert_same_project`, and
   `AURA-SEC-9004` halts rather than degrades.
3. **The photographer wins.** `identities.user_locked` is checked inside the statement that
   would overwrite it, and `People::regroup` replays the decision journal onto every fresh
   grouping *before* it draws a conclusion from it.
4. **Erasure is real and verified.** `PeopleStore::erase` deletes the key first - so a
   crash mid-erasure leaves unreadable data rather than readable data - then the crops,
   then the rows, then checks that nothing survived. Culling and edit decisions are
   untouched.
5. **Coverage travels with every answer.** `SubjectHierarchy::coverage` is phase 05's rule
   inherited: a conclusion drawn over a 40 %-scanned wedding is a conclusion about 40 % of
   a wedding.

## Layout

| Module | What it holds |
|---|---|
| `vault` | the envelope: BLAKE3 encrypt-then-MAC with a synthetic nonce, and the key-store port |
| `store` | every read and write migration 6 adds, sealing on the way in and opening on the way out |
| `scan` | the project walk: one frame at a time, resumable, cancellable |
| `graph` | the co-occurrence graph role inference runs on |
| `timeline` | when each person appears, with whom, and the gaps |
| `importance` | what the product thinks each person is worth, and why |
| `api` | `People`: the one implementation of the frozen `PeopleService` |
| `errors` | ten coded failures, five security and five model |

## What is not real yet

The three shipped models are placeholders with the right architecture and no training, so
the *templates* carry no identity information. Every mechanism around them is real and is
measured against the synthetic ground truth in `aura_vision::face::fixtures`.

This is condition **C1** in `docs/progress/PHASE-06-EXIT.md`, it is a Sev 2 trigger, and
**no later phase may claim a quality result that depends on face recognition being accurate
until it closes.**

## See also

- `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md` - every decision, with
  the alternative that was rejected
- `docs/adr/ADR-0014-people-ipc-surface.md` - what crosses the boundary, and what does not
- `docs/runbooks/AURA-SEC-9001.md` to `AURA-SEC-9005.md` - the biometric failures
- `docs/model-cards/face_detect.md`, `face_embed.md`, `face_quality.md`
