"""Verify an individual public custody release against its committed native head and original enrollment."""
import argparse
import hashlib
import json
import struct
import subprocess
import tempfile
from pathlib import Path

from check_fresh_audit import records
from check_fresh_qualification import public_keys, sized, u32, verify_signature, verify as verify_qualification
from check_fresh_window import check_maturity
from check_genesis_draw import header_hash
from check_legacy_work import HERE, require


def block_hash(block):
    h = block["header"]
    # These isolated carrier blocks have one empty-input, empty-output coinbase.
    # Its transaction hash is the single-leaf Merkle root.
    [coinbase] = block["transactions"]
    require(coinbase["subnetworkId"] == "01" + "00"*19 and not coinbase["inputs"] and
            not coinbase["outputs"] and coinbase["mass"] == 0, "different carrier transaction layout")
    payload = bytes.fromhex(coinbase["payload"])
    encoded = (struct.pack("<HQQQ", coinbase["version"], 0, 0, coinbase["lockTime"]) +
               bytes.fromhex(coinbase["subnetworkId"]) + struct.pack("<QQ", coinbase["gas"], len(payload)) + payload)
    require(hashlib.blake2b(encoded, key=b"TransactionHash64").hexdigest() == h["hashMerkleRoot"],
            "carrier coinbase differs from its header commitment")
    return header_hash(h, bytes(h["palwCommitment"]), False).hex()


def enrolled_origin(archive):
    raw = subprocess.check_output(["tar", "-xOf", archive, "public.jsonl"])
    return json.loads(raw.splitlines()[0])


def release_message(audit, row, openssl, origin=None):
    round_id, qual, freeze = map(bytes.fromhex, (audit["rounds"][0], audit["qualifications"][0], audit["freeze"]))
    holder = row["holder"]
    require(holder in range(3) and [d for d, _ in row["shares"]] == [0, 1, 2], "unexpected holder or dealers")
    if origin is None:
        origin = enrolled_origin(HERE / "fresh-retained-audit-public.tar.zst")
    require(origin["tip"] == audit["tip"] and origin["registered"] == audit["registered"], "wrong custody enrollment")
    dealer_roots, seals = origin["dealer_roots"][0], origin["custody"][0]
    message = bytearray(sized(round_id) + qual + freeze + u32(holder) + u32(3))
    for d, data in row["shares"]:
        require(len(data) == 66 and int.from_bytes(bytes(data), "big") < (1 << 521)-1, "noncanonical share")
        message.extend(u32(d) + bytes(data))
    original = json.dumps({"roots": dealer_roots, "shares": [data for _, data in row["shares"]]},
                          separators=(",", ":")).encode()
    seal = hashlib.sha512(b"MISAKA/test/TB1/retained-input/v1" + round_id + u32(holder) + original).hexdigest()
    require(seal == seals[holder], "release differs from the original enrolled holder material")
    signature = bytes(row["signature"])
    with tempfile.TemporaryDirectory() as tmp:
        require(verify_signature(openssl, public_keys()[holder], message, signature,
                                 "MISAKA/private/TB1/ShareRelease/v1", Path(tmp)), "wrong holder release signature")
    return bytes(message), signature, dealer_roots, seal


def publication_parent(root, window):
    """Authenticate successive raw blocks from the original covering pruning point."""
    held = json.loads((HERE / "fresh-holder-window-end.json").read_bytes())
    require(held["tip"] == window["tip"], "different independently observed starting state")
    tip, daa, blue, total = window["tip"], window["daa"], window["blue"], window["blocks"]
    extension = root / "extension.jsonl"
    count = 0
    if extension.exists():
        with extension.open("rb") as f:
            for count, line in enumerate(f, 1):
                block = json.loads(line)
                h = block["header"]
                require(h["parentsByLevel"][0] == [tip] and h["blueScore"] == blue + 1 and
                        daa <= h["daaScore"] < window["deadline"] and h["version"] == 2 and h["powAlgoId"] == 8 and
                        len(block["transactions"]) == 1 and block["transactions"][0]["subnetworkId"] == "01" + "00"*19,
                        "different branch, non-heartbeat or expired extension")
                if count == 1:
                    require(h["palwStateRoot"] == held["state"], "wrong root at extension boundary")
                tip, daa, blue = block_hash(block), h["daaScore"], h["blueScore"]
    return dict(tip=tip, daa=daa, blue=blue, blocks=total+count)


