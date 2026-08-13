//! The cheap descriptors, and the difference hash.
//!
//! Each of these is a summary of a photograph, so the tests are written the way a
//! photographer would check them: change one property of the frame and assert that
//! the number which describes that property moves, and that the others do not.

mod support;

use aura_vision::embed::descriptors::{
    box_resize, describe, edge_energy, hsv_histogram, luma_plane, luma_stats, palette, to_hsv,
    DESCRIPTOR_VER, HSV_AXIS,
};
use aura_vision::embed::hash::{dhash, hamming, is_near_duplicate, DHASH_HEIGHT, DHASH_WIDTH};

use support::{flat, synthetic};

fn plane_of(
    buffer: &aura_raw::contract::pixels::PixelBuffer,
) -> aura_vision::embed::descriptors::LumaPlane {
    let Some(rgb) = buffer.as_srgb8() else {
        panic!("the fixture is not 8-bit sRGB");
    };
    luma_plane(rgb, buffer.width as usize, buffer.height as usize)
}

// -- luminance ---------------------------------------------------------------

#[test]
fn luminance_statistics_follow_the_brightness_of_the_frame() {
    let dark = plane_of(&synthetic(256, 170, 0.0, 0.35, 0));
    let bright = plane_of(&synthetic(256, 170, 0.0, 1.0, 0));

    let dark_stats = luma_stats(&dark);
    let bright_stats = luma_stats(&bright);

    assert!(
        dark_stats.mean < bright_stats.mean,
        "the darker frame reported a higher mean: {} vs {}",
        dark_stats.mean,
        bright_stats.mean
    );
    assert!(dark_stats.p99 < bright_stats.p99);
    assert!(dark_stats.p1 <= dark_stats.p50 && dark_stats.p50 <= dark_stats.p99);
    assert!(bright_stats.spread() > 0.0);
}

#[test]
fn a_black_frame_reports_its_shadows_clipped_and_a_white_frame_its_highlights() {
    let black = luma_stats(&plane_of(&flat(64, 64, 0)));
    assert!(black.clip_lo > 0.99, "clip_lo was {}", black.clip_lo);
    assert!(black.clip_hi < 0.01);

    let white = luma_stats(&plane_of(&flat(64, 64, 255)));
    assert!(white.clip_hi > 0.99, "clip_hi was {}", white.clip_hi);
    assert!(white.clip_lo < 0.01);
}

#[test]
fn a_mid_grey_frame_clips_at_neither_end() {
    let grey = luma_stats(&plane_of(&flat(64, 64, 128)));
    assert!(grey.clip_lo < 0.01 && grey.clip_hi < 0.01);
    assert!((grey.mean - 0.5).abs() < 0.02, "mean was {}", grey.mean);
    assert!((grey.p50 - grey.mean).abs() < 0.02);
}

#[test]
fn an_empty_plane_reports_zeros_rather_than_dividing_by_nothing() {
    let empty = aura_vision::embed::descriptors::LumaPlane {
        values: Vec::new(),
        width: 0,
        height: 0,
    };
    let stats = luma_stats(&empty);
    assert_eq!(stats.mean, 0.0);
    assert_eq!(stats.spread(), 0.0);
}

// -- edges -------------------------------------------------------------------

#[test]
fn edge_energy_follows_the_number_of_edges_in_the_frame() {
    let smooth = plane_of(&synthetic(256, 256, 0.0, 0.8, 0));
    let striped = plane_of(&synthetic(256, 256, 0.0, 0.8, 4));

    let (smooth_mean, smooth_p90) = edge_energy(&smooth);
    let (striped_mean, striped_p90) = edge_energy(&striped);

    assert!(
        striped_mean > smooth_mean * 3.0,
        "stripes scored {striped_mean} and a smooth gradient {smooth_mean}"
    );
    assert!(striped_p90 >= striped_mean);
    assert!(smooth_p90 >= smooth_mean);
}

#[test]
fn a_flat_frame_has_no_edge_energy() {
    let (mean, p90) = edge_energy(&plane_of(&flat(128, 128, 90)));
    assert!(mean < 1e-6, "a flat frame reported edge energy {mean}");
    assert!(p90 < 1e-6);
}

#[test]
fn edge_energy_does_not_depend_on_how_big_the_sensor_was() {
    // The whole reason the measurement happens on a fixed 128 px plane: two
    // cameras must be comparable. The same content at two resolutions must score
    // about the same.
    let small = plane_of(&synthetic(256, 256, 0.0, 0.8, 4));
    let large = plane_of(&synthetic(1024, 1024, 0.0, 0.8, 16));
    let (small_mean, _) = edge_energy(&small);
    let (large_mean, _) = edge_energy(&large);
    let ratio = small_mean / large_mean.max(1e-9);
    assert!(
        (0.5..2.0).contains(&ratio),
        "the same content scored {small_mean} at 256 px and {large_mean} at 1024 px"
    );
}

// -- colour ------------------------------------------------------------------

