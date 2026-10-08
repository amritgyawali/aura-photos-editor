#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::disallowed_methods,
    clippy::disallowed_types,
    missing_debug_implementations,
    unreachable_pub,
    rust_2018_idioms
)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]

//! Offline licence keys. ADR-0105.
//!
//! A licence is a short JSON statement - who, which edition, issued when, until when - signed with
//! the vendor's ed25519 key. The application carries only the public half, so it can check a key
//! with no network and nothing to phone home to. A photographer working in a marquee with no
//! signal is never locked out of a licence they paid for.
//!
//! The key a photographer pastes is `AURA1.<payload>.<signature>`, both parts base64url without
//! padding. It is ordinary text: it survives an email, a PDF receipt and a chat message.
//!
//! What this deliberately is not: copy protection. Nothing here binds a key to a machine, and a
//! trial clock kept on the photographer's own disk can be reset by somebody determined to. The
//! aim is that paying is the easy path and that an honest customer never meets a check that fails
//! them, which is the trade every offline licence makes.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use time::macros::format_description;
use time::Date;

/// The prefix and format version of every key.
pub const PREFIX: &str = "AURA1";

/// How long a new installation may use everything before a licence is needed.
pub const TRIAL_DAYS: i64 = 14;

/// The vendor's public key. The private half lives offline with the vendor and never enters this
/// repository; `tools/licence-issue` signs with it.
pub const VENDOR_PUBLIC_KEY: [u8; 32] = [
    0xcf, 0xb6, 0xbf, 0x91, 0x85, 0x12, 0x33, 0x14, 0x21, 0xb8, 0x90, 0xfa, 0xaf, 0x70, 0xce, 0xa8,
    0x88, 0x67, 0x57, 0xf2, 0x34, 0x25, 0x74, 0x83, 0x59, 0xc0, 0x63, 0xb7, 0x7a, 0x33, 0xfd, 0x2c,
];

/// Who may use AURA, and on what terms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Licence {
    /// The vendor's own reference, e.g. an order number.
    pub id: String,
    /// The licensee as they should be greeted.
    pub name: String,
    /// Where the receipt went.
    pub email: String,
    /// `pro`, `studio`, ... Shown, not interpreted: every edition can use every feature today.
    pub edition: String,
    /// `YYYY-MM-DD`.
    pub issued: String,
    /// `YYYY-MM-DD`, or absent for a perpetual licence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
}

/// Why a key was refused, in words a photographer can act on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    /// Not the shape of an AURA key at all.
    Malformed,
    /// Shaped right, but not signed by the vendor - mistyped, truncated or made up.
    BadSignature,
    /// Signed, but the statement inside is not one this version understands.
    Unreadable(String),
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed => f.write_str(
                "That is not an AURA licence key. It starts with AURA1. - paste the whole key from your receipt.",
            ),
            Self::BadSignature => f.write_str(
                "That licence key does not check out. It may have been cut short or mistyped - paste it again from your receipt.",
            ),
            Self::Unreadable(why) => write!(
                f,
                "That licence key is for a different version of AURA ({why}). Update AURA, or contact support with the key."
            ),
        }
    }
}

impl std::error::Error for KeyError {}

fn date(text: &str) -> Option<Date> {
    Date::parse(text, format_description!("[year]-[month]-[day]")).ok()
}

impl Licence {
    /// The last day it is valid, if it ever stops being.
    #[must_use]
    pub fn expiry(&self) -> Option<Date> {
        self.expires.as_deref().and_then(date)
    }

    fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() || self.edition.trim().is_empty() {
            return Err("no licensee or edition".into());
        }
        if date(&self.issued).is_none() {
            return Err("issue date".into());
        }
        if self.expires.is_some() && self.expiry().is_none() {
            return Err("expiry date".into());
        }
        Ok(())
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 4 / 3 + 3);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk.first().copied().unwrap_or(0),
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..=chunk.len() {
            let index = (n >> (18 - 6 * i)) & 63;
            out.push(char::from(
                ALPHABET.get(index as usize).copied().unwrap_or(b'A'),
            ));
        }
    }
    out
}

