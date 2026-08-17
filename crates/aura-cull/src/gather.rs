//! Six phases, one flat table of numbers.
//!
//! This is the only module in the crate that knows what an `IntegrityResult`, an
//! `ImageEmotion`, a `CompositionResult`, an `ImageSubjects`, a `Segment` or a `Moment` is.
//! By the time a candidate reaches [`fusion`](crate::fusion) it is four floats, a scene, a
//! chapter and a timestamp.
//!
//! That flattening is deliberate and it is what keeps six phases' rules intact. A pass that
//! could read `IntegrityFlags` would eventually decide for itself what a flag means, and
//! phase 09's rule - one answer in this product to "is this frame sharp" - would quietly
//! acquire a second answer in the phase where it matters most.
//!
//! ## Everything comes through a frozen trait
//!
//! `IntegrityService`, `EmotionService`, `CompositionService`, `PeopleService`,
//! `StoryService` and `MomentService`. This crate does not depend on a single one of the
//! crates that implement them, and its `Cargo.toml` says so at length.
//!
//! Five of the six are **optional**. A wedding whose phase 10 pass has not run is still
//! cullable - with `emotion` at [`KeepScore::NEUTRAL`], a confidence penalty and an
//! `emotion_aware` figure that says how much of the fusion was real. `IntegrityService` is
//! the one that is not optional, because a frame with no technical verdict is not a frame
//! this phase may judge at all.
//!
//! ## The vetoes are decided here, from measurements
//!
//! Section 6.1's three hard vetoes are read off the phase 09 verdict rather than derived
//! from a composite:
//!
//! * the subject sharpness is under `focus_floor` - "completely out of focus", not "softer
//!   than average";
//! * `ExposureVerdict::Lost` - phase 09 has already decided the information is not in the
//!   file;
//! * the eyes are closed, the frame is in a posed scene, and phase 09 did **not** set
//!   `EYES_CLOSED_OK`.
//!
//! The third has three conditions because it is the veto that would do the most damage if
//! it fired in the wrong place. `EYES_CLOSED_OK` is phase 09's own exoneration - it is set
//! for a kiss, a prayer and, since phase 10 closed condition C4, for tears - and this phase
//! honours it rather than re-deciding it. The posed-scene list is the second line of the
//! same defence and it lives in `coverage_rules.toml` with a rationale beside it.
//!
//! ## One read per photograph per service, and no pixels at all
//!
//! Phase 12 opens no image file and runs no model. Everything here is an indexed read of a
//! table an earlier phase filled, which is why section 11 budgets 1.5 seconds for the whole
//! selection over four thousand frames and eight minutes for the analysis underneath it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::contract::composition::CompositionService;
use aura_core::contract::cull::ImageId;
use aura_core::contract::emotion::EmotionService;
use aura_core::contract::integrity::{ExposureVerdict, IntegrityFlags, IntegrityService};
use aura_core::contract::moment::MomentService;
use aura_core::contract::people::PeopleService;
use aura_core::contract::scene::{ChapterId, StoryService, Timestamp};
use aura_core::errors::db::statement_failed;
use aura_core::{
    AuraResult, CullCode, IdentityId, KeepScore, MomentId, PhotoId, ProjectId, SceneId,
};
use rusqlite::params;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::input::{Candidate, CullInput, Framing, IdentityInfo, MomentInfo};
use crate::rules::RuleTable;
use crate::store::RowContext;

/// Reads the six services and produces one [`CullInput`].
#[derive(Debug)]
pub struct Gatherer {
    catalog: Arc<Catalog>,
    integrity: Arc<dyn IntegrityService>,
    emotion: Option<Arc<dyn EmotionService>>,
    composition: Option<Arc<dyn CompositionService>>,
    people: Option<Arc<dyn PeopleService>>,
    story: Option<Arc<dyn StoryService>>,
    moments: Option<Arc<dyn MomentService>>,
}

