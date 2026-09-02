-- Migration 29: curation - monochrome candidates, the portfolio, the album, the sets, the teaser.
--
-- PHASE-29 sections 5 and 6. Eleven tables, two views and five triggers.
--
-- What this migration stores is a **proposal**. Every row here is a suggestion a photographer may
-- take, replace or ignore, and none of it changes a photograph: there is no recipe column, no path
-- column, no strength anywhere, and no `applied` flag. Phase 14's `Recipe.bw` block is where a
-- monochrome conversion lives once somebody has accepted one, and it is written by the develop
-- surface with a person behind it. ADR-0059 section 3.
--
-- ## Why the album is two tables and not one
--
-- `curate_album` is the plan and `album_spread` is its sequence. A single table with a nullable
-- spread index would make "this project has been curated" and "this project's album has spreads"
-- the same query, and they are not: a wedding whose gallery held forty frames gets a plan, a
-- coverage report and a sentence saying the album could not be filled - which is the answer, and
-- which a schema that stored only spreads could not express.
--
-- ## Why a spread has an id and a rank
--
-- `spread_id` is stable and `ix` renumbers. That split is the whole of section 13's "reordering is
-- instant and remembered": a drag rewrites every `ix` after the moved frame, and the accepted
-- pairing, the reasons and phase 30's eventual export record all keep pointing at the same
-- `spread_id`. A schema keyed by position would lose all three on the first drag.
--
-- ## Why there is no `caption_edited_by_user` column
--
-- Because there is no path by which a photographer's caption reaches this table. `caption` rows
-- come from the local template or from a cloud draft that passed the grounding check, and a caption
-- somebody typed is a caption the check never saw - which the delivery report would nonetheless
-- attribute to AURA. Editing a caption is a photographer typing into their own scheduler.
-- ADR-0060 section 6.
--
-- ## Why there is one reason table and not six
--
-- `curate_reason` carries a `subject_kind` discriminator, because a reason is the same shape in all
-- six cases - a code, a weight, an optional specific half of a sentence - and six tables would be
-- six places to forget the `MAX_REASONS` bound. The discriminator is checked, and the two triggers
-- that enforce invariant 2 read it.
--
-- ## Storage
--
-- Measured rather than predicted, which phase 21 learned the hard way and phases 26 and 28
-- restated. `crates/aura-perf/tests/curate_budgets.rs` reports the figure and asserts the *bound*
-- as well as the number, by running the same pass over ten times the gallery.
--
-- The shape to expect: everything here scales with the **gallery**, not with the project, and most
-- of it is bounded by a constant rather than by the gallery. Twenty heroes, at most 60 spreads, ten
-- grid frames, eight story frames, thirty teaser frames and at most 24 captions are all fixed;
-- only `curate_bw` grows with the wedding, and it is bounded by the candidate floor.

-- ---------------------------------------------------------------------------
-- The pass
-- ---------------------------------------------------------------------------

