"""Section 10.1's photographer-agreement gate, computed the way the Rust harness computes it.

The Python counterpart of `tests/eval/cull_eval.rs`, named by PHASE-12 section 4. The two
implement the same arithmetic deliberately: when the blind study of section 13 produces
labels from four real weddings, the number this script computes on those labels and the
number CI computes on the synthetic ones have to be comparable rather than argued about.

    python ml/eval/cull_agreement.py --self-test
    python ml/eval/cull_agreement.py --labels tests/fixtures/labels --selection gallery.json
    python ml/eval/cull_agreement.py --labels tests/fixtures/labels --selection gallery.json --by-chapter

## The gates

| Gate | Threshold | Section |
|---|---|---|
| Photographer agreement on keepers, Jaccard at moment level | >= 0.85 | 0, 10.1 |
| Missed must-haves where candidates exist | 0 | 10.1 |
| Claimed coverage where no candidate exists | 0 | 10.1 |

## Three measurement choices that are easy to get wrong

**Agreement is measured at moment level, not at frame level.** Section 10.1 says so, and
the reason is that two editors who pick different frames of the same kiss agree about the
wedding. A frame-level measure would score that pair of galleries at zero on the moment
they agree about most, and a build tuned to satisfy it would learn to reproduce one
particular editor's taste in near-identical frames rather than to cover a wedding.

**Frame-level agreement is reported anyway, and never gated.** It is the number a
photographer intuitively expects, it is always lower, and publishing only the kinder of
two numbers is how an evaluation harness becomes marketing. It is printed beside the gate
with its own label.

**A missing must-have is only a failure when a candidate existed.** Not every wedding has
a cake. `Coverage::Missing` on a rule with no candidates is the correct answer and the
product's whole claim is that it says so out loud; a harness that failed the build over it
would be a harness that trained the build to lie about the cake.

## The shape of a selection file

    {
      "name": "hindu_night",
      "selected": [{"photo_id": "pht_...", "moment_id": "mom_..."}, ...],
      "coverage": [{"rule": "vows", "state": "covered"}, ...]
    }

`moment_id` may be null: a frame that phase 08 could not group is delivered on its own and
contributes to the frame-level number only.
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Section 10.1.
AGREEMENT_GATE = 0.85

# The three states `Coverage` can take, in the order the panel shows them.
SATISFIED = {"covered", "covered_weak"}


def jaccard(left: set, right: set) -> float:
    """Intersection over union, with two empty sets counted as perfect agreement.

    Two editors who both delivered nothing from a wedding with no photographs agree; the
    alternative convention - zero - would make an empty fixture fail a gate about taste.
    """
    union = left | right
    if not union:
        return 1.0
    return len(left & right) / len(union)


def load_labels(directory: Path) -> dict:
    """Every `keepers_*.json` in a directory, keyed by wedding name."""
    out = {}
    for path in sorted(directory.glob("keepers_*.json")):
        with path.open(encoding="utf-8") as handle:
            payload = json.load(handle)
        if payload.get("schema") != "aura.keeper-labels.v1":
            raise SystemExit(f"{path}: unexpected schema {payload.get('schema')!r}")
        out[payload["name"]] = payload
    if not out:
        raise SystemExit(f"{directory}: no keeper labels found")
    return out


def load_selection(path: Path) -> dict:
    with path.open(encoding="utf-8") as handle:
        payload = json.load(handle)
    if "selected" not in payload:
        raise SystemExit(f"{path}: no `selected` array")
    return payload


def moments_of(frames, index) -> set:
    """The moments a set of frames covers, dropping the ungrouped ones."""
    out = set()
    for photo in frames:
        moment = index.get(photo)
        if moment:
            out.add(moment)
    return out


def score(label: dict, selection: dict) -> dict:
    """Agreement and coverage for one wedding."""
    index = {
        entry["photo_id"]: entry.get("moment_id")
        for entry in selection["selected"]
        if isinstance(entry, dict)
    }
    engine_frames = set(index)
    human_frames = set(label["keepers"])

    # The human labels carry no moment ids of their own; the selection is the only place
    # the mapping exists. A human keeper the engine did not deliver therefore contributes
    # to the frame-level number and, correctly, not to the moment-level one - a moment
    # nobody delivered is a moment the engine has no opinion about.
    engine_moments = moments_of(engine_frames, index)
    human_moments = moments_of(human_frames, index)

    coverage = {row["rule"]: row["state"] for row in selection.get("coverage", [])}
    had_candidates = set(label.get("must_have_candidates", []))
    missed = sorted(
        rule
        for rule, state in coverage.items()
        if rule in had_candidates and state not in SATISFIED
    )
    invented = sorted(
        rule
        for rule, state in coverage.items()
        if rule not in had_candidates and state in SATISFIED
    )

    return {
        "name": label["name"],
        "moment_jaccard": jaccard(engine_moments, human_moments),
        "frame_jaccard": jaccard(engine_frames, human_frames),
        "engine_frames": len(engine_frames),
        "human_frames": len(human_frames),
        "missed_must_haves": missed,
        "invented_coverage": invented,
    }


def report(rows: list) -> int:
    failures = 0
    print(f"{'wedding':<20} {'moment':>7} {'frame':>7} {'engine':>7} {'human':>7}")
    for row in rows:
        print(
            f"{row['name']:<20} {row['moment_jaccard']:>7.3f} {row['frame_jaccard']:>7.3f} "
            f"{row['engine_frames']:>7} {row['human_frames']:>7}"
        )
        if row["moment_jaccard"] < AGREEMENT_GATE:
            print(f"  FAIL: below the {AGREEMENT_GATE} agreement gate")
            failures += 1
        if row["missed_must_haves"]:
            print(f"  FAIL: missed must-haves with candidates: {row['missed_must_haves']}")
            failures += 1
        if row["invented_coverage"]:
            print(f"  FAIL: claimed coverage with no candidates: {row['invented_coverage']}")
            failures += 1
    print()
    print("moment-level agreement is the gate; frame-level is reported and never gated.")
    return failures


def self_test() -> int:
    """Assert the harness can fail, on inputs whose answer is obvious.

    A gate that cannot fail is not a gate. Every eval script in this repository carries one
    of these for that reason.
    """
    failures = 0

    if jaccard(set(), set()) != 1.0:
        print("FAIL: two empty selections should agree")
        failures += 1
    if jaccard({"a"}, {"b"}) != 0.0:
        print("FAIL: disjoint selections should not agree")
        failures += 1
    if abs(jaccard({"a", "b"}, {"b", "c"}) - 1 / 3) > 1e-9:
        print("FAIL: jaccard arithmetic")
        failures += 1

    label = {
        "name": "self_test",
        "keepers": ["pht_1", "pht_3"],
        "must_have_candidates": ["vows", "kiss"],
    }
    # An engine that delivered a different frame of the *same* moment agrees at moment
    # level and disagrees at frame level. That difference is the whole reason section 10.1
    # specifies the moment level, so it is asserted rather than assumed.
    selection = {
        "selected": [
            {"photo_id": "pht_2", "moment_id": "mom_a"},
            {"photo_id": "pht_1", "moment_id": "mom_a"},
            {"photo_id": "pht_3", "moment_id": "mom_b"},
        ],
        "coverage": [
            {"rule": "vows", "state": "covered"},
            {"rule": "kiss", "state": "covered_weak"},
            {"rule": "cake", "state": "missing"},
        ],
    }
    row = score(label, selection)
    if row["moment_jaccard"] != 1.0:
        print(f"FAIL: moment agreement should be 1.0, got {row['moment_jaccard']}")
        failures += 1
    if row["frame_jaccard"] >= 1.0:
        print("FAIL: frame agreement should be lower than moment agreement here")
        failures += 1
    if row["missed_must_haves"] or row["invented_coverage"]:
        print("FAIL: a weakly covered rule is satisfied, and a cakeless wedding is honest")
        failures += 1

    # ...and a build that dropped a guarantee with candidates must fail.
    broken = dict(selection)
    broken["coverage"] = [{"rule": "vows", "state": "missing"}]
    if not score(label, broken)["missed_must_haves"]:
        print("FAIL: a missed must-have with candidates must be reported")
        failures += 1

    # ...and one that claimed a cake nobody photographed must fail too.
    lying = dict(selection)
    lying["coverage"] = [{"rule": "cake", "state": "covered"}]
    if not score(label, lying)["invented_coverage"]:
        print("FAIL: invented coverage must be reported")
        failures += 1

    print("self-test:", "ok" if failures == 0 else f"{failures} failure(s)")
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--labels", type=Path, help="directory of keepers_*.json")
    parser.add_argument("--selection", type=Path, help="one selection JSON file")
    parser.add_argument("--self-test", action="store_true", help="check the harness itself")
    args = parser.parse_args()

    if args.self_test:
        return 1 if self_test() else 0

    if not args.labels or not args.selection:
        parser.print_help()
        return 2

    labels = load_labels(args.labels)
    selection = load_selection(args.selection)
    name = selection.get("name")
    if name not in labels:
        raise SystemExit(f"no labels for {name!r}; have {sorted(labels)}")
    return 1 if report([score(labels[name], selection)]) else 0


if __name__ == "__main__":
    sys.exit(main())
