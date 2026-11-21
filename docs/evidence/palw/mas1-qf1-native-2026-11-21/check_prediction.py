"""Check the three-process synthetic TIR fixture against the supplied QF1 reference."""
import hashlib
import json
from pathlib import Path
import re
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "design/palw/models/qf1"))
import qf1_reference as qf1


def shared_rows(log, pattern):
    rows = re.findall(pattern, log)
    if len(rows) != 3 or len({r[0] for r in rows}) != 3 or len({r[1:] for r in rows}) != 1:
        raise ValueError("expected three different processes with identical observations")
    return {r[0] for r in rows}, rows[0][1:]


def check(log):
    peers, (attempt, prompt) = shared_rows(log, r"QF1 input pid=(\d+) attempt=([0-9a-f]{64}) tokens=(\[[0-9, ]+\])")
    pids, (cls, descriptor, positions, answer, stop, digest) = shared_rows(
        log, r"QF1 fixture pid=(\d+) class=(\w+) descriptor=(\w+) positions=(\d+) tokens=(\[[0-9, ]+\]) stop=(\w+) z=(\w+)"
    )
    if peers != pids:
        raise ValueError("input and inference processes differ")
    prompt, answer = json.loads(prompt), json.loads(answer)
    def update(state, token):
        return max(-32767, min(32767, state + (-10000 if token == 7 else 1 + token % 7)))

    state = 0
    for token in prompt:
        state = update(state, token)
    steps = []
    for j, chosen in enumerate(answer):
        if j:
            state = update(state, answer[j - 1])
        rows = []
        for offset in [*range(1, 37), 41]:
            rows.append([min(32767, max(-32767, min(32767, state + i % 7 - 3)) + offset) for i in range(4096)])
        maximum = max(rows[-1])
        expected = 7 if maximum > 0 else (151643 if maximum < 0 else 0)
        if chosen != expected:
            raise ValueError("greedy result differs from the signed fixture logit table")
        steps.append(rows)
    schedule = qf1.schedule(prompt, answer, 3)
    if (schedule["processed_positions"], schedule["stop_reason"], qf1.readout(steps).hex()) != (int(positions), stop.upper(), digest):
        raise ValueError("position count, stop or readout differs from distributed reference")
    return dict(pids=sorted(map(int, peers)), model="synthetic PALWTIR1 schedule fixture", instance=descriptor,
                class_id=cls, attempt=attempt, input_tokens=prompt, answer_tokens=answer, readout=digest, **schedule)


if __name__ == "__main__":
    result = check(Path(sys.argv[1]).read_text())
    with open(sys.argv[2], "rb") as binary:
        digest = hashlib.sha256()
        for chunk in iter(lambda: binary.read(1 << 20), b""):
            digest.update(chunk)
        result["binary_sha256"] = digest.hexdigest()
    print(json.dumps(result, indent=2))
