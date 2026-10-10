//! Learned selections for masking: the subject, the sky, and an object somebody points at.
//! ADR-0103.
//!
//! Three networks, run by ONNX Runtime (`aura_infer::accelerated`) on the graphics card where
//! there is one:
//!
//! * **IS-Net general use** (Apache-2.0) for the subject - whatever the photograph is of, at
//!   1024 pixels. BiRefNet-lite was tried first and needs more memory than an 8 GB laptop has
//!   (its deformable convolutions expand to buffers of most of a gigabyte), on the card and
//!   on the processor;
//! * **SkySeg**, a U²-Net (MIT), for sky, clouds and sunsets included;
//! * **Segment Anything 2.1, tiny** (Apache-2.0) for an object inside a box someone draws.
//!
//! Each returns a soft matte over the whole frame at most [`MATTE_EDGE`] on its long side; the
//! renderer refines its edge against the photograph at full resolution. When a model or the
//! runtime is not installed the error says so, and the caller falls back to what it did before.
// Every index below comes from validated dimensions; values are clamped before they become
// bytes, and grid sizes are small enough to be exact in f32.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::doc_markdown
)]

use std::sync::OnceLock;

use aura_infer::accelerated::{Array, Model};

use crate::skin::Matte;

/// Long edge of a stored learned matte.
pub const MATTE_EDGE: usize = 512;

/// A pinned model: file name and SHA-256.
#[derive(Debug, Clone, Copy)]
pub struct Pinned {
    pub file: &'static str,
    pub sha256: &'static str,
}

pub const SUBJECT: Pinned = Pinned {
    file: "isnet_general.onnx",
    sha256: "60920e99c45464f2ba57bee2ad08c919a52bbf852739e96947fbb4358c0d964a",
};
pub const SKY: Pinned = Pinned {
    file: "skyseg.onnx",
    sha256: "ab9c34c64c3d821220a2886a4a06da4642ffa14d5b30e8d5339056a089aa1d39",
};
pub const OBJECT_ENCODER: Pinned = Pinned {
    file: "sam21_tiny_encoder.onnx",
    sha256: "667384d1e686de6828b841ac8a24db0fafa2b3452494225f82eeedac56141230",
};
pub const OBJECT_DECODER: Pinned = Pinned {
    file: "sam21_tiny_decoder.onnx",
    sha256: "c40f5aa7d37b681cd500481a85d44839fd81c93dce1e86271a2c866470d22105",
};

/// Every model this module can use, for an installer or a status panel.
pub const ALL: [Pinned; 4] = [SUBJECT, SKY, OBJECT_ENCODER, OBJECT_DECODER];

fn load(
    slot: &'static OnceLock<Result<Model, String>>,
    pinned: Pinned,
) -> Result<&'static Model, String> {
    slot.get_or_init(|| Model::open(pinned.file, pinned.sha256))
        .as_ref()
        .map_err(Clone::clone)
}

fn subject_model() -> Result<&'static Model, String> {
    static SLOT: OnceLock<Result<Model, String>> = OnceLock::new();
    load(&SLOT, SUBJECT)
}

fn sky_model() -> Result<&'static Model, String> {
    static SLOT: OnceLock<Result<Model, String>> = OnceLock::new();
    load(&SLOT, SKY)
}

fn object_models() -> Result<(&'static Model, &'static Model), String> {
    static ENCODER: OnceLock<Result<Model, String>> = OnceLock::new();
    static DECODER: OnceLock<Result<Model, String>> = OnceLock::new();
    Ok((
        load(&ENCODER, OBJECT_ENCODER)?,
        load(&DECODER, OBJECT_DECODER)?,
    ))
}

/// Load every installed model in the background, so the first selection a photographer asks
/// for does not wait for a model to be verified and compiled for the graphics card.
pub fn warm_up() {
    let _ = std::thread::Builder::new()
        .name("aura-ai-warm-up".into())
        .spawn(|| {
            let _ = subject_model();
            let _ = sky_model();
            let _ = object_models();
        });
}

