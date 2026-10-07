#!/usr/bin/env python3
"""ADR-0125 semantic amendment: PALW semantic consumers must not interpret raw GHOSTDAG reds.

GHOSTDAG/storage, pruning/IBD, reachability, merge-depth and legacy RPC conversion
retain raw access. Unit-test fixtures may inspect it to pin the existing format.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
CONSUMERS = [
    "consensus/src/processes/coinbase.rs",
    "consensus/src/pipeline/header_processor/post_pow_validation.rs",
    "consensus/src/pipeline/virtual_processor/processor.rs",
    "kaspad/src/palw_lane_watch.rs",
]
paths = {ROOT / name for name in CONSUMERS}
for directory in ("consensus/core/src", "kaspad/src", "rpc/service/src"):
    paths.update((ROOT / directory).glob("palw_*.rs"))
violations = []
for path in sorted(paths):
    source = path.read_text().split("#[cfg(test)]", 1)[0]
    for number, line in enumerate(source.splitlines(), 1):
        if re.search(r"\.mergeset_reds\b|\bget_mergeset_reds\s*\(", line):
            violations.append(f"{path.relative_to(ROOT)}:{number}: {line.strip()}")
if violations:
    print("PALW raw-red access bypasses ADR-0125 semantic amendment classification:", file=sys.stderr)
    print("\n".join(violations), file=sys.stderr)
    sys.exit(1)
print(f"ADR-0125 semantic amendment raw-red access audit passed ({len(paths)} semantic consumer files).")