CREATE TABLE curate_run (
    project_id         TEXT PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,

    -- The denominator for everything in this phase: what phase 12 selected. Phase 18's rule rather
    -- than phases 09 to 15's - a frame nobody selected is not a gap in curation, it is a frame
    -- nobody asked about.
    selected           INTEGER NOT NULL DEFAULT 0 CHECK (selected >= 0),
    -- Selected frames curation could read at all.
    curated            INTEGER NOT NULL DEFAULT 0 CHECK (curated >= 0),
    -- Photographs in the project. Stored beside `selected` so that a project whose cull has not run
    -- is visibly different from one whose gallery is genuinely small.
    photos             INTEGER NOT NULL DEFAULT 0 CHECK (photos >= 0),

    bw_offered         INTEGER NOT NULL DEFAULT 0,
    heroes             INTEGER NOT NULL DEFAULT 0,
    chapters_covered   INTEGER NOT NULL DEFAULT 0,
    spreads            INTEGER NOT NULL DEFAULT 0,
    album_size         INTEGER NOT NULL DEFAULT 0,

    rhythm_score       REAL NOT NULL DEFAULT 0.0 CHECK (rhythm_score BETWEEN 0.0 AND 1.0),
    -- The share of the album whose shot scale could be measured at all. **Not a quality number.**
    -- A rhythm of 1.000 measured over eight per cent of an album is not a claim about the album,
    -- and on this build - where phase 06's detector finds no faces - eight per cent is the
    -- realistic figure. Phase 27's rule: clean and skipped are different values.
    rhythm_measurable  REAL NOT NULL DEFAULT 0.0 CHECK (rhythm_measurable BETWEEN 0.0 AND 1.0),
    pairing_score      REAL NOT NULL DEFAULT 0.0 CHECK (pairing_score BETWEEN 0.0 AND 1.0),

    facing_unknown     INTEGER NOT NULL DEFAULT 0,
    duplicates_refused INTEGER NOT NULL DEFAULT 0,
    slots_unfilled     INTEGER NOT NULL DEFAULT 0,
    captions_refused   INTEGER NOT NULL DEFAULT 0,

    -- Whether the cloud sequencing task was reached, and what came of it. Three columns rather than
    -- one because "the model was asked and agreed with us", "the model was asked and we refused it"
    -- and "the model was never reached" are three different facts about an album, and a
    -- photographer paying per call is entitled to tell them apart. ADR-0059 section 11.
    cloud_used         INTEGER NOT NULL DEFAULT 0 CHECK (cloud_used IN (0, 1)),
    cloud_applied      INTEGER NOT NULL DEFAULT 0 CHECK (cloud_applied >= 0),
    cloud_refused      INTEGER NOT NULL DEFAULT 0 CHECK (cloud_refused >= 0),

    -- Three version columns, because they invalidate three different things. `policy_ver` is
    -- `curation.toml`'s and invalidates every weight and quota; `analysis_ver` is this build's
    -- arithmetic and invalidates every score; `embed_ver` is phase 05's and invalidates the
    -- uniqueness term and every pairing similarity underneath it. `AURA-ML-5142` exists so a
    -- comparison across any of them never happens silently. Phase 08's rule, seventh application.
    policy_ver         INTEGER NOT NULL DEFAULT 0,
    analysis_ver       INTEGER NOT NULL DEFAULT 0,
    embed_ver          INTEGER NOT NULL DEFAULT 0,

    -- Whether either of this phase's two heads is trained. False in this build, and a column rather
    -- than a build constant because a catalog outlives a build: a wedding curated by a deterministic
    -- solver and one curated by a trained ranker are not comparable, and a support case a year from
    -- now has to be able to tell which it is looking at.
    heads_trained      INTEGER NOT NULL DEFAULT 0 CHECK (heads_trained IN (0, 1)),

    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,

    CHECK (curated <= selected)
) STRICT;

-- ---------------------------------------------------------------------------
-- Monochrome
-- ---------------------------------------------------------------------------

