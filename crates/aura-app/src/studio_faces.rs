//! Portrait studio faces both of AURA's face detectors agree on. ADR-0185.
//!
//! The studio's face, skin, makeup and light tools act on the faces the renderer's portrait
//! parse finds, re-derived from the pixels at render time (Haar cascades, `aura-portrait`).
//! Automatic retouch finds faces with the learned detector instead (YuNet, `aura-vision`). On
//! 989A3150 the parse took a woman's clasped hands for a face and missed a groom in sunglasses;
//! the learned detector did neither. The renderer cannot run the learned detector - it sits
//! above the renderer - so the two are compared here, once, the first time a tool needs the
//! parse on a photograph, and the answer is written into the recipe in the two forms the
//! renderer already reads: a face hint for each face only the learned detector found, and the
//! chosen faces for the tools.
//!
//! Two rules keep it from doing harm. A face the parse found is left out only when the learned
//! detector should have seen it - at least most of the size of the smallest face it did find -
//! because the learned detector deliberately skips small faces in a crowd. And when the two
//! agree nothing is written: the Faces tab still says every face.
use crate::AppState;
use aura_core::PhotoId;
use aura_portrait::FaceHint;
use aura_preview::contract::service::PreviewService;
use aura_recipe::{Mask, MaskKind, MaskParams, Recipe};

/// The ids of the face hints this module writes; never a photographer's own drawn face.
pub(crate) const HINT_PREFIX: &str = "studio-faces-hint-";

/// A face the parse found is one the learned detector should have seen when it is at least this
/// share of the width of the smallest face the learned detector did find.
const SEEN: f32 = 0.6;

/// What the two detectors decided between them.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Agreement {
    /// The faces the studio's tools act on, as normalised centres - the renderer's chosen faces.
    pub chosen: Vec<[f32; 2]>,
    /// Faces only the learned detector found, as hints the parse will now include.
    pub added: Vec<FaceHint>,
    /// Faces only the parse found, large enough that the learned detector should have seen them.
    pub left_out: usize,
}

impl Agreement {
    /// The sentence the panel shows.
    pub(crate) fn note(&self) -> String {
        let mut parts = Vec::new();
        if !self.added.is_empty() {
            parts.push(format!(
                "added {} face{} only the learned face detector found",
                self.added.len(),
                if self.added.len() == 1 { "" } else { "s" }
            ));
        }
        if self.left_out > 0 {
            parts.push(format!(
                "left out {} it does not see as a face",
                if self.left_out == 1 {
                    "one".to_string()
                } else {
                    self.left_out.to_string()
                }
            ));
        }
        format!(
            "AURA's two face detectors disagreed here, so the face tools apply to the {} faces both accept: {}. Choose faces in the Faces tab to change this.",
            self.chosen.len(),
            parts.join(" and ")
        )
    }
}

fn centre(b: &[f32; 4]) -> [f32; 2] {
    [f32::midpoint(b[0], b[2]), f32::midpoint(b[1], b[3])]
}

fn inside(p: [f32; 2], b: &[f32; 4]) -> bool {
    (b[0]..=b[2]).contains(&p[0]) && (b[1]..=b[3]).contains(&p[1])
}

/// One face, seen by both: either one's centre falls inside the other's box.
fn same(a: &[f32; 4], b: &[f32; 4]) -> bool {
    inside(centre(a), b) || inside(centre(b), a)
}

/// Compare the parse's faces with the learned detector's, both as normalised left, top, right,
/// bottom boxes. `None` when there is nothing to change: they agree, or the learned detector
/// found nobody and so offers no second opinion.
pub(crate) fn agree(parsed: &[[f32; 4]], learned: &[[f32; 4]]) -> Option<Agreement> {
    let smallest = learned
        .iter()
        .map(|b| b[2] - b[0])
        .filter(|w| w.is_finite() && *w > 0.0)
        .reduce(f32::min)?;
    let mut chosen = Vec::new();
    let mut left_out = 0;
    for face in parsed {
        let confirmed = learned.iter().any(|l| same(face, l));
        let seen = face[2] - face[0] >= smallest * SEEN;
        if confirmed || !seen {
            chosen.push(centre(face));
        } else {
            left_out += 1;
        }
    }
    let added: Vec<FaceHint> = learned
        .iter()
        .filter(|l| !parsed.iter().any(|p| same(p, l)))
        .map(|l| FaceHint {
            x: l[0],
            y: l[1],
            w: l[2] - l[0],
            h: l[3] - l[1],
        })
        .collect();
    if added.is_empty() && left_out == 0 {
        return None;
    }
    chosen.extend(added.iter().map(|h| [h.x + h.w / 2.0, h.y + h.h / 2.0]));
    Some(Agreement {
        chosen,
        added,
        left_out,
    })
}

