//! The persisted graph, so the second project open is instant.
//!
//! Section 6.3: "build in parallel at project open, then persist a snapshot so
//! subsequent opens are instant". A snapshot is a cache in exactly the sense the
//! preview cache is - it holds nothing that cannot be recomputed from catalog
//! rows - so the rules are the cache's rules:
//!
//! * every field that could invalidate it is in the header, and a mismatch is a
//!   rejection rather than a best-effort load;
//! * the body carries a digest, so a half-written file after a power cut is
//!   detected rather than deserialised into a plausible-looking graph;
//! * a rejection is `AURA-ML-5014`, a warning, and the fallback is to rebuild.
//!
//! It is written to a temporary file and renamed, so a crash during the write
//! leaves the previous snapshot intact.

use std::path::{Path, PathBuf};

use aura_core::AuraResult;
use uuid_lite::{uuid_from_bytes, uuid_to_bytes};

use crate::contract::index::{EMBED_DIM, F16};
use crate::errors::snapshot_unusable;
use crate::hnsw::{raw::RawNode, EntryMeta, HnswIndex, HnswParams};

/// File magic. The trailing digit is the format generation, bumped when the
/// layout changes in a way an older reader would misinterpret.
const MAGIC: &[u8; 8] = b"AURAIDX1";

/// Format generation, checked separately from the magic so a change can be
/// described in the error rather than looking like a corrupt file.
const FORMAT: u32 = 1;

/// Everything that decides whether a snapshot may be loaded at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotHeader {
    /// Format generation.
    pub format: u32,
    /// Graph shape. A snapshot built with different parameters describes a
    /// different graph and is not loadable into this one.
    pub params: HnswParams,
    /// Embedding model version every vector in the body was produced by.
    pub model_ver: u16,
    /// Preprocessing version, so a resize or colour change invalidates too.
    pub preprocess_ver: u16,
    /// Nodes in the body.
    pub nodes: u32,
}

/// A snapshot on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    /// Where it lives.
    pub path: PathBuf,
    /// What it claims to be.
    pub header: SnapshotHeader,
}

impl Snapshot {
    /// The conventional file name for a project's snapshot.
    #[must_use]
    pub fn path_for(cache_dir: &Path, project_id: &str, model_ver: u16) -> PathBuf {
        cache_dir
            .join("index")
            .join(format!("{project_id}.v{model_ver}.hnsw"))
    }

