"""Rebind one archived QF1 answer and readout to the five other public attempt IDs."""
import json
import struct

from check_g_cost_shared_prefix import frozen_plan
from check_prediction import qf1


def compare(plan):
    first, *remaining = plan["trials"]
    answer = first["output_ids"]
    leaves = [struct.pack("<I", token) for token in answer]
    cached_z = bytes.fromhex(first["z"])
    rows = []
    for row in remaining:
        attempt = bytes.fromhex(row["attempt"])
        answer_root = qf1.root(attempt, 2, leaves).hex()
        readout_root = qf1.root(attempt, 3, [cached_z]).hex()
        rows.append(dict(attempt=row["attempt"], predicted_length=len(answer), actual_length=len(row["output_ids"]),
                         predicted_answer_root=answer_root, answer_matches=answer == row["output_ids"],
                         answer_root_matches=answer_root == row["roots"][1], predicted_readout_root=readout_root,
                         readout_root_matches=readout_root == row["roots"][2]))
    return dict(descriptor=plan["descriptor"], source_attempt=first["attempt"], trials=rows,
                measured_execution_cost=False, g_cost="UNKNOWN")


if __name__ == "__main__":
    print(json.dumps(compare(frozen_plan()), indent=2))
