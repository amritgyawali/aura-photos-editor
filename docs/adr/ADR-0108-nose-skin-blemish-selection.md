# ADR-0108: Include nose skin in blemish repair

Accepted 2026-10-08. Supersedes ADR-0091's exclusion of the entire nose from
blemish correction. The user requested repair of the nose and small missed skin areas.

The broad nose guard kept visible acne on the bridge and sidewalls outside every
automatic operation. Spot detection also used a less complete skin mask than frequency
healing, leaving small segmentation holes untreated.

Blemish tools now have a separate selection that includes nose skin and protects
the nostril openings and underside crease with rotation-aware soft exclusions.
Eye protection remains unchanged. Broad smoothing, tone correction and texture
finishing continue to use the full nose guard when Preserve nose detail is enabled.
The feature guard cache separates these tool classes, even if their source matte
is the same. Stored recipes retain their original masks until the user reruns retouch.

Residual spot search uses the same connected skin fill as the blemish surface.
It still requires a complete repair disk and a clean donor; background, brows,
eyes and lips remain excluded. The predicted frequency-healed pixels use the
actual protected mask, so residual planning matches the saved render selection.

The first nose-inclusive real-photo export failed visual review: frequency healing
mistook the chromatic edge of the nose shadow for a red mark and rebuilt it from
brighter skin. Disabling that operation in an unsaved native preview isolated the
cause. The redness test now also requires compatible light around the candidate;
a substantially darker neighbour or a strong difference between surrounding
sectors rejects the repair. Residual patch proposals require compatible surrounding
light too. Ambiguous marks remain rather than being repaired across a contour.
The same renderer rule applies to existing frequency-heal recipes at render time.

Regressions check nose selection at preview/export scales, nose-mark improvement,
small enclosed segmentation holes, independent finishing masks, and unchanged
eye/nostril pixels. Native validation must include a fresh full-resolution export
and visual inspection. Five facial landmarks are approximate; this does not claim
perfect arbitrary-photo segmentation or removal of every mark.

On 2026-10-09 the user requested smaller remaining marks and full visible body skin.
Acne only now selects face and body, raises the local face-repair limit from 80 to
220, and enables compact-mark frequency healing on body skin. Face and body
smoothing, colour matching and broad tone/light changes stay off in that preset.
The renderer and residual detector search one additional smaller scale, retaining
the pore/glint, compactness, donor and directional-light checks.

Body masks are intersected with the exterior of every detected face, with a cell
margin and edge refinement disabled. This protects facial details when segmentation
incorrectly includes a face in a body mask; body-only work cannot override the
face tool's protection. Saved manual selections can reuse the guarded body mask.
