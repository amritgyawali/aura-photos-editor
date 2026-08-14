# ADR-0014 - The people IPC surface

**Status:** accepted
**Date:** 2026-08-13
**Phase:** 06 - Face Detection, Recognition & People Intelligence
**Deciders:** CTO, Senior Frontend Engineer, Security & Privacy Engineer

## Context

`crates/aura-app/src/contract/ipc.rs` and `ui/src/ipc/types.ts` are a frozen pair: both
are digested in `contracts.lock`, and changing one without the other fails CI. Phase 06
adds eleven commands, fifteen types and one event enum to them. This ADR records the four
decisions that shaped what crosses the boundary, because the boundary is where a privacy
promise is either kept or quietly broken.

## Decision

### 1. No template ever crosses the boundary

There is no `get_face_embedding`, and there will not be one.

A 512-d recognition template is a biometric identifier. A web view has no use for a number
it cannot compare, and a template in a JSON payload is a template in a crash log, a
support bundle and a browser's memory dump. Every comparison - clustering, verification,
the merge dialog's suspicion warning - happens in Rust.

This mirrors ADR-0012's "no vector crosses the boundary" for the similarity surface, and
it is stricter for the same reason phase 06 is stricter than phase 05 everywhere: a
perceptual vector describes a photograph, a template describes a person.

### 2. A face crop is behind a command that says what it is

`identity_cover(projectId, faceId)` unseals exactly one crop and returns it as a data URL.
It is the only route from the sealed store to a screen.

It exists because a People panel with no faces in it is unusable - a photographer cannot
merge two identities they cannot see. It is one crop per call rather than a batch, so a
panel of sixty identities does not decrypt sixty crops before it can draw anything, and so
that a single command in a log is a single face.

The crop was JPEG-encoded before it was sealed (ADR-0013 section 4), so this command
decrypts and hands the bytes through. There is no decode-and-re-encode on the interactive
path.

### 3. Every DTO that carries a decision carries its reasons

`IdentityCardDto.roleReasons` and `FaceBoxDto.reasons` are on the wire rather than
reconstructed in the UI.

Invariant 2 is not satisfied by a confidence alone, and the People panel is the surface
where a photographer is most likely to disagree with the product. "Why is my cousin not in
this group" has an answer - "38 px tall, below the 48 px identity gate" - and that answer
has to travel with the row.

`FaceBoxDto.reasons` is **re-derived from the stored numbers** rather than stored as text.
The quality verdict's reasons are a function of the stored measurements and the gate
version, so a second copy in a column would be a cache that could disagree with the
numbers beside it.

### 4. The destructive command requires the photographer to type its target

`eraseBiometrics` refuses unless `confirm` equals `projectId`, and the check is in the
**backend** rather than only in the dialog.

Erasing biometric data is the one operation in this product with no undo. A command whose
only argument is the thing it destroys is one mis-click from a support ticket nobody can
resolve, and a confirmation that lives only in the UI is a confirmation a script can skip.

## The surface

| Command | Shape | Blocking |
|---|---|---|
| `people_status` | `projectId -> PeopleStatusDto` | one view read |
| `list_identities` | `projectId -> IdentityCardDto[]` | one view read |
| `image_subjects` | `photoId -> ImageSubjectsDto` | one frame's rows |
| `identity_timelines` | `projectId -> IdentityTimelineDto[]` | one project's rows |
| `identity_cover` | `projectId, faceId -> FaceCropDto` | one sealed file |
| `scan_faces` | `ScanFacesInput -> ScanFacesDto` | **a batch pass**; resumable |
| `group_people` | `GroupPeopleInput -> GroupPeopleDto` | **a clustering pass** |
| `merge_identities` | `MergeIdentitiesInput -> IdentityHandleDto` | one transaction |
| `split_identity` | `SplitIdentityInput -> IdentityHandleDto` | one transaction |
| `set_identity_role` | `SetIdentityRoleInput -> ()` | one statement |
| `rename_identity` | `RenameIdentityInput -> ()` | one statement |
| `set_identity_importance` | `SetIdentityImportanceInput -> ()` | one statement |
| `erase_biometrics` | `EraseBiometricsInput -> EraseBiometricsDto` | irreversible |

Two of them take longer than the 50 ms the application layer promises, and both say so:
the UI calls them from a job rather than a click handler, and both are resumable so a
cancel costs nothing.

### Two shapes worth explaining

**`PeopleStatusDto` carries coverage, staleness and erasure.** An empty People panel has
four different causes - unscanned, no faces, gated out, erased - and a panel that cannot
tell them apart generates a support ticket for each. `coverageSentence` in the UI is one
function over this DTO and it is unit-tested directly.

**`ImageSubjectsDto.prominence` is an array, not a map.** A `Record<string, number>` would
serialise fine and would leave the ordering to whichever side iterated last. An array is
the server's order, and two panels cannot disagree about it.

### `PeopleEvent` is typed and not yet emitted

Four variants mirroring section 11's four telemetry events. They are typed on both sides
and no emitter exists, for exactly the reason `IngestEvent`, `InferEvent`, `CloudEvent`
and `IndexEvent` have none: the Tauri shell has not been launched on the development
machine, so an emitter would be code nobody has run. The `tracing` events *are* emitted,
under the same names, and the phase gate reads them.

## Consequences

**Good.** The privacy boundary is enforced by the shape of the surface rather than by
review: there is no command that could leak a template, because none returns one.

**Bad.** `identity_cover` is a per-card round trip. A panel of sixty identities makes
sixty calls, each unsealing one file. Measured at well under the 300 ms panel budget
because the cards fetch lazily as they scroll, but it is a shape that would need batching
if a project ever had hundreds of identities.

**Ugly.** `FaceBoxDto` has seventeen fields. It is a debug surface as much as a product
one - phases 09 to 22 all inspect faces - and a narrower DTO would mean a second command
within a phase.

## References

- The contract: `crates/aura-app/src/contract/ipc.rs`, `ui/src/ipc/types.ts`
- Commands: `crates/aura-app/src/people_commands.rs`
- The panel: `ui/src/components/people/`
- The store and its guarantees: `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md`
- The similarity surface this one mirrors: `docs/adr/ADR-0012-similarity-ipc-surface.md`
