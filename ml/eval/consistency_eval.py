#!/usr/bin/env python3
"""Phase 25's gates, from the catalog side.

`tests/eval/consistency_eval.rs` measures the algorithms on synthetic galleries whose drift was
authored. This measures a **real project** - a catalog somebody has actually run the consistency
pass over - and reports the four numbers section 10.1 asks about:

    within-node warmth spread, before and after
    within-node exposure spread, before and after
    per-identity skin dE00 spread across the gallery
    how many frames are still out of line, and by how much

It reads the catalog read-only and writes nothing.

Why this exists beside the Rust harness
---------------------------------------

The Rust gates prove the arithmetic against an answer they already know. This one asks a question
nobody knows the answer to: what did the pass actually do to a wedding. The two are different jobs
and only the second can be run on a photographer's machine when something looks wrong.

`--self-test` runs the whole computation against an in-memory catalog whose numbers are chosen, so
the script is exercised on every CI run even though there are no weddings in this repository.

    python ml/eval/consistency_eval.py --self-test
    python ml/eval/consistency_eval.py path/to/catalog.sqlite
    python ml/eval/consistency_eval.py path/to/catalog.sqlite --project prj_<uuid>
"""

from __future__ import annotations

import argparse
import sqlite3
import statistics
import sys
from dataclasses import dataclass, field

# Section 10.1's two headline gates.
CCT_SPREAD_REDUCTION = 0.60
EV_SPREAD_REDUCTION = 0.50

# Section 6.3's promise.
SKIN_DE00_SPREAD_CEILING = 2.0

# The five bounds, from the frozen contract. Restated here rather than parsed out of the Rust,
# because a report that read them from the build under test could not detect a build whose bounds
# had moved - which is exactly the thing a support case needs to find out.
BOUNDS = {
    "cct": 450.0,
    "tint": 12.0,
    "exposure": 0.35,
    "contrast": 8.0,
    "saturation": 6.0,
}


@dataclass
class NodeReport:
    """One lighting group's before and after."""

    label: str
    frames: int
    anchored: bool
    before_cct: float
    after_cct: float
    before_ev: float
    after_ev: float
    bounded: int


@dataclass
class ProjectReport:
    """A whole wedding."""

    project: str
    photos: int
    normalised: int
    nodes: list[NodeReport] = field(default_factory=list)
    outliers: list[tuple[str, float, str]] = field(default_factory=list)
    skin: list[tuple[str, int, float, float]] = field(default_factory=list)
    mood_preserved: int = 0
    user_edited: int = 0

    @property
    def anchored_nodes(self) -> int:
        return sum(1 for node in self.nodes if node.anchored)

    def spread(self, which: str) -> tuple[float, float]:
        """The mean before and after spread over the nodes that could be measured.

        Averaged over *nodes* rather than over frames, because the claim is about how consistent
        each part of a wedding is and a four-hundred-frame reception would otherwise decide the
        number on its own.
        """
        pairs = [
            (getattr(node, f"before_{which}"), getattr(node, f"after_{which}"))
            for node in self.nodes
            if node.frames >= 2 and getattr(node, f"before_{which}") > 0.0
        ]
        if not pairs:
            return (0.0, 0.0)
        return (
            statistics.fmean(before for before, _ in pairs),
            statistics.fmean(after for _, after in pairs),
        )


def mad(values: list[float]) -> float:
    """Mean absolute deviation.

    The same estimator the Rust uses, and for the same reason: what a person sees is how far apart
    two adjacent frames look, and a standard deviation squares the contribution of the one that
    drifted furthest.
    """
    if len(values) < 2:
        return 0.0
    mean = statistics.fmean(values)
    return statistics.fmean(abs(value - mean) for value in values)


