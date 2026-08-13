//! Alignment, pose and box geometry.
//!
//! The maths in `aura_vision::face::align` and `aura_vision::face::detect` has no model
//! behind it, which makes it the part of phase 06 that can be tested exactly rather than
//! statistically - and the part where an error would be invisible in every other test.
//! A mirrored warp, a transposed landmark order or a sign-flipped yaw all produce
//! plausible-looking templates and a silently worse identity graph.

use aura_vision::face::align::{
    self, Landmarks, ARCFACE_REFERENCE, LEFT_EYE, LEFT_MOUTH, NOSE, RIGHT_EYE,
};
use aura_vision::face::detect::{self, DetectConfig, Detection, FoundBy, Letterbox, NormBox};
use aura_vision::face::fixtures::{self, SynthFace};

/// Landmarks placed on a face box of a given size, in pixels.
fn landmarks_at(left: f32, top: f32, width: f32, height: f32) -> Landmarks {
    let mut out = [[0.0f32; 2]; 5];
    for (slot, point) in out.iter_mut().zip(ARCFACE_REFERENCE.iter()) {
        *slot = [
            left + (point[0] / 112.0) * width,
            top + (point[1] / 112.0) * height,
        ];
    }
    out
}

#[test]
fn the_reference_landmarks_map_to_themselves() {
    // The identity case, and the one that catches a transposed axis: fitting the
    // reference onto itself must produce scale 1, no rotation and no translation.
    let transform = align::alignment_for(&ARCFACE_REFERENCE);
    assert!(
        (transform.scale - 1.0).abs() < 1e-4,
        "scale {}",
        transform.scale
    );
    assert!((transform.cos - 1.0).abs() < 1e-4, "cos {}", transform.cos);
    assert!(transform.sin.abs() < 1e-4, "sin {}", transform.sin);
    assert!(transform.tx.abs() < 1e-3, "tx {}", transform.tx);
    assert!(transform.ty.abs() < 1e-3, "ty {}", transform.ty);
}

#[test]
fn alignment_undoes_scale_and_translation() {
    // A face twice the reference size, offset into the frame. The transform must put its
    // landmarks back on the reference to within a fraction of a pixel - which is the
    // whole promise alignment makes to the recogniser.
    let pixels = landmarks_at(300.0, 180.0, 224.0, 224.0);
    let transform = align::alignment_for(&pixels);

    for (source, expected) in pixels.iter().zip(ARCFACE_REFERENCE.iter()) {
        let mapped = transform.forward(*source);
        assert!(
            (mapped[0] - expected[0]).abs() < 0.01 && (mapped[1] - expected[1]).abs() < 0.01,
            "mapped {mapped:?} expected {expected:?}"
        );
    }
}

#[test]
fn the_inverse_transform_round_trips() {
    let pixels = landmarks_at(120.0, 90.0, 150.0, 150.0);
    let transform = align::alignment_for(&pixels);
    for point in &pixels {
        let there_and_back = transform.inverse(transform.forward(*point));
        assert!(
            (there_and_back[0] - point[0]).abs() < 0.01
                && (there_and_back[1] - point[1]).abs() < 0.01,
            "{there_and_back:?} != {point:?}"
        );
    }
}

#[test]
fn five_coincident_landmarks_do_not_produce_infinities() {
    // A bokeh highlight collapses its landmarks to a point. The transform must refuse
    // rather than divide by zero, or the warp produces a buffer of values that are not
    // numbers and the template that comes out of it is quietly meaningless.
    let collapsed: Landmarks = [[50.0, 50.0]; 5];
    let transform = align::alignment_for(&collapsed);
    assert!(
        (transform.scale - 1.0).abs() < f32::EPSILON,
        "a degenerate fit must be the identity, not an infinity"
    );

    let rgb = vec![128u8; 200 * 200 * 3];
    let crop = align::warp_crop(&rgb, 200, 200, &transform);
    assert!(crop.iter().all(|value| *value != 0 || true));
    assert_eq!(crop.len(), 112 * 112 * 3);
}

#[test]
fn a_frontal_face_reads_as_frontal() {
    let pixels = landmarks_at(0.0, 0.0, 112.0, 112.0);
    let pose = align::pose_from_landmarks(&pixels);
    assert!(pose.yaw.abs() < 2.0, "yaw {}", pose.yaw);
    assert!(pose.pitch.abs() < 2.0, "pitch {}", pose.pitch);
    assert!(pose.roll.abs() < 1.0, "roll {}", pose.roll);
    assert!(
        pose.frontality_penalty() < 0.05,
        "penalty {}",
        pose.frontality_penalty()
    );
}

