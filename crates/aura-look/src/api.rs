//! The frozen `LookService` and the measuring pass.
//!
//! ## The pass, in the order it runs
//!
//! 1. **Resolve.** What was typed into the box becomes a [`ReferenceOrigin`]; the folder becomes
//!    a list of files. A source this build cannot fetch through refuses here, before a disk is
//!    touched.
//! 2. **Measure the reference.** One decode each, one [`ReferenceReading`] each.
//! 3. **Measure the baseline.** The photographer's own frames, rendered at what phases 15 and 16
//!    already decided and read with the same measurer. This is the half that makes a look a
//!    residual, and it is the half that is skipped when a project has no analysed frames - with
//!    [`LookCode::BaselineAbsent`] on the row rather than a quiet fallback.
//! 4. **Solve.** Per lighting bucket, and globally.
//! 5. **Refine.** Walk each parameter through the real renderer until the measured distance
//!    stops falling.
//! 6. **Verify.** Render both sides and record what actually moved.
//! 7. **Store.** The look, its buckets, its reference rows, its reasons and the match.
//!
//! Step 6 is not optional and step 7 will not write a `StyleProfile` without it. A look whose
//! effect has not been measured is a delta nobody has checked, and
//! `project_look_needs_a_match` is the lock in the database underneath that sentence.

use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::error::AuraResult;
use aura_core::contract::ids::{ProfileId, ProjectId};
use aura_core::contract::look::{
    LookAggregate, LookBucket, LookCode, LookDiagnostics, LookMatchReport, LookOutline,
    LookOverride, LookProfile, LookReason, LookService, MediaSource, ReferenceOrigin,
    ReferenceReading, MIN_REFERENCES, USABLE_REFERENCES, WEAK_BUCKET_REFERENCES,
};
use aura_core::contract::style::{LightingBucket, StyleDelta};
use aura_core::errors::ml::look_refused;

use crate::aggregate;
use crate::materialise;
use crate::solve;
use crate::source::{self, Reference};
use crate::store::LookStore;
use crate::verify::{self, FrameProbe, OwnFrame, Renderer};

/// What one measuring pass did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MeasureReport {
    /// Reference photographs the walk found.
    pub found: usize,
    /// Reference photographs that read.
    pub measured: usize,
    /// Reference photographs that were refused.
    pub refused: usize,
    /// The photographer's own frames the baseline was measured over.
    pub baseline_frames: usize,
    /// Lighting buckets populated.
    pub buckets: usize,
    /// The look that came out.
    pub profile: Option<ProfileId>,
    /// What the match measured, when there were frames to measure it on.
    pub matched: Option<LookMatchReport>,
    /// True when the pass was cancelled.
    pub cancelled: bool,
}

/// One sentence a photographer reads, assembled from a closed vocabulary.
///
/// **Rendered from codes, never stored.** Phase 27's rule, and this is the function that keeps
/// it: there is no free-text field anywhere in migration 31 that automation could write into, so
/// the only way a sentence exists is by being built here from reasons that are already on a row.
///
/// At most two clauses. The panel shows every reason underneath; a summary that listed all of
/// them would be a paragraph nobody reads, and a summary that listed the wrong two would be
/// worse than one clause and a list.
#[must_use]
pub fn summarise(reasons: &[LookReason]) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for reason in reasons {
        if reason.code.is_actionable() && parts.len() < 2 {
            parts.push(reason.code.sentence());
        }
    }
    if parts.is_empty() {
        if let Some(first) = reasons.first() {
            parts.push(first.code.sentence());
        }
    }
    parts.join(" ")
}

/// The frozen `LookService`.
#[derive(Debug)]
pub struct Look {
    store: Arc<LookStore>,
    engine: String,
}

impl Look {
    /// Wrap one store.
    #[must_use]
    pub fn new(store: Arc<LookStore>) -> Self {
        Self {
            store,
            engine: aura_recipe::contract::recipe::ENGINE.to_string(),
        }
    }

    /// The store underneath, for the panel and the gate.
    #[must_use]
    pub fn store(&self) -> &Arc<LookStore> {
        &self.store
    }

    /// The engine this build renders with.
    #[must_use]
    pub fn engine(&self) -> &str {
        &self.engine
    }

