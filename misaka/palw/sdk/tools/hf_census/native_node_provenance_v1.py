"""Readonly provenance for explicitly declared local auditor processes.

Only known PIDs, binaries, database directories and RPC ports are inspected. No
argv/environment dump and no process launch. Separate local processes/databases
establish local replay separation; they do NOT prove independent controllers.
"""
from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
from urllib.parse import urlparse
import acceptance_cohort as ac


def validate_specs(specs: list) -> None:
    pids, databases, endpoints = set(), set(), set()
    for owner in specs:
        p = owner.get("node_process")
        if (not isinstance(p, dict) or set(p) != {"pid", "binary", "binary_sha256", "database_directory", "rpc"}
                or type(p["pid"]) is not int or p["pid"] <= 1
                or not isinstance(p["binary"], str) or not Path(p["binary"]).is_absolute()
                or not isinstance(p["binary_sha256"], str) or not re.fullmatch("[0-9a-f]{64}", p["binary_sha256"])
                or not isinstance(p["database_directory"], str) or not Path(p["database_directory"]).is_absolute()
                or p["database_directory"] != owner.get("node_database") or not isinstance(p["rpc"], str)):
            raise ValueError("frozen local auditor PID/pinned binary/actual database directory required")
        endpoint = urlparse(p["rpc"])
        if endpoint.hostname not in {"127.0.0.1", "::1", "localhost"} or not endpoint.port or endpoint.username or endpoint.password:
            raise ValueError("receipt local replay provenance requires an explicit loopback RPC endpoint")
        argv = owner.get("argv", [])
        if argv.count("--rpc") != 1 or argv.index("--rpc") + 1 >= len(argv) or argv[argv.index("--rpc") + 1] != p["rpc"]:
            raise ValueError("auditor fetch must use its observed local process RPC endpoint")
        pids.add(p["pid"]); databases.add(str(Path(p["database_directory"]).resolve())); endpoints.add(endpoint.port)
    if len(specs) != 2 or len(pids) != 2 or len(databases) != 2 or len(endpoints) != 2:
        raise ValueError("two actual local PIDs/databases/RPC listeners required")
    paths = [Path(p).resolve() for p in databases]
    if paths[0].is_relative_to(paths[1]) or paths[1].is_relative_to(paths[0]):
        raise ValueError("auditor database directories must be separate, not aliases or nested labels")


def _command(argv: list[str]) -> str:
    p = subprocess.run(argv, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                       timeout=15, check=False)
    if p.returncode or len(p.stdout) > 16 << 20:
        raise ValueError("known local node process metadata unavailable")
    return p.stdout.decode("utf-8", errors="strict")


