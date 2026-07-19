#!/usr/bin/env python3
from __future__ import annotations

import argparse
import html
import json
import os
import re
import shutil
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse


class JobState:
    def __init__(self) -> None:
        self.lock = threading.Lock()
        self.running = False
        self.name = ""
        self.started_at = 0.0
        self.finished_at = 0.0
        self.exit_code: int | None = None
        self.logs: list[str] = []

    def snapshot(self) -> dict:
        with self.lock:
            return {
                "running": self.running,
                "name": self.name,
                "startedAt": self.started_at,
                "finishedAt": self.finished_at,
                "exitCode": self.exit_code,
                "logs": "".join(self.logs[-800:]),
            }

    def start(self, name: str) -> bool:
        with self.lock:
            if self.running:
                return False
            self.running = True
            self.name = name
            self.started_at = time.time()
            self.finished_at = 0.0
            self.exit_code = None
            self.logs = []
            return True

    def append(self, text: str) -> None:
        with self.lock:
            self.logs.append(text)
            if len(self.logs) > 1200:
                self.logs = self.logs[-800:]

    def finish(self, code: int) -> None:
        with self.lock:
            self.running = False
            self.exit_code = code
            self.finished_at = time.time()


COMMANDS: dict[str, list[str]] = {
    "prepare": ["prepare"],
    "auto-node": ["auto-node"],
    "start-node": ["start-node"],
    "status": ["status"],
    "doctor": ["doctor"],
    "collect-diagnostic-log": ["collect-diagnostic-log"],
    "collect-support-log": ["collect-support-log"],
    "wait-sync": ["wait-sync"],
    "auto-validator": ["auto-validator"],
    "keygen": ["keygen"],
    "miner-start": ["miner-start"],
    "miner-stop": ["miner-stop"],
    "balance": ["balance"],
    "bond": ["bond", "10MSK"],
    "validator-start": ["validator-start"],
    "validator-stop": ["validator-stop"],
    "stop-all": ["stop-all"],
    "logs": ["logs"],
}

ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")


def clean_output(text: str) -> str:
    return ANSI_RE.sub("", text)


def desktop_home() -> Path:
    return Path(os.environ.get("MISAKA_DESKTOP_HOME", str(Path.home() / ".misaka-desktop-node"))).expanduser()


def read_pid(path: Path) -> int | None:
    try:
        value = int(path.read_text().strip())
    except Exception:
        return None
    try:
        os.kill(value, 0)
    except OSError:
        return None
    expected = {
        "kaspad.pid": "kaspad",
        "misaminer.pid": "misaminer",
        "validator.pid": "kaspa-pq-validator",
    }.get(path.name)
    if expected:
        command = ""
        try:
            command = Path(f"/proc/{value}/cmdline").read_bytes().replace(b"\0", b" ").decode(errors="replace")
        except OSError:
            try:
                command = subprocess.run(
                    ["ps", "-p", str(value), "-o", "command="],
                    check=False,
                    text=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.DEVNULL,
                ).stdout
            except OSError:
                return None
        if expected not in command:
            return None
    return value


def read_state() -> dict[str, str]:
    state_path = desktop_home() / "state" / "state.env"
    data: dict[str, str] = {}
    if not state_path.exists():
        return data
    for line in state_path.read_text(errors="replace").splitlines():
        if "=" not in line or line.strip().startswith("#"):
            continue
        key, value = line.split("=", 1)
        data[key.strip()] = value.strip().strip("'").strip('"')
    return data


def tool_path(name: str) -> str | None:
    path = shutil.which(name)
    return path


def binary_info(name: str) -> dict:
    path = desktop_home() / "bin" / name
    return {"name": name, "ok": path.exists() and os.access(path, os.X_OK), "path": str(path) if path.exists() else None}


def job_json(job: JobState) -> dict:
    snap = job.snapshot()
    complete = bool(snap["name"] == "prepare" and snap["exitCode"] == 0)
    failed = bool(snap["name"] == "prepare" and snap["exitCode"] not in (None, 0))
    return {
        "name": snap["name"] or "prepare-local",
        "running": snap["running"],
        "complete": complete,
        "failed": failed,
        "pid": None,
        "logPath": "local Web UI memory log",
        "logs": snap["logs"],
    }


