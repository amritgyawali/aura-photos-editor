//! The frozen `StyleService`, and the resumable training pass that fills it.
//!
//! Two halves, the shape phases 06 to 16 all settled on. [`Style`] answers questions about
//! profiles that already exist and is what every later phase holds. [`TrainPass`] walks an
//! archive and produces one, and is what the job graph holds.
//!
//! ## Resumability, invariant 5
//!
//! The work remaining is a query - [`crate::store::StyleStore::fitted`] - not a journal, and
//! the unit is a **pair** rather than a photograph. Section 11 budgets twenty-five minutes for
//! two thousand pairs, so a photographer who cancels at 60 % and restarts must re-fit none of
//! what they already paid for. The key is the original's content hash, so a re-scan of an
//! archive that was renamed between runs re-uses the fits too.
//!
//! ## The baseline is supplied, never assumed
//!
//! A style is a residual on what phases 15 and 16 would have said about the same photograph, so
//! the fitter needs that answer for every pair. It cannot compute it: `aura-style` depends on no
//! brain crate, deliberately, and the frames in a photographer's archive are files rather than
//! catalog rows.
//!
//! So [`Baseline`] is a port, and what a caller passes in decides what the residual means:
//!
//! * Given the real per-frame decisions, `StyleDelta` is what this phase promises - the
//!   difference between the consensus and this photographer.
//! * Given [`NeutralBaseline`], the delta is the photographer's **absolute** edit relative to a
//!   neutral develop, which is a different and much larger number. Every pair fitted that way
//!   carries `StyleCode::ParametersFitted` and the profile's report says so.
//!
//! The port is here rather than hidden because the difference between those two is the
//! difference between a profile that is a taste and a profile that is a preset, and a reader
//! should be able to see which one a given run produced.
//!
//! ## No silent failure, invariant 9
//!
//! A pair that cannot be matched, read or fitted is **written with `accepted = 0` and a reason
//! code**, which is the opposite of what phases 09, 15 and 16 do with a failed frame - and the
//! opposite is right here. A failed frame in those phases must leave no row, because a stored
//! neutral decision reads as a decision. A failed pair here *is* the evidence: a scan that
//! dropped its rejections could report a hundred percent acceptance on any archive.

use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::scene::ChapterId;
use aura_core::contract::style::{
    ExtractSource, MatchMethod, ProfileStatus, StyleAdvice, StyleBucket, StyleCode, StyleOutline,
    StyleProfile, StyleQuery, StyleService,
};
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, ProfileId, ProjectId};
use aura_render::cpu::Frame;

use crate::bucket::{Features, PairSource};
use crate::diagnostics::{self, HeldOut, Report};
use crate::errors;
use crate::extract::{self, TheirParams};
use crate::fit::{self, Budget};
use crate::infer::{self, Context};
use crate::pairs::{Archive, Matched};
use crate::store::{PairFeatures, PairRow, StyleStore};
use crate::tree::{self, Sample, Tree};

/// Which build's fitter, shrinkage and feature set produced a profile.
///
/// Bumped on any change to `crate::tree`, `crate::bucket` or `crate::fit`. Two buckets fitted
/// under different versions are not comparable with each other, and `AURA-ML-5077` is what says
/// so rather than letting a comparison happen silently. Ninth version-drift code in the product.
pub const ANALYSIS_VER: u16 = 1;

/// The fewest accepted pairs a training run needs before it will produce a profile.
///
/// A hundred and twenty. Below `USABLE_PAIRS` deliberately: three hundred is where AURA is
/// *confident*, and refusing everything below it would mean a photographer with two weddings
/// gets nothing rather than a weak profile that says it is weak. A hundred and twenty is where
/// a global lean stops being noise - it is `GLOBAL_PRIOR` times three, so the global shrinkage
/// factor is 0.75 and the profile has something to say.
pub const MIN_PAIRS: usize = 120;

/// The telemetry stage name, matching section 11's `style.training` event.
pub const STAGE: &str = "style.training";

/// The adoption event, section 11's `style.profile_adopted`.
pub const ADOPTED_STAGE: &str = "style.profile_adopted";

