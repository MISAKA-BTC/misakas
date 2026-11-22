"""Feed a growing PANEL archive to a native reader and attest the final parent before EOF."""
import argparse
import json
import os
import sys
import time
from pathlib import Path

from check_arithmetic_panel_parent import verify


def stream_prefix(prefix, writer_alive, output):
    with prefix.open("rb") as source:
        while True:
            start = source.tell()
            line = source.readline()
            if line.endswith(b"\n"):
                output.write(line)
                continue
            output.flush()
            source.seek(start)
            if writer_alive():
                time.sleep(0.25)
                continue
            # The writer may append its final bytes between the read and
            # the liveness check. Re-read the last position after it exits.
            last = source.readline()
            if not last:
                return
            if not last.endswith(b"\n"):
                raise ValueError("writer stopped with an incomplete PANEL block")
            output.write(last)


def stream_and_verify(prefix, native_log, writer_alive, report, output):
    stream_prefix(prefix, writer_alive, output)
    verified = verify(prefix, native_log)
    pending = report.with_suffix(".tmp")
    with pending.open("x") as result:
        json.dump(verified, result, indent=2)
        result.write("\n")
    pending.replace(report)


def running(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    return True


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    parser.add_argument("writer_log", type=Path)
    parser.add_argument("writer_pid", type=int)
    parser.add_argument("parent_report", type=Path)
    args = parser.parse_args()
    if args.writer_pid <= 0:
        parser.error("writer PID must be positive")
    if args.parent_report.exists() or args.parent_report.with_suffix(".tmp").exists():
        parser.error("report must be a fresh path")
    stream_and_verify(args.prefix, args.writer_log, lambda: running(args.writer_pid), args.parent_report, sys.stdout.buffer)
