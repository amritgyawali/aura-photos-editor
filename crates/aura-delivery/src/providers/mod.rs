//! The provider trait, the transport port, and the two providers this build ships.
//!
//! ## A provider is two things
//!
//! A [`Provider`] knows what a service's collections are called, how a set maps onto one, whether it
//! can be resumed, and how it reports what it received. A [`Transport`] knows how to put bytes
//! somewhere and how to ask what arrived.
//!
//! Separating them is what makes section 6.2's "adding a provider must not touch core code" true.
//! `Pic-Time` and `ShootProof` differ in their collection vocabulary and in nothing else that matters
//! here; a new one is a new `Provider` reusing whichever transport it needs.
//!
//! ## Why the transport's `put` takes an offset
//!
//! Because resumption is the whole point. A transport that could only put a whole file would make
//! [`crate::resume`] a state machine with two states, and a wedding that dropped at 90 % would
//! start again. The offset is what makes a network drop a pause.
//!
//! ## What ships and what does not
//!
//! [`FolderTransport`] is real and is what a folder, a NAS and an external drive use.
//! [`ScriptedTransport`] is what the resume tests drop connections through.
//!
//! **No network transport ships**, because `scripts/check-banned.sh` refuses every outbound
//! networking API outside `aura-cloud`. That is a rule about where the socket lives rather than a
//! gap in this design: everything above the socket exists, is exercised, and would not change.
//! ADR-0061 decision 4, exit condition C3.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use aura_core::contract::delivery::{ProviderId, SetMapping};
use aura_core::AuraResult;

use crate::errors::{unknown_provider, unreachable};

/// What a transport did with the bytes it was given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accepted {
    /// How many bytes the far end now holds for this key.
    pub bytes: u64,
    /// The digest the far end computed, when it computed one.
    ///
    /// `None` is not a failure. Some services report nothing until a file is complete, and a
    /// provider that returned a plausible digest rather than admitting it has none would make
    /// [`crate::resume`]'s corruption check silently vacuous.
    pub digest: Option<String>,
}

/// The one thing this crate needs from a network, a share, or a disk.
///
/// Two methods. `put` sends a slice starting at an offset; `head` asks what the far end already
/// holds. Everything about resumption is built from those two.
pub trait Transport: Send + Sync + std::fmt::Debug {
    /// Send `bytes` for `key`, starting at `offset` in the file.
    ///
    /// # Errors
    ///
    /// `AURA-DLV-10002` when the far end cannot be reached.
    fn put(&self, key: &str, offset: u64, bytes: &[u8]) -> AuraResult<Accepted>;

    /// What the far end already holds for `key`, or `None` when it holds nothing.
    ///
    /// # Errors
    ///
    /// `AURA-DLV-10002` when the far end cannot be reached.
    fn head(&self, key: &str) -> AuraResult<Option<Accepted>>;

    /// Whether this transport can accept a partial file at all.
    ///
    /// A transport that cannot resume is not a broken transport - it is a service whose API takes
    /// whole files - and the difference has to be visible, because on one a drop costs the tail of
    /// a file and on the other it costs the file.
    fn resumable(&self) -> bool {
        true
    }
}

/// What a gallery service is, apart from its transport.
pub trait Provider: Send + Sync + std::fmt::Debug {
    /// The provider's name, as a photographer configured it.
    fn id(&self) -> ProviderId;

    /// The human name, for the panel.
    fn label(&self) -> &'static str;

    /// The remote key one file lands at, given its set's mapping and its relative path.
    ///
    /// A method rather than a format string, because services differ: one wants
    /// `collection/filename`, another wants a flat namespace with the collection as a parameter.
    fn key_for(&self, mapping: &SetMapping, rel_path: &Path) -> String;

    /// Whether a set may be published on upload.
    ///
    /// Every provider in this build answers `false` for the same reason: publishing is a thing a
    /// photographer does, not a thing an upload does. A provider that answered `true` would need an
    /// ADR, because the failure it enables is a whole wedding visible on the wedding night.
    fn may_publish(&self) -> bool {
        false
    }
}

/// A transport that writes into a directory.
///
/// What a folder, a NAS and an external drive use, and it is the same code for all three: the
/// difference between them is how they *fail*, which is the caller's problem rather than the
/// transport's.
#[derive(Debug, Clone)]
pub struct FolderTransport {
    root: PathBuf,
}

