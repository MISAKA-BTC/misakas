#!/usr/bin/env python3
"""Execute the actual foreign-GC function/tests without compiling the Node.

Only Hash64 is substituted by u64: this checks filesystem lifetime and oracle
use, not hash parsing/consensus/session integration or the complete Node build.
"""
from pathlib import Path
import hashlib
import subprocess
import tempfile

root = Path(__file__).resolve().parents[4]
panel = (root / "kaspad/src/palw_panel.rs").read_text()
tests = (root / "kaspad/src/palw_panel/seat_r_tests.rs").read_text()


def function(text, name):
    start = text.index("fn " + name + "(")
    body = text.index("{", start)
    depth = 1
    end = body + 1
    while depth:
        depth += (text[end] == "{") - (text[end] == "}")
        end += 1
    return text[start:end]


production = function(panel, "prune_foreign_retention_v1")
print("production_sha256=" + hashlib.sha256(production.encode()).hexdigest(), flush=True)
source = "use std::collections::HashSet;\ntype Hash64 = u64;\nfn h(n: u64) -> Hash64 { n }\n" + production
for test in [
    "the_foreign_retention_keeps_a_live_claims_material_past_the_age_bound",
    "foreign_gc_rereads_accepted_retention_after_restart_and_branch_change",
]:
    source += "\n#[test]\n" + function(tests, test)
with tempfile.TemporaryDirectory(prefix="misaka-foreign-gc-") as work:
    work = Path(work)
    src = work / "foreign_gc.rs"
    binary = work / "foreign_gc"
    src.write_text(source)
    subprocess.run(["rustc", "--edition", "2024", "--test", str(src), "-o", str(binary)], check=True)
    subprocess.run([str(binary), "--nocapture"], check=True)
