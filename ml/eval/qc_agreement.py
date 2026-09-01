#!/usr/bin/env python3
"""Phase 27's gates, from the catalog side.

`tests/eval/qc_eval.rs` measures the algorithms on a synthetic gallery whose defects were
authored, so it always knows the right answer. This measures a **real project** - a catalog
somebody has actually run the QC pass over - and reports the four things section 10.1 asks about:

    how much of the gallery was actually inspected, and how much of it could not be
    the false-ticket rate: findings a photographer looked at and disagreed with
    what the re-edit loop achieved, and what it had to put back
    whether any stored row broke a bound the contract owns

It reads the catalog read-only and writes nothing.

Why this exists beside the Rust harness
---------------------------------------

The Rust gates prove the arithmetic against an answer they already know. This one asks the
question nobody in this repository can answer: on a wedding somebody actually shot and actually
reviewed, **did the photographer agree with the tickets**. That is the headline KPI of the phase
and it cannot be measured from a fixture, because a fixture cannot disagree.

The number it reports is the false-ticket rate, and its denominator is deliberately findings a
person *reviewed* rather than findings that exist. A queue nobody has opened has no disagreement
rate, and reporting one as zero would read as unanimous agreement with work nobody has looked at.

    python ml/eval/qc_agreement.py --self-test
    python ml/eval/qc_agreement.py path/to/catalog.sqlite
    python ml/eval/qc_agreement.py path/to/catalog.sqlite --project prj_<uuid>
"""

from __future__ import annotations

import argparse
import json
import sqlite3
import sys
from dataclasses import dataclass, field

# Section 10.1's headline gates.
#
# `FALSE_TICKET_CEILING` is the one that matters and the one this repository cannot meet from a
# fixture: no more than five per cent of the findings a photographer reviews may be findings they
# disagree with. A product that cries wolf on one frame in ten is a product whose queue gets
# closed without being read, and every real finding in it is then invisible.
FALSE_TICKET_CEILING = 0.05

# At least this share of the checks a pass wanted to run must actually have run before its report
# is worth reading as a statement about the gallery. Below it, an empty result means "AURA could
# not look" far more often than it means "AURA looked and it is fine".
COMPLETENESS_FLOOR = 0.70

# At least this share of the findings the loop attempted must have survived re-inspection. A loop
# that puts back most of what it tries is a loop whose remedies do not match its diagnoses, which
# is a worse failure than not attempting them - it spends a photographer's time and changes
# nothing.
KEPT_FLOOR = 0.50

# The bounds from the frozen contract. Restated here rather than read out of the build under test,
# because a report that read them from that build could not detect a build whose bounds had moved -
# which is exactly what a support case needs to find out.
BOUNDS = {
    "max_rounds": 2,
    "max_planner_calls": 40,
    "min_strength_factor": 0.25,
    "max_strength_factor": 0.90,
    "max_note_chars": 280,
}

# `QcCategory::ALL`, in contract order.
CATEGORIES = (
    "consistency",
    "skin",
    "exposure",
    "sharpness",
    "retouch",
    "mask",
    "crop",
    "cleanup",
    "duplicate",
    "coverage",
)

# The two statuses a person may set. Everything else is automation's record of what happened.
USER_SET = ("accepted", "dismissed")


@dataclass
class Report:
    """What one project's QC rows say."""

    project: str
    images: int = 0
    images_unreached: int = 0
    checks_run: int = 0
    checks_skipped: int = 0
    found: int = 0
    fixed: int = 0
    reverted: int = 0
    escalated: int = 0
    replaced: int = 0
    accepted: int = 0
    dismissed: int = 0
    planner_calls: int = 0
    rounds_attempted: int = 0
    rounds_kept: int = 0
    per_category: dict[str, dict[str, int]] = field(default_factory=dict)
    violations: list[str] = field(default_factory=list)

    @property
    def completeness(self) -> float | None:
        """The share of attempted inspections that ran, or `None` when none were attempted."""
        total = self.checks_run + self.checks_skipped
        return self.checks_run / total if total else None

    @property
    def reviewed(self) -> int:
        """Findings a person actually looked at and formed a view on."""
        return self.accepted + self.dismissed

    @property
    def false_ticket_rate(self) -> float | None:
        """The share of *reviewed* findings a photographer disagreed with.

        `None` when nobody has reviewed anything. That is the honest answer and it is not zero:
        an unopened queue has no agreement rate, and rendering one as a passing number is the
        single most misleading thing this script could do.
        """
        return self.dismissed / self.reviewed if self.reviewed else None

    @property
    def kept_share(self) -> float | None:
        """The share of attempted rounds whose change survived re-inspection."""
        if not self.rounds_attempted:
            return None
        return self.rounds_kept / self.rounds_attempted


