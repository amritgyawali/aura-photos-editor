//! The frozen [`DeliveryService`], and the two passes behind it.
//!
//! ## Backup and upload are one service because verification is the thing that must not be
//! duplicated
//!
//! They look like two features and they are one question asked twice: take a verified local file,
//! put it somewhere, check what arrived. The transport differs and nothing else does, which is why
//! there is one service with two methods rather than two services - two would be two answers to
//! "did this wedding get out safely".
//!
//! ## Resuming is not a separate call
//!
//! [`Delivery::upload`] picks up from stored per-file state. There is no `resume_upload` on the
//! contract, because a photographer pressing "upload" after their wifi came back is doing the same
//! thing they did the first time, and a surface with two buttons for it is a surface where one of
//! them is the wrong one.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::contract::delivery::{
    DeliveryCode, DeliveryOutline, DeliveryReason, DeliveryService, Destination, ExportedFile,
    ProviderId, SetMapping, UploadItem, UploadProgress, UploadState,
};
use aura_core::{AuraResult, ProjectId};

use crate::backup::copy_all;
use crate::errors::{no_credential, unreachable};
use crate::mapping::Mapping;
use crate::providers::{registry, Provider, Transport};
use crate::resume::send;
use crate::store::DeliveryStore;

/// One upload pass over one provider.
#[derive(Debug)]
pub struct UploadPass<'a> {
    store: &'a DeliveryStore,
    provider: &'a dyn Provider,
    transport: &'a dyn Transport,
}

/// What a pass produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadResult {
    /// How it went.
    pub progress: UploadProgress,
    /// What the panel says about the pass as a whole.
    pub reasons: Vec<DeliveryReason>,
    /// Sets that had nowhere to go.
    pub unmapped: Vec<String>,
}

impl<'a> UploadPass<'a> {
    /// A pass over one provider through one transport.
    #[must_use]
    pub fn new(
        store: &'a DeliveryStore,
        provider: &'a dyn Provider,
        transport: &'a dyn Transport,
    ) -> Self {
        Self {
            store,
            provider,
            transport,
        }
    }

    /// Start or resume an upload of one sealed delivery.
    ///
    /// # Errors
    ///
    /// `AURA-DLV-10002` when the local file cannot be read, `AURA-DLV-10005` when the far end's
    /// digest disagrees after every attempt.
    pub fn run(
        &self,
        project: ProjectId,
        job_id: &str,
        export_root: &Path,
        files: &[ExportedFile],
        mapping: &[SetMapping],
    ) -> AuraResult<UploadResult> {
        let mapping = Mapping::new(mapping);
        // Publishing is a thing a photographer does, not a thing an upload does. A provider that
        // cannot publish is asked not to, and the rows that wanted to are named rather than
        // silently ignored.
        let (mapping, mut reasons) = if self.provider.may_publish() {
            (mapping, Vec::new())
        } else {
            mapping.without_publish()
        };

        let mut set_names: Vec<String> = files.iter().map(|f| f.set.clone()).collect();
        set_names.sort_unstable();
        set_names.dedup();
        let (_, unmapped) = mapping.split(&set_names);
        for (_, reason) in &unmapped {
            reasons.push(reason.clone());
        }
        let unmapped_names: Vec<String> =
            unmapped.iter().map(|(name, _)| (*name).clone()).collect();

        let target_id = self.store.upsert_target(
            project,
            "provider",
            self.provider.id().as_str(),
            self.provider.id().as_str(),
            mapping
                .rows()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
                .as_slice(),
            true,
        )?;

        let items: Vec<UploadItem> = files
            .iter()
            .filter(|f| mapping.get(&f.set).is_some())
            .map(|f| UploadItem {
                image: f.image,
                set: f.set.clone(),
                path: f.path.clone(),
                bytes: f.bytes,
                hash: f.hash.clone(),
                state: UploadState::Pending,
            })
            .collect();
        // `INSERT OR IGNORE`, so a re-run does not reset the 640 files that already arrived.
        self.store.seed_upload(&target_id, job_id, &items)?;

        // Work from the stored state rather than from the seed, which is what makes this call a
        // resume when it needs to be.
        let stored = self.store.items(&target_id)?;
        for item in &stored {
            if item.state == UploadState::Verified {
                continue;
            }
            let Some(map) = mapping.get(&item.set) else {
                continue;
            };
            let local = export_root.join(&item.path);
            let bytes = std::fs::read(&local)
                .map_err(|e| unreachable(format!("cannot read `{}`: {e}", local.display())))?;
            let key = self.provider.key_for(map, &item.path);
            let step = send(self.transport, item, &bytes, &key)?;
            self.store.set_state(
                &target_id,
                job_id,
                &item.path.to_string_lossy(),
                &step.state,
            )?;
            for r in step.reasons {
                if !reasons.iter().any(|e| e.code == r.code) {
                    reasons.push(r);
                }
            }
        }

        if !self.provider.may_publish()
            && !reasons
                .iter()
                .any(|r| r.code == DeliveryCode::LeftUnpublished)
        {
            reasons.push(DeliveryReason::plain(DeliveryCode::LeftUnpublished));
        }

        Ok(UploadResult {
            progress: self.store.progress(&target_id)?,
            reasons,
            unmapped: unmapped_names,
        })
    }
}

