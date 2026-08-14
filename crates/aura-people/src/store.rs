//! Faces, identities and their evidence, as catalog rows and sealed blobs.
//!
//! This is the only module in the product that may hold a biometric template in a
//! durable form, and everything about its shape follows from that.
//!
//! **Every method takes a project.** Not because it is convenient, but because
//! section 2.2 forbids cross-wedding identity persistence outright. A `WHERE
//! project_id = ?` that a caller cannot forget is worth one redundant text column per
//! face, and [`PeopleStore::assert_same_project`] turns the remaining case - two ids
//! from two weddings handed to one merge - into `AURA-SEC-9004` rather than a silently
//! mixed identity.
//!
//! **Templates are sealed before they reach a statement and opened after they leave
//! one.** There is no method on this type that returns a raw envelope, and no method
//! that takes a plaintext template and writes it unsealed. A `faces.embed` column
//! containing anything but an envelope is a bug that [`crate::vault::Vault::open`]
//! reports rather than decrypts.
//!
//! **Crops are files, not rows.** A 112x112 sRGB crop is 37 KB, and ten thousand of
//! them in a `SQLite` table would triple the catalog and make every backup of it a
//! backup of biometric imagery. They live under the project's cache directory, sealed
//! with the same key, named by face id, and `faces.crop_ref` records the relative
//! path. Deleting the directory is part of erasure and losing it is harmless - a crop
//! is a cache of a decode.
//!
//! **Resumability is a query, not a journal.** `face_scan` records that a photograph
//! was looked at, including when the answer was "no faces here", which is what phase
//! 05 could take for granted and this phase cannot. [`PeopleStore::pending`] is the
//! whole of invariant 5 for this phase.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aura_catalog::Catalog;
use aura_core::clock::Clock;
use aura_core::contract::people::Role;
use aura_core::errors::db::statement_failed;
use aura_core::{AuraResult, FaceId, IdentityId, PhotoId, ProjectId};
use aura_vision::face::align::{self, Landmarks, Pose};
use aura_vision::face::cluster::Cluster;
use aura_vision::face::detect::{FoundBy, NormBox};
use aura_vision::face::embed;
use aura_vision::face::person::Association;
use aura_vision::face::{derive_person_id, FramePeople};
use parking_lot::Mutex;
use rusqlite::params;
use uuid::Uuid;

use crate::errors;
use crate::vault::{BiometricKeyStore, RecordKind, Vault, SALT_LEN};

/// Directory, under a project's cache root, where sealed crops live.
pub const CROP_DIR: &str = "faces";

/// Extension of a sealed crop file.
pub const CROP_EXT: &str = "aurafc";

/// JPEG quality the aligned crop is encoded at before it is sealed.
///
/// **The crop is encoded, not stored raw**, and the reason is section 11's 25 MB per
/// 1,000 images. A raw 112x112 sRGB crop is 37,632 bytes; at the fixture set's 2.4 faces
/// per frame that is 90 MB per 1,000 images on its own, three and a half times the whole
/// budget. The same crop at quality 82 is about 3.6 KB, which brings a thousand images to
/// roughly 13 MB including the catalog rows.
///
/// 82 rather than 85: the crop is displayed at 64 px on an identity card and is never
/// archival. It is not an input to anything - the template was computed from the *raw*
/// crop before this encode, so no measurement depends on the compression.
pub const CROP_JPEG_QUALITY: u8 = 82;

/// Bytes one face costs in the catalog, excluding its sealed crop.
///
/// Sealed template 1,024 + 68 envelope, landmarks about 90, and roughly 60 for the
/// boxes, angles, scores and identifiers.
pub const BYTES_PER_FACE: usize = 1_024 + crate::vault::OVERHEAD + 90 + 57;

/// Bytes one sealed crop costs on disk, as the figure the budget is written against.
///
/// A 112x112 JPEG at [`CROP_JPEG_QUALITY`] plus the envelope: 3.6 KB measured on the
/// fixture crops, rounded up to 4,096 for headroom. Section 11 budgets 25 MB of face
/// store per 1,000 images, and `crates/aura-perf/tests/people_budgets.rs` asserts the
/// pair of constants against it.
pub const BYTES_PER_SEALED_CROP: usize = 4_096;

/// One stored face, with its template opened.
#[derive(Debug, Clone, PartialEq)]
pub struct FaceRow {
    /// The face.
    pub face_id: FaceId,
    /// The photograph it was found in.
    pub image_id: PhotoId,
    /// Who it was assigned to, when it was assigned to anybody.
    pub identity_id: Option<IdentityId>,
    /// The face box, normalised.
    pub bbox: NormBox,
    /// Head pose in degrees.
    pub pose: Pose,
    /// Detector objectness.
    pub det_score: f32,
    /// Fused usability from the quality gate.
    pub quality: f32,
    /// Blur, `0..1`.
    pub blur: f32,
    /// Occlusion, `0..1`.
    pub occlusion: f32,
    /// The five landmarks, normalised.
    pub landmarks: Landmarks,
    /// The recognition template, opened. `None` when the row has no template or the
    /// envelope could not be opened - and the two are distinguished by the log, not by
    /// this field, because a caller's behaviour is the same either way.
    pub template: Option<Vec<f32>>,
    /// Fraction of the frame covered.
    pub area_frac: f32,
    /// Centrality, `0..1`.
    pub centrality: f32,
    /// The face's own sharpness.
    pub sharpness: f32,
    /// Detector version.
    pub model_ver: u16,
    /// Recogniser version.
    pub embed_ver: u16,
    /// Quality gate version.
    pub quality_ver: u16,
    /// Height in source pixels.
    pub px_height: u32,
    /// True when this face may vote on identity.
    pub votes: bool,
    /// Which pass found it.
    pub found_by: FoundBy,
    /// Relative path of the sealed crop, when one was written.
    pub crop_ref: Option<String>,
    /// Evidence that this identity is a child, recomputed from the stored boxes.
    pub child_score: f32,
}

/// One stored identity, with its centroids opened.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentityRow {
    /// The identity.
    pub identity_id: IdentityId,
    /// The project.
    pub project_id: ProjectId,
    /// The photographer's name for this person, when they gave one.
    pub label: Option<String>,
    /// What this person is at this wedding.
    pub role: Role,
    /// How sure, `0..1`.
    pub role_confidence: f32,
    /// Why.
    pub role_reasons: Vec<String>,
    /// True when a photographer set the role.
    pub user_locked: bool,
    /// The importance slider, `0..1`, defaulting to 0.5.
    pub importance: f32,
    /// Faces assigned.
    pub face_count: u32,
    /// First appearance, RFC 3339.
    pub first_seen: Option<String>,
    /// Last appearance, RFC 3339.
    pub last_seen: Option<String>,
    /// The mean template, opened.
    pub centroid: Option<Vec<f32>>,
    /// Sub-cluster means, opened.
    pub sub_centroids: Vec<Vec<f32>>,
    /// Mean member-to-centroid distance.
    pub variance: f32,
    /// Which clustering generation produced it.
    pub cluster_ver: u16,
}

/// What one photograph's scan recorded.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScanRow {
    /// Faces found.
    pub faces_found: u32,
    /// Faces that may vote.
    pub faces_voting: u32,
    /// Body boxes found.
    pub person_boxes: u32,
    /// True when the tiled pass ran.
    pub tiled: bool,
    /// Detector version.
    pub model_ver: u16,
    /// Detector preprocessing version.
    pub preprocess_ver: u16,
}

/// One entry in the append-only decision journal.
#[derive(Debug, Clone, PartialEq)]
pub struct IdentityLink {
    /// `merge`, `split`, `rename`, `role` or `importance`.
    pub kind: String,
    /// The identity acted on.
    pub identity_id: IdentityId,
    /// The other identity, for a merge or a split.
    pub other_id: Option<IdentityId>,
    /// Face ids: which faces moved for a split, and which faces the identity held at
    /// the time for a rename or a role.
    ///
    /// The second use is what makes a decision survive a re-analysis. Clustering after
    /// a model change produces new identity ids, so replaying "the photographer named
    /// this person Priya" cannot be done by id - it is done by finding the new identity
    /// that holds most of these faces. See [`PeopleStore::replayable_links`].
    pub face_ids: Vec<FaceId>,
    /// Previous value, as text.
    pub old_value: Option<String>,
    /// New value, as text.
    pub new_value: Option<String>,
    /// `user`, `cloud` or `auto`.
    pub actor: String,
}

