use super::*;
#[test]
fn silhouette_refuses_empty_and_round_regions_and_measures_tilted_axes() {
    let make = |alpha| aura_vision::skin::Matte {
        bounds: [0.0, 0.0, 1.0, 1.0],
        width: 20,
        height: 30,
        alpha,
    };
    assert_eq!(silhouette(&make(vec![0; 600]), 20.0 / 30.0), (None, None));
    let alpha = (0..600)
        .map(|i| {
            let y = i / 20;
            let x = i % 20;
            if (8..=11).contains(&x) && (2..28).contains(&y) {
                255
            } else {
                0
            }
        })
        .collect();
    let (bounds, axis) = silhouette(&make(alpha), 20.0 / 30.0);
    assert!(bounds.is_some());
    assert!((axis.expect("axis").abs() - 90.0).abs() < 0.01);
    let alpha = (0..600)
        .map(|i| {
            let y = i / 20;
            let x = i % 20;
            if (4..16).contains(&x) && (6..24).contains(&y) {
                255
            } else {
                0
            }
        })
        .collect();
    assert!(silhouette(&make(alpha), 1.0).1.is_none());
}