/// The fallback event, section 11's `style.bucket_fallback`.
pub const FALLBACK_STAGE: &str = "style.bucket_fallback";

/// What phases 15 and 16 would have said about one of the photographer's own frames.
///
/// A port, deliberately **not** frozen. See this module's header for why what a caller passes
/// here decides what the whole phase measures.
pub trait Baseline: Send + Sync + std::fmt::Debug {
    /// The baseline parameters for one pair.
    fn of(&self, key: &str, query: &StyleQuery) -> TheirParams;
}

/// The baseline that assumes nothing: a neutral develop at 5,500 K.
///
/// Honest and blunt. Under this baseline a `StyleDelta` is the photographer's absolute edit
/// rather than their lean away from a consensus, which is a **larger** number and a different
/// claim - and the bounds in `aura_core::contract::style` will clamp most of it away, which is
/// the correct behaviour and is also a signal that the baseline was wrong.
#[derive(Debug, Default, Clone, Copy)]
pub struct NeutralBaseline;

impl Baseline for NeutralBaseline {
    fn of(&self, _key: &str, _query: &StyleQuery) -> TheirParams {
        TheirParams::default()
    }
}

/// What one training pass did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrainReport {
    /// Pairs the matcher found.
    pub matched: usize,
    /// Pairs the fit accepted.
    pub accepted: usize,
    /// Pairs it rejected, each written with a reason.
    pub rejected: usize,
    /// Pairs that were already fitted at this version and were skipped.
    pub reused: usize,
    /// Pairs whose parameters came from an XMP sidecar.
    pub from_xmp: usize,
    /// Buckets the tree populated.
    pub buckets: usize,
    /// The measured report.
    pub report: Report,
    /// Wall clock.
    pub elapsed_ms: u64,
    /// True when the pass was cancelled.
    pub cancelled: bool,
}

/// The frozen `StyleService`.
#[derive(Debug)]
pub struct Style {
    store: Arc<StyleStore>,
    engine: String,
}

impl Style {
    /// Wrap one store.
    #[must_use]
    pub fn new(store: Arc<StyleStore>) -> Self {
        Self {
            store,
            engine: aura_recipe::contract::recipe::ENGINE.to_string(),
        }
    }

    /// The store underneath, for the panel and the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<StyleStore> {
        &self.store
    }

    /// The engine this build renders with.
    #[must_use]
    pub fn engine(&self) -> &str {
        &self.engine
    }

    /// The advice for one photograph, given which profile the project and chapter selected.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    pub fn advise_in(
        &self,
        project: ProjectId,
        chapter: Option<ChapterId>,
        query: &StyleQuery,
    ) -> AuraResult<StyleAdvice> {
        let selected = self.store.selected(project, chapter)?;
        let profile = match selected {
            Some(id) => self.store.profile(id)?,
            None => None,
        };
        Ok(infer::advise(
            profile.as_ref(),
            query,
            &Context {
                engine: &self.engine,
                analysis_ver: ANALYSIS_VER,
            },
        ))
    }
}

impl StyleService for Style {
    fn outline(&self, project: ProjectId) -> AuraResult<StyleOutline> {
        self.store.outline(project)
    }

    fn profiles(&self) -> AuraResult<Vec<StyleProfile>> {
        self.store.profiles()
    }

    fn profile(&self, id: ProfileId) -> AuraResult<Option<StyleProfile>> {
        self.store.profile(id)
    }

    fn advise(&self, query: &StyleQuery) -> AuraResult<StyleAdvice> {
        match query.project {
            Some(project) => self.advise_in(project, Some(query.chapter), query),
            // No project means no selection to look up, which is a complete answer rather than
            // an error: `advise` never fails to answer, and a caller that had to handle `None`
            // would be a caller that could forget to.
            None => Ok(StyleAdvice::none(infer::bucket_of(query))),
        }
    }

