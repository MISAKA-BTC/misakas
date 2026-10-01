#!/usr/bin/env python3
"""audit-tir/classrows.py — one class's row on one drill node (read-only JSON-wRPC, getPalwClasses).

  classrows.py <json port> <class id> <label>

Prints `ANSWERS` when the node answers, and — if its class table lists the class — one line
`<label> <registeredDaa> <canonicalLeaves> <artifactRoot> <status>`. Prints nothing (exit 0) when the node does not
answer, so a stopped node is not mistaken for one that refused the class. The int-10 flag-day step script
(scripts/misaka-palw-int10-flagday.sh, `classes`) asks every running new node and judges the rows together."""
import os, sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from rpc import call, pick


def main():
    port, cid, label = int(sys.argv[1]), sys.argv[2], sys.argv[3]
    try:
        r = call(port, "getPalwClasses", {})
    except Exception:
        return 0
    print("ANSWERS")
    for c in pick(r, "classes", default=[]) or []:
        if pick(c, "classId") == cid:
            print(label, pick(c, "registeredDaa"), pick(c, "canonicalLeaves"), pick(c, "artifactRoot"), pick(c, "status"))
    return 0


if __name__ == "__main__":
    sys.exit(main())