impl Gatherer {
    /// A gatherer with the one service that is not optional.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, integrity: Arc<dyn IntegrityService>) -> Self {
        Self {
            catalog,
            integrity,
            emotion: None,
            composition: None,
            people: None,
            story: None,
            moments: None,
        }
    }

    /// Attach phase 10.
    #[must_use]
    pub fn with_emotion(mut self, service: Arc<dyn EmotionService>) -> Self {
        self.emotion = Some(service);
        self
    }

    /// Attach phase 11.
    #[must_use]
    pub fn with_composition(mut self, service: Arc<dyn CompositionService>) -> Self {
        self.composition = Some(service);
        self
    }

    /// Attach phase 06.
    #[must_use]
    pub fn with_people(mut self, service: Arc<dyn PeopleService>) -> Self {
        self.people = Some(service);
        self
    }

    /// Attach phase 07.
    #[must_use]
    pub fn with_story(mut self, service: Arc<dyn StoryService>) -> Self {
        self.story = Some(service);
        self
    }

    /// Attach phase 08.
    #[must_use]
    pub fn with_moments(mut self, service: Arc<dyn MomentService>) -> Self {
        self.moments = Some(service);
        self
    }

    /// Read one project.
    ///
    /// Returns the input the engine consumes and the per-row context the store writes, so
    /// that neither is read twice.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the project's photographs cannot be listed, and whatever any
    /// attached service raised.
    #[allow(clippy::too_many_lines)]
    pub fn gather(
        &self,
        project: ProjectId,
    ) -> AuraResult<(CullInput, BTreeMap<ImageId, RowContext>)> {
        let frames = self.frames(project)?;

        // Phase 08's grouping, read once. A moment lookup per photograph would be four
        // thousand round trips through the frozen service, which is the cost phase 08's own
        // exit report names.
        let mut moment_of: BTreeMap<ImageId, MomentId> = BTreeMap::new();
        let mut moments: Vec<MomentInfo> = Vec::new();
        let mut duplicate_of: BTreeMap<ImageId, usize> = BTreeMap::new();
        if let Some(service) = &self.moments {
            let outline = service.outline(project)?;
            for moment in &outline.moments {
                let mut groups: Vec<Vec<ImageId>> = Vec::new();
                for set in &moment.duplicate_sets {
                    let index = groups.len();
                    for image in &set.image_ids {
                        duplicate_of.insert(*image, index);
                    }
                    groups.push(set.image_ids.clone());
                }
                for image in &moment.image_ids {
                    moment_of.insert(*image, moment.id);
                }
                moments.push(MomentInfo {
                    id: moment.id,
                    frames: u32::try_from(moment.image_ids.len()).unwrap_or(u32::MAX),
                    diversity: moment.diversity,
                    // Significance is the peak's own confidence when phase 10 named one,
                    // and the moment's grouping confidence otherwise. Not a new model: it
                    // is the strength of the claim that this moment has an apex, which is
                    // exactly what section 6.2 wants `k` to respond to.
                    significance: moment.confidence,
                    duplicate_groups: groups,
                    user_locked: moment.user_locked,
                });
            }
        }

        // Phase 10's peaks, one read per moment rather than per frame.
        let mut peaks: BTreeMap<ImageId, bool> = BTreeMap::new();
        let mut flat_peaks: BTreeSet<ImageId> = BTreeSet::new();
        if let Some(service) = &self.emotion {
            for moment in &mut moments {
                if let Some(peak) = service.peak_of(moment.id)? {
                    peaks.insert(peak.image_id, true);
                    if !peak.is_resolved() {
                        flat_peaks.insert(peak.image_id);
                    }
                    moment.significance = moment.significance.max(peak.confidence);
                }
            }
        }

        // Phase 07's chapters, as a timeline the frames are looked up in.
        let mut segments: Vec<(Timestamp, Timestamp, ChapterId)> = Vec::new();
        if let Some(service) = &self.story {
            for segment in service.outline(project)?.segments {
                segments.push((segment.start_ts, segment.end_ts, segment.chapter));
            }
        }

        // Phase 06's hierarchy.
        let mut identities: Vec<IdentityInfo> = Vec::new();
        let mut couple_unconfirmed = false;
        if let Some(service) = &self.people {
            let hierarchy = service.hierarchy(project)?;
            couple_unconfirmed = hierarchy.couple_unconfirmed;
            let primary: BTreeSet<IdentityId> = hierarchy.primary.iter().copied().collect();
            let secondary: BTreeSet<IdentityId> = hierarchy.secondary.iter().copied().collect();
            for (id, weight) in &hierarchy.weights {
                identities.push(IdentityInfo {
                    id: *id,
                    weight: *weight,
                    primary: primary.contains(id),
                    // `SubjectHierarchy` exposes no per-identity `Role`, so `secondary` -
                    // "close family and anybody marked important" - is what the close-family
                    // rule reads. ADR-0025 section 5.
                    close_family: secondary.contains(id),
                    appearances: 0,
                });
            }
        }

        let mut candidates = Vec::with_capacity(frames.len());
        let mut context = BTreeMap::new();
        let mut appearances: BTreeMap<IdentityId, u32> = BTreeMap::new();
        let mut model_ver = u16::MAX;
        for (image, timeline_ts) in frames {
            let verdict = self.integrity.of_image(image)?;
            let subjects = match &self.people {
                Some(service) => Some(service.subjects(image)?),
                None => None,
            };
            let emotion = match &self.emotion {
                Some(service) => service.of_image(image)?,
                None => None,
            };
            let composition = match &self.composition {
                Some(service) => service.of_image(image)?,
                None => None,
            };
            let scene = verdict.as_ref().map_or(SceneId::Unknown, |v| v.scene);

            let mut identity_list: Vec<IdentityId> = Vec::new();
            let prominence = match &subjects {
                Some(subjects) => {
                    // Prominence order: `ImageSubjects::prominence` is a map, so the frame's
                    // own ordering is by weight descending with the id as the tie-break -
                    // which is what makes `dominant_identity` in the diversity pass a stable
                    // choice rather than whichever id sorted first.
                    let mut ranked: Vec<(IdentityId, f32)> = subjects
                        .prominence
                        .iter()
                        .map(|(id, w)| (*id, *w))
                        .collect();
                    ranked.sort_by(|left, right| {
                        right
                            .1
                            .total_cmp(&left.1)
                            .then_with(|| left.0.cmp(&right.0))
                    });
                    identity_list = ranked.iter().map(|(id, _)| *id).collect();
                    for id in &identity_list {
                        *appearances.entry(*id).or_insert(0) += 1;
                    }
                    weighted_prominence(subjects.subject_focus_score, &ranked, &identities)
                }
                None => KeepScore::NEUTRAL,
            };

            let framing = subjects
                .as_ref()
                .and_then(|subjects| {
                    subjects
                        .faces
                        .iter()
                        .map(|face| face.area_frac)
                        .fold(None::<f32>, |best, area| {
                            Some(best.map_or(area, |value| value.max(area)))
                        })
                })
                .map_or(Framing::Wide, Framing::from_face_area);

            let technical = verdict.as_ref().map_or(0.0, |v| v.technical_score);
            let emotion_score = emotion
                .as_ref()
                .map_or(KeepScore::NEUTRAL, |e| e.emotion_score);
            let composition_score = composition
                .as_ref()
                .map_or(KeepScore::NEUTRAL, |c| c.composition_score);

            let mut confidence = verdict.as_ref().map_or(0.0, |v| v.confidence);
            if let Some(reading) = &emotion {
                confidence = confidence.min(reading.confidence);
            }
            if let Some(judgement) = &composition {
                confidence = confidence.min(judgement.confidence);
            }
            for version in [
                verdict.as_ref().map(|v| v.model_ver),
                emotion.as_ref().map(|e| e.model_ver),
                composition.as_ref().map(|c| c.model_ver),
            ]
            .into_iter()
            .flatten()
            {
                model_ver = model_ver.min(version);
            }

            let chapter = segments
                .iter()
                .find(|(start, end, _)| timeline_ts >= *start && timeline_ts <= *end)
                .map_or(ChapterId::Other, |(_, _, chapter)| *chapter);

            let candidate = Candidate {
                image_id: image,
                moment_id: moment_of.get(&image).copied(),
                chapter,
                scene,
                timeline_ts,
                technical,
                emotion: emotion_score,
                composition: composition_score,
                prominence,
                analysed: verdict.is_some(),
                has_emotion: emotion.is_some(),
                has_composition: composition.is_some(),
                veto: None,
                is_peak: peaks.contains_key(&image),
                peak_flat: flat_peaks.contains(&image),
                duplicate_group: duplicate_of.get(&image).copied(),
                identities: identity_list,
                framing,
                interactions: emotion
                    .as_ref()
                    .map(|reading| {
                        reading
                            .interactions
                            .iter()
                            .map(|(interaction, _)| *interaction)
                            .collect()
                    })
                    .unwrap_or_default(),
                confidence,
                model_ver,
            };

            context.insert(
                image,
                RowContext {
                    scene: scene.as_str().to_string(),
                    chapter: chapter.as_str().to_string(),
                    timeline_ts,
                    sub_scores: candidate.sub_scores(),
                },
            );
            candidates.push(candidate);
        }

        for identity in &mut identities {
            identity.appearances = appearances.get(&identity.id).copied().unwrap_or(0);
        }

        let hours = span_hours(&candidates);
        let mut input = CullInput {
            candidates,
            moments,
            identities,
            hours,
            couple_unconfirmed,
            model_ver: if model_ver == u16::MAX { 0 } else { model_ver },
        };
        input.sort();
        Ok((input, context))
    }

    /// Apply a rule table's veto policy to an already-gathered input.
    ///
    /// Separate from [`Gatherer::gather`] because the veto policy is configuration and the
    /// gather is a set of reads: a slider move re-runs the second and not the first, and a
    /// rule-table change re-runs both. Keeping them apart is what makes a two-second slider
    /// possible without a second code path.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when a verdict cannot be re-read.
    pub fn apply_vetoes(&self, input: &mut CullInput, rules: &RuleTable) -> AuraResult<()> {
        let policy = rules.veto();
        for candidate in &mut input.candidates {
            if !candidate.analysed {
                continue;
            }
            let Some(verdict) = self.integrity.of_image(candidate.image_id)? else {
                continue;
            };
            candidate.veto = veto_for(
                verdict.subject_sharpness,
                verdict.exposure,
                verdict.flags,
                candidate.scene,
                policy.focus_floor,
                &policy.posed_scenes,
            );
        }
        Ok(())
    }

    /// Every photograph in a project, with its clock-aligned time, in timeline order.
    fn frames(&self, project: ProjectId) -> AuraResult<Vec<(ImageId, Timestamp)>> {
        let key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id, timeline_time, sub_sec
                       FROM photo
                      WHERE project_id = ?1
                      ORDER BY timeline_time, photo_id",
                )
                .map_err(|e| statement_failed("could not list the project's photographs", &e))?;
            let mut out = Vec::new();
            let mut cursor = statement
                .query(params![key])
                .map_err(|e| statement_failed("could not list the project's photographs", &e))?;
            while let Some(row) = cursor
                .next()
                .map_err(|e| statement_failed("could not list the project's photographs", &e))?
            {
                let text: String = row.get(0).unwrap_or_default();
                let Ok(image) = PhotoId::from_db(&text) else {
                    continue;
                };
                let stamp: Option<String> = row.get(1).unwrap_or_default();
                let sub_sec: Option<i64> = row.get(2).unwrap_or_default();
                let millis = stamp
                    .as_deref()
                    .and_then(parse_rfc3339_ms)
                    .unwrap_or_default()
                    + sub_sec_ms(sub_sec);
                out.push((image, millis));
            }
            Ok(out)
        })
    }
}

