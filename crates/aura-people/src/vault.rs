//! The sealed envelope every biometric template lives inside.
//!
//! Section 6.5 requires that face embeddings and crops live in a per-project store
//! encrypted with a key in the operating system keychain, and that deleting a project
//! shreds them. This module is that store's crypto. It is small on purpose, and every
//! choice in it is written down, because a reviewer's first question about biometric
//! encryption should be answerable without reading the code.
//!
//! ## The construction
//!
//! Encrypt-then-MAC, with BLAKE3 providing both halves:
//!
//! * **Keystream** from BLAKE3's extendable output function, keyed with the project's
//!   encryption key and seeded with the record's nonce. BLAKE3's XOF is specified for
//!   exactly this use, and the resulting stream cipher is the ChaCha-family
//!   permutation the hash is built on.
//! * **Tag** is a keyed BLAKE3 hash over the header, the record identifier and the
//!   ciphertext, under a separate MAC key. Separate keys, so no length-extension or
//!   cross-protocol confusion between the two uses is possible.
//! * **Nonce** is a keyed BLAKE3 hash over the record kind, the record identifier and
//!   **the plaintext**. This makes the construction synthetic-IV: see below.
//!
//! ## Why not `chacha20poly1305`
//!
//! It would be one line in `Cargo.toml` and it is the conventional answer. Two
//! reasons it is not the answer here, recorded in
//! `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md` section 4:
//!
//! 1. It brings six crates into a workspace that currently has no cipher dependency
//!    at all, and every one of them becomes a supply-chain surface for the most
//!    sensitive data the product holds.
//! 2. BLAKE3 is already a workspace dependency, already used for content addressing
//!    and migration hashes, and its XOF-as-keystream and keyed-hash-as-MAC uses are
//!    both documented by its authors rather than invented here.
//!
//! What is *not* claimed: that this is better than an AEAD from a cipher crate. It is
//! a composition of two documented primitive uses, chosen to avoid adding a
//! dependency. [`ENVELOPE_VER`] exists so that moving to `chacha20poly1305` later is a
//! version bump and a re-seal rather than a rewrite - the header carries the version,
//! and [`Vault::open`] refuses a version it does not implement rather than guessing.
//!
//! ## Synthetic IV, and why the nonce depends on the plaintext
//!
//! A stream cipher that reuses a (key, nonce) pair across two different plaintexts
//! leaks their XOR. That is not a theoretical risk here, it is the *expected*
//! behaviour of this product: a face id is derived from the photograph and the box, so
//! re-scanning after a recogniser upgrade writes a **different template under the same
//! id**. A nonce derived from the id alone would reuse the keystream, and the
//! difference between two templates of the same face is exactly what an attacker would
//! want.
//!
//! So the nonce is derived from the plaintext as well. Two consequences, both wanted:
//!
//! * **Nonce reuse is impossible by construction.** Different plaintext, different
//!   nonce, always.
//! * **Sealing is deterministic.** The same template under the same id produces
//!   byte-identical output, which is what invariant 4 requires of every stage and what
//!   lets the phase gate assert that two runs agree.
//!
//! The cost is that identical plaintexts are visible as identical ciphertexts. For 512
//! half-precision floats that is a non-event: two faces with bit-identical templates
//! are the same crop of the same photograph.
//!
//! ## What is sealed, and what is not
//!
//! Sealed: recognition templates, identity centroids, sub-centroids, and the aligned
//! 112 px crops. Those are biometric identifiers.
//!
//! Not sealed: boxes, landmarks, pose angles, quality scores, counts and timestamps.
//! Five normalised points inside a box are not a biometric template - they cannot be
//! matched against a person - and they are needed in the clear by the debug panel, the
//! prominence scoring and every support investigation. Encrypting them would buy
//! nothing and would put the People panel behind the keychain.

use aura_core::{AuraResult, ProjectId};
use std::fmt;

use crate::errors;

