# The reason codes

Every explanation AURA gives is one of the codes below. A code is stable, a sentence is not: the wording here is what the product says today and a release may improve it, but the code is what is stored, counted and translated against.

**Generated from the registry** by `cargo run --package aura-explain --example emit_reason_codes`. Do not edit by hand - `tests/eval/explain_eval.rs` asserts that this file documents every code the product can emit, so an edit here that disagrees with the code is a failing build rather than a wrong document.

## How to read the severity column

| Severity | What it means |
|---|---|
| `credit` | Something good about the photograph. It argued for the decision. |
| `note` | Neutral, or a suspicion explicitly cleared - a deliberate tilt, a blink at a kiss, a frame that simply lost to a better one. |
| `caveat` | **Something was not measured.** The decision was made with less than it wanted, and the confidence is lower for it. Never a fault: "AURA did not check this" and "AURA checked this and it is bad" are opposite sentences. |
| `fault` | Something is wrong with the photograph. It argued against. |

## Technical - whether the frame worked

Phase 09's vocabulary. Focus, motion, exposure, noise and eyes.

| Code | Severity | What AURA says |
|---|---|---|
| `back_focus` | `fault` | what is behind the subject is sharper than the subject is, which usually means                  the focus landed behind them |
| `camera_shake` | `fault` | the whole frame is smeared in one direction, which is the camera moving |
| `eyes_closed` | `fault` | their eyes are closed here, and nothing about this moment explains it |
| `eyes_closed_ok` | `note` | closed eyes belong to this moment, so this is not marked as a fault |
| `front_focus` | `fault` | something in front of the subject is sharper than the subject is, which usually                  means the focus landed short |
| `group_blink` | `fault` | several people in this group have their eyes closed at the same time |
| `heavy_noise` | `fault` | there is more noise here than this kind of photograph carries well |
| `highlight_lost` | `fault` | the highlights are clipped past what this camera can bring back |
| `highlight_recoverable` | `note` | some highlights are clipped, but within what this camera can bring back |
| `intentional_motion` | `note` | the blur here runs in one direction and the subject is held: this reads as a                  deliberate pan or drag rather than a mistake |
| `no_subject` | `caveat` | no face or person was found here, so the sharpness reading is about the whole                  frame rather than about a subject |
| `noise_within_scene` | `note` | this frame is noisy, and no noisier than this kind of photograph at this ISO                  usually is |
| `shadow_lost` | `fault` | the shadows are below this camera's noise floor at this ISO, so lifting them                  would show more noise than detail |
| `shallow_depth_of_field` | `note` | the background is soft and the subject is sharp: this reads as a deliberate                  shallow depth of field, not as a missed focus |
| `specular_highlight` | `note` | the brightest parts of this frame are lights rather than blown detail, so they                  are not counted against it |
| `squint` | `fault` | they are squinting here, which usually means the sun or a flash was in their eyes |
| `subject_motion` | `fault` | the subject is smeared while the background is not, so they moved during the                  exposure |
| `subject_soft` | `fault` | the subject is softer than this camera should manage in this kind of photograph |
| `uncalibrated` | `caveat` | AURA has not measured this camera model yet, so every reading here is a                  cautious one |

## Emotion - what the frame is worth

Phase 10's vocabulary. Expressions, interactions, gaze and peaks.