    /// One look, as phase 17's profile shape, once its effect has been measured.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5147` when the look is not stored or has never been measured.
    pub fn as_style(
        &self,
        id: ProfileId,
        project: ProjectId,
    ) -> AuraResult<aura_core::contract::style::StyleProfile> {
        let Some(look) = self.store.profile(id)? else {
            return Err(look_refused("that look is not stored"));
        };
        let Some(matched) = self.store.last_match(project)? else {
            return Err(look_refused(
                "that look has not been measured on this project, so there is no figure to put \
                 beside it",
            ));
        };
        Ok(materialise::into_style(&look, &matched))
    }
}

impl LookService for Look {
    fn outline(&self, project: ProjectId) -> AuraResult<LookOutline> {
        self.store.outline(project)
    }

    fn profiles(&self) -> AuraResult<Vec<LookProfile>> {
        self.store.profiles()
    }

    fn profile(&self, id: ProfileId) -> AuraResult<Option<LookProfile>> {
        self.store.profile(id)
    }

    fn advise(&self, project: ProjectId, lighting: LightingBucket) -> AuraResult<StyleDelta> {
        let Some((id, strength)) = self.store.selected(project)? else {
            // Nothing selected is the neutral delta, which changes nothing. Phase 17's
            // guarantee inherited: there is no state of this system in which switching the
            // feature on makes a photograph worse than leaving it off.
            return Ok(StyleDelta::neutral());
        };
        let Some(look) = self.store.profile(id)? else {
            return Ok(StyleDelta::neutral());
        };
        // A look measured against a renderer that has since moved is not applied at its stored
        // value. `AURA-ML-5148` is the code, and returning the neutral delta rather than an
        // error is deliberate: a stale look should make the product do nothing, not stop.
        if look.engine_ver != self.engine {
            tracing::warn!(
                stored = %look.engine_ver,
                current = %self.engine,
                "look measured against a different render engine; not applied"
            );
            return Ok(StyleDelta::neutral());
        }
        let (delta, _) = look.resolve(lighting);
        // The photographer's strength override. Down only - `LookOverride::strength` clamps on
        // construction and migration 31's CHECK is the second lock.
        Ok(solve::blend(&StyleDelta::neutral(), &delta, strength))
    }

    fn select(&self, project: ProjectId, profile: Option<ProfileId>) -> AuraResult<()> {
        if let Some(id) = profile {
            if self.store.profile(id)?.is_none() {
                return Err(look_refused("that look is not stored"));
            }
        }
        self.store.select(project, profile)
    }

    fn override_with(&self, project: ProjectId, change: &LookOverride) -> AuraResult<()> {
        match change {
            LookOverride::Rename { name } => {
                let Some((id, _)) = self.store.selected(project)? else {
                    return Err(look_refused("this project has no look selected"));
                };
                self.store.rename(id, name)
            }
            LookOverride::Strength { fraction } => {
                self.store.set_strength(project, fraction.clamp(0.0, 1.0))
            }
            LookOverride::Clear => self.store.select(project, None),
        }
    }

    fn match_report(&self, project: ProjectId) -> AuraResult<Option<LookMatchReport>> {
        self.store.last_match(project)
    }

    fn forget(&self, id: ProfileId) -> AuraResult<()> {
        self.store.forget(id)
    }
}

/// The reasons every look carries, whatever its numbers turn out to be.
///
/// Three of them are unconditional and they are the three this phase is most often asked about:
/// there is no scene axis, nothing was learned about skin, and no hue was rotated. A code that
/// only appeared when something went wrong would leave a photographer assuming the opposite on
/// every look that worked.
fn standing_reasons(
    reference: &Reference,
    references: usize,
    baseline_frames: usize,
) -> Vec<LookReason> {
    let mut reasons: Vec<LookReason> = reference.reasons.clone();
    reasons.push(LookReason::bare(LookCode::SceneAxisNotLearned));
    reasons.push(LookReason::bare(LookCode::SkinNotLearned));
    reasons.push(LookReason::bare(LookCode::HueRotationWithheld));

    if baseline_frames == 0 {
        reasons.push(LookReason::bare(LookCode::BaselineAbsent));
    } else {
        reasons.push(LookReason::counted(
            LookCode::BaselineMeasured,
            baseline_frames as f32,
        ));
        if baseline_frames < references / 2 {
            reasons.push(LookReason::counted(
                LookCode::BaselineThin,
                baseline_frames as f32,
            ));
        }
    }
    if u32::try_from(references).unwrap_or(0) < USABLE_REFERENCES {
        reasons.push(LookReason::measured(
            LookCode::ReferencesBelowUsable,
            references as f32,
            USABLE_REFERENCES as f32,
        ));
    }
    reasons
}

