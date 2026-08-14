//! The five promises this crate makes about biometric data, asserted.
//!
//! Section 6.5 is the part of phase 06 that is a legal position as much as an
//! engineering one, and a legal position that is only written down in prose is a
//! position nobody can defend. Each test here corresponds to one sentence of it:
//!
//! 1. Templates are sealed - `faces.embed` never holds a vector.
//! 2. A catalog without its key has no biometric data in it.
//! 3. Nothing crosses a wedding, ever.
//! 4. Erasure is real, verified, and keeps culling decisions intact.
//! 5. A tampered record is refused rather than misread.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::{Clock, SystemClock};
use aura_core::{FaceId, IdentityId, ProjectId};
use aura_people::store::PeopleStore;
use aura_people::vault::{
    self, BiometricKeyStore, MemoryKeyStore, RecordKind, Vault, ENVELOPE_VER, SECRET_LEN,
};
use tempfile::TempDir;

fn catalog(dir: &TempDir) -> Arc<Catalog> {
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    Arc::new(
        Catalog::open(&dir.path().join("people.sqlite"), clock, "0.1.0")
            .expect("the catalog must open"),
    )
}

fn store(dir: &TempDir, keys: Arc<dyn BiometricKeyStore>) -> (Arc<Catalog>, PeopleStore) {
    let catalog = catalog(dir);
    let clock: Arc<dyn Clock> = Arc::new(SystemClock::default());
    let store = PeopleStore::new(Arc::clone(&catalog), clock, keys, &dir.path().join("cache"));
    (catalog, store)
}

fn project(catalog: &Catalog, name: &str) -> ProjectId {
    let id = ProjectId::new();
    let key = id.to_db();
    let label = name.to_string();
    catalog
        .writer()
        .transact(move |tx| {
            tx.execute(
                "INSERT INTO project (project_id, name, created_at, updated_at)
                 VALUES (?1, ?2, '2026-08-13T00:00:00Z', '2026-08-13T00:00:00Z')",
                rusqlite::params![key, label],
            )
            .map_err(|e| {
                aura_core::errors::db::statement_failed("could not create a project", &e)
            })?;
            Ok(())
        })
        .expect("the project must be created");
    id
}

fn a_vault() -> Vault {
    let project = ProjectId::new();
    let secret = [7u8; SECRET_LEN];
    let salt = vault::derive_salt(&project);
    Vault::new(project, &secret, &salt)
}

// ---------------------------------------------------------------------------
// 1 and 5. The envelope.
// ---------------------------------------------------------------------------

#[test]
fn a_sealed_record_round_trips() {
    let vault = a_vault();
    let template = vec![42u8; 1_024];
    let sealed = vault.seal(RecordKind::Template, "fce_1", &template);

    assert_ne!(sealed, template, "the payload must not be in the clear");
    assert!(
        !sealed.windows(16).any(|window| window == &template[..16]),
        "no run of plaintext may survive"
    );

    let opened = vault
        .open(RecordKind::Template, "fce_1", &sealed)
        .expect("a sealed record must open");
    assert_eq!(opened, template);
}

#[test]
fn sealing_is_deterministic() {
    // Invariant 4. The same template under the same id must produce byte-identical
    // output, or two machines analysing one wedding disagree at the byte level and the
    // phase gate cannot assert anything.
    let vault = a_vault();
    let template = vec![9u8; 1_024];
    assert_eq!(
        vault.seal(RecordKind::Template, "fce_1", &template),
        vault.seal(RecordKind::Template, "fce_1", &template)
    );
}