| Code | Severity | What AURA says |
|---|---|---|
| `action_of` | `credit` | another frame catches somebody reacting to this one |
| `composure` | `credit` | the faces here are composed, and in this part of the day that is what the moment asks for rather than an absence of feeling |
| `discomfort` | `credit` | a face here reads as awkward, or caught mid-word, which is usually a frame either side of a better one |
| `genuine_smile` | `credit` | somebody is smiling here, and it reads as unposed |
| `interaction_detected` | `credit` | something is happening between the people in this frame |
| `laughter` | `credit` | somebody is laughing here |
| `looking_at_camera` | `credit` | the subjects are looking at the lens |
| `mutual_gaze` | `credit` | two people are looking at each other here |
| `narrative_weight` | `caveat` | AURA asked a cloud model how important this moment is to the day's story, and this is what it said |
| `near_peak` | `credit` | this is close to the strongest frame of its moment |
| `no_faces` | `caveat` | no face was found here, so this reading is about the frame rather than about anybody in it |
| `off_peak` | `credit` | this frame is either side of the strongest one in its moment, which is often the run-up or the settle rather than a fault |
| `posed_smile` | `credit` | the smile here reads as held for the camera rather than caught, which is exactly right in a posed frame and less so in a candid one |
| `reaction_to` | `credit` | this frame is somebody reacting to something in another frame |
| `surprise` | `credit` | a face here reads as surprised |
| `tears` | `credit` | a face here reads as crying: wet eyes and the expression around them agree |
| `tenderness` | `credit` | the expression here reads as tender rather than as a smile |
| `unremarkable` | `credit` | nothing here stands out as a moment, which is not the same as nothing being wrong with it |

## Composition - how the frame is built

Phase 11's vocabulary. Horizon, crops, thirds, balance and background.

| Code | Severity | What AURA says |
|---|---|---|
| `aesthetic_unavailable` | `caveat` | the learned part of the composition judgement did not run here, so this score is geometry alone |
| `balanced` | `note` | the weight in this frame is spread evenly |
| `bright_blob` | `fault` | something brighter than the subject is pulling the eye away from them |
| `centred` | `note` | the subject is centred, which is how this kind of photograph is usually framed |
| `clean` | `credit` | the framing reads normally here: level, well placed and uncluttered |
| `clean_background` | `note` | the background behind the subject is quiet |
| `cluttered_background` | `fault` | there is more going on behind the subject than this kind of photograph carries well |
| `colour_competition` | `fault` | a strong colour behind the subject is competing with them |
| `deliberate_crop` | `note` | this is a close portrait, where cropping into the head is a normal choice rather than a mistake |
| `edge_intrusion` | `fault` | something is entering the frame at the edge |
| `head_cropped` | `fault` | the top of a head is cut off by the edge of the frame |
| `head_merge` | `fault` | a vertical line in the background runs straight out of somebody's head |
| `headroom_excessive` | `fault` | there is more empty space above their head than this kind of photograph usually leaves |
| `headroom_tight` | `fault` | there is less space above their head than this kind of photograph usually leaves |
| `horizon_tilted` | `fault` | the horizon is off level by more than this kind of photograph usually carries |
| `horizon_uncertain` | `caveat` | there is no clear horizon in this frame, so AURA has not made a claim about whether it is level |
| `intentional_tilt` | `note` | the tilt here is large, the subject is centred and the scene is one where a tilted frame is a choice: this reads as deliberate |
| `joint_cut` | `fault` | the frame cuts through a joint, which reads as a severed limb rather than as a limb continuing out of shot |
| `keypoints_unavailable` | `caveat` | body keypoints were unavailable, so AURA did not make limb or joint crop claims for this photograph |
| `limb_cut` | `fault` | the frame cuts through a limb between joints |
| `no_geometry` | `caveat` | no people were found here, so the rules about headroom, cropping and head merges were not applied |
| `no_rule` | `caveat` | AURA has no framing rules recorded for this kind of photograph yet, so it has judged this one cautiously against neutral ones |
| `off_balance` | `fault` | the visual weight is heavily on one side with nothing on the other to answer it |
| `off_thirds` | `fault` | the subject sits away from both the centre and the natural placement points |
| `thirds_aligned` | `note` | the subject sits close to one of the natural placement points |
| `verticals_converging` | `fault` | the vertical lines lean towards each other, which happens when the camera is pointed up or down at a building |

## Selection - whether the frame is delivered

Phase 12's vocabulary. What put a photograph in the gallery, and what kept it out.

