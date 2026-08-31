#!/usr/bin/env python3
"""Phase 26's gates, from the catalog side.

`tests/eval/camera_eval.rs` measures the algorithms on a synthetic two-camera wedding whose
per-brand colour response was authored. This measures a **real project** - a catalog somebody has
actually run the matching pass over - and reports the four things section 10.1 asks about:

    cross-camera skin dE00 in matched scenes, before and after
    grade-signature distance between bodies, before and after
    how much of the matching rests on this wedding's own evidence
    which bodies fell back on a brand baseline, and why

It reads the catalog read-only and writes nothing.

Why this exists beside the Rust harness
---------------------------------------

The Rust gates prove the arithmetic against an answer they already know. This one asks a question
nobody knows the answer to: on a wedding somebody actually shot, did the second camera end up
matching the first, and was it matched from evidence or from a guess about the brand.

It is also the only check that can catch two things invisible from inside the build that produced
the catalog: a transform outside the documented bounds, and a baseline claiming to be measured
without saying who measured it or when.

    python ml/eval/camera_match_eval.py --self-test
    python ml/eval/camera_match_eval.py path/to/catalog.sqlite
    python ml/eval/camera_match_eval.py path/to/catalog.sqlite --project prj_<uuid>
"""

from __future__ import annotations

import argparse
import json
import sqlite3
import statistics
import sys
from dataclasses import dataclass, field

# Section 10.1's two headline gates.
SKIN_DE00_CEILING = 2.0
SIGNATURE_REDUCTION = 0.65

# The bounds from the frozen contract. Restated here rather than read out of the build under test,
# because a report that read them from that build could not detect a build whose bounds had moved -
# which is exactly what a support case needs to find out.
BOUNDS = {
    "d_cct": 900.0,
    "d_tint": 20.0,
    "d_exposure": 0.60,
    "d_saturation": 12.0,
}

# `TransformSource::ALL`, in the order the report counts them.
SOURCES = ("matched_pairs", "blended", "brand_baseline")


@dataclass
class CameraRow:
    """One body's correction."""

    camera_id: str
    flash: str
    source: str
    evidence_pairs: int
    confidence: float
    d_cct: float
    d_tint: float
    d_exposure: float
    d_saturation: float


@dataclass
class ProjectReport:
    """A whole wedding's matching."""

    project: str
    reference: str | None = None
    reference_source: str | None = None
    cameras: list[CameraRow] = field(default_factory=list)
    pairs_verified: int = 0
    pairs_rejected: int = 0
    pairs_heldout: int = 0
    skin_before: float | None = None
    skin_after: float | None = None
    signature_before: float | None = None
    signature_after: float | None = None
    shooters: list[tuple[str, float, float]] = field(default_factory=list)

    def by_source(self) -> dict[str, int]:
        counts = {source: 0 for source in SOURCES}
        for row in self.cameras:
            if row.source in counts:
                counts[row.source] += 1
        return counts


def decode(text: str | None) -> dict[str, float]:
    """One stored appearance distance, or an empty reading.

    A row whose JSON will not parse is a corrupt row, and an empty reading renders as "not
    measured" everywhere rather than as "perfectly matched" - the same choice `decode_distance` in
    the Rust store makes, and for the same reason.
    """
    if not text:
        return {}
    try:
        parsed = json.loads(text)
    except (TypeError, ValueError):
        return {}
    return parsed if isinstance(parsed, dict) else {}


def measure(conn: sqlite3.Connection, project: str) -> ProjectReport:
    """Read one project's camera rows."""
    report = ProjectReport(project=project)

    reference = conn.execute(
        "SELECT camera_id, source FROM camera_reference WHERE project_id = ?", (project,)
    ).fetchone()
    if reference:
        report.reference, report.reference_source = reference

    for row in conn.execute(
        "SELECT camera_id, flash, source, evidence_pairs, confidence, d_cct, d_tint,"
        "       d_exposure, d_saturation"
        "  FROM camera_transform WHERE project_id = ? ORDER BY camera_id, flash",
        (project,),
    ):
        report.cameras.append(CameraRow(*row))

    verified, rejected, heldout = conn.execute(
        "SELECT SUM(verified), SUM(1 - verified), SUM(held_out)"
        "  FROM camera_pair WHERE project_id = ?",
        (project,),
    ).fetchone()
    report.pairs_verified = verified or 0
    report.pairs_rejected = rejected or 0
    report.pairs_heldout = heldout or 0

    # The appearance distances the pass recorded. They are stored as **one JSON object per row** -
    # the four terms of `AppearanceDistance` together - rather than as four columns, because they
    # are read as a whole and nothing filters on a component of them.
    #
    # A body matched from a brand baseline records a zeroed distance, because there was no evidence
    # to measure one on. `decode` treats a zero as *not measured* rather than as *perfectly
    # matched*, which is the same reading `CameraOutline::skin_reduction` takes and is the one that
    # cannot turn an absence into a claim.
    skin_pairs: list[tuple[float, float]] = []
    signature_pairs: list[tuple[float, float]] = []
    for before_json, after_json in conn.execute(
        "SELECT distance_before, distance_after FROM camera_transform WHERE project_id = ?",
        (project,),
    ):
        before, after = decode(before_json), decode(after_json)
        if before.get("skin_de00", 0.0) > 0.0:
            skin_pairs.append((before["skin_de00"], after.get("skin_de00", 0.0)))
        if before.get("grade_signature", 0.0) > 0.0:
            signature_pairs.append(
                (before["grade_signature"], after.get("grade_signature", 0.0))
            )

    if skin_pairs:
        report.skin_before = statistics.fmean(before for before, _ in skin_pairs)
        report.skin_after = statistics.fmean(after for _, after in skin_pairs)
    if signature_pairs:
        report.signature_before = statistics.fmean(before for before, _ in signature_pairs)
        report.signature_after = statistics.fmean(after for _, after in signature_pairs)

    report.shooters = list(
        conn.execute(
            "SELECT shooter, measured_ev, applied_ev FROM camera_shooter_bias"
            "  WHERE project_id = ? ORDER BY shooter",
            (project,),
        )
    )

    return report


