//! What lights are in this photograph, and which one is on the people.
//!
//! PHASE-15 section 8's steps 3 and 7: "implement the illuminant hypothesis generators
//! including a learned CNN predictor" and "implement mixed-light and coloured-light handling
//! with explicit notes". Section 6.2 is the design and this module is all of it except the
//! scoring, which is [`super::wb`], and the choice, which is [`super::solve`].
//!
//! ## Six generators, and none of them is trusted
//!
//! | Generator | What it knows | How it fails |
//! |---|---|---|
//! | camera as-shot | what the body decided | the body's own auto white balance, wrong in the same places ours would be |
//! | grey-world | the frame averages to grey | a frame that is mostly one colour, which at a wedding it very often is |
//! | white-patch | the brightest surface is white | the brightest thing is a coloured light |
//! | learned | a thumbnail | untrained in this build; see the exit report's condition C1 |
//! | known neutral | a dress, a tablecloth, paper | there was no neutral, and it found a beige wall |
//! | skin | this person's own measured locus | nobody has a locus yet |
//!
//! The point of six is that they fail in *different* places. Section 6.2's design is
//! multi-hypothesis for that reason and not for redundancy: the frames where grey-world is
//! catastrophically wrong - a red mandap, a green marquee, a purple dance floor - are exactly
//! the frames where a known neutral or a skin locus is decisive.
//!
//! ## The two hard cases, and where they are decided
//!
//! **Mixed light.** [`detect_mixed`] splits the frame into `stats::GRID` squared cells and
//! asks whether their chromaticities form two groups rather than one. Two illuminants of the
//! same colour in different directions produce one group and are correctly not mixed; a
//! window on one side of a tungsten-lit room produces two. Section 6.2 then says the *global*
//! answer is the light on the subject and both are recorded, which is
//! [`dominant_on_subject`] and phase 18's input.
//!
//! **Coloured light.** [`chroma_of`] is the distance from the Planckian and daylight loci,
//! not the distance from D65. That distinction is the whole test: a 2700 K tungsten bulb is a
//! very long way from D65 and is *not* a creative choice, while a purple uplighter at a
//! nominal 4000 K is barely warmer than daylight and is entirely a creative choice. Asking
//! "how far from white" would get both backwards.

use aura_core::contract::integrity::CropRect;
use aura_core::contract::people::FaceRef;
use aura_core::contract::tone::{HypothesisSource, Illuminant, IlluminantKind};
use aura_core::AttrFlags;
use aura_raw::colour::illuminant::{
    cct_from_uv, cct_to_uv, duv, uv_distance, uv_to_linear_srgb, D65_UV,
};

use crate::tone::neutrals::NeutralPatch;
use crate::tone::stats::{Cell, FrameStats, GRID};

/// How far apart two groups of cells must sit before the frame is called mixed-light.
///
/// 0.030 in `u'v'`. For scale: 2800 K and 5500 K are 0.072 apart, and 4800 K and 5500 K are
/// 0.010 apart. So this threshold catches a tungsten room with a window and ignores two
/// softboxes of nominally the same temperature, which is the line section 6.2 draws when it
/// says "if two strong illuminants disagree spatially".
pub const MIXED_SEPARATION: f32 = 0.030;

/// The smallest share of voting cells a second illuminant needs to count.
///
/// A fifth. Below that the "second light" is a lamp in the corner of the frame, and marking
/// a photograph for local correction because of a lamp in the corner would put half a
/// reception into phase 18's queue.
pub const MIXED_MIN_SHARE: f32 = 0.20;

/// Below this correlated colour temperature a light is tungsten rather than daylight.
pub const TUNGSTEN_BELOW_K: f32 = 3500.0;

/// Below this a light is a flame rather than a bulb.
pub const CANDLE_BELOW_K: f32 = 2200.0;

/// Above this a light is open shade rather than daylight.
pub const SHADE_ABOVE_K: f32 = 7000.0;

/// Above this a light is overcast rather than direct sun.
pub const CLOUDY_ABOVE_K: f32 = 6200.0;

/// How far off the locus a light must sit before it is a discharge source rather than a
/// black body.
///
/// A tenth of [`Illuminant::SATURATED_ABOVE`]-ish territory: fluorescent tubes and cheap
/// white LEDs sit between about 0.008 and 0.030 above the locus in `u'v'`, which is visible
/// as a green cast and is nowhere near a stage wash.
pub const DISCHARGE_DUV: f32 = 0.008;

