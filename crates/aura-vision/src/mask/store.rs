//! Compression, and the rows migration 18 owns.
//!
//! Two jobs in one file because they are one decision. Section 6.3 sets a budget of 180 KB for
//! every class of one photograph, and the only way to meet it is for the storage *form* to be
//! a property of the class - which means the encoder and the schema have to agree about which
//! form each of the twenty is in, and putting them in two files is how they stop agreeing.
//!
//! # The two forms
//!
//! **Run length** for the sixteen classes whose boundary is a curve rather than a gradient.
//! The bitmap is row major over the analysis grid, the runs alternate starting with `off`, and
//! each length is an unsigned LEB128. The cost scales with the region's *perimeter* rather than
//! its area, which is why sixteen of them fit in a third of the budget.
//!
//! **Eight-bit alpha at a quarter of the analysis grid** for subject, hair, face and skin - the
//! four whose boundary is the point. At [`crate::mask::ANALYSIS_EDGE`] that is a 192 px long
//! edge, 24 KB each.
//!
//! # The budget is a guarantee, so the encoder enforces it
//!
//! A pathological region - a lace veil, a crowd behind a bokeh gate - can produce a run length
//! with more runs than a raw bitmap has bytes. Rather than let one frame blow the budget, the
//! encoder halves the bitmap and re-encodes, once, and returns `AURA-ML-5080` so the coarsening
//! is a recorded fact rather than a mystery at 100 % zoom. ADR-0037 decision 5.

use std::collections::BTreeMap;
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::contract::error::{AuraError, AuraResult};
use aura_core::contract::ids::{IdentityId, MaskId};
use aura_core::errors::db::statement_failed;
use aura_core::{PhotoId, ProjectId};
use rusqlite::params;

use crate::contract::mask::{
    EdgeQuality, Mask, MaskKind, MaskOutline, MaskPayload, MaskReason, Storage, AGGRESSIVE_FLOOR,
    PAYLOAD_BUDGET_BYTES,
};
use crate::mask::algebra::Plane;
use crate::mask::quality;
use crate::mask::{errors, ALPHA_DIVISOR, ANALYSIS_VER, MODEL_VER};

/// The largest a single run-length payload may be before it is coarsened.
///
/// A twentieth of the whole-image budget. Sixteen run-length classes at this ceiling is 144 KB,
/// which leaves the four alpha planes inside 180 KB only if they are also bounded - and they
/// are, by construction, because a quarter-resolution plane has a fixed size.
pub const RLE_MAX_BYTES: usize = PAYLOAD_BUDGET_BYTES / 20;

// ---------------------------------------------------------------------------
// The codec
// ---------------------------------------------------------------------------

/// Encode a plane in the form its kind is stored in.
///
/// Returns the payload and, when the region had to be coarsened to fit, the warning that says
/// so.
#[must_use]
pub fn encode(kind: MaskKind, plane: &Plane) -> (MaskPayload, Option<AuraError>) {
    match kind.stored_as() {
        Storage::Alpha => (encode_alpha(plane), None),
        Storage::Rle => {
            let payload = encode_rle(plane);
            if payload.byte_len() <= RLE_MAX_BYTES {
                return (payload, None);
            }
            let half = plane.resize_nearest((plane.w / 2).max(1), (plane.h / 2).max(1));
            let coarse = encode_rle(&half);
            let note =
                errors::payload_coarsened(kind.as_str(), payload.byte_len(), coarse.byte_len());
            (coarse, Some(note))
        }
    }
}

/// Encode a plane as eight-bit alpha at a quarter of its own resolution.
#[must_use]
pub fn encode_alpha(plane: &Plane) -> MaskPayload {
    let w = (plane.w / ALPHA_DIVISOR).max(1);
    let h = (plane.h / ALPHA_DIVISOR).max(1);
    // Bilinear on the way down, because the alpha values have already been decided by the
    // matting and averaging them is what a quarter-resolution store means. Nearest would throw
    // away three quarters of a matted boundary and keep the fourth, which is a jagged edge
    // wearing a soft edge's storage cost.
    let small = plane.resize_bilinear(w, h);
    let encoded = small
        .a
        .iter()
        .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    MaskPayload::Alpha8 {
        w,
        h,
        alpha: encoded,
    }
}

