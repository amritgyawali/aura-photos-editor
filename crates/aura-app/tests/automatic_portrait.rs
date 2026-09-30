//! The bundled portrait planner must respect manual work and stable history.
use aura_app::portrait_auto::{self, KEY};
use aura_recipe::{fixtures, retouch_tools, schema, EditSource};

#[test]
fn blank_analysis_is_repeatable_and_does_not_create_retouch_operations() {
    let mut recipe = fixtures::neutral(fixtures::FIXTURE_HASH, "test");
    let report = portrait_auto::apply(&mut recipe, &vec![128; 64 * 96 * 3], 64, 96).unwrap();
    assert_eq!(report.detected_faces, 0);
    assert_eq!(report.operations, 0);
    assert!(retouch_tools::read(&recipe).unwrap().is_empty());
    schema::Validation::check(&recipe).unwrap();
    let base = recipe.clone();
    portrait_auto::apply(&mut recipe, &vec![128; 64 * 96 * 3], 64, 96).unwrap();
    assert_eq!(recipe.extra, base.extra);
    assert!(schema::merge(&base, &recipe, EditSource::Ai)
        .unwrap()
        .1
        .changed
        .is_empty());
}

#[test]
fn manual_stack_is_protected_before_any_pixels_are_analysed() {
    let mut recipe = fixtures::neutral(fixtures::FIXTURE_HASH, "test");
    recipe
        .provenance
        .user_edited_fields
        .push(retouch_tools::KEY.into());
    recipe.global.exposure = 0.73;
    let report = portrait_auto::apply(&mut recipe, &[], 0, 0).unwrap();
    assert_eq!(report.status, "protected");
    assert_eq!(report.operations, 0);
    assert_eq!(recipe.global.exposure, 0.73);
    assert_eq!(recipe.extra[KEY]["status"], "protected");
}

#[test]
fn invalid_pixels_fail_without_a_success_report() {
    let mut recipe = fixtures::neutral(fixtures::FIXTURE_HASH, "test");
    assert!(portrait_auto::apply(&mut recipe, &[0; 3], 32, 32).is_err());
    assert!(!recipe.extra.contains_key(KEY));
}
