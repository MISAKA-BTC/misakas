"""Match the isolated control population at the original close to its registration and pinned header."""
import argparse
import hashlib
import json
import subprocess
from pathlib import Path

from check_fresh_audit import records
from check_genesis_draw import header_hash
from check_legacy_work import HERE, require
from check_transaction_root import verify_block_transactions


def verify(directory):
    audit = json.loads((HERE / "qf1-arithmetic-audit-close.json").read_bytes())["audit"]
    archive = HERE / "qf1-arithmetic-audit-close-public.tar.zst"
    wire = subprocess.check_output(["tar", "-xOf", archive, "public.jsonl"])
    meta, *blocks = map(json.loads, wire.splitlines())
    tip, headers, registrations = meta["genesis"], {}, []
    for block in blocks:
        h = block["header"]
        require(h["parentsByLevel"][0] == [tip], "different registration prefix")
        verify_block_transactions(block)
        tip = header_hash(h, bytes(h["palwCommitment"]), False).hex()
        headers[tip] = h
        for tx in block["transactions"][1:]:
            payload = bytes.fromhex(tx["payload"])
            if payload[:3] == b"\x01\x00\x3d" and payload[3:67].hex() == audit["class"]:
                registrations.append(payload)
    require(tip == meta["tip"] == audit["tip"] and len(registrations) == 1, "different registered class history")
    [registration] = registrations
    require(registration[67:131].hex() == audit["inventory"], "different registered artifact")
    # The final fields of PalwTirAdmissionCarriageV1 are the bond outpoint and the ML-DSA-87 signature vector.
    require(len(registration) >= 4699 and int.from_bytes(registration[-4631:-4627], "little") == 4627,
            "different signed registration layout")
    bond = [registration[-4699:-4635].hex(), int.from_bytes(registration[-4635:-4631], "little")]
    key_bytes = (HERE / "isolated-card-public-keys.bin").read_bytes()
    require(len(key_bytes) == 8 * 2592 and bond[1] == 2, "different isolated registrant")
    key_id = hashlib.blake2b(key_bytes[2*2592:3*2592]).hexdigest()
    close = headers[audit["close"]]
    observed, pids = [], []
    for slot in range(3):
        log = (directory / f"registered-{slot}.log").read_text()
        require("test result: ok. 1 passed; 0 failed" in log, "incomplete registration replay")
        [(pid, row)] = records(log, "arithmetic control population")
        require(row["block"] == audit["close"] and close["parentsByLevel"][0] == [row["source"]] == [audit["preparation"]]
                and row["state"] == close["palwStateRoot"] and row["source_daa"] == headers[row["source"]]["daaScore"]
                and row["class"] == audit["class"] and row["registrant"] == bond and row["registrant_key"] == key_id,
                "population differs from the pinned registration or source state")
        bonds = row["bonds"]
        identities = {(b["txid"], b["index"]) for b in bonds}
        require(len(identities) == len(bonds) and tuple(bond) not in identities
                and row["registrant_operator"] not in {b["operator"] for b in bonds}
                and key_id not in {b["key_id"] for b in bonds}
                and row["operator_count"] == len({b["operator"] for b in bonds}), "registrant or duplicate entered the population")
        observed.append(row)
        pids.append(pid)
    require(len(set(pids)) == 3 and observed[0] == observed[1] == observed[2], "different population or duplicate process")
    return dict(processes=pids, snapshot=observed[0])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    print(json.dumps(verify(parser.parse_args().directory), indent=2))
