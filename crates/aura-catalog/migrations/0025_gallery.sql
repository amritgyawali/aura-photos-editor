-- Migration 25 - the tree a wedding is matched inside, the frames each part is anchored to, how
-- far every other frame moves toward them, and the ones that would not come.
--
-- PHASE-25 section 4 names four tables: `scene_nodes`, `anchors`, `normalisation`, `outliers`.
-- What ships is five, with `gallery_` prefixes, and the fifth is the per-identity skin target
-- section 6.3 needs and section 4 does not name.
-- `docs/adr/ADR-0051-gallery-consistency-and-normalisation.md` records the rest.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Twenty-four migrations recorded facts about **one photograph**. `image_integrity` has a row per
-- frame; so do `image_emotion`, `image_composition`, `tone_estimate`, `colour_decision`,
-- `geometry_plan`, `retouch_plan` and `restore_plan`. Every one of them is a function whose domain
-- is a photograph.
--
-- This one records a fact about a **set** of them. `gallery_node` has a row per lighting group and
-- `gallery_delta` has a row per photograph that is meaningless on its own: `d_cct = -180` says
-- nothing at all until you know which node it is 180 K away from and what that node's anchors said.
-- That is the whole shape of the phase and it produces six properties enforced here rather than
-- remembered:
--
-- 1. **A delta is a residual and the schema cannot express an absolute.** Every movement column is
--    named `d_` and is CHECKed against the contract's own ceiling. The three `from_` columns beside
--    them record what the movement was measured from, and nothing reads them back - they exist so a
--    panel can draw the arrow and an audit can check it. There is no `cct_k` column on
--    `gallery_delta`, and a caller that wanted one would be trying to make this phase a second
--    answer to "what colour was the light", which is phase 15's question and phase 15's row.
--
-- 2. **The bounds are in the SQL, not only in the solver.** Five CHECK constraints, one per
--    `Bound::ALL`, at the contract's ceilings. `NormalisationDelta::within_bounds` refuses first;
--    this is the second layer, and it exists because the first lives in Rust a future caller could
--    route around with a raw INSERT. Section 10.1 makes "no frame exceeds documented maximum
--    movement" a gate, and a gate that can only be measured is weaker than one that cannot be
--    violated.
--
-- 3. **A pinned anchor survives automation, and it takes two statements to guarantee it.**
--    `gallery_anchor.user_pinned = 0` is inside the DELETE a re-selection starts with, *and* a
--    pinned image is skipped on re-insert. Phase 18 learned that the first alone is not enough:
--    `INSERT OR REPLACE` deletes the row it conflicts with, and this table has a unique key on
--    `(node_id, image_id)`. A rejection is as durable as a pin and is stored on the same row,
--    with a CHECK that refuses a frame that is somehow both.
--
-- 4. **Node membership is the delta table.** Section 5's `SceneNode::image_ids` is not a join
--    table: every frame placed in a node gets a `gallery_delta` row, including the frames that
--    moved by nothing, so `WHERE node_id = ?` is the membership query. A separate membership table
--    would have cost about 85 B per image - a fifth of section 11's whole 500 B budget - to store
--    a fact already on the row beside it.
--
-- 5. **A reason set is one integer.** `reasons` is a bitmask over `GalleryCode::ALL`, twenty-six
--    codes in a `u32`. Phase 09's rule was that a reason stores its code rather than its sentence;
--    this is that rule taken one step, because a list of slugs on the one table with a row per
--    photograph would have cost about sixty bytes and this costs eight. The *weight* is a property
--    of the code rather than of the frame - `GalleryCode::default_weight` - so it is rendered on
--    read like the sentence is.
--
-- 6. **An unanchored node and a consistent node are different rows.** `gallery_node.cct_k` is NULL
--    when the node could not be anchored, and every frame in it still gets a delta row carrying
--    `GalleryCode::NodeUnanchored` and five zeroes. A frame the product could not judge and a frame
--    it judged and left alone must never be the same query. Phase 24's rule - an absent input is
--    ignorance, not permission - in the phase where the two are easiest to confuse, because both
--    look like "nothing happened".
--
-- ---------------------------------------------------------------------------
-- STORAGE
-- ---------------------------------------------------------------------------
--
-- Section 11 budgets 500 B per image for the whole of this phase, which is the second tightest
-- figure in the product after phase 09's 1 KB. `crates/aura-perf/tests/gallery_budgets.rs` prints
-- the per-object breakdown on every run and the budget in `perf/budgets.toml` carries the
-- measurement plus headroom rather than being pinned at it - phase 19's correction, and phase 21's
-- defect is why the figure here was measured before it was written down.
--
-- ---------------------------------------------------------------------------
-- ROLLBACK
-- ---------------------------------------------------------------------------
--
-- Reversible: DROP the five tables, the two views and the three triggers. Nothing here alters an
-- earlier table and nothing earlier references these, so a downgrade loses this phase's decisions
-- and no other.