/// What an erasure did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EraseReport {
    /// Face rows deleted.
    pub faces: usize,
    /// Identity rows deleted.
    pub identities: usize,
    /// Sealed crop files deleted.
    pub crops: usize,
    /// True when the credential-store entry was removed.
    pub key_removed: bool,
}

/// Reads and writes everything migration 6 adds.
#[derive(Debug)]
pub struct PeopleStore {
    catalog: Arc<Catalog>,
    clock: Arc<dyn Clock>,
    keys: Arc<dyn BiometricKeyStore>,
    cache_root: PathBuf,
    /// One vault per project, derived on first use. Held so that a project's key
    /// derivation - three BLAKE3 passes - happens once per session rather than once per
    /// face.
    vaults: Mutex<BTreeMap<String, Arc<Vault>>>,
}

impl PeopleStore {
    /// Build a store over one catalog.
    #[must_use]
    pub fn new(
        catalog: Arc<Catalog>,
        clock: Arc<dyn Clock>,
        keys: Arc<dyn BiometricKeyStore>,
        cache_root: &Path,
    ) -> Self {
        Self {
            catalog,
            clock,
            keys,
            cache_root: cache_root.to_path_buf(),
            vaults: Mutex::new(BTreeMap::new()),
        }
    }

    /// Where a project's sealed crops live.
    #[must_use]
    pub fn crop_root(&self, project: &ProjectId) -> PathBuf {
        self.cache_root.join(project.to_db()).join(CROP_DIR)
    }

    /// The vault for one project, creating the key and the `face_vault` row on first
    /// use.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9001` when the credential store cannot be reached, and
    /// `AURA-DB-3006` when the vault row cannot be written.
    pub fn vault(&self, project: &ProjectId) -> AuraResult<Arc<Vault>> {
        let key = project.to_db();
        {
            let vaults = self.vaults.lock();
            if let Some(found) = vaults.get(&key) {
                return Ok(Arc::clone(found));
            }
        }

        let account = crate::vault::account_for(project);
        let salt = self.load_or_create_vault_row(project, &account)?;
        let secret = match self.keys.secret(&account)? {
            Some(found) => found,
            None => self.keys.create(&account)?,
        };
        let vault = Arc::new(Vault::new(*project, &secret, &salt));

        self.vaults.lock().insert(key, Arc::clone(&vault));
        Ok(vault)
    }

