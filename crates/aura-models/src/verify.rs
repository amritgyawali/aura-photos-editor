//! Signature and digest verification.
//!
//! The order is fixed by Article IX rule S6 and is not negotiable: **signature,
//! then digest, then card, then load**. Each step is only meaningful if the one
//! before it passed - a digest from an unsigned manifest proves nothing, and a
//! card named by an unverified manifest is a claim rather than a fact.
//!
//! Nothing here touches the network. Verification runs identically on a machine
//! that has been offline for a month, which is what makes Law 4 - offline is the
//! default - true of model updates as well as of analysis.

use std::fs;
use std::path::Path;

use aura_core::AuraResult;
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

/// Bytes read at a time when digesting a file.
///
/// A 400 MB model must not be read into memory to be digested: the machine
/// verifying it may be the 16 GB laptop from the reference set, and it may be
/// verifying during an import.
const DIGEST_CHUNK_BYTES: usize = 1024 * 1024;

/// Lower-case hex sha256 of a byte slice.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex(&hasher.finalize())
}

/// Lower-case hex sha256 of a file, read in chunks.
///
/// # Errors
///
/// `AURA-ML-5003` when the file cannot be read.
pub fn sha256_file(path: &Path) -> AuraResult<String> {
    use std::io::Read;

    let mut file = fs::File::open(path).map_err(|err| {
        aura_core::errors::ml::digest_mismatch(
            &aura_core::redact::redact_path(path),
            "readable",
            &format!("unreadable: {err}"),
        )
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; DIGEST_CHUNK_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(|err| {
            aura_core::errors::ml::digest_mismatch(
                &aura_core::redact::redact_path(path),
                "readable",
                &format!("read failed: {err}"),
            )
        })?;
        if read == 0 {
            break;
        }
        match buffer.get(..read) {
            Some(chunk) => hasher.update(chunk),
            None => break,
        }
    }
    Ok(hex(&hasher.finalize()))
}

/// Verify a detached ed25519 signature over the manifest bytes.
///
/// # Errors
///
/// `AURA-ML-5002` when the key or the signature is malformed, or the signature
/// does not verify. All three are the same answer to the operator - re-install
/// the pack - and all three are a refusal, never a warning.
pub fn verify_manifest(
    manifest_bytes: &[u8],
    signature_bytes: &[u8],
    public_key: &[u8],
) -> AuraResult<()> {
    let key: [u8; 32] = public_key.try_into().map_err(|_| {
        aura_core::errors::ml::signature_invalid(format!(
            "release public key is {} bytes, expected 32",
            public_key.len()
        ))
    })?;
    let verifying_key = VerifyingKey::from_bytes(&key).map_err(|err| {
        aura_core::errors::ml::signature_invalid(format!("release public key is not valid: {err}"))
    })?;

    let signature: [u8; 64] = signature_bytes.try_into().map_err(|_| {
        aura_core::errors::ml::signature_invalid(format!(
            "signature is {} bytes, expected 64",
            signature_bytes.len()
        ))
    })?;
    let signature = Signature::from_bytes(&signature);

    verifying_key
        .verify_strict(manifest_bytes, &signature)
        .map_err(|err| {
            aura_core::errors::ml::signature_invalid(format!(
                "manifest signature did not verify: {err}"
            ))
        })
}

/// Verify one file's size and digest against the manifest.
///
/// Size first: a truncated download is the common case and comparing two integers
/// is free, where digesting 400 MB to reach the same conclusion is not.
///
/// # Errors
///
/// `AURA-ML-5003` when the size or the digest disagrees with the manifest.
pub fn verify_file(path: &Path, expected_sha256: &str, expected_bytes: u64) -> AuraResult<()> {
    let redacted = aura_core::redact::redact_path(path);
    let metadata = fs::metadata(path).map_err(|err| {
        aura_core::errors::ml::digest_mismatch(&redacted, "present", &format!("missing: {err}"))
    })?;
    if metadata.len() != expected_bytes {
        return Err(aura_core::errors::ml::digest_mismatch(
            &redacted,
            &format!("{expected_bytes} bytes"),
            &format!("{} bytes", metadata.len()),
        ));
    }
    let actual = sha256_file(path)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(aura_core::errors::ml::digest_mismatch(
            &redacted,
            expected_sha256,
            &actual,
        ));
    }
    Ok(())
}

/// Parse a lower-case hex string into bytes.
///
/// # Errors
///
/// `AURA-ML-5002` when the text is not an even number of hex digits.
pub fn from_hex(text: &str) -> AuraResult<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return Err(aura_core::errors::ml::signature_invalid(format!(
            "hex string of odd length {}",
            text.len()
        )));
    }
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(text.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = hex_digit(pair.first().copied().unwrap_or(b'x'))?;
        let low = hex_digit(pair.get(1).copied().unwrap_or(b'x'))?;
        out.push(high << 4 | low);
    }
    Ok(out)
}

fn hex_digit(byte: u8) -> AuraResult<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        other => Err(aura_core::errors::ml::signature_invalid(format!(
            "{} is not a hex digit",
            other as char
        ))),
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;

    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        // Writing into a String cannot fail; the error is matched away rather
        // than unwrapped, which R1 forbids.
        if write!(out, "{byte:02x}").is_err() {
            return String::new();
        }
    }
    out
}
