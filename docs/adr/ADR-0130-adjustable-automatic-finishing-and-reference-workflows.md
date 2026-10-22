# ADR-0130: Adjustable automatic finishing and reusable reference workflows

Date: 2026-10-20. Roles: rendering/colour, UI engineering, QA.

The user requests Evoto's editing workflows, reference-photo colour matching, and
independent strengths after automatic editing. Existing AURA already implements
different-scene reference fitting, four matching strengths and individual saved
operation controls. This decision extends those working paths.

## Automatic finishing

Store each automatic operation's original amount and a fingerprint of its other
parameters in a versioned recipe extension. Overall and category strengths are
absolute multipliers from 0 to 200 percent, bounded by each renderer's existing
amount range. Zero remains reversible. Changes save together as one history step.
Keep manual operations, disabled operations, selections and unrelated recipe
fields. Snapshot automatic mattes when an automatic operation becomes a manual
strength adjustment, using the existing native-retouch preservation mechanism.
Individual editing detaches a managed operation from subsequent group adjustment.
Check the expected recipe hash before saving so delayed controls cannot overwrite
newer work. Undo/Redo carries both the operations and their baseline metadata.

## Reference colour

Keep the existing local, colour-managed reference decoder and measured native
renderer fitting. Add fine matching strengths, reusable library organisation and
reference creation from an edited preview. Preserve the baseline and manual-field
guards. A rendered photo can guide appearance; it cannot identify every original
editor parameter uniquely. No Evoto source, resources or weights enter AURA.

## Qualification

Use native history/pixel tests for zero, half, full and increased strength;
reopening; manual and disabled operations; stale writes; and mask preservation.
Use frontend interaction tests for independent controls and navigation. Maintain
an evidence ledger distinguishing observed UI, public documentation, implemented
behaviour and photographic quality. Missing task models, hardware workflows and
unqualified image cohorts remain explicit gaps; labels do not establish parity.

No frozen contract, database migration, new model or external image upload.
