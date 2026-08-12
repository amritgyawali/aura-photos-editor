use aura_core::contract::consent::{DataClass, Permit, ProjectConsent};
use aura_core::contract::ids::ProjectId;
use time::OffsetDateTime;

#[test]
fn default_consent_denies_everything() {
    let c = ProjectConsent::denied(ProjectId::new());
    assert!(!c.cloud_metadata && !c.cloud_derived_imagery && !c.cloud_full_imagery);
    assert!(!c.telemetry && !c.improve_models);
    assert!(c.granted_at.is_none());
}

#[test]
fn revoked_consent_is_not_active() {
    let now = OffsetDateTime::UNIX_EPOCH;
    let mut c = ProjectConsent::denied(ProjectId::new());
    assert!(c.is_active(now));
    c.revoked_at = Some(now);
    assert!(!c.is_active(now));
}

#[test]
fn expired_consent_is_not_active() {
    let now = OffsetDateTime::UNIX_EPOCH + time::Duration::days(2);
    let mut c = ProjectConsent::denied(ProjectId::new());
    c.expires_at = Some(OffsetDateTime::UNIX_EPOCH + time::Duration::days(1));
    assert!(!c.is_active(now));
}

#[test]
fn no_source_file_can_construct_a_granting_default() {
    // Guard against a future refactor adding Default with true fields.
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/contract/consent.rs"),
    )
    .expect("read");
    assert!(
        !src.contains("impl Default for ProjectConsent"),
        "ProjectConsent must not have a Default impl; use denied()"
    );
}

/// The gate a later phase will implement. Biometric data has no allowing branch.
#[derive(Debug)]
struct AllowEverythingExceptBiometric;

impl aura_core::contract::consent::ConsentGate for AllowEverythingExceptBiometric {
    fn may_egress(&self, _project: ProjectId, class: DataClass) -> Permit {
        match class {
            DataClass::C3Biometric => Permit::Denied {
                reason: "biometric data never leaves the machine",
            },
            _ => Permit::Allowed,
        }
    }
}

#[test]
fn biometric_egress_is_always_refused() {
    use aura_core::contract::consent::ConsentGate;
    let gate = AllowEverythingExceptBiometric;
    let project = ProjectId::new();
    assert!(matches!(
        gate.may_egress(project, DataClass::C3Biometric),
        Permit::Denied { .. }
    ));
}
