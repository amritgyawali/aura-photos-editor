//! Versioning, canonical form, signing, export and import.
//!
//! PHASE-17 section 2.1: "Profile versioning, export/import (signed `.auraprofile` bundle), and
//! A/B comparison against the previous version before adoption." Section 6.4: "Profile bundles
//! are signed and portable so a studio can distribute one look to a team."
//!
//! ## What the signature proves, said plainly and said once
//!
//! **It proves the document has not changed since it was signed by whoever holds the embedded
//! key. It does not prove who that was.**
//!
//! There is no key distribution in this product and nothing to check a key against, so a
//! signature made with a key that travels inside the bundle is an integrity guarantee and not an
//! authenticity one. That is enough for the threat this phase actually has - a profile corrupted
//! in transit, or hand-edited to carry parameters no fitter would produce - and it is not enough
//! for "this look really came from that studio". ADR-0035 decision 8.
//!
//! The panel shows the fingerprint and the words "unchanged since signing". It never shows the
//! word "verified", and `ProfileReport.test.tsx` asserts that it never will.
//!
//! ## The canonical form, and why the bundle carries a digest as well as a signature
//!
//! [`canonical`] serialises a profile with sorted keys, fixed float formatting and no
//! whitespace, which is the same discipline `aura_recipe::hash::canonical` applies to a recipe
//! and for the same reason: a hash of a document that can be re-serialised two ways is a hash of
//! a formatting choice.
//!
//! The digest is stored beside the signature even though the signature already covers the bytes,
//! because the two answer different questions at different costs. A digest mismatch is a
//! truncated download and is detectable in a millisecond; a signature failure needs the
//! ed25519 verification and means something more serious. Checking the cheap one first is why
//! [`import`] refuses a 3 MB blob without ever parsing it.
//!
//! ## Why every version is kept
//!
//! A gallery delivered under version 3 has to stay reproducible after version 4 is adopted -
//! invariant 4 reaching further than one render. So training writes a **new row** with the next
//! version for that name, adoption retires the previous one, and nothing is ever updated in
//! place or deleted. `profiles.status` is the three-state lifecycle and migration 17's partial
//! unique index is what enforces "at most one adopted version per name" in the database rather
//! than in the code that writes it.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use aura_core::contract::colour::{HslAdjustments, HslBand, HslShift};
use aura_core::contract::style::{
    BucketDiagnostic, BucketModel, CurveShift, FallbackLevel, ProfileDiagnostics, ProfileStatus,
    SceneGroup, SkinBias, StyleBucket, StyleDelta, StyleProfile, MAX_BUNDLE_BYTES,
};
use aura_core::{AuraResult, ProfileId};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};

use crate::errors;

/// The bundle format's magic string.
pub const MAGIC: &str = "AURAPROF";

/// The bundle schema this build writes and the highest it reads.
///
/// A bundle from a future schema is **refused rather than partially read**. A partially read
/// profile is a look nobody chose, and the fields a newer build added are exactly the ones that
/// would be missing.
pub const BUNDLE_SCHEMA: u16 = 1;

/// How many hex characters of the public key the panel shows.
///
/// Sixteen, in groups of four. Long enough that two studios' keys will not collide by accident
/// and short enough that somebody can read it down a telephone, which is the only way it is ever
/// actually compared.
pub const FINGERPRINT_CHARS: usize = 16;

/// A signing identity for this installation.
///
/// The private half is thirty-two bytes the caller keeps; this type is what turns it into
/// signatures. It is deliberately **not** stored by this crate: where an installation's key
/// lives is a platform question and `aura-app` owns it, exactly as it owns where a cloud API key
/// lives.
#[derive(Debug, Clone)]
pub struct Identity {
    key: SigningKey,
}

impl Identity {
    /// An identity from thirty-two secret bytes.
    #[must_use]
    pub fn from_seed(seed: [u8; 32]) -> Self {
        Self {
            key: SigningKey::from_bytes(&seed),
        }
    }

