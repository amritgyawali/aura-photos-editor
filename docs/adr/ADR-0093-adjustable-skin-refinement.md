# ADR-0093: Adjustable skin cleanup and pore refinement

Date: 2026-10-10

The user requested further blemish removal and visible-pore refinement while
retaining facial structure. Acne-only intentionally disables surface finishing,
so increasing its spot budget cannot meet the pore-refinement part of this request.

Add a separate `Skin cleanup · refine pores` preset using the existing native
tools: face and body blemish cleanup, moderate face/body smoothing, fine pore
refinement and restrained redness correction. Reuse the acne-only protection
settings. Preserve the acne-only preset's behavior and make every strength
adjustable. Do not label either preset perfect or certify uninspected skin as clear.

Validate pale-centered red-lesion detection separately from bright pores before
changing candidate rejection. Retain lighting, shape, donor and feature guards.
Native fresh imports and full-resolution exports must establish the actual
appearance; test counts and saved-operation counts alone cannot establish quality.

The expanded test reproduced a missed red lesion with a pale center (radius eight
pixels). Candidate rejection previously checked redness only at the center. It now
uses the maximum redness of the measured connected component; ordinary highlight
pores still lack red evidence. Existing shape, light consistency and clean-donor
requirements remain in force. This addresses that specific false rejection, not
every possible acne appearance.

Native review of the angled portrait exposed a flattened patch at the outer
nostril wing, also present in the earlier acne-only result. Checks limited to
the nostril openings had missed it. Extend the rotation-aware exclusion to
both wings for healing and broad finishing, while leaving bridge and tip skin
eligible for spot removal. Add wing assertions at two raster sizes, native
wing pixel checks, and a regression rectangle over the observed damaged area.

The expanded wing exclusion revealed a second interaction: frequency healing's
lighting check read only selected pixels, so masking a nearby dark contour could
make the remaining bright skin appear to surround a red lesion. Read lighting
context from all in-frame ring samples, while still accepting donors and skin
evidence only from selected pixels. A masked-shadow regression fails before this
change; its evenly lit partial-selection control must still accept a lesion.
