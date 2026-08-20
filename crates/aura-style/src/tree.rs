//! The regression, the shrinkage and the archive cap: turning a pile of pairs into a tree.
//!
//! PHASE-17 section 6.2:
//!
//! > Per bucket, fit a small ridge-regularised model on features (subject luma, CCT, dynamic
//! > range, ISO, flash, scene, skin group) predicting each delta. James-Stein style shrinkage
//! > toward the parent group and global delta, weighted by sample count. Robust fitting (Huber
//! > loss) so one wildly different wedding cannot skew the profile.
//!
//! ## What is fitted, and what is kept
//!
//! A ridge regression over [`crate::bucket::FEATURES`] columns is fitted for every one of
//! [`TARGETS`] deltas, at every level of the tree. **What is stored is the intercept.** The
//! slopes are used and then discarded, and that is a decision rather than an oversight.
//!
//! The slopes' real job is *decontamination*. A photographer's dance-floor pairs are all shot
//! at high ISO, and if they lift shadows on noisy frames, an unconditioned mean would attribute
//! that lift to "dance floor" - and then apply it to the one dance-floor frame they shot at ISO
//! 400. Ridge with an ISO column removes the confound from the intercept, which is exactly what
//! the intercept is supposed to mean: **this photographer's lean for this kind of frame in this
//! kind of light, at ordinary values of everything else.**
//!
//! Keeping the slopes and applying them would be extrapolation nobody checked. A bucket with
//! eleven samples spanning ISO 1600 to 4000 has a slope that is not identified outside that
//! range, and the frame it would be applied to is the ISO 400 one - the only frame in the whole
//! wedding where the answer is a guess dressed as a measurement. Discarding them costs a
//! second-order refinement and removes a whole class of confident wrongness.
//!
//! It also makes inference free: [`crate::infer::advise`] is three map lookups and an addition,
//! which is how section 11's 2 ms budget is met with room to spare.
//!
//! ## The feature section 6.2 names that is deliberately absent
//!
//! There is no skin group. `crate::bucket`'s header has the argument in full; the short version
//! is that phase 06's rule - never infer anything about a person - is not weakened for a feature
//! vector, and a profile conditioned on a guess about somebody's skin is a product that edits
//! two people differently because of what it decided they were.
//!
//! ## Two robustness mechanisms, because section 10.1 asks two different questions
//!
//! **Huber, via iteratively reweighted least squares**, bounds the influence of one *pair*. It
//! is what stops a single mis-matched original from dragging a bucket.
//!
//! **The archive cap** bounds the influence of one *wedding*. Section 10.1's gate is "one
//! outlier wedding cannot shift the global profile beyond a bounded amount", and Huber does not
//! make that claim: four hundred consistent pairs from a single very cold, very flat archive are
//! not outliers to a robust loss, they are a mode. [`MAX_ARCHIVE_INFLUENCE`] is the second
//! mechanism and it is the only place in this phase where a photographer's own data is
//! deliberately not fully believed. ADR-0035 decision 6.

use std::collections::BTreeMap;

use aura_core::contract::colour::{HslBand, HslShift, ToneCurve};
use aura_core::contract::style::{
    BucketModel, CurveShift, SceneGroup, SkinBias, StyleBucket, StyleDelta,
};

use crate::bucket::{Features, FEATURES};

/// How many deltas are fitted per level.
///
/// Forty-one: ten scalars, five curve anchors, twenty-four HSL numbers and two skin leans. They
/// share one design matrix, so the 7x7 inverse is computed once per level and reused - which is
/// what makes fitting forty-one regressions cost barely more than fitting one.
pub const TARGETS: usize = 41;

/// The ridge penalty.
///
/// A tenth, on centred and scaled columns. Small: the columns are already standardised by
/// `crate::bucket` so the penalty means the same thing for each of them, and the job here is to
/// keep a nearly-collinear design from producing an enormous slope rather than to shrink toward
/// zero - the shrinkage that matters is the hierarchical kind below, and doing it twice would
/// flatten every profile.
pub const RIDGE: f32 = 0.1;

/// The prior strength for a bucket's and a group's shrinkage toward its parent.
///
/// Twelve. It is a **product** decision rather than a fitted one: it says how many frames of
/// evidence it takes before AURA half-believes that this photographer treats one kind of light
/// differently. At twelve, a bucket with twelve pairs contributes half of its own answer, a
/// bucket with four contributes a quarter, and a bucket with four hundred contributes
/// ninety-seven percent.
///
/// It also sets, indirectly, the smallest bucket that can speak at all: the shrink factor is the
/// bucket's confidence, and `aura_core::contract::style::APPLY_ABOVE` is 0.35, so seven pairs is
/// the floor. That relationship is asserted by a test rather than left to arithmetic somebody
/// does in their head.
pub const PRIOR_STRENGTH: f32 = 12.0;

