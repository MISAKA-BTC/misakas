"""Bind the signed second bond allocation and its mature opportunity to the 8K seat branch."""
import argparse
import hashlib
import json
import re
from pathlib import Path

from check_legacy_work import HERE, read_peer, require


def verify(archive, logs):
    seat = json.loads((HERE / "genesis-seat-boundary.json").read_text())
    branch = json.loads((HERE / "legacy-work.json").read_text())
    raw = archive.read_bytes()
    chain = json.loads(raw)
    peers = []
    for path in logs:
        text = path.read_text()
        binary, pid, history, _ = read_peer(path)
        require(history["result"]["tip"] == branch["tip"], "starting fork mismatch")
        observed = re.findall(r"^legacy genesis capital (\d+) (\{.*\})$", text, re.M)
        require(len(observed) == 1 and int(observed[0][0]) == pid, "missing assignment observation")
        peers.append((pid, binary, json.loads(observed[0][1])))
    [binary] = {b for _, b, _ in peers}
    row = peers[0][2]
    pids = sorted({pid for pid, _, data in peers if data == row})
    require(len(pids) == 3, "three distinct matching native peers required")
    require(chain["base"] == row["base"] == seat["tip"] and row["class"] == seat["class"]
            and row["bond"] == seat["cards"][1]["bond"] and row["assigned"] == chain["assigned"], "allocation identity")
    blocks = chain["blocks"]
    first = blocks[0]
    require(first["header"]["parentsByLevel"][0] == [seat["tip"]] and first["header"]["palwStateRoot"] == seat["state"],
            "allocation parent commitment")
    require(len(first["transactions"]) == 2 and all(len(b["transactions"]) == 1 for b in blocks[1:]), "unexpected spending")
    require(blocks[2]["header"]["parentsByLevel"][0] == [row["assigned"]] and
            blocks[1]["header"]["daaScore"] == row["assignment_daa"], "allocation acceptance block")
    require(all(b["header"]["powAlgoId"] == 8 for b in blocks)
            and all(b["header"]["blueScore"] == a["header"]["blueScore"] + 1 for a, b in zip(blocks, blocks[1:])), "heartbeat lineage")
    tx = first["transactions"][1]
    require(tx["subnetworkId"] == "4b" + "00" * 19, "wrong carrier subnetwork")
    payload = bytes.fromhex(tx["payload"])
    match = re.fullmatch(r"\(([0-9a-f]{128}), (\d+)\)", row["bond"])
    require(match and payload[:3] == bytes.fromhex("01008c") and payload[3:67].hex() == match[1]
            and int.from_bytes(payload[67:71], "little") == int(match[2]), "wrong signed bond")
    require(int.from_bytes(payload[71:75], "little") == 1 and payload[75:139].hex() == row["class"]
            and int.from_bytes(payload[139:147], "little") == row["assigned_sompi"] == row["collateral"] // 2
            and int.from_bytes(payload[147:155], "little") == 1, "allocation vector or sequence")
    require(blocks[-1]["header"]["daaScore"] == row["daa"] < row["deadline"] and row["daa"] > row["assignment_daa"]
            and row["available_microblocks"] > 0 and row["claims"] == 0, "maturity bounds")
    return dict(binary_sha256=binary, processes=pids, archive_sha256=hashlib.sha256(raw).hexdigest(), blocks=len(blocks), **row)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("logs", type=Path, nargs=3)
    args = parser.parse_args()
    print(json.dumps(verify(args.archive, args.logs), indent=2))
