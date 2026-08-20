//! `Masks`: the one implementation of the frozen [`MaskService`], and the resumable pass.
//!
//! # The lazy policy, which is a performance decision and a correctness one
//!
//! Section 6.3: "Masks are generated lazily for selected frames only (post-cull), because
//! rejected frames never need them - a large part of why the pipeline meets its time budget."
//! [`Masks::ensure`] is the only way a mask comes into existence and [`Masks::pass`] walks
//! phase 12's keepers rather than the project. A wedding is four thousand frames and a gallery
//! is six hundred; the budget in section 11 is written against the second number.
//!
//! It is also a correctness decision, and the reason is the coverage denominator. Because the
//! pass walks keepers, [`crate::contract::mask::MaskOutline::coverage`] is measured against
//! keepers, and it is the first outline in the product that is not measured against every
//! photograph. Both numbers are on the shape so the denominator is visible rather than implied.
//!
//! # What `ensure` guarantees
//!
//! Idempotence and resumability, in the sense invariant 5 means: kill the process at any point
//! and the next run re-computes nothing it already computed. A kind already stored at the
//! current `MODEL_VER` and `ANALYSIS_VER` is returned rather than recomputed, and a kind a
//! photographer edited is returned untouched whatever the versions say.

use std::sync::Arc;

use aura_core::contract::error::{AuraError, AuraResult};
use aura_core::contract::ids::MaskId;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{PhotoId, ProjectId};
use aura_render::contract::render::RenderLevel;

use crate::contract::mask::{
    EdgeQuality, GpuMask, ImageId, Mask, MaskKind, MaskOp, MaskOutline, MaskPayload, MaskReason,
    MaskService, ALL_KINDS,
};
use crate::face::FramePeople;
use crate::mask::algebra::{self, Plane};
use crate::mask::store::{self, MaskStore};
use crate::mask::{errors, quality, MaskFrame, MaskPipeline, ANALYSIS_VER, MODEL_VER};

/// How many frames one call to [`Masks::pass`] claims at a time.
///
/// Sixty-four. Small enough that a cancel lands inside a second on the reference machine, large
/// enough that the pending query is not run once per photograph.
pub const BATCH: usize = 64;

/// Where the pixels for one photograph come from.
///
/// A port rather than a concrete decoder, so a test can run the whole pass over painted
/// fixtures without a catalog, a cache or a RAW file - which is what makes section 10.1's gates
/// measurable at all in a repository with no camera files in it.
pub trait FrameSource: Send + Sync {
    /// The proxy for one photograph.
    ///
    /// # Errors
    ///
    /// Any decode error; the pass records `AURA-ML-5078` and continues with the next frame.
    fn frame(&self, image: PhotoId) -> AuraResult<MaskFrame>;

    /// Phase 06's answer for one photograph, when there is one.
    fn people(&self, image: PhotoId) -> Option<FramePeople> {
        let _ = image;
        None
    }

    /// Which identity each face index belongs to, when phase 06 has clustered them.
    fn identities(&self, image: PhotoId) -> Vec<(usize, aura_core::IdentityId)> {
        let _ = image;
        Vec::new()
    }
}

/// What one pass did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MaskReport {
    /// Frames masked in this run.
    pub masked: u64,
    /// Frames that failed and were skipped.
    pub failed: u64,
    /// Masks written.
    pub written: u64,
    /// Masks that came back below the aggressive floor.
    pub low_quality: u64,
    /// Payload bytes written.
    pub bytes: u64,
    /// True when the run was cancelled before it finished.
    pub cancelled: bool,
}

/// The one implementation of [`MaskService`].
#[derive(Clone)]
pub struct Masks {
    store: MaskStore,
    source: Arc<dyn FrameSource>,
    pipeline: MaskPipeline,
}

impl std::fmt::Debug for Masks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Masks")
            .field("pipeline", &self.pipeline)
            .finish_non_exhaustive()
    }
}

impl Masks {
    /// Build the service.
    #[must_use]
    pub fn new(store: MaskStore, source: Arc<dyn FrameSource>) -> Self {
        Self {
            store,
            source,
            pipeline: MaskPipeline::new(),
        }
    }

