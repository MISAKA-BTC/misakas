"""Compare unchanged numerical readouts across archived, independently verified QF1 executions."""
import hashlib
import json
import subprocess
from itertools import combinations
from pathlib import Path

from check_real_lifecycle import check

HERE = Path(__file__).resolve().parent
# Successful originals with complete Panel material for the same tiled instance.
LOGS = [
    ("qf1-native-tile-three-peer.txt", {}),
    ("qf1-held-stop-three-peer.txt.zst", {"retained_stop": True}),
    ("qf1-stop-intake-three-peer.txt.zst", {"retained_stop": True}),
    ("qf1-stop-end-three-peer.txt.zst", {"retained_stop": True, "retained_end": True}),
    ("qf1-signed-readout-control.txt.zst", {"retained_stop": True, "retained_end": True}),
    ("qf1-undisputed-replay.txt.zst", {"undisputed": True}),
]


def compare():
    instance = json.loads((HERE / "qf1-native-tile-artifacts.json").read_bytes())["full"]
    expected = {"class_id": instance["class_id"], "artifact_root": instance["inventory"]}
    records = []
    for name, mode in LOGS:
        path = HERE / name
        raw = subprocess.check_output(["zstd", "-dc", path]) if path.suffix == ".zst" else path.read_bytes()
        result = check(raw.decode(), expected, decode_limit=16, **mode)
        records.append(dict(source=name, log_sha256=hashlib.sha256(raw).hexdigest(), attempt=result["attempt"],
                            descriptor=result["descriptor"], input_ids=result["input_ids"], output_ids=result["output_ids"],
                            exec_seed=result["panel"]["exec_seed"], z=result["readout_z"]))
    for key in ("attempt", "exec_seed", "input_ids"):
        if len({json.dumps(r[key]) for r in records}) != len(records):
            raise ValueError("repeated " + key)
    if any(r["output_ids"] != records[0]["output_ids"] or r["descriptor"] != records[0]["descriptor"] for r in records):
        raise ValueError("different answer or tiled instance in comparison")
    pairs = list(combinations(records, 2))
    matches = [[a["attempt"], b["attempt"]] for a, b in pairs if a["z"] == b["z"]]
    prefixes = []
    for a, b in pairs:
        shared = 0
        for left, right in zip(a["input_ids"], b["input_ids"]):
            if left != right:
                break
            shared += 1
        prefixes.append(shared)
    for row in records:
        tokens = row.pop("input_ids")
        row["input_tokens"] = len(tokens)
        row["input_le32_sha256"] = hashlib.sha256(b"".join(t.to_bytes(4, "little") for t in tokens)).hexdigest()
    return dict(class_id=expected["class_id"], comparisons=len(pairs), equal_numerical_readouts=matches,
                common_prefix_tokens=dict(minimum=min(prefixes), maximum=max(prefixes)),
                measurement="unchanged whole readout reuse across archived attempts with the same answer",
                cost_units_measured=False, g_cost="UNKNOWN", trials=records)


if __name__ == "__main__":
    print(json.dumps(compare(), indent=2))