| Code | Severity | What AURA says |
|---|---|---|
| `below_floor` | `note` | it is weaker than this kind of photograph usually needs to be |
| `chapter_full` | `note` | this part of the day was already well covered by stronger frames |
| `chapter_quota` | `credit` | this part of the day still had room in the gallery |
| `diversity_cap` | `note` | the gallery already carries several frames very like this one |
| `diversity_spread` | `credit` | it shows something the other keepers from this moment do not |
| `lost_moment_rank` | `note` | another frame of the same moment is stronger |
| `moment_winner` | `credit` | the strongest frame of this moment |
| `near_duplicate` | `note` | effectively the same photograph as one already in the gallery |
| `no_moment` | `caveat` | this photograph was not grouped with any others, so it was judged on its own |
| `no_scene` | `caveat` | AURA does not know what kind of photograph this is, so it was judged cautiously |
| `not_analysed` | `caveat` | AURA has not checked this photograph yet, so it has not been considered - this is not a judgement about it |
| `only_candidate` | `credit` | the only frame there was |
| `peak_frame` | `credit` | the moment of this sequence where the most was happening |
| `peak_rejected` | `note` | this was the strongest instant of its moment, and a different frame of the same instant was delivered instead |
| `runner_up` | `note` | the closest alternative to the frame that was delivered |
| `size_target` | `credit` | the best frame left when the gallery was still short of its size |
| `size_trim` | `note` | it was the weakest frame left when the gallery reached its size |
| `user_kept` | `credit` | you asked for this one to be kept |
| `user_rejected` | `note` | you asked for this one to be left out |
| `veto_exposure_lost` | `fault` | the exposure cannot be brought back in editing |
| `veto_eyes_closed` | `fault` | the main subject's eyes are closed and nothing in the moment explains it |
| `veto_out_of_focus` | `fault` | the subject is not in focus |

## Develop - how the photograph was treated

Phases 15 and 16's vocabularies. What the light was, what was done about it, and what the grade did to skin. Added to this reference in phase 30, when the learning loop needed a develop decision to attribute a correction to.

