//! The frozen `CameraMatchService`, and the pass that fills it.
//!
//! Two halves, the shape phases 06 to 25 all settled on. [`CameraMatching`] answers questions about
//! transforms that already exist and is what phase 25, phase 27 and the panel hold.
//! [`MatchingPass`] produces them, and is what the job graph holds.
//!
//! ## Resumability, invariant 5
//!
//! At the level of the pass rather than the row, exactly as phase 25's is and for a stronger
//! version of the same reason: a transform is a statement about a **body**, and a project whose
//! Sony was solved against one reference and whose Canon was solved against another has been
//! matched to nothing. [`crate::camera::store::CameraStore::is_current`] asks whether the stored
//! rows came from this build's arithmetic and this policy table, and a run that was killed half way
//! through answers `false` and starts again.
//!
//! That is cheap, which is what makes it acceptable: section 11 budgets eighteen seconds for
//! fingerprinting and pairing and one second per camera for the solve, and the pass opens no pixels
//! at all - every number it reads was stored by phases 05, 07, 15, 16 and 25.
//!
//! ## What survives a re-pass
//!
//! Everything a photographer said. [`store::CameraStore::take_decisions`] reads the reference
//! choice, the switched-off bodies and the hand-set transforms out **before** the project is
//! cleared, and `restore_decisions` puts them back onto the pass's own rows before they are
//! written - which is the only ordering that is safe against a crash, because a pass that wrote its
//! answers first would leave a window in which a person's decision was gone from disk.
//!
//! ## The order inside one pass
//!
//! Fingerprint, choose the reference, pair, solve, verify, blend, measure the shooters, write. Two
//! of those orderings are load-bearing rather than convenient:
//!
//! **The reference is chosen before anything is paired**, because a pair is defined against the
//! reference body. Choosing it afterwards on the evidence would let the choice be made by whichever
//! body happened to overlap most, which is a different question from whose camera the wedding
//! should look like.
//!
//! **Verification happens before blending**, because a transform that is blended and then checked
//! is a transform whose held-out check was run against a different answer from the one that ships.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::camera::{
    Brand, CameraCode, CameraFingerprint, CameraMatchService, CameraOutline, CameraOverride,
    CameraReason, CameraTransform, FlashState, MatchedPair, Reference, ReferenceSource,
    ShooterBias, TransformSource, MIN_MATCHED_PAIRS,
};
use aura_core::contract::gallery::ImageId;
use aura_core::contract::moment::CameraId;
use aura_core::contract::tone::SkinLocus;
use aura_core::errors::ml::camera_decision_refused;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraError, AuraResult, ProjectId};

use super::baseline::{self, Library};
use super::fingerprint::{self, CameraFrame};
use super::policy::Matching;
use super::store::{CameraStore, Decisions, PassWrite};
use super::transform::{Field, PairReading};
use super::{errors, pairs, shooter, solve, transform, ANALYSIS_VER};

/// The one implementation of the frozen service.
///
/// Holds a store and the policy table the stored rows were bounded by, and nothing else.
#[derive(Debug, Clone)]
pub struct CameraMatching {
    store: CameraStore,
    policy: Matching,
    library: Library,
}

impl CameraMatching {
    /// Wrap a catalog with the bundled policy table and baseline library.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self {
            store: CameraStore::new(catalog, clock),
            policy: Matching::default(),
            library: Library::bundled(),
        }
    }

    /// Wrap a catalog with a studio's own table.
    #[must_use]
    pub fn with_policy(
        catalog: Arc<Catalog>,
        clock: Arc<dyn Clock>,
        policy: Matching,
        library: Library,
    ) -> Self {
        Self {
            store: CameraStore::new(catalog, clock),
            policy,
            library,
        }
    }

    /// The store underneath, for the gate and the budget test.
    #[must_use]
    pub fn store(&self) -> &CameraStore {
        &self.store
    }

    /// The policy table the stored rows were bounded by.
    #[must_use]
    pub fn policy(&self) -> &Matching {
        &self.policy
    }

    /// The brand baselines this build knows about.
    #[must_use]
    pub fn library(&self) -> &Library {
        &self.library
    }

    /// Which manufacturers this project uses that this build has no baseline for.
    fn unknown_brands(&self, project: ProjectId) -> Vec<String> {
        let Ok(prints) = self.store.fingerprints(project) else {
            return Vec::new();
        };
        let mut out: BTreeSet<String> = BTreeSet::new();
        for print in prints {
            if !self.library.knows(print.brand) {
                out.insert(print.brand.as_str().to_string());
            }
        }
        out.into_iter().collect()
    }
}

