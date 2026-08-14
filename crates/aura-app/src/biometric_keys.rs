//! The adapter between the biometric store and the operating system's credential
//! store.
//!
//! `aura-people` declares [`BiometricKeyStore`] as a port and implements none of it.
//! `aura-cloud` owns the platform credential-store integration, because that is where
//! the API-key handling that needed it first lives. Neither crate depends on the other,
//! and this file is the reason they do not have to.
//!
//! **Why the two are kept apart.** `aura-cloud` is the one crate permitted to open a
//! socket - `scripts/check-banned.sh` enforces it - and `aura-people` is the one crate
//! permitted to hold a biometric template. Putting the two capabilities in one
//! dependency graph would mean the crate that can reach the network can also reach the
//! templates, for the sake of avoiding forty lines of adapter. The application layer
//! already depends on both, so it is the right place for the join.
//!
//! ## The secret, and where its entropy comes from
//!
//! A biometric key must be **random**, which makes it the one value in this workspace
//! that must not be derived from anything. `scripts/check-banned.sh` bans the standard
//! random-number helpers because a determinism risk in a pipeline stage is a
//! reproducibility bug; here non-determinism is the requirement.
//!
//! The entropy comes from two version-4 UUIDs, which `uuid` sources from the operating
//! system's cryptographic generator - `BCryptGenRandom` on Windows, `getrandom` on
//! Linux, `getentropy` on macOS. That is 256 bits from a CSPRNG, which is exactly what
//! is wanted, without adding a dependency whose name the banned-pattern check would
//! have to be taught to allow.
//!
//! The secret is stored in the credential store as 64 lower-case hex characters. Hex
//! rather than raw bytes because the platform stores hold text, and lower-case because a
//! round trip through a store that normalises case must not change the key.

use std::fmt;
use std::sync::Arc;

use aura_cloud::keys::{KeyStore, SecretKey};
use aura_core::AuraResult;
use aura_people::vault::{BiometricKeyStore, SECRET_LEN};
use uuid::Uuid;

/// Wraps a credential store so `aura-people` can use it without depending on
/// `aura-cloud`.
pub struct OsBiometricKeys {
    keys: Arc<dyn KeyStore>,
}

impl fmt::Debug for OsBiometricKeys {
    /// Never prints the wrapped store's contents.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OsBiometricKeys").finish_non_exhaustive()
    }
}

impl OsBiometricKeys {
    /// Wrap one credential store.
    #[must_use]
    pub fn new(keys: Arc<dyn KeyStore>) -> Self {
        Self { keys }
    }
}

impl BiometricKeyStore for OsBiometricKeys {
    fn secret(&self, account: &str) -> AuraResult<Option<[u8; SECRET_LEN]>> {
        let Some(stored) = self.keys.load(account)? else {
            return Ok(None);
        };
        // A secret of the wrong width is treated as absent rather than as an error. The
        // caller's next move is to create one, which is the right recovery for a
        // credential-store entry that some other tool wrote; returning an error would
        // leave the photographer with a People panel that will not open and no way to
        // fix it.
        Ok(from_hex(stored.expose()))
    }

    fn create(&self, account: &str) -> AuraResult<[u8; SECRET_LEN]> {
        if let Some(existing) = self.secret(account)? {
            return Ok(existing);
        }
        let mut secret = [0u8; SECRET_LEN];
        // Two v4 UUIDs: 256 bits from the operating system's cryptographic generator.
        // See this module's header for why it is not a random-number crate.
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        for (slot, byte) in secret
            .iter_mut()
            .zip(first.as_bytes().iter().chain(second.as_bytes().iter()))
        {
            *slot = *byte;
        }
        self.keys.store(account, &SecretKey::new(to_hex(&secret)))?;
        Ok(secret)
    }

    fn delete(&self, account: &str) -> AuraResult<()> {
        self.keys.delete(account)
    }
}

/// 32 bytes to 64 lower-case hex characters.
fn to_hex(bytes: &[u8; SECRET_LEN]) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(SECRET_LEN * 2);
    for byte in bytes {
        // Writing into a String cannot fail; the result is discarded deliberately.
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// 64 hex characters back to 32 bytes, or `None` for anything else.
fn from_hex(text: &str) -> Option<[u8; SECRET_LEN]> {
    let trimmed = text.trim();
    if trimmed.len() != SECRET_LEN * 2 {
        return None;
    }
    let mut out = [0u8; SECRET_LEN];
    for (index, slot) in out.iter_mut().enumerate() {
        let pair = trimmed.get(index * 2..index * 2 + 2)?;
        *slot = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(out)
}
