-- Migration 30: delivery - exports, manifests, backups, uploads, corrections, updates, consent.
--
-- PHASE-30 sections 5 and 6. Twelve tables, three views and six triggers.
--
-- The last migration, and the first whose rows are about things **outside the catalog**. Every
-- migration from 05 to 29 records a judgement about a photograph, a person, a camera body, a
-- gallery or the product's own work; all of those live in this file's own database and can be
-- recomputed from it. A row in `export_file` is about a JPEG on somebody's external drive, and a
-- row in `delivery_upload` is about a file on somebody else's server.
--
-- That changes what a row here is *for*. It is not a cache of a decision - it is the only record
-- that the thing happened at all.
--
-- ## Why the manifest is a table and a file
--
-- `delivery_manifest` is the catalog's copy and `aura-delivery-manifest.json` is the copy that
-- travels with the gallery. Both, because they answer different questions. The catalog copy is what
-- the panel reads and what a re-run compares against; the file copy is what survives the catalog
-- being lost, which is exactly the situation in which somebody needs to know what was delivered.
--
-- Phase 14 made the same call about edit recipes and the sidecars beside the RAWs, and its rollback
-- runbook is why: a migration that is not recomputable needs a second copy somewhere the migration
-- cannot reach.
--
-- ## Why `export_file.hash` has a CHECK and `bytes` does not
--
-- A hash of the wrong length is a hash nobody computed - a placeholder, an empty string, a
-- truncated write of the digest itself - and a manifest full of those looks exactly like a
-- manifest. `bytes` has no CHECK because a zero-byte file is a real thing that a verification
-- failure should report rather than a constraint should refuse.
--
-- ## Why `learn_correction.decision_id` is NOT NULL with a foreign key
--
-- Because a correction with no decision behind it is a residual measured from no baseline, which is
-- an absolute edit wearing a residual's shape. Phase 17 found that from the other side as its
-- condition C4; this is the phase that would carry it into every future wedding, so the refusal is
-- in the schema rather than in a validator. `AURA-LRN-11004` is what a caller meets.
--
-- ## Why `learn_update.adopted` has a trigger and not just a default
--
-- `learn_update_no_self_adopt` refuses an INSERT that arrives already adopted. Adoption is a
-- separate UPDATE by `LearnService::adopt`, which is the one code path a person's click reaches.
-- Section 10.1's "no learning update is adopted without explicit user action" is a property of the
-- database here, not only of the service - because a promise enforced in one layer lasts until
-- somebody writes a second caller, which phase 21 wrote down after finding it twice.
--
-- ## Why consent is a row per project and carries the app version
--
-- Section 6.3: "strictly opt-in per project with a clear consent record". A consent that is a
-- boolean in a settings file is a consent nobody can produce a year later. The app version is on
-- the row because a consent given to one release's wording is a consent to *that wording*, and a
-- privacy page that changes while the consent does not is a consent that has quietly become about
-- something else.
--
-- ROLLBACK. Every object here is new and nothing outside this file references it:
--
--   DROP VIEW    IF EXISTS v_delivery_state;
--   DROP VIEW    IF EXISTS v_learn_buckets;
--   DROP VIEW    IF EXISTS v_export_coverage;
--   DROP TABLE   IF EXISTS learn_consent;
--   DROP TABLE   IF EXISTS learn_profile_snapshot;
--   DROP TABLE   IF EXISTS learn_update_row;
--   DROP TABLE   IF EXISTS learn_update;
--   DROP TABLE   IF EXISTS learn_correction;
--   DROP TABLE   IF EXISTS delivery_upload;
--   DROP TABLE   IF EXISTS delivery_target;
--   DROP TABLE   IF EXISTS delivery_backup;
--   DROP TABLE   IF EXISTS delivery_manifest;
--   DROP TABLE   IF EXISTS export_reason;
--   DROP TABLE   IF EXISTS export_file;
--   DROP TABLE   IF EXISTS export_set;
--   DROP TABLE   IF EXISTS export_job;
--   DELETE FROM schema_version WHERE version = 30;
--
-- Running those returns the catalog to schema 29 and **loses the record of what was delivered**.
-- The files themselves are untouched - nothing in this phase can delete a delivered file - and
-- `aura-delivery-manifest.json` beside each delivery is the second copy that makes the rollback
-- survivable. Corrections are lost too, and a profile trained from them is not: the profile lives
-- in migration 17.
--
-- STORAGE. Measured rather than predicted, which phase 21 learned the hard way and phases 26, 28
-- and 29 restated. `crates/aura-perf/tests/delivery_budgets.rs` reports the figure and asserts the
-- bound as well as the number by running the same job over ten times the gallery.
--
-- The shape to expect: `export_file` is one row per delivered file and is the only table here that
-- grows with the wedding without a cap. Everything else is bounded - eight sets a job,
-- ROLLBACK_DEPTH snapshots a profile, one consent row a project - and `learn_correction` grows with
-- how much a photographer disagrees rather than with how many photographs they took.

