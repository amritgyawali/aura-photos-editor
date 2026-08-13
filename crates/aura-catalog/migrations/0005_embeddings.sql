-- Migration 5 - perceptual embeddings and the cheap descriptors computed with
-- them.
--
-- PHASE-05 section 4 names this file `0005_embeddings.sql` and section 5 freezes
-- the shape of what it stores. The two tables below hold exactly the frozen
-- `ImageEmbedding`, split by access pattern rather than by concept: the index
-- reads `embeddings` for every image at project open and never wants the
-- histograms, while phases 25 and 26 read `descriptors` for colour consistency
-- and camera matching and never want the vectors.
--
-- Why the version jumps from 4 to 5 with no 2 or 3: phases 02 and 03 added no
-- tables, so those versions were never used. See 0004_cloud_audit.sql.
--
-- ROLLBACK. Every object here is new and nothing outside this file references
-- it, so the down migration is three statements, kept as a comment because the
-- runner is forward-only by design:
--
--   DROP TABLE IF EXISTS descriptors;
--   DROP TABLE IF EXISTS embeddings;
--   DELETE FROM schema_version WHERE version = 5;
--
-- Running those returns the catalog to schema 4 with no loss of photographic
-- truth. Everything in these two tables is derived from pixels the product never
-- modifies and can be recomputed by re-running the embedding pass; the HNSW
-- snapshot beside them in the cache directory is a cache of a cache.
--
-- STORAGE. Section 11 budgets 1.6 KB per image for a vector plus its
-- descriptors. The payload here is 1,623 bytes: 1,024 for the vector, 8 for the
-- hash, 512 for the histogram, 48 for the six luminance statistics, 16 for the
-- two edge statistics and 15 for the palette. `crates/aura-perf` asserts it, and
-- the margin is 15 bytes - which is deliberate. A future column is a budget
-- conversation, not a quiet addition.

-- ---------------------------------------------------------------------------
-- One 512-d half-precision vector per photograph, plus the 64-bit difference
-- hash that answers the trivial duplicate questions without touching the index.
--
-- `model_ver` is on the row rather than in a settings key because section 12
-- names "embedding version drift invalidates stored vectors" as the failure mode
-- worth engineering against: a mixed catalog is normal during a background
-- re-embed, and a query that silently compared two model versions would return
-- plausible nonsense. The index reports the mismatch instead.
-- ---------------------------------------------------------------------------
CREATE TABLE embeddings (
  photo_id       TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id     TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  -- 1,024 bytes: 512 IEEE 754 binary16 values, little-endian, L2-normalised.
  vec            BLOB    NOT NULL,
  -- 64-bit difference hash, stored as a signed integer because SQLite has no
  -- unsigned type. Read back with a bit cast, never with a comparison.
  dhash          INTEGER NOT NULL,
  -- Which embedding model produced `vec`.
  model_ver      INTEGER NOT NULL,
  -- Which resize, colour and normalisation path produced the pixels it saw.
  preprocess_ver INTEGER NOT NULL DEFAULT 1,
  -- Which pixel tier the vector was computed from: 1 embedded, 2 proxy.
  pixel_tier     INTEGER NOT NULL DEFAULT 2 CHECK (pixel_tier IN (1,2,3)),
  -- 'embedded' or 'decoded'. Invariant: pixels carry their provenance, and a
  -- vector from a camera JPEG is not the same measurement as one from our render.
  pixel_source   TEXT    NOT NULL DEFAULT 'decoded' CHECK (pixel_source IN ('embedded','decoded')),
  created_at     TEXT    NOT NULL
) STRICT;

CREATE INDEX idx_embeddings_project ON embeddings(project_id, model_ver);
CREATE INDEX idx_embeddings_dhash   ON embeddings(project_id, dhash);

-- ---------------------------------------------------------------------------
-- The cheap descriptors, computed on the same decoded pixels in the same pass.
--
-- Section 6.2: they earn their keep by never needing the image opened again.
-- The histogram and the luminance percentiles are what phases 25 and 26 use for
-- gallery colour consistency and multi-camera matching; the edge energy is the
-- free global sharpness prior phase 09 refines with a real model; the palette is
-- what phase 29 uses to lay out an album.
-- ---------------------------------------------------------------------------
CREATE TABLE descriptors (
  photo_id       TEXT    NOT NULL PRIMARY KEY REFERENCES photo(photo_id) ON DELETE CASCADE,
  project_id     TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
  descriptor_ver INTEGER NOT NULL,
  -- 512 bytes: an 8x8x8 hue/saturation/value histogram, each bin scaled to 0..255.
  hsv_hist       BLOB    NOT NULL,
  -- All six in 0..1 against the 8-bit sRGB proxy.
  luma_mean      REAL    NOT NULL,
  luma_p1        REAL    NOT NULL,
  luma_p50       REAL    NOT NULL,
  luma_p99       REAL    NOT NULL,
  clip_lo        REAL    NOT NULL,
  clip_hi        REAL    NOT NULL,
  -- Mean gradient magnitude and its ninetieth percentile, both in 0..1.
  edge_energy    REAL    NOT NULL,
  edge_p90       REAL    NOT NULL,
  -- 15 bytes: five dominant colours, three bytes each, most frequent first.
  palette        BLOB    NOT NULL,
  created_at     TEXT    NOT NULL
) STRICT;

CREATE INDEX idx_descriptors_project ON descriptors(project_id, descriptor_ver);

-- ---------------------------------------------------------------------------
-- A convenience view for the debug panel and the phase gate: how much of a
-- project has been embedded. A view rather than a column, because coverage is
-- derived and a derived column is a cache with no invalidation rule.
-- ---------------------------------------------------------------------------
CREATE VIEW v_embedding_coverage AS
SELECT p.project_id,
       COUNT(DISTINCT ph.photo_id) AS photos,
       COUNT(DISTINCT e.photo_id)  AS embedded,
       COUNT(DISTINCT d.photo_id)  AS described
FROM project p
LEFT JOIN photo ph      ON ph.project_id = p.project_id
LEFT JOIN embeddings e  ON e.photo_id = ph.photo_id
LEFT JOIN descriptors d ON d.photo_id = ph.photo_id
WHERE p.deleted_at IS NULL
GROUP BY p.project_id;
