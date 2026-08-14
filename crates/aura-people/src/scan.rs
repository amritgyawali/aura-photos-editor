//! The project walk: one frame at a time, resumable, cancellable, and honest about
//! what it skipped.
//!
//! The same four properties as the embedding pass in `aura_vision::embed::batch`, with
//! one difference that matters.
//!
//! **Resumable (invariant 5).** The work remaining is a query -
//! [`crate::store::PeopleStore::pending`] - not a journal. The difference from phase 05
//! is that "no faces in this frame" is a legitimate result, so the presence of a face
//! row cannot mean "done"; `face_scan` records the look rather than the finding. Kill
//! the process at 10 %, 50 % or 90 % and the next run asks the catalog which
//! photographs have not been looked at.
//!
//! **Three-tier (invariant 3), with a documented exception.** Faces run on the 2048 px
//! proxy rather than the 384 px thumbnail, and the reason is written down at
//! `aura_vision::face::FACE_LEVEL`: a guest's face is eleven pixels tall on a
//! thumbnail. The proxy is already built and cached by phase 02, so the cost is a cache
//! read.
//!
//! **One frame at a time, not one batch of frames.** This is the other difference from
//! phase 05, and it is deliberate. An embedding pass has one tensor per image and
//! batching them is free. A face pass has a variable number of faces per frame, five
//! detector passes on a wide frame, and a 640 px input that is twenty-eight times the
//! area of an embedding's - so a batch of sixteen frames could hold anything from
//! sixteen to five hundred crops and 400 MB of activations. Batching happens *inside* a
//! frame, where the count is known, and the memory ledger in `aura-infer` sees a
//! predictable request every time.
//!
//! **No silent failure (invariant 9).** A frame whose proxy will not decode is counted,
//! coded as `AURA-ML-5017` and reported. The run continues, and - importantly - no
//! `face_scan` row is written for it, so the next pass tries again. That is the right
//! behaviour for a transient decode failure and harmless for a permanent one.

use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraResult, PhotoId, Priority, ProjectId};
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};
use aura_vision::face::{detect, FacePipeline, FACE_LEVEL};

use crate::store::PeopleStore;

/// The telemetry stage name, matching section 11's `face.detect` event.
pub const STAGE: &str = "face.detect";

/// What one pass over a project did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScanReport {
    /// Photographs looked at.
    pub scanned: usize,
    /// Faces found.
    pub faces: usize,
    /// Faces that passed the quality gate.
    pub voting: usize,
    /// Faces below the gate. Stored, visible, and excluded from identity voting.
    pub gated: usize,
    /// Body boxes found.
    pub person_boxes: usize,
    /// Bodies with no face, which is the case body detection exists for.
    pub headless: usize,
    /// Photographs that could not be scanned, each logged with a code.
    pub failed: usize,
    /// Photographs that earned the tiled pass.
    pub tiled_frames: usize,
    /// Forward passes across all three models.
    pub passes: usize,
    /// Wall clock for the whole pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

impl ScanReport {
    /// Milliseconds per scanned photograph, or zero when nothing was scanned.
    #[must_use]
    pub fn ms_per_image(&self) -> f64 {
        if self.scanned == 0 {
            return 0.0;
        }
        self.elapsed_ms as f64 / self.scanned as f64
    }

    /// Fraction of scanned frames that earned the tiled pass, `0..1`.
    ///
    /// Section 12's fifth failure mode is "tiled detection doubles cost", and the
    /// mitigation it asks for is a measured cost/benefit rather than a promise. This is
    /// the measurement: the phase gate prints it and the benchmark report records it.
    #[must_use]
    pub fn tile_ratio(&self) -> f64 {
        if self.scanned == 0 {
            return 0.0;
        }
        self.tiled_frames as f64 / self.scanned as f64
    }

    /// Faces per scanned photograph.
    #[must_use]
    pub fn faces_per_image(&self) -> f64 {
        if self.scanned == 0 {
            return 0.0;
        }
        self.faces as f64 / self.scanned as f64
    }
}

/// Walks a project, scanning what has not been looked at.
#[derive(Debug)]
pub struct FaceScanner {
    previews: Arc<dyn PreviewService>,
    pipeline: FacePipeline,
    store: Arc<PeopleStore>,
    clock: Arc<dyn Clock>,
}