/// The prior strength for the global delta's shrinkage toward zero.
///
/// Forty, so `USABLE_PAIRS` of three hundred gives 0.88. Much larger than
/// [`PRIOR_STRENGTH`] because the global delta applies to **every photograph in every
/// wedding**: the cost of being wrong about it is a whole gallery, where the cost of being wrong
/// about one bucket is the frames in that bucket.
pub const GLOBAL_PRIOR: f32 = 40.0;

/// The largest share of the fitting weight any one archive may hold.
///
/// Thirty-five percent, and the cap is raised to `1 / archives` when there are fewer than three,
/// so a photographer training on one wedding is not fighting a cap that can do nothing. See this
/// module's header for why Huber alone does not answer section 10.1's robustness gate.
pub const MAX_ARCHIVE_INFLUENCE: f32 = 0.35;

/// The Huber threshold, in units of the residual's own median absolute deviation.
///
/// 1.345, the textbook value for 95 % efficiency against a Gaussian. Named rather than inlined
/// because somebody will eventually want to know whether it was chosen or copied, and the answer
/// is that it was copied on purpose.
pub const HUBER_K: f32 = 1.345;

/// How many reweighting passes the robust fit takes.
///
/// Two. The first pass is ordinary least squares and the second is the reweighted one; a third
/// moves the answer by less than the numbers are stored to.
pub const HUBER_PASSES: usize = 2;

/// What fraction of the pairs is held out to measure the fit.
///
/// A fifth, and the split is **deterministic**: every fifth pair in hash order. Invariant 4 -
/// a profile whose measured accuracy depended on a random split would be a profile whose
/// headline number changed on every re-train with no change to the data.
pub const HOLDOUT_EVERY: usize = 5;

/// One pair, as the fitter sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    /// Which leaf, and the design row.
    pub features: Features,
    /// The deltas this pair implies: what the photographer did minus what the baseline said.
    pub targets: [f32; TARGETS],
    /// True when this pair carries real HSL evidence - which only an XMP pair does.
    pub hsl_known: bool,
    /// True when this pair had pixels in the skin band to measure a skin lean from.
    pub skin_known: bool,
    /// Which wedding it came from, for the archive cap.
    pub archive: String,
    /// True when this pair is held out of the fit and used to measure it.
    pub held_out: bool,
}

impl Sample {
    /// The targets a delta occupies, in this module's fixed layout.
    ///
    /// Positional, and the positions are named by [`Target`] rather than written as integers at
    /// the call sites - a forty-one element array indexed by bare numbers is an array somebody
    /// eventually reorders.
    #[must_use]
    pub fn new(features: Features, targets: [f32; TARGETS], archive: impl Into<String>) -> Self {
        Self {
            features,
            targets,
            hsl_known: false,
            skin_known: false,
            archive: archive.into(),
            held_out: false,
        }
    }
}

/// Where each delta sits in the target vector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Exposure in stops.
    Exposure,
    /// Colour temperature in kelvin.
    Temperature,
    /// Tint.
    Tint,
    /// Contrast.
    Contrast,
    /// Highlights.
    Highlights,
    /// Shadows.
    Shadows,
    /// Whites.
    Whites,
    /// Blacks.
    Blacks,
    /// Vibrance.
    Vibrance,
    /// Saturation.
    Saturation,
}

impl Target {
    /// Where the ten scalars start.
    pub const SCALARS: usize = 0;
    /// Where the five curve anchors start.
    pub const CURVE: usize = 10;
    /// Where the twenty-four HSL numbers start.
    pub const HSL: usize = 15;
    /// Where the two skin leans start.
    pub const SKIN: usize = 39;