/// The envelope format generation.
///
/// A change here means re-seal, never reinterpret. [`Vault::open`] refuses any other
/// version with `AURA-SEC-9002` rather than attempting to read it.
pub const ENVELOPE_VER: u8 = 1;

/// Magic bytes at the front of every envelope.
///
/// Eight bytes, so a hex dump of a stray blob is identifiable, and so a plaintext
/// template accidentally written to the column is rejected rather than decrypted into
/// noise.
pub const MAGIC: &[u8; 8] = b"AURAFV1\0";

/// Nonce length, in bytes.
///
/// Twenty-four, matching the `XChaCha` convention, and comfortably wide enough that the
/// synthetic derivation has no collision worth considering.
pub const NONCE_LEN: usize = 24;

/// Authentication tag length, in bytes.
pub const TAG_LEN: usize = 32;

/// Bytes an envelope adds to its payload.
pub const OVERHEAD: usize = MAGIC.len() + 4 + NONCE_LEN + TAG_LEN;

/// Salt length, in bytes.
pub const SALT_LEN: usize = 32;

/// Length of the master secret held in the credential store.
pub const SECRET_LEN: usize = 32;

/// What a sealed record holds. Bound into the nonce and the tag, so a template cannot
/// be moved onto a centroid's row or vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordKind {
    /// A 512-d recognition template for one face.
    Template,
    /// An identity's mean template.
    Centroid,
    /// An identity's concatenated sub-cluster means.
    SubCentroids,
    /// An aligned 112x112 sRGB crop.
    Crop,
}

impl RecordKind {
    /// The byte written into the header.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::Template => 1,
            Self::Centroid => 2,
            Self::SubCentroids => 3,
            Self::Crop => 4,
        }
    }

    /// Stable text for an error message.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Template => "template",
            Self::Centroid => "centroid",
            Self::SubCentroids => "sub_centroids",
            Self::Crop => "crop",
        }
    }
}

/// The one thing this crate needs from an operating system credential store.
///
/// A port rather than a direct dependency on `aura_cloud::keys`, and the reason is
/// architectural: `aura-cloud` is the crate that is allowed to open sockets, and
/// linking it into the crate that holds biometric templates would put the two most
/// sensitive capabilities in the product in one dependency graph for no reason. The
/// application layer, which already depends on both, supplies the adapter.
pub trait BiometricKeyStore: Send + Sync + fmt::Debug {
    /// The master secret for one account, or `None` when there is not one.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9001` when the store exists but could not be read. A missing entry is
    /// `Ok(None)`, not an error: a project that has never had a face scanned has no
    /// key, and that is a normal state.
    fn secret(&self, account: &str) -> AuraResult<Option<[u8; SECRET_LEN]>>;

    /// Create and store a master secret for one account, returning it.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9001` when the credential store refuses.
    fn create(&self, account: &str) -> AuraResult<[u8; SECRET_LEN]>;

    /// Forget an account's secret. Deleting one that is not there is success.
    ///
    /// This is the operation that makes erasure real: without the secret, the sealed
    /// rows are noise even if a backup of the catalog survives somewhere.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9001` when the credential store refuses.
    fn delete(&self, account: &str) -> AuraResult<()>;
}

/// The credential-store account name for one project.
///
/// Includes the project id so two weddings on one machine have unrelated keys, and
/// carries a version so a future key-derivation change is a new account rather than a
/// reinterpretation of an old one.
#[must_use]
pub fn account_for(project: &ProjectId) -> String {
    format!("aura.people.v1.{}", project.to_db())
}

/// A deterministic per-project salt.
///
/// Derived from the project id rather than random, and that is a deliberate trade. A
/// random salt would have to be stored, and it is stored - in `face_vault.salt` - but
/// deriving it means a catalog whose `face_vault` row was lost can still be opened
/// with the keychain secret alone. The salt's job here is domain separation between
/// two projects sharing one secret, not entropy: the entropy is all in the secret.
#[must_use]
pub fn derive_salt(project: &ProjectId) -> [u8; SALT_LEN] {
    let digest = blake3::derive_key("aura.people.salt.v1", project.to_db().as_bytes());
    let mut out = [0u8; SALT_LEN];
    out.copy_from_slice(&digest);
    out
}

