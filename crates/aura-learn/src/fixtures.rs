//! Authored corrections, so the loop can be driven without a photographer's archive.
//!
//! ## What a fixture here proves and what it does not
//!
//! It proves the attribution, the split, the trimming, the two bounds, the held-out measurement,
//! the adoption path and the rollback. It says **nothing** about whether a real photographer's
//! corrections would produce a profile they recognise, because there is no consented archive in
//! this repository - which is phase 17's condition C1 reaching this phase.
//!
//! `FITTED_ON_REAL_CORRECTIONS` is false and is on the wire for that reason. Exit condition C4.

use aura_core::contract::ids::{DecisionId, IdentityId, ProfileId, ProjectId};
use aura_core::contract::learn::{Consent, Correction, CorrectionContext, Learnable};
use aura_core::contract::ledger::DecisionKind;
use aura_core::contract::scene::ImageId;
use aura_core::contract::scene::SceneId;

/// A photographer whose corrections say one consistent thing.
#[derive(Debug, Clone)]
pub struct Archive {
    /// The corrections, in the order they were made.
    pub corrections: Vec<(Correction, CorrectionContext)>,
    /// The weddings they came from.
    pub projects: Vec<ProjectId>,
}

/// Build an archive: `weddings` projects, `per_wedding` corrections each, all saying the same
/// thing about one value.
///
/// Deterministic in the seed, including the identifiers. Phase 29 found the trap the hard way:
/// `ImageId::new()` is a v7 UUID, random in its low bits, so a fixture that minted one looked
/// deterministic for a whole phase while every tie-break downstream fell back on the id. Here the
/// ids are derived from the seed.
#[must_use]
pub fn archive(
    learnable: Learnable,
    magnitude: f32,
    weddings: usize,
    per_wedding: usize,
    seed: u64,
) -> Archive {
    let mut corrections = Vec::with_capacity(weddings * per_wedding);
    let mut projects = Vec::with_capacity(weddings);
    for w in 0..weddings {
        let project = derived_project(seed, w);
        projects.push(project);
        for c in 0..per_wedding {
            let decision = derived_decision(seed, w, c);
            let image = derived_image(seed, w, c);
            corrections.push((
                Correction {
                    decision_id: decision,
                    kind: learnable.decision_kind(),
                    before_json: format!("{{\"{}\":0.0}}", learnable.as_str()),
                    after_json: format!("{{\"{}\":{magnitude}}}", learnable.as_str()),
                    scene: SceneId::Unknown,
                    identity: None,
                    magnitude,
                    created_at: 1_760_000_000_000 + (w * per_wedding + c) as i64,
                },
                CorrectionContext {
                    project,
                    image,
                    learnable,
                    held_out: false,
                    consumed_by: None,
                },
            ));
        }
    }
    Archive {
        corrections,
        projects,
    }
}

/// One correction that disagrees with the rest, to exercise the trim.
#[must_use]
pub fn outlier(
    learnable: Learnable,
    magnitude: f32,
    project: ProjectId,
    seed: u64,
) -> (Correction, CorrectionContext) {
    let decision = derived_decision(seed, 999, 999);
    (
        Correction {
            decision_id: decision,
            kind: learnable.decision_kind(),
            before_json: "{}".to_owned(),
            after_json: "{}".to_owned(),
            scene: SceneId::Unknown,
            // **No identity**, deliberately. `attribute` puts a correction about somebody close
            // to the couple in its own bucket, so an outlier that carried one would land
            // somewhere else and the trim would never see it - which is a fixture that proves
            // nothing while looking like it proves the trim.
            identity: None,
            magnitude,
            created_at: 1_760_000_999_999,
        },
        CorrectionContext {
            project,
            image: derived_image(seed, 999, 999),
            learnable,
            held_out: false,
            consumed_by: None,
        },
    )
}

/// A consent that permits local learning and nothing else.
///
/// Which is the shape a photographer who wants a personal profile and no telemetry would set, and
/// is what almost every test here needs.
#[must_use]
pub fn learning_only(project: ProjectId) -> Consent {
    Consent {
        project,
        local_learning: true,
        dataset_contribution: false,
        crash_reports: false,
        telemetry: false,
        decided_at: 1_760_000_000_000,
        app_version: "0.1.0".to_owned(),
    }
}

/// A profile id derived from a seed, so a test can name the same profile twice.
#[must_use]
pub fn derived_profile(seed: u64) -> ProfileId {
    ProfileId::from_uuid(uuid_from(b"profile", seed, 0, 0))
}

/// A project id derived from a seed and an index.
#[must_use]
pub fn derived_project(seed: u64, index: usize) -> ProjectId {
    ProjectId::from_uuid(uuid_from(b"project", seed, index, 0))
}

/// A decision id derived from a seed and two indices.
#[must_use]
pub fn derived_decision(seed: u64, wedding: usize, index: usize) -> DecisionId {
    DecisionId::from_uuid(uuid_from(b"decision", seed, wedding, index))
}

/// An image id derived from a seed and two indices.
#[must_use]
pub fn derived_image(seed: u64, wedding: usize, index: usize) -> ImageId {
    ImageId::from_uuid(uuid_from(b"image", seed, wedding, index))
}

/// An identity id derived from a seed.
#[must_use]
pub fn derived_identity(seed: u64) -> IdentityId {
    IdentityId::from_uuid(uuid_from(b"identity", seed, 0, 0))
}

/// A deterministic UUID. **Not `now_v7`** - see the note on [`archive`].
fn uuid_from(domain: &[u8], seed: u64, a: usize, b: usize) -> uuid::Uuid {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(&seed.to_be_bytes());
    hasher.update(&(a as u64).to_be_bytes());
    hasher.update(&(b as u64).to_be_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    uuid::Uuid::from_bytes(bytes)
}

/// Which decision kinds the fixtures produce, for a test that wants to assert the mapping.
#[must_use]
pub fn kinds() -> Vec<DecisionKind> {
    let mut out: Vec<DecisionKind> = Learnable::ALL.iter().map(|l| l.decision_kind()).collect();
    out.sort_by_key(|k| k.as_str());
    out.dedup();
    out
}