/// One candidate answer, before anything has scored it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hypothesis {
    /// The chromaticity this generator thinks the light has.
    pub uv: [f32; 2],
    /// Which generator proposed it.
    pub source: HypothesisSource,
    /// How much this generator's own evidence is worth here, `0..1`.
    ///
    /// A **prior**, not a score. It says how much this generator deserves to be listened to
    /// on this photograph before anybody has checked whether its answer makes skin plausible
    /// - which is what `wb::score` does next.
    pub prior: f32,
    /// Where this generator was looking, when it was looking somewhere in particular.
    pub region: Option<CropRect>,
}

/// The camera's own as-shot white balance, if EXIF recorded one.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AsShot {
    /// Colour temperature in kelvin. Zero when unknown.
    pub temperature_k: f32,
    /// Tint. Zero when unknown.
    pub tint: f32,
    /// True when the camera reported the flash fired.
    pub flash_fired: bool,
}

impl AsShot {
    /// True when the camera recorded a usable temperature.
    #[must_use]
    pub fn is_known(&self) -> bool {
        self.temperature_k >= 1500.0 && self.temperature_k <= 30_000.0
    }

    /// The as-shot value as a chromaticity, or D65 when the camera said nothing.
    #[must_use]
    pub fn uv(&self) -> [f32; 2] {
        if self.is_known() {
            cct_to_uv(self.temperature_k)
        } else {
            D65_UV
        }
    }
}

/// Build every hypothesis this frame supports, in a fixed order.
///
/// The order is the order of [`HypothesisSource::ALL`] and it is fixed rather than
/// convenient: the scorer breaks ties by position, and a tie broken by whichever generator
/// happened to run first is a white balance that changes when somebody reorders a match arm.
#[must_use]
pub fn generate(
    stats: &FrameStats,
    patches: &[NeutralPatch],
    as_shot: AsShot,
    learned: Option<[f32; 2]>,
    neutral_prior: f32,
    skin_hint: Option<[f32; 2]>,
) -> Vec<Hypothesis> {
    let mut out = Vec::with_capacity(HypothesisSource::COUNT);

    out.push(Hypothesis {
        uv: as_shot.uv(),
        source: HypothesisSource::CameraAsShot,
        // Low but never zero. The body's auto white balance is a real estimate made with
        // information we do not have - the metering pattern, the lens, sometimes a colour
        // sensor - and it is the floor this phase can never do worse than.
        prior: if as_shot.is_known() { 0.35 } else { 0.10 },
        region: None,
    });

    out.push(Hypothesis {
        uv: stats.grey_world_uv(),
        source: HypothesisSource::GreyWorld,
        // Falls away as the frame becomes more strongly coloured, because that is exactly
        // when the grey-world assumption stops being true. A frame with a 0.08 cast is a
        // frame that is mostly one colour, and grey-world's answer there is a description of
        // the subject rather than of the light.
        prior: (1.0 - stats.cast / 0.09).clamp(0.05, 0.55),
        region: None,
    });

    out.push(Hypothesis {
        uv: stats.white_patch_uv(),
        source: HypothesisSource::WhitePatch,
        // Worthless when the frame has almost nothing bright in it, which is most of a
        // reception.
        prior: (stats.linear_luma / 0.12).clamp(0.05, 0.45),
        region: None,
    });

    if let Some(uv) = learned {
        out.push(Hypothesis {
            uv,
            source: HypothesisSource::Learned,
            // **Deliberately low in this build.** The shipped `white_balance` head is
            // untrained, so its output over a real photograph is a random projection of a
            // thumbnail. `analyse` withholds this hypothesis entirely unless the head is
            // trained; the prior here is what it will be worth when it is, and it is stated
            // rather than left as a magic number in a future diff.
            prior: 0.60,
            region: None,
        });
    }

    for patch in patches {
        out.push(Hypothesis {
            uv: patch.uv,
            source: HypothesisSource::KnownNeutral,
            // The strongest evidence available, scaled by how neutral-like the patch is and
            // by how much this scene expects a neutral to exist at all. A dance floor's
            // brightest uniform region is an uplighter on a wall, and `neutral_prior` is the
            // product decision that says so.
            prior: (patch.strength() * neutral_prior.clamp(0.0, 1.0) * 1.4).clamp(0.0, 1.0),
            region: Some(patch.area),
        });
    }

    if let Some(uv) = skin_hint {
        out.push(Hypothesis {
            uv,
            source: HypothesisSource::Skin,
            // The chromaticity that would put this frame's skin exactly on its locus. It is
            // a *candidate*, not the answer: proposing it and then scoring it the same way
            // as every other hypothesis is what keeps the neutral evidence able to overrule
            // it, which matters because a locus is itself an estimate.
            prior: 0.55,
            region: None,
        });
    }

    if as_shot.flash_fired {
        out.push(Hypothesis {
            // A shoe-mount or built-in flash is a xenon tube close to daylight. 5600 K is
            // the manufacturers' own nominal figure and the one every gel is cut against.
            uv: cct_to_uv(5600.0),
            source: HypothesisSource::Flash,
            prior: 0.70,
            region: None,
        });
    }

    out
}