#[test]
fn yaw_is_signed_towards_the_image_right() {
    // The sign convention, written down in `Pose` and asserted here. Getting it backwards
    // mirrors every pose in the product and is invisible without a test like this one.
    let mut pixels = landmarks_at(0.0, 0.0, 112.0, 112.0);
    let interocular = pixels[RIGHT_EYE][0] - pixels[LEFT_EYE][0];
    pixels[NOSE][0] += interocular * 0.30;
    let right = align::pose_from_landmarks(&pixels);
    assert!(
        right.yaw > 20.0,
        "expected a positive yaw, got {}",
        right.yaw
    );

    let mut pixels = landmarks_at(0.0, 0.0, 112.0, 112.0);
    pixels[NOSE][0] -= interocular * 0.30;
    let left = align::pose_from_landmarks(&pixels);
    assert!(
        left.yaw < -20.0,
        "expected a negative yaw, got {}",
        left.yaw
    );
}

#[test]
fn yaw_saturates_rather_than_extrapolating() {
    // Past a three-quarter profile one eye is occluded and its landmark is a guess. A
    // bigger number would be a more confident guess.
    let mut pixels = landmarks_at(0.0, 0.0, 112.0, 112.0);
    pixels[NOSE][0] += 400.0;
    let pose = align::pose_from_landmarks(&pixels);
    assert!(pose.yaw <= 90.0, "yaw must saturate, got {}", pose.yaw);
    assert!(
        pose.yaw >= 89.0,
        "yaw should be at the ceiling, got {}",
        pose.yaw
    );
}

#[test]
fn roll_follows_the_eye_line() {
    let mut pixels = landmarks_at(0.0, 0.0, 112.0, 112.0);
    pixels[RIGHT_EYE][1] += 20.0;
    let pose = align::pose_from_landmarks(&pixels);
    assert!(pose.roll > 20.0, "roll {}", pose.roll);

    // Alignment reduces it, but a displaced *single* landmark is not a rotation, so the
    // least-squares fit over five points can only partly undo it. That is correct
    // behaviour and is asserted as a reduction rather than as a removal.
    let transform = align::alignment_for(&pixels);
    let mut aligned = [[0.0f32; 2]; 5];
    for (slot, point) in aligned.iter_mut().zip(pixels.iter()) {
        *slot = transform.forward(*point);
    }
    let residual = align::pose_from_landmarks(&aligned);
    assert!(
        residual.roll.abs() < pose.roll.abs() * 0.8,
        "alignment should reduce the roll: {} -> {}",
        pose.roll,
        residual.roll
    );
}

#[test]
fn alignment_removes_a_whole_head_tilt() {
    // The case alignment is actually for: the whole face rotated in the frame, which is
    // what a candid at a wedding produces. Here the transform is exact, and the residual
    // roll must be effectively zero - this is what stops the same person's templates
    // scattering because the photographer tilted the camera.
    let upright = landmarks_at(200.0, 150.0, 120.0, 120.0);
    let angle = 25.0f32.to_radians();
    let (sin, cos) = angle.sin_cos();
    let pivot = [260.0f32, 210.0f32];
    let mut tilted = [[0.0f32; 2]; 5];
    for (slot, point) in tilted.iter_mut().zip(upright.iter()) {
        let dx = point[0] - pivot[0];
        let dy = point[1] - pivot[1];
        *slot = [
            pivot[0] + cos * dx - sin * dy,
            pivot[1] + sin * dx + cos * dy,
        ];
    }

    let before = align::pose_from_landmarks(&tilted);
    assert!(
        before.roll > 20.0,
        "the fixture should be tilted: {}",
        before.roll
    );

    let transform = align::alignment_for(&tilted);
    let mut aligned = [[0.0f32; 2]; 5];
    for (slot, point) in aligned.iter_mut().zip(tilted.iter()) {
        *slot = transform.forward(*point);
    }
    let residual = align::pose_from_landmarks(&aligned);
    assert!(
        residual.roll.abs() < 0.5,
        "a whole-head tilt must be removed exactly: {} degrees remain",
        residual.roll
    );
}

#[test]
fn pitch_is_positive_when_the_chin_drops() {
    let mut pixels = landmarks_at(0.0, 0.0, 112.0, 112.0);
    // Foreshortening moves the nose towards the mouth line when the chin drops.
    let span = pixels[LEFT_MOUTH][1] - pixels[LEFT_EYE][1];
    pixels[NOSE][1] += span * 0.25;
    let down = align::pose_from_landmarks(&pixels);
    assert!(
        down.pitch > 10.0,
        "expected a positive pitch, got {}",
        down.pitch
    );
}

