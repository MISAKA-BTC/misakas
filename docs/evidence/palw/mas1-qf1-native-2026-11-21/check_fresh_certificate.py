"""Verify the funded Gate C draw, signed numerical reviews and cold state on the fresh audit branch."""
import argparse
import json
import re
import struct
from pathlib import Path

from check_fresh_audit import records
from check_fresh_final import panel, walk
from check_funded_certificate import verify as verify_certificate
from check_genesis_draw import verify_readers
from check_legacy_work import HERE, require


def studio_economics(path, binary):
    log = path.read_text()
    require(log.splitlines()[0] == "test_binary_sha256=" + binary
            and log.count("test result: ok. 1 passed;") == 3, "incomplete same-build MOE2 registration")
    registrations = re.findall(r"pid=(\d+) tip=(\w+) root=(\w+) class=(\w+) artifact=(\w+) daa=1 burn=100000000 market=", log)
    require(len(registrations) == 3 and len({r[1:] for r in registrations}) == 1, "different registered Studio market")
    pids = {r[0] for r in registrations}
    accounting = re.findall(r"^MOE2 accounting projection pid=(\d+) (Observation .*?) rollback=true$", log, re.M)
    require(len(pids) == 3 and len(accounting) == 3 and {p for p, _ in accounting} == pids
            and len({v for _, v in accounting}) == 1, "different native accounting checkpoints")
    require("consumed: 16000000000, reserve: 8000000000, retired: 1, miner: 6400000000, panel: 1600000000"
            in accounting[0][1], "different MOE2 fixture funding or distributions")
    checked = re.findall(r"^MOE2 pid=(\d+) detached_enrollment=true native_parent=(\w+) native_child=(\w+) undo_bytes=\d+ restored_market=true restored_worker_rights=true restored_lp=true replay=true$", log, re.M)
    require(len(checked) == 3 and {r[0] for r in checked} == pids and len({r[1:] for r in checked}) == 1,
            "missing native MOE2 undo")
    return dict(processes=sorted(map(int, pids)), model=registrations[0][3], inventory=registrations[0][4],
                registration=registrations[0][1], settlement_parent=checked[0][1], settlement_child=checked[0][2],
                reservation_sompi=16_000_000_000, market_sompi=8_000_000_000, retired_units=1,
                miner_sompi=6_400_000_000, panel_sompi=1_600_000_000)