    /// Write `index` to `path`, atomically.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5014` when the file cannot be created, written or renamed. A
    /// snapshot that cannot be written is not a failure of the run: the index is
    /// already in memory and the next open simply rebuilds.
    pub fn write(
        index: &HnswIndex,
        path: &Path,
        model_ver: u16,
        preprocess_ver: u16,
    ) -> AuraResult<Self> {
        let (nodes, camera_labels, entry, top_level) = index.to_raw();
        let body = encode_body(&nodes, &camera_labels, entry, top_level);
        let digest = blake3::hash(&body);

        let header = SnapshotHeader {
            format: FORMAT,
            params: index.params(),
            model_ver,
            preprocess_ver,
            nodes: nodes.len() as u32,
        };

        let mut out = Vec::with_capacity(body.len() + 96);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT.to_le_bytes());
        out.extend_from_slice(&(header.params.m as u32).to_le_bytes());
        out.extend_from_slice(&(header.params.m0 as u32).to_le_bytes());
        out.extend_from_slice(&(header.params.ef_construction as u32).to_le_bytes());
        out.extend_from_slice(&(header.params.ef_search as u32).to_le_bytes());
        out.extend_from_slice(&model_ver.to_le_bytes());
        out.extend_from_slice(&preprocess_ver.to_le_bytes());
        out.extend_from_slice(&header.nodes.to_le_bytes());
        out.extend_from_slice(digest.as_bytes());
        out.extend_from_slice(&body);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                snapshot_unusable(&format!("cannot create {}: {e}", parent.display()))
            })?;
        }
        let temporary = path.with_extension("hnsw.part");
        std::fs::write(&temporary, &out).map_err(|e| {
            snapshot_unusable(&format!("cannot write {}: {e}", temporary.display()))
        })?;
        std::fs::rename(&temporary, path).map_err(|e| {
            drop(std::fs::remove_file(&temporary));
            snapshot_unusable(&format!("cannot rename into {}: {e}", path.display()))
        })?;

        Ok(Self {
            path: path.to_path_buf(),
            header,
        })
    }

    /// Load a snapshot, or say precisely why it cannot be trusted.
    ///
    /// # Errors
    ///
    /// `AURA-ML-5014` when the file is missing, too short, from another format,
    /// built with different graph parameters, produced by a different model or
    /// preprocessing version, or fails its digest. Every one of those is a
    /// rebuild, not a crash.
    pub fn load(
        path: &Path,
        params: HnswParams,
        model_ver: u16,
        preprocess_ver: u16,
    ) -> AuraResult<HnswIndex> {
        let bytes = std::fs::read(path)
            .map_err(|e| snapshot_unusable(&format!("cannot read {}: {e}", path.display())))?;
        let mut cursor = Cursor::new(&bytes);

        let magic = cursor
            .take(8)
            .ok_or_else(|| snapshot_unusable("shorter than its own magic"))?;
        if magic != MAGIC {
            return Err(snapshot_unusable("not an AURA index snapshot"));
        }
        let format = cursor
            .u32()
            .ok_or_else(|| snapshot_unusable("truncated header"))?;
        if format != FORMAT {
            return Err(snapshot_unusable(&format!(
                "format {format}, this build reads {FORMAT}"
            )));
        }
        let stored = HnswParams {
            m: cursor.u32().unwrap_or(0) as usize,
            m0: cursor.u32().unwrap_or(0) as usize,
            ef_construction: cursor.u32().unwrap_or(0) as usize,
            ef_search: cursor.u32().unwrap_or(0) as usize,
        };
        if stored != params {
            return Err(snapshot_unusable("built with different graph parameters"));
        }
        let stored_model = cursor
            .u16()
            .ok_or_else(|| snapshot_unusable("truncated header"))?;
        let stored_preprocess = cursor
            .u16()
            .ok_or_else(|| snapshot_unusable("truncated header"))?;
        if stored_model != model_ver {
            return Err(snapshot_unusable(&format!(
                "vectors are model version {stored_model}, this build uses {model_ver}"
            )));
        }
        if stored_preprocess != preprocess_ver {
            return Err(snapshot_unusable(&format!(
                "vectors were preprocessed by version {stored_preprocess}, this build uses \
                 {preprocess_ver}"
            )));
        }
        let node_count = cursor
            .u32()
            .ok_or_else(|| snapshot_unusable("truncated header"))?;
        let digest = cursor
            .take(32)
            .ok_or_else(|| snapshot_unusable("truncated digest"))?
            .to_vec();
        let body = cursor.rest();
        if blake3::hash(body).as_bytes().as_slice() != digest.as_slice() {
            return Err(snapshot_unusable("body does not match its digest"));
        }

        let (nodes, camera_labels, entry, top_level) =
            decode_body(body).ok_or_else(|| snapshot_unusable("body is malformed"))?;
        if nodes.len() as u32 != node_count {
            return Err(snapshot_unusable(&format!(
                "header says {node_count} nodes, body has {}",
                nodes.len()
            )));
        }

        Ok(HnswIndex::from_raw(
            params,
            nodes,
            camera_labels,
            entry,
            top_level,
        ))
    }
}

/// Serialise the graph. Every integer is little-endian; the layout is fixed so a
/// future reader can be written against this function alone.
fn encode_body(
    nodes: &[RawNode],
    camera_labels: &[String],
    entry: Option<u32>,
    top_level: usize,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(nodes.len() * (EMBED_DIM * 2 + 128));
    out.extend_from_slice(&entry.map_or(u32::MAX, |slot| slot).to_le_bytes());
    out.extend_from_slice(&(top_level as u32).to_le_bytes());
    out.extend_from_slice(&(camera_labels.len() as u32).to_le_bytes());
    for label in camera_labels {
        let bytes = label.as_bytes();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(bytes);
    }
    out.extend_from_slice(&(nodes.len() as u32).to_le_bytes());
    for node in nodes {
        out.extend_from_slice(&uuid_to_bytes(&node.id));
        out.extend_from_slice(&node.dhash.to_le_bytes());
        out.extend_from_slice(&node.model_ver.to_le_bytes());
        if let Some(stamp) = node.meta.timeline_ms {
            out.push(1);
            out.extend_from_slice(&stamp.to_le_bytes());
        } else {
            out.push(0);
            out.extend_from_slice(&0i64.to_le_bytes());
        }
        out.extend_from_slice(&node.meta.camera.map_or(u32::MAX, |id| id.0).to_le_bytes());
        out.extend_from_slice(&node.meta.scene.map_or(u32::MAX, |id| id.0).to_le_bytes());
        for value in &node.vector {
            out.extend_from_slice(&F16::from_f32(*value).to_bits().to_le_bytes());
        }
        out.push(node.neighbours.len() as u8);
        for level in &node.neighbours {
            out.extend_from_slice(&(level.len() as u32).to_le_bytes());
            for neighbour in level {
                out.extend_from_slice(&neighbour.to_le_bytes());
            }
        }
    }
    out
}

