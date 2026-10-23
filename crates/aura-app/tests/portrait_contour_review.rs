//! Run the contour measurement against previously rendered native, aligned full-size pairs.
//! Review artifacts only: this never opens the production catalog or changes a photograph.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::print_stdout
)]

use aura_app::advanced_retouch::quality;
use aura_vision::portrait::PortraitFace;
use std::path::{Path, PathBuf};

fn ppm(path: &Path) -> (Vec<u8>, usize, usize) {
    let bytes = std::fs::read(path).expect("native review render");
    let mut parts = bytes.splitn(4, |b| *b == b'\n');
    assert_eq!(parts.next(), Some(b"P6".as_slice()));
    let dimensions = std::str::from_utf8(parts.next().unwrap()).unwrap();
    let size: Vec<usize> = dimensions
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    assert_eq!(parts.next(), Some(b"255".as_slice()));
    let rgb = parts.next().unwrap().to_vec();
    assert_eq!(size.len(), 2);
    assert_eq!(rgb.len(), size[0] * size[1] * 3);
    (rgb, size[0], size[1])
}

#[test]
#[ignore = "requires aligned native render pairs in AURA_CONTOUR_REVIEW_ROOT"]
fn inspect_full_resolution_native_contours_without_claiming_visual_parity() {
    let root = PathBuf::from(std::env::var_os("AURA_CONTOUR_REVIEW_ROOT").unwrap());
    let output = PathBuf::from(std::env::var_os("AURA_CONTOUR_REVIEW_OUT").unwrap());
    let mut results = Vec::new();
    for name in ["989A2698", "989A2699", "989A2812", "989A2843"] {
        let (before, w, h) = ppm(&root.join(format!("{name}-before.ppm")));
        let (after, aw, ah) = ppm(&root.join(format!("{name}-after.ppm")));
        assert_eq!((w, h), (aw, ah));
        let recipe: serde_json::Value = serde_json::from_slice(
            &std::fs::read(root.join(format!("{name}-recipe.json"))).unwrap(),
        )
        .unwrap();
        let faces: Vec<PortraitFace> =
            serde_json::from_value(recipe[aura_app::portrait_auto::KEY]["faces"].clone()).unwrap();
        assert!(!faces.is_empty());
        let mut readings = Vec::new();
        for (i, face) in faces.iter().enumerate() {
            let reading = quality::contour(&before, &after, w, h, face, i)
                .expect("measurable validation face");
            println!("{name}: {reading:?}");
            readings.push(reading);
        }
        results.push(serde_json::json!({"photo": name, "dimensions": [w,h], "contours": readings,
            "scope": "heuristic contour audit only; not a full visual-quality or Evoto-parity verdict"}));
    }
    std::fs::create_dir_all(output.parent().unwrap()).unwrap();
    std::fs::write(output, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
}
