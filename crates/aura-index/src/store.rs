//! Vectors and descriptors as catalog rows.
//!
//! The catalog is the only place AURA stores truth, and an embedding is not
//! truth - it is derived from pixels the product never modifies. It lives here
//! anyway, for two reasons that matter more than the purity argument: it must be
//! transactional with the photograph it describes, so a project deleted mid-run
//! cannot leave orphan vectors; and it must survive a restart, because
//! re-embedding 4,000 images to answer one question is not resumability,
//! it is a rebuild.
//!
//! Everything here is written through the catalog's single writer thread and
//! read through the read pool, exactly as every other table is.

use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, PhotoId};
use rusqlite::{params, Connection};

use crate::contract::index::{ImageEmbedding, ImageId, LumaStats, EMBED_DIM, F16, HSV_BINS};
use crate::hnsw::{EntryMeta, IndexEntry};

/// Bytes one vector occupies in the `vec` blob.
pub const VECTOR_BYTES: usize = EMBED_DIM * 2;

/// Bytes one histogram occupies in the `hsv_hist` blob.
pub const HISTOGRAM_BYTES: usize = HSV_BINS;

/// Dominant colours kept per image.
pub const PALETTE_LEN: usize = 5;

/// Bytes the palette blob occupies.
pub const PALETTE_BYTES: usize = PALETTE_LEN * 3;

/// The payload one image costs, in bytes, across both tables.
///
/// Vector 1,024 + hash 8 + histogram 512 + six luminance statistics at 8 + two
/// edge statistics at 8 + palette 15. Section 11 budgets 1.6 KB; this is 1,623.
pub const BYTES_PER_IMAGE: usize =
    VECTOR_BYTES + 8 + HISTOGRAM_BYTES + 6 * 8 + 2 * 8 + PALETTE_BYTES;

/// The descriptors that ride alongside a vector.
#[derive(Debug, Clone, PartialEq)]
pub struct Descriptors {
    /// 8x8x8 HSV histogram, each bin scaled to `0..255`.
    pub hsv_hist: [u8; HSV_BINS],
    /// Luminance statistics.
    pub luma: LumaStats,
    /// Mean gradient magnitude, `0..1`.
    pub edge_energy: f32,
    /// Ninetieth percentile of the gradient magnitude, `0..1`.
    pub edge_p90: f32,
    /// Five dominant colours, most frequent first.
    pub palette: [[u8; 3]; PALETTE_LEN],
    /// Which descriptor implementation produced them.
    pub descriptor_ver: u16,
}

impl Descriptors {
    /// Empty descriptors at a given version.
    #[must_use]
    pub const fn empty(descriptor_ver: u16) -> Self {
        Self {
            hsv_hist: [0; HSV_BINS],
            luma: LumaStats::zero(),
            edge_energy: 0.0,
            edge_p90: 0.0,
            palette: [[0; 3]; PALETTE_LEN],
            descriptor_ver,
        }
    }
}

/// Everything one analysed photograph contributes to the substrate.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredEmbedding {
    /// The vector and the parts of it the contract carries.
    pub embedding: ImageEmbedding,
    /// The rest of the cheap descriptors.
    pub descriptors: Descriptors,
    /// Which preprocessing path produced the pixels the model saw.
    pub preprocess_ver: u16,
    /// Which pixel tier those pixels came from.
    pub pixel_tier: u8,
    /// `embedded` when the pixels were the camera's own JPEG, `decoded` when they
    /// were AURA's render. Invariant: pixels carry their provenance.
    pub pixel_source: &'static str,
}

/// Reads and writes the two tables migration 5 adds.
#[derive(Debug)]
pub struct EmbeddingStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
}