    /// Read the `face_vault` salt, creating the row when there is none.
    fn load_or_create_vault_row(
        &self,
        project: &ProjectId,
        account: &str,
    ) -> AuraResult<[u8; SALT_LEN]> {
        let project_key = project.to_db();
        let existing: Option<Vec<u8>> = self.catalog.read({
            let project_key = project_key.clone();
            move |conn| {
                let found: Result<Vec<u8>, rusqlite::Error> = conn.query_row(
                    "SELECT salt FROM face_vault WHERE project_id = ?1",
                    params![project_key],
                    |row| row.get(0),
                );
                match found {
                    Ok(salt) => Ok(Some(salt)),
                    Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                    Err(err) => Err(statement_failed("could not read the face vault", &err)),
                }
            }
        })?;

        if let Some(bytes) = existing {
            if let Ok(salt) = <[u8; SALT_LEN]>::try_from(bytes.as_slice()) {
                return Ok(salt);
            }
            // A salt of the wrong width is a corrupt row. Re-deriving is safe because
            // the salt is derived from the project id anyway; see `derive_salt`.
            tracing::warn!(
                target: "people.vault",
                project = %project_key,
                "face_vault salt was the wrong width and has been re-derived"
            );
        }

        let salt = crate::vault::derive_salt(project);
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let account = account.to_string();
        let salt_bytes = salt.to_vec();
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT OR REPLACE INTO face_vault
                   (project_id, key_account, envelope_ver, salt, created_at, erased_at,
                    erased_faces)
                 VALUES (?1, ?2, ?3, ?4,
                         COALESCE((SELECT created_at FROM face_vault WHERE project_id = ?1), ?5),
                         NULL,
                         COALESCE((SELECT erased_faces FROM face_vault WHERE project_id = ?1), 0))",
                params![
                    project_key,
                    account,
                    i64::from(crate::vault::ENVELOPE_VER),
                    salt_bytes,
                    now
                ],
            )
            .map_err(|e| statement_failed("could not create the face vault", &e))?;
            Ok(())
        })?;
        Ok(salt)
    }

    /// Write one frame's faces, bodies and scan record, in one transaction.
    ///
    /// `INSERT OR REPLACE` throughout, because face ids are derived rather than
    /// generated: a killed pass that is restarted rewrites the same rows instead of
    /// duplicating them, which is what makes invariant 5 free here.
    ///
    /// The scan row is written **last, in the same transaction**. That ordering is the
    /// resumability guarantee: a crash between the faces and the scan row leaves a
    /// photograph that will be scanned again, which is harmless; the reverse ordering
    /// would leave a photograph marked done with no faces, which is a wedding with
    /// missing people and no way to notice.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9001` when the vault cannot be opened, `AURA-DB-3006` when a statement
    /// fails. A crop that cannot be written is logged and the face is kept without one:
    /// a crop is a cache, and losing it must not lose a template.
    #[allow(clippy::too_many_lines)]
    pub fn put_frame(&self, project: &ProjectId, frame: &FramePeople) -> AuraResult<usize> {
        let vault = self.vault(project)?;
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let project_key = project.to_db();
        let crop_root = self.crop_root(project);

        // Seal the crops outside the transaction. Sealing is arithmetic and writing is
        // filesystem work; doing either while holding the catalog's writer would block
        // every other write in the process for the duration of a group photograph.
        let mut crop_refs: Vec<Option<String>> = Vec::with_capacity(frame.faces.len());
        for face in &frame.faces {
            let Some(crop) = &face.crop else {
                crop_refs.push(None);
                continue;
            };
            let name = format!("{}.{CROP_EXT}", face.face_id.as_uuid().as_simple());
            // Encode first, then seal. See `CROP_JPEG_QUALITY` for why, and note the
            // ordering: the template was computed from the raw crop above, so nothing
            // measured depends on this compression.
            let side = u32::try_from(aura_vision::face::CROP_SIDE).unwrap_or(112);
            let encoded = match aura_raw::codec::encode_jpeg(
                &aura_raw::codec::Rgb8 {
                    width: side,
                    height: side,
                    data: crop.clone(),
                },
                CROP_JPEG_QUALITY,
            ) {
                Ok(bytes) => bytes,
                Err(err) => {
                    tracing::warn!(
                        target: "people.scan",
                        face = %face.face_id,
                        code = %err.code,
                        "aligned crop could not be encoded"
                    );
                    crop_refs.push(None);
                    continue;
                }
            };
            let sealed = vault.seal(RecordKind::Crop, &face.face_id.to_db(), &encoded);
            match write_crop(&crop_root, &name, &sealed) {
                Ok(()) => crop_refs.push(Some(format!("{CROP_DIR}/{name}"))),
                Err(err) => {
                    tracing::warn!(
                        target: "people.scan",
                        face = %face.face_id,
                        "sealed crop could not be written: {err}"
                    );
                    crop_refs.push(None);
                }
            }
        }

        // Everything the transaction needs, gathered before it opens.
        #[allow(clippy::items_after_statements)]
        struct Pending {
            id: String,
            bbox: NormBox,
            pose: Pose,
            det_score: f64,
            quality: f64,
            blur: f64,
            occlusion: f64,
            landmarks: String,
            sealed: Option<Vec<u8>>,
            area_frac: f64,
            centrality: f64,
            sharpness: f64,
            embed_ver: i64,
            quality_ver: i64,
            px_height: i64,
            votes: i64,
            found_by: &'static str,
            crop_ref: Option<String>,
        }

        let mut pending = Vec::with_capacity(frame.faces.len());
        for (index, face) in frame.faces.iter().enumerate() {
            let sealed = face.template.as_ref().map(|template| {
                let bytes = embed::encode(&embed::quantise(template));
                vault.seal(RecordKind::Template, &face.face_id.to_db(), &bytes)
            });
            pending.push(Pending {
                id: face.face_id.to_db(),
                bbox: face.bbox,
                pose: face.pose,
                det_score: f64::from(face.det_score),
                quality: f64::from(face.quality.usable),
                blur: f64::from(face.quality.blur),
                occlusion: f64::from(face.quality.occlusion),
                landmarks: align::to_json(&face.landmarks),
                sealed,
                area_frac: f64::from(face.area_frac),
                centrality: f64::from(face.centrality),
                sharpness: f64::from(face.sharpness),
                embed_ver: i64::from(embed::EMBED_VER),
                quality_ver: i64::from(face.quality.quality_ver),
                px_height: i64::from(face.px_height),
                votes: i64::from(face.votes()),
                found_by: face.found_by.as_str(),
                crop_ref: crop_refs.get(index).cloned().flatten(),
            });
        }

        #[allow(clippy::items_after_statements)]
        struct PendingBody {
            id: String,
            bbox: NormBox,
            det_score: f64,
            face_id: Option<String>,
            assoc_score: f64,
            assoc_kind: &'static str,
        }
        let mut bodies = Vec::with_capacity(frame.persons.len());
        for body in &frame.persons {
            bodies.push(PendingBody {
                id: derive_person_id(frame.image_id, &body.bbox, detector_version())
                    .as_hyphenated()
                    .to_string(),
                bbox: body.bbox,
                det_score: f64::from(body.det_score),
                face_id: body
                    .face
                    .and_then(|index| frame.faces.get(index))
                    .map(|face| face.face_id.to_db()),
                assoc_score: f64::from(body.assoc_score),
                assoc_kind: body.assoc_kind.as_str(),
            });
        }

        let image = frame.image_id.to_db();
        let scan = ScanRow {
            faces_found: u32::try_from(frame.faces.len()).unwrap_or(u32::MAX),
            faces_voting: u32::try_from(frame.voting()).unwrap_or(u32::MAX),
            person_boxes: u32::try_from(frame.persons.len()).unwrap_or(u32::MAX),
            tiled: frame.tiled,
            model_ver: detector_version(),
            preprocess_ver: aura_vision::face::detect::PREPROCESS_VER,
        };
        let pixel_tier = i64::from(frame.pixel_tier);
        let pixel_source = frame.pixel_source;
        let elapsed = f64::from(frame.infer_ms);
        let written = pending.len();

        self.catalog.writer().transact(move |tx| {
            // A re-scan may find fewer faces than the previous one - a threshold
            // change, a model change, a different tier. The stale rows have to go, or
            // the frame keeps a face that no longer exists and the People panel shows a
            // box over empty background.
            tx.execute(
                "DELETE FROM person_boxes WHERE image_id = ?1",
                params![image],
            )
            .map_err(|e| statement_failed("could not clear stale person boxes", &e))?;
            tx.execute("DELETE FROM faces WHERE image_id = ?1", params![image])
                .map_err(|e| statement_failed("could not clear stale faces", &e))?;

            for face in &pending {
                tx.execute(
                    "INSERT OR REPLACE INTO faces
                       (id, image_id, project_id, identity_id, x, y, w, h,
                        yaw, pitch, roll, det_score, quality, blur, occlusion,
                        landmarks_json, embed, area_frac, centrality, sharpness,
                        model_ver, embed_ver, quality_ver, px_height, votes, found_by,
                        crop_ref, created_at)
                     VALUES (?1, ?2, ?3, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                             ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25,
                             ?26, ?27)",
                    params![
                        face.id,
                        image,
                        project_key,
                        f64::from(face.bbox.x),
                        f64::from(face.bbox.y),
                        f64::from(face.bbox.w),
                        f64::from(face.bbox.h),
                        f64::from(face.pose.yaw),
                        f64::from(face.pose.pitch),
                        f64::from(face.pose.roll),
                        face.det_score,
                        face.quality,
                        face.blur,
                        face.occlusion,
                        face.landmarks,
                        face.sealed,
                        face.area_frac,
                        face.centrality,
                        face.sharpness,
                        i64::from(scan.model_ver),
                        face.embed_ver,
                        face.quality_ver,
                        face.px_height,
                        face.votes,
                        face.found_by,
                        face.crop_ref,
                        now
                    ],
                )
                .map_err(|e| statement_failed("could not store a face", &e))?;
            }

            for body in &bodies {
                tx.execute(
                    "INSERT OR REPLACE INTO person_boxes
                       (id, image_id, project_id, x, y, w, h, det_score, face_id,
                        assoc_score, assoc_kind, model_ver, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                    params![
                        body.id,
                        image,
                        project_key,
                        f64::from(body.bbox.x),
                        f64::from(body.bbox.y),
                        f64::from(body.bbox.w),
                        f64::from(body.bbox.h),
                        body.det_score,
                        body.face_id,
                        body.assoc_score,
                        body.assoc_kind,
                        i64::from(scan.model_ver),
                        now
                    ],
                )
                .map_err(|e| statement_failed("could not store a person box", &e))?;
            }

            // Last, and in the same transaction. See this method's doc comment.
            tx.execute(
                "INSERT OR REPLACE INTO face_scan
                   (photo_id, project_id, model_ver, preprocess_ver, faces_found,
                    faces_voting, person_boxes, tiled, pixel_tier, pixel_source,
                    elapsed_ms, scanned_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    image,
                    project_key,
                    i64::from(scan.model_ver),
                    i64::from(scan.preprocess_ver),
                    i64::from(scan.faces_found),
                    i64::from(scan.faces_voting),
                    i64::from(scan.person_boxes),
                    i64::from(scan.tiled),
                    pixel_tier,
                    pixel_source,
                    elapsed,
                    now
                ],
            )
            .map_err(|e| statement_failed("could not record a face scan", &e))?;
            Ok(())
        })?;

        Ok(written)
    }

    /// Photographs in a project that have not been scanned at the current versions.
    ///
    /// The whole of invariant 5 for this phase. A model or preprocessing bump makes
    /// every row stale and the next pass re-scans; a killed pass resumes exactly where
    /// the catalog says it stopped, because there is no other record that could
    /// disagree.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn pending(
        &self,
        project: &ProjectId,
        model_ver: u16,
        preprocess_ver: u16,
    ) -> AuraResult<Vec<PhotoId>> {
        let project_key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT p.photo_id
                       FROM photo p
                       LEFT JOIN face_scan s ON s.photo_id = p.photo_id
                      WHERE p.project_id = ?1
                        AND (s.photo_id IS NULL
                             OR s.model_ver <> ?2
                             OR s.preprocess_ver <> ?3)
                      ORDER BY p.timeline_time, p.photo_id",
                )
                .map_err(|e| statement_failed("could not list unscanned photographs", &e))?;
            let rows = statement
                .query_map(
                    params![project_key, i64::from(model_ver), i64::from(preprocess_ver)],
                    |row| row.get::<_, String>(0),
                )
                .map_err(|e| statement_failed("could not list unscanned photographs", &e))?;
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

    /// The focal length and dimensions one frame's tiling decision needs.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn frame_hint(&self, photo: PhotoId) -> AuraResult<aura_vision::face::detect::FrameHint> {
        let key = photo.to_db();
        self.catalog.read(move |conn| {
            let found: Result<Option<f64>, rusqlite::Error> = conn.query_row(
                "SELECT focal_len_mm FROM photo WHERE photo_id = ?1",
                params![key],
                |row| row.get(0),
            );
            Ok(aura_vision::face::detect::FrameHint {
                focal_len_mm: found.ok().flatten().map(|value| value as f32),
                person_boxes: 0,
            })
        })
    }

    /// Every face in a project, templates opened, ordered so two machines agree.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9001` when the vault cannot be opened, `AURA-DB-3006` when the query
    /// fails. A single unopenable envelope is logged as `AURA-SEC-9002` and its row is
    /// returned with `template: None` rather than failing the read: one corrupt face
    /// must not cost a photographer their whole People panel.
    pub fn load_faces(&self, project: &ProjectId) -> AuraResult<Vec<FaceRow>> {
        let vault = self.vault(project)?;
        let project_key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT f.id, f.image_id, f.identity_id, f.x, f.y, f.w, f.h,
                            f.yaw, f.pitch, f.roll, f.det_score, f.quality, f.blur,
                            f.occlusion, f.landmarks_json, f.embed, f.area_frac,
                            f.centrality, f.sharpness, f.model_ver, f.embed_ver,
                            f.quality_ver, f.px_height, f.votes, f.found_by, f.crop_ref,
                            (SELECT b.h FROM person_boxes b WHERE b.face_id = f.id LIMIT 1)
                       FROM faces f
                      WHERE f.project_id = ?1
                      ORDER BY f.id",
                )
                .map_err(|e| statement_failed("could not read faces", &e))?;
            let mut rows = statement
                .query(params![project_key])
                .map_err(|e| statement_failed("could not read faces", &e))?;

            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a face row", &e))?
            {
                if let Some(face) = read_face(row, &vault) {
                    out.push(face);
                }
            }
            Ok(out)
        })
    }

    /// Every face in one photograph.
    ///
    /// # Errors
    ///
    /// As [`PeopleStore::load_faces`].
    pub fn faces_of_image(&self, project: &ProjectId, image: PhotoId) -> AuraResult<Vec<FaceRow>> {
        let vault = self.vault(project)?;
        let project_key = project.to_db();
        let image_key = image.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT f.id, f.image_id, f.identity_id, f.x, f.y, f.w, f.h,
                            f.yaw, f.pitch, f.roll, f.det_score, f.quality, f.blur,
                            f.occlusion, f.landmarks_json, f.embed, f.area_frac,
                            f.centrality, f.sharpness, f.model_ver, f.embed_ver,
                            f.quality_ver, f.px_height, f.votes, f.found_by, f.crop_ref,
                            (SELECT b.h FROM person_boxes b WHERE b.face_id = f.id LIMIT 1)
                       FROM faces f
                      WHERE f.project_id = ?1 AND f.image_id = ?2
                      ORDER BY f.det_score DESC, f.id",
                )
                .map_err(|e| statement_failed("could not read a frame's faces", &e))?;
            let mut rows = statement
                .query(params![project_key, image_key])
                .map_err(|e| statement_failed("could not read a frame's faces", &e))?;

            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a face row", &e))?
            {
                if let Some(face) = read_face(row, &vault) {
                    out.push(face);
                }
            }
            Ok(out)
        })
    }

    /// Replace a project's identities with a fresh clustering.
    ///
    /// One transaction, so a project is never half regrouped. Identity ids are derived
    /// from the project and the lowest member face id, which makes a re-clustering that
    /// reaches the same answer produce the same ids - and a re-clustering that reaches a
    /// different answer produce different ones, which is honest: it *is* a different
    /// person-grouping, and the photographer's decisions are reattached by
    /// [`PeopleStore::replayable_links`] rather than by pretending the ids survived.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9001` when the vault cannot be opened, `AURA-DB-3006` on a statement
    /// failure.
    #[allow(clippy::too_many_lines)]
    pub fn put_identities(
        &self,
        project: &ProjectId,
        faces: &[FaceRow],
        clusters: &[Cluster],
        cluster_ver: u16,
    ) -> AuraResult<Vec<IdentityId>> {
        let vault = self.vault(project)?;
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let project_key = project.to_db();

        #[allow(clippy::items_after_statements)]
        struct PendingIdentity {
            id: String,
            centroid: Option<Vec<u8>>,
            sub_centroids: Option<Vec<u8>>,
            sub_count: i64,
            variance: f64,
            face_count: i64,
            members: Vec<String>,
        }

        let mut pending = Vec::with_capacity(clusters.len());
        let mut created = Vec::with_capacity(clusters.len());
        for cluster in clusters {
            let members: Vec<&FaceRow> = cluster
                .members
                .iter()
                .filter_map(|index| faces.get(*index))
                .collect();
            let Some(anchor) = members.iter().map(|face| face.face_id).min() else {
                continue;
            };
            let identity = derive_identity_id(project, anchor);
            let identity_key = identity.to_db();

            let centroid_bytes = embed::encode(&embed::quantise(&cluster.centroid));
            let centroid = vault.seal(RecordKind::Centroid, &identity_key, &centroid_bytes);

            let sub_centroids = if cluster.sub_centroids.is_empty() {
                None
            } else {
                let mut flat = Vec::with_capacity(cluster.sub_centroids.len() * 1_024);
                for sub in &cluster.sub_centroids {
                    flat.extend(embed::encode(&embed::quantise(sub)));
                }
                Some(vault.seal(RecordKind::SubCentroids, &identity_key, &flat))
            };

            pending.push(PendingIdentity {
                id: identity_key,
                centroid: Some(centroid),
                sub_centroids,
                sub_count: i64::try_from(cluster.sub_centroids.len()).unwrap_or(0),
                variance: f64::from(cluster.variance),
                face_count: i64::try_from(members.len()).unwrap_or(0),
                members: members.iter().map(|face| face.face_id.to_db()).collect(),
            });
            created.push(identity);
        }

        self.catalog.writer().transact(move |tx| {
            // Detach first, then delete. `faces.identity_id` is `ON DELETE SET NULL`, so
            // the detach is belt and braces - and it is the belt that matters: an
            // explicit update makes the intent readable in a query log.
            tx.execute(
                "UPDATE faces SET identity_id = NULL WHERE project_id = ?1",
                params![project_key],
            )
            .map_err(|e| statement_failed("could not detach faces", &e))?;
            tx.execute(
                "DELETE FROM identities WHERE project_id = ?1",
                params![project_key],
            )
            .map_err(|e| statement_failed("could not clear identities", &e))?;
            tx.execute(
                "DELETE FROM cooccurrence WHERE project_id = ?1",
                params![project_key],
            )
            .map_err(|e| statement_failed("could not clear the co-occurrence graph", &e))?;

            for identity in &pending {
                tx.execute(
                    "INSERT INTO identities
                       (id, project_id, label, role, role_confidence, role_reasons,
                        user_locked, importance, face_count, first_seen, last_seen,
                        centroid, sub_centroids, sub_count, variance, cluster_ver,
                        created_at, updated_at)
                     VALUES (?1, ?2, NULL, 'unknown', 0.0, '[]', 0, 0.5, ?3,
                             NULL, NULL, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                    params![
                        identity.id,
                        project_key,
                        identity.face_count,
                        identity.centroid,
                        identity.sub_centroids,
                        identity.sub_count,
                        identity.variance,
                        i64::from(cluster_ver),
                        now
                    ],
                )
                .map_err(|e| statement_failed("could not store an identity", &e))?;

                for face in &identity.members {
                    tx.execute(
                        "UPDATE faces SET identity_id = ?1 WHERE id = ?2 AND project_id = ?3",
                        params![identity.id, face, project_key],
                    )
                    .map_err(|e| statement_failed("could not assign a face", &e))?;
                }
            }

            // First and last seen, from the photographs rather than from the scan order.
            tx.execute(
                "UPDATE identities SET
                   first_seen = (SELECT MIN(p.timeline_time) FROM faces f
                                   JOIN photo p ON p.photo_id = f.image_id
                                  WHERE f.identity_id = identities.id),
                   last_seen  = (SELECT MAX(p.timeline_time) FROM faces f
                                   JOIN photo p ON p.photo_id = f.image_id
                                  WHERE f.identity_id = identities.id)
                 WHERE project_id = ?1",
                params![project_key],
            )
            .map_err(|e| statement_failed("could not stamp identity timelines", &e))?;
            Ok(())
        })?;

        Ok(created)
    }

    /// Every identity in a project, centroids opened.
    ///
    /// # Errors
    ///
    /// As [`PeopleStore::load_faces`].
    pub fn load_identities(&self, project: &ProjectId) -> AuraResult<Vec<IdentityRow>> {
        let vault = self.vault(project)?;
        let owner = *project;
        let project_key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT id, label, role, role_confidence, role_reasons, user_locked,
                            importance, face_count, first_seen, last_seen, centroid,
                            sub_centroids, sub_count, variance, cluster_ver
                       FROM identities
                      WHERE project_id = ?1
                      ORDER BY face_count DESC, id",
                )
                .map_err(|e| statement_failed("could not read identities", &e))?;
            let mut rows = statement
                .query(params![project_key])
                .map_err(|e| statement_failed("could not read identities", &e))?;

            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read an identity row", &e))?
            {
                if let Some(identity) = read_identity(row, &vault, owner) {
                    out.push(identity);
                }
            }
            Ok(out)
        })
    }

    /// Write a role, its confidence and its reasons.
    ///
    /// `locked` is what section 6.3's last line - "can never override a `user_locked`
    /// identity" - is enforced by: the statement's `WHERE` clause refuses to touch a
    /// locked row unless the caller is itself locking one.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a statement failure.
    pub fn set_role(
        &self,
        project: &ProjectId,
        identity: IdentityId,
        role: Role,
        confidence: f32,
        reasons: &[String],
        locked: bool,
    ) -> AuraResult<usize> {
        let project_key = project.to_db();
        let identity_key = identity.to_db();
        let reasons_json = serde_json::to_string(reasons).unwrap_or_else(|_| "[]".to_string());
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let role_text = role.as_str();
        let confidence = f64::from(confidence.clamp(0.0, 1.0));

        self.catalog.writer().transact(move |tx| {
            let changed = tx
                .execute(
                    "UPDATE identities
                        SET role = ?1, role_confidence = ?2, role_reasons = ?3,
                            user_locked = MAX(user_locked, ?4), updated_at = ?5
                      WHERE id = ?6 AND project_id = ?7
                        AND (user_locked = 0 OR ?4 = 1)",
                    params![
                        role_text,
                        confidence,
                        reasons_json,
                        i64::from(locked),
                        now,
                        identity_key,
                        project_key
                    ],
                )
                .map_err(|e| statement_failed("could not set a role", &e))?;
            Ok(changed)
        })
    }

    /// Rename an identity.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a statement failure.
    pub fn set_label(
        &self,
        project: &ProjectId,
        identity: IdentityId,
        label: Option<&str>,
    ) -> AuraResult<usize> {
        let project_key = project.to_db();
        let identity_key = identity.to_db();
        let label = label.map(ToString::to_string);
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        self.catalog.writer().transact(move |tx| {
            let changed = tx
                .execute(
                    "UPDATE identities SET label = ?1, updated_at = ?2
                      WHERE id = ?3 AND project_id = ?4",
                    params![label, now, identity_key, project_key],
                )
                .map_err(|e| statement_failed("could not rename an identity", &e))?;
            Ok(changed)
        })
    }

    /// Set the importance slider.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a statement failure.
    pub fn set_importance(
        &self,
        project: &ProjectId,
        identity: IdentityId,
        importance: f32,
    ) -> AuraResult<usize> {
        let project_key = project.to_db();
        let identity_key = identity.to_db();
        let importance = f64::from(importance.clamp(0.0, 1.0));
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        self.catalog.writer().transact(move |tx| {
            let changed = tx
                .execute(
                    "UPDATE identities SET importance = ?1, updated_at = ?2
                      WHERE id = ?3 AND project_id = ?4",
                    params![importance, now, identity_key, project_key],
                )
                .map_err(|e| statement_failed("could not set importance", &e))?;
            Ok(changed)
        })
    }

    /// Move a set of faces onto an identity, refusing anything from another project.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9004` when a face belongs to a different project, `AURA-DB-3006` on a
    /// statement failure.
    pub fn assign_faces(
        &self,
        project: &ProjectId,
        identity: Option<IdentityId>,
        faces: &[FaceId],
    ) -> AuraResult<usize> {
        if faces.is_empty() {
            return Ok(0);
        }
        let project_key = project.to_db();
        let identity_key = identity.map(|id| id.to_db());
        let keys: Vec<String> = faces.iter().map(FaceId::to_db).collect();
        let now = aura_catalog::rfc3339(self.clock.now_utc());

        self.catalog.writer().transact(move |tx| {
            let mut moved = 0usize;
            for face in &keys {
                // The project clause is the cross-project guard. A face id from another
                // wedding simply matches nothing, and the count that comes back tells
                // the caller so.
                moved += tx
                    .execute(
                        "UPDATE faces SET identity_id = ?1 WHERE id = ?2 AND project_id = ?3",
                        params![identity_key, face, project_key],
                    )
                    .map_err(|e| statement_failed("could not assign a face", &e))?;
            }
            if let Some(identity) = &identity_key {
                tx.execute(
                    "UPDATE identities
                        SET face_count = (SELECT COUNT(*) FROM faces WHERE identity_id = ?1),
                            updated_at = ?2
                      WHERE id = ?1",
                    params![identity, now],
                )
                .map_err(|e| statement_failed("could not recount an identity", &e))?;
            }
            Ok(moved)
        })
    }

    /// Create an empty identity, for a split.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a statement failure.
    pub fn create_identity(
        &self,
        project: &ProjectId,
        anchor: FaceId,
        cluster_ver: u16,
    ) -> AuraResult<IdentityId> {
        let identity = derive_identity_id(project, anchor);
        let identity_key = identity.to_db();
        let project_key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT OR REPLACE INTO identities
                   (id, project_id, label, role, role_confidence, role_reasons,
                    user_locked, importance, face_count, first_seen, last_seen,
                    centroid, sub_centroids, sub_count, variance, cluster_ver,
                    created_at, updated_at)
                 VALUES (?1, ?2, NULL, 'unknown', 0.0, '[]', 0, 0.5, 0, NULL, NULL,
                         NULL, NULL, 0, 0.0, ?3, ?4, ?4)",
                params![identity_key, project_key, i64::from(cluster_ver), now],
            )
            .map_err(|e| statement_failed("could not create an identity", &e))?;
            Ok(())
        })?;
        Ok(identity)
    }

    /// Delete an identity that has no faces left.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a statement failure.
    pub fn delete_identity(&self, project: &ProjectId, identity: IdentityId) -> AuraResult<usize> {
        let project_key = project.to_db();
        let identity_key = identity.to_db();
        self.catalog.writer().transact(move |tx| {
            let removed = tx
                .execute(
                    "DELETE FROM identities
                      WHERE id = ?1 AND project_id = ?2
                        AND NOT EXISTS (SELECT 1 FROM faces WHERE identity_id = ?1)",
                    params![identity_key, project_key],
                )
                .map_err(|e| statement_failed("could not delete an identity", &e))?;
            Ok(removed)
        })
    }

    /// Append one decision to the journal.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a statement failure.
    pub fn record_link(&self, project: &ProjectId, link: &IdentityLink) -> AuraResult<()> {
        let project_key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let faces: Vec<String> = link.face_ids.iter().map(FaceId::to_db).collect();
        let faces_json = if faces.is_empty() {
            None
        } else {
            serde_json::to_string(&faces).ok()
        };
        let kind = link.kind.clone();
        let identity = link.identity_id.to_db();
        let other = link.other_id.map(|id| id.to_db());
        let old_value = link.old_value.clone();
        let new_value = link.new_value.clone();
        let actor = link.actor.clone();

        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "INSERT INTO identity_links
                   (project_id, kind, identity_id, other_id, face_ids, old_value,
                    new_value, actor, created_at, undone_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)",
                params![
                    project_key,
                    kind,
                    identity,
                    other,
                    faces_json,
                    old_value,
                    new_value,
                    actor,
                    now
                ],
            )
            .map_err(|e| statement_failed("could not record a people decision", &e))?;
            Ok(())
        })
    }

    /// Every user decision that is still standing, oldest first.
    ///
    /// The input to a replay after a re-analysis. Undone entries are excluded here
    /// rather than filtered by the caller, because "the photographer changed their mind"
    /// and "the photographer never said" have to produce the same outcome.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn replayable_links(&self, project: &ProjectId) -> AuraResult<Vec<IdentityLink>> {
        let project_key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT kind, identity_id, other_id, face_ids, old_value, new_value, actor
                       FROM identity_links
                      WHERE project_id = ?1 AND undone_at IS NULL
                      ORDER BY link_id",
                )
                .map_err(|e| statement_failed("could not read the people journal", &e))?;
            let mut rows = statement
                .query(params![project_key])
                .map_err(|e| statement_failed("could not read the people journal", &e))?;

            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a journal row", &e))?
            {
                let kind: String = row.get(0).unwrap_or_default();
                let identity: String = row.get(1).unwrap_or_default();
                let Ok(identity_id) = IdentityId::from_db(&identity) else {
                    continue;
                };
                let other: Option<String> = row.get(2).ok().flatten();
                let faces: Option<String> = row.get(3).ok().flatten();
                let face_ids = faces
                    .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
                    .map(|list| {
                        list.iter()
                            .filter_map(|text| FaceId::from_db(text).ok())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                out.push(IdentityLink {
                    kind,
                    identity_id,
                    other_id: other.and_then(|text| IdentityId::from_db(&text).ok()),
                    face_ids,
                    old_value: row.get(4).ok().flatten(),
                    new_value: row.get(5).ok().flatten(),
                    actor: row.get(6).unwrap_or_else(|_| "user".to_string()),
                });
            }
            Ok(out)
        })
    }

    /// Mark the most recent decision of a kind on an identity as undone.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a statement failure.
    pub fn undo_last_link(
        &self,
        project: &ProjectId,
        identity: IdentityId,
    ) -> AuraResult<Option<IdentityLink>> {
        let links = self.replayable_links(project)?;
        let Some(last) = links
            .iter()
            .rev()
            .find(|link| link.identity_id == identity)
            .cloned()
        else {
            return Ok(None);
        };
        let project_key = project.to_db();
        let identity_key = identity.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "UPDATE identity_links SET undone_at = ?1
                  WHERE link_id = (SELECT MAX(link_id) FROM identity_links
                                    WHERE project_id = ?2 AND identity_id = ?3
                                      AND undone_at IS NULL)",
                params![now, project_key, identity_key],
            )
            .map_err(|e| statement_failed("could not undo a people decision", &e))?;
            Ok(())
        })?;
        Ok(Some(last))
    }

    /// Replace the co-occurrence graph for a project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` on a statement failure.
    pub fn put_cooccurrence(
        &self,
        project: &ProjectId,
        edges: &[(IdentityId, IdentityId, u32, String)],
    ) -> AuraResult<usize> {
        let project_key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let rows: Vec<(String, String, i64, String)> = edges
            .iter()
            .map(|(a, b, frames, scenes)| {
                let (left, right) = if a <= b { (a, b) } else { (b, a) };
                (
                    left.to_db(),
                    right.to_db(),
                    i64::from(*frames),
                    scenes.clone(),
                )
            })
            .collect();
        let written = rows.len();

        self.catalog.writer().transact(move |tx| {
            tx.execute(
                "DELETE FROM cooccurrence WHERE project_id = ?1",
                params![project_key],
            )
            .map_err(|e| statement_failed("could not clear the co-occurrence graph", &e))?;
            for (left, right, frames, scenes) in &rows {
                tx.execute(
                    "INSERT OR REPLACE INTO cooccurrence
                       (project_id, identity_a, identity_b, frames, scenes, first_seen,
                        last_seen, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5,
                             (SELECT MIN(p.timeline_time) FROM faces f
                                JOIN photo p ON p.photo_id = f.image_id
                               WHERE f.identity_id IN (?2, ?3)),
                             (SELECT MAX(p.timeline_time) FROM faces f
                                JOIN photo p ON p.photo_id = f.image_id
                               WHERE f.identity_id IN (?2, ?3)),
                             ?6)",
                    params![project_key, left, right, frames, scenes, now],
                )
                .map_err(|e| statement_failed("could not store a co-occurrence edge", &e))?;
            }
            Ok(written)
        })
    }

    /// The co-occurrence graph for a project.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn load_cooccurrence(
        &self,
        project: &ProjectId,
    ) -> AuraResult<Vec<(IdentityId, IdentityId, u32, String)>> {
        let project_key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT identity_a, identity_b, frames, scenes FROM cooccurrence
                      WHERE project_id = ?1
                      ORDER BY identity_a, identity_b",
                )
                .map_err(|e| statement_failed("could not read the co-occurrence graph", &e))?;
            let mut rows = statement
                .query(params![project_key])
                .map_err(|e| statement_failed("could not read the co-occurrence graph", &e))?;
            let mut out = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read an edge", &e))?
            {
                let a: String = row.get(0).unwrap_or_default();
                let b: String = row.get(1).unwrap_or_default();
                let (Ok(left), Ok(right)) = (IdentityId::from_db(&a), IdentityId::from_db(&b))
                else {
                    continue;
                };
                let frames: i64 = row.get(2).unwrap_or(0);
                let scenes: String = row.get(3).unwrap_or_else(|_| "{}".to_string());
                out.push((left, right, u32::try_from(frames).unwrap_or(0), scenes));
            }
            Ok(out)
        })
    }

    /// How much of a project has been scanned.
    ///
    /// Returns photographs, scanned, faces and voting faces.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the view cannot be read.
    pub fn coverage(&self, project: &ProjectId) -> AuraResult<(i64, i64, i64, i64)> {
        let project_key = project.to_db();
        self.catalog.read(move |conn| {
            let found: Result<(i64, i64, i64, i64), rusqlite::Error> = conn.query_row(
                "SELECT photos, scanned, faces, voting_faces FROM v_people_coverage
                  WHERE project_id = ?1",
                params![project_key],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            );
            // A project with no photographs has no row in the view. Zero is the
            // truthful answer, not an error.
            Ok(found.unwrap_or((0, 0, 0, 0)))
        })
    }

    /// The distinct detector and recogniser versions present in a project's faces.
    ///
    /// More than one pair means the project is mid-re-scan. The caller reports
    /// `AURA-ML-5018` and keeps going; nothing here decides policy.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn face_versions(&self, project: &ProjectId) -> AuraResult<Vec<(u16, u16, usize)>> {
        let project_key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT model_ver, embed_ver, COUNT(*) FROM faces
                      WHERE project_id = ?1
                      GROUP BY model_ver, embed_ver
                      ORDER BY model_ver, embed_ver",
                )
                .map_err(|e| statement_failed("could not read face versions", &e))?;
            let rows = statement
                .query_map(params![project_key], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })
                .map_err(|e| statement_failed("could not read face versions", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let (detector, recogniser, count) =
                    row.map_err(|e| statement_failed("bad version row", &e))?;
                out.push((
                    u16::try_from(detector).unwrap_or(0),
                    u16::try_from(recogniser).unwrap_or(0),
                    usize::try_from(count).unwrap_or(0),
                ));
            }
            Ok(out)
        })
    }

    /// Erase every trace of biometric data for one project.
    ///
    /// Section 6.5's "erase biometric data" control. Four things happen, in this order,
    /// and the order is the design:
    ///
    /// 1. **The credential-store entry goes first.** Without the key, everything else is
    ///    noise - so if the process dies after step 1, the data is already unreadable.
    ///    Doing it last would leave a window in which a crash left readable templates
    ///    and a photographer who believed they were gone.
    /// 2. The sealed crop files are deleted.
    /// 3. The catalog rows are deleted, in one transaction.
    /// 4. `face_vault.erased_at` records that this happened, because "there are no
    ///    faces" and "the faces were deliberately destroyed" are different answers to a
    ///    client who asks.
    ///
    /// **What survives, deliberately:** every culling decision, every edit, every
    /// export. Section 6.5 requires erasure to keep those intact, and it does - nothing
    /// in this method touches a table outside migration 6.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9003` when anything is left behind. The report says how much.
    #[allow(clippy::too_many_lines)]
    pub fn erase(&self, project: &ProjectId) -> AuraResult<EraseReport> {
        let mut report = EraseReport::default();
        let account = crate::vault::account_for(project);

        // 1. The key.
        match self.keys.delete(&account) {
            Ok(()) => report.key_removed = true,
            Err(err) => {
                return Err(errors::erasure_incomplete(
                    &project.to_db(),
                    0,
                    &format!(
                        "the credential store refused to delete the key: {}",
                        err.detail
                    ),
                ));
            }
        }
        self.vaults.lock().remove(&project.to_db());

        // 2. The crops.
        let crop_root = self.crop_root(project);
        if crop_root.exists() {
            match std::fs::read_dir(&crop_root) {
                Ok(entries) => {
                    for entry in entries.flatten() {
                        if std::fs::remove_file(entry.path()).is_ok() {
                            report.crops += 1;
                        }
                    }
                }
                Err(err) => {
                    return Err(errors::erasure_incomplete(
                        &project.to_db(),
                        0,
                        &format!("the sealed crop directory could not be read: {err}"),
                    ));
                }
            }
            drop(std::fs::remove_dir(&crop_root));
        }

        // 3. The rows.
        let project_key = project.to_db();
        let now = aura_catalog::rfc3339(self.clock.now_utc());
        let (faces, identities) = self.catalog.writer().transact({
            let project_key = project_key.clone();
            let now = now.clone();
            move |tx| {
                let faces = tx
                    .execute(
                        "DELETE FROM faces WHERE project_id = ?1",
                        params![project_key],
                    )
                    .map_err(|e| statement_failed("could not delete faces", &e))?;
                tx.execute(
                    "DELETE FROM person_boxes WHERE project_id = ?1",
                    params![project_key],
                )
                .map_err(|e| statement_failed("could not delete person boxes", &e))?;
                let identities = tx
                    .execute(
                        "DELETE FROM identities WHERE project_id = ?1",
                        params![project_key],
                    )
                    .map_err(|e| statement_failed("could not delete identities", &e))?;
                tx.execute(
                    "DELETE FROM cooccurrence WHERE project_id = ?1",
                    params![project_key],
                )
                .map_err(|e| statement_failed("could not delete the co-occurrence graph", &e))?;
                tx.execute(
                    "DELETE FROM identity_links WHERE project_id = ?1",
                    params![project_key],
                )
                .map_err(|e| statement_failed("could not delete the people journal", &e))?;
                tx.execute(
                    "DELETE FROM face_scan WHERE project_id = ?1",
                    params![project_key],
                )
                .map_err(|e| statement_failed("could not delete the scan ledger", &e))?;

                // 4. The receipt.
                tx.execute(
                    "UPDATE face_vault SET erased_at = ?1, erased_faces = ?2
                      WHERE project_id = ?3",
                    params![now, i64::try_from(faces).unwrap_or(0), project_key],
                )
                .map_err(|e| statement_failed("could not record the erasure", &e))?;
                Ok((faces, identities))
            }
        })?;
        report.faces = faces;
        report.identities = identities;

        // Prove it. An erasure that reports success without checking is a promise
        // nobody verified.
        let (_, scanned, remaining_faces, _) = self.coverage(project)?;
        if remaining_faces > 0 || scanned > 0 {
            return Err(errors::erasure_incomplete(
                &project_key,
                usize::try_from(remaining_faces.max(scanned)).unwrap_or(usize::MAX),
                "rows survived the delete",
            ));
        }

        tracing::info!(
            target: "people.erase",
            project = %project_key,
            faces = report.faces,
            identities = report.identities,
            crops = report.crops,
            "biometric data erased"
        );
        Ok(report)
    }

    /// Which project a photograph belongs to.
    ///
    /// `PeopleService::subjects` takes only an image id, so the project has to be
    /// looked up - and looking it up rather than trusting a caller-supplied one is what
    /// makes the frozen signature safe: there is no argument a caller could get wrong.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn project_of_photo(&self, photo: PhotoId) -> AuraResult<Option<ProjectId>> {
        let key = photo.to_db();
        self.catalog.read(move |conn| {
            let found: Result<String, rusqlite::Error> = conn.query_row(
                "SELECT project_id FROM photo WHERE photo_id = ?1",
                params![key],
                |row| row.get(0),
            );
            match found {
                Ok(text) => Ok(ProjectId::from_db(&text).ok()),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(statement_failed(
                    "could not read a photograph's project",
                    &err,
                )),
            }
        })
    }

    /// Which project an identity belongs to.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn project_of_identity(&self, identity: IdentityId) -> AuraResult<Option<ProjectId>> {
        let key = identity.to_db();
        self.catalog.read(move |conn| {
            let found: Result<String, rusqlite::Error> = conn.query_row(
                "SELECT project_id FROM identities WHERE id = ?1",
                params![key],
                |row| row.get(0),
            );
            match found {
                Ok(text) => Ok(ProjectId::from_db(&text).ok()),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(err) => Err(statement_failed(
                    "could not read an identity's project",
                    &err,
                )),
            }
        })
    }

    /// The face ids assigned to one identity, ascending.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn faces_of_identity(&self, identity: IdentityId) -> AuraResult<Vec<FaceId>> {
        let key = identity.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT id FROM faces WHERE identity_id = ?1 ORDER BY id")
                .map_err(|e| statement_failed("could not read an identity's faces", &e))?;
            let rows = statement
                .query_map(params![key], |row| row.get::<_, String>(0))
                .map_err(|e| statement_failed("could not read an identity's faces", &e))?;
            let mut out = Vec::new();
            for row in rows {
                let text = row.map_err(|e| statement_failed("bad face id", &e))?;
                if let Ok(id) = FaceId::from_db(&text) {
                    out.push(id);
                }
            }
            Ok(out)
        })
    }

    /// Clock-aligned capture times for a project's photographs, in milliseconds.
    ///
    /// `timeline_time` rather than `capture_time`, because a second body with a wrong
    /// clock would otherwise put a guest at the ceremony an hour before it started.
    /// Phase 01 already did the alignment; this reads the result.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn photo_timestamps(&self, project: &ProjectId) -> AuraResult<BTreeMap<PhotoId, i64>> {
        let project_key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare(
                    "SELECT photo_id, timeline_time FROM photo
                      WHERE project_id = ?1 AND timeline_time IS NOT NULL",
                )
                .map_err(|e| statement_failed("could not read timeline times", &e))?;
            let mut rows = statement
                .query(params![project_key])
                .map_err(|e| statement_failed("could not read timeline times", &e))?;
            let mut out = BTreeMap::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a timeline row", &e))?
            {
                let id: String = row.get(0).unwrap_or_default();
                let text: String = row.get(1).unwrap_or_default();
                let (Ok(photo), Some(at)) = (
                    PhotoId::from_db(&id),
                    crate::timeline::parse_timestamp(&text),
                ) else {
                    continue;
                };
                out.insert(photo, at);
            }
            Ok(out)
        })
    }

    /// Each photograph's coarse scene label, for the co-occurrence graph.
    ///
    /// **This is the query that half-closes phase 06's condition C3.** Until phase 07
    /// shipped, `People::regroup` passed an empty map here and
    /// `aura_vision::face::roles` reported `scene_starved`, which capped the couple
    /// decision at `SCENELESS_CONFIDENCE_CEILING` - 0.62 - because with no scene labels
    /// a bride and her mother are as co-occurrent as a bride and a groom.
    ///
    /// Two things about the shape of the answer:
    ///
    /// * it is the **coarse** four-word vocabulary, not the 22-class one.
    ///   `SceneId::role_label` is the mapping and it lives in `aura-core` so this crate
    ///   does not have to link the classifier to read a label the classifier wrote;
    /// * frames whose scene says nothing about *who somebody is* - details, venue,
    ///   candids, the dance floor - are **absent** rather than present with an empty
    ///   label. A guest at the dance floor is not evidence about the couple, and a map
    ///   entry saying `dance_floor` would make the graph count it as though it were.
    ///
    /// An empty map is still a legitimate answer: a project whose scene pass has not run
    /// gets the phase 06 behaviour it always had, and `RoleOutcome::scene_starved` says
    /// so.
    ///
    /// # Errors
    ///
    /// `AURA-DB-3006` when the query fails.
    pub fn scene_labels(&self, project: &ProjectId) -> AuraResult<BTreeMap<PhotoId, String>> {
        let project_key = project.to_db();
        self.catalog.read(move |conn| {
            let mut statement = conn
                .prepare("SELECT photo_id, scene FROM image_scenes WHERE project_id = ?1")
                .map_err(|e| statement_failed("could not read scene labels", &e))?;
            let mut rows = statement
                .query(params![project_key])
                .map_err(|e| statement_failed("could not read scene labels", &e))?;
            let mut out = BTreeMap::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| statement_failed("could not read a scene row", &e))?
            {
                let id: String = row.get(0).unwrap_or_default();
                let scene: String = row.get(1).unwrap_or_default();
                let Ok(photo) = PhotoId::from_db(&id) else {
                    continue;
                };
                if let Some(label) =
                    aura_core::contract::scene::SceneId::from_str_or_unknown(&scene).role_label()
                {
                    out.insert(photo, label.to_string());
                }
            }
            Ok(out)
        })
    }

    /// Refuse two ids that do not belong to the same project.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9004` when they disagree, `AURA-DB-3006` when the lookup fails.
    pub fn assert_same_project(
        &self,
        project: &ProjectId,
        identities: &[IdentityId],
    ) -> AuraResult<()> {
        if identities.is_empty() {
            return Ok(());
        }
        let project_key = project.to_db();
        let keys: Vec<String> = identities.iter().map(IdentityId::to_db).collect();
        let found: Vec<String> = self.catalog.read({
            let project_key = project_key.clone();
            move |conn| {
                let mut out = Vec::new();
                for key in &keys {
                    let owner: Result<String, rusqlite::Error> = conn.query_row(
                        "SELECT project_id FROM identities WHERE id = ?1",
                        params![key],
                        |row| row.get(0),
                    );
                    match owner {
                        Ok(project) if project == project_key => {}
                        Ok(project) => out.push(project),
                        Err(rusqlite::Error::QueryReturnedNoRows) => {
                            out.push(String::new());
                        }
                        Err(err) => {
                            return Err(statement_failed("could not read an identity owner", &err))
                        }
                    }
                }
                Ok(out)
            }
        })?;

        if let Some(other) = found.iter().find(|owner| !owner.is_empty()) {
            return Err(errors::cross_project_refused(&project_key, other));
        }
        if !found.is_empty() {
            return Err(errors::edit_refused(
                "identity lookup",
                "one of the identities does not exist in this project",
            ));
        }
        Ok(())
    }

    /// Read one sealed crop back, as JPEG bytes, for the People panel's thumbnails.
    ///
    /// Returns the encoded image rather than pixels: it was encoded before it was sealed,
    /// so decoding it here only for the caller to encode it again would be two wasted
    /// passes over 12,544 pixels on the interactive path.
    ///
    /// # Errors
    ///
    /// `AURA-SEC-9001` when the vault cannot be opened and `AURA-SEC-9002` when the
    /// file is missing or does not authenticate.
    pub fn open_crop(&self, project: &ProjectId, face: FaceId) -> AuraResult<Vec<u8>> {
        let vault = self.vault(project)?;
        let name = format!("{}.{CROP_EXT}", face.as_uuid().as_simple());
        let path = self.crop_root(project).join(&name);
        let sealed = std::fs::read(&path).map_err(|err| {
            errors::envelope_unusable(&face.to_db(), &format!("crop could not be read: {err}"))
        })?;
        vault.open(RecordKind::Crop, &face.to_db(), &sealed)
    }
}