/// Which veto applies, if any.
///
/// Free of the gatherer so that the eval harness can exercise section 6.1's three rules
/// directly against known inputs, which is the only way to prove that the closed-eye veto
/// does not fire on a kiss.
#[must_use]
pub fn veto_for(
    subject_sharpness: f32,
    exposure: ExposureVerdict,
    flags: IntegrityFlags,
    scene: SceneId,
    focus_floor: f32,
    posed_scenes: &BTreeSet<SceneId>,
) -> Option<CullCode> {
    if subject_sharpness < focus_floor {
        return Some(CullCode::VetoOutOfFocus);
    }
    if exposure == ExposureVerdict::Lost {
        return Some(CullCode::VetoExposureLost);
    }
    // Three conditions, and the third is phase 09's own exoneration. See this module's
    // header: this phase honours `EYES_CLOSED_OK` rather than re-deciding it.
    if flags.contains(IntegrityFlags::EYES_CLOSED)
        && !flags.contains(IntegrityFlags::EYES_CLOSED_OK)
        && posed_scenes.contains(&scene)
    {
        return Some(CullCode::VetoEyesClosed);
    }
    None
}

/// The prominence sub-score: how much this photograph is *of somebody who matters*.
///
/// `subject_focus_score` is phase 06's prominence-weighted sharpness of the faces present.
/// It is scaled by the hierarchy weight of the most important identity in the frame, so a
/// sharp photograph of a vendor and a sharp photograph of the bride are not the same
/// number - which is the whole reason the fourth sub-score exists.
fn weighted_prominence(
    focus: f32,
    ranked: &[(IdentityId, f32)],
    identities: &[IdentityInfo],
) -> f32 {
    let best = ranked
        .iter()
        .filter_map(|(id, _)| {
            identities
                .iter()
                .find(|entry| entry.id == *id)
                .map(|entry| entry.weight)
        })
        .fold(0.0f32, f32::max);
    if best <= 0.0 {
        // Faces with no hierarchy weight yet. Neutral rather than zero: a face phase 06 has
        // not weighted is an unknown, and multiplying it into a geometric mean as zero would
        // veto a frame for a phase that has not finished.
        return KeepScore::NEUTRAL;
    }
    (focus.clamp(0.0, 1.0) * 0.5 + best.clamp(0.0, 1.0) * 0.5).clamp(0.02, 1.0)
}