    /// This target's index.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Build the target vector from what a photographer did and what the baseline said.
///
/// Both sides are absolute; the difference is the style. `their_curve` and `base_curve` are
/// compared **at the five fixed anchors** rather than point by point, because two curves with
/// different point counts have no point-by-point correspondence and phase 16's fitter produces
/// between four and eight of them.
#[must_use]
pub fn targets_of(
    their: &crate::extract::TheirParams,
    baseline: &crate::extract::TheirParams,
    skin: Option<SkinBias>,
) -> [f32; TARGETS] {
    let mut out = [0.0_f32; TARGETS];
    let scalars = [
        their.exposure - baseline.exposure,
        their.temperature_k - baseline.temperature_k,
        their.tint - baseline.tint,
        their.contrast - baseline.contrast,
        their.highlights - baseline.highlights,
        their.shadows - baseline.shadows,
        their.whites - baseline.whites,
        their.blacks - baseline.blacks,
        their.vibrance - baseline.vibrance,
        their.saturation - baseline.saturation,
    ];
    for (offset, value) in scalars.iter().enumerate() {
        if let Some(slot) = out.get_mut(Target::SCALARS + offset) {
            *slot = *value;
        }
    }
    for (offset, anchor) in CurveShift::ANCHORS.iter().enumerate() {
        if let Some(slot) = out.get_mut(Target::CURVE + offset) {
            *slot = curve_at(&their.curve, *anchor) - curve_at(&baseline.curve, *anchor);
        }
    }
    for (band_index, band) in HslBand::ALL.into_iter().enumerate() {
        let (theirs, base) = (their.hsl.get(band), baseline.hsl.get(band));
        let triple = [theirs.h - base.h, theirs.s - base.s, theirs.l - base.l];
        for (axis, value) in triple.iter().enumerate() {
            if let Some(slot) = out.get_mut(Target::HSL + band_index * 3 + axis) {
                *slot = *value;
            }
        }
    }
    if let Some(bias) = skin {
        if let Some(slot) = out.get_mut(Target::SKIN) {
            *slot = bias.warmth_deg;
        }
        if let Some(slot) = out.get_mut(Target::SKIN + 1) {
            *slot = bias.chroma;
        }
    }
    out
}

/// The output value of a curve at one input level.
///
/// Linear between control points rather than through the renderer's Fritsch-Carlson
/// interpolation, and deliberately: what is wanted here is where the photographer *put the
/// control point*, and a monotone spline's value between two points is a property of the
/// interpolator rather than of the decision. Phase 16 owns the interpolation and this is not it.
#[must_use]
pub fn curve_at(curve: &ToneCurve, level: u16) -> f32 {
    let points = curve.points();
    let level_f = f32::from(level);
    let mut previous = points.first().copied();
    for point in points {
        if point.x >= level {
            let Some(before) = previous else {
                return f32::from(point.y);
            };
            if point.x == before.x {
                return f32::from(point.y);
            }
            let span = f32::from(point.x) - f32::from(before.x);
            let t = ((level_f - f32::from(before.x)) / span).clamp(0.0, 1.0);
            return f32::from(before.y) + (f32::from(point.y) - f32::from(before.y)) * t;
        }
        previous = Some(*point);
    }
    points.last().map_or(level_f, |point| f32::from(point.y))
}

/// The tree one training run produced.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tree {
    /// The lean that applies to every photograph.
    pub global: StyleDelta,
    /// Per scene group, on top of the global.
    pub groups: BTreeMap<SceneGroup, StyleDelta>,
    /// Per leaf, on top of its group.
    pub buckets: BTreeMap<StyleBucket, BucketModel>,
    /// How many archives contributed, for the report.
    pub archives: usize,
    /// True when at least one archive's weight was capped.
    pub archive_capped: bool,
}

