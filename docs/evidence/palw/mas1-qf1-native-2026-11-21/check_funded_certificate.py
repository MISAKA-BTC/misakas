"""Match the funded Gate C certificate to its seven signed native transactions and retained job."""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path
from check_fresh_qualification import verify_card
from check_genesis_draw import header_hash, keyed, verify_readers
from check_legacy_work import HERE, require
from check_transaction_root import verify_block_transactions


def verify(branch_path, writer_path, winning_path, *, funded=None, original=None, openssl=None):
    if funded is None:
        funded = json.loads((HERE / "genesis-funded-draw.json").read_bytes())
    if original is None:
        original = json.loads((HERE / "genesis-draw-reexecution.json").read_bytes())
    claim = original["claim"]
    job = json.loads(winning_path.read_bytes())
    h = job["raw"]["header"]
    attempt = bytes(h["palwCommitment"])
    require(job["claim"] == claim and header_hash(h, attempt, False).hex() == original["block"], "original accepted attempt")
    network = attempt[6:70]
    exam = keyed("misaka-palw/security-certification/id/v1", bytes.fromhex(claim))
    transcript = original["reexecution"]["replay"]
    log = writer_path.read_text()
    require("test result: ok. 1 passed; 0 failed" in log, "native writer incomplete")
    [binary] = re.findall(r"^test_binary_sha256=([0-9a-f]{64})$", log, re.M)
    boundaries = [(pid, json.loads(raw)) for pid, raw in re.findall(r"^legacy genesis boundary (\d+) (\{.*\})$", log, re.M)]
    [(pid, before)] = [(pid, row) for pid, row in boundaries if row["boundary"] == original["daa"] - 1]
    by_bond = {}
    for card in before["cards"]:
        txid, index = re.fullmatch(r"\(([0-9a-f]{128}), (\d+)\)", card["bond"]).groups()
        by_bond[bytes.fromhex(txid) + struct.pack("<I", int(index))] = card["card"]
    reviews = re.findall(r"^verified Gate C execution (\w+) card=(\d+) transcript=(\w+)$", log, re.M)
    workers = [int(card) for c, card, t in reviews if (c, t) == (claim, transcript)]
    require(len(workers) == len(reviews) == len(set(workers)) == 7 and 1 not in workers, "expected seven non-producer replays")
    raw = branch_path.read_bytes()
    branch = json.loads(raw)
    tip = funded["tip"]
    require((branch["base"], branch["claim"]) == (tip, claim), "certification branch")
    carriers = []
    for block in branch["blocks"]:
        verify_block_transactions(block)
        h = block["header"]
        require(h["parentsByLevel"][0] == [tip], "disconnected selected header")
        tip = header_hash(h, bytes(h["palwCommitment"]), False).hex()
        for tx in block["transactions"]:
            if tx["subnetworkId"] != "4b" + "00"*19:
                continue
            wire = bytes.fromhex(tx["payload"])
            require(wire[:3] == bytes.fromhex("01006e"), "unexpected lifecycle object in certification extension")
            size = int.from_bytes(wire[3:7], "little")
            inner, rest = wire[7:7+size], wire[7+size:]
            require(len(inner) == 265 and inner[:4] == bytes.fromhex("014b0100"), "not a canonical Gate C attestation")
            bond = rest[:68]
            require(len(rest) == 68+4+4627 and rest[68:72] == struct.pack("<I",4627), "native signature envelope")
            require(inner[4:68] == network and inner[68:132] == exam and inner[132:196] == keyed("misaka-palw/kernel-route/bond-id/v1", bond),
                    "attestation network, exam or verifier")
            require(inner[196] == 0 and inner[197:261].hex() == transcript and inner[261:] == bytes(4), "different verdict or execution")
            if openssl:
                message = keyed("misaka-palw/kernel-route/object-message/v1", network, bond, struct.pack("<Q", len(inner)), inner)
                verify_card(openssl, by_bond[bond], message, rest[72:], "misaka-palw/kernel-route/object/v1")
            carriers.append(dict(card=by_bond[bond], verifier=inner[132:196].hex(), block=tip, daa=h["daaScore"]))
    [(c, end, daa, approvals)] = re.findall(r"^funded floor certified claim=(\w+) tip=(\w+) outcome=Some\(\(Certified, (\d+)\)\) approvals=(\d+)$", log, re.M)
    require((c,end,int(daa),int(approvals)) == (claim,tip,h["daaScore"],75) and tip == branch["tip"], "writer certification outcome")
    require([row["card"] for row in carriers] == workers, "attestation sequence differs from native replays")
    return dict(claim=claim, accepted=original["block"], draw_daa=funded["drawn_daa"], certified_daa=int(daa), tip=tip,
                positive_positions=int(approvals), transcript=transcript, attestations=carriers, blocks=len(branch["blocks"]),
                archive_sha256=hashlib.sha256(raw).hexdigest(), writer=int(pid), binary_sha256=binary)


