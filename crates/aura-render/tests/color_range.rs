//! Sampled colour eligibility and local chroma controls through public rendering.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]
use aura_recipe::retouch_tools::{self, ColorRange, Edit, LuminanceRange, Selection, Tool};
use aura_render::retouch_tools::{apply, selection_mask};

fn operation() -> Edit {
    serde_json::from_value(serde_json::json!({
        "id":"color", "tool":"color_saturation", "enabled":true,
        "region":[0.5,0.5,1,1], "source":[0.9,0.9], "amount":1.0,
        "feather":0.0,"radius":0.01,"texture":1.0,"tone":0.5,"warmth":-1.0,"tint":0.0,
        "selection":{"color":{"sample":[0.0,0.0],"tolerance":5,"softness":10,"lightness":1}}
    }))
    .unwrap()
}
fn color(op: &mut Edit) -> &mut ColorRange {
    op.selection.as_mut().unwrap().color.as_mut().unwrap()
}
fn luma(p: &[f32]) -> f32 {
    p[0] * 0.2627 + p[1] * 0.678 + p[2] * 0.0593
}
fn rendered(input: &[f32], w: usize, h: usize, op: &Edit) -> Vec<f32> {
    let mut rgb = input.to_vec();
    apply(&mut rgb, w, h, std::slice::from_ref(op));
    rgb
}

#[test]
fn sampled_colour_selects_matching_pixels_and_refuses_opposite_hues() {
    let input = [
        0.6, 0.1, 0.05, 0.05, 0.6, 0.1, 0.6, 0.1, 0.05, 0.18, 0.18, 0.18,
    ];
    let op = operation();
    let mask = selection_mask(&input, 4, 1, &op);
    assert_eq!(mask, vec![1.0, 0.0, 1.0, 0.0]);
    let output = rendered(&input, 4, 1, &op);
    assert_eq!(&output[3..6], &input[3..6]);
    assert_eq!(&output[9..12], &input[9..12]);
    assert!((output[0] - output[1]).abs() < 1e-6);
    assert!((luma(&output[..3]) - luma(&input[..3])).abs() < 1e-6);
}

#[test]
fn lightness_sensitivity_and_falloff_change_coverage_continuously() {
    let mut op = operation();
    let values = [0.18, 0.21, 0.25, 0.35, 0.6];
    let input: Vec<f32> = values.into_iter().flat_map(|v| [v; 3]).collect();
    color(&mut op).tolerance = 0.0;
    color(&mut op).softness = 25.0;
    let mask = selection_mask(&input, 5, 1, &op);
    assert_eq!(mask[0], 1.0);
    assert!(mask[1] < 1.0 && mask[1] > mask[2] && mask[2] > mask[3] && mask[3] > mask[4]);
    color(&mut op).lightness = 0.0;
    assert!(selection_mask(&input, 5, 1, &op)
        .iter()
        .all(|v| (*v - 1.0).abs() < 1e-5));
    color(&mut op).lightness = 1.0;
    color(&mut op).softness = 0.0;
    assert_eq!(
        selection_mask(&input, 5, 1, &op),
        vec![1.0, 0.0, 0.0, 0.0, 0.0]
    );
}

#[test]
fn shape_inversion_does_not_invert_colour_or_brightness_limits() {
    let mut op = operation();
    op.region = [0.25, 0.5, 0.25, 1.0];
    op.selection.as_mut().unwrap().inverted = true;
    let input = [
        0.6, 0.1, 0.05, 0.6, 0.1, 0.05, 0.6, 0.1, 0.05, 0.05, 0.6, 0.1,
    ];
    assert_eq!(selection_mask(&input, 4, 1, &op), vec![0.0, 0.0, 1.0, 0.0]);
    op.selection.as_mut().unwrap().luminance = Some(LuminanceRange {
        low: 3.0,
        high: 16.0,
        softness: 0.0,
    });
    assert_eq!(selection_mask(&input, 4, 1, &op), vec![0.0; 4]);
}

#[test]
fn colour_sample_is_independent_of_clone_source_and_tracks_prior_pixels() {
    let input = [0.6, 0.1, 0.05, 0.05, 0.6, 0.1];
    let mut op = operation();
    let original = selection_mask(&input, 2, 1, &op);
    op.source = Some([0.0, 0.0]);
    assert_eq!(original, selection_mask(&input, 2, 1, &op));
    color(&mut op).sample = [1.0, 1.0];
    assert_eq!(selection_mask(&input, 2, 1, &op), vec![0.0, 1.0]);
    let changed = [0.05, 0.6, 0.1, 0.05, 0.6, 0.1];
    assert_eq!(selection_mask(&changed, 2, 1, &op), vec![1.0, 1.0]);
}

#[test]
fn repeated_colour_ranges_are_resampled_after_upstream_edits() {
    let red = [0.6, 0.1, 0.05];
    let gray = [luma(&red); 3];
    let input: Vec<f32> = red.into_iter().chain(gray).collect();
    let first = operation();
    let mut second = first.clone();
    second.id = "lighten".into();
    second.tool = Tool::Dodge;
    let intermediate = rendered(&input, 2, 1, &first);
    assert_eq!(selection_mask(&intermediate, 2, 1, &second), vec![1.0, 1.0]);
    let expected = rendered(&intermediate, 2, 1, &second);
    assert!(expected[3] > input[3]);
    let mut stacked = input;
    apply(&mut stacked, 2, 1, &[first, second]);
    assert_eq!(stacked, expected);
}