    /// A fresh identity from the operating system's randomness.
    ///
    /// The bytes come from two v4 UUIDs, which is `getrandom` on every platform this product
    /// runs on and is already a workspace dependency. `tools/model-sign` reads `/dev/urandom`
    /// directly and cannot do so on Windows; a release signing key is generated once on an
    /// offline machine and a profile key is generated on a photographer's laptop, so the two
    /// have different constraints and get different answers.
    #[must_use]
    pub fn generate() -> Self {
        let mut seed = [0u8; 32];
        let first = *uuid::Uuid::new_v4().as_bytes();
        let second = *uuid::Uuid::new_v4().as_bytes();
        if let Some(head) = seed.get_mut(..16) {
            head.copy_from_slice(&first);
        }
        if let Some(tail) = seed.get_mut(16..) {
            tail.copy_from_slice(&second);
        }
        Self::from_seed(seed)
    }

    /// The public half, as it appears in a bundle.
    #[must_use]
    pub fn public_hex(&self) -> String {
        hex(self.key.verifying_key().as_bytes())
    }

    /// What the panel shows.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        fingerprint_of(&self.public_hex())
    }
}

/// A signed, portable profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    /// The canonical bytes the signature covers.
    pub body: String,
    /// BLAKE3 of [`Bundle::body`], hex.
    pub digest: String,
    /// The detached ed25519 signature, hex.
    pub signature: String,
    /// The public key, hex.
    pub public_key: String,
}

impl Bundle {
    /// The whole file, as it is written to disk.
    #[must_use]
    pub fn to_file(&self) -> String {
        // Header lines then the body, so a refusal can read the first four lines and stop. The
        // body is one line of canonical JSON and may be megabytes; the header is never more than
        // three hundred bytes.
        format!(
            "{MAGIC}\nschema={BUNDLE_SCHEMA}\ndigest={}\nkey={}\nsig={}\n{}",
            self.digest, self.public_key, self.signature, self.body
        )
    }

    /// What the panel shows about where this bundle came from.
    #[must_use]
    pub fn fingerprint(&self) -> String {
        fingerprint_of(&self.public_key)
    }
}

/// Sign a profile into a portable bundle.
///
/// # Errors
///
/// `AURA-ML-5076` when the canonical form would exceed [`MAX_BUNDLE_BYTES`], which means the
/// profile is carrying something a profile does not carry.
pub fn export(profile: &StyleProfile, identity: &Identity) -> AuraResult<Bundle> {
    let body = canonical(profile);
    if body.len() as u64 > MAX_BUNDLE_BYTES {
        return Err(errors::bundle_refused(format!(
            "the canonical profile is {} bytes against a ceiling of {MAX_BUNDLE_BYTES}",
            body.len()
        )));
    }
    let digest = hex(blake3::hash(body.as_bytes()).as_bytes());
    let signature = identity.key.sign(body.as_bytes());
    Ok(Bundle {
        body,
        digest,
        signature: hex(&signature.to_bytes()),
        public_key: identity.public_hex(),
    })
}

/// What one import produced.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The profile.
    pub profile: StyleProfile,
    /// The key's fingerprint, for the panel.
    pub fingerprint: String,
    /// True when the document is unchanged since it was signed.
    ///
    /// **Not called `verified`.** See this module's header.
    pub unchanged_since_signing: bool,
}

