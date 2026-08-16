-- Migration 11 - how each photograph is framed, and where the framing went wrong.
--
-- PHASE-11 section 4 names this file `0011_composition.sql` and the table it must
-- contain: `image_composition` with "flags and evidence". Both are here. Section 5
-- freezes `CompositionResult`; every column below either comes from that shape or is
-- listed in `docs/adr/ADR-0023-composition-rules-and-aesthetics.md`.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Phase 12 culls, phase 13 explains, phase 23 crops and phase 29 builds albums - and all
-- four need to know whether the frame is level, what the edge cut, where the subject sits
-- and what is going on behind them. This is the table that makes those four questions a
-- join.
--
-- Six properties are enforced here rather than remembered:
--
-- 1. **Nothing in this schema can crop, straighten or reject a photograph.** No `crop`
--    column, no `rotation`, no `culled`, no rank. `crop_hint` is spelled *hint* and its
--    JSON carries a region and a margin rather than a rectangle to apply. Section 2.2
--    puts the cropping in phase 23, the removal in phase 24 and the culling in phase 12.
--
-- 2. **`user_reviewed` is unbeatable.** A photographer who dismisses a framing note has
--    made a statement about the photograph, not about the build that analysed it. The
--    check is *inside* the upsert a re-analysis performs - not read first and branched on,
--    which leaves a window. Migrations 6, 7, 8 and 9 all do this; this is the fifth time.
--
-- 3. **Three version columns, because they invalidate three different things.**
--    `model_ver` invalidates the keypoints and the learned aesthetic; `analysis_ver`
--    invalidates the tilt, the crop audit, the placement, the background measures and the
--    composite; `rules_ver` invalidates every number that was compared against a *band*.
--    `AURA-ML-5043` exists so a comparison across any of them never happens silently.
--    Seventh phase, seventh version-drift code.
--
-- 4. **A judgement carries its reasons, and a reason carries its pixels.** Invariant 2 as
--    a CHECK the database refuses to break, and section 13's second criterion - "the
--    overlay shows exactly why a frame was marked badly composed" - as a required field
--    inside each reason's JSON object.
--
-- 5. **"Not checked" is not "well framed".** The absence of a row means nobody looked.
--    There is no `checked` boolean and no zero-row-means-fine convention, because phase 12
--    and phase 29 both read a clean judgement as evidence and would read a missing one the
--    same way. `AURA-ML-5045` exists to make the difference visible.
--
-- 6. **The three non-defect bits are stored like any other.** Bit 1 (intentional tilt),
--    bit 10 (centred) and bit 15 (no geometry) are counted by `v_composition_flags`
--    exactly as the violations are, because a photographer wants to filter *for* them.
--    `CompositionFlags::DEFECTS` is where the distinction lives, and it lives in one
--    place.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it, so the
-- down migration is a list of drops, kept as a comment because the runner is forward-only
-- by design:
--
--   DROP VIEW  IF EXISTS v_composition_flags;
--   DROP VIEW  IF EXISTS v_composition_coverage;
--   DROP TABLE IF EXISTS image_composition;
--   DELETE FROM schema_version WHERE version = 11;
--
-- Running those returns the catalog to schema 10. Everything it costs is recomputable
-- from the pixels except one thing - `user_reviewed` and the dismissals it records - so
-- the rollback runbook says to export those rows first.
--
-- STORAGE. Section 11 budgets **800 B per image**, which is a fifth tighter than phase
-- 09's kilobyte while carrying a comparable amount of evidence. Three decisions pay for
-- that, and all three are phase 09's lessons applied a phase later:
--
--   * a reason stores its **code**, not its sentence, and stores `text` only when the
--     sentence differs from `CompositionCode::user_text` - which is the case for exactly
--     three of the twenty-six codes, the ones that name an angle or a joint;
--   * the cuts, the intrusions and the bright blobs share **one** `violations` JSON
--     column rather than three, and empty arrays are omitted from it entirely;
--   * there is **one** index, and it is the one the review queue actually seeks on.
--
-- The measured figure is asserted by `crates/aura-perf/tests/composition_budgets.rs`
-- against a real catalog, not against this comment.

