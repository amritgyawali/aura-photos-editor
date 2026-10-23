//! Content-normalized descriptors. Segmentation selects analysis samples; it never paints pixels.
use aura_core::contract::look::LookAggregate;
use aura_core::AuraResult;
use aura_look::{aggregate, measure, solve};
use aura_raw::codec::Rgb8;
use aura_vision::{portrait, skin};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Region {
    pub name: String,
    pub points: Vec<[f32; 2]>,
    pub appearance: LookAggregate,
}

fn describe(image: &Rgb8) -> LookAggregate {
    aggregate::fold(&[&measure::read_at(
        "colour-match",
        image,
        image.width.max(image.height),
    )])
}

fn selected_region(image: &Rgb8, name: &str, mattes: &[&skin::Matte]) -> Option<Region> {
    let mut points = Vec::new();
    // Reject uncertain borders and their immediate neighbours. The fixed grid keeps
    // a large class from receiving more weight just because it covers more pixels.
    for row in 1..127 {
        for column in 1..127 {
            let x = (column as f32 + 0.5) / 128.0;
            let y = (row as f32 + 0.5) / 128.0;
            let inside = |dx: f32, dy: f32| mattes.iter().any(|m| m.at(x + dx, y + dy) >= 0.85);
            if inside(0.0, 0.0)
                && inside(-1.0 / 128.0, 0.0)
                && inside(1.0 / 128.0, 0.0)
                && inside(0.0, -1.0 / 128.0)
                && inside(0.0, 1.0 / 128.0)
            {
                points.push([x, y]);
            }
        }
    }
    let stride = points.len().div_ceil(2048).max(1);
    let points: Vec<_> = points.into_iter().step_by(stride).collect();
    let appearance = at_points(image, &points)?;
    Some(Region {
        name: name.into(),
        points,
        appearance,
    })
}

pub(super) fn at_points(image: &Rgb8, points: &[[f32; 2]]) -> Option<LookAggregate> {
    let mut samples = Vec::with_capacity(points.len() * 3);
    for &[x, y] in points {
        let column =
            ((x * image.width as f32) as usize).min(image.width.saturating_sub(1) as usize);
        let row = ((y * image.height as f32) as usize).min(image.height.saturating_sub(1) as usize);
        let at = (row * image.width as usize + column) * 3;
        samples.extend_from_slice(image.data.get(at..at + 3)?);
    }
    if samples.len() < 32 * 3 {
        return None;
    }
    Some(describe(&Rgb8 {
        width: (samples.len() / 3) as u32,
        height: 1,
        data: samples,
    }))
}

pub(super) fn analyse(image: &Rgb8) -> AuraResult<Vec<Region>> {
    let faces = portrait::detect(&image.data, image.width, image.height)?;
    let masks = skin::analyse(
        &image.data,
        image.width,
        image.height,
        &faces,
        skin::Options::default(),
    )?;
    let mut regions = vec![Region {
        name: "whole image".into(),
        points: Vec::new(),
        appearance: describe(image),
    }];
    for name in ["face skin", "body skin", "hair", "clothes", "background"] {
        let mattes: Vec<&skin::Matte> = if name == "background" {
            masks.background.iter().collect()
        } else {
            masks
                .people
                .iter()
                .filter_map(|person| match name {
                    "face skin" => person.face.as_ref(),
                    "body skin" => person.body.as_ref(),
                    "hair" => person.hair.as_ref(),
                    _ => person.clothes.as_ref(),
                })
                .collect()
        };
        if let Some(region) = selected_region(image, name, &mattes) {
            regions.push(region);
        }
    }
    // Guard every supported person separately. An unchanged pooled skin statistic
    // must not conceal damage to one person in a group photograph. These regions
    // are not matched to another person's skin in the reference.
    for (index, person) in masks.people.iter().enumerate() {
        for (kind, matte) in [("face skin", &person.face), ("body skin", &person.body)] {
            if let Some(matte) = matte {
                if let Some(region) =
                    selected_region(image, &format!("guard:person-{index} {kind}"), &[matte])
                {
                    regions.push(region);
                }
            }
        }
    }
    Ok(regions)
}

pub(super) fn readings(image: &Rgb8, regions: &[Region]) -> Vec<Region> {
    regions
        .iter()
        .filter_map(|r| {
            let appearance = if r.points.is_empty() {
                Some(describe(image))
            } else {
                at_points(image, &r.points)
            }?;
            Some(Region {
                name: r.name.clone(),
                points: Vec::new(),
                appearance,
            })
        })
        .collect()
}

pub(super) fn weight(name: &str) -> f32 {
    match name {
        "face skin" => 2.0,
        "body skin" => 1.0,
        "whole image" => 0.25,
        _ => 0.5,
    }
}

pub(super) fn distance(reference: &[Region], actual: &[Region]) -> f32 {
    let mut sum = 0.0;
    let mut mass = 0.0;
    for a in actual {
        if let Some(r) = reference.iter().find(|r| r.name == a.name) {
            let w = weight(&r.name);
            sum += solve::distance(&r.appearance, &a.appearance) * w;
            mass += w;
        }
    }
    if mass > 0.0 {
        sum / mass
    } else {
        f32::INFINITY
    }
}

