#!/usr/bin/env python3
"""Merge the reduced SD3 class's manifest entry (sd3-tiny.json, written by `write_the_sd3_class`) into drill-classes.json
(written by `write_the_drill_classes`), so audit-gen/dg.sh's class_field/class_file find it. Idempotent."""
import json, os, sys

d = sys.argv[1]
path = os.path.join(d, "drill-classes.json")
m = json.load(open(path))
entry = json.load(open(os.path.join(d, "sd3-tiny.json")))
m["classes"] = [c for c in m["classes"] if c["name"] != "sd3-tiny"] + [entry]
json.dump(m, open(path, "w"), indent=2)
print("drill-classes.json:", ", ".join(c["name"] for c in m["classes"]))
