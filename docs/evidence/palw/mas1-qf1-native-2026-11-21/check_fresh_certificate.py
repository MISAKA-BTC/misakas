"""Verify the funded Gate C draw, signed numerical reviews and cold state on the fresh audit branch."""
import argparse
import json
import re
import struct
from pathlib import Path

from check_fresh_audit import records, verify as verify_audit
from check_fresh_final import panel, walk
from check_funded_certificate import verify as verify_certificate
from check_genesis_draw import attempt_identity, header_hash, verify_readers
from check_legacy_work import HERE, require
from check_transaction_root import verify_block_transactions


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


def funded_draw(root, final, writer):
    claim = final["claim"]
    log = writer.read_text()
    [(pid, recovered)] = records(log, "legacy genesis recovered")
    require(recovered["claim"] == claim and recovered["tip"] == final["tip"] and recovered["state"] == final["state"]
            and recovered["final_daa"] == final["daa"] and recovered["draws"] == recovered["approvals"] == 0
            and recovered["outcome"] == "None", "writer did not resume the uncredited Final")
    require(recovered["accepted"] == final["accepted"] and recovered["replay"] == final["transcript"]
            and log.splitlines()[0] == "test_binary_sha256="+final["binary_sha256"], "wrong recovered original or binary")
    final_branch = json.loads((root / "final.json").read_bytes())
    draw, headers = walk(root / "funded/draw.json", final["tip"], claim, final_branch["blocks"][-1]["header"]["blueScore"])
    [(c, anchor_claim, anchor, end, drawn_at, positions)] = re.findall(
        r"^funded floor draw claim=(\w+) anchor_claim=(\w+) block=(\w+) tip=(\w+) daa=(\d+) positions=(\d+)$", log, re.M)
    require(c == claim and end == draw["tip"] and headers[anchor]["powAlgoId"] == 6 and int(positions) == 75
            and final["daa"] <= headers[anchor]["daaScore"] <= int(drawn_at) < recovered["certify_by_daa"], "wrong funded draw")
    _, _, observed_anchor_claim, _, _ = attempt_identity(headers[anchor])
    require(anchor_claim == observed_anchor_claim and headers[end]["daaScore"] == int(drawn_at), "different draw anchor or DAA")
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
    return dict(card=int(card), model=floor, collateral_sompi=int(amount), microblocks=int(capacity), daa=int(at),
                anchor_claim=anchor_claim, anchor=anchor, tip=end), dict(drawn_daa=int(drawn_at), tip=end,
                positions=int(positions), deadline=int(deadline), blocks=len(headers), writer=pid)


def verify(root, winning, arithmetic_work=False, draw_only=False, openssl=None, writer_only=False, first_reader=False):
    if arithmetic_work:
        fresh = verify_audit(root, arithmetic_work=True)
        replay = fresh["native_replay"]
        writer = root / "funded-0.log"
    else:
        fresh = json.loads((HERE / "fresh-retained-audit.json").read_bytes())
        replay = fresh["cold_8k_replay"]
        writer = root / "funded-1.log"
    original = fresh["native_8k_draw"]
    claim = original["claim"]
    final = panel(root, arithmetic_work, openssl)
    funding, draw = funded_draw(root, final, writer)
    pid = draw["writer"]
    if draw_only:
        return dict(panel=final, funding=funding, draw=draw)
    cert = verify_certificate(root / "funded/certification.json", writer, winning,
                              original=dict(original, reexecution=replay),
                              funded=draw, openssl=openssl)
    require(cert["writer"] == pid and cert["binary_sha256"] == final["binary_sha256"], "different writer build")
    if writer_only:
        return dict(panel=final, funding=funding, certificate=cert)
    peers = [root / f"read-cert-{i}.log" for i in ((0,) if first_reader else (0, 2))]
    cold = verify_readers(original, peers, len(peers))
    require(cold["binary_sha256"] == cert["binary_sha256"] and pid not in cold["processes"], "independent certificate readback")
    end = json.loads((root / "funded/certification.json").read_bytes())["blocks"][-1]["header"]
    observations = []
    for reader, path in zip(cold["processes"], peers):
        [(claim_pid, row)] = records(path.read_text(), "legacy genesis certification")
        [(audit_pid, audit)] = records(path.read_text(), "signed audit after nonfloor")
        require(reader == claim_pid == audit_pid and row["claim"] == claim and row["accepted"] == original["block"]
                and row["frontier"] == [original["blue_score"], original["block"]]
                and row["class"] == original["class_id"] and row["final_daa"] == final["daa"]
                and row["tip"] == audit["tip"] == cert["tip"] and row["state"] == audit["state"]
                and row["certified_daa"] == audit["daa"] == cert["certified_daa"] and row["approvals"] == row["drawn"] == 75
                and row["replay"] == cert["transcript"] and audit["deadline"] == fresh["audit"]["audit_deadline"]
                and audit["close"] == final["audit_close"], "native reader disagrees with signed state")
        readiness = audit["readiness"]
        require(readiness["generation"] == readiness["tip"]["block"] == audit["tip"]
                and readiness["tip"]["blue"] == readiness["sinkBlue"] == end["blueScore"]
                and readiness["tip"]["daa"] == readiness["sinkDaa"] == end["daaScore"]
                and audit["publication"] == "held"
                and readiness["safe"] is None and readiness["finalized"]["finalized"] is None, "unexpected audit state")
        blocking = readiness["blocking"]
        [wait] = [w for w in blocking["waits"] if w["kind"] == "waitingMaturity"]
        [retention] = [w for w in readiness["tip"]["waits"] if w["kind"] == "openClaim" and w["claim"] == claim]
        require(blocking["evidence"]["pendingFacts"] == wait["facts"] == 1 and blocking["evidence"]["maturedFacts"] == 0
                and blocking["evidence"]["pendingWork"] == wait["work"] and int(wait["work"]) > 0
                and audit["daa"] < wait["readyDaa"] == retention["retentionDaa"] < audit["deadline"]
                and audit["daa"] + wait["waitDaa"] == wait["readyDaa"], "different pending work or signed release window")
        observations.append((row, audit))
    if first_reader:
        return dict(reader=cold["processes"][0], binary_sha256=cert["binary_sha256"], native=observations[0][0], audit=observations[0][1])
    require(observations[0] == observations[1], "readers disagree about remaining work")
    return dict(panel=final, funding=funding,
                certificate=cert, readers=cold["processes"], native=observations[0][0], audit=observations[0][1],
                separate_studio_moe2=studio_economics(root / "moe2-studio.log", cert["binary_sha256"]))


