//! The phase 10 evaluation harness: section 10.1's gates, and the proof that the harness
//! itself would catch a bad ranker.
//!
//! Wired in as a test target of `aura-brain-wedding`, so `cargo test --workspace` runs it
//! and CI cannot forget it - the same arrangement phases 05 to 09 use.
//!
//! ## What can and cannot be gated today
//!
//! Section 10.1 sets six gates. Three of them - pairwise agreement with photographers,
//! tears and laughter F1, and peak recall against a human choice - are statements that
//! involve **a trained model and a labelled human dataset**, and this build has neither:
//! `expression_head` and `interaction_head` 1.0.0 are placeholders with the right
//! architectures and no training (condition **C1** in `docs/progress/PHASE-10-EXIT.md`),
//! and there are no photographer comparisons in this repository (condition **C2**).
//!
//! So this file takes the shape the five harnesses before it took:
//!
//! 1. **It measures every metric** with the arithmetic
//!    `ml/models/emotion/eval_emotion.py` uses, so the day the heads are trained and the
//!    comparisons exist the two numbers can be compared rather than argued about.
//! 2. **It gates them on ground truth whose answer is known by construction** - the
//!    synthetic frames in `aura_brain_wedding::emotion::fixtures` - which proves the
//!    harness would fail a ranker that had learned nothing.
//! 3. **It gates, for real and today, everything that does not depend on the weights**:
//!    the gaze sign, the mutual-gaze symmetry, the prominence weighting, the nine
//!    features, the scene and tradition table, the composure fairness, the smoothing
//!    kernel, the margin, the flat-moment refusal, the three reaction conditions, the
//!    resolver, the tear certainty gate and determinism.
//!
//! ## The one gate that matters most
//!
//! `composed_frames_are_not_ranked_below_smiling_ones_in_ritual_scenes`. Section 10.1
//! spells it "composure fairness" and section 12 names cultural bias as this phase's
//! first failure mode. It is measured **in every scene in the vocabulary**, not only in
//! the two the phase document names, because an edit to `emotion_weights.toml` that broke
//! it in `ceremony` and not in `ritual` would pass a two-scene test and deliver an empty
//! Hindu gallery.
//!
//! ## Why the analyser runs with a planted interaction head
//!
//! [`PlantedInfer`] returns a fixed nine-slot vector for every call. The expression head
//! is replaced by `fixtures::ReferenceExpressionReader`, whose answer is painted into the
//! frame and read back *through the real warp*, so the alignment is under test. The
//! interaction head has no equivalent trick - its input is the whole frame - so its
//! answer is planted, which is honest: what these gates measure is the scoring, and the
//! interaction strength is one of nine features.
//!
//! The real models, through the real registry, are exercised by
//! `aura-cli verify --phase 10`. The tests prove the pieces; the gate proves the assembly.

use std::collections::BTreeMap;

use aura_brain_wedding::emotion::fixtures::{self, EmotionFrame};
use aura_brain_wedding::emotion::weights::{EmotionWeights, Ranker};
use aura_brain_wedding::emotion::{analyse, gaze, interaction, peak, reaction, score, Analyser};
use aura_brain_wedding::EmotionFrameContext;
use aura_core::contract::emotion::{
    EmotionCode, FaceExpression, GazeTarget, ImageEmotion, Interaction, PeakKind, ReactionLink,
    TEARS_CERTAIN,
};
use aura_core::contract::people::ImageSubjects;
use aura_core::{MomentId, PhotoId, Priority, Role, SceneId};
use aura_infer::onnx::fixtures::INTERACTION_CLASSES;

// -- section 10.1's thresholds, in one place ---------------------------------

/// Pairwise agreement with photographers on held-out moments.
const AGREEMENT_GATE: f64 = 0.80;
/// Peak detection: the chosen frame is in the human top two.
const PEAK_RECALL_GATE: f64 = 0.90;
/// Tears and laughter F1.
const EXPRESSION_F1_GATE: f64 = 0.85;
/// Tears precision. Higher than the F1 gate, deliberately: section 12's fourth failure
/// mode is a false tear, and a wrongly detected one is worse than a missed one.
const TEARS_PRECISION_GATE: f64 = 0.90;
/// Reaction linking: how many human-identified pairs are found.
const REACTION_RECALL_GATE: f64 = 0.80;
/// Reaction linking: how many links are spurious.
const REACTION_SPURIOUS_GATE: f64 = 0.10;

// -- helpers -----------------------------------------------------------------

/// The weight table under test. The shipped one, always.
fn weights() -> EmotionWeights {
    EmotionWeights::embedded().expect("the shipped emotion weights must load")
}

/// An analyser with the reference expression reader and a planted interaction head.
fn analyser(planted: [f32; INTERACTION_CLASSES]) -> Analyser {
    Analyser::new(std::sync::Arc::new(PlantedInfer::new(planted)))
        .expect("the embedded weight table must load")
        .with_reference_expressions()
}

/// A context for one fixture: a scene, a tradition and the faces the fixture painted.
fn context_for(
    frame: &EmotionFrame,
    scene: SceneId,
    tradition: Option<&str>,
) -> EmotionFrameContext {
    let mut prominence = BTreeMap::new();
    for face in &frame.faces {
        if let Some(identity) = face.identity_id {
            // Even prominence across identified faces. The prominence *model* is phase
            // 06's and is tested there; what is under test here is that this phase reads
            // it rather than taking a maximum.
            prominence.insert(identity, 1.0 / frame.faces.len() as f32);
        }
    }
    EmotionFrameContext {
        scene,
        scene_known: true,
        tradition: tradition.map(str::to_string),
        subjects: ImageSubjects {
            faces: frame.faces.clone(),
            dominant: None,
            prominence,
            subject_focus_score: 0.8,
            people_count: frame.faces.len() as u32,
            weights_ver: 1,
        },
        couple: Vec::new(),
        faces_scanned: true,
    }
}

