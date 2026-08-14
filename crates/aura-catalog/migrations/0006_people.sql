-- Migration 6 - faces, identities and the evidence that ranks them.
--
-- PHASE-06 section 4 names this file `0006_people.sql` and section 5 freezes the
-- shape of `faces` and `identities`. Both are copied column for column; what is
-- added around them is listed in
-- `docs/adr/ADR-0013-people-intelligence-and-the-biometric-store.md` section 3,
-- and every addition is there because something in sections 6, 10 or 13 cannot be
-- satisfied without it.
--
-- ---------------------------------------------------------------------------
-- WHAT MAKES THIS MIGRATION DIFFERENT FROM EVERY OTHER ONE
-- ---------------------------------------------------------------------------
--
-- Everything else in this catalog is a fact about a photograph. This file stores
-- facts about *people*, which in most of the world is a regulated category, and
-- the schema is where that difference is enforced rather than remembered:
--
-- 1. **`faces.embed` holds ciphertext, never a vector.** The 512-d template is
--    sealed in an envelope keyed from the operating system's credential store
--    before it reaches this column. A catalog copied off the machine without the
--    keychain entry is a catalog with no biometric data in it. See
--    `crates/aura-people/src/vault.rs` for the envelope format and
--    `face_vault` below for the per-project record of which key sealed it.
--
-- 2. **Every people row carries `project_id`, and no query may join across
--    projects.** Section 2.2: cross-wedding identity persistence is not a feature
--    that is switched off, it is a feature that must be impossible. The column is
--    on `faces` even though `image_id` already implies it, because a `WHERE
--    project_id = ?` that cannot be forgotten is worth one redundant text column
--    per face. `tests/people_privacy.rs` asserts there is no path from one
--    project's identity to another's.
--
-- 3. **`user_locked` is load-bearing.** A photographer who says "this is the
--    bride" has ended the argument. Automation reads that column before it writes
--    a role, and `identity_links` keeps the append-only record that makes the
--    decision survive a full re-analysis and stay undoable.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it,
-- so the down migration is a list of drops, kept as a comment because the runner
-- is forward-only by design:
--
--   DROP VIEW  IF EXISTS v_identity_summary;
--   DROP VIEW  IF EXISTS v_people_coverage;
--   DROP TABLE IF EXISTS cooccurrence;
--   DROP TABLE IF EXISTS person_boxes;
--   DROP TABLE IF EXISTS identity_links;
--   DROP TABLE IF EXISTS identities;
--   DROP TABLE IF EXISTS faces;
--   DROP TABLE IF EXISTS face_scan;
--   DROP TABLE IF EXISTS face_vault;
--   DELETE FROM schema_version WHERE version = 6;
--
-- Running those returns the catalog to schema 5 and destroys every byte of
-- biometric data in it, which is the correct direction for this particular
-- rollback to be lossy in.
--
-- STORAGE. Section 11 budgets 25 MB of face store per 1,000 images. At the
-- measured 2.4 faces per wedding frame that is roughly 10 KB per face, and the
-- payload here is 1,239 bytes per face plus the encrypted 112x112 crop on disk.
-- `crates/aura-perf/tests/people_budgets.rs` asserts both.

-- ---------------------------------------------------------------------------
-- Which key sealed a project's biometric data, and whether it has been erased.
--
-- One row per project, created the first time a face is stored. `key_account` is
-- the name the credential store knows, never the key: a key in a catalog row is a
-- key in every backup of that catalog.
--
-- `erased_at` is the receipt for the "erase biometric data" control in section
-- 6.5. Erasure deletes the rows *and* records that it happened, because
-- "there are no faces" and "the faces were deliberately destroyed" are different
-- answers to a client who asks.
-- ---------------------------------------------------------------------------
CREATE TABLE face_vault (
  project_id   TEXT    NOT NULL PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,
  -- Account name in the OS credential store. Derived from the project id.
  key_account  TEXT    NOT NULL,
  -- Envelope format generation. A change here re-seals, never reinterprets.
  envelope_ver INTEGER NOT NULL DEFAULT 1,
  -- 32 bytes of per-project salt, mixed into every key derivation so two
  -- projects sealed with the same passphrase produce unrelated ciphertext.
  salt         BLOB    NOT NULL,
  created_at   TEXT    NOT NULL,
  erased_at    TEXT,
  erased_faces INTEGER NOT NULL DEFAULT 0
) STRICT;

