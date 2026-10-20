use super::*;
use aura_core::contract::style::StyleDelta;
use aura_recipe::local_masks::{Component, LocalMask, Mode, Source};

fn session() -> Session {
    let mut baseline = aura_recipe::fixtures::neutral(aura_recipe::fixtures::FIXTURE_HASH, "test");
    let manual = LocalMask {
        id: "manual-light".into(),
        name: "Photographer's light".into(),
        enabled: true,
        amount: 0.5,
        components: vec![Component {
            mode: Mode::Add,
            invert: false,
            source: Source::Linear {
                start: [0.0, 0.0],
                end: [0.0, 1.0],
            },
        }],
        params: aura_recipe::MaskParams {
            exposure: Some(0.05),
            ..aura_recipe::MaskParams::default()
        },
    };
    local_masks::write(&mut baseline, &[manual], &BTreeMap::new()).unwrap();
    let regions = [(Kind::Background, [255, 0]), (Kind::Hair, [0, 255])]
        .into_iter()
        .map(|(kind, alpha)| {
            let mut matte =
                aura_recipe::retouch_tools::Matte::encode([0.0, 0.0, 1.0, 1.0], 2, 1, &alpha);
            matte.refine_edges = false;
            matte.refine_contour = true;
            let mut fitted = StyleDelta::neutral();
            fitted.exposure = 0.3;
            FittedRegion {
                kind,
                matte,
                fitted,
                controls: Controls::default(),
                before: 2.0,
                after: 1.0,
            }
        })
        .collect();
    Session {
        engine: ENGINE.into(),
        id: "test-session".into(),
        photo_id: PhotoId::new().to_db(),
        reference: ReferenceSummary {
            id: "a".repeat(64),
            name: "Reference".into(),
            width: 64,
            height: 48,
            converted_icc: false,
            regions: vec!["background".into(), "hair".into()],
            group: String::new(),
            favorite: false,
            default_controls: None,
            thumbnail: None,
        },
        baseline_hash: aura_recipe::hash::recipe_hash(&baseline).unwrap(),
        last_hash: "last".into(),
        baseline,
        regions,
        unavailable: Vec::new(),
        reference_regions: Vec::new(),
        target_regions: Vec::new(),
        checked_hash: None,
    }
}
fn strengths(background: f32, hair: f32) -> BTreeMap<Kind, Controls> {
    [
        (
            Kind::Background,
            Controls {
                amount: background,
                ..Controls::default()
            },
        ),
        (
            Kind::Hair,
            Controls {
                amount: hair,
                ..Controls::default()
            },
        ),
    ]
    .into()
}

#[test]
fn region_strength_changes_real_pixels_only_inside_its_frozen_selection() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::open(&directory.path().join("catalog.aura")).unwrap();
    let session = session();
    let frame = aura_render::Frame::working([0.2_f32, 0.16, 0.12].repeat(96 * 64), 96, 64, "test");
    let baseline = rendered(&state, &frame, &session.baseline).unwrap();
    let background = rendered(
        &state,
        &frame,
        &fit::candidate(&session, &strengths(1.0, 0.0)).unwrap(),
    )
    .unwrap();
    let hair = rendered(
        &state,
        &frame,
        &fit::candidate(&session, &strengths(0.0, 1.0)).unwrap(),
    )
    .unwrap();
    let pixel = |image: &aura_raw::codec::Rgb8, x: usize| {
        image
            .data
            .get((32 * 96 + x) * 3..(32 * 96 + x + 1) * 3)
            .unwrap()
            .to_vec()
    };
    assert_ne!(pixel(&background, 8), pixel(&baseline, 8));
    assert_eq!(pixel(&background, 88), pixel(&baseline, 88));
    assert_eq!(pixel(&hair, 8), pixel(&baseline, 8));
    assert_ne!(pixel(&hair, 88), pixel(&baseline, 88));
    let masks =
        local_masks::read(&fit::candidate(&session, &strengths(1.0, 1.0)).unwrap()).unwrap();
    assert_eq!(
        masks.first(),
        local_masks::read(&session.baseline).unwrap().first()
    );
}

#[test]
fn zero_restores_exact_recipe_and_adjustments_do_not_compound_or_change_mattes() {
    let mut session = session();
    let first = fit::candidate(&session, &strengths(1.0, 1.0)).unwrap();
    let half = fit::candidate(&session, &strengths(0.5, 0.5)).unwrap();
    session.id = "a-different-analysis-handle".into();
    let again = fit::candidate(&session, &strengths(1.0, 1.0)).unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&again).unwrap()
    );
    assert_eq!(
        local_masks::read_mattes(&first).unwrap(),
        local_masks::read_mattes(&half).unwrap()
    );
    let zero = fit::candidate(&session, &strengths(0.0, 0.0)).unwrap();
    assert_eq!(
        serde_json::to_value(&zero).unwrap(),
        serde_json::to_value(&session.baseline).unwrap()
    );
}

#[test]
fn missing_corrupt_unsupported_and_stale_region_requests_fail_closed() {
    let mut session = session();
    let request = |strengths, hash: &str| ApplyInput {
        photo_id: session.photo_id.clone(),
        session_id: session.id.clone(),
        recipe_hash: hash.into(),
        strengths,
        cancel_id: uuid::Uuid::new_v4().to_string(),
    };
    assert!(validate_apply(
        &session,
        &request(strengths(1.0, 1.0), &session.baseline_hash),
        &session.baseline_hash
    )
    .is_ok());
    assert!(validate_apply(
        &session,
        &request(BTreeMap::new(), &session.baseline_hash),
        &session.baseline_hash
    )
    .is_err());
    let mut unsupported = strengths(1.0, 1.0);
    unsupported.insert(Kind::Clothes, Controls::default());
    assert!(validate_apply(
        &session,
        &request(unsupported, &session.baseline_hash),
        &session.baseline_hash
    )
    .is_err());
    assert!(validate_apply(
        &session,
        &request(strengths(f32::NAN, 1.0), &session.baseline_hash),
        &session.baseline_hash
    )
    .is_err());
    assert!(validate_apply(&session, &request(strengths(1.0, 1.0), "newer"), "newer").is_err());
    session.regions.first_mut().unwrap().matte.data = "invalid".into();
    assert!(fit::candidate(&session, &strengths(1.0, 1.0)).is_err());
}
