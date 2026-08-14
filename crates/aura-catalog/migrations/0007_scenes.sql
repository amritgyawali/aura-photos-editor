-- Migration 7 - what each photograph is of, and how the day was structured.
--
-- PHASE-07 section 4 names this file `0007_scenes.sql` and the four tables it must
-- contain: `image_scenes`, `segments`, `segment_images` and `scene_profiles`. All
-- four are here. Section 5 freezes the shapes of `SceneResult`, `Segment` and
-- `SceneProfile`; every column below either comes from one of those or is listed in
-- `docs/adr/ADR-0015-wedding-scene-taxonomy-and-story-segmentation.md`.
--
-- ---------------------------------------------------------------------------
-- WHAT THIS MIGRATION IS FOR
-- ---------------------------------------------------------------------------
--
-- Invariant 7 says "no threshold is global; every threshold is a function of the
-- detected scene and subject role". For six phases that has been a promise with
-- nothing behind it, because there was no scene. `scene_profiles` is the table that
-- makes it a join.
--
-- Three properties are enforced here rather than remembered:
--
-- 1. **Three version columns, because they invalidate three different things.**
--    `model_ver` invalidates the scene posterior, `taxonomy_ver` invalidates the
--    ritual slug, and `profile_ver` invalidates every threshold derived from a
--    scene. Comparing across any of them returns a plausible number that means
--    nothing. This is phase 06's rule, inherited literally: `AURA-ML-5022` exists so
--    that never happens silently.
--
-- 2. **`source = 'user'` is unbeatable.** Section 6.4: "anything the user touches is
--    `user_locked` and re-analysis never overwrites it". Both `image_scenes.source`
--    and `segments.user_locked` are checked *inside* the statement that would
--    overwrite them, exactly as `identities.user_locked` is in migration 6.
--
-- 3. **A chapter with a confidence carries reasons.** Invariant 2, as a CHECK the
--    database refuses to break, copied from `identities.role_reasons`.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it, so
-- the down migration is a list of drops, kept as a comment because the runner is
-- forward-only by design:
--
--   DROP VIEW  IF EXISTS v_chapter_summary;
--   DROP VIEW  IF EXISTS v_scene_coverage;
--   DROP TABLE IF EXISTS segment_images;
--   DROP TABLE IF EXISTS segments;
--   DROP TABLE IF EXISTS scene_profiles;
--   DROP TABLE IF EXISTS image_scenes;
--   DELETE FROM schema_version WHERE version = 7;
--
-- Running those returns the catalog to schema 6. Unlike migration 6's rollback this
-- one is cheap to reverse: nothing here is irreplaceable, because every row can be
-- recomputed from pixels the product still has.
--
-- STORAGE. Section 11 budgets 400 bytes of extra storage per image, and the measured
-- figure is 362: 80 bytes of prefixed UUIDs on the scene row, 66 for the scene slug,
-- ritual slug, source, tradition and timestamp, 48 for the padded top-3, 40 for the
-- numeric columns and SQLite's overhead, and 128 for the membership row.
--
-- The dominant cost is text ids, five per photograph across the two tables at 40
-- characters each. That is phase 01's decision; what this migration controls is the
-- top-3 encoding, which is why it is a list of pairs rather than of objects.
--
-- `crates/aura-perf/tests/scene_budgets.rs` asserts this against a real catalog rather
-- than against this comment - which is how the first draft's claim of 330 was found to
-- be 410.

