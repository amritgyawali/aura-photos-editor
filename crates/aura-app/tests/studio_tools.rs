use aura_app::studio_tools::neutral_white_balance;

#[test]
fn neutral_picker_recovers_cast_using_the_actual_render_transform() {
    for kelvin in [2300, 3500, 5500, 8500, 15_000, 40_000] {
        for tint in [-100, 0, 100] {
            let gains = aura_render::colour::white_balance(kelvin as f32, tint as f32);
            let patch = gains.map(|v| 0.08 / v);
            let (temperature, recovered_tint) =
                neutral_white_balance(patch).expect("usable neutral patch");
            let corrected =
                aura_render::colour::white_balance(temperature as f32, recovered_tint as f32);
            for (value, gain) in patch.into_iter().zip(corrected) {
                assert!(
                    (value * gain - 0.08).abs() < 0.001,
                    "{kelvin}/{tint} -> {temperature}/{recovered_tint}"
                );
            }
        }
    }
}

#[test]
fn picker_rejects_unusable_dark_clipped_or_invalid_patches() {
    for sample in [
        [0.0; 3],
        [1.0; 3],
        [f32::NAN; 3],
        [f32::INFINITY; 3],
        [0.9, 0.01, 0.01],
    ] {
        assert!(neutral_white_balance(sample).is_err());
    }
}
