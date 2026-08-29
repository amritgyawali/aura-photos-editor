//! Squaring up architecture. PHASE-23 section 6.2.
//!
//! A camera pointed up at a church makes its walls lean together. Correcting that means
//! stretching one end of the frame relative to the other, and the whole difficulty is knowing
//! when to stop: past a certain factor a squared-up doorway stops looking squared up and
//! starts looking like a photograph taken through a letterbox.
//!
//! ## The correction is refused past the cap, not reduced to it
//!
//! [`MAX_STRETCH`] is 1.25. Above it the keystone is **abandoned**, and that is a deliberate
//! choice over the alternative of clamping: a keystone that has been halved to fit a cap has
//! stopped correcting anything - the walls still lean, by half as much, and the frame has been
//! resampled and cropped to achieve it. Half a correction is the worst of both. This is the
//! same argument phase 16 made about clamping a curve node, arrived at from the other side.
//!
//! ## Three verticals, not two
//!
//! Two lines always meet somewhere, and calling that a vanishing point is how a keystone tool
//! squares up a frame containing one door frame and a guest. [`Keystone::MIN_VERTICALS`] is
//! three, and `verticals` is stored on the row so a stored correction can say what it was
//! fitted from.
//!
//! ## What "vertical" means here
//!
//! A line whose direction is within [`VERTICAL_TOLERANCE_DEG`] of the frame's vertical axis
//! **after straightening**. The order matters: a frame that is three degrees off level has
//! every vertical in it three degrees off vertical, and a keystone fitted before the rotation
//! is a keystone fitted to a tilt.

use aura_core::contract::geometry::{
    GeometryCode, GeometryReason, Keystone, MAX_STRETCH, MIN_KEYSTONE,
};

/// How far from vertical a line may run and still be architecture.
///
/// Thirty-five degrees. Generous, because the whole point is that they are *converging* - a
/// wide-angle frame of a tall building has verticals at the frame edge running fifteen degrees
/// off, and a tolerance tight enough to exclude a diagonal excludes those too.
pub const VERTICAL_TOLERANCE_DEG: f32 = 35.0;

/// One near-vertical line, as its x position at the top and bottom of the frame.
///
/// Normalised. A line that leaves the frame is still described by where it *would* cross,
/// which is what makes the convergence measurable on a line that only spans the middle third.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerticalLine {
    /// Where it crosses `y = 0`.
    pub top_x: f32,
    /// Where it crosses `y = 1`.
    pub bottom_x: f32,
    /// How much gradient energy voted for it, `0..1`. Used only to drop the weakest.
    pub strength: f32,
}

impl VerticalLine {
    /// The angle from vertical, in degrees.
    #[must_use]
    pub fn angle_deg(&self, aspect: f32) -> f32 {
        ((self.bottom_x - self.top_x) * aspect).atan().to_degrees()
    }

    /// True when this line is close enough to vertical to be architecture.
    #[must_use]
    pub fn is_architecture(&self, aspect: f32) -> bool {
        self.angle_deg(aspect).abs() <= VERTICAL_TOLERANCE_DEG
    }
}

/// What squaring up decided.
#[derive(Debug, Clone, PartialEq)]
pub struct Keystoned {
    /// The correction, when one survived.
    pub keystone: Option<Keystone>,
    /// Why.
    pub reasons: Vec<GeometryReason>,
}

impl Keystoned {
    fn none(code: GeometryCode) -> Self {
        Self {
            keystone: None,
            reasons: vec![GeometryReason::plain(code, -0.01)],
        }
    }
}