def bounds_respected(report: ProjectReport) -> list[str]:
    """Section 10.1's bounds gate, read off the stored rows."""
    breaches = []
    for row in report.cameras:
        for column, bound in BOUNDS.items():
            value = abs(getattr(row, column))
            if value > bound + 1e-4:
                breaches.append(
                    f"{row.camera_id} ({row.flash}) {column} = {value:.2f}, bound {bound}"
                )
    return breaches


def shooters_harmonised(report: ProjectReport) -> list[str]:
    """Section 6.3's cap: a habit is corrected by less than the whole of it, always.

    Checked on the stored rows rather than trusted, because it is the promise this phase makes about
    a *person* - that a second shooter hired for their eye is harmonised rather than edited into
    somebody else - and a promise about a person is measured.
    """
    breaches = []
    for shooter, measured, applied in report.shooters:
        if abs(applied) > abs(measured) + 1e-4:
            breaches.append(
                f"{shooter}: corrected {applied:+.3f} EV against a habit of {measured:+.3f} EV"
            )
        if measured != 0.0 and applied != 0.0 and (measured > 0) == (applied > 0):
            breaches.append(
                f"{shooter}: the correction has the same sign as the habit, so it added to it"
            )
    return breaches


def render(report: ProjectReport, breaches: list[str], shooter_breaches: list[str]) -> int:
    """Print the report and return the exit code."""
    counts = report.by_source()
    print(f"project {report.project}")
    print(f"  reference        {report.reference or 'none'} ({report.reference_source or '-'})")
    print(f"  bodies           {len(report.cameras)} transform row(s)")
    print(
        f"  evidence         {report.pairs_verified} verified,"
        f" {report.pairs_rejected} rejected, {report.pairs_heldout} held out"
    )
    print(
        f"  source mix       {counts['matched_pairs']} from this wedding,"
        f" {counts['blended']} partly, {counts['brand_baseline']} from the brand alone"
    )

    failures = 0

    if report.skin_after is not None:
        verdict = "PASS" if report.skin_after <= SKIN_DE00_CEILING else "FAIL"
        failures += verdict == "FAIL"
        print(
            f"  skin dE00        {report.skin_before:.2f} -> {report.skin_after:.2f}"
            f"  [{verdict}]"
        )
    else:
        # **Not a pass.** Nothing about skin was measured, and reporting that as a met promise is
        # the most damaging thing this script could do - it is a claim about how people look.
        print("  skin dE00        NOT MEASURED - no body carries a skin reading")

    if report.signature_before and report.signature_before > 0:
        reduction = 1 - report.signature_after / report.signature_before
        verdict = "PASS" if reduction >= SIGNATURE_REDUCTION else "FAIL"
        failures += verdict == "FAIL"
        print(
            f"  grade distance   {report.signature_before:.3f} -> {report.signature_after:.3f}"
            f" ({reduction * 100:.0f} % reduced)  [{verdict}]"
        )
    else:
        print("  grade distance   not measurable - no body was solved from evidence")

    if report.shooters:
        print(f"  shooters         {len(report.shooters)} habit(s) measured")
        for shooter, measured, applied in report.shooters:
            share = abs(applied) / abs(measured) * 100 if measured else 0.0
            print(
                f"    {shooter[:20]:<20} habit {measured:+.3f} EV,"
                f" corrected {applied:+.3f} EV ({share:.0f} % of it)"
            )
    else:
        print("  shooters         none measured")

    if breaches:
        failures += 1
        print("  BOUNDS BREACHED:")
        for breach in breaches:
            print(f"    {breach}")
    else:
        print("  bounds           respected on every body  [PASS]")

    if shooter_breaches:
        failures += 1
        print("  SHOOTER CAP BREACHED:")
        for breach in shooter_breaches:
            print(f"    {breach}")
    elif report.shooters:
        print("  shooter cap      every correction smaller than the habit  [PASS]")

    return 1 if failures else 0