def bootstrap_status(share_dir: Path, job: JobState) -> dict:
    home = desktop_home()
    repo_dir = Path(os.environ.get("MISAKA_REPO_DIR", str(home / "misakas"))).expanduser()
    binaries = [binary_info(name) for name in ["kaspad", "misaka", "kaspa-pq-validator", "misaminer"]]
    tools = [{"name": name, "ok": tool_path(name) is not None, "path": tool_path(name)} for name in ["git", "curl", "python3"]]
    cargo = tool_path("cargo")
    rustc = tool_path("rustc")
    ready = all(item["ok"] for item in binaries)
    return {
        "ok": True,
        "bootstrap": {
            "ready": ready,
            "sourceExists": (repo_dir / "Cargo.toml").exists(),
            "repoDir": str(repo_dir),
            "repoUrl": os.environ.get("MISAKA_REPO_URL", "https://github.com/MISAKA-BTC/misakas.git"),
            "cargo": cargo,
            "rustc": rustc,
            "tools": tools,
            "binaries": binaries,
        },
        "job": job_json(job),
    }


def public_ip_info(confirmed: bool = True) -> dict:
    return {
        "ok": True,
        "publicIpInfo": {
            "publicIp": "127.0.0.1",
            "savedIp": "127.0.0.1",
            "detectedIp": "127.0.0.1",
            "browserHost": "127.0.0.1",
            "source": "local",
            "confirmed": confirmed,
        },
    }


def parse_balance(output: str) -> tuple[int | None, str | None]:
    for line in output.splitlines():
        parts = line.split("\t")
        if len(parts) >= 3 and parts[1].isdigit():
            return int(parts[1]), parts[2].strip()
    match = re.search(r"\b([0-9]+)\s+sompi\b", output)
    if match:
        return int(match.group(1)), None
    return None, None


