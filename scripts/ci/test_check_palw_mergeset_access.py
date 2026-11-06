#!/usr/bin/env python3
"""The mergeset guard must fail on a planted production use, including one the old scan missed."""
import subprocess
import sys
import tempfile
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "check-palw-mergeset-access.py"
PLANTED = 2


def write(root: Path, relative: str, source: str) -> None:
    path = root / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(source)


def run(root: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run([sys.executable, str(SCRIPT), "--root", str(root)], capture_output=True, text=True)


def expect_fail(root: Path, needle: str) -> None:
    result = run(root)
    if result.returncode == 0:
        raise SystemExit(f"planted source was accepted:\n{result.stdout}")
    if needle not in result.stderr:
        raise SystemExit(f"refusal did not name {needle}:\n{result.stderr}")


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        write(
            root,
            "rpc/service/src/service/palw_query/mod.rs",
            "fn production() {\n    let _ = header.mergeset_reds;\n}\n",
        )
        expect_fail(root, "rpc/service/src/service/palw_query/mod.rs:2")
        write(root, "rpc/service/src/service/palw_query/mod.rs", "fn production() {}\n")
        write(
            root,
            "consensus/core/src/palw_after_test.rs",
            "#[cfg(test)]\nfn fixture() {\n    let _ = header.mergeset_reds;\n}\n"
            "fn production() {\n    get_mergeset_reds(&header);\n}\n",
        )
        expect_fail(root, "consensus/core/src/palw_after_test.rs:6")
        write(
            root,
            "consensus/core/src/palw_after_test.rs",
            "#[cfg(test)]\nfn fixture() {\n    let _ = header.mergeset_reds;\n}\nfn production() {}\n",
        )
        clean = run(root)
        if clean.returncode != 0:
            raise SystemExit(clean.stderr)
    repo = subprocess.run([sys.executable, str(SCRIPT)], capture_output=True, text=True)
    if repo.returncode != 0:
        sys.stderr.write(repo.stderr)
        return repo.returncode
    print(f"mergeset guard refused {PLANTED} planted productions and accepted the repository")
    print(repo.stdout.strip())
    return 0


if __name__ == "__main__":
    sys.exit(main())