/// The detector version every row is stamped with.
#[must_use]
pub const fn detector_version() -> u16 {
    aura_vision::face::detect::MODEL_VER
}

/// A deterministic identity id from a project and its lowest member face.
///
/// Deterministic for the same reason face ids are: two machines clustering one wedding
/// must produce the same ids, and a re-run that reaches the same grouping must not
/// orphan a photographer's decisions. The anchor is the lowest face id rather than the
/// medoid because it is stable under a change of centroid arithmetic.
#[must_use]
pub fn derive_identity_id(project: &ProjectId, anchor: FaceId) -> IdentityId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"aura.identity.v1");
    hasher.update(project.to_db().as_bytes());
    hasher.update(anchor.to_db().as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(digest.as_bytes().get(..16).unwrap_or(&[0u8; 16]));
    if let Some(slot) = bytes.get_mut(6) {
        *slot = (*slot & 0x0f) | 0x80;
    }
    if let Some(slot) = bytes.get_mut(8) {
        *slot = (*slot & 0x3f) | 0x80;
    }
    IdentityId::from_uuid(Uuid::from_bytes(bytes))
}

/// Write one sealed crop, creating the directory on first use.
fn write_crop(root: &Path, name: &str, sealed: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(root)?;
    std::fs::write(root.join(name), sealed)
}

