//! FROZEN CONTRACT. Default is all-false. There is no code path that sets a
//! consent flag other than an explicit photographer action.

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::contract::ids::ProjectId;

/// Per-project egress permissions. Created denied when the project is created.
///
/// The five flags are deliberately separate booleans rather than a bitset or an
/// enum: each one is a distinct promise to a couple, they are granted and revoked
/// independently, and a support engineer reading the row must see which is which.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConsent {
    /// The project these permissions belong to.
    pub project_id: ProjectId,
    /// Filenames, EXIF, counts may go to a cloud model.
    pub cloud_metadata: bool,
    /// Downscaled derived imagery may go to a cloud model.
    pub cloud_derived_imagery: bool,
    /// Full-resolution client imagery may leave the machine. Rarely granted.
    pub cloud_full_imagery: bool,
    /// Anonymous product telemetry.
    pub telemetry: bool,
    /// This project's decisions may improve shared models.
    pub improve_models: bool,
    /// When the photographer granted the permissions.
    pub granted_at: Option<OffsetDateTime>,
    /// Who granted them, as typed by the photographer.
    pub granted_by: Option<String>,
    /// Optional expiry after which the grant stops being active.
    pub expires_at: Option<OffsetDateTime>,
    /// When the grant was withdrawn.
    pub revoked_at: Option<OffsetDateTime>,
}

impl ProjectConsent {
    /// The only constructor. Everything denied.
    #[must_use]
    pub fn denied(project_id: ProjectId) -> Self {
        Self {
            project_id,
            cloud_metadata: false,
            cloud_derived_imagery: false,
            cloud_full_imagery: false,
            telemetry: false,
            improve_models: false,
            granted_at: None,
            granted_by: None,
            expires_at: None,
            revoked_at: None,
        }
    }

    /// True while the grant has neither been revoked nor expired.
    #[must_use]
    pub fn is_active(&self, now: OffsetDateTime) -> bool {
        self.revoked_at.is_none() && self.expires_at.is_none_or(|e| e > now)
    }
}

/// Sensitivity class of a payload, used to decide whether egress is permitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataClass {
    /// Public or non-sensitive product data.
    C0Public,
    /// Operational: counts, timings, error codes.
    C1Operational,
    /// Identifying: filenames, imagery, EXIF GPS.
    C2Identifying,
    /// Biometric: face embeddings, face crops.
    C3Biometric,
}

/// The answer a [`ConsentGate`] gives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permit {
    /// Egress is permitted for this project and class.
    Allowed,
    /// Egress is refused, with a stable reason for the log and the UI.
    Denied {
        /// Why the request was refused.
        reason: &'static str,
    },
}

/// Phase 01 registers the gate; PHASE-04 is the first caller.
pub trait ConsentGate: Send + Sync + std::fmt::Debug {
    /// Decide whether `class` data for `project` may leave the machine.
    fn may_egress(&self, project: ProjectId, class: DataClass) -> Permit;
}