/// Read a bundle, refusing anything that is not one.
///
/// The five checks run in this order, and the order is part of the defence: size, then magic,
/// then schema, then digest, then signature. Nothing large is ever parsed, nothing from a future
/// build is ever partially read, and the expensive elliptic-curve check happens last.
///
/// A bundle that fails any of them raises `AURA-ML-5076` and **no profile is produced**. There
/// is no flag to skip the checks and adding one would make the only integrity guarantee in the
/// phase optional.
///
/// # Errors
///
/// `AURA-ML-5076` for every one of the five.
pub fn import(text: &str) -> AuraResult<Imported> {
    if text.len() as u64 > MAX_BUNDLE_BYTES {
        return Err(errors::bundle_refused(format!(
            "the bundle is {} bytes against a ceiling of {MAX_BUNDLE_BYTES}",
            text.len()
        )));
    }
    let mut lines = text.lines();
    if lines.next() != Some(MAGIC) {
        return Err(errors::bundle_refused(
            "this file does not begin with the style-profile marker, so it is not a profile",
        ));
    }
    let mut header: BTreeMap<&str, &str> = BTreeMap::new();
    let mut consumed = MAGIC.len() + 1;
    // Four header lines: schema, digest, key, sig. Read as a fixed count rather than "until a
    // blank line", so a body that happens to begin with `key=` cannot extend the header.
    for _ in 0..4 {
        let Some(line) = lines.next() else {
            return Err(errors::bundle_refused("the bundle header is truncated"));
        };
        consumed += line.len() + 1;
        let Some((key, value)) = line.split_once('=') else {
            return Err(errors::bundle_refused(format!(
                "the bundle header line {line:?} is not a key and a value"
            )));
        };
        header.insert(key, value);
    }
    // The schema line is written first and read here, so a future bundle is refused before its
    // body is looked at.
    let Some(schema) = header
        .get("schema")
        .and_then(|value| value.parse::<u16>().ok())
    else {
        return Err(errors::bundle_refused("the bundle declares no schema"));
    };
    if schema > BUNDLE_SCHEMA {
        return Err(errors::bundle_refused(format!(
            "the bundle is schema {schema} and this build reads up to {BUNDLE_SCHEMA}; a \
             partially read profile is a look nobody chose"
        )));
    }
    let Some(body) = text.get(consumed..) else {
        return Err(errors::bundle_refused("the bundle has no body"));
    };

    let (Some(digest), Some(key_hex), Some(sig_hex)) = (
        header.get("digest").copied(),
        header.get("key").copied(),
        header.get("sig").copied(),
    ) else {
        return Err(errors::bundle_refused(
            "the bundle header is missing its digest, key or signature",
        ));
    };

    let measured = hex(blake3::hash(body.as_bytes()).as_bytes());
    if measured != digest {
        return Err(errors::bundle_refused(format!(
            "the bundle's contents do not match its digest: {digest} declared, {measured} \
             measured"
        )));
    }

    let key_bytes: [u8; 32] = unhex(key_hex)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| errors::bundle_refused("the bundle's public key is not 32 bytes"))?;
    let verifying = VerifyingKey::from_bytes(&key_bytes).map_err(|err| {
        errors::bundle_refused(format!("the bundle's public key is not valid: {err}"))
    })?;
    let sig_bytes: [u8; 64] = unhex(sig_hex)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or_else(|| errors::bundle_refused("the bundle's signature is not 64 bytes"))?;
    let signature = ed25519_dalek::Signature::from_bytes(&sig_bytes);
    verifying
        .verify(body.as_bytes(), &signature)
        .map_err(|err| {
            errors::bundle_refused(format!(
                "the bundle has been altered since it was signed: {err}"
            ))
        })?;

    let profile = parse(body)?;
    Ok(Imported {
        profile,
        fingerprint: fingerprint_of(key_hex),
        unchanged_since_signing: true,
    })
}