/// The three keys one project's vault uses.
///
/// Three and not one. Using a single key for the keystream, the tag and the nonce
/// derivation would mean one primitive's output could be used as another's input,
/// which is the class of mistake that turns a sound construction into an unsound one.
/// BLAKE3's `derive_key` exists for exactly this separation.
#[derive(Clone)]
pub struct VaultKeys {
    encryption: [u8; 32],
    mac: [u8; 32],
    nonce: [u8; 32],
}

impl fmt::Debug for VaultKeys {
    /// Never prints key material. A `Debug` that leaked a key into a log or a panic
    /// message would defeat the whole module, and the derive would have done exactly
    /// that.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VaultKeys")
            .field("keys", &"<sealed>")
            .finish()
    }
}

impl VaultKeys {
    /// Derive the three keys from a master secret and a project salt.
    #[must_use]
    pub fn derive(secret: &[u8; SECRET_LEN], salt: &[u8; SALT_LEN]) -> Self {
        let mut material = Vec::with_capacity(SECRET_LEN + SALT_LEN);
        material.extend_from_slice(secret);
        material.extend_from_slice(salt);
        Self {
            encryption: blake3::derive_key("aura.people.enc.v1", &material),
            mac: blake3::derive_key("aura.people.mac.v1", &material),
            nonce: blake3::derive_key("aura.people.nonce.v1", &material),
        }
    }
}

/// One project's sealed store.
#[derive(Debug, Clone)]
pub struct Vault {
    keys: VaultKeys,
    project: ProjectId,
}

impl Vault {
    /// Build a vault from a master secret.
    #[must_use]
    pub fn new(project: ProjectId, secret: &[u8; SECRET_LEN], salt: &[u8; SALT_LEN]) -> Self {
        Self {
            keys: VaultKeys::derive(secret, salt),
            project,
        }
    }

    /// The project this vault belongs to.
    #[must_use]
    pub const fn project(&self) -> ProjectId {
        self.project
    }

    /// Seal one record.
    ///
    /// `id` is the record's own identifier - a face id for a template, an identity id
    /// for a centroid, a face id for a crop. It is bound into both the nonce and the
    /// tag, so a sealed template cannot be moved onto another face's row without the
    /// move being detected.
    #[must_use]
    pub fn seal(&self, kind: RecordKind, id: &str, plaintext: &[u8]) -> Vec<u8> {
        let nonce = self.derive_nonce(kind, id, plaintext);
        let header = header_bytes(kind, &nonce);

        let mut out = Vec::with_capacity(header.len() + plaintext.len() + TAG_LEN);
        out.extend_from_slice(&header);

        let mut ciphertext = plaintext.to_vec();
        self.apply_keystream(&nonce, &mut ciphertext);
        out.extend_from_slice(&ciphertext);

        let tag = self.tag(&header, id, &ciphertext);
        out.extend_from_slice(&tag);
        out
    }