-- ---------------------------------------------------------------------------
-- The job
-- ---------------------------------------------------------------------------

CREATE TABLE export_job (
    job_id             TEXT    PRIMARY KEY,
    project_id         TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

    -- `Destination::kind`: folder | nas | cloud_bucket | provider. The place itself is in
    -- `destination`, JSON, because a bucket has a prefix and a folder has a path and neither
    -- shape fits the other's columns.
    destination_kind   TEXT    NOT NULL
        CHECK (destination_kind IN ('folder','nas','cloud_bucket','provider')),
    destination        TEXT    NOT NULL,

    -- The `MetadataPolicy`, JSON. A policy is read whole or not at all, and lifting `strip_gps`
    -- into a column would invite a query that answers "was the location removed" for one job while
    -- the rest of the policy stayed unexamined.
    metadata_policy    TEXT    NOT NULL,

    -- Whether the job asked for the read-back check. On the row rather than derived from the files,
    -- because a job that wrote nothing still has an answer to this and a photographer still needs
    -- to see it. Section 6.1 and ADR-0061 decision 2.
    verify             INTEGER NOT NULL DEFAULT 1 CHECK (verify IN (0,1)),

    -- What happened. `sealed` means a manifest exists.
    status             TEXT    NOT NULL DEFAULT 'running'
        CHECK (status IN ('running','sealed','failed','cancelled')),

    files_written      INTEGER NOT NULL DEFAULT 0 CHECK (files_written >= 0),
    files_verified     INTEGER NOT NULL DEFAULT 0 CHECK (files_verified >= 0),
    bytes_written      INTEGER NOT NULL DEFAULT 0 CHECK (bytes_written >= 0),

    started_at         TEXT    NOT NULL,
    finished_at        TEXT,
    ms                 INTEGER NOT NULL DEFAULT 0 CHECK (ms >= 0),

    -- The versions this delivery was made by, JSON pairs. Phase 14's rule: a delivered file is
    -- re-creatable from four values, and three of them are here.
    engine_versions    TEXT    NOT NULL DEFAULT '[]',

    -- Which build wrote the row.
    app_version        TEXT    NOT NULL
);

CREATE INDEX idx_export_job_project ON export_job(project_id, started_at DESC);