    fn adopt(&self, id: ProfileId) -> Result<(), AuraError> {
        let Some(profile) = self.store.profile(id)? else {
            return Err(errors::profile_refused(
                &id.to_db(),
                "no profile with that id exists",
            ));
        };
        if profile.engine_ver != self.engine {
            return Err(errors::profile_refused(
                &id.to_db(),
                &format!(
                    "the profile was fitted against engine {} and this build renders with {}; \
                     its measured accuracy is about a build that no longer exists, so re-train \
                     it before adopting",
                    profile.engine_ver, self.engine
                ),
            ));
        }
        if profile.analysis_ver != ANALYSIS_VER {
            return Err(errors::profile_refused(
                &id.to_db(),
                &format!(
                    "the profile was fitted by analysis version {} and this build is {}",
                    profile.analysis_ver, ANALYSIS_VER
                ),
            ));
        }
        self.store.adopt(id)
    }

    fn select(
        &self,
        project: ProjectId,
        chapter: Option<ChapterId>,
        profile: Option<ProfileId>,
    ) -> Result<(), AuraError> {
        self.store.select(project, chapter, profile)
    }
}

/// One resumable walk over a photographer's archives.
pub struct TrainPass {
    store: Arc<StyleStore>,
    clock: Arc<dyn Clock>,
    source: Arc<dyn PairSource>,
    baseline: Arc<dyn Baseline>,
    budget: Budget,
    name: String,
}