/// Fit a whole tree from a pile of accepted pairs.
///
/// Three passes down the hierarchy, each fitted on what the level above left behind:
///
/// 1. **Global.** Every training sample, against its own target.
/// 2. **Group.** Each scene group's samples, against `target - global`.
/// 3. **Bucket.** Each leaf's samples, against `target - global - group`.
///
/// So the levels compose by addition, which is exactly how
/// [`aura_core::contract::style::StyleProfile::resolve`] reads them back. A design where a
/// bucket *replaced* its parent would have been simpler to fit and would produce a visible step
/// at every boundary between a taught bucket and an untaught one.
#[must_use]
pub fn fit(samples: &[Sample]) -> Tree {
    let training: Vec<&Sample> = samples.iter().filter(|sample| !sample.held_out).collect();
    if training.is_empty() {
        return Tree::default();
    }
    let (weights, capped, archives) = archive_weights(&training);

    // 1. The global lean.
    let global_raw = solve_level(&training, &weights, |sample| sample.targets);
    let global_shrink = shrink_factor(training.len(), GLOBAL_PRIOR);
    let global = delta_of(&global_raw, global_shrink, &training).scaled(global_shrink);

    // 2. Each scene group, on what the global left behind.
    let mut groups: BTreeMap<SceneGroup, StyleDelta> = BTreeMap::new();
    let mut group_raws: BTreeMap<SceneGroup, [f32; TARGETS]> = BTreeMap::new();
    for group in SceneGroup::ALL {
        let members: Vec<&Sample> = training
            .iter()
            .copied()
            .filter(|sample| sample.features.bucket.group == group)
            .collect();
        if members.is_empty() {
            continue;
        }
        let member_weights = subset_weights(&members, &training, &weights);
        let raw = solve_level(&members, &member_weights, |sample| {
            residual(sample.targets, &global_raw, global_shrink)
        });
        let factor = shrink_factor(members.len(), PRIOR_STRENGTH);
        let delta = delta_of(&raw, factor, &members).scaled(factor);
        if !delta.is_neutral() || delta.confidence > 0.0 {
            group_raws.insert(group, scale_targets(&raw, factor));
            groups.insert(group, delta);
        }
    }

    // 3. Each leaf, on what the global and its group left behind.
    let mut buckets: BTreeMap<StyleBucket, BucketModel> = BTreeMap::new();
    let mut leaves: BTreeMap<StyleBucket, Vec<&Sample>> = BTreeMap::new();
    for sample in &training {
        leaves
            .entry(sample.features.bucket)
            .or_default()
            .push(sample);
    }
    for (bucket, members) in leaves {
        let parent = group_raws
            .get(&bucket.group)
            .copied()
            .unwrap_or([0.0; TARGETS]);
        let member_weights = subset_weights(&members, &training, &weights);
        let raw = solve_level(&members, &member_weights, |sample| {
            let after_global = residual(sample.targets, &global_raw, global_shrink);
            residual(after_global, &parent, 1.0)
        });
        let factor = shrink_factor(members.len(), PRIOR_STRENGTH);
        let delta = delta_of(&raw, factor, &members).scaled(factor);
        buckets.insert(
            bucket,
            BucketModel {
                bucket,
                delta,
                samples: u32::try_from(members.len()).unwrap_or(u32::MAX),
                held_out: 0,
                match_de00: None,
                shrink: factor,
            },
        );
    }

    Tree {
        global,
        groups,
        buckets,
        archives,
        archive_capped: capped,
    }
}

/// The most one archive's own lean can move a delta it disagrees with everybody else about.
///
/// Section 10.1's robustness gate as a number rather than a sentence: with one archive held to
/// [`MAX_ARCHIVE_INFLUENCE`] of the fitting weight, a weighted mean can move at most that
/// fraction of the distance between that archive's lean and everybody else's. Huber then
/// reduces it further and the bounds in `aura_core::contract::style` clamp it again, so this is
/// a ceiling rather than an estimate.
///
/// A function rather than a constant because it is a claim about a *distance*, and a constant
/// reading `0.35` with no units is a constant somebody eventually compares against a number of
/// stops.
#[must_use]
pub fn influence_bound(disagreement: f32) -> f32 {
    MAX_ARCHIVE_INFLUENCE * disagreement.abs()
}

/// The James-Stein shrinkage factor, `n / (n + prior)`.
///
/// Continuous rather than a cut-off, which is ADR-0035 decision 5's whole argument: a cut-off is
/// a cliff, and a photographer sees two visibly different looks either side of a boundary that
/// exists nowhere in their wedding.
#[must_use]
pub fn shrink_factor(samples: usize, prior: f32) -> f32 {
    let n = samples as f32;
    if n <= 0.0 {
        return 0.0;
    }
    (n / (n + prior)).clamp(0.0, 1.0)
}

/// Per-sample fitting weights, with each archive's share capped.
///
/// Returns the weights, whether any cap bound, and how many archives there were.
fn archive_weights(samples: &[&Sample]) -> (Vec<f32>, bool, usize) {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for sample in samples {
        *counts.entry(sample.archive.as_str()).or_insert(0) += 1;
    }
    let archives = counts.len().max(1);
    // With one or two archives a 35 % cap could only make the fit worse - a photographer
    // training on one wedding would be fighting a cap that has nothing to compare against - so
    // the cap is never tighter than an equal share.
    let cap = MAX_ARCHIVE_INFLUENCE.max(1.0 / archives as f32);
    let total = samples.len() as f32;

    let mut scale: BTreeMap<&str, f32> = BTreeMap::new();
    let mut capped = false;
    for (archive, count) in &counts {
        let mine = *count as f32;
        let share = mine / total;
        if share > cap + 1e-6 && cap < 1.0 {
            // The cap is on the share this archive holds **after** scaling, which is not the
            // same as scaling its share down to the cap: shrinking one archive's weight also
            // shrinks the total, so a naive `cap / share` leaves it well above the cap.
            // Solving `w * mine / (rest + w * mine) = cap` gives the factor below. Getting this
            // wrong is how a robustness guarantee reads correct and measures 48 % instead of
            // 35 %; it was wrong first, and `gate_5` in `tests/eval/style_eval.rs` caught it.
            let rest = (total - mine).max(0.0);
            let wanted = cap * rest / (1.0 - cap);
            scale.insert(archive, (wanted / mine).clamp(0.0, 1.0));
            capped = true;
        } else {
            scale.insert(archive, 1.0);
        }
    }
    let weights = samples
        .iter()
        .map(|sample| scale.get(sample.archive.as_str()).copied().unwrap_or(1.0))
        .collect();
    (weights, capped, archives)
}

