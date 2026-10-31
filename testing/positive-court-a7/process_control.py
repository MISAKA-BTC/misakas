"""Own a session/process group until all descendants can no longer write logs."""
import os
import signal
import subprocess
import time


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
    with log.open("xb") as stream:
        process = subprocess.Popen(argv, cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            try:
                code = process.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                code = "TIMEOUT"
                cleanup = stop_group(process)
            else:
                # A successful wrapper that leaves a writer running is not success.
                if active(process.pid):
                    code = "DESCENDANT_LEAK"
                    cleanup = stop_group(process)
                else:
                    cleanup = {"group": process.pid, "live_descendants": [], "zombies": members(process.pid)}
        except BaseException:
            stop_group(process)
            raise
    # Only now may the caller read/hash the completed log.
    return {"exit_code": code, "elapsed_seconds": time.monotonic()-start, "cleanup": cleanup}