/// Which learned selections this machine can make, without loading any of them: the runtime
/// is there and the model files are installed.
#[must_use]
pub fn installed() -> Vec<(&'static str, bool)> {
    let runtime = aura_infer::accelerated::runtime().is_ok();
    let present = |p: Pinned| {
        runtime && aura_infer::accelerated::models_dir().is_some_and(|d| d.join(p.file).is_file())
    };
    vec![
        ("subject", present(SUBJECT)),
        ("sky", present(SKY)),
        (
            "objects",
            present(OBJECT_ENCODER) && present(OBJECT_DECODER),
        ),
    ]
}

/// ImageNet's mean and spread, which the sky and object models were trained with.
const IMAGENET: ([f32; 3], [f32; 3]) = ([0.485, 0.456, 0.406], [0.229, 0.224, 0.225]);
/// IS-Net's own: centred, not scaled.
const CENTRED: ([f32; 3], [f32; 3]) = ([0.5, 0.5, 0.5], [1.0, 1.0, 1.0]);

/// Packed sRGB resampled bilinearly to `side x side`, normalised and laid out NCHW.
fn tensor(rgb: &[u8], w: usize, h: usize, side: usize, norm: ([f32; 3], [f32; 3])) -> Array {
    let (mean, std) = norm;
    let mut values = vec![0.0_f32; 3 * side * side];
    let at = |x: usize, y: usize, c: usize| {
        f32::from(rgb[(y.min(h - 1) * w + x.min(w - 1)) * 3 + c]) / 255.0
    };
    for y in 0..side {
        let fy = ((y as f32 + 0.5) * h as f32 / side as f32 - 0.5).clamp(0.0, (h - 1) as f32);
        let (y0, ty) = (fy as usize, fy.fract());
        for x in 0..side {
            let fx = ((x as f32 + 0.5) * w as f32 / side as f32 - 0.5).clamp(0.0, (w - 1) as f32);
            let (x0, tx) = (fx as usize, fx.fract());
            for c in 0..3 {
                let top = at(x0, y0, c) * (1.0 - tx) + at(x0 + 1, y0, c) * tx;
                let bottom = at(x0, y0 + 1, c) * (1.0 - tx) + at(x0 + 1, y0 + 1, c) * tx;
                let v = top * (1.0 - ty) + bottom * ty;
                values[c * side * side + y * side + x] = (v - mean[c]) / std[c];
            }
        }
    }
    Array {
        shape: vec![1, 3, side, side],
        values,
    }
}