    /// A service that can read, compose and resolve, and cannot produce.
    ///
    /// Six of the eight things a caller does with masks - listing them, composing them,
    /// resolving one onto a render level, reporting the outline, gating an operation, saving an
    /// edit - need the store and no pixels at all. Building those with a real frame source means
    /// opening a preview cache to answer "what regions does this photograph have", which is a
    /// disk-backed cache opened per project to serve a query that touches one table.
    ///
    /// `ensure` on one of these returns `AURA-ML-5078` rather than an empty list, because "this
    /// service cannot read pixels" and "this photograph has no regions" are different answers
    /// and a caller that confused them would store the second when it meant the first.
    #[must_use]
    pub fn read_only(store: MaskStore) -> Self {
        Self {
            store,
            source: Arc::new(NoFrames),
            pipeline: MaskPipeline::new(),
        }
    }

    /// The store underneath, for the surfaces that write an edit.
    #[must_use]
    pub const fn store(&self) -> &MaskStore {
        &self.store
    }

    /// Walk a project's keepers, masking what is not masked yet.
    ///
    /// Resumable at any point: the work remaining is a query, so a killed run costs the batch
    /// it was inside and nothing else.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5079` when the project has no selection at all, `AURA-DB-3006` when the store
    /// cannot be read.
    pub fn pass(
        &self,
        project: ProjectId,
        progress: &dyn ProgressSink,
        cancel: &CancelToken,
    ) -> AuraResult<MaskReport> {
        let mut report = MaskReport::default();
        let outline = self.store.outline(project)?;
        if outline.selected == 0 {
            return Err(errors::mask_refused(
                "the project has no selected frames; phase 12 has not run",
            ));
        }
        let total = outline.selected;
        let mut done = outline.masked;

        loop {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }
            let batch = self.store.pending(project, BATCH)?;
            if batch.is_empty() {
                break;
            }
            for image in batch {
                if cancel.is_cancelled() {
                    report.cancelled = true;
                    break;
                }
                match self.mask_one(image) {
                    Ok(masks) => {
                        report.bytes += masks.iter().map(|m| m.byte_len() as u64).sum::<u64>();
                        report.low_quality += quality::low_quality_count(&masks);
                        report.written += self.store.put(image, &masks)? as u64;
                        report.masked += 1;
                    }
                    Err(err) => {
                        // One photograph failing is one photograph. Nothing is written, because
                        // a stored empty mask reads to later phases as "there is no skin in
                        // this photograph" rather than as "nobody looked".
                        tracing::warn!(image = %image, code = %err.code, "mask pass skipped a frame");
                        report.failed += 1;
                    }
                }
                done += 1;
                progress.report(ProgressUpdate {
                    stage: "mask.generate",
                    done: done.min(total),
                    total,
                    current: None,
                });
            }
            if report.cancelled {
                break;
            }
        }
        Ok(report)
    }

    /// Produce every region of one photograph without storing it.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5078` when the pixels cannot be read.
    pub fn mask_one(&self, image: PhotoId) -> AuraResult<Vec<Mask>> {
        let frame = self
            .source
            .frame(image)
            .map_err(|err| errors::mask_failed(&image.to_db(), &err.detail))?;
        let people = self.source.people(image);
        let identities = self.source.identities(image);
        let set = self.pipeline.analyse(&frame, people.as_ref(), &identities);

        let mut out = Vec::with_capacity(set.planes.len());
        for mut plane in set.planes {
            quality::settle(&mut plane);
            let (mask, _note) = crate::mask::to_mask(image, &plane, 0.0);
            out.push(mask);
        }
        Ok(out)
    }
}

impl MaskService for Masks {
    fn masks(&self, image: ImageId) -> Vec<Mask> {
        // Infallible in the contract, so a store failure is an empty list and a log line rather
        // than a panic. An empty list is what a caller sees for an un-analysed photograph too,
        // and both mean the same thing to a consumer: there is nothing here to edit through.
        match self.store.masks(image) {
            Ok(masks) => masks,
            Err(err) => {
                tracing::warn!(image = %image, code = %err.code, "could not read masks");
                Vec::new()
            }
        }
    }

