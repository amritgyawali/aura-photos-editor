#!/usr/bin/env python3
"""Read one value out of a JSON document on stdin.

Used by the phase tooling so that the branch and landing scripts do not depend on
`jq`, which is not installed on the Windows development machine. The path is
dotted; a numeric component indexes a list. A missing path prints nothing and
exits 1, so a caller can tell "absent" from "empty string".

    printf '{"number":7}' | python scripts/json-get.py number
"""

import json
import sys


def main() -> int:
    if len(sys.argv) < 2:
        sys.stderr.write("usage: json-get.py <dotted.path> [default]\n")
        return 2
    try:
        doc = json.load(sys.stdin)
    except ValueError as exc:
        sys.stderr.write(f"json-get: not JSON: {exc}\n")
        return 2

    node = doc
    for part in sys.argv[1].split("."):
        if isinstance(node, list):
            try:
                node = node[int(part)]
            except (ValueError, IndexError):
                node = None
        elif isinstance(node, dict):
            node = node.get(part)
        else:
            node = None
        if node is None:
            break

    if node is None:
        if len(sys.argv) > 2:
            print(sys.argv[2])
            return 0
        return 1

    if isinstance(node, (dict, list)):
        print(json.dumps(node))
    elif isinstance(node, bool):
        print("true" if node else "false")
    else:
        print(node)
    return 0


if __name__ == "__main__":
    sys.exit(main())