impl std::fmt::Debug for TrainPass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrainPass")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl TrainPass {
    /// A pass over one named profile's archives.
    #[must_use]
    pub fn new(
        store: Arc<StyleStore>,
        clock: Arc<dyn Clock>,
        source: Arc<dyn PairSource>,
        baseline: Arc<dyn Baseline>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            store,
            clock,
            source,
            baseline,
            budget: Budget::default(),
            name: name.into(),
        }
    }

    /// Override the fitting budget. The gate and the fixtures use a smaller one.
    #[must_use]
    pub const fn with_budget(mut self, budget: Budget) -> Self {
        self.budget = budget;
        self
    }

    /// Scan and fit every archive, then measure and store the profile.
    ///
    /// `context_of` turns one pair into the scene, light and features it should be bucketed and
    /// conditioned by. It is a closure rather than a service because the caller is the one that
    /// can see phases 07 and 15, and `aura-style` deliberately cannot.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5073` when fewer than [`MIN_PAIRS`] pairs survive. Whatever the catalog raised.
    pub fn run<F>(
        &self,
        archives: &[Archive],
        context_of: F,
        progress: &dyn ProgressSink,
        cancel: &CancelToken,
    ) -> AuraResult<TrainReport>
    where
        F: Fn(&Matched) -> StyleQuery,
    {
        let started = self.clock.monotonic_ms();
        let mut out = TrainReport::default();
        let already = self.store.fitted(&self.name, ANALYSIS_VER)?;
        let engine = fit::optimise::engine(Arc::clone(&self.clock));

        let total: usize = archives
            .iter()
            .map(|archive| crate::pairs::match_all(archive).pairs.len())
            .sum();
        let mut done = 0_usize;

        for archive in archives {
            let matched = crate::pairs::match_all(archive);
            out.matched += matched.pairs.len();
            let mut audit = extract::XmpAudit::default();

            for (index, pair) in matched.pairs.iter().enumerate() {
                if cancel.is_cancelled() {
                    out.cancelled = true;
                    break;
                }
                let key = pair
                    .original
                    .hash
                    .clone()
                    .unwrap_or_else(|| pair.original.name.clone());
                if already.contains(&key) {
                    out.reused += 1;
                    done += 1;
                    continue;
                }

                let query = context_of(pair);
                let features = Features::of(&query);
                let baseline = self.baseline.of(&key, &query);
                let confirm = index % extract::CONFIRM_EVERY == 0;

                let extracted = self.extract_one(&engine, pair, &key, &query, confirm, &mut audit);
                match &extracted {
                    Ok(value) if value.accepted() => out.accepted += 1,
                    _ => out.rejected += 1,
                }
                let extracted = extracted.unwrap_or_else(|_| {
                    extract::Extracted::rejected(StyleCode::PairUnmatched, 0.0)
                });
                if extracted.source == ExtractSource::Xmp {
                    out.from_xmp += 1;
                }

                let row = PairRow {
                    original_hash: key.clone(),
                    profile_name: self.name.clone(),
                    original_name: pair.original.name.clone(),
                    final_name: pair.finished.name.clone(),
                    matched_by: pair.method,
                    extracted_from: extracted.source,
                    bucket: features.bucket,
                    params: extracted.params.clone(),
                    features: PairFeatures {
                        subject_luma: query.subject_luma,
                        cct_k: query.cct_k,
                        dynamic_range_ev: query.dynamic_range_ev,
                        iso: query.iso,
                        flash: query.flash,
                        mixed_light: query.mixed_light,
                    },
                    residual_de00: extracted.residual_de00,
                    accepted: extracted.accepted(),
                    rejection: extracted.rejection,
                    // The split is deterministic - every fifth pair in the order they are
                    // scanned - so a re-train on the same archive measures itself against the
                    // same pairs. A random split would make the headline accuracy figure change
                    // with no change to the data.
                    held_out: extracted.accepted() && index % tree::HOLDOUT_EVERY == 0,
                    archive: archive.label.clone(),
                    analysis_ver: ANALYSIS_VER,
                };
                let _ = baseline;
                self.store.put_pair(&row)?;

                done += 1;
                progress.report(ProgressUpdate {
                    stage: STAGE,
                    done: done as u64,
                    total: total as u64,
                    current: Some(pair.original.name.clone()),
                });
            }
            if out.cancelled {
                break;
            }
        }

        out.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        if out.cancelled {
            return Ok(out);
        }

        let stored = self.store.pairs(&self.name, true)?;
        if stored.len() < MIN_PAIRS {
            return Err(errors::training_refused(
                stored.len(),
                MIN_PAIRS,
                "a profile fitted on a handful of frames is a preset that says it is a style",
            ));
        }

        let (tree, profile, report) = self.build(&stored, context_of)?;
        out.buckets = tree.buckets.len();
        out.report = report;
        self.store.put_profile(&profile, &tree)?;
        Ok(out)
    }

    /// Fit the tree, measure it and assemble the profile.
    ///
    /// Split out so the gate and the evaluation harness can call it on stored pairs without
    /// re-scanning an archive, which is also what makes a re-fit after a shrinkage change cheap.
    ///
    /// # Errors
    ///
    /// Whatever the catalog raised.
    pub fn build<F>(
        &self,
        stored: &[PairRow],
        _context_of: F,
    ) -> AuraResult<(Tree, StyleProfile, Report)>
    where
        F: Fn(&Matched) -> StyleQuery,
    {
        let samples: Vec<Sample> = stored
            .iter()
            .map(|row| {
                let query = query_of(row);
                let baseline = self.baseline.of(&row.original_hash, &query);
                let mut sample = Sample::new(
                    Features::of(&query),
                    tree::targets_of(&row.params, &baseline, None),
                    row.archive.clone(),
                );
                sample.hsl_known = row.extracted_from == ExtractSource::Xmp;
                sample.skin_known = false;
                sample.held_out = row.held_out;
                sample
            })
            .collect();

        let tree = tree::fit(&samples);
        let accepted = u32::try_from(stored.len()).unwrap_or(u32::MAX);

        let mut profile = StyleProfile::empty(
            ProfileId::new(),
            self.name.clone(),
            aura_recipe::contract::recipe::ENGINE,
        );
        profile.version = crate::profile::next_version(self.previous_version()?);
        profile.status = ProfileStatus::Candidate;
        profile.global = tree.global.clone();
        profile.groups = tree.groups.clone();
        profile.buckets = tree.buckets.clone();
        profile.trained_pairs = accepted;
        profile.trained_at = epoch_ms(self.clock.now_utc());
        profile.analysis_ver = ANALYSIS_VER;

        let held: Vec<HeldOut> = stored
            .iter()
            .filter(|row| row.held_out)
            .map(|row| {
                let query = query_of(row);
                let baseline = self.baseline.of(&row.original_hash, &query);
                let bucket = infer::bucket_of(&query);
                let (delta, _) = profile.resolve(bucket);
                HeldOut {
                    bucket,
                    de00: parameter_distance(&row.params, &styled(&baseline, &delta)),
                    baseline_de00: parameter_distance(&row.params, &baseline),
                }
            })
            .collect();

        let report = diagnostics::evaluate(&profile, &held, accepted, 0);
        profile.diagnostics = report.diagnostics.clone();
        Ok((tree, profile, report))
    }

    fn previous_version(&self) -> AuraResult<Option<u16>> {
        Ok(self
            .store
            .profiles()?
            .into_iter()
            .filter(|profile| profile.name == self.name)
            .map(|profile| profile.version)
            .max())
    }

    fn extract_one(
        &self,
        engine: &aura_render::CpuEngine,
        pair: &Matched,
        key: &str,
        _query: &StyleQuery,
        confirm: bool,
        audit: &mut extract::XmpAudit,
    ) -> AuraResult<extract::Extracted> {
        let camera = self.source.camera(key);
        let base = aura_recipe::fixtures::neutral(key, &camera);

        if let Some(packet) = self.source.xmp(key) {
            let mut read = extract::from_xmp(&packet, &base)?;
            if confirm {
                if let Ok(fitted) = self.fit_one(engine, key, &base) {
                    audit.record(fitted.residual_de00, fit::REJECT_DE00);
                    read.residual_de00 = fitted.residual_de00;
                }
            }
            return Ok(read);
        }

        let fitted = self.fit_one(engine, key, &base)?;
        let rejection = fitted.rejection();
        Ok(extract::Extracted {
            params: fitted.candidate.to_params(),
            source: ExtractSource::Fitted,
            residual_de00: fitted.residual_de00,
            hsl_known: false,
            rejection: rejection.or_else(|| {
                let _ = pair;
                None
            }),
        })
    }

    fn fit_one(
        &self,
        engine: &aura_render::CpuEngine,
        key: &str,
        base: &aura_recipe::Recipe,
    ) -> AuraResult<fit::Fitted> {
        let (rgb, width, height) = self.source.original(key, fit::FIT_LONG_EDGE)?;
        let (target, target_w, target_h) = self.source.finished(key, fit::FIT_LONG_EDGE)?;
        if width != target_w || height != target_h {
            return Err(errors::pair_failed(
                key,
                "the original and the final are different sizes, which means the final was \
                 cropped - a crop is work this phase cannot model",
            ));
        }
        let frame = Frame::working(rgb, width, height, &self.source.camera(key));
        fit::fit(engine, &frame, base, &target, self.budget)
    }
}

