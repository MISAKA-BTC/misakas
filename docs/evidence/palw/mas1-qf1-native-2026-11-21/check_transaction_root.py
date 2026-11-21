"""Recover the transaction commitment of a native post-Crescendo RPC block."""
import hashlib
import struct

from check_legacy_work import require


def sized(data):
    return struct.pack("<Q", len(data)) + data


def transaction_hash(tx):
    encoded = bytearray(struct.pack("<HQ", tx["version"], len(tx["inputs"])))
    for row in tx["inputs"]:
        outpoint = row["previousOutpoint"]
        encoded.extend(bytes.fromhex(outpoint["transactionId"]))
        encoded.extend(struct.pack("<I", outpoint["index"]))
        encoded.extend(sized(bytes.fromhex(row["signatureScript"])))
        encoded.extend(struct.pack("<BQ", row["sigOpCount"], row["sequence"]))
    encoded.extend(struct.pack("<Q", len(tx["outputs"])))
    for row in tx["outputs"]:
        # RPC hex encodes the script version as a big-endian prefix.
        script = bytes.fromhex(row["scriptPublicKey"])
        require(len(script) >= 2, "missing output script version")
        encoded.extend(struct.pack("<QH", row["value"], int.from_bytes(script[:2], "big")))
        encoded.extend(sized(script[2:]))
    encoded.extend(struct.pack("<Q", tx["lockTime"]))
    encoded.extend(bytes.fromhex(tx["subnetworkId"]))
    encoded.extend(struct.pack("<Q", tx["gas"]))
    encoded.extend(sized(bytes.fromhex(tx["payload"])))
    if tx["mass"]:
        encoded.extend(struct.pack("<Q", tx["mass"]))
    return hashlib.blake2b(encoded, key=b"TransactionHash64").digest()


def verify_block_transactions(block):
    level = [transaction_hash(tx) for tx in block["transactions"]]
    require(level, "missing block transactions")
    while len(level) > 1:
        if len(level) % 2:
            level.append(bytes(64))
        level = [hashlib.blake2b(left + right, key=b"MerkleBranchHash64").digest()
                 for left, right in zip(level[::2], level[1::2])]
    require(level[0].hex() == block["header"]["hashMerkleRoot"], "transactions differ from the header commitment")