impl CameraMatchService for CameraMatching {
    fn outline(&self, project: ProjectId) -> AuraResult<CameraOutline> {
        let mut outline = self.store.outline(project, self.unknown_brands(project))?;
        outline.policy_ver = self.policy.version;
        // The two signature figures cannot be recovered from a table of transforms - a disabled
        // body is in the before and not the after - so the pass records them and an outline read
        // without one carries zeroes and a reduction of zero. `signature_reduction` returns 0.0 on
        // a zero baseline rather than 1.0, which is what stops "we have not measured" reading as
        // "we removed all of it". Phase 25's shape.
        if outline.signature_before <= f32::EPSILON {
            outline.signature_before = 0.0;
            outline.signature_after = 0.0;
        }
        Ok(outline)
    }

    fn fingerprints(&self, project: ProjectId) -> AuraResult<Vec<CameraFingerprint>> {
        self.store.fingerprints(project)
    }

    fn fingerprint(
        &self,
        project: ProjectId,
        camera: &CameraId,
        flash: FlashState,
    ) -> AuraResult<Option<CameraFingerprint>> {
        Ok(self
            .store
            .fingerprints(project)?
            .into_iter()
            .find(|print| &print.camera_id == camera && print.flash == flash))
    }

    fn transforms(&self, project: ProjectId) -> AuraResult<Vec<CameraTransform>> {
        self.store.transforms(project)
    }

    fn transform(
        &self,
        project: ProjectId,
        camera: &CameraId,
        flash: FlashState,
    ) -> AuraResult<Option<CameraTransform>> {
        self.store.transform(project, camera, flash)
    }

    fn transform_for_image(&self, image: ImageId) -> AuraResult<Option<CameraTransform>> {
        self.store.transform_for_image(image)
    }

    fn pairs(
        &self,
        project: ProjectId,
        camera: &CameraId,
        limit: usize,
    ) -> AuraResult<Vec<MatchedPair>> {
        self.store.pairs(project, camera, limit)
    }

    fn shooter_bias(&self, project: ProjectId) -> AuraResult<Vec<ShooterBias>> {
        self.store.shooter_bias(project)
    }

    fn reference(&self, project: ProjectId) -> AuraResult<Option<Reference>> {
        self.store.reference(project)
    }

    fn set_reference(&self, project: ProjectId, camera: &CameraId) -> Result<(), AuraError> {
        let transforms = self.store.transforms(project)?;
        let known = transforms.iter().any(|t| &t.camera_id == camera);
        if !known {
            return Err(camera_decision_refused(format!(
                "camera {camera} has no transform in this project"
            )));
        }
        let frames = self
            .store
            .fingerprints(project)?
            .into_iter()
            .filter(|print| &print.camera_id == camera)
            .map(|print| print.samples)
            .sum::<u32>();
        if frames == 0 {
            return Err(camera_decision_refused(format!(
                "camera {camera} shot no measurable photographs, so it cannot be the reference \
                 everything else is matched to"
            )));
        }
        self.store.set_reference(project, camera, frames, None)?;
        Ok(())
    }

    fn set_enabled(
        &self,
        project: ProjectId,
        camera: &CameraId,
        enabled: bool,
    ) -> Result<(), AuraError> {
        let changed = self.store.set_enabled(project, camera, enabled)?;
        if changed == 0 {
            return Err(camera_decision_refused(format!(
                "camera {camera} has no transform in this project"
            )));
        }
        Ok(())
    }

