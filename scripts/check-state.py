#!/usr/bin/env python3
"""Reduce a GitHub check-runs or combined-status payload to one word.

Reads the JSON on stdin and prints exactly one of:

    none      nothing has reported on this commit
    pending   at least one check is queued or running
    failing   at least one check finished badly
    passing   everything that reported, reported well

The landing script refuses to merge on `failing` and is content with `pending`,
so the distinction between the two is the whole point of this file. Parsing it
here rather than with a regular expression in the shell keeps the answer
independent of how the API happens to whitespace its JSON.
"""

import json
import sys

BAD = {"failure", "timed_out", "action_required", "startup_failure", "stale"}
RUNNING = {"queued", "in_progress", "waiting", "pending", "requested"}


def from_check_runs(doc, ignored):
    runs = doc.get("check_runs")
    if runs is None:
        return None
    runs = [r for r in runs if (r.get("name") or "") not in ignored]
    if not runs:
        return "none"
    states = []
    for run in runs:
        status = (run.get("status") or "").lower()
        conclusion = (run.get("conclusion") or "").lower()
        if status in RUNNING or not conclusion:
            states.append("pending")
        elif conclusion in BAD:
            states.append("failing")
        else:
            states.append("passing")
    if "failing" in states:
        return "failing"
    if "pending" in states:
        return "pending"
    return "passing"


def from_combined_status(doc, ignored):
    state = (doc.get("state") or "").lower()
    if not state:
        return None
    statuses = [s for s in doc.get("statuses") or [] if (s.get("context") or "") not in ignored]
    if not statuses:
        return "none"
    worst = {(s.get("state") or "").lower() for s in statuses}
    if worst & {"failure", "error"}:
        return "failing"
    if "pending" in worst:
        return "pending"
    return "passing"
def main() -> int:
    # Every argument is the name of a check whose verdict is not this landing's
    # business - a job that is known red for a reason recorded elsewhere. Naming
    # one is a much narrower statement than --force-merge, which waves through
    # every check at once including the ones nobody meant to excuse.
    args = sys.argv[1:]
    list_failures = "--list-failures" in args
    ignored = {a for a in args if not a.startswith("--")}
    try:
        doc = json.load(sys.stdin)
    except ValueError:
        print("unknown")
        return 0
    if not isinstance(doc, dict):
        print("unknown")
        return 0

    if list_failures:
        for run in doc.get("check_runs") or []:
            name = run.get("name") or "?"
            if name in ignored:
                continue
            if (run.get("conclusion") or "").lower() in BAD:
                print(name)
        for status in doc.get("statuses") or []:
            name = status.get("context") or "?"
            if name in ignored:
                continue
            if (status.get("state") or "").lower() in ("failure", "error"):
                print(name)
        return 0

    answer = from_check_runs(doc, ignored)
    if answer is None:
        answer = from_combined_status(doc, ignored)
    print(answer or "unknown")
    return 0


if __name__ == "__main__":
    sys.exit(main())