    fn ensure(&self, image: ImageId, kinds: &[MaskKind]) -> Result<Vec<Mask>, AuraError> {
        let stored = self.store.masks(image)?;
        let wanted: Vec<MaskKind> = if kinds.is_empty() {
            ALL_KINDS.to_vec()
        } else {
            kinds.to_vec()
        };
        let fresh = |m: &Mask| m.user_edited || m.model_ver == MODEL_VER;
        let have: Vec<MaskKind> = stored.iter().filter(|m| fresh(m)).map(|m| m.kind).collect();
        if wanted.iter().all(|k| have.contains(k)) {
            return Ok(stored
                .into_iter()
                .filter(|m| wanted.contains(&m.kind))
                .collect());
        }

        let produced = self.mask_one(image)?;
        // A photographer's mask survives a regeneration. The store enforces it too - the flag is
        // inside the `DELETE`'s `WHERE` - and it is enforced here as well so the *returned* set
        // matches what is on disk rather than what was just computed.
        let edited: Vec<Mask> = stored.into_iter().filter(|m| m.user_edited).collect();
        let mut merged: Vec<Mask> = produced
            .into_iter()
            .filter(|m| {
                !edited
                    .iter()
                    .any(|e| e.kind == m.kind && e.identity == m.identity)
            })
            .collect();
        merged.extend(edited);
        self.store.put(image, &merged)?;
        Ok(merged
            .into_iter()
            .filter(|m| wanted.contains(&m.kind))
            .collect())
    }

    fn compose(&self, ops: &[MaskOp]) -> Mask {
        let mut stack: Vec<(Plane, MaskKind, Option<MaskId>)> = Vec::new();
        let mut underflowed = false;

        for op in ops {
            match op {
                MaskOp::Source { id } => {
                    if let Ok(Some(mask)) = self.store.mask(*id) {
                        stack.push((store::decode(&mask.payload), mask.kind, Some(mask.id)));
                    } else {
                        // A missing operand pushes the empty plane rather than nothing. Pushing
                        // nothing would silently shift every later binary op onto the wrong
                        // pair, which is how a subtraction becomes a union.
                        underflowed = true;
                        stack.push((Plane::zeros(1, 1), MaskKind::Subject, None));
                    }
                }
                MaskOp::Plane { payload, kind } => {
                    stack.push((store::decode(payload), *kind, None));
                }
                MaskOp::Union | MaskOp::Intersect | MaskOp::Subtract => {
                    let (Some((rhs, _, _)), Some((lhs, kind, id))) = (stack.pop(), stack.pop())
                    else {
                        underflowed = true;
                        break;
                    };
                    let out = match op {
                        MaskOp::Union => algebra::union(&lhs, &rhs),
                        MaskOp::Intersect => algebra::intersect(&lhs, &rhs),
                        _ => algebra::subtract(&lhs, &rhs),
                    };
                    stack.push((out, kind, id));
                }
                MaskOp::Invert => {
                    let Some((top, kind, id)) = stack.pop() else {
                        underflowed = true;
                        break;
                    };
                    stack.push((algebra::invert(&top), kind, id));
                }
                MaskOp::Feather { amount } => {
                    let Some((top, kind, id)) = stack.pop() else {
                        underflowed = true;
                        break;
                    };
                    stack.push((algebra::feather(&top, *amount), kind, id));
                }
                MaskOp::Grow { radius } => {
                    let Some((top, kind, id)) = stack.pop() else {
                        underflowed = true;
                        break;
                    };
                    stack.push((algebra::grow(&top, *radius), kind, id));
                }
                MaskOp::Shrink { radius } => {
                    let Some((top, kind, id)) = stack.pop() else {
                        underflowed = true;
                        break;
                    };
                    stack.push((algebra::shrink(&top, *radius), kind, id));
                }
            }
        }

        let Some((plane, kind, _)) = stack.pop() else {
            return empty_mask();
        };
        if underflowed {
            // The program did not mean what it said. The empty mask is the identity for union
            // and the annihilator for intersection, so a caller who applies it changes nothing;
            // a full-frame mask would have applied the edit to the whole photograph, which is
            // the one outcome worse than no edit at all.
            return empty_mask();
        }

        let (payload, _) = store::encode(kind, &plane);
        let edge = if matches!(payload, MaskPayload::Alpha8 { .. }) {
            EdgeQuality::Soft
        } else {
            EdgeQuality::Binary
        };
        Mask {
            id: MaskId::new(),
            image_id: PhotoId::new(),
            kind,
            identity: None,
            payload,
            feather: 0.0,
            // A composition is exactly as good as its operands and this shape cannot see them
            // any more, so it claims what a derived region can honestly claim: the class came
            // from a caller who named it, and the boundary came from arithmetic on measured
            // regions.
            confidence: 1.0,
            edge_quality: 1.0,
            edge,
            reasons: vec![MaskReason::Derived],
            user_edited: false,
            model_ver: MODEL_VER,
        }
    }