/// Encode a plane as a run length over a thresholded bitmap.
#[must_use]
pub fn encode_rle(plane: &Plane) -> MaskPayload {
    let mut runs = Vec::new();
    let mut run: u64 = 0;
    let mut value = false;
    for sample in &plane.a {
        let bit = *sample >= 0.5;
        if bit == value {
            run += 1;
        } else {
            write_varint(&mut runs, run);
            value = bit;
            run = 1;
        }
    }
    write_varint(&mut runs, run);
    MaskPayload::Rle {
        w: plane.w,
        h: plane.h,
        runs,
    }
}

/// Decode a payload back into a working plane.
#[must_use]
pub fn decode(payload: &MaskPayload) -> Plane {
    match payload {
        MaskPayload::Alpha8 { w, h, alpha } => Plane::from_vec(
            *w,
            *h,
            alpha.iter().map(|b| f32::from(*b) / 255.0).collect(),
        ),
        MaskPayload::Rle { w, h, runs } => {
            let mut plane = Plane::zeros(*w, *h);
            let mut cursor = 0_usize;
            let mut index = 0_usize;
            let mut value = false;
            let total = (*w as usize).saturating_mul(*h as usize);
            while cursor < runs.len() && index < total {
                let Some((run, next)) = read_varint(runs, cursor) else {
                    break;
                };
                cursor = next;
                if value {
                    let end = (index + run as usize).min(total);
                    for slot in plane.a.get_mut(index..end).unwrap_or_default() {
                        *slot = 1.0;
                    }
                }
                index += run as usize;
                value = !value;
            }
            plane
        }
    }
}

fn write_varint(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return;
        }
        out.push(byte | 0x80);
    }
}