def certified_successor(log, block, first, writer):
    """Check the next peer's certification and the first child committing its parent state."""
    [(pid, native)] = records(log, "legacy genesis certification")
    [(audit_pid, audit)] = records(log, "signed audit after nonfloor")
    windows = records(log, "signed audit release window")
    require(windows and windows[0][0] == audit_pid == pid, "missing initial release observation")
    initial = windows[0][1]
    cert = writer["certificate"]
    binaries = re.findall(r"^test_binary_sha256=([a-f0-9]{64})$", log, re.M)
    require(len({pid, first["reader"], cert["writer"]}) == 3 and binaries == [first["binary_sha256"]] == [cert["binary_sha256"]],
            "expected a third native process on the original build")
    require(native == first["native"] and audit == first["audit"] and native["tip"] == cert["tip"] and
            native["claim"] == cert["claim"] and native["replay"] == cert["transcript"], "different certified claim or cold state")
    require(initial["blocks"] == 0 and initial["base"] == initial["tip"] == native["tip"] and initial["close"] == audit["close"] and
            initial["publication"] == audit["publication"] == "held" and initial["deadline"] == audit["deadline"] and
            initial["readiness"] == audit["readiness"] and initial["daa"] == cert["certified_daa"] and
            initial["blue"] == audit["readiness"]["sinkBlue"], "different release window at certification")
    header = block["header"]
    verify_block_transactions(block)
    require(header["parentsByLevel"][0] == [native["tip"]] and header["blueScore"] == initial["blue"] + 1 and
            initial["daa"] <= header["daaScore"] < audit["deadline"] and header["palwStateRoot"] == native["state"],
            "successor does not commit the certified parent state")
    require(header["version"] == 2 and header["powAlgoId"] == 8 and len(block["transactions"]) == 1 and
            block["transactions"][0]["subnetworkId"] == "01" + "00" * 19, "unexpected certification successor")
    return dict(claim=native["claim"], certificate=native["tip"], certified_daa=initial["daa"], approvals=native["approvals"],
                state=native["state"], successor=header_hash(header, bytes(header["palwCommitment"]), False).hex(),
                successor_blue=header["blueScore"], audit_close=audit["close"], publication_at_certificate=initial["publication"],
                processes=[cert["writer"], first["reader"], pid], binary_sha256=cert["binary_sha256"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("winning", type=Path, help="accepted/winning.json from the original fresh audit archive")
    parser.add_argument("--arithmetic-work", action="store_true")
    stage = parser.add_mutually_exclusive_group()
    stage.add_argument("--writer-only", action="store_true", help="verify the certificate before cold replica checks")
    stage.add_argument("--certified-successor", type=Path, metavar="FIRST_BLOCK", help="compare both replicas and the first parent-state commitment")
    stage.add_argument("--first-reader", action="store_true", help="check the first completed cold replica against the archived certification")
    stage.add_argument("--draw-only", action="store_true", help="verify only through the first funded Gate C draw")
    parser.add_argument("--openssl", help="verify the isolated Panel and Gate C card signatures")
    args = parser.parse_args()
    if args.certified_successor:
        first = verify(args.root, args.winning, args.arithmetic_work, openssl=args.openssl, first_reader=True)
        writer = verify(args.root, args.winning, args.arithmetic_work, openssl=args.openssl, writer_only=True)
        result = certified_successor((args.root / "read-cert-2.log").read_text(), json.loads(args.certified_successor.read_bytes()), first, writer)
    else:
        result = verify(args.root, args.winning, args.arithmetic_work, args.draw_only, args.openssl, args.writer_only, args.first_reader)
    print(json.dumps(result, indent=2))
