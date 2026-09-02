#!/usr/bin/env python3
"""Phase 29's three headline gates, from the catalog side.

`tests/eval/curate_eval.rs` measures the selectors on a synthetic wedding whose portfolio was
planted, so it always knows the right answer. This measures a **real project** - a catalog somebody
has actually curated and actually reviewed - and reports the three numbers section 10.1 asks for and
that no fixture can supply:

    hero agreement:      the share of AURA's top twenty a photographer kept
    album reordering:    the share of the album a photographer moved
    monochrome accepted: the share of offered conversions a photographer took

It reads the catalog read-only and writes nothing.

Why this exists beside the Rust harness
---------------------------------------

The Rust gates prove the arithmetic against an answer they already know: `fixtures::planted` names
twenty frames as the ones a photographer would pick, and the selector is measured against that. It
is a test of the selector against a file in this repository. **A fixture cannot disagree**, and the
whole of section 10.1's first three rows is about disagreement.

Three denominators, and each of them is chosen so that a project nobody has reviewed reports
nothing rather than reporting unanimity:

  - hero agreement counts heroes a photographer *decided about*, so an untouched portfolio has no
    agreement rate rather than a perfect one;
  - reordering counts against the album only when `album_order` holds a row, because an album
    nobody dragged is not an album somebody approved;
  - monochrome acceptance counts offered conversions a photographer decided about, for the same
    reason - and `bw_offered` is printed beside it, because an acceptance rate of 1.000 over three
    offers is a different fact from one over ninety.

    python ml/models/curate/eval_curate.py --self-test
    python ml/models/curate/eval_curate.py path/to/catalog.sqlite
    python ml/models/curate/eval_curate.py path/to/catalog.sqlite --project prj_<uuid>
"""

from __future__ import annotations

import argparse
import sqlite3
import sys
from dataclasses import dataclass

# Section 10.1's own numbers.
HERO_AGREEMENT_FLOOR = 0.75
REORDER_CEILING = 0.15
BW_ACCEPTANCE_FLOOR = 0.70

# Below this many decisions a rate is printed and not judged. A gate that fails a studio on two
# reviewed heroes is a gate a studio turns off.
MIN_DECISIONS = 8


@dataclass
class Report:
    project: str
    heroes: int
    heroes_decided: int
    heroes_kept: int
    album_size: int
    album_moved: int
    album_ordered: bool
    bw_offered: int
    bw_decided: int
    bw_accepted: int
    heads_trained: int

    def hero_agreement(self) -> float | None:
        if self.heroes_decided < MIN_DECISIONS:
            return None
        return self.heroes_kept / self.heroes_decided

    def reordering(self) -> float | None:
        if not self.album_ordered or self.album_size == 0:
            return None
        return self.album_moved / self.album_size

    def bw_acceptance(self) -> float | None:
        if self.bw_decided < MIN_DECISIONS:
            return None
        return self.bw_accepted / self.bw_decided


def longest_increasing(values: list[int]) -> int:
    """Patience sorting. Used to turn a photographer's order into a count of *moves*.

    `n` minus this is the fewest images somebody had to drag, which is what "% of images reordered"
    means. Counting positions that changed instead would report a single drag near the front of the
    album as having reordered the whole of it.
    """
    tails: list[int] = []
    for value in values:
        lo, hi = 0, len(tails)
        while lo < hi:
            mid = (lo + hi) // 2
            if tails[mid] < value:
                lo = mid + 1
            else:
                hi = mid
        if lo == len(tails):
            tails.append(value)
        else:
            tails[lo] = value
    return len(tails)


def measure(db: sqlite3.Connection, project: str) -> Report:
    run = db.execute(
        "SELECT heroes, album_size, bw_offered, heads_trained FROM curate_run WHERE project_id = ?",
        (project,),
    ).fetchone()
    heroes, album_size, bw_offered, heads_trained = run if run else (0, 0, 0, 0)

    decided = db.execute(
        """SELECT COUNT(*), COALESCE(SUM(accepted), 0)
             FROM curate_override
            WHERE project_id = ? AND kind = 'hero'""",
        (project,),
    ).fetchone()
    bw = db.execute(
        """SELECT COUNT(*), COALESCE(SUM(accepted), 0)
             FROM curate_override
            WHERE project_id = ? AND kind = 'bw'""",
        (project,),
    ).fetchone()

    # The photographer's order against the order the composer proposed. `album_spread.ix` holds
    # AURA's sequence and `album_order.ix` holds theirs; the moves are the difference.
    proposed = [
        row[0]
        for row in db.execute(
            """SELECT image_id FROM (
                   SELECT ix, left_image  AS image_id FROM album_spread WHERE project_id = ?
                     UNION ALL
                   SELECT ix, right_image AS image_id FROM album_spread WHERE project_id = ?
               ) WHERE image_id IS NOT NULL ORDER BY ix""",
            (project, project),
        )
    ]
    theirs = [
        row[0]
        for row in db.execute(
            "SELECT image_id FROM album_order WHERE project_id = ? ORDER BY ix", (project,)
        )
    ]

    moved = 0
    if theirs:
        seat = {image: ix for ix, image in enumerate(proposed)}
        ranks = [seat[image] for image in theirs if image in seat]
        moved = len(ranks) - longest_increasing(ranks)

    return Report(
        project=project,
        heroes=heroes,
        heroes_decided=decided[0] if decided else 0,
        heroes_kept=decided[1] if decided else 0,
        album_size=album_size,
        album_moved=moved,
        album_ordered=bool(theirs),
        bw_offered=bw_offered,
        bw_decided=bw[0] if bw else 0,
        bw_accepted=bw[1] if bw else 0,
        heads_trained=heads_trained,
    )


