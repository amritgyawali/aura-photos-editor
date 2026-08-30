//! Synthetic frames whose answers are known by construction.
//!
//! Every number this phase reports is measured against these. There are no wedding photographs in
//! this repository and no labelled distraction vocabulary, so a frame here is one where the exit
//! sign was **painted into the pixels** at a rectangle the test already knows - which proves the
//! arithmetic and says nothing about a photograph. Conditions C1 and C2 of the exit report.
//!
//! The same shape phases 09, 10, 15, 16, 18, 20, 21 and 22 all use, and the same caveat: a gate
//! that passes here proves the detector finds a rectangle somebody drew, the safety engine refuses
//! what it is supposed to refuse, the fill copies texture rather than inventing it, and the
//! self-check catches an artefact that was deliberately introduced.

use aura_core::contract::cleanup::{Box2, DistractionClass};

use crate::denylist::{Coverage, Protected};
use crate::pixels::{Image, Rect};
use crate::policy::ScenePolicy;
use crate::safety::Candidate;

/// The size every fixture frame is, in pixels.
///
/// Small enough that the exhaustive four-subset homography fit and the exemplar search run in a
/// unit test's budget, large enough that a 4 % area cap is still forty pixels across rather than
/// four - which matters, because a rectangle of four pixels would pass a size test for the wrong
/// reason.
pub const FIXTURE_W: usize = 200;
/// See [`FIXTURE_W`].
pub const FIXTURE_H: usize = 200;

/// What kind of background a fixture frame carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Background {
    /// Isotropic texture with no dominant direction. A lawn, or a carpet. Fillable.
    Grass,
    /// A single flat tone. A painted wall. Fillable, and the easiest case there is.
    Wall,
    /// One strong direction. A railing, a skirting board, a run of panelling. **Not** fillable, and
    /// the case section 6.2's structure check exists for.
    Railing,
    /// Non-repeating detail with structure in every direction. A hedge, a crowd, confetti.
    Busy,
}

impl Background {
    /// The value at one position.
    #[must_use]
    pub fn sample(self, x: usize, y: usize, shift: f32) -> [f32; 3] {
        let fx = x as f32 + shift;
        let fy = y as f32;
        match self {
            Self::Grass => {
                let v = 0.30
                    + 0.06 * (fx * 0.9).sin()
                    + 0.06 * (fy * 0.9).sin()
                    + 0.03 * ((fx + fy) * 0.5).cos();
                [v * 0.6, v, v * 0.5]
            }
            Self::Wall => [0.42, 0.41, 0.40],
            Self::Railing => {
                let v = if ((y as f32 / 7.0) as usize).is_multiple_of(2) {
                    0.62
                } else {
                    0.18
                };
                [v, v, v]
            }
            Self::Busy => {
                let v = 0.32
                    + 0.11 * (fx * 0.21).sin()
                    + 0.09 * (fy * 0.13).cos()
                    + 0.06 * ((fx + fy) * 0.07).sin()
                    + 0.05 * ((fx - fy) * 0.31).cos();
                [v, v * 0.94, v * 0.86]
            }
        }
    }
}

/// A frame of one background, with nothing in it.
#[must_use]
pub fn clean(background: Background) -> Image {
    shifted(background, 0.0)
}

/// A frame of one background, as though the camera had drifted `shift` pixels sideways.
///
/// What a sibling frame of the same burst looks like: the same room, the same light, a hand-held
/// camera's own movement between two shutter releases.
#[must_use]
pub fn shifted(background: Background, shift: f32) -> Image {
    let mut image = Image::black(FIXTURE_W, FIXTURE_H);
    for y in 0..FIXTURE_H {
        for x in 0..FIXTURE_W {
            image.put(x, y, background.sample(x, y, shift));
        }
    }
    image
}

/// Paint a solid rectangle into a frame. The distraction.
pub fn paint(image: &mut Image, rect: Rect, value: [f32; 3]) {
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            image.put(x, y, value);
        }
    }
}

/// A frame with one bright object painted into it, and the rectangle it occupies.
///
/// The object is deliberately a flat colour rather than a rendered bin: what the modules downstream
/// are being measured on is whether a rectangle of *foreign content* is replaced by something that
/// matches its surroundings, and a flat block is the hardest version of that for a fill and the
/// easiest to check.
#[must_use]
pub fn with_object(background: Background, rect: Rect) -> (Image, Box2) {
    let mut image = clean(background);
    paint(&mut image, rect, [0.95, 0.08, 0.08]);
    (image, normalise(rect))
}

/// One pixel rectangle as the normalised region the contract carries.
#[must_use]
pub fn normalise(rect: Rect) -> Box2 {
    Box2 {
        x: rect.x as f32 / FIXTURE_W as f32,
        y: rect.y as f32 / FIXTURE_H as f32,
        w: rect.w as f32 / FIXTURE_W as f32,
        h: rect.h as f32 / FIXTURE_H as f32,
    }
}

/// A candidate over one region, of a class that is safe to remove.
///
/// `removability` is high and `salience` is high, so that anything blocking it in a test is
/// blocking it for the reason the test is about rather than because the fixture was weak.
#[must_use]
pub fn candidate(region: Box2, class: DistractionClass) -> Candidate {
    Candidate {
        region,
        class,
        salience: 0.85,
        removability: 0.90,
        crosses_structure: false,
        touches_identity: false,
    }
}

