// Tests assert by unwrapping and stamp a real file time; neither reaches a photographer.
#![allow(clippy::unwrap_used, clippy::disallowed_methods)]

use aura_core::contract::delivery::DeliveryColour;
use aura_export::{
    read::{Rendered, Samples},
    watermark::{Anchor, Watermark},
};

fn mark() -> Watermark {
    Watermark {
        width: 1,
        height: 1,
        rgba: vec![255, 255, 255, 128],
        opacity: 1.0,
        width_fraction: 0.5,
        margin_fraction: 0.0,
        anchor: Anchor::BottomRight,
    }
}
fn frame(sixteen: bool, colour: DeliveryColour) -> Rendered {
    Rendered {
        width: 4,
        height: 4,
        data: if sixteen {
            Samples::Sixteen(vec![0; 48])
        } else {
            Samples::Eight(vec![0; 48])
        },
        colour,
        render_hash: "base".into(),
    }
}

#[test]
fn watermark_blends_in_linear_light_at_requested_anchor_without_changing_source() {
    let source = frame(false, DeliveryColour::Srgb);
    let output = mark().apply(&source).unwrap();
    let Samples::Eight(pixels) = &output.data else {
        panic!("depth changed")
    };
    assert_eq!(&pixels[0..6], &[0; 6]);
    assert!(
        (i32::from(pixels[30]) - 188).abs() <= 1,
        "half white should be 188, not 128"
    );
    assert_eq!(source.data, Samples::Eight(vec![0; 48]));
    assert_ne!(output.render_hash, source.render_hash);
    assert_eq!(output, mark().apply(&source).unwrap());
}

#[test]
fn watermark_preserves_sixteen_bit_and_handles_all_output_primaries() {
    for colour in [
        DeliveryColour::Srgb,
        DeliveryColour::AdobeRgb,
        DeliveryColour::DisplayP3,
    ] {
        let output = mark().apply(&frame(true, colour)).unwrap();
        let Samples::Sixteen(pixels) = output.data else {
            panic!("depth changed")
        };
        assert!(pixels[30] > 45_000 && pixels[30] < 50_000);
        // The existing standard-primary matrices use slightly different rounded D65
        // whites (about 0.0002 linear). Allow their sub-0.02% encoded quantization drift.
        assert!((i32::from(pixels[30]) - i32::from(pixels[31])).abs() <= 12);
        assert_eq!(output.colour, colour);
    }
}

#[test]
fn zero_alpha_is_exact_identity_and_invalid_payloads_are_rejected() {
    let source = frame(false, DeliveryColour::Srgb);
    let mut overlay = mark();
    overlay.rgba[3] = 0;
    assert_eq!(overlay.apply(&source).unwrap(), source);
    overlay.opacity = f32::NAN;
    assert!(overlay.validate().is_err());
    overlay.opacity = 1.0;
    overlay.rgba.pop();
    assert!(overlay.validate().is_err());
    overlay.rgba.push(255);
    overlay.width = u32::MAX;
    assert!(overlay.validate().is_err());
}

#[test]
fn archived_graphic_is_reusable_and_a_mismatched_existing_file_is_never_replaced() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "aura-watermark-test-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&root).unwrap();
    let archived = mark().archive(&root).unwrap();
    let path = root.join(&archived.0);
    let original = std::fs::read(&path).unwrap();
    assert!(original.starts_with(b"{\"width\":1,\"height\":1,\"rgba\":[255,255,255,128]"));
    assert_eq!(mark().archive(&root).unwrap(), archived);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::write(&path, b"different existing content").unwrap();
    assert!(mark().archive(&root).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"different existing content");
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir(&root).unwrap();
}
