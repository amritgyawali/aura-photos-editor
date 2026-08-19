-- Migration 18 - which pixels of a photograph are her hair.
--
-- PHASE-18 section 4 names one table, `masks`, "with compressed payloads". It is here, plus one
-- the phase document does not name and section 6.4 requires: `mask_gate`, which records the one
-- thing this phase hands forward to phases 19 to 24 - how much a region is allowed to carry, and
-- why it is not allowed to carry more.
--
-- `docs/adr/ADR-0037-semantic-masks-matting-and-quality-gating.md` records the decisions behind
-- the columns and `ADR-0038` the ones behind what reaches a panel.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Every migration since 09 stored a judgement about a photograph. This one stores a claim about
-- *where in it*, and that is a different kind of row with a different set of ways to go wrong.
--
-- Eight properties are enforced here rather than remembered:
--
-- 1. **There is no image in this schema and nowhere for one to live.** `payload` is a run-length
--    bitmap or an eight-bit alpha plane - derived geometry about a region, never the pixels of
--    the photograph. There is no path column, no thumbnail, no crop and no URL. A support bundle
--    that exported this table would export shapes, which is what makes phase 13's "evidence can
--    never be a pixel" survive a phase whose whole output is pixel-shaped.
--
-- 2. **The storage budget is a property of the row, not of a report.** `bytes` is written by the
--    encoder rather than computed by a query, so section 11's 180 KB per image is
--    `SELECT SUM(bytes) ... GROUP BY image_id` and the CI budget test is one statement. A figure
--    that had to be recomputed from blobs is a figure nobody would run on a real gallery.
--
-- 3. **A photographer's mask is unbeatable, and the check is inside the statement that would
--    overwrite it.** `user_edited = 0` is in the `DELETE` a regeneration pass starts with -
--    `crates/aura-vision/src/mask/store.rs`, `MaskStore::put` - exactly as `moments.user_locked`
--    is in phase 08 and `identities.user_locked` is in phase 06. Third time, same shape.
--
-- 4. **Two version columns, because they invalidate two different things.** `model_ver`
--    invalidates *what a region is* - the class assignment - and `analysis_ver` invalidates
--    *where its boundary is* and how good it is. A change to the trimap band is not a change to
--    whether that is somebody's hair, and re-deciding every class because a radius moved is four
--    thousand photographs of work nobody asked for. `AURA-ML-5083` is the tenth version-drift
--    code in the product.
--
-- 5. **`kind_ix` is stored rather than derived.** The twenty classes have a frozen iteration
--    order and `ORDER BY kind` would sort it alphabetically, which puts `background` before
--    `face` and makes the panel's list order depend on English. One small integer column so a
--    list of regions reads the same in every locale.
--
-- 6. **An identity-scoped mask and its unscoped parent both exist.** `(image_id, kind,
--    identity_id)` is unique and `identity_id` is nullable, so "all the skin in this frame" and
--    "the bride's skin" are two rows rather than one row and a query. Phase 16's skin guard wants
--    the first and phase 25 wants the second, and a caller that had to union eleven rows back
--    together to get the first would eventually get it wrong.
--
-- 7. **The gate is stored, not recomputed.** `mask_gate` holds the allowance a region carried and
--    the reason it was limited, per operation. It is derived from `confidence` and `edge_quality`
--    and it is written down anyway, because the number a later phase *acted on* is part of the
--    record of why a photograph looks the way it does - and phase 13's ledger cites it.
--
-- 8. **Every mask carries at least one reason.** Invariant 2, as a CHECK rather than as a
--    convention: `length(reasons) > 0`. A region that cannot say why it is where it is is a
--    region whose uncertainty a photographer cannot act on.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW  IF EXISTS v_mask_coverage;
--   DROP TABLE IF EXISTS mask_gate;
--   DROP TABLE IF EXISTS masks;
--   DELETE FROM schema_version WHERE version = 18;
--
-- Running those returns the catalog to schema 17. **It is fully recomputable** - every mask is a
-- function of the photograph, phase 06's faces and this build's arithmetic - with exactly one
-- exception, and it is the important one: a mask a photographer brushed by hand is not derivable
-- from anything. The rollback runbook says to export the edited masks first, and
-- `SELECT * FROM masks WHERE user_edited = 1` is the whole of what needs exporting.