impl FolderTransport {
    /// A transport over one directory.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, key: &str) -> PathBuf {
        // A key is provider-shaped text and reaches a path. Every separator is honoured and every
        // parent-directory hop is not, because a provider that returned `../` would otherwise
        // write outside the destination.
        let mut out = self.root.clone();
        for part in key.split('/') {
            if part.is_empty() || part == "." || part == ".." {
                continue;
            }
            out.push(part);
        }
        out
    }
}

impl Transport for FolderTransport {
    fn put(&self, key: &str, offset: u64, bytes: &[u8]) -> AuraResult<Accepted> {
        let path = self.path_for(key);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| unreachable(format!("cannot create `{}`: {e}", parent.display())))?;
        }
        let mut file = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(offset == 0)
            .open(&path)
            .map_err(|e| unreachable(format!("cannot open `{}`: {e}", path.display())))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| unreachable(format!("cannot seek `{}`: {e}", path.display())))?;
        file.write_all(bytes)
            .map_err(|e| unreachable(format!("cannot write `{}`: {e}", path.display())))?;
        file.sync_all()
            .map_err(|e| unreachable(format!("cannot sync `{}`: {e}", path.display())))?;
        drop(file);
        self.head(key)?
            .ok_or_else(|| unreachable(format!("`{}` vanished after a write", path.display())))
    }

    fn head(&self, key: &str) -> AuraResult<Option<Accepted>> {
        let path = self.path_for(key);
        let Ok(meta) = fs::metadata(&path) else {
            return Ok(None);
        };
        let mut file = fs::File::open(&path)
            .map_err(|e| unreachable(format!("cannot read `{}`: {e}", path.display())))?;
        let mut hasher = blake3::Hasher::new();
        let mut buf = vec![0_u8; 1024 * 256];
        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| unreachable(format!("cannot read `{}`: {e}", path.display())))?;
            if n == 0 {
                break;
            }
            match buf.get(..n) {
                Some(slice) => hasher.update(slice),
                None => break,
            };
        }
        Ok(Some(Accepted {
            bytes: meta.len(),
            digest: Some(hasher.finalize().to_hex().to_string()),
        }))
    }
}

/// A transport a test can make fail.
///
/// It holds everything in memory and can be told to drop after N bytes, to refuse outright, or to
/// return a digest that does not match - which is the three ways a real service fails and is what
/// makes section 10.1's "provider uploads resume correctly after a network drop" testable at all.
#[derive(Debug)]
pub struct ScriptedTransport {
    state: std::sync::Mutex<ScriptedState>,
    resumable: bool,
}

#[derive(Debug, Default)]
struct ScriptedState {
    files: BTreeMap<String, Vec<u8>>,
    /// Drop the connection after this many bytes of any single `put`. `None` means never.
    drop_after: Option<usize>,
    /// Refuse every call until this is cleared.
    offline: bool,
    /// Report a wrong digest for these keys.
    corrupt: Vec<String>,
}

impl ScriptedTransport {
    /// A transport that works.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: std::sync::Mutex::new(ScriptedState::default()),
            resumable: true,
        }
    }

    /// A transport that cannot take a partial file.
    #[must_use]
    pub fn whole_files_only() -> Self {
        Self {
            state: std::sync::Mutex::new(ScriptedState::default()),
            resumable: false,
        }
    }

    /// Accept at most `n` bytes per call, then drop.
    pub fn drop_after(&self, n: usize) {
        if let Ok(mut s) = self.state.lock() {
            s.drop_after = Some(n);
        }
    }

    /// Accept whole calls again.
    pub fn recover(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.drop_after = None;
            s.offline = false;
        }
    }

    /// Refuse every call.
    pub fn go_offline(&self) {
        if let Ok(mut s) = self.state.lock() {
            s.offline = true;
        }
    }

    /// Report a wrong digest for one key.
    pub fn corrupt(&self, key: &str) {
        if let Ok(mut s) = self.state.lock() {
            s.corrupt.push(key.to_owned());
        }
    }

    /// What the far end holds for a key.
    #[must_use]
    pub fn contents(&self, key: &str) -> Option<Vec<u8>> {
        self.state
            .lock()
            .ok()
            .and_then(|s| s.files.get(key).cloned())
    }
}