/// Everything one measuring pass needs, in one place.
///
/// A struct rather than eight arguments, and the grouping is not only clippy's threshold: the
/// six fields are what a caller has to have decided before a pass can start, and a command that
/// forgot one of them should fail to compile rather than pass `None` twice in a row.
#[derive(Debug, Clone, Copy)]
pub struct MeasureRequest<'a> {
    /// What the photographer typed into the box. May be empty.
    pub address: &'a str,
    /// Which route the files arrived by.
    pub media: MediaSource,
    /// The folder to walk. `None` only for a route that refuses.
    pub folder: Option<&'a std::path::Path>,
    /// The project the look is measured against and stored for.
    pub project: ProjectId,
    /// The photographer's own frames, already decoded, carrying phases 15 and 16's answers.
    pub frames: &'a [OwnFrame],
    /// The engine every measurement in the pass goes through.
    pub renderer: &'a Renderer,
}

/// The measuring pass: a reference folder in, a stored look out.
pub struct MeasurePass {
    store: Arc<LookStore>,
    clock: Arc<dyn Clock>,
    name: String,
}

impl std::fmt::Debug for MeasurePass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeasurePass")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl MeasurePass {
    /// Build a pass that will store its result under one name.
    #[must_use]
    pub fn new(store: Arc<LookStore>, clock: Arc<dyn Clock>, name: impl Into<String>) -> Self {
        Self {
            store,
            clock,
            name: name.into(),
        }
    }

    /// Measure a reference and store the look, measuring its effect on one project's frames.
    ///
    /// `frames` is the photographer's own work, already decoded and carrying what phases 15 and
    /// 16 decided about it. An empty slice is a legal call and produces a look with
    /// [`LookCode::BaselineAbsent`] and no match report - which cannot then be selected, because
    /// `project_look_needs_a_match` refuses it.
    ///
    /// # Errors
    ///
    /// [`aura_core::errors::ml::ML_LOOK_REFERENCE_REFUSED`] when the reference will not resolve,
    /// or `AURA-DB-3006` when the catalog refuses the write.
    pub fn run<F>(&self, request: &MeasureRequest<'_>, mut cancelled: F) -> AuraResult<MeasureReport>
    where
        F: FnMut() -> bool,
    {
        let MeasureRequest {
            address,
            media,
            folder,
            project,
            frames,
            renderer,
        } = *request;
        let reference = source::resolve(address, media, folder)?;
        let mut report = MeasureReport {
            found: reference.len(),
            ..MeasureReport::default()
        };

        // --- 2. measure the reference -----------------------------------
        let mut readings: Vec<ReferenceReading> = Vec::with_capacity(reference.len());
        let mut refused = 0_usize;
        for file in &reference.files {
            if cancelled() {
                report.cancelled = true;
                return Ok(report);
            }
            match source::decode(file) {
                Ok(image) => readings.push(crate::measure::read(file.key.clone(), &image)),
                Err(error) => {
                    tracing::debug!(path = %file.path.display(), %error, "reference refused");
                    refused = refused.saturating_add(1);
                }
            }
        }
        report.measured = readings.len();
        report.refused = refused;

        if u32::try_from(readings.len()).unwrap_or(0) < MIN_REFERENCES {
            return Err(aura_core::errors::ml::look_reference_refused(format!(
                "{} of {} reference photographs read, against a minimum of {MIN_REFERENCES}",
                readings.len(),
                reference.len()
            )));
        }

        // --- 3. measure the baseline ------------------------------------
        let neutral = StyleDelta::neutral();
        let baseline_readings: Vec<ReferenceReading> = frames
            .iter()
            .filter_map(|frame| renderer.read_with(frame, &neutral).ok())
            .collect();
        report.baseline_frames = baseline_readings.len();

        // --- 4 and 5. solve, then refine --------------------------------
        let mut look = self.assemble(&reference, &readings, &baseline_readings, frames, renderer);
        report.buckets = look.buckets.len();

        // --- 6. verify ---------------------------------------------------
        let matched = if frames.is_empty() {
            None
        } else {
            let residuals = verify::measure(&look, renderer, frames);
            let user_edited = frames.iter().filter(|frame| frame.user_edited).count();
            let user_edited = u32::try_from(user_edited).unwrap_or(0);
            Some(LookMatchReport {
                profile: look.id,
                project,
                before_de00: verify::weighted(&residuals, |row| row.before_de00),
                after_de00: verify::weighted(&residuals, |row| row.after_de00),
                frames: u32::try_from(frames.len()).unwrap_or(u32::MAX),
                user_edited,
                reasons: verify::reasons_for(&residuals, user_edited),
                buckets: residuals,
            })
        };

        // --- 7. store -----------------------------------------------------
        if let Some(report) = &matched {
            look.diagnostics.reasons.extend(report.reasons.clone());
        }
        look.diagnostics.summary = summarise(&look.diagnostics.reasons);
        self.store.put(&look, &readings)?;
        if let Some(report) = &matched {
            self.store.put_match(report, &look.engine_ver)?;
        }

        report.profile = Some(look.id);
        report.matched = matched;
        Ok(report)
    }

    /// Solve every bucket and the global lean, and assemble the look.
    fn assemble(
        &self,
        reference: &Reference,
        readings: &[ReferenceReading],
        baseline_readings: &[ReferenceReading],
        frames: &[OwnFrame],
        renderer: &Renderer,
    ) -> LookProfile {
        let engine = aura_recipe::contract::recipe::ENGINE;
        let mut look = LookProfile::empty(ProfileId::new(), self.name.clone(), engine);
        look.origin = reference.origin.clone();
        look.source = reference.source;
        look.references = u32::try_from(readings.len()).unwrap_or(u32::MAX);
        look.measured_at = (self.clock.now_utc().unix_timestamp_nanos() / 1_000_000) as i64;
        look.analysis_ver = crate::ANALYSIS_VER;

        let reference_global = aggregate::fold(&aggregate::all(readings));
        let baseline_global = aggregate::fold(&aggregate::all(baseline_readings));

        let mut reasons = standing_reasons(reference, readings.len(), baseline_readings.len());

        // The global lean, refined against every frame the photographer has.
        look.global = Self::solve_one(&reference_global, &baseline_global, frames, renderer, None);

        // One bucket per light, each shrunk toward the global lean.
        let by_light = aggregate::by_lighting(readings);
        for (lighting, in_bucket) in by_light {
            let reference_bucket = aggregate::fold(&in_bucket);
            let frames_here: Vec<&OwnFrame> = frames
                .iter()
                .filter(|frame| frame.lighting == lighting)
                .collect();
            let baseline_here: Vec<&ReferenceReading> = baseline_readings
                .iter()
                .zip(frames.iter())
                .filter(|(_, frame)| frame.lighting == lighting)
                .map(|(reading, _)| reading)
                .collect();
            let baseline_bucket = if baseline_here.is_empty() {
                baseline_global.clone()
            } else {
                aggregate::fold(&baseline_here)
            };

            let solved = Self::solve_one(
                &reference_bucket,
                &baseline_bucket,
                frames,
                renderer,
                Some(&frames_here),
            );
            // The bucket contributes on top of the global lean, so what is stored is the
            // difference between the two rather than the absolute answer. `LookProfile::resolve`
            // adds them back, and storing the absolute would make a bucket that happens to match
            // the global apply it twice.
            let contribution = difference(&solved, &look.global);
            let shrunk = solve::shrink(&contribution, &StyleDelta::neutral(), reference_bucket.samples);

            let mut bucket_reasons = Vec::new();
            if reference_bucket.is_weak() {
                bucket_reasons.push(LookReason::measured(
                    LookCode::BucketWeak,
                    f32::from(u16::try_from(reference_bucket.samples).unwrap_or(u16::MAX)),
                    WEAK_BUCKET_REFERENCES as f32,
                ));
            }
            if aggregate::is_incoherent(&reference_bucket) {
                bucket_reasons.push(LookReason::measured(
                    LookCode::BucketIncoherent,
                    reference_bucket.tone_spread,
                    aggregate::INCOHERENT_SPREAD,
                ));
            }

            let confidence = solve::confidence_of(&reference_bucket, &baseline_bucket);
            look.buckets.insert(
                lighting,
                LookBucket {
                    lighting,
                    reference: reference_bucket,
                    baseline: baseline_bucket,
                    delta: shrunk,
                    confidence,
                    reasons: bucket_reasons,
                },
            );
        }

        if solve::saturation_of_bounds(&look.global) >= 1.0 {
            reasons.push(LookReason::bare(LookCode::DeltaClamped));
        }

        let weak = look
            .buckets
            .values()
            .filter(|bucket| bucket.reference.is_weak())
            .count();
        look.diagnostics = LookDiagnostics {
            found: u32::try_from(reference.len()).unwrap_or(u32::MAX),
            measured: look.references,
            refused: u32::try_from(reference.len().saturating_sub(readings.len())).unwrap_or(0),
            baseline_frames: u32::try_from(baseline_readings.len()).unwrap_or(u32::MAX),
            buckets_populated: u32::try_from(look.buckets.len()).unwrap_or(0),
            buckets_weak: u32::try_from(weak).unwrap_or(0),
            strength: strength_of(readings.len(), baseline_readings.len()),
            summary: String::new(),
            reasons,
        };
        look
    }

    /// Solve one aggregate pair and refine it against whichever frames apply.
    fn solve_one(
        reference: &LookAggregate,
        baseline: &LookAggregate,
        all_frames: &[OwnFrame],
        renderer: &Renderer,
        subset: Option<&[&OwnFrame]>,
    ) -> StyleDelta {
        let initial = solve::initial(reference, baseline);
        // Nothing to render against means nothing to refine with, and the authored mapping's
        // answer ships labelled as a guess rather than quietly presented as a measurement.
        if all_frames.is_empty() {
            return initial;
        }
        match subset {
            Some([]) => initial,
            Some(frames) => {
                let probe = FrameProbe::over(renderer, frames);
                solve::refine(&initial, reference, &probe).0
            }
            None => {
                let probe = FrameProbe::new(renderer, all_frames);
                solve::refine(&initial, reference, &probe).0
            }
        }
    }
}