-- ---------------------------------------------------------------------------
-- One row per judged photograph. Section 5's frozen `CompositionResult`.
--
-- `project_id` is carried here for migration 9's reason: every query in phases 12, 23 and
-- 29 is scoped to a project, and the `WHERE project_id = ?` that cannot be forgotten is
-- worth its forty characters.
-- ---------------------------------------------------------------------------
CREATE TABLE image_composition (
  photo_id            TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id          TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

  -- --- The horizon. Section 6.1. ------------------------------------------
  -- Degrees off level, positive clockwise. Bounded at forty-five because past that the
  -- "horizon" found is the other axis of the frame, and an 89 degree tilt on a portrait
  -- frame is how a straightening tool rotates a wedding onto its side.
  tilt_deg            REAL    NOT NULL DEFAULT 0.0
                      CHECK (tilt_deg >= -45.0 AND tilt_deg <= 45.0),
  -- 1 when all four of section 6.1's conditions hold: a large tilt, a centred subject, no
  -- strong horizon, and a scene whose rule row permits it. The single most trust-critical
  -- bit in this phase - section 12's first failure mode is a rule engine that punishes
  -- creative work, and this is the bit that stops it.
  tilt_intentional    INTEGER NOT NULL DEFAULT 0 CHECK (tilt_intentional IN (0,1)),
  horizon_conf        REAL    NOT NULL DEFAULT 0.0
                      CHECK (horizon_conf >= 0.0 AND horizon_conf <= 1.0),
  -- 'none', 'gradient', 'vanishing_lines' or 'gravity'. Stored as the slug for the reason
  -- every migration since 6 gives: an integer whose meaning lives in a Rust enum is a
  -- column a support engineer cannot read. Here it is also the difference between a 0.4
  -- degree claim measured off a gravity tag and one measured off a gradient histogram.
  horizon_source      TEXT    NOT NULL DEFAULT 'none'
                      CHECK (horizon_source IN ('none','gradient','vanishing_lines','gravity')),

  -- --- Placement. Section 6.1. --------------------------------------------
  -- Space above the highest crown, as a fraction of frame height. **Negative when a head
  -- is cut**, and the depth of the cut is the magnitude - which is what phase 23 needs to
  -- know how far out to crop. Zero with bit 15 set means "not measured".
  headroom            REAL    NOT NULL DEFAULT 0.0
                      CHECK (headroom >= -1.0 AND headroom <= 1.0),
  -- Distance from the subject to the nearest power point, over the frame diagonal.
  thirds_offset       REAL    NOT NULL DEFAULT 0.0
                      CHECK (thirds_offset >= 0.0 AND thirds_offset <= 1.0),
  balance             REAL    NOT NULL DEFAULT 1.0
                      CHECK (balance >= 0.0 AND balance <= 1.0),
  negative_space      REAL    NOT NULL DEFAULT 0.0
                      CHECK (negative_space >= 0.0 AND negative_space <= 1.0),

  -- --- The crop audit. Section 6.1. ---------------------------------------
  head_crop           INTEGER NOT NULL DEFAULT 0 CHECK (head_crop IN (0,1)),

  -- --- The background. Section 6.2. ---------------------------------------
  -- Edge density relative to what this scene tolerates: 1.0 is exactly tolerable. Not an
  -- absolute density - the same crowded room behind a portrait and inside a reception
  -- frame must produce two different numbers, which is invariant 7 applied to a
  -- measurement rather than to a threshold. Phase 09's `noise_sigma_rel`, one phase later.
  clutter             REAL    NOT NULL DEFAULT 0.0 CHECK (clutter >= 0.0),
  head_merge          INTEGER NOT NULL DEFAULT 0 CHECK (head_merge IN (0,1)),
  colour_competition  REAL    NOT NULL DEFAULT 0.0
                      CHECK (colour_competition >= 0.0 AND colour_competition <= 1.0),

  -- --- Taste and the composite. Section 6.3. ------------------------------
  -- The learned, scene-conditioned reading. Stored beside the composite rather than
  -- folded into it so that a caller can see how much of a score was taste - and so that
  -- the follow-up in section 12 ("re-validate after Phase 18") can be measured against
  -- the frames it changed.
  aesthetic           REAL    NOT NULL DEFAULT 0.5
                      CHECK (aesthetic >= 0.0 AND aesthetic <= 1.0),
  composition_score   REAL    NOT NULL DEFAULT 0.0
                      CHECK (composition_score >= 0.0 AND composition_score <= 1.0),
  -- Where this frame's composition sits inside its moment, 0..1. The mission sentence as
  -- a number: "among six technically equal frames the best-composed one wins". 0.5 means
  -- "alone, or not in a moment" - neither the best nor the worst of no siblings.
  relative_composition REAL   NOT NULL DEFAULT 0.5
                      CHECK (relative_composition >= 0.0 AND relative_composition <= 1.0),
  -- How many people had usable keypoints. The per-frame numerator behind
  -- `CompositionOutline::keypoint_aware`, and the number that says whether the crop audit
  -- looked at anything at all.
  keypoint_subjects   INTEGER NOT NULL DEFAULT 0 CHECK (keypoint_subjects >= 0),

  -- --- Flags, evidence and reasons ----------------------------------------
  -- The sixteen bits of `CompositionFlags`. A stored bit pattern, so the positions in the
  -- contract are a wire format and a test asserts them.
  flags               INTEGER NOT NULL DEFAULT 0 CHECK (flags >= 0),
  -- JSON object with three optional arrays, each omitted when empty:
  --   {"cuts":[{"joint","edge","at_joint","severity","box":{x,y,w,h}}],
  --    "intrusions":[{x,y,w,h}],
  --    "blobs":[{x,y,w,h}]}
  -- One column rather than three, because a frame with no violations stores `{}` at two
  -- bytes instead of `[]` three times - and because the three are always read together by
  -- the overlay that draws them.
  violations          TEXT    NOT NULL DEFAULT '{}',
  -- JSON array of at most six objects: {code, weight, evidence:{x,y,w,h}|null, text?}.
  -- `text` is present only when the sentence differs from `CompositionCode::user_text`,
  -- which is the case for the three codes that name an angle or a joint. Invariant 2,
  -- plus section 13's evidence-box criterion.
  reasons             TEXT    NOT NULL DEFAULT '[]',
  -- JSON object or NULL: {"region":{x,y,w,h},"safe_margin":r,"straighten_deg":d|null,
  -- "confidence":c}. NULL when there is nothing to hand phase 23 - which is the honest
  -- answer for a frame with no subject and a horizon nobody could measure.
  crop_hint           TEXT,
  confidence          REAL    NOT NULL DEFAULT 0.0
                      CHECK (confidence >= 0.0 AND confidence <= 1.0),

  -- Which scene the bands were conditioned on, as the slug `image_scenes` uses. Invariant
  -- 7: a stored judgement that does not say which scene it assumed cannot be reproduced.
  -- 'unknown' means the neutral row was used.
  scene               TEXT    NOT NULL DEFAULT 'unknown',
  -- 1 when this scene had no rule row and the neutral one was used. The scene's *name* is
  -- already in `scene`, so this is one bit rather than a second copy of it - and it is the
  -- bit the coverage view sums.
  unruled             INTEGER NOT NULL DEFAULT 0 CHECK (unruled IN (0,1)),

  -- 1 when the photographer dismissed a flag here. Checked inside the upsert.
  user_reviewed       INTEGER NOT NULL DEFAULT 0 CHECK (user_reviewed IN (0,1)),
  -- The bits the photographer dismissed, kept separately from `flags` so that a
  -- re-analysis can re-apply the dismissal to a freshly computed set. Storing only the
  -- cleared `flags` would lose *which* judgement was overruled the moment the analyser
  -- stopped making it. Migration 9's fourth column, for its reason.
  dismissed           INTEGER NOT NULL DEFAULT 0 CHECK (dismissed >= 0),

  model_ver           INTEGER NOT NULL DEFAULT 0,
  analysis_ver        INTEGER NOT NULL DEFAULT 0,
  rules_ver           INTEGER NOT NULL DEFAULT 0,
  created_at          TEXT    NOT NULL,
  updated_at          TEXT    NOT NULL,

  -- A judgement with a confidence has reasons behind it. Two characters is an empty JSON
  -- array, so `length > 2` is "the array has something in it".
  CHECK (confidence <= 0.0 OR length(reasons) > 2),
  -- A head cannot be cut and have positive headroom at the same time. The one CHECK here
  -- that is about arithmetic rather than about range, and it is here because getting the
  -- sign of `headroom` backwards is the single easiest mistake in this table to make and
  -- the hardest to notice: a wrong sign produces plausible numbers everywhere and a crop
  -- hint that moves the frame the wrong way.
  CHECK (head_crop = 0 OR headroom <= 0.0)
) STRICT;