fn b64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0_u32;
    let mut bits = 0_u32;
    for c in text.bytes() {
        let v = u32::try_from(ALPHABET.iter().position(|a| *a == c)?).ok()?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xff).ok()?);
        }
    }
    Some(out)
}

/// Sign a licence. Used only by the vendor's issuing tool.
///
/// # Errors
/// A licence whose dates or names are not well formed.
pub fn encode(licence: &Licence, key: &SigningKey) -> Result<String, KeyError> {
    licence.check().map_err(KeyError::Unreadable)?;
    let payload = serde_json::to_vec(licence).map_err(|e| KeyError::Unreadable(e.to_string()))?;
    let signature = key.sign(&payload);
    Ok(format!(
        "{PREFIX}.{}.{}",
        b64_encode(&payload),
        b64_encode(&signature.to_bytes())
    ))
}

/// Read and verify a pasted key against `public`. Whitespace and line breaks a mail client
/// inserts are ignored.
///
/// # Errors
/// [`KeyError`], each with a sentence for the photographer.
pub fn decode_with(key: &str, public: &VerifyingKey) -> Result<Licence, KeyError> {
    let compact: String = key.chars().filter(|c| !c.is_whitespace()).collect();
    let mut parts = compact.split('.');
    let (Some(PREFIX), Some(payload), Some(signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(KeyError::Malformed);
    };
    let payload = b64_decode(payload).ok_or(KeyError::Malformed)?;
    let signature: [u8; 64] = b64_decode(signature)
        .and_then(|s| s.try_into().ok())
        .ok_or(KeyError::Malformed)?;
    public
        .verify(&payload, &Signature::from_bytes(&signature))
        .map_err(|_| KeyError::BadSignature)?;
    let licence: Licence =
        serde_json::from_slice(&payload).map_err(|e| KeyError::Unreadable(e.to_string()))?;
    licence.check().map_err(KeyError::Unreadable)?;
    Ok(licence)
}

/// Read and verify a pasted key against the vendor's public key.
///
/// # Errors
/// As [`decode_with`].
pub fn decode(key: &str) -> Result<Licence, KeyError> {
    let public =
        VerifyingKey::from_bytes(&VENDOR_PUBLIC_KEY).map_err(|_| KeyError::BadSignature)?;
    decode_with(key, &public)
}

/// Where an installation stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// A valid licence.
    Licensed(Licence),
    /// A licence whose term has ended.
    LicenceExpired(Licence),
    /// Within the trial; `days_left` includes today.
    Trial { days_left: i64, ends: Date },
    /// The trial is over and there is no licence.
    TrialEnded { ended: Date },
}

impl Standing {
    /// Work out the standing from a verified licence (if any), the day the trial began, and today.
    #[must_use]
    pub fn of(licence: Option<Licence>, trial_started: Date, today: Date) -> Self {
        if let Some(licence) = licence {
            return match licence.expiry() {
                Some(last) if today > last => Self::LicenceExpired(licence),
                _ => Self::Licensed(licence),
            };
        }
        let ends = trial_started + time::Duration::days(TRIAL_DAYS - 1);
        if today <= ends {
            Self::Trial {
                days_left: (ends - today).whole_days() + 1,
                ends,
            }
        } else {
            Self::TrialEnded { ended: ends }
        }
    }