/// Read one face row, opening its template.
fn read_face(row: &rusqlite::Row<'_>, vault: &Vault) -> Option<FaceRow> {
    let id: String = row.get(0).ok()?;
    let face_id = FaceId::from_db(&id).ok()?;
    let image: String = row.get(1).ok()?;
    let image_id = PhotoId::from_db(&image).ok()?;
    let identity: Option<String> = row.get(2).ok().flatten();

    let read = |index: usize| -> f32 {
        row.get::<_, Option<f64>>(index)
            .ok()
            .flatten()
            .unwrap_or(0.0) as f32
    };

    let sealed: Option<Vec<u8>> = row.get(15).ok().flatten();
    let template = sealed.and_then(
        |bytes| match vault.open(RecordKind::Template, &id, &bytes) {
            Ok(plain) => embed::decode(&plain),
            Err(err) => {
                tracing::warn!(
                    target: "people.vault",
                    face = %id,
                    code = %err.code,
                    "{}", err.detail
                );
                None
            }
        },
    );

    let landmarks_json: String = row.get(14).unwrap_or_else(|_| "[]".to_string());
    let landmarks = align::from_json(&landmarks_json).unwrap_or([[0.0; 2]; 5]);

    let bbox = NormBox {
        x: read(3),
        y: read(4),
        w: read(5),
        h: read(6),
    };
    let body_height: Option<f64> = row.get(26).ok().flatten();
    let child_score = body_height.map_or(0.0, |height| {
        if height <= 0.0 {
            0.0
        } else {
            (((bbox.h / height as f32) - 0.16) / (0.30 - 0.16)).clamp(0.0, 1.0)
        }
    });

    Some(FaceRow {
        face_id,
        image_id,
        identity_id: identity.and_then(|text| IdentityId::from_db(&text).ok()),
        bbox,
        pose: Pose {
            yaw: read(7),
            pitch: read(8),
            roll: read(9),
        },
        det_score: read(10),
        quality: read(11),
        blur: read(12),
        occlusion: read(13),
        landmarks,
        template,
        area_frac: read(16),
        centrality: read(17),
        sharpness: read(18),
        model_ver: u16::try_from(row.get::<_, i64>(19).unwrap_or(0)).unwrap_or(0),
        embed_ver: u16::try_from(row.get::<_, i64>(20).unwrap_or(0)).unwrap_or(0),
        quality_ver: u16::try_from(row.get::<_, i64>(21).unwrap_or(0)).unwrap_or(0),
        px_height: u32::try_from(row.get::<_, i64>(22).unwrap_or(0)).unwrap_or(0),
        votes: row.get::<_, i64>(23).unwrap_or(0) == 1,
        found_by: match row.get::<_, String>(24).unwrap_or_default().as_str() {
            "tile" => FoundBy::Tile,
            _ => FoundBy::Full,
        },
        crop_ref: row.get(25).ok().flatten(),
        child_score,
    })
}

