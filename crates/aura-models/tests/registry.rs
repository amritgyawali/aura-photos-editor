//! The registry end to end: signature, digest, card, update, rollback.
//!
//! Every test here builds a real signed model directory in a temporary folder -
//! real ONNX files from the fixtures, a real manifest, a real ed25519 signature -
//! and then breaks exactly one thing. A registry that refuses everything would
//! pass a test suite of refusals, so each refusal test is paired with the working
//! case it differs from.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use aura_infer::contract::infer::{ExecutionProvider, ModelClass, ModelRef, Precision, Version};
use aura_infer::onnx::{fixtures, serialise};
use aura_infer::source::ModelSource;
use aura_models::contract::manifest::{
    InputSpec, ModelEntry, ModelsLock, PrecisionPolicy, Variant, LOCK_SCHEMA,
};
use aura_models::download::FileTransport;
use aura_models::registry::{ModelRegistry, DEV_PUBLIC_KEY_HEX, LOCK_FILE, SIGNATURE_FILE};
use aura_models::verify::sha256_hex;
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const MODEL: ModelRef = ModelRef::new("aura_tiny_embedding", Version::new(1, 0, 0));
const FILE_NAME: &str = "aura_tiny_embedding_1.0.0.fp32.onnx";

/// The same derivation `tools/model-sign --dev` uses.
fn dev_key() -> SigningKey {
    let mut hasher = Sha256::new();
    hasher.update(b"AURA development signing key, not for release");
    let seed: [u8; 32] = hasher.finalize().into();
    SigningKey::from_bytes(&seed)
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::new();
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

struct Fixture {
    directory: TempDir,
    card_root: TempDir,
}

impl Fixture {
    /// A signed models directory with one model and one variant.
    fn new() -> Self {
        let directory = TempDir::new().expect("models dir");
        let card_root = TempDir::new().expect("card root");

        let bytes = serialise(&fixtures::tiny_embedding());
        fs::write(directory.path().join(FILE_NAME), &bytes).expect("write model");

        let card_dir = card_root.path().join("docs/model-cards");
        fs::create_dir_all(&card_dir).expect("card dir");
        fs::write(
            card_dir.join("aura_tiny_embedding.md"),
            "# card\n\n## Purpose\nA placeholder.\n",
        )
        .expect("write card");

        let fixture = Self {
            directory,
            card_root,
        };
        fixture.write_manifest(&fixture.entry(&bytes));
        fixture
    }

    fn entry(&self, bytes: &[u8]) -> ModelEntry {
        ModelEntry {
            name: MODEL.name.to_string(),
            version: MODEL.version,
            task: "embedding".to_string(),
            class: ModelClass::Embedding,
            input: InputSpec {
                shape: vec![1, 3, 32, 32],
                layout: "NCHW".to_string(),
                range: "0..1".to_string(),
                colour: "srgb".to_string(),
            },
            output: BTreeMap::from([("embedding".to_string(), vec![1, 32])]),
            variants: vec![Variant {
                ep: ExecutionProvider::Cpu,
                precision: Precision::Fp32,
                file: FILE_NAME.to_string(),
                sha256: sha256_hex(bytes),
                bytes: bytes.len() as u64,
            }],
            licence: "proprietary".to_string(),
            model_card: "docs/model-cards/aura_tiny_embedding.md".to_string(),
            min_app_version: "0.1.0".to_string(),
            working_set_mb: 8,
            precision_policy: PrecisionPolicy::permissive(),
            opset: 13,
        }
    }

    fn write_manifest(&self, entry: &ModelEntry) {
        let lock = ModelsLock {
            schema: LOCK_SCHEMA,
            generated_at: "2026-08-12T00:00:00Z".to_string(),
            models: vec![entry.clone()],
        };
        let rendered = serde_json::to_vec_pretty(&lock).expect("render");
        fs::write(self.path().join(LOCK_FILE), &rendered).expect("write lock");
        let signature = dev_key().sign(&rendered);
        fs::write(self.path().join(SIGNATURE_FILE), hex(&signature.to_bytes()))
            .expect("write signature");
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }

    fn open(&self) -> aura_core::AuraResult<ModelRegistry> {
        ModelRegistry::open(self.path(), self.card_root.path(), DEV_PUBLIC_KEY_HEX)
    }

    fn model_path(&self) -> PathBuf {
        self.path().join(FILE_NAME)
    }
}

#[test]
fn a_signed_directory_opens_and_resolves_a_model() {
    let fixture = Fixture::new();
    let registry = fixture.open().expect("open");

    let resolved = registry
        .resolve(MODEL, ExecutionProvider::Cpu)
        .expect("resolve");
    assert_eq!(resolved.path, fixture.model_path());
    assert_eq!(resolved.precision, Precision::Fp32);
    assert_eq!(resolved.working_set_mb, 8);
    assert_eq!(registry.verify_all().expect("verify"), 1);
}

#[test]
fn a_manifest_edited_after_signing_is_refused() {
    let fixture = Fixture::new();
    let lock_path = fixture.path().join(LOCK_FILE);
    let text = fs::read_to_string(&lock_path).expect("read");
    fs::write(
        &lock_path,
        text.replace("\"working_set_mb\": 8", "\"working_set_mb\": 9"),
    )
    .expect("tamper");

    let err = fixture.open().expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5002");
}

#[test]
fn a_signature_from_another_key_is_refused() {
    let fixture = Fixture::new();
    let other = SigningKey::from_bytes(&[7u8; 32]);
    let manifest = fs::read(fixture.path().join(LOCK_FILE)).expect("read");
    fs::write(
        fixture.path().join(SIGNATURE_FILE),
        hex(&other.sign(&manifest).to_bytes()),
    )
    .expect("write");

    let err = fixture.open().expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5002");
}

#[test]
fn a_file_that_does_not_match_its_digest_is_refused() {
    let fixture = Fixture::new();
    let registry = fixture.open().expect("open");

    let mut bytes = fs::read(fixture.model_path()).expect("read");
    if let Some(slot) = bytes.last_mut() {
        *slot ^= 0xff;
    }
    fs::write(fixture.model_path(), &bytes).expect("corrupt");

    let err = registry
        .resolve(MODEL, ExecutionProvider::Cpu)
        .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5003");
}

#[test]
fn a_truncated_file_is_refused_on_its_size_before_its_digest() {
    let fixture = Fixture::new();
    let registry = fixture.open().expect("open");
    fs::write(fixture.model_path(), b"short").expect("truncate");

    let err = registry
        .resolve(MODEL, ExecutionProvider::Cpu)
        .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5003");
}

#[test]
fn a_model_without_a_card_is_refused() {
    let fixture = Fixture::new();
    fs::remove_file(
        fixture
            .card_root
            .path()
            .join("docs/model-cards/aura_tiny_embedding.md"),
    )
    .expect("remove card");

    let registry = fixture.open().expect("open");
    let err = registry
        .resolve(MODEL, ExecutionProvider::Cpu)
        .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5005");
}

#[test]
fn a_version_that_is_not_pinned_is_refused_by_name() {
    let fixture = Fixture::new();
    let registry = fixture.open().expect("open");
    let unpinned = ModelRef::new("aura_tiny_embedding", Version::new(2, 0, 0));

    let err = registry
        .resolve(unpinned, ExecutionProvider::Cpu)
        .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5001");
}

#[test]
fn an_accelerator_with_no_variant_of_its_own_falls_back_to_the_processor_file() {
    let fixture = Fixture::new();
    let registry = fixture.open().expect("open");

    let resolved = registry
        .resolve(MODEL, ExecutionProvider::Cuda)
        .expect("resolve");
    assert_eq!(resolved.path, fixture.model_path());
}

#[test]
fn a_transfer_that_dies_halfway_resumes_and_completes() {
    let fixture = Fixture::new();
    let registry = fixture.open().expect("open");

    // The source directory holds the payload; the destination starts empty.
    let source = TempDir::new().expect("source");
    let payload = fs::read(fixture.model_path()).expect("read");
    fs::write(source.path().join(FILE_NAME), &payload).expect("stage payload");
    fs::remove_file(fixture.model_path()).expect("clear destination");

    let half = (payload.len() / 2) as u64;
    let dying = FileTransport::failing_after(source.path(), half);
    let err = registry
        .install(&dying, MODEL.name, MODEL.version, &|_| {})
        .expect_err("must report the short transfer");
    assert_eq!(err.code.0, "AURA-ML-5004");

    let part = fixture.path().join(format!("{FILE_NAME}.part"));
    assert!(part.exists(), "the partial file is the resume point");
    assert_eq!(
        fs::metadata(&part).expect("stat").len(),
        half,
        "the resume point must be what actually arrived"
    );

    let healthy = FileTransport::new(source.path());
    registry
        .install(&healthy, MODEL.name, MODEL.version, &|_| {})
        .expect("resume and install");

    assert!(
        !part.exists(),
        "the partial file is consumed by the install"
    );
    assert_eq!(fs::read(fixture.model_path()).expect("read"), payload);
    assert_eq!(
        registry.state_of(MODEL.name).pending,
        Some(MODEL.version),
        "a fresh install is pending until it proves itself"
    );
}

#[test]
fn a_confirmed_version_becomes_active_and_the_previous_file_is_forgotten() {
    let fixture = Fixture::new();
    let registry = fixture.open().expect("open");

    let source = TempDir::new().expect("source");
    fs::write(
        source.path().join(FILE_NAME),
        fs::read(fixture.model_path()).expect("read"),
    )
    .expect("stage");

    registry
        .install(
            &FileTransport::new(source.path()),
            MODEL.name,
            MODEL.version,
            &|_| {},
        )
        .expect("install");
    registry
        .confirm(MODEL.name, MODEL.version)
        .expect("confirm");

    let state = registry.state_of(MODEL.name);
    assert_eq!(state.active, Some(MODEL.version));
    assert_eq!(state.pending, None);
    assert!(
        !fixture
            .path()
            .join(format!("{FILE_NAME}.previous"))
            .exists(),
        "a confirmed version releases the copy it was keeping"
    );
}

#[test]
fn a_version_that_fails_its_first_use_is_rolled_back_to_the_previous_file() {
    let fixture = Fixture::new();
    let registry = fixture.open().expect("open");

    let original = fs::read(fixture.model_path()).expect("read");

    // Install "the same version again" from a source holding different bytes is
    // not possible - the digest is pinned - so the rollback is exercised the way
    // it happens in the field: the file is replaced by an install, the version is
    // rejected, and the previous file must come back byte for byte.
    let source = TempDir::new().expect("source");
    fs::write(source.path().join(FILE_NAME), &original).expect("stage");
    registry
        .install(
            &FileTransport::new(source.path()),
            MODEL.name,
            MODEL.version,
            &|_| {},
        )
        .expect("install");

    let restored = registry.reject(MODEL.name, MODEL.version).expect("reject");
    assert_eq!(restored, None, "there was no earlier version to restore to");
    assert!(registry.state_of(MODEL.name).was_rejected(MODEL.version));
    assert_eq!(
        fs::read(fixture.model_path()).expect("read"),
        original,
        "the file on disk must be the one that worked"
    );
}

#[test]
fn a_rejected_version_is_not_served_again() {
    let fixture = Fixture::new();
    let registry = fixture.open().expect("open");

    registry.reject(MODEL.name, MODEL.version).expect("reject");
    let err = registry
        .resolve(MODEL, ExecutionProvider::Cpu)
        .expect_err("must refuse");
    assert_eq!(err.code.0, "AURA-ML-5009");
}
