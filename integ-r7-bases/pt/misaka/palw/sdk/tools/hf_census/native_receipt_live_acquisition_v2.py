"""Contemporaneous independent transport; no producer counts or PASS authority.

Each collector starts on its OWN validated sink before the first frozen window.
The original node lag cap remains unchanged. Raw proof streams are subsequently
verified by the pinned offline Core checker, including every consecutive state.
"""
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import json
import threading
import time


def validate_live_collectors(specs: list) -> None:
    for auditor in specs:
        command = auditor.get("live_collection")
        if not isinstance(command, dict):
            raise ValueError("each window auditor needs predeclared contemporaneous collection")
        argv = command.get("argv")
        if (not isinstance(argv, list) or not argv or any(not isinstance(v, str) or not v or "\0" in v for v in argv)
                or not isinstance(command.get("executable_sha256"), str) or len(command["executable_sha256"]) != 64
                or type(command.get("timeout_seconds")) is not int or not 1 <= command["timeout_seconds"] <= 604800):
            raise ValueError("live collector exact executable pin/argv and bounded timeout required")
        required = {"--start-child": "own-current-sink", "--ready-out": "{live_ready}",
                    "--out": "{live_collected}", "--operator-id": "{operator_id}",
                    "--node-database-id": "{node_database}", "--genesis": "{genesis}",
                    "--network": "{network}", "--first-window-start-daa": "{receipt_first_daa}",
                    "--stop-daa": "{receipt_stop_daa}"}
        for flag, value in required.items():
            if argv.count(flag) != 1 or argv.index(flag) + 1 == len(argv) or argv[argv.index(flag) + 1] != value:
                raise ValueError("live collector must bind original own-node identity, clocks and independent output")
        if "--receipt-live-collected" not in auditor["argv"]:
            raise ValueError("offline window auditor must use its independent live collection")
        i = auditor["argv"].index("--receipt-live-collected")
        if auditor["argv"].count("--receipt-live-collected") != 1 or i + 1 == len(auditor["argv"]) or auditor["argv"][i + 1] != "{live_collected}":
            raise ValueError("offline window auditor live collection pointer must come from its own runner context")


class LiveReceiptCollectors:
    def __init__(self, specs: list, contract: dict, context: dict, folder: Path):
        validate_live_collectors(specs)
        self.specs, self.contract, self.context, self.folder = specs, contract, context, folder
        self.cancel = threading.Event()
        self.pool = None
        self.futures, self.ready = [], []

    def __enter__(self):
        import checkpoint_cohort_runner as runner
        self.pool = ThreadPoolExecutor(max_workers=2)
        try:
            for i, auditor in enumerate(self.specs):
                folder = self.folder / f"receipt-live-{i}"
                folder.mkdir()
                (folder / "objects").mkdir()
                ready, collected = folder / "ready.json", folder / "collected"
                ctx = {**self.context, **{k: auditor[k] for k in ("operator_id", "node_database", "genesis", "network")},
                       "live_ready": str(ready.resolve()), "live_collected": str(collected.resolve()),
                       "live_objects": str((folder / "objects").resolve()),
                       "receipt_first_daa": str(self.contract["windows"][0]["start_daa"]),
                       "receipt_stop_daa": str(max(w["end_daa"] + w["maximum_final_lag_daa"] for w in self.contract["windows"]))}
                self.ready.append(ready)
                self.futures.append(self.pool.submit(runner.run_command, auditor["live_collection"], ctx,
                                                      folder / "collect.log", self.cancel))
            began = time.monotonic()
            while True:
                if any(f.done() for f in self.futures):
                    for f in self.futures:
                        if f.done():
                            code, _, error = f.result()
                            if code or error:
                                raise ValueError(error or "independent live collector failed before producer start")
                if all(p.is_file() for p in self.ready):
                    break
                if time.monotonic() - began > 1200:
                    raise ValueError("independent live collectors did not capture original first state before startup deadline")
                time.sleep(.05)
            for auditor, path in zip(self.specs, self.ready):
                if path.stat().st_size > 16384:
                    raise ValueError("independent live readiness metadata bound")
                doc = runner.read_json(path)
                if (doc.get("schema") not in ("misaka.rfc02.receipt-live-ready.v2", "misaka.rfc02.receipt-live-ready.v39")
                        or any(doc.get(k) != auditor[k] for k in ("operator_id", "node_database", "genesis", "network"))
                        or type(doc.get("first_parent_daa")) is not int
                        or doc["first_parent_daa"] >= self.contract["windows"][0]["start_daa"]):
                    raise ValueError("independent live collector missed original first window or changed own-node identity")
            return self
        except BaseException:
            self.cancel.set()
            self.pool.shutdown(wait=True, cancel_futures=True)
            raise

    def __exit__(self, kind, value, traceback):
        try:
            if kind is not None:
                self.cancel.set()
            else:
                for future in self.futures:
                    code, _, error = future.result()
                    if code or error:
                        self.cancel.set()
                        raise ValueError(error or "independent original live collection failed")
        except BaseException:
            self.cancel.set()
            raise
        finally:
            self.pool.shutdown(wait=True, cancel_futures=True)
        return False


def canonical_window_stream(source: Path, output: Path, evidence: Path, contract: dict) -> int:
    """Retain the unique consecutive window interval from a longer live stream.

    Headers here are merely used to locate a candidate interval; the offline
    verifier then authenticates them and all pointed rows. No interior sampling.
    """
    import checkpoint_cohort_runner as runner
    first = contract["windows"][0]["start_daa"]
    stop = max(w["end_daa"] + w["maximum_final_lag_daa"] for w in contract["windows"])
    prior, started, ended, count = None, False, False, 0
    with source.open("rb") as src, output.open("xb") as dst:
        header = src.readline(4097)
        if header != b'{"schema":"misaka.rfc02.receipt-state-stream.v2"}\n':
            raise ValueError("original live32 pointer stream schema required")
        dst.write(header)
        while raw := src.readline(4097):
            if len(raw) > 4096 or not raw.endswith(b"\n"):
                raise ValueError("original live pointer line partial/oversized")
            point = json.loads(raw)
            if set(point) != {"child", "selected_parent", "collected"}:
                raise ValueError("original live pointer grammar differs")
            folder = runner.relative_file(evidence, point["collected"])
            parent = runner.read_json(folder / "selected-parent-header.json")
            daa = parent.get("daaScore")
            if type(daa) is not int or daa < 0:
                raise ValueError("original live parent DAA absent")
            if not started:
                if daa < first:
                    prior = raw
                    continue
                if prior is None:
                    raise ValueError("independent live collection began after original first window")
                dst.write(prior); count += 1; started = True
            dst.write(raw); count += 1
            if daa >= stop:
                ended = True
                break
        if not ended or count < 3:
            raise ValueError("independent live collection ended before original complete windows")
    return count