def current_daa_value() -> int | None:
    binary = desktop_home() / "bin" / "misaka"
    if not binary.exists():
        return None
    try:
        proc = subprocess.run(
            [
                str(binary),
                "--network",
                os.environ.get("MISAKA_NETWORK", "testnet-10"),
                "--rpc",
                f"127.0.0.1:{os.environ.get('MISAKA_WRPC_BORSH_PORT', '27210')}",
                "node",
                "doctor",
            ],
            env={**os.environ, "HOME": str(desktop_home() / "home"), "NO_COLOR": "1"},
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=20,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    match = re.search(r"Virtual DAA score\s+([0-9]+)", clean_output(proc.stdout))
    return int(match.group(1)) if match else None


def first_mined_daa_value() -> int | None:
    log_path = desktop_home() / "logs" / "misaminer.log"
    try:
        with log_path.open(errors="replace") as log_file:
            for line in log_file:
                match = re.search(r"mined block\s+#\d+.*?daa_score=(\d+)", line, re.IGNORECASE)
                if match:
                    return int(match.group(1))
    except OSError:
        return None
    return None


def maturity_value(current_daa: int | None) -> dict:
    state = read_state()
    start_raw = state.get("MINER_START_DAA")
    start_daa = int(start_raw) if start_raw and start_raw.isdigit() else None
    basis = "minerStartDaa"
    if start_daa is None:
        start_daa = first_mined_daa_value()
        if start_daa is not None:
            basis = "firstMinedDaa"
    try:
        required = max(0, int(os.environ.get("MISAKA_COINBASE_MATURITY_BLOCKS", "1000")))
    except ValueError:
        required = 1000
    elapsed = max(0, current_daa - start_daa) if current_daa is not None and start_daa is not None else None
    remaining = max(0, required - elapsed) if elapsed is not None else None
    percent = min(100, elapsed * 100 // required) if elapsed is not None and required > 0 else None
    return {
        "approx": True,
        "basis": basis,
        "coinbaseMaturityBlocks": required,
        "minerStartDaa": start_daa,
        "currentDaa": current_daa,
        "elapsedBlocks": elapsed,
        "remainingBlocks": remaining,
        "percent": percent,
        "readyByStartEstimate": bool(elapsed is not None and elapsed >= required),
    }


def local_node_service_state(status: dict) -> str:
    home = desktop_home()
    pid_path = home / "run" / "kaspad.pid"
    if status["kaspadRunning"] or read_pid(pid_path):
        return "active"
    if pid_path.exists():
        return "failed"
    state = read_state()
    if state.get("NODE_STARTED_ONCE") == "1" or (home / "logs" / "kaspad.log").exists():
        return "inactive"
    return "not configured"


def status_value(share_dir: Path) -> dict:
    code, output = run_script(share_dir, ["status"], timeout=45)
    status = parse_status(output)
    node_state = local_node_service_state(status)
    return {
        "ok": bool(status["synced"]),
        "network": "testnet-10",
        "publicIp": "127.0.0.1",
        "node": {
            "error": None if status["wrpcListening"] else "node is not reachable",
            "network": "testnet-10",
            "reachable": bool(status["wrpcListening"]),
            "service": "misaka-local-kaspad",
            "serviceState": node_state,
            "synced": bool(status["synced"]),
            "utxoIndex": bool(status["utxoIndex"]),
            "version": "1.1.0",
            "virtualDaaScore": status["daa"],
        },
        "p2p": {"listening": bool(status["p2pListening"]), "port": 26211},
        "seeder": {"service": "not used locally", "serviceState": "not configured"},
        "validator": {"service": "misaka-local-validator", "serviceState": "active" if status["validatorRunning"] else "not configured"},
        "miner": miner_status_value(status["daa"]),
        "raw": output,
        "exitCode": code,
    }


def normalize_bond_outpoint(value: str | None) -> str | None:
    candidate = "".join((value or "").split())
    txid_pattern = r"(?:[0-9a-fA-F]{64}|[0-9a-fA-F]{128})"
    if re.fullmatch(rf"{txid_pattern}:[0-9]+", candidate):
        return candidate
    if re.fullmatch(txid_pattern, candidate):
        return f"{candidate}:0"
    return None


def validator_status_value(share_dir: Path) -> dict:
    home = desktop_home()
    state = read_state()
    status = status_value(share_dir)
    key_path = home / "validator" / "validator.seed"
    validator_pid = read_pid(home / "run" / "validator.pid")
    funding = state.get("FUNDING_ADDRESS")
    bond = normalize_bond_outpoint(state.get("BOND_OUTPOINT"))
    value = {
        "ok": bool(validator_pid and bond),
        "validator": {
            "binary": str(home / "bin" / "kaspa-pq-validator") if (home / "bin" / "kaspa-pq-validator").exists() else None,
            "service": "misaka-local-validator",
            "serviceState": "active" if validator_pid else "not configured",
            "keyExists": key_path.exists(),
            "keyPath": str(key_path),
            "signedEpochDb": str(home / "validator" / "validator.state"),
            "validatorId": None,
            "fundingAddress": funding,
            "bondOutpoint": bond,
            "nodeReachable": bool(status["node"]["reachable"]),
            "nodeSynced": bool(status["node"]["synced"]),
        },
    }
    return value


def miner_status_value(current_daa: int | None = None) -> dict:
    home = desktop_home()
    state = read_state()
    miner_pid = read_pid(home / "run" / "misaminer.pid")
    threads = state.get("MINER_THREADS") or os.environ.get("MISAKA_MINER_THREADS", "1")
    try:
        thread_count = int(threads)
    except ValueError:
        thread_count = 1
    return {
        "service": "misaka-local-miner",
        "serviceState": "active" if miner_pid else "not configured",
        "binary": str(home / "bin" / "misaminer") if (home / "bin" / "misaminer").exists() else None,
        "grpc": "127.0.0.1:26210",
        "threads": thread_count,
        "miningAddress": state.get("FUNDING_ADDRESS"),
        "fundingAddress": state.get("FUNDING_ADDRESS"),
        "maturity": maturity_value(current_daa),
    }


def diagnostics_value() -> dict:
    cpus = os.cpu_count() or 1
    max_threads = max(1, min(16, cpus))
    recommended = max(1, min(max_threads, max(1, cpus // 4)))
    return {
        "ok": True,
        "diagnostics": {
            "logicalCpus": cpus,
            "load1m": os.getloadavg()[0] if hasattr(os, "getloadavg") else None,
            "memoryAvailableGiB": None,
            "recommendedThreads": recommended,
            "maxThreads": max_threads,
            "options": list(range(1, max_threads + 1)),
        },
    }


def local_ui_html(share_dir: Path, name: str, token: str) -> str:
    ui_path = share_dir / "ui" / name
    text = ui_path.read_text(encoding="utf-8")
    text = text.replace("__SETUP_TOKEN__", token)
    if name == "learn.html":
        return text
    replacements = {
        "VPS public IP": "Local address",
        "VPS公開IP": "ローカルアドレス",
        "VPS CPU": "PC CPU",
        "VPS": "PC",
        "public IP": "local address",
        "Public IP": "Local address",
        "node local address": "local node address",
        "公開IP": "ローカルアドレス",
        "PCのローカルアドレス": "ローカルアドレス",
        "PCのIPv4": "127.0.0.1",
        "systemdで起動": "PIDで起動",
        "Start with systemd": "Start locally",
        "systemd services": "system services",
        "Prepare the PC": "Prepare this PC",
        "Prepare PC": "Prepare this PC",
        "PC準備": "PC準備",
        "Detecting the Local address": "Checking the local address",
        "Could not detect the local address automatically. Enter the PC IPv4 address.": "Could not confirm the local address automatically. Usually 127.0.0.1 is fine for local mode.",
        "Use this IP as the local node address?": "Use this local address?",
        "Local address saved.": "Local address confirmed.",
        "detected Local address": "detected local address",
        "detected on PC": "detected locally",
        "PC側で検出": "ローカルで検出",
        "ローカルアドレスを検出しています...": "ローカルアドレスを確認しています...",
        "ローカルアドレスを自動検出できませんでした。127.0.0.1を入力してください。": "ローカルアドレスを確認できませんでした。通常は127.0.0.1で進めます。",
        "このIPをnodeのローカルアドレスとして使いますか？": "このローカルアドレスで進めますか？",
        "ローカルアドレスを保存しました。": "ローカルアドレスを確認しました。",
        "診断対象はPCです。あなたのPC/Macではなく、PCのCPU数と現在負荷からおすすめを出します。": "診断対象はこのPCです。CPU数と現在負荷から、控えめなおすすめスレッド数を出します。",
        "The diagnosis checks the PC, not your PC/Mac. It recommends threads from PC CPU count and current load.": "The diagnosis checks this local machine and recommends a conservative thread count from CPU count and current load.",
    }
    for old, new in replacements.items():
        text = text.replace(old, new)
    return text


def run_script(
    share_dir: Path,
    args: list[str],
    timeout: int | None = None,
    extra_env: dict[str, str] | None = None,
) -> tuple[int, str]:
    script = share_dir / "scripts" / "misaka-desktop-node.sh"
    env = os.environ.copy()
    env.setdefault("NO_COLOR", "1")
    env.setdefault("MISAKA_WEB_JOB", "1")
    if extra_env:
        env.update(extra_env)
    proc = subprocess.run(
        [str(script), *args],
        cwd=str(share_dir),
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=timeout,
    )
    return proc.returncode, clean_output(proc.stdout)


def run_job(job: JobState, share_dir: Path, name: str, args: list[str], extra_env: dict[str, str] | None = None) -> None:
    script = share_dir / "scripts" / "misaka-desktop-node.sh"
    env = os.environ.copy()
    env.setdefault("NO_COLOR", "1")
    env.setdefault("MISAKA_WEB_JOB", "1")
    if extra_env:
        env.update(extra_env)
    code = 1
    try:
        proc = subprocess.Popen(
            [str(script), *args],
            cwd=str(share_dir),
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            bufsize=1,
        )
        assert proc.stdout is not None
        for line in proc.stdout:
            job.append(clean_output(line))
        code = proc.wait()
    except Exception as exc:  # noqa: BLE001
        job.append(f"ERROR: {exc}\n")
        code = 1
    finally:
        job.append(f"\n[local-web] command {name} exited with {code}\n")
        job.finish(code)


def parse_status(output: str) -> dict:
    def has(pattern: str) -> bool:
        return re.search(pattern, output, re.IGNORECASE) is not None

    def reports_listening(label: str) -> bool:
        pattern = rf"^\s*{re.escape(label)}[^\r\n]*(?<!not )\blistening\b"
        return re.search(pattern, output, re.IGNORECASE | re.MULTILINE) is not None

    daa_match = re.search(r"Virtual DAA score\s+([0-9]+)", output)
    synced = has(r"Synced\s+true")
    kaspad_running = has(r"kaspad:\s+running")
    p2p_listening = reports_listening("P2P")
    wrpc_listening = reports_listening("wRPC Borsh")
    utxo_enabled = has(r"UTXO index\s+enabled")
    miner_running = has(r"miner:\s+running")
    validator_running = has(r"valid\.:\s+running")
    return {
        "kaspadRunning": kaspad_running,
        "minerRunning": miner_running,
        "validatorRunning": validator_running,
        "wrpcListening": wrpc_listening,
        "p2pListening": p2p_listening,
        "utxoIndex": utxo_enabled,
        "synced": synced,
        "daa": int(daa_match.group(1)) if daa_match else None,
        "raw": output,
    }



class Handler(BaseHTTPRequestHandler):
    server_version = "MisakaLocalWeb/0.1"

    def log_message(self, fmt: str, *args: object) -> None:
        return

    @property
    def token_ok(self) -> bool:
        parsed = urlparse(self.path)
        params = parse_qs(parsed.query)
        return params.get("token", [""])[0] == self.server.token  # type: ignore[attr-defined]

    def send_text(self, code: int, body: str, content_type: str = "text/plain; charset=utf-8") -> None:
        raw = body.encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(raw)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("Referrer-Policy", "no-referrer")
        self.send_header("X-Frame-Options", "DENY")
        self.end_headers()
        self.wfile.write(raw)

    def send_json(self, value: dict, code: int = 200) -> None:
        self.send_text(code, json.dumps(value), "application/json; charset=utf-8")

    def require_token(self) -> bool:
        if self.token_ok:
            return True
        self.send_json({"ok": False, "error": "bad or missing token"}, 403)
        return False

    def do_GET(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        if parsed.path in ("/", "/setup"):
            if not self.token_ok:
                self.send_text(403, "bad or missing token")
                return
            self.send_text(200, local_ui_html(self.server.share_dir, "setup.html", self.server.token), "text/html; charset=utf-8")  # type: ignore[attr-defined]
            return
        if parsed.path == "/dashboard":
            if not self.token_ok:
                self.send_text(403, "bad or missing token")
                return
            self.send_text(200, local_ui_html(self.server.share_dir, "dashboard.html", self.server.token), "text/html; charset=utf-8")  # type: ignore[attr-defined]
            return
        if parsed.path == "/learn":
            if not self.token_ok:
                self.send_text(403, "bad or missing token")
                return
            self.send_text(200, local_ui_html(self.server.share_dir, "learn.html", self.server.token), "text/html; charset=utf-8")  # type: ignore[attr-defined]
            return
        if not self.require_token():
            return
        if parsed.path == "/api/session/ping":
            self.send_json({"ok": True, "message": "local setup session is alive"})
        elif parsed.path == "/api/bootstrap/status":
            self.send_json(bootstrap_status(self.server.share_dir, self.server.job))  # type: ignore[attr-defined]
        elif parsed.path == "/api/bootstrap/logs":
            job = job_json(self.server.job)  # type: ignore[attr-defined]
            self.send_json({"ok": True, "job": job, "logs": job["logs"]})
        elif parsed.path == "/api/public-ip":
            self.send_json(public_ip_info(True))
        elif parsed.path == "/api/preflight":
            self.send_json({
                "ok": True,
                "checks": [
                    {"check": "Local script", "value": "scripts/misaka-desktop-node.sh", "status": "OK", "detail": "available"},
                    {"check": "Local bind", "value": "127.0.0.1", "status": "OK", "detail": "local-only Web UI"},
                    {"check": "Runtime", "value": "PID files", "status": "OK", "detail": "systemd is not required locally"},
                ],
            })
        elif parsed.path == "/api/status":
            self.send_json(status_value(self.server.share_dir))  # type: ignore[attr-defined]
        elif parsed.path == "/api/validator/status":
            self.send_json(validator_status_value(self.server.share_dir))  # type: ignore[attr-defined]
        elif parsed.path == "/api/validator/chain-status":
            value = validator_status_value(self.server.share_dir)  # type: ignore[attr-defined]
            value["message"] = "Local validator status checked."
            self.send_json(value)
        elif parsed.path == "/api/miner/status":
            self.send_json({"ok": True, "miner": miner_status_value(current_daa_value())})
        elif parsed.path == "/api/miner/diagnostics":
            value = diagnostics_value()
            value["validator"] = validator_status_value(self.server.share_dir)["validator"]  # type: ignore[attr-defined]
            self.send_json(value)
        elif parsed.path == "/api/logs":
            code, output = run_script(self.server.share_dir, ["node-logs"], timeout=45)  # type: ignore[attr-defined]
            self.send_json({"ok": code == 0, "logs": output})
        elif parsed.path == "/api/miner/logs":
            code, output = run_script(self.server.share_dir, ["miner-logs"], timeout=45)  # type: ignore[attr-defined]
            self.send_json({"ok": code == 0, "logs": output, "miner": miner_status_value(current_daa_value())})
        elif parsed.path == "/api/validator/logs":
            code, output = run_script(self.server.share_dir, ["validator-logs"], timeout=45)  # type: ignore[attr-defined]
            self.send_json({"ok": code == 0, "logs": output, "validator": validator_status_value(self.server.share_dir)["validator"]})  # type: ignore[attr-defined]
        elif parsed.path == "/api/job":
            self.send_json({"ok": True, "job": self.server.job.snapshot()})  # type: ignore[attr-defined]
        else:
            self.send_json({"ok": False, "error": "not found"}, 404)

    def do_POST(self) -> None:  # noqa: N802
        parsed = urlparse(self.path)
        if not self.require_token():
            return
        params = parse_qs(parsed.query)
        if parsed.path == "/api/public-ip/confirm":
            self.send_json(public_ip_info(True))
        elif parsed.path == "/api/bootstrap/prepare":
            if not self.server.job.start("prepare"):  # type: ignore[attr-defined]
                self.send_json({"ok": False, "error": "another command is already running"}, 409)
                return
            thread = threading.Thread(
                target=run_job,
                args=(self.server.job, self.server.share_dir, "prepare", ["prepare"]),  # type: ignore[attr-defined]
                daemon=True,
            )
            thread.start()
            job = job_json(self.server.job)  # type: ignore[attr-defined]
            self.send_json({"ok": True, "message": "Prepare local PC started.", "job": job})
        elif parsed.path == "/api/node/dry-run":
            self.send_json({
                "ok": True,
                "commands": [
                    "scripts/misaka-desktop-node.sh prepare",
                    "scripts/misaka-desktop-node.sh start-node",
                    "scripts/misaka-desktop-node.sh status",
                ],
                "unit": "Local desktop mode uses PID files, not systemd services.",
            })
        elif parsed.path == "/api/node/apply":
            code, output = run_script(self.server.share_dir, ["start-node"], timeout=60)  # type: ignore[attr-defined]
            self.send_json({
                "ok": code == 0,
                "message": "Local node started." if code == 0 else "Local node failed to start.",
                "logs": output,
                "service": "misaka-local-kaspad",
                "p2pPort": 26211,
            }, 200 if code == 0 else 500)
        elif parsed.path == "/api/node/restart":
            code, output = run_script(self.server.share_dir, ["restart-node"], timeout=60)  # type: ignore[attr-defined]
            self.send_json({"ok": code == 0, "message": "Local node restarted.", "logs": output}, 200 if code == 0 else 500)
        elif parsed.path == "/api/validator/keygen":
            code, output = run_script(self.server.share_dir, ["keygen"], timeout=60)  # type: ignore[attr-defined]
            value = validator_status_value(self.server.share_dir)  # type: ignore[attr-defined]
            value["ok"] = code == 0
            value["message"] = "Validator key created." if code == 0 else "Validator key generation failed."
            value["logs"] = output
            self.send_json(value, 200 if code == 0 else 500)
        elif parsed.path == "/api/miner/service/apply":
            threads = params.get("threads", ["1"])[0]
            if not re.fullmatch(r"[0-9]{1,2}", threads):
                threads = "1"
            code, output = run_script(self.server.share_dir, ["miner-start"], timeout=60, extra_env={"MISAKA_MINER_THREADS": threads})  # type: ignore[attr-defined]
            self.send_json({
                "ok": code == 0,
                "message": "Local funding miner started." if code == 0 else "Local funding miner failed.",
                "logs": output,
                "miner": miner_status_value(current_daa_value()),
            }, 200 if code == 0 else 500)
        elif parsed.path == "/api/miner/service/stop":
            code, output = run_script(self.server.share_dir, ["miner-stop"], timeout=45)  # type: ignore[attr-defined]
            self.send_json({"ok": code == 0, "message": "Local miner stopped.", "logs": output, "miner": miner_status_value(current_daa_value())})
        elif parsed.path == "/api/validator/balance":
            code, output = run_script(self.server.share_dir, ["balance"], timeout=90)  # type: ignore[attr-defined]
            value = validator_status_value(self.server.share_dir)  # type: ignore[attr-defined]
            balance_sompi, balance_msk = parse_balance(output)
            value["ok"] = code == 0
            value["logs"] = output
            value["message"] = "Funding balance checked."
            value["validator"]["balanceOutput"] = output
            value["validator"]["balanceSompi"] = balance_sompi
            value["validator"]["balanceMsk"] = balance_msk
            self.send_json(value, 200 if code == 0 else 500)
        elif parsed.path == "/api/validator/bond":
            amount = params.get("amount", ["10MSK"])[0]
            code, output = run_script(self.server.share_dir, ["bond", amount], timeout=120)  # type: ignore[attr-defined]
            value = validator_status_value(self.server.share_dir)  # type: ignore[attr-defined]
            value["logs"] = output
            value["message"] = "Bond submitted." if code == 0 else "Bond failed."
            if code != 0:
                value["ok"] = False
                value["error"] = output
            self.send_json(value, 200 if code == 0 else 500)
        elif parsed.path == "/api/validator/service/apply":
            code, output = run_script(self.server.share_dir, ["validator-start"], timeout=60)  # type: ignore[attr-defined]
            value = validator_status_value(self.server.share_dir)  # type: ignore[attr-defined]
            value["ok"] = code == 0
            value["logs"] = output
            value["message"] = "Validator started." if code == 0 else "Validator failed to start."
            self.send_json(value, 200 if code == 0 else 500)
        elif parsed.path == "/api/stop-setup":
            self.send_json({"ok": True, "message": "Local setup page is stopping."})
            threading.Thread(target=self.server.shutdown, daemon=True).start()
        elif parsed.path == "/api/run":
            cmd = parse_qs(parsed.query).get("cmd", [""])[0]
            if cmd not in COMMANDS:
                self.send_json({"ok": False, "error": f"unknown command: {html.escape(cmd)}"}, 400)
                return
            if not self.server.job.start(cmd):  # type: ignore[attr-defined]
                self.send_json({"ok": False, "error": "another command is already running"}, 409)
                return
            thread = threading.Thread(
                target=run_job,
                args=(self.server.job, self.server.share_dir, cmd, COMMANDS[cmd]),  # type: ignore[attr-defined]
                daemon=True,
            )
            thread.start()
            self.send_json({"ok": True, "job": self.server.job.snapshot()})  # type: ignore[attr-defined]
        elif parsed.path == "/api/stop-web":
            self.send_json({"ok": True})
            threading.Thread(target=self.server.shutdown, daemon=True).start()
        else:
            self.send_json({"ok": False, "error": "not found"}, 404)


class LocalServer(ThreadingHTTPServer):
    def __init__(self, addr: tuple[str, int], token: str, share_dir: Path) -> None:
        super().__init__(addr, Handler)
        self.token = token
        self.share_dir = share_dir
        self.job = JobState()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8788)
    parser.add_argument("--token", required=True)
    parser.add_argument("--share-dir", required=True)
    args = parser.parse_args()

    server = LocalServer((args.host, args.port), args.token, Path(args.share_dir).resolve())
    print(f"serving on http://127.0.0.1:{args.port}/?token={args.token}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