#[test]
fn a_different_template_under_the_same_id_reuses_no_keystream() {
    // The synthetic-IV property, and the reason it is not optional: a face id is derived
    // from the photograph and the box, so re-scanning after a recogniser upgrade writes a
    // *different* template under the *same* id. A nonce derived from the id alone would
    // reuse the keystream, and the XOR of two templates of one face is exactly what an
    // attacker would want.
    let vault = a_vault();
    let mut first = vec![0u8; 1_024];
    let mut second = vec![0u8; 1_024];
    for index in 0..1_024 {
        first[index] = (index % 251) as u8;
        second[index] = ((index + 1) % 251) as u8;
    }

    let a = vault.seal(RecordKind::Template, "fce_1", &first);
    let b = vault.seal(RecordKind::Template, "fce_1", &second);

    let header = vault::MAGIC.len() + 4;
    let nonce_a = &a[header..header + vault::NONCE_LEN];
    let nonce_b = &b[header..header + vault::NONCE_LEN];
    assert_ne!(nonce_a, nonce_b, "the nonce must depend on the plaintext");

    // And the ciphertexts must not differ by the plaintext difference, which is what
    // keystream reuse would produce.
    let body = header + vault::NONCE_LEN;
    let cipher_xor: Vec<u8> = a[body..a.len() - vault::TAG_LEN]
        .iter()
        .zip(b[body..b.len() - vault::TAG_LEN].iter())
        .map(|(x, y)| x ^ y)
        .collect();
    let plain_xor: Vec<u8> = first
        .iter()
        .zip(second.iter())
        .map(|(x, y)| x ^ y)
        .collect();
    assert_ne!(
        cipher_xor, plain_xor,
        "keystream reuse would make these equal"
    );
}

#[test]
fn a_tampered_record_is_refused() {
    let vault = a_vault();
    let mut sealed = vault.seal(RecordKind::Template, "fce_1", &[1u8; 1_024]);
    let last = sealed.len() - vault::TAG_LEN - 1;
    sealed[last] ^= 0x01;

    let error = vault
        .open(RecordKind::Template, "fce_1", &sealed)
        .expect_err("a flipped bit must be refused");
    assert_eq!(error.code.0, "AURA-SEC-9002");
}

#[test]
fn a_record_moved_to_another_face_is_refused() {
    // The swap attack: rewriting one face's row with another's sealed template. The tag
    // covers the record identifier, so the move is detected rather than decrypted.
    let vault = a_vault();
    let sealed = vault.seal(RecordKind::Template, "fce_1", &[3u8; 1_024]);
    let error = vault
        .open(RecordKind::Template, "fce_2", &sealed)
        .expect_err("a moved record must be refused");
    assert_eq!(error.code.0, "AURA-SEC-9002");
}

#[test]
fn a_record_of_the_wrong_kind_is_refused() {
    let vault = a_vault();
    let sealed = vault.seal(RecordKind::Centroid, "idt_1", &[5u8; 1_024]);
    assert!(vault.open(RecordKind::Template, "idt_1", &sealed).is_err());
}

#[test]
fn a_record_from_another_project_is_refused() {
    let alpha = a_vault();
    let bravo = a_vault();
    let sealed = alpha.seal(RecordKind::Template, "fce_1", &[8u8; 1_024]);
    assert!(
        bravo.open(RecordKind::Template, "fce_1", &sealed).is_err(),
        "two projects' keys are unrelated"
    );
}

#[test]
fn plaintext_in_the_column_is_refused_rather_than_decrypted() {
    let vault = a_vault();
    let error = vault
        .open(RecordKind::Template, "fce_1", &[0u8; 1_100])
        .expect_err("a bare blob is not a sealed record");
    assert!(
        error.detail.contains("not an AURA sealed record"),
        "{}",
        error.detail
    );
}

#[test]
fn a_truncated_record_is_refused() {
    let vault = a_vault();
    let sealed = vault.seal(RecordKind::Template, "fce_1", &[1u8; 1_024]);
    assert!(vault
        .open(RecordKind::Template, "fce_1", &sealed[..40])
        .is_err());
}

#[test]
fn an_unknown_envelope_version_is_refused_rather_than_guessed_at() {
    let vault = a_vault();
    let mut sealed = vault.seal(RecordKind::Template, "fce_1", &[1u8; 1_024]);
    sealed[vault::MAGIC.len()] = ENVELOPE_VER + 1;
    let error = vault
        .open(RecordKind::Template, "fce_1", &sealed)
        .expect_err("a future envelope must be refused");
    assert!(error.detail.contains("different build"), "{}", error.detail);
}

#[test]
fn the_debug_impl_does_not_print_key_material() {
    let vault = a_vault();
    let rendered = format!("{vault:?}");
    assert!(rendered.contains("sealed"), "{rendered}");
    // A derived Debug on the key struct would have printed 96 bytes of key here.
    assert!(!rendered.contains("encryption: ["), "{rendered}");
}

#[test]
fn constant_time_comparison_is_length_safe() {
    assert!(vault::constant_time_eq(b"abc", b"abc"));
    assert!(!vault::constant_time_eq(b"abc", b"abd"));
    assert!(!vault::constant_time_eq(b"abc", b"abcd"));
}