impl EmbeddingStore {
    /// Use one catalog's `embeddings` and `descriptors` tables.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>, clock: Arc<dyn Clock>) -> Self {
        Self { catalog, clock }
    }

    /// Write one photograph's vector and descriptors in a single transaction.
    ///
    /// `INSERT OR REPLACE`, so re-embedding after a model change overwrites
    /// rather than duplicating, and a killed run that is restarted repeats work
    /// harmlessly - which is what invariant 5 asks of every stage.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when either statement fails. The two writes share a
    /// transaction: a vector with no descriptors would be a row phases 25 and 26
    /// silently skip.
    pub fn put(&self, project_id: &str, row: &StoredEmbedding) -> AuraResult<()> {
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let project = project_id.to_string();
        let photo = row.embedding.image_id.to_db();
        let vector = encode_vector(&row.embedding.vec);
        let hash = row.embedding.dhash;
        let model_ver = i64::from(row.embedding.model_ver);
        let preprocess_ver = i64::from(row.preprocess_ver);
        let tier = i64::from(row.pixel_tier);
        let source = row.pixel_source;
        let histogram = row.descriptors.hsv_hist.to_vec();
        let luma = row.descriptors.luma;
        let edge_energy = f64::from(row.descriptors.edge_energy);
        let edge_p90 = f64::from(row.descriptors.edge_p90);
        let palette = encode_palette(&row.descriptors.palette);
        let descriptor_ver = i64::from(row.descriptors.descriptor_ver);

        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT OR REPLACE INTO embeddings
                   (photo_id, project_id, vec, dhash, model_ver, preprocess_ver,
                    pixel_tier, pixel_source, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    photo,
                    project,
                    vector,
                    hash.cast_signed(),
                    model_ver,
                    preprocess_ver,
                    tier,
                    source,
                    now
                ],
            )
            .map_err(|e| statement_failed("could not store an embedding", &e))?;
            tx.execute(
                "INSERT OR REPLACE INTO descriptors
                   (photo_id, project_id, descriptor_ver, hsv_hist, luma_mean, luma_p1,
                    luma_p50, luma_p99, clip_lo, clip_hi, edge_energy, edge_p90, palette,
                    created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    photo,
                    project,
                    descriptor_ver,
                    histogram,
                    f64::from(luma.mean),
                    f64::from(luma.p1),
                    f64::from(luma.p50),
                    f64::from(luma.p99),
                    f64::from(luma.clip_lo),
                    f64::from(luma.clip_hi),
                    edge_energy,
                    edge_p90,
                    palette,
                    now
                ],
            )
            .map_err(|e| statement_failed("could not store descriptors", &e))?;
            Ok(())
        })
    }

    /// Photographs in a project that have no vector at the current version.
    ///
    /// This is what makes acceptance criterion 5 - "importing another card embeds
    /// only the new files" - a query rather than a bookkeeping exercise, and what
    /// makes a killed run resume: the work left is always derivable from the
    /// catalog, never from a journal that could disagree with it.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(&self, project_id: &str, model_ver: u16) -> AuraResult<Vec<ImageId>> {
        let project = project_id.to_string();
        self.catalog.read(|conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       LEFT JOIN embeddings e ON e.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (e.photo_id IS NULL OR e.model_ver <> ?2)
                      ORDER BY p.timeline_time, p.photo_id",
                )
                .map_err(|e| statement_failed("could not list unembedded photographs", &e))?;
            let rows = statement
                .query_map(params![project, i64::from(model_ver)], |row| {
                    row.get::<_, String>(0)
                })
                .map_err(|e| statement_failed("could not list unembedded photographs", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let text = row.map_err(|e| statement_failed("bad photo id", &e))?;
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Load every vector in a project, with the metadata a filtered query needs.
    ///
    /// The camera handles are assigned here, from the project's camera ids sorted
    /// as text, so the same wedding produces the same handles on every machine.
    /// The labels are returned alongside so the index can answer
    /// `camera_handle("cam_...")`.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn load_entries(&self, project_id: &str) -> AuraResult<(Vec<IndexEntry>, Vec<String>)> {
        let project = project_id.to_string();
        self.catalog.read(move |conn| {
            let cameras = camera_handles(conn, &project)?;
            let mut statement = conn
                .prepare(
                    "SELECT e.photo_id, e.vec, e.dhash, e.model_ver,
                            d.hsv_hist, d.luma_mean, d.luma_p1, d.luma_p50, d.luma_p99,
                            d.clip_lo, d.clip_hi,
                            p.timeline_time, c.camera_id
                       FROM embeddings e
                       JOIN photo p            ON p.photo_id = e.photo_id
                       LEFT JOIN descriptors d ON d.photo_id = e.photo_id
                       LEFT JOIN camera c      ON c.project_id = p.project_id
                                              AND c.body_serial = COALESCE(p.camera_serial, '')
                      WHERE e.project_id = ?1
                      ORDER BY p.timeline_time, e.photo_id",
                )
                .map_err(|e| statement_failed("could not read embeddings", &e))?;

            let mut entries = Vec::new();
            let mut rows = statement
                .query(params![project])
                .map_err(|e| statement_failed("could not read embeddings", &e))?;
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read an embedding row", &e))?
            {
                let Some(entry) = read_entry(row, &cameras) else {
                    continue;
                };
                entries.push(entry);
            }
            Ok((entries, cameras))
        })
    }

    /// One photograph's stored row, or `None` when it has not been analysed.
    ///
    /// The interactive path: the debug panel asks about the frame under the cursor,
    /// and reading 1,623 bytes is cheaper than holding a project's worth of vectors
    /// to answer one question.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn load_one(
        &self,
        project_id: &str,
        photo: ImageId,
    ) -> AuraResult<Option<StoredEmbedding>> {
        let project = project_id.to_string();
        let key = photo.to_db();
        self.catalog.read(move |conn| {
            let found: Result<StoredEmbedding, rusqlite::Error> = conn.query_row(
                "SELECT e.vec, e.dhash, e.model_ver, e.preprocess_ver, e.pixel_tier,
                        e.pixel_source,
                        d.descriptor_ver, d.hsv_hist, d.luma_mean, d.luma_p1, d.luma_p50,
                        d.luma_p99, d.clip_lo, d.clip_hi, d.edge_energy, d.edge_p90, d.palette
                   FROM embeddings e
                   LEFT JOIN descriptors d ON d.photo_id = e.photo_id
                  WHERE e.project_id = ?1 AND e.photo_id = ?2",
                params![project, key],
                |row| read_stored(row, photo),
            );
            match found {
                Ok(stored) => Ok(Some(stored)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(statement_failed("could not read an embedding", &err)),
            }
        })
    }

    /// How many photographs a project has, and how many carry a vector.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the view cannot be read.
    pub fn coverage(&self, project_id: &str) -> AuraResult<(i64, i64)> {
        let project = project_id.to_string();
        self.catalog.read(move |conn| {
            let found: Result<(i64, i64), rusqlite::Error> = conn.query_row(
                "SELECT photos, embedded FROM v_embedding_coverage WHERE project_id = ?1",
                params![project],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );
            // A project with no photographs has no row in the view. Zero is the
            // truthful answer, not an error.
            Ok(found.unwrap_or((0, 0)))
        })
    }

    /// Model versions present in a project's rows, ascending.
    ///
    /// A project with more than one is mid-re-embed. The caller reports
    /// `AURA-ML-5015` and keeps going; nothing here decides policy.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn model_versions(&self, project_id: &str) -> AuraResult<Vec<u16>> {
        let project = project_id.to_string();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT DISTINCT model_ver FROM embeddings WHERE project_id = ?1
                      ORDER BY model_ver",
                )
                .map_err(|e| statement_failed("could not read embedding versions", &e))?;
            let rows = statement
                .query_map(params![project], |row| row.get::<_, i64>(0))
                .map_err(|e| statement_failed("could not read embedding versions", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let value = row.map_err(|e| statement_failed("bad version", &e))?;
                out.push(u16::try_from(value).unwrap_or(0));
            }
            Ok(out)
        })
    }

    /// Forget every vector produced by a superseded model version.
    ///
    /// The rollback switch: a prompt-free equivalent of purging the cloud cache
    /// after a prompt change. Returns how many rows went.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the delete fails.
    pub fn purge_version(&self, project_id: &str, model_ver: u16) -> AuraResult<usize> {
        let project = project_id.to_string();
        self.catalog.writer().transact(move |tx| {
            let removed = tx
                .execute(
                    "DELETE FROM descriptors WHERE photo_id IN
                       (SELECT photo_id FROM embeddings WHERE project_id = ?1 AND model_ver = ?2)",
                    params![project, i64::from(model_ver)],
                )
                .map_err(|e| statement_failed("could not purge descriptors", &e))?;
            tx.execute(
                "DELETE FROM embeddings WHERE project_id = ?1 AND model_ver = ?2",
                params![project, i64::from(model_ver)],
            )
            .map_err(|e| statement_failed("could not purge embeddings", &e))?;
            Ok(removed)
        })
    }
}

