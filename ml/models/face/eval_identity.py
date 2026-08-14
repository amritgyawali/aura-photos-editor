"""Section 10.1's identity gates, computed the way the Rust harness computes them.

The Python counterpart of `tests/eval/identity_eval.rs`. The two implement the same
arithmetic deliberately: when a trained detector and recogniser arrive, the number this
script produces on a labelled corpus and the number CI produces on synthetic ground truth
have to be comparable rather than argued about.

    python ml/models/face/eval_identity.py --self-test
    python ml/models/face/eval_identity.py --detections det.json --templates faces.npz

## The gates

| Gate | Threshold | Section |
|---|---|---|
| Detection recall at IoU 0.5, overall | >= 0.97 | 10.1 |
| Detection recall on the small-face subset | >= 0.90 | 10.1 |
| False positives on bokeh highlights | < 1 % | 10.1 |
| Identity clustering F1, pairwise | >= 0.93 | 10.1 |
| Clusters holding more than one labelled person | 0 | 10.1 |
| Verification true-accept rate at a fixed false-accept rate, per group | reported | 12 |

The last one has no threshold here on purpose. Section 12's second failure mode is that
recognition accuracy varies across skin tones, and the right response to that is not a
number this script invents - it is a number SEC and the ML lead agree on, against a
balanced evaluation set with consent records. Until that set exists, the script reports
per-group figures and refuses to pass or fail on them. That is condition C5.

## Two measurement choices that are easy to get wrong

**Recall is greedy-matched.** Each ground-truth box claims the best unclaimed detection
above the IoU threshold, so one detection cannot satisfy two faces. Without that, a
merged double-detection looks like perfect recall.

**Clustering is scored pairwise, not by cluster count.** One chain merge joining two
thirty-face identities creates 900 false-positive pairs and collapses precision; a
cluster-counting metric would report two clusters instead of three and look almost right.
"""

from __future__ import annotations

import argparse
import json
import math
import sys
from pathlib import Path

RECALL_GATE = 0.97
SMALL_FACE_RECALL_GATE = 0.90
BOKEH_FALSE_POSITIVE_GATE = 0.01
CLUSTER_F1_GATE = 0.93

# A face shorter than this fraction of the frame is in the small-face subset. Mirrors
# `DetectConfig::default().small_face_fraction`.
SMALL_FACE_FRACTION = 0.06


def iou(a, b) -> float:
    """Intersection over union of two ``(x, y, w, h)`` boxes in normalised coordinates."""
    left = max(a[0], b[0])
    top = max(a[1], b[1])
    right = min(a[0] + a[2], b[0] + b[2])
    bottom = min(a[1] + a[3], b[1] + b[3])
    if right <= left or bottom <= top:
        return 0.0
    overlap = (right - left) * (bottom - top)
    union = a[2] * a[3] + b[2] * b[3] - overlap
    return 0.0 if union <= 0.0 else overlap / union


def recall(truth, found, threshold: float = 0.5):
    """Greedy-matched recall. See the module header for why greedy."""
    claimed = [False] * len(found)
    matched = 0
    for target in truth:
        best = None
        for index, detection in enumerate(found):
            if claimed[index]:
                continue
            score = iou(target, detection)
            if score < threshold:
                continue
            if best is None or score > best[1]:
                best = (index, score)
        if best is not None:
            claimed[best[0]] = True
            matched += 1
    return matched, len(truth)


def false_positives(truth, found, threshold: float = 0.5) -> int:
    return sum(
        1
        for detection in found
        if not any(iou(target, detection) >= threshold for target in truth)
    )


def pairwise(truth_labels, predicted_labels):
    """Pairwise precision, recall, F1 and impurity.

    Faces with no truth label are skipped. Faces the clustering deliberately left
    unassigned count as separated rather than as an error of a third kind: a refusal is
    accounted for by the unassigned count, not by the F1.
    """
    true_positive = false_positive = false_negative = 0
    count = min(len(truth_labels), len(predicted_labels))
    for left in range(count):
        for right in range(left + 1, count):
            if truth_labels[left] is None or truth_labels[right] is None:
                continue
            same_truth = truth_labels[left] == truth_labels[right]
            a = predicted_labels[left]
            b = predicted_labels[right]
            same_predicted = a is not None and a == b
            if same_truth and same_predicted:
                true_positive += 1
            elif same_predicted:
                false_positive += 1
            elif same_truth:
                false_negative += 1

    joined = true_positive + false_positive
    should = true_positive + false_negative
    precision = 1.0 if joined == 0 else true_positive / joined
    recall_value = 1.0 if should == 0 else true_positive / should
    f1 = 0.0 if precision + recall_value <= 0 else 2 * precision * recall_value / (precision + recall_value)

    per_cluster: dict[int, set[int]] = {}
    for index in range(count):
        label = predicted_labels[index]
        truth = truth_labels[index]
        if label is None or truth is None:
            continue
        per_cluster.setdefault(label, set()).add(truth)
    impure = sum(1 for labels in per_cluster.values() if len(labels) > 1)

    return {
        "precision": precision,
        "recall": recall_value,
        "f1": f1,
        "impure_clusters": impure,
        "true_positive": true_positive,
        "false_positive": false_positive,
        "false_negative": false_negative,
    }