#[test]
fn colour_ranges_intersect_painted_erasure_gradient_and_saved_matte_holes() {
    use retouch_tools::{BrushMask, BrushStroke, Gradient, Matte};
    let (w, h) = (32, 32);
    let input: Vec<f32> = (0..w * h).flat_map(|_| [0.6, 0.1, 0.05]).collect();
    let mut op = operation();
    op.mask = Some(BrushMask {
        strokes: vec![
            BrushStroke {
                erase: false,
                radius: 0.25,
                opacity: 0.7,
                points: vec![[0.5, 0.5, 1.0]],
            },
            BrushStroke {
                erase: true,
                radius: 0.08,
                opacity: 1.0,
                points: vec![[0.5, 0.5, 1.0]],
            },
        ],
    });
    let mask = selection_mask(&input, w, h, &op);
    assert_eq!(mask[16 * w + 16], 0.0);
    assert_eq!(mask[0], 0.0);
    assert!(mask.iter().any(|v| *v > 0.0 && *v <= 0.7));
    let output = rendered(&input, w, h, &op);
    for (i, weight) in mask.iter().enumerate() {
        if *weight == 0.0 {
            assert_eq!(&output[i * 3..i * 3 + 3], &input[i * 3..i * 3 + 3]);
        }
    }
    op.mask = None;
    op.selection.as_mut().unwrap().gradient = Some(Gradient {
        start: [0.0, 0.5],
        end: [1.0, 0.5],
    });
    let gradient = selection_mask(&input, w, h, &op);
    assert!(gradient[16 * w + 2] < gradient[16 * w + 28]);
    op.matte = Some("protected".into());
    let mut matte = Matte::encode(
        [0.0, 0.0, 1.0, 1.0],
        4,
        4,
        &[
            255, 255, 255, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255,
        ],
    );
    matte.refine_edges = false;
    let mattes = std::collections::BTreeMap::from([("protected".into(), matte)]);
    let mut output = input.clone();
    aura_render::retouch_tools::apply_with_mattes(&mut output, w, h, &[op], &mattes);
    assert_eq!(
        &output[(16 * w + 16) * 3..(16 * w + 16) * 3 + 3],
        &input[(16 * w + 16) * 3..(16 * w + 16) * 3 + 3]
    );
    assert_ne!(
        &output[(16 * w + 28) * 3..(16 * w + 28) * 3 + 3],
        &input[(16 * w + 28) * 3..(16 * w + 28) * 3 + 3]
    );
}

#[test]
fn range_validation_round_trips_and_rejects_bad_coordinates_and_parameters() {
    let op = operation();
    retouch_tools::validate(std::slice::from_ref(&op)).unwrap();
    assert_eq!(
        serde_json::from_str::<Edit>(&serde_json::to_string(&op).unwrap()).unwrap(),
        op
    );
    for (field, value) in [("tolerance", -1.0), ("softness", 101.0), ("lightness", 1.1)] {
        let mut json = serde_json::to_value(&op).unwrap();
        json["selection"]["color"][field] = value.into();
        let invalid: Edit = serde_json::from_value(json).unwrap();
        assert!(retouch_tools::validate(&[invalid]).is_err());
    }
    let mut bad = op.clone();
    color(&mut bad).sample = [1.01, 0.0];
    assert!(retouch_tools::validate(&[bad]).is_err());
    let legacy: Selection = serde_json::from_str("{\"inverted\":false}").unwrap();
    assert!(legacy.color.is_none());
}

#[test]
fn saturation_preserves_luminance_and_neutrals_without_new_negative_channels() {
    let input = [
        0.4, 0.42, 0.39, 0.9, 0.02, 0.03, 0.18, 0.18, 0.18, 0.0, 0.0, 0.0, 2.0, 1.0, 0.4,
    ];
    let mut op = operation();
    op.selection = None;
    for saturation in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        for vibrance in [-1.0, 0.0, 1.0] {
            op.warmth = saturation;
            op.tint = vibrance;
            let output = rendered(&input, 5, 1, &op);
            for (a, b) in input.chunks_exact(3).zip(output.chunks_exact(3)) {
                assert!((luma(a) - luma(b)).abs() < 1e-6);
                assert!(b.iter().all(|v| v.is_finite() && *v >= -1e-7));
            }
            assert_eq!(&input[6..12], &output[6..12]);
        }
    }
}

#[test]
fn vibrance_favours_muted_colours_and_identity_is_exact() {
    let input = [0.4, 0.42, 0.39, 0.9, 0.2, 0.2];
    let mut op = operation();
    op.selection = None;
    op.warmth = 0.0;
    op.tint = 1.0;
    let output = rendered(&input, 2, 1, &op);
    let gain = |a: &[f32], b: &[f32]| (b[0] - b[1]) / (a[0] - a[1]);
    assert!(gain(&input[..3], &output[..3]) > gain(&input[3..], &output[3..]));
    op.tint = 0.0;
    assert_eq!(rendered(&input, 2, 1, &op), input);
    op.warmth = 1.0;
    op.amount = 0.0;
    assert_eq!(rendered(&input, 2, 1, &op), input);
    op.amount = 1.0;
    op.enabled = false;
    assert_eq!(rendered(&input, 2, 1, &op), input);
    op.enabled = true;
    op.tool = Tool::ColorSaturation;
    assert_eq!(rendered(&input, 2, 1, &op), rendered(&input, 2, 1, &op));
}