fn read_varint(runs: &[u8], mut cursor: usize) -> Option<(u64, usize)> {
    let mut value = 0_u64;
    let mut shift = 0_u32;
    loop {
        let byte = *runs.get(cursor)?;
        cursor += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some((value, cursor));
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

// ---------------------------------------------------------------------------
// The store
// ---------------------------------------------------------------------------

/// The rows migration 18 owns.
///
/// **This is the first storage `aura-vision` has ever had.** Phase 06 wrote that this crate
/// could not write a face template because it had no catalog dependency; that is no longer
/// structurally true, and `tests/no_template_writes.rs` is the grep-as-a-test that replaces it.
/// ADR-0037 decision 11.
#[derive(Debug, Clone)]
pub struct MaskStore {
    catalog: Arc<Catalog>,
}

impl MaskStore {
    /// Open the store over a catalog.
    #[must_use]
    pub fn new(catalog: Arc<Catalog>) -> Self {
        Self { catalog }
    }

    /// Every mask of one photograph, in [`crate::contract::mask::ALL_KINDS`] order.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be read.
    pub fn masks(&self, image: PhotoId) -> AuraResult<Vec<Mask>> {
        let key = image.to_db();
        self.catalog.read(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT mask_id, kind, identity_id, form, payload_w, payload_h, payload, \
                            feather, confidence, edge_quality, edge, reasons, user_edited, \
                            model_ver \
                       FROM masks WHERE image_id = ?1 \
                      ORDER BY kind_ix, identity_id IS NULL DESC, identity_id, mask_id",
                )
                .map_err(|e| statement_failed("could not prepare a mask read", &e))?;
            let rows = stmt
                .query_map(params![key], |row| Ok(row_to_mask(row, image)))
                .map_err(|e| statement_failed("could not read masks", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let row = row.map_err(|e| statement_failed("could not read a mask row", &e))?;
                if let Some(mask) = row {
                    out.push(mask);
                }
            }
            Ok(out)
        })
    }

    /// One mask by id.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be read.
    pub fn mask(&self, id: MaskId) -> AuraResult<Option<Mask>> {
        let key = id.to_db();
        self.catalog.read(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT mask_id, kind, identity_id, form, payload_w, payload_h, payload, \
                            feather, confidence, edge_quality, edge, reasons, user_edited, \
                            model_ver, image_id \
                       FROM masks WHERE mask_id = ?1",
                )
                .map_err(|e| statement_failed("could not prepare a mask read", &e))?;
            let mut rows = stmt
                .query(params![key])
                .map_err(|e| statement_failed("could not read a mask", &e))?;
            let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a mask row", &e))?
            else {
                return Ok(None);
            };
            let image_text: String = row
                .get(14)
                .map_err(|e| statement_failed("a mask row had no image", &e))?;
            let Ok(image) = PhotoId::from_db(&image_text) else {
                return Ok(None);
            };
            Ok(row_to_mask(row, image))
        })
    }

    /// Write a photograph's masks, replacing whatever automation wrote before.
    ///
    /// **A mask a photographer edited is not touched, and it takes two statements to say so.**
    ///
    /// The first is `user_edited = 0` inside the `DELETE`'s own `WHERE`, exactly as
    /// `moments.user_locked` is in phase 08 and `identities.user_locked` is in phase 06: the
    /// check lives in the statement that would overwrite the row rather than in the code that
    /// calls it, so no path through this module can skip it.
    ///
    /// The second is the one that is easy to miss and was missed once. `masks` has a
    /// `UNIQUE (image_id, kind, identity_id)`, and an `INSERT OR REPLACE` **deletes the row it
    /// conflicts with** - so a regeneration that survived the `DELETE` would have destroyed the
    /// photographer's mask on the way back in, through a constraint rather than through a
    /// statement anybody was looking at. What stops it is reading the edited coordinates first
    /// and skipping them: a class a photographer has drawn is not re-measured at all.
    ///
    /// ADR-0037 decision 7.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the rows cannot be written.
    pub fn put(&self, image: PhotoId, masks: &[Mask]) -> AuraResult<usize> {
        let key = image.to_db();
        let rows: Vec<MaskRow> = masks
            .iter()
            .filter(|m| !m.user_edited)
            .map(|m| MaskRow::of(image, m))
            .collect();
        self.catalog.writer().transact(move |tx| {
            // Which coordinates the photographer owns. Read inside the same transaction as the
            // write, so a hand edit landing between the two cannot be lost.
            let mut owned: std::collections::BTreeSet<(String, Option<String>)> =
                std::collections::BTreeSet::new();
            {
                let mut stmt = tx
                    .prepare(
                        "SELECT kind, identity_id FROM masks \
                          WHERE image_id = ?1 AND user_edited = 1",
                    )
                    .map_err(|e| statement_failed("could not read edited masks", &e))?;
                let found = stmt
                    .query_map(params![key], |row| {
                        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                    })
                    .map_err(|e| statement_failed("could not read edited masks", &e))?;
                for entry in found {
                    owned.insert(
                        entry.map_err(|e| statement_failed("could not read an edited mask", &e))?,
                    );
                }
            }

            tx.execute(
                "DELETE FROM masks WHERE image_id = ?1 AND user_edited = 0",
                params![key],
            )
            .map_err(|e| statement_failed("could not clear stale masks", &e))?;

            let mut written = 0_usize;
            {
                let mut stmt = tx
                    .prepare(INSERT_SQL)
                    .map_err(|e| statement_failed("could not prepare a mask write", &e))?;
                for row in &rows {
                    if owned.contains(&(row.kind.clone(), row.identity.clone())) {
                        continue;
                    }
                    row.execute(&mut stmt, false)
                        .map_err(|e| statement_failed("could not store a mask", &e))?;
                    written += 1;
                }
            }
            Ok(written)
        })
    }

    /// Write one mask a photographer edited.
    ///
    /// The one path that sets `user_edited`, and there is no argument that clears it. Clearing
    /// it is [`MaskStore::regenerate`], which is a separate deliberate act.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be written.
    pub fn save_edit(&self, mask: &Mask) -> AuraResult<()> {
        let row = MaskRow::of(mask.image_id, mask);
        self.catalog.writer().transact(move |tx| {
            let mut stmt = tx
                .prepare(INSERT_SQL)
                .map_err(|e| statement_failed("could not prepare a mask write", &e))?;
            row.execute(&mut stmt, true)
                .map_err(|e| statement_failed("could not store an edited mask", &e))?;
            Ok(())
        })
    }

    /// Drop a photographer's edit so the next pass regenerates the mask.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the row cannot be deleted.
    pub fn regenerate(&self, id: MaskId) -> AuraResult<bool> {
        let key = id.to_db();
        self.catalog.writer().transact(move |tx| {
            let n = tx
                .execute("DELETE FROM masks WHERE mask_id = ?1", params![key])
                .map_err(|e| statement_failed("could not drop a mask", &e))?;
            Ok(n > 0)
        })
    }

    /// Which selected frames still need masks at the current versions.
    ///
    /// Invariant 5 as a query rather than as a journal: kill the pass at 60 % and the next run
    /// re-computes nothing it already computed. The `LEFT JOIN` is what makes an out-of-date
    /// frame indistinguishable from an un-analysed one, which is what a resume wants and what
    /// a model bump also wants.
    ///
    /// `selection` is phase 12's gallery table - the keepers, and only the keepers - which is
    /// what makes this the lazy policy section 6.3 asks for rather than a project-wide pass.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(&self, project: ProjectId, limit: usize) -> AuraResult<Vec<PhotoId>> {
        let key = project.to_db();
        self.catalog.read(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT s.photo_id FROM selection s \
                       LEFT JOIN masks m ON m.image_id = s.photo_id \
                                        AND m.model_ver = ?2 AND m.analysis_ver = ?3 \
                      WHERE s.project_id = ?1 AND m.mask_id IS NULL \
                      GROUP BY s.photo_id ORDER BY s.timeline_ts, s.photo_id LIMIT ?4",
                )
                .map_err(|e| statement_failed("could not prepare the pending query", &e))?;
            let rows = stmt
                .query_map(
                    params![
                        key,
                        i64::from(MODEL_VER),
                        i64::from(ANALYSIS_VER),
                        i64::try_from(limit).unwrap_or(i64::MAX)
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|e| statement_failed("could not read pending frames", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let text = row.map_err(|e| statement_failed("could not read a photo id", &e))?;
                if let Ok(id) = PhotoId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// What the panel's header shows.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn outline(&self, project: ProjectId) -> AuraResult<MaskOutline> {
        let key = project.to_db();
        let floor = f64::from(AGGRESSIVE_FLOOR * AGGRESSIVE_FLOOR);
        self.catalog.read(move |conn| {
            let selected: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM selection WHERE project_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|e| statement_failed("could not count selected frames", &e))?;
            let row = conn
                .query_row(
                    "SELECT COUNT(DISTINCT m.image_id), COUNT(m.mask_id), \
                            COALESCE(SUM(m.user_edited), 0), \
                            COALESCE(SUM(CASE WHEN m.bytes > 0 \
                                  AND (m.confidence * m.edge_quality) < ?2 \
                                 THEN 1 ELSE 0 END), 0), \
                            COALESCE(AVG(m.confidence), 0.0), \
                            COALESCE(AVG(m.edge_quality), 0.0), \
                            COALESCE(SUM(m.bytes), 0) \
                       FROM masks m JOIN photo p ON p.photo_id = m.image_id \
                      WHERE p.project_id = ?1",
                    params![key, floor],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                            row.get::<_, f64>(4)?,
                            row.get::<_, f64>(5)?,
                            row.get::<_, i64>(6)?,
                        ))
                    },
                )
                .map_err(|e| statement_failed("could not summarise masks", &e))?;

            Ok(MaskOutline {
                selected: selected.max(0) as u64,
                masked: row.0.max(0) as u64,
                masks: row.1.max(0) as u64,
                user_edited: row.2.max(0) as u64,
                low_quality: row.3.max(0) as u64,
                mean_confidence: row.4 as f32,
                mean_edge_quality: row.5 as f32,
                payload_bytes: row.6.max(0) as u64,
                model_ver: MODEL_VER,
                analysis_ver: ANALYSIS_VER,
                head_trained: crate::mask::segment::SEG_HEAD_TRAINED,
            })
        })
    }

    /// Total stored bytes for one photograph. What the budget test measures.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn bytes_for(&self, image: PhotoId) -> AuraResult<usize> {
        let key = image.to_db();
        self.catalog.read(|conn| {
            let bytes: i64 = conn
                .query_row(
                    "SELECT COALESCE(SUM(bytes), 0) FROM masks WHERE image_id = ?1",
                    params![key],
                    |row| row.get(0),
                )
                .map_err(|e| statement_failed("could not measure mask storage", &e))?;
            Ok(usize::try_from(bytes.max(0)).unwrap_or(0))
        })
    }
}