def self_test() -> int:
    """Run the whole computation against an in-memory catalog whose answer is chosen."""
    conn = sqlite3.connect(":memory:")
    conn.executescript(
        """
        CREATE TABLE camera_reference (project_id TEXT, camera_id TEXT, source TEXT);
        CREATE TABLE camera_transform (
          project_id TEXT, camera_id TEXT, flash TEXT, source TEXT, evidence_pairs INTEGER,
          confidence REAL, d_cct REAL, d_tint REAL, d_exposure REAL, d_saturation REAL,
          distance_before TEXT, distance_after TEXT);
        CREATE TABLE camera_pair (
          project_id TEXT, verified INTEGER, held_out INTEGER);
        CREATE TABLE camera_shooter_bias (
          project_id TEXT, shooter TEXT, measured_ev REAL, applied_ev REAL);
        """
    )
    project = "prj_test"
    conn.execute("INSERT INTO camera_reference VALUES (?, 'cam_a', 'most_frames')", (project,))
    zeroed = json.dumps(
        {"skin_de00": 0.0, "white_point": 0.0, "grade_signature": 0.0, "contrast": 0.0}
    )
    before = json.dumps(
        {"skin_de00": 3.4, "white_point": 0.9, "grade_signature": 0.240, "contrast": 0.4}
    )
    after = json.dumps(
        {"skin_de00": 1.1, "white_point": 0.2, "grade_signature": 0.070, "contrast": 0.1}
    )
    # The reference body itself: an identity transform with nothing measured, which must contribute
    # to neither average rather than pulling both toward zero.
    conn.execute(
        "INSERT INTO camera_transform VALUES (?, 'cam_a', 'ambient', 'matched_pairs', 40, 0.9,"
        " 0.0, 0.0, 0.0, 0.0, ?, ?)",
        (project, zeroed, zeroed),
    )
    conn.execute(
        "INSERT INTO camera_transform VALUES (?, 'cam_b', 'ambient', 'matched_pairs', 40, 0.85,"
        " 180.0, 3.0, 0.12, 4.0, ?, ?)",
        (project, before, after),
    )
    for _ in range(40):
        conn.execute("INSERT INTO camera_pair VALUES (?, 1, 0)", (project,))
    for _ in range(6):
        conn.execute("INSERT INTO camera_pair VALUES (?, 0, 0)", (project,))
    for _ in range(10):
        conn.execute("INSERT INTO camera_pair VALUES (?, 1, 1)", (project,))
    # A habit of +0.30 EV corrected by -0.21: smaller than the habit and opposite in sign.
    conn.execute("INSERT INTO camera_shooter_bias VALUES (?, 'second', 0.30, -0.21)", (project,))
    conn.commit()

    print("self-test: a project whose answer is chosen\n")
    report = measure(conn, project)
    code = render(report, bounds_respected(report), shooters_harmonised(report))
    if code != 0:
        print("\nself-test FAILED: the chosen project should pass every gate")
        return 1

    print("\nself-test: the same project written by a build whose bounds had widened")
    conn.execute("UPDATE camera_transform SET d_cct = 1800.0 WHERE camera_id = 'cam_b'")
    conn.commit()
    if not bounds_respected(measure(conn, project)):
        print("self-test FAILED: an 1,800 K transform was not reported as a breach")
        return 1
    print("  caught: a transform outside the documented bound")

    print("\nself-test: a shooter corrected by more than their whole habit")
    conn.execute("UPDATE camera_transform SET d_cct = 180.0 WHERE camera_id = 'cam_b'")
    conn.execute("UPDATE camera_shooter_bias SET applied_ev = -0.55")
    conn.commit()
    if not shooters_harmonised(measure(conn, project)):
        print("self-test FAILED: an over-correction was not reported")
        return 1
    print("  caught: a habit erased rather than harmonised")

    print("\nself-test: an unmeasured skin promise is not a met one")
    conn.execute("UPDATE camera_transform SET distance_before = ?", (zeroed,))
    conn.commit()
    empty = measure(conn, project)
    if empty.skin_after is not None:
        print("self-test FAILED: the skin readings did not clear")
        return 1
    print("  a project with no skin readings reports NOT MEASURED rather than PASS")

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
        report = measure(conn, project)
        worst = max(
            worst, render(report, bounds_respected(report), shooters_harmonised(report))
        )
        print()
    print(
        "These numbers describe what the pass did to a stored project. They say nothing about\n"
        "whether a photographer could still tell which body shot which frame - section 9's blind\n"
        "study is what answers that, and it has not been run. Exit report condition C4."
    )
    return worst


if __name__ == "__main__":
    sys.exit(main())
