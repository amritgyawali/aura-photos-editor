# Saved selection tools

In Develop, open **Masks**, select a mask, then open **Selection tools**.
The additional [authored geometry and topology workflows](mask-authoring-topology-workflows.md)
include stored stroke editing, component composition, selection placement and region isolation.
The [selection laboratory](mask-selection-laboratory.md) adds 25 more shape,
source-sampled, topology, placement and mask-transfer workflows.
Each action keeps the selected mask's exposure, colour, amount and enabled state.
Other masks and global edits are preserved. Undo restores the prior recipe.

| Action | Result |
| --- | --- |
| Duplicate whole mask | Creates an independent mask with the same components and adjustments, including extended local colour settings. |
| Invert whole selection | Computes the full composed coverage first, then selects its complement across the entire uncropped frame. Subtracted holes become selected. |
| Freeze composed selection | Replaces the component stack with saved coverage, including current portrait-region and brightness-range results. |
| Union with saved mask | Uses the larger coverage from the two complete selections. |
| Intersect with saved mask | Multiplies the two complete selections' coverage. |
| Subtract saved mask | Multiplies the selected mask by the complement of the second mask. |
| Feather saved selection | Applies a separable box filter to soften the saved coverage. |
| Expand selection | Applies a square maximum filter to grow the selection. |
| Contract selection | Applies a square minimum filter to shrink the selection. |
| Refine edge contrast | Steepens coverage around its 50% midpoint while retaining full black and white. |
| Remove small selection islands | Removes selected components no larger than the specified number of cells. |
| Fill small selection holes | Fills enclosed unselected components no larger than the specified number of cells; frame-connected background remains unselected. |

Combination uses coverage, independently of each mask's adjustment amount and
enabled state. It changes the selected mask only; the second mask remains separate.
Duplicating an enabled adjustment can visibly strengthen its effect where the
copies overlap.

Coverage comes from AURA's existing native interactive masking renderer, measured
before crop and without the retouch stack. Freezing a luminance selection therefore
uses this masking preview's global adjustments. It does not sample the final exported
colour after every retouch and local adjustment. Saved coverage uses the recipe's
existing matte format: at most 512 cells on its longest side, with 16 coverage levels
and interpolation at render resolution. Fine edges can change slightly when frozen.
Refinement radii are in saved-grid cells, not original-image pixels. Island and hole
cleanup uses four-connected neighbours and a 50% coverage threshold.

Radius is bounded to 32 cells, edge contrast to 100%, and component area to 4,096
cells. A zero parameter performs no operation and adds no history row. Native recipe
hash checks reject outdated requests, and each successful change is saved atomically
as one history step. Cancellation is checked before and after native coverage
extraction, throughout refinement, and before saving. The synchronous coverage call
itself cannot currently be interrupted. Original image files remain unchanged.

These are independently implemented AURA extensions to its masking workflow.
Evoto's complete saved-selection operations and numerical equivalence have not been
verified; these additions do not establish parity with proprietary algorithms.