-- ---------------------------------------------------------------------------
-- One row per classified photograph. Section 5's frozen `SceneResult`.
--
-- `scene` is stored as the 22-class slug rather than as an index, for the reason
-- migration 6 gives for storing role text: an integer whose meaning lives in a Rust
-- enum is a column that a support engineer cannot read and a newer build can
-- silently reinterpret.
--
-- `attrs` is the 14-bit field from `AttrFlags`, and `attrs_measured` is beside it
-- because zero is a *description* - outdoors, no flash, daylight, nobody around - and
-- "the head did not run" is not. Without the second column a caller cannot tell those
-- apart, and invariant 9 forbids that.
-- ---------------------------------------------------------------------------
CREATE TABLE image_scenes (
  photo_id       TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  -- Redundant with photo_id by design, exactly as `faces.project_id` is: a
  -- `WHERE project_id = ?` that cannot be forgotten is worth one text column.
  project_id     TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  -- One of the 22 slugs in `SceneId::ALL`, or 'unknown'.
  scene          TEXT    NOT NULL DEFAULT 'unknown',
  scene_conf     REAL    NOT NULL DEFAULT 0.0,
  -- JSON array of [slug, score] pairs, exactly three, strongest first. Padded
  -- with ["unknown",0] so the arity is a constant and the Explain panel never has
  -- to reason about a short list.
  --
  -- Pairs rather than objects, and it is a measured decision: repeating the key
  -- names "scene" and "score" three times per photograph costs 77 bytes, which is
  -- a fifth of section 11's whole 400-byte per-image budget. The slug stays text
  -- so the column is still readable by eye. See `story::segment::top3_json`.
  top3_json      TEXT    NOT NULL DEFAULT '[]',
  -- The 14 attribute bits. The two high bits of the u16 are reserved.
  attrs          INTEGER NOT NULL DEFAULT 0 CHECK (attrs >= 0 AND attrs < 16384),
  attrs_measured INTEGER NOT NULL DEFAULT 0 CHECK (attrs_measured IN (0,1)),
  -- The ritual's slug from a taxonomy file, never its authored id. See ADR-0015
  -- section 3: a catalog whose meaning depends on a config file that has since been
  -- edited is a catalog that lies.
  ritual         TEXT,
  ritual_conf    REAL    NOT NULL DEFAULT 0.0,
  -- Which tradition file named the ritual, when one did.
  tradition      TEXT,
  -- 'local', 'cloud' or 'user'. 'user' is never overwritten by automation.
  source         TEXT    NOT NULL DEFAULT 'local' CHECK (source IN ('local','cloud','user')),
  -- Scene classifier version. A bump re-classifies.
  model_ver      INTEGER NOT NULL,
  -- Which crop, resize and context-feature path the classifier saw.
  preprocess_ver INTEGER NOT NULL DEFAULT 1,
  -- Which ritual taxonomy produced `ritual`.
  taxonomy_ver   INTEGER NOT NULL DEFAULT 1,
  -- Which embedding the trunk read. Section 12's fourth failure mode: a scene head
  -- is versioned against the embedding it sits on, and a mismatch forces
  -- re-inference rather than a plausible wrong answer.
  embed_ver      INTEGER NOT NULL DEFAULT 0,
  elapsed_ms     REAL    NOT NULL DEFAULT 0.0,
  classified_at  TEXT    NOT NULL,
  -- A confident label must have a top-3 list behind it. Two characters is an empty
  -- JSON array, so `length > 2` is "the array has something in it".
  CHECK (scene_conf <= 0.0 OR length(top3_json) > 2)
) STRICT;

CREATE INDEX idx_image_scenes_project ON image_scenes(project_id, scene);
-- The segmentation pass reads exactly this: one project's labelled frames, in the
-- order two machines will both walk them in.
CREATE INDEX idx_image_scenes_walk    ON image_scenes(project_id, photo_id);
CREATE INDEX idx_image_scenes_locked  ON image_scenes(project_id, source);