def render(report: Report) -> bool:
    print(f"project {report.project}")
    print(f"  heads trained: {'yes' if report.heads_trained else 'no'}")
    ok = True

    agreement = report.hero_agreement()
    if agreement is None:
        print(
            f"  hero agreement: not measured - {report.heroes_decided} of {report.heroes} heroes "
            f"reviewed, fewer than the {MIN_DECISIONS} a rate is worth quoting over"
        )
    else:
        verdict = "PASS" if agreement >= HERO_AGREEMENT_FLOOR else "FAIL"
        print(
            f"  hero agreement: {agreement:.3f} over {report.heroes_decided} reviewed "
            f"(floor {HERO_AGREEMENT_FLOOR}) {verdict}"
        )
        ok &= agreement >= HERO_AGREEMENT_FLOOR

    reordering = report.reordering()
    if reordering is None:
        print("  album reordering: not measured - nobody has dragged this album")
    else:
        verdict = "PASS" if reordering <= REORDER_CEILING else "FAIL"
        print(
            f"  album reordering: {reordering:.3f} - {report.album_moved} of {report.album_size} "
            f"images moved (ceiling {REORDER_CEILING}) {verdict}"
        )
        ok &= reordering <= REORDER_CEILING

    acceptance = report.bw_acceptance()
    if acceptance is None:
        print(
            f"  monochrome accepted: not measured - {report.bw_decided} of {report.bw_offered} "
            f"offers reviewed"
        )
    else:
        verdict = "PASS" if acceptance >= BW_ACCEPTANCE_FLOOR else "FAIL"
        print(
            f"  monochrome accepted: {acceptance:.3f} over {report.bw_decided} reviewed of "
            f"{report.bw_offered} offered (floor {BW_ACCEPTANCE_FLOOR}) {verdict}"
        )
        ok &= acceptance >= BW_ACCEPTANCE_FLOOR

    return ok


def self_test() -> int:
    """Prove the three statistics on a catalog this function builds, including the ones that abstain."""
    db = sqlite3.connect(":memory:")
    db.executescript(
        """
        CREATE TABLE curate_run (project_id TEXT PRIMARY KEY, heroes INT, album_size INT,
                                 bw_offered INT, heads_trained INT);
        CREATE TABLE curate_override (project_id TEXT, kind TEXT, image_id TEXT, accepted INT);
        CREATE TABLE album_spread (project_id TEXT, ix INT, left_image TEXT, right_image TEXT);
        CREATE TABLE album_order (project_id TEXT, ix INT, image_id TEXT);
        """
    )
    db.execute("INSERT INTO curate_run VALUES ('prj_a', 20, 8, 30, 0)")
    for ix in range(20):
        db.execute(
            "INSERT INTO curate_override VALUES ('prj_a', 'hero', ?, ?)",
            (f"img_{ix}", 1 if ix < 17 else 0),
        )
    for ix in range(10):
        db.execute(
            "INSERT INTO curate_override VALUES ('prj_a', 'bw', ?, ?)",
            (f"bw_{ix}", 1 if ix < 8 else 0),
        )
    # An eight-image album whose owner moved exactly one frame to the front.
    for ix in range(4):
        db.execute(
            "INSERT INTO album_spread VALUES ('prj_a', ?, ?, ?)",
            (ix, f"a{2 * ix}", f"a{2 * ix + 1}"),
        )
    order = ["a7"] + [f"a{i}" for i in range(7)]
    for ix, image in enumerate(order):
        db.execute("INSERT INTO album_order VALUES ('prj_a', ?, ?)", (ix, image))

    # A second project nobody has reviewed at all.
    db.execute("INSERT INTO curate_run VALUES ('prj_b', 20, 80, 40, 0)")

    ok = True
    reviewed = measure(db, "prj_a")
    if abs((reviewed.hero_agreement() or 0.0) - 0.85) > 1e-6:
        print(f"FAIL: hero agreement read {reviewed.hero_agreement()}")
        ok = False
    if reviewed.album_moved != 1:
        print(f"FAIL: one dragged frame counted as {reviewed.album_moved} moves")
        ok = False
    if abs((reviewed.bw_acceptance() or 0.0) - 0.8) > 1e-6:
        print(f"FAIL: monochrome acceptance read {reviewed.bw_acceptance()}")
        ok = False
    render(reviewed)

    untouched = measure(db, "prj_b")
    if untouched.hero_agreement() is not None:
        print("FAIL: a portfolio nobody reviewed reported an agreement rate")
        ok = False
    if untouched.reordering() is not None:
        print("FAIL: an album nobody dragged reported a reordering rate")
        ok = False
    if untouched.bw_acceptance() is not None:
        print("FAIL: offers nobody reviewed reported an acceptance rate")
        ok = False
    render(untouched)

    print("self-test: PASS" if ok else "self-test: FAIL")
    return 0 if ok else 1


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("catalog", nargs="?", help="path to a catalog")
    parser.add_argument("--project", help="one project id; default every curated project")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.catalog:
        parser.print_help()
        return 2

    db = sqlite3.connect(f"file:{args.catalog}?mode=ro", uri=True)
    projects = (
        [args.project]
        if args.project
        else [row[0] for row in db.execute("SELECT project_id FROM curate_run ORDER BY project_id")]
    )
    if not projects:
        print("no curated projects in this catalog", file=sys.stderr)
        return 1

    ok = True
    for project in projects:
        ok &= render(measure(db, project))
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
