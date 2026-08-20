#!/usr/bin/env python3
"""Phase 17 ships no model, and this script is what keeps that true.

Every phase since 05 that shipped a learned head has an ``export.py`` beside its trainer. This
one exists to say plainly that there is nothing to export, and to **check** it - because "we
did not add a model" is the kind of statement that quietly stops being true.

Why there is no model
---------------------

Section 6.2 asks for "a small ridge-regularised model on features ... predicting each delta",
with James-Stein shrinkage and a Huber loss. All three have closed forms. The fit is a
seven-by-seven linear solve repeated forty-one times, and it runs in
``crates/aura-style/src/tree.rs`` in about a millisecond on a wedding's worth of pairs.

There is nothing to train offline, nothing to quantise, nothing to sign and no card to write.
``models/models.lock`` is untouched by this phase - the third time since phase 08, and the
first time it is a statement about the **method** rather than about the data.

What that is not
----------------

It is not a claim that this phase is fully proven. There are no photographers' archives in
this repository, so the *inputs* are missing even though the method is complete. That is
condition C1 in ``docs/progress/PHASE-17-EXIT.md`` and it is a different gap from every
placeholder-weights condition before it: those phases have real code waiting for real weights,
and this one has real code waiting for real weddings.

``--check`` verifies three things
---------------------------------

1. ``models.lock`` names no style model, so a future change that adds one has to change this
   script too.
2. The constants this phase's Python and Rust both restate agree with each other.
3. The bundle format's schema number matches what the Rust side writes, because a profile
   bundle is the only artefact this phase produces and it is the one thing that leaves a
   machine.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# The constants both sides restate. `train_residual.py` carries the same values and
# `crates/aura-style/src/tree.rs` is the authority; a disagreement here is a real defect and
# not a formatting difference.
SHARED = {
    "FEATURES": 7,
    "RIDGE": 0.1,
    "PRIOR_STRENGTH": 12.0,
    "GLOBAL_PRIOR": 40.0,
    "MAX_ARCHIVE_INFLUENCE": 0.35,
    "HUBER_K": 1.345,
    "HUBER_PASSES": 2,
    "HOLDOUT_EVERY": 5,
}

BUNDLE_SCHEMA = 1

REPO = Path(__file__).resolve().parents[3]


def check_no_model() -> list[str]:
    """`models.lock` must name nothing to do with style."""
    lock = REPO / "models" / "models.lock"
    if not lock.exists():
        return [f"{lock} is missing"]
    payload = json.loads(lock.read_text(encoding="utf-8"))
    names = [str(model.get("name", "")) for model in payload.get("models", [])]
    offenders = [name for name in names if "style" in name.lower()]
    if offenders:
        return [
            "models.lock names a style model: "
            + ", ".join(offenders)
            + " - if this phase now ships a model, this script has to export it and the exit "
            "report has to stop saying it does not"
        ]
    return []


def check_constants() -> list[str]:
    """The Rust and the Python must agree about every shared number.

    `FEATURES` lives in `bucket.rs` because it is a property of the design vector rather than
    of the regression; everything else lives in `tree.rs`. Both files are read so that moving
    a constant between them is a change this check notices rather than one it silently passes.
    """
    source = "\n".join(
        (REPO / "crates" / "aura-style" / "src" / name).read_text(encoding="utf-8")
        for name in ("tree.rs", "bucket.rs")
    )
    problems = []
    for name, expected in SHARED.items():
        match = re.search(rf"pub const {name}: [a-z0-9]+ = ([0-9._]+);", source)
        if not match:
            problems.append(f"{name} is not declared in tree.rs or bucket.rs")
            continue
        found = float(match.group(1).rstrip("."))
        if abs(found - float(expected)) > 1e-9:
            problems.append(f"{name}: Rust says {found}, this file says {expected}")
    return problems


def check_bundle_schema() -> list[str]:
    """The bundle schema number is the one thing about this phase that leaves a machine."""
    source = (REPO / "crates" / "aura-style" / "src" / "profile.rs").read_text(encoding="utf-8")
    match = re.search(r"pub const BUNDLE_SCHEMA: u16 = ([0-9]+);", source)
    if not match:
        return ["BUNDLE_SCHEMA is not declared in profile.rs"]
    found = int(match.group(1))
    if found != BUNDLE_SCHEMA:
        return [f"BUNDLE_SCHEMA: Rust says {found}, this file says {BUNDLE_SCHEMA}"]
    return []


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="verify there is nothing to export")
    args = parser.parse_args()

    if not args.check:
        print(
            "Phase 17 ships no model. There is nothing to export: the fit is a closed-form "
            "ridge solve with James-Stein shrinkage and a Huber reweighting, and it runs in "
            "crates/aura-style/src/tree.rs. Run --check to verify that is still true."
        )
        return 0

    problems = check_no_model() + check_constants() + check_bundle_schema()
    for problem in problems:
        print(f"  FAIL {problem}")
    if problems:
        print(f"\nexport check: {len(problems)} problem(s)")
        return 1
    print("  ok   models.lock names no style model")
    print("  ok   every shared constant agrees with crates/aura-style/src")
    print(f"  ok   the bundle schema is {BUNDLE_SCHEMA} on both sides")
    print("\nexport check: OK - this phase ships no model, and that is still true")
    return 0


if __name__ == "__main__":
    sys.exit(main())
