//! Specular sheen: finding it, and reducing it without destroying what is under it.
//!
//! PHASE-19 section 6.3's last bullet:
//!
//! > Shine control detects specular pixels (high luma, low chroma, small area, near-highlight)
//! > and reduces luminance only, preserving underlying texture.
//!
//! ## The four conditions, and why all four are needed
//!
//! * **high luma** - sheen is the brightest thing on a face;
//! * **low chroma** - sheen is the *light's* colour rather than the skin's, so it is markedly
//!   less saturated than the face around it. This is the condition that separates a shiny
//!   forehead from a warm one, and without it a well-lit dark forehead reads as sheen and gets
//!   pulled down, which is the failure this phase must not have;
//! * **small area** - past [`SHINE_MAX_AREA`] the bright region is not a hot spot, it is the
//!   lighting;
//! * **near-highlight** - measured as the region's own brightness relative to the *face's*
//!   bright end rather than to an absolute value, because a face at a candle-lit ritual has
//!   sheen at 0.55 and a face under a window has skin at 0.80.
//!
//! ## Luminance only, and the type says so
//!
//! There is no radius, no smoothing strength and no texture parameter anywhere in
//! [`aura_core::contract::local::ShineReduction`], in migration 16's columns, or in this
//! module. The obvious wrong fix - blur the shiny bit - is an ADR away rather than a refactor
//! away, and that boundary is what separates this phase from phase 20.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::local::{
    MaskField, ShineReduction, MAX_SHINE_REDUCTION_EV, SHINE_CHROMA_CEILING, SHINE_LUMA_FLOOR,
    SHINE_MAX_AREA,
};

use crate::local::measure::{apply_ev, ev_between, FrameMeasure};

/// The grid a region search runs on, per side.
///
/// Thirty-two cells. Fine enough to separate a nose from a forehead on a face that fills a
/// tenth of the frame, coarse enough that the search is a few hundred cells rather than four
/// million pixels - which is what keeps section 11's 80 ms reachable.
pub const SEARCH_SIDE: usize = 32;

/// How far below a face's own bright end a pixel may sit and still be specular.
///
/// Relative rather than absolute, and the module header says why: a candle-lit face has sheen
/// at 0.55.
pub const RELATIVE_FLOOR: f32 = 0.88;

/// One found hot spot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    /// Where it is, in frame coordinates.
    pub bounds: CropRect,
    /// Its mean luminance.
    pub peak: f32,
    /// The fraction of the *face* it covers.
    pub area_of_face: f32,
}

/// What the detector concluded about one face.
#[derive(Debug, Clone, PartialEq)]
pub enum Found {
    /// Nothing specular. A real answer.
    Nothing,
    /// A bright region too large to be sheen. Also a real answer, and a different one.
    TooLarge,
    /// Hot spots worth reducing.
    Spots(Vec<Spot>),
}

/// Search one face for specular sheen.
///
/// `face` is the face box in frame coordinates. `skin` is phase 18's skin field, and there is
/// no fallback: reducing luminance inside a rectangle rather than inside a skin matte is how a
/// pearl earring and a white collar get darkened along with a forehead.
#[must_use]
pub fn find(frame: &FrameMeasure, face: CropRect, skin: Option<&MaskField>) -> Found {
    let Some(skin) = skin else {
        return Found::Nothing;
    };
    if !skin.is_usable() || !skin.is_readable() {
        return Found::Nothing;
    }
    let face = face.clamped();
    if face.is_empty() || frame.width == 0 || frame.height == 0 {
        return Found::Nothing;
    }
    let face_stats = frame.rect(face);
    let floor = SHINE_LUMA_FLOOR.min(face_stats.p95_luma * RELATIVE_FLOOR);

    // Walk the face on a coarse grid; a cell is specular when its own mean crosses all four
    // conditions.
    let mut cells: Vec<(usize, usize, f32)> = Vec::new();
    let mut specular_cells = 0usize;
    for gy in 0..SEARCH_SIDE {
        for gx in 0..SEARCH_SIDE {
            let cell = CropRect {
                x: face.x + face.w * gx as f32 / SEARCH_SIDE as f32,
                y: face.y + face.h * gy as f32 / SEARCH_SIDE as f32,
                w: face.w / SEARCH_SIDE as f32,
                h: face.h / SEARCH_SIDE as f32,
            };
            let stats = frame.rect(cell);
            if stats.is_empty() {
                continue;
            }
            // The skin field decides whether this is skin at all.
            let mx = ((cell.x + cell.w / 2.0) * f32::from(skin.width)) as u16;
            let my = ((cell.y + cell.h / 2.0) * f32::from(skin.height)) as u16;
            if skin.sample(mx, my) < 0.5 {
                continue;
            }
            if stats.mean_luma >= floor && stats.mean_chroma <= SHINE_CHROMA_CEILING {
                specular_cells += 1;
                cells.push((gx, gy, stats.mean_luma));
            }
        }
    }
    if cells.is_empty() {
        return Found::Nothing;
    }
    let area_of_face = specular_cells as f32 / (SEARCH_SIDE * SEARCH_SIDE) as f32;
    if area_of_face > SHINE_MAX_AREA {
        return Found::TooLarge;
    }

    Found::Spots(cluster(&cells, face, area_of_face))
}