def measure(conn: sqlite3.Connection, project: str) -> ProjectReport:
    """Read one project's gallery rows and compute the four numbers."""
    photos = conn.execute(
        "SELECT COUNT(*) FROM photo WHERE project_id = ?", (project,)
    ).fetchone()[0]
    normalised = conn.execute(
        "SELECT COUNT(*) FROM gallery_delta WHERE project_id = ?", (project,)
    ).fetchone()[0]

    report = ProjectReport(project=project, photos=photos, normalised=normalised)

    nodes = conn.execute(
        "SELECT node_id, label, cct_k IS NOT NULL FROM gallery_node "
        "WHERE project_id = ? ORDER BY first_ts",
        (project,),
    ).fetchall()

    for node_id, label, anchored in nodes:
        rows = conn.execute(
            "SELECT from_cct_k, d_cct, from_exposure_ev, d_exposure, bounded_by "
            "FROM gallery_delta WHERE node_id = ?",
            (node_id,),
        ).fetchall()
        before_cct = [row[0] for row in rows]
        after_cct = [row[0] + row[1] for row in rows]
        before_ev = [row[2] for row in rows]
        after_ev = [row[2] + row[3] for row in rows]
        report.nodes.append(
            NodeReport(
                label=label,
                frames=len(rows),
                anchored=bool(anchored),
                before_cct=mad(before_cct),
                after_cct=mad(after_cct),
                before_ev=mad(before_ev),
                after_ev=mad(after_ev),
                bounded=sum(1 for row in rows if row[4] is not None),
            )
        )

    for photo_id, deviation, cct, skin in conn.execute(
        "SELECT photo_id, deviation, residual_cct, residual_skin_de00 FROM gallery_outlier "
        "WHERE project_id = ? ORDER BY deviation DESC LIMIT 50",
        (project,),
    ):
        parts = []
        if abs(cct) >= 1.0:
            parts.append(f"{cct:+.0f} K")
        if skin >= 0.1:
            parts.append(f"skin {skin:.1f} dE00")
        report.outliers.append((photo_id, deviation, ", ".join(parts) or "within tolerance"))

    report.skin = list(
        conn.execute(
            "SELECT identity_id, frames, spread_before, spread_after FROM gallery_skin_target "
            "WHERE project_id = ? ORDER BY spread_after DESC",
            (project,),
        )
    )

    # `GalleryCode::MoodPreserved` is bit 14 and `UserEdited` bit 16 in `GalleryCode::ALL` order.
    # Read as a mask rather than joined against a table, because a reason set is one integer.
    report.mood_preserved = conn.execute(
        "SELECT COUNT(*) FROM gallery_delta WHERE project_id = ? AND (reasons & ?) <> 0",
        (project, 1 << 14),
    ).fetchone()[0]
    report.user_edited = conn.execute(
        "SELECT COUNT(*) FROM gallery_delta WHERE project_id = ? AND user_edited = 1",
        (project,),
    ).fetchone()[0]

    return report


def bounds_respected(conn: sqlite3.Connection, project: str) -> list[str]:
    """Section 10.1's bounds gate, read off the stored rows.

    The SQL already refuses a row outside its CHECK, so a violation found here means a catalog
    written by a build whose bounds were wider - which is precisely what a support case is looking
    for and is invisible from inside that build.
    """
    breaches = []
    for column, bound in (
        ("d_cct", BOUNDS["cct"]),
        ("d_tint", BOUNDS["tint"]),
        ("d_exposure", BOUNDS["exposure"]),
        ("d_contrast", BOUNDS["contrast"]),
        ("d_saturation", BOUNDS["saturation"]),
    ):
        worst = conn.execute(
            f"SELECT COALESCE(MAX(ABS({column})), 0.0) FROM gallery_delta WHERE project_id = ?",
            (project,),
        ).fetchone()[0]
        if worst > bound + 1e-4:
            breaches.append(f"{column} reached {worst:.3f} against a bound of {bound}")
    return breaches


def render(report: ProjectReport, breaches: list[str]) -> int:
    """Print the report and return the exit code."""
    print(f"project {report.project}")
    print(f"  photographs      {report.photos}")
    print(
        f"  matched          {report.normalised}"
        f" ({report.normalised / report.photos * 100:.0f} %)"
        if report.photos
        else "  matched          0"
    )
    print(f"  parts            {len(report.nodes)}")
    print(
        f"  parts anchored   {report.anchored_nodes}"
        + (
            f" ({report.anchored_nodes / len(report.nodes) * 100:.0f} %)"
            if report.nodes
            else ""
        )
    )
    print(f"  left alone       {report.mood_preserved} for their light, {report.user_edited} by you")

    failures = 0

    before_cct, after_cct = report.spread("cct")
    if before_cct > 0:
        reduction = 1 - after_cct / before_cct
        verdict = "PASS" if reduction >= CCT_SPREAD_REDUCTION else "FAIL"
        failures += verdict == "FAIL"
        print(
            f"  warmth spread    {before_cct:.0f} K -> {after_cct:.0f} K"
            f" ({reduction * 100:.0f} % reduced)  [{verdict}]"
        )
    else:
        print("  warmth spread    not measurable - no node had two frames with an estimate")

    before_ev, after_ev = report.spread("ev")
    if before_ev > 0:
        reduction = 1 - after_ev / before_ev
        verdict = "PASS" if reduction >= EV_SPREAD_REDUCTION else "FAIL"
        failures += verdict == "FAIL"
        print(
            f"  bright spread    {before_ev:.3f} EV -> {after_ev:.3f} EV"
            f" ({reduction * 100:.0f} % reduced)  [{verdict}]"
        )
    else:
        print("  bright spread    not measurable")

    if report.skin:
        worst = max(row[3] for row in report.skin)
        verdict = "PASS" if worst <= SKIN_DE00_SPREAD_CEILING else "FAIL"
        failures += verdict == "FAIL"
        print(
            f"  skin spread      worst {worst:.2f} dE00 over {len(report.skin)} people"
            f"  [{verdict}]"
        )
    else:
        # **Not a pass.** Nothing about anybody's skin was measured, and reporting that as a met
        # promise is the single most damaging thing this script could do.
        print("  skin spread      NOT MEASURED - no identity has a gallery skin target")

    print(f"  still out of line {len(report.outliers)}")
    for photo_id, deviation, detail in report.outliers[:10]:
        print(f"    {photo_id[:16]}  {deviation * 100:3.0f} %  {detail}")

    if breaches:
        failures += 1
        print("  BOUNDS BREACHED:")
        for breach in breaches:
            print(f"    {breach}")
    else:
        print("  bounds           respected on every frame  [PASS]")

    return 1 if failures else 0