def measure(conn: sqlite3.Connection, project: str) -> Report:
    """Read one project's QC rows."""
    report = Report(project=project)

    run = conn.execute(
        """SELECT images, images_unreached, checks_run, checks_skipped, found, fixed,
                  reverted, escalated, replaced, planner_calls, by_category
             FROM qc_run WHERE project_id = ?""",
        (project,),
    ).fetchone()
    if run is not None:
        (
            report.images,
            report.images_unreached,
            report.checks_run,
            report.checks_skipped,
            report.found,
            report.fixed,
            report.reverted,
            report.escalated,
            report.replaced,
            report.planner_calls,
            by_category,
        ) = run
        try:
            parsed = json.loads(by_category)
        except (TypeError, ValueError):
            parsed = []
            report.violations.append("by_category is not readable JSON")
        for row in parsed:
            name = row.get("category")
            if name in CATEGORIES:
                report.per_category[name] = {
                    "found": int(row.get("found", 0)),
                    "fixed": int(row.get("fixed", 0)),
                    "escalated": int(row.get("escalated", 0)),
                    "skipped": int(row.get("skipped", 0)),
                }

    for status, count in conn.execute(
        "SELECT status, COUNT(*) FROM qc_ticket WHERE project_id = ? GROUP BY status",
        (project,),
    ):
        if status == "accepted":
            report.accepted = count
        elif status == "dismissed":
            report.dismissed = count

    attempted, kept = conn.execute(
        """SELECT COUNT(*), COALESCE(SUM(kept), 0)
             FROM qc_round r JOIN qc_ticket t ON t.ticket_id = r.ticket_id
            WHERE t.project_id = ?""",
        (project,),
    ).fetchone()
    report.rounds_attempted = attempted or 0
    report.rounds_kept = kept or 0

    report.violations.extend(bound_violations(conn, project))
    return report


def bound_violations(conn: sqlite3.Connection, project: str) -> list[str]:
    """Every stored row that breaks a bound the contract owns.

    The CHECK constraints in migration 27 refuse these at write time, so a row here means either
    that the schema was widened or that the rows were written by something other than `QcStore`.
    Both are worth a support case, and neither is visible from inside the build that wrote them.
    """
    problems: list[str] = []

    (over_rounds,) = conn.execute(
        """SELECT COUNT(*) FROM (
               SELECT r.ticket_id FROM qc_round r
                 JOIN qc_ticket t ON t.ticket_id = r.ticket_id
                WHERE t.project_id = ?
                GROUP BY r.ticket_id HAVING COUNT(*) > ?)""",
        (project, BOUNDS["max_rounds"]),
    ).fetchone()
    if over_rounds:
        problems.append(
            f"{over_rounds} findings carry more than {BOUNDS['max_rounds']} rounds"
        )

    (over_factor,) = conn.execute(
        """SELECT COUNT(*) FROM qc_ticket
            WHERE project_id = ? AND remedy_factor IS NOT NULL
              AND (remedy_factor < ? OR remedy_factor > ?)""",
        (project, BOUNDS["min_strength_factor"], BOUNDS["max_strength_factor"]),
    ).fetchone()
    if over_factor:
        problems.append(f"{over_factor} remedies sit outside the strength bounds")

    (over_calls,) = conn.execute(
        "SELECT COUNT(*) FROM qc_run WHERE project_id = ? AND planner_calls > ?",
        (project, BOUNDS["max_planner_calls"]),
    ).fetchone()
    if over_calls:
        problems.append(f"the pass made more than {BOUNDS['max_planner_calls']} planner calls")

    # A replacement whose coverage was not re-validated is the one stored row that could have
    # taken a guarantee out of a gallery. It is refused in three places and checked here anyway.
    (unheld,) = conn.execute(
        "SELECT COUNT(*) FROM qc_replacement WHERE project_id = ? AND coverage_held = 0",
        (project,),
    ).fetchone()
    if unheld:
        problems.append(f"{unheld} swaps were stored without coverage being re-validated")

    # A round that names collateral damage without naming the check that took it.
    (unnamed,) = conn.execute(
        """SELECT COUNT(*) FROM qc_round r JOIN qc_ticket t ON t.ticket_id = r.ticket_id
            WHERE t.project_id = ? AND r.collateral > 0.0 AND r.collateral_category IS NULL""",
        (project,),
    ).fetchone()
    if unnamed:
        problems.append(f"{unnamed} rounds report collateral damage with no check named")

    return problems


