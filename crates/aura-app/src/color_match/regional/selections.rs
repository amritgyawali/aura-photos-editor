//! Frozen class selections in the coordinate system where local masks are rendered.
use std::collections::BTreeMap;

use aura_core::AuraResult;
use aura_raw::codec::Rgb8;
use aura_recipe::retouch_tools::Matte;
use aura_vision::{portrait, skin};

use super::Kind;

pub(super) fn validate(matte: &Matte) -> AuraResult<()> {
    let [left, top, right, bottom] = matte.bounds;
    if matte.width > 512
        || matte.height > 512
        || !matte
            .bounds
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        || right <= left
        || bottom <= top
        || matte.decode().is_none()
    {
        return Err(super::invalid(
            "A saved regional selection is invalid. Analyse again.",
        ));
    }
    Ok(())
}

pub(super) fn analyse(image: &Rgb8) -> AuraResult<BTreeMap<Kind, Matte>> {
    let faces = portrait::detect(&image.data, image.width, image.height)?;
    let masks = skin::analyse(
        &image.data,
        image.width,
        image.height,
        &faces,
        skin::Options::default(),
    )?;
    let mut selections = BTreeMap::new();
    for kind in Kind::ALL {
        let found: Vec<&skin::Matte> = if kind == Kind::Background {
            masks.background.iter().collect()
        } else {
            masks
                .people
                .iter()
                .filter_map(|person| match kind {
                    Kind::FaceSkin => person.face.as_ref(),
                    Kind::BodySkin => person.body.as_ref(),
                    Kind::Hair => person.hair.as_ref(),
                    Kind::Clothes => person.clothes.as_ref(),
                    Kind::Background => None,
                })
                .collect()
        };
        if found.is_empty() {
            continue;
        }
        // All classes use one bounded grid; probabilities are retained, never promoted to a
        // geometric box or a whole-frame mask when a class is missing.
        let side = 256_u32;
        let mut alpha = Vec::with_capacity((side * side) as usize);
        for y in 0..side {
            for x in 0..side {
                let u = (x as f32 + 0.5) / side as f32;
                let v = (y as f32 + 0.5) / side as f32;
                let coverage = found.iter().map(|m| m.at(u, v)).fold(0.0_f32, f32::max);
                alpha.push((coverage.clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        if alpha.iter().filter(|a| **a >= 217).count() < 128 {
            continue;
        }
        let mut matte = Matte::encode([0.0, 0.0, 1.0, 1.0], side, side, &alpha);
        // Tighten uncertain contours without ever reopening an excluded zero cell.
        matte.refine_contour = true;
        selections.insert(kind, matte);
    }
    Ok(selections)
}