/// The canonical serialisation a signature covers.
///
/// Sorted keys, six decimal places on every float, no whitespace. Six because the deltas are
/// stored to about a thousandth of their range and a seventh digit is the difference between two
/// machines' `f32` formatting rather than a difference between two profiles.
#[must_use]
pub fn canonical(profile: &StyleProfile) -> String {
    let mut out = String::new();
    out.push('{');
    let _ = write!(out, "\"analysis_ver\":{}", profile.analysis_ver);
    let _ = write!(out, ",\"buckets\":[");
    for (index, (bucket, model)) in profile.buckets.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"bucket\":\"{}\",\"delta\":{},\"held_out\":{},\"match_de00\":{},\"samples\":{},\
             \"shrink\":{}}}",
            bucket.as_key(),
            delta_json(&model.delta),
            model.held_out,
            model.match_de00.map_or_else(|| "null".to_string(), number),
            model.samples,
            number(model.shrink)
        );
    }
    out.push(']');
    let _ = write!(out, ",\"engine_ver\":\"{}\"", escape(&profile.engine_ver));
    let _ = write!(out, ",\"global\":{}", delta_json(&profile.global));
    let _ = write!(out, ",\"groups\":[");
    for (index, (group, delta)) in profile.groups.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let _ = write!(
            out,
            "{{\"delta\":{},\"group\":\"{}\"}}",
            delta_json(delta),
            group.as_str()
        );
    }
    out.push(']');
    let _ = write!(out, ",\"id\":\"{}\"", profile.id.to_db());
    let _ = write!(out, ",\"name\":\"{}\"", escape(&profile.name));
    let _ = write!(
        out,
        ",\"overall_de00\":{}",
        number(profile.diagnostics.overall_de00)
    );
    let _ = write!(
        out,
        ",\"recommendation\":\"{}\"",
        escape(&profile.diagnostics.recommendation)
    );
    let _ = write!(out, ",\"trained_at\":{}", profile.trained_at);
    let _ = write!(out, ",\"trained_pairs\":{}", profile.trained_pairs);
    let _ = write!(out, ",\"version\":{}", profile.version);
    out.push('}');
    out
}

/// Read a canonical profile back.
///
/// The parser is deliberately narrow: it reads the shape [`canonical`] writes and refuses
/// anything else, exactly as `aura_raw::container` reads a TIFF. A general JSON parser here
/// would accept documents this product never produces and would have to decide what to do with
/// them.
///
/// # Errors
///
/// `AURA-ML-5076` when the body is not a canonical profile.
#[allow(clippy::too_many_lines)]
pub fn parse(body: &str) -> AuraResult<StyleProfile> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|err| errors::bundle_refused(format!("the bundle body is not JSON: {err}")))?;
    let object = value
        .as_object()
        .ok_or_else(|| errors::bundle_refused("the bundle body is not an object"))?;

    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .and_then(|text| ProfileId::from_db(text).ok())
        .ok_or_else(|| errors::bundle_refused("the bundle has no profile id"))?;
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("imported")
        .to_string();
    let engine = object
        .get("engine_ver")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();

    let mut profile = StyleProfile::empty(id, name, engine);
    profile.version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1) as u16;
    profile.analysis_ver = object
        .get("analysis_ver")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u16;
    profile.trained_pairs = object
        .get("trained_pairs")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u32;
    profile.trained_at = object
        .get("trained_at")
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    // An imported profile is always a candidate, whatever it says about itself. Adoption is an
    // act somebody performs on this machine (section 6.3), and a bundle that could arrive
    // already adopted would be a file that changes how every photograph looks by being opened.
    profile.status = ProfileStatus::Candidate;

    if let Some(global) = object.get("global") {
        profile.global = delta_from(global);
    }
    if let Some(groups) = object.get("groups").and_then(serde_json::Value::as_array) {
        for entry in groups {
            let (Some(group), Some(delta)) = (
                entry
                    .get("group")
                    .and_then(serde_json::Value::as_str)
                    .map(SceneGroup::from_str_or_other),
                entry.get("delta"),
            ) else {
                continue;
            };
            profile.groups.insert(group, delta_from(delta));
        }
    }
    let mut per_bucket = Vec::new();
    if let Some(buckets) = object.get("buckets").and_then(serde_json::Value::as_array) {
        for entry in buckets {
            let Some(key) = entry.get("bucket").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let bucket = StyleBucket::from_key(key);
            let delta = entry.get("delta").map(delta_from).unwrap_or_default();
            let samples = entry
                .get("samples")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32;
            let held_out = entry
                .get("held_out")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0) as u32;
            let match_de00 = entry
                .get("match_de00")
                .and_then(serde_json::Value::as_f64)
                .map(|value| value as f32);
            let shrink = entry
                .get("shrink")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0) as f32;
            per_bucket.push(BucketDiagnostic {
                bucket,
                samples,
                match_de00,
                level: if delta.confidence >= aura_core::contract::style::APPLY_ABOVE {
                    FallbackLevel::Bucket
                } else {
                    FallbackLevel::Group
                },
            });
            profile.buckets.insert(
                bucket,
                BucketModel {
                    bucket,
                    delta,
                    samples,
                    held_out,
                    match_de00,
                    shrink,
                },
            );
        }
    }
    profile.diagnostics = ProfileDiagnostics {
        per_bucket,
        weak_buckets: profile
            .buckets
            .values()
            .filter(|model| model.is_weak())
            .map(|model| model.bucket)
            .collect(),
        overall_de00: object
            .get("overall_de00")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32,
        accepted_pairs: profile.trained_pairs,
        rejected_pairs: 0,
        recommendation: object
            .get("recommendation")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string(),
    };
    Ok(profile)
}