def render(report: Report) -> int:
    """Print one project's findings. Returns 1 when a gate failed."""
    print(f"project {report.project}")
    print(f"  images inspected      {report.images} ({report.images_unreached} unreached)")

    completeness = report.completeness
    if completeness is None:
        print("  inspection coverage   NOT MEASURED - no pass has run")
    else:
        verdict = "ok" if completeness >= COMPLETENESS_FLOOR else "BELOW FLOOR"
        print(
            f"  inspection coverage   {completeness:.1%} of attempted checks ran "
            f"({report.checks_skipped} skipped) [{verdict}]"
        )

    print(f"  findings              {report.found}")
    print(f"  fixed / put back      {report.fixed} / {report.reverted}")
    print(f"  handed to a person    {report.escalated}")
    print(f"  frames swapped        {report.replaced}")

    rate = report.false_ticket_rate
    if rate is None:
        print(
            "  false-ticket rate     NOT MEASURED - nobody has reviewed a finding.\n"
            "                        This is not zero. An unopened queue has no agreement rate."
        )
    else:
        verdict = "ok" if rate <= FALSE_TICKET_CEILING else "ABOVE CEILING"
        print(
            f"  false-ticket rate     {rate:.1%} of {report.reviewed} reviewed "
            f"({report.dismissed} dismissed) [{verdict}]"
        )

    kept = report.kept_share
    if kept is None:
        print("  remedies kept         NOT MEASURED - the loop attempted nothing")
    else:
        verdict = "ok" if kept >= KEPT_FLOOR else "BELOW FLOOR"
        print(
            f"  remedies kept         {kept:.1%} of {report.rounds_attempted} attempts [{verdict}]"
        )

    if report.per_category:
        print("  by inspection:")
        for name in CATEGORIES:
            row = report.per_category.get(name)
            if not row:
                continue
            # A category that found nothing and skipped everything is reported as unchecked, not
            # as clean. Same rule as the panel, for the same reason.
            state = "not checked" if row["found"] == 0 and row["skipped"] > 0 else "checked"
            print(
                f"    {name:<13} found {row['found']:>4}  fixed {row['fixed']:>4}  "
                f"for a person {row['escalated']:>4}  skipped {row['skipped']:>5}  ({state})"
            )

    failed = 0
    if completeness is not None and completeness < COMPLETENESS_FLOOR:
        failed = 1
    if rate is not None and rate > FALSE_TICKET_CEILING:
        failed = 1
    if kept is not None and kept < KEPT_FLOOR:
        failed = 1
    for problem in report.violations:
        print(f"  VIOLATION: {problem}")
        failed = 1
    return failed