/// Both detectors on one photograph, as the renderer will see it: the parse with every hint
/// except this module's own, and the learned detector on the same proxy.
///
/// # Errors
/// The photograph's proxy cannot be read, or the learned detector cannot run.
// The parse's grid is at most a few thousand pixels a side, exact in an f32.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn reconcile(
    state: &AppState,
    project: &str,
    photo: PhotoId,
    recipe: &Recipe,
) -> aura_core::AuraResult<Option<Agreement>> {
    let mut without = recipe.clone();
    without.masks.retain(|m| !m.id.starts_with(HINT_PREFIX));
    let (_, map) = crate::evoto_workflow::parse_of(state, photo, &without)?;
    let parsed: Vec<[f32; 4]> = map.as_deref().map_or_else(Vec::new, |m| {
        let (sx, sy) = (m.width.max(1) as f32, m.height.max(1) as f32);
        m.faces
            .iter()
            .map(|g| {
                [
                    (g.center[0] - g.size / 2.0) / sx,
                    (g.center[1] - g.size / 2.0) / sy,
                    (g.center[0] + g.size / 2.0) / sx,
                    (g.center[1] + g.size / 2.0) / sy,
                ]
            })
            .collect()
    });
    let previews = state.previews(project)?;
    let proxy = previews.get(
        photo,
        aura_raw::PixelLevel::Proxy2048,
        aura_preview::contract::service::Priority::Interactive,
    )?;
    let Some(rgb) = proxy.as_srgb8() else {
        return Ok(None);
    };
    let found = aura_vision::portrait::detect(rgb, proxy.width, proxy.height)?;
    let found = aura_vision::portrait::detect_small_faces(rgb, proxy.width, proxy.height, &found)
        .unwrap_or(found);
    let learned: Vec<[f32; 4]> = found.iter().map(|f| f.bounds).collect();
    Ok(agree(&parsed, &learned))
}

/// Write an agreement's hints into a recipe, replacing any this module wrote before.
pub(crate) fn write_hints(recipe: &mut Recipe, agreement: &Agreement) {
    recipe.masks.retain(|m| !m.id.starts_with(HINT_PREFIX));
    for (i, hint) in agreement.added.iter().enumerate() {
        recipe.masks.push(Mask {
            id: format!("{HINT_PREFIX}{}", i + 1),
            kind: MaskKind::Face,
            target: Some(hint.to_target()),
            invert_of: None,
            feather: 0.0,
            params: MaskParams::default(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 989A3150 as the two detectors saw it, normalised: the bride, two standing women and the
    /// clasped hands from the parse; the bride, both women, a man cut off at the top and the
    /// groom in sunglasses from the learned detector.
    const PARSED: [[f32; 4]; 4] = [
        [0.28, 0.32, 0.375, 0.46],
        [0.848, 0.60, 0.94, 0.74],
        [0.53, 0.07, 0.615, 0.2],
        [0.887, 0.04, 0.962, 0.155],
    ];
    const LEARNED: [[f32; 4]; 5] = [
        [0.289, 0.33, 0.36, 0.456],
        [0.39, 0.002, 0.449, 0.103],
        [0.544, 0.072, 0.611, 0.199],
        [0.609, 0.301, 0.688, 0.442],
        [0.899, 0.043, 0.964, 0.166],
    ];

    #[test]
    fn the_hands_are_left_out_and_the_groom_is_added() {
        let a = agree(&PARSED, &LEARNED).unwrap();
        assert_eq!(a.left_out, 1, "the clasped hands");
        assert_eq!(a.added.len(), 2, "the groom and the man cut off at the top");
        assert_eq!(a.chosen.len(), 5);
        assert!(
            !a.chosen.iter().any(|p| inside(*p, &PARSED[1])),
            "no chosen face on the hands"
        );
        assert!(
            a.chosen.iter().any(|p| inside(*p, &LEARNED[3])),
            "the groom is chosen"
        );
        assert!(a.note().contains("left out one"));
    }

    #[test]
    fn agreement_writes_nothing() {
        assert_eq!(agree(&LEARNED, &LEARNED), None);
        // No second opinion: the learned detector found nobody.
        assert_eq!(agree(&PARSED, &[]), None);
    }

    #[test]
    fn a_small_crowd_face_the_learned_detector_skips_is_kept() {
        // A guest far smaller than anything the learned detector found is not evidence against.
        let guest = [0.1, 0.1, 0.12, 0.13];
        assert_eq!(agree(&[LEARNED[0], guest], &[LEARNED[0]]), None);
    }

    #[test]
    fn hints_replace_their_own_and_keep_a_photographers() {
        let mut recipe = aura_recipe::fixtures::neutral("test", "test");
        let drawn = Mask {
            id: "portrait-retouch-hint-1".into(),
            kind: MaskKind::Face,
            target: Some("hint:0.1000,0.1000,0.1000,0.1000".into()),
            invert_of: None,
            feather: 0.0,
            params: MaskParams::default(),
        };
        recipe.masks.push(drawn.clone());
        let a = agree(&PARSED, &LEARNED).unwrap();
        write_hints(&mut recipe, &a);
        write_hints(&mut recipe, &a);
        assert_eq!(
            recipe
                .masks
                .iter()
                .filter(|m| m.id.starts_with(HINT_PREFIX))
                .count(),
            2
        );
        assert!(recipe.masks.contains(&drawn));
        assert!(recipe
            .masks
            .iter()
            .filter_map(|m| m.target.as_deref())
            .all(|t| !t.starts_with("hint:") || FaceHint::parse(t).is_some()));
    }
}