/// How far a chromaticity sits from the black-body and daylight loci.
///
/// **Not the distance from D65.** See this module's header: a tungsten bulb is far from
/// white and is not a creative choice, and a purple uplighter is near a plausible temperature
/// and is entirely one. This is the number `Illuminant::chroma` carries and
/// `Illuminant::SATURATED_ABOVE` is compared against.
#[must_use]
pub fn chroma_of(uv: [f32; 2]) -> f32 {
    duv(uv).abs()
}

/// What kind of light a chromaticity is.
///
/// The scene's attributes are consulted rather than derived: phase 07 already decided whether
/// a photograph is on a stage or under tungsten, and re-deriving it here from the pixels
/// would be a second answer to a question a frozen service owns.
#[must_use]
pub fn classify(uv: [f32; 2], attrs: AttrFlags, from_flash: bool) -> IlluminantKind {
    if from_flash {
        return IlluminantKind::Flash;
    }
    let chroma = chroma_of(uv);
    let cct = cct_from_uv(uv);

    // A saturated light in a scene phase 07 marked as a stage is the unambiguous case, and
    // it is checked first because everything below would call a purple wash "tungsten" or
    // "shade" depending on which way it happened to lean.
    if chroma >= Illuminant::SATURATED_ABOVE {
        if attrs.contains(AttrFlags::STAGE) || chroma >= Illuminant::SATURATED_ABOVE * 2.0 {
            return IlluminantKind::Stage;
        }
        // Saturated, but not on a stage and not extreme. A discharge source with a bad
        // spectrum is the honest answer; calling it `Stage` would tell the solve to preserve
        // a green cast nobody chose.
        return IlluminantKind::Fluorescent;
    }

    if cct <= CANDLE_BELOW_K {
        return IlluminantKind::Candle;
    }
    if chroma >= DISCHARGE_DUV {
        // Off the locus but not saturated. Above the locus is green, which is a tube; below
        // it is magenta, which at these temperatures is what a cheap white LED does.
        return if duv(uv) > 0.0 {
            IlluminantKind::Fluorescent
        } else {
            IlluminantKind::Led
        };
    }
    if cct <= TUNGSTEN_BELOW_K {
        return IlluminantKind::Tungsten;
    }
    if cct >= SHADE_ABOVE_K {
        return IlluminantKind::Shade;
    }
    if cct >= CLOUDY_ABOVE_K {
        return IlluminantKind::Cloudy;
    }
    IlluminantKind::Daylight
}

/// How many `u'v'` units one tint unit is.
///
/// Chosen so that a fluorescent tube's typical +0.02 in `u'v'` lands near +4 tint - the unit
/// section 10.1's gate is written in - and so that the slider's +/-150 range spans everything
/// a wedding can contain.
///
/// One constant rather than a literal at each site: [`describe`] divides by it, [`uv_of`]
/// multiplies by it and `store::codec` reads it back, and a tint that means one thing in the
/// solve and another in the panel is a bug nobody would see until a photographer compared two
/// screens.
pub const TINT_PER_UV: f32 = 0.004_5;

/// The chromaticity a kelvin-and-tint pair names. The inverse of [`describe`].
///
/// Needed because the correction in [`super::solve::correct`] walks *between* two lights, and
/// one of its endpoints - the camera's own as-shot value - arrives as the pair a photographer
/// sets rather than as a chromaticity. Interpolating the pair directly is what invariant 8
/// forbids: a distance in kelvin is not a distance in colour, and on an off-locus light it is
/// not even in the right direction.
///
/// The offset is taken along `v'`, which is the axis [`duv`] takes its sign from, so a
/// chromaticity built here reads back through [`describe`] with the tint it was given.
#[must_use]
pub fn uv_of(kelvin: f32, tint: f32) -> [f32; 2] {
    let locus = cct_to_uv(kelvin);
    [locus[0], locus[1] + tint * TINT_PER_UV]
}

