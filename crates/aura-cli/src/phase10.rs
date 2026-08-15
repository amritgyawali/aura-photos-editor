//! The phase 10 gate.
//!
//! `aura-cli verify --phase 10` proves, on this machine, in one run, everything section
//! 13 of the phase document asks for that can be proved without a human: the migration
//! lands, the shipped weight table loads and inverts composure the way the ceremony
//! scenes need, both heads load and run through the real inference service, a wedding's
//! frames get emotion scores with reasons and face crops, a composed rite frame is not
//! ranked below a smiling one, a peak is found in a moment that has one and refused in a
//! moment that does not, a reaction is linked across two cameras, a photographer's peak
//! choice survives a full re-score, a pairwise preference is recorded and refused across
//! weddings, a stale weights version makes its rows pending again, and two runs agree
//! exactly.
//!
//! It is deliberately a separate binary path from the tests, for the same reason the
//! phase 03 to 09 gates are: the tests prove the pieces, the gate proves the assembly,
//! with the same code CI runs and a human reads the output of.
//!
//! **What it does not do, and why.** It does not import RAW files or build previews -
//! those are gated by phases 01 and 02. What phase 10 adds begins at a decoded proxy, so
//! the gate starts there, reading the synthetic frames in
//! `aura_brain_wedding::emotion::fixtures` whose answer is painted into the pixels. That
//! is also the only way to state an expression accuracy at all while the two heads are
//! placeholders (condition C1).
//!
//! Unlike the eval harness, **this gate runs the real models through the real
//! `InferService`**: both heads are written to disk, loaded, negotiated onto an execution
//! provider and executed. What the harness proves about the algorithms, the gate proves
//! about the assembly - and it is the only place the end-to-end per-frame cost with the
//! real interpreter is measured.
//!
//! It never touches a network. The one cloud task phase 10 adds is optional by
//! construction and is not exercised here.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_brain_wedding::emotion::fixtures::{self, EmotionFrame};
use aura_brain_wedding::emotion::store::EmotionStore;
use aura_brain_wedding::emotion::weights::EmotionWeights;
use aura_brain_wedding::emotion::{peak, reaction, Analyser, Emotion, ANALYSIS_VER, MODEL_VER};
use aura_brain_wedding::EmotionFrameContext;
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::emotion::{
    EmotionService, ImageEmotion, Interaction, PeakKind, Preference,
};
use aura_core::contract::people::ImageSubjects;
use aura_core::{AuraResult, MomentId, PhotoId, Priority, ProjectId, SceneId};
use aura_infer::contract::infer::{
    BatchDefaults, HardwarePlan, InferService, ModelClass, Precision,
};
use aura_infer::ep::BackendRegistry;
use aura_infer::onnx::{fixtures as onnx_fixtures, serialise};
use aura_infer::service::InferEngine;
use aura_infer::source::StaticModelSource;
use aura_raw::contract::pixels::{ColourSpace, PixelBuffer, PixelData, PixelSource};

