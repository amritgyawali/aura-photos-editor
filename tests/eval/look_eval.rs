//! PHASE-31 section 10.1, as an ordinary test so a red gate is a red build.
//!
//! **Every number here is measured against synthetic reference galleries whose look was
//! authored, applied to an authored plate by an analytic transform in `fixtures.rs`, and read
//! back through the real measurer, the real sort, the real fold and the real solver.** That
//! proves the arithmetic. It says nothing about whether a photographer would recognise a page
//! they admire in the result, which is condition C1 in the phase 31 exit report and is a Sev 2
//! trigger.

use aura_core::contract::look::{LookAggregate, MIN_REFERENCES, USABLE_REFERENCES};
use aura_core::contract::style::StyleDelta;
use aura_look::fixtures::{self, SyntheticLook};
use aura_look::solve::Probe;
use aura_look::{aggregate, measure, solve};

/// A probe that applies a delta analytically rather than by rendering.
///
/// The refinement's search is what these gates measure, and a probe that rendered would fold
/// the renderer's own behaviour into every number. `tests/render_match.rs` is where the real
/// renderer is measured; here the question is whether the search finds the minimum it is
/// pointed at.
#[derive(Debug)]
struct AnalyticProbe {
    base: LookAggregate,
}

impl solve::Probe for AnalyticProbe {
    fn aggregate_with(&self, delta: &StyleDelta) -> LookAggregate {
        let mut out = self.base.clone();
        // Exposure moves every landmark in luminance, which in L* is roughly a third of the
        // stop count near the middle of the range.
        let lift = delta.exposure * 0.33;
        let mut tone = out.tone.as_array();
        for value in &mut tone {
            *value = (*value + lift).clamp(0.0, 1.0);
        }
        out.tone = aura_core::contract::look::ToneLandmarks::from_array(tone);
        out.mid.b += delta.temperature_k / 80.0;
        out.mid.a += delta.tint / 2.0;
        out.chroma_p50 *= 1.0 + delta.vibrance / 60.0;
        out.chroma_p90 *= 1.0 + delta.saturation / 30.0;
        out
    }
}

// ---------------------------------------------------------------------------
// 10.1 row 1: a look measured from a gallery of itself is the neutral look
// ---------------------------------------------------------------------------

#[test]
fn a_reference_that_matches_the_baseline_asks_for_nothing() {
    let look = SyntheticLook::warm_film();
    let reference = fixtures::aggregate(24, look);
    let baseline = fixtures::aggregate(24, look);

    let delta = solve::initial(&reference, &baseline);

    assert!(
        delta.is_neutral(),
        "a look measured against itself must change nothing, got {delta:?}"
    );
}

// ---------------------------------------------------------------------------
// 10.1 row 2: direction. A brighter, warmer, richer reference asks for exactly that
// ---------------------------------------------------------------------------

#[test]
fn a_brighter_reference_asks_for_more_exposure() {
    let baseline = fixtures::aggregate(24, SyntheticLook::neutral());
    let reference = fixtures::aggregate(24, SyntheticLook::light_and_airy());

    let delta = solve::initial(&reference, &baseline);

    assert!(
        delta.exposure > 0.05,
        "a gallery a quarter stop brighter should ask for exposure, got {}",
        delta.exposure
    );
    assert!(
        delta.blacks > 0.0,
        "a gallery with lifted blacks should ask for blacks, got {}",
        delta.blacks
    );
}

#[test]
fn a_darker_reference_asks_for_less_exposure_and_more_contrast() {
    let baseline = fixtures::aggregate(24, SyntheticLook::neutral());
    let reference = fixtures::aggregate(24, SyntheticLook::dark_and_moody());

    let delta = solve::initial(&reference, &baseline);

    assert!(
        delta.exposure < -0.02,
        "a darker gallery should ask for less exposure, got {}",
        delta.exposure
    );
    assert!(
        delta.contrast > 0.0,
        "a more contrasty gallery should ask for contrast, got {}",
        delta.contrast
    );
}

#[test]
fn a_warmer_reference_asks_for_a_warmer_temperature() {
    let baseline = fixtures::aggregate(24, SyntheticLook::neutral());
    let reference = fixtures::aggregate(24, SyntheticLook::warm_film());

    let delta = solve::initial(&reference, &baseline);

    assert!(
        delta.temperature_k > 20.0,
        "a warmer gallery should ask for a warmer temperature, got {}",
        delta.temperature_k
    );
}

// ---------------------------------------------------------------------------
// 10.1 row 3: every solved delta is inside the contract's bounds
// ---------------------------------------------------------------------------