CREATE TABLE curate_bw (
    project_id      TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
    image_id        TEXT NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

    score           REAL NOT NULL CHECK (score BETWEEN 0.0 AND 1.0),
    confidence      REAL NOT NULL CHECK (confidence BETWEEN 0.0 AND 1.0),

    -- The five measured terms of section 6.1, stored rather than derived, because "why does this
    -- frame suit black and white" is answered by which of the five is high and a panel with only a
    -- score leaves a photographer comparing two numbers that differ by 0.02.
    tonal_separation REAL NOT NULL CHECK (tonal_separation BETWEEN 0.0 AND 1.0),
    colour_distraction REAL NOT NULL CHECK (colour_distraction BETWEEN 0.0 AND 1.0),
    gesture         REAL NOT NULL CHECK (gesture BETWEEN 0.0 AND 1.0),
    emotion         REAL NOT NULL CHECK (emotion BETWEEN 0.0 AND 1.0),
    grain           REAL NOT NULL CHECK (grain BETWEEN 0.0 AND 1.0),

    -- The mix, one column per band in the recipe's own order. Eight columns rather than a blob
    -- because `SELECT MAX(ABS(mix_orange))` is how the phase gate proves the skin bound held, and a
    -- blob would make that promise a sentence in a document again. Phase 16's rule: a guarantee is
    -- measured, not asserted.
    mix_red         INTEGER NOT NULL DEFAULT 0 CHECK (mix_red BETWEEN -70 AND 70),
    mix_orange      INTEGER NOT NULL DEFAULT 0 CHECK (mix_orange BETWEEN -70 AND 70),
    mix_yellow      INTEGER NOT NULL DEFAULT 0 CHECK (mix_yellow BETWEEN -70 AND 70),
    mix_green       INTEGER NOT NULL DEFAULT 0 CHECK (mix_green BETWEEN -70 AND 70),
    mix_aqua        INTEGER NOT NULL DEFAULT 0 CHECK (mix_aqua BETWEEN -70 AND 70),
    mix_blue        INTEGER NOT NULL DEFAULT 0 CHECK (mix_blue BETWEEN -70 AND 70),
    mix_purple      INTEGER NOT NULL DEFAULT 0 CHECK (mix_purple BETWEEN -70 AND 70),
    mix_magenta     INTEGER NOT NULL DEFAULT 0 CHECK (mix_magenta BETWEEN -70 AND 70),

    -- Which bands somebody's *measured* skin locus put them in, as a comma-separated list of band
    -- indices. Empty when nobody in the frame has a usable locus - which is not the same as "no
    -- people in the frame", and is why `skin_locus_unavailable` is a reason code of its own.
    --
    -- There is no skin *target* here and there is no column one could go in. The band is looked up
    -- per identity from phase 15's `ToneService::skin_loci`; a stored target would be the constant
    -- `docs/skin-fairness.md` says this product does not have. The phase gate scans this schema for
    -- one on every run, as phases 15, 25 and 27 scan their own.
    skin_bands      TEXT NOT NULL DEFAULT '',

    PRIMARY KEY (project_id, image_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_curate_bw_rank ON curate_bw(project_id, score DESC);

-- ---------------------------------------------------------------------------
-- The portfolio
-- ---------------------------------------------------------------------------

CREATE TABLE curate_hero (
    project_id      TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
    image_id        TEXT NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

    rank            INTEGER NOT NULL CHECK (rank >= 0),
    score           REAL NOT NULL CHECK (score BETWEEN 0.0 AND 1.0),
    confidence      REAL NOT NULL CHECK (confidence BETWEEN 0.0 AND 1.0),

    technical       REAL NOT NULL CHECK (technical BETWEEN 0.0 AND 1.0),
    emotion         REAL NOT NULL CHECK (emotion BETWEEN 0.0 AND 1.0),
    composition     REAL NOT NULL CHECK (composition BETWEEN 0.0 AND 1.0),
    uniqueness      REAL NOT NULL CHECK (uniqueness BETWEEN 0.0 AND 1.0),
    story           REAL NOT NULL CHECK (story BETWEEN 0.0 AND 1.0),

    chapter         TEXT NOT NULL,
    moment_id       TEXT,
    scale           TEXT NOT NULL DEFAULT 'unknown'
                    CHECK (scale IN ('wide', 'medium', 'tight', 'unknown')),

    -- Which diversity constraint was binding when this pick was made. "Why is this one a hero and
    -- that one not" is answered by the constraint far more often than by the score.
    binding         TEXT NOT NULL DEFAULT 'unconstrained'
                    CHECK (binding IN ('unconstrained', 'chapter_quota', 'moment_exhausted',
                                       'scale_quota')),

    -- The veto is a schema constraint as well as a code path. A hero below the floor is not a worse
    -- candidate, it is not a candidate - phase 12's word, and this is where a bug that forgot it
    -- fails loudly rather than shipping a soft photograph to somebody's website.
    CHECK (technical >= 0.55),

    PRIMARY KEY (project_id, image_id)
) STRICT, WITHOUT ROWID;

CREATE UNIQUE INDEX idx_curate_hero_rank ON curate_hero(project_id, rank);

-- ---------------------------------------------------------------------------
-- The album
-- ---------------------------------------------------------------------------

CREATE TABLE curate_album (
    project_id      TEXT PRIMARY KEY REFERENCES project(project_id) ON DELETE CASCADE,

    target_size     INTEGER NOT NULL CHECK (target_size BETWEEN 60 AND 120),
    size            INTEGER NOT NULL DEFAULT 0 CHECK (size >= 0),

    rhythm_score      REAL NOT NULL DEFAULT 0.0 CHECK (rhythm_score BETWEEN 0.0 AND 1.0),
    rhythm_measurable REAL NOT NULL DEFAULT 0.0 CHECK (rhythm_measurable BETWEEN 0.0 AND 1.0),
    pairing_score     REAL NOT NULL DEFAULT 0.0 CHECK (pairing_score BETWEEN 0.0 AND 1.0),

    -- Set when a photographer reorders by hand, and never cleared by a pass. The operating manual's
    -- fifth code rule, and the flag `curate_album_no_reorder` reads.
    user_ordered    INTEGER NOT NULL DEFAULT 0 CHECK (user_ordered IN (0, 1)),
    -- How many times. Section 11's `curate.user_reorder` telemetry, and phase 30's learning signal:
    -- an album reordered nine times is an album the product got wrong nine times.
    reorders        INTEGER NOT NULL DEFAULT 0 CHECK (reorders >= 0),

    -- The album's own coverage report, as the same three shapes phase 12 stores. `must_haves` is a
    -- JSON object of rule slug to coverage state and `warnings` a JSON array, because both are
    -- lists of variable length that nothing queries by element - the queries are "is the album
    -- covered" (the counts below) and "show me the report" (the whole thing at once).
    --
    -- Computed over the **album**, never over the gallery. Phase 12 already reported that the
    -- gallery covers the ring exchange; the question here is whether the album does.
    must_haves      TEXT NOT NULL DEFAULT '{}',
    warnings        TEXT NOT NULL DEFAULT '[]',
    covered         INTEGER NOT NULL DEFAULT 0 CHECK (covered >= 0),
    missing         INTEGER NOT NULL DEFAULT 0 CHECK (missing >= 0),

    updated_at      TEXT NOT NULL
) STRICT;

CREATE TABLE album_spread (
    spread_id       TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL REFERENCES curate_album(project_id) ON DELETE CASCADE,

    -- The position. Renumbers on every drag; `spread_id` does not.
    ix              INTEGER NOT NULL CHECK (ix >= 0),

    left_image      TEXT REFERENCES photo(photo_id) ON DELETE CASCADE,
    right_image     TEXT REFERENCES photo(photo_id) ON DELETE CASCADE,
    single          INTEGER NOT NULL DEFAULT 0 CHECK (single IN (0, 1)),

    chapter         TEXT NOT NULL,

    -- The four pairing measurements of section 6.3, and the combined score. All four rather than
    -- the score alone, because a photographer who disagrees with a pairing wants to know which of
    -- the four the optimiser was happy with.
    tonal_gap       REAL NOT NULL DEFAULT 0.0 CHECK (tonal_gap >= 0.0),
    warmth_gap_k    REAL NOT NULL DEFAULT 0.0 CHECK (warmth_gap_k >= 0.0),
    facing_score    REAL NOT NULL DEFAULT 0.0 CHECK (facing_score BETWEEN 0.0 AND 1.0),
    -- Whether anything could be measured about which way the subjects are facing. A spread whose
    -- facing could not be measured is not a spread whose subjects face outward; it is a spread
    -- nobody could check. Phase 24's rule, and on this build it is 0 almost everywhere.
    facing_known    INTEGER NOT NULL DEFAULT 0 CHECK (facing_known IN (0, 1)),
    similarity      REAL NOT NULL DEFAULT 0.0 CHECK (similarity BETWEEN -1.0 AND 1.0),
    pair_score      REAL NOT NULL DEFAULT 0.0 CHECK (pair_score BETWEEN 0.0 AND 1.0),

    -- An empty spread is not a spread. The composer leaves a *page* blank rather than an opening:
    -- a chapter one image short ends on a single, which is a design a photographer recognises.
    CHECK (left_image IS NOT NULL OR right_image IS NOT NULL),
    -- A single carries exactly one image.
    CHECK (single = 0 OR (left_image IS NULL) <> (right_image IS NULL)),
    -- The two hard pairing constraints, in the schema as well as in the solver. A near-duplicate
    -- pairing is refused by `spread::pair` and refused again here, because "no facing
    -- near-duplicates" is section 10.1's own property test and a promise enforced in one layer
    -- lasts until somebody writes a second caller. Phase 21's rule.
    CHECK (single = 1 OR similarity <= 0.92),
    CHECK (single = 1 OR tonal_gap <= 0.34)
) STRICT;

CREATE UNIQUE INDEX idx_album_spread_order ON album_spread(project_id, ix);

CREATE TABLE album_chapter (
    project_id      TEXT NOT NULL REFERENCES curate_album(project_id) ON DELETE CASCADE,
    chapter         TEXT NOT NULL,

    first_ix        INTEGER NOT NULL CHECK (first_ix >= 0),
    len             INTEGER NOT NULL CHECK (len >= 0),
    -- What the allocator wanted to give this chapter. Larger than `len` when the chapter ran out of
    -- frames, which is what `chapter_under_allocated` reports and what the panel shows.
    target          INTEGER NOT NULL CHECK (target >= 0),

    PRIMARY KEY (project_id, chapter)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Social, teaser and captions
-- ---------------------------------------------------------------------------

CREATE TABLE social_pick (
    project_id      TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
    -- 'grid', 'story', 'hero' or 'teaser'. One table for four sets, because a pick is the same
    -- shape in all four and four tables would be four places to forget the aspect is one phase 23
    -- called safe.
    set_kind        TEXT NOT NULL CHECK (set_kind IN ('grid', 'story', 'hero', 'teaser')),
    image_id        TEXT NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

    rank            INTEGER NOT NULL CHECK (rank >= 0),
    slot            TEXT NOT NULL
                    CHECK (slot IN ('hero', 'portrait', 'detail', 'candid', 'group', 'exit')),
    -- Only ever a variant `GeometryService` says is safe. `original` is the honest fallback when
    -- phase 23 found no safe crop at the shape the set wanted.
    aspect          TEXT NOT NULL DEFAULT 'original'
                    CHECK (aspect IN ('original', '4:5', '5:4', '1:1', '16:9')),
    legibility      REAL NOT NULL DEFAULT 0.0 CHECK (legibility BETWEEN 0.0 AND 1.0),

    PRIMARY KEY (project_id, set_kind, image_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX idx_social_pick_order ON social_pick(project_id, set_kind, rank);

CREATE TABLE curate_caption (
    project_id      TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
    -- NULL for a caption about a whole chapter, which is what the cloud task returns and what the
    -- album's section headings use. The empty string rather than NULL in the key, because SQLite
    -- treats NULLs in a primary key as distinct and two chapter captions would both be stored.
    image_id        TEXT NOT NULL DEFAULT '',
    chapter         TEXT NOT NULL,

    text            TEXT NOT NULL CHECK (length(text) BETWEEN 1 AND 90),
    source          TEXT NOT NULL DEFAULT 'template' CHECK (source IN ('template', 'cloud')),

    -- Always 1 on a stored row. An ungrounded caption is replaced by the template rather than
    -- stored with a flag, so this is a `CHECK` that a future caption path cannot quietly skip
    -- rather than a column with two states. Section 10.1's automated grounding check reads it.
    grounded        INTEGER NOT NULL DEFAULT 1 CHECK (grounded = 1),

    PRIMARY KEY (project_id, image_id, chapter)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Reasons
-- ---------------------------------------------------------------------------

CREATE TABLE curate_reason (
    project_id      TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
    -- What this reason is about: 'bw', 'hero', 'spread', 'social', 'teaser' or 'album'.
    subject_kind    TEXT NOT NULL
                    CHECK (subject_kind IN ('bw', 'hero', 'spread', 'social', 'teaser', 'album')),
    -- An image id, a spread id, or the empty string for an album-level note.
    subject_id      TEXT NOT NULL,
    ix              INTEGER NOT NULL CHECK (ix >= 0),

    -- The reason's *code*, never its sentence. Phase 09's decision at its conclusion: a stored
    -- sentence is copy a release has to maintain, and a catalog full of English cannot be
    -- translated. The panel renders `CurateCode::user_text`, and `AURA-ML-5142` is what a code from
    -- a newer build produces.
    code            TEXT NOT NULL,
    -- How much it moved the decision. Exactly -1.0 for a veto, because a veto did not move a score,
    -- it replaced one.
    weight          REAL NOT NULL CHECK (weight BETWEEN -1.0 AND 1.0),
    -- The specific half of a sentence, when there is one - "the only frame of the ring exchange"
    -- rather than "a coverage rule applies". Empty means the code's own sentence is the whole of it.
    detail          TEXT NOT NULL DEFAULT '',

    PRIMARY KEY (project_id, subject_kind, subject_id, ix)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- What the photographer decided
-- ---------------------------------------------------------------------------

CREATE TABLE curate_override (
    project_id      TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
    kind            TEXT NOT NULL
                    CHECK (kind IN ('hero', 'bw', 'social_grid', 'social_story', 'social_hero',
                                    'teaser')),
    image_id        TEXT NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,

    accepted        INTEGER NOT NULL CHECK (accepted IN (0, 1)),
    note            TEXT NOT NULL DEFAULT '' CHECK (length(note) <= 500),
    decided_at      TEXT NOT NULL,

    PRIMARY KEY (project_id, kind, image_id)
) STRICT, WITHOUT ROWID;

-- The photographer's own album order, kept separately from the spreads so that a re-run can
-- rebuild the spreads and still honour the order.
--
-- **This is the row that makes "reordering is remembered" true across a re-curation.** A pass that
-- stored the order only in `album_spread.ix` would lose it the moment the pass rebuilt the spreads,
-- which is exactly what a photographer would do after adding twenty frames to the gallery.
CREATE TABLE album_order (
    project_id      TEXT NOT NULL REFERENCES project(project_id) ON DELETE CASCADE,
    ix              INTEGER NOT NULL CHECK (ix >= 0),
    image_id        TEXT NOT NULL REFERENCES photo(photo_id) ON DELETE CASCADE,
    -- Always `'user'`. A column with one legal value rather than no column at all, because it is
    -- what `curate_album_order_is_the_photographers` reads: a future writer that reached this table
    -- would have to claim, in the row itself, that a person did it.
    source          TEXT NOT NULL DEFAULT 'user' CHECK (source = 'user'),
    set_at          TEXT NOT NULL,

    PRIMARY KEY (project_id, ix)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------------------
-- Views
-- ---------------------------------------------------------------------------

-- What the Curate panel's header draws, in one read.
CREATE VIEW v_curate_status AS
SELECT
  r.project_id,
  r.photos,
  r.selected,
  r.curated,
  CASE WHEN r.selected = 0 THEN 0.0
       ELSE CAST(r.curated AS REAL) / r.selected END AS coverage,
  r.bw_offered,
  (SELECT COUNT(*) FROM curate_override o
    WHERE o.project_id = r.project_id AND o.kind = 'bw' AND o.accepted = 1) AS bw_accepted,
  (SELECT COUNT(*) FROM curate_override o
    WHERE o.project_id = r.project_id AND o.kind = 'bw' AND o.accepted = 0) AS bw_rejected,
  r.heroes,
  r.chapters_covered,
  r.spreads,
  r.album_size,
  r.rhythm_score,
  r.rhythm_measurable,
  r.pairing_score,
  r.cloud_used,
  r.policy_ver,
  r.analysis_ver,
  r.embed_ver,
  r.heads_trained
FROM curate_run r;

-- The albums whose rhythm was measured over too little of themselves to be worth reporting.
--
-- A view rather than a query in Rust, because it is the answer to "which of my weddings did AURA
-- actually manage to sequence" and because on this build it is **every one of them**. A number that
-- is always the same is a number nobody looks at twice; a list a photographer can open is harder to
-- ignore. Phase 28's rule about printing what was not proved, applied to the catalog.
CREATE VIEW v_curate_unmeasured AS
SELECT
  a.project_id,
  a.size,
  a.rhythm_measurable,
  a.rhythm_score,
  (SELECT COUNT(*) FROM album_spread s
    WHERE s.project_id = a.project_id AND s.facing_known = 0) AS facing_unknown
FROM curate_album a
WHERE a.rhythm_measurable < 0.33;

-- ---------------------------------------------------------------------------
-- Triggers
-- ---------------------------------------------------------------------------

-- Invariant 2, twice. A hero and a monochrome candidate are the two picks a photographer is asked to
-- agree with, and a pick with no reason is a decision without an explanation.
--
-- `AFTER INSERT` rather than a `CHECK`, because the reasons are written after the pick they belong
-- to and a constraint on the pick's own row could not see them. The pass writes both inside one
-- transaction, so a failure here rolls the whole curation back rather than leaving an unexplained
-- hero in the catalog.
CREATE TRIGGER curate_hero_reason_count
AFTER INSERT ON curate_hero
FOR EACH ROW WHEN (SELECT COUNT(*) FROM curate_reason
                    WHERE project_id = NEW.project_id
                      AND subject_kind = 'hero'
                      AND subject_id = NEW.image_id) > 4
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5144: a portfolio pick carries at most four reasons');
END;

CREATE TRIGGER curate_reason_bounded
BEFORE INSERT ON curate_reason
FOR EACH ROW WHEN NEW.ix >= 4 AND NEW.subject_kind <> 'album'
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5144: a curation pick carries at most four reasons');
END;

-- A photographer's album order is not overwritten by a pass, and it takes three statements to
-- guarantee it.
--
-- **One.** The curation pass has no statement anywhere in it that names `album_order`.
-- `crates/aura-curate/tests/no_outputs.rs` greps for one, which is the strongest of the three
-- because it is checked at build time rather than at write time.
--
-- **Two.** `album_order.source` is `'user'` and nothing else may be written, so a future writer
-- that reached this table would have to say, in the row itself, that a person did it.
--
-- **Three.** This trigger. It refuses a delete while `curate_album.user_ordered` is 1, which is the
-- protocol phase 18 established for a hand-edited mask and phase 08 for a locked moment: the guard
-- goes *inside* the statement that would destroy the work. `CurateService::set_order` legitimately
-- replaces an order, and does it by clearing the flag, replacing the rows and setting the flag
-- again, all inside one transaction - so the one caller entitled to do this has to say so first.
--
-- It is a weaker guarantee than a trigger that could tell a person from a pass, and there is no
-- such trigger: SQLite cannot see who is calling. The exit report says so.
CREATE TRIGGER curate_album_no_reorder
BEFORE DELETE ON album_order
FOR EACH ROW WHEN (SELECT user_ordered FROM curate_album
                    WHERE project_id = OLD.project_id) = 1
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5143: an album order the photographer set is not overwritten by automation');
END;

-- Nothing but a person writes an album order.
CREATE TRIGGER curate_album_order_is_the_photographers
BEFORE INSERT ON album_order
FOR EACH ROW WHEN NEW.source <> 'user'
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5143: an album order is only ever something a photographer set');
END;

-- A caption a model drafted is stored only when the grounding check passed.
--
-- The `grounded = 1` CHECK on the column is the first half; this is the half that catches a future
-- caption path inserting a cloud draft without running the check at all, by refusing a cloud row
-- whose text is not inside the contract's bounds. It is a weaker guarantee than the vocabulary
-- check itself - which cannot live in SQL - and the exit report says so.
CREATE TRIGGER curate_caption_bounded
BEFORE INSERT ON curate_caption
FOR EACH ROW WHEN length(trim(NEW.text)) = 0
              OR (length(NEW.text) - length(replace(NEW.text, ' ', ''))) >= 12
BEGIN
  SELECT RAISE(ABORT, 'AURA-ML-5144: a caption is one to twelve words and at most ninety characters');
END;