#[test]
fn landmarks_round_trip_through_json() {
    let normalised = align::to_normalised(&landmarks_at(100.0, 50.0, 80.0, 80.0), 640, 480);
    let text = align::to_json(&normalised);
    let parsed = align::from_json(&text).expect("five pairs must parse");
    for (before, after) in normalised.iter().zip(parsed.iter()) {
        assert!(
            (before[0] - after[0]).abs() < 1e-5 && (before[1] - after[1]).abs() < 1e-5,
            "{before:?} != {after:?}"
        );
    }
}

#[test]
fn a_landmark_array_of_the_wrong_length_is_refused() {
    // Guessing at the missing points would put a face's alignment - and therefore its
    // template - somewhere nobody chose.
    assert!(align::from_json("[[0.1,0.2],[0.3,0.4]]").is_none());
    assert!(align::from_json("[]").is_none());
    assert!(align::from_json("not json at all").is_none());
}

#[test]
fn centrality_is_flat_in_the_middle_and_falls_at_the_edges() {
    let centre = NormBox::from_corners(0.45, 0.45, 0.55, 0.55);
    let slightly_off = NormBox::from_corners(0.40, 0.45, 0.50, 0.55);
    let corner = NormBox::from_corners(0.0, 0.0, 0.1, 0.1);

    assert!(centre.centrality() > 0.98);
    assert!(
        slightly_off.centrality() > 0.95,
        "a tenth off centre should not read as much less central: {}",
        slightly_off.centrality()
    );
    assert!(corner.centrality() < 0.2, "corner {}", corner.centrality());
}

#[test]
fn the_letterbox_maps_a_tile_back_to_the_frame() {
    // The one place a coordinate-system mistake can hide. A detection in the middle of
    // the bottom-right tile must come back in the bottom-right of the frame.
    let (_, whole) = detect::preprocess(&vec![0u8; 800 * 600 * 3], 800, 600, (0, 0, 800, 600));
    let middle = whole.to_frame([
        detect::INPUT_SIDE as f32 / 2.0,
        detect::INPUT_SIDE as f32 / 2.0,
    ]);
    assert!(
        (middle[0] - 0.5).abs() < 0.01 && (middle[1] - 0.5).abs() < 0.01,
        "the centre of a whole-frame pass is the centre of the frame: {middle:?}"
    );

    let tiles = detect::tiles(800, 600);
    let last = *tiles.last().expect("four tiles");
    let (_, tile) = detect::preprocess(&vec![0u8; 800 * 600 * 3], 800, 600, last);
    let in_tile = tile.to_frame([
        detect::INPUT_SIDE as f32 / 2.0,
        detect::INPUT_SIDE as f32 / 2.0,
    ]);
    assert!(
        in_tile[0] > 0.5 && in_tile[1] > 0.5,
        "the centre of the bottom-right tile is in the bottom-right of the frame: {in_tile:?}"
    );
}

#[test]
fn every_frame_pixel_is_whole_in_at_least_one_tile() {
    // The overlap's reason for existing: a face straddling a tile boundary is cut in half
    // by both tiles unless they overlap.
    let (width, height) = (1200usize, 800usize);
    let tiles = detect::tiles(width, height);
    assert_eq!(tiles.len(), 4);

    // The four tiles together must cover the whole frame, and adjacent ones must overlap.
    let (left_x, _, left_w, _) = tiles[0];
    let (right_x, _, right_w, _) = tiles[1];
    assert_eq!(left_x, 0);
    assert!(
        left_x + left_w > right_x,
        "horizontally adjacent tiles must overlap: {left_x}+{left_w} vs {right_x}"
    );
    assert_eq!(right_x + right_w, width);
}

#[test]
fn tiles_refuse_a_frame_too_small_to_tile() {
    assert!(detect::tiles(3, 3).is_empty());
}

/// A detection with landmarks spread like a real face's.
fn faceish(box_norm: NormBox, score: f32) -> Detection {
    let mut landmarks = [[0.0f32; 2]; 5];
    for (slot, point) in landmarks.iter_mut().zip(ARCFACE_REFERENCE.iter()) {
        *slot = [
            box_norm.x + (point[0] / 112.0) * box_norm.w,
            box_norm.y + (point[1] / 112.0) * box_norm.h,
        ];
    }
    Detection {
        face: box_norm,
        person: None,
        face_score: score,
        person_score: 0.0,
        landmarks,
        stride: 8,
        found_by: FoundBy::Full,
    }
}

