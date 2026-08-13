//! The project walk: batched, resumable, cancellable, and honest about what it
//! skipped.
//!
//! Four properties are designed in, and each one is an invariant rather than a
//! nicety.
//!
//! **Resumable (invariant 5).** The work remaining is a query -
//! `EmbeddingStore::pending` - not a journal. Kill the process at 10 %, 50 % or
//! 90 % and the next run asks the catalog which photographs still have no vector
//! at the current model version. There is no state that can disagree with the
//! rows.
//!
//! **Three-tier (invariant 3).** Embeddings are the cheap pass, so they run on
//! the tier that costs milliseconds: the 384 px thumbnail derived from the
//! camera's embedded preview. The tier and the pixel source are stored per row,
//! so a later phase that needs to know whether a vector came from the camera's
//! look or from ours can find out.
//!
//! **Batched by the plan, not by a constant.** The batch size comes from
//! `HardwarePlan::start_batch`, which is what the phase 03 scheduler learned on
//! this machine. Section 8 step 2 asks for 8/16/32/64 to be benchmarked; the
//! answer belongs in the plan, not in a literal here.
//!
//! **No silent failure (invariant 9).** A photograph whose preview will not
//! decode, or whose vector comes back degenerate, is counted, coded and reported.
//! The run continues. One unreadable frame out of 4,000 is an item failure, and
//! the other 3,999 are still comparable.

use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::progress::{CancelToken, ProgressSink, ProgressUpdate};
use aura_core::{AuraResult, PhotoId, Priority};
use aura_index::contract::index::ImageEmbedding;
use aura_index::store::EmbeddingStore;
use aura_infer::contract::infer::ModelClass;
use aura_preview::contract::service::{PreviewService, Priority as PreviewPriority};
use aura_raw::contract::pixels::{PixelLevel, PixelSource};

use crate::embed::model::{EmbeddingModel, EMBED_INPUT_SIDE, MODEL_VER};
use crate::embed::{descriptors, hash};

/// The pixel rung embeddings are computed from.
///
/// Tier 1 at 384 px: the embedded camera preview, scaled. Section 2.1 says
/// "Tier-1/Tier-2 previews at 384 px", and tier 1 is what makes 4,000 images a
/// two-minute pass rather than a twenty-minute one. A caller that wants the
/// demosaiced look asks for [`PixelLevel::Proxy2048`] explicitly.
pub const EMBED_LEVEL: PixelLevel = PixelLevel::Thumb(384);

/// The telemetry stage name, matching section 11's `embed.batch` event.
pub const STAGE: &str = "embed.batch";

/// What one pass over a project did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmbedReport {
    /// Photographs that gained a vector.
    pub embedded: usize,
    /// Photographs that already had one at this version.
    pub skipped: usize,
    /// Photographs that could not be embedded, with their reason logged.
    pub failed: usize,
    /// Batches submitted to the runtime.
    pub batches: usize,
    /// Wall clock for the whole pass.
    pub elapsed_ms: u64,
    /// True when the pass stopped early because it was cancelled.
    pub cancelled: bool,
}

impl EmbedReport {
    /// Milliseconds per embedded photograph, or zero when nothing was embedded.
    #[must_use]
    pub fn ms_per_image(&self) -> f64 {
        if self.embedded == 0 {
            return 0.0;
        }
        self.elapsed_ms as f64 / self.embedded as f64
    }
}

/// One progress observation, shaped for the `embedProgress` IPC event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbedProgress {
    /// Photographs finished.
    pub done: u64,
    /// Photographs in this pass.
    pub total: u64,
    /// Size of the batch just submitted.
    pub batch_size: u16,
}

/// Walks a project, embedding what has no vector yet.
#[derive(Debug)]
pub struct EmbeddingRunner {
    previews: Arc<dyn PreviewService>,
    model: EmbeddingModel,
    store: Arc<EmbeddingStore>,
    clock: Arc<dyn Clock>,
    batch_size: u16,
}