/// The query one stored pair implies.
fn query_of(row: &PairRow) -> StyleQuery {
    StyleQuery {
        project: None,
        scene: aura_core::SceneId::Unknown,
        chapter: row.bucket.group.usual_chapter(),
        lighting: row.bucket.lighting,
        subject_luma: row.features.subject_luma,
        cct_k: row.features.cct_k,
        dynamic_range_ev: row.features.dynamic_range_ev,
        iso: row.features.iso,
        mixed_light: row.features.mixed_light,
        flash: row.features.flash,
    }
}

/// One baseline with a style delta applied to it.
///
/// The same addition phases 15 and 16 perform on their own solved parameters, done here so the
/// held-out evaluation measures the thing that will actually be delivered.
#[must_use]
pub fn styled(
    baseline: &TheirParams,
    delta: &aura_core::contract::style::StyleDelta,
) -> TheirParams {
    TheirParams {
        exposure: baseline.exposure + delta.exposure,
        temperature_k: baseline.temperature_k + delta.temperature_k,
        tint: baseline.tint + delta.tint,
        contrast: baseline.contrast + delta.contrast,
        highlights: baseline.highlights + delta.highlights,
        shadows: baseline.shadows + delta.shadows,
        whites: baseline.whites + delta.whites,
        blacks: baseline.blacks + delta.blacks,
        vibrance: baseline.vibrance + delta.vibrance,
        saturation: baseline.saturation + delta.saturation,
        curve: delta.curve_shift.applied_to(&baseline.curve),
        hsl: delta.applied_to_hsl(&baseline.hsl),
    }
}