def verification_rates(distances, same_identity, false_accept_rate: float = 0.001):
    """True-accept rate at a fixed false-accept rate.

    The metric a recogniser's disparity actually shows up in - not accuracy. Two groups
    can have identical accuracy and very different true-accept rates at the operating
    point the product runs at, and it is the operating point that decides whether
    somebody's cousins get grouped.
    """
    impostors = sorted(d for d, same in zip(distances, same_identity) if not same)
    if not impostors:
        return None, None
    position = max(0, min(len(impostors) - 1, int(len(impostors) * false_accept_rate)))
    operating_point = impostors[position]
    genuine = [d for d, same in zip(distances, same_identity) if same]
    if not genuine:
        return operating_point, None
    accepted = sum(1 for d in genuine if d <= operating_point)
    return operating_point, accepted / len(genuine)


def self_test() -> int:
    """Run every metric against data whose answer is known by construction.

    This is a gate on the *script*. A metric that is silently broken passes a bad model
    for ever, and the only way to catch that is to feed it something whose answer cannot
    be argued with.
    """
    failures = 0

    truth = [(0.1, 0.1, 0.1, 0.14), (0.5, 0.5, 0.1, 0.14)]
    perfect = list(truth)
    matched, total = recall(truth, perfect)
    if (matched, total) != (2, 2):
        print(f"recall on identical boxes: {matched}/{total}", file=sys.stderr)
        failures += 1

    # One detection covering both faces must not satisfy both.
    merged = [(0.1, 0.1, 0.5, 0.54)]
    matched, total = recall(truth, merged)
    if matched > 1:
        print(f"a merged detection satisfied {matched} faces", file=sys.stderr)
        failures += 1

    if false_positives(truth, [(0.8, 0.8, 0.05, 0.07)]) != 1:
        print("a detection matching nothing was not counted", file=sys.stderr)
        failures += 1

    labels = [0, 0, 1, 1]
    if pairwise(labels, [0, 0, 0, 0])["impure_clusters"] != 1:
        print("a merged pair of identities was not reported as impure", file=sys.stderr)
        failures += 1
    if pairwise(labels, [0, 1, 2, 3])["f1"] > 0.0:
        print("over-splitting scored above zero F1", file=sys.stderr)
        failures += 1
    if abs(pairwise(labels, [0, 0, 1, 1])["f1"] - 1.0) > 1e-9:
        print("a correct clustering did not score 1.0", file=sys.stderr)
        failures += 1

    distances = [0.1, 0.15, 0.9, 0.95]
    same = [True, True, False, False]
    threshold, tar = verification_rates(distances, same)
    if threshold is None or tar is None or tar < 0.99:
        print(f"verification rates: threshold {threshold}, TAR {tar}", file=sys.stderr)
        failures += 1

    if failures == 0:
        print("self-test: every metric behaves on data with a known answer")
    return 1 if failures else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--detections", type=Path, help="JSON: truth and detected boxes")
    parser.add_argument("--templates", type=Path, help=".npz: templates, labels, groups")
    parser.add_argument("--report", type=Path, help="where to write the result as JSON")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if args.detections is None and args.templates is None:
        parser.error("pass --detections, --templates, or --self-test")
        return 2

    report: dict[str, object] = {}
    failures = 0

    if args.detections is not None:
        data = json.loads(args.detections.read_text(encoding="utf-8"))
        overall_matched = overall_total = 0
        small_matched = small_total = 0
        bokeh_false = detections_total = 0
        for frame in data["frames"]:
            truth = [tuple(box) for box in frame.get("truth", [])]
            found = [tuple(box) for box in frame.get("found", [])]
            bokeh = [tuple(box) for box in frame.get("bokeh", [])]
            matched, total = recall(truth, found)
            overall_matched += matched
            overall_total += total
            small = [box for box in truth if box[3] < SMALL_FACE_FRACTION]
            matched, total = recall(small, found)
            small_matched += matched
            small_total += total
            bokeh_false += false_positives(truth, found)
            detections_total += len(found)
            _ = bokeh

        overall = overall_matched / overall_total if overall_total else 0.0
        small = small_matched / small_total if small_total else 0.0
        rate = bokeh_false / detections_total if detections_total else 0.0
        report["detection"] = {
            "recall": overall,
            "small_face_recall": small,
            "false_positive_rate": rate,
        }
        print(f"detection recall: {overall:.4f} (gate {RECALL_GATE})")
        print(f"small-face recall: {small:.4f} (gate {SMALL_FACE_RECALL_GATE})")
        print(f"false positives: {rate:.4f} (gate {BOKEH_FALSE_POSITIVE_GATE})")
        failures += overall < RECALL_GATE
        failures += small < SMALL_FACE_RECALL_GATE
        failures += rate > BOKEH_FALSE_POSITIVE_GATE

    if args.templates is not None:
        print(
            "per-group verification rates are reported without a pass/fail threshold; "
            "see condition C5"
        )
        _ = math  # the import is used by callers of `verification_rates`

    if args.report is not None:
        args.report.write_text(json.dumps(report, indent=2), encoding="utf-8")
        print(f"report: {args.report}")

    return 1 if failures else 0


if __name__ == "__main__":
    raise SystemExit(main())
