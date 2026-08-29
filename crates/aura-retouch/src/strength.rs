//! How much care one person gets, everywhere in the gallery.
//!
//! PHASE-20 section 6.4 lists four inputs - face size, scene, role and preset - and section
//! 10.1 asks that one identity retouch strength vary by no more than five per cent across a
//! wedding. Read literally those are incompatible: face size varies by an order of magnitude
//! between a portrait and a dance-floor wide.
//!
//! The resolution, argued in `docs/adr/ADR-0043-portrait-retouch-and-texture-protection.md`
//! section 6, is that all four inputs are taken as **gallery statistics**: a person role, the
//! preset, the *median* size their face is in the frames they appear in, and the scene they
//! mostly appear in. The result is one number per identity per project, and section 10.1 gate
//! is satisfied by construction rather than by measurement - the spread is zero, not five per
//! cent.
//!
//! What the individual frame decides is which operations run at all, and that lives in
//! [`crate::ops`].
//!
//! ## Why the median and not the mean
//!
//! One accidental close-up of a guest walking past the lens would pull a mean up far enough to
//! retouch them like a subject in four hundred photographs. The median asks the question that
//! matters: how does this person usually appear in this wedding.

use aura_core::contract::people::Role;
use aura_core::contract::retouch::{RetouchPreset, MIN_RETOUCHABLE_FACE};
use aura_core::{IdentityId, SceneId};

use crate::presets::PresetTable;

/// The face size, as a fraction of the frame shorter side, at which the size term reaches one.
///
/// A fifth. A face that fills a fifth of the frame is a portrait, and past that the person is
/// being photographed rather than being present.
pub const FULL_SIZE_FRACTION: f32 = 0.20;

/// How much of the strength the size term can take away.
///
/// Half. A person who only ever appears small still gets *some* care, because the frames they
/// are small in are also the frames nobody enlarges - and the one frame somebody does enlarge
/// would otherwise be the one frame they are unretouched in.
pub const SIZE_SHARE: f32 = 0.50;

/// What is known about one person across a whole project.
///
/// Every field is a gallery statistic. Nothing here is a property of one photograph, which is
/// what makes the answer a constant.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentityStats {
    /// Who.
    pub identity: IdentityId,
    /// What phase 06 decided they are to this wedding.
    pub role: Role,
    /// The median fraction of the frame their face covers, over the frames they appear in.
    pub median_face_frac: f32,
    /// The scene they appear in most.
    ///
    /// Used only to break the tie between two people with the same role and size: somebody whose
    /// frames are mostly `getting_ready_bride` is being photographed more deliberately than
    /// somebody whose frames are mostly `dance_floor`.
    pub dominant_scene: SceneId,
    /// How many frames they appear in.
    pub frames: u32,
}

impl IdentityStats {
    /// The stats of somebody nobody has identified.
    ///
    /// **The state every person in every photograph is in on this build**, because phase 06
    /// detector is a placeholder. It has to be a setting that is safe when it turns out to have
    /// been applied to the bride.
    #[must_use]
    pub fn unknown(identity: IdentityId) -> Self {
        Self {
            identity,
            role: Role::Unknown,
            median_face_frac: 0.0,
            dominant_scene: SceneId::Unknown,
            frames: 0,
        }
    }
}

/// The gallery-constant strength for one person.
///
/// Four terms, multiplied: the preset base, the role weight, the size ramp and the scene ceiling
/// of the scene they mostly appear in. Multiplied rather than averaged, for the reason phase 12
/// fuses its sub-scores geometrically: no term may rescue another, so a vendor who happens to be
/// photographed close up in a scene that allows full retouching is still a vendor.
#[must_use]
pub fn assign(stats: &IdentityStats, table: &PresetTable, preset: RetouchPreset) -> f32 {
    if preset.is_off() {
        return 0.0;
    }
    let row = table.preset(preset);
    let (scene, _) = table.scene(stats.dominant_scene);
    let size = size_term(stats.median_face_frac);
    (row.base_strength * table.role(stats.role) * size * scene.limit).clamp(0.0, 1.0)
}

