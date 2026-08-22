//! The synthetic ground truth every section 10.1 gate is measured against.
//!
//! **These are not photographs and no number measured on them is a claim about one.** Every
//! fixture here is a frame whose noise, blur and structure were *painted into the pixels* by a
//! function in this file and are read back through the real detectors, the real operators, the
//! real renderer and the real store. That proves the arithmetic, the thresholds, the refusals and
//! the guarantees; it says nothing about a wedding. Condition C1 of
//! `docs/progress/PHASE-22-EXIT.md` is what closes when a real corpus arrives.
//!
//! The pattern is phase 10's and phase 20's: paint the answer in, read it back through the whole
//! path rather than through a copy of it, and make the fixture's own parameter the thing the gate
//! asserts against.
//!
//! ## The noise is deterministic and it is not a Gaussian
//!
//! Invariant 4 forbids a seedless generator, and the clock is a banned pattern for the same
//! reason, so [`noise_at`] is a hash of the sample index. It is uniform rather than normal, which makes it a
//! slightly *harder* test than real sensor noise: a uniform field has more energy in its tails at
//! the same standard deviation, so an edge-preserving filter has more chances to mistake a sample
//! for structure.
//!
//! ## The clean plate is kept
//!
//! Every noisy fixture is built from a plate that is also returned, so a gate can measure PSNR
//! and SSIM against the frame the noise was added to rather than against a blurred version of it.
//! Section 10.1's denoise gate is written that way and it is the only honest form of it.

use aura_core::contract::composition::Box2;
use aura_core::contract::integrity::MotionKind;
use aura_core::contract::restore::{RestoreField, RestoreRegion};
use aura_core::SceneId;
use aura_core::{IdentityId, PhotoId};

use crate::decide::RestoreFrame;
use crate::face_recovery::{FaceCandidate, IdentityProbe};

/// The side of every fixture frame, in pixels.
///
/// Ninety-six. Large enough that `bands::separate`'s radii are several samples across - below
/// about sixty-four the low-band radius collapses to one and the decomposition stops being a
/// decomposition - and small enough that a test that renders a frame four times still runs in
/// milliseconds.
pub const SIDE: usize = 96;

/// A photo id for one fixture index.
#[must_use]
pub fn photo(index: u8) -> PhotoId {
    let text = format!("pht_00000000-0000-4000-8000-0000000002{index:02}");
    PhotoId::from_db(&text).unwrap_or_else(|_| {
        // Unreachable: the format above is a valid v4 shape for every `u8`. A fallback rather
        // than an unwrap because this crate forbids both.
        PhotoId::from_db("pht_00000000-0000-4000-8000-000000000200").unwrap_or_default()
    })
}

/// An identity id for one fixture index.
#[must_use]
pub fn identity(index: u8) -> IdentityId {
    let text = format!("idt_00000000-0000-4000-8000-0000000002{index:02}");
    IdentityId::from_db(&text).unwrap_or_default()
}

/// Deterministic pseudo-noise at one sample, in `-1..1`.
///
/// A hash rather than a generator: invariant 4 requires the same input to produce the same
/// output, and a test fixture that changed between runs would make every threshold in this phase
/// a coin toss.
#[must_use]
pub fn noise_at(index: usize, channel: usize, salt: u32) -> f32 {
    let mut hash = (index as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((channel as u64) << 32)
        .wrapping_add(u64::from(salt).wrapping_mul(0x1234_5678_9ABC_DEF1));
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    hash ^= hash >> 33;
    ((hash >> 40) as f32 / 8_388_608.0) - 1.0
}

/// A plate with fine structure in it, at the scale a denoiser destroys first.
///
/// Lace, in the only sense this repository can produce one: a high-frequency pattern whose
/// amplitude is a few per cent of the local level, over a subject that has its own large-scale
/// shape. A denoiser that flattens the pattern loses the high band; one that flattens the shape
/// loses the photograph.
#[must_use]
pub fn lace_plate(width: usize, height: usize) -> Vec<f32> {
    let mut pixels = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            // The subject: a broad bright region on the left half.
            let shape: f32 = if x < width / 2 { 0.55 } else { 0.22 };
            // The lace: a two-sample chequer plus a six-sample one, so the pattern has energy in
            // both the mid and the high band and a filter cannot pass the gate by preserving one.
            let fine = if (x + y) % 2 == 0 { 0.030 } else { -0.030 };
            let coarse = if ((x / 3) + (y / 3)) % 2 == 0 {
                0.045
            } else {
                -0.045
            };
            let value = (shape + fine + coarse).clamp(0.02, 0.98);
            pixels.extend_from_slice(&[value, value * 0.97, value * 0.93]);
        }
    }
    pixels
}