/// Group adjacent specular cells into spots, largest first.
///
/// A flood fill on the coarse grid. Deterministic: cells are visited in row-major order and
/// the output is sorted by area and then by position, so a re-run produces the same
/// rectangles in the same order - which invariant 4 needs and a hash-set-based fill would not
/// give.
fn cluster(cells: &[(usize, usize, f32)], face: CropRect, area_of_face: f32) -> Vec<Spot> {
    let mut occupied = vec![false; SEARCH_SIDE * SEARCH_SIDE];
    let mut peak = vec![0.0f32; SEARCH_SIDE * SEARCH_SIDE];
    for (x, y, luma) in cells {
        if let Some(slot) = occupied.get_mut(y * SEARCH_SIDE + x) {
            *slot = true;
        }
        if let Some(slot) = peak.get_mut(y * SEARCH_SIDE + x) {
            *slot = *luma;
        }
    }
    let mut seen = vec![false; SEARCH_SIDE * SEARCH_SIDE];
    let mut spots: Vec<Spot> = Vec::new();
    for start in 0..(SEARCH_SIDE * SEARCH_SIDE) {
        if !occupied.get(start).copied().unwrap_or(false)
            || seen.get(start).copied().unwrap_or(true)
        {
            continue;
        }
        let mut stack = vec![start];
        let (mut x0, mut y0, mut x1, mut y1) = (SEARCH_SIDE, SEARCH_SIDE, 0usize, 0usize);
        let mut sum = 0.0f32;
        let mut count = 0usize;
        while let Some(index) = stack.pop() {
            if seen.get(index).copied().unwrap_or(true) {
                continue;
            }
            if !occupied.get(index).copied().unwrap_or(false) {
                continue;
            }
            if let Some(slot) = seen.get_mut(index) {
                *slot = true;
            }
            let x = index % SEARCH_SIDE;
            let y = index / SEARCH_SIDE;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            sum += peak.get(index).copied().unwrap_or(0.0);
            count += 1;
            if x > 0 {
                stack.push(index - 1);
            }
            if x + 1 < SEARCH_SIDE {
                stack.push(index + 1);
            }
            if y > 0 {
                stack.push(index - SEARCH_SIDE);
            }
            if y + 1 < SEARCH_SIDE {
                stack.push(index + SEARCH_SIDE);
            }
        }
        if count == 0 {
            continue;
        }
        spots.push(Spot {
            bounds: CropRect {
                x: face.x + face.w * x0 as f32 / SEARCH_SIDE as f32,
                y: face.y + face.h * y0 as f32 / SEARCH_SIDE as f32,
                w: face.w * (x1 + 1 - x0) as f32 / SEARCH_SIDE as f32,
                h: face.h * (y1 + 1 - y0) as f32 / SEARCH_SIDE as f32,
            }
            .clamped(),
            peak: sum / count as f32,
            area_of_face: area_of_face * count as f32 / cells.len().max(1) as f32,
        });
    }
    spots.sort_by(|a, b| {
        b.area_of_face
            .total_cmp(&a.area_of_face)
            .then(a.bounds.y.total_cmp(&b.bounds.y))
            .then(a.bounds.x.total_cmp(&b.bounds.x))
    });
    spots.truncate(ShineReduction::MAX_REGIONS);
    spots
}

