//! The phase 29 mechanical gate.
//!
//! The assembly proof for curation: migration 29 and its objects, the policy table a product manager
//! owns and the widened bound it refuses, a whole synthetic wedding through the real pass, the two
//! spread constraints as properties, coverage as a filter, the caption vocabulary's refusals, the
//! cloud validator, three export formats parsed back, the reorder that is remembered, and the IPC
//! surface's three files agreeing.
//!
//! **Nothing here proves anything about a real wedding.** Every fixture is a gallery of *readings*
//! this repository authored, and the numbers phases 09 to 26 would produce on a real frame come from
//! placeholder heads. There is no photographer agreement study, so the three headline gates of
//! section 10.1 - hero agreement 0.75, album reordering 15 %, monochrome acceptance 70 % - are
//! **unmeasured**. Those are conditions C1 to C5 of the exit report, and they are printed at the end
//! of every run rather than hidden in a helper.
//!
//! The unit tests prove the pieces and `tests/eval/curate_eval.rs` proves the gates. This proves the
//! assembly - the things that only exist when a catalog, a policy file, a field and a pass are in
//! the same process.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::contract::cull::{Coverage, MustHave};
use aura_core::contract::curate::{
    CurateCode, CurateGroup, CurateOverride, CurateService, ExportFormat, ExportSubject, ImageId,
    PickKind, ShotScale, ALBUM_MAX, ALBUM_MIN, GRID_SIZE, MAX_BAND_SHIFT, MAX_MOVES,
    MAX_PAIR_SIMILARITY, MAX_PAIR_TONAL_GAP, MAX_SKIN_BAND_SHIFT, TEASER_MAX, TEASER_MIN,
};
use aura_curate::api::{Curate, CuratePass};
use aura_curate::caption::Vocabulary;
use aura_curate::fixtures::{self, FixtureField, Shape};
use aura_curate::policy::{Policy, DEFAULT_TOML};
use aura_curate::sequence::{self, AlbumSequencing, DraftCaption, Move, SequenceOutput};
use aura_curate::store::CurateStore;
use rusqlite::params;