/// A plate of hard vertical bars, for the kernel estimator and the ringing measurement.
#[must_use]
pub fn edge_plate(width: usize, height: usize, period: usize) -> Vec<f32> {
    let mut pixels = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let _ = y;
            let value = if (x / period.max(1)).is_multiple_of(2) {
                0.16
            } else {
                0.74
            };
            pixels.extend_from_slice(&[value, value, value]);
        }
    }
    pixels
}

/// Add deterministic noise of one standard deviation to a plate, in place.
///
/// Returns the plate it was added to, so a gate can measure against the frame the noise went onto
/// rather than against a blurred approximation of it.
#[must_use]
pub fn add_noise(plate: &[f32], sigma: f32, salt: u32) -> Vec<f32> {
    // A uniform field of half-width `h` has standard deviation `h / sqrt(3)`, so this is the
    // half-width that produces the sigma asked for.
    let half_width = sigma * 3.0_f32.sqrt();
    let mut out = Vec::with_capacity(plate.len());
    for (index, value) in plate.iter().enumerate() {
        let sample = noise_at(index / 3, index % 3, salt) * half_width;
        out.push((value + sample).clamp(0.0, 1.0));
    }
    out
}

/// A field covering the whole frame, for the regions this phase reads.
#[must_use]
pub fn full_field(region: RestoreRegion) -> RestoreField {
    RestoreField {
        region,
        identity: None,
        bounds: Box2 {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        width: 8,
        height: 8,
        alpha: vec![255; 64],
        confidence: 0.95,
        edge_quality: 0.92,
        model_ver: 1,
    }
}

/// A field covering the lower half of the frame, for the exclusion tests.
#[must_use]
pub fn lower_half_field(region: RestoreRegion) -> RestoreField {
    let mut alpha = vec![0u8; 64];
    for index in 32..64 {
        if let Some(slot) = alpha.get_mut(index) {
            *slot = 255;
        }
    }
    RestoreField {
        region,
        identity: None,
        bounds: Box2 {
            x: 0.0,
            y: 0.5,
            w: 1.0,
            h: 0.5,
        },
        width: 8,
        height: 8,
        alpha,
        confidence: 0.95,
        edge_quality: 0.92,
        model_ver: 1,
    }
}

/// The three regions a sharpenable frame carries: a subject, some skin and a sky.
#[must_use]
pub fn regions() -> Vec<RestoreField> {
    vec![
        full_field(RestoreRegion::Subject),
        full_field(RestoreRegion::Skin),
        lower_half_field(RestoreRegion::Sky),
    ]
}

/// A frame with no more noise than its scene tolerates and nothing to sharpen.
///
/// The control. Every gate that asserts a refusal needs a frame where the same code path does
/// nothing, so that a refusal is distinguishable from a solver that never ran.
#[must_use]
pub fn clean_frame() -> RestoreFrame {
    RestoreFrame {
        image_id: photo(1),
        pixels: lace_plate(SIDE, SIDE),
        width: SIDE,
        height: SIDE,
        scene: SceneId::CouplePortrait,
        make: "SONY".to_string(),
        model: "ILCE-7M3".to_string(),
        iso: 200,
        noise_sigma_rel: Some(0.5),
        motion: MotionKind::None,
        motion_severity: 0.0,
        focus_offset: 0.0,
        prominence: 0.3,
        output_long_edge: 2048,
        regions: regions(),
        faces: Vec::new(),
    }
}

/// A dance-floor frame: far past its scene's tolerance, with real noise in the pixels.
#[must_use]
pub fn noisy_frame() -> RestoreFrame {
    let plate = lace_plate(SIDE, SIDE);
    RestoreFrame {
        image_id: photo(2),
        pixels: add_noise(&plate, 0.030, 22),
        scene: SceneId::DanceFloor,
        iso: 12_800,
        noise_sigma_rel: Some(2.8),
        ..clean_frame()
    }
}

/// The clean plate `noisy_frame` was built from, for the PSNR and SSIM gate.
#[must_use]
pub fn noisy_frame_plate() -> Vec<f32> {
    lace_plate(SIDE, SIDE)
}

/// A frame that is slightly soft in a way deconvolution can recover.
///
/// Blurred at radius one, which measures inside `SHARPEN_KERNEL_LO..SHARPEN_KERNEL_HI`. Radius
/// two is past the ceiling: three box passes at radius `r` approximate a Gaussian of sigma
/// about `r * 1.4`, so radius two is around 2.8 and `KernelTooLarge` refuses it - which is the
/// operator working, and makes it the wrong fixture for a gate about sharpening happening.
#[must_use]
pub fn soft_frame() -> RestoreFrame {
    let plate = edge_plate(SIDE, SIDE, 8);
    let plane = aura_render::spatial::luma_plane(&plate, SIDE, SIDE);
    let blurred = aura_render::bands::blur(&plane, SIDE, SIDE, 1);
    let mut pixels = Vec::with_capacity(SIDE * SIDE * 3);
    for value in blurred {
        pixels.extend_from_slice(&[value, value, value]);
    }
    RestoreFrame {
        image_id: photo(3),
        pixels,
        scene: SceneId::FirstLook,
        iso: 800,
        noise_sigma_rel: Some(0.6),
        ..clean_frame()
    }
}

/// A frame whose softness is motion rather than focus. Section 2.2's exclusion, as a fixture.
#[must_use]
pub fn motion_frame() -> RestoreFrame {
    RestoreFrame {
        image_id: photo(4),
        motion: MotionKind::SubjectMotion,
        motion_severity: 0.6,
        ..soft_frame()
    }
}

/// A frame whose focus landed behind the subject.
#[must_use]
pub fn back_focus_frame() -> RestoreFrame {
    RestoreFrame {
        image_id: photo(5),
        focus_offset: 0.55,
        ..soft_frame()
    }
}

/// A frame with one face inside the soft band, for the identity constraint.
#[must_use]
pub fn soft_face_frame() -> RestoreFrame {
    RestoreFrame {
        image_id: photo(6),
        scene: SceneId::Kiss,
        faces: vec![FaceCandidate {
            identity: Some(identity(1)),
            bounds: Box2 {
                x: 0.20,
                y: 0.20,
                w: 0.45,
                h: 0.45,
            },
            sharpness: 0.55,
        }],
        ..clean_frame()
    }
}

/// A frame whose face is far too blurred for a prior to be told what to do.
#[must_use]
pub fn blurred_face_frame() -> RestoreFrame {
    let mut frame = soft_face_frame();
    frame.image_id = photo(7);
    if let Some(face) = frame.faces.first_mut() {
        face.sharpness = 0.10;
    }
    frame
}

/// A frame in a scene whose profile row forbids sharpening.
#[must_use]
pub fn no_sharpen_scene_frame() -> RestoreFrame {
    RestoreFrame {
        image_id: photo(8),
        scene: SceneId::Vows,
        ..soft_frame()
    }
}

/// A frame with no integrity verdict at all.
#[must_use]
pub fn unmeasured_frame() -> RestoreFrame {
    RestoreFrame {
        image_id: photo(9),
        noise_sigma_rel: None,
        ..noisy_frame()
    }
}

/// A probe whose vector rotates with the crop own high-band energy.
///
/// **Not phase 06 recogniser and not a stand-in for one.** What it gives a gate is a measurable,
/// monotone response to the operator, which is what the identity constraint has to be exercised
/// against: a probe that returned a constant would let a broken constraint pass, and a probe that
/// returned noise would fail a correct one.
///
/// The response is an **angle** rather than a scaled component, and that is the whole design.
/// The obvious probe - put the band energy into one element of the vector and multiply it by a
/// gain - does not work, because cosine distance is a function of *direction*: scaling one
/// component by a large factor makes both the before and the after vector point along that
/// component, and the distance between them collapses toward zero as the gain rises. A probe
/// built that way reports a *smaller* identity change the more sensitive it claims to be. Here
/// the energy is a rotation of `gain` radians per unit, so the distance grows with the gain the
/// way a gate needs it to.
///
/// `gain` is how violently it reacts, so one fixture can describe an operator the constraint
/// waves through and another can describe one it must refuse.
#[derive(Debug, Clone, Copy)]
pub struct BandProbe {
    /// How strongly the vector responds to the high band.
    pub gain: f32,
}

impl BandProbe {
    /// A probe that treats the operator as almost harmless.
    #[must_use]
    pub const fn gentle() -> Self {
        Self { gain: 2.0 }
    }

    /// A probe that treats any recovery as an identity change.
    ///
    /// A hundred radians per unit of high-band energy, which is enough that even the strength
    /// left after three reductions still moves the vector past `MAX_IDENTITY_DRIFT`.
    #[must_use]
    pub const fn severe() -> Self {
        Self { gain: 100.0 }
    }
}

impl IdentityProbe for BandProbe {
    fn embed(&self, rgb: &[f32], width: usize, height: usize) -> Option<Vec<f32>> {
        if width == 0 || height == 0 || rgb.len() < width * height * 3 {
            return None;
        }
        let plane = aura_render::spatial::luma_plane(rgb, width, height);
        let bands = aura_render::bands::separate(&plane, width, height);
        let angle = bands.high_energy() * self.gain;
        Some(vec![angle.cos(), angle.sin()])
    }
}

/// A probe that cannot embed anything, for the "a guarantee that cannot be measured" path.
#[derive(Debug, Clone, Copy)]
pub struct BlindProbe;

impl IdentityProbe for BlindProbe {
    fn embed(&self, _rgb: &[f32], _width: usize, _height: usize) -> Option<Vec<f32>> {
        None
    }
}

/// Peak signal-to-noise ratio between two frames, in decibels.
///
/// Section 10.1's denoise gate is written against a bilinear baseline, so both numbers have to be
/// produced the same way. This is that measurement and it lives here rather than in the harness
/// because two harnesses computing it separately is two answers.
#[must_use]
pub fn psnr(reference: &[f32], candidate: &[f32]) -> f32 {
    if reference.is_empty() || reference.len() != candidate.len() {
        return 0.0;
    }
    let mut sum = 0.0f64;
    for (a, b) in reference.iter().zip(candidate.iter()) {
        let error = f64::from(*a) - f64::from(*b);
        sum += error * error;
    }
    let mse = sum / reference.len() as f64;
    if mse <= 1e-12 {
        return 99.0;
    }
    (10.0 * (1.0 / mse).log10()) as f32
}

/// The SSIM stabiliser for the luminance term, for data in `0..1`.
const SSIM_C1: f64 = 0.0001;

/// The SSIM stabiliser for the contrast and structure terms.
const SSIM_C2: f64 = 0.0009;

/// A global structural similarity index between two frames, `0..1`.
///
/// The single-window form over the whole luminance plane. It is a weaker statistic than the
/// windowed version and it is the one this repository can honestly compute: a windowed SSIM over
/// a 96 px synthetic plate is dominated by the window size rather than by the images.
#[must_use]
pub fn ssim(reference: &[f32], candidate: &[f32], width: usize, height: usize) -> f32 {
    if width == 0 || height == 0 {
        return 0.0;
    }
    let a = aura_render::spatial::luma_plane(reference, width, height);
    let b = aura_render::spatial::luma_plane(candidate, width, height);
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let n = a.len() as f64;
    let mean_a: f64 = a.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let mean_b: f64 = b.iter().map(|v| f64::from(*v)).sum::<f64>() / n;
    let mut var_a = 0.0f64;
    let mut var_b = 0.0f64;
    let mut covariance = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        let da = f64::from(*x) - mean_a;
        let db = f64::from(*y) - mean_b;
        var_a += da * da;
        var_b += db * db;
        covariance += da * db;
    }
    var_a /= n;
    var_b /= n;
    covariance /= n;
    let numerator = (2.0 * mean_a * mean_b + SSIM_C1) * (2.0 * covariance + SSIM_C2);
    let denominator = (mean_a * mean_a + mean_b * mean_b + SSIM_C1) * (var_a + var_b + SSIM_C2);
    if denominator.abs() <= f64::EPSILON {
        return 0.0;
    }
    ((numerator / denominator) as f32).clamp(0.0, 1.0)
}