-- ---------------------------------------------------------------------------
-- One row per region of one photograph.
-- ---------------------------------------------------------------------------
CREATE TABLE masks (
  mask_id             TEXT    NOT NULL PRIMARY KEY,
  image_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

  -- One of the twenty in the frozen `MaskKind`. Stored as its slug so a catalog opened by a
  -- future build that added a class can still be read by this one - it will not know the slug,
  -- and `MaskKind::from_str` returning `None` drops the row rather than mis-reading it.
  kind                TEXT    NOT NULL,
  -- The class's position in the frozen iteration order. See note 5.
  kind_ix             INTEGER NOT NULL CHECK (kind_ix >= 0),
  -- Which person, or NULL for the unscoped region. See note 6. Deliberately NOT a foreign key
  -- to `identities`: a re-clustering rebuilds that table, and a mask that a re-clustering could
  -- cascade-delete would lose a photographer's brush work to an unrelated phase. Phase 12 made
  -- the same call about `selection.moment_id` for the same reason.
  identity_id         TEXT,

  -- 'rle' or 'alpha8'. Which of the two forms in note 1 the payload is in. Derived from `kind`
  -- by `MaskKind::stored_as` and stored anyway, so a payload written by an older build is
  -- decoded by what it *is* rather than by what this build thinks its class should be.
  form                TEXT    NOT NULL CHECK (form IN ('rle','alpha8')),
  payload_w           INTEGER NOT NULL CHECK (payload_w >= 0),
  payload_h           INTEGER NOT NULL CHECK (payload_h >= 0),
  payload             BLOB    NOT NULL,
  -- The payload's length, written by the encoder. See note 2.
  bytes               INTEGER NOT NULL CHECK (bytes >= 0),

  feather             REAL    NOT NULL DEFAULT 0.0
                        CHECK (feather >= 0.0 AND feather <= 1.0),
  -- How sure the class assignment is.
  confidence          REAL    NOT NULL DEFAULT 0.0
                        CHECK (confidence >= 0.0 AND confidence <= 1.0),
  -- How well determined the boundary is. Separate from `confidence` because they fail
  -- independently and are fixed by different things. ADR-0037 decision 6.
  edge_quality        REAL    NOT NULL DEFAULT 0.0
                        CHECK (edge_quality >= 0.0 AND edge_quality <= 1.0),
  -- The word for `edge_quality`, from the frozen `EdgeQuality`.
  edge                TEXT    NOT NULL DEFAULT 'unknown'
                        CHECK (edge IN ('matted','soft','binary','unknown')),

  -- Comma-separated codes from the frozen `MaskReason` vocabulary. Codes rather than sentences,
  -- for the reason phase 09 wrote down: a stored sentence is copy a release can change, and a
  -- catalog full of English cannot be translated. See note 8.
  reasons             TEXT    NOT NULL CHECK (length(reasons) > 0),

  -- See note 3. Never set by automation; `MaskStore::save_edit` is the one path that writes 1.
  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),

  -- See note 4.
  model_ver           INTEGER NOT NULL CHECK (model_ver    >= 0),
  analysis_ver        INTEGER NOT NULL CHECK (analysis_ver >= 0),

  -- One region of one kind per person per photograph. See note 6. `identity_id` is nullable and
  -- SQLite does not treat two NULLs as equal, so the unscoped row is kept unique by the partial
  -- index below rather than by this constraint.
  UNIQUE (image_id, kind, identity_id)
) STRICT;

-- Exactly one unscoped region per kind per photograph. The `UNIQUE` above cannot express this,
-- because two NULL `identity_id` values do not collide in SQLite.
CREATE UNIQUE INDEX idx_masks_unscoped
  ON masks(image_id, kind)
  WHERE identity_id IS NULL;