/// Turn a chromaticity into the recorded shape.
#[must_use]
pub fn describe(
    uv: [f32; 2],
    source: HypothesisSource,
    weight: f32,
    region: Option<CropRect>,
    attrs: AttrFlags,
) -> Illuminant {
    let from_flash = source == HypothesisSource::Flash;
    let chroma = chroma_of(uv);
    Illuminant {
        kind: classify(uv, attrs, from_flash),
        cct_k: cct_from_uv(uv),
        // The recipe's tint units. See `TINT_PER_UV` for the divisor's reason.
        tint: (duv(uv) / TINT_PER_UV).clamp(-150.0, 150.0),
        uv,
        weight: weight.clamp(0.0, 1.0),
        region,
        source,
        chroma,
    }
}

/// The frame's own reading of the light in the room, whichever hypothesis won.
///
/// ## Why this exists
///
/// [`super::solve::choose`] picks the chromaticity that best explains the *subject*, and
/// that is the right answer for the correction. It is the wrong witness for "what kind of
/// light is this room lit by", because the winner changes with the evidence available on
/// that frame while the room does not.
///
/// The purple dance floor is where that bites. With no skin locus yet, white-patch wins and
/// reads the wash at a chroma of about 0.063 - above [`Illuminant::SATURATED_ABOVE`], so
/// [`classify`] calls it `Stage` and the preserve-mood policy engages. Once the wedding has
/// accumulated loci, the skin-anchored hypothesis wins the same frame at about 0.041, below
/// the threshold, so the identical room classifies as `Led` and the policy silently does not
/// engage. The cast survives either way - the correction is anchored on skin and barely
/// moves - but on the second path nothing *says* the light was kept on purpose, so the panel
/// cannot tell the photographer and [`aura_core::contract::tone::ToneOutline::coloured_light`]
/// under-counts. That was condition C5 in the phase 15 exit report.
///
/// ## What it reads
///
/// Only the two generators that measure the light *falling on the frame*: grey-world and
/// white-patch. The skin and known-neutral hypotheses are anchored to a subject, so their
/// chromaticity is a statement about that subject and not about the room; the camera's
/// as-shot value is the body's own guess rather than a measurement; and the learned head is
/// untrained in this build and never generated. The most saturated of the two is taken,
/// because a wash that one of them reads as strongly coloured is a wash.
///
/// ## The red mandap, and why this does not fire on it
///
/// Both generators this reads are confounded by a strongly coloured *surface* - that is the
/// failure mode listed in this module's own table - and a red canopy over a ceremony is
/// exactly such a surface. Nothing new guards against it here, because [`classify`] already
/// does: a saturated reading is `Stage` only when phase 07 marked the scene `STAGE`, or when
/// the chroma is twice the threshold. A mandap is neither, so it classifies as a warm light
/// and [`Illuminant::should_preserve`] stays false. The scene attribute is doing the work,
/// which is the same division of labour [`classify`] documents: phase 07 owns whether this
/// is a stage, and this module owns what colour the light is.
#[must_use]
pub fn ambient(hypotheses: &[Hypothesis], attrs: AttrFlags) -> Option<Illuminant> {
    hypotheses
        .iter()
        .filter(|hypothesis| {
            matches!(
                hypothesis.source,
                HypothesisSource::GreyWorld | HypothesisSource::WhitePatch
            )
        })
        .map(|hypothesis| describe(hypothesis.uv, hypothesis.source, 0.0, None, attrs))
        // `total_cmp` rather than `partial_cmp`: a NaN chroma would silently drop the
        // candidate from a `partial_cmp` sort and take the coloured-light note with it.
        .max_by(|left, right| left.chroma.total_cmp(&right.chroma))
}

/// What [`detect_mixed`] found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MixedLight {
    /// True when two groups of cells disagree by more than [`MIXED_SEPARATION`].
    pub mixed: bool,
    /// The two group centres, warmer first. Empty when the frame is not mixed.
    pub groups: Vec<(f32, [f32; 2], CropRect)>,
    /// How far apart the two groups are, in `u'v'`. Zero when not mixed.
    pub separation: f32,
}