/// The bilinear baseline section 10.1's denoise gate is measured against.
///
/// A plain box blur at radius one, which is what "bilinear" means as a denoiser: every sample
/// replaced by the average of its neighbourhood, with no edge awareness at all. It removes noise
/// and it removes the photograph, and beating it decisively is the least a real denoiser can do.
#[must_use]
pub fn bilinear_baseline(pixels: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(pixels.len());
    for channel in 0..3 {
        let mut plane = Vec::with_capacity(width * height);
        for index in 0..width * height {
            plane.push(pixels.get(index * 3 + channel).copied().unwrap_or(0.0));
        }
        let blurred = aura_render::bands::blur(&plane, width, height, 1);
        out.push(blurred);
    }
    let mut interleaved = Vec::with_capacity(pixels.len());
    for index in 0..width * height {
        for channel in 0..3 {
            interleaved.push(
                out.get(channel)
                    .and_then(|plane| plane.get(index))
                    .copied()
                    .unwrap_or(0.0),
            );
        }
    }
    interleaved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_noise_is_the_amplitude_it_claims_and_it_is_deterministic() {
        let plate = lace_plate(SIDE, SIDE);
        let once = add_noise(&plate, 0.03, 7);
        let again = add_noise(&plate, 0.03, 7);
        assert_eq!(once, again, "the fixture noise is not deterministic");

        let mut sum = 0.0f64;
        let mut count = 0u32;
        for (a, b) in plate.iter().zip(once.iter()) {
            // Samples the clamp touched are excluded: they are not the noise the fixture added.
            if *b > 0.001 && *b < 0.999 {
                let d = f64::from(*b) - f64::from(*a);
                sum += d * d;
                count += 1;
            }
        }
        let measured = (sum / f64::from(count.max(1))).sqrt() as f32;
        assert!(
            (measured - 0.03).abs() < 0.004,
            "the fixture noise measured {measured}"
        );

        // And a different salt is a different field.
        assert_ne!(once, add_noise(&plate, 0.03, 8));
    }

    #[test]
    fn the_lace_plate_has_energy_in_both_bands() {
        // A plate whose structure were all in one band would let a filter pass the smearing gate
        // by preserving the other.
        let plate = lace_plate(SIDE, SIDE);
        let plane = aura_render::spatial::luma_plane(&plate, SIDE, SIDE);
        let bands = aura_render::bands::separate(&plane, SIDE, SIDE);
        assert!(bands.high_energy() > 0.005, "{}", bands.high_energy());
        assert!(bands.mid_energy() > 0.005, "{}", bands.mid_energy());
    }

    #[test]
    fn psnr_and_ssim_are_perfect_on_an_identical_pair_and_worse_on_a_blurred_one() {
        let plate = lace_plate(SIDE, SIDE);
        assert!(psnr(&plate, &plate) > 90.0);
        assert!((ssim(&plate, &plate, SIDE, SIDE) - 1.0).abs() < 1e-3);

        let blurred = bilinear_baseline(&plate, SIDE, SIDE);
        assert!(psnr(&plate, &blurred) < 60.0);
        assert!(ssim(&plate, &blurred, SIDE, SIDE) < 1.0);
    }

    #[test]
    fn every_fixture_frame_is_readable() {
        for frame in [
            clean_frame(),
            noisy_frame(),
            soft_frame(),
            motion_frame(),
            back_focus_frame(),
            soft_face_frame(),
            blurred_face_frame(),
            no_sharpen_scene_frame(),
            unmeasured_frame(),
        ] {
            assert!(frame.is_readable(), "{} is not readable", frame.image_id);
            assert!(frame.megapixels() > 0.0);
        }
    }

    #[test]
    fn the_probes_behave_the_way_the_gates_need_them_to() {
        let plate = lace_plate(32, 32);
        assert!(BandProbe::gentle().embed(&plate, 32, 32).is_some());
        assert!(BlindProbe.embed(&plate, 32, 32).is_none());
        // A severe probe really is more sensitive: the same *change* in the pixels moves its
        // vector further. Measured rather than asserted about one component, because the whole
        // point of the angular form is that a component comparison is the wrong test.
        let mut smoother = plate.clone();
        for index in 0..32 * 32 {
            for offset in 0..3 {
                if let Some(slot) = smoother.get_mut(index * 3 + offset) {
                    *slot = 0.4;
                }
            }
        }
        let moved = |probe: BandProbe| -> f32 {
            let a = probe.embed(&plate, 32, 32).unwrap_or_default();
            let b = probe.embed(&smoother, 32, 32).unwrap_or_default();
            crate::face_recovery::cosine_distance(&a, &b)
        };
        assert!(moved(BandProbe::severe()) > moved(BandProbe::gentle()));
    }
}