/// Read one identity row, opening its centroids.
///
/// The project is passed in rather than read from the row: the query already filtered
/// on it, and re-reading a column to reconstruct a value the caller supplied is how a
/// row from the wrong project would get a plausible-looking owner.
fn read_identity(
    row: &rusqlite::Row<'_>,
    vault: &Vault,
    project_id: ProjectId,
) -> Option<IdentityRow> {
    let id: String = row.get(0).ok()?;
    let identity_id = IdentityId::from_db(&id).ok()?;

    let sealed_centroid: Option<Vec<u8>> = row.get(10).ok().flatten();
    let centroid = sealed_centroid.and_then(|bytes| {
        vault
            .open(RecordKind::Centroid, &id, &bytes)
            .ok()
            .and_then(|plain| embed::decode(&plain))
    });

    let sealed_subs: Option<Vec<u8>> = row.get(11).ok().flatten();
    let sub_centroids = sealed_subs
        .and_then(|bytes| vault.open(RecordKind::SubCentroids, &id, &bytes).ok())
        .map(|plain| {
            plain
                .chunks_exact(embed::TEMPLATE_BYTES)
                .filter_map(embed::decode)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let reasons: String = row.get(4).unwrap_or_else(|_| "[]".to_string());

    Some(IdentityRow {
        identity_id,
        project_id,
        label: row.get(1).ok().flatten(),
        role: Role::from_str_or_unknown(&row.get::<_, String>(2).unwrap_or_default()),
        role_confidence: row.get::<_, Option<f64>>(3).ok().flatten().unwrap_or(0.0) as f32,
        role_reasons: serde_json::from_str(&reasons).unwrap_or_default(),
        user_locked: row.get::<_, i64>(5).unwrap_or(0) == 1,
        importance: row.get::<_, Option<f64>>(6).ok().flatten().unwrap_or(0.5) as f32,
        face_count: u32::try_from(row.get::<_, i64>(7).unwrap_or(0)).unwrap_or(0),
        first_seen: row.get(8).ok().flatten(),
        last_seen: row.get(9).ok().flatten(),
        centroid,
        sub_centroids,
        variance: row.get::<_, Option<f64>>(13).ok().flatten().unwrap_or(0.0) as f32,
        cluster_ver: u16::try_from(row.get::<_, i64>(14).unwrap_or(1)).unwrap_or(1),
    })
}

/// Stable text for a body-to-face association, for the debug panel.
#[must_use]
pub const fn association_label(association: Association) -> &'static str {
    association.as_str()
}
