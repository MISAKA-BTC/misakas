"""Verify both frozen test-harness rosters and READY cuts using the public archive and OpenSSL ML-DSA."""
import argparse
import hashlib
import json
import subprocess
import tempfile
from pathlib import Path

from check_fresh_audit import audit_binding, coinbase_extra
from check_genesis_draw import header_hash
from check_transaction_root import verify_block_transactions
from check_legacy_work import HERE, require


def u32(value):
    return value.to_bytes(4, "big")


def u64(value):
    return value.to_bytes(8, "big")


def sized(value):
    return u32(len(value)) + value


def h512(*fields):
    return hashlib.sha3_512(b"".join(fields)).digest()


def public_keys():
    raw = (HERE / "fresh-holder-public-keys.bin").read_bytes()
    require(len(raw) == 4 * 1952, "incomplete original ML-DSA-65 roster")
    return [raw[i:i+1952] for i in range(0, len(raw), 1952)]


def verify_signature(openssl, key, message, signature, context, directory):
    # SubjectPublicKeyInfo for id-ml-dsa-65 with absent parameters.
    der = bytes.fromhex("308207b2300b0609608648016503040312038207a100") + key
    (directory / "key.der").write_bytes(der)
    (directory / "signature").write_bytes(signature)
    (directory / "message").write_bytes(message)
    result = subprocess.run([openssl, "pkeyutl", "-verify", "-rawin", "-pubin", "-keyform", "DER",
                             "-inkey", "key.der", "-sigfile", "signature", "-in", "message",
                             "-pkeyopt", "context-string:" + context], cwd=directory, capture_output=True)
    return result.returncode == 0


def verify(openssl, arithmetic=False):
    algorithms = subprocess.run([openssl, "list", "-signature-algorithms"], capture_output=True)
    require(algorithms.returncode == 0 and b"ML-DSA-65" in algorithms.stdout,
            "select an OpenSSL with ML-DSA-65 support using --openssl")
    keys = public_keys()
    roster = h512(b"MISAKA/private/TB1/roster/v1", u32(len(keys)),
                  *(x.to_bytes(66, "big") + key for x, key in enumerate(keys, 1)))
    name = "qf1-arithmetic-audit-close" if arithmetic else "fresh-retained-audit"
    record = json.loads((HERE / f"{name}.json").read_bytes())
    audit = record["audit"]
    number = 15 if arithmetic else 13
    archive = HERE / f"{name}-public.tar.zst"
    raw = subprocess.check_output(["tar", "-xOf", archive, "public.jsonl"])
    meta, *blocks = map(json.loads, raw.splitlines())
    tip, by_hash = meta["genesis"], {}
    for blue, block in enumerate(blocks, 1):
        verify_block_transactions(block)
        header = block["header"]
        require(header["parentsByLevel"][0] == [tip] and header["blueScore"] == blue, "disconnected public prefix")
        tip = header_hash(header, bytes(header["palwCommitment"]), False).hex()
        by_hash[tip] = block
    require(tip == audit["tip"] == meta["tip"] and audit["registered"] == meta["registered"] and
            by_hash[audit["close"]]["header"]["parentsByLevel"][0] == [audit["preparation"]],
            "different selected registration or close")
    pair, _, freeze = audit_binding(audit, bytes.fromhex(record["suite"]) if arithmetic else None)
    require(coinbase_extra(by_hash[audit["preparation"]]) == pair and coinbase_extra(by_hash[audit["close"]]) == freeze,
            "paired qualification or candidate freeze differs from the original carriers")
    network = b"testnet-12"
    chain = hashlib.blake2b(len(network).to_bytes(8, "little") + network + bytes.fromhex(meta["genesis"]),
                            key=b"misaka-palw/attempt-v2/network-domain/v1", digest_size=64).digest()
    results = []
    with tempfile.TemporaryDirectory() as tmp:
        directory = Path(tmp)
        for i, purpose in enumerate((b"MODEL_AUDIT", b"PANEL")):
            session = bytes([41+i]) * 64
            close_blue = (by_hash[audit["close"]]["header"]["blueScore"] if i == 0 else 2*audit["audit_deadline"]+100)
            label = ("audit", "panel")[i]
            rid = h512(b"MISAKA/private/TB1/BGW-PQ/round/v1", chain, roster, u64(1), bytes([len(purpose)]), purpose,
                       u64(number+i), bytes([number])*64, u64(close_blue), u64(audit[label+"_from"]), u64(audit[label+"_deadline"]))
            require(rid.hex() == audit["rounds"][i], "round identity differs from the signed schedule")
            roots = [bytes.fromhex(x) for x in meta["dealer_roots"][i]]
            signatures = meta["signatures"][i]
            require(len(roots) == 4 and len(signatures) == 9, "incomplete dealer or READY archive")
            supporters = []
            for j, sig in enumerate(signatures):
                dealer, holder = divmod(j, 3)
                slots = u32(dealer) + u32(holder)
                message = sized(rid) + slots + roots[dealer] + session
                require(verify_signature(openssl, keys[holder], message, bytes(sig), "MISAKA/private/TB1/Ready/v1", directory),
                        f"invalid {label} READY from holder {holder} for dealer {dealer}")
                supporters.append(slots)
            qualified = [0, 1, 2]
            root = h512(b"MISAKA/private/TB1/Qualification/v1", sized(rid), session, u64(3), roster,
                        *(b"\x01" + u64(1) + t + session for t in roots), u32(len(supporters)), *supporters,
                        u32(len(qualified)), *(u32(d) for d in qualified))
            require(root.hex() == audit["qualifications"][i], "signed preparation has another qualification")
            results.append(dict(purpose=purpose.decode(), round=rid.hex(), qualification=root.hex(), ready_signatures=len(supporters),
                                qualified_dealers=qualified))
    return dict(close=audit["close"], roster=roster.hex(), rounds=results)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--openssl", default="openssl", help="OpenSSL with ML-DSA support")
    parser.add_argument("--arithmetic", action="store_true", help="verify the arithmetic MODEL_AUDIT and its later PANEL")
    args = parser.parse_args()
    print(json.dumps(verify(args.openssl, args.arithmetic), indent=2))