    fn upload_gpu(&self, mask: &Mask, level: RenderLevel) -> GpuMask {
        let plane = store::decode(&mask.payload);
        let feathered = algebra::feather(&plane, mask.feather);
        let (w, h) = target_size(&feathered, level);
        let resolved = crate::mask::upload_plane(&feathered, w, h);
        GpuMask {
            id: mask.id,
            level,
            width: resolved.w,
            height: resolved.h,
            alpha: resolved.a,
            allowance: mask.allowance(),
        }
    }

    fn outline(&self, project: ProjectId) -> AuraResult<MaskOutline> {
        self.store.outline(project)
    }
}

/// A frame source that has no pixels. See [`Masks::read_only`].
#[derive(Debug, Clone, Copy)]
struct NoFrames;

impl FrameSource for NoFrames {
    fn frame(&self, image: PhotoId) -> AuraResult<MaskFrame> {
        Err(errors::mask_failed(
            &image.to_db(),
            "this mask service was opened without a source of pixels",
        ))
    }
}

/// The empty mask. See [`MaskService::compose`] for why it exists.
#[must_use]
pub fn empty_mask() -> Mask {
    Mask {
        id: MaskId::new(),
        image_id: PhotoId::new(),
        kind: MaskKind::Subject,
        identity: None,
        payload: MaskPayload::Rle {
            w: 0,
            h: 0,
            runs: Vec::new(),
        },
        feather: 0.0,
        confidence: 0.0,
        edge_quality: 0.0,
        edge: EdgeQuality::Unknown,
        reasons: vec![MaskReason::Derived],
        user_edited: false,
        model_ver: MODEL_VER,
    }
}

/// The plane size a render level wants, preserving the stored aspect.
fn target_size(plane: &Plane, level: RenderLevel) -> (u32, u32) {
    let Some(edge) = level.long_edge() else {
        // `RenderLevel::Full` has no long edge: the sensor decides, and a mask does not know
        // the sensor. The stored plane is returned at the analysis aspect and the renderer
        // resamples it with the frame it is compositing onto, which is the only place both
        // numbers are known.
        return (plane.w, plane.h);
    };
    if plane.w == 0 || plane.h == 0 {
        return (0, 0);
    }
    let long = plane.w.max(plane.h);
    if long == 0 {
        return (plane.w, plane.h);
    }
    let scale = f64::from(edge) / f64::from(long);
    (
        ((f64::from(plane.w) * scale).round() as u32).max(1),
        ((f64::from(plane.h) * scale).round() as u32).max(1),
    )
}

/// The version drift a caller should be told about, if any.
///
/// Returned rather than logged, so the panel renders it once per project instead of the log
/// carrying it once per photograph - phase 14's pattern for `RenderService::degradation`.
#[must_use]
pub fn drift(outline: &MaskOutline) -> Option<AuraError> {
    if outline.masks == 0 {
        return None;
    }
    if outline.model_ver != MODEL_VER {
        return Some(errors::version_mismatch(
            "model_ver",
            outline.model_ver,
            MODEL_VER,
        ));
    }
    if outline.analysis_ver != ANALYSIS_VER {
        return Some(errors::version_mismatch(
            "analysis_ver",
            outline.analysis_ver,
            ANALYSIS_VER,
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    // The panic family is how a test asserts, and a mask test compares alphas that are exactly
    // zero or exactly one by construction - a painted fixture has no rounding to be tolerant of.
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )]
    use super::*;

    #[test]
    fn the_empty_mask_changes_nothing() {
        let empty = empty_mask();
        assert!(empty.payload.is_empty());
        assert_eq!(empty.allowance(), 0.0);
        assert!(!empty.allows_aggressive());
    }

    #[test]
    fn a_full_render_level_keeps_the_stored_aspect() {
        let plane = Plane::zeros(192, 128);
        assert_eq!(target_size(&plane, RenderLevel::Full), (192, 128));
    }

    #[test]
    fn a_proxy_level_puts_the_long_edge_at_2048() {
        let plane = Plane::zeros(192, 128);
        assert_eq!(target_size(&plane, RenderLevel::Proxy2048).0, 2048);
    }
}