| Code | Severity | What AURA says |
|---|---|---|
| `backlit_subject` | `note` | the light is behind them, so AURA exposed for their faces and let the background go bright on purpose |
| `band_unconfident` | `note` | AURA was not sure enough about what one of these colours belongs to, so it left it alone |
| `blacks_set` | `note` | the black point was placed on the darkest real tone in the frame |
| `camera_as_shot_used` | `note` | AURA could not work out the colour of the light, so it kept the setting your camera recorded |
| `clipping_guard_resolved` | `note` | one setting was reduced, because the grade would have clipped more of this frame than this kind of photograph accepts |
| `coloured_light_partial` | `note` | the lighting here is deliberately coloured, so AURA corrected it only far enough to keep skin looking like skin |
| `coloured_light_preserved` | `note` | the lighting here is deliberately coloured, so AURA left it coloured rather than correcting it to white |
| `content_inferred` | `note` | the greenery, sky and decor here were worked out from colour rather than outlined, so the colour adjustments are deliberately small |
| `contrast_added` | `note` | contrast was added, because this kind of photograph is usually a little more separated than the camera recorded it |
| `contrast_reduced` | `note` | contrast was reduced, because the light here was already harder than this kind of photograph carries |
| `curve_fitted` | `note` | a tone curve was drawn to give the people in this frame the separation this kind of photograph wants |
| `curve_flattened_for_clipping` | `note` | the curve was made gentler than intended, because the stronger one would have clipped the brightest areas |
| `curve_identity` | `note` | this frame did not need a curve |
| `distractor_tamed` | `note` | the most distracting colour in the frame was reduced, rather than the whole frame being desaturated |
| `dominant_illuminant_chosen` | `note` | more of this photograph is lit by one light, but the people are lit by another, and AURA set the colour for the people |
| `dress_protected` | `note` | the bright near-white areas were kept neutral rather than picking up a colour cast |
| `exposure_held_for_highlights` | `note` | the faces are still a little dark, because lifting them further would have blown out the bright areas behind them |
| `exposure_held_for_shadows` | `note` | the exposure was not brought down as far as usual, because the shadows would have become noisier than this kind of photograph carries |
| `exposure_unavailable` | `note` | AURA could not measure anything useful here, so it left the exposure alone |
| `flash_detected` | `note` | flash was used, so the colour was set for the flash on the people rather than for the room behind them |
| `greenery_tamed` | `note` | the foliage was brought back from yellow-green toward green and calmed down a little |
| `grey_world_fallback` | `note` | there was nothing white and no recognised faces here, so the colour was set from the photograph's own average - treat it as a starting point |
| `highlights_recovered` | `note` | the brightest areas were pulled back so the detail in them survives |
| `hsl_neutral` | `note` | no colour in this frame needed adjusting |
| `hue_conflict_found` | `note` | two strong colours here were competing with the people in the frame |
| `mixed_light` | `note` | there are two different-coloured lights in this photograph. AURA has set the colour for the light on the people; the rest can be corrected separately |
| `mood_preserved` | `note` | this is meant to be a dark photograph, so AURA left it dark rather than brightening it to an average |
| `neutral_found` | `note` | something in this photograph is known to be white or grey, and the colour was set from it |
| `no_face_scene_model` | `note` | there are no faces in this photograph, so the exposure was set from what this kind of scene usually wants |
| `no_intent_row` | `note` | AURA has no grading guidance recorded for this kind of photograph yet, so it graded cautiously |
| `no_skin_in_frame` | `note` | there is nobody in this frame, so the skin protection did not apply |
| `no_target_row` | `note` | AURA has no exposure guidance recorded for this kind of photograph yet, so it has used a cautious neutral setting |
| `reference_anchor` | `note` | this photograph is one of the reference frames for this part of the day; the rest of the section will be matched to it |
| `shadow_lift_held_for_noise` | `note` | the shadows were opened less than usual, because lifting them further would have made this frame noisy |
| `shadows_lifted` | `note` | the darkest areas were opened up |
| `skin_anchored` | `note` | the colour was chosen to make the skin here match how it looks in the rest of this wedding |
| `skin_guard_resolved` | `note` | the first grade moved skin further than AURA allows, so it was worked out again more gently |
| `skin_guard_withdrew` | `note` | the colour adjustments were dropped entirely, because there was no version of them that left skin where it was |
| `skin_locus_constrained` | `note` | a warmer or cooler setting would have made somebody's skin an implausible colour, so it was ruled out |
| `skin_protected` | `note` | every colour adjustment was held back over skin, so nobody's colour moved |
| `sky_deepened` | `note` | the sky was deepened slightly |
| `subject_exposed` | `note` | the faces here were already at the brightness this kind of photograph wants |
| `subject_overexposed` | `note` | the faces here were brighter than this kind of photograph wants, so the exposure was brought down |
| `subject_underexposed` | `note` | the faces here were darker than this kind of photograph wants, so the exposure was lifted |
| `subtlety_capped` | `note` | the whole grade was scaled back, because everything together would have looked processed |
| `tone_already_right` | `note` | this frame was already at the contrast this kind of photograph wants, so the tone was left where it was |
| `tone_unavailable` | `note` | AURA could not measure this frame well enough to grade it, so it was left as it was |
| `tungsten_reception` | `note` | this room is lit by warm bulbs. AURA has kept some of that warmth rather than making the whole room look blue |
| `wb_low_confidence` | `note` | AURA is not confident about the colour here. It is worth a look |
| `whites_set` | `note` | the white point was placed on the brightest real tone in the frame |
| `wood_warmed` | `note` | the wood and warm surfaces were brought toward each other |

## Curation - what to show

Phase 29's vocabulary. Monochrome, portfolio, album sequence, social sets and captions.