-- ---------------------------------------------------------------------------
-- The tree
-- ---------------------------------------------------------------------------

CREATE TABLE gallery_node (
  node_id             TEXT    NOT NULL PRIMARY KEY,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  segment_id          TEXT    NOT NULL REFERENCES segments(id) ON DELETE CASCADE,

  -- The node this one was split or sub-clustered out of. Deliberately not a foreign key onto
  -- itself: the only referential actions SQLite offers are a CASCADE that would delete a whole
  -- sub-tree when its parent is re-analysed, and a RESTRICT that would refuse the re-analysis.
  -- Phase 13 made the same argument about `decisions.supersedes`.
  parent_id           TEXT,

  -- What to call it. Derived from the segment's chapter plus an ordinal when the node is one of
  -- several inside it, so a photographer reads "Ceremony (2 of 3)" rather than a uuid.
  label               TEXT    NOT NULL,

  -- Its place among the nodes of its segment, zero first, in capture order.
  ordinal             INTEGER NOT NULL CHECK (ordinal >= 0),

  -- The scene it was built under. Invariant 7, and stored rather than re-read: a re-classification
  -- changes which policy row applied, and a node that silently starts describing itself under a
  -- new scene is a node nobody can audit. Phase 24 wrote this note for `cleanup_image.scene`.
  scene               TEXT    NOT NULL,

  -- How many frames it contains, and when the first of them was taken. The count is denormalised
  -- from `gallery_delta` because the panel lists every node and a COUNT per node is a scan per
  -- node; `first_ts` is what the list is ordered by.
  image_count         INTEGER NOT NULL DEFAULT 0 CHECK (image_count >= 0),
  first_ts            TEXT    NOT NULL,

  -- The target, or NULL on every column when the node could not be anchored. See note 6: the NULL
  -- is the claim, and a zero would be a different and false one.
  cct_k               REAL             CHECK (cct_k IS NULL OR (cct_k >= 2000.0 AND cct_k <= 50000.0)),
  cct_tol             REAL             CHECK (cct_tol IS NULL OR cct_tol >= 0.0),
  tint                REAL             CHECK (tint IS NULL OR (tint >= -150.0 AND tint <= 150.0)),
  tint_tol            REAL             CHECK (tint_tol IS NULL OR tint_tol >= 0.0),
  subject_luma        REAL             CHECK (subject_luma IS NULL OR (subject_luma >= 0.0 AND subject_luma <= 1.0)),
  luma_tol            REAL             CHECK (luma_tol IS NULL OR luma_tol >= 0.0),
  contrast            REAL             CHECK (contrast IS NULL OR (contrast >= -100.0 AND contrast <= 100.0)),
  saturation          REAL             CHECK (saturation IS NULL OR (saturation >= -100.0 AND saturation <= 100.0)),

  -- `NodeTarget::grade_signature`, eight numbers, as a JSON array. A blob rather than eight columns
  -- because nothing filters on a component of it: it is compared whole, by
  -- `NodeTarget::signature_distance`, and eight columns nobody queries is eight columns of index
  -- pressure for nothing.
  grade_signature     TEXT,

  anchor_count        INTEGER NOT NULL DEFAULT 0 CHECK (anchor_count >= 0 AND anchor_count <= 5),
  cohesion            REAL    NOT NULL DEFAULT 0.0 CHECK (cohesion >= 0.0 AND cohesion <= 1.0),

  -- Why the node is shaped the way it is. A bitmask over `GalleryCode::ALL`; see note 5.
  reasons             INTEGER NOT NULL DEFAULT 0 CHECK (reasons >= 0),

  -- The two versions. A change to either makes every node of the project stale, which is what
  -- makes the resumable pass a query rather than a journal. Invariant 5. There is no `model_ver`
  -- because this phase ships no model, and a column that can never change is a column that will
  -- eventually be compared against and mean nothing.
  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  policy_ver          INTEGER NOT NULL DEFAULT 0,

  created_at          TEXT    NOT NULL,

  -- A target is all-or-nothing: eight columns that describe one thing, and a row carrying three of
  -- them is a row somebody will read the other five off as zero.
  CHECK ((cct_k IS NULL) = (subject_luma IS NULL)),
  CHECK ((cct_k IS NULL) = (grade_signature IS NULL)),
  CHECK (cct_k IS NOT NULL OR anchor_count = 0)
);