fn subset_weights(subset: &[&Sample], all: &[&Sample], weights: &[f32]) -> Vec<f32> {
    // The archive scale is a property of the archive, so a subset's weights are the same numbers
    // looked up again rather than recomputed - recomputing them per bucket would let a bucket
    // dominated by one wedding escape the cap.
    subset
        .iter()
        .map(|sample| {
            all.iter()
                .position(|candidate| std::ptr::eq(*candidate, *sample))
                .and_then(|index| weights.get(index).copied())
                .unwrap_or(1.0)
        })
        .collect()
}

fn residual(targets: [f32; TARGETS], parent: &[f32; TARGETS], factor: f32) -> [f32; TARGETS] {
    let mut out = targets;
    for (index, slot) in out.iter_mut().enumerate() {
        *slot -= parent.get(index).copied().unwrap_or(0.0) * factor;
    }
    out
}

fn scale_targets(values: &[f32; TARGETS], factor: f32) -> [f32; TARGETS] {
    let mut out = *values;
    for slot in &mut out {
        *slot *= factor;
    }
    out
}

/// Fit every target at one level and return the intercepts.
fn solve_level<F>(samples: &[&Sample], weights: &[f32], target_of: F) -> [f32; TARGETS]
where
    F: Fn(&Sample) -> [f32; TARGETS],
{
    let rows: Vec<[f32; FEATURES]> = samples.iter().map(|sample| sample.features.row).collect();
    let targets: Vec<[f32; TARGETS]> = samples.iter().map(|sample| target_of(sample)).collect();

    let mut out = [0.0_f32; TARGETS];
    for index in 0..TARGETS {
        let mask: Vec<bool> = samples
            .iter()
            .map(|sample| target_applies(index, sample))
            .collect();
        let y: Vec<f32> = targets
            .iter()
            .map(|row| row.get(index).copied().unwrap_or(0.0))
            .collect();
        if let Some(slot) = out.get_mut(index) {
            *slot = intercept(&rows, &y, weights, &mask);
        }
    }
    out
}

/// True when one sample carries evidence about one target.
///
/// The HSL columns are only filled by an XMP pair and the skin columns only by a pair with skin
/// in it. A sample that carries no evidence about a target is **excluded from that target's fit**
/// rather than contributing a zero - contributing a zero would be asserting the photographer
/// makes no adjustment, which is a claim, and the absence of evidence is not one.
fn target_applies(index: usize, sample: &Sample) -> bool {
    if (Target::HSL..Target::SKIN).contains(&index) {
        sample.hsl_known
    } else if index >= Target::SKIN {
        sample.skin_known
    } else {
        true
    }
}

/// The ridge intercept for one target, Huber-reweighted.
///
/// The intercept is **not penalised**: the penalty is applied to the slopes only. Penalising it
/// would shrink the photographer's average lean toward zero, and the average lean is precisely
/// what this whole phase is trying to learn.
fn intercept(rows: &[[f32; FEATURES]], y: &[f32], weights: &[f32], mask: &[bool]) -> f32 {
    let live: Vec<usize> = (0..rows.len())
        .filter(|index| mask.get(*index).copied().unwrap_or(false))
        .collect();
    if live.is_empty() {
        return 0.0;
    }
    let mut w: Vec<f32> = live
        .iter()
        .map(|index| weights.get(*index).copied().unwrap_or(1.0))
        .collect();

    let mut beta = [0.0_f32; FEATURES];
    for pass in 0..HUBER_PASSES {
        beta = ridge(rows, y, &live, &w);
        if pass + 1 == HUBER_PASSES {
            break;
        }
        // Reweight by the Huber influence of each residual, scaled by the median absolute
        // deviation so the threshold is in units of this fit's own spread rather than in units
        // of whatever this target happens to be measured in.
        let residuals: Vec<f32> = live
            .iter()
            .map(|index| {
                let row = rows.get(*index).copied().unwrap_or([0.0; FEATURES]);
                let observed = y.get(*index).copied().unwrap_or(0.0);
                observed - dot(&row, &beta)
            })
            .collect();
        let scale = mad(&residuals).max(1e-4);
        for (slot, residual) in w.iter_mut().zip(residuals.iter()) {
            let standard = (residual / scale).abs();
            let huber = if standard <= HUBER_K {
                1.0
            } else {
                HUBER_K / standard
            };
            *slot *= huber;
        }
    }
    beta.first().copied().unwrap_or(0.0)
}