def inspect(owner: dict) -> dict:
    p = owner["node_process"]; pid = p["pid"]
    binary, database = Path(p["binary"]).resolve(strict=True), Path(p["database_directory"]).resolve(strict=True)
    if not binary.is_file() or not database.is_dir() or ac.digest(binary) != p["binary_sha256"]:
        raise ValueError("actual node binary/database differs from original pins")
    binary_stat, db_stat = binary.stat(), database.stat()
    endpoint = urlparse(p["rpc"])
    port = endpoint.port
    files, listener, actual_executable = [], False, False
    system = platform.system()
    if system == "Linux":
        process = Path("/proc") / str(pid)
        exe_stat = (process / "exe").stat()
        actual_executable = (exe_stat.st_dev, exe_stat.st_ino) == (binary_stat.st_dev, binary_stat.st_ino)
        # Field22 is process start tick; the comm field may contain spaces.
        stat = (process / "stat").read_text()
        start = stat[stat.rfind(")")+2:].split()[19]
        sockets = set()
        descriptors = list((process / "fd").iterdir())
        if len(descriptors) > 65536:
            raise ValueError("known node descriptor metadata cap")
        for fd in descriptors:
            try:
                name = os.readlink(fd)
            except FileNotFoundError:
                continue
            if name.startswith("socket:["):
                sockets.add(name[8:-1])
            elif name.startswith("/"):
                candidate = Path(name).resolve()
                if candidate.is_relative_to(database) and candidate.is_file():
                    s = candidate.stat(); files.append((str(candidate.relative_to(database)), s.st_dev, s.st_ino))
        for family in ("tcp", "tcp6"):
            table = process / "net" / family
            if table.is_file():
                for line in table.read_text().splitlines()[1:]:
                    columns = line.split()
                    if len(columns) > 9 and columns[3] == "0A" and columns[9] in sockets and int(columns[1].split(":")[1], 16) == port:
                        listener = True
    elif system == "Darwin":
        lsof, ps = shutil.which("lsof"), shutil.which("ps")
        if not lsof or not ps:
            raise ValueError("known local node provenance requires lsof and ps on macOS")
        start = _command([ps, "-p", str(pid), "-o", "lstart="]).strip()
        text = _command([lsof, "-a", "-p", str(pid), "-d", "txt", "-Fn"])
        for line in text.splitlines():
            if line.startswith("n/"):
                candidate = Path(line[1:])
                if candidate.is_file():
                    s = candidate.stat()
                    actual_executable |= (s.st_dev, s.st_ino) == (binary_stat.st_dev, binary_stat.st_ino)
        for line in _command([lsof, "-a", "-p", str(pid), "-Fn"]).splitlines():
            if line.startswith("n/"):
                candidate = Path(line[1:]).resolve()
                if candidate.is_relative_to(database) and candidate.is_file():
                    s = candidate.stat(); files.append((str(candidate.relative_to(database)), s.st_dev, s.st_ino))
        selected = _command([lsof, "-a", "-p", str(pid), "-iTCP:" + str(port), "-sTCP:LISTEN", "-Fn"])
        listener = f"p{pid}" in selected.splitlines() and any(line.startswith("n") and line.rsplit(":", 1)[-1] == str(port) for line in selected.splitlines())
    else:
        raise ValueError("local node provenance platform unsupported; keep measurement NOT_RUN")
    if (not start or not actual_executable or not listener
            or not any(Path(name).name == "LOCK" for name, _, _ in files)
            or not any(Path(name).name.startswith("MANIFEST-") for name, _, _ in files)):
        raise ValueError("known process does not demonstrably own pinned executable, RocksDB and RPC listener")
    return {"pid": pid, "process_start": start, "binary_sha256": p["binary_sha256"],
        "binary_device": binary_stat.st_dev, "binary_inode": binary_stat.st_ino,
        "database_directory": str(database), "database_device": db_stat.st_dev, "database_inode": db_stat.st_ino,
        "opened_database_files_root": hashlib.sha256(json.dumps(sorted(set(files)), separators=(",", ":")).encode()).hexdigest(),
        "rpc": p["rpc"], "operator_id": owner["operator_id"], "local_replay_separation_only": True,
        "independent_controllers_proven": False}


def collect(specs: list, output: Path) -> dict:
    validate_specs(specs)
    if output.exists():
        raise ValueError("fresh local node provenance output required")
    observations = [inspect(s) for s in specs]
    if len({(v["database_device"], v["database_inode"]) for v in observations}) != 2:
        raise ValueError("auditor database labels resolve to the same actual directory")
    doc = {"schema": "misaka.rfc02.observed-local-node-provenance.v1", "nodes": observations,
           "local_qualified_validation_only": True, "public_operator_independence_proven": False}
    ac.write(output, doc)
    return doc


def same_processes(before: dict, after: dict) -> bool:
    stable = ("pid", "process_start", "binary_sha256", "binary_device", "binary_inode", "database_directory",
              "database_device", "database_inode", "rpc", "operator_id")
    return len(before.get("nodes", [])) == len(after.get("nodes", [])) == 2 and all(
        all(a.get(k) == b.get(k) for k in stable) for a, b in zip(before["nodes"], after["nodes"]))