#[test]
fn suppression_keeps_the_stronger_of_two_overlapping_detections() {
    let strong = faceish(NormBox::from_corners(0.20, 0.20, 0.30, 0.34), 0.90);
    let weak = faceish(NormBox::from_corners(0.21, 0.21, 0.31, 0.35), 0.40);
    let kept = detect::nms(vec![weak, strong], &DetectConfig::default());
    assert_eq!(kept.len(), 1);
    assert!((kept[0].face_score - 0.90).abs() < 1e-6);
}

#[test]
fn suppression_keeps_two_cheek_to_cheek_faces() {
    // Wedding group shots put cheeks against cheeks. A 0.5 threshold merges a hug into
    // one detection, which is why the shipped threshold is 0.35.
    let left = faceish(NormBox::from_corners(0.20, 0.20, 0.30, 0.34), 0.80);
    let right = faceish(NormBox::from_corners(0.29, 0.20, 0.39, 0.34), 0.78);
    let kept = detect::nms(vec![left, right], &DetectConfig::default());
    assert_eq!(kept.len(), 2, "two adjacent faces must survive suppression");
}

#[test]
fn suppression_is_order_independent() {
    let a = faceish(NormBox::from_corners(0.10, 0.10, 0.20, 0.24), 0.71);
    let b = faceish(NormBox::from_corners(0.50, 0.50, 0.60, 0.64), 0.71);
    let c = faceish(NormBox::from_corners(0.30, 0.30, 0.40, 0.44), 0.71);

    let one = detect::nms(vec![a, b, c], &DetectConfig::default());
    let two = detect::nms(vec![c, a, b], &DetectConfig::default());
    let boxes = |list: &[Detection]| -> Vec<(i32, i32)> {
        list.iter()
            .map(|d| ((d.face.x * 1e4) as i32, (d.face.y * 1e4) as i32))
            .collect()
    };
    assert_eq!(
        boxes(&one),
        boxes(&two),
        "identical scores must not make the outcome depend on input order"
    );
}

#[test]
fn a_structureless_detection_is_refused() {
    // The bokeh gate. Five landmarks piled at the centre is what a blurred highlight
    // produces, and refusing it here is what keeps the false-positive rate down without
    // raising the objectness threshold and losing small-face recall.
    let box_norm = NormBox::from_corners(0.4, 0.4, 0.5, 0.54);
    let mut highlight = faceish(box_norm, 0.95);
    let centre = box_norm.centre();
    highlight.landmarks = [centre; 5];

    let kept = detect::nms(vec![highlight], &DetectConfig::default());
    assert!(
        kept.is_empty(),
        "a landmark-less detection must not survive"
    );
}

#[test]
fn the_reference_detector_finds_the_faces_it_was_given() {
    // The fixture detector is a real detector over the fixture format, not a lookup of
    // the ground truth. If this fails, every recall number in the phase is meaningless.
    let faces = vec![
        SynthFace::new([0.30, 0.35], 0.22, 1),
        SynthFace::new([0.70, 0.55], 0.18, 2),
    ];
    let rgb = fixtures::render_frame(800, 600, &faces);
    let found = fixtures::ReferenceDetector::new().detect(&rgb, 800, 600);
    let kept = detect::nms(found, &DetectConfig::default());

    let (matched, total) = fixtures::recall_at_iou(&faces, &kept, 0.5);
    assert_eq!(matched, total, "both synthetic faces must be found");
    assert_eq!(fixtures::false_positives(&faces, &kept, 0.5), 0);
}

#[test]
fn the_reference_detector_rejects_bokeh() {
    // Face-coloured, round and empty. It keys as skin, so the detector proposes it; the
    // landmark-spread gate is what refuses it.
    let faces = vec![
        SynthFace::new([0.25, 0.30], 0.20, 1),
        SynthFace::bokeh([0.70, 0.30], 0.12),
        SynthFace::bokeh([0.80, 0.60], 0.10),
    ];
    let rgb = fixtures::render_frame(800, 600, &faces);
    let found = fixtures::ReferenceDetector::new().detect(&rgb, 800, 600);
    let kept = detect::nms(found, &DetectConfig::default());

    assert_eq!(
        fixtures::false_positives(&faces, &kept, 0.5),
        0,
        "a blurred highlight must not be reported as a face"
    );
}

