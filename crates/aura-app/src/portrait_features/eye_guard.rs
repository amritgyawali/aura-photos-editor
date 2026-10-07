//! Shared, feathered exclusions for the orbital region and nose. ADR-0091.
//! Applied after planning to every automatic face operation, including texture restoration.
use super::{Geometry, Pixels};
use aura_core::AuraResult;
use aura_recipe::retouch_tools::{Edit, Matte, Tool};
use aura_vision::portrait::PortraitFace;
use std::collections::BTreeMap;

/// Rotation-aware eye sockets, including the tear trough and inner-corner shadow.
/// The zero core has a sampling margin; the smooth transition lies outside that core.
pub(super) fn weight(g: &Geometry, point: [f32; 2]) -> f32 {
    socket(g, point, 0.30)
}

/// [`weight`] with the socket's upper half `up` eye distances tall. Mark repairs use a lower
/// one: the upper lid stays protected, while the brow bone and the skin just above and
/// between the brows - where acne is common and a mark is as visible as on a cheek - can be
/// repaired. Brow hair is left out of the heal selection where it actually is.
fn socket(g: &Geometry, point: [f32; 2], up: f32) -> f32 {
    g.eyes.iter().fold(1.0_f32, |weight, eye| {
        let dx = point[0] - eye[0];
        let dy = point[1] - eye[1];
        let across = (dx * g.u[0] + dy * g.u[1]) / g.d;
        let down = (dx * g.v[0] + dy * g.v[1]) / g.d - 0.035;
        let distance = (across / 0.40).hypot(down / if down < 0.0 { up } else { 0.30 });
        let t = ((distance - 1.0) / 0.4).clamp(0.0, 1.0);
        weight.min(t * t * (3.0 - 2.0 * t))
    })
}

pub(super) fn feature_weight(g: &Geometry, point: [f32; 2], settings: &super::Settings) -> f32 {
    weight_for(g, point, settings, settings.protect_nose_detail, false)
}

/// Repairs of marks - acne clear, frequency healing, spot repairs - and the texture restore,
/// which only puts the nose's own pores back, may work on the nose: *Preserve nose detail*
/// keeps its pores, shape and shading out of smoothing and toning, and acne on a nose is as
/// visible as acne anywhere. The nostrils are left out of the heal selection itself.
pub(super) fn repairs(tool: Tool) -> bool {
    matches!(
        tool,
        Tool::AcneClear | Tool::FrequencyHeal | Tool::PatchHeal | Tool::Heal | Tool::TextureGraft
    )
}

fn weight_for(
    g: &Geometry,
    point: [f32; 2],
    settings: &super::Settings,
    nose: bool,
    repair: bool,
) -> f32 {
    let eyes = if !settings.protect_eye_area {
        1.0
    } else if repair {
        socket(g, point, 0.19)
    } else {
        weight(g, point)
    };
    if !nose {
        return eyes;
    }
    let dx = point[0] - g.nose[0];
    let dy = point[1] - g.nose[1];
    let across = (dx * g.u[0] + dy * g.u[1]) / g.d;
    let down = (dx * g.v[0] + dy * g.v[1]) / g.d + 0.17;
    let distance = (across / 0.29).hypot(down / 0.53);
    let t = ((distance - 1.0) / 0.32).clamp(0.0, 1.0);
    eyes.min(t * t * (3.0 - 2.0 * t))
}

fn guarded(
    g: &Geometry,
    px: &Pixels<'_>,
    original: &Matte,
    settings: &super::Settings,
    nose: bool,
    repair: bool,
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
                    weight_for(g, [point[0] + dx, point[1] + dy], settings, nose, repair)
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
        // Only acne clear and frequency healing tell a mark from a lid crease or a brow hair;
        // a patch heal copies whatever its donor holds, so it keeps the full socket.
        let repair = matches!(edit.tool, Tool::AcneClear | Tool::FrequencyHeal);
        let nose = settings.protect_nose_detail && !repairs(edit.tool);
        let suffix = match (nose, repair) {
            (true, _) => "feature-safe",
            (false, false) => "eye-safe",
            (false, true) => "repair-safe",
        };
        let id = edit.matte.as_ref().map_or_else(
            || format!("{guard_id}-{suffix}"),
            |id| format!("{id}-{suffix}"),
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
                guarded(&g, px, source, settings, nose, repair).ok_or_else(invalid)?,
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
    fn mark_repairs_reach_the_nose_and_brow_bone_and_never_the_eyes() {
        let photo = [150_u8, 100, 75].repeat(256 * 256);
        let px = Pixels::new(&photo, 256, 256).unwrap();
        let full = |id: &str, tool: Tool| {
            let mut edit =
                super::super::base_edit(id.into(), tool, 1.0, &px, [128., 128., 256., 256.]);
            edit.feather = 0.;
            edit
        };
        let mut edits = [
            full("clear", Tool::AcneClear),
            full("smooth", Tool::SkinSmooth),
        ];
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
        let [clear, smooth] = edits.map(|e| e.matte.unwrap());
        assert_ne!(clear, smooth, "repairs and smoothing share one guard");
        let at = |id: &str, [x, y]: [f32; 2]| {
            let m = &mattes[id];
            let [l, t, r, b] = m.bounds;
            let col = (((x - l) / (r - l)) * m.width as f32) as usize;
            let row = (((y - t) / (b - t)) * m.height as f32) as usize;
            m.decode().unwrap()[row * m.width as usize + col]
        };
        // The nose tip: smoothing keeps out, acne clear works there.
        assert_eq!(at(&smooth, [0.5, 0.56]), 0);
        assert!(at(&clear, [0.5, 0.56]) > 200);
        // The brow bone a third of the eye distance above an eye: repairs reach it.
        assert!(
            at(&clear, [0.35, 0.29]) > 200,
            "{}",
            at(&clear, [0.35, 0.29])
        );
        // The eyes, their inner corners and the skin under them stay out of both.
        for point in [[0.35, 0.4], [0.445, 0.42], [0.35, 0.47]] {
            assert_eq!(at(&clear, point), 0, "{point:?}");
            assert_eq!(at(&smooth, point), 0, "{point:?}");
        }
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