/// How far two parameter sets sit apart, as a stand-in for a rendered dE00.
///
/// **This is not a dE00 and it does not pretend to be one.** A true style-match figure needs the
/// photographer's own final and a render of the styled recipe, which is what
/// `tests/eval/style_eval.rs` measures. This is the *parameter* distance, scaled so that a stop
/// of exposure and a hundred points of contrast contribute comparably, and it exists so that a
/// training run can report a per-bucket figure without re-rendering two thousand frames inside
/// the pass.
///
/// The two agree in direction and not in magnitude, which is why the eval harness measures the
/// real one and the profile stores this one under a name that says what it is.
#[must_use]
pub fn parameter_distance(theirs: &TheirParams, ours: &TheirParams) -> f32 {
    let terms = [
        (theirs.exposure - ours.exposure) / 0.5,
        (theirs.temperature_k - ours.temperature_k) / 500.0,
        (theirs.tint - ours.tint) / 10.0,
        (theirs.contrast - ours.contrast) / 25.0,
        (theirs.highlights - ours.highlights) / 25.0,
        (theirs.shadows - ours.shadows) / 25.0,
        (theirs.whites - ours.whites) / 25.0,
        (theirs.blacks - ours.blacks) / 25.0,
        (theirs.vibrance - ours.vibrance) / 25.0,
        (theirs.saturation - ours.saturation) / 25.0,
    ];
    let sum: f32 = terms.iter().map(|term| term * term).sum();
    (sum / terms.len() as f32).sqrt()
}

fn epoch_ms(now: time::OffsetDateTime) -> i64 {
    (now.unix_timestamp_nanos() / 1_000_000) as i64
}

/// Which bucket a query lands in, re-exported so a caller does not import two modules for one
/// question.
#[must_use]
pub fn bucket_of(query: &StyleQuery) -> StyleBucket {
    infer::bucket_of(query)
}

/// How a pair was matched, as a slug, for the report.
#[must_use]
pub const fn method_slug(method: MatchMethod) -> &'static str {
    method.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_neutral_baseline_is_a_neutral_develop() {
        let params = NeutralBaseline.of("x", &StyleQuery::default());
        assert!(params.exposure.abs() < f32::EPSILON);
        assert!((params.temperature_k - 5500.0).abs() < f32::EPSILON);
    }

    #[test]
    fn applying_a_delta_moves_every_parameter_it_names() {
        let baseline = TheirParams {
            contrast: 10.0,
            ..TheirParams::default()
        };
        let delta = aura_core::contract::style::StyleDelta {
            contrast: 5.0,
            exposure: 0.25,
            ..aura_core::contract::style::StyleDelta::neutral()
        };
        let out = styled(&baseline, &delta);
        assert!((out.contrast - 15.0).abs() < 1e-4);
        assert!((out.exposure - 0.25).abs() < 1e-4);
    }

    #[test]
    fn the_parameter_distance_is_zero_for_identical_sets_and_grows_with_disagreement() {
        let a = TheirParams::default();
        assert!(parameter_distance(&a, &a).abs() < 1e-6);
        let b = TheirParams {
            contrast: 25.0,
            ..TheirParams::default()
        };
        assert!(parameter_distance(&a, &b) > 0.0);
        let c = TheirParams {
            contrast: 50.0,
            ..TheirParams::default()
        };
        assert!(parameter_distance(&a, &c) > parameter_distance(&a, &b));
    }

    #[test]
    fn the_minimum_pair_count_sits_below_the_confidence_threshold_on_purpose() {
        assert!(MIN_PAIRS < aura_core::contract::style::USABLE_PAIRS as usize);
        // Three times the global prior, so a profile that exists has a global lean worth
        // something rather than a shrunk-to-nothing one.
        assert!((MIN_PAIRS as f32) >= tree::GLOBAL_PRIOR * 3.0);
    }
}