/// Weighted ridge regression: `(X'WX + lambda D) b = X'W y`, with `D` zero in the intercept.
fn ridge(rows: &[[f32; FEATURES]], y: &[f32], live: &[usize], weights: &[f32]) -> [f32; FEATURES] {
    let mut xtx = [[0.0_f64; FEATURES]; FEATURES];
    let mut xty = [0.0_f64; FEATURES];
    for (position, index) in live.iter().enumerate() {
        let row = rows.get(*index).copied().unwrap_or([0.0; FEATURES]);
        let observed = f64::from(y.get(*index).copied().unwrap_or(0.0));
        let weight = f64::from(weights.get(position).copied().unwrap_or(1.0));
        for i in 0..FEATURES {
            let ri = f64::from(row.get(i).copied().unwrap_or(0.0));
            if let Some(slot) = xty.get_mut(i) {
                *slot += weight * ri * observed;
            }
            for j in 0..FEATURES {
                let rj = f64::from(row.get(j).copied().unwrap_or(0.0));
                if let Some(cell) = xtx.get_mut(i).and_then(|line| line.get_mut(j)) {
                    *cell += weight * ri * rj;
                }
            }
        }
    }
    for i in 1..FEATURES {
        if let Some(cell) = xtx.get_mut(i).and_then(|line| line.get_mut(i)) {
            *cell += f64::from(RIDGE);
        }
    }
    // A tiny floor on the intercept's own diagonal, so a level with one sample is solvable at
    // all. Without it the matrix is singular whenever every row is identical, which is the
    // ordinary case for a bucket with one pair.
    if let Some(cell) = xtx.first_mut().and_then(|line| line.first_mut()) {
        *cell += 1e-6;
    }
    let solution = solve(&mut xtx, &mut xty);
    let mut beta = [0.0_f32; FEATURES];
    for (slot, value) in beta.iter_mut().zip(solution.iter()) {
        *slot = *value as f32;
    }
    beta
}

/// Gaussian elimination with partial pivoting.
///
/// Seven by seven. A decomposition would be tidier and this is called forty-one times per level
/// on a matrix small enough that the pivoting dominates; the ordering of operations is fixed, so
/// it is deterministic, which is what invariant 4 needs from it.
fn solve(a: &mut [[f64; FEATURES]; FEATURES], b: &mut [f64; FEATURES]) -> [f64; FEATURES] {
    for column in 0..FEATURES {
        let mut pivot = column;
        let mut best = 0.0_f64;
        for row in column..FEATURES {
            let value = a
                .get(row)
                .and_then(|line| line.get(column))
                .copied()
                .unwrap_or(0.0)
                .abs();
            if value > best {
                best = value;
                pivot = row;
            }
        }
        if best < 1e-12 {
            continue;
        }
        if pivot != column {
            a.swap(pivot, column);
            b.swap(pivot, column);
        }
        let head = a
            .get(column)
            .and_then(|line| line.get(column))
            .copied()
            .unwrap_or(1.0);
        for row in (column + 1)..FEATURES {
            let factor = a
                .get(row)
                .and_then(|line| line.get(column))
                .copied()
                .unwrap_or(0.0)
                / head;
            if factor.abs() < 1e-18 {
                continue;
            }
            for col in column..FEATURES {
                let value = a
                    .get(column)
                    .and_then(|line| line.get(col))
                    .copied()
                    .unwrap_or(0.0);
                if let Some(cell) = a.get_mut(row).and_then(|line| line.get_mut(col)) {
                    *cell -= factor * value;
                }
            }
            let head_b = b.get(column).copied().unwrap_or(0.0);
            if let Some(cell) = b.get_mut(row) {
                *cell -= factor * head_b;
            }
        }
    }
    let mut out = [0.0_f64; FEATURES];
    for row in (0..FEATURES).rev() {
        let mut sum = b.get(row).copied().unwrap_or(0.0);
        for col in (row + 1)..FEATURES {
            sum -= a
                .get(row)
                .and_then(|line| line.get(col))
                .copied()
                .unwrap_or(0.0)
                * out.get(col).copied().unwrap_or(0.0);
        }
        let head = a
            .get(row)
            .and_then(|line| line.get(row))
            .copied()
            .unwrap_or(0.0);
        if let Some(slot) = out.get_mut(row) {
            *slot = if head.abs() < 1e-12 { 0.0 } else { sum / head };
        }
    }
    out
}

