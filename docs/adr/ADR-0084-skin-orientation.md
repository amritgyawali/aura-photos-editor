# ADR-0084: Normalize skin segmentation orientation

Accepted, 2026-10-04.

The face detector tries quarter-turn views, but skin segmentation and its forehead/body
geometry previously consumed the original orientation. A real portrait regression found
face-mask Dice overlap of 0.92464 after a half turn and 0.94452 after a three-quarter turn.
These measure consistency with the upright result, not accuracy against ground truth.

Normalize the bounded analysis proxy using the largest confident face's eye-to-mouth
direction. Run segmentation, person crops, ownership and colour sampling in that view.
Map all face/body/hair/clothing/background mattes and sample positions back to the original
coordinate system. Bump the skin pipeline to v3; model weights and hash remain unchanged.
Reject invalid face geometry before inference. No original pixels or saved manual edits change.

This adds at most one bounded proxy rotation and no model passes. Photos with people at
different orientations still share one chosen orientation. Arbitrary rotation, severe
occlusion, similar-coloured clothing and overlapping people remain limitations. This is
not a guarantee of complete or error-free skin selection.

Validation: non-square pixel/matte coordinate round trips, landmark orientation tests,
and the opt-in `skin_rotation` real-photo regression comparing all three quarter turns.