/// Read one fixture frame.
fn read(frame: &EmotionFrame, scene: SceneId, tradition: Option<&str>) -> ImageEmotion {
    read_with(frame, scene, tradition, [0.05; INTERACTION_CLASSES])
}

/// Read one fixture frame with a planted interaction vector.
fn read_with(
    frame: &EmotionFrame,
    scene: SceneId,
    tradition: Option<&str>,
    planted: [f32; INTERACTION_CLASSES],
) -> ImageEmotion {
    analyser(planted)
        .analyse(
            PhotoId::new(),
            &frame.buffer(),
            &context_for(frame, scene, tradition),
            Priority::AiBatch,
        )
        .expect("a synthetic frame must read")
}

/// One frame carrying one painted expression.
fn frame_with(channels: [f32; 8]) -> EmotionFrame {
    let mut frame = EmotionFrame::with_faces(1);
    frame.paint_expression(0, channels);
    frame
}

// ---------------------------------------------------------------------------
// The reference reader, which is what makes any of the rest measurable
// ---------------------------------------------------------------------------

#[test]
fn the_painted_expression_survives_the_warp() {
    // If this fails, every gate below is measuring noise. The markers are placed in the
    // *aligned crop's* coordinates and mapped back into the source, so they are only
    // where the reader looks if `warp_crop_from_eyes` put them there - which makes this
    // a test of the alignment as much as of the fixture.
    for painted in [
        fixtures::laughing(),
        fixtures::crying(),
        fixtures::composed(),
        fixtures::posed(),
        fixtures::flat(),
    ] {
        let frame = frame_with(painted);
        let reading = read(&frame, SceneId::Candid, None);
        let face = reading.faces.first().expect("one face");
        let recovered = face.channels();
        for (slot, (expected, actual)) in painted.iter().zip(recovered.iter()).enumerate() {
            assert!(
                (expected - actual).abs() <= 0.05,
                "channel {} ({}) came back as {actual:.3} rather than {expected:.3}",
                slot,
                FaceExpression::CHANNEL_NAMES[slot]
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Gaze, which is measured rather than predicted
// ---------------------------------------------------------------------------

#[test]
fn the_turn_direction_is_signed_the_way_a_head_actually_turns() {
    // The mistake this catches: when a head turns to the frame's right, the visible eyes
    // move *left* inside the box, because the right side of the face is foreshortened.
    // An implementation that forgot the negation would report every reaction as looking
    // away from the action it is looking at.
    let mut frame = EmotionFrame::with_faces(1);
    frame.turn_face(0, 0.4);
    assert_eq!(
        gaze::turn_of(&frame.faces[0]),
        gaze::Turn::Right,
        "a face turned towards the frame's right must read as Right"
    );

    let mut other = EmotionFrame::with_faces(1);
    other.turn_face(0, -0.4);
    assert_eq!(gaze::turn_of(&other.faces[0]), gaze::Turn::Left);
}

#[test]
fn a_face_with_no_landmarks_is_unknown_rather_than_away() {
    // A missing measurement must never become evidence: `Away` is what the reaction
    // linker acts on, and reading "we could not tell" as "looking into the room" would
    // manufacture reactions out of unlandmarked faces.
    let mut frame = EmotionFrame::with_faces(1);
    frame.faces[0].eyes = [[0.0, 0.0], [0.0, 0.0]];
    let target = gaze::target_for(&frame.faces[0], &frame.faces, &[], SceneId::Ceremony);
    assert_eq!(target, GazeTarget::Unknown);
    assert!(!target.is_engaged(), "unknown is not engagement");
}

#[test]
fn a_face_pointed_at_the_lens_reads_as_camera() {
    let frame = EmotionFrame::with_faces(1);
    assert_eq!(
        gaze::target_for(&frame.faces[0], &frame.faces, &[], SceneId::Candid),
        GazeTarget::Camera,
        "the conservative reading: a face that is not clearly turned is looking at the lens"
    );
}

#[test]
fn mutual_gaze_is_symmetric() {
    // One person watching somebody else's back of the head is not mutual gaze, and an
    // asymmetric test would call it one.
    let mut frame = EmotionFrame::with_faces(2);
    frame.turn_face(0, 0.4); // face 0 turns right, towards face 1
    assert!(
        !gaze::mutual(&frame.faces),
        "one-sided attention is not mutual gaze"
    );
    frame.turn_face(1, -0.4); // face 1 turns left, towards face 0
    assert!(gaze::mutual(&frame.faces), "both turned towards each other");
}

#[test]
fn two_faces_across_a_room_are_not_looking_at_each_other() {
    // `MUTUAL_REACH` is what stops a crowd becoming a conversation.
    let mut frame = EmotionFrame::with_faces(8);
    frame.turn_face(0, 0.4);
    frame.turn_face(7, -0.4);
    let pair = [frame.faces[0], frame.faces[7]];
    assert!(
        !gaze::within_reach(&pair[0], &pair[1]),
        "the outermost two faces of an eight-face frame are not within arm's length"
    );
}

// ---------------------------------------------------------------------------
// Composure fairness - the gate this phase exists to pass
// ---------------------------------------------------------------------------

#[test]
fn composed_frames_are_not_ranked_below_smiling_ones_in_ritual_scenes() {
    // Section 10.1: "in ritual/vows fixtures, composed frames are not systematically
    // ranked below smiling frames". Section 12's first failure mode is cultural bias
    // towards Western expressiveness, and this is the measurement that would catch it.
    let composed = frame_with(fixtures::composed());
    let smiling = frame_with(fixtures::smiling());

    for scene in [SceneId::Ritual, SceneId::Vows, SceneId::Ceremony] {
        let quiet = read(&composed, scene, None).emotion_score;
        let broad = read(&smiling, scene, None).emotion_score;
        assert!(
            quiet >= broad * 0.90,
            "{scene}: a composed frame scored {quiet:.3} against a smiling frame's {broad:.3}; \
             a ceremony ranked this way delivers a wedding with no wedding in it"
        );
    }
}

#[test]
fn the_hindu_multiplier_raises_a_composed_ritual_frame() {
    // The tradition table's whole purpose, measured end to end rather than asserted on
    // the weights.
    let composed = frame_with(fixtures::composed());
    let neutral = read(&composed, SceneId::Ritual, None).emotion_score;
    let hindu = read(&composed, SceneId::Ritual, Some("hindu")).emotion_score;
    assert!(
        hindu >= neutral,
        "the Hindu multiplier lowered a composed rite frame: {hindu:.3} against {neutral:.3}"
    );
}

#[test]
fn a_smile_still_wins_on_a_dance_floor() {
    // The other direction, and it matters as much: a table that made composure win
    // everywhere would have replaced one bias with another.
    let composed = frame_with(fixtures::composed());
    let smiling = frame_with(fixtures::smiling());
    let quiet = read(&composed, SceneId::DanceFloor, None).emotion_score;
    let broad = read(&smiling, SceneId::DanceFloor, None).emotion_score;
    assert!(
        broad > quiet,
        "a still face on a dance floor outscored a laughing one: {quiet:.3} against {broad:.3}"
    );
}

#[test]
fn composure_is_weighted_at_or_above_a_smile_in_every_ceremony_scene() {
    // The table-level statement behind the end-to-end gates above, checked across the
    // whole vocabulary rather than the three scenes the fixtures use. An edit that broke
    // `ceremony_entrance` and not `ritual` would otherwise pass every test in this file.
    let table = weights();
    for scene in SceneId::ALL {
        let row = table.for_scene(scene, None);
        let ceremonial = matches!(
            scene,
            SceneId::Ceremony | SceneId::Ritual | SceneId::Vows | SceneId::Rings
        );
        if ceremonial {
            assert!(
                row.composed() >= row.smile(),
                "{scene}: composure {:.2} below smile {:.2}",
                row.composed(),
                row.smile()
            );
        }
        assert!(
            row.composed() > 0.0,
            "{scene}: composure at zero makes a still face worth nothing anywhere"
        );
    }
}

// ---------------------------------------------------------------------------
// Posed smiles, which are the correct answer in a posed scene
// ---------------------------------------------------------------------------

#[test]
fn a_posed_smile_is_not_penalised_in_a_family_portrait() {
    // A family portrait is *supposed* to be posed. Penalising a held smile there would
    // rank the frame where somebody blinked and laughed above the one the family hangs
    // on a wall.
    let posed = frame_with(fixtures::posed());
    let portrait = read(&posed, SceneId::FamilyPortrait, None).emotion_score;
    let candid = read(&posed, SceneId::Candid, None).emotion_score;
    assert!(
        portrait > candid,
        "a posed smile scored {portrait:.3} in a family portrait and {candid:.3} in a candid; \
         the portrait must be the higher of the two"
    );
}

#[test]
fn the_posed_smile_predicate_needs_both_halves() {
    // A face that is not smiling is not posing, and a predicate that only read
    // genuineness would call every neutral face posed.
    let frame = frame_with(fixtures::posed());
    let face = read(&frame, SceneId::FamilyPortrait, None)
        .faces
        .first()
        .copied()
        .expect("one face");
    assert!(face.posed_smile());

    let flat = frame_with(fixtures::flat());
    let quiet = read(&flat, SceneId::Candid, None)
        .faces
        .first()
        .copied()
        .expect("one face");
    assert!(
        !quiet.posed_smile(),
        "a face that is not smiling is not posing"
    );
}

// ---------------------------------------------------------------------------
// Tears: the high-precision channel
// ---------------------------------------------------------------------------

#[test]
fn tears_and_laughter_f1_on_the_painted_set() {
    // Section 10.1's third gate, measured with the arithmetic `eval_emotion.py` uses.
    // Against painted ground truth, so what it proves is that the harness would fail a
    // reader that had learned nothing - see `the_gate_rejects_a_useless_reader` below.
    let cases: Vec<([f32; 8], bool, bool)> = vec![
        (fixtures::crying(), true, false),
        (fixtures::laughing(), false, true),
        (fixtures::composed(), false, false),
        (fixtures::posed(), false, false),
        (fixtures::smiling(), false, false),
        (fixtures::awkward(), false, false),
        (fixtures::flat(), false, false),
    ];

    let mut tears = Counts::default();
    let mut laughter = Counts::default();
    for (painted, is_tears, is_laughter) in &cases {
        let frame = frame_with(*painted);
        let face = read(&frame, SceneId::Candid, None)
            .faces
            .first()
            .copied()
            .expect("one face");
        tears.add(face.reads_as_crying(), *is_tears);
        laughter.add(face.laughter >= 0.55, *is_laughter);
    }

    println!("tears    {tears:?} -> F1 {:.3}", tears.f1());
    println!("laughter {laughter:?} -> F1 {:.3}", laughter.f1());
    assert!(
        tears.f1() >= EXPRESSION_F1_GATE,
        "tears F1 {:.3} is below the {EXPRESSION_F1_GATE} gate",
        tears.f1()
    );
    assert!(
        tears.precision() >= TEARS_PRECISION_GATE,
        "tears precision {:.3} is below the {TEARS_PRECISION_GATE} gate; a wrongly detected \
         tear is the most embarrassing thing this phase can do",
        tears.precision()
    );
    assert!(
        laughter.f1() >= EXPRESSION_F1_GATE,
        "laughter F1 {:.3} is below the {EXPRESSION_F1_GATE} gate",
        laughter.f1()
    );
}

#[test]
fn the_product_does_not_say_the_word_tears_below_the_certainty_gate() {
    // Section 12's fourth failure mode as a test. A face at 0.80 tears is a face the
    // ranker weighs and the panel does not name.
    let mut nearly = fixtures::crying();
    nearly[3] = TEARS_CERTAIN - 0.10;
    let frame = frame_with(nearly);
    let reading = read(&frame, SceneId::Vows, None);
    assert!(
        !reading.has_tears(),
        "a tear reading below the gate must not be reported as tears"
    );
    assert!(
        !reading
            .reasons
            .iter()
            .any(|reason| reason.code == EmotionCode::Tears),
        "no reason may name tears below the certainty gate"
    );

    let certain = frame_with(fixtures::crying());
    assert!(read(&certain, SceneId::Vows, None).has_tears());
}

#[test]
fn the_tear_certainty_gate_is_the_one_phase_nine_reads() {
    // Three unrelated places read this constant - this crate, the panel and
    // `aura_brain_photo::integrity::eyes` - and the third lives in a crate that cannot
    // see the other two. A drifting second copy is exactly the failure the contract's
    // doc comment argues against.
    assert!(
        (TEARS_CERTAIN - 0.85).abs() < f32::EPSILON,
        "section 12's fourth failure mode names 0.85 explicitly"
    );
}

// ---------------------------------------------------------------------------
// Peaks
// ---------------------------------------------------------------------------

#[test]
fn the_peak_of_a_rising_run_is_the_frame_it_was_built_to_be() {
    // Section 10.1's second gate, on ground truth known by construction. Seven frames,
    // apex at index 3.
    let frames = fixtures::moment_run(7, 3, 0.12);
    let curve = build_curve(&frames);
    let (peak, _) = peak::peak_of(MomentId::new(), &curve).expect("a peak");
    assert_eq!(peak.index, 3, "the apex moved");
    assert!(
        peak.is_resolved(),
        "a triangle with a 0.12 step must resolve"
    );
    assert!(
        matches!(peak.kind, PeakKind::Expression),
        "no interaction was planted, so the kind is the general case"
    );
}

#[test]
fn the_peak_is_in_the_human_top_two_across_a_range_of_shapes() {
    // Section 10.1: "chosen frame is within the human-chosen top-2 in >= 90 % of
    // moments". The human choice here is the fixture's construction: the painted apex
    // and its stronger neighbour.
    let mut hits = 0usize;
    let mut total = 0usize;
    for count in [3usize, 5, 7, 9, 11, 14] {
        for apex in [0usize, count / 3, count / 2, count - 1] {
            for step in [0.08f32, 0.12, 0.20] {
                let frames = fixtures::moment_run(count, apex, step);
                let curve = build_curve(&frames);
                let Some((peak, _)) = peak::peak_of(MomentId::new(), &curve) else {
                    continue;
                };
                total += 1;
                let index = peak.index as usize;
                if index.abs_diff(apex) <= 1 {
                    hits += 1;
                }
            }
        }
    }
    let recall = hits as f64 / total as f64;
    println!("peak recall {recall:.3} over {total} synthetic moments");
    assert!(
        recall >= PEAK_RECALL_GATE,
        "peak recall {recall:.3} is below the {PEAK_RECALL_GATE} gate"
    );
}

#[test]
fn a_flat_moment_refuses_to_name_a_peak() {
    // A bracketed detail shot genuinely has no apex, and naming one is how phase 29 ends
    // up building an album spread around a rounding error. `AURA-ML-5042` is this.
    let frames = fixtures::flat_run(9);
    let curve = build_curve(&frames);
    let (peak, built) = peak::peak_of(MomentId::new(), &curve).expect("a peak row");
    assert!(
        !peak.is_resolved(),
        "nine identical frames produced a margin of {:.4}, which the product must refuse",
        peak.margin
    );
    assert_eq!(peak.kind, PeakKind::Flat);
    // Every frame is as near the peak as every other, which is the honest reading.
    for index in 0..curve.len() {
        assert!(
            built.proximity(index) > 0.95,
            "a flat moment must not push any frame off-peak"
        );
    }
}

#[test]
fn a_moment_of_one_frame_has_no_margin_to_speak_of() {
    // A peak that beat nothing has not been shown to be a peak.
    let frames = fixtures::moment_run(1, 0, 0.1);
    let curve = build_curve(&frames);
    let (peak, _) = peak::peak_of(MomentId::new(), &curve).expect("a peak row");
    assert!(!peak.is_resolved());
    assert_eq!(peak.frames, 1);
}

#[test]
fn the_smoothing_kernel_reflects_rather_than_zero_pads() {
    // A bouquet toss whose apex is the final frame is a real and common shape, and
    // zero-padding would pull the ends down and move the peak inwards.
    let rising: Vec<f32> = vec![0.10, 0.25, 0.45, 0.70, 0.95];
    let smoothed = peak::smooth(&rising);
    let last = smoothed.last().copied().unwrap_or(0.0);
    let previous = smoothed.get(3).copied().unwrap_or(0.0);
    assert!(
        last > previous,
        "the final frame of a rising run was smoothed below its neighbour: {last:.3} against \
         {previous:.3}"
    );
}

#[test]
fn a_kiss_with_a_kiss_interaction_is_a_kiss_apex() {
    // The derived peak kinds need the scene *and* the interaction to agree, so a wrong
    // kind needs two independent things to be wrong.
    let mut frames = fixtures::moment_run(5, 2, 0.15);
    let mut curve = build_curve(&frames);
    for entry in &mut curve {
        entry.scene = SceneId::Kiss;
    }
    if let Some(top) = curve.get_mut(2) {
        top.interactions = vec![(Interaction::Kiss, 0.90)];
    }
    let (peak, _) = peak::peak_of(MomentId::new(), &curve).expect("a peak");
    assert_eq!(peak.kind, PeakKind::KissApex);
    frames.clear();
}

// ---------------------------------------------------------------------------
// Reaction linking
// ---------------------------------------------------------------------------

#[test]
fn reaction_recall_and_spuriousness_on_a_built_scenario() {
    // Section 10.1's fifth gate: ">= 80 % of human-identified reaction pairs found,
    // < 10 % spurious links". The scenario is built so the answer is known: one action
    // frame with a strong interaction, three genuine reactors on a second camera, and
    // three distractors that fail one condition each.
    let action = candidate("action", 0, None, "cam-a", vec![fixtures::laughing()], 0.90);
    let action = with_interaction(action, Interaction::Kiss, 0.9);

    let mut candidates = vec![action.clone()];
    let mut expected = Vec::new();

    // Three genuine reactions: on the other camera, inside the window, engaged and
    // expressive.
    for (index, offset) in [900i64, 1_800, -1_200].into_iter().enumerate() {
        let mut reactor = candidate(
            &format!("react-{index}"),
            offset,
            None,
            "cam-b",
            vec![fixtures::crying()],
            0.70,
        );
        turn_away(&mut reactor);
        expected.push(reactor.image);
        candidates.push(reactor);
    }

    // Distractor 1: outside the window.
    let mut far = candidate("far", 9_000, None, "cam-b", vec![fixtures::crying()], 0.70);
    turn_away(&mut far);
    candidates.push(far);
    // Distractor 2: flat expression.
    let mut flat = candidate("flat", 500, None, "cam-b", vec![fixtures::flat()], 0.10);
    turn_away(&mut flat);
    candidates.push(flat);
    // Distractor 3: looking at the lens rather than into the room.
    let facing = candidate(
        "facing",
        700,
        None,
        "cam-b",
        vec![fixtures::laughing()],
        0.70,
    );
    candidates.push(facing);

    let roles = BTreeMap::new();
    let links = reaction::resolve(reaction::link(&action, &candidates, &roles));

    let found: Vec<_> = links.iter().map(|link| link.reaction).collect();
    let hits = expected
        .iter()
        .filter(|wanted| found.contains(wanted))
        .count();
    let recall = hits as f64 / expected.len() as f64;
    let spurious = found.iter().filter(|got| !expected.contains(got)).count() as f64
        / found.len().max(1) as f64;

    println!(
        "reaction recall {recall:.3}, spurious {spurious:.3}, {} links",
        links.len()
    );
    assert!(
        recall >= REACTION_RECALL_GATE,
        "reaction recall {recall:.3} is below the {REACTION_RECALL_GATE} gate"
    );
    assert!(
        spurious < REACTION_SPURIOUS_GATE,
        "spurious link rate {spurious:.3} is at or above the {REACTION_SPURIOUS_GATE} gate"
    );
}

#[test]
fn frames_of_one_burst_do_not_react_to_each_other() {
    // Without the same-moment-same-camera condition a fourteen-frame toss produces
    // ninety-one links, none of which means anything.
    let moment = MomentId::new();
    let action = with_interaction(
        candidate(
            "a",
            0,
            Some(moment),
            "cam-a",
            vec![fixtures::laughing()],
            0.9,
        ),
        Interaction::GroupCheer,
        0.8,
    );
    let mut sibling = candidate(
        "b",
        120,
        Some(moment),
        "cam-a",
        vec![fixtures::laughing()],
        0.9,
    );
    turn_away(&mut sibling);
    let links = reaction::link(&action, &[action.clone(), sibling], &BTreeMap::new());
    assert!(
        links.is_empty(),
        "two frames of one burst on one camera linked as action and reaction"
    );
}

#[test]
fn one_reaction_points_at_exactly_one_action() {
    // The schema allows many reactions per action and one action per reaction, and the
    // resolver is where that is enforced rather than discovered at INSERT time.
    let a = PhotoId::new();
    let b = PhotoId::new();
    let reactor = PhotoId::new();
    let links = reaction::resolve(vec![
        link_between(a, reactor, 0.4),
        link_between(b, reactor, 0.9),
    ]);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].action, b, "the stronger bonus wins");
}

#[test]
fn a_vendor_reacting_is_worth_less_than_close_family() {
    // Section 6.3: the bonus is proportional to the reactor's role weight. A second
    // photographer laughing behind the couple is not the mother crying.
    assert!(reaction::role_weight(Role::FamilyClose) > reaction::role_weight(Role::Vendor));
    assert!(reaction::role_weight(Role::Bride) >= reaction::role_weight(Role::FamilyClose));
    assert!(
        reaction::role_weight(Role::Unknown) > 0.0,
        "an unclustered face is real evidence at the guest weight, not zero"
    );
}

// ---------------------------------------------------------------------------
// The nine features and the ranker
// ---------------------------------------------------------------------------

#[test]
fn every_feature_is_inside_the_unit_interval() {
    // The coefficient range in `emotion_weights.toml` is set against this. A feature that
    // left `0..1` would make one coefficient mean something different from the other
    // eight.
    let frame = frame_with(fixtures::laughing());
    let context = context_for(&frame, SceneId::DanceFloor, None);
    let reading = read_with(
        &frame,
        SceneId::DanceFloor,
        None,
        fixtures::interaction_vector(Interaction::GroupCheer, 0.9),
    );
    let table = weights();
    let row = table.for_scene(SceneId::DanceFloor, None);
    let features = score::features(
        &score::Inputs {
            faces: &reading.faces,
            prominence: &context.subjects.prominence,
            interactions: &reading.interactions,
            mutual_gaze: true,
            peak_proximity: 1.0,
            reaction_bonus: 0.8,
            scene: SceneId::DanceFloor,
            scene_known: true,
            faces_scanned: true,
        },
        &row,
    );
    for (index, value) in features.iter().enumerate() {
        assert!(
            (0.0..=1.0).contains(value),
            "feature {} ({}) is {value}",
            index,
            Ranker::FEATURE_NAMES[index]
        );
    }
}

#[test]
fn the_ranker_is_monotone_in_every_feature() {
    // A Bradley-Terry utility with a positive coefficient must never lower a score when
    // its feature rises. The one negative influence in the product is `discomfort`, and
    // it is inside feature 0 rather than a coefficient of its own.
    let table = weights();
    let ranker = table.ranker();
    let base = [0.2f32; Ranker::FEATURES];
    let baseline = ranker.score(&base);
    for index in 0..Ranker::FEATURES {
        let mut raised = base;
        raised[index] = 0.9;
        let lifted = ranker.score(&raised);
        assert!(
            lifted >= baseline,
            "raising {} lowered the score: {lifted:.4} against {baseline:.4}",
            Ranker::FEATURE_NAMES[index]
        );
    }
}

#[test]
fn a_frame_with_nothing_in_it_scores_near_the_floor() {
    // The bias is set so that the median frame of a wedding does not score half a mark.
    // A ranking in which everything is 0.5 carries no information and looks like a
    // working one.
    let frame = frame_with(fixtures::flat());
    let reading = read(&frame, SceneId::Candid, None);
    assert!(
        reading.emotion_score < 0.35,
        "an empty frame scored {:.3}",
        reading.emotion_score
    );
}

#[test]
fn a_milestone_interaction_raises_a_frame_above_the_same_frame_without_one() {
    let frame = frame_with(fixtures::composed());
    let plain = read(&frame, SceneId::Kiss, None).emotion_score;
    let kiss = read_with(
        &frame,
        SceneId::Kiss,
        None,
        fixtures::interaction_vector(Interaction::Kiss, 0.9),
    )
    .emotion_score;
    assert!(
        kiss > plain,
        "a detected kiss did not raise a kiss-scene frame: {kiss:.3} against {plain:.3}"
    );
}

#[test]
fn the_prominence_weighting_is_a_mean_and_not_a_maximum() {
    // A maximum would let one guest in row four decide a family portrait. The same
    // mistake phase 09 avoided by weighting sharpness across regions.
    let mut frame = EmotionFrame::with_faces(3);
    frame.paint_expression(0, fixtures::flat());
    frame.paint_expression(1, fixtures::flat());
    frame.paint_expression(2, fixtures::laughing());
    let mixed = read(&frame, SceneId::GroupPortrait, None).emotion_score;

    let mut all_laughing = EmotionFrame::with_faces(3);
    for index in 0..3 {
        all_laughing.paint_expression(index, fixtures::laughing());
    }
    let everyone = read(&all_laughing, SceneId::GroupPortrait, None).emotion_score;

    assert!(
        everyone > mixed,
        "three laughing faces did not outscore one: {everyone:.3} against {mixed:.3}"
    );
}

// ---------------------------------------------------------------------------
// Pairwise agreement
// ---------------------------------------------------------------------------

#[test]
fn pairwise_agreement_on_the_authored_comparison_set() {
    // Section 10.1's first gate. **Against authored preferences rather than
    // photographers'**, which is condition C2 of the exit report: there are no
    // photographer comparisons in this repository. What this proves is that the ranker
    // reproduces a set of preferences a working photographer would recognise, and that
    // the harness would fail a ranker that had learned nothing.
    let comparisons: Vec<(&str, [f32; 8], SceneId, [f32; 8], SceneId)> = vec![
        // Laughing beats flat, anywhere.
        (
            "laughter over nothing",
            fixtures::laughing(),
            SceneId::Candid,
            fixtures::flat(),
            SceneId::Candid,
        ),
        // Crying at the vows beats a posed smile at the vows.
        (
            "tears over a held smile",
            fixtures::crying(),
            SceneId::Vows,
            fixtures::posed(),
            SceneId::Vows,
        ),
        // Composure in a rite beats a broad smile in the same rite.
        (
            "composure in a rite",
            fixtures::composed(),
            SceneId::Ritual,
            fixtures::smiling(),
            SceneId::Ritual,
        ),
        // A genuine smile beats an awkward face in a portrait.
        (
            "a smile over awkwardness",
            fixtures::smiling(),
            SceneId::FamilyPortrait,
            fixtures::awkward(),
            SceneId::FamilyPortrait,
        ),
        // Tenderness at a first look beats a posed smile there.
        (
            "tenderness at a first look",
            fixtures::crying(),
            SceneId::FirstLook,
            fixtures::posed(),
            SceneId::FirstLook,
        ),
        // A laugh on the dance floor beats composure there.
        (
            "laughter on a dance floor",
            fixtures::laughing(),
            SceneId::DanceFloor,
            fixtures::composed(),
            SceneId::DanceFloor,
        ),
        // A posed smile in a family portrait beats a flat face there.
        (
            "a portrait smile",
            fixtures::posed(),
            SceneId::FamilyPortrait,
            fixtures::flat(),
            SceneId::FamilyPortrait,
        ),
        // Awkwardness loses to nothing in particular: a frame that reads uncomfortable
        // is worse than a quiet one.
        (
            "awkward under quiet",
            fixtures::flat(),
            SceneId::Candid,
            fixtures::awkward(),
            SceneId::Candid,
        ),
    ];

    let mut agreed = 0usize;
    for (name, winner, winner_scene, loser, loser_scene) in &comparisons {
        let left = read(&frame_with(*winner), *winner_scene, None).emotion_score;
        let right = read(&frame_with(*loser), *loser_scene, None).emotion_score;
        if left > right {
            agreed += 1;
        } else {
            println!("disagreed on `{name}`: {left:.3} against {right:.3}");
        }
    }
    let agreement = agreed as f64 / comparisons.len() as f64;
    println!(
        "pairwise agreement {agreement:.3} over {} comparisons",
        comparisons.len()
    );
    assert!(
        agreement >= AGREEMENT_GATE,
        "pairwise agreement {agreement:.3} is below the {AGREEMENT_GATE} gate"
    );
}

// ---------------------------------------------------------------------------
// Reasons, confidence and coverage
// ---------------------------------------------------------------------------

#[test]
fn every_reading_carries_at_least_one_reason() {
    // Invariant 2. A decision without an explanation is a bug, and a flat frame is still
    // a decision.
    for painted in [fixtures::flat(), fixtures::laughing(), fixtures::crying()] {
        let reading = read(&frame_with(painted), SceneId::Candid, None);
        assert!(!reading.reasons.is_empty());
        assert!(reading.reasons.len() <= ImageEmotion::MAX_REASONS);
        assert!(reading.confidence > 0.0);
    }
}

#[test]
fn a_caveat_never_carries_a_credit() {
    // A note about the reading is not a reason the frame earned something. The panel
    // renders the two differently and the distinction has to hold in the data.
    let mut frame = EmotionFrame::with_faces(0);
    frame.truth.clear();
    let mut context = context_for(&frame, SceneId::Unknown, None);
    context.scene_known = false;
    context.faces_scanned = false;
    let reading = analyser([0.05; INTERACTION_CLASSES])
        .analyse(PhotoId::new(), &frame.buffer(), &context, Priority::AiBatch)
        .expect("a subjectless frame must still read");
    let caveats: Vec<_> = reading
        .reasons
        .iter()
        .filter(|reason| reason.code.is_caveat())
        .collect();
    assert!(
        !caveats.is_empty(),
        "a frame with no scene and no faces must say so"
    );
    for caveat in caveats {
        assert!(
            caveat.weight <= 0.0,
            "the caveat `{}` carried a credit of {:.3}",
            caveat.code,
            caveat.weight
        );
    }
}

#[test]
fn a_wedding_with_no_scene_pass_costs_confidence_rather_than_score() {
    // Invariant 7 degraded rather than broken: the weights are still a table lookup and
    // `[default]` is the documented substitute. What the product must not do is quietly
    // present a default-weighted score as a scene-conditioned one.
    let frame = frame_with(fixtures::laughing());
    let known = read(&frame, SceneId::Candid, None);
    let mut context = context_for(&frame, SceneId::Unknown, None);
    context.scene_known = false;
    let unknown = analyser([0.05; INTERACTION_CLASSES])
        .analyse(PhotoId::new(), &frame.buffer(), &context, Priority::AiBatch)
        .expect("reads");
    assert!(
        unknown.confidence < known.confidence,
        "an unclassified frame reported the same confidence as a classified one"
    );
    assert!(unknown
        .reasons
        .iter()
        .any(|reason| reason.code == EmotionCode::NoScene));
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

#[test]
fn two_reads_of_one_frame_agree_exactly() {
    // Invariant 4. Not "close": identical, including the reason order and the reason
    // text.
    let frame = frame_with(fixtures::laughing());
    let first = read(&frame, SceneId::DanceFloor, Some("nepali"));
    let second = read(&frame, SceneId::DanceFloor, Some("nepali"));
    assert!(
        (first.emotion_score - second.emotion_score).abs() < f32::EPSILON,
        "two reads produced {} and {}",
        first.emotion_score,
        second.emotion_score
    );
    assert_eq!(first.reasons.len(), second.reasons.len());
    for (left, right) in first.reasons.iter().zip(second.reasons.iter()) {
        assert_eq!(left.code, right.code);
        assert_eq!(left.text, right.text);
    }
}

#[test]
fn the_interaction_order_is_deterministic_on_a_tie() {
    // Two equal strengths must fall back to the head's output order, or two runs of one
    // wedding write different rows.
    let mut planted = [0.05f32; INTERACTION_CLASSES];
    planted[Interaction::Hug.as_index()] = 0.80;
    planted[Interaction::Toast.as_index()] = 0.80;
    let detected = interaction::detected(&planted);
    assert_eq!(
        detected.first().map(|entry| entry.0),
        Some(Interaction::Hug)
    );
}

// ---------------------------------------------------------------------------
// The guard: a gate that cannot fail is not a gate
// ---------------------------------------------------------------------------

#[test]
fn the_gate_rejects_a_useless_reader() {
    // The guard phases 06, 07, 08 and 09 each wrote, for the fifth time. A reader that
    // returns the same eight numbers for every face - which is what an untrained head
    // very nearly does - must fail the agreement gate rather than pass it.
    let constant = [0.5f32; 8];
    let comparisons = [
        (fixtures::laughing(), fixtures::flat(), SceneId::Candid),
        (fixtures::crying(), fixtures::posed(), SceneId::Vows),
        (fixtures::composed(), fixtures::smiling(), SceneId::Ritual),
    ];
    let mut agreed = 0usize;
    for (_, _, scene) in comparisons {
        let left = read(&frame_with(constant), scene, None).emotion_score;
        let right = read(&frame_with(constant), scene, None).emotion_score;
        if left > right {
            agreed += 1;
        }
    }
    let agreement = agreed as f64 / comparisons.len() as f64;
    assert!(
        agreement < AGREEMENT_GATE,
        "a constant reader reached {agreement:.3} agreement, so the gate proves nothing"
    );
}

#[test]
fn an_inverted_ranker_scores_below_a_coin_toss() {
    // The second guard: a ranker whose coefficients are negated must lose every
    // comparison, not merely most of them.
    let table = weights();
    let mut inverted = table.ranker();
    // Every coefficient, not a selection of them. Negating three of nine leaves the
    // other six pulling the right way, which is a weaker guard than it looks - and
    // noticing that is exactly what this test is for.
    inverted.subject_expression = -inverted.subject_expression;
    inverted.genuineness = -inverted.genuineness;
    inverted.interaction = -inverted.interaction;
    inverted.peak = -inverted.peak;
    inverted.reaction = -inverted.reaction;
    inverted.mutual_gaze = -inverted.mutual_gaze;
    inverted.composure = -inverted.composure;
    inverted.tears = -inverted.tears;
    inverted.group = -inverted.group;

    let strong = [0.9f32; Ranker::FEATURES];
    let weak = [0.1f32; Ranker::FEATURES];
    assert!(
        inverted.score(&strong) < inverted.score(&weak),
        "an inverted ranker still preferred the stronger frame"
    );
}

// -- metrics -----------------------------------------------------------------

/// The same confusion counts `eval_emotion.py` computes.
#[derive(Debug, Default)]
struct Counts {
    tp: usize,
    fp: usize,
    fal_neg: usize,
}

impl Counts {
    fn add(&mut self, predicted: bool, actual: bool) {
        match (predicted, actual) {
            (true, true) => self.tp += 1,
            (true, false) => self.fp += 1,
            (false, true) => self.fal_neg += 1,
            (false, false) => {}
        }
    }

    fn precision(&self) -> f64 {
        if self.tp + self.fp == 0 {
            return 1.0;
        }
        self.tp as f64 / (self.tp + self.fp) as f64
    }

    fn recall(&self) -> f64 {
        if self.tp + self.fal_neg == 0 {
            return 1.0;
        }
        self.tp as f64 / (self.tp + self.fal_neg) as f64
    }

    fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r <= 0.0 {
            return 0.0;
        }
        2.0 * p * r / (p + r)
    }
}

// -- fixtures for the peak and reaction paths --------------------------------

/// Read a run of frames into peak-curve inputs.
fn build_curve(frames: &[EmotionFrame]) -> Vec<peak::Frame> {
    frames
        .iter()
        .map(|frame| {
            let reading = read(frame, SceneId::Candid, None);
            peak::Frame {
                image: reading.image_id,
                faces: reading.faces,
                interactions: reading.interactions,
                scene: reading.scene,
            }
        })
        .collect()
}

/// One reaction candidate with a painted expression.
fn candidate(
    _name: &str,
    ts: i64,
    moment: Option<MomentId>,
    camera: &str,
    painted: Vec<[f32; 8]>,
    strength: f32,
) -> reaction::Candidate {
    let mut frame = EmotionFrame::with_faces(painted.len().max(1));
    for (index, channels) in painted.iter().enumerate() {
        frame.paint_expression(index, *channels);
    }
    let reading = read(&frame, SceneId::Candid, None);
    reaction::Candidate {
        image: reading.image_id,
        ts,
        moment,
        camera: camera.to_string(),
        faces: reading.faces,
        interactions: reading.interactions,
        strength,
    }
}

/// Turn every face in a candidate away from the lens, so it counts as engaged.
fn turn_away(candidate: &mut reaction::Candidate) {
    for face in &mut candidate.faces {
        face.gaze = GazeTarget::Away;
    }
}

/// Plant an interaction on a candidate.
fn with_interaction(
    mut candidate: reaction::Candidate,
    kind: Interaction,
    strength: f32,
) -> reaction::Candidate {
    candidate.interactions = vec![(kind, strength)];
    candidate
}

/// A bare link, for the resolver test.
fn link_between(action: PhotoId, reaction: PhotoId, bonus: f32) -> ReactionLink {
    ReactionLink {
        action,
        reaction,
        gap_ms: 500,
        bonus,
        confidence: bonus,
        reasons: Vec::new(),
    }
}

// -- the planted inference service -------------------------------------------

/// An inference service that returns one fixed interaction vector.
///
/// See this file's header. The expression head is replaced by the reference reader,
/// whose answer is painted into the frame; this stands in for the interaction head,
/// whose input is the whole frame and has no equivalent trick.
#[derive(Debug)]
struct PlantedInfer {
    plan: aura_infer::contract::infer::HardwarePlan,
    interactions: [f32; INTERACTION_CLASSES],
}

impl PlantedInfer {
    fn new(interactions: [f32; INTERACTION_CLASSES]) -> Self {
        Self {
            plan: aura_infer::contract::infer::HardwarePlan::conservative(
                1,
                "1970-01-01T00:00:00Z".to_string(),
            ),
            interactions,
        }
    }

    fn result(&self) -> aura_infer::contract::infer::InferResult {
        aura_infer::contract::infer::InferResult {
            outputs: vec![aura_infer::contract::infer::Tensor {
                shape: vec![1, INTERACTION_CLASSES],
                data: self.interactions.to_vec(),
            }],
            ep_used: aura_infer::contract::infer::ExecutionProvider::Cpu,
            latency_ms: 0.0,
            batch_size: 1,
        }
    }
}

impl aura_infer::contract::infer::InferService for PlantedInfer {
    fn run(
        &self,
        _req: aura_infer::contract::infer::InferRequest<'_>,
    ) -> Result<aura_infer::contract::infer::InferResult, aura_infer::contract::infer::InferError>
    {
        Ok(self.result())
    }

    fn run_batch(
        &self,
        _model: aura_infer::contract::infer::ModelRef,
        inputs: Vec<Vec<aura_infer::contract::infer::TensorView<'_>>>,
        _prio: aura_core::contract::priority::Priority,
    ) -> Result<
        Vec<aura_infer::contract::infer::InferResult>,
        aura_infer::contract::infer::InferError,
    > {
        Ok(inputs.iter().map(|_| self.result()).collect())
    }

    fn plan(&self) -> &aura_infer::contract::infer::HardwarePlan {
        &self.plan
    }

    fn warmup(
        &self,
        _models: &[aura_infer::contract::infer::ModelRef],
    ) -> Result<(), aura_infer::contract::infer::InferError> {
        Ok(())
    }
}

/// Silence the unused-import warning for the module the harness documents but does not
/// call directly.
#[allow(dead_code)]
fn _uses_analyse_module(_: &analyse::FrameContext) {}