-- ---------------------------------------------------------------------------
-- The resumability ledger: which photographs have been looked at.
--
-- Phase 05 could derive its remaining work from the presence of an `embeddings`
-- row, because every photograph yields exactly one vector. A face pass cannot:
-- "no faces in this frame" is a legitimate and common result, and a pipeline that
-- inferred work-remaining from missing `faces` rows would re-scan every landscape
-- and every detail shot on every run, for ever.
--
-- So the scan records itself. Invariant 5 is a query against this table, and
-- `tiled` is here because section 12 asks for the cost of the tiled pass to be
-- measured rather than assumed.
-- ---------------------------------------------------------------------------
CREATE TABLE face_scan (
  photo_id       TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id     TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  -- Detector version. A bump re-scans; comparing detections across versions is
  -- the same category of silent error AURA-ML-5015 exists to prevent.
  model_ver      INTEGER NOT NULL,
  -- Which crop, resize and normalisation path the detector saw.
  preprocess_ver INTEGER NOT NULL DEFAULT 1,
  faces_found    INTEGER NOT NULL DEFAULT 0,
  -- Faces that passed the quality gate and may vote on identity.
  faces_voting   INTEGER NOT NULL DEFAULT 0,
  person_boxes   INTEGER NOT NULL DEFAULT 0,
  -- 1 when the 2x2 tiled pass ran for this frame.
  tiled          INTEGER NOT NULL DEFAULT 0 CHECK (tiled IN (0,1)),
  pixel_tier     INTEGER NOT NULL DEFAULT 2 CHECK (pixel_tier IN (1,2,3)),
  pixel_source   TEXT    NOT NULL DEFAULT 'decoded'
                 CHECK (pixel_source IN ('embedded','decoded','unknown')),
  elapsed_ms     REAL    NOT NULL DEFAULT 0.0,
  scanned_at     TEXT    NOT NULL
) STRICT;

CREATE INDEX idx_face_scan_project ON face_scan(project_id, model_ver);

-- ---------------------------------------------------------------------------
-- One row per person the project believes exists. Section 5's frozen shape,
-- plus the sub-centroids section 6.2 asks for.
--
-- Declared before `faces` because `faces.identity_id` points at it. SQLite would
-- resolve a forward reference anyway; a reader should not have to know that.
--
-- `centroid` and `sub_centroids` are sealed with the same key as `faces.embed`:
-- a mean of biometric templates is a biometric template.
-- ---------------------------------------------------------------------------
CREATE TABLE identities (
  id              TEXT    NOT NULL PRIMARY KEY,
  project_id      TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  label           TEXT,
  role            TEXT    NOT NULL DEFAULT 'unknown'
                  CHECK (role IN ('bride','groom','couple','family_close','family_extended',
                                  'vip','guest','vendor','child','unknown')),
  role_confidence REAL    NOT NULL DEFAULT 0.0,
  -- JSON array of short evidence strings. Invariant 2: a role without reasons is
  -- a bug, and the CHECK below makes it one the database refuses.
  role_reasons    TEXT    NOT NULL DEFAULT '[]',
  user_locked     INTEGER NOT NULL DEFAULT 0 CHECK (user_locked IN (0,1)),
  importance      REAL    NOT NULL DEFAULT 0.5,
  face_count      INTEGER NOT NULL DEFAULT 0,
  first_seen      TEXT,
  last_seen       TEXT,
  -- SEALED. The mean template of this identity's voting faces.
  centroid        BLOB,
  -- SEALED. Concatenated sub-cluster means for an identity whose internal
  -- variance is high - the outfit and hairstyle change of section 6.2.
  sub_centroids   BLOB,
  sub_count       INTEGER NOT NULL DEFAULT 0,
  -- Mean cosine distance from members to `centroid`. What decides whether an
  -- identity needs sub-centroids, and what a support engineer reads first.
  variance        REAL    NOT NULL DEFAULT 0.0,
  cluster_ver     INTEGER NOT NULL DEFAULT 1,
  created_at      TEXT    NOT NULL,
  updated_at      TEXT    NOT NULL,
  -- A role above zero confidence must carry at least one reason. An empty JSON
  -- array is two characters, so `length > 2` is "the array has something in it".
  CHECK (role_confidence <= 0.0 OR length(role_reasons) > 2)
) STRICT;

CREATE INDEX idx_identities_project ON identities(project_id, role);
CREATE INDEX idx_identities_locked  ON identities(project_id, user_locked);

