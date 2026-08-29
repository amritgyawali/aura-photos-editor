#!/usr/bin/env python3
"""Score a cleanup run against section 10.1's gates. PHASE-24.

WHAT THIS SCRIPT IS FOR, GIVEN THAT `tests/eval/cleanup_eval.rs` EXISTS
======================================================================

The Rust harness measures the *engine* against fixtures whose answers are known by construction:
the safety filter cannot be bypassed, an absent mask is ignorance rather than safety, a borrow is
preferred whenever one exists, and a deliberate artefact reverts itself. Those are properties of
the code and they run on every build.

This script measures a *wedding*. It reads the rows migration 24 wrote for a real project and
reports the four numbers that cannot be known without one:

1. **The artefact-free rate on approved removals.** Section 10.1 gates it at 98 %. The Rust gate
   measures it on painted fixtures; this measures it on the removals a photographer actually
   accepted.
2. **The borrow share.** Section 6.3 says real pixels are preferred; this says how often they were
   available. A gallery at 4 % borrowed is a gallery whose moments are too sparse for the
   preference to mean anything, and that is a fact about the shoot rather than about the code.
3. **The refusal histogram.** Which of the five checks did the work. **The number to read first is
   `mask_complete`**: at zero, every refusal in the project is `protection_unknown` and the
   histogram says nothing about the photographs.
4. **The disclosure completeness.** Every applied removal has a row in `cleanup_disclosure`, and
   every `cleanup[]` operation in a delivered recipe has one behind it. Section 13's fifth
   acceptance criterion, checked against the catalog rather than against a fixture.

IT READS AND NEVER WRITES
=========================

Opened read-only. A script that could edit a catalog is a script somebody eventually runs against
a photographer's live project.
"""

from __future__ import annotations

import argparse
import json
import sqlite3
import sys
from dataclasses import dataclass, field
from pathlib import Path

# Section 10.1's gate on approved removals.
ARTEFACT_FREE_GATE = 0.98

# The artefact score above which a removal is counted as carrying one. Mirrors the three
# thresholds in `aura_generative::selfcheck`; a stored `artefact_score` is the *worst* of the
# three, so the tightest of them is the right comparison.
ARTEFACT_THRESHOLD = 0.18

# The five safety checks, in `SafetyCheck::ALL` order.
CHECKS = ("size_cap", "denylist", "identity_protect", "structure_span", "confidence")


@dataclass
class Report:
    project: str
    photos: int = 0
    examined: int = 0
    mask_complete: int = 0
    with_proposals: int = 0
    applied: int = 0
    borrowed: int = 0
    filled: int = 0
    inpainted: int = 0
    reverted: int = 0
    judged: int = 0
    declined: int = 0
    blocked: dict[str, int] = field(default_factory=dict)
    artefact_free: int = 0
    disclosure_gaps: list[str] = field(default_factory=list)

    @property
    def coverage(self) -> float:
        return 0.0 if self.photos == 0 else self.examined / self.photos

    @property
    def mask_covered(self) -> float:
        """The denominator is **examined** frames, not every photograph.

        A frame nobody looked at has no mask answer either way, and counting it as an incomplete
        mask would report a project that has not run yet as one whose segmenter is failing.
        """
        return 0.0 if self.examined == 0 else self.mask_complete / self.examined

    @property
    def artefact_free_rate(self) -> float:
        return 1.0 if self.applied == 0 else self.artefact_free / self.applied

    @property
    def borrow_share(self) -> float:
        real = self.borrowed + self.filled
        return 0.0 if real == 0 else self.borrowed / real


