#!/usr/bin/env python3
"""ADR-0125 semantic amendment: PALW semantic consumers must not interpret raw GHOSTDAG reds.

GHOSTDAG/storage, pruning/IBD, reachability, merge-depth and legacy RPC conversion
retain raw access. Unit-test fixtures may inspect it to pin the existing format.

The scan covers nested production modules (`palw_query/`, `palw_state_v2/`, …), not only
`palw_*.rs` sitting directly in the three roots. A `#[cfg(test)]` item is omitted, and
production code that follows that item is still scanned.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[2]
CONSUMERS = [
    "consensus/src/processes/coinbase.rs",
    "consensus/src/pipeline/header_processor/post_pow_validation.rs",
    "consensus/src/pipeline/virtual_processor/processor.rs",
    "kaspad/src/palw_lane_watch.rs",
]
RAW_RED = re.compile(r"\.mergeset_reds\b|\bget_mergeset_reds\s*\(")
# RFC-0008 v2 appends covered blocks to a local clone so later consumers see them as red.
# That writer is not a semantic reading of the stored GHOSTDAG set. A read, including
# `get_mergeset_reds(`, stays a violation.
AUGMENT_WRITE = re.compile(r"BlockHashes::make_mut\(\s*&mut \w+\.mergeset_reds\s*\)|&mut \w+\.mergeset_reds\b")


def semantic_paths(root: Path) -> set[Path]:
    paths = {root / name for name in CONSUMERS}
    for directory in ("consensus/core/src", "kaspad/src", "rpc/service/src"):
        base = root / directory
        if not base.is_dir():
            continue
        paths.update(base.rglob("palw_*.rs"))
        for path in base.rglob("*.rs"):
            parents = path.relative_to(base).parts[:-1]
            if any(part.startswith("palw_") for part in parents):
                paths.add(path)
    return {path for path in paths if path.is_file()}


def production_lines(source: str) -> list[tuple[int, str]]:
    """Drop `#[cfg(test)]` items. Keep every production line that follows them."""
    lines = source.splitlines()
    index = 0
    kept: list[tuple[int, str]] = []
    while index < len(lines):
        stripped = lines[index].strip()
        if stripped.startswith("#[cfg(test)]"):
            index += 1
            while index < len(lines) and lines[index].strip().startswith("#["):
                index += 1
            if index >= len(lines):
                break
            depth = lines[index].count("{") - lines[index].count("}")
            index += 1
            while index < len(lines) and depth > 0:
                depth += lines[index].count("{") - lines[index].count("}")
                index += 1
            continue
        kept.append((index + 1, lines[index]))
        index += 1
    return kept


def audit(root: Path) -> list[str]:
    violations = []
    paths = semantic_paths(root)
    for path in sorted(paths):
        text = path.read_text()
        lines = text.splitlines()
        for number, line in production_lines(text):
            if not RAW_RED.search(line):
                continue
            previous = lines[number - 2] if number >= 2 else ""
            if AUGMENT_WRITE.search(line) and ("BlockHashes::make_mut" in line or "BlockHashes::make_mut" in previous):
                continue
            violations.append(f"{path.relative_to(root)}:{number}: {line.strip()}")
    return violations


def main() -> int:
    root = ROOT
    if len(sys.argv) == 3 and sys.argv[1] == "--root":
        root = Path(sys.argv[2])
    violations = audit(root)
    if violations:
        print("PALW raw-red access bypasses ADR-0125 semantic amendment classification:", file=sys.stderr)
        print("\n".join(violations), file=sys.stderr)
        return 1
    print(f"ADR-0125 semantic amendment raw-red access audit passed ({len(semantic_paths(root))} semantic consumer files).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
