//! Shared, feathered exclusions for the orbital region and nose. ADR-0091.
//! Applied after planning to every automatic face operation, including texture restoration.
use super::{Geometry, Pixels};
use aura_core::AuraResult;
use aura_recipe::retouch_tools::{Edit, Matte};
use aura_vision::portrait::PortraitFace;
use std::collections::BTreeMap;

/// Rotation-aware eye sockets, including the tear trough and inner-corner shadow.
/// The zero core has a sampling margin; the smooth transition lies outside that core.
pub(super) fn weight(g: &Geometry, point: [f32; 2]) -> f32 {
    eye_weight(g, point, 0.40, 0.30, 0.4)
}

fn eye_weight(g: &Geometry, point: [f32; 2], rx: f32, ry: f32, feather: f32) -> f32 {
    g.eyes.iter().fold(1.0_f32, |weight, eye| {
        let dx = point[0] - eye[0];
        let dy = point[1] - eye[1];
        let across = (dx * g.u[0] + dy * g.u[1]) / g.d;
        let down = (dx * g.v[0] + dy * g.v[1]) / g.d - 0.035;
        let distance = (across / rx).hypot(down / ry);
        let t = ((distance - 1.0) / feather).clamp(0.0, 1.0);
        weight.min(t * t * (3.0 - 2.0 * t))
    })
}

pub(super) fn feature_weight(g: &Geometry, point: [f32; 2], settings: &super::Settings) -> f32 {
    let eyes = if settings.protect_eye_area {
        weight(g, point)
    } else {
        1.0
    };
    if !settings.protect_nose_detail {
        return eyes;
    }
    let dx = point[0] - g.nose[0];
    let dy = point[1] - g.nose[1];
    let across = (dx * g.u[0] + dy * g.u[1]) / g.d;
    let down = (dx * g.v[0] + dy * g.v[1]) / g.d + 0.17;
    let distance = (across / 0.29).hypot(down / 0.53);
    let t = ((distance - 1.0) / 0.32).clamp(0.0, 1.0);
    eyes.min(t * t * (3.0 - 2.0 * t))
        .min(nostril_weight(g, point))
}

/// Protect nostril openings, wings and the underside crease, without excluding
/// bridge and tip skin from spot repair. Coordinates rotate with the landmarks.
pub(super) fn nostril_weight(g: &Geometry, point: [f32; 2]) -> f32 {
    nostril_exclusion(g, point, true)
}

pub(super) fn nostril_opening_weight(g: &Geometry, point: [f32; 2]) -> f32 {
    if let Some(openings) = g.nostrils {
        let dx = point[0] - g.nose[0];
        let dy = point[1] - g.nose[1];
        let across = (dx * g.u[0] + dy * g.u[1]) / g.d;
        let down = (dx * g.v[0] + dy * g.v[1]) / g.d;
        let mut safe = openings.into_iter().fold(1.0_f32, |safe, opening| {
            let distance =
                ((across - opening.across) / opening.rx).hypot((down - opening.down) / opening.ry);
            let t = ((distance - 1.0) / 0.35).clamp(0.0, 1.0);
            safe.min(t * t * (3.0 - 2.0 * t))
        });
        // The original underside crease remains protected independently of the
        // cavity positions. Broad healing/finishing retains its original guard.
        let distance = (across / 0.13).hypot((down - 0.22) / 0.06);
        let t = ((distance - 1.0) / 0.35).clamp(0.0, 1.0);
        safe = safe.min(t * t * (3.0 - 2.0 * t));
        return safe;
    }
    nostril_exclusion(g, point, false)
}