/// The inverse of [`encode_body`]. Returns `None` on anything it cannot read,
/// which the caller turns into `AURA-ML-5014`.
#[allow(clippy::type_complexity)]
fn decode_body(bytes: &[u8]) -> Option<(Vec<RawNode>, Vec<String>, Option<u32>, usize)> {
    let mut cursor = Cursor::new(bytes);
    let entry = cursor.u32()?;
    let top_level = cursor.u32()? as usize;

    let label_count = cursor.u32()?;
    let mut camera_labels = Vec::with_capacity(label_count as usize);
    for _ in 0..label_count {
        let len = cursor.u32()? as usize;
        let text = cursor.take(len)?;
        camera_labels.push(String::from_utf8(text.to_vec()).ok()?);
    }

    let node_count = cursor.u32()?;
    let mut nodes = Vec::with_capacity(node_count as usize);
    for _ in 0..node_count {
        let id = uuid_from_bytes(cursor.take(16)?)?;
        let dhash = cursor.u64()?;
        let model_ver = cursor.u16()?;
        let has_time = cursor.u8()?;
        let stamp = cursor.i64()?;
        let camera = cursor.u32()?;
        let scene = cursor.u32()?;
        let mut vector = Vec::with_capacity(EMBED_DIM);
        for _ in 0..EMBED_DIM {
            vector.push(F16::from_bits(cursor.u16()?).to_f32());
        }
        let levels = cursor.u8()?;
        let mut neighbours = Vec::with_capacity(levels as usize);
        for _ in 0..levels {
            let count = cursor.u32()? as usize;
            let mut list = Vec::with_capacity(count);
            for _ in 0..count {
                list.push(cursor.u32()?);
            }
            neighbours.push(list);
        }
        nodes.push(RawNode {
            id,
            vector,
            dhash,
            model_ver,
            meta: EntryMeta {
                timeline_ms: (has_time == 1).then_some(stamp),
                camera: (camera != u32::MAX).then_some(crate::contract::index::CameraId(camera)),
                scene: (scene != u32::MAX).then_some(crate::contract::index::SceneId(scene)),
            },
            neighbours,
        });
    }

    Some((
        nodes,
        camera_labels,
        (entry != u32::MAX).then_some(entry),
        top_level,
    ))
}

/// A bounds-checked reader. Every method returns `None` rather than panicking,
/// because the input is a file that may have been truncated by a power cut.
#[derive(Debug)]
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let slice = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(slice)
    }

    fn rest(&self) -> &'a [u8] {
        self.bytes.get(self.at..).unwrap_or_default()
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }

    fn u16(&mut self) -> Option<u16> {
        let bytes: [u8; 2] = self.take(2)?.try_into().ok()?;
        Some(u16::from_le_bytes(bytes))
    }

    fn u32(&mut self) -> Option<u32> {
        let bytes: [u8; 4] = self.take(4)?.try_into().ok()?;
        Some(u32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Option<u64> {
        let bytes: [u8; 8] = self.take(8)?.try_into().ok()?;
        Some(u64::from_le_bytes(bytes))
    }

    fn i64(&mut self) -> Option<i64> {
        let bytes: [u8; 8] = self.take(8)?.try_into().ok()?;
        Some(i64::from_le_bytes(bytes))
    }
}

/// Sixteen raw bytes in, one typed id out, and back again.
///
/// `PhotoId`'s text form is 36 characters plus a four-character prefix; storing
/// that per node would add 25 % to a 4,000-image snapshot for no benefit. The
/// UUID's own byte order is the stable one.
mod uuid_lite {
    use crate::contract::index::ImageId;

    /// The sixteen bytes of an id, big-endian as the UUID standard defines.
    pub(super) fn uuid_to_bytes(id: &ImageId) -> [u8; 16] {
        *id.as_uuid().as_bytes()
    }

    /// Rebuild an id from sixteen bytes.
    pub(super) fn uuid_from_bytes(bytes: &[u8]) -> Option<ImageId> {
        let fixed: [u8; 16] = bytes.try_into().ok()?;
        Some(ImageId::from_uuid(uuid::Uuid::from_bytes(fixed)))
    }
}