    fn set_override(
        &self,
        project: ProjectId,
        camera: &CameraId,
        flash: FlashState,
        values: CameraOverride,
    ) -> Result<(), AuraError> {
        if values.is_empty() {
            return Err(camera_decision_refused(
                "an override that sets nothing would take a camera out of automation without \
                 changing anything about it",
            ));
        }
        if !values.within_bounds() {
            return Err(camera_decision_refused(
                "a value is outside the movement AURA will make on a whole camera; a camera that \
                 needs to move further is a camera whose per-frame estimates are wrong",
            ));
        }
        let changed = self.store.set_override(
            project,
            camera,
            flash,
            [
                values.d_cct,
                values.d_tint,
                values.d_exposure,
                values.d_saturation,
            ],
        )?;
        if changed == 0 {
            return Err(camera_decision_refused(format!(
                "camera {camera} has no {flash} transform in this project"
            )));
        }
        Ok(())
    }
}

/// What one matching pass did, for the gate, the CLI and the telemetry.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MatchReport {
    /// Bodies seen.
    pub cameras: u32,
    /// The reference body, when one could be chosen.
    pub reference: Option<CameraId>,
    /// How the reference was chosen.
    pub reference_source: ReferenceSource,
    /// Verified pairs found.
    pub pairs: u32,
    /// Candidate pairs rejected because their backgrounds disagreed.
    pub pairs_rejected: u32,
    /// Pairs held out of every fit.
    pub heldout_pairs: u32,
    /// Transforms solved on this wedding's own pairs.
    pub solved: u32,
    /// Transforms blended toward a baseline.
    pub blended: u32,
    /// Transforms that are a baseline alone.
    pub baseline_only: u32,
    /// Fits thrown away because they did not improve held-out evidence.
    pub heldout_failures: u32,
    /// The mean appearance distance between the non-reference bodies and the reference, before.
    pub distance_before: f32,
    /// The same after.
    pub distance_after: f32,
    /// The mean grade-signature distance before.
    pub signature_before: f32,
    /// The same after.
    pub signature_after: f32,
    /// The worst per-body skin dE00 after matching.
    pub worst_skin_de00: f32,
    /// Shooter habits measured.
    pub shooters_measured: u32,
    /// Shooter corrections a cap reduced.
    pub shooters_capped: u32,
}

impl MatchReport {
    /// The share of the appearance distance the pass removed, `0..1`.
    #[must_use]
    pub fn distance_reduction(&self) -> f32 {
        if self.distance_before <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - self.distance_after / self.distance_before).clamp(0.0, 1.0)
    }

    /// The share of the grade-signature distance the pass removed, `0..1`.
    ///
    /// Section 10.1's second gate.
    #[must_use]
    pub fn signature_reduction(&self) -> f32 {
        if self.signature_before <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - self.signature_after / self.signature_before).clamp(0.0, 1.0)
    }
}

/// The resumable pass that fills the service.
#[derive(Clone)]
pub struct MatchingPass {
    store: CameraStore,
    policy: Matching,
    library: Library,
}

impl fmt::Debug for MatchingPass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MatchingPass")
            .field("policy_ver", &self.policy.version)
            .field("baselines", &self.library.len())
            // `store` is a catalog handle; printing it would print a connection pool.
            // `finish_non_exhaustive` says the struct is not fully described rather than implying
            // it is.
            .finish_non_exhaustive()
    }
}

