//! The blemish brush on a real photograph, run by hand. ADR-0092.
//!
//! `AURA_BRUSH_PHOTO` names a packed sRGB `NAME_WxH.rgb`; `AURA_BRUSH_AT` is a `;`-separated
//! list of normalized `x,y` dabs and `AURA_BRUSH_RADIUS` the brush radius as a share of the
//! short edge (default 0.016). The result is written beside the input as `NAME_WxH.brush.rgb`,
//! for a person to look at; the assertion is only that nothing outside the strokes moves.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::disallowed_methods
)]

use aura_recipe::retouch_tools::{BrushMask, BrushStroke, Edit, Tool};

#[test]
#[ignore = "needs AURA_BRUSH_PHOTO; writes an image for a person to look at"]
fn brushes_a_real_photograph() {
    use aura_raw::colour::curve::{srgb_decode, srgb_encode};
    use aura_raw::colour::matrix::{invert, mul, REC2020_TO_XYZ_D65, SRGB_TO_XYZ_D65};
    let path = std::path::PathBuf::from(std::env::var("AURA_BRUSH_PHOTO").unwrap());
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    let (w, h) = stem.rsplit('_').next().unwrap().split_once('x').unwrap();
    let (w, h): (usize, usize) = (w.parse().unwrap(), h.parse().unwrap());
    let bytes = std::fs::read(&path).unwrap();
    let to_working =
        aura_render::colour::narrow(mul(invert(REC2020_TO_XYZ_D65).unwrap(), SRGB_TO_XYZ_D65));
    let mut rgb: Vec<f32> = bytes
        .chunks_exact(3)
        .flat_map(|p| {
            aura_render::colour::apply_f32(
                to_working,
                [p[0], p[1], p[2]].map(|v| srgb_decode(f32::from(v) / 255.0)),
            )
        })
        .collect();
    let radius: f32 = std::env::var("AURA_BRUSH_RADIUS").map_or(0.016, |v| v.parse().unwrap());
    let points: Vec<[f32; 3]> = std::env::var("AURA_BRUSH_AT")
        .unwrap()
        .split(';')
        .map(|p| {
            let (x, y) = p.split_once(',').unwrap();
            [x.parse().unwrap(), y.parse().unwrap(), 1.0]
        })
        .collect();
    let edit = Edit {
        id: "brush".into(),
        tool: Tool::AcneClear,
        enabled: true,
        region: [points[0][0], points[0][1], 0.035, 0.035],
        source: None,
        amount: 1.0,
        feather: 0.35,
        radius: 0.005,
        source_scale: 1.0,
        preserve_microtexture: true,
        texture_heal: false,
        clean_ring_fit: false,
        curved_heal: false,
        heal_samples: Vec::new(),
        texture_sources: Vec::new(),
        sensitivity: Some(0.75),
        keep_dark_marks: false,
        texture: 0.25,
        tone: 1.0,
        warmth: 0.0,
        tint: 0.0,
        selection: None,
        matte: None,
        mask: Some(BrushMask {
            strokes: points
                .iter()
                .map(|p| BrushStroke {
                    erase: false,
                    radius,
                    opacity: 1.0,
                    points: vec![*p],
                })
                .collect(),
        }),
        skin: None,
    };
    aura_recipe::retouch_tools::validate(std::slice::from_ref(&edit)).unwrap();
    let before = rgb.clone();
    aura_render::retouch_tools::apply(&mut rgb, w, h, std::slice::from_ref(&edit));
    let reach = radius * w.min(h) as f32 + 1.0;
    for (i, (a, b)) in rgb.chunks_exact(3).zip(before.chunks_exact(3)).enumerate() {
        let (x, y) = ((i % w) as f32, (i / w) as f32);
        let far = points
            .iter()
            .all(|p| (x - p[0] * w as f32).hypot(y - p[1] * h as f32) > reach + 1.0);
        if far {
            assert_eq!(a, b, "pixel {x},{y} outside the strokes moved");
        }
    }
    let to_display = aura_render::output::working_to_output(aura_render::OutputColour::Srgb);
    let out: Vec<u8> = rgb
        .chunks_exact(3)
        .flat_map(|p| {
            aura_render::colour::apply_f32(to_display, [p[0], p[1], p[2]])
                .map(|v| (srgb_encode(v.clamp(0.0, 1.0)) * 255.0).round() as u8)
        })
        .collect();
    std::fs::write(path.with_file_name(format!("{stem}.brush.rgb")), out).unwrap();
}