CREATE INDEX idx_gallery_node_project ON gallery_node(project_id, first_ts);
CREATE INDEX idx_gallery_node_segment ON gallery_node(segment_id, ordinal);

-- ---------------------------------------------------------------------------
-- The anchors
-- ---------------------------------------------------------------------------

CREATE TABLE gallery_anchor (
  node_id             TEXT    NOT NULL REFERENCES gallery_node(node_id) ON DELETE CASCADE,
  photo_id            TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

  -- Its place in the node's ordering, zero best. A rejected frame keeps a rank so the panel can
  -- show what it would have been.
  rank                INTEGER NOT NULL CHECK (rank >= 0),

  -- How good an anchor it is, `0..1`. One is the best frame in the node.
  quality             REAL    NOT NULL DEFAULT 0.0 CHECK (quality >= 0.0 AND quality <= 1.0),

  -- See note 3. Both survive automation; neither is cleared by a re-selection.
  user_pinned         INTEGER NOT NULL DEFAULT 0 CHECK (user_pinned IN (0,1)),
  user_rejected       INTEGER NOT NULL DEFAULT 0 CHECK (user_rejected IN (0,1)),

  PRIMARY KEY (node_id, photo_id),

  -- A frame cannot be both chosen by hand and thrown out by hand. The UI cannot produce it; a raw
  -- INSERT could, and the row it would produce has no meaning.
  CHECK (NOT (user_pinned = 1 AND user_rejected = 1))
) WITHOUT ROWID;

CREATE INDEX idx_gallery_anchor_photo ON gallery_anchor(photo_id);

-- ---------------------------------------------------------------------------
-- The deltas
-- ---------------------------------------------------------------------------

CREATE TABLE gallery_delta (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  node_id             TEXT    NOT NULL REFERENCES gallery_node(node_id) ON DELETE CASCADE,

  -- See note 2. Five movements, five ceilings, straight off `Bound::ceiling`.
  d_exposure          REAL    NOT NULL DEFAULT 0.0 CHECK (d_exposure   >= -0.35  AND d_exposure   <= 0.35),
  d_cct               REAL    NOT NULL DEFAULT 0.0 CHECK (d_cct        >= -450.0 AND d_cct        <= 450.0),
  d_tint              REAL    NOT NULL DEFAULT 0.0 CHECK (d_tint       >= -12.0  AND d_tint       <= 12.0),
  d_contrast          REAL    NOT NULL DEFAULT 0.0 CHECK (d_contrast   >= -8.0   AND d_contrast   <= 8.0),
  d_saturation        REAL    NOT NULL DEFAULT 0.0 CHECK (d_saturation >= -6.0   AND d_saturation <= 6.0),

  -- See note 1. What the movement is a residual from. A record, never an input.
  from_exposure_ev    REAL    NOT NULL DEFAULT 0.0,
  from_cct_k          REAL    NOT NULL DEFAULT 0.0,
  from_tint           REAL    NOT NULL DEFAULT 0.0,

  damping             REAL    NOT NULL DEFAULT 0.7 CHECK (damping >= 0.30 AND damping <= 0.90),

  -- Which ceiling bit, or NULL when none did.
  bounded_by          TEXT             CHECK (bounded_by IS NULL OR bounded_by IN
                                       ('cct','tint','exposure','contrast','saturation')),

  confidence          REAL    NOT NULL DEFAULT 0.0 CHECK (confidence >= 0.0 AND confidence <= 1.0),

  -- The skin half. Six columns, all NULL together, because a correction without an identity is a
  -- correction to nobody's skin and a `d_uv` without a `de00` is a movement nobody measured.
  skin_identity       TEXT             REFERENCES identities(id) ON DELETE SET NULL,
  skin_du             REAL,
  skin_dv             REAL,
  skin_dluma          REAL             CHECK (skin_dluma IS NULL OR (skin_dluma >= -0.06 AND skin_dluma <= 0.06)),
  skin_de00_before    REAL             CHECK (skin_de00_before IS NULL OR skin_de00_before >= 0.0),
  skin_de00_after     REAL             CHECK (skin_de00_after  IS NULL OR skin_de00_after  >= 0.0),
  skin_cap            REAL             CHECK (skin_cap IS NULL OR skin_cap >= 0.0),
  skin_capped         INTEGER          CHECK (skin_capped IS NULL OR skin_capped IN (0,1)),

  -- A photographer's own values, and their own switch. Neither is cleared by automation; the check
  -- is inside the statement that would overwrite the row, exactly as `identities.user_locked`,
  -- `segments.user_locked`, `moments.user_locked` and `masks.user_edited` are.
  user_edited         INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  enabled             INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0,1)),

  reasons             INTEGER NOT NULL DEFAULT 0 CHECK (reasons >= 0),

  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  policy_ver          INTEGER NOT NULL DEFAULT 0,

  updated_at          TEXT    NOT NULL,

  CHECK ((skin_identity IS NULL) = (skin_du IS NULL)),
  CHECK ((skin_identity IS NULL) = (skin_de00_after IS NULL)),

  -- A frame the photographer switched off may not carry a movement. The panel cannot produce it;
  -- this is what stops a disabled frame quietly keeping the last delta it had.
  CHECK (enabled = 1 OR (d_exposure = 0.0 AND d_cct = 0.0 AND d_tint = 0.0
                         AND d_contrast = 0.0 AND d_saturation = 0.0 AND skin_identity IS NULL))
);