| Code | Severity | What AURA says |
|---|---|---|
| `aspect_variant_absent` | `note` | no safe crop at this shape exists, so it is posted as it was shot |
| `aspect_variant_available` | `note` | a safe crop at this shape exists |
| `caption_grounded` | `note` | every word here came from this wedding's own labels |
| `caption_refused` | `note` | a suggested caption said something this wedding did not supply |
| `chapter_quota_binding` | `note` | stronger frames were passed over because that part of the day already had four |
| `chapter_under_allocated` | `note` | this part of the day has fewer pages than planned, because there were not enough frames |
| `cloud_move_applied` | `note` | a suggested move that made the sequence read better |
| `cloud_move_refused` | `note` | a suggested move that would have made the sequence worse |
| `colour_distraction` | `note` | colour away from the subject is pulling the eye here |
| `colour_is_the_subject` | `note` | the colour is the subject here |
| `coverage_protected` | `note` | in the album because it is the only frame of this moment |
| `emotional_peak` | `note` | the peak of its moment |
| `facing_near_duplicate_refused` | `note` | AURA would not put two versions of the same shot opposite each other |
| `flat_when_desaturated` | `note` | without the colour this frame goes flat |
| `gesture_led` | `note` | this frame is carried by what people are doing |
| `grain_tolerant` | `note` | the noise in this frame would read as grain |
| `high_emotion` | `note` | the moment is strong enough that the colour is not the point |
| `identity_coverage` | `note` | in the album because somebody close to the couple would not otherwise appear |
| `mix_bounded` | `note` | the mix wanted to go further and AURA stopped it |
| `moment_already_represented` | `note` | that shot is already in the set |
| `rhythm_improved` | `note` | moved here because the sequence reads better |
| `rhythm_unmeasurable` | `note` | AURA could not tell how close this frame is, so it does not count toward the rhythm |
| `scale_quota_binding` | `note` | chosen to keep the set from being all close-ups |
| `single_spread` | `note` | this page stands alone |
| `skin_locus_unavailable` | `note` | AURA has not measured anybody's skin in this wedding yet, so the mix protects nobody in particular |
| `skin_separation_held` | `note` | the mix was held back so skin barely moved |
| `slot_unfilled` | `note` | there was nothing in this wedding for this slot |
| `spread_facing_unknown` | `note` | AURA could not tell which way anybody is facing on this spread |
| `spread_paired` | `note` | these two work together across the fold |
| `spread_tonal_gap` | `note` | one of these is a good deal darker than the other |
| `story_important` | `note` | this moment matters to the story of the day |
| `strong_composition` | `note` | the framing is the strongest thing about it |
| `strong_tonal_separation` | `note` | the light and dark in this frame stay far apart without the colour |
| `technical_excellence` | `note` | sharp where it matters, and cleanly exposed |
| `technical_veto` | `note` | not sharp enough for portfolio work |
| `thumbnail_legible` | `note` | this reads clearly at thumbnail size |
| `unique_frame` | `note` | unlike anything else already in the set |
| `uniqueness_unavailable` | `note` | AURA could not tell how similar this is to the rest, so it did not count either way |
| `user_ordered` | `note` | you set this order, and AURA has left it alone |

## Quality control - what AURA found wrong with its own work

Phase 27's vocabulary. Every one of these is a caveat by construction: a QC finding is a fault the product found in itself, and phase 27's vocabulary carries no positive code.

