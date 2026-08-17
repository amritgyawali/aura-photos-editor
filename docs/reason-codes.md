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
| `mixed_light` | `caveat` | two different colours of light are falling on this scene, so one white balance                  will not correct all of it |
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
| `coverage_protected` | `credit` | part of the wedding's story that would otherwise be missing from the gallery |
| `diversity_cap` | `note` | the gallery already carries several frames very like this one |
| `diversity_spread` | `credit` | it shows something the other keepers from this moment do not |
| `identity_coverage` | `credit` | somebody who should be in the gallery is in very few other frames |
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

93 codes in total. See `docs/how-confidence-works.md` for what the number beside them means.