/// Does more than one colour of light fall on this frame?
///
/// Two-means over the voting cells, with a **deterministic** initialisation: the two seeds
/// are the cells furthest apart along the chromaticity axis with the greater spread, found by
/// a fixed scan. Random seeding would make the answer depend on a generator, and a
/// mixed-light flag that flickers between runs is a phase 18 queue that flickers with it.
///
/// Four iterations, which is enough for a two-cluster problem in two dimensions to have
/// stopped moving; a convergence test would cost a comparison per iteration to save at most
/// one of four.
#[must_use]
pub fn detect_mixed(stats: &FrameStats) -> MixedLight {
    const ITERATIONS: usize = 4;

    // Each voting cell keeps its index into the full grid, so the rectangle a group covers
    // is read off directly rather than reconstructed by counting.
    let voting: Vec<(usize, &Cell)> = stats
        .cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.is_usable())
        .collect();
    if voting.len() < 8 {
        return MixedLight::default();
    }

    // Seeds: the extremes along whichever axis is more spread out. A fixed rule, so the
    // answer is a function of the pixels alone.
    let (mut min_u, mut max_u) = (f32::MAX, f32::MIN);
    let (mut min_v, mut max_v) = (f32::MAX, f32::MIN);
    for (_, cell) in &voting {
        min_u = min_u.min(cell.uv[0]);
        max_u = max_u.max(cell.uv[0]);
        min_v = min_v.min(cell.uv[1]);
        max_v = max_v.max(cell.uv[1]);
    }
    let axis = usize::from(max_v - min_v > max_u - min_u);
    let mut seeds = [[0.0f32; 2]; 2];
    let (mut low, mut high) = (f32::MAX, f32::MIN);
    for (_, cell) in &voting {
        let value = cell.uv.get(axis).copied().unwrap_or(0.0);
        if value < low {
            low = value;
            seeds[0] = cell.uv;
        }
        if value > high {
            high = value;
            seeds[1] = cell.uv;
        }
    }

    let mut assignment = vec![0usize; voting.len()];
    for _ in 0..ITERATIONS {
        for (slot, (_, cell)) in assignment.iter_mut().zip(voting.iter()) {
            let d0 = uv_distance(cell.uv, seeds[0]);
            let d1 = uv_distance(cell.uv, seeds[1]);
            *slot = usize::from(d1 < d0);
        }
        let mut sums = [[0.0f64; 2]; 2];
        let mut counts = [0u32; 2];
        for (group, (_, cell)) in assignment.iter().zip(voting.iter()) {
            let index = (*group).min(1);
            if let (Some(sum), Some(count)) = (sums.get_mut(index), counts.get_mut(index)) {
                sum[0] += f64::from(cell.uv[0]);
                sum[1] += f64::from(cell.uv[1]);
                *count += 1;
            }
        }
        for index in 0..2 {
            let (Some(sum), Some(count)) = (sums.get(index), counts.get(index)) else {
                continue;
            };
            if *count == 0 {
                continue;
            }
            if let Some(seed) = seeds.get_mut(index) {
                *seed = [
                    (sum[0] / f64::from(*count)) as f32,
                    (sum[1] / f64::from(*count)) as f32,
                ];
            }
        }
    }

    let mut counts = [0u32; 2];
    for group in &assignment {
        if let Some(slot) = counts.get_mut((*group).min(1)) {
            *slot += 1;
        }
    }
    let total = voting.len() as f32;
    let separation = uv_distance(seeds[0], seeds[1]);
    let minority = counts[0].min(counts[1]) as f32 / total;
    if separation < MIXED_SEPARATION || minority < MIXED_MIN_SHARE {
        return MixedLight {
            mixed: false,
            groups: Vec::new(),
            separation,
        };
    }

    // Warmer first, so `groups[0]` is a stable thing to describe. Warmer means the lower
    // correlated colour temperature, which is what a photographer means by the word.
    let mut groups: Vec<(f32, [f32; 2], CropRect)> = (0..2)
        .filter_map(|index| {
            let count = counts.get(index).copied().unwrap_or(0);
            if count == 0 {
                return None;
            }
            let seed = seeds.get(index).copied()?;
            Some((
                count as f32 / total,
                seed,
                bounds_of(&assignment, &voting, index),
            ))
        })
        .collect();
    groups.sort_by(|a, b| cct_from_uv(a.1).total_cmp(&cct_from_uv(b.1)));

    MixedLight {
        mixed: true,
        groups,
        separation,
    }
}