/// Read one joined row into a stored row.
///
/// A missing descriptors row is tolerated rather than fatal: the two are written
/// in one transaction, so it should not happen, and a vector that survived a
/// half-restored backup is still a usable vector.
fn read_stored(
    row: &rusqlite::Row<'_>,
    photo: ImageId,
) -> Result<StoredEmbedding, rusqlite::Error> {
    let vector: Vec<u8> = row.get(0)?;
    let dhash: i64 = row.get(1)?;
    let model_ver: i64 = row.get(2)?;
    let preprocess_ver: i64 = row.get(3)?;
    let pixel_tier: i64 = row.get(4)?;
    let pixel_source: String = row.get(5)?;

    let mut embedding = ImageEmbedding::zeroed(photo, u16::try_from(model_ver).unwrap_or(0));
    if decode_vector(&vector, &mut embedding.vec).is_none() {
        return Err(rusqlite::Error::IntegralValueOutOfRange(
            0,
            i64::try_from(vector.len()).unwrap_or(i64::MAX),
        ));
    }
    embedding.dhash = dhash as u64;

    let descriptor_ver: Option<i64> = row.get(6)?;
    let mut descriptors =
        Descriptors::empty(u16::try_from(descriptor_ver.unwrap_or(0)).unwrap_or(0));
    if let Some(bytes) = row.get::<_, Option<Vec<u8>>>(7)? {
        if bytes.len() == HSV_BINS {
            for (slot, value) in descriptors.hsv_hist.iter_mut().zip(bytes) {
                *slot = value;
            }
        }
    }
    let read = |index: usize| -> Result<f32, rusqlite::Error> {
        Ok(row.get::<_, Option<f64>>(index)?.unwrap_or(0.0) as f32)
    };
    descriptors.luma = LumaStats {
        mean: read(8)?,
        p1: read(9)?,
        p50: read(10)?,
        p99: read(11)?,
        clip_lo: read(12)?,
        clip_hi: read(13)?,
    };
    descriptors.edge_energy = read(14)?;
    descriptors.edge_p90 = read(15)?;
    if let Some(bytes) = row.get::<_, Option<Vec<u8>>>(16)? {
        for (slot, chunk) in descriptors.palette.iter_mut().zip(bytes.chunks_exact(3)) {
            if let Ok(colour) = <[u8; 3]>::try_from(chunk) {
                *slot = colour;
            }
        }
    }
    embedding.hsv_hist = descriptors.hsv_hist;
    embedding.luma = descriptors.luma;

    Ok(StoredEmbedding {
        embedding,
        descriptors,
        preprocess_ver: u16::try_from(preprocess_ver).unwrap_or(0),
        pixel_tier: u8::try_from(pixel_tier).unwrap_or(2),
        // The column is one of two known strings, so the borrow is resolved to a
        // static rather than leaked. An unknown value is reported as such instead
        // of being coerced into a lie about provenance.
        pixel_source: match pixel_source.as_str() {
            "embedded" => "embedded",
            "decoded" => "decoded",
            _ => "unknown",
        },
    })
}