    /// Open one record.
    ///
    /// Verifies the magic, the version, the kind, the tag and - because the nonce is
    /// synthetic - that the recovered plaintext regenerates the nonce it was sealed
    /// under. That last check is free and it means a forged envelope has to defeat both
    /// the MAC key and the nonce key.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9002` when anything about the envelope does not hold. The message
    /// says which, because "could not decrypt" is not a support answer.
    pub fn open(&self, kind: RecordKind, id: &str, envelope: &[u8]) -> AuraResult<Vec<u8>> {
        let header_len = MAGIC.len() + 4 + NONCE_LEN;
        if envelope.len() < header_len + TAG_LEN {
            return Err(errors::envelope_unusable(
                id,
                &format!(
                    "{} bytes is shorter than the {} byte minimum envelope",
                    envelope.len(),
                    header_len + TAG_LEN
                ),
            ));
        }
        let Some(header) = envelope.get(..header_len) else {
            return Err(errors::envelope_unusable(id, "header is truncated"));
        };
        if header.get(..MAGIC.len()) != Some(MAGIC.as_slice()) {
            return Err(errors::envelope_unusable(
                id,
                "this is not an AURA sealed record",
            ));
        }
        let version = header.get(MAGIC.len()).copied().unwrap_or(0);
        if version != ENVELOPE_VER {
            return Err(errors::envelope_unusable(
                id,
                &format!(
                    "envelope version {version} was written by a different build; this one \
                     implements {ENVELOPE_VER}"
                ),
            ));
        }
        let stored_kind = header.get(MAGIC.len() + 1).copied().unwrap_or(0);
        if stored_kind != kind.tag() {
            return Err(errors::envelope_unusable(
                id,
                &format!(
                    "this record is kind {stored_kind}, and a {} was asked for",
                    kind.as_str()
                ),
            ));
        }
        let Some(nonce_slice) = header.get(MAGIC.len() + 4..) else {
            return Err(errors::envelope_unusable(id, "nonce is truncated"));
        };
        let Ok(nonce) = <[u8; NONCE_LEN]>::try_from(nonce_slice) else {
            return Err(errors::envelope_unusable(id, "nonce is the wrong width"));
        };

        let body_end = envelope.len() - TAG_LEN;
        let Some(ciphertext) = envelope.get(header_len..body_end) else {
            return Err(errors::envelope_unusable(id, "body is truncated"));
        };
        let Some(stored_tag) = envelope.get(body_end..) else {
            return Err(errors::envelope_unusable(id, "tag is truncated"));
        };

        let expected = self.tag(header, id, ciphertext);
        if !constant_time_eq(&expected, stored_tag) {
            return Err(errors::envelope_unusable(
                id,
                "the authentication tag does not match; the record was altered, sealed for a \
                 different record, or sealed with a different key",
            ));
        }

        let mut plaintext = ciphertext.to_vec();
        self.apply_keystream(&nonce, &mut plaintext);

        // The synthetic-IV check. Cheap, and it turns a nonce mismatch - which a MAC
        // forgery would have to produce - into a second independent failure.
        let recomputed = self.derive_nonce(kind, id, &plaintext);
        if !constant_time_eq(&recomputed, &nonce) {
            return Err(errors::envelope_unusable(
                id,
                "the synthetic nonce does not match the recovered contents",
            ));
        }
        Ok(plaintext)
    }

    /// The synthetic nonce for one record.
    fn derive_nonce(&self, kind: RecordKind, id: &str, plaintext: &[u8]) -> [u8; NONCE_LEN] {
        let mut hasher = blake3::Hasher::new_keyed(&self.keys.nonce);
        hasher.update(&[kind.tag()]);
        hasher.update(&u32::try_from(id.len()).unwrap_or(u32::MAX).to_le_bytes());
        hasher.update(id.as_bytes());
        hasher.update(plaintext);
        let digest = hasher.finalize();
        let mut out = [0u8; NONCE_LEN];
        out.copy_from_slice(
            digest
                .as_bytes()
                .get(..NONCE_LEN)
                .unwrap_or(&[0u8; NONCE_LEN]),
        );
        out
    }

    /// XOR the buffer with the keystream for one nonce.
    fn apply_keystream(&self, nonce: &[u8; NONCE_LEN], buffer: &mut [u8]) {
        let mut hasher = blake3::Hasher::new_keyed(&self.keys.encryption);
        hasher.update(nonce);
        let mut reader = hasher.finalize_xof();
        // A block at a time rather than byte at a time: the XOF reader fills a slice,
        // and 64 bytes is its natural output block.
        let mut block = [0u8; 64];
        let mut offset = 0usize;
        while offset < buffer.len() {
            reader.fill(&mut block);
            let take = block.len().min(buffer.len() - offset);
            for index in 0..take {
                if let (Some(slot), Some(key)) = (buffer.get_mut(offset + index), block.get(index))
                {
                    *slot ^= *key;
                }
            }
            offset += take;
        }
    }