-- The panel reads one photograph's regions in frozen order. One index, because that is the query.
CREATE INDEX idx_masks_image ON masks(image_id, kind_ix);

-- The pending set: which selected frames have no mask at the current versions. Invariant 5 as a
-- query. Two columns because the resume and the model bump are the same question asked twice.
CREATE INDEX idx_masks_versions ON masks(model_ver, analysis_ver, image_id);

-- Phase 25 asks for one person's regions across a whole wedding, which is the only query that
-- starts from an identity. Partial, because most rows are unscoped and indexing their NULLs
-- would double the index for no reader.
CREATE INDEX idx_masks_identity
  ON masks(identity_id, kind_ix)
  WHERE identity_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- What each region was allowed to carry, per operation.
--
-- See note 7. This is the row phases 19 to 24 read before they apply anything, and the row phase
-- 13's ledger cites when a photographer asks why the skin smoothing did not happen on one frame.
-- ---------------------------------------------------------------------------
CREATE TABLE mask_gate (
  mask_id             TEXT    NOT NULL REFERENCES masks(mask_id) ON DELETE CASCADE,
  -- 'local_tone', 'skin_smooth', 'micro_retouch', 'restoration' or 'generative_cleanup', from
  -- the closed `quality::Operation`. A closed set rather than free text, because "is this
  -- operation aggressive" is a question this phase has to answer for operators that do not exist
  -- yet, and free text would let the caller decide.
  operation           TEXT    NOT NULL
                        CHECK (operation IN ('local_tone','skin_smooth','micro_retouch',
                                             'restoration','generative_cleanup')),
  -- The strength ceiling. Multiply by it; do not compare against it.
  ceiling             REAL    NOT NULL DEFAULT 0.0
                        CHECK (ceiling >= 0.0 AND ceiling <= 1.0),
  -- 0 when the operation is refused outright, which is only ever the two section 6.4 names.
  permitted           INTEGER NOT NULL DEFAULT 1 CHECK (permitted IN (0,1)),
  -- Why the ceiling is below one, as `MaskReason` codes. Empty when nothing is limiting, which
  -- is the one place in this schema where empty means "nothing to say" rather than "not
  -- measured" - `ceiling = 1.0` is what says it was measured.
  reasons             TEXT    NOT NULL DEFAULT '',

  PRIMARY KEY (mask_id, operation)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- How much of a project's gallery has masks, and how good they are.
--
-- **The denominator is selected frames, not every photograph**, and it is the first coverage view
-- in the product that is. Every one since phase 09 has counted against the whole project,
-- deliberately, because a verdict needs only pixels. A mask over a rejected frame is not a gap -
-- it is a frame nobody asked about, and reporting 12 % coverage on a wedding where every keeper
-- is masked would send a photographer looking for a bug that is a design decision.
--
-- `selected` and `masked` are both here rather than a ratio, so the denominator is visible.
-- ADR-0037 decision 8, and phase 08's rule: say what the denominator is.
-- ---------------------------------------------------------------------------
CREATE VIEW v_mask_coverage AS
SELECT
  s.project_id                                                   AS project_id,
  COUNT(DISTINCT s.photo_id)                                     AS selected,
  COUNT(DISTINCT m.image_id)                                     AS masked,
  COALESCE(SUM(CASE WHEN m.mask_id IS NOT NULL THEN 1 ELSE 0 END), 0) AS masks,
  COALESCE(SUM(m.user_edited), 0)                                AS user_edited,
  COALESCE(SUM(m.bytes), 0)                                      AS payload_bytes,
  COALESCE(AVG(m.confidence), 0.0)                               AS mean_confidence,
  COALESCE(AVG(m.edge_quality), 0.0)                             AS mean_edge_quality,
  COALESCE(SUM(CASE WHEN m.mask_id IS NOT NULL
                     AND (m.confidence * m.edge_quality) < 0.2025
                    THEN 1 ELSE 0 END), 0)                       AS low_quality
FROM selection s
LEFT JOIN masks m ON m.image_id = s.photo_id
GROUP BY s.project_id;