/// Assign a dense handle to every camera in a project, ordered by catalog id.
fn camera_handles(conn: &Connection, project_id: &str) -> AuraResult<Vec<String>> {
    let mut statement = conn
        .prepare("SELECT camera_id FROM camera WHERE project_id = ?1 ORDER BY camera_id")
        .map_err(|e| statement_failed("could not read cameras", &e))?;
    let rows = statement
        .query_map(params![project_id], |row| row.get::<_, String>(0))
        .map_err(|e| statement_failed("could not read cameras", &e))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|e| statement_failed("bad camera id", &e))?);
    }
    Ok(out)
}

/// Turn one joined row into an index entry, or `None` when it cannot be trusted.
fn read_entry(row: &rusqlite::Row<'_>, cameras: &[String]) -> Option<IndexEntry> {
    let photo: String = row.get(0).ok()?;
    let image_id = PhotoId::from_db(&photo).ok()?;
    let vector: Vec<u8> = row.get(1).ok()?;
    let dhash: i64 = row.get(2).ok()?;
    let model_ver: i64 = row.get(3).ok()?;
    let histogram: Option<Vec<u8>> = row.get(4).ok();

    let mut embedding = ImageEmbedding::zeroed(image_id, u16::try_from(model_ver).unwrap_or(0));
    decode_vector(&vector, &mut embedding.vec)?;
    embedding.dhash = dhash as u64;
    if let Some(bytes) = histogram {
        if bytes.len() == HSV_BINS {
            for (slot, value) in embedding.hsv_hist.iter_mut().zip(bytes) {
                *slot = value;
            }
        }
    }
    embedding.luma = LumaStats {
        mean: row.get::<_, Option<f64>>(5).ok().flatten().unwrap_or(0.0) as f32,
        p1: row.get::<_, Option<f64>>(6).ok().flatten().unwrap_or(0.0) as f32,
        p50: row.get::<_, Option<f64>>(7).ok().flatten().unwrap_or(0.0) as f32,
        p99: row.get::<_, Option<f64>>(8).ok().flatten().unwrap_or(0.0) as f32,
        clip_lo: row.get::<_, Option<f64>>(9).ok().flatten().unwrap_or(0.0) as f32,
        clip_hi: row.get::<_, Option<f64>>(10).ok().flatten().unwrap_or(0.0) as f32,
    };

    let timeline: Option<String> = row.get(11).ok().flatten();
    let camera: Option<String> = row.get(12).ok().flatten();
    let meta = EntryMeta {
        timeline_ms: timeline.as_deref().and_then(parse_rfc3339_ms),
        camera: camera.and_then(|id| {
            cameras
                .iter()
                .position(|label| *label == id)
                .map(|position| crate::contract::index::CameraId(position as u32))
        }),
        scene: None,
    };

    Some(IndexEntry { embedding, meta })
}

