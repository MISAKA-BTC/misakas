"""Own one process group until its members can no longer write logs.

This is not an OS sandbox: a program that deliberately creates a different
session/group is outside this boundary. SIGKILL of this controller cannot be
handled. The normal runner calls this module from Python's main thread.
"""
from contextlib import contextmanager
import os
import signal
import subprocess
import threading
import time


class ProcessInterrupted(KeyboardInterrupt):
    """Cancellation propagated only after the owned group has been stopped."""
    def __init__(self, signum, cleanup):
        self.signum = signum
        self.cleanup = cleanup
        super().__init__(f"signal {signum}; owned process group terminated")


@contextmanager
def cancellation_signals():
    state = {"signal": None}
    previous = {}

    def record(signum, _frame):
        # Do not raise asynchronously inside Popen or group cleanup. In
        # particular, a second Ctrl-C must not interrupt the SIGKILL/reap step.
        if state["signal"] is None:
            state["signal"] = signum

    if threading.current_thread() is threading.main_thread():
        for name in ("SIGINT", "SIGTERM", "SIGHUP"):
            if hasattr(signal, name):
                signum = getattr(signal, name)
                previous[signum] = signal.signal(signum, record)
    try:
        yield state
    finally:
        for signum, handler in previous.items():
            signal.signal(signum, handler)


def members(group):
    rows = subprocess.check_output(["ps", "-A", "-o", "pid=,pgid=,stat="], text=True)
    return [(int(pid), state) for pid, pgid, state in (x.split() for x in rows.splitlines()) if int(pgid) == group]


def active(group):
    return [pid for pid, state in members(group) if not state.startswith("Z")]


def stop_group(process):
    group = process.pid
    for sig, grace in [(signal.SIGTERM, 1.0), (signal.SIGKILL, 5.0)]:
        try:
            os.killpg(group, sig)
        except ProcessLookupError:
            pass
        until = time.monotonic() + grace
        while time.monotonic() < until:
            process.poll()  # reap the direct child as soon as it exits
            if not active(group):
                process.wait(timeout=1)
                return {"group": group, "live_descendants": [], "zombies": members(group)}
            time.sleep(0.05)
    raise RuntimeError(f"process group {group} still active; evidence MUST NOT be finalized")


def run_group(argv, *, cwd, env, log, timeout):
    start = time.monotonic()
    with cancellation_signals() as cancelled, log.open("xb") as stream:
        if cancelled["signal"] is not None:
            raise ProcessInterrupted(cancelled["signal"], {"group": None, "live_descendants": [], "zombies": []})
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
        cleanup = None
        try:
            deadline = start + timeout
            while True:
                if cancelled["signal"] is not None:
                    cleanup = stop_group(process)
                    raise ProcessInterrupted(cancelled["signal"], cleanup)
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    code = "TIMEOUT"
                    cleanup = stop_group(process)
                    break
                try:
                    code = process.wait(timeout=min(0.1, remaining))
                    break
                except subprocess.TimeoutExpired:
                    continue
            if cleanup is None:
                # A successful wrapper that leaves a writer running is not success.
                if active(process.pid):
                    code = "DESCENDANT_LEAK"
                    cleanup = stop_group(process)
                else:
                    cleanup = {"group": process.pid, "live_descendants": [], "zombies": members(process.pid)}
            if cancelled["signal"] is not None:
                raise ProcessInterrupted(cancelled["signal"], cleanup)
        except BaseException:
            if cleanup is None:
                stop_group(process)
            raise
    # Only now may the caller read/hash the completed log.
    if cancelled["signal"] is not None:
        raise ProcessInterrupted(cancelled["signal"], cleanup)
    return {"exit_code": code, "leader_exit_code": process.returncode,
            "elapsed_seconds": time.monotonic()-start, "cleanup": cleanup}