-- The review queue's query: this project's worst-framed frames first. **The only index on
-- this table**, and the two that phase 09 measured away are not repeated here.
--
--   * `(project_id, flags)` is not an index a filter chip can use. A chip asks
--     `flags & mask <> 0`, which is a bit test rather than a range, and SQLite cannot seek
--     a btree on it - so it would narrow the scan to one project, which this already does.
--   * `(project_id, model_ver, analysis_ver, rules_ver)` is not the re-analysis pass's
--     index either: `CompositionStore::pending` drives from `photo` LEFT JOIN this table,
--     so it is a lookup by `photo_id` per row and never a range scan over versions.
--
-- Both were written from a guess about a query plan in phase 09 and removed after being
-- measured. Section 11's 800 bytes is what made not repeating them worth writing down.
CREATE INDEX idx_composition_project ON image_composition(project_id, composition_score);

-- ---------------------------------------------------------------------------
-- How much of a project has been judged.
--
-- The seventh coverage view, after embeddings, people, scenes, moments, integrity and
-- emotion. The denominator is **every photograph**, as phase 09's and phase 10's are: a
-- horizon, a balance and a background reading need only pixels, and every photograph in a
-- catalog has pixels.
--
-- `keypoint_aware` is this phase's refinement of the rule, and it is the number that
-- matters when it is low. Six of the sixteen flags and the whole crop audit come from
-- keypoints; a wedding at 100 % coverage and 4 % keypoint-aware has had its horizon
-- measured everywhere and its limb cropping measured almost nowhere.
-- ---------------------------------------------------------------------------
CREATE VIEW v_composition_coverage AS
SELECT p.project_id,
       COUNT(DISTINCT ph.photo_id)                                AS photos,
       COUNT(DISTINCT ic.photo_id)                                AS scored,
       SUM(CASE WHEN ic.keypoint_subjects > 0 THEN 1 ELSE 0 END)  AS keypoint_aware,
       SUM(CASE WHEN ic.user_reviewed = 1 THEN 1 ELSE 0 END)      AS reviewed,
       SUM(CASE WHEN ic.unruled = 1 THEN 1 ELSE 0 END)            AS unruled,
       SUM(CASE WHEN ic.crop_hint IS NOT NULL THEN 1 ELSE 0 END)  AS hinted,
       AVG(ic.composition_score)                                  AS mean_score,
       MIN(ic.model_ver)                                          AS model_ver,
       MIN(ic.analysis_ver)                                       AS analysis_ver,
       MIN(ic.rules_ver)                                          AS rules_ver