#[test]
fn hsv_conversion_agrees_with_the_textbook_on_the_primaries() {
    let (hue, saturation, value) = to_hsv(255, 0, 0);
    // Red sits at the seam: hue zero and hue one are the same colour.
    assert!(!(0.01..=0.99).contains(&hue), "red hue was {hue}");
    assert!((saturation - 1.0).abs() < 0.01);
    assert!((value - 1.0).abs() < 0.01);

    let (green_hue, _, _) = to_hsv(0, 255, 0);
    assert!(
        (green_hue - 1.0 / 3.0).abs() < 0.01,
        "green hue was {green_hue}"
    );

    let (blue_hue, _, _) = to_hsv(0, 0, 255);
    assert!(
        (blue_hue - 2.0 / 3.0).abs() < 0.01,
        "blue hue was {blue_hue}"
    );

    let (_, grey_saturation, grey_value) = to_hsv(128, 128, 128);
    assert!(
        grey_saturation < 0.01,
        "grey was saturated: {grey_saturation}"
    );
    assert!((grey_value - 128.0 / 255.0).abs() < 0.01);
}

#[test]
fn the_histogram_moves_when_the_colour_moves_and_not_otherwise() {
    let neutral = synthetic(256, 170, 0.0, 0.8, 0);
    let shifted = synthetic(256, 170, 0.35, 0.8, 0);
    let Some(neutral_rgb) = neutral.as_srgb8() else {
        panic!("not sRGB");
    };
    let Some(shifted_rgb) = shifted.as_srgb8() else {
        panic!("not sRGB");
    };

    let first = hsv_histogram(neutral_rgb, 256, 170);
    let again = hsv_histogram(neutral_rgb, 256, 170);
    assert_eq!(first, again, "the histogram is not deterministic");

    let moved = hsv_histogram(shifted_rgb, 256, 170);
    assert_ne!(first, moved, "a hue shift did not move the histogram");
}

#[test]
fn the_histogram_is_scaled_so_its_fullest_bin_is_full() {
    let frame = synthetic(128, 128, 0.1, 0.7, 2);
    let Some(rgb) = frame.as_srgb8() else {
        panic!("not sRGB");
    };
    let histogram = hsv_histogram(rgb, 128, 128);
    assert_eq!(
        histogram.iter().copied().max(),
        Some(255),
        "no bin reached the top of the range"
    );
    assert_eq!(histogram.len(), HSV_AXIS * HSV_AXIS * HSV_AXIS);
}

#[test]
fn a_flat_frame_puts_everything_in_one_histogram_bin() {
    let histogram = hsv_histogram(&[64u8; 3 * 64], 8, 8);
    let occupied = histogram.iter().filter(|bin| **bin > 0).count();
    assert_eq!(occupied, 1, "{occupied} bins held a single colour");
}

#[test]
fn the_palette_reports_the_dominant_colours_most_frequent_first() {
    // Two thirds dark, one third light: the dark bucket must come first.
    let mut bytes = vec![20u8; 3 * 200];
    bytes.extend(std::iter::repeat_n(230u8, 3 * 100));
    let found = palette(&bytes, 300, 1);
    let Some(first) = found.first() else {
        panic!("no palette");
    };
    let Some(second) = found.get(1) else {
        panic!("no second colour");
    };
    assert!(
        first.iter().all(|channel| *channel < 128),
        "{first:?} is not the dark bucket"
    );
    assert!(
        second.iter().all(|channel| *channel > 128),
        "{second:?} is not the light bucket"
    );
    // Only two colours are present, so the rest of the palette is empty.
    assert_eq!(found.get(2), Some(&[0u8, 0, 0]));
}

#[test]
fn the_palette_is_deterministic_when_two_buckets_tie() {
    // Equal counts: the tie is broken by bucket index, so two runs agree. A
    // clustering pass would need a seed to make this true.
    let mut bytes = vec![20u8; 3 * 100];
    bytes.extend(std::iter::repeat_n(230u8, 3 * 100));
    let first = palette(&bytes, 200, 1);
    let again = palette(&bytes, 200, 1);
    assert_eq!(first, again);
}

// -- the difference hash -----------------------------------------------------

#[test]
fn the_difference_hash_is_stable_and_sized_as_documented() {
    assert_eq!(DHASH_WIDTH, 9);
    assert_eq!(DHASH_HEIGHT, 8);
    assert_eq!((DHASH_WIDTH - 1) * DHASH_HEIGHT, 64);

    // Stripes wide enough to survive the reduction to nine columns. Finer stripes
    // average out, which is correct - the hash describes coarse structure, which is
    // exactly why it survives a resize and an exposure change.
    let frame = synthetic(256, 170, 0.0, 0.8, 40);
    let plane = plane_of(&frame);
    let first = dhash(&plane.values, plane.width, plane.height);
    let again = dhash(&plane.values, plane.width, plane.height);
    assert_eq!(first, again, "the hash is not deterministic");
    assert_ne!(first, 0, "a structured frame hashed to zero");
}