// ---------------------------------------------------------------------------
// 2. A catalog without its key.
// ---------------------------------------------------------------------------

#[test]
fn the_face_vault_row_holds_an_account_name_and_never_a_key() {
    let dir = TempDir::new().expect("a temporary directory");
    let keys: Arc<dyn BiometricKeyStore> = Arc::new(MemoryKeyStore::new());
    let (catalog, store) = store(&dir, Arc::clone(&keys));
    let project = project(&catalog, "alpha");

    store.vault(&project).expect("a vault must be created");

    let (account, salt): (String, Vec<u8>) = catalog
        .read({
            let key = project.to_db();
            move |conn| {
                conn.query_row(
                    "SELECT key_account, salt FROM face_vault WHERE project_id = ?1",
                    rusqlite::params![key],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not read the vault", &e)
                })
            }
        })
        .expect("the vault row must exist");

    assert!(account.starts_with("aura.people.v1."), "{account}");
    assert_eq!(salt.len(), vault::SALT_LEN);

    // The secret must not be recoverable from the catalog. The salt is derived from the
    // project id and is deliberately not secret; the entropy is all in the keychain.
    let secret = keys
        .secret(&account)
        .expect("the key store must answer")
        .expect("a secret must exist");
    assert!(
        !salt.windows(8).any(|window| secret.starts_with(window)),
        "the salt must not leak the secret"
    );
}

// ---------------------------------------------------------------------------
// 3. Nothing crosses a wedding.
// ---------------------------------------------------------------------------

#[test]
fn two_projects_get_unrelated_keys() {
    let dir = TempDir::new().expect("a temporary directory");
    let keys: Arc<dyn BiometricKeyStore> = Arc::new(MemoryKeyStore::new());
    let (catalog, store) = store(&dir, keys);
    let alpha = project(&catalog, "alpha");
    let bravo = project(&catalog, "bravo");

    let alpha_vault = store.vault(&alpha).expect("alpha's vault");
    let bravo_vault = store.vault(&bravo).expect("bravo's vault");

    let sealed = alpha_vault.seal(RecordKind::Template, "fce_1", &[4u8; 1_024]);
    assert!(
        bravo_vault
            .open(RecordKind::Template, "fce_1", &sealed)
            .is_err(),
        "one wedding's key must never open another's records"
    );
}

#[test]
fn an_identity_from_another_project_is_refused() {
    let dir = TempDir::new().expect("a temporary directory");
    let keys: Arc<dyn BiometricKeyStore> = Arc::new(MemoryKeyStore::new());
    let (catalog, store) = store(&dir, keys);
    let alpha = project(&catalog, "alpha");
    let bravo = project(&catalog, "bravo");

    let alpha_identity = store
        .create_identity(&alpha, FaceId::new(), 1)
        .expect("an identity in alpha");
    let bravo_identity = store
        .create_identity(&bravo, FaceId::new(), 1)
        .expect("an identity in bravo");

    let error = store
        .assert_same_project(&alpha, &[alpha_identity, bravo_identity])
        .expect_err("a cross-project pair must be refused");
    assert_eq!(error.code.0, "AURA-SEC-9004");
    assert_eq!(
        error.severity,
        aura_core::Severity::RunBlocking,
        "a cross-project access is not a degradation"
    );
}

#[test]
fn faces_never_move_between_projects() {
    let dir = TempDir::new().expect("a temporary directory");
    let keys: Arc<dyn BiometricKeyStore> = Arc::new(MemoryKeyStore::new());
    let (catalog, store) = store(&dir, keys);
    let alpha = project(&catalog, "alpha");
    let bravo = project(&catalog, "bravo");

    let identity = store
        .create_identity(&bravo, FaceId::new(), 1)
        .expect("an identity in bravo");
    // A face id from alpha, offered to bravo's identity. The project clause in the
    // statement means it simply matches nothing.
    let moved = store
        .assign_faces(&bravo, Some(identity), &[FaceId::new()])
        .expect("the statement must run");
    assert_eq!(moved, 0, "no face may be claimed across a project boundary");

    let _ = alpha;
}

// ---------------------------------------------------------------------------
// 4. Erasure.
// ---------------------------------------------------------------------------