| Code | Severity | What AURA says |
|---|---|---|
| `allowance_exceeded` | `caveat` | more local adjustments were spent on this frame than its budget |
| `budget_spent` | `caveat` | the quality pass ran out of time before reaching this photograph |
| `check_skipped` | `caveat` | AURA could not check this, so it has not made a claim either way |
| `cleanup_artefact` | `caveat` | a removal left a visible mark |
| `cleanup_undisclosed` | `caveat` | a removal reached the gallery without a record of it |
| `clipping_introduced` | `caveat` | the edit clipped highlights or shadows the original still had |
| `collateral_damage` | `caveat` | the correction fixed this and made something else worse, so it was put back |
| `consistency_drift` | `caveat` | this frame's colour sits outside the rest of its lighting group |
| `contradictory_findings` | `caveat` | two findings on this frame cannot both be fixed |
| `coverage_missing` | `caveat` | a moment the gallery has to include is not covered |
| `coverage_weak` | `caveat` | a moment is covered only by a photograph that did not work |
| `crop_content_lost` | `caveat` | the delivered crop drops more of the frame than the rules allow |
| `crop_resolution_low` | `caveat` | the delivered crop is smaller than this use needs |
| `crop_unsafe` | `caveat` | the delivered crop cuts something it should not |
| `duplicate_leak` | `caveat` | two nearly identical frames are both in the gallery |
| `escalated_to_human` | `caveat` | this one needs your eyes |
| `exposure_regression` | `caveat` | the finished frame is brighter or darker than this kind of scene should be |
| `identity_drift` | `caveat` | a recovered face moved further from the person than AURA permits |
| `identity_under_covered` | `caveat` | somebody appears fewer times than the gallery guarantees |
| `mask_edge_artefact` | `caveat` | an adjustment shows along the edge of the region it was applied to |
| `mask_quality_low` | `caveat` | a region was not determined well enough for what was done inside it |
| `mask_uncovered` | `caveat` | a local adjustment ran with no region behind it |
| `multi_symptom` | `caveat` | this frame has more problems than one correction can fix |
| `naturalness_missed` | `caveat` | the retouching on this frame reads as worked on |
| `planner_refused` | `caveat` | the second opinion proposed something AURA is not allowed to do, so it was ignored |
| `planner_unavailable` | `caveat` | the second opinion was unavailable, so AURA used its own ordering |
| `remedy_applied` | `caveat` | AURA corrected this and checked the correction worked |
| `remedy_refused_by_policy` | `caveat` | the correction this needs is not one AURA is allowed to make here |
| `remedy_reverted` | `caveat` | AURA tried a correction, it did not help, and it was put back |
| `replaced_with_runner_up` | `caveat` | AURA delivered a better frame from the same moment instead |
| `replacement_breaks_coverage` | `caveat` | a better frame exists but swapping to it would leave a moment uncovered |
| `ringing_detected` | `caveat` | sharpening left a bright halo along an edge |
| `rounds_exhausted` | `caveat` | AURA tried twice and this still needs your eyes |
| `runner_up_absent` | `caveat` | there is no alternative frame: this moment was photographed once |
| `runner_up_not_better` | `caveat` | the alternative frame is not clearly better than this one |
| `shadows_crushed` | `caveat` | the edit lost shadow detail the original held |
| `sharpness_below_floor` | `caveat` | the subject is softer than this kind of photograph needs |
| `signature_drift` | `caveat` | this frame is graded differently from its reference frames |
| `skin_drift` | `caveat` | somebody's skin here does not match how they look elsewhere in the gallery |
| `skin_guard_exceeded` | `caveat` | the grade moved skin further than AURA allows |
| `texture_floor_missed` | `caveat` | the skin here has less texture than AURA's floor allows |
| `texture_lost` | `caveat` | noise reduction took more texture than it should have |
| `user_edited` | `caveat` | you set this yourself, so AURA has left it alone |

## The record itself

Phase 13's own codes. These are facts about the decision rather than about the photograph.

| Code | Severity | What AURA says |
|---|---|---|
| `raised_for_irreversible` | `caveat` | this is not easy to undo, so AURA asked for a closer look than the confidence alone would need |
| `raised_for_must_have` | `caveat` | this touches a part of the day the gallery may not be missing, so AURA asked for a closer look than the confidence alone would need |
| `supersedes_earlier` | `note` | this replaces an earlier decision about the same thing, which is still on record |
| `uncalibrated_confidence` | `caveat` | AURA has not yet learned how often it is right about this kind of decision, so read the confidence as a rough guide |
| `user_override` | `note` | you decided this one yourself, and AURA has kept your decision |

---

223 codes in total. See `docs/how-confidence-works.md` for what the number beside them means.