#[test]
fn no_solved_delta_can_leave_its_bounds() {
    // A reference nothing like the baseline: the case where an unbounded solver would ask for
    // three stops and a thousand kelvin.
    let baseline = fixtures::aggregate(24, SyntheticLook::dark_and_moody());
    let reference = fixtures::aggregate(
        24,
        SyntheticLook {
            gain: 3.0,
            contrast: 0.5,
            warmth: 0.45,
            green: 0.2,
            saturation: 2.2,
            lift: 0.2,
        },
    );

    let delta = solve::initial(&reference, &baseline);

    assert!(
        delta.exposure.abs() <= aura_core::contract::look::MAX_EXPOSURE_DELTA_EV + 1e-4,
        "exposure left its bound: {}",
        delta.exposure
    );
    assert!(
        delta.temperature_k.abs() <= aura_core::contract::style::MAX_TEMPERATURE_DELTA_K + 1e-2,
        "temperature left its bound: {}",
        delta.temperature_k
    );
    for band in aura_core::contract::colour::HslBand::ALL {
        let shift = delta.hsl.get(band);
        assert!(
            shift.h.abs() < 1e-6,
            "{band:?} was given a hue rotation, which this phase never solves"
        );
    }
}

// ---------------------------------------------------------------------------
// 10.1 row 4: the refinement only ever improves
// ---------------------------------------------------------------------------

#[test]
fn refinement_never_makes_the_match_worse() {
    let baseline = fixtures::aggregate(24, SyntheticLook::neutral());
    let reference = fixtures::aggregate(24, SyntheticLook::light_and_airy());

    let initial = solve::initial(&reference, &baseline);
    let probe = AnalyticProbe {
        base: baseline.clone(),
    };
    let before = solve::distance(&probe.aggregate_with(&initial), &reference);
    let (_, after) = solve::refine(&initial, &reference, &probe);

    assert!(
        after <= before + 1e-4,
        "refinement made the match worse: {before} -> {after}"
    );
}

#[test]
fn refinement_closes_most_of_the_gap() {
    let baseline = fixtures::aggregate(24, SyntheticLook::neutral());
    let reference = fixtures::aggregate(24, SyntheticLook::light_and_airy());

    let probe = AnalyticProbe {
        base: baseline.clone(),
    };
    let unmatched = solve::distance(&baseline, &reference);
    let initial = solve::initial(&reference, &baseline);
    let (_, matched) = solve::refine(&initial, &reference, &probe);

    let closed = (unmatched - matched) / unmatched.max(1e-6);
    assert!(
        closed > 0.5,
        "the solver closed {:.0}% of a {unmatched:.2} dE00 gap, which is not a match",
        closed * 100.0
    );
}

// ---------------------------------------------------------------------------
// 10.1 row 5: determinism, including the identifiers
// ---------------------------------------------------------------------------

#[test]
fn the_same_reference_produces_the_same_look() {
    let look = SyntheticLook::warm_film();
    let one = fixtures::readings(24, look);
    let two = fixtures::readings(24, look);

    assert_eq!(one.len(), two.len());
    for (a, b) in one.iter().zip(two.iter()) {
        // Phase 29's lesson: a determinism test that does not compare the identifiers is not a
        // determinism test.
        assert_eq!(a.key, b.key, "the same gallery produced different keys");
        assert_eq!(a, b, "the same photograph measured differently twice");
    }

    let first = aggregate::fold(&aggregate::all(&one));
    let second = aggregate::fold(&aggregate::all(&two));
    assert_eq!(first, second, "the same gallery folded differently twice");
}

// ---------------------------------------------------------------------------
// 10.1 row 6: the measurement does not move with the scale it is taken at
// ---------------------------------------------------------------------------

#[test]
fn a_reading_does_not_depend_much_on_the_scale_it_was_taken_at() {
    let image = fixtures::apply(
        &fixtures::plate(768, 512, 7),
        SyntheticLook::light_and_airy(),
    );

    let full = measure::read_at("plate", &image, 768);
    let proxy = measure::read_at("plate", &image, 256);

    assert!(
        (full.tone.p50 - proxy.tone.p50).abs() < 0.02,
        "the median tone moved with the scale: {} against {}",
        full.tone.p50,
        proxy.tone.p50
    );
    assert!(
        (full.chroma_p50 - proxy.chroma_p50).abs() < 0.02,
        "the median chroma moved with the scale: {} against {}",
        full.chroma_p50,
        proxy.chroma_p50
    );
}

// ---------------------------------------------------------------------------
// 10.1 row 7: the thresholds are ones a correct implementation can meet
// ---------------------------------------------------------------------------

/// Phases 19, 21, 22, 25 and 29 each shipped a gate a correct implementation could not meet.
/// This is the cheapest possible guard against the sixth: a refusal floor above the usable
/// count would refuse every look it then called weak.
///
/// A `const` assertion rather than a test body, so it fails at compile time and cannot be
/// skipped by a filtered test run.
const _: () = assert!(
    MIN_REFERENCES < USABLE_REFERENCES,
    "the refusal floor is at or above the usable count, so no look can be weak"
);

#[test]
fn an_empty_aggregate_solves_to_nothing_rather_than_to_a_guess() {
    let empty = LookAggregate::default();
    let real = fixtures::aggregate(24, SyntheticLook::light_and_airy());

    assert!(solve::initial(&real, &empty).is_neutral());
    assert!(solve::initial(&empty, &real).is_neutral());
}