-- ---------------------------------------------------------------------------
-- One row per detected face. Section 5's frozen shape, plus provenance.
--
-- `x`, `y`, `w`, `h` are normalised to the frame, so a box survives a change of
-- preview tier. `yaw`, `pitch` and `roll` are degrees, estimated from the five
-- landmarks rather than predicted by a head of their own - a geometric estimate
-- from points the detector already produced is free, deterministic, and does not
-- pretend to a precision a placeholder model could not deliver.
--
-- `embed` is a sealed envelope. It is not a vector and must never be compared as
-- one; `aura_people::vault::Vault::open` is the only way in.
-- ---------------------------------------------------------------------------
CREATE TABLE faces (
  id             TEXT    NOT NULL PRIMARY KEY,
  image_id       TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  -- Redundant with image_id by design; see note 2 in this file's header.
  project_id     TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  identity_id    TEXT    REFERENCES identities(id) ON DELETE SET NULL,
  x              REAL    NOT NULL,
  y              REAL    NOT NULL,
  w              REAL    NOT NULL,
  h              REAL    NOT NULL,
  yaw            REAL    NOT NULL DEFAULT 0.0,
  pitch          REAL    NOT NULL DEFAULT 0.0,
  roll           REAL    NOT NULL DEFAULT 0.0,
  det_score      REAL    NOT NULL,
  quality        REAL    NOT NULL,
  blur           REAL    NOT NULL,
  occlusion      REAL    NOT NULL,
  -- Five points as [[x,y],...] normalised to the frame, in the ArcFace order:
  -- left eye, right eye, nose, left mouth corner, right mouth corner.
  landmarks_json TEXT    NOT NULL,
  -- SEALED. 1,024 bytes of fp16 template inside an envelope; see face_vault.
  embed          BLOB,
  area_frac      REAL    NOT NULL,
  centrality     REAL    NOT NULL,
  sharpness      REAL    NOT NULL,
  model_ver      INTEGER NOT NULL,
  -- Which recogniser produced `embed`. Separate from `model_ver`, which is the
  -- detector's: the two are versioned and replaced independently.
  embed_ver      INTEGER NOT NULL DEFAULT 0,
  -- Which quality head and gate decided `votes`.
  quality_ver    INTEGER NOT NULL DEFAULT 1,
  -- Longest side of the face in source pixels. The 48 px gate in section 6.1
  -- is a statement about the sensor, not about the preview it was found in.
  px_height      INTEGER NOT NULL DEFAULT 0,
  -- 1 when this face passed the quality gate and may vote on identity.
  votes          INTEGER NOT NULL DEFAULT 0 CHECK (votes IN (0,1)),
  -- 'full' for the whole-frame pass, 'tile' when only the tiled pass found it.
  found_by       TEXT    NOT NULL DEFAULT 'full' CHECK (found_by IN ('full','tile')),
  -- Relative path of the sealed 112x112 aligned crop under the project's cache.
  crop_ref       TEXT,
  created_at     TEXT    NOT NULL
) STRICT;

CREATE INDEX idx_faces_image    ON faces(image_id);
CREATE INDEX idx_faces_identity ON faces(identity_id);
-- The clustering pass reads exactly this: one project's voting faces, ordered so
-- that two machines walk them in the same order and produce the same skeleton.
CREATE INDEX idx_faces_voting   ON faces(project_id, votes, id);

-- ---------------------------------------------------------------------------
-- The append-only journal of every people decision a human made.
--
-- This table is why acceptance criterion "merging is undoable and never
-- overwritten by a re-analysis" is true. A re-analysis rebuilds clusters from
-- pixels; it then replays this journal over the result. Nothing here is ever
-- updated except `undone_at`, and nothing is ever deleted except by erasure.
-- ---------------------------------------------------------------------------
CREATE TABLE identity_links (
  link_id     INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
  project_id  TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  kind        TEXT    NOT NULL CHECK (kind IN ('merge','split','rename','role','importance')),
  -- The identity the action was applied to.
  identity_id TEXT    NOT NULL,
  -- For a merge, the identity that was absorbed; for a split, the one created.
  other_id    TEXT,
  -- For a split, the faces that moved, as a JSON array of face ids.
  face_ids    TEXT,
  -- Previous and new value, as text, for rename / role / importance.
  old_value   TEXT,
  new_value   TEXT,
  actor       TEXT    NOT NULL DEFAULT 'user' CHECK (actor IN ('user','cloud','auto')),
  created_at  TEXT    NOT NULL,
  undone_at   TEXT
) STRICT;

CREATE INDEX idx_identity_links_project ON identity_links(project_id, link_id);
CREATE INDEX idx_identity_links_target  ON identity_links(identity_id, link_id);

