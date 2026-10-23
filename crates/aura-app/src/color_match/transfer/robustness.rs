//! Adversarial image content, independent of the engine's own generated grades.
use super::*;

#[test]
fn a_dominant_object_does_not_replace_the_neutral_axis() {
    let cast = [0.012, -0.018];
    for object in [[0.18, 0.03], [-0.12, 0.08], [0.02, -0.2]] {
        let labs: Vec<_> = (0..2000)
            .map(|i| {
                let c = if i % 10 == 0 { cast } else { object };
                [0.5, c[0], c[1]]
            })
            .collect();
        let (axis, evidence) = neutral_axis(&labs);
        assert!(evidence[4] > 0.9);
        let c = axis[4];
        assert!(
            (c[0] - cast[0]).hypot(c[1] - cast[1]) < 1e-5,
            "{object:?}: {c:?}"
        );
        assert!(axis
            .iter()
            .all(|c| c[0].hypot(c[1]) <= 2.0 * cast[0].hypot(cast[1]) + 1e-5));
    }
}

#[test]
fn an_object_without_neutrals_has_no_white_balance_evidence() {
    let labs = vec![[0.5, 0.15, 0.08]; 2000];
    let stats = measure(&labs);
    assert!(stats.axis_weight.iter().all(|w| w.abs() < 1e-6));
    assert!(stats.axis.iter().all(|c| c[0].hypot(c[1]) < 1e-6));
    assert!(
        !stats.achromatic,
        "a single coloured subject is not a monochrome edit"
    );
}

#[test]
fn split_tone_evidence_is_kept_separate_at_different_lightnesses() {
    let labs: Vec<_> = (0..4000)
        .map(|i| {
            if i % 2 == 0 {
                [0.25, -0.008, -0.028]
            } else {
                [0.75, 0.01, 0.03]
            }
        })
        .collect();
    let (axis, evidence) = neutral_axis(&labs);
    assert!(evidence[2] > 0.9 && evidence[6] > 0.9);
    assert!(axis[2][1] < -0.02 && axis[6][1] > 0.02);
}

#[test]
fn a_small_selective_colour_detail_is_not_turned_monochrome() {
    let mut labs = vec![[0.5, 0.0, 0.0]; 4000];
    assert!(measure(&labs).achromatic);
    labs[123] = [0.5, 0.13, 0.07];
    assert!(!measure(&labs).achromatic);
    assert!(
        !measure(&vec![[0.5, 0.025, 0.06]; 4000]).achromatic,
        "toned monochrome must retain its visible cast"
    );
}

#[test]
fn spatial_sampling_obeys_the_budget_for_every_aspect_ratio() {
    for (width, height) in [(8000, 8), (8, 8000), (257, 263), (1, 4096), (4096, 1)] {
        let image = Rgb8 {
            width,
            height,
            data: vec![128; (width * height * 3) as usize],
        };
        for limit in [0, 1, 17, 1000, usize::MAX] {
            let pixels = samples(&image, limit);
            assert!(pixels.len() <= limit && pixels.len() <= (width * height) as usize);
            if limit >= (width * height) as usize {
                assert_eq!(pixels.len(), (width * height) as usize);
            }
        }
    }
    let invalid = Rgb8 {
        width: u32::MAX,
        height: u32::MAX,
        data: Vec::new(),
    };
    assert!(samples(&invalid, 10).is_empty());
}

#[test]
fn regular_fabric_is_not_measured_as_only_one_colour() {
    // The earlier even grid used step=2 and hit only the red stripes.
    let image = Rgb8 {
        width: 512,
        height: 512,
        data: (0..512 * 512)
            .flat_map(|i| {
                if i % 2 == 0 {
                    [220, 40, 30]
                } else {
                    [30, 50, 220]
                }
            })
            .collect(),
    };
    let pixels = samples(&image, 65_536);
    let red = pixels.iter().filter(|s| s.linear[0] > s.linear[2]).count();
    let fraction = red as f32 / pixels.len() as f32;
    assert!((0.48..=0.52).contains(&fraction), "red fraction {fraction}");
    let again = samples(&image, 65_536);
    assert!(pixels.iter().zip(&again).all(|(a, b)| a.linear == b.linear));
}

#[test]
fn an_already_matched_image_keeps_the_exact_identity() {
    let image = super::tests::outdoor();
    let pixels = samples(&image, TARGET_SAMPLES);
    let stats = measure(&pixels.iter().map(|s| s.lab).collect::<Vec<_>>());
    let fitted = fit(&stats, &pixels, Some(1.0));
    assert_eq!(fitted.grade, ReferenceGrade::identity());
    assert_eq!(fitted.iterations, 0);
    assert_eq!(fitted.before, fitted.after);
}

#[test]
fn cancellation_interrupts_fitting_between_colour_updates() {
    let image = super::tests::outdoor();
    let pixels = samples(&image, TARGET_SAMPLES);
    let mut stats = measure(&pixels.iter().map(|s| s.lab).collect::<Vec<_>>());
    stats.lightness.iter_mut().for_each(|v| *v *= 0.8);
    let mut checks = 0;
    let fitted = fit_with_checkpoint(&stats, &pixels, None, || {
        checks += 1;
        if checks == 3 {
            Err(super::super::invalid("cancelled fit"))
        } else {
            Ok(())
        }
    });
    assert_eq!(fitted.unwrap_err().user_message, "cancelled fit");
    assert_eq!(checks, 3);
}