/// What the panel shows about a key.
#[must_use]
pub fn fingerprint_of(public_hex: &str) -> String {
    let head: String = public_hex.chars().take(FINGERPRINT_CHARS).collect();
    head.as_bytes()
        .chunks(4)
        .filter_map(|chunk| std::str::from_utf8(chunk).ok())
        .collect::<Vec<&str>>()
        .join("-")
}

/// The next version number for one name.
#[must_use]
pub const fn next_version(previous: Option<u16>) -> u16 {
    match previous {
        Some(version) => version.saturating_add(1),
        None => 1,
    }
}

fn delta_json(delta: &StyleDelta) -> String {
    let mut out = String::new();
    out.push('{');
    let _ = write!(out, "\"blacks\":{}", number(delta.blacks));
    let _ = write!(out, ",\"confidence\":{}", number(delta.confidence));
    let _ = write!(out, ",\"contrast\":{}", number(delta.contrast));
    let _ = write!(out, ",\"curve\":[");
    for (index, value) in delta.curve_shift.as_array().iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&number(*value));
    }
    out.push(']');
    let _ = write!(out, ",\"exposure\":{}", number(delta.exposure));
    let _ = write!(out, ",\"highlights\":{}", number(delta.highlights));
    let _ = write!(out, ",\"hsl\":[");
    for (index, band) in HslBand::ALL.into_iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        let shift = delta.hsl.get(band);
        let _ = write!(
            out,
            "{},{},{}",
            number(shift.h),
            number(shift.s),
            number(shift.l)
        );
    }
    out.push(']');
    let _ = write!(out, ",\"samples\":{}", delta.samples);
    let _ = write!(out, ",\"saturation\":{}", number(delta.saturation));
    let _ = write!(out, ",\"shadows\":{}", number(delta.shadows));
    let _ = write!(
        out,
        ",\"skin\":[{},{}]",
        number(delta.skin_bias.warmth_deg),
        number(delta.skin_bias.chroma)
    );
    let _ = write!(out, ",\"temperature_k\":{}", number(delta.temperature_k));
    let _ = write!(out, ",\"tint\":{}", number(delta.tint));
    let _ = write!(out, ",\"vibrance\":{}", number(delta.vibrance));
    let _ = write!(out, ",\"whites\":{}", number(delta.whites));
    out.push('}');
    out
}

