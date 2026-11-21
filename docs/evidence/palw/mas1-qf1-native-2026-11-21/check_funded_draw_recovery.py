"""Recompute both archived header sequences and match two cold readers at the funded Gate C draw."""
import argparse
import hashlib
import json
import re
from pathlib import Path
from check_genesis_draw import header_hash, verify_readers
from check_legacy_work import HERE, require

def verify(suffix, extension, paths):
    draw = json.loads((HERE / "genesis-draw.json").read_text())
    funded = json.loads((HERE / "genesis-funded-draw.json").read_text())
    old = json.loads((HERE / "genesis-recovered-three-peer.json").read_text())["recovered"]
    common = verify_readers(draw, paths, 2)
    exported = json.loads((HERE / "genesis-exported-suffix.json").read_text())
    block = draw["block"]
    count = 0
    for raw_path, digest in ((suffix, exported["sha256"]), (extension, funded["archive_sha256"])):
        raw = raw_path.read_bytes()
        require(hashlib.sha256(raw).hexdigest() == digest, "archive digest")
        branch = json.loads(raw)
        require(branch["base"] == block and branch["claim"] == draw["claim"] == funded["original_claim"], "archive base or claim")
        for row in branch["blocks"]:
            h = row["header"]
            require(h["parentsByLevel"][0] == [block], "disconnected raw header")
            block = header_hash(h, bytes(h["palwCommitment"]), False).hex()
        require(block == branch["tip"], "archive tip")
        count += len(branch["blocks"])
    require(block == funded["tip"] and count == old["blocks"] + funded["blocks"], "different funded draw")
    recovered = []
    for pid, path in zip(common["processes"], paths):
        [(process, raw)] = re.findall(r"^legacy genesis recovered (\d+) (\{.*\})$", path.read_text(), re.M)
        require(int(process) == pid, "different claim intake and recovered processes")
        recovered.append(json.loads(raw))
    a, b = recovered
    require(a == b, "different native state after cold recovery")
    for key, value in dict(claim=draw["claim"], accepted=draw["block"], blocks=count, tip=block, daa=h["daaScore"],
                           final_daa=old["final_daa"], phase=old["phase"], current_seed_anchor=[funded["anchor_daa"], funded["anchor_block"]],
                           drawn_daa=funded["drawn_daa"], draws=funded["drawn_positions"], approvals=0, outcome="None",
                           certify_by_daa=funded["certify_by_daa"], replay=common["replay"]).items():
        require(a[key] == value, f"recovered {key}")
    return dict(processes=common["processes"], binary_sha256=common["binary_sha256"], recovered=a)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("final_suffix", type=Path)
    parser.add_argument("funded_extension", type=Path)
    parser.add_argument("readers", type=Path, nargs=2)
    args = parser.parse_args()
    print(json.dumps(verify(args.final_suffix, args.funded_extension, args.readers), indent=2))
