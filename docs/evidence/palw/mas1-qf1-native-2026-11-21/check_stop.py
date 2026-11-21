"""Verify the two disclosed leaves and the native peers' local Stop verdict."""
from pathlib import Path
import json
import re
import struct
import sys
from check_panel import check_panel
from check_prediction import qf1, shared_rows


def check(log):
    return verify_case(log, check_panel(log))


def verify_case(log, panel):
    peers, fields = shared_rows(log, r"TB1 Court Stop pid=(\d+) original=([0-9a-f]{128}) case=([0-9a-f]{128}) "
                                r"verdict=([0-9a-f]{128}) result=(\w+) witness=([0-9a-f]+)")
    original, case, verdict, outcome, wire = fields
    if sorted(map(int, peers)) != panel["pids"] or original != panel["original"]:
        raise ValueError("different case participants or Original")
    if len({case, verdict, panel["accepted_replies"], panel["receipt"]}) != 4:
        raise ValueError("duplicate receipt, case or verdict carrier")
    _, (attempt,) = shared_rows(log, r"QF1 input pid=(\d+) attempt=([0-9a-f]{64})")
    l, t, _, d = panel["counts"]
    wire = bytes.fromhex(wire)
    answer_depth, trace_depth = (t - 1).bit_length(), (d - 1).bit_length()
    if len(wire) != 22 + 64 * (answer_depth + trace_depth):
        raise ValueError("invalid opening lengths")
    token = int.from_bytes(wire[:4], "little")
    stop = wire[4:22]
    siblings = [wire[i:i+64] for i in range(22, len(wire), 64)]
    stop_index = d - 2 - panel["readout_chunks"]
    for kind, count, index, data, path in [(2, t, t - 1, wire[:4], siblings[:answer_depth]),
                                           (4, d, stop_index, stop, siblings[answer_depth:])]:
        if not qf1.verify_opening(bytes.fromhex(attempt), kind, count, index, data, path, bytes.fromhex(panel["roots"][kind-1])):
            raise ValueError("unauthenticated case material")
    reason = 0 if token in qf1.EOS else 1
    expected = struct.pack("<BIIIIB", 3, 13, t, l+t-1, token, reason)
    if stop != expected or (token not in qf1.EOS and t != 3) or outcome != "ChallengeNotEstablished":
        raise ValueError("honest Stop or local verdict differs from reference")
    return dict(original=original, case=case, verdict=verdict, result=outcome, token=token,
                answer_index=t-1, stop_index=stop_index, processed_positions=l+t-1, witness_bytes=len(wire),
                siblings=[answer_depth, trace_depth], pids=panel["pids"], binary_sha256=panel["binary_sha256"])


if __name__ == "__main__":
    print(json.dumps(check(Path(sys.argv[1]).read_text()), indent=2))
