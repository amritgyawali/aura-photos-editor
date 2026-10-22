# AURA Workflow lab: 25 additional workflow capabilities

These are original AURA review and batch-authoring workflows. They are distinct from the existing single-photo named snapshots, absolute Sync Settings, and the earlier photo inspection tools. They do not claim to be recovered Evoto algorithms or learned models.

| # | Workflow | User-facing operation |
| --- | --- | --- |
| 1 | Multi-photo recipe checkpoint | Capture a cohort's complete saved recipes together in one metadata transaction |
| 2 | Checkpoint drift comparison | Show settings sections changed since a selected cohort checkpoint |
| 3 | Section-selective checkpoint restoration | Prepare a restoration from each photo's own checkpoint while retaining other sections and local edits |
| 4 | Conflict-aware rollback preview | Offer checkpoint rollback only for sections still matching the preceding plan's proposed output |
| 5 | Relative edit transfer | Add source changes since the checkpoint to target exposure/tone or saturation/vibrance controls |
| 6 | Batch change manifest | Review exact changed recipe paths and section conflicts before explicitly applying |
| 7 | Baseline-aware three-way transfer | Copy changed source sections only where a target still agrees with the checkpoint ancestor; report divergent sections |
| 8 | Batch saved-revision verification | Verify every loaded photo hash before review, comparison or handoff |
| 9 | Edit fingerprint cohorts | Compare canonical fingerprints of seven settings sections across the cohort |
| 10 | Cross-photo settings-section matrix | Inspect individual section signatures to locate differing settings |
| 11 | Durable ordered review queue | Save cohort order, recall the queue, and schedule its order by due date |
| 12 | Revision-bound custom checklist signoff | Define review criteria and pass/reopen a criterion for selected photos |
| 13 | Approval drift tracking | A changed saved recipe invalidates earlier signoffs and blocks handoff until reviewed again |
| 14 | Photo-coordinate correction issues | Add bounded correction notes with optional normalized coordinate anchors |
| 15 | Correction prerequisite gating | Resolve predecessor tasks before resolving dependent correction issues |
| 16 | Batch correction propagation | Copy a correction note to other photos while discarding inappropriate source coordinates |
| 17 | Local reviewer workload assignment | Persist a reviewer label per selected photo, with no external messaging |
| 18 | Due-date review scheduling | Validate calendar dates, surface overdue unapproved work and sort saved queue by deadlines |
| 19 | Saved-revision pair preference | Preview two saved edits after revision checks and record left/right/tied/undecided preference |
| 20 | Unresolved comparison review | Find tied, undecided or changed-revision pair comparisons |
| 21 | Saved compound review filters | Find cohort photos by status, reviewer, overdue state and open issues |
| 22 | Checklist-gated handoff manifest | Build JSON indicating whether every selected revision is approved and has no open corrections |
| 23 | Portable review metadata export | Export versioned queue, checklist, notes, reviewer labels and comparison records without recipes or file paths |
| 24 | Validated review metadata import | Replace bounded metadata for explicitly selected member photos; reopen imported corrections and clear approvals |
| 25 | Durable workflow activity history | Read the collection's last 128 successful workflow mutations after reopening the catalogue |

## Usage and limits

Open **Workflow lab**, select up to 32 collection photographs, then click **Load review workspace**. Every operation is explicit; mounting or changing tabs performs no background edits. The Review, Checkpoints, Compare and Handoff tabs expose the operations in the table. The ID list supports a supplied multi-selection and manual queue ordering. Reload after changing the cohort. No source files are modified by this feature.

Checkpoints are metadata snapshots of complete recipes, limited to eight and a 4 MiB workspace budget. They are captured atomically with an expected workspace revision and saved-recipe baseline checks. They differ from per-photo named snapshots by capturing the selected cohort together. Restores preserve source identity, camera/profile association, local masks, semantic mattes, retouch, cleanup, and unselected settings. Geometry restoration is confined to that same photo's checkpoint; cross-photo three-way transfer refuses geometry. Relative transfer changes bounded numeric tone or colour fields, leaving local edits and geometric composition intact. Values clamp to recipe-supported ranges and the existing user merge records protection/provenance.

Prepared edit plans are read-only until **Apply reviewed batch plan**. Apply preflights all target/source revisions, refuses any conflict, then uses the existing per-photo compare-and-save transaction. Each successful photo gets its own Undo history; this is deliberately reported as **per-photo application, not a cross-photo all-or-nothing edit**. A concurrent failure produces an explicit result for that target. Source originals and sidecars are never written here. Review metadata mutations use a single revision-checked transaction and include an activity event.

Review records apply to saved recipes. Editing a photo after signoff produces `changed_since_review`; an open correction produces `changes_requested`. Only a nonempty complete checklist for the current saved hash, with no open corrections, produces `approved`. Reviewer labels are local catalogue metadata. Due dates use calendar `YYYY-MM-DD` values. Portable review import cannot manufacture approval: all imported signoffs clear and imported corrections reopen. Imports require all referenced photos to be explicitly selected members of the current collection, enforce acyclic same-photo prerequisites, and are limited to 256 KiB. No manifest action publishes, exports photo pixels, or sends messages.

Fingerprint and section matrix outputs compare recipe settings; identical fingerprints do not establish identical visual appearance. Pair previews render bounded 512-pixel images after checking saved hashes, then check them again before showing the pair. Original 100% inspection remains necessary for final detail decisions.

## Validation

Native tests exercise catalogue persistence/reopening, checklist invalidation after edits, blocked handoffs, protected metadata revisions, cohort checkpoint/delta planning and application, geometry preservation, stale plans, issue dependencies, propagation, portable interchange trust limits, three-way conflicts, unresolved pairs, filters, foreign membership, malformed dates, input budgets and unchanged imported source bytes. UI tests exercise explicit load/invocation, native revision payloads, planning versus applying, conflict blocking, interchange, navigation races, errors and verified image-pair preview.