fn dot(row: &[f32; FEATURES], beta: &[f32; FEATURES]) -> f32 {
    row.iter().zip(beta.iter()).map(|(a, b)| a * b).sum()
}

fn mad(values: &[f32]) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted: Vec<f32> = values.iter().map(|value| value.abs()).collect();
    sorted.sort_by(f32::total_cmp);
    sorted.get(sorted.len() / 2).copied().unwrap_or(0.0) * 1.4826
}

/// Turn one level's fitted intercepts into a delta.
///
/// The confidence is the shrink factor, so "how much of its own answer this level kept" and
/// "how much a caller should believe it" are one number rather than two that can disagree. The
/// clamp is applied here, on construction, so a delta that exists is a delta already inside its
/// bounds.
fn delta_of(values: &[f32; TARGETS], confidence: f32, samples: &[&Sample]) -> StyleDelta {
    let at = |index: usize| values.get(index).copied().unwrap_or(0.0);
    let mut hsl = aura_core::contract::colour::HslAdjustments::default();
    for (band_index, band) in HslBand::ALL.into_iter().enumerate() {
        hsl.set(
            band,
            HslShift {
                h: at(Target::HSL + band_index * 3),
                s: at(Target::HSL + band_index * 3 + 1),
                l: at(Target::HSL + band_index * 3 + 2),
            },
        );
    }
    StyleDelta {
        exposure: at(Target::Exposure.index()),
        temperature_k: at(Target::Temperature.index()),
        tint: at(Target::Tint.index()),
        contrast: at(Target::Contrast.index()),
        highlights: at(Target::Highlights.index()),
        shadows: at(Target::Shadows.index()),
        whites: at(Target::Whites.index()),
        blacks: at(Target::Blacks.index()),
        curve_shift: CurveShift::from_array([
            at(Target::CURVE),
            at(Target::CURVE + 1),
            at(Target::CURVE + 2),
            at(Target::CURVE + 3),
            at(Target::CURVE + 4),
        ]),
        hsl,
        vibrance: at(Target::Vibrance.index()),
        saturation: at(Target::Saturation.index()),
        skin_bias: SkinBias {
            warmth_deg: at(Target::SKIN),
            chroma: at(Target::SKIN + 1),
        },
        confidence: confidence.clamp(0.0, 1.0),
        samples: u32::try_from(samples.len()).unwrap_or(u32::MAX),
    }
    .clamped()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::style::{LightingBucket, APPLY_ABOVE};

    fn sample(bucket: StyleBucket, exposure: f32, archive: &str) -> Sample {
        let mut targets = [0.0_f32; TARGETS];
        if let Some(slot) = targets.get_mut(Target::Exposure.index()) {
            *slot = exposure;
        }
        Sample::new(Features::intercept_only(bucket), targets, archive)
    }

    fn portraits_daylight() -> StyleBucket {
        StyleBucket::new(SceneGroup::Portraits, LightingBucket::Daylight)
    }

    #[test]
    fn a_consistent_lean_is_recovered() {
        let bucket = portraits_daylight();
        let samples: Vec<Sample> = (0..60).map(|_| sample(bucket, 0.30, "w1")).collect();
        let tree = fit(&samples);
        // Global shrinkage at 60 samples is 60/100 = 0.6.
        assert!(
            (tree.global.exposure - 0.30 * 0.6).abs() < 0.02,
            "global exposure {}",
            tree.global.exposure
        );
    }

    #[test]
    fn the_levels_compose_by_addition_rather_than_replacing_each_other() {
        let bucket = portraits_daylight();
        let mut samples: Vec<Sample> = (0..200).map(|_| sample(bucket, 0.20, "w1")).collect();
        samples.extend((0..200).map(|_| {
            sample(
                StyleBucket::new(SceneGroup::Dance, LightingBucket::Flash),
                -0.20,
                "w1",
            )
        }));
        let tree = fit(&samples);
        // The global sits between the two; each group pulls back toward its own.
        let portraits = tree.groups.get(&SceneGroup::Portraits).cloned();
        let dance = tree.groups.get(&SceneGroup::Dance).cloned();
        let (Some(portraits), Some(dance)) = (portraits, dance) else {
            unreachable!("both groups have two hundred samples")
        };
        assert!(
            portraits.exposure > dance.exposure,
            "portraits {} dance {}",
            portraits.exposure,
            dance.exposure
        );
    }

    #[test]
    fn shrinkage_is_continuous_and_has_no_cliff() {
        // `n / (n + k)` is concave and increasing, so its largest single step is the first one -
        // from no evidence to one pair, which is `1 / (1 + k)` - and every step after that is
        // smaller. That is exactly the shape ADR-0035 decision 5 wanted and a hard cut-off
        // does not have: there is no `n` at which one more pair changes the answer visibly.
        let first = shrink_factor(1, PRIOR_STRENGTH);
        assert!((first - 1.0 / (1.0 + PRIOR_STRENGTH)).abs() < 1e-6);

        let mut previous = first;
        let mut previous_step = first;
        for n in 2..200 {
            let factor = shrink_factor(n, PRIOR_STRENGTH);
            let step = factor - previous;
            assert!(step > 0.0, "shrinkage did not increase at {n}");
            assert!(
                step <= previous_step + 1e-6,
                "the steps stopped shrinking at {n}: {step} after {previous_step}"
            );
            previous = factor;
            previous_step = step;
        }
        assert!(previous < 1.0, "shrinkage must never reach one");
    }

    #[test]
    fn seven_pairs_is_the_smallest_bucket_that_can_speak() {
        assert!(shrink_factor(6, PRIOR_STRENGTH) < APPLY_ABOVE);
        assert!(shrink_factor(7, PRIOR_STRENGTH) >= APPLY_ABOVE);
    }

    #[test]
    fn one_outlier_wedding_cannot_dominate_the_global_profile() {
        let bucket = portraits_daylight();
        // Four ordinary weddings, forty pairs each, all neutral.
        let mut samples: Vec<Sample> = Vec::new();
        for wedding in 0..4 {
            let label = format!("normal-{wedding}");
            samples.extend((0..40).map(|_| sample(bucket, 0.0, &label)));
        }
        // One wildly different wedding, four hundred pairs, all two stops up.
        samples.extend((0..400).map(|_| sample(bucket, 2.0, "outlier")));

        let tree = fit(&samples);
        assert!(tree.archive_capped, "the cap should have bound");
        assert!(
            tree.global.exposure < 0.67,
            "the outlier moved the global lean to {}",
            tree.global.exposure
        );
    }

    #[test]
    fn a_target_nobody_has_evidence_about_stays_at_zero() {
        let bucket = portraits_daylight();
        let mut samples: Vec<Sample> = (0..60).map(|_| sample(bucket, 0.30, "w1")).collect();
        // Nothing sets `hsl_known`, so every HSL target is excluded from its own fit.
        for entry in &mut samples {
            entry.hsl_known = false;
        }
        let tree = fit(&samples);
        assert!(tree.global.hsl.is_neutral());
    }

    #[test]
    fn an_empty_pile_produces_a_tree_that_changes_nothing() {
        let tree = fit(&[]);
        assert!(tree.global.is_neutral());
        assert!(tree.buckets.is_empty());
    }

    #[test]
    fn every_pair_held_out_is_the_same_as_an_empty_pile() {
        let bucket = portraits_daylight();
        let samples: Vec<Sample> = (0..10)
            .map(|_| {
                let mut entry = sample(bucket, 1.0, "w1");
                entry.held_out = true;
                entry
            })
            .collect();
        assert!(fit(&samples).global.is_neutral());
    }

    #[test]
    fn the_curve_is_compared_at_the_five_fixed_anchors() {
        let straight = ToneCurve::identity();
        assert!((curve_at(&straight, 128) - 128.0).abs() < 1e-3);
        assert!((curve_at(&straight, 0) - 0.0).abs() < 1e-3);
        assert!((curve_at(&straight, 255) - 255.0).abs() < 1e-3);
    }

    #[test]
    fn a_delta_is_bounded_on_construction() {
        let bucket = portraits_daylight();
        // A photographer who apparently runs five stops hot: bounded to the documented maximum.
        let samples: Vec<Sample> = (0..500).map(|_| sample(bucket, 5.0, "w1")).collect();
        let tree = fit(&samples);
        assert!(
            tree.global.exposure <= aura_core::contract::style::MAX_EXPOSURE_DELTA_EV + 1e-4,
            "unbounded exposure {}",
            tree.global.exposure
        );
    }
}