/// The most permissive scene row the contract allows.
///
/// Used wherever a test needs to prove that something is refused *by the mechanism* rather than by
/// a strict policy row. A refusal under a strict row proves nothing about the engine.
#[must_use]
pub fn permissive_policy() -> ScenePolicy {
    ScenePolicy {
        area_cap: aura_core::contract::cleanup::AREA_CAP_DEFAULT,
        denylist_overlap_max: aura_core::contract::cleanup::DENYLIST_OVERLAP_MAX,
        zero_touch_confidence: aura_core::contract::cleanup::ZERO_TOUCH_CONFIDENCE,
        enabled: true,
        reason: "the most permissive row the contract allows, for a mechanism test".into(),
    }
}

/// Coverage in which every protected kind was askable and none was found.
///
/// **Not what a real photograph produces in this build.** Phase 18 has no class for a ring or a
/// cake, so `api::coverage_from_masks` can never return this. It exists so that a gate about the
/// *size cap*, the *structure check* or the *fill* is not silently a gate about the denylist.
#[must_use]
pub fn fully_masked_empty() -> Coverage {
    Coverage::known_empty()
}

/// Coverage carrying one protected region, with every kind askable.
#[must_use]
pub fn masked_with(kind: Protected, region: Box2) -> Coverage {
    Coverage::known(vec![(kind, region)])
}

/// A frame with a deliberate inpainting artefact: texture repeating at a period nothing else uses.
///
/// What a bad synthesis looks like, painted in so the self-check has a known answer.
#[must_use]
pub fn with_repeat_artefact(background: Background, rect: Rect) -> (Image, Box2) {
    let mut image = clean(background);
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let v = if (x / 2) % 2 == 0 { 0.62 } else { 0.20 };
            image.put(x, y, [v, v, v]);
        }
    }
    (image, normalise(rect))
}

/// A frame with a deliberate warp: a line that enters the region and turns a right angle inside it.
#[must_use]
pub fn with_warp_artefact(rect: Rect) -> (Image, Box2) {
    let mut image = clean(Background::Railing);
    for y in rect.y..rect.bottom() {
        for x in rect.x..rect.right() {
            let v = if ((x as f32 / 7.0) as usize).is_multiple_of(2) {
                0.62
            } else {
                0.18
            };
            image.put(x, y, [v, v, v]);
        }
    }
    (image, normalise(rect))
}

/// A frame with a deliberate ghost edge: a hard step at the region's own boundary in a smooth
/// frame that has no steps anywhere else.
#[must_use]
pub fn with_ghost_artefact(rect: Rect) -> (Image, Box2) {
    let mut image = Image::black(FIXTURE_W, FIXTURE_H);
    for y in 0..FIXTURE_H {
        for x in 0..FIXTURE_W {
            let v = 0.30 + 0.0005 * x as f32;
            image.put(x, y, [v, v, v]);
        }
    }
    paint(&mut image, rect, [0.72, 0.72, 0.72]);
    (image, normalise(rect))
}

/// A small region near the bottom-left corner, well inside the area cap.
///
/// 16 by 16 of a 200 by 200 frame is 0.64 % of it, which is comfortably under the 4 % cap and is
/// also *not* on the cap - a fixture sitting on its own threshold tests f32 arithmetic rather than
/// the rule, which is the trap phases 19, 21 and 22 each hit from one side or the other.
pub const CORNER: Rect = Rect {
    x: 16,
    y: 160,
    w: 16,
    h: 16,
};

/// A region in the middle of the frame, where a subject would be.
pub const CENTRE: Rect = Rect {
    x: 92,
    y: 92,
    w: 16,
    h: 16,
};

/// A region above the area cap: 44 by 44 of a 200 by 200 frame is 4.84 %.
pub const OVERSIZE: Rect = Rect {
    x: 20,
    y: 20,
    w: 44,
    h: 44,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corner_region_is_under_the_cap_and_not_on_it() {
        let region = normalise(CORNER);
        let area = region.w * region.h;
        assert!(area < aura_core::contract::cleanup::AREA_CAP_DEFAULT);
        assert!(
            area < aura_core::contract::cleanup::AREA_CAP_DEFAULT * 0.5,
            "the fixture must not sit near its own threshold, area was {area}"
        );
    }

    #[test]
    fn the_oversize_region_is_above_the_cap_and_not_on_it() {
        let region = normalise(OVERSIZE);
        let area = region.w * region.h;
        assert!(area > aura_core::contract::cleanup::AREA_CAP_DEFAULT);
    }

    #[test]
    fn every_background_produces_a_well_formed_frame() {
        for background in [
            Background::Grass,
            Background::Wall,
            Background::Railing,
            Background::Busy,
        ] {
            let image = clean(background);
            assert!(image.is_well_formed());
            assert_eq!(image.w, FIXTURE_W);
            assert_eq!(image.h, FIXTURE_H);
        }
    }

    #[test]
    fn a_shifted_frame_differs_from_its_original_but_is_the_same_room() {
        let clean = clean(Background::Busy);
        let drifted = shifted(Background::Busy, 7.0);
        assert_ne!(clean, drifted);
        assert_eq!(clean.w, drifted.w);
    }

    #[test]
    fn the_object_is_actually_in_the_pixels() {
        let (image, region) = with_object(Background::Grass, CORNER);
        let centre = image.at((CORNER.x + 8) as isize, (CORNER.y + 8) as isize);
        assert!(centre[0] > 0.9 && centre[1] < 0.2);
        assert!((region.w - 0.08).abs() < 1e-5);
    }
}