/// RFC 3339 to milliseconds since the epoch.
///
/// The catalog stores every timestamp as sortable RFC 3339 UTC text, which is
/// right for a support engineer reading a hex dump and wrong for a time-window
/// filter that has to subtract. The conversion happens once, at load.
#[must_use]
pub fn parse_rfc3339_ms(text: &str) -> Option<i64> {
    use time::format_description::well_known::Rfc3339;
    use time::OffsetDateTime;

    let parsed = OffsetDateTime::parse(text, &Rfc3339).ok()?;
    let nanos = parsed.unix_timestamp_nanos();
    i64::try_from(nanos / 1_000_000).ok()
}

/// 512 halves to 1,024 little-endian bytes.
#[must_use]
pub fn encode_vector(vector: &[F16; EMBED_DIM]) -> Vec<u8> {
    let mut out = Vec::with_capacity(VECTOR_BYTES);
    for half in vector {
        out.extend_from_slice(&half.to_bits().to_le_bytes());
    }
    out
}

/// 1,024 little-endian bytes back to 512 halves. `None` on any other length: a
/// vector of the wrong width is a schema change nobody recorded.
pub fn decode_vector(bytes: &[u8], into: &mut [F16; EMBED_DIM]) -> Option<()> {
    if bytes.len() != VECTOR_BYTES {
        return None;
    }
    for (slot, pair) in into.iter_mut().zip(bytes.chunks_exact(2)) {
        let fixed: [u8; 2] = pair.try_into().ok()?;
        *slot = F16::from_bits(u16::from_le_bytes(fixed));
    }
    Some(())
}

/// Five RGB triples to fifteen bytes.
#[must_use]
pub fn encode_palette(palette: &[[u8; 3]; PALETTE_LEN]) -> Vec<u8> {
    let mut out = Vec::with_capacity(PALETTE_BYTES);
    for colour in palette {
        out.extend_from_slice(colour);
    }
    out
}