fn nostril_exclusion(g: &Geometry, point: [f32; 2], wings: bool) -> f32 {
    let dx = point[0] - g.nose[0];
    let dy = point[1] - g.nose[1];
    let across = (dx * g.u[0] + dy * g.u[1]) / g.d;
    let down = (dx * g.v[0] + dy * g.v[1]) / g.d;
    [
        (-0.18, 0.12, 0.15, 0.10),
        (0.18, 0.12, 0.15, 0.10),
        (0.0, 0.22, 0.13, 0.06),
        // Openings alone miss the curved outer wing, especially in a turned
        // face. Keep both its highlight and shadow out of healing/finishing.
        (-0.30, 0.12, 0.21, 0.20),
        (0.30, 0.12, 0.21, 0.20),
    ]
    .into_iter()
    .take(if wings { 5 } else { 3 })
    .fold(1.0_f32, |safe, (x, y, rx, ry)| {
        let distance = ((across - x) / rx).hypot((down - y) / ry);
        let t = ((distance - 1.0) / 0.35).clamp(0.0, 1.0);
        safe.min(t * t * (3.0 - 2.0 * t))
    })
}

/// A measured, compact spot can sit on wing skin without being the wing contour.
/// Its surrounding-light and donor checks run before this precise mask is used.
pub(super) fn spot_weight(g: &Geometry, point: [f32; 2], settings: &super::Settings) -> f32 {
    let eyes = if settings.protect_eye_area {
        // Compact measured repairs can reach upper-cheek skin. Keep generous
        // eye/inner-corner cores, but do not inherit the broad orbital exclusion.
        eye_weight(g, point, 0.34, 0.23, 0.25)
    } else {
        1.0
    };
    eyes.min(nostril_opening_weight(g, point))
}

pub(super) fn blemish_weight(g: &Geometry, point: [f32; 2], settings: &super::Settings) -> f32 {
    let eye = if settings.protect_eye_area {
        weight(g, point)
    } else {
        1.0
    };
    eye.min(nostril_weight(g, point))
}

fn is_blemish(tool: super::Tool) -> bool {
    matches!(
        tool,
        super::Tool::FrequencyHeal
            | super::Tool::PatchHeal
            | super::Tool::Heal
            | super::Tool::AutoBlemish
    )
}

fn guarded(
    g: &Geometry,
    px: &Pixels<'_>,
    original: &Matte,
    settings: &super::Settings,
    blemish: bool,
    precise: bool,
) -> Option<Matte> {
    let mut alpha = original.decode()?;
    let [l, t, r, b] = original.bounds;
    let w = original.width as usize;
    // Expand the protected core by half a cell diagonal. Bilinear interpolation at export
    // resolution must not turn an excluded inner corner into a faintly selected pixel.
    let margin = (((r - l) * px.width as f32 / original.width as f32)
        .hypot((b - t) * px.height as f32 / original.height as f32))
        * 0.5;
    for (i, value) in alpha.iter_mut().enumerate() {
        let point = [
            (l + (i % w) as f32 / w as f32 * (r - l) + (r - l) / w as f32 * 0.5) * px.width as f32,
            (t + ((i / w) as f32 + 0.5) / original.height as f32 * (b - t)) * px.height as f32,
        ];
        let safe = [-margin, 0.0, margin]
            .into_iter()
            .flat_map(|dy| {
                [-margin, 0.0, margin].into_iter().map(move |dx| {
                    let at = [point[0] + dx, point[1] + dy];
                    if precise {
                        spot_weight(g, at, settings)
                    } else if blemish {
                        blemish_weight(g, at, settings)
                    } else {
                        feature_weight(g, at, settings)
                    }
                })
            })
            .fold(1.0_f32, f32::min);
        *value = (f32::from(*value) * safe).round() as u8;
    }
    let mut matte = Matte::encode(original.bounds, original.width, original.height, &alpha);
    // A color-guided upsampler can expand a hole back into skin-colored eyelids.
    matte.refine_edges = false;
    Some(matte)
}