/// The one INSERT, written once so the edited and the automatic path cannot drift.
const INSERT_SQL: &str = "INSERT OR REPLACE INTO masks \
     (mask_id, image_id, kind, kind_ix, identity_id, form, payload_w, payload_h, payload, \
      feather, confidence, edge_quality, edge, reasons, user_edited, model_ver, analysis_ver, \
      bytes) \
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)";

/// One row's worth of owned values.
///
/// Owned rather than borrowed because `Writer::transact` runs the closure on the writer
/// thread, which needs `Send + 'static` - so a `&Mask` cannot cross into it.
#[derive(Debug, Clone)]
struct MaskRow {
    mask_id: String,
    image_id: String,
    kind: String,
    kind_ix: i64,
    identity: Option<String>,
    form: &'static str,
    w: i64,
    h: i64,
    payload: Vec<u8>,
    feather: f64,
    confidence: f64,
    edge_quality: f64,
    edge: &'static str,
    reasons: String,
    model_ver: i64,
}

impl MaskRow {
    fn of(image: PhotoId, mask: &Mask) -> Self {
        let (form, w, h, bytes) = payload_columns(&mask.payload);
        Self {
            mask_id: mask.id.to_db(),
            image_id: image.to_db(),
            kind: mask.kind.as_str().to_string(),
            kind_ix: kind_index(mask.kind),
            identity: mask.identity.map(|id| id.to_db()),
            form,
            w,
            h,
            payload: bytes.to_vec(),
            feather: f64::from(mask.feather),
            confidence: f64::from(mask.confidence),
            edge_quality: f64::from(mask.edge_quality),
            edge: mask.edge.as_str(),
            reasons: reasons_text(&mask.reasons),
            model_ver: i64::from(mask.model_ver),
        }
    }