-- ---------------------------------------------------------------------------
-- Body boxes, and how each one was tied to a face.
--
-- Section 6.1: back-of-head and hair-covered cases still have to register a
-- person, or a people count in a wide reception frame is wrong and a later mask
-- has nothing to attach to. `face_id` is null exactly when the body has no face -
-- which is the case this table exists for.
-- ---------------------------------------------------------------------------
CREATE TABLE person_boxes (
  id          TEXT    NOT NULL PRIMARY KEY,
  image_id    TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id  TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  x           REAL    NOT NULL,
  y           REAL    NOT NULL,
  w           REAL    NOT NULL,
  h           REAL    NOT NULL,
  det_score   REAL    NOT NULL,
  face_id     TEXT    REFERENCES faces(id) ON DELETE SET NULL,
  -- How confident the association is, 0..1.
  assoc_score REAL    NOT NULL DEFAULT 0.0,
  -- 'contained' when the face box sits inside the upper body region,
  -- 'nearest' when it was the closest unclaimed face, 'none' when headless.
  assoc_kind  TEXT    NOT NULL DEFAULT 'none'
              CHECK (assoc_kind IN ('contained','nearest','none')),
  model_ver   INTEGER NOT NULL,
  created_at  TEXT    NOT NULL
) STRICT;

CREATE INDEX idx_person_boxes_image ON person_boxes(image_id);
CREATE INDEX idx_person_boxes_face  ON person_boxes(face_id);

-- ---------------------------------------------------------------------------
-- Who appears with whom, and where.
--
-- The evidence role inference actually runs on (section 6.3). Stored rather than
-- recomputed because the couple decision is re-read by phases 12, 25, 27 and 29,
-- and because a photographer who merges two identities must see the graph change
-- immediately rather than after a full pass.
--
-- The pair is canonical - `identity_a < identity_b` as text - so one pair is one
-- row and no arithmetic can double-count it.
-- ---------------------------------------------------------------------------
CREATE TABLE cooccurrence (
  project_id  TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  identity_a  TEXT    NOT NULL,
  identity_b  TEXT    NOT NULL,
  -- Frames both identities appear in.
  frames      INTEGER NOT NULL DEFAULT 0,
  -- JSON object of scene label to frame count. Empty until phase 07 assigns
  -- scenes; the column is here now because the pair key is here now.
  scenes      TEXT    NOT NULL DEFAULT '{}',
  first_seen  TEXT,
  last_seen   TEXT,
  updated_at  TEXT    NOT NULL,
  PRIMARY KEY (project_id, identity_a, identity_b),
  CHECK (identity_a < identity_b)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_cooccurrence_a ON cooccurrence(project_id, identity_a, frames DESC);
CREATE INDEX idx_cooccurrence_b ON cooccurrence(project_id, identity_b, frames DESC);

-- ---------------------------------------------------------------------------
-- How much of a project has been read for people.
--
-- The same argument as `v_embedding_coverage`: a grouping conclusion drawn over a
-- 40 %-scanned wedding is a conclusion about 40 % of a wedding, and the panel has
-- to be able to say so. A view rather than a column, because coverage is derived
-- and a derived column is a cache with no invalidation rule.
-- ---------------------------------------------------------------------------
CREATE VIEW v_people_coverage AS
SELECT p.project_id,
       COUNT(DISTINCT ph.photo_id) AS photos,
       COUNT(DISTINCT fs.photo_id) AS scanned,
       COALESCE(SUM(fs.faces_found), 0)  AS faces,
       COALESCE(SUM(fs.faces_voting), 0) AS voting_faces,
       COALESCE(SUM(fs.tiled), 0)        AS tiled_frames
FROM project p
LEFT JOIN photo ph     ON ph.project_id = p.project_id
LEFT JOIN face_scan fs ON fs.photo_id = ph.photo_id
WHERE p.deleted_at IS NULL
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- One row per identity, with the counts the People panel opens with.
--
-- Section 11 budgets 300 ms for the panel. A view that aggregates once is what
-- makes that a read rather than a fan-out of one query per identity card.
-- ---------------------------------------------------------------------------
CREATE VIEW v_identity_summary AS
SELECT i.id,
       i.project_id,
       i.label,
       i.role,
       i.role_confidence,
       i.role_reasons,
       i.user_locked,
       i.importance,
       i.variance,
       i.sub_count,
       COUNT(f.id)                                   AS faces,
       SUM(CASE WHEN f.votes = 1 THEN 1 ELSE 0 END)  AS voting_faces,
       COUNT(DISTINCT f.image_id)                    AS frames,
       MIN(ph.timeline_time)                         AS first_seen,
       MAX(ph.timeline_time)                         AS last_seen,
       AVG(f.quality)                                AS mean_quality,
       MAX(f.area_frac)                              AS max_area_frac
FROM identities i
LEFT JOIN faces f  ON f.identity_id = i.id
LEFT JOIN photo ph ON ph.photo_id = f.image_id
GROUP BY i.id;