/// Turn found spots into a reduction.
///
/// The reduction aims to bring the sheen down to the face's own bright end rather than to a
/// fixed value - which is the same relative reasoning [`RELATIVE_FLOOR`] uses, for the same
/// reason - and is then bounded by [`MAX_SHINE_REDUCTION_EV`] and by the strength.
#[must_use]
pub fn reduce(
    spots: &[Spot],
    identities: Vec<Option<aura_core::IdentityId>>,
    face_p95: f32,
    strength: f32,
    mask_scale: f32,
) -> Option<ShineReduction> {
    if spots.is_empty() {
        return None;
    }
    let scale = (strength.clamp(0.0, 1.0) * mask_scale.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    if scale <= 0.0 {
        return None;
    }
    let peak = spots.iter().map(|s| s.peak).sum::<f32>() / spots.len() as f32;
    let wanted = ev_between(peak, face_p95.min(peak)).min(0.0);
    let reduction = (wanted * scale).clamp(-MAX_SHINE_REDUCTION_EV, 0.0);
    if reduction >= -1e-3 {
        return None;
    }
    let mut identities = identities;
    identities.resize(spots.len(), None);
    Some(ShineReduction {
        regions: spots.iter().map(|s| s.bounds).collect(),
        identities,
        reduction_ev: reduction,
        area_fraction: spots.iter().map(|s| s.area_of_face).sum::<f32>().min(1.0),
        peak_before: peak,
        peak_after: apply_ev(peak, reduction),
        mask_scale,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame that is a mid-grey face with a bright, desaturated patch on it.
    fn frame_with(patch: Option<(usize, usize, usize, u8, u8, u8)>) -> FrameMeasure {
        let (w, h) = (64usize, 64usize);
        let mut rgb = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let inside = patch.is_some_and(|(px, py, size, _, _, _)| {
                    x >= px && x < px + size && y >= py && y < py + size
                });
                if inside {
                    if let Some((_, _, _, r, g, b)) = patch {
                        rgb.extend_from_slice(&[r, g, b]);
                        continue;
                    }
                }
                // Warm mid skin: clearly chromatic.
                rgb.extend_from_slice(&[168, 128, 104]);
            }
        }
        FrameMeasure::of(&rgb, w, h)
    }

    fn skin() -> MaskField {
        MaskField {
            kind: aura_core::contract::local::MaskKind::Skin,
            identity: None,
            bounds: CropRect::FULL,
            width: 8,
            height: 8,
            alpha: vec![255; 64],
            confidence: 0.9,
            edge_quality: 0.9,
            model_ver: 1,
        }
    }

    #[test]
    fn a_clean_face_reports_nothing_rather_than_a_tiny_spot() {
        let frame = frame_with(None);
        assert_eq!(find(&frame, CropRect::FULL, Some(&skin())), Found::Nothing);
    }

    #[test]
    fn a_bright_desaturated_patch_is_found() {
        let frame = frame_with(Some((26, 10, 8, 252, 250, 248)));
        let found = find(&frame, CropRect::FULL, Some(&skin()));
        match found {
            Found::Spots(spots) => assert!(!spots.is_empty()),
            other => panic!("a forehead hot spot read as {other:?}"),
        }
    }

    #[test]
    fn a_bright_but_saturated_patch_is_not_shine() {
        // The condition that protects a well-lit dark forehead. A warm bright patch is skin
        // catching the light, not the light itself.
        let frame = frame_with(Some((26, 10, 8, 255, 196, 150)));
        assert_eq!(find(&frame, CropRect::FULL, Some(&skin())), Found::Nothing);
    }

    #[test]
    fn a_large_bright_region_is_the_lighting_rather_than_shine() {
        let frame = frame_with(Some((8, 8, 48, 252, 250, 248)));
        assert_eq!(find(&frame, CropRect::FULL, Some(&skin())), Found::TooLarge);
    }

    #[test]
    fn no_skin_mask_means_no_shine_work_rather_than_a_rectangle() {
        let frame = frame_with(Some((26, 10, 8, 252, 250, 248)));
        assert_eq!(find(&frame, CropRect::FULL, None), Found::Nothing);
    }

    #[test]
    fn the_reduction_is_bounded_and_always_negative() {
        let spots = vec![Spot {
            bounds: CropRect {
                x: 0.4,
                y: 0.15,
                w: 0.12,
                h: 0.12,
            },
            peak: 0.98,
            area_of_face: 0.02,
        }];
        let reduction = reduce(&spots, vec![None], 0.62, 1.0, 1.0).expect("a hot spot reduces");
        assert!(reduction.reduction_ev < 0.0);
        assert!(reduction.reduction_ev >= -MAX_SHINE_REDUCTION_EV - 1e-6);
        assert!(reduction.peak_after < reduction.peak_before);
        assert_eq!(reduction.identities.len(), reduction.regions.len());
    }

    #[test]
    fn nothing_found_is_nothing_reduced() {
        assert!(reduce(&[], Vec::new(), 0.6, 1.0, 1.0).is_none());
    }

    #[test]
    fn a_weak_mask_reduces_less() {
        let spots = vec![Spot {
            bounds: CropRect::FULL,
            peak: 0.98,
            area_of_face: 0.02,
        }];
        let strong = reduce(&spots, vec![None], 0.62, 1.0, 1.0).expect("acts");
        let weak = reduce(&spots, vec![None], 0.62, 1.0, 0.3).expect("acts");
        assert!(weak.reduction_ev > strong.reduction_ev);
    }
}