def verify(root, winning):
    fresh = json.loads((HERE / "fresh-retained-audit.json").read_bytes())
    original = fresh["native_8k_draw"]
    claim = original["claim"]
    final = panel(root)
    writer = root / "funded-1.log"
    log = writer.read_text()
    [(pid, recovered)] = records(log, "legacy genesis recovered")
    require(recovered["claim"] == claim and recovered["tip"] == final["tip"] and recovered["state"] == final["state"]
            and recovered["final_daa"] == final["daa"] and recovered["draws"] == recovered["approvals"] == 0
            and recovered["outcome"] == "None", "writer did not resume the uncredited Final")
    final_branch = json.loads((root / "final.json").read_bytes())
    draw, headers = walk(root / "funded/draw.json", final["tip"], claim, final_branch["blocks"][-1]["header"]["blueScore"])
    [(c, anchor_claim, anchor, end, drawn_at, positions)] = re.findall(
        r"^funded floor draw claim=(\w+) anchor_claim=(\w+) block=(\w+) tip=(\w+) daa=(\d+) positions=(\d+)$", log, re.M)
    require(c == claim and end == draw["tip"] and headers[anchor]["powAlgoId"] == 6 and int(positions) == 75
            and final["daa"] <= headers[anchor]["daaScore"] <= int(drawn_at) < recovered["certify_by_daa"], "wrong funded draw")
    [(c, base, opportunity, at, floor, card, amount, capacity, deadline)] = re.findall(
        r"^funded floor opportunity claim=(\w+) base=(\w+) tip=(\w+) daa=(\d+) floor=(\w+) card=(\d+) assigned=(\d+) microblocks=(\d+) deadline=(\d+)$",
        log, re.M)
    require(c == claim and base == final["tip"] and int(deadline) == recovered["certify_by_daa"]
            and headers[opportunity]["daaScore"] == int(at) and headers[anchor]["parentsByLevel"][0] == [opportunity]
            and int(card) == 5 and int(amount) > 0 and int(capacity) >= 1_000_000, "no available floor opportunity")
    [first, *_] = draw["blocks"]
    [payload] = [bytes.fromhex(t["payload"]) for t in first["transactions"] if t["subnetworkId"] == "4b" + "00"*19]
    require(payload[:3] == bytes.fromhex("01008c") and payload[71:75] == struct.pack("<I", 1)
            and payload[75:139].hex() == floor and int.from_bytes(payload[139:147], "little") == int(amount)
            and payload[147:155] == struct.pack("<Q", 1), "different floor assignment")
    views = records(log, "legacy genesis boundary")
    [(txid, index)] = re.findall(r"\(([0-9a-f]{128}), (\d+)\)", views[-1][1]["cards"][int(card)]["bond"])
    require(payload[3:71] == bytes.fromhex(txid) + struct.pack("<I", int(index)), "wrong funding bond")
    cert = verify_certificate(root / "funded/certification.json", writer, winning,
                              original=dict(original, reexecution=fresh["cold_8k_replay"]),
                              funded=dict(tip=end, drawn_daa=int(drawn_at)))
    require(cert["writer"] == pid and cert["binary_sha256"] == final["binary_sha256"], "different writer build")
    peers = [root / f"read-cert-{i}.log" for i in (0, 2)]
    cold = verify_readers(original, peers, 2)
    require(cold["binary_sha256"] == cert["binary_sha256"] and pid not in cold["processes"], "independent certificate readback")
    observations = []
    for reader, path in zip(cold["processes"], peers):
        [(claim_pid, row)] = records(path.read_text(), "legacy genesis certification")
        [(audit_pid, audit)] = records(path.read_text(), "signed audit after nonfloor")
        require(reader == claim_pid == audit_pid and row["claim"] == claim and row["accepted"] == original["block"]
                and row["class"] == original["class_id"] and row["final_daa"] == final["daa"]
                and row["tip"] == audit["tip"] == cert["tip"] and row["state"] == audit["state"]
                and row["certified_daa"] == audit["daa"] == cert["certified_daa"] and row["approvals"] == row["drawn"] == 75
                and row["replay"] == cert["transcript"] and audit["deadline"] == fresh["audit"]["audit_deadline"]
                and audit["close"] == final["audit_close"], "native reader disagrees with signed state")
        readiness = audit["readiness"]
        require(readiness["generation"] == audit["tip"] and audit["publication"] == "held"
                and readiness["safe"] is None and readiness["finalized"]["finalized"] is None, "unexpected audit state")
        blocking = readiness["blocking"]
        [wait] = [w for w in blocking["waits"] if w["kind"] == "waitingMaturity"]
        [retention] = [w for w in readiness["tip"]["waits"] if w["kind"] == "openClaim" and w["claim"] == claim]
        require(blocking["evidence"]["pendingFacts"] == wait["facts"] == 1 and blocking["evidence"]["maturedFacts"] == 0
                and blocking["evidence"]["pendingWork"] == wait["work"] and int(wait["work"]) > 0
                and audit["daa"] < wait["readyDaa"] == retention["retentionDaa"] < audit["deadline"]
                and audit["daa"] + wait["waitDaa"] == wait["readyDaa"], "different pending work or signed release window")
        observations.append((row, audit))
    require(observations[0] == observations[1], "readers disagree about remaining work")
    return dict(panel=final, funding=dict(card=int(card), model=floor, collateral_sompi=int(amount), microblocks=int(capacity),
                                         daa=int(at), anchor_claim=anchor_claim, anchor=anchor, tip=end),
                certificate=cert, readers=cold["processes"], native=observations[0][0], audit=observations[0][1],
                separate_studio_moe2=studio_economics(root / "moe2-studio.log", cert["binary_sha256"]))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("winning", type=Path, help="accepted/winning.json from the original fresh audit archive")
    args = parser.parse_args()
    print(json.dumps(verify(args.root, args.winning), indent=2))