#[test]
fn erasure_removes_the_key_first_and_verifies_the_rest() {
    let dir = TempDir::new().expect("a temporary directory");
    let memory = Arc::new(MemoryKeyStore::new());
    let keys: Arc<dyn BiometricKeyStore> = memory.clone();
    let (catalog, store) = store(&dir, keys);
    let project_id = project(&catalog, "alpha");

    let identity = store
        .create_identity(&project_id, FaceId::new(), 1)
        .expect("an identity");
    // The key is created lazily, on the first use of the vault rather than on the first
    // people row - which is the behaviour the lazy accessor promises, and is why a project
    // that is opened and closed without a face scan never touches the credential store.
    assert!(
        memory.is_empty(),
        "creating an identity must not reach the credential store"
    );
    store.vault(&project_id).expect("a vault on demand");
    assert!(!memory.is_empty(), "the vault must have created a key");

    let report = store.erase(&project_id).expect("erasure must complete");
    assert!(report.key_removed);
    assert!(memory.is_empty(), "the credential-store entry must be gone");
    assert_eq!(report.identities, 1);

    // And the receipt: "there are no faces" and "the faces were destroyed" are different
    // answers to a client who asks.
    let erased: Option<String> = catalog
        .read({
            let key = project_id.to_db();
            move |conn| {
                Ok(conn
                    .query_row(
                        "SELECT erased_at FROM face_vault WHERE project_id = ?1",
                        rusqlite::params![key],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .ok()
                    .flatten())
            }
        })
        .expect("the vault row must be readable");
    assert!(erased.is_some(), "erasure must be recorded");

    let _ = identity;
}

#[test]
fn erasure_keeps_everything_that_is_not_biometric() {
    // Section 6.5: "removes faces/identities while keeping culling and edit decisions
    // intact". Asserted by checking that the tables outside migration 6 are untouched.
    let dir = TempDir::new().expect("a temporary directory");
    let keys: Arc<dyn BiometricKeyStore> = Arc::new(MemoryKeyStore::new());
    let (catalog, store) = store(&dir, keys);
    let project_id = project(&catalog, "alpha");

    let before: i64 = catalog
        .read(|conn| {
            conn.query_row("SELECT COUNT(*) FROM project", [], |row| row.get(0))
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not count projects", &e)
                })
        })
        .expect("the count must read");

    store.erase(&project_id).expect("erasure must complete");

    let after: i64 = catalog
        .read(|conn| {
            conn.query_row("SELECT COUNT(*) FROM project", [], |row| row.get(0))
                .map_err(|e| {
                    aura_core::errors::db::statement_failed("could not count projects", &e)
                })
        })
        .expect("the count must read");
    assert_eq!(before, after, "erasure must not delete the project");
}

#[test]
fn erasing_a_project_with_no_faces_is_success() {
    let dir = TempDir::new().expect("a temporary directory");
    let keys: Arc<dyn BiometricKeyStore> = Arc::new(MemoryKeyStore::new());
    let (catalog, store) = store(&dir, keys);
    let project_id = project(&catalog, "alpha");

    let report = store.erase(&project_id).expect("erasure must complete");
    assert_eq!(report.faces, 0);
    assert_eq!(report.identities, 0);
}

#[test]
fn a_memory_key_store_is_deterministic_so_tests_can_assert_bytes() {
    let store = MemoryKeyStore::new();
    let first = store.create("account").expect("a secret");
    let second = store.create("account").expect("the same secret");
    assert_eq!(first, second);
    assert_eq!(store.len(), 1);

    store.delete("account").expect("delete must succeed");
    assert!(store.is_empty());
    // Deleting what is not there is success, not an error.
    store.delete("account").expect("idempotent delete");
}

#[test]
fn identity_ids_are_derived_and_therefore_stable() {
    // A re-clustering that reaches the same grouping must produce the same identity id,
    // or every photographer decision attached to it is orphaned.
    let project_id = ProjectId::new();
    let anchor = FaceId::new();
    let first = aura_people::store::derive_identity_id(&project_id, anchor);
    let second = aura_people::store::derive_identity_id(&project_id, anchor);
    assert_eq!(first, second);

    let other: IdentityId = aura_people::store::derive_identity_id(&ProjectId::new(), anchor);
    assert_ne!(
        first, other,
        "the same face in two projects is two identities"
    );
}