#[test]
fn a_monotonic_gradient_hashes_to_zero_and_that_is_correct() {
    // Every column is brighter than the one to its left, so every comparison is
    // false. Documented rather than worked around: a caller that wants to know
    // whether anything was measured checks the pixel count, not the hash.
    let plane = plane_of(&synthetic(256, 170, 0.0, 0.8, 0));
    assert_eq!(dhash(&plane.values, plane.width, plane.height), 0);
}

#[test]
fn the_same_frame_one_stop_apart_is_a_near_duplicate() {
    // Section 10.1: "including near-duplicates one stop apart". A perceptual hash
    // that changed when the exposure did would be useless for the thing phase 08
    // needs it for.
    let base = plane_of(&synthetic(400, 300, 0.0, 0.5, 50));
    let brighter = plane_of(&synthetic(400, 300, 0.0, 1.0, 50));

    let left = dhash(&base.values, base.width, base.height);
    let right = dhash(&brighter.values, brighter.width, brighter.height);
    assert!(
        is_near_duplicate(left, right),
        "one stop apart scored {} bits of difference",
        hamming(left, right)
    );
}

#[test]
fn two_different_photographs_are_not_near_duplicates() {
    let left_plane = plane_of(&synthetic(400, 300, 0.0, 0.8, 133));
    let right_plane = plane_of(&synthetic(400, 300, 0.4, 0.8, 50));
    let left = dhash(&left_plane.values, left_plane.width, left_plane.height);
    let right = dhash(&right_plane.values, right_plane.width, right_plane.height);
    assert!(
        !is_near_duplicate(left, right),
        "two unrelated frames scored only {} bits apart",
        hamming(left, right)
    );
}

#[test]
fn the_hash_survives_a_resize_of_the_same_content() {
    // The same photograph at two proxy sizes must hash to nearly the same value,
    // because a wedding contains the same frame at tier 1 and tier 2.
    let small = plane_of(&synthetic(200, 150, 0.0, 0.8, 25));
    let large = plane_of(&synthetic(800, 600, 0.0, 0.8, 100));
    let left = dhash(&small.values, small.width, small.height);
    let right = dhash(&large.values, large.width, large.height);
    assert!(
        hamming(left, right) <= 8,
        "the same content at two sizes differed by {} bits",
        hamming(left, right)
    );
}

#[test]
fn a_flat_frame_hashes_to_zero_and_an_empty_plane_does_too() {
    assert_eq!(dhash(&[0.5; 64], 8, 8), 0);
    assert_eq!(dhash(&[], 0, 0), 0);
}

#[test]
fn hamming_distance_is_the_bit_count() {
    assert_eq!(hamming(0, 0), 0);
    assert_eq!(hamming(0, u64::MAX), 64);
    assert_eq!(hamming(0b1011, 0b1110), 2);
}

// -- resampling --------------------------------------------------------------

#[test]
fn the_box_resize_averages_rather_than_sampling() {
    // A checkerboard halved must go to its mean. Bilinear sampling of a downscale
    // would return one of the two values instead, and an aliased embedding input
    // depends on where the subject fell relative to the sampling grid.
    let checker: Vec<f32> = (0..64)
        .map(|n| if (n / 8 + n % 8) % 2 == 0 { 1.0 } else { 0.0 })
        .collect();
    let halved = box_resize(&checker, 8, 8, 4, 4);
    for value in &halved {
        assert!(
            (*value - 0.5).abs() < 1e-6,
            "a halved checkerboard produced {value}, not its mean"
        );
    }
}

#[test]
fn the_box_resize_handles_degenerate_sizes_without_panicking() {
    assert_eq!(box_resize(&[], 0, 0, 4, 4).len(), 16);
    assert_eq!(box_resize(&[1.0], 1, 1, 0, 0).len(), 0);
    // Upscaling degenerates to nearest neighbour, which is what a tiny camera
    // thumbnail needs.
    let up = box_resize(&[0.25, 0.75], 2, 1, 4, 1);
    assert_eq!(up.len(), 4);
    assert!((up.first().copied().unwrap_or(0.0) - 0.25).abs() < 1e-6);
}

// -- all of them together ----------------------------------------------------

#[test]
fn describe_produces_every_descriptor_in_one_pass() {
    let frame = synthetic(512, 340, 0.15, 0.7, 6);
    let Some(rgb) = frame.as_srgb8() else {
        panic!("not sRGB");
    };
    let plane = luma_plane(rgb, 512, 340);
    let described = describe(rgb, 512, 340, &plane);

    assert_eq!(described.descriptor_ver, DESCRIPTOR_VER);
    assert!(described.hsv_hist.iter().any(|bin| *bin > 0));
    assert!(described.luma.mean > 0.0);
    assert!(described.edge_energy > 0.0);
    assert!(described.palette.iter().any(|colour| colour != &[0, 0, 0]));

    // Twice over the same pixels is the same answer: every descriptor is a pure
    // function, which is what lets a re-run be free.
    let again = describe(rgb, 512, 340, &plane);
    assert_eq!(described, again);
}
