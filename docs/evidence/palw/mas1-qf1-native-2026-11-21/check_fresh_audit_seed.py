"""Reconstruct the isolated MODEL_AUDIT seed from its three public releases and compare the native readers."""
import argparse
import hashlib
import json
from pathlib import Path

from check_fresh_audit import audit_binding, coinbase_extra, records
from check_fresh_qualification import h512, sized, u32, verify as verify_qualification
from check_fresh_window import check_maturity
from check_fresh_holder_release import block_hash, release_message, verify_holder
from check_legacy_work import HERE, require


def reconstruct(audit, pub, openssl):
    _, root, _ = audit_binding(audit)
    round_id, qual, freeze = map(bytes.fromhex, (audit["rounds"][0], audit["qualifications"][0], audit["freeze"]))
    context = sized(round_id) + qual + freeze
    rows = pub["rows"]
    require([row["holder"] for row in rows] == [0, 1, 2], "missing or duplicate release")
    batch_message = bytearray(b"MISAKA/private/TB1/signed-batch/v1" + u32(3))
    points = [[] for _ in range(3)]
    p = (1 << 521) - 1
    for row in rows:
        message, signature, dealer_roots, _ = release_message(audit, row, openssl)
        for d, data in row["shares"]:
            points[d].append(int.from_bytes(bytes(data), "big"))
        batch_message.extend(sized(message) + signature)
    batch = block_hash(pub["batch"])
    require(coinbase_extra(pub["batch"]) == h512(batch_message), "different committed release batch")
    seed_material = bytearray(sized(b"PALW-TB1/beacon") + context + u32(3))
    cert = bytearray(b"MISAKA/private/TB1/batch-certificate/v1" + bytes.fromhex(batch) + context + u32(3))
    for dealer, (a, b, c) in enumerate(points):
        slope = (b - a) % p
        require((b + slope) % p == c, "inconsistent third holder")
        constant = ((a - slope) % p).to_bytes(66, "big")
        coefficients = constant + slope.to_bytes(66, "big")
        expected = hashlib.sha512(b"MISAKA/test/TB1/dealer-polynomial/v1" + round_id + u32(dealer) + coefficients)
        require(expected.hexdigest() == dealer_roots[dealer], "recovered polynomial differs from original dealer commitment")
        seed_material.extend(u32(dealer) + constant)
        cert.extend(u32(dealer) + u32(2) + coefficients + u32(3) + u32(0) + u32(1) + u32(2))
    require(coinbase_extra(pub["certificate"]) == h512(cert) and pub["certificate"]["header"]["parentsByLevel"][0] == [batch],
            "wrong polynomial certificate or carrier")
    seed = hashlib.shake_256(seed_material).digest(32)
    cases = [hashlib.sha3_256(b"MISAKA/private/MAS1/evaluation-challenge/v1" + root + seed + u32(0) + u32(i)).hexdigest()
             for i in range(2)]
    return dict(close=audit["close"], batch=batch, certificate=block_hash(pub["certificate"]), seed=seed.hex(), cases=cases)


def verify(root, window, openssl, publisher_only=False):
    audit = json.loads((HERE / "fresh-retained-audit.json").read_bytes())["audit"]
    require(window["close"] == audit["close"], "different registered audit")
    pub = json.loads((root / "publication.json").read_bytes())
    holders = [verify_holder(root, window, i, openssl) for i in range(3)]
    common = holders[0]
    for field in ("binary_sha256", "close", "tip", "state", "finalized", "daa", "blue", "deadline"):
        require(all(row[field] == common[field] for row in holders), "different holder " + field)
    require(len({row["process"] for row in holders}) == 3, "duplicate release process")
    require(pub["rows"] == [json.loads((root / f"released-{i}.json").read_bytes()) for i in range(3)],
            "publication differs from signed original releases")
    batch, cert = (pub[k]["header"] for k in ("batch", "certificate"))
    require(batch["parentsByLevel"][0] == [common["tip"]] and batch["palwStateRoot"] == common["state"] and
            common["daa"] <= batch["daaScore"] <= cert["daaScore"] < common["deadline"] and
            (batch["blueScore"], cert["blueScore"]) == (common["blue"]+1, common["blue"]+2),
            "wrong publication parent, state, height or original deadline")
    expected = reconstruct(audit, pub, openssl)
    slots = (2,) if publisher_only else (2, 0, 1)
    readers, states = {}, set()
    for slot in slots:
        path = root / ("reader-2.log" if slot == 2 else f"publication-{slot}.log")
        maturity = check_maturity(window, path)
        log = path.read_text()
        [(pid, native)] = records(log, "fresh audit certificate")
        [(end_pid, end)] = records(log, "fresh audit window end")
        snapshot = end["committed_snapshot"]["Ok"]
        require("test result: ok. 1 passed; 0 failed" in log and
                pid == end_pid == maturity["reader"] and native["slot"] == end["slot"] == maturity["slot"] == slot and
                maturity["binary_sha256"] == common["binary_sha256"] and
                end["publication"] == "ready" and end["blocks"] == common["blocks"] and end["daa"] == common["daa"] and
                end["state"] == common["state"] and end["close"] == common["close"] and
                end["tip"] == native["tip"] == snapshot["generation"] == common["tip"] and
                snapshot["finalized"] == common["finalized"], "incomplete or mismatched native publication reader")
        require(all(native[k] == v for k, v in expected.items()), "native seed, cases or carriers differ from releases")
        require(pid == holders[2]["process"] if slot == 2 else pid not in {r["process"] for r in holders},
                "expected original publisher and fresh replay processes")
        readers[slot] = pid
        states.add(native["state"])
    require(len(set(readers.values())) == len(slots) and len(states) == 1, "different states or duplicate reader")
    return dict(**expected, state=states.pop(), publication_readers=readers, holders=[r["process"] for r in holders],
                binary_sha256=common["binary_sha256"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path)
    parser.add_argument("--publisher-only", action="store_true")
    parser.add_argument("--openssl", default="openssl")
    args = parser.parse_args()
    verify_qualification(args.openssl)
    window = json.loads((HERE / "fresh-pruning-transition.json").read_bytes())
    result = verify(args.root, window, args.openssl, args.publisher_only)
    print(json.dumps(result, indent=2))
