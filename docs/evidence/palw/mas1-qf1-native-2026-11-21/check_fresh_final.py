"""Check the Panel checkpoint for the first accepted 8K job on the fresh audit branch."""
import argparse
import json
import re
import struct
from pathlib import Path

from check_fresh_audit import records, verify as verify_audit
from check_genesis_draw import header_hash, verify_readers
from check_legacy_work import HERE, require
from check_transaction_root import verify_block_transactions


def walk(path, base, claim, blue):
    branch = json.loads(path.read_bytes())
    require((branch["claim"], branch["base"]) == (claim, base), "wrong accepted branch")
    headers = {}
    for block in branch["blocks"]:
        verify_block_transactions(block)
        h = block["header"]
        require(h["parentsByLevel"][0] == [base] and h["blueScore"] == blue + 1, "disconnected selected block")
        base = header_hash(h, bytes(h["palwCommitment"]), False).hex()
        blue = h["blueScore"]
        headers[base] = h
    require(headers and branch["tip"] == base, "wrong selected tip")
    return branch, headers


def panel(root, arithmetic_work=False):
    if arithmetic_work:
        fresh = verify_audit(root, arithmetic_work=True)
        reference = fresh["native_replay"]
    else:
        fresh = json.loads((HERE / "fresh-retained-audit.json").read_bytes())
        reference = fresh["cold_8k_replay"]
    original = fresh["native_8k_draw"]
    claim, accepted = original["claim"], original["block"]
    writer = root / "final-0.log"
    log = writer.read_text()
    native = verify_readers(original, [writer], 1)
    require(native["replay"] == reference["replay"], "different canonical material")
    if arithmetic_work:
        require(native["binary_sha256"] == reference["binary_sha256"] and native["processes"][0] not in reference["processes"],
                "expected a fresh Final reader on the arithmetic work build")
    [(pid, audit)] = records(log, "fresh retained audit")
    require(pid == native["processes"][0] and audit["close"] == fresh["audit"]["close"]
            and audit["rounds"] == fresh["audit"]["rounds"], "different audit or process")
    branch, headers = walk(root / "final.json", accepted, claim, original["blue_score"])
    tip = branch["tip"]
    [(c, origin, end, final_daa, state)] = re.findall(
        r"^legacy genesis final checkpoint claim=(\w+) accepted=(\w+) tip=(\w+) phase=Final \{ final_daa: (\d+) \} state=(\w+)$",
        log, re.M)
    require((c, origin, end, int(final_daa)) == (claim, accepted, tip, headers[tip]["daaScore"]), "different Final checkpoint")
    [(anchor, anchor_daa, c)] = re.findall(r"^executed panel anchor=(\w+) daa=(\d+) claim=(\w+) ", log, re.M)
    require(c == claim and headers[anchor]["powAlgoId"] == 6 and headers[anchor]["daaScore"] == int(anchor_daa) < int(final_daa),
            "different executed Panel anchor")
    boundaries = records(log, "legacy genesis boundary")
    require(len(boundaries) == 2 and all(p == pid for p, _ in boundaries), "missing seat receipts")
    by_bond = {card["bond"]: card["card"] for card in boundaries[-1][1]["cards"]}
    carrier = []
    for block in branch["blocks"]:
        for tx in block["transactions"]:
            if tx["subnetworkId"] == "4b" + "00"*19:
                body = bytes.fromhex(tx["payload"])
                if body[:3] == bytes.fromhex("010006"):
                    carrier.append((body, block["header"]["daaScore"]))
    [(body, carrying_daa)] = carrier
    require(body[3:67] == bytes.fromhex(claim), "different Panel approval subject")
    seats, = struct.unpack_from("<I", body, 67)
    position = 71
    cards = []
    for _ in range(seats):
        row = body[position:]
        require(row[:64] == bytes.fromhex(claim) and row[64] == 0, "different Panel receipt claim or verdict")
        index, signed_at, size = struct.unpack_from("<IQI", row, 129)
        bond = f"({row[65:129].hex()}, {index})"
        require(size == 4627 and len(row) >= 145 + size and int(anchor_daa) <= signed_at <= carrying_daa <= int(final_daa),
                "invalid signed receipt envelope or time")
        cards.append(by_bond[bond])
        position += 145 + size
    signed = [int(card) for c, card in re.findall(r"^verified execution (\w+) for signing card (\d+)$", log, re.M) if c == claim]
    require(position == len(body) and len(set(cards)) == len(cards) == len(signed) == 3
            and sorted(signed) == sorted(cards) and 1 not in cards, "selected independent replays and receipts differ")
    return dict(claim=claim, accepted=accepted, audit_close=audit["close"], anchor=anchor, anchor_daa=int(anchor_daa),
                cards=cards, receipt_daa=carrying_daa, tip=tip, daa=int(final_daa), state=state, blocks=len(headers),
                transcript=native["replay"], binary_sha256=native["binary_sha256"], writer=pid)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, help="extracted native logs and raw Final suffix")
    parser.add_argument("--arithmetic-work", action="store_true")
    args = parser.parse_args()
    print(json.dumps(panel(args.root, args.arithmetic_work), indent=2))