    /// Bind and run. The parameter list is built inside the call rather than returned,
    /// because three of the eighteen values are temporaries and a `Params` that borrowed them
    /// would outlive them.
    fn execute(
        &self,
        stmt: &mut rusqlite::Statement<'_>,
        user_edited: bool,
    ) -> rusqlite::Result<usize> {
        let edited = i64::from(user_edited);
        let analysis = i64::from(ANALYSIS_VER);
        let bytes = i64::try_from(self.payload.len()).unwrap_or(i64::MAX);
        stmt.execute(params![
            self.mask_id,
            self.image_id,
            self.kind,
            self.kind_ix,
            self.identity,
            self.form,
            self.w,
            self.h,
            self.payload,
            self.feather,
            self.confidence,
            self.edge_quality,
            self.edge,
            self.reasons,
            edited,
            self.model_ver,
            analysis,
            bytes,
        ])
    }
}

/// The `kind_ix` column: a kind's position in the frozen iteration order.
///
/// Stored rather than derived in SQL, because the order is a contract and an `ORDER BY kind`
/// would sort it alphabetically - which puts `background` before `face` and makes the panel's
/// list order depend on English.
#[must_use]
pub fn kind_index(kind: MaskKind) -> i64 {
    crate::contract::mask::ALL_KINDS
        .iter()
        .position(|k| *k == kind)
        .and_then(|index| i64::try_from(index).ok())
        .unwrap_or(i64::MAX)
}