FROM project p
LEFT JOIN photo ph             ON ph.project_id = p.project_id
LEFT JOIN image_composition ic ON ic.photo_id = ph.photo_id
WHERE p.deleted_at IS NULL
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- The flag histogram and the tilt telemetry, as one row per project.
--
-- Section 11's two events - `composition.scored {flag_histogram}` and `composition.tilt
-- {mean_abs_deg, intentional_ratio}` - in one scan. Sixteen sums plus two averages rather
-- than eighteen queries: the chips are drawn on every render of the grid header.
--
-- `mean_abs_tilt` is averaged over frames **with a measurable horizon** rather than over
-- every frame. Averaging in the zero an unmeasurable frame carries would make a wedding
-- shot entirely indoors look perfectly level, which is the opposite of what the number is
-- watched for.
-- ---------------------------------------------------------------------------
CREATE VIEW v_composition_flags AS
SELECT project_id,
       COUNT(*)                                                    AS scored,
       SUM(CASE WHEN flags &     1 <> 0 THEN 1 ELSE 0 END)         AS horizon_tilted,
       SUM(CASE WHEN flags &     2 <> 0 THEN 1 ELSE 0 END)         AS intentional_tilt,
       SUM(CASE WHEN flags &     4 <> 0 THEN 1 ELSE 0 END)         AS verticals_converging,
       SUM(CASE WHEN flags &     8 <> 0 THEN 1 ELSE 0 END)         AS headroom_tight,
       SUM(CASE WHEN flags &    16 <> 0 THEN 1 ELSE 0 END)         AS headroom_excessive,
       SUM(CASE WHEN flags &    32 <> 0 THEN 1 ELSE 0 END)         AS head_cropped,
       SUM(CASE WHEN flags &    64 <> 0 THEN 1 ELSE 0 END)         AS joint_cut,
       SUM(CASE WHEN flags &   128 <> 0 THEN 1 ELSE 0 END)         AS limb_cut,
       SUM(CASE WHEN flags &   256 <> 0 THEN 1 ELSE 0 END)         AS edge_intrusion,
       SUM(CASE WHEN flags &   512 <> 0 THEN 1 ELSE 0 END)         AS off_balance,
       SUM(CASE WHEN flags &  1024 <> 0 THEN 1 ELSE 0 END)         AS centred,
       SUM(CASE WHEN flags &  2048 <> 0 THEN 1 ELSE 0 END)         AS cluttered_background,
       SUM(CASE WHEN flags &  4096 <> 0 THEN 1 ELSE 0 END)         AS bright_blob,
       SUM(CASE WHEN flags &  8192 <> 0 THEN 1 ELSE 0 END)         AS head_merge,
       SUM(CASE WHEN flags & 16384 <> 0 THEN 1 ELSE 0 END)         AS colour_competition,
       SUM(CASE WHEN flags & 32768 <> 0 THEN 1 ELSE 0 END)         AS no_geometry,
       AVG(CASE WHEN horizon_source <> 'none' THEN abs(tilt_deg) END) AS mean_abs_tilt,
       SUM(CASE WHEN tilt_intentional = 1 THEN 1 ELSE 0 END)       AS tilted_intentional,
       SUM(CASE WHEN flags & 3 <> 0 THEN 1 ELSE 0 END)             AS tilted_any
FROM image_composition
GROUP BY project_id;