/// The frame rectangle covering one group's cells.
fn bounds_of(assignment: &[usize], voting: &[(usize, &Cell)], group: usize) -> CropRect {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (GRID, GRID, 0usize, 0usize);
    for (slot, (index, _)) in assignment.iter().zip(voting.iter()) {
        if *slot != group {
            continue;
        }
        let row = index / GRID;
        let column = index % GRID;
        min_x = min_x.min(column);
        min_y = min_y.min(row);
        max_x = max_x.max(column);
        max_y = max_y.max(row);
    }
    if min_x == GRID {
        return CropRect::FULL;
    }
    CropRect {
        x: min_x as f32 / GRID as f32,
        y: min_y as f32 / GRID as f32,
        w: (max_x + 1 - min_x) as f32 / GRID as f32,
        h: (max_y + 1 - min_y) as f32 / GRID as f32,
    }
    .clamped()
}

/// Which of a frame's illuminants falls on the people.
///
/// Section 6.2: "choose the illuminant governing the subject". The answer is the illuminant
/// whose chromaticity is closest to what the **cells the faces sit in** actually look like,
/// weighted by each face's area - a large face in a doorway outvotes three small ones under
/// a lamp, which is right, because the large one is what the photograph is of.
///
/// A flash illuminant short-circuits it. `IlluminantKind::is_subject_local` is the property
/// that says why: a flash lights the face it is pointed at whatever the room is doing, and
/// this is the one case where "which light is on the subject" is not a judgement call.
#[must_use]
pub fn dominant_on_subject(
    illuminants: &[Illuminant],
    faces: &[FaceRef],
    stats: &FrameStats,
) -> Option<usize> {
    if illuminants.is_empty() {
        return None;
    }
    if let Some(index) = illuminants
        .iter()
        .position(|light| light.kind.is_subject_local())
    {
        return Some(index);
    }
    if faces.is_empty() {
        return None;
    }

    let mut sums = [0.0f64; 2];
    let mut weight_total = 0.0f64;
    for face in faces {
        let weight = f64::from(face.area_frac.max(1e-4) * face.quality.max(0.1));
        let Some(uv) = cells_under(&face.bbox, stats) else {
            continue;
        };
        sums[0] += f64::from(uv[0]) * weight;
        sums[1] += f64::from(uv[1]) * weight;
        weight_total += weight;
    }
    if weight_total <= 0.0 {
        return None;
    }
    let subject_uv = [
        (sums[0] / weight_total) as f32,
        (sums[1] / weight_total) as f32,
    ];

    let mut best: Option<(usize, f32)> = None;
    for (index, light) in illuminants.iter().enumerate() {
        let distance = uv_distance(light.uv, subject_uv);
        if best.is_none_or(|(_, current)| distance < current) {
            best = Some((index, distance));
        }
    }
    best.map(|(index, _)| index)
}

/// The mean chromaticity of the grid cells a rectangle overlaps.
fn cells_under(area: &CropRect, stats: &FrameStats) -> Option<[f32; 2]> {
    let x0 = ((area.x * GRID as f32) as usize).min(GRID - 1);
    let y0 = ((area.y * GRID as f32) as usize).min(GRID - 1);
    let x1 = (((area.x + area.w) * GRID as f32).ceil() as usize).min(GRID);
    let y1 = (((area.y + area.h) * GRID as f32).ceil() as usize).min(GRID);
    let mut sums = [0.0f64; 2];
    let mut count = 0u32;
    for row in y0..y1.max(y0 + 1) {
        for column in x0..x1.max(x0 + 1) {
            let Some(cell) = stats.cells.get(row * GRID + column) else {
                continue;
            };
            if !cell.is_usable() {
                continue;
            }
            sums[0] += f64::from(cell.uv[0]);
            sums[1] += f64::from(cell.uv[1]);
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    Some([
        (sums[0] / f64::from(count)) as f32,
        (sums[1] / f64::from(count)) as f32,
    ])
}

/// The chromaticity that would put an observed skin patch exactly on its locus.
///
/// The [`HypothesisSource::Skin`] generator. Given what the skin looks like in the file and
/// where that person's skin is known to sit, the light must be the ratio between them - which
/// in a von Kries model is a per-channel divide, and this is that divide read backwards.
#[must_use]
pub fn skin_implied_light(observed: [f32; 3], locus_uv: [f32; 2]) -> [f32; 2] {
    let reflectance = uv_to_linear_srgb(locus_uv);
    let mut light = [0.0f32; 3];
    for ((slot, seen), refl) in light
        .iter_mut()
        .zip(observed.iter())
        .zip(reflectance.iter())
    {
        *slot = seen / refl.max(1e-4);
    }
    aura_raw::colour::illuminant::linear_srgb_to_uv(light)
}