impl FaceScanner {
    /// Assemble a scanner.
    #[must_use]
    pub fn new(
        previews: Arc<dyn PreviewService>,
        pipeline: FacePipeline,
        store: Arc<PeopleStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            previews,
            pipeline,
            store,
            clock,
        }
    }

    /// The pipeline this scanner drives.
    #[must_use]
    pub const fn pipeline(&self) -> &FacePipeline {
        &self.pipeline
    }

    /// Load all three models before a photographer is waiting on them.
    ///
    /// # Errors
    ///
    /// Whatever the loader raised.
    pub fn warmup(&self) -> AuraResult<()> {
        self.pipeline.warmup()
    }

    /// Scan every photograph in a project that has not been looked at.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the pending set cannot be read, `AURA-SEC-9001` when the
    /// biometric key is unavailable. Per-photograph failures are counted in the report
    /// rather than returned: one unreadable frame must not end a 4,000-image pass.
    pub fn scan_project(
        &self,
        project: &ProjectId,
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<ScanReport> {
        let pending = self
            .store
            .pending(project, detect::MODEL_VER, detect::PREPROCESS_VER)?;
        self.scan_ids(project, &pending, prio, cancel, progress)
    }

    /// Scan a specific list of photographs.
    ///
    /// The incremental path: after a second card is imported, the caller passes the new
    /// ids and nothing else is touched.
    ///
    /// # Errors
    ///
    /// As [`FaceScanner::scan_project`].
    #[allow(clippy::too_many_lines)]
    pub fn scan_ids(
        &self,
        project: &ProjectId,
        ids: &[PhotoId],
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<ScanReport> {
        // Prefetching is worth it even though the pass is per frame: the preview service
        // decodes on its own threads, so the next frame's proxy is being built while this
        // one is being scanned. The window is small on purpose - a large one would hold a
        // hundred 12 MB proxies to save a few milliseconds.
        const PREFETCH_WINDOW: usize = 4;

        let started = self.clock.monotonic_ms();
        let mut report = ScanReport::default();
        let total = ids.len() as u64;
        let preview_priority = match prio {
            Priority::Visible => PreviewPriority::Visible,
            Priority::Interactive => PreviewPriority::Interactive,
            Priority::AiBatch => PreviewPriority::AiBatch,
            Priority::Background => PreviewPriority::Background,
        };

        for (position, id) in ids.iter().enumerate() {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }

            if let Some(window) =
                ids.get(position + 1..(position + 1 + PREFETCH_WINDOW).min(ids.len()))
            {
                self.previews.prefetch(window, FACE_LEVEL);
            }

            let buffer = match self.previews.get(*id, FACE_LEVEL, preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    let coded = crate::errors::face_scan_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "people.scan",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };

            let hint = self.store.frame_hint(*id).unwrap_or_default();
            let frame_started = self.clock.monotonic_us();
            let mut frame = match self.pipeline.analyse(*id, &buffer, FACE_LEVEL, &hint, prio) {
                Ok(frame) => frame,
                Err(err) => {
                    let coded = crate::errors::face_scan_failed(&id.to_db(), &err.detail);
                    tracing::warn!(
                        target: "people.scan",
                        photo = %id,
                        code = %coded.code,
                        "{}", coded.detail
                    );
                    report.failed += 1;
                    continue;
                }
            };
            let frame_us = self.clock.monotonic_us().saturating_sub(frame_started);
            frame.infer_ms = frame_us as f32 / 1_000.0;

            // Drop the decoded proxy before writing. A 2048 px proxy is 12 MB, and
            // holding one across a transaction is how a 4,000-image pass ends up with a
            // resident set nobody predicted.
            drop(buffer);

            report.faces += frame.faces.len();
            report.voting += frame.voting();
            report.gated += frame.faces.len() - frame.voting();
            report.person_boxes += frame.persons.len();
            report.headless += frame.count.headless;
            report.passes += frame.passes;
            if frame.tiled {
                report.tiled_frames += 1;
            }

            match self.store.put_frame(project, &frame) {
                Ok(_) => report.scanned += 1,
                Err(err) => {
                    tracing::warn!(
                        target: "people.scan",
                        photo = %id,
                        code = %err.code,
                        "could not store a frame's faces"
                    );
                    report.failed += 1;
                }
            }

            // Section 11: `face.detect` {images, faces, small_faces, ms, tiled_pass_used}.
            tracing::info!(
                target: STAGE,
                images = 1,
                faces = frame.faces.len(),
                small_faces = frame.small_faces(
                    self.pipeline.detect_config().small_face_fraction
                ),
                ms = frame.infer_ms,
                tiled_pass_used = frame.tiled,
                ep = self.pipeline.ep(),
                "frame scanned"
            );

            progress.report(ProgressUpdate {
                stage: "people.scan",
                done: (report.scanned + report.failed) as u64,
                total,
                current: None,
            });
        }

        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);

        tracing::info!(
            target: STAGE,
            images = report.scanned,
            faces = report.faces,
            voting = report.voting,
            failed = report.failed,
            tiled_frames = report.tiled_frames,
            tile_ratio = report.tile_ratio(),
            ms = report.elapsed_ms,
            "face pass finished"
        );
        Ok(report)
    }
}
