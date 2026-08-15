//! The phase 09 gate.
//!
//! `aura-cli verify --phase 09` proves, on this machine, in one run, everything section
//! 13 of the phase document asks for that can be proved without a human: the migration
//! lands, the shipped calibration table loads and is fair, both heads load and run
//! through the real inference service, a wedding's frames get technical verdicts with
//! reasons and evidence crops, a shallow-depth-of-field portrait is never called soft, a
//! kiss with closed eyes is exonerated rather than flagged, shake and a pan are
//! distinguished, a candle is not a blown highlight, a photographer's dismissal survives
//! a full re-analysis, the within-moment ranking answers phase 12's question, a stale
//! calibration version makes its rows pending again, and two runs agree exactly.
//!
//! It is deliberately a separate binary path from the tests, for the same reason the
//! phase 03 to 08 gates are: the tests prove the pieces, the gate proves the assembly,
//! with the same code CI runs and a human reads the output of.
//!
//! **What it does not do, and why.** It does not import RAW files or build previews -
//! those are gated by phases 01 and 02. What phase 09 adds begins at a decoded proxy, so
//! the gate starts there, analysing the synthetic frames in `aura_brain_photo::fixtures`
//! whose answer is known by construction. That is also the only way to state a technical
//! accuracy at all while the two heads are placeholders (condition C1).
//!
//! Unlike the eval harness, **this gate runs the real models through the real
//! `InferService`**: the focus head is loaded from disk, negotiated onto an execution
//! provider and executed. What the harness proves about the algorithms, the gate proves
//! about the assembly.
//!
//! It never touches a network - nothing in phase 09 can.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_brain_photo::fixtures::{self, Frame};
use aura_brain_photo::integrity::calibration::CalibrationTable;
use aura_brain_photo::integrity::store::{IntegrityStore, Provenance};
use aura_brain_photo::integrity::{Analyser, ANALYSIS_VER, MODEL_VER};
use aura_brain_photo::{FrameContext, FrameExif};
use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::integrity::{
    IntegrityFlags, IntegrityResult, IntegrityService, MotionKind,
};
use aura_core::contract::people::{FaceRef, ImageSubjects};
use aura_core::{AuraResult, PhotoId, Priority, ProjectId, SceneId, SceneProfile};
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
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase09-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }

    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // 1. The catalog reaches schema 9 and has every object migration 9 adds.
    let catalog_path = work.join("phase09.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(catalog) => Arc::new(catalog),
        Err(err) => {
            eprintln!("[{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 9 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 9, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }
    for table in ["image_integrity", "face_eye_state"] {
        match catalog.count(table) {
            Ok(count) => println!("  {table}: {count} rows"),
            Err(err) => {
                eprintln!("  {table}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    // 2. The calibration table loads. This is the one thing in phase 09 that halts, so
    //    it is checked before anything is built on top of it.
    let calibration = match CalibrationTable::embedded() {
        Ok(loaded) => {
            println!(
                "calibration: version {}, {} bodies measured",
                loaded.version(),
                loaded.rows()
            );
            loaded
        }
        Err(err) => {
            eprintln!("calibration: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    // The counter-intuitive direction, checked rather than trusted: a 61 MP body must
    // expect a *lower* per-output-pixel edge response than a 24 MP one, or the fairness
    // gate is dividing by the wrong number and the expensive camera wins.
    let low = calibration.get("SONY", "ILCE-7M3", 24.2);
    let high = calibration.get("SONY", "ILCE-7RM4", 61.0);
    if high.expected_mtf50 < low.expected_mtf50 {
        println!(
            "  the 61 MP body expects {:.2} against the 24 MP body's {:.2}, as section 6.1 \
             requires",
            high.expected_mtf50, low.expected_mtf50
        );
    } else {
        eprintln!("  the calibration table is the wrong way round");
        failures += 1;
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
        Ok(()) => println!(
            "models: focus_head and eye_state loaded on {}",
            analyser.ep()
        ),
        Err(err) => {
            eprintln!("models: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 4. A wedding's worth of frames gets verdicts, through the real analyser and the
    //    real store. The eye states come from the fixture marker - see
    //    `Analyser::with_reference_eyes` - because the shipped head is a placeholder and
    //    a blink number measured against it would describe a random projection.
    let reference = analyser.clone().with_reference_eyes();
    let store = Arc::new(IntegrityStore::new(
        Arc::clone(&catalog),
        Arc::clone(&clock),
    ));
    let integrity = aura_brain_photo::integrity::Integrity::new(Arc::clone(&store));
    let project = ProjectId::new();

    let mut wedding = gate_wedding();
    // Every fixture is built from the same subject rectangle, so their derived face ids
    // collide. Salting by position makes them distinct photographs of distinct faces,
    // which is what the catalog's primary key expects.
    for (index, (_, frame, _)) in wedding.iter_mut().enumerate() {
        frame.salt_faces(i32::try_from(index).unwrap_or(0));
    }
    let wedding = wedding;
    let planted = match plant(&catalog, &project, &wedding) {
        Ok(planted) => planted,
        Err(err) => {
            eprintln!("fixtures: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    println!(
        "fixtures: {} frames across {} situations",
        planted.len(),
        wedding.len()
    );

    let mut verdicts = Vec::new();
    for ((label, frame, scene), photo) in wedding.iter().zip(&planted) {
        let context = context_for(frame, *scene, *photo);
        match reference.analyse(*photo, &buffer_of(frame), &context, Priority::AiBatch) {
            Ok(result) => {
                if let Err(err) = store.put(
                    &project,
                    &result,
                    &Provenance {
                        uncalibrated: false,
                    },
                ) {
                    eprintln!("  store {label}: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
                verdicts.push((label.to_string(), result));
            }
            Err(err) => {
                eprintln!("  analyse {label}: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
    }

    let outline = match integrity.outline(project) {
        Ok(outline) => outline,
        Err(err) => {
            eprintln!("outline: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    if outline.scored as usize == planted.len() {
        println!(
            "verdicts: {} of {} frames checked, coverage {:.0}%, {:.0}% subject-aware, mean \
             score {:.2}",
            outline.scored,
            outline.photos,
            outline.coverage * 100.0,
            outline.subject_aware * 100.0,
            outline.mean_score
        );
    } else {
        eprintln!(
            "  {} frames planted but {} carry a verdict",
            planted.len(),
            outline.scored
        );
        failures += 1;
    }

    // 5. Section 13's acceptance criteria, one line each.
    let find = |label: &str| -> Option<&IntegrityResult> {
        verdicts
            .iter()
            .find(|(name, _)| name == label)
            .map(|(_, result)| result)
    };

    // Criterion 1: every image exposes the five measurements with reasons.
    let complete = verdicts.iter().all(|(_, result)| {
        !result.reasons.is_empty() && result.confidence > 0.0 && result.technical_score > 0.0
    });
    if complete {
        println!("  every frame carries a score, a confidence and at least one reason");
    } else {
        eprintln!("  a frame came back without a reason or a confidence");
        failures += 1;
    }

    // Criterion 2: a shallow-depth-of-field portrait is never called soft.
    match find("shallow depth of field") {
        Some(result) if !result.flags.contains(IntegrityFlags::SUBJECT_SOFT) => {
            println!(
                "  a bokeh portrait is not soft: subject {:.2}, background {:.2}",
                result.subject_sharpness, result.bg_sharpness
            );
        }
        Some(result) => {
            eprintln!(
                "  a bokeh portrait was called soft at subject sharpness {:.2}",
                result.subject_sharpness
            );
            failures += 1;
        }
        None => {
            eprintln!("  the bokeh fixture did not analyse");
            failures += 1;
        }
    }

    // Criterion 3: a kiss with closed eyes is exonerated, not flagged.
    match find("a kiss, eyes closed") {
        Some(result)
            if result.flags.contains(IntegrityFlags::EYES_CLOSED_OK)
                && !result.flags.contains(IntegrityFlags::EYES_CLOSED) =>
        {
            println!(
                "  a kiss with closed eyes is exonerated: {} of {} gating faces, and the panel \
                 says why",
                result.eyes.iter().filter(|eye| eye.intentional).count(),
                result.gating_faces
            );
        }
        Some(_) => {
            eprintln!("  a kiss with closed eyes was marked as a defect");
            failures += 1;
        }
        None => {
            eprintln!("  the kiss fixture did not analyse");
            failures += 1;
        }
    }

    // Criterion 4: shake and a pan are distinguished.
    let shake = find("camera shake").map(|result| result.motion);
    let pan = find("a panned exit").map(|result| result.motion);
    if shake == Some(MotionKind::CameraShake) && pan == Some(MotionKind::Intentional) {
        println!("  camera shake and a panned exit are told apart, and the pan is not a defect");
    } else {
        eprintln!("  motion: shake read as {shake:?} and the pan as {pan:?}");
        failures += 1;
    }

    // Section 10.1: a candle is not a blown highlight.
    match (find("a candle"), find("a blown dress")) {
        (Some(candle), Some(blown))
            if !candle.flags.contains(IntegrityFlags::HIGHLIGHT_LOST)
                && blown.flags.contains(IntegrityFlags::HIGHLIGHT_LOST) =>
        {
            println!("  a candle flame is a light source and a blown dress is a loss");
        }
        _ => {
            eprintln!("  the specular exclusion did not separate a flame from a blown frame");
            failures += 1;
        }
    }

    // Criterion 6: the card can show the crop that caused each penalty.
    let evidence = verdicts
        .iter()
        .flat_map(|(_, result)| &result.reasons)
        .filter(|reason| reason.is_penalty() && reason.evidence.is_some())
        .count();
    println!("  {evidence} penalties across the set carry an evidence rectangle");

    // 6. The photographer outranks the automation. A dismissal survives a re-analysis.
    let soft_frame = verdicts
        .iter()
        .find(|(_, result)| result.flags.contains(IntegrityFlags::SUBJECT_SOFT))
        .map(|(_, result)| result.image_id);
    if let Some(photo) = soft_frame {
        match integrity.dismiss(photo, IntegrityFlags::SUBJECT_SOFT) {
            Ok(()) => println!("dismissal: the photographer overruled a softness mark"),
            Err(err) => {
                eprintln!("dismissal: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        }
        // Re-analyse the same frame from scratch and store it again. The dismissal must
        // be re-applied inside the upsert rather than reverted by it.
        if let Some(index) = planted.iter().position(|id| *id == photo) {
            if let Some((label, frame, scene)) = wedding.get(index) {
                let context = context_for(frame, *scene, photo);
                if let Ok(result) =
                    reference.analyse(photo, &buffer_of(frame), &context, Priority::AiBatch)
                {
                    drop(store.put(
                        &project,
                        &result,
                        &Provenance {
                            uncalibrated: false,
                        },
                    ));
                }
                match integrity.of_image(photo) {
                    Ok(Some(after))
                        if !after.flags.contains(IntegrityFlags::SUBJECT_SOFT)
                            && after.user_reviewed =>
                    {
                        println!("  the dismissal survived a full re-analysis of {label}");
                    }
                    Ok(Some(_)) => {
                        eprintln!("  the re-analysis put the dismissed mark back");
                        failures += 1;
                    }
                    Ok(None) | Err(_) => {
                        eprintln!("  the re-analysed verdict could not be read back");
                        failures += 1;
                    }
                }
            }
        }
        // And a dismissal of something that is not a fault is refused.
        match integrity.dismiss(photo, IntegrityFlags::NO_SUBJECT_DETECTED) {
            Err(err) if err.code.0 == "AURA-ML-5034" => {
                println!(
                    "  dismissing a mark that is not a fault is refused with {}",
                    err.code
                );
            }
            _ => {
                eprintln!("  a mark that is not a fault was dismissable");
                failures += 1;
            }
        }
    } else {
        eprintln!("  no frame in the fixture set was marked soft, so the dismissal was untested");
        failures += 1;
    }

    // 7. Phase 12's question: which of these six is sharpest.
    match plant_moment(&catalog, &project, &planted) {
        Ok(moment) => match integrity.within_moment(moment) {
            Ok(ranked) if ranked.len() >= 2 => {
                let sharpest = ranked.first().map(|(_, rank)| *rank).unwrap_or(0.0);
                let softest = ranked.last().map(|(_, rank)| *rank).unwrap_or(0.0);
                if sharpest > softest {
                    println!(
                        "within-moment: {} frames ranked, sharpest {sharpest:.2}, softest \
                         {softest:.2}",
                        ranked.len()
                    );
                } else {
                    eprintln!("  the within-moment ranking is not ordered");
                    failures += 1;
                }
            }
            Ok(_) => {
                eprintln!("  the within-moment ranking came back empty");
                failures += 1;
            }
            Err(err) => {
                eprintln!("within-moment: [{}] {}", err.code, err.detail);
                failures += 1;
            }
        },
        Err(err) => {
            eprintln!("moment: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 8. A stale calibration version makes its rows pending again, without anybody
    //    triggering anything. This is what makes `AURA-ML-5033` self-healing.
    match store.pending(&project, MODEL_VER, ANALYSIS_VER, calibration.version()) {
        Ok(pending) if pending.is_empty() => {
            match store.pending(&project, MODEL_VER, ANALYSIS_VER, calibration.version() + 1) {
                Ok(stale) if stale.len() == planted.len() => {
                    println!(
                        "versions: nothing pending at the current table, all {} frames pending \
                         at the next one",
                        stale.len()
                    );
                }
                Ok(stale) => {
                    eprintln!(
                        "  a calibration bump left {} of {} pending",
                        stale.len(),
                        planted.len()
                    );
                    failures += 1;
                }
                Err(err) => {
                    eprintln!("versions: [{}] {}", err.code, err.detail);
                    failures += 1;
                }
            }
        }
        Ok(pending) => {
            eprintln!(
                "  {} frames are pending at the version they were written at",
                pending.len()
            );
            failures += 1;
        }
        Err(err) => {
            eprintln!("versions: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    // 9. Determinism, invariant 4. Two analyses of one frame, byte-identical where it
    //    matters.
    if let Some((label, frame, scene)) = wedding.first() {
        let photo = planted.first().copied().unwrap_or_else(PhotoId::new);
        let context = context_for(frame, *scene, photo);
        let first = reference.analyse(photo, &buffer_of(frame), &context, Priority::AiBatch);
        let second = reference.analyse(photo, &buffer_of(frame), &context, Priority::AiBatch);
        match (first, second) {
            (Ok(a), Ok(b))
                if a.flags == b.flags
                    && a.motion == b.motion
                    && a.exposure == b.exposure
                    && (a.technical_score - b.technical_score).abs() < f32::EPSILON =>
            {
                println!("determinism: two runs of {label} agree exactly");
            }
            (Ok(_), Ok(_)) => {
                eprintln!("  two runs of one frame disagreed");
                failures += 1;
            }
            _ => {
                eprintln!("  a determinism run failed");
                failures += 1;
            }
        }
    }

    // 10. What the phase cannot claim, said in the gate's own output so that a reader of
    //     a CI log sees the same list the exit report carries.
    println!("deferred, and why:");
    println!("  the focus head and the eye head ship untrained - condition C1");
    println!("  the twenty calibration rows are derived, not measured - condition C2");
    println!("  clipping is measured on the proxy, not the RAW histogram - condition C3");
    println!("  the tears intent rule needs phase 10 - condition C4");
    println!("  the per-scene score calibration is the identity map - condition C5");

    if failures == 0 {
        println!("phase-09 verify: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!("phase-09 verify: {failures} failures");
        ExitCode::FAILURE
    }
}

/// The situations the gate checks, each with the scene it would be shot in.
fn gate_wedding() -> Vec<(&'static str, Frame, SceneId)> {
    vec![
        ("a sharp portrait", Frame::base(), SceneId::CouplePortrait),
        (
            "shallow depth of field",
            fixtures::shallow_depth_of_field(),
            SceneId::CouplePortrait,
        ),
        ("a soft frame", soft_frame(), SceneId::FamilyPortrait),
        ("camera shake", fixtures::camera_shake(), SceneId::Ceremony),
        ("a panned exit", fixtures::panned(), SceneId::Exit),
        (
            "a blown dress",
            fixtures::clipped(),
            SceneId::CouplePortrait,
        ),
        ("a candle", fixtures::candle(), SceneId::Ceremony),
        (
            "a kiss, eyes closed",
            fixtures::group_frame(2, 2),
            SceneId::Kiss,
        ),
        (
            "a group, one blinking",
            fixtures::group_frame(4, 1),
            SceneId::GroupPortrait,
        ),
        ("a noisy reception", noisy_frame(), SceneId::DanceFloor),
    ]
}

/// A frame whose subject is clearly out of focus.
fn soft_frame() -> Frame {
    let mut frame = Frame::base();
    frame.blur_rect(fixtures::FACE_RECT, 6);
    frame
}

/// A frame with noise well past what a dance floor tolerates.
fn noisy_frame() -> Frame {
    let mut frame = Frame::base();
    frame.add_noise(0.06, 0x0000_9015_E000_0001);
    frame
}

fn buffer_of(frame: &Frame) -> PixelBuffer {
    PixelBuffer {
        width: frame.width as u32,
        height: frame.height as u32,
        data: PixelData::Srgb8(frame.rgb.clone()),
        colour_space: ColourSpace::Srgb,
        source: PixelSource::Demosaiced,
        decode_ms: 0,
    }
}

fn context_for(frame: &Frame, scene: SceneId, _photo: PhotoId) -> FrameContext {
    FrameContext {
        scene,
        profile: SceneProfile::neutral(scene),
        subjects: ImageSubjects {
            faces: frame.faces.clone(),
            people_count: u32::try_from(frame.faces.len()).unwrap_or(0),
            weights_ver: 1,
            ..ImageSubjects::default()
        },
        couple: Vec::new(),
        exif: FrameExif {
            make: "SONY".into(),
            model: "ILCE-7M3".into(),
            iso: 3200,
            shutter_s: Some(1.0 / 60.0),
            focal_mm: Some(85.0),
            stabilised: false,
            megapixels: 24.2,
        },
        scene_known: true,
        // Phase 10's third intent rule. False here, which is what a wedding with no
        // emotion pass gets and is exactly what phase 09 shipped with: the gate that
        // exercises the rule with tears present is `aura-cli verify --phase 10`.
        tears: false,
    }
}

/// Write the project, its photographs and their faces.
fn plant(
    catalog: &Arc<Catalog>,
    project: &ProjectId,
    wedding: &[(&'static str, Frame, SceneId)],
) -> AuraResult<Vec<PhotoId>> {
    let key = project.to_db();
    let rows: Vec<(PhotoId, Vec<FaceRef>)> = wedding
        .iter()
        .map(|(_, frame, _)| (PhotoId::new(), frame.faces.clone()))
        .collect();
    let planted: Vec<PhotoId> = rows.iter().map(|(photo, _)| *photo).collect();
    let owned = rows.clone();

    catalog.writer().with(move |conn| {
        let tx = conn
            .transaction()
            .map_err(|e| aura_core::errors::db::statement_failed("open", &e))?;
        tx.execute(
            "INSERT INTO project (project_id, name, created_at, updated_at)
             VALUES (?1, 'phase-09', '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
            rusqlite::params![key],
        )
        .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;

        for (index, (photo, faces)) in owned.iter().enumerate() {
            tx.execute(
                "INSERT INTO photo (photo_id, project_id, capture_time, timeline_time,
                                    camera_make, camera_model, iso, width_px, height_px,
                                    created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?3, 'SONY', 'ILCE-7M3', 3200, 6000, 4000,
                         '2026-08-15T00:00:00Z', '2026-08-15T00:00:00Z')",
                rusqlite::params![
                    photo.to_db(),
                    key,
                    format!("2026-08-15T12:{:02}:{:02}Z", index / 60, index % 60),
                ],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;

            for face in faces {
                tx.execute(
                    "INSERT INTO faces (id, image_id, project_id, x, y, w, h, det_score,
                                        quality, blur, occlusion, landmarks_json, area_frac,
                                        centrality, sharpness, model_ver, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0.9, 0.8, 0.1, 0.0, ?8, ?9, ?10, 0.8,
                             100, '2026-08-15T00:00:00Z')",
                    rusqlite::params![
                        face.face_id.to_db(),
                        photo.to_db(),
                        key,
                        f64::from(face.bbox.x),
                        f64::from(face.bbox.y),
                        f64::from(face.bbox.w),
                        f64::from(face.bbox.h),
                        format!(
                            "[[{},{}],[{},{}],[0.0,0.0],[0.0,0.0],[0.0,0.0]]",
                            face.eyes[0][0], face.eyes[0][1], face.eyes[1][0], face.eyes[1][1]
                        ),
                        f64::from(face.area_frac),
                        f64::from(face.centrality),
                    ],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("face", &e))?;
            }
        }
        tx.commit()
            .map_err(|e| aura_core::errors::db::statement_failed("commit", &e))
    })?;

    Ok(planted)
}

/// One moment over the first four frames, so the within-moment ranking has something to
/// rank.
///
/// Written with SQL rather than through `MomentService`, deliberately and narrowly: this
/// gate is about phase 09, the grouping it would produce is phase 08's business and is
/// gated by `verify --phase 08`, and building a real grouping here would mean planting
/// embeddings for a question that is about ranking rather than about grouping.
fn plant_moment(
    catalog: &Arc<Catalog>,
    project: &ProjectId,
    photos: &[PhotoId],
) -> AuraResult<aura_core::MomentId> {
    let moment = aura_core::MomentId::new();
    let key = project.to_db();
    let id = moment.to_db();
    let members: Vec<String> = photos.iter().take(4).map(PhotoId::to_db).collect();

    catalog.writer().with(move |conn| {
        let tx = conn
            .transaction()
            .map_err(|e| aura_core::errors::db::statement_failed("open", &e))?;
        tx.execute(
            "INSERT INTO moments (id, project_id, start_ts, end_ts, frame_count, confidence,
                                  reasons, created_at, updated_at)
             VALUES (?1, ?2, 0, 1000, ?3, 0.9, '[\"the gate planted this\"]',
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

/// An inference service over the two phase 09 graphs, written to disk for this run.
fn gate_engine(
    directory: &std::path::Path,
    clock: Arc<dyn Clock>,
) -> AuraResult<Arc<dyn InferService>> {
    let focus = directory.join("focus_head.onnx");
    let eyes = directory.join("eye_state.onnx");
    for (path, model) in [
        (&focus, onnx_fixtures::focus_head()),
        (&eyes, onnx_fixtures::eye_state()),
    ] {
        std::fs::write(path, serialise(&model))
            .map_err(|err| aura_core::errors::io::unreadable(path, &err))?;
    }

    let source = Arc::new(
        StaticModelSource::new()
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_brain_photo::integrity::focus::FOCUS_MODEL,
                    aura_brain_photo::integrity::focus::FOCUS_MODEL_VERSION,
                ),
                &focus,
                Precision::Fp32,
                ModelClass::Embedding,
                64,
            )
            .with(
                aura_infer::contract::infer::ModelRef::new(
                    aura_brain_photo::integrity::eyes::EYE_MODEL,
                    aura_brain_photo::integrity::eyes::EYE_MODEL_VERSION,
                ),
                &eyes,
                Precision::Fp32,
                ModelClass::Embedding,
                32,
            ),
    );
    let mut plan = HardwarePlan::conservative(4, "2026-08-15T00:00:00Z".to_string());
    plan.default_batch = BatchDefaults {
        embedding: 64,
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