    /// The authentication tag over the header, the record id and the ciphertext.
    ///
    /// The id is length-prefixed. Without the prefix, `("ab", "c")` and `("a", "bc")`
    /// would produce the same tag input, which is the canonical MAC concatenation
    /// mistake.
    fn tag(&self, header: &[u8], id: &str, ciphertext: &[u8]) -> [u8; TAG_LEN] {
        let mut hasher = blake3::Hasher::new_keyed(&self.keys.mac);
        hasher.update(header);
        hasher.update(&u32::try_from(id.len()).unwrap_or(u32::MAX).to_le_bytes());
        hasher.update(id.as_bytes());
        hasher.update(
            &u64::try_from(ciphertext.len())
                .unwrap_or(u64::MAX)
                .to_le_bytes(),
        );
        hasher.update(ciphertext);
        let digest = hasher.finalize();
        let mut out = [0u8; TAG_LEN];
        out.copy_from_slice(digest.as_bytes().get(..TAG_LEN).unwrap_or(&[0u8; TAG_LEN]));
        out
    }
}

/// The fixed-width header: magic, version, kind, two reserved bytes, nonce.
fn header_bytes(kind: RecordKind, nonce: &[u8; NONCE_LEN]) -> Vec<u8> {
    let mut header = Vec::with_capacity(MAGIC.len() + 4 + NONCE_LEN);
    header.extend_from_slice(MAGIC);
    header.push(ENVELOPE_VER);
    header.push(kind.tag());
    // Reserved. Zero, and checked to be zero by nothing - a future field goes here and
    // an old build reading it would fail the tag check anyway, which is the correct
    // outcome.
    header.extend_from_slice(&[0u8, 0u8]);
    header.extend_from_slice(nonce);
    header
}

/// Compare two byte slices without an early exit.
///
/// A `==` on the tag would return as soon as it found a differing byte, and the timing
/// of that return is a measurement of how many leading bytes an attacker guessed
/// correctly. This folds every byte before deciding.
#[must_use]
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        difference |= a ^ b;
    }
    difference == 0
}

/// A key store that keeps secrets in memory only.
///
/// For tests and for the phase gate. It exists so that no test in this workspace ever
/// writes to a developer's real keychain, which is the same reason
/// `aura_cloud::keys::MemoryKeyStore` exists.
#[derive(Debug, Default)]
pub struct MemoryKeyStore {
    entries: parking_lot::Mutex<std::collections::BTreeMap<String, [u8; SECRET_LEN]>>,
}

impl MemoryKeyStore {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many accounts it holds. Used by the erasure test.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    /// True when it holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }
}

impl BiometricKeyStore for MemoryKeyStore {
    fn secret(&self, account: &str) -> AuraResult<Option<[u8; SECRET_LEN]>> {
        Ok(self.entries.lock().get(account).copied())
    }

    fn create(&self, account: &str) -> AuraResult<[u8; SECRET_LEN]> {
        let mut entries = self.entries.lock();
        if let Some(found) = entries.get(account) {
            return Ok(*found);
        }
        // Derived from the account name rather than random. A random secret would make
        // every test run produce different ciphertext, and the determinism tests in
        // this phase assert that two runs agree byte for byte. A real key store must
        // use the operating system's generator; see the adapter in `aura-app`.
        let secret = blake3::derive_key("aura.people.test-secret.v1", account.as_bytes());
        entries.insert(account.to_string(), secret);
        Ok(secret)
    }

    fn delete(&self, account: &str) -> AuraResult<()> {
        self.entries.lock().remove(account);
        Ok(())
    }
}