/// A `pw x ph` probability plane resampled onto a matte over the whole frame.
fn matte(plane: &[f32], pw: usize, ph: usize, w: usize, h: usize) -> Matte {
    let (mw, mh) = if w >= h {
        (
            MATTE_EDGE.min(w),
            (MATTE_EDGE.min(w) * h).div_ceil(w).max(1),
        )
    } else {
        (
            (MATTE_EDGE.min(h) * w).div_ceil(h).max(1),
            MATTE_EDGE.min(h),
        )
    };
    let at = |x: usize, y: usize| plane[y.min(ph - 1) * pw + x.min(pw - 1)];
    let mut alpha = Vec::with_capacity(mw * mh);
    for y in 0..mh {
        let fy = ((y as f32 + 0.5) * ph as f32 / mh as f32 - 0.5).clamp(0.0, (ph - 1) as f32);
        let (y0, ty) = (fy as usize, fy.fract());
        for x in 0..mw {
            let fx = ((x as f32 + 0.5) * pw as f32 / mw as f32 - 0.5).clamp(0.0, (pw - 1) as f32);
            let (x0, tx) = (fx as usize, fx.fract());
            let top = at(x0, y0) * (1.0 - tx) + at(x0 + 1, y0) * tx;
            let bottom = at(x0, y0 + 1) * (1.0 - tx) + at(x0 + 1, y0 + 1) * tx;
            let v = top * (1.0 - ty) + bottom * ty;
            alpha.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    Matte {
        bounds: [0.0, 0.0, 1.0, 1.0],
        width: mw,
        height: mh,
        alpha,
    }
}

fn sigmoid(v: f32) -> f32 {
    1.0 / (1.0 + (-v).exp())
}

fn check(rgb: &[u8], width: u32, height: u32) -> Result<(usize, usize), String> {
    let (w, h) = (width as usize, height as usize);
    if w < 8 || h < 8 || rgb.len() != w * h * 3 {
        return Err("invalid pixels".into());
    }
    Ok((w, h))
}

/// The first output as a square probability plane.
fn plane(outputs: &[(String, Array)], logits: bool) -> Result<(Vec<f32>, usize, usize), String> {
    let (_, out) = outputs.first().ok_or("the model returned nothing")?;
    let dims = out.shape.len();
    if dims < 2 {
        return Err("unexpected output shape".into());
    }
    let (ph, pw) = (out.shape[dims - 2], out.shape[dims - 1]);
    let values = out
        .values
        .get(..pw * ph)
        .ok_or("short output")?
        .iter()
        .map(|v| if logits { sigmoid(*v) } else { *v })
        .collect();
    Ok((values, pw, ph))
}

/// The subject of a packed sRGB photograph: people, an animal, a bouquet, a car.
///
/// # Errors
/// The model or the runtime is not installed, or the run failed.
pub fn subject(rgb: &[u8], width: u32, height: u32) -> Result<Matte, String> {
    let (w, h) = check(rgb, width, height)?;
    let model = subject_model()?;
    let input = model.inputs().first().cloned().unwrap_or_default();
    let first = model.outputs().first().cloned().unwrap_or_default();
    let outputs = model.run_for(vec![(input, tensor(rgb, w, h, 1024, CENTRED))], &[&first])?;
    let (values, pw, ph) = plane(&outputs, false)?;
    Ok(matte(&values, pw, ph, w, h))
}

/// The sky of a packed sRGB photograph, clouds and sunset included. `Ok(None)` when there is
/// essentially none.
///
/// # Errors
/// The model or the runtime is not installed, or the run failed.
pub fn sky(rgb: &[u8], width: u32, height: u32) -> Result<Option<Matte>, String> {
    let (w, h) = check(rgb, width, height)?;
    let model = sky_model()?;
    let input = model.inputs().first().cloned().unwrap_or_default();
    let first = model.outputs().first().cloned().unwrap_or_default();
    let outputs = model.run_for(vec![(input, tensor(rgb, w, h, 320, IMAGENET))], &[&first])?;
    let (values, pw, ph) = plane(&outputs, false)?;
    // A probability already; anything the model is unsure of stays soft.
    let found = matte(&values, pw, ph, w, h);
    Ok(plausible_sky(&found).then_some(found))
}

/// Whether what the sky model found is open sky: some of it, reaching the top of the frame,
/// and with ground under it. The model also answers "sky" for a plain studio backdrop and for
/// out-of-focus background behind a close-up - smooth, bright, everywhere - and both of those
/// run down to the bottom of the frame, which a sky does not.
fn plausible_sky(m: &Matte) -> bool {
    let (w, h) = (m.width, m.height);
    if w == 0 || h == 0 {
        return false;
    }
    let on = |x: usize, y: usize| m.alpha[y * w + x] > 127;
    let area = m.alpha.iter().filter(|a| **a > 127).count() as f32 / (w * h) as f32;
    let top_rows = (h / 50).max(1);
    let top = (0..w).filter(|x| (0..top_rows).any(|y| on(*x, y))).count() as f32 / w as f32;
    let bottom_rows = h - h * 9 / 10;
    let bottom = (0..w)
        .filter(|x| (h - bottom_rows..h).any(|y| on(*x, y)))
        .count() as f32
        / w as f32;
    area >= 0.01 && top >= 0.2 && bottom <= 0.3
}

/// The object inside `bounds` (normalised left, top, right, bottom) of a packed sRGB
/// photograph, as Lightroom's Objects tool.
///
/// # Errors
/// The models or the runtime are not installed, the box is empty, or the run failed.
pub fn object(rgb: &[u8], width: u32, height: u32, bounds: [f32; 4]) -> Result<Matte, String> {
    let (w, h) = check(rgb, width, height)?;
    let [l, t, r, b] = bounds.map(|v| v.clamp(0.0, 1.0));
    if r - l < 0.005 || b - t < 0.005 {
        return Err("draw a box around the object".into());
    }
    let (encoder, decoder) = object_models()?;
    let image = encoder.inputs().first().cloned().unwrap_or_default();
    let encoded = encoder.run(vec![(image, tensor(rgb, w, h, 1024, IMAGENET))])?;
    let feature = |name: &str| {
        encoded
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, a)| a.clone())
            .ok_or_else(|| format!("the encoder returned no {name}"))
    };
    // A box is its two corners, labelled 2 and 3, in the encoder's 1024-pixel space.
    let coords = Array {
        shape: vec![1, 2, 2],
        values: vec![l * 1024.0, t * 1024.0, r * 1024.0, b * 1024.0],
    };
    let labels = Array {
        shape: vec![1, 2],
        values: vec![2.0, 3.0],
    };
    let outputs = decoder.run(vec![
        ("image_embed".into(), feature("image_embed")?),
        ("high_res_feats_0".into(), feature("high_res_feats_0")?),
        ("high_res_feats_1".into(), feature("high_res_feats_1")?),
        ("point_coords".into(), coords),
        ("point_labels".into(), labels),
        (
            "mask_input".into(),
            Array {
                shape: vec![1, 1, 256, 256],
                values: vec![0.0; 256 * 256],
            },
        ),
        (
            "has_mask_input".into(),
            Array {
                shape: vec![1],
                values: vec![0.0],
            },
        ),
    ])?;
    let masks = outputs
        .iter()
        .find(|(n, _)| n == "masks")
        .map(|(_, a)| a)
        .ok_or("the decoder returned no mask")?;
    let scores = outputs
        .iter()
        .find(|(n, _)| n == "iou_predictions")
        .map(|(_, a)| a.values.clone())
        .unwrap_or_default();
    let dims = masks.shape.len();
    if dims < 3 {
        return Err("unexpected mask shape".into());
    }
    let (mh, mw) = (masks.shape[dims - 2], masks.shape[dims - 1]);
    let count = masks.values.len() / (mw * mh).max(1);
    let best = (0..count)
        .max_by(|a, b| {
            scores
                .get(*a)
                .unwrap_or(&0.0)
                .total_cmp(scores.get(*b).unwrap_or(&0.0))
        })
        .unwrap_or(0);
    let start = best * mw * mh;
    let logits = masks
        .values
        .get(start..start + mw * mh)
        .ok_or("short mask")?;
    let probability: Vec<f32> = logits.iter().map(|v| sigmoid(*v)).collect();
    let found = matte(&probability, mw, mh, w, h);
    // Nothing outside the box the person drew.
    let alpha = found
        .alpha
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let (x, y) = (
                ((i % found.width) as f32 + 0.5) / found.width as f32,
                ((i / found.width) as f32 + 0.5) / found.height as f32,
            );
            let margin = 0.01;
            if x < l - margin || x > r + margin || y < t - margin || y > b + margin {
                0
            } else {
                *a
            }
        })
        .collect();
    Ok(Matte { alpha, ..found })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(w: usize, h: usize, f: impl Fn(usize, usize) -> bool) -> Matte {
        Matte {
            bounds: [0.0, 0.0, 1.0, 1.0],
            width: w,
            height: h,
            alpha: (0..w * h)
                .map(|i| if f(i % w, i / w) { 255 } else { 0 })
                .collect(),
        }
    }

    #[test]
    fn a_sky_reaches_the_top_and_has_ground_under_it() {
        assert!(plausible_sky(&grid(100, 60, |_, y| y < 25)));
        // A studio backdrop: everywhere but the person, down to the bottom.
        assert!(!plausible_sky(&grid(100, 60, |x, y| !(40..60)
            .contains(&x)
            || y < 10)));
        // A patch in the middle of the frame is not sky.
        assert!(!plausible_sky(&grid(100, 60, |x, y| (40..60).contains(&x)
            && (20..30).contains(&y))));
    }

    #[test]
    fn a_matte_covers_the_frame_at_most_matte_edge_long() {
        let m = matte(&[0.0, 1.0, 0.0, 1.0], 2, 2, 3000, 2000);
        assert_eq!((m.width, m.height), (512, 342));
        assert!(m.at(0.9, 0.5) > 0.9 && m.at(0.1, 0.5) < 0.1);
    }

    #[test]
    fn without_models_every_selection_says_why() {
        let rgb = vec![128_u8; 16 * 16 * 3];
        if installed().iter().all(|(_, ok)| !ok) {
            assert!(subject(&rgb, 16, 16).is_err());
            assert!(sky(&rgb, 16, 16).is_err());
            assert!(object(&rgb, 16, 16, [0.2, 0.2, 0.8, 0.8]).is_err());
        }
        assert!(object(&rgb, 16, 16, [0.5, 0.5, 0.5, 0.5]).is_err());
    }
}