fn payload_columns(payload: &MaskPayload) -> (&'static str, i64, i64, &[u8]) {
    match payload {
        MaskPayload::Rle { w, h, runs } => ("rle", i64::from(*w), i64::from(*h), runs.as_slice()),
        MaskPayload::Alpha8 { w, h, alpha } => {
            ("alpha8", i64::from(*w), i64::from(*h), alpha.as_slice())
        }
    }
}

fn reasons_text(reasons: &[MaskReason]) -> String {
    reasons
        .iter()
        .map(|r| r.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// Parse the stored reason list.
///
/// **A code this build does not know is dropped rather than guessed.** A stored reason from a
/// newer build is a sentence this build cannot render, and rendering an unknown code as
/// "unknown" in a panel is worse than one fewer reason.
#[must_use]
pub fn parse_reasons(text: &str) -> Vec<MaskReason> {
    text.split(',')
        .filter(|s| !s.is_empty())
        .filter_map(MaskReason::parse)
        .collect()
}

fn row_to_mask(row: &rusqlite::Row<'_>, image: PhotoId) -> Option<Mask> {
    let id_text: String = row.get(0).ok()?;
    let kind_text: String = row.get(1).ok()?;
    let identity_text: Option<String> = row.get(2).ok()?;
    let form: String = row.get(3).ok()?;
    let w: i64 = row.get(4).ok()?;
    let h: i64 = row.get(5).ok()?;
    let bytes: Vec<u8> = row.get(6).ok()?;
    let feather: f64 = row.get(7).ok()?;
    let confidence: f64 = row.get(8).ok()?;
    let edge_quality: f64 = row.get(9).ok()?;
    let edge_text: String = row.get(10).ok()?;
    let reasons_raw: String = row.get(11).ok()?;
    let user_edited: i64 = row.get(12).ok()?;
    let model_ver: i64 = row.get(13).ok()?;

    let payload = match form.as_str() {
        "alpha8" => MaskPayload::Alpha8 {
            w: w.max(0) as u32,
            h: h.max(0) as u32,
            alpha: bytes,
        },
        _ => MaskPayload::Rle {
            w: w.max(0) as u32,
            h: h.max(0) as u32,
            runs: bytes,
        },
    };
    let edge = match edge_text.as_str() {
        "matted" => EdgeQuality::Matted,
        "soft" => EdgeQuality::Soft,
        "binary" => EdgeQuality::Binary,
        _ => EdgeQuality::Unknown,
    };
    let mut reasons = parse_reasons(&reasons_raw);
    if reasons.is_empty() {
        reasons.push(MaskReason::HeadUntrained);
    }
    Some(Mask {
        id: MaskId::from_db(&id_text).ok()?,
        image_id: image,
        kind: MaskKind::parse(&kind_text)?,
        identity: identity_text
            .as_deref()
            .and_then(|t| IdentityId::from_db(t).ok()),
        payload,
        feather: feather as f32,
        confidence: confidence as f32,
        edge_quality: edge_quality as f32,
        edge,
        reasons,
        user_edited: user_edited != 0,
        model_ver: model_ver.max(0) as u16,
    })
}

/// Bytes per class of one photograph, for the budget report.
#[must_use]
pub fn bytes_by_kind(masks: &[Mask]) -> BTreeMap<&'static str, usize> {
    let mut out = BTreeMap::new();
    for mask in masks {
        *out.entry(mask.kind.as_str()).or_insert(0) += mask.byte_len();
    }
    out
}

/// The mean edge quality of a set, for the outline. Re-exported so a caller reads one name.
#[must_use]
pub fn mean_edge_quality(masks: &[Mask]) -> f32 {
    quality::mean_edge_quality(masks)
}

#[cfg(test)]
mod tests {
    // The panic family is how a test asserts, and a mask test compares alphas that are exactly
    // zero or exactly one by construction - a painted fixture has no rounding to be tolerant of.
    #![allow(
        clippy::float_cmp,
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants,
        clippy::uninlined_format_args
    )]
    use super::*;

    fn disc(w: u32, h: u32, r: f32) -> Plane {
        let mut p = Plane::zeros(w, h);
        let cx = w as f32 / 2.0;
        let cy = h as f32 / 2.0;
        for y in 0..h {
            for x in 0..w {
                if (x as f32 - cx).hypot(y as f32 - cy) <= r {
                    p.set(i64::from(x), i64::from(y), 1.0);
                }
            }
        }
        p
    }

    #[test]
    fn a_run_length_round_trips_exactly() {
        let plane = disc(128, 96, 30.0);
        let decoded = decode(&encode_rle(&plane));
        assert_eq!(decoded, plane);
    }

    #[test]
    fn an_empty_region_round_trips() {
        let plane = Plane::zeros(64, 64);
        assert_eq!(decode(&encode_rle(&plane)), plane);
    }

    #[test]
    fn a_full_region_round_trips() {
        let plane = Plane::ones(64, 64);
        assert_eq!(decode(&encode_rle(&plane)), plane);
    }

    #[test]
    fn a_run_length_costs_perimeter_rather_than_area() {
        // The claim the budget rests on: doubling the area of a disc does not double its
        // payload.
        let small = encode_rle(&disc(256, 256, 30.0)).byte_len();
        let large = encode_rle(&disc(256, 256, 60.0)).byte_len();
        assert!(
            large < small * 2,
            "small {small} bytes, large {large} bytes"
        );
    }

    #[test]
    fn an_alpha_payload_is_a_quarter_of_the_grid() {
        let payload = encode_alpha(&disc(768, 512, 100.0));
        assert_eq!(payload.dimensions(), (192, 128));
        assert_eq!(payload.byte_len(), 192 * 128);
    }

    #[test]
    fn an_alpha_payload_keeps_partial_alpha() {
        let mut plane = Plane::zeros(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                plane.set(i64::from(x), i64::from(y), 0.5);
            }
        }
        let decoded = decode(&encode_alpha(&plane));
        assert!((decoded.at(4, 4) - 0.5).abs() < 0.01);
    }

    #[test]
    fn a_pathological_region_is_coarsened_rather_than_allowed_to_blow_the_budget() {
        // Every other pixel on, which is the worst case for a run length.
        let mut plane = Plane::zeros(512, 512);
        for y in 0..512 {
            for x in 0..512 {
                if (x + y) % 2 == 0 {
                    plane.set(i64::from(x), i64::from(y), 1.0);
                }
            }
        }
        let (payload, note) = encode(MaskKind::Sky, &plane);
        assert!(note.is_some(), "no coarsening warning");
        assert!(payload.byte_len() < 512 * 512);
        assert_eq!(note.map(|e| e.code.0), Some("AURA-ML-5080"));
    }

    #[test]
    fn the_kind_index_follows_the_frozen_order_rather_than_the_alphabet() {
        assert!(kind_index(MaskKind::Skin) < kind_index(MaskKind::Background));
    }

    #[test]
    fn an_unknown_reason_code_is_dropped_rather_than_guessed() {
        let parsed = parse_reasons("seeded_by_face,invented_in_2027,matted");
        assert_eq!(parsed, vec![MaskReason::SeededByFace, MaskReason::Matted]);
    }
}
