"""Compare the three native boundary reads with the original branch and its raw custody carriers."""
import argparse
import hashlib
import json
import re
from pathlib import Path

from check_legacy_work import HERE, read_peer, require


def verify(archive, logs, renewed=False):
    original = json.loads((HERE / "legacy-work.json").read_text())
    parent = json.loads((HERE / "genesis-capital.json").read_text()) if renewed else original
    parent_state = parent["state"] if renewed else original["state_root"]
    manifest_path = HERE.parents[3] / "consensus/core/src/config/class-manifests/qwen25-1.5b-a16-8k.palwmanifest"
    manifest = json.loads(manifest_path.read_text())
    [model] = manifest["classes"]
    blobs = archive.read_bytes()
    extension = json.loads(blobs)
    blocks, beats = extension["blocks"], extension["boundary_blocks"]
    binary = None
    observation = None
    pids = set()
    for path in logs:
        checksum, pid, history, requested = read_peer(path)
        old = history["result"]
        require(requested == 0 and old["tip"] == original["tip"] and old["state"] == original["state_root"]
                and old["close"] == original["close"], "wrong starting audit fork")
        line = re.findall(r"^legacy genesis boundary (\d+) (\{.*\})$", path.read_text(), re.M)
        require(len(line) == (2 if renewed else 1) and all(int(v[0]) == pid for v in line) and pid not in pids, "missing or repeated peer")
        if renewed:
            capital = re.findall(r"^legacy genesis capital (\d+) (\{.*\})$", path.read_text(), re.M)
            require(len(capital) == 1 and int(capital[0][0]) == pid, "missing mature allocation")
            funded = json.loads(capital[0][1])
            require(all(parent.get(k) == value for k, value in funded.items()) and funded["base"] == json.loads(line[0][1])["tip"],
                    "renewal did not extend the original duty and funded class")
        row = json.loads(line[-1][1])
        if binary is None:
            binary, observation = checksum, row
        require((checksum, row) == (binary, observation), "peer observations disagree")
        pids.add(pid)
    row = observation
    require(len(pids) == 3 and row is not None, "three distinct native processes required")
    require(extension["base"] == row["base"] == parent["tip"], "extension origin")
    require(row["class"] == model["class_id"] and row["artifact"] == model["inventory_root"], "genesis class binding")
    require(len(blocks) == 16 and 0 < len(beats) == row["heartbeats"] and row["claims"] == 0, "extension bounds")
    require(blocks[0]["header"]["parentsByLevel"][0] == [parent["tip"]]
            and blocks[0]["header"]["palwStateRoot"] == parent_state, "first parent commitment")
    require(blocks[-1]["header"]["daaScore"] == row["seat_daa"] < row["boundary"]
            and row["boundary"] == (row["seat_daa"] // row["span_daa"] + 1) * row["span_daa"]
            and beats[-1]["header"]["daaScore"] == row["daa"] == row["boundary"] + 1, "registry evaluation boundary")
    headers = [b["header"] for b in blocks + beats]
    require(all(h["powAlgoId"] == 8 for h in headers)
            and all(b["blueScore"] == a["blueScore"] + 1 for a, b in zip(headers, headers[1:])), "ordinary consecutive heartbeats")
    cards = row["cards"]
    require([c["card"] for c in cards] == list(range(8)) and len({c["bond"] for c in cards}) == 8, "distinct signing bonds")
    for card, carrying, child in zip(cards, blocks[::2], blocks[1::2]):
        require(len(carrying["transactions"]) == 2 and len(child["transactions"]) == 1, "expected one duty per carrier")
        tx = carrying["transactions"][1]
        require(tx["subnetworkId"] == "4b" + "00" * 19 and len(tx["inputs"]) == len(tx["outputs"]) == 1, "lifecycle carrier")
        data = bytes.fromhex(tx["payload"])
        match = re.fullmatch(r"\(([0-9a-f]{128}), (\d+)\)", card["bond"])
        require(match is not None and data[:3] == bytes.fromhex("010032"), "invalid bond or V2 custody carrier")
        require(data[3:67].hex() == match[1] and int.from_bytes(data[67:71], "little") == int(match[2])
                and data[71:135].hex() == row["class"] and int.from_bytes(data[135:143], "little") == card["span"],
                "payload bond, class or challenge span differs from the native row")
        require(0 < card["chunks"] == int.from_bytes(data[147:151], "little") <= int.from_bytes(data[143:147], "little")
                and int.from_bytes(data[-4631:-4627], "little") == 4627, "drawn leaf count or ML-DSA-87 signature size")
    require(all(len(b["transactions"]) == 1 for b in beats), "unexpected payload during registry evaluation")
    require(row["ready"] == sum(c["fresh"] and not c["seat_reason"] for c in cards), "ready count")
    return dict(binary_sha256=binary, processes=sorted(pids), archive_sha256=hashlib.sha256(blobs).hexdigest(),
                artifact_digest=manifest["artifact_digest"], artifact_bytes=manifest["artifact_bytes"],
                duties=len(cards), selected_blocks=len(headers), **row)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("logs", type=Path, nargs=3)
    parser.add_argument("--renewed", action="store_true", help="check duties on the seasoned allocation instead of the initial fork")
    args = parser.parse_args()
    print(json.dumps(verify(args.archive, args.logs, args.renewed), indent=2))