impl MatchingPass {
    /// A pass with the bundled policy table and baseline library.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self {
            store: CameraStore::new(catalog, clock),
            policy: Matching::default(),
            library: Library::bundled(),
        }
    }

    /// A pass with a studio's own table.
    #[must_use]
    pub fn with_policy(
        catalog: Arc<Catalog>,
        clock: Arc<dyn Clock>,
        policy: Matching,
        library: Library,
    ) -> Self {
        Self {
            store: CameraStore::new(catalog, clock),
            policy,
            library,
        }
    }

    /// The policy table this pass bounds its answers by.
    #[must_use]
    pub fn policy(&self) -> &Matching {
        &self.policy
    }

    /// The store underneath.
    #[must_use]
    pub fn store(&self) -> &CameraStore {
        &self.store
    }

    /// True when a project's stored rows came from this build and this policy.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn is_current(&self, project: ProjectId) -> AuraResult<bool> {
        self.store
            .is_current(project, (ANALYSIS_VER, self.policy.version))
    }

    /// Raise `AURA-ML-5132` when a project's rows came from different arithmetic.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5132` when the versions differ; `AURA-DB-3006` when the query fails.
    pub fn check_versions(&self, project: ProjectId) -> AuraResult<()> {
        let current = (ANALYSIS_VER, self.policy.version);
        if let Some(stored) = self.store.stored_versions(project)? {
            if stored != current {
                return Err(errors::version_drift(stored, current));
            }
        }
        Ok(())
    }

    /// Run the whole matching pass over one project's frames.
    ///
    /// `loci` are phase 15's per-identity skin loci, which are the hard constraint of section 6.2.
    /// An empty list admits every candidate, which is this build on a real photograph.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5130` when the pass cannot run; `AURA-DB-3006` when a statement fails.
    // One function, deliberately, and for phase 25's reason: every stage reads what the stage
    // before it produced - the fingerprints feed the pairing, the pairs feed the solve, the solve
    // feeds the blend, the blend feeds the shooter correction - and splitting it would mean passing
    // eight intermediate collections between private functions with no other caller. The modules are
    // where this phase is decomposed.
    #[allow(clippy::too_many_lines)]
    pub fn run(
        &self,
        project: ProjectId,
        frames: &[CameraFrame],
        loci: &[SkinLocus],
        progress: &dyn ProgressSink,
        cancel: &CancelToken,
    ) -> AuraResult<MatchReport> {
        let decisions = self.store.take_decisions(project)?;
        let mut report = MatchReport::default();

        let groups = fingerprint::group(frames);
        if groups.is_empty() {
            return Err(errors::pass_failed(
                "no photograph in this project names a camera",
            ));
        }

        // ---- 1. fingerprint every body, in both flash states ---------------------------
        progress.report(ProgressUpdate {
            stage: super::FINGERPRINT_STAGE,
            done: 0,
            total: groups.len() as u64,
            current: None,
        });
        let mut fingerprints: Vec<CameraFingerprint> = Vec::new();
        let bodies: BTreeSet<String> = groups
            .keys()
            .map(|(camera, _)| camera.as_str().to_string())
            .collect();
        for ((camera, flash), indices) in &groups {
            if cancel.is_cancelled() {
                return Err(errors::pass_failed("cancelled during fingerprinting"));
            }
            let split = FlashState::ALL
                .iter()
                .all(|state| groups.contains_key(&(camera.clone(), *state)));
            let subset: Vec<&CameraFrame> = indices
                .iter()
                .filter_map(|index| frames.get(*index))
                .collect();
            let brand = subset.first().map_or(Brand::default(), |frame| frame.brand);
            if let Some(print) = fingerprint::measure(camera, *flash, brand, &subset, split) {
                fingerprints.push(print);
            }
        }
        report.cameras = u32::try_from(bodies.len()).unwrap_or(0);

        // ---- 2. choose the reference, before anything is paired -----------------------
        let reference = choose_reference(project, frames, &groups, &decisions);
        let Some(reference) = reference else {
            return Err(errors::pass_failed(
                "no camera in this project shot a measurable photograph",
            ));
        };
        report.reference = Some(reference.camera_id.clone());
        report.reference_source = reference.source;

        // ---- 3. pair, solve, verify, blend --------------------------------------------
        progress.report(ProgressUpdate {
            stage: super::MATCH_STAGE,
            done: 0,
            total: bodies.len() as u64,
            current: None,
        });
        let mut all_pairs: Vec<MatchedPair> = Vec::new();
        let mut transforms: Vec<CameraTransform> = Vec::new();
        let reference_brand = fingerprints
            .iter()
            .find(|print| print.camera_id == reference.camera_id)
            .map_or(Brand::default(), |print| print.brand);

        let mut distance_before = 0.0_f32;
        let mut distance_after = 0.0_f32;
        let mut signature_before = 0.0_f32;
        let mut signature_after = 0.0_f32;
        let mut measured = 0_u32;

        for body in &bodies {
            if cancel.is_cancelled() {
                return Err(errors::pass_failed("cancelled during matching"));
            }
            let camera = CameraId::new(body.clone());
            let is_reference = camera == reference.camera_id;

            let mut body_pairs = if is_reference {
                Vec::new()
            } else {
                pairs::find(frames, &reference.camera_id, &camera, &self.policy)
            };
            pairs::split_heldout(&mut body_pairs);

            for flash in FlashState::ALL {
                if is_reference {
                    let mut identity = CameraTransform::identity(
                        camera.clone(),
                        flash,
                        reference.camera_id.clone(),
                        ANALYSIS_VER,
                        self.policy.version,
                    );
                    identity.confidence = 1.0;
                    identity.reasons = vec![
                        CameraReason::of(CameraCode::IsReference),
                        CameraReason::of(reference.source.code()),
                    ];
                    transforms.push(identity);
                    continue;
                }

                let body_brand = fingerprints
                    .iter()
                    .find(|print| print.camera_id == camera && print.flash == flash)
                    .map_or(Brand::default(), |print| print.brand);

                let (fitting, heldout) = readings_for(frames, &body_pairs, flash);
                let mut solved = self.solve_one(
                    &camera,
                    flash,
                    &reference,
                    reference_brand,
                    body_brand,
                    &fitting,
                    &heldout,
                    loci,
                    &mut report,
                );

                // The channel gains are derived from the two fingerprints rather than fitted; see
                // `solve::channel_gain`. A body with no fingerprint keeps unity gains, which is the
                // honest answer when there is nothing to derive from.
                if let (Some(reference_print), Some(body_print)) = (
                    fingerprints
                        .iter()
                        .find(|p| p.camera_id == reference.camera_id && p.flash == flash),
                    fingerprints
                        .iter()
                        .find(|p| p.camera_id == camera && p.flash == flash),
                ) {
                    solved.channel_gain =
                        solve::channel_gain(reference_print, body_print, &self.policy);
                } else if !fingerprints.iter().any(|p| p.camera_id == camera) {
                    solved
                        .reasons
                        .push(CameraReason::of(CameraCode::FingerprintAbsent));
                }

                if !fitting.is_empty() {
                    let before = transform::measure(&fitting, None);
                    let after = transform::measure(&fitting, Some(&solved));
                    distance_before += before.total();
                    distance_after += after.total();
                    signature_before += before.grade_signature;
                    signature_after += after.grade_signature;
                    measured += 1;
                }

                transforms.push(solved);
            }

            all_pairs.extend(body_pairs);
        }

        let (verified, held, rejected) = pairs::counts(&all_pairs);
        report.pairs = verified;
        report.heldout_pairs = held;
        report.pairs_rejected = rejected;
        if measured > 0 {
            let n = f64::from(measured) as f32;
            report.distance_before = distance_before / n;
            report.distance_after = distance_after / n;
            report.signature_before = signature_before / n;
            report.signature_after = signature_after / n;
        }

        // ---- 4. the shooters ----------------------------------------------------------
        let shooter_rows = shooter::measure(frames, &reference.camera_id, &self.policy);
        let (shooters_measured, shooters_capped) = shooter::counts(&shooter_rows);
        report.shooters_measured = shooters_measured;
        report.shooters_capped = shooters_capped;
        for transform in &mut transforms {
            if transform.camera_id == reference.camera_id {
                continue;
            }
            let folded = shooter::folded_ev(&shooter_rows, &transform.camera_id);
            if folded.abs() > f32::EPSILON {
                // The habit joins the metering difference on the one exposure axis a transform
                // has, then the whole thing is re-bounded - a correction that was already at the
                // ceiling does not get to exceed it because a second cause was added to it.
                let ceiling = self
                    .policy
                    .bound(aura_core::contract::camera::TransformBound::Exposure);
                transform.d_exposure = (transform.d_exposure + folded).clamp(-ceiling, ceiling);
                transform
                    .reasons
                    .push(CameraReason::of(CameraCode::ShooterBiasCorrected));
            }
        }

        // ---- 5. tally and write -------------------------------------------------------
        for transform in &transforms {
            match transform.source {
                TransformSource::MatchedPairs if transform.camera_id != reference.camera_id => {
                    report.solved += 1;
                }
                TransformSource::Blended => report.blended += 1,
                TransformSource::BrandBaseline if transform.camera_id != reference.camera_id => {
                    report.baseline_only += 1;
                }
                TransformSource::MatchedPairs | TransformSource::BrandBaseline => {}
            }
            if transform.camera_id != reference.camera_id {
                report.worst_skin_de00 = report
                    .worst_skin_de00
                    .max(transform.skin_correction.de00_after);
            }
        }

        let mut write = PassWrite {
            reference: Some(reference),
            fingerprints,
            transforms,
            pairs: all_pairs,
            shooter_bias: shooter_rows,
        };
        for transform in &mut write.transforms {
            transform.reasons = dedupe(&transform.reasons);
            if !transform.within_bounds() {
                return Err(errors::pass_failed(format!(
                    "solved transform for {} is outside its bounds; refusing to write it",
                    transform.camera_id
                )));
            }
        }
        CameraStore::restore_decisions(&mut write, &decisions);
        self.store.write_pass(project, &write)?;

        if report.baseline_only > 0 {
            progress.report(ProgressUpdate {
                stage: super::FALLBACK_STAGE,
                done: u64::from(report.baseline_only),
                total: u64::from(report.cameras),
                current: None,
            });
        }
        Ok(report)
    }

    /// Solve one body in one flash state: fit, verify, blend, in that order.
    #[allow(clippy::too_many_arguments)]
    // The whole of one body's answer: fingerprint, pairs, fit, held-out check, blend and reasons.
    // Splitting it would separate the fit from the check that decides whether to keep it, which is
    // the one pairing in this phase that must be read together.
    #[allow(clippy::too_many_lines)]
    fn solve_one(
        &self,
        camera: &CameraId,
        flash: FlashState,
        reference: &Reference,
        reference_brand: aura_core::contract::camera::Brand,
        body_brand: aura_core::contract::camera::Brand,
        fitting: &[PairReading],
        heldout: &[PairReading],
        loci: &[SkinLocus],
        report: &mut MatchReport,
    ) -> CameraTransform {
        let fallback = || {
            solve::from_baseline(
                camera,
                flash,
                &reference.camera_id,
                reference_brand,
                body_brand,
                &self.library,
                &self.policy,
            )
        };

        let evidence = u32::try_from(fitting.len()).unwrap_or(0);
        let Some(fit) = solve::fit(
            camera,
            flash,
            &reference.camera_id,
            fitting,
            loci,
            &self.policy,
        ) else {
            let mut out = fallback();
            out.reasons.push(CameraReason::of(if evidence == 0 {
                CameraCode::PairsAbsent
            } else {
                CameraCode::PairsInsufficient
            }));
            return out;
        };

        // Verification before blending. A transform that is blended and then checked is a
        // transform whose held-out check was run against a different answer from the one that
        // ships.
        let (heldout_before, heldout_after, verdict) = solve::verify(&fit, heldout);
        if verdict == Some(false) {
            report.heldout_failures += 1;
            let mut out = fallback();
            out.reasons
                .push(CameraReason::of(CameraCode::HeldOutFailed));
            out.evidence_pairs = evidence;
            out.heldout_before = heldout_before;
            out.heldout_after = heldout_after;
            out.heldout_pairs = u32::try_from(heldout.len()).unwrap_or(0);
            out.skin_correction.de00_before = fit.before.skin_de00;
            out.skin_correction.de00_after = fit.before.skin_de00;
            tracing::warn!(
                camera = camera.as_str(),
                flash = flash.as_str(),
                "a solved camera transform did not improve held-out evidence; falling back on the \
                 brand baseline"
            );
            return out;
        }

        let mut out = fit.transform.clone();
        out.evidence_pairs = evidence;
        out.distance_before = fit.before;
        out.distance_after = fit.after;
        out.heldout_before = heldout_before;
        out.heldout_after = heldout_after;
        out.heldout_pairs = u32::try_from(heldout.len()).unwrap_or(0);
        out.bounded = fit.bounded;
        out.skin_correction = solve::skin_report(&fit.transform, fit.before, fit.after);

        let weight = solve::evidence_weight(evidence, &self.policy);
        let (departure, _) = baseline::between(&self.library, body_brand, reference_brand, flash);
        solve::blend(&mut out, departure, weight);

        let mut reasons = vec![CameraReason::of(CameraCode::PairsFound)];
        match out.source {
            TransformSource::MatchedPairs => {
                reasons.push(CameraReason::of(CameraCode::SolvedFromPairs));
            }
            TransformSource::Blended => {
                reasons.push(CameraReason::of(CameraCode::BlendedWithBaseline));
                reasons.push(CameraReason::of(CameraCode::PairsInsufficient));
            }
            TransformSource::BrandBaseline => {
                reasons.push(CameraReason::of(CameraCode::BaselineOnly));
            }
        }
        if verdict == Some(true) {
            reasons.push(CameraReason::of(CameraCode::HeldOutImproved));
        }
        if fit.bounded.is_some() {
            reasons.push(CameraReason::of(CameraCode::BoundedByPolicy));
        }
        if fit.locus_refusals > 0 {
            reasons.push(CameraReason::of(CameraCode::SkinLocusRefused));
        }
        if out.is_identity() {
            reasons.push(CameraReason::of(CameraCode::AlreadyMatched));
        } else {
            if out.d_cct.abs() >= 15.0 || out.d_tint.abs() >= 0.5 {
                reasons.push(CameraReason::of(CameraCode::WhitePointMatched));
            }
            if out.skin_correction.de00_after < out.skin_correction.de00_before {
                reasons.push(CameraReason::of(CameraCode::SkinMatched));
            }
            if out.d_saturation.abs() >= 0.5
                || out.contrast_shape.iter().any(|c| (c - 1.0).abs() >= 0.01)
            {
                reasons.push(CameraReason::of(CameraCode::GradeMatched));
            }
        }
        out.reasons = reasons;

        // Three terms, multiplied, so no term can rescue another - the shape phase 12 established
        // for its keep score and phase 25 for its anchor ranking. How much evidence there was, how
        // much of the difference the transform actually removed, and whether the held-out check
        // passed at all.
        let evidence_term = (f64::from(evidence) / f64::from(MIN_MATCHED_PAIRS.max(1))) as f32;
        let improvement = fit.before.reduction_to(&fit.after);
        let verified_term = match verdict {
            Some(true) => 1.0,
            Some(false) => 0.0,
            // An unchecked fit is worth less than a checked one and more than a failed one. It is
            // not zero, because the fit itself is real evidence; it is not one, because nobody
            // looked.
            None => 0.55,
        };
        out.confidence =
            (evidence_term.clamp(0.0, 1.0) * improvement.max(0.15) * verified_term).clamp(0.0, 1.0);
        out
    }
}