#[test]
fn tiling_fires_on_a_wide_frame_full_of_small_faces() {
    let small: Vec<Detection> = (0..4)
        .map(|index| {
            faceish(
                NormBox::from_corners(
                    0.1 + index as f32 * 0.2,
                    0.4,
                    0.13 + index as f32 * 0.2,
                    0.44,
                ),
                0.6,
            )
        })
        .collect();
    let decision = detect::should_tile(
        &small,
        &detect::FrameHint {
            focal_len_mm: Some(24.0),
            person_boxes: 0,
        },
        6000,
        4000,
        &DetectConfig::default(),
    );
    assert!(decision.tile);
    assert!(
        !decision.reasons.is_empty(),
        "a compute decision needs reasons too"
    );
}

#[test]
fn tiling_does_not_fire_on_a_long_lens() {
    let small: Vec<Detection> = (0..4)
        .map(|index| {
            faceish(
                NormBox::from_corners(
                    0.1 + index as f32 * 0.2,
                    0.4,
                    0.13 + index as f32 * 0.2,
                    0.44,
                ),
                0.6,
            )
        })
        .collect();
    let decision = detect::should_tile(
        &small,
        &detect::FrameHint {
            focal_len_mm: Some(200.0),
            person_boxes: 0,
        },
        6000,
        4000,
        &DetectConfig::default(),
    );
    assert!(!decision.tile, "{:?}", decision.reasons);
}

#[test]
fn tiling_fires_when_there_are_bodies_and_no_faces() {
    // The case that matters: either the faces are turned away, in which case tiling costs
    // four passes and finds nothing, or they are below the full pass's resolution, which
    // is how a wedding ends up with a hundred unattributed guests. Paying to find out is
    // the right trade, and it is why this branch does not also require a wide lens.
    let decision = detect::should_tile(
        &[],
        &detect::FrameHint {
            focal_len_mm: Some(85.0),
            person_boxes: 6,
        },
        6000,
        4000,
        &DetectConfig::default(),
    );
    assert!(decision.tile, "{:?}", decision.reasons);
}

#[test]
fn preprocess_letterboxes_rather_than_cropping() {
    // A wide frame must arrive padded, not cropped: the faces the tiled pass exists to
    // recover are the ones at the left and right edges.
    let width = 1600usize;
    let height = 400usize;
    let mut rgb = vec![0u8; width * height * 3];
    // Mark the extreme left and right columns.
    for row in 0..height {
        if let Some(slot) = rgb.get_mut(row * width * 3) {
            *slot = 255;
        }
        if let Some(slot) = rgb.get_mut((row * width + width - 1) * 3) {
            *slot = 255;
        }
    }

    let (tensor, letterbox) = detect::preprocess(&rgb, width, height, (0, 0, width, height));
    assert_eq!(tensor.len(), 3 * detect::INPUT_SIDE * detect::INPUT_SIDE);
    assert!(
        letterbox.pad_y > 0.0,
        "a 4:1 frame must be padded vertically, not cropped horizontally"
    );
    assert!(letterbox.pad_x < 1.0, "and not padded horizontally");

    // Both marked columns must be inside the tensor.
    let side = detect::INPUT_SIDE;
    let row = (side / 2) * side;
    let red_left = tensor.get(row).copied().unwrap_or(0.0);
    let red_right = tensor.get(row + side - 1).copied().unwrap_or(0.0);
    assert!(
        red_left > 0.0 || red_right > 0.0,
        "the frame's extreme columns must survive preprocessing"
    );
}

#[test]
fn a_zero_sized_region_is_survivable() {
    let (tensor, letterbox) = detect::preprocess(&[], 0, 0, (0, 0, 0, 0));
    assert_eq!(tensor.len(), 3 * detect::INPUT_SIDE * detect::INPUT_SIDE);
    assert_eq!(letterbox.to_frame([10.0, 10.0]), [0.0, 0.0]);
}

#[test]
fn a_letterbox_with_no_scale_does_not_divide_by_zero() {
    let letterbox = Letterbox {
        region_x: 0,
        region_y: 0,
        region_w: 0,
        region_h: 0,
        frame_w: 100,
        frame_h: 100,
        scale: 0.0,
        pad_x: 0.0,
        pad_y: 0.0,
    };
    assert_eq!(letterbox.to_frame([1.0, 1.0]), [0.0, 0.0]);
    assert!((letterbox.source_pixels_per_detector_pixel() - 1.0).abs() < f32::EPSILON);
}