/// One delta minus another.
///
/// The inverse of `aura_core::contract::look::add_deltas`, so that a bucket can be stored as its
/// contribution on top of the global lean rather than as an absolute. It is here rather than in
/// the contract because subtraction is a thing this phase's solver does and not a shape anybody
/// else consumes.
#[must_use]
fn difference(whole: &StyleDelta, part: &StyleDelta) -> StyleDelta {
    let mut negated = part.clone();
    negated.exposure = -negated.exposure;
    negated.temperature_k = -negated.temperature_k;
    negated.tint = -negated.tint;
    negated.contrast = -negated.contrast;
    negated.highlights = -negated.highlights;
    negated.shadows = -negated.shadows;
    negated.whites = -negated.whites;
    negated.blacks = -negated.blacks;
    negated.vibrance = -negated.vibrance;
    negated.saturation = -negated.saturation;

    let curve = part.curve_shift.as_array();
    let mut flipped = [0.0_f32; 5];
    for (slot, value) in flipped.iter_mut().zip(curve.iter()) {
        *slot = -value;
    }
    negated.curve_shift = aura_core::contract::style::CurveShift::from_array(flipped);

    for band in aura_core::contract::colour::HslBand::ALL {
        let shift = part.hsl.get(band);
        negated.hsl.set(
            band,
            aura_core::contract::colour::HslShift {
                h: 0.0,
                s: -shift.s,
                l: -shift.l,
            },
        );
    }

    aura_core::contract::look::add_deltas(whole, &negated)
}

/// How strong a look is, `0..1`.
///
/// Two fractions multiplied rather than averaged, for the reason every geometric fusion in this
/// product is geometric: a look measured from four reference photographs is not rescued by
/// having a thousand of the photographer's own frames to be a residual from, and an average
/// would let it be.
#[must_use]
fn strength_of(references: usize, baseline_frames: usize) -> f32 {
    let usable = USABLE_REFERENCES as f32;
    let reference_weight = (references as f32 / usable).clamp(0.0, 1.0);
    let baseline_weight = (baseline_frames as f32 / usable).clamp(0.0, 1.0);
    (reference_weight * baseline_weight).clamp(0.0, 1.0)
}

/// What a look would be called when a photographer has not named it.
///
/// The origin's own title, so a look measured from `@somebody` is called `@somebody` until
/// somebody renames it. A default of "Untitled" would make a panel of four looks unreadable.
#[must_use]
pub fn default_name(origin: &ReferenceOrigin) -> String {
    origin.title()
}