/// The reference body, chosen before any pair is formed.
///
/// Section 2.1's three policies in the order they are tried: a photographer's choice, then the body
/// labelled as the primary shooter's, then the body with the most frames. The order is the product
/// decision of section 9's PM row - a shooter label beats a frame count because a lead who shot
/// four hundred frames while the second shot two thousand is still the lead, and their look is the
/// one the gallery should have.
fn choose_reference(
    project: ProjectId,
    frames: &[CameraFrame],
    groups: &BTreeMap<fingerprint::Key, Vec<usize>>,
    decisions: &Decisions,
) -> Option<Reference> {
    let mut counts: BTreeMap<String, u32> = BTreeMap::new();
    let mut labels: BTreeMap<String, (CameraId, String)> = BTreeMap::new();
    for ((camera, _), indices) in groups {
        let key = camera.as_str().to_string();
        *counts.entry(key.clone()).or_default() += u32::try_from(indices.len()).unwrap_or(0);
        if let Some(frame) = indices.first().and_then(|index| frames.get(*index)) {
            labels
                .entry(key)
                .or_insert_with(|| (camera.clone(), frame.shooter.clone()));
        }
    }
    if counts.is_empty() {
        return None;
    }

    let build = |key: &str, source: ReferenceSource| -> Option<Reference> {
        let (camera, shooter) = labels.get(key)?.clone();
        Some(Reference {
            project,
            camera_id: camera,
            source,
            frames: counts.get(key).copied().unwrap_or(0),
            shooter: Some(shooter),
        })
    };

    if let Some(chosen) = decisions.reference.as_ref() {
        if let Some(reference) = build(chosen.as_str(), ReferenceSource::User) {
            return Some(reference);
        }
    }

    // The primary shooter's body, and the busiest of them when they carried two.
    let primary = labels
        .iter()
        .filter(|(_, (_, shooter))| shooter == "primary")
        .max_by_key(|(key, _)| counts.get(*key).copied().unwrap_or(0))
        .map(|(key, _)| key.clone());
    if let Some(key) = primary {
        if let Some(reference) = build(&key, ReferenceSource::PrimaryShooter) {
            return Some(reference);
        }
    }

    let busiest = counts
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(key, _)| key.clone())?;
    build(&busiest, ReferenceSource::FrameCount)
}

