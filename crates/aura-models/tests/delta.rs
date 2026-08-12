//! The `AURADLT1` block delta, round-tripped and broken on purpose.
//!
//! A delta is an optimisation, and an optimisation that cannot prove its result
//! is worse than not having one. Every failure below must produce
//! `AURA-ML-5012`, which tells the caller to fetch the whole file instead.

use aura_models::delta::{apply, encode};

fn base_file() -> Vec<u8> {
    (0..40_000u32).map(|i| (i % 251) as u8).collect()
}

#[test]
fn a_delta_round_trips_a_small_change() {
    let base = base_file();
    let mut target = base.clone();
    if let Some(slot) = target.get_mut(20_000) {
        *slot = 42;
    }

    let delta = encode(&base, &target);
    assert!(
        delta.len() < base.len() / 2,
        "a one-byte change produced a {} byte delta against a {} byte file",
        delta.len(),
        base.len()
    );
    assert_eq!(apply(&base, &delta).expect("apply"), target);
}

#[test]
fn a_delta_round_trips_an_insertion_and_a_truncation() {
    let base = base_file();

    let mut inserted = base.clone();
    inserted.splice(1000..1000, [7u8; 33]);
    assert_eq!(
        apply(&base, &encode(&base, &inserted)).expect("apply"),
        inserted
    );

    let truncated: Vec<u8> = base.iter().take(9_000).copied().collect();
    assert_eq!(
        apply(&base, &encode(&base, &truncated)).expect("apply"),
        truncated
    );
}

#[test]
fn applying_against_the_wrong_base_is_refused() {
    let base = base_file();
    let target: Vec<u8> = base.iter().map(|b| b.wrapping_add(1)).collect();
    let delta = encode(&base, &target);

    let mut wrong_base = base.clone();
    if let Some(slot) = wrong_base.get_mut(0) {
        *slot = 99;
    }
    assert_eq!(
        apply(&wrong_base, &delta).expect_err("must refuse").code.0,
        "AURA-ML-5012"
    );
}

#[test]
fn a_truncated_delta_is_refused_rather_than_applied_partially() {
    let base = base_file();
    let target: Vec<u8> = base.iter().rev().copied().collect();
    let delta = encode(&base, &target);
    let cut: Vec<u8> = delta.iter().take(delta.len() - 10).copied().collect();
    assert_eq!(
        apply(&base, &cut).expect_err("must refuse").code.0,
        "AURA-ML-5012"
    );
}

#[test]
fn a_delta_with_the_wrong_magic_is_refused_immediately() {
    let base = base_file();
    let mut delta = encode(&base, &base);
    if let Some(slot) = delta.get_mut(0) {
        *slot = b'X';
    }
    assert_eq!(
        apply(&base, &delta).expect_err("must refuse").code.0,
        "AURA-ML-5012"
    );
}

#[test]
fn a_delta_whose_payload_was_tampered_with_fails_its_target_digest() {
    let base = base_file();
    let mut target = base.clone();
    target.extend_from_slice(b"a new tensor");
    let mut delta = encode(&base, &target);

    // Flip a byte in the literal section, past the header.
    let last = delta.len() - 1;
    if let Some(slot) = delta.get_mut(last) {
        *slot ^= 0xff;
    }
    assert_eq!(
        apply(&base, &delta).expect_err("must refuse").code.0,
        "AURA-ML-5012"
    );
}