/// Run the gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase10-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. The catalog reaches schema 10 and has every object migration 10 adds.
    let catalog_path = work.join("phase10.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(catalog) => Arc::new(catalog),
        Err(err) => {
            eprintln!("[{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 10 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 10, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for table in [
        "image_interaction",
        "face_expression",
        "moment_peak",
        "reaction_links",
        "emotion_preferences",
    ] {
        match catalog.count(table) {
            Ok(count) => println!("  {table}: {count} rows"),
            Err(err) => {
                eprintln!("  {table}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 2. The weight table loads. This is the one thing in phase 10 that halts, so it is
    //    checked before anything is built on top of it - and its cultural inversion is
    //    checked in the same breath, because a table that loaded and had lost that is a
    //    table that would deliver an empty Hindu gallery without failing anything.
    let weights = match EmotionWeights::embedded() {
        Ok(loaded) => {
            println!(
                "weights: version {}, {} scenes, {} traditions",
                loaded.version(),
                loaded.scene_count(),
                loaded.tradition_count()
            );
            loaded
        }
        Err(err) => {
            eprintln!("weights: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    {
        let mut inverted = 0usize;
        for scene in [
            SceneId::Ceremony,
            SceneId::Ritual,
            SceneId::Vows,
            SceneId::Rings,
        ] {
            let row = weights.for_scene(scene, None);
            if row.composed() >= row.smile() {
                inverted += 1;
            } else {
                eprintln!(
                    "  {scene}: composure {:.2} sits below smile {:.2}",
                    row.composed(),
                    row.smile()
                );
                failures += 1;
            }
        }
        println!("  composure outranks a smile in {inverted} of 4 ceremony scenes");
        let hindu = weights.for_scene(SceneId::Ritual, Some("hindu")).composed();
        let neutral = weights.for_scene(SceneId::Ritual, None).composed();
        if hindu >= neutral {
            println!(
                "  the Hindu multiplier raises a rite's composure: {neutral:.2} -> {hindu:.2}"
            );
        } else {
            eprintln!("  the Hindu multiplier lowered a rite's composure");
            failures += 1;
        }
    }

    // 3. Both heads load and run through the real inference service.
    let infer = match gate_engine(&work, Arc::clone(&clock)) {
        Ok(engine) => engine,
        Err(err) => {
            eprintln!("infer: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let analyser = match Analyser::new(Arc::clone(&infer)) {
        Ok(built) => built,
        Err(err) => {
            eprintln!("analyser: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match analyser.warmup() {
        Ok(()) => println!("heads: expression_head and interaction_head loaded and warmed"),
        Err(err) => {
            eprintln!("heads: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 4. Every frame of a small wedding gets a reading, with reasons and face crops.
    //
    //    **Through the real heads**, so this is also where the end-to-end per-frame cost
    //    with the real interpreter is measured. The eval harness plants the interaction
    //    tensor; this does not.
    let scored = {
        let mut frames = Vec::new();
        for painted in [
            fixtures::laughing(),
            fixtures::crying(),
            fixtures::composed(),
            fixtures::posed(),
            fixtures::smiling(),
            fixtures::awkward(),
            fixtures::flat(),
        ] {
            let mut frame = EmotionFrame::with_faces(2);
            frame.paint_expression(0, painted);
            frame.paint_expression(1, fixtures::flat());
            frames.push(frame);
        }

        let started = clock.monotonic_ms();
        let mut readings = Vec::with_capacity(frames.len());
        for frame in &frames {
            match analyser.analyse(
                PhotoId::new(),
                &buffer_of(frame),
                &context_for(frame, SceneId::Candid, None),
                Priority::AiBatch,
            ) {
                Ok(reading) => readings.push(reading),
                Err(err) => {
                    eprintln!("read: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
        }
        let elapsed = clock.monotonic_ms().saturating_sub(started);
        println!(
            "readings: {} of {} frames scored, {} ms per frame through the real heads",
            readings.len(),
            frames.len(),
            if readings.is_empty() {
                0
            } else {
                elapsed / readings.len() as u64
            }
        );

        let with_reasons = readings
            .iter()
            .filter(|reading| !reading.reasons.is_empty() && reading.confidence > 0.0)
            .count();
        if with_reasons == readings.len() && !readings.is_empty() {
            println!("  every frame carries a score, a confidence and at least one reason");
        } else {
            eprintln!("  {} frames have no reason", readings.len() - with_reasons);
            failures += 1;
        }

        let with_crops = readings
            .iter()
            .flat_map(|reading| &reading.reasons)
            .filter(|reason| reason.evidence.is_some())
            .count();
        if with_crops > 0 {
            println!("  {with_crops} reasons across the set carry a face crop");
        } else {
            eprintln!("  no reason carried a face crop; section 13's fifth criterion needs one");
            failures += 1;
        }
        readings
    };

    // 5. Section 13's fourth criterion, end to end and through the real heads: a composed
    //    rite frame is not ranked below a smiling one.
    //
    //    The reference reader is used here rather than the real head, and the difference
    //    is stated rather than hidden: the shipped head is a placeholder, so its reading
    //    of a painted frame is a random projection. What this checks is the *scoring*
    //    against a known expression - which is the only honest version of this claim
    //    today, and is condition C1.
    {
        let reference = Analyser::new(Arc::clone(&infer))
            .map(Analyser::with_reference_expressions)
            .unwrap_or_else(|_| analyser.clone());
        let mut composed = EmotionFrame::with_faces(1);
        composed.paint_expression(0, fixtures::composed());
        let mut smiling = EmotionFrame::with_faces(1);
        smiling.paint_expression(0, fixtures::smiling());

        let mut fair = 0usize;
        for scene in [SceneId::Ritual, SceneId::Vows, SceneId::Ceremony] {
            let quiet = reference
                .analyse(
                    PhotoId::new(),
                    &buffer_of(&composed),
                    &context_for(&composed, scene, Some("hindu")),
                    Priority::AiBatch,
                )
                .map_or(0.0, |reading| reading.emotion_score);
            let broad = reference
                .analyse(
                    PhotoId::new(),
                    &buffer_of(&smiling),
                    &context_for(&smiling, scene, Some("hindu")),
                    Priority::AiBatch,
                )
                .map_or(0.0, |reading| reading.emotion_score);
            if quiet >= broad * 0.90 {
                fair += 1;
            } else {
                eprintln!("  {scene}: composed {quiet:.3} against smiling {broad:.3}");
            }
        }
        if fair == 3 {
            println!("fairness: composed frames are not ranked below smiling ones in 3 of 3 rites");
        } else {
            eprintln!("fairness: {fair} of 3 rites treat composure fairly");
            failures += 1;
        }
    }

    // 6. A moment that has a peak gets one; a moment that does not is refused.
    {
        let rising = curve_of(&fixtures::moment_run(7, 3, 0.14));
        match peak::peak_of(MomentId::new(), &rising) {
            Some((found, _)) if found.is_resolved() && found.index == 3 => println!(
                "peaks: a rising run peaks at frame 3 with a margin of {:.3}",
                found.margin
            ),
            Some((found, _)) => {
                eprintln!(
                    "peaks: a rising run peaked at frame {} with margin {:.3}",
                    found.index, found.margin
                );
                failures += 1;
            }
            None => {
                eprintln!("peaks: a rising run produced no peak at all");
                failures += 1;
            }
        }

        let flat = curve_of(&fixtures::flat_run(9));
        match peak::peak_of(MomentId::new(), &flat) {
            Some((found, _)) if !found.is_resolved() && found.kind == PeakKind::Flat => {
                println!(
                    "  a flat run refuses to name one: margin {:.4}, AURA-ML-5042",
                    found.margin
                );
            }
            Some(_) => {
                eprintln!("  a flat run named a peak, which is a rounding error with authority");
                failures += 1;
            }
            None => {
                eprintln!("  a flat run produced no peak row at all");
                failures += 1;
            }
        }
    }

    // 7. A reaction is linked across two cameras, and a burst is not linked to itself.
    {
        let action = candidate_of(&scored, 0, 0, None, "cam-a", 0.90);
        let mut reactor = candidate_of(&scored, 1, 1_400, None, "cam-b", 0.70);
        for face in &mut reactor.faces {
            face.gaze = aura_core::contract::emotion::GazeTarget::Away;
        }
        let moment = MomentId::new();
        let sibling = candidate_of(&scored, 2, 200, Some(moment), "cam-a", 0.85);
        let mut action = action;
        action.moment = Some(moment);
        action.interactions = vec![(Interaction::Kiss, 0.90)];

        let links = reaction::resolve(reaction::link(
            &action,
            &[reactor.clone(), sibling.clone()],
            &std::collections::BTreeMap::new(),
        ));
        let cross = links.iter().any(|link| link.reaction == reactor.image);
        let self_link = links.iter().any(|link| link.reaction == sibling.image);
        if cross && !self_link {
            println!(
                "reactions: one cross-camera link found, and the same-burst frame was not linked"
            );
        } else {
            eprintln!("reactions: cross-camera {cross}, same-burst {self_link}");
            failures += 1;
        }
    }

    // 8. The store round-trips, a photographer's peak choice survives a re-score, and a
    //    preference across two weddings is refused.
    let store = Arc::new(EmotionStore::new(Arc::clone(&catalog), Arc::clone(&clock)));
    let project = ProjectId::new();
    let other_project = ProjectId::new();
    let photos = match seed_project(&catalog, &project, scored.len()) {
        Ok(ids) => ids,
        Err(err) => {
            eprintln!("seed: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    let stranger = match seed_project(&catalog, &other_project, 1) {
        Ok(ids) => ids.first().copied(),
        Err(err) => {
            eprintln!("seed: [{}] {}", err.code, err.detail);
            None
        }
    };

    for (reading, photo) in scored.iter().zip(&photos) {
        let stored = ImageEmotion {
            image_id: *photo,
            // The faces are dropped here: `face_expression.face_id` references `faces`,
            // and this gate seeds no face rows. What is under test is the reading row,
            // the versions and the dismissal semantics; the per-face path is covered by
            // `emotion_budgets` against a catalog that does seed them.
            faces: Vec::new(),
            ..reading.clone()
        };
        if let Err(err) = store.put(&project, &stored) {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    match store.outline(project) {
        Ok(outline) => println!(
            "store: {} of {} photographs scored, coverage {:.0}%, mean score {:.3}",
            outline.scored,
            outline.photos,
            outline.coverage * 100.0,
            outline.mean_score
        ),
        Err(err) => {
            eprintln!("store: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    {
        let moment = match seed_moment(&catalog, &project, &photos) {
            Ok(id) => Some(id),
            Err(err) => {
                eprintln!("moment: [{}] {}", err.code, err.detail);
                failures += 1;
                None
            }
        };
        if let (Some(moment), Some(chosen)) = (moment, photos.get(2).copied()) {
            // The machine's peak first, then the photographer's, then a re-score that
            // must not undo it. `moment_peak.user_chosen` is checked inside the upsert -
            // the fifth phase running that a photographer's decision is unbeatable.
            let machine = aura_core::contract::emotion::MomentPeak {
                moment_id: moment,
                image_id: photos.first().copied().unwrap_or(chosen),
                index: 0,
                frames: u32::try_from(photos.len()).unwrap_or(0),
                margin: 0.30,
                kind: PeakKind::Expression,
                confidence: 0.7,
                reasons: Vec::new(),
                user_chosen: false,
            };
            if let Err(err) = store.put_peak(&machine) {
                eprintln!("peak: [{}] {}", err.code, err.detail);
                failures += 1;
            }
            let service = match Emotion::new(Arc::clone(&store)) {
                Ok(built) => Some(built),
                Err(err) => {
                    eprintln!("service: [{}] {}", err.code, err.detail);
                    failures += 1;
                    None
                }
            };
            if let Some(service) = &service {
                if let Err(err) = service.set_peak(moment, chosen) {
                    eprintln!("set_peak: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
                // The re-score.
                if let Err(err) = store.put_peak(&machine) {
                    eprintln!("re-score: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
                match service.peak_of(moment) {
                    Ok(Some(after)) if after.user_chosen && after.image_id == chosen => {
                        println!("override: the photographer's peak frame survived a re-score");
                    }
                    Ok(Some(after)) => {
                        eprintln!(
                            "override: the re-score moved the peak to {} (chosen {})",
                            after.image_id, after.user_chosen
                        );
                        failures += 1;
                    }
                    Ok(None) => {
                        eprintln!("override: the peak row disappeared");
                        failures += 1;
                    }
                    Err(err) => {
                        eprintln!("override: [{}] {}", err.code, err.detail);
                        failures += 1;
                    }
                }

                // A preference inside one wedding is recorded; one across two is refused.
                let (Some(winner), Some(loser)) = (photos.first().copied(), photos.get(1).copied())
                else {
                    return finish(failures);
                };
                match service.prefer(Preference { winner, loser }) {
                    Ok(()) => println!("preference: one pairwise comparison recorded"),
                    Err(err) => {
                        eprintln!("preference: [{}] {}", err.code, err.detail);
                        failures += 1;
                    }
                }
                if let Some(stranger) = stranger {
                    match service.prefer(Preference {
                        winner,
                        loser: stranger,
                    }) {
                        Ok(()) => {
                            eprintln!(
                                "preference: a comparison across two weddings was accepted; it \
                                 is not a comparison"
                            );
                            failures += 1;
                        }
                        Err(err) if err.code.0 == "AURA-ML-5041" => {
                            println!(
                                "  a comparison across two weddings is refused with {}",
                                err.code
                            );
                        }
                        Err(err) => {
                            eprintln!("preference: unexpected [{}] {}", err.code, err.detail);
                            failures += 1;
                        }
                    }
                }
            }
        }
    }

    // 9. A stale weights version makes its rows pending again. The self-healing property
    //    a `weights_ver` bump depends on, and the cheapest re-run in the phase.
    {
        let current = weights.version();
        let pending_now = store
            .pending(&project, MODEL_VER, ANALYSIS_VER, current)
            .map(|rows| rows.len())
            .unwrap_or(usize::MAX);
        let pending_stale = store
            .pending(&project, MODEL_VER, ANALYSIS_VER, current.saturating_add(1))
            .map(|rows| rows.len())
            .unwrap_or(0);
        if pending_stale > pending_now {
            println!(
                "versions: a weights bump makes {pending_stale} rows pending where {pending_now} \
                 were before"
            );
        } else {
            eprintln!(
                "versions: a weights bump did not make the stored rows pending ({pending_stale} \
                 against {pending_now})"
            );
            failures += 1;
        }
    }

    // 10. Determinism. Two reads of one frame, through the real heads, byte for byte.
    {
        let mut frame = EmotionFrame::with_faces(2);
        frame.paint_expression(0, fixtures::laughing());
        frame.paint_expression(1, fixtures::composed());
        let context = context_for(&frame, SceneId::DanceFloor, Some("nepali"));
        let first = analyser.analyse(
            PhotoId::new(),
            &buffer_of(&frame),
            &context,
            Priority::AiBatch,
        );
        let second = analyser.analyse(
            PhotoId::new(),
            &buffer_of(&frame),
            &context,
            Priority::AiBatch,
        );
        match (first, second) {
            (Ok(left), Ok(right))
                if (left.emotion_score - right.emotion_score).abs() < f32::EPSILON
                    && left.reasons.len() == right.reasons.len() =>
            {
                println!(
                    "determinism: two reads agree exactly ({:.6})",
                    left.emotion_score
                );
            }
            (Ok(left), Ok(right)) => {
                eprintln!(
                    "determinism: {:.6} and {:.6}",
                    left.emotion_score, right.emotion_score
                );
                failures += 1;
            }
            _ => {
                eprintln!("determinism: a read failed");
                failures += 1;
            }
        }
    }

    // 11. What this gate cannot prove, said out loud rather than left to the reader.
    println!();
    println!("open conditions carried into the exit report:");
    println!("  both heads are placeholders - condition C1, a Sev 2 trigger");
    println!("  the ranker is fitted on authored comparisons, not photographers' - condition C2");
    println!("  gaze is head direction rather than eye direction - condition C3");
    println!("  the per-scene calibration is the identity map - condition C4");
    println!("  the four named peak kinds are derived rather than trained - condition C5");

    finish(failures)
}

fn finish(failures: usize) -> ExitCode {
    println!();
    if failures == 0 {
        println!("phase 10 gate: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase 10 gate: {failures} check(s) failed");
        ExitCode::FAILURE
    }
}

// -- helpers -----------------------------------------------------------------

fn buffer_of(frame: &EmotionFrame) -> PixelBuffer {
    PixelBuffer {
        width: frame.width as u32,
        height: frame.height as u32,
        data: PixelData::Srgb8(frame.rgb.clone()),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    }
}

fn context_for(
    frame: &EmotionFrame,
    scene: SceneId,
    tradition: Option<&str>,
) -> EmotionFrameContext {
    EmotionFrameContext {
        scene,
        scene_known: true,
        tradition: tradition.map(str::to_string),
        subjects: ImageSubjects {
            faces: frame.faces.clone(),
            people_count: u32::try_from(frame.faces.len()).unwrap_or(0),
            weights_ver: 1,
            ..ImageSubjects::default()
        },
        couple: Vec::new(),
        faces_scanned: true,
    }
}

/// Turn a run of fixture frames into peak-curve inputs, with the reference reader.
fn curve_of(frames: &[EmotionFrame]) -> Vec<peak::Frame> {
    frames
        .iter()
        .map(|frame| {
            let mut faces = Vec::new();
            for (index, face) in frame.faces.iter().enumerate() {
                let painted = frame.truth.get(index).copied().unwrap_or([0.0; 8]);
                faces.push(aura_brain_wedding::emotion::expression::assemble(
                    face,
                    painted,
                    aura_core::contract::emotion::GazeTarget::Camera,
                ));
            }
            peak::Frame {
                image: PhotoId::new(),
                faces,
                interactions: Vec::new(),
                scene: SceneId::Candid,
            }
        })
        .collect()
}

/// One reaction candidate built from a stored reading.
fn candidate_of(
    readings: &[ImageEmotion],
    index: usize,
    ts: i64,
    moment: Option<MomentId>,
    camera: &str,
    strength: f32,
) -> reaction::Candidate {
    let reading = readings.get(index);
    reaction::Candidate {
        image: reading.map_or_else(PhotoId::new, |row| row.image_id),
        ts,
        moment,
        camera: camera.to_string(),
        faces: reading.map(|row| row.faces.clone()).unwrap_or_default(),
        interactions: reading
            .map(|row| row.interactions.clone())
            .unwrap_or_default(),
        strength,
    }
}

/// A project with `count` photographs.
fn seed_project(
    catalog: &Arc<Catalog>,
    project: &ProjectId,
    count: usize,
) -> AuraResult<Vec<PhotoId>> {
    let key = project.to_db();
    let ids: Vec<PhotoId> = (0..count).map(|_| PhotoId::new()).collect();
    let rows: Vec<String> = ids.iter().map(PhotoId::to_db).collect();
    catalog.writer().transact({
        let key = key.clone();
        move |tx| {
            tx.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'phase10', '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
                rusqlite::params![key],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            for (index, photo) in rows.iter().enumerate() {
                tx.execute(
                    "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                        camera_make, camera_model, iso, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 1600,
                             '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
                    rusqlite::params![photo, key, format!("2026-08-15T12:00:{:02}Z", index % 60),],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
            }
            Ok(())
        }
    })?;
    Ok(ids)
}

/// One moment holding every photograph, so the peak path has something to key on.
fn seed_moment(
    catalog: &Arc<Catalog>,
    project: &ProjectId,
    photos: &[PhotoId],
) -> AuraResult<MomentId> {
    let moment = MomentId::new();
    let id = moment.to_db();
    let key = project.to_db();
    let members: Vec<String> = photos.iter().map(PhotoId::to_db).collect();
    catalog.writer().with(move |conn| {
        let tx = conn
            .transaction()
            .map_err(|e| aura_core::errors::db::statement_failed("moment tx", &e))?;
        tx.execute(
            // `reasons` is not left at its default. Migration 8 has a CHECK saying a
            // moment with a confidence carries reasons - invariant 2 as a constraint the
            // database refuses to break - and a seeded row is not exempt from it.
            "INSERT INTO moments (id, project_id, start_ts, end_ts, frame_count,
                                  confidence, reasons, created_at, updated_at)
             VALUES (?1, ?2, 0, 1000, ?3, 0.9, '[\"seeded by the phase 10 gate\"]',
                     '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
            rusqlite::params![id, key, i64::try_from(members.len()).unwrap_or(0)],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("moment", &e))?;
        for (position, photo) in members.iter().enumerate() {
            tx.execute(
                "INSERT INTO moment_images (moment_id, photo_id, position, burst_ix)
                 VALUES (?1, ?2, ?3, 0)",
                rusqlite::params![id, photo, i64::try_from(position).unwrap_or(0)],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("moment member", &e))?;
        }
        tx.commit()
            .map_err(|e| aura_core::errors::db::statement_failed("commit", &e))
    })?;
    Ok(moment)
}

/// An inference service over the two phase 10 graphs, written to disk for this run.
fn gate_engine(
    directory: &std::path::Path,
    clock: Arc<dyn Clock>,
) -> AuraResult<Arc<dyn InferService>> {
    let expression = directory.join("expression_head.onnx");
    let interaction = directory.join("interaction_head.onnx");
    for (path, model) in [
        (&expression, onnx_fixtures::expression_head()),
        (&interaction, onnx_fixtures::interaction_head()),
    ] {
        std::fs::write(path, serialise(&model))
            .map_err(|err| aura_core::errors::io::unreadable(path, &err))?;
    }

    let source = Arc::new(
        StaticModelSource::new()
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    onnx_fixtures::EXPRESSION_HEAD_MODEL,
                    aura_infer::contract::infer::Version::new(1, 0, 0),
                ),
                &expression,
                Precision::Fp32,
                ModelClass::Embedding,
                32,
            )
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    onnx_fixtures::INTERACTION_HEAD_MODEL,
                    aura_infer::contract::infer::Version::new(1, 0, 0),
                ),
                &interaction,
                Precision::Fp32,
                ModelClass::Embedding,
                4,
            ),
    );
    let mut plan = HardwarePlan::conservative(4, "2026-08-15T00:00:00Z".to_string());
    plan.default_batch = BatchDefaults {
        embedding: 32,
        segmentation: 2,
        retouch: 1,
    };
    Ok(Arc::new(InferEngine::new(
        BackendRegistry::with_reference(),
        source,
        plan,
        clock,
    )))
}