pub(super) fn skin_preserved(before: &[Region], after: &[Region]) -> bool {
    before.iter().filter(|r| r.name.ends_with("skin")).all(|a| {
        after.iter().find(|b| b.name == a.name).is_some_and(|b| {
            // Pilot protection limits, not calibrated correctness probabilities.
            a.appearance.shadow.distance(b.appearance.shadow) <= 5.0
                && a.appearance.mid.distance(b.appearance.mid) <= 5.0
                && a.appearance.high.distance(b.appearance.high) <= 5.0
                && (a.appearance.chroma_p50 - b.appearance.chroma_p50).abs() * 128.0 <= 5.0
                && (a.appearance.tone.p50 - b.appearance.tone.p50).abs() <= 0.12
        })
    })
}

/// A genuinely achromatic reference requests removal of chroma. Preserve each
/// person's tone and reject new casts while allowing movement toward neutral.
/// Ordinary colour references always use the stricter colour-preservation guard.
pub(super) fn skin_preserved_achromatic(before: &[Region], after: &[Region]) -> bool {
    let toward_neutral = |a: aura_core::contract::look::ZoneTint,
                          b: aura_core::contract::look::ZoneTint| {
        let norm = a.a * a.a + a.b * a.b;
        let projection = if norm > 0.0001 {
            ((a.a * b.a + a.b * b.b) / norm).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (b.a - a.a * projection).hypot(b.b - a.b * projection) <= 2.0
    };
    before.iter().filter(|r| r.name.ends_with("skin")).all(|a| {
        after.iter().find(|b| b.name == a.name).is_some_and(|b| {
            a.appearance
                .tone
                .as_array()
                .iter()
                .zip(b.appearance.tone.as_array())
                .all(|(x, y)| (x - y).abs() <= 0.12)
                && b.appearance.chroma_p50 <= a.appearance.chroma_p50 + 0.5 / 128.0
                && b.appearance.chroma_p90 <= a.appearance.chroma_p90 + 0.5 / 128.0
                && toward_neutral(a.appearance.shadow, b.appearance.shadow)
                && toward_neutral(a.appearance.mid, b.appearance.mid)
                && toward_neutral(a.appearance.high, b.appearance.high)
        })
    })
}

pub(super) fn achromatic_reference(regions: &[Region]) -> bool {
    regions
        .iter()
        .find(|r| r.name == "whole image")
        .is_some_and(|r| {
            r.appearance.chroma_p90 * 128.0 <= 0.75
                && r.appearance.mid.a.hypot(r.appearance.mid.b) <= 0.75
        })
}

pub(super) fn clipped(image: &Rgb8) -> f32 {
    let total = image.data.len() / 3;
    if total == 0 {
        return 1.0;
    }
    image
        .data
        .chunks_exact(3)
        .filter(|p| p.contains(&255))
        .count() as f32
        / total as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    fn region(name: &str, rgb: [u8; 3]) -> Region {
        Region {
            name: name.into(),
            points: Vec::new(),
            appearance: describe(&Rgb8 {
                width: 8,
                height: 8,
                data: rgb.repeat(64),
            }),
        }
    }
    #[test]
    fn pooled_skin_cannot_hide_a_colour_shift_on_one_person() {
        let before = vec![
            region("face skin", [130, 100, 80]),
            region("guard:person-0 face skin", [140, 110, 85]),
            region("guard:person-1 face skin", [85, 65, 45]),
        ];
        let mut after = before.clone();
        after[2] = region("guard:person-1 face skin", [150, 60, 45]);
        assert!(skin_preserved(&before, &before));
        assert!(!skin_preserved(&before, &after));
    }

    #[test]
    fn achromatic_intent_allows_desaturation_but_rejects_casts_and_person_damage() {
        let before = vec![
            region("face skin", [140, 110, 85]),
            region("guard:person-0 face skin", [140, 110, 85]),
        ];
        let neutral = vec![
            region("face skin", [116, 116, 116]),
            region("guard:person-0 face skin", [116, 116, 116]),
        ];
        assert!(!skin_preserved(&before, &neutral));
        assert!(skin_preserved_achromatic(&before, &neutral));
        let mut damaged = neutral.clone();
        damaged[1] = region("guard:person-0 face skin", [20, 20, 20]);
        assert!(!skin_preserved_achromatic(&before, &damaged));
        damaged[1] = region("guard:person-0 face skin", [90, 120, 130]);
        assert!(!skin_preserved_achromatic(&before, &damaged));
    }

    #[test]
    fn only_nearly_achromatic_references_enable_the_achromatic_guard() {
        assert!(achromatic_reference(&[region(
            "whole image",
            [120, 120, 120]
        )]));
        assert!(!achromatic_reference(&[region(
            "whole image",
            [120, 118, 115]
        )]));
        assert!(!achromatic_reference(&[]));
    }
}
