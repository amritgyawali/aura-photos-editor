# ADR-0111: Apply cleanup settings independently across a collection

Accepted 2026-10-11.

The user requested missing Evoto-style workflows in addition to detail-preserving
skin cleanup. Preset synchronization is useful only when each photograph keeps its
own masks, spot coordinates, donor texture, grade and manual edits.

The native Retouch workspace adds **Apply cleanup settings to this collection**.
It enumerates the collection in pages of 240, deduplicates photo IDs, and invokes
the existing native planner independently and sequentially for each photo. It
copies requested options only. It never transfers an operation, matte or donor
between photographs. Global grading is not requested by this action.

Each completed photo reports retouched, skipped, or failed. A failure does not
hide other outcomes or interrupt the remaining photos. **Stop after current
photo** lets the current native transaction finish and prevents starting another.
Leaving the workspace also requests a stop. Completed changes remain individually
reversible in each photograph's existing history.

The workspace busy lock prevents conflicting local writes while a collection
run is active. Results describe saved planner outcomes; a positive operation count
does not establish perfect skin or equivalence to any commercial retoucher.

Unit tests cover independent calls, duplicate enumeration, per-photo failure,
no-skin skips, cancellation, and the native workspace control. Native verification
is performed through `scripts/verify-collection-retouch.py` against an isolated
three-photo collection, including cancellation and preservation of saved grading.
