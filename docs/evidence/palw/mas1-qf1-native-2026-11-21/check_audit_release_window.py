"""Check the original held MODEL_AUDIT from the certified branch to its signed deadline."""
import argparse
import hashlib
import json
import re
from pathlib import Path

from check_genesis_draw import header_hash
from check_legacy_work import HERE, require


def verify(branch, native_log):
    recovered = json.loads((HERE / "genesis-funded-certificate-replicas.json").read_bytes())
    audit = recovered["cold_readback"]["signed_audit"]
    start = audit["readiness"]
    deadline = audit["deadline"]
    log = native_log.read_text()
    require("test result: ok. 1 passed; 0 failed" in log, "native run did not finish")
    [binary] = re.findall(r"^test_binary_sha256=([0-9a-f]{64})$", log, re.M)
    rows = [(int(pid), json.loads(data)) for pid, data in re.findall(r"^signed audit release window (\d+) (\{.*\})$", log, re.M)]
    require(len(rows) >= 3 and len({pid for pid, _ in rows}) == 1, "expected one complete run through the release window")
    states = {row["blocks"]: row for _, row in rows}
    require(len(states) == len(rows), "duplicate native observation")
    initial, last = rows[0][1], rows[-1][1]
    require(initial["blocks"] == 0 and initial["readiness"] == start, "different originating certified state")
    require(any(row["daa"] == deadline - 1 for _, row in rows) and last["daa"] == deadline, "exclusive deadline boundary")
    pruning = initial["pruning"]
    original = json.loads((HERE / "registered-certified-frontier.json").read_bytes())
    require(original["finalized"]["pruningBlue"] == 0 and original["finalized"]["pruningPoint"] == pruning, "original genesis pruning point")
    digest = hashlib.sha256()
    tip, daa, blue = audit["tip"], start["sinkDaa"], start["sinkBlue"]
    observations = []

    def check_observation(index):
        if index not in states:
            return
        row = states[index]
        require((row["base"], row["close"], row["deadline"], row["tip"], row["daa"], row["blue"]) ==
                (audit["tip"], audit["close"], deadline, tip, daa, blue), "native observation differs from the raw branch")
        require(row["pruning"] == row["header_pruning"] == pruning and row["covers_close"] is False, "unexpected pruning coverage")
        require(row["publication"] == ("held" if daa < deadline else "expired"), "publication crossed the original deadline")
        ready = row["readiness"]
        require((ready["generation"], ready["sinkBlue"], ready["sinkDaa"]) == (tip, blue, daa) and
                ready["finalized"]["finalized"] is None and ready["safe"] is None, "unexpected native settlement")
        evidence = ready["blocking"]["evidence"]
        require(ready["blocking"]["block"] == start["blocking"]["block"] and evidence["maturedFacts"] == 0 and evidence["work"] == "0",
                "unexpected mature work on the blocking prefix")
        if daa >= deadline - 1:
            require(evidence["pendingFacts"] == 0 and evidence["pendingWork"] == "0", "pending evidence at the deadline boundary")
        observations.append(dict(blocks=index, tip=tip, daa=daa, blue=blue, publication=row["publication"], evidence=evidence))

    check_observation(0)
    with branch.open("rb") as stream:
        count = 0
        for count, line in enumerate(stream, 1):
            digest.update(line)
            block = json.loads(line)
            h = block["header"]
            require(h["parentsByLevel"][0] == [tip] and h["blueScore"] == blue + 1 and
                    daa <= h["daaScore"] <= deadline, "disconnected or nonmonotonic release window")
            require(h["powAlgoId"] == 8 and h["pruningPoint"] == pruning and len(block["transactions"]) == 1
                    and block["transactions"][0]["subnetworkId"] == "01" + "00" * 19, "expected an ordinary isolated heartbeat")
            tip, daa, blue = header_hash(h, bytes(h["palwCommitment"]), False).hex(), h["daaScore"], h["blueScore"]
            check_observation(count)
    require(count == last["blocks"] and daa == deadline and len(observations) == len(rows), "incomplete branch")
    return dict(close=audit["close"], base=audit["tip"], deadline=deadline, tip=tip, blocks=count,
                initial_earliest_maturity_daa=start["sinkDaa"] + start["blocking"]["earliestReadyInDaa"], pruning=pruning,
                pruning_depth=initial["pruning_depth"], finality_depth=initial["finality_depth"], observations=observations,
                binary_sha256=binary, process=rows[0][0], archive_sha256=digest.hexdigest())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("branch", type=Path, help="raw successor blocks in JSONL format")
    parser.add_argument("native_log", type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.branch, args.native_log), indent=2))