def self_test() -> int:
    """Run the whole computation against an in-memory catalog whose answer is chosen.

    There are no weddings in this repository, so this is what CI runs. It exercises the queries,
    the spread arithmetic, the gate comparisons and the report - and proves the script would catch
    a build whose bounds had widened, which is the one thing it can check that the Rust cannot.
    """
    conn = sqlite3.connect(":memory:")
    conn.executescript(
        """
        CREATE TABLE photo (photo_id TEXT PRIMARY KEY, project_id TEXT);
        CREATE TABLE gallery_node (
          node_id TEXT PRIMARY KEY, project_id TEXT, label TEXT, cct_k REAL, first_ts TEXT);
        CREATE TABLE gallery_delta (
          photo_id TEXT PRIMARY KEY, project_id TEXT, node_id TEXT,
          from_cct_k REAL, d_cct REAL, from_exposure_ev REAL, d_exposure REAL,
          bounded_by TEXT, d_tint REAL DEFAULT 0, d_contrast REAL DEFAULT 0,
          d_saturation REAL DEFAULT 0, reasons INTEGER DEFAULT 0, user_edited INTEGER DEFAULT 0);
        CREATE TABLE gallery_outlier (
          photo_id TEXT PRIMARY KEY, project_id TEXT, deviation REAL,
          residual_cct REAL, residual_skin_de00 REAL);
        CREATE TABLE gallery_skin_target (
          identity_id TEXT PRIMARY KEY, project_id TEXT, frames INTEGER,
          spread_before REAL, spread_after REAL);
        """
    )
    project = "prj_test"
    conn.execute(
        "INSERT INTO gallery_node VALUES ('nod_1', ?, 'Ceremony', 5000.0, '0')", (project,)
    )
    # Twenty frames spread 400 K apart, each moved 80 % of the way to 5,000 K. The after spread is
    # therefore a fifth of the before spread: an 80 % reduction, comfortably past the 60 % gate.
    for i in range(20):
        cct = 4800.0 + (i % 5) * 100.0
        conn.execute(
            "INSERT INTO gallery_delta (photo_id, project_id, node_id, from_cct_k, d_cct,"
            " from_exposure_ev, d_exposure, bounded_by) VALUES (?, ?, 'nod_1', ?, ?, ?, ?, NULL)",
            (f"pht_{i}", project, cct, (5000.0 - cct) * 0.8, (i % 5) * 0.05, -(i % 5) * 0.05 * 0.8),
        )
        conn.execute("INSERT INTO photo VALUES (?, ?)", (f"pht_{i}", project))
    conn.execute(
        "INSERT INTO gallery_skin_target VALUES ('idt_1', ?, 12, 2.6, 0.9)", (project,)
    )
    conn.execute(
        "INSERT INTO gallery_outlier VALUES ('pht_z', ?, 0.8, 310.0, 4.2)", (project,)
    )
    conn.commit()

    print("self-test: a project whose answer is chosen\n")
    report = measure(conn, project)
    code = render(report, bounds_respected(conn, project))
    if code != 0:
        print("\nself-test FAILED: the chosen project should pass every gate")
        return 1

    print("\nself-test: the same project written by a build whose bounds had widened")
    conn.execute("UPDATE gallery_delta SET d_cct = 900.0 WHERE photo_id = 'pht_0'")
    conn.commit()
    breaches = bounds_respected(conn, project)
    if not breaches:
        print("self-test FAILED: a 900 K movement was not reported as a breach")
        return 1
    print(f"  caught: {breaches[0]}")

    print("\nself-test: an unmeasured skin promise is not a met one")
    conn.execute("DELETE FROM gallery_skin_target")
    conn.commit()
    empty = measure(conn, project)
    if empty.skin:
        print("self-test FAILED: the skin targets did not clear")
        return 1
    print("  a project with no skin targets reports NOT MEASURED rather than PASS")

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
        worst = max(worst, render(measure(conn, project), bounds_respected(conn, project)))
        print()
    print(
        "These numbers describe what the pass did to a stored project. They say nothing about\n"
        "whether a photographer would call the result consistent - section 9's QAIQ audit is what\n"
        "answers that, and it has not been run. Exit report condition C3."
    )
    return worst


if __name__ == "__main__":
    sys.exit(main())