/// Decide whether and how far to square the frame up.
///
/// `lines` are the near-vertical lines found **after** the rotation has been decided.
#[must_use]
pub fn decide(lines: &[VerticalLine], aspect: f32) -> Keystoned {
    let usable: Vec<&VerticalLine> = lines
        .iter()
        .filter(|line| line.is_architecture(aspect) && line.strength > 0.1)
        .collect();
    if usable.len() < Keystone::MIN_VERTICALS as usize {
        return Keystoned::none(GeometryCode::KeystoneNoVerticals);
    }

    // The convergence ratio: how wide the fan of verticals is at the top of the frame against
    // how wide it is at the bottom. Below one the top is narrower, which is a camera pointed
    // up - the church case, and by far the common one.
    let spread = |at_top: bool| -> f32 {
        let xs: Vec<f32> = usable
            .iter()
            .map(|line| if at_top { line.top_x } else { line.bottom_x })
            .collect();
        let min = xs.iter().copied().fold(f32::INFINITY, f32::min);
        let max = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        max - min
    };
    let (top, bottom) = (spread(true), spread(false));
    if top <= 1e-3 || bottom <= 1e-3 {
        // Every line at the same x: parallel, or one line found three times.
        return Keystoned::none(GeometryCode::KeystoneNoVerticals);
    }
    let ratio = top / bottom;
    let stretch = ratio.max(1.0 / ratio);
    if stretch > MAX_STRETCH {
        return Keystoned::none(GeometryCode::KeystoneCapped);
    }
    // `vertical` in the recipe's -100..100 units: how much the narrow end must be widened.
    // Positive widens the top, which is the camera-pointed-up case.
    let vertical = ((1.0 / ratio) - 1.0) * 100.0;
    // Applied symmetrically - the narrow end out by the square root, the wide end in by the
    // same - so the frame's own scale does not move. The inscribed rectangle is then bounded
    // by whichever end ends up narrower, which is why the scale is exactly that square root.
    let scale = stretch.sqrt();
    let candidate = Keystone::new(vertical, 0.0, scale, stretch, usable.len() as u16);
    match candidate {
        Ok(keystone) if !keystone.is_negligible() => Keystoned {
            keystone: Some(keystone),
            reasons: vec![GeometryReason::frame(
                GeometryCode::KeystoneApplied,
                format!(
                    "Converging vertical lines were squared up, from {} lines.",
                    usable.len()
                ),
                0.05,
            )],
        },
        // Below `MIN_KEYSTONE` the correction is a resample for a change nobody can see, which
        // is the same argument `MIN_ROTATE_DEG` makes about a rotation.
        Ok(_) => Keystoned::none(GeometryCode::KeystoneNoVerticals),
        Err(_) => Keystoned::none(GeometryCode::KeystoneCapped),
    }
}

/// The rectangle a keystone leaves usable, as a fraction of the frame.
///
/// A keystone opens two corners; they are cropped away, never filled. Section 2.2 puts filling
/// in phase 24, and until it exists there is nothing to put in them.
#[must_use]
pub fn usable_fraction(keystone: &Keystone) -> f32 {
    if keystone.scale <= 1.0 {
        return 1.0;
    }
    (1.0 / keystone.scale).clamp(0.0, 1.0)
}

/// True when this much keystone is worth applying at all.
#[must_use]
pub fn is_worth_it(vertical: f32) -> bool {
    vertical.abs() >= MIN_KEYSTONE
}

/// Track near-vertical lines out of a luminance plane.
///
/// A restricted Hough: every strong vertical gradient point votes for the lines through it in
/// a bounded fan of angles, and the peaks become [`VerticalLine`]s. Restricted rather than
/// general because the only thing this phase does with a line is measure how the fan of them
/// converges, and a general transform spends most of its votes on the diagonals of a dance
/// floor.
///
/// Deterministic: the accumulator is scanned in a fixed order and ties resolve to the lower
/// index.
/// Angle bins in the restricted Hough accumulator.
const ANGLE_BINS: usize = 33;

/// Offset bins in the restricted Hough accumulator.
const OFFSET_BINS: usize = 64;

