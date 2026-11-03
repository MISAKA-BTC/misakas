#!/usr/bin/env python3
"""Run production GC source and snapshot authorization with std-only Rust.

Hash64 is replaced by u64 in the filesystem fixture only; this does not test hash
encoding, the RocksDB cache, or complete Node/session integration.
"""
from pathlib import Path
import hashlib
import json
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
panel = (root / "kaspad/src/palw_panel.rs").read_text()
tests = (root / "kaspad/src/palw_panel/seat_r_tests.rs").read_text()

def function(text, name):
    start = text.index("fn " + name + "(")
    body = text.index("{", start)
    depth, end = 1, body + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]

production = function(panel, "prune_foreign_retention_v1")
print("production_gc_sha256=" + hashlib.sha256(production.encode()).hexdigest(), flush=True)
source = "use std::collections::HashSet;\ntype Hash64 = u64;\nfn h(n:u64)->Hash64{n}\n" + production
for name in ["the_foreign_retention_keeps_a_live_claims_material_past_the_age_bound",
             "foreign_gc_rereads_accepted_retention_after_restart_and_branch_change",
             "foreign_gc_read_failures_keep_material_but_healthy_absence_does_not_pin_forever"]:
    source += "\n#[test]\n" + function(tests, name)
with tempfile.TemporaryDirectory(prefix="misaka-foreign-gc-safety-") as work:
    work = Path(work)
    src, binary = work / "gc.rs", work / "gc"
    src.write_text(source)
    subprocess.run(["rustc", "--edition", "2024", "--test", str(src), "-o", str(binary)], check=True)
    subprocess.run([str(binary), "--nocapture"], check=True)
    helper = root / "consensus/src/consensus/palw_material_hold.rs"
    print("snapshot_helper_sha256=" + hashlib.sha256(helper.read_bytes()).hexdigest(), flush=True)
    src.write_text("#[path=" + json.dumps(str(helper)) + "] mod snapshot_authority;\n")
    subprocess.run(["rustc", "--edition", "2024", "--test", str(src), "-o", str(binary)], check=True)
    subprocess.run([str(binary), "--nocapture"], check=True)