-- One row per set inside a job. Section 5's `ExportSet`, minus its image list - the images are the
-- `export_file` rows, and storing the requested list separately would give a job two answers to
-- "what was in the gallery set" the moment one frame failed to render.
CREATE TABLE export_set (
    job_id             TEXT    NOT NULL REFERENCES export_job(job_id) ON DELETE CASCADE,
    name               TEXT    NOT NULL CHECK (length(name) BETWEEN 1 AND 64),

    format             TEXT    NOT NULL CHECK (format IN ('jpeg','tiff','png')),
    quality            INTEGER NOT NULL CHECK (quality BETWEEN 1 AND 100),
    colour_space       TEXT    NOT NULL CHECK (colour_space IN ('srgb','adobe_rgb','display_p3')),
    bit_depth          INTEGER NOT NULL CHECK (bit_depth IN (8,16)),
    resize             TEXT    NOT NULL,
    sharpen            TEXT    NOT NULL CHECK (sharpen IN ('none','screen','print')),
    naming             TEXT    NOT NULL CHECK (length(naming) BETWEEN 1 AND 200),
    sidecar            INTEGER NOT NULL DEFAULT 0 CHECK (sidecar IN (0,1)),

    requested          INTEGER NOT NULL CHECK (requested >= 0),
    written            INTEGER NOT NULL DEFAULT 0 CHECK (written >= 0),

    PRIMARY KEY (job_id, name)
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- What was written
-- ---------------------------------------------------------------------------

CREATE TABLE export_file (
    job_id             TEXT    NOT NULL REFERENCES export_job(job_id) ON DELETE CASCADE,
    set_name           TEXT    NOT NULL,
    photo_id           TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

    -- Relative to the destination root, always. An absolute path in this column would be a path
    -- that stops being true the moment the drive is mounted somewhere else, which for an external
    -- drive is every time.
    rel_path           TEXT    NOT NULL,

    bytes              INTEGER NOT NULL CHECK (bytes >= 0),

    -- BLAKE3 of the bytes **read back from the destination**. See the note at the top: a hash of
    -- the wrong length is a hash nobody computed.
    hash               TEXT    NOT NULL CHECK (length(hash) IN (0, 64)),

    width              INTEGER NOT NULL CHECK (width > 0),
    height             INTEGER NOT NULL CHECK (height > 0),

    -- Phase 14's four-input hash, so `AURA-RENDER-8007` can say which of the four moved.
    render_hash        TEXT    NOT NULL CHECK (length(render_hash) = 64),

    verified           INTEGER NOT NULL DEFAULT 1 CHECK (verified IN (0,1)),
    renamed            INTEGER NOT NULL DEFAULT 0 CHECK (renamed IN (0,1)),

    written_at         TEXT    NOT NULL,

    PRIMARY KEY (job_id, rel_path)
);

CREATE INDEX idx_export_file_photo ON export_file(photo_id, written_at DESC);
CREATE INDEX idx_export_file_set   ON export_file(job_id, set_name);

-- A file that says it was verified must carry a digest. The pair of columns can express
-- "verified with no hash", which is the one combination that would make a manifest a lie.
CREATE TRIGGER export_file_verified_needs_a_hash
BEFORE INSERT ON export_file
WHEN NEW.verified = 1 AND length(NEW.hash) <> 64
BEGIN
    SELECT RAISE(ABORT, 'a verified file must carry a 64-character digest');
END;

CREATE TRIGGER export_file_verified_needs_a_hash_upd
BEFORE UPDATE ON export_file
WHEN NEW.verified = 1 AND length(NEW.hash) <> 64
BEGIN
    SELECT RAISE(ABORT, 'a verified file must carry a 64-character digest');
END;

-- One reason table for the whole phase, with a subject discriminator. Phase 29's shape and the
-- same argument: a reason is the same shape in every case - a code, an optional measured half -
-- and four tables would be four places to forget the MAX_REASONS bound.
CREATE TABLE export_reason (
    subject_kind       TEXT    NOT NULL CHECK (subject_kind IN ('file','job','backup','upload')),
    subject_key        TEXT    NOT NULL,
    ix                 INTEGER NOT NULL CHECK (ix BETWEEN 0 AND 5),

    code               TEXT    NOT NULL,
    detail             TEXT,

    PRIMARY KEY (subject_kind, subject_key, ix)
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- The manifest
-- ---------------------------------------------------------------------------

CREATE TABLE delivery_manifest (
    job_id             TEXT    PRIMARY KEY REFERENCES export_job(job_id) ON DELETE CASCADE,
    project_id         TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

    created_at         INTEGER NOT NULL,

    files              INTEGER NOT NULL CHECK (files > 0),
    bytes              INTEGER NOT NULL CHECK (bytes >= 0),

    -- Phase 27's archived report, when there is one.
    qc_report_path     TEXT,

    -- Phase 24's disclosures, JSON. A removal that is not disclosed in the thing handed to the
    -- client is a removal nobody can audit, and this is the thing handed to the client.
    cleanup_disclosures TEXT   NOT NULL DEFAULT '[]',

    -- Where the travelling copy was written.
    manifest_path      TEXT    NOT NULL,

    -- Digest of the manifest document itself, so a manifest that was edited is detectable.
    manifest_hash      TEXT    NOT NULL CHECK (length(manifest_hash) = 64)
);

-- A manifest is sealed once and never edited. The document it describes is a claim about files
-- that exist; a manifest that could be updated in place would be a claim that changed after
-- somebody read it.
CREATE TRIGGER delivery_manifest_no_update
BEFORE UPDATE ON delivery_manifest
BEGIN
    SELECT RAISE(ABORT, 'a sealed delivery manifest is never edited; seal a new job instead');
END;

-- ---------------------------------------------------------------------------
-- Backup and upload
-- ---------------------------------------------------------------------------

CREATE TABLE delivery_target (
    target_id          TEXT    PRIMARY KEY,
    project_id         TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,

    kind               TEXT    NOT NULL CHECK (kind IN ('backup','provider')),

    -- A `Destination::kind` for a backup, a `ProviderId` for a provider.
    name               TEXT    NOT NULL,
    destination        TEXT    NOT NULL,

    -- Per-set mapping for a provider, JSON `[{set, remote, publish}]`. Empty for a backup, which
    -- takes the whole delivery by definition.
    mapping            TEXT    NOT NULL DEFAULT '[]',

    -- Whether the OS credential store holds a secret for this target. The secret itself never
    -- appears here, in a config file, or in a log. Phase 04's rule.
    has_credential     INTEGER NOT NULL DEFAULT 0 CHECK (has_credential IN (0,1)),

    created_at         TEXT    NOT NULL,

    UNIQUE (project_id, kind, name)
);

CREATE TABLE delivery_backup (
    target_id          TEXT    NOT NULL REFERENCES delivery_target(target_id) ON DELETE CASCADE,
    job_id             TEXT    NOT NULL REFERENCES export_job(job_id) ON DELETE CASCADE,
    rel_path           TEXT    NOT NULL,

    bytes              INTEGER NOT NULL CHECK (bytes >= 0),

    -- The digest of the copy, read back from the backup destination. Compared against
    -- `export_file.hash`; `diverged` is that comparison having failed, which is a different
    -- situation from the copy not being there.
    hash               TEXT    NOT NULL CHECK (length(hash) IN (0, 64)),
    diverged           INTEGER NOT NULL DEFAULT 0 CHECK (diverged IN (0,1)),
    already_present    INTEGER NOT NULL DEFAULT 0 CHECK (already_present IN (0,1)),

    copied_at          TEXT    NOT NULL,

    PRIMARY KEY (target_id, job_id, rel_path)
);

CREATE TABLE delivery_upload (
    target_id          TEXT    NOT NULL REFERENCES delivery_target(target_id) ON DELETE CASCADE,
    job_id             TEXT    NOT NULL REFERENCES export_job(job_id) ON DELETE CASCADE,
    rel_path           TEXT    NOT NULL,

    set_name           TEXT    NOT NULL,
    photo_id           TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

    bytes              INTEGER NOT NULL CHECK (bytes >= 0),
    hash               TEXT    NOT NULL CHECK (length(hash) = 64),

    -- `UploadState`. `corrupt` is deliberately not `failed`: a file that did not arrive and a file
    -- that arrived wrong need different responses, and only the second is worth re-sending at once.
    state              TEXT    NOT NULL DEFAULT 'pending'
        CHECK (state IN ('pending','in_progress','verified','corrupt','failed')),

    -- What the far end has acknowledged. **This column is why a network drop is a pause rather
    -- than a restart**: a resumed job re-sends the tail of one file.
    sent_bytes         INTEGER NOT NULL DEFAULT 0 CHECK (sent_bytes >= 0),
    resumes            INTEGER NOT NULL DEFAULT 0 CHECK (resumes >= 0),

    -- The transport's own code when the state is `failed`.
    failure_code       TEXT,

    updated_at         TEXT    NOT NULL,

    PRIMARY KEY (target_id, job_id, rel_path)
);

CREATE INDEX idx_delivery_upload_outstanding
    ON delivery_upload(target_id, state)
    WHERE state <> 'verified';

-- A file cannot claim to have sent more bytes than it has. Cheap, and it catches the resume
-- arithmetic getting a sign wrong - which would look like a completed upload.
CREATE TRIGGER delivery_upload_sent_within_bytes
BEFORE UPDATE ON delivery_upload
WHEN NEW.sent_bytes > NEW.bytes
BEGIN
    SELECT RAISE(ABORT, 'an upload cannot have sent more bytes than the file has');
END;

-- ---------------------------------------------------------------------------
-- The learning loop
-- ---------------------------------------------------------------------------

CREATE TABLE learn_correction (
    correction_id      TEXT    PRIMARY KEY,

    -- NOT NULL, with a foreign key. See the note at the top of this file: a correction with no
    -- decision behind it is an absolute edit wearing a residual's shape.
    decision_id        TEXT    NOT NULL REFERENCES decisions(decision_id) ON DELETE CASCADE,

    project_id         TEXT    NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
    photo_id           TEXT    NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

    kind               TEXT    NOT NULL CHECK (kind IN ('cull','edit','retouch','qc','curate','export')),
    learnable          TEXT    NOT NULL,
    scene              TEXT    NOT NULL,
    identity_id        TEXT REFERENCES identities(id) ON DELETE SET NULL,

    -- Whether the subject was somebody close to the couple. The bucket's fourth coordinate, and a
    -- *role* rather than a person: a profile that learned "brighten this specific bride" is a
    -- profile that is wrong on every subsequent wedding.
    subject_close      INTEGER NOT NULL DEFAULT 0 CHECK (subject_close IN (0,1)),

    before_json        TEXT    NOT NULL,
    after_json         TEXT    NOT NULL,
    magnitude          REAL    NOT NULL,

    -- Drawn deterministically from a hash of `decision_id`, never from a shuffle. ADR-0061
    -- decision 6: a shuffle re-draws the split on every fit, so a disappointing fit can be re-run
    -- until the line falls somewhere flattering, and nothing about that would look wrong.
    held_out           INTEGER NOT NULL DEFAULT 0 CHECK (held_out IN (0,1)),

    -- Which profile version consumed it, when one has.
    consumed_by        INTEGER,

    created_at         INTEGER NOT NULL
);

CREATE INDEX idx_learn_correction_bucket
    ON learn_correction(learnable, scene, subject_close, held_out);
CREATE INDEX idx_learn_correction_project ON learn_correction(project_id);

CREATE TABLE learn_update (
    update_id          TEXT    PRIMARY KEY,
    profile_id         TEXT    NOT NULL,

    from_version       INTEGER NOT NULL CHECK (from_version >= 0),
    to_version         INTEGER NOT NULL CHECK (to_version > 0),

    corrections_used   INTEGER NOT NULL CHECK (corrections_used >= 0),
    held_out_used      INTEGER NOT NULL CHECK (held_out_used >= 0),

    -- Measured on the held-out split, both sides on the *same* corrections.
    current_error      REAL    NOT NULL CHECK (current_error >= 0),
    candidate_error    REAL    NOT NULL CHECK (candidate_error >= 0),
    expected_improvement REAL  NOT NULL CHECK (expected_improvement BETWEEN 0 AND 1),

    diff_summary       TEXT    NOT NULL DEFAULT '[]',

    -- Set by `LearnService::adopt` and by nothing else. The trigger below refuses an INSERT that
    -- arrives already adopted.
    adopted            INTEGER NOT NULL DEFAULT 0 CHECK (adopted IN (0,1)),
    adopted_at         TEXT,

    computed_at        TEXT    NOT NULL,

    CHECK (to_version > from_version)
);

CREATE INDEX idx_learn_update_profile ON learn_update(profile_id, computed_at DESC);

-- Section 10.1: "no learning update is adopted without explicit user action". A property of the
-- database as well as of the service, because a promise enforced in one layer lasts until somebody
-- writes a second caller - which phase 21 wrote down after finding it twice.
CREATE TRIGGER learn_update_no_self_adopt
BEFORE INSERT ON learn_update
WHEN NEW.adopted <> 0
BEGIN
    SELECT RAISE(ABORT, 'a learning update is adopted by a person, never on insert');
END;

-- One row per changed value inside an update. What the A/B comparison renders.
CREATE TABLE learn_update_row (
    update_id          TEXT    NOT NULL REFERENCES learn_update(update_id) ON DELETE CASCADE,
    learnable          TEXT    NOT NULL,
    scene              TEXT    NOT NULL,

    current_value      REAL    NOT NULL,
    candidate_value    REAL    NOT NULL,
    corrections        INTEGER NOT NULL CHECK (corrections >= 0),

    summary            TEXT    NOT NULL,

    PRIMARY KEY (update_id, learnable, scene)
) WITHOUT ROWID;

-- What a rollback restores. The whole profile document, not a delta.
--
-- Phase 14 made the same call about `edit_history.body` and for the same reason: a chain of deltas
-- is exact only if every delta was computed against the state that actually preceded it, and a
-- rollback that is nearly right is worse than no rollback. Section 10.1 asks that rollback
-- "restores the previous profile exactly", and exactly is a byte comparison.
CREATE TABLE learn_profile_snapshot (
    profile_id         TEXT    NOT NULL,
    version            INTEGER NOT NULL CHECK (version > 0),

    body               TEXT    NOT NULL CHECK (length(body) > 2),
    body_hash          TEXT    NOT NULL CHECK (length(body_hash) = 64),

    taken_at           TEXT    NOT NULL,

    PRIMARY KEY (profile_id, version)
) WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Consent
-- ---------------------------------------------------------------------------

CREATE TABLE learn_consent (
    project_id         TEXT    PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,

    -- Four switches and not one. "May this machine learn from this wedding" and "may anonymised
    -- evidence leave it" are different questions, and collapsing them is how the second one
    -- happens by accident. All four default to off.
    local_learning       INTEGER NOT NULL DEFAULT 0 CHECK (local_learning IN (0,1)),
    dataset_contribution INTEGER NOT NULL DEFAULT 0 CHECK (dataset_contribution IN (0,1)),
    crash_reports        INTEGER NOT NULL DEFAULT 0 CHECK (crash_reports IN (0,1)),
    telemetry            INTEGER NOT NULL DEFAULT 0 CHECK (telemetry IN (0,1)),

    decided_at         INTEGER NOT NULL DEFAULT 0,

    -- A consent given to one release's wording is a consent to that wording.
    app_version        TEXT    NOT NULL
);

-- ---------------------------------------------------------------------------
-- Views
-- ---------------------------------------------------------------------------

-- What a project's last export covered. Three denominators, because a panel that measured an
-- 80-frame album export against a 4,000-frame project would report it as having missed 98 % of a
-- wedding it was never asked about.
CREATE VIEW v_export_coverage AS
SELECT
    j.project_id                                        AS project_id,
    j.job_id                                            AS job_id,
    (SELECT COUNT(*) FROM photo p WHERE p.project_id = j.project_id) AS photos,
    (SELECT COALESCE(SUM(s.requested), 0) FROM export_set s WHERE s.job_id = j.job_id) AS requested,
    j.files_written                                     AS written,
    j.files_verified                                    AS verified,
    j.files_written - j.files_verified                  AS unverified,
    j.bytes_written                                     AS bytes,
    j.verify                                            AS verify_requested,
    j.status                                            AS status
FROM export_job j;

-- Every bucket with a count, which is what the learning panel groups by and what
-- `MIN_CORRECTIONS` and `MIN_PROJECTS` are checked against.
CREATE VIEW v_learn_buckets AS
SELECT
    c.learnable                                         AS learnable,
    c.scene                                             AS scene,
    c.subject_close                                     AS subject_close,
    COUNT(*)                                            AS corrections,
    COUNT(DISTINCT c.project_id)                        AS projects,
    SUM(c.held_out)                                     AS held_out
FROM learn_correction c
GROUP BY c.learnable, c.scene, c.subject_close;

-- Where every file of a delivery has got to, at every target.
CREATE VIEW v_delivery_state AS
SELECT
    t.project_id                                        AS project_id,
    t.target_id                                         AS target_id,
    t.kind                                              AS target_kind,
    t.name                                              AS target_name,
    (SELECT COUNT(*) FROM delivery_upload u
        WHERE u.target_id = t.target_id AND u.state = 'verified')      AS uploaded,
    (SELECT COUNT(*) FROM delivery_upload u
        WHERE u.target_id = t.target_id AND u.state <> 'verified')     AS outstanding,
    (SELECT COALESCE(SUM(u.resumes), 0) FROM delivery_upload u
        WHERE u.target_id = t.target_id)                               AS resumes,
    (SELECT COUNT(*) FROM delivery_backup b
        WHERE b.target_id = t.target_id AND b.diverged = 0)            AS backed_up,
    (SELECT COUNT(*) FROM delivery_backup b
        WHERE b.target_id = t.target_id AND b.diverged = 1)            AS diverged
FROM delivery_target t;