/// How much of the strength a person of this typical size keeps.
///
/// One at [`FULL_SIZE_FRACTION`] and above, and never below `1 - SIZE_SHARE`. Below
/// [`MIN_RETOUCHABLE_FACE`] it is zero, which is the one place the size term is a switch rather
/// than a ramp: at that size the periorbital region is four pixels tall and every operation
/// would be measuring its own resampling.
#[must_use]
pub fn size_term(median_face_frac: f32) -> f32 {
    if median_face_frac < MIN_RETOUCHABLE_FACE {
        return 0.0;
    }
    let ramp = ((median_face_frac - MIN_RETOUCHABLE_FACE)
        / (FULL_SIZE_FRACTION - MIN_RETOUCHABLE_FACE))
        .clamp(0.0, 1.0);
    (1.0 - SIZE_SHARE) + SIZE_SHARE * ramp
}

/// The largest spread of one identity strength across a set of stored values.
///
/// Section 10.1 cross-frame consistency measurement, asked in one place so the store, the
/// outline and the eval harness cannot disagree about it. Zero while strength is a gallery
/// constant, and reported anyway - because a future change that made it per-frame should be
/// visible in the product rather than only in a diff.
#[must_use]
pub fn spread(values: &[f32]) -> f32 {
    if values.len() < 2 {
        return 0.0;
    }
    let lo = values.iter().copied().fold(f32::MAX, f32::min);
    let hi = values.iter().copied().fold(f32::MIN, f32::max);
    (hi - lo).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(n: u32) -> IdentityId {
        IdentityId::from_db(&format!("idt_00000000-0000-4000-8000-{n:012}")).expect("an identity")
    }

    fn stats(role: Role, size: f32, scene: SceneId) -> IdentityStats {
        IdentityStats {
            identity: identity(1),
            role,
            median_face_frac: size,
            dominant_scene: scene,
            frames: 40,
        }
    }

    #[test]
    fn the_bride_in_portraits_is_retouched_more_than_a_guest_on_the_dance_floor() {
        let table = PresetTable::embedded().expect("the table");
        let bride = assign(
            &stats(Role::Bride, 0.25, SceneId::CouplePortrait),
            &table,
            RetouchPreset::Natural,
        );
        let guest = assign(
            &stats(Role::Guest, 0.03, SceneId::DanceFloor),
            &table,
            RetouchPreset::Natural,
        );
        assert!(bride > guest, "{bride} is not above {guest}");
        assert!(guest <= 0.0, "a small guest on a dance floor got {guest}");
    }

    #[test]
    fn switching_the_preset_off_gives_everybody_zero() {
        let table = PresetTable::embedded().expect("the table");
        let off = assign(
            &stats(Role::Bride, 0.30, SceneId::CouplePortrait),
            &table,
            RetouchPreset::Off,
        );
        assert!(off <= 0.0);
    }

    #[test]
    fn a_face_below_the_floor_is_never_retouched_whatever_its_role() {
        assert!(size_term(MIN_RETOUCHABLE_FACE * 0.9) <= 0.0);
        assert!(size_term(MIN_RETOUCHABLE_FACE) > 0.0);
    }

    #[test]
    fn a_person_who_is_always_small_still_keeps_half_their_strength() {
        let term = size_term(MIN_RETOUCHABLE_FACE + 1e-4);
        assert!((term - (1.0 - SIZE_SHARE)).abs() < 1e-3);
    }

    #[test]
    fn no_single_term_can_rescue_another() {
        // A vendor photographed close up in the most generous scene is still a vendor.
        let table = PresetTable::embedded().expect("the table");
        let vendor = assign(
            &stats(Role::Vendor, 0.35, SceneId::GettingReadyBride),
            &table,
            RetouchPreset::Polished,
        );
        let couple = assign(
            &stats(Role::Couple, 0.10, SceneId::Ceremony),
            &table,
            RetouchPreset::Natural,
        );
        assert!(vendor < couple, "vendor {vendor} against couple {couple}");
    }

    #[test]
    fn the_spread_of_one_stored_strength_is_zero() {
        assert!(spread(&[0.7, 0.7, 0.7]) <= f32::EPSILON);
        assert!((spread(&[0.4, 0.7]) - 0.3).abs() < 1e-6);
    }
}