    /// Whether finished photographs may be written out. Editing never depends on this.
    #[must_use]
    pub fn may_export(&self) -> bool {
        matches!(self, Self::Licensed(_) | Self::Trial { .. })
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::indexing_slicing,
        clippy::disallowed_methods
    )]
    use super::*;
    use time::macros::date;

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    fn licence(expires: Option<&str>) -> Licence {
        Licence {
            id: "order-1001".into(),
            name: "Asha Studio".into(),
            email: "asha@example.com".into(),
            edition: "pro".into(),
            issued: "2026-10-08".into(),
            expires: expires.map(str::to_string),
        }
    }

    #[test]
    fn a_signed_key_reads_back_and_survives_an_email() {
        let text = encode(&licence(None), &key()).unwrap();
        assert!(text.starts_with("AURA1."));
        let wrapped: String = text
            .chars()
            .enumerate()
            .flat_map(|(i, c)| {
                if i % 40 == 39 {
                    vec![c, '\n', ' ']
                } else {
                    vec![c]
                }
            })
            .collect();
        assert_eq!(
            decode_with(&wrapped, &key().verifying_key()).unwrap(),
            licence(None)
        );
    }

    #[test]
    fn a_tampered_or_foreign_key_is_refused() {
        let text = encode(&licence(None), &key()).unwrap();
        let public = key().verifying_key();
        // Change the payload: claim a different name.
        let parts: Vec<&str> = text.split('.').collect();
        let forged_payload = b64_encode(
            &serde_json::to_vec(&Licence {
                name: "Somebody else".into(),
                ..licence(None)
            })
            .unwrap(),
        );
        let forged = format!("AURA1.{forged_payload}.{}", parts[2]);
        assert_eq!(decode_with(&forged, &public), Err(KeyError::BadSignature));
        // Signed by somebody other than the vendor.
        let other = encode(&licence(None), &SigningKey::from_bytes(&[9; 32])).unwrap();
        assert_eq!(decode_with(&other, &public), Err(KeyError::BadSignature));
        // Cut short, or not a key.
        assert!(decode_with(&text[..text.len() - 10], &public).is_err());
        assert_eq!(decode_with("hello", &public), Err(KeyError::Malformed));
        // And the shipped public key does not accept a key signed with a test key.
        assert!(decode(&text).is_err());
    }

    /// The shop's licence server (`shop/lib/licence.js`) signs the same licence with the same seed
    /// and asserts the same string, so the two implementations cannot drift apart.
    #[test]
    fn the_shop_and_the_application_write_the_same_key() {
        let licence = Licence {
            id: "sub_01crosscheck".into(),
            name: "Asha Studio".into(),
            email: "asha@example.com".into(),
            edition: "pro".into(),
            issued: "2026-10-08".into(),
            expires: Some("2026-11-15".into()),
        };
        let shop = "AURA1.eyJpZCI6InN1Yl8wMWNyb3NzY2hlY2siLCJuYW1lIjoiQXNoYSBTdHVkaW8iLCJlbWFpbCI6ImFzaGFAZXhhbXBsZS5jb20iLCJlZGl0aW9uIjoicHJvIiwiaXNzdWVkIjoiMjAyNi0xMC0wOCIsImV4cGlyZXMiOiIyMDI2LTExLTE1In0.wQQ-xPBkezUha17sAt21Mp5KUrOYLXH09shvgG4jowttQU_KsPaG3kO_VY5w8Clvk9hjZiCsujEpAc0f3bUlAA";
        assert_eq!(encode(&licence, &key()).unwrap(), shop);
        assert_eq!(decode_with(shop, &key().verifying_key()).unwrap(), licence);
    }

    #[test]
    fn base64_round_trips_every_length() {
        for n in 0..70_u8 {
            let bytes: Vec<u8> = (0..n).map(|i| i.wrapping_mul(37)).collect();
            assert_eq!(b64_decode(&b64_encode(&bytes)).unwrap(), bytes);
        }
    }

    #[test]
    fn the_trial_runs_fourteen_days_and_then_only_export_stops() {
        let start = date!(2026 - 10 - 01);
        let first = Standing::of(None, start, start);
        assert_eq!(
            first,
            Standing::Trial {
                days_left: 14,
                ends: date!(2026 - 10 - 14)
            }
        );
        assert!(first.may_export());
        let last = Standing::of(None, start, date!(2026 - 10 - 14));
        assert!(matches!(last, Standing::Trial { days_left: 1, .. }));
        let after = Standing::of(None, start, date!(2026 - 10 - 15));
        assert_eq!(
            after,
            Standing::TrialEnded {
                ended: date!(2026 - 10 - 14)
            }
        );
        assert!(!after.may_export());
        // A licence outranks the trial; a term licence ends after its last day.
        let term = licence(Some("2027-10-07"));
        assert!(Standing::of(Some(term.clone()), start, date!(2027 - 10 - 07)).may_export());
        let lapsed = Standing::of(Some(term), start, date!(2027 - 10 - 08));
        assert!(matches!(lapsed, Standing::LicenceExpired(_)));
        assert!(!lapsed.may_export());
    }
}
