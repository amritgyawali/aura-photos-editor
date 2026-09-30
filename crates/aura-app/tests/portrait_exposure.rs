//! Exercise the shipped exposure policy without building the full app unit harness.
use aura_app::photo_enhance::correction;

#[test]
fn normally_exposed_portraits_do_not_get_forced_toward_middle_gray() {
    for brightness in [120, 150, 175, 195] {
        let (exposure, _, _, _) = correction(&[brightness; 300]).expect("valid RGB fixture");
        assert!(exposure.abs() < f32::EPSILON);
    }
}

#[test]
fn bright_backgrounds_receive_only_restrained_global_darkening() {
    let mut rgb = vec![90; 120];
    rgb.extend([230; 180]);
    let (exposure, highlights, _, _) = correction(&rgb).expect("valid RGB fixture");
    assert!((-0.25..=0.0).contains(&exposure));
    assert!(highlights < 0);
}

#[test]
fn blank_frames_remain_neutral_and_backlit_highlights_are_protected() {
    assert_eq!(correction(&[0; 300]).expect("black"), (0.0, 0, 0, 0));
    assert_eq!(correction(&[255; 300]).expect("white"), (0.0, 0, 0, 0));
    let mut rgb = vec![40; 270];
    rgb.extend([250; 30]);
    let (exposure, highlights, shadows, _) = correction(&rgb).expect("backlit");
    assert!(exposure < 0.1);
    assert!(highlights < 0 && shadows > 0);
    assert!(correction(&[]).is_err());
    assert!(correction(&[1, 2]).is_err());
}