impl EmbeddingRunner {
    /// Assemble a runner. The batch size comes from the hardware plan the
    /// inference service is already carrying.
    #[must_use]
    pub fn new(
        previews: Arc<dyn PreviewService>,
        infer: Arc<dyn aura_infer::contract::infer::InferService>,
        store: Arc<EmbeddingStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let batch_size = infer
            .plan()
            .start_batch(crate::embed::model::EMBED_MODEL, ModelClass::Embedding);
        Self {
            previews,
            model: EmbeddingModel::new(infer),
            store,
            clock,
            batch_size: batch_size.max(1),
        }
    }

    /// Override the batch size. Used by the benchmark that answers section 8
    /// step 2, and by nothing in the product.
    #[must_use]
    pub const fn with_batch_size(mut self, batch_size: u16) -> Self {
        self.batch_size = if batch_size == 0 { 1 } else { batch_size };
        self
    }

    /// The batch size in force.
    #[must_use]
    pub const fn batch_size(&self) -> u16 {
        self.batch_size
    }

    /// The model this runner is pinned to.
    #[must_use]
    pub const fn model(&self) -> &EmbeddingModel {
        &self.model
    }

    /// Embed every photograph in a project that has no current vector.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the pending set cannot be read. Per-photograph failures
    /// are counted in the report rather than returned: one unreadable frame must
    /// not end a 4,000-image pass.
    pub fn embed_project(
        &self,
        project_id: &str,
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<EmbedReport> {
        let pending = self.store.pending(project_id, MODEL_VER)?;
        self.embed_ids(project_id, &pending, prio, cancel, progress)
    }

    /// Embed a specific list of photographs.
    ///
    /// The incremental path: after a second card is imported, the caller passes
    /// the new ids and nothing else is touched. Acceptance criterion 5.
    ///
    /// # Errors
    ///
    /// As [`EmbeddingRunner::embed_project`].
    pub fn embed_ids(
        &self,
        project_id: &str,
        ids: &[PhotoId],
        prio: Priority,
        cancel: &CancelToken,
        progress: &dyn ProgressSink,
    ) -> AuraResult<EmbedReport> {
        let started = self.clock.monotonic_ms();
        let mut report = EmbedReport::default();
        let total = ids.len() as u64;
        let preview_priority = match prio {
            Priority::Visible => PreviewPriority::Visible,
            Priority::Interactive => PreviewPriority::Interactive,
            Priority::AiBatch => PreviewPriority::AiBatch,
            Priority::Background => PreviewPriority::Background,
        };

        for chunk in ids.chunks(usize::from(self.batch_size)) {
            if cancel.is_cancelled() {
                report.cancelled = true;
                break;
            }

            let (tensors, carried, unreadable) = self.decode_chunk(chunk, preview_priority);
            report.failed += unreadable;
            if tensors.is_empty() {
                continue;
            }

            let batch_started = self.clock.monotonic_ms();
            let vectors = match self.model.run_batch(&tensors, prio) {
                Ok(vectors) => vectors,
                Err(err) => {
                    tracing::warn!(
                        target: "embed",
                        code = %err.code,
                        batch = tensors.len(),
                        "a batch failed"
                    );
                    report.failed += tensors.len();
                    continue;
                }
            };
            report.batches += 1;
            let batch_ms = self.clock.monotonic_ms().saturating_sub(batch_started);
            let per_image = if vectors.is_empty() {
                0.0
            } else {
                batch_ms as f32 / vectors.len() as f32
            };

            // Section 11: `embed.batch` {count, ms, ep, batch_size}.
            tracing::info!(
                target: STAGE,
                count = vectors.len(),
                ms = batch_ms,
                ep = self.model.ep(),
                batch_size = self.batch_size,
                "embedded a batch"
            );

            for (item, vector) in carried.into_iter().zip(vectors) {
                match crate::embed::finish(
                    item.id,
                    EMBED_LEVEL,
                    item.source,
                    &vector,
                    item.dhash,
                    item.described,
                    per_image,
                ) {
                    Ok(analysed) => match self.store.put(project_id, &analysed.stored) {
                        Ok(()) => report.embedded += 1,
                        Err(err) => {
                            tracing::warn!(
                                target: "embed",
                                photo = %item.id,
                                code = %err.code,
                                "could not store an embedding"
                            );
                            report.failed += 1;
                        }
                    },
                    Err(err) => {
                        tracing::warn!(
                            target: "embed",
                            photo = %item.id,
                            code = %err.code,
                            "{}", err.detail
                        );
                        report.failed += 1;
                    }
                }
            }

            progress.report(ProgressUpdate {
                stage: "embed.project",
                done: (report.embedded + report.failed) as u64,
                total,
                current: None,
            });
        }

        report.elapsed_ms = self.clock.monotonic_ms().saturating_sub(started);
        Ok(report)
    }

    /// Decode one chunk, describe every frame in it, and build its tensors.
    ///
    /// Returns the tensors, what has to survive until the batch's output arrives,
    /// and how many frames could not be read. The decoded buffers are dropped
    /// before this returns: a 4,000-image wedding is never 4,000 resident proxies,
    /// and that is the memory ceiling this phase promised.
    fn decode_chunk(
        &self,
        chunk: &[PhotoId],
        preview_priority: PreviewPriority,
    ) -> (Vec<Vec<f32>>, Vec<Carried>, usize) {
        let mut tensors: Vec<Vec<f32>> = Vec::with_capacity(chunk.len());
        let mut carried: Vec<Carried> = Vec::with_capacity(chunk.len());
        let mut unreadable = 0usize;

        self.previews.prefetch(chunk, EMBED_LEVEL);
        for id in chunk {
            let buffer = match self.previews.get(*id, EMBED_LEVEL, preview_priority) {
                Ok(buffer) => buffer,
                Err(err) => {
                    tracing::warn!(
                        target: "embed",
                        photo = %id,
                        code = %err.code,
                        "could not read a preview to embed"
                    );
                    unreadable += 1;
                    continue;
                }
            };
            let Some(rgb) = buffer.as_srgb8() else {
                tracing::warn!(target: "embed", photo = %id, "preview is not 8-bit sRGB");
                unreadable += 1;
                continue;
            };
            let width = buffer.width as usize;
            let height = buffer.height as usize;
            if width == 0 || height == 0 || rgb.len() < width * height * 3 {
                tracing::warn!(target: "embed", photo = %id, "preview dimensions disagree with its payload");
                unreadable += 1;
                continue;
            }

            let plane = descriptors::luma_plane(rgb, width, height);
            let described = descriptors::describe(rgb, width, height, &plane);
            let dhash = hash::dhash(&plane.values, width, height);
            tensors.push(EmbeddingModel::preprocess(rgb, width, height));
            carried.push(Carried {
                id: *id,
                described,
                dhash,
                source: buffer.source,
            });
        }
        (tensors, carried, unreadable)
    }

    /// Embed one photograph the caller already has pixels for.
    ///
    /// The interactive path: the debug panel's "find similar" on a frame that was
    /// never analysed, and the loupe. One frame, one decode, no batch.
    ///
    /// # Errors
    ///
    /// As [`crate::embed::run_one`], plus `AURA-DB-3006` when the row cannot be
    /// written.
    pub fn embed_one(
        &self,
        project_id: &str,
        id: PhotoId,
        prio: Priority,
    ) -> AuraResult<ImageEmbedding> {
        let preview_priority = match prio {
            Priority::Visible => PreviewPriority::Visible,
            Priority::Interactive => PreviewPriority::Interactive,
            Priority::AiBatch => PreviewPriority::AiBatch,
            Priority::Background => PreviewPriority::Background,
        };
        let buffer = self.previews.get(id, EMBED_LEVEL, preview_priority)?;
        let analysed = crate::embed::run_one(&self.model, id, &buffer, EMBED_LEVEL, prio)?;
        self.store.put(project_id, &analysed.stored)?;
        Ok(analysed.stored.embedding)
    }

    /// The tensor width one image costs, for the benchmark's report.
    #[must_use]
    pub const fn tensor_len() -> usize {
        3 * EMBED_INPUT_SIDE * EMBED_INPUT_SIDE
    }
}

/// What survives the decode so it can be joined to the batch's output.
///
/// The decoded buffer itself is dropped as soon as its tensor and its descriptors
/// exist, which is the point of the whole pass: a 4,000-image wedding is never
/// 4,000 resident proxies. What is carried forward is the tensor, the descriptors,
/// the hash and the provenance - a few kilobytes rather than twelve megabytes.
#[derive(Debug)]
struct Carried {
    id: PhotoId,
    described: aura_index::store::Descriptors,
    dhash: u64,
    source: PixelSource,
}
