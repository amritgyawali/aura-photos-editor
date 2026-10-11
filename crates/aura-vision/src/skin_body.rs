//! Body-only segmentation for crops that contain no detectable face.
use super::{
    guided, invalid, segment, to_matte, working, Class, Field, Matte, Options, SIDE, WORKING_EDGE,
};
use aura_core::AuraResult;

fn body_probability(field: &Field) -> Vec<f32> {
    field
        .plane(Class::BodySkin)
        .iter()
        .enumerate()
        .map(|(i, body)| {
            let other = Class::ALL
                .iter()
                .filter(|c| **c != Class::BodySkin)
                .map(|c| field.plane(*c)[i])
                .fold(0.0_f32, f32::max);
            // A missing face detector cannot authorize healing facial features. Use
            // body skin only, with the network's competing labels as an explicit veto.
            if *body > other && *body > 0.5 {
                *body
            } else {
                0.0
            }
        })
        .collect()
}

/// Select visible body skin without requiring a face or inventing face landmarks.
/// # Errors
/// Invalid pixels, unavailable bundled segmentation, or inference failure.
pub fn analyse_body(
    rgb: &[u8],
    width: u32,
    height: u32,
    options: Options,
) -> AuraResult<(Option<Matte>, usize)> {
    let (w, h) = (width as usize, height as usize);
    if w < 8 || h < 8 || w.checked_mul(h).and_then(|n| n.checked_mul(3)) != Some(rgb.len()) {
        return Err(invalid("Invalid body skin analysis pixels"));
    }
    let image = working(rgb, w, h, WORKING_EDGE);
    let (field, passes) = segment(&image, &[], 0)?;
    let support = body_probability(&field);
    let softness = if options.softness.is_finite() {
        options.softness.clamp(0.0, 1.0)
    } else {
        0.35
    };
    let cell = (image.w.max(image.h) as f32 / SIDE as f32).max(1.0);
    let radius = ((cell * (0.8 + softness * 2.2)).round() as usize).max(1);
    let mut refined = guided(
        &image.luma(),
        &support,
        image.w,
        image.h,
        radius,
        0.0004 + softness * 0.004,
    );
    for (value, allowed) in refined.iter_mut().zip(&support) {
        if *allowed == 0.0 {
            *value = 0.0;
        }
    }
    Ok((to_matte(&refined, image.w, image.h, 0.03), passes))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn body_only_detection_rejects_face_clothing_hair_and_background() {
        let mut field = Field {
            planes: vec![vec![0.02; 6]; 6],
        };
        for i in 0..6 {
            field.planes[i][i] = 0.9;
        }
        let body = body_probability(&field);
        for (i, probability) in body.iter().enumerate() {
            let expected = if i == Class::BodySkin as usize {
                0.9
            } else {
                0.0
            };
            assert!((*probability - expected).abs() < f32::EPSILON);
        }
    }
}