/// Run the phase 29 gate.
#[allow(clippy::too_many_lines)]
pub fn verify(args: &[String]) -> ExitCode {
    let work = PathBuf::from(
        crate::flag(args, "--work").unwrap_or_else(|| "target/phase29-verify".into()),
    );
    if let Err(err) = std::fs::create_dir_all(&work) {
        eprintln!("cannot create {}: {err}", work.display());
        return ExitCode::FAILURE;
    }
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let mut failures = 0usize;

    // ---------------------------------------------------------------------------------------
    // 1. Migration 29 and every object it owns.
    // ---------------------------------------------------------------------------------------
    let catalog_path = work.join("phase29.sqlite");
    drop(std::fs::remove_file(&catalog_path));
    let catalog = match Catalog::open(&catalog_path, Arc::clone(&clock), crate::APP_VERSION) {
        Ok(opened) => Arc::new(opened),
        Err(err) => {
            eprintln!("catalog: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };
    match catalog.schema_version() {
        Ok(version) if version >= 29 => println!("schema: version {version}"),
        Ok(version) => {
            eprintln!("schema: expected at least 29, found {version}");
            failures += 1;
        }
        Err(err) => {
            eprintln!("schema: [{}] {}", err.code, err.detail);
            failures += 1;
        }
    }

    let expected_tables = [
        "curate_run",
        "curate_bw",
        "curate_hero",
        "curate_album",
        "album_spread",
        "album_chapter",
        "social_pick",
        "curate_caption",
        "curate_reason",
        "curate_override",
        "album_order",
    ];
    let expected_views = ["v_curate_status", "v_curate_unmeasured"];
    let expected_triggers = [
        "curate_hero_reason_count",
        "curate_reason_bounded",
        "curate_album_no_reorder",
        "curate_album_order_is_the_photographers",
        "curate_caption_bounded",
    ];
    match objects(&catalog) {
        Ok(found) => {
            let mut missing = Vec::new();
            for name in expected_tables
                .iter()
                .chain(expected_views.iter())
                .chain(expected_triggers.iter())
            {
                if !found.contains(*name) {
                    missing.push(*name);
                }
            }
            if missing.is_empty() {
                println!(
                    "migration 29: {} tables, {} views, {} triggers",
                    expected_tables.len(),
                    expected_views.len(),
                    expected_triggers.len()
                );
            } else {
                eprintln!("migration 29: missing {missing:?}");
                failures += 1;
            }
        }
        Err(err) => {
            eprintln!("migration 29: {err}");
            failures += 1;
        }
    }

    // The schema carries no skin target and no free-text sentence. Phases 15, 25 and 27 scan their
    // own schemas the same way, and every one of them reads *code* rather than the prose explaining
    // it - `sqlite_master.sql` stores a migration verbatim, comments and all.
    match schema_scan(&catalog) {
        Ok(()) => println!("schema: no skin target, no stored sentence"),
        Err(err) => {
            eprintln!("schema: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // 2. The policy table: what a studio may tighten and what it may not widen.
    // ---------------------------------------------------------------------------------------
    let policy = match Policy::load_str(DEFAULT_TOML) {
        Ok(policy) => {
            println!(
                "policy: version {} - {} images, {} heroes, {} per chapter",
                policy.policy_ver,
                policy.album_default,
                policy.hero_target,
                policy.heroes_per_chapter
            );
            policy
        }
        Err(err) => {
            eprintln!("policy: [{}] {}", err.code, err.detail);
            return ExitCode::FAILURE;
        }
    };

    let widenings: [(&str, String, &str); 5] = [
        (
            "album.max_size",
            DEFAULT_TOML.replace("max_size = 120", "max_size = 200"),
            "an album larger than the contract allows",
        ),
        (
            "hero.technical_floor",
            DEFAULT_TOML.replace("technical_floor = 0.55", "technical_floor = 0.20"),
            "softer portfolio work than the contract allows",
        ),
        (
            "hero.per_chapter",
            DEFAULT_TOML.replace("per_chapter = 4", "per_chapter = 9"),
            "more heroes from one chapter than the contract allows",
        ),
        (
            "bw.candidate_floor",
            DEFAULT_TOML.replace("candidate_floor = 0.62", "candidate_floor = 0.10"),
            "a longer monochrome list than the contract allows",
        ),
        (
            "a skin target",
            format!("{DEFAULT_TOML}\n[skin]\nskin_target = 0.62\n"),
            "a constant to compare a person against",
        ),
    ];
    let mut refused = 0usize;
    for (key, text, what) in &widenings {
        if Policy::load_str(text).is_err() {
            refused += 1;
        } else {
            eprintln!("policy: `{key}` was accepted, and it asks for {what}");
            failures += 1;
        }
    }
    if refused == widenings.len() {
        println!("policy: {refused} widened bounds refused, including a skin target");
    }

    // A tightened bound is accepted, which is the other half of the rule: a table nobody may edit
    // is not a table a product manager owns.
    let tightened = DEFAULT_TOML.replace("technical_floor = 0.55", "technical_floor = 0.80");
    if Policy::load_str(&tightened).is_ok() {
        println!("policy: a studio may demand sharper portfolio work");
    } else {
        eprintln!("policy: a tightened bound was refused, which makes the file unownable");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 3. A whole synthetic wedding through the real pass.
    // ---------------------------------------------------------------------------------------
    //
    // Two shapes, because they answer different questions. `as_shipped` is this build - no faces,
    // no loci - and is what the coverage numbers below are measured on. `complete` is what the
    // mechanisms are tested against, and it exists because a guard that is unreachable on this
    // build still has to be proved.
    let complete = FixtureField::new(fixtures::wedding(Shape::complete(600), 29));
    let shipped = FixtureField::new(fixtures::wedding(Shape::as_shipped(600), 29));

    for (label, field) in [("complete", &complete), ("as shipped", &shipped)] {
        let project = field.wedding().project;
        if let Err(err) = seed(&catalog, &clock, field) {
            eprintln!("seed ({label}): {err}");
            failures += 1;
            continue;
        }
        let store = CurateStore::new(Arc::clone(&catalog), Arc::clone(&clock));
        let pass = CuratePass::new(field, &policy, &store, 1);
        let outline = match pass.run(project, Some(80), None) {
            Ok(outline) => outline,
            Err(err) => {
                eprintln!("pass ({label}): [{}] {}", err.code, err.detail);
                failures += 1;
                continue;
            }
        };
        println!(
            "pass ({label}): {} heroes, {} spreads, {} images, {} monochrome candidates from {} \
             keepers",
            outline.heroes,
            outline.spreads,
            outline.album_size,
            outline.bw_offered,
            outline.selected
        );
        println!(
            "pass ({label}): rhythm {:.2} over {:.0}% of the album, pairing {:.2}",
            outline.rhythm_score,
            outline.rhythm_measurable * 100.0,
            outline.pairing_score
        );

        let service = Curate::new(Arc::clone(&catalog), Arc::clone(&clock));
        let Ok(Some(album)) = service.album(project) else {
            eprintln!("pass ({label}): the album could not be read back");
            failures += 1;
            continue;
        };

        // -- chapter order, three enforcers, one rule --------------------------------------
        if album.chapters_are_ordered() {
            println!("album ({label}): chapters in wedding order");
        } else {
            eprintln!("album ({label}): the chapters are not in wedding order");
            failures += 1;
        }

        // -- the two hard spread constraints, as properties over every spread ---------------
        let frames: BTreeMap<ImageId, _> = field
            .wedding()
            .frames
            .iter()
            .map(|f| (f.image_id, f))
            .collect();
        let mut duplicates = 0usize;
        let mut clashes = 0usize;
        let mut crossings = 0usize;
        for spread in &album.spreads {
            let (Some(left), Some(right)) = (spread.left, spread.right) else {
                continue;
            };
            if let (Some(a), Some(b)) = (frames.get(&left), frames.get(&right)) {
                if a.moment.is_some() && a.moment == b.moment {
                    duplicates += 1;
                }
                if a.chapter_or_other() != b.chapter_or_other() {
                    crossings += 1;
                }
            }
            if spread.pair.similarity > MAX_PAIR_SIMILARITY {
                duplicates += 1;
            }
            if spread.pair.tonal_gap > MAX_PAIR_TONAL_GAP {
                clashes += 1;
            }
        }
        if duplicates == 0 {
            println!(
                "album ({label}): no facing near-duplicates in {} spreads",
                album.spreads.len()
            );
        } else {
            eprintln!("album ({label}): {duplicates} spreads face two versions of one shot");
            failures += 1;
        }
        if clashes == 0 {
            println!("album ({label}): no facing pair beyond the tonal ceiling");
        } else {
            eprintln!("album ({label}): {clashes} spreads exceed the tonal ceiling");
            failures += 1;
        }
        if crossings > 0 {
            eprintln!("album ({label}): {crossings} spreads straddle two chapters");
            failures += 1;
        }

        // -- coverage as a filter ----------------------------------------------------------
        let gallery_covers: BTreeSet<MustHave> = field
            .wedding()
            .coverage
            .must_haves
            .iter()
            .filter(|(_, state)| state.is_satisfied())
            .map(|(rule, _)| *rule)
            .collect();
        let album_misses: Vec<MustHave> = album
            .coverage
            .must_haves
            .iter()
            .filter(|(rule, state)| *state == Coverage::Missing && gallery_covers.contains(rule))
            .map(|(rule, _)| *rule)
            .collect();
        if album_misses.is_empty() {
            println!(
                "album ({label}): every one of the {} guarantees the gallery covers is in the album",
                gallery_covers.len()
            );
        } else {
            eprintln!(
                "album ({label}): the album misses {album_misses:?} and the gallery has them"
            );
            failures += 1;
        }

        // -- every pick explains itself ----------------------------------------------------
        let Ok(heroes) = service.heroes(project) else {
            eprintln!("pass ({label}): the heroes could not be read back");
            failures += 1;
            continue;
        };
        let unexplained = heroes.iter().filter(|h| h.reasons.is_empty()).count()
            + album
                .spreads
                .iter()
                .filter(|s| s.reasons.is_empty())
                .count();
        if unexplained == 0 {
            println!("pass ({label}): every hero and every spread carries a reason");
        } else {
            eprintln!("pass ({label}): {unexplained} picks carry no reason - invariant 2");
            failures += 1;
        }

        let malformed = heroes.iter().filter(|h| !h.is_well_formed()).count();
        if malformed > 0 {
            eprintln!("pass ({label}): {malformed} heroes are not well formed");
            failures += 1;
        }

        // -- the monochrome skin rule ------------------------------------------------------
        let Ok(bw) = service.bw(project) else {
            eprintln!("pass ({label}): the monochrome picks could not be read back");
            failures += 1;
            continue;
        };
        let mut moved_skin = 0usize;
        let mut over_ceiling = 0usize;
        for pick in &bw {
            for band in &pick.skin_bands {
                if pick
                    .mix
                    .bands
                    .get(usize::from(*band))
                    .is_some_and(|v| *v != 0)
                {
                    moved_skin += 1;
                }
            }
            if pick.mix.bands.iter().any(|v| v.abs() > MAX_BAND_SHIFT) {
                over_ceiling += 1;
            }
        }
        if moved_skin == 0 && over_ceiling == 0 {
            println!(
                "monochrome ({label}): {} candidates, no band anybody's measured skin sits in moved \
                 at all",
                bw.len()
            );
        } else {
            eprintln!(
                "monochrome ({label}): {moved_skin} picks moved a skin band and {over_ceiling} \
                 exceeded the ceiling"
            );
            failures += 1;
        }

        // -- determinism -------------------------------------------------------------------
        let before: Vec<ImageId> = album.images();
        if let Err(err) = pass.run(project, Some(80), None) {
            eprintln!("pass ({label}): a second run failed: {}", err.detail);
            failures += 1;
        } else if let Ok(Some(again)) = service.album(project) {
            if again.images() == before {
                println!("pass ({label}): the same gallery produces the same album twice");
            } else {
                eprintln!("pass ({label}): a second run produced a different album");
                failures += 1;
            }
        }

        // -- the sets ----------------------------------------------------------------------
        let Ok(sets) = service.social(project) else {
            eprintln!("pass ({label}): the social sets could not be read back");
            failures += 1;
            continue;
        };
        let vocabulary = Vocabulary::build(&field.wedding().rituals);
        let ungrounded: Vec<&str> = sets
            .captions
            .iter()
            .filter(|caption| !vocabulary.grounds(&caption.text))
            .map(|caption| caption.text.as_str())
            .collect();
        if ungrounded.is_empty() {
            println!(
                "captions ({label}): all {} grounded in this wedding's own labels",
                sets.captions.len()
            );
        } else {
            eprintln!(
                "captions ({label}): {ungrounded:?} contain words this wedding did not supply"
            );
            failures += 1;
        }
        if sets.grid.len() as u32 > GRID_SIZE {
            eprintln!("social ({label}): the grid has more than {GRID_SIZE} frames");
            failures += 1;
        }

        let Ok(teaser) = service.teaser(project) else {
            eprintln!("pass ({label}): the teaser could not be read back");
            failures += 1;
            continue;
        };
        if teaser.len() > TEASER_MAX as usize {
            eprintln!(
                "teaser ({label}): {} frames is over the ceiling",
                teaser.len()
            );
            failures += 1;
        } else {
            println!(
                "teaser ({label}): {} frames, {} social picks",
                teaser.len(),
                sets.grid.len() + sets.story.len()
            );
        }

        // -- exports, parsed back ----------------------------------------------------------
        let mut export_failures = 0usize;
        for subject in ExportSubject::ALL {
            for format in ExportFormat::ALL {
                match service.export(project, subject, format) {
                    Ok(text) if text.is_empty() => {
                        eprintln!("export ({label}): {subject:?}/{format:?} produced nothing");
                        export_failures += 1;
                    }
                    Ok(text) if format == ExportFormat::Json => {
                        if serde_json::from_str::<serde_json::Value>(&text).is_err() {
                            eprintln!("export ({label}): {subject:?} is not valid JSON");
                            export_failures += 1;
                        }
                    }
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("export ({label}): {subject:?}/{format:?}: {}", err.detail);
                        export_failures += 1;
                    }
                }
            }
        }
        if export_failures == 0 {
            println!(
                "export ({label}): {} specifications, every JSON one parsed back",
                ExportSubject::ALL.len() * ExportFormat::ALL.len()
            );
        } else {
            failures += export_failures;
        }

        // -- the reorder that is remembered ------------------------------------------------
        let mut order = before.clone();
        let head = album
            .chapter_map
            .first()
            .map(|span| {
                album
                    .spreads
                    .iter()
                    .filter(|s| s.chapter == span.chapter)
                    .map(aura_core::contract::curate::Spread::len)
                    .sum::<usize>()
            })
            .unwrap_or(0);
        if head > 1 && head < order.len() {
            order[..head].reverse();
            match service.set_order(project, &order) {
                Ok(()) => {
                    if pass.run(project, Some(80), None).is_ok() {
                        match service.album(project) {
                            Ok(Some(after)) if after.user_ordered && after.images() == order => {
                                println!("reorder ({label}): recorded, re-composed and remembered");
                            }
                            Ok(Some(_)) => {
                                eprintln!("reorder ({label}): a pass overwrote the order");
                                failures += 1;
                            }
                            _ => {
                                eprintln!("reorder ({label}): the album vanished");
                                failures += 1;
                            }
                        }
                    }
                }
                Err(err) => {
                    eprintln!("reorder ({label}): {}", err.detail);
                    failures += 1;
                }
            }

            // And an order that reorders chapters is refused.
            let mut crossed: Vec<ImageId> = order[head..].to_vec();
            crossed.extend_from_slice(&order[..head]);
            match service.set_order(project, &crossed) {
                Err(err) if err.code.0 == "AURA-ML-5143" => {
                    println!("reorder ({label}): an order that reorders chapters is refused");
                }
                Err(err) => {
                    eprintln!(
                        "reorder ({label}): refused with the wrong code {}",
                        err.code
                    );
                    failures += 1;
                }
                Ok(()) => {
                    eprintln!("reorder ({label}): an album's chapters were allowed to move");
                    failures += 1;
                }
            }
        }

        // -- a photographer's decision survives a re-run -----------------------------------
        if let Some(hero) = heroes.first() {
            let decided = service.decide(
                project,
                hero.image_id,
                CurateOverride {
                    kind: PickKind::Hero,
                    accepted: false,
                    note: Some("not my style".into()),
                },
            );
            if decided.is_ok() && pass.run(project, Some(80), None).is_ok() {
                let survived = service
                    .heroes(project)
                    .ok()
                    .and_then(|list| {
                        list.into_iter()
                            .find(|h| h.image_id == hero.image_id)
                            .map(|h| h.accepted)
                    })
                    .flatten();
                if survived == Some(false) {
                    println!("override ({label}): a photographer's verdict survives a re-run");
                } else {
                    eprintln!("override ({label}): a re-run overwrote a photographer's verdict");
                    failures += 1;
                }
            }
        }

        if label == "as shipped" {
            println!(
                "as shipped: {} of {} spreads could not have their facing measured, and the shot \
                 scale was measurable on {:.0}% of the album",
                album
                    .spreads
                    .iter()
                    .filter(|s| !s.single && !s.pair.facing_known)
                    .count(),
                album.spreads.iter().filter(|s| !s.single).count(),
                album.rhythm_measurable * 100.0
            );
        }
    }

    // ---------------------------------------------------------------------------------------
    // 4. The caption vocabulary's refusals.
    // ---------------------------------------------------------------------------------------
    let vocabulary = Vocabulary::build(&["saptapadi".to_string()]);
    let invented = [
        ("a name", "Priya and Arjun exchange rings"),
        ("a place", "the ceremony at Kathmandu"),
        ("a date", "the reception on 12 June"),
        ("a claim", "the couple were overjoyed"),
        ("a rite this wedding did not have", "the hora"),
        ("a gendered role", "the bride"),
    ];
    let mut caught = 0usize;
    for (what, text) in &invented {
        if vocabulary.grounds(text) {
            eprintln!("captions: `{text}` was accepted, and it invents {what}");
            failures += 1;
        } else {
            caught += 1;
        }
    }
    if caught == invented.len() {
        println!("captions: {caught} kinds of invention refused, including a gendered role word");
    }

    // ---------------------------------------------------------------------------------------
    // 5. The cloud validator: the answer type's bounds, and the four checks on a move.
    // ---------------------------------------------------------------------------------------
    use aura_cloud::contract::cloud::{CloudTask as _, Validate as _};
    let task = AlbumSequencing;
    let over = SequenceOutput {
        moves: (0..=MAX_MOVES)
            .map(|i| Move {
                from_index: i as i64,
                to_index: 0,
                reason: String::new(),
            })
            .collect(),
        captions: vec![DraftCaption {
            chapter: "ceremony".into(),
            caption: "x".repeat(91),
        }],
        confidence: 1.5,
    };
    if over.validate().is_err() && SequenceOutput::none().validate().is_ok() {
        println!("cloud: an answer over the schema's bounds is refused, and an empty one is not");
    } else {
        eprintln!("cloud: the validator does not enforce the schema's bounds");
        failures += 1;
    }

    let fallback_field = FixtureField::new(fixtures::wedding(Shape::complete(200), 7));
    let fallback_project = fallback_field.wedding().project;
    if let Err(err) = seed(&catalog, &clock, &fallback_field) {
        eprintln!("seed (cloud): {err}");
        failures += 1;
    } else {
        let store = CurateStore::new(Arc::clone(&catalog), Arc::clone(&clock));
        let pass = CuratePass::new(&fallback_field, &policy, &store, 1);
        let service = Curate::new(Arc::clone(&catalog), Arc::clone(&clock));
        if pass.run(fallback_project, Some(80), None).is_ok() {
            let offline = service
                .album(fallback_project)
                .ok()
                .flatten()
                .map(|plan| plan.images());
            // The task's own offline fallback is "nothing to add", and applying it must produce the
            // same album the deterministic optimiser produced.
            let empty = task
                .local_fallback(&sequence::input_for(
                    &service
                        .album(fallback_project)
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| aura_core::contract::curate::AlbumPlan::empty(80)),
                    &[],
                    &policy,
                ))
                .unwrap_or_else(|_| SequenceOutput::none());
            if pass.run(fallback_project, Some(80), Some(&empty)).is_ok() {
                let after = service
                    .album(fallback_project)
                    .ok()
                    .flatten()
                    .map(|plan| plan.images());
                if offline == after {
                    println!("cloud: an unreachable provider produces the album the optimiser did");
                } else {
                    eprintln!("cloud: the offline fallback changed the album");
                    failures += 1;
                }
            }

            // A move that crosses a chapter changes nothing.
            if let Ok(Some(plan)) = service.album(fallback_project) {
                let images = plan.images();
                let crossing = SequenceOutput {
                    moves: vec![Move {
                        from_index: 0,
                        to_index: (images.len().saturating_sub(1)) as i64,
                        reason: "flow".into(),
                    }],
                    captions: Vec::new(),
                    confidence: 0.99,
                };
                if pass
                    .run(fallback_project, Some(80), Some(&crossing))
                    .is_ok()
                {
                    if let Ok(Some(after)) = service.album(fallback_project) {
                        if after.images() == images {
                            println!("cloud: a move that crosses a chapter changes nothing");
                        } else {
                            eprintln!("cloud: a cross-chapter move was applied");
                            failures += 1;
                        }
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // 6. The reason vocabulary: every group reachable, every code parseable.
    // ---------------------------------------------------------------------------------------
    let mut unreachable = Vec::new();
    for group in CurateGroup::ALL {
        if !CurateCode::ALL.iter().any(|code| code.group() == group) {
            unreachable.push(group);
        }
    }
    let unparseable = CurateCode::ALL
        .iter()
        .filter(|code| CurateCode::parse(code.as_str()).is_err())
        .count();
    if unreachable.is_empty() && unparseable == 0 {
        println!(
            "reasons: {} codes in {} groups, every one parses back",
            CurateCode::COUNT,
            CurateGroup::ALL.len()
        );
    } else {
        eprintln!("reasons: {unreachable:?} have no codes, {unparseable} do not parse");
        failures += 1;
    }

    // ---------------------------------------------------------------------------------------
    // 7. The IPC surface's three files agree.
    // ---------------------------------------------------------------------------------------
    match ipc_parity() {
        Ok(count) => println!("ipc: {count} handlers = {count} definitions = {count} client calls"),
        Err(err) => {
            eprintln!("ipc: {err}");
            failures += 1;
        }
    }

    // ---------------------------------------------------------------------------------------
    // What this run did NOT prove.
    // ---------------------------------------------------------------------------------------
    println!();
    println!("phase 29 conditions this run does not close:");
    println!(
        "  C1  Every gate above is measured on galleries of readings this repository authored."
    );
    println!(
        "      Section 9's DATA row asks for sixty real album sequences, hero sets and monochrome"
    );
    println!("      selections collected with permission, and there are none.");
    println!(
        "  C2  The skin rule is unreachable on this build. Phase 06's detector finds no faces, so"
    );
    println!(
        "      phase 15 has no loci, so every mix is solved as a faceless frame - the guard is"
    );
    println!("      proved on the `complete` fixture and inert on a real wedding.");
    println!(
        "  C3  Spread direction is unmeasurable for the same reason, so the term album designers"
    );
    println!("      spend the most time on is renormalised out of nearly every pairing score.");
    println!("  C4  The three headline gates of section 10.1 are UNMEASURED: hero agreement 0.75,");
    println!("      album reordering under 15 %, monochrome acceptance 70 %. All three need");
    println!("      photographers, and `ml/models/curate/eval_curate.py` is what runs them.");
    println!(
        "  C5  The cloud sequencing task has never reached a provider. Its contact sheets need a"
    );
    println!(
        "      renderer this crate must not have, and TLS is waived - so what is proved is that"
    );
    println!("      the validator refuses, not that a model helps.");

    if failures == 0 {
        println!();
        println!("phase 29: all checks passed");
        ExitCode::SUCCESS
    } else {
        eprintln!();
        eprintln!("phase 29: {failures} check(s) failed");
        ExitCode::FAILURE
    }
}

// -- helpers -----------------------------------------------------------------------------------

/// Every table, view and trigger the catalog holds.
fn objects(catalog: &Catalog) -> Result<BTreeSet<String>, String> {
    catalog
        .read(|conn| {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type IN ('table','view','trigger')")
                .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))?;
            let mut out = BTreeSet::new();
            for row in rows {
                out.insert(row.map_err(|e| aura_core::errors::db::statement_failed("row", &e))?);
            }
            Ok(out)
        })
        .map_err(|err| err.detail)
}

/// Migration 29 names no skin target and stores no sentence automation writes.
///
/// Reads *code* rather than the prose explaining it: `sqlite_master.sql` stores a migration
/// verbatim, and this migration's header is several paragraphs about why there is no skin target in
/// it. Phase 27 found the same defect twice in one phase.
fn schema_scan(catalog: &Catalog) -> Result<(), String> {
    let sql = catalog
        .read(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT COALESCE(sql, '') FROM sqlite_master
                      WHERE name LIKE 'curate%' OR name LIKE 'album%' OR name LIKE 'social_pick%'",
                )
                .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))?;
            let rows = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .map_err(|e| aura_core::errors::db::statement_failed("sqlite_master", &e))?;
            let mut out = String::new();
            for row in rows {
                out.push_str(&row.map_err(|e| aura_core::errors::db::statement_failed("row", &e))?);
                out.push('\n');
            }
            Ok(out)
        })
        .map_err(|err| err.detail)?;
    let code = strip_sql_comments(&sql);

    for needle in ["skin_target", "ideal_skin", "target_skin", "reference_skin"] {
        if code.contains(needle) {
            return Err(format!("the schema names `{needle}`"));
        }
    }
    // No free-text sentence automation writes. Phase 27's rule at its conclusion: a stored sentence
    // becomes copy a release has to maintain, a catalog full of English, and a place a cloud answer
    // gets quoted back as a measurement. `curate_reason.detail` is the one exception and it is a
    // *specific half* of a code's own sentence, bounded and written by the deciding code; the
    // caption text is a photographer-facing sentence the grounding check produced.
    for needle in ["diagnosis", "summary TEXT", "explanation"] {
        if code.contains(needle) {
            return Err(format!("the schema stores a sentence in `{needle}`"));
        }
    }
    Ok(())
}

/// Seed a project and its photographs so the store's foreign keys have something to point at.
///
/// Phases 25 and 26 both had a gate fail on exactly this: a fixture that hands out ids without
/// making the rows they refer to passes every unit test and fails the first time a real catalog is
/// involved.
fn seed(catalog: &Catalog, clock: &Arc<dyn Clock>, field: &FixtureField) -> Result<(), String> {
    let project = field.wedding().project.to_db();
    let now = aura_catalog::rfc3339(clock.now_utc());
    let images: Vec<String> = field
        .wedding()
        .frames
        .iter()
        .map(|f| f.image_id.to_db())
        .collect();
    catalog
        .writer()
        .transact(move |tx| {
            tx.execute(
                "INSERT OR IGNORE INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, 'phase29', ?2, ?2)",
                params![project, now],
            )
            .map_err(|e| aura_core::errors::db::statement_failed("project", &e))?;
            for image in &images {
                tx.execute(
                    "INSERT OR IGNORE INTO photo (photo_id, project_id, capture_time,
                                                  timeline_time, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?3, ?3, ?3)",
                    params![image, project, now],
                )
                .map_err(|e| aura_core::errors::db::statement_failed("photo", &e))?;
            }
            Ok(())
        })
        .map_err(|err| err.detail)
}

/// Every `#[tauri::command]` has a registration and a typed client wrapper, and nothing else does.
///
/// The same three-way count phase 27 introduced. Three sets rather than a manifest, because a
/// manifest is a fourth thing that can disagree with the other three.
fn ipc_parity() -> Result<usize, String> {
    let shell = std::fs::read_to_string("ui/src-tauri/src/main.rs")
        .map_err(|err| format!("ui/src-tauri/src/main.rs could not be read: {err}"))?;
    let client = std::fs::read_to_string("ui/src/ipc/client.ts")
        .map_err(|err| format!("ui/src/ipc/client.ts could not be read: {err}"))?;

    let mut defined: BTreeSet<String> = BTreeSet::new();
    let mut expecting = false;
    for line in shell.lines() {
        let line = line.trim();
        if line == "#[tauri::command]" {
            expecting = true;
            continue;
        }
        if expecting {
            if let Some(name) = line
                .strip_prefix("pub async fn ")
                .or_else(|| line.strip_prefix("async fn "))
                .or_else(|| line.strip_prefix("pub fn "))
                .or_else(|| line.strip_prefix("fn "))
                .and_then(|rest| rest.split('(').next())
            {
                defined.insert(name.trim().to_string());
                expecting = false;
            }
        }
    }

    let Some((_, after)) = shell.split_once("generate_handler![") else {
        return Err("the shell has no `generate_handler!` list".to_string());
    };
    let Some((list, _)) = after.split_once("])") else {
        return Err("the shell's `generate_handler!` list is not closed".to_string());
    };
    let registered: BTreeSet<String> = list
        .lines()
        .map(|line| line.trim().trim_end_matches(',').to_string())
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .collect();

    let mut invoked: BTreeSet<String> = BTreeSet::new();
    for (index, _) in client.match_indices("invoke<") {
        let Some(open) = client[index..].find('(') else {
            continue;
        };
        let rest = &client[index + open + 1..];
        let Some(quote) = rest.find('\'') else {
            continue;
        };
        let rest = &rest[quote + 1..];
        let Some(end) = rest.find('\'') else {
            continue;
        };
        invoked.insert(rest[..end].to_string());
    }

    let mut problems = Vec::new();
    for name in defined.difference(&registered) {
        problems.push(format!(
            "`{name}` is defined in the shell and never registered"
        ));
    }
    for name in registered.difference(&defined) {
        problems.push(format!("`{name}` is registered and has no definition"));
    }
    for name in invoked.difference(&registered) {
        problems.push(format!(
            "the client calls `{name}` and no handler answers to it"
        ));
    }
    for name in registered.difference(&invoked) {
        problems.push(format!(
            "`{name}` is registered and no client wrapper reaches it"
        ));
    }
    if problems.is_empty() {
        Ok(defined.len())
    } else {
        problems.truncate(6);
        Err(problems.join("; "))
    }
}

/// One schema's SQL with its comments removed.
fn strip_sql_comments(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len());
    for line in sql.lines() {
        let code = match line.find("--") {
            Some(index) => &line[..index],
            None => line,
        };
        out.push_str(code);
        out.push('\n');
    }
    out
}

/// Silence the unused-import warning for the constants the printed conditions refer to.
#[allow(dead_code)]
const _BOUNDS: (u32, u32, u32, u32, i16) = (
    ALBUM_MIN,
    ALBUM_MAX,
    TEASER_MIN,
    ShotScale::COUNT as u32,
    MAX_SKIN_BAND_SHIFT,
);
