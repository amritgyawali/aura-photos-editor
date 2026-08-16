# Composition and framing

AURA's Composition card describes visible framing evidence. It never crops, straightens,
removes an object, rejects a photograph, or declares a photographer's style wrong. A low
score means several measured signals moved away from the bands recorded for that scene;
it is review evidence, not a selection.

The overlay draws each evidence rectangle over the preview. A solid outline marks a
measured issue, a dashed outline marks the region a crop hint asks phase 23 to preserve,
and an “allowed” note is an exoneration: a rule fired, but the scene says the choice reads
as deliberate. No rectangle means the evidence is frame-wide, such as horizon tilt or
balance. “Not checked” is different from a clean result.

## The 16 flags

The stable flag slugs are stored as bits and exposed as filterable facts. Only the four
most immediately verifiable defects (`horizon_tilted`, `head_cropped`, `joint_cut`, and
`head_merge`) are proposed as grid chips. `intentional_tilt`, `centred`, and `no_geometry`
are facts, not defects.

| Flag | Meaning | Typical overlay |
|---|---|---|
| `horizon_tilted` | A reliable horizon exceeds the scene's level tolerance. | Frame-wide angle and confidence. |
| `intentional_tilt` | A large tilt, centred subject, weak horizon, and permissive scene make the angle read as deliberate. | Frame-wide “allowed” note. |
| `verticals_converging` | Strong verticals converge as if the camera was pitched at architecture. | Dominant vertical-line region. |
| `headroom_tight` | Space above the highest reliable crown is below the scene band. | Subject/head region. |
| `headroom_excessive` | Space above the subject is above the scene band. | Subject plus empty upper region. |
| `head_cropped` | A head crosses a frame edge where the scene does not allow a tight crop. | Face/head evidence box at the edge. |
| `joint_cut` | The frame cuts through a neck, shoulder, elbow, wrist, hip, knee, or ankle. | Small box around the joint and named edge. |
| `limb_cut` | The frame cuts between two joints. | Small box on the limb and named edge. |
| `edge_intrusion` | A detected region enters from an edge and competes with the subject. | Intruding edge region. |
| `off_balance` | Saliency weight lies mainly on one side without a counterweight. | Frame-wide balance note. |
| `centred` | The subject is centred; in a symmetric/detail scene this is an allowed fact. | Subject box and centre guide. |
| `cluttered_background` | Background edge energy exceeds the scene's tolerance. | Background region around, not through, the subject. |
| `bright_blob` | A region brighter than the subject pulls attention. | Bright-region box. |
| `head_merge` | A strong near-vertical structure crosses above a detected head. | Head box plus intersecting structure. |
| `colour_competition` | Saturated background energy competes with the subject colour. | Competing background region when localisable. |
| `no_geometry` | No usable bodies or faces were available for body-dependent rules. | Frame-wide caveat; not a defect. |

## The 26 reason codes

The table is the public, localisable vocabulary. Text can change without changing stored
rows; the slug and meaning cannot. “Exoneration” means zero penalty and a visibly softer
card treatment.

| Code | Kind | Photographer-facing meaning |
|---|---|---|
| `clean` | exoneration | The frame reads level, placed, and uncluttered under the evidence available. |
| `horizon_tilted` | issue | A reliable horizon is farther off level than this scene normally carries. |
| `horizon_uncertain` | exoneration | No reliable horizon was found, so AURA makes no level claim. |
| `intentional_tilt` | exoneration | The angle is large and the subject/scene evidence makes it read as a deliberate dutch frame. |
| `verticals_converging` | issue | Architectural verticals lean together, usually from camera pitch. |
| `headroom_tight` | issue | The scene's expected space above the head is missing. |
| `headroom_excessive` | issue | The scene carries more empty space above the subject than expected. |
| `head_cropped` | issue | The frame cuts through the top of a head. |
| `deliberate_crop` | exoneration | A close-portrait rule allows the tight head crop as a normal choice. |
| `joint_cut` | issue | The edge cuts through a named joint; the card names the joint and edge. |
| `limb_cut` | issue | The edge cuts through a limb between joints. |
| `edge_intrusion` | issue | Something visibly enters from a frame edge. |
| `off_thirds` | issue | The subject is away from both centre and the scene's natural placement points. |
| `thirds_aligned` | exoneration | The subject is near a power point. |
| `centred` | exoneration | The scene's symmetry or detail rule supports a centred subject. |
| `off_balance` | issue | Visual weight collects on one side with no counterweight. |
| `balanced` | exoneration | Measured visual weight is distributed evenly. |
| `cluttered_background` | issue | Background edge energy is high for this scene. It does not identify a specific object. |
| `clean_background` | exoneration | The measured background is quiet around the subject. |
| `bright_blob` | issue | A region brighter than the subject competes for attention. |
| `head_merge` | issue | A background vertical appears to grow from a head. |
| `colour_competition` | issue | Strong saturated background colour competes with the subject. |
| `no_geometry` | exoneration | No usable body or face geometry existed, so crop/headroom/head-merge rules abstained. |
| `keypoints_unavailable` | exoneration | Faces were present but body keypoints were unavailable, so limb and joint crop rules abstained. |
| `no_rule` | exoneration | The scene had no measured row; neutral bands were used and confidence was reduced. |
| `aesthetic_unavailable` | exoneration | No trained aesthetic reading was available; the score is geometry/background evidence only. |

## How to read the score

`compositionScore` is 0 to 1 and is comparable only when `modelVer`, `analysisVer`, and
`rulesVer` match. `relativeComposition` is this frame's position among scored siblings in
the same moment, not among the whole wedding. `confidence` describes evidence coverage;
it is reduced for missing people, unknown scenes, uncertain horizons, or an unavailable
learned head.

The aesthetic value is bounded so it cannot erase a geometric problem or create a
delivery decision. The checked-in Phase 11 model artifacts are untrained architecture
fixtures; until trained provenance and photographer validation exist, the card explicitly
labels that component unavailable and the reference score is the documented substitute.

## What the background reading does not claim

Phase 11 measures edges, luminance, colour, and simple vertical structure. It does not yet
recognise an exit sign, bin, mirror, or rubbish as a semantic category. Phase 18's masks
will permit that re-validation, and phase 24 owns removal. A generic clutter or bright-box
note must never be rewritten as a named-object claim without that evidence.

## Dismissal and rollback

Dismiss removes one note from one photograph and records the exact flag. Re-analysis may
update all measurements but cannot silently restore that dismissed note. Turning the
feature off means not running the composition pass and hiding the card; no existing photo,
edit, or selection depends on it. Before rolling migration 11 back, export reviewed and
dismissed bits—the analysis can be recomputed, but a photographer's decisions cannot.

The authored examples and their known evidence live in
`tests/eval/composition_eval.rs`; they test overlay geometry and
algorithm regressions, not real-wedding quality.