/// Split a body's pairs into fitting and held-out readings, for one flash state.
fn readings_for(
    frames: &[CameraFrame],
    body_pairs: &[MatchedPair],
    flash: FlashState,
) -> (Vec<PairReading>, Vec<PairReading>) {
    let by_image: BTreeMap<ImageId, &CameraFrame> =
        frames.iter().map(|frame| (frame.image, frame)).collect();
    let mut fitting = Vec::new();
    let mut heldout = Vec::new();
    for pair in body_pairs.iter().filter(|pair| pair.flash == flash) {
        let (Some(left), Some(right)) = (by_image.get(&pair.left), by_image.get(&pair.right))
        else {
            continue;
        };
        let Some(reading) = PairReading::of(left, right) else {
            continue;
        };
        if pair.is_heldout() {
            heldout.push(reading);
        } else if pair.is_fitting() {
            fitting.push(reading);
        }
    }
    (fitting, heldout)
}

/// Collapse a reason list, keeping the strongest first and each code once.
fn dedupe(reasons: &[CameraReason]) -> Vec<CameraReason> {
    let bits = CameraReason::to_bits(reasons);
    CameraReason::from_bits(bits)
}

/// Build the field phase 25 reads, from a project's stored transforms.
///
/// **Section 6.4's ordering, as one call.** The caller hands it to
/// `crate::api::collect_frames`, which folds each frame's transform into the `Frame` it builds -
/// so the consistency pass's tree, change points, anchors and targets are all computed over
/// already-comparable numbers.
///
/// # Errors
///
/// `AURA-DB-3006` when the transforms cannot be read.
pub fn field_for(
    service: &dyn CameraMatchService,
    project: ProjectId,
    frames: &[(ImageId, CameraId, FlashState)],
) -> AuraResult<Field> {
    let transforms = service.transforms(project)?;
    Ok(Field::from_transforms(&transforms, frames))
}