/// How long the wedding ran, from the first frame to the last.
fn span_hours(candidates: &[Candidate]) -> f32 {
    let mut first = i64::MAX;
    let mut last = i64::MIN;
    for candidate in candidates {
        first = first.min(candidate.timeline_ts);
        last = last.max(candidate.timeline_ts);
    }
    if first >= last {
        return 0.0;
    }
    let minutes = (last - first) / 60_000;
    f32::from(u16::try_from(minutes.clamp(0, 65_535)).unwrap_or(u16::MAX)) / 60.0
}

/// The fraction of a second, in milliseconds.
///
/// Phase 08's `sub_sec_ms`, restated rather than imported: `aura-brain-wedding` is one of
/// the crates this one deliberately does not depend on, and the alternative - a moment
/// service call per photograph to recover a number already in the `photo` row - is four
/// thousand round trips to avoid twelve lines.
///
/// EXIF's `SubSecTimeOriginal` is a decimal *fraction* written without its point, so "5" is
/// half a second and "05" is five hundredths. Two digits is hundredths, three is
/// thousandths; anything longer is truncated to milliseconds.
fn sub_sec_ms(sub_sec: Option<i64>) -> i64 {
    let Some(value) = sub_sec else {
        return 0;
    };
    match value {
        v if v < 0 => 0,
        v if v < 10 => v * 100,
        v if v < 100 => v * 10,
        v if v < 1_000 => v,
        v => v % 1_000,
    }
}

/// RFC 3339 to milliseconds since the epoch.
fn parse_rfc3339_ms(text: &str) -> Option<i64> {
    OffsetDateTime::parse(text, &Rfc3339)
        .ok()
        .map(|stamp| stamp.unix_timestamp() * 1_000)
}
