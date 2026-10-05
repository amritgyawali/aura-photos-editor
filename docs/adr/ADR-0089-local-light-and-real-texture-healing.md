# ADR-0089: Match blemish repairs to local light while preserving real texture

Status: accepted
Date: 2026-10-06

## Problem

Deep cleanup can spend its bounded repair budget on bright pores. A donor can also
introduce low-frequency colour or shading into a repair, even after matching its
boundary. The requested result is cleaner skin that retains texture and facial shape.

## Decision

New automatic deep repairs save `textureHeal: true` in the existing recipe extension.
The renderer robustly fits local RGB lighting from three rings around the repair,
downweights inconsistent samples jointly across channels, and transfers only the
high-frequency detail of the clean donor. Small donors retain the existing reflected,
unscaled sampling. No noise texture, external generated image or reshaping is used.
Missing or false `textureHeal` keeps the previous harmonic healing algorithm. An
insufficient neighbourhood falls back to that algorithm. The setting is editable
in the existing Retouch panel and travels with presets, undo and saved recipes.

Spot selection requires local darkness or redness instead of treating every bright
pore as a defect. Larger compact defects receive more priority. Modestly larger
enclosed segmentation holes can be repaired; feature exclusions are reapplied after
filling, and repair disks must still fit inside selected skin. Target selection accepts lower-confidence skin inside the segmented face, but donors
still require stronger skin confidence. Lip protection insets its end points before
adding lip height, avoiding cheek-wide circles at mouth corners. Budgets do not grow.

Automatic feature planning is versioned `measured-features-v4`. The permanent studio
layout, originals, manual overrides and existing frequency/dodge-and-burn steps stay
in place. Automatic reruns replace their previous operations.

## Limits

Borrowed skin detail is real donor texture, not recovery of the texture hidden by a
blemish. A lighting plane is a local approximation. Clusters, occlusion, facial hair,
skin edges and insufficient clean donors can still need manual correction. Dark-mark
removal remains an explicit option because marks can be intentional. There is no
claim of universal acne removal or a comparison with an experienced human editor.

## Validation

Regression tests exercise colour and sloping light across three exposure levels,
retention of donor pores, exact saved-recipe replay, unchanged pixels outside the
repair, and protection against spending the acne budget on bright pores. Desktop
validation uses AURA's import, automatic retouch and verified export controls.