CREATE INDEX idx_gallery_delta_node    ON gallery_delta(node_id);
CREATE INDEX idx_gallery_delta_project ON gallery_delta(project_id, analysis_ver, policy_ver);

-- ---------------------------------------------------------------------------
-- The skin targets
-- ---------------------------------------------------------------------------

CREATE TABLE gallery_skin_target (
  identity_id         TEXT    NOT NULL PRIMARY KEY REFERENCES identities(id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- CIE 1976 u'v'. Measured from that person's own best-lit frames, and there is no column here,
  -- in `consistency.toml`, or anywhere in the code path for an *ideal* value to compare them
  -- against. Phase 15's rule, and the phase gate scans the schema for a violation on every run: a
  -- fixed target is how an editor lightens dark skin while believing it is correcting a cast, and
  -- a schema with nowhere to put such a constant cannot do it.
  u                   REAL    NOT NULL,
  v                   REAL    NOT NULL,
  luma                REAL    NOT NULL CHECK (luma >= 0.0 AND luma <= 1.0),

  frames              INTEGER NOT NULL DEFAULT 0 CHECK (frames >= 0),

  -- Section 6.3's promise, as two stored numbers rather than a sentence. "The same person's skin
  -- dE00 spread across the gallery is at or below 2.0" is
  -- `SELECT MAX(spread_after) FROM gallery_skin_target`, which is a query a support case can run
  -- and a document cannot. Phase 16 wrote this rule for its skin guard and phase 22 for its
  -- identity drift; this is its third application and the first at gallery scale.
  spread_before       REAL    NOT NULL DEFAULT 0.0 CHECK (spread_before >= 0.0),
  spread_after        REAL    NOT NULL DEFAULT 0.0 CHECK (spread_after  >= 0.0),

  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  updated_at          TEXT    NOT NULL
);

CREATE INDEX idx_gallery_skin_project ON gallery_skin_target(project_id);

-- ---------------------------------------------------------------------------
-- The outliers
-- ---------------------------------------------------------------------------

CREATE TABLE gallery_outlier (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  node_id             TEXT    NOT NULL REFERENCES gallery_node(node_id) ON DELETE CASCADE,

  -- What is **left after** the delta was applied, signed. Not the raw deviation: a frame 900 K
  -- from its node that the bound could only move 450 K is an outlier with a 450 K residual, and a
  -- frame 300 K away that was corrected in full is not an outlier at all. ADR-0051 section 7.
  residual_cct        REAL    NOT NULL DEFAULT 0.0,
  residual_tint       REAL    NOT NULL DEFAULT 0.0,
  residual_exposure   REAL    NOT NULL DEFAULT 0.0,
  residual_skin_de00  REAL    NOT NULL DEFAULT 0.0 CHECK (residual_skin_de00 >= 0.0),
  worst_identity      TEXT             REFERENCES identities(id) ON DELETE SET NULL,

  deviation           REAL    NOT NULL DEFAULT 0.0 CHECK (deviation >= 0.0 AND deviation <= 1.0),
  reasons             INTEGER NOT NULL DEFAULT 0 CHECK (reasons >= 0),

  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  created_at          TEXT    NOT NULL
);

-- What phase 27's QC queue reads: worst first, one project.
CREATE INDEX idx_gallery_outlier_queue ON gallery_outlier(project_id, deviation DESC);

-- ---------------------------------------------------------------------------
-- Views
-- ---------------------------------------------------------------------------

-- What the panel's project header shows. Two denominators rather than one, because a project at
-- 100 % coverage and 20 % anchored has had almost nothing normalised: an unanchored node produces
-- a zero delta for every frame in it, and a zero delta is still a row. Phase 05's rule - say what
-- the denominator is - in the phase where reading one number alone is most misleading.
CREATE VIEW v_gallery_coverage AS
SELECT
  p.project_id                                                          AS project_id,
  (SELECT COUNT(*) FROM photo WHERE project_id = p.project_id)          AS photos,
  (SELECT COUNT(*) FROM gallery_delta WHERE project_id = p.project_id)  AS normalised,
  (SELECT COUNT(*) FROM gallery_node  WHERE project_id = p.project_id)  AS nodes,
  (SELECT COUNT(*) FROM gallery_node
     WHERE project_id = p.project_id AND cct_k IS NOT NULL)             AS anchored_nodes,
  (SELECT COUNT(*) FROM gallery_outlier WHERE project_id = p.project_id) AS outliers,
  (SELECT COUNT(*) FROM gallery_anchor a
     JOIN gallery_node n ON n.node_id = a.node_id
     WHERE n.project_id = p.project_id AND a.user_pinned = 1)           AS pinned_anchors,
  (SELECT COUNT(*) FROM gallery_skin_target WHERE project_id = p.project_id) AS skin_targets
FROM project p;

-- The drift a photographer looks at: every frame that still does not match, worst first, with the
-- node it should have matched and the sentence section 6.4 asks for assembled from the residuals.
CREATE VIEW v_gallery_drift AS
SELECT
  o.photo_id            AS photo_id,
  o.project_id          AS project_id,
  n.label               AS node_label,
  o.deviation           AS deviation,
  o.residual_cct        AS residual_cct,
  o.residual_tint       AS residual_tint,
  o.residual_exposure   AS residual_exposure,
  o.residual_skin_de00  AS residual_skin_de00
FROM gallery_outlier o
JOIN gallery_node n ON n.node_id = o.node_id
ORDER BY o.deviation DESC;

-- ---------------------------------------------------------------------------
-- Triggers
-- ---------------------------------------------------------------------------

-- An outlier is a statement about a frame that was normalised. A row for a frame with no delta
-- would be a claim the pass never made, and phase 27 would open a QC ticket on it.
CREATE TRIGGER gallery_outlier_needs_delta
BEFORE INSERT ON gallery_outlier
FOR EACH ROW
WHEN (SELECT COUNT(*) FROM gallery_delta WHERE photo_id = NEW.photo_id) = 0
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5126: an outlier needs a normalisation delta');
END;

-- A pin is a photographer's decision about which frames define a look. Automation may re-rank the
-- anchors of a node and may not unpin one, so the UPDATE that a re-selection would run is refused
-- rather than ignored. Note 3's first statement; the second is in the Rust, because a DELETE this
-- trigger allowed would be a DELETE that removed the row instead of unsetting the flag.
CREATE TRIGGER gallery_anchor_pin_is_final
BEFORE UPDATE OF user_pinned ON gallery_anchor
FOR EACH ROW
WHEN OLD.user_pinned = 1 AND NEW.user_pinned = 0 AND OLD.node_id = NEW.node_id
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5124: a pinned anchor is not unpinned by automation');
END;

-- A skin target with no frames behind it is a target that looks like evidence. `MIN_SKIN_FRAMES`
-- is five and the Rust refuses first; this refuses the row that would exist if it did not.
CREATE TRIGGER gallery_skin_target_needs_frames
BEFORE INSERT ON gallery_skin_target
FOR EACH ROW
WHEN NEW.frames < 5
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5128: a skin target needs at least five frames');
END;
