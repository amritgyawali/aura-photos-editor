# AURA-ML-5145 - The curation policy table was refused

**Severity / recovery:** see `crates/aura-core/errors.toml` for the registered values.

## What the photographer sees

Nothing has been curated at all, and the panel says so. No album, no portfolio, no sets.

## What actually happened

`crates/aura-curate/config/curation.toml` could not be read, could not be parsed, or asked for
something the contract does not permit.

The pass **halts** rather than falling back on defaults. The same choice phases 24, 25, 26, 27 and
28 made for their own policy tables, and the reason is sharper here than in most: curation is a
taste decision, the config is where a studio's taste lives, and a curated album produced from
numbers nobody chose would be a proposal a photographer trusts for the wrong reason.

## What the loader refuses

The file may **tighten** a bound and may never widen one:

* `album_min` below `ALBUM_MIN`, or `album_max` above `ALBUM_MAX`;
* `teaser_min` below `TEASER_MIN`, or `teaser_max` above `TEASER_MAX`;
* `heroes_per_chapter` above `MAX_HEROES_PER_CHAPTER`;
* `hero_technical_floor` below `HERO_TECHNICAL_FLOOR` - a studio may demand sharper portfolio work,
  never softer;
* `max_pair_tonal_gap` above `MAX_PAIR_TONAL_GAP`, or `max_pair_similarity` above
  `MAX_PAIR_SIMILARITY` - the second is the near-duplicate constraint and there is no configuration
  in which two versions of the same photograph may face each other;
* `bw_candidate_floor` below `BW_CANDIDATE_FLOOR`;
* any hero weight that is negative, or a weight row whose five weights sum to zero;
* a rhythm pattern containing a token that is not `wide`, `medium` or `tight`;
* **any key whose name contains a skin target.** The loader scans for one, as phases 15, 25 and 27
  scan their own schemas, because the band a monochrome mix protects is measured per person from
  `ToneService::skin_loci` and a file that could name one would be the constant
  `docs/skin-fairness.md` says this product does not have.

## What to do

1. Compare against the shipped file in the repository. `git diff` on
   `crates/aura-curate/config/curation.toml` is usually the whole diagnosis.
2. Restore it, or reinstall.
3. Re-run curation. Nothing else needs repeating: the pass reads no other state it could have
   half-written.

## Related

* `docs/adr/ADR-0059-curation-selection-and-album-composition.md` sections 5 and 6
* `docs/curation.md`