/// The frozen [`DeliveryService`], over one catalog.
///
/// Reads, plus the two acts. The acts need a transport and a local root, which `aura-app` supplies:
/// the same shape phase 29's `Curate` has, and for the same reason. A service that could open a
/// socket would be a service that needs one to answer "what has this project uploaded".
#[derive(Debug, Clone)]
pub struct Delivery {
    store: DeliveryStore,
    export_root: Option<PathBuf>,
    files: Vec<ExportedFile>,
    job_id: String,
}

impl Delivery {
    /// Wrap a catalog. Reads only until `with_delivery` names one.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>) -> Self {
        Self {
            store: DeliveryStore::new(catalog),
            export_root: None,
            files: Vec::new(),
            job_id: String::new(),
        }
    }

    /// Point this service at a sealed delivery so `backup` and `upload` have something to send.
    #[must_use]
    pub fn with_delivery(
        mut self,
        job_id: impl Into<String>,
        export_root: impl Into<PathBuf>,
        files: Vec<ExportedFile>,
    ) -> Self {
        self.job_id = job_id.into();
        self.export_root = Some(export_root.into());
        self.files = files;
        self
    }

    /// The store, for a caller that needs to seed a target.
    #[must_use]
    pub fn store(&self) -> &DeliveryStore {
        &self.store
    }
}

impl DeliveryService for Delivery {
    fn outline(&self, project: ProjectId) -> AuraResult<DeliveryOutline> {
        self.store.outline(project)
    }

    fn backup(&self, project: ProjectId, to: &Destination) -> AuraResult<DeliveryOutline> {
        let Some(root) = &self.export_root else {
            return Err(unreachable(
                "this service has not been pointed at a sealed delivery",
            ));
        };
        let Some(dest) = to.local_root() else {
            return Err(unreachable(format!(
                "`{}` is a provider rather than a backup destination",
                to.kind()
            )));
        };
        let (copied, outline) = copy_all(root, dest, &self.files)?;
        let target_id = self.store.upsert_target(
            project,
            "backup",
            &dest.to_string_lossy(),
            &dest.to_string_lossy(),
            &[],
            false,
        )?;
        self.store.write_backup(&target_id, &self.job_id, &copied)?;
        Ok(outline)
    }

    fn upload(
        &self,
        project: ProjectId,
        provider: &ProviderId,
        mapping: &[SetMapping],
    ) -> AuraResult<UploadProgress> {
        // No transport ships in this build. The refusal names the reason rather than reporting an
        // empty upload, because "nothing was sent" and "nothing can be sent from this build" are
        // different facts and a photographer who saw the first would go looking at their
        // credentials. Phase 24's rule; exit condition C3.
        let _ = (project, provider, mapping);
        Err(unreachable(format!(
            "this build has no network transport, so `{}` cannot be reached; \
             NETWORK_TRANSPORT_AVAILABLE is false",
            provider.as_str()
        )))
    }

    fn progress(&self, project: ProjectId, provider: &ProviderId) -> AuraResult<UploadProgress> {
        match self.store.target(project, "provider", provider.as_str())? {
            Some((target_id, _, _)) => self.store.progress(&target_id),
            None => Ok(UploadProgress::default()),
        }
    }

    fn items(&self, project: ProjectId, provider: &ProviderId) -> AuraResult<Vec<UploadItem>> {
        match self.store.target(project, "provider", provider.as_str())? {
            Some((target_id, _, _)) => self.store.items(&target_id),
            None => Ok(Vec::new()),
        }
    }

    fn providers(&self) -> AuraResult<Vec<(ProviderId, bool)>> {
        self.store.providers()
    }
}

/// Look a provider up and check that it has a credential before anything is sent.
///
/// Two checks in one place, because they produce different runbooks and a caller that did them
/// separately would eventually do only one.
///
/// # Errors
///
/// `AURA-DLV-10001` when the provider is unknown, `AURA-DLV-10004` when it has no credential.
pub fn resolve(name: &str, has_credential: bool) -> AuraResult<Box<dyn Provider>> {
    let provider = registry(name)?;
    if !has_credential {
        return Err(no_credential(name));
    }
    Ok(provider)
}