impl Default for ScriptedTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for ScriptedTransport {
    fn put(&self, key: &str, offset: u64, bytes: &[u8]) -> AuraResult<Accepted> {
        let mut s = self
            .state
            .lock()
            .map_err(|_| unreachable("the scripted transport's lock is poisoned"))?;
        if s.offline {
            return Err(unreachable("the scripted transport is offline"));
        }
        let take = s.drop_after.map_or(bytes.len(), |n| n.min(bytes.len()));
        let entry = s.files.entry(key.to_owned()).or_default();
        let at = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(entry.len());
        entry.truncate(at);
        if let Some(slice) = bytes.get(..take) {
            entry.extend_from_slice(slice);
        }
        let held = entry.clone();
        let dropped = take < bytes.len();
        let corrupt = s.corrupt.iter().any(|k| k == key);
        drop(s);

        if dropped {
            // The connection died mid-transfer. The far end kept what it got, which is exactly
            // what makes the next call a resume rather than a restart.
            return Err(unreachable(format!(
                "the scripted transport dropped after {take} bytes"
            )));
        }
        Ok(Accepted {
            bytes: held.len() as u64,
            digest: Some(if corrupt {
                "0".repeat(64)
            } else {
                blake3::hash(&held).to_hex().to_string()
            }),
        })
    }

    fn head(&self, key: &str) -> AuraResult<Option<Accepted>> {
        let s = self
            .state
            .lock()
            .map_err(|_| unreachable("the scripted transport's lock is poisoned"))?;
        if s.offline {
            return Err(unreachable("the scripted transport is offline"));
        }
        let Some(held) = s.files.get(key) else {
            return Ok(None);
        };
        let corrupt = s.corrupt.iter().any(|k| k == key);
        Ok(Some(Accepted {
            bytes: held.len() as u64,
            digest: Some(if corrupt {
                "0".repeat(64)
            } else {
                blake3::hash(held).to_hex().to_string()
            }),
        }))
    }

    fn resumable(&self) -> bool {
        self.resumable
    }
}

/// A provider whose collections are directories under a root.
///
/// The shape a folder-backed gallery, a NAS share and a mounted object store all have: a set maps
/// onto a collection name, and a file lands at `collection/filename`.
#[derive(Debug, Clone)]
pub struct FolderProvider {
    id: ProviderId,
    label: &'static str,
}

impl FolderProvider {
    /// The provider a photographer configures when their "gallery" is a synced folder.
    ///
    /// # Errors
    ///
    /// `AURA-DLV-10001` when the name is not one a path and a catalog key both accept.
    pub fn new(name: &str, label: &'static str) -> AuraResult<Self> {
        Ok(Self {
            id: ProviderId::parse(name)?,
            label,
        })
    }
}

impl Provider for FolderProvider {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }
    fn label(&self) -> &'static str {
        self.label
    }
    fn key_for(&self, mapping: &SetMapping, rel_path: &Path) -> String {
        let name = rel_path
            .file_name()
            .map_or_else(|| "unnamed".to_owned(), |n| n.to_string_lossy().to_string());
        format!("{}/{name}", mapping.remote)
    }
}

/// A provider whose namespace is flat and whose collection travels beside the key.
///
/// The other shape services come in, and the reason `key_for` is a method rather than a format
/// string: a flat service given `collection/filename` creates four thousand files whose names all
/// begin with the same word.
#[derive(Debug, Clone)]
pub struct FlatProvider {
    id: ProviderId,
    label: &'static str,
}

impl FlatProvider {
    /// A flat-namespace provider.
    ///
    /// # Errors
    ///
    /// `AURA-DLV-10001` when the name is not one a path and a catalog key both accept.
    pub fn new(name: &str, label: &'static str) -> AuraResult<Self> {
        Ok(Self {
            id: ProviderId::parse(name)?,
            label,
        })
    }
}

impl Provider for FlatProvider {
    fn id(&self) -> ProviderId {
        self.id.clone()
    }
    fn label(&self) -> &'static str {
        self.label
    }
    fn key_for(&self, _mapping: &SetMapping, rel_path: &Path) -> String {
        rel_path
            .file_name()
            .map_or_else(|| "unnamed".to_owned(), |n| n.to_string_lossy().to_string())
    }
}

/// The providers this build has.
///
/// Two, and adding a third touches this function and nothing else - which is the property section
/// 6.2 asks for.
///
/// # Errors
///
/// `AURA-DLV-10001` when the name is not one this build has.
pub fn registry(name: &str) -> AuraResult<Box<dyn Provider>> {
    match name {
        "folder-gallery" => Ok(Box::new(FolderProvider::new(
            "folder-gallery",
            "A synced folder",
        )?)),
        "flat-gallery" => Ok(Box::new(FlatProvider::new(
            "flat-gallery",
            "A flat gallery",
        )?)),
        other => Err(unknown_provider(other)),
    }
}