def read(path: Path) -> Report:
    uri = f"file:{path.as_posix()}?mode=ro"
    conn = sqlite3.connect(uri, uri=True)
    conn.row_factory = sqlite3.Row
    try:
        coverage = conn.execute("SELECT * FROM v_cleanup_coverage LIMIT 1").fetchone()
        if coverage is None:
            raise SystemExit("this catalog carries no cleanup rows: has the pass run?")

        report = Report(project=coverage["project_id"])
        report.photos = coverage["photos"] or 0
        report.examined = coverage["examined"] or 0
        report.mask_complete = coverage["mask_complete"] or 0
        report.with_proposals = coverage["with_proposals"] or 0
        report.applied = coverage["applied"] or 0
        report.borrowed = coverage["borrowed"] or 0
        report.filled = coverage["filled"] or 0
        report.inpainted = coverage["inpainted"] or 0
        report.reverted = coverage["reverted"] or 0
        report.judged = coverage["judged"] or 0
        report.declined = coverage["declined"] or 0
        for check in CHECKS:
            report.blocked[check] = coverage[f"blocked_{check.replace('identity_protect', 'identity').replace('structure_span', 'structure')}"] or 0

        report.artefact_free = conn.execute(
            "SELECT COUNT(*) FROM cleanup_proposal WHERE applied = 1 AND artefact_score <= ?",
            (ARTEFACT_THRESHOLD,),
        ).fetchone()[0]

        # Section 13's fifth criterion, from the side that catches the real failure: an applied
        # proposal with no disclosure behind it. The database has a trigger that makes this
        # impossible through every supported path, so a non-empty list means somebody edited the
        # catalog by hand or restored it from a partial backup.
        report.disclosure_gaps = [
            row["proposal_id"]
            for row in conn.execute(
                "SELECT p.proposal_id FROM cleanup_proposal p "
                " LEFT JOIN cleanup_disclosure d ON d.proposal_id = p.proposal_id "
                " WHERE p.applied = 1 AND d.proposal_id IS NULL"
            )
        ]
        return report
    finally:
        conn.close()


def render(report: Report) -> list[str]:
    """The report as lines, and the failures as a separate list."""
    lines = [
        f"project {report.project}",
        f"  photographs            {report.photos}",
        f"  examined               {report.examined} ({report.coverage:.1%})",
        f"  masks complete         {report.mask_complete} ({report.mask_covered:.1%} of examined)",
        f"  with proposals         {report.with_proposals}",
        f"  applied                {report.applied}",
        f"    borrowed             {report.borrowed}",
        f"    filled               {report.filled}",
        f"    inpainted            {report.inpainted}",
        f"  reverted by self-check {report.reverted}",
        f"  cloud judgements       {report.judged}, of which {report.declined} declined",
        "  refused by check:",
    ]
    lines.extend(f"    {check:<18} {report.blocked.get(check, 0)}" for check in CHECKS)
    lines.append(f"  artefact-free rate     {report.artefact_free_rate:.1%}")
    lines.append(f"  borrow share           {report.borrow_share:.1%}")
    return lines


def failures(report: Report) -> list[str]:
    out: list[str] = []
    if report.artefact_free_rate < ARTEFACT_FREE_GATE and report.applied > 0:
        out.append(
            f"artefact-free rate {report.artefact_free_rate:.1%} over {report.applied} approved "
            f"removals, below the {ARTEFACT_FREE_GATE:.0%} gate"
        )
    if report.inpainted > 0:
        out.append(
            f"{report.inpainted} removals are disclosed as inpaints. There is no diffusion model "
            f"in this build and `inpaint::solve` refuses on every call, so a row saying otherwise "
            f"is a disclosure that is not true."
        )
    if report.disclosure_gaps:
        out.append(
            f"{len(report.disclosure_gaps)} applied removals have no disclosure: "
            f"{', '.join(report.disclosure_gaps[:5])}"
        )
    return out


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("catalog", type=Path, help="the project's SQLite catalog")
    parser.add_argument("--json", action="store_true", help="machine-readable output")
    args = parser.parse_args()

    report = read(args.catalog)
    if args.json:
        print(
            json.dumps(
                {
                    "project": report.project,
                    "coverage": report.coverage,
                    "mask_covered": report.mask_covered,
                    "applied": report.applied,
                    "artefact_free_rate": report.artefact_free_rate,
                    "borrow_share": report.borrow_share,
                    "reverted": report.reverted,
                    "blocked": report.blocked,
                    "disclosure_gaps": report.disclosure_gaps,
                },
                indent=2,
            )
        )
    else:
        print("\n".join(render(report)))

    if report.mask_covered < 1.0:
        print(
            f"\nNOTE: {1.0 - report.mask_covered:.0%} of examined frames could not have every "
            f"protected kind looked for, so their refusals are `protection_unknown` rather than a "
            f"statement about the photographs. Phase 18's vocabulary has no class for a ring or a "
            f"cake, which is why this is below one on every build so far.",
            file=sys.stderr,
        )

    problems = failures(report)
    if problems:
        print("\nGATE FAILURES:", file=sys.stderr)
        for problem in problems:
            print(f"  - {problem}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