def recover(certificate, paths):
    original = json.loads((HERE / "genesis-draw.json").read_bytes())
    funded = json.loads((HERE / "genesis-funded-draw-replicas.json").read_bytes())["recovered"]
    peers = verify_readers(original, paths, 2)
    require(peers["binary_sha256"] == certificate["binary_sha256"] and certificate["writer"] not in peers["processes"],
            "three independent native processes on the same executable")
    audit = json.loads((HERE / "registered-certified-frontier.json").read_bytes())
    views = []
    for pid, path in zip(peers["processes"], paths):
        log = path.read_text()
        def observation(kind):
            [(observed, raw)] = re.findall(r"^" + kind + r" (\d+) (\{.*\})$", log, re.M)
            require(int(observed) == pid, "readback process identity")
            return json.loads(raw)
        row = observation("legacy genesis certification")
        expected = dict(claim=certificate["claim"], accepted=original["block"], tip=certificate["tip"],
                        frontier=[original["blue_score"], original["block"]],
                        certified_daa=certificate["certified_daa"], approvals=certificate["positive_positions"], drawn=funded["draws"],
                        final_daa=funded["final_daa"], replay=certificate["transcript"])
        expected["class"] = original["class_id"]
        for key, value in expected.items():
            require(row[key] == value, "certification readback differs for " + key)
        signed = observation("signed audit after nonfloor")
        require(signed["close"] == audit["close"] and signed["deadline"] == audit["deadline"] and
                (signed["tip"], signed["state"], signed["daa"]) == (row["tip"], row["state"], row["certified_daa"]),
                "the audit observation and certificate must share one state")
        ready = signed["readiness"]
        require(ready["generation"] == signed["tip"] and ready["sinkDaa"] == signed["daa"] < signed["deadline"], "audit snapshot")
        require(signed["publication"] == "held" and ready["safe"] is None and ready["finalized"]["finalized"] is None, "unreleased native audit")
        blocker = ready["blocking"]
        require(blocker["evidence"] == dict(anchors=0, maturedFacts=0, pendingFacts=1, pendingWork="1388926972416", work="0"),
                "native pending work on the blocking prefix")
        [maturity] = [wait for wait in blocker["waits"] if wait["kind"] == "waitingMaturity"]
        [retention] = [wait for wait in ready["tip"]["waits"] if wait["kind"] == "openClaim" and wait["claim"] == certificate["claim"]]
        require(maturity["readyDaa"] == retention["retentionDaa"] == 17931 and
                signed["deadline"] < maturity["readyDaa"] == signed["daa"] + blocker["earliestReadyInDaa"],
                "maturity and original claim retention must extend past the signed deadline")
        views.append((row, signed))
    require(views[0] == views[1], "readers disagree on the native certification or settlement")
    return dict(processes=peers["processes"], certification=views[0][0], signed_audit=views[0][1])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("certification_branch", "writer", "original_attempt"):
        parser.add_argument(name, type=Path)
    parser.add_argument("--readers", type=Path, nargs=2, help="validate two completed independent cold replays of the final branch")
    args = parser.parse_args()
    result = verify(args.certification_branch, args.writer, args.original_attempt)
    if args.readers:
        result["cold_readback"] = recover(result, args.readers)
    print(json.dumps(result, indent=2))