/// Every provider this build has, by name.
#[must_use]
pub fn known() -> Vec<&'static str> {
    vec!["folder-gallery", "flat-gallery"]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(set: &str, remote: &str) -> SetMapping {
        SetMapping {
            set: set.to_owned(),
            remote: remote.to_owned(),
            publish: false,
        }
    }

    #[test]
    fn a_folder_transport_round_trips_and_reports_what_it_holds() {
        let dir = tempfile::tempdir().unwrap();
        let t = FolderTransport::new(dir.path());
        assert!(t.head("gallery/a.jpg").unwrap().is_none());

        let bytes = b"a photograph".to_vec();
        let accepted = t.put("gallery/a.jpg", 0, &bytes).unwrap();
        assert_eq!(accepted.bytes, bytes.len() as u64);
        assert_eq!(
            accepted.digest,
            Some(blake3::hash(&bytes).to_hex().to_string())
        );
        assert_eq!(t.head("gallery/a.jpg").unwrap(), Some(accepted));
    }

    #[test]
    fn a_folder_transport_appends_at_an_offset_which_is_what_makes_a_resume_possible() {
        let dir = tempfile::tempdir().unwrap();
        let t = FolderTransport::new(dir.path());
        t.put("a.bin", 0, b"first-half").unwrap();
        let done = t.put("a.bin", 10, b"second-half").unwrap();
        assert_eq!(done.bytes, 21);
        assert_eq!(
            fs::read(dir.path().join("a.bin")).unwrap(),
            b"first-halfsecond-half"
        );
    }

    #[test]
    fn a_key_cannot_escape_the_transport_root() {
        // A key is provider-shaped text. A service that returned `../` would otherwise write
        // outside the destination a photographer chose.
        let dir = tempfile::tempdir().unwrap();
        let t = FolderTransport::new(dir.path().join("root"));
        t.put("../../escaped.jpg", 0, b"x").unwrap();
        assert!(dir.path().join("root/escaped.jpg").exists());
        assert!(!dir.path().join("escaped.jpg").exists());
    }

    #[test]
    fn the_scripted_transport_keeps_what_it_got_before_it_dropped() {
        // Which is the whole reason a resume is possible: a far end that discarded a partial
        // transfer would make every drop a restart.
        let t = ScriptedTransport::new();
        t.drop_after(4);
        assert!(t.put("a", 0, b"0123456789").is_err());
        assert_eq!(t.contents("a"), Some(b"0123".to_vec()));
        t.recover();
        t.put("a", 4, b"456789").unwrap();
        assert_eq!(t.contents("a"), Some(b"0123456789".to_vec()));
    }

    #[test]
    fn a_transport_that_cannot_resume_says_so_rather_than_pretending() {
        assert!(ScriptedTransport::new().resumable());
        assert!(!ScriptedTransport::whole_files_only().resumable());
        assert!(FolderTransport::new(".").resumable());
    }

    #[test]
    fn the_two_provider_shapes_produce_different_keys_for_one_file() {
        // Which is why `key_for` is a method. A flat service given `collection/filename` creates
        // four thousand files whose names all begin with the same word.
        let m = mapping("gallery", "wedding-2026");
        let path = Path::new("gallery/2026-05-16_alex_0001.jpg");
        let folder = FolderProvider::new("folder-gallery", "x").unwrap();
        let flat = FlatProvider::new("flat-gallery", "x").unwrap();
        assert_eq!(
            folder.key_for(&m, path),
            "wedding-2026/2026-05-16_alex_0001.jpg"
        );
        assert_eq!(flat.key_for(&m, path), "2026-05-16_alex_0001.jpg");
    }

    #[test]
    fn no_provider_in_this_build_may_publish_on_upload() {
        // Publishing is a thing a photographer does, not a thing an upload does. A provider that
        // answered true would need an ADR, because the failure it enables is a whole wedding
        // visible on the wedding night.
        for name in known() {
            assert!(!registry(name).unwrap().may_publish(), "{name} may publish");
        }
    }

    #[test]
    fn an_unknown_provider_is_refused_by_name() {
        assert!(registry("pic-time").is_err());
        assert_eq!(known().len(), 2);
    }
}