-- ---------------------------------------------------------------------------
-- The thresholds every later phase reads. Section 5's frozen `SceneProfile`.
--
-- Shipped in `crates/aura-brain-wedding/config/scene_profiles.toml` and loaded into
-- this table per project, so a photographer can override one wedding's tolerances
-- without editing an installed file - section 6.3's "overridable per project".
--
-- `rationale` is NOT NULL and CHECKed non-empty on purpose. Section 12's third
-- failure mode is that this table becomes a dumping ground of magic numbers; a value
-- nobody can explain does not load, and `AURA-ML-5024` is what the loader raises.
-- ---------------------------------------------------------------------------
CREATE TABLE scene_profiles (
  project_id           TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  scene                TEXT    NOT NULL,
  keeper_min           REAL    NOT NULL,
  keeper_max           REAL    NOT NULL,
  max_acceptable_noise REAL    NOT NULL,
  max_acceptable_blur  REAL    NOT NULL,
  subject_focus_weight REAL    NOT NULL,
  emotion_weight       REAL    NOT NULL,
  composition_weight   REAL    NOT NULL,
  editing_intent       TEXT    NOT NULL DEFAULT 'neutral'
                       CHECK (editing_intent IN ('airy','neutral','warm','moody','punchy')),
  must_cover           INTEGER NOT NULL DEFAULT 0 CHECK (must_cover IN (0,1)),
  -- Why these numbers. PM owns it; the loader refuses a profile without it.
  rationale            TEXT    NOT NULL,
  -- 1 when the photographer changed this row for this wedding.
  user_edited          INTEGER NOT NULL DEFAULT 0 CHECK (user_edited IN (0,1)),
  profile_ver          INTEGER NOT NULL DEFAULT 1,
  updated_at           TEXT    NOT NULL,
  PRIMARY KEY (project_id, scene),
  CHECK (keeper_min >= 0.0 AND keeper_max <= 1.0 AND keeper_min <= keeper_max),
  CHECK (length(rationale) > 8)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- The story. Section 5's frozen `Segment`.
--
-- Ordered by `start_ts` within a project, and `ordinal` is stored rather than derived
-- because the chapter strip is drawn from this table in one read and section 11
-- budgets 200 ms for opening it.
--
-- `user_locked` is the same load-bearing column as `identities.user_locked`, and it
-- is set by four different edits - rename, move a boundary, split, merge - because
-- all four are the photographer saying the automation was wrong.
-- ---------------------------------------------------------------------------
CREATE TABLE segments (
  id             TEXT    NOT NULL PRIMARY KEY,
  project_id     TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  ordinal        INTEGER NOT NULL,
  -- One of the 9 slugs in `ChapterId::ALL`.
  chapter        TEXT    NOT NULL DEFAULT 'other'
                 CHECK (chapter IN ('getting_ready','details','ceremony','rituals',
                                    'portraits','reception','dance','exit','other')),
  -- The photographer's own name for this chapter, when they renamed it.
  label          TEXT,
  -- Milliseconds since the Unix epoch. See ADR-0015 section 2 for why not RFC 3339
  -- here: a boundary is arithmetic, and the 45-second gate is measured on it.
  start_ts       INTEGER NOT NULL,
  end_ts         INTEGER NOT NULL,
  dominant_scene TEXT    NOT NULL DEFAULT 'unknown',
  confidence     REAL    NOT NULL DEFAULT 0.0,
  key_frame      TEXT    REFERENCES photo(photo_id) ON DELETE SET NULL,
  image_count    INTEGER NOT NULL DEFAULT 0,
  -- JSON array of at most six short evidence strings. Invariant 2.
  reasons        TEXT    NOT NULL DEFAULT '[]',
  user_locked    INTEGER NOT NULL DEFAULT 0 CHECK (user_locked IN (0,1)),
  -- 1 when this chapter's label came from a cloud SegmentNaming answer.
  cloud_named    INTEGER NOT NULL DEFAULT 0 CHECK (cloud_named IN (0,1)),
  -- The tradition the ritual evidence supported, when it supported one.
  tradition      TEXT,
  -- The PELT penalty this run settled on. A support engineer reads it first when a
  -- wedding comes out with three chapters or thirty.
  penalty        REAL    NOT NULL DEFAULT 0.0,
  scene_ver      INTEGER NOT NULL DEFAULT 1,
  created_at     TEXT    NOT NULL,
  updated_at     TEXT    NOT NULL,
  CHECK (end_ts >= start_ts),
  CHECK (confidence <= 0.0 OR length(reasons) > 2)
) STRICT;

CREATE INDEX idx_segments_project ON segments(project_id, ordinal);
CREATE INDEX idx_segments_time    ON segments(project_id, start_ts);
CREATE INDEX idx_segments_locked  ON segments(project_id, user_locked);

-- ---------------------------------------------------------------------------
-- Which photographs are in which chapter.
--
-- A join table rather than a `segment_id` column on `image_scenes`, and the reason is
-- the two-shooter case in section 10.1: a second photographer's frames interleave
-- with the first's, and a re-segmentation rewrites membership for a whole chapter at
-- once. Deleting and re-inserting one project's rows here is one statement; updating
-- a column on four thousand `image_scenes` rows is four thousand.
--
-- It also keeps the two lifetimes separate. A scene label survives a re-segmentation;
-- a chapter membership does not.
-- ---------------------------------------------------------------------------
CREATE TABLE segment_images (
  segment_id TEXT    NOT NULL REFERENCES segments(id) ON DELETE CASCADE,
  photo_id   TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  -- Position within the chapter, in timeline order. Zero-based.
  position   INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (segment_id, photo_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_segment_images_photo   ON segment_images(photo_id);
CREATE INDEX idx_segment_images_project ON segment_images(project_id, segment_id, position);

-- ---------------------------------------------------------------------------
-- How much of a project has been read for scenes.
--
-- The same argument as `v_embedding_coverage` and `v_people_coverage`, for the third
-- time: a story drawn over a 40 %-classified wedding is a story about 40 % of a
-- wedding, and the timeline has to be able to say so.
-- ---------------------------------------------------------------------------
CREATE VIEW v_scene_coverage AS
SELECT p.project_id,
       COUNT(DISTINCT ph.photo_id)                                       AS photos,
       COUNT(DISTINCT isc.photo_id)                                      AS classified,
       SUM(CASE WHEN isc.source = 'user' THEN 1 ELSE 0 END)              AS user_labelled,
       SUM(CASE WHEN isc.ritual IS NOT NULL THEN 1 ELSE 0 END)           AS with_ritual,
       SUM(CASE WHEN isc.scene_conf >= 0.75 THEN 1 ELSE 0 END)           AS confident
FROM project p
LEFT JOIN photo ph        ON ph.project_id = p.project_id
LEFT JOIN image_scenes isc ON isc.photo_id = ph.photo_id
WHERE p.deleted_at IS NULL
GROUP BY p.project_id;

-- ---------------------------------------------------------------------------
-- One row per chapter, with the counts the timeline strip opens with.
--
-- Section 11 budgets 200 ms for the timeline. A view that aggregates once is what
-- makes that a read rather than a fan-out of one query per chapter card.
-- ---------------------------------------------------------------------------
CREATE VIEW v_chapter_summary AS
SELECT s.id,
       s.project_id,
       s.ordinal,
       s.chapter,
       s.label,
       s.start_ts,
       s.end_ts,
       (s.end_ts - s.start_ts) / 1000                    AS duration_s,
       s.dominant_scene,
       s.confidence,
       s.key_frame,
       s.reasons,
       s.user_locked,
       s.cloud_named,
       s.tradition,
       COUNT(si.photo_id)                                AS frames,
       COUNT(DISTINCT isc.scene)                         AS distinct_scenes
FROM segments s
LEFT JOIN segment_images si ON si.segment_id = s.id
LEFT JOIN image_scenes isc  ON isc.photo_id = si.photo_id
GROUP BY s.id;
