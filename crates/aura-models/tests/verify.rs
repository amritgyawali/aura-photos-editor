//! Digests and signatures, on their own.
//!
//! `registry.rs` proves the chain end to end; this file proves the two
//! primitives, so a failure says which one moved.

use aura_models::verify::{from_hex, sha256_hex, verify_file, verify_manifest};
use ed25519_dalek::{Signer, SigningKey};
use tempfile::tempdir;

#[test]
fn the_digest_matches_the_published_vector() {
    // sha256 of the empty string, from FIPS 180-4.
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn hex_parsing_round_trips_and_refuses_rubbish() {
    assert_eq!(
        from_hex("000f10ff80").expect("parse"),
        [0, 15, 16, 255, 128]
    );
    assert!(from_hex("abc").is_err(), "odd length");
    assert!(from_hex("zz").is_err(), "not hex");
}

#[test]
fn a_malformed_key_is_refused_before_any_signature_work() {
    let err = verify_manifest(b"payload", &[0u8; 64], &[0u8; 8]).expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5002");
}

#[test]
fn a_valid_signature_verifies_and_a_tampered_payload_does_not() {
    let key = SigningKey::from_bytes(&[42u8; 32]);
    let public = key.verifying_key();
    let signature = key.sign(b"models.lock bytes");

    verify_manifest(
        b"models.lock bytes",
        &signature.to_bytes(),
        public.as_bytes(),
    )
    .expect("the real payload verifies");

    let err = verify_manifest(
        b"models.lock bytes, edited",
        &signature.to_bytes(),
        public.as_bytes(),
    )
    .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5002");
}

#[test]
fn a_signature_from_another_key_does_not_verify() {
    let key = SigningKey::from_bytes(&[1u8; 32]);
    let other = SigningKey::from_bytes(&[2u8; 32]);
    let signature = key.sign(b"payload");

    let err = verify_manifest(
        b"payload",
        &signature.to_bytes(),
        other.verifying_key().as_bytes(),
    )
    .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5002");
}

#[test]
fn a_file_is_checked_on_its_size_before_its_digest() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("model.onnx");
    std::fs::write(&path, b"twelve bytes").expect("write");

    verify_file(&path, &sha256_hex(b"twelve bytes"), 12).expect("the real file verifies");

    let err = verify_file(&path, &sha256_hex(b"twelve bytes"), 13).expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5003");
    assert!(
        err.detail.contains("13 bytes"),
        "the refusal should name both sizes: {}",
        err.detail
    );
}

#[test]
fn a_file_whose_bytes_changed_is_refused_on_its_digest() {
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("model.onnx");
    std::fs::write(&path, b"twelve bytes").expect("write");

    let err = verify_file(&path, &sha256_hex(b"twelve bytez"), 12).expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5003");
}

#[test]
fn a_missing_file_is_refused_rather_than_treated_as_empty() {
    let dir = tempdir().expect("tempdir");
    let err =
        verify_file(&dir.path().join("absent.onnx"), &sha256_hex(b""), 0).expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5003");
}