def self_test() -> int:
    """Run against a catalog whose answer is chosen, so a broken reader is caught."""
    conn = sqlite3.connect(":memory:")
    conn.executescript(
        """
        CREATE TABLE project (project_id TEXT PRIMARY KEY);
        CREATE TABLE photo (photo_id TEXT PRIMARY KEY, project_id TEXT);
        CREATE TABLE qc_run (
          project_id TEXT PRIMARY KEY, images INTEGER, images_unreached INTEGER,
          checks_run INTEGER, checks_skipped INTEGER, found INTEGER, fixed INTEGER,
          reverted INTEGER, escalated INTEGER, replaced INTEGER, planner_calls INTEGER,
          by_category TEXT);
        CREATE TABLE qc_ticket (
          ticket_id TEXT PRIMARY KEY, project_id TEXT, status TEXT, remedy_factor REAL);
        CREATE TABLE qc_round (
          ticket_id TEXT, round INTEGER, kept INTEGER, collateral REAL,
          collateral_category TEXT);
        CREATE TABLE qc_replacement (
          ticket_id TEXT PRIMARY KEY, project_id TEXT, coverage_held INTEGER);
        INSERT INTO project VALUES ('prj_test');
        INSERT INTO qc_run VALUES ('prj_test', 100, 0, 900, 100, 20, 12, 3, 5, 1, 4,
          '[{"category":"skin","found":0,"fixed":0,"escalated":0,"skipped":100}]');
        """
    )
    for index in range(20):
        status = "accepted" if index < 19 else "dismissed"
        conn.execute(
            "INSERT INTO qc_ticket VALUES (?, 'prj_test', ?, NULL)",
            (f"tkt_{index}", status),
        )
    for index in range(12):
        conn.execute(
            "INSERT INTO qc_round VALUES (?, 1, 1, 0.0, NULL)", (f"tkt_{index}",)
        )
    for index in range(12, 15):
        conn.execute(
            "INSERT INTO qc_round VALUES (?, 1, 0, 0.0, NULL)", (f"tkt_{index}",)
        )
    conn.commit()

    print("self-test: a healthy project")
    report = measure(conn, "prj_test")
    if render(report) != 0:
        print("self-test FAILED: a healthy project did not pass")
        return 1

    print("\nself-test: an unopened queue has no agreement rate")
    conn.execute("UPDATE qc_ticket SET status = 'open'")
    conn.commit()
    if measure(conn, "prj_test").false_ticket_rate is not None:
        print("self-test FAILED: an unreviewed queue reported a rate")
        return 1
    print("  reports NOT MEASURED rather than 0.0%")

    print("\nself-test: a photographer who disagrees with a fifth of the queue")
    for index in range(4):
        conn.execute(
            "UPDATE qc_ticket SET status = 'dismissed' WHERE ticket_id = ?",
            (f"tkt_{index}",),
        )
    for index in range(4, 20):
        conn.execute(
            "UPDATE qc_ticket SET status = 'accepted' WHERE ticket_id = ?",
            (f"tkt_{index}",),
        )
    conn.commit()
    if render(measure(conn, "prj_test")) == 0:
        print("self-test FAILED: a 20 % false-ticket rate passed")
        return 1
    print("  caught: too many findings a photographer disagreed with")

    print("\nself-test: a third round on one finding")
    conn.execute("UPDATE qc_ticket SET status = 'accepted'")
    conn.execute("INSERT INTO qc_round VALUES ('tkt_0', 2, 1, 0.0, NULL)")
    conn.execute("INSERT INTO qc_round VALUES ('tkt_0', 3, 1, 0.0, NULL)")
    conn.commit()
    if not any("rounds" in problem for problem in measure(conn, "prj_test").violations):
        print("self-test FAILED: a third round was not reported")
        return 1
    print("  caught: a finding remediated more times than the bound allows")

    print("\nself-test: a swap stored without coverage being re-validated")
    conn.execute("INSERT INTO qc_replacement VALUES ('tkt_1', 'prj_test', 0)")
    conn.commit()
    if not any("coverage" in problem for problem in measure(conn, "prj_test").violations):
        print("self-test FAILED: an unvalidated swap was not reported")
        return 1
    print("  caught: a guarantee that was never re-checked")

    print("\nself-test passed.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("catalog", nargs="?", help="path to a catalog, opened read-only")
    parser.add_argument("--project", help="one project id; every project when omitted")
    parser.add_argument("--self-test", action="store_true", help="run against a chosen answer")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if not args.catalog:
        parser.error("a catalog path or --self-test is required")

    conn = sqlite3.connect(f"file:{args.catalog}?mode=ro", uri=True)
    projects = (
        [args.project]
        if args.project
        else [row[0] for row in conn.execute("SELECT project_id FROM project")]
    )
    worst = 0
    for project in projects:
        worst = max(worst, render(measure(conn, project)))
        print()
    print(
        "The false-ticket rate above is the only number in this phase that measures agreement\n"
        "with a person, and it is only meaningful once somebody has actually worked the queue.\n"
        "Section 10.1's blind study over five weddings is what turns it into a claim about the\n"
        "product rather than about one photographer, and it has not been run. Exit report\n"
        "condition C2."
    )
    return worst


if __name__ == "__main__":
    sys.exit(main())