def verify_holder(root, window, slot, openssl):
    audit = json.loads((HERE / "fresh-retained-audit.json").read_bytes())["audit"]
    parent = publication_parent(root, window)
    path = root / f"reader-{slot}.log"
    maturity = check_maturity(window, path)
    log = path.read_text()
    [(pid, end)] = records(log, "fresh audit window end")
    require("test result: ok. 1 passed;" in log and end["slot"] == maturity["slot"] == slot and pid == maturity["reader"] and
            end["tip"] == parent["tip"] and end["daa"] == parent["daa"] < window["deadline"] and
            end["blocks"] == parent["blocks"] and end["close"] == audit["close"] and end["publication"] == "ready" and
            f"fresh audit signed holder={slot} tip={parent['tip']} close={audit['close']}" in log,
            "wrong native release position or incomplete holder")
    snapshot = end["committed_snapshot"]["Ok"]
    require(snapshot["generation"] == parent["tip"] and snapshot["finalized"] == window["pruning_header"]["point"],
            "committed publication does not cover the signed close")
    if slot == 0:
        [(start_pid, start), (stop_pid, stop)] = records(log, "signed audit release window")
        require(start_pid == stop_pid == pid and start["close"] == stop["close"] == audit["close"] and
                start["deadline"] == stop["deadline"] == window["deadline"] and
                start["base"] == stop["base"] == start["tip"] == window["tip"] and
                start["blocks"] == 0 and start["publication"] == "held" and
                start["committed_snapshot"]["Ok"]["generation"] == start["tip"] and
                start["committed_snapshot"]["Ok"]["finalized"] is None and stop["publication"] == "ready" and
                stop["tip"] == end["tip"] and stop["blocks"] == parent["blocks"] - window["blocks"] and
                stop["committed_snapshot"]["Ok"] == snapshot, "wrong committed transition from held to ready")
    row = json.loads((root / f"released-{slot}.json").read_bytes())
    require(row["holder"] == slot, "wrong released slot")
    message, _, _, seal = release_message(audit, row, openssl)
    return dict(holder=slot, process=pid, binary_sha256=maturity["binary_sha256"], close=audit["close"],
                deadline=window["deadline"], **parent, state=end["state"], finalized=snapshot["finalized"],
                custody_seal=seal, signed_message_sha256=hashlib.sha256(message).hexdigest())


def arithmetic_holder(root, window, slot, openssl):
    from check_arithmetic_release_window import publication_parent as arithmetic_parent
    audit = json.loads((HERE / "qf1-arithmetic-audit-close.json").read_bytes())["audit"]
    expected_build = json.loads((HERE / "qf1-arithmetic-stream-moe2-book.json").read_bytes())["binary_sha256"]
    parent = arithmetic_parent(root, window)
    path = root / f"holder-{slot}.log"
    maturity = check_maturity(window, path, arithmetic=True)
    log = path.read_text()
    [(pid, end)] = records(log, "fresh audit window end")
    require("test result: ok. 1 passed; 0 failed" in log and pid == maturity["reader"] and
            end["slot"] == maturity["slot"] == slot and maturity["binary_sha256"] == expected_build and
            all(end[k] == v for k, v in parent.items()) and end["close"] == audit["close"] and
            end["publication"] == "ready" and audit["audit_from"] <= end["daa"] < audit["audit_deadline"] and
            log.count(f"fresh audit signed holder={slot} tip={end['tip']} close={end['close']}") == 1,
            "wrong arithmetic signing position, executable or release window")
    snap = end["committed_snapshot"]["Ok"]
    require(snap["generation"] == end["readiness"]["generation"] == parent["tip"] and
            snap["finalized"] == end["readiness"]["finalized"]["finalized"] == window["observations"][-1]["finalized"],
            "different committed finality over the arithmetic close")
    row = json.loads((root / f"released-{slot}.json").read_bytes())
    require(row["holder"] == slot, "wrong arithmetic release slot")
    origin = enrolled_origin(HERE / "qf1-arithmetic-audit-close-public.tar.zst")
    message, signature, _, seal = release_message(audit, row, openssl, origin)
    return dict(holder=slot, process=pid, binary_sha256=expected_build, **parent, finalized=snap["finalized"],
                custody_seal=seal, signed_message_sha256=hashlib.sha256(message).hexdigest(),
                signature_sha256=hashlib.sha256(signature).hexdigest(), maturity=maturity)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--holder", type=int, required=True, choices=range(3))
    parser.add_argument("--arithmetic", action="store_true")
    parser.add_argument("--openssl", default="openssl")
    args = parser.parse_args()
    verify_qualification(args.openssl, arithmetic=args.arithmetic)
    if args.arithmetic:
        window = json.loads((HERE / "qf1-arithmetic-release-window.json").read_bytes())
        result = arithmetic_holder(args.root, window, args.holder, args.openssl)
    else:
        window = json.loads((HERE / "fresh-pruning-transition.json").read_bytes())
        result = verify_holder(args.root, window, args.holder, args.openssl)
    print(json.dumps(result, indent=2))
