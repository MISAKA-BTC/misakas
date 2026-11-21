"""Derive a fixed replay plan from signed-original evidence and check both routes' timed results."""
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

from check_g_cost_readout_reuse import HERE, LOGS
from check_real_lifecycle import check


def frozen_plan():
    identity = json.loads((HERE / "qf1-native-tile-artifacts.json").read_bytes())["full"]
    expected = {"class_id": identity["class_id"], "artifact_root": identity["inventory"]}
    samples = []
    for name, mode in LOGS:
        path = HERE / name
        data = subprocess.check_output(["zstd", "-dc", path]) if path.suffix == ".zst" else path.read_bytes()
        log = data.decode()
        r = check(log, expected, decode_limit=16, **mode)
        opened = [json.loads(line) for line in re.findall(r"^QF1 disclosure (\{.*\})$", log, re.M)]
        [init] = {bytes.fromhex(row["record"]) for row in opened if row["index"] == 0}
        samples.append(dict(source=name, attempt=r["attempt"], context=init[5:69].hex(), input_ids=r["input_ids"],
                            output_ids=r["output_ids"], z=r["readout_z"], roots=r["panel"]["roots"], ticket=r["panel"]["ticket"],
                            descriptor=r["descriptor"]))
    [descriptor] = {s["descriptor"] for s in samples}
    if len({s["attempt"] for s in samples}) != len(samples):
        raise ValueError("duplicate attempt in cost plan")
    return dict(class_id=expected["class_id"], inventory=expected["artifact_root"], descriptor=descriptor,
                decode_limit=16, max_leaves=32_000_000, trials=samples)


def verify(plan, log):
    rows = [json.loads(line) for line in log.read_text().splitlines()]
    header, *runs = rows
    if any(header[k] != plan[k] for k in ("class_id", "inventory", "descriptor", "decode_limit", "max_leaves")):
        raise ValueError("different numerical instance or replay constraints")
    if header.get("discovered_answer_length") is not True:
        raise ValueError("generation length must be discovered before building the trace")
    trials = plan["trials"]
    if len(runs) != len(trials) or header["pairs"] != len(trials) // 2:
        raise ValueError("incomplete experiment")
    pairs = []
    for i in range(header["pairs"]):
        a, b = trials[2*i:2*i+2]
        prefix = 0
        for x, y in zip(a["input_ids"][:-1], b["input_ids"][:-1]):
            if x != y:
                break
            prefix += 1
        values = {}
        for row, shared in zip(runs[2*i:2*i+2], [i % 2 != 0, i % 2 == 0]):
            tickets = [bytes(x).hex() for x in row["verified_tickets"]]
            if (tickets != [a["ticket"], b["ticket"]] or row["pair"] != i or row["shared"] is not shared
                    or row["common_positions_executed_once"] != (prefix if shared else 0) or row["elapsed_us"] <= 0):
                raise ValueError("route, order or verified original mismatch")
            values["shared_us" if shared else "independent_us"] = row["elapsed_us"]
        # Both routes first discover the stopping position, then build the trace.
        # Prefill and decode positions have different costs; counts describe the schedule.
        independent_steps = 2 * sum(len(r["input_ids"]) + len(r["output_ids"]) - 1 for r in (a, b))
        pairs.append(dict(pair=i, attempts=[a["attempt"], b["attempt"]], prefix=prefix,
                          independent_forward_positions=independent_steps,
                          shared_forward_positions=independent_steps - 2 * prefix, **values))
    baseline = sum(p["independent_us"] for p in pairs)
    shared = sum(p["shared_us"] for p in pairs)
    return dict(binary_and_host_scope="one local process, warmed shared model, elapsed wall-clock microseconds",
                setup_ms=header["setup_ms"], pairs=pairs, summed_independent_us=baseline,
                summed_shared_us=shared, ratio_shared_to_independent=shared/baseline, g_cost="UNKNOWN",
                packed_plan_sha256=hashlib.sha256(json.dumps(plan, indent=2).encode()+b"\n").hexdigest())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--log", type=Path)
    args = parser.parse_args()
    plan = frozen_plan()
    print(json.dumps(verify(plan, args.log) if args.log else plan, indent=2))
