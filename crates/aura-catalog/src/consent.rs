//! The consent gate, reading the table phase 01 created for it.
//!
//! `project_consent` has existed since migration 1 and nothing has read it until
//! now - the frozen contract's doc comment says "phase 01 registers the gate;
//! PHASE-04 is the first caller", and this is that caller.
//!
//! Two decisions here are load-bearing.
//!
//! **A read failure denies.** If the row cannot be read - a locked database, a
//! project id that does not exist, a corrupt file - the answer is `Denied`, not
//! an error and certainly not `Allowed`. A gate that opened when it could not
//! check is not a gate, and the caller's fallback path means a wrong denial costs
//! a slightly worse decision while a wrong permission costs a client's privacy.
//!
//! **Every change is appended to `consent_ledger`.** The ledger is append-only by
//! schema and by policy: "when did we get permission to send this couple's
//! photographs anywhere" is a question that may be asked years later, by someone
//! who is not an engineer.

use std::sync::Arc;

use aura_core::clock::Clock;
use aura_core::contract::consent::{ConsentGate, DataClass, Permit};
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, ProjectId};
use rusqlite::params;

use crate::Catalog;

/// The consent gate over one catalog.
#[derive(Debug)]
pub struct CatalogConsent {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl CatalogConsent {
    /// A gate reading one catalog's `project_consent` table.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// Read the three cloud flags and the validity window for one project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read. Callers that must not fail
    /// use [`ConsentGate::may_egress`], which turns this into a denial.
    pub fn flags(&self, project: &ProjectId) -> AuraResult<CloudConsent> {
        let id = project.to_string();
        self.catalog.read(move |conn| {
            conn.query_row(
                "SELECT cloud_metadata, cloud_derived_imagery, cloud_full_imagery,
                        expires_at, revoked_at
                 FROM project_consent WHERE project_id = ?1",
                params![id],
                |row| {
                    Ok(CloudConsent {
                        metadata: row.get::<_, i64>(0)? != 0,
                        derived_imagery: row.get::<_, i64>(1)? != 0,
                        full_imagery: row.get::<_, i64>(2)? != 0,
                        expires_at: row.get(3)?,
                        revoked_at: row.get(4)?,
                    })
                },
            )
            .map_err(|err| statement_failed("consent read failed", &err))
        })
    }

    /// Grant or withdraw the two cloud permissions, and write the ledger.
    ///
    /// `cloud_full_imagery` is deliberately absent from this API. No task in the
    /// product asks for full-resolution imagery to leave the machine, the gateway
    /// refuses that class unconditionally, and a setter would be the first step
    /// towards something that did.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the update or the ledger entry cannot be written.
    pub fn grant(
        &self,
        project: &ProjectId,
        metadata: bool,
        derived_imagery: bool,
        actor: &str,
    ) -> AuraResult<()> {
        let id = project.to_string();
        let now = crate::rfc3339(self.clock.now_utc());
        let actor = actor.to_string();

        self.catalog.writer().transact(move |tx| {
            let previous: (i64, i64) = tx
                .query_row(
                    "SELECT cloud_metadata, cloud_derived_imagery
                     FROM project_consent WHERE project_id = ?1",
                    params![id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap_or((0, 0));

            tx.execute(
                "UPDATE project_consent
                 SET cloud_metadata = ?2, cloud_derived_imagery = ?3,
                     granted_at = ?4, granted_by = ?5, revoked_at = NULL
                 WHERE project_id = ?1",
                params![
                    id,
                    i64::from(metadata),
                    i64::from(derived_imagery),
                    now,
                    actor
                ],
            )
            .map_err(|err| statement_failed("consent update failed", &err))?;

            for (field, old, new) in [
                ("cloud_metadata", previous.0, i64::from(metadata)),
                (
                    "cloud_derived_imagery",
                    previous.1,
                    i64::from(derived_imagery),
                ),
            ] {
                if old == new {
                    continue;
                }
                tx.execute(
                    "INSERT INTO consent_ledger
                       (project_id, changed_at, field, old_value, new_value, actor)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![id, now, field, old, new, actor],
                )
                .map_err(|err| statement_failed("consent ledger append failed", &err))?;
            }
            Ok(())
        })
    }
}

/// The cloud half of one project's consent row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudConsent {
    /// Filenames, EXIF summaries and counts may go to a cloud model.
    pub metadata: bool,
    /// Downscaled derived imagery may go to a cloud model.
    pub derived_imagery: bool,
    /// Full-resolution imagery may leave the machine. Never granted by AURA.
    pub full_imagery: bool,
    /// RFC 3339 expiry, when the grant has one.
    pub expires_at: Option<String>,
    /// RFC 3339 withdrawal, when the grant was revoked.
    pub revoked_at: Option<String>,
}

impl CloudConsent {
    /// True while the grant has neither been revoked nor expired.
    ///
    /// Expiry is compared as text rather than parsed, which works because RFC
    /// 3339 in UTC sorts lexicographically and every timestamp this product
    /// writes goes through `crate::rfc3339`.
    #[must_use]
    pub fn is_active(&self, now_rfc3339: &str) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        match &self.expires_at {
            None => true,
            Some(expiry) => expiry.as_str() > now_rfc3339,
        }
    }
}

impl ConsentGate for CatalogConsent {
    /// Decide whether one class of data may leave the machine.
    ///
    /// A read failure denies. See the module note for why.
    fn may_egress(&self, project: ProjectId, class: DataClass) -> Permit {
        let Ok(flags) = self.flags(&project) else {
            return Permit::Denied {
                reason: "consent could not be read, so egress is refused",
            };
        };
        if !flags.is_active(&crate::rfc3339(self.clock.now_utc())) {
            return Permit::Denied {
                reason: "the permission has been withdrawn or has expired",
            };
        }
        match class {
            DataClass::C0Public => Permit::Allowed,
            DataClass::C1Operational => {
                if flags.metadata {
                    Permit::Allowed
                } else {
                    Permit::Denied {
                        reason: "this job has not been given permission to send camera details",
                    }
                }
            }
            DataClass::C2Identifying => {
                if flags.derived_imagery {
                    Permit::Allowed
                } else {
                    Permit::Denied {
                        reason: "this job has not been given permission to send imagery",
                    }
                }
            }
            // Biometric data never leaves, whatever the row says. The gate is the
            // last line rather than the only one, and it does not consult a flag
            // that no part of the product is allowed to set.
            DataClass::C3Biometric => Permit::Denied {
                reason: "face data never leaves this computer",
            },
        }
    }
}

/// A gate that permits the two cloud classes. Tests and the phase gate use it.
#[derive(Debug, Default)]
pub struct AlwaysConsent;

impl ConsentGate for AlwaysConsent {
    fn may_egress(&self, _project: ProjectId, class: DataClass) -> Permit {
        match class {
            DataClass::C3Biometric => Permit::Denied {
                reason: "face data never leaves this computer",
            },
            _ => Permit::Allowed,
        }
    }
}

/// A gate that permits nothing. The offline acceptance test uses it.
#[derive(Debug, Default)]
pub struct NeverConsent;

impl ConsentGate for NeverConsent {
    fn may_egress(&self, _project: ProjectId, _class: DataClass) -> Permit {
        Permit::Denied {
            reason: "this job has not been given permission to use cloud AI",
        }
    }
}