fn delta_from(value: &serde_json::Value) -> StyleDelta {
    let at = |key: &str| -> f32 {
        value
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0) as f32
    };
    let array = |key: &str| -> Vec<f32> {
        value
            .get(key)
            .and_then(serde_json::Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.as_f64().unwrap_or(0.0) as f32)
                    .collect()
            })
            .unwrap_or_default()
    };
    let curve = array("curve");
    let hsl_flat = array("hsl");
    let skin = array("skin");

    let mut hsl = HslAdjustments::default();
    for (index, band) in HslBand::ALL.into_iter().enumerate() {
        hsl.set(
            band,
            HslShift {
                h: hsl_flat.get(index * 3).copied().unwrap_or(0.0),
                s: hsl_flat.get(index * 3 + 1).copied().unwrap_or(0.0),
                l: hsl_flat.get(index * 3 + 2).copied().unwrap_or(0.0),
            },
        );
    }
    StyleDelta {
        exposure: at("exposure"),
        temperature_k: at("temperature_k"),
        tint: at("tint"),
        contrast: at("contrast"),
        highlights: at("highlights"),
        shadows: at("shadows"),
        whites: at("whites"),
        blacks: at("blacks"),
        curve_shift: CurveShift::from_array([
            curve.first().copied().unwrap_or(0.0),
            curve.get(1).copied().unwrap_or(0.0),
            curve.get(2).copied().unwrap_or(0.0),
            curve.get(3).copied().unwrap_or(0.0),
            curve.get(4).copied().unwrap_or(0.0),
        ]),
        hsl,
        vibrance: at("vibrance"),
        saturation: at("saturation"),
        skin_bias: SkinBias {
            warmth_deg: skin.first().copied().unwrap_or(0.0),
            chroma: skin.get(1).copied().unwrap_or(0.0),
        },
        confidence: at("confidence"),
        samples: value
            .get("samples")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32,
    }
    // Clamped on the way in as well as on the way out. A hand-edited bundle whose signature was
    // re-made with its own key would otherwise be a valid document carrying a style outside every
    // bound this phase documents.
    .clamped()
}