#[must_use]
pub fn track_verticals(
    luma: &[f32],
    width: usize,
    height: usize,
    aspect: f32,
) -> Vec<VerticalLine> {
    if width < 8 || height < 8 {
        return Vec::new();
    }
    let at = |x: usize, y: usize| -> f32 { luma.get(y * width + x).copied().unwrap_or(0.0) };
    let max_slope = VERTICAL_TOLERANCE_DEG.to_radians().tan() / aspect.max(1e-3);
    let mut votes = vec![0.0f32; ANGLE_BINS * OFFSET_BINS];

    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let gradient = (at(x + 1, y) - at(x - 1, y)).abs();
            if gradient < 0.10 {
                continue;
            }
            let ny = y as f32 / height as f32;
            let nx = x as f32 / width as f32;
            for bin in 0..ANGLE_BINS {
                let t = bin as f32 / (ANGLE_BINS - 1) as f32 * 2.0 - 1.0;
                let slope = t * max_slope;
                // Where this line would cross the top of the frame.
                let top = nx - slope * ny;
                if !(0.0..=1.0).contains(&top) {
                    continue;
                }
                let offset =
                    ((top * (OFFSET_BINS - 1) as f32).round() as usize).min(OFFSET_BINS - 1);
                if let Some(slot) = votes.get_mut(bin * OFFSET_BINS + offset) {
                    *slot += gradient;
                }
            }
        }
    }

    // Peak-pick with a small exclusion so one wall does not become six lines.
    let mut peaks: Vec<(usize, usize, f32)> = Vec::new();
    let strongest = votes.iter().copied().fold(0.0f32, f32::max);
    if strongest <= f32::EPSILON {
        return Vec::new();
    }
    for bin in 0..ANGLE_BINS {
        for offset in 0..OFFSET_BINS {
            let value = votes
                .get(bin * OFFSET_BINS + offset)
                .copied()
                .unwrap_or(0.0);
            if value < strongest * 0.35 {
                continue;
            }
            if peaks.iter().any(|(_, other, _)| other.abs_diff(offset) < 3) {
                continue;
            }
            peaks.push((bin, offset, value));
        }
    }
    peaks.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
    peaks.truncate(12);
    peaks
        .into_iter()
        .map(|(bin, offset, value)| {
            let t = bin as f32 / (ANGLE_BINS - 1) as f32 * 2.0 - 1.0;
            let slope = t * max_slope;
            let top_x = offset as f32 / (OFFSET_BINS - 1) as f32;
            VerticalLine {
                top_x,
                bottom_x: top_x + slope,
                strength: (value / strongest).clamp(0.0, 1.0),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASPECT: f32 = 0.6667; // A portrait frame, which is how a church is shot.

    fn fan(convergence: f32, count: usize) -> Vec<VerticalLine> {
        // `convergence` is the top spread as a fraction of the bottom spread.
        (0..count)
            .map(|i| {
                let t = i as f32 / (count - 1).max(1) as f32;
                let bottom_x = 0.1 + 0.8 * t;
                let top_x = 0.5 + (bottom_x - 0.5) * convergence;
                VerticalLine {
                    top_x,
                    bottom_x,
                    strength: 0.8,
                }
            })
            .collect()
    }

    #[test]
    fn a_fan_of_converging_verticals_is_squared_up() {
        let out = decide(&fan(0.88, 5), ASPECT);
        let keystone = out.keystone.expect("a correction");
        assert!(keystone.vertical > 0.0, "the top should widen");
        assert!(keystone.stretch <= MAX_STRETCH);
        assert!(keystone.scale >= 1.0);
        assert_eq!(keystone.verticals, 5);
        assert!(out
            .reasons
            .iter()
            .any(|r| r.code == GeometryCode::KeystoneApplied));
    }

    #[test]
    fn too_much_convergence_is_refused_rather_than_clamped() {
        let out = decide(&fan(0.55, 6), ASPECT);
        assert!(
            out.keystone.is_none(),
            "a correction past the cap survived: {:?}",
            out.keystone
        );
        assert!(out
            .reasons
            .iter()
            .any(|r| r.code == GeometryCode::KeystoneCapped));
    }

    #[test]
    fn two_lines_are_never_a_vanishing_point() {
        let out = decide(&fan(0.88, 2), ASPECT);
        assert!(out.keystone.is_none());
        assert!(out
            .reasons
            .iter()
            .any(|r| r.code == GeometryCode::KeystoneNoVerticals));
    }

    #[test]
    fn parallel_verticals_are_left_alone() {
        let out = decide(&fan(1.0, 6), ASPECT);
        assert!(out.keystone.is_none());
    }

    #[test]
    fn a_diagonal_is_not_architecture() {
        // A line crossing the whole width of a *landscape* frame: 56 degrees off vertical,
        // which is a handrail rather than a wall. The tolerance is generous on purpose - a
        // wide-angle frame of a tall building has verticals fifteen degrees off at the edge -
        // so the line that fails it has to be a real diagonal.
        let diagonal = VerticalLine {
            top_x: 0.05,
            bottom_x: 0.95,
            strength: 1.0,
        };
        assert!(!diagonal.is_architecture(1.5));
        assert!(diagonal.angle_deg(1.5).abs() > VERTICAL_TOLERANCE_DEG);
        let leaning = VerticalLine {
            top_x: 0.40,
            bottom_x: 0.50,
            strength: 1.0,
        };
        assert!(leaning.is_architecture(ASPECT));
    }

    #[test]
    fn a_weak_line_does_not_count_toward_the_minimum() {
        let mut lines = fan(0.88, 4);
        for line in lines.iter_mut().take(2) {
            line.strength = 0.05;
        }
        assert!(decide(&lines, ASPECT).keystone.is_none());
    }

    #[test]
    fn the_scale_is_exactly_what_hides_the_corners() {
        let out = decide(&fan(0.90, 5), ASPECT);
        let keystone = out.keystone.expect("a correction");
        assert!(
            (keystone.scale - keystone.stretch.sqrt()).abs() < 1e-5,
            "{} vs {}",
            keystone.scale,
            keystone.stretch.sqrt()
        );
        let usable = usable_fraction(&keystone);
        assert!(usable < 1.0 && usable > 0.85, "{usable}");
    }

    #[test]
    fn a_camera_pointed_down_widens_the_bottom() {
        // Convergence above one: the top is wider, so the camera was pointed down.
        let out = decide(&fan(1.12, 5), ASPECT);
        let keystone = out.keystone.expect("a correction");
        assert!(keystone.vertical < 0.0, "the bottom should widen");
    }
}