/// Intersect existing masks; operations without one receive the feature exclusions.
/// Existing ellipses, strokes, skin samples and luminance limits still apply. No recipe,
/// user-authored operation, body, hair or clothing operation is passed to this function.
pub(crate) fn protect<'a>(
    face: &PortraitFace,
    px: &Pixels<'_>,
    edits: impl Iterator<Item = &'a mut Edit>,
    mattes: &mut BTreeMap<String, Matte>,
    guard_id: &str,
    settings: &super::Settings,
) -> AuraResult<()> {
    let invalid = || {
        aura_core::errors::render::recipe_invalid(
            "facial detail protection",
            "Cannot validate the automatic eye and nose exclusions",
        )
    };
    let g = Geometry::new(face, px).ok_or_else(invalid)?;
    let mut created = BTreeMap::new();
    for edit in edits {
        let blemish = is_blemish(edit.tool);
        let precise = matches!(edit.tool, super::Tool::PatchHeal | super::Tool::Heal)
            || (edit.tool == super::Tool::SkinUniformity && edit.id.contains("-spot-deep-"));
        let id = edit.matte.as_ref().map_or_else(
            || {
                if precise {
                    format!("{guard_id}-spot-feature-guard")
                } else if blemish {
                    format!("{guard_id}-blemish-feature-guard")
                } else {
                    guard_id.to_owned()
                }
            },
            |id| {
                if precise {
                    format!("{id}-spot-feature-safe")
                } else if blemish {
                    format!("{id}-blemish-feature-safe")
                } else {
                    format!("{id}-feature-safe")
                }
            },
        );
        if !created.contains_key(&id) {
            let full;
            let source = if let Some(existing) = &edit.matte {
                mattes.get(existing).ok_or_else(invalid)?
            } else {
                full = Matte::encode([0.0, 0.0, 1.0, 1.0], 256, 256, &vec![255; 256 * 256]);
                &full
            };
            created.insert(
                id.clone(),
                guarded(&g, px, source, settings, blemish, precise).ok_or_else(invalid)?,
            );
        }
        edit.matte = Some(id);
    }
    mattes.extend(created);
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::disallowed_methods,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;
    use aura_recipe::retouch_tools::Tool;

    #[test]
    fn actual_openings_are_protected_without_hiding_adjacent_nose_skin() {
        let mut rgb = [150_u8, 100, 75].repeat(256 * 256);
        let original = Pixels::new(&rgb, 256, 256).unwrap();
        let g = Geometry::new(&face(), &original).unwrap();
        for y in 0..256 {
            for x in 0..256 {
                let across = (x as f32 + 0.5 - g.nose[0]) / g.d;
                let down = (y as f32 + 0.5 - g.nose[1]) / g.d;
                if [-0.12, 0.12]
                    .into_iter()
                    .any(|u| ((across - u) / 0.075).powi(2) + ((down - 0.11) / 0.03).powi(2) <= 1.0)
                {
                    rgb[(y * 256 + x) * 3..(y * 256 + x) * 3 + 3].copy_from_slice(&[20, 13, 10]);
                }
            }
        }
        let px = Pixels::new(&rgb, 256, 256).unwrap();
        let measured = Geometry::new(&face(), &px).unwrap();
        let point = [g.nose[0] - g.d * 0.22, g.nose[1]];
        assert_eq!(
            spot_weight(&measured, point, &super::super::Settings::default()),
            1.0,
            "Landmark fallback hides skin above the actual opening"
        );
        for y in 0..256 {
            for x in 0..256 {
                if rgb[(y * 256 + x) * 3] == 20 {
                    assert_eq!(
                        spot_weight(
                            &measured,
                            [x as f32 + 0.5, y as f32 + 0.5],
                            &super::super::Settings::default()
                        ),
                        0.0
                    );
                }
            }
        }
    }

    fn face() -> PortraitFace {
        PortraitFace {
            bounds: [0.1, 0.05, 0.9, 0.95],
            confidence: 0.99,
            landmarks: [
                [0.35, 0.4],
                [0.65, 0.4],
                [0.5, 0.58],
                [0.4, 0.72],
                [0.6, 0.72],
            ],
        }
    }

    #[test]
    fn compact_spots_can_reach_wing_skin_without_selecting_nostril_openings() {
        let photo = [150_u8, 100, 75].repeat(256 * 256);
        let px = Pixels::new(&photo, 256, 256).unwrap();
        let g = Geometry::new(&face(), &px).unwrap();
        let settings = super::super::Settings::default();
        for p in [[0.368, 0.637], [0.632, 0.637]] {
            let point = p.map(|v| v * 256.0);
            assert!(spot_weight(&g, point, &settings) > 0.8);
            assert!(blemish_weight(&g, point, &settings) < f32::EPSILON);
        }
        for p in [[0.446, 0.616], [0.554, 0.616], [0.5, 0.646]] {
            assert!(spot_weight(&g, p.map(|v| v * 256.0), &settings) < f32::EPSILON);
        }
    }

    #[test]
    fn blemish_selection_reaches_nose_skin_but_finishing_still_protects_it() {
        let photo = [150_u8, 100, 75].repeat(256 * 256);
        let px = Pixels::new(&photo, 256, 256).unwrap();
        let mut edits = [
            super::super::base_edit(
                "heal".into(),
                Tool::FrequencyHeal,
                1.0,
                &px,
                [128., 128., 256., 256.],
            ),
            super::super::base_edit(
                "finish".into(),
                Tool::Frequency,
                1.0,
                &px,
                [128., 128., 256., 256.],
            ),
            super::super::base_edit(
                "spot".into(),
                Tool::PatchHeal,
                1.0,
                &px,
                [128., 128., 256., 256.],
            ),
        ];
        for edit in &mut edits {
            edit.feather = 0.;
        }
        let mut mattes = BTreeMap::new();
        protect(
            &face(),
            &px,
            edits.iter_mut(),
            &mut mattes,
            "guard",
            &super::super::Settings::default(),
        )
        .unwrap();
        assert_ne!(
            edits[0].matte, edits[1].matte,
            "spot and broad tools must not share a guard"
        );
        assert_ne!(
            edits[0].matte, edits[2].matte,
            "compact and broad healing need different wing guards"
        );
        for size in [128, 512] {
            let pixels = [0.3_f32, 0.2, 0.1].repeat(size * size);
            let heal = aura_render::retouch_tools::selection_mask_with_mattes(
                &pixels, size, size, &edits[0], &mattes,
            );
            let finish = aura_render::retouch_tools::selection_mask_with_mattes(
                &pixels, size, size, &edits[1], &mattes,
            );
            let spot = aura_render::retouch_tools::selection_mask_with_mattes(
                &pixels, size, size, &edits[2], &mattes,
            );
            for [x, y] in [[0.5, 0.53], [0.48, 0.55]] {
                let i = (y * size as f32) as usize * size + (x * size as f32) as usize;
                assert!(
                    heal[i] > 0.8,
                    "nose skin must be eligible at {size}: {x},{y}"
                );
                assert_eq!(finish[i], 0., "broad finishing must preserve nose shading");
            }
            for [x, y] in [
                [0.35, 0.4],
                [0.65, 0.4],
                [0.446, 0.616],
                [0.554, 0.616],
                [0.5, 0.646],
                // Outer nostril wings, which lie beyond the openings on a
                // turned face. Healing these can flatten the nose contour.
                [0.368, 0.637],
                [0.632, 0.637],
            ] {
                let i = (y * size as f32) as usize * size + (x * size as f32) as usize;
                assert_eq!(
                    heal[i], 0.,
                    "eye or nostril core selected at {size}: {x},{y}"
                );
                assert_eq!(
                    finish[i], 0.,
                    "finishing must preserve eye and nostril wings at {size}: {x},{y}"
                );
                if (0.38..=0.62).contains(&x) {
                    assert_eq!(
                        spot[i], 0.0,
                        "compact healing must exclude nostril openings"
                    );
                } else if y > 0.6 {
                    assert!(spot[i] > 0.8, "compact wing repair is masked at {size}");
                } else {
                    assert_eq!(spot[i], 0.0);
                }
            }
        }
    }

    #[test]
    fn orbital_core_stays_unchanged_at_preview_and_export_sizes() {
        let photo = [150_u8, 100, 75].repeat(256 * 256);
        let px = Pixels::new(&photo, 256, 256).unwrap();
        let mut edits = [super::super::base_edit(
            "automatic".into(),
            Tool::Dodge,
            1.0,
            &px,
            [128., 128., 256., 256.],
        )];
        edits[0].feather = 0.;
        let mut mattes = BTreeMap::new();
        protect(
            &face(),
            &px,
            edits.iter_mut(),
            &mut mattes,
            "eyes",
            &super::super::Settings::default(),
        )
        .unwrap();
        for size in [128, 512] {
            let original = [0.3_f32, 0.2, 0.1].repeat(size * size);
            let mut rendered = original.clone();
            aura_render::retouch_tools::apply_with_mattes(
                &mut rendered,
                size,
                size,
                &edits,
                &mattes,
            );
            // Eye centres, inner corners and under-eye troughs, including the side toward
            // the nose. The old small circular exclusion missed these corner points.
            for [x, y] in [
                [0.35, 0.4],
                [0.65, 0.4],
                [0.445, 0.42],
                [0.555, 0.42],
                [0.35, 0.47],
                [0.65, 0.47],
                [0.5, 0.48],
                [0.5, 0.58],
                [0.44, 0.58],
                [0.56, 0.58],
            ] {
                let i = ((y * size as f32) as usize * size + (x * size as f32) as usize) * 3;
                assert_eq!(
                    &rendered[i..i + 3],
                    &original[i..i + 3],
                    "eye core changed at {size}: {x},{y}"
                );
            }
            let cheek = ((size as f32 * 0.65) as usize * size + (size as f32 * 0.3) as usize) * 3;
            assert!(
                rendered[cheek] > original[cheek],
                "guard must not disable cheek correction"
            );
        }
    }

    #[test]
    fn guard_intersects_existing_selection_without_changing_original_matte() {
        let photo = [150_u8, 100, 75].repeat(256 * 256);
        let px = Pixels::new(&photo, 256, 256).unwrap();
        let original = Matte::encode(
            [0.1, 0.1, 0.9, 0.9],
            4,
            4,
            &[0, 0, 0, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 0, 0, 0],
        );
        let mut mattes = BTreeMap::from([("skin".into(), original.clone())]);
        let mut edits = [super::super::base_edit(
            "restore".into(),
            Tool::TextureGraft,
            1.0,
            &px,
            [128., 128., 256., 256.],
        )];
        edits[0].matte = Some("skin".into());
        protect(
            &face(),
            &px,
            edits.iter_mut(),
            &mut mattes,
            "eyes",
            &super::super::Settings::default(),
        )
        .unwrap();
        assert_eq!(mattes["skin"], original);
        let safe = &mattes[edits[0].matte.as_ref().unwrap()];
        assert!(!safe.refine_edges);
        for (a, b) in safe
            .decode()
            .unwrap()
            .iter()
            .zip(original.decode().unwrap())
        {
            assert!(*a <= b);
        }
        edits[0].matte = Some("missing".into());
        assert!(protect(
            &face(),
            &px,
            edits.iter_mut(),
            &mut mattes,
            "eyes",
            &super::super::Settings::default()
        )
        .is_err());
    }

    #[test]
    fn eye_geometry_rotates_with_the_face() {
        let photo = [150_u8, 100, 75].repeat(256 * 256);
        let px = Pixels::new(&photo, 256, 256).unwrap();
        let original = face();
        let mut rotated = original.clone();
        rotated.landmarks = rotated.landmarks.map(|[x, y]| [1.0 - y, x]);
        let a = Geometry::new(&original, &px).unwrap();
        let b = Geometry::new(&rotated, &px).unwrap();
        for [x, y] in [[0.445, 0.42], [0.35, 0.49], [0.3, 0.65], [0.5, 0.23]] {
            assert!(
                (feature_weight(&a, [x * 256., y * 256.], &super::super::Settings::default())
                    - feature_weight(
                        &b,
                        [(1. - y) * 256., x * 256.],
                        &super::super::Settings::default()
                    ))
                .abs()
                    < 1e-5
            );
        }
    }
}