/// Six decimal places, and never `-0`.
fn number(value: f32) -> String {
    let value = if value == 0.0 { 0.0 } else { value };
    format!("{value:.6}")
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    let text = text.trim();
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 2);
    for index in 0..text.len() / 2 {
        let pair = text.get(index * 2..index * 2 + 2)?;
        out.push(u8::from_str_radix(pair, 16).ok()?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use aura_core::contract::style::LightingBucket;

    fn identity() -> Identity {
        Identity::from_seed([7u8; 32])
    }

    fn profile() -> StyleProfile {
        let mut profile =
            StyleProfile::empty(ProfileId::new(), "light and airy", "aura-render/1.0.0");
        profile.version = 3;
        profile.trained_pairs = 812;
        profile.trained_at = 1_700_000_000_000;
        profile.analysis_ver = 1;
        profile.global = StyleDelta {
            exposure: 0.21,
            contrast: -6.0,
            shadows: 12.0,
            confidence: 0.86,
            samples: 812,
            ..StyleDelta::neutral()
        };
        profile.groups.insert(
            SceneGroup::Portraits,
            StyleDelta {
                vibrance: 5.0,
                confidence: 0.7,
                samples: 210,
                ..StyleDelta::neutral()
            },
        );
        let bucket = StyleBucket::new(SceneGroup::Portraits, LightingBucket::GoldenHour);
        profile.buckets.insert(
            bucket,
            BucketModel {
                bucket,
                delta: StyleDelta {
                    temperature_k: 180.0,
                    confidence: 0.62,
                    samples: 44,
                    ..StyleDelta::neutral()
                },
                samples: 44,
                held_out: 9,
                match_de00: Some(1.84),
                shrink: 0.62,
            },
        );
        profile.diagnostics.overall_de00 = 1.9;
        profile.diagnostics.recommendation = "add one indoor flash reception".to_string();
        profile
    }

    #[test]
    fn a_bundle_round_trips_with_its_signature_intact() {
        let original = profile();
        let Ok(bundle) = export(&original, &identity()) else {
            unreachable!("export of a small profile cannot fail")
        };
        let Ok(read) = import(&bundle.to_file()) else {
            unreachable!("a bundle this build wrote must import")
        };
        assert!(read.unchanged_since_signing);
        assert_eq!(read.profile.name, original.name);
        assert_eq!(read.profile.version, original.version);
        assert!((read.profile.global.exposure - 0.21).abs() < 1e-4);
        assert_eq!(read.profile.buckets.len(), 1);
    }

    #[test]
    fn a_tampered_body_is_refused() {
        let Ok(bundle) = export(&profile(), &identity()) else {
            unreachable!("export cannot fail here")
        };
        let file = bundle.to_file().replace("0.210000", "0.900000");
        let Err(err) = import(&file) else {
            unreachable!("a tampered bundle must be refused")
        };
        assert_eq!(err.code.0, "AURA-ML-5076");
    }

    #[test]
    fn a_bundle_signed_by_another_key_is_refused_when_the_body_is_swapped_in() {
        let Ok(mine) = export(&profile(), &identity()) else {
            unreachable!()
        };
        let other = Identity::from_seed([9u8; 32]);
        let mut theirs = profile();
        theirs.global.exposure = -0.4;
        let Ok(other_bundle) = export(&theirs, &other) else {
            unreachable!()
        };
        // Their body under my header: the digest check catches it before the signature does.
        let frankenstein = format!(
            "{MAGIC}\nschema={BUNDLE_SCHEMA}\ndigest={}\nkey={}\nsig={}\n{}",
            mine.digest, mine.public_key, mine.signature, other_bundle.body
        );
        let Err(err) = import(&frankenstein) else {
            unreachable!("a swapped body must be refused")
        };
        assert_eq!(err.code.0, "AURA-ML-5076");
    }

    #[test]
    fn a_future_schema_is_refused_rather_than_partially_read() {
        let Ok(bundle) = export(&profile(), &identity()) else {
            unreachable!()
        };
        let future = bundle
            .to_file()
            .replacen(&format!("schema={BUNDLE_SCHEMA}"), "schema=99", 1);
        let Err(err) = import(&future) else {
            unreachable!("a future schema must be refused")
        };
        assert_eq!(err.code.0, "AURA-ML-5076");
    }

    #[test]
    fn a_file_that_is_not_a_bundle_is_refused_without_being_parsed() {
        let Err(err) = import("{\"looks\":\"like json\"}") else {
            unreachable!("only a bundle may be imported")
        };
        assert_eq!(err.code.0, "AURA-ML-5076");
    }

    #[test]
    fn an_oversized_file_is_refused_before_anything_reads_it() {
        let huge = "x".repeat((MAX_BUNDLE_BYTES + 1) as usize);
        let Err(err) = import(&huge) else {
            unreachable!("a bundle above the ceiling must be refused")
        };
        assert_eq!(err.code.0, "AURA-ML-5076");
    }

    #[test]
    fn an_imported_profile_arrives_as_a_candidate_however_it_was_exported() {
        let mut adopted = profile();
        adopted.status = ProfileStatus::Adopted;
        let Ok(bundle) = export(&adopted, &identity()) else {
            unreachable!()
        };
        let Ok(read) = import(&bundle.to_file()) else {
            unreachable!()
        };
        assert_eq!(read.profile.status, ProfileStatus::Candidate);
    }

    #[test]
    fn the_canonical_form_is_stable_across_two_serialisations() {
        let profile = profile();
        assert_eq!(canonical(&profile), canonical(&profile.clone()));
    }

    #[test]
    fn a_fingerprint_is_short_enough_to_read_down_a_telephone() {
        let fingerprint = identity().fingerprint();
        assert_eq!(fingerprint.len(), FINGERPRINT_CHARS + 3);
        assert!(fingerprint.contains('-'));
    }

    #[test]
    fn a_hand_edited_delta_outside_its_bounds_is_clamped_on_the_way_in() {
        let mut wild = profile();
        wild.global.exposure = 4.0;
        // Re-signed with a valid key, so every integrity check passes and only the clamp is left.
        let Ok(bundle) = export(&wild, &identity()) else {
            unreachable!()
        };
        let Ok(read) = import(&bundle.to_file()) else {
            unreachable!()
        };
        assert!(
            read.profile.global.exposure
                <= aura_core::contract::style::MAX_EXPOSURE_DELTA_EV + 1e-4
        );
    }

    #[test]
    fn versions_increase_and_start_at_one() {
        assert_eq!(next_version(None), 1);
        assert_eq!(next_version(Some(3)), 4);
        assert_eq!(next_version(Some(u16::MAX)), u16::MAX);
    }
}
