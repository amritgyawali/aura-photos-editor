//! The shipped `tone_intent.toml`, checked against its own documentation. PHASE-16.
//!
//! Phase 09's `calibration.rs`, phase 11's `composition_rules.rs` and phase 15's target file
//! all have a test like this one, and it exists for the same reason every time: **a documented
//! range nobody enforces is a comment, and an enforced range nobody documented is a trap.**
//! The file's own header lists a bound for every field; this asserts the loader refuses a row
//! outside it, and that every shipped row is inside it.
//!
//! The last four tests are different in kind. They are not about ranges - they are the product
//! decisions the file exists to record, expressed so that changing one is a deliberate act
//! rather than a tuning session.

use aura_brain_photo::colour::intent::{IntentTable, MIN_RATIONALE};
use aura_core::contract::colour::{CLIP_CEILING, SUBTLETY_CEILING};
use aura_core::SceneId;

/// The shipped table. Every test starts here.
fn shipped() -> IntentTable {
    IntentTable::embedded().expect("the shipped intents are embedded in the binary")
}

/// A minimal valid document, for the refusal tests to mutate.
fn document(field: &str, value: &str) -> String {
    let mut rows = vec![
        ("contrast", "8"),
        ("subject_contrast", "0.26"),
        ("shadow_lift", "8"),
        ("highlight_recovery", "-10"),
        ("vibrance", "6"),
        ("saturation", "0"),
        ("clip_tolerance", "0.008"),
        ("greenery_hue_deg", "120"),
        ("greenery_saturation", "0.42"),
        ("sky_saturation", "0.46"),
        ("distractor_ceiling", "0.80"),
        ("max_band_shift", "8"),
        ("subtlety_cap", "0.22"),
    ];
    for row in &mut rows {
        if row.0 == field {
            row.1 = value;
        }
    }
    let body = rows
        .iter()
        .map(|(key, value)| format!("{key} = {value}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("version = 1\n[neutral]\n{body}\nrationale = \"a written reason\"\n")
}

fn refused(field: &str, value: &str) -> bool {
    IntentTable::parse("test", &document(field, value))
        .err()
        .is_some_and(|err| err.code.0 == "AURA-ML-5070")
}

#[test]
fn the_shipped_table_loads() {
    let table = shipped();
    assert!(table.version() >= 1, "the version reaches every decision");
    assert_eq!(table.rows(), SceneId::ALL.len() - 1);
}

#[test]
fn every_scene_in_the_taxonomy_has_a_row() {
    // A scene with no row is graded against the neutral one and says so, which is a real
    // fallback - but shipping with one is a gap somebody forgot rather than a decision.
    assert_eq!(shipped().untargeted(), Vec::<String>::new());
}

#[test]
fn every_documented_range_is_enforced() {
    // The file's own header, field by field. Each of these is a value one step outside the
    // documented bound.
    assert!(refused("contrast", "140"), "contrast");
    assert!(refused("shadow_lift", "-140"), "shadow_lift");
    assert!(refused("highlight_recovery", "180"), "highlight_recovery");
    assert!(refused("vibrance", "-200"), "vibrance");
    assert!(refused("saturation", "300"), "saturation");
    assert!(refused("subject_contrast", "0.01"), "subject_contrast low");
    assert!(refused("subject_contrast", "0.90"), "subject_contrast high");
    assert!(refused("clip_tolerance", "0.30"), "clip_tolerance");
    assert!(refused("greenery_hue_deg", "40"), "greenery_hue_deg low");
    assert!(refused("greenery_hue_deg", "200"), "greenery_hue_deg high");
    assert!(refused("greenery_saturation", "1.4"), "greenery_saturation");
    assert!(refused("sky_saturation", "-0.2"), "sky_saturation");
    assert!(refused("distractor_ceiling", "1.4"), "distractor_ceiling");
    assert!(refused("max_band_shift", "80"), "max_band_shift");
    assert!(refused("subtlety_cap", "0.01"), "subtlety_cap low");
    assert!(refused("subtlety_cap", "0.90"), "subtlety_cap high");
}

#[test]
fn every_shipped_row_is_inside_every_enforced_range() {
    let table = shipped();
    for scene in SceneId::ALL {
        let row = table.get(scene);
        assert!(
            (-100.0..=100.0).contains(&row.contrast),
            "{scene:?} contrast"
        );
        assert!(
            (0.05..=0.60).contains(&row.subject_contrast),
            "{scene:?} subject_contrast"
        );
        assert!(
            (0.0..=0.20).contains(&row.clip_tolerance),
            "{scene:?} clip_tolerance"
        );
        assert!(
            (90.0..=160.0).contains(&row.greenery_hue_deg),
            "{scene:?} greenery_hue_deg"
        );
        assert!(
            (0.0..=50.0).contains(&row.max_band_shift),
            "{scene:?} max_band_shift"
        );
        assert!(
            row.subtlety_cap <= SUBTLETY_CEILING,
            "{scene:?} subtlety_cap"
        );
    }
}

#[test]
fn a_row_with_no_written_reason_is_refused() {
    let short = "x".repeat(MIN_RATIONALE - 1);
    let text = document("contrast", "8").replace("a written reason", &short);
    assert!(IntentTable::parse("test", &text).is_err());
}

#[test]
fn a_scene_outside_the_frozen_taxonomy_is_refused() {
    let text = format!(
        "{}\n[[scene]]\nid = \"reception_afterparty\"\ncontrast = 8\nsubject_contrast = 0.26\n\
         shadow_lift = 8\nhighlight_recovery = -10\nvibrance = 6\nsaturation = 0\n\
         clip_tolerance = 0.008\ngreenery_hue_deg = 120\ngreenery_saturation = 0.42\n\
         sky_saturation = 0.46\ndistractor_ceiling = 0.8\nmax_band_shift = 8\n\
         subtlety_cap = 0.22\nrationale = \"a written reason\"\n",
        document("contrast", "8")
    );
    let error = IntentTable::parse("test", &text).err();
    assert!(
        error.is_some_and(|err| err.detail.contains("frozen taxonomy")),
        "adding a scene is a phase 07 contract change and needs an ADR"
    );
}

#[test]
fn one_scene_appearing_twice_is_refused() {
    let row = "[[scene]]\nid = \"ceremony\"\ncontrast = 8\nsubject_contrast = 0.26\n\
               shadow_lift = 8\nhighlight_recovery = -10\nvibrance = 6\nsaturation = 0\n\
               clip_tolerance = 0.008\ngreenery_hue_deg = 120\ngreenery_saturation = 0.42\n\
               sky_saturation = 0.46\ndistractor_ceiling = 0.8\nmax_band_shift = 8\n\
               subtlety_cap = 0.22\nrationale = \"a written reason\"\n";
    let text = format!("{}\n{row}\n{row}", document("contrast", "8"));
    assert!(
        IntentTable::parse("test", &text).is_err(),
        "two rows for one scene would make the answer depend on file order"
    );
}

#[test]
fn a_broken_override_falls_back_and_a_broken_baseline_does_not() {
    // The asymmetry every registry in the product has. The baseline is known good, so falling
    // back to it keeps the product open; a broken baseline is a build bug and halts.
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(directory.path().join("tone_intent.toml"), "not toml at all")
        .expect("write the override");
    let (table, refusal) =
        IntentTable::load_or_embedded(directory.path()).expect("the baseline still loads");
    assert!(
        refusal.is_some(),
        "the refusal is reported rather than silent"
    );
    assert_eq!(table.rows(), shipped().rows());
}

// ---------------------------------------------------------------------------
// The product decisions, not the ranges
// ---------------------------------------------------------------------------

#[test]
fn no_scene_may_buy_its_way_past_a_guarantee() {
    // Two frozen ceilings, checked per row rather than clamped. A row above one is a row
    // somebody wrote believing it would be honoured.
    let table = shipped();
    for scene in SceneId::ALL {
        let row = table.get(scene);
        assert!(row.subtlety_cap <= SUBTLETY_CEILING, "{scene:?} subtlety");
        assert!(
            aura_brain_photo::colour::intent::clip_tolerance(&row) <= CLIP_CEILING,
            "{scene:?} clipping"
        );
    }
}

#[test]
fn the_frames_a_couple_keeps_are_graded_most_gently() {
    // The single most consequential ordering in the file. A ceremony or a family portrait that
    // looks graded is the most damaging thing this phase can produce; a couple portrait that
    // does not is the least.
    let table = shipped();
    let portrait = table.get(SceneId::CouplePortrait).subtlety_cap;
    for scene in [
        SceneId::Ceremony,
        SceneId::Ritual,
        SceneId::Vows,
        SceneId::FamilyPortrait,
    ] {
        assert!(
            table.get(scene).subtlety_cap < portrait,
            "{scene:?} is graded as hard as a couple portrait"
        );
    }
}

#[test]
fn a_ritual_is_the_least_colour_adjusted_scene_in_the_file() {
    // The culturally loaded row. A havan, a tea ceremony and a chuppah are defined by their
    // own saturated colours, so almost nothing in one is a distraction and almost nothing
    // about it should move.
    let table = shipped();
    let ritual = table.get(SceneId::Ritual);
    for scene in SceneId::ALL {
        if scene == SceneId::Ritual {
            continue;
        }
        let other = table.get(scene);
        assert!(
            ritual.max_band_shift <= other.max_band_shift,
            "{scene:?} moves a band less than a ritual does"
        );
    }
    assert!(ritual.distractor_ceiling >= 0.90);
    assert!(ritual.vibrance <= 6.0);
}

#[test]
fn a_coloured_room_keeps_its_colour() {
    // Phase 15 decided not to neutralise a purple dance floor. This is the assertion that
    // phase 16 cannot do it by another route.
    let table = shipped();
    for scene in [SceneId::FirstDance, SceneId::DanceFloor] {
        let row = table.get(scene);
        assert!(
            row.distractor_ceiling >= 0.90,
            "{scene:?} tames too eagerly"
        );
        assert!(row.max_band_shift <= 6.0, "{scene:?} moves a band too far");
        assert!(
            row.vibrance <= 6.0,
            "{scene:?} saturates a room somebody lit"
        );
    }
}

#[test]
fn no_row_reaches_for_flat_saturation() {
    // Vibrance weights away from what is already saturated; flat saturation does not. The
    // second is what makes a sari look like a screenshot, and a row that used it would be a
    // change somebody should have to argue for.
    let table = shipped();
    for scene in SceneId::ALL {
        assert!(
            table.get(scene).saturation.abs() < f32::EPSILON,
            "{scene:?} sets flat saturation"
        );
    }
}

#[test]
fn there_is_no_skin_target_anywhere_in_the_file() {
    // The structural half of section 6.3's fairness argument, checked on the file itself. A
    // file with an ideal-skin row is how an editor lightens dark skin while believing it is
    // following a convention.
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/config/tone_intent.toml"
    ))
    .expect("the shipped intents are in the repository");
    for forbidden in [
        "skin_hue",
        "skin_saturation",
        "skin_luma",
        "skin_target",
        "ideal_skin",
        "skin_tone",
        "monk",
        "fitzpatrick",
    ] {
        assert!(
            !text.to_ascii_lowercase().contains(forbidden),
            "`{forbidden}` appears in tone_intent.toml"
        );
    }
}
