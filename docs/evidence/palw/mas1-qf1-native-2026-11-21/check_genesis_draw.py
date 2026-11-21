"""Recompute the isolated 8K draw from its raw algo-6 header and compare retained evidence."""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path

from check_legacy_work import HERE, require


def keyed(key, *fields):
    return hashlib.blake2b(b"".join(fields), key=key.encode(), digest_size=64).digest()


def le64(value):
    return struct.pack("<Q", value)


def header_hash(header, commitment, pre_pow):
    parents = header["parentsByLevel"]
    encoded = bytearray(struct.pack("<HQ", header["version"], len(parents)))
    for level in parents:
        encoded.extend(le64(len(level)) + b"".join(bytes.fromhex(h) for h in level))
    for key in ("hashMerkleRoot", "acceptedIdMerkleRoot", "utxoCommitment"):
        encoded.extend(bytes.fromhex(header[key]))
    timestamp, nonce = (0, 0) if pre_pow else (header["timestamp"], header["nonce"])
    encoded.extend(struct.pack("<QIQBQQ", timestamp, header["bits"], nonce, header["powAlgoId"], header["daaScore"], header["blueScore"]))
    work = bytes.fromhex(header["blueWork"]).lstrip(b"\x00")
    encoded.extend(le64(len(work)) + work)
    for key in ("pruningPoint", "evmPayloadHash", "evmCommitmentRoot", "overlayCommitmentRoot", "palwStateRoot"):
        encoded.extend(bytes.fromhex(header[key]))
    if not pre_pow and commitment:
        encoded.extend(le64(len(commitment)) + commitment)
    return keyed("BlockPrePowHash64" if pre_pow else "BlockHash64", encoded)


def verify(record, capture, producer):
    renewed = json.loads((HERE / "genesis-renew.json").read_text())
    row = json.loads(record.read_bytes())
    h = row["raw"]["header"]
    require(row["base"] == renewed["tip"] and h["parentsByLevel"][0] == [row["base"]], "different renewed branch")
    require((h["version"], h["powAlgoId"], h["palwStateRoot"]) == (2, 6, renewed["state"]), "different lane or parent state")
    wire = bytes(h["palwCommitment"])
    require(wire[:6] == b"PAV2\x06\x00", "expected committed version 6 attempt")
    signature_bytes = 4627
    unsigned = wire[4:-signature_bytes-4]
    require(wire[-signature_bytes-4:-signature_bytes] == struct.pack("<I", signature_bytes), "ML-DSA-87 envelope length")
    domain, challenge, cls, bond = unsigned[2:66], unsigned[66:130], unsigned[130:194], unsigned[194:262]
    require(cls.hex() == renewed["class"], "different registered class")
    key_size, = struct.unpack_from("<I", unsigned, 262)
    root_offset = 266 + key_size + 64
    require(key_size == 2592 and len(unsigned) == root_offset + 5*64 + 20, "unexpected attempt fields")
    artifact, trace, output = (unsigned[i:i+64].hex() for i in range(root_offset, root_offset+192, 64))
    require(artifact == renewed["artifact"], "different registered inventory")
    prefix = "misaka-palw/attempt-v2/"
    original = keyed(prefix + "attempt-id/v1", le64(len(unsigned)), unsigned).hex()
    require(original == row["claim"], "claim does not match the raw unsigned envelope")
    pre_pow = header_hash(h, wire, True)
    require(keyed(prefix+"challenge/v1", domain, pre_pow, le64(h["timestamp"]), le64(h["nonce"]), cls, bond) == challenge, "header challenge mismatch")
    anchor = keyed("misaka-palw/base0/rc-job-anchor/v1", domain, pre_pow, cls, bond, le64(h["nonce"] >> 22))
    view = unsigned[:66] + bytes(64) + unsigned[130:]
    execution = keyed(prefix+"execution-commitment/v3", anchor, le64(len(view)), view)
    price = int.from_bytes(keyed(prefix+"class-ticket/v3", execution)[:16], "little")
    log = producer.read_text()
    [target_hex] = re.findall(r"^\[real-attempt\] unchanged class target ([0-9a-f]{32}),", log, re.M)
    [ticket_hex] = re.findall(r"^\[real-attempt\] completed draw 1 after [\d.]+s, ticket ([0-9a-f]{32})$", log, re.M)
    require(price == int(ticket_hex, 16) <= int(target_hex, 16), "live template class ticket/target")
    require(f"1 executions before accepted claim {original}" in log, "missing native receipt")
    data = capture.read_bytes()
    return dict(base=row["base"], block=header_hash(h, wire, False).hex(), claim=original,
                daa=h["daaScore"], blue_score=h["blueScore"], class_id=cls.hex(), inventory=artifact,
                anchor=anchor.hex(), trace=trace, output=output, execution=unsigned[-64:].hex(), ticket=ticket_hex,
                template_target=target_hex, capture_sha256=hashlib.sha256(data).hexdigest(), capture_bytes=len(data))


def verify_readers(draw, paths, count):
    material = b"".join(bytes.fromhex(draw[k]) for k in ("claim", "anchor", "trace", "output", "execution"))
    transcript = keyed("misaka-test/real-claim-replay/v1", le64(len(material)), material).hex()
    peers = []
    for path in paths:
        log = path.read_text()
        require("test result: ok. 1 passed;" in log, "peer did not complete its native run")
        [(pid, raw)] = re.findall(r"^legacy genesis claim (\d+) (\{.*\})$", log, re.M)
        [binary] = re.findall(r"^test_binary_sha256=([0-9a-f]{64})$", log, re.M)
        native = json.loads(raw)
        for local, archived in (("class", "class_id"), ("artifact", "inventory"), ("tip", "block"), ("base", "base"),
                                ("claim", "claim"), ("daa", "daa"), ("trace", "trace"), ("output", "output"),
                                ("execution", "execution"), ("capture_bytes", "capture_bytes"), ("capture_sha256", "capture_sha256")):
            require(native[local] == draw[archived], "native readback differs for " + local)
        require(native["replay"] == transcript, "re-execution transcript")
        peers.append((int(pid), binary, native["state"], native["phase"]))
    require(len(peers) == len({r[0] for r in peers}) == count, f"{count} distinct completed peer(s) required")
    [common] = {r[1:] for r in peers}
    return dict(processes=[r[0] for r in peers], binary_sha256=common[0], state=common[1], phase=common[2], replay=transcript)


def verify_final_suffix(draw, replay, packed, paths):
    raw = packed.read_bytes()
    archive = json.loads(raw)
    inventory = json.loads((HERE / "genesis-exported-suffix.json").read_text())
    require(hashlib.sha256(raw).hexdigest() == inventory["sha256"], "archived suffix digest")
    tip = draw["block"]
    require((archive["base"], archive["claim"]) == (tip, draw["claim"]), "wrong parent or original claim")
    for block in archive["blocks"]:
        h = block["header"]
        require(h["version"] == 2 and h["powAlgoId"] in (6, 8) and any(bytes.fromhex(h["palwStateRoot"])), "unsupported suffix header")
        require(h["parentsByLevel"][0] == [tip], "discontinuous selected chain")
        tip = header_hash(h, bytes(h["palwCommitment"]), False).hex()
    require(tip == archive["tip"] == inventory["tip"], "wrong end of archived branch")
    common = None
    for pid, path in zip(replay["processes"], paths):
        [(observed, row)] = re.findall(r"^legacy genesis recovered (\d+) (\{.*\})$", path.read_text(), re.M)
        native = json.loads(row)
        require(int(observed) == pid and native["replay"] == replay["replay"], "different numerical replay")
        require(common is None or native == common, "different recovered Gate C state")
        common = native
    require((common["claim"], common["accepted"], common["tip"], common["blocks"], common["daa"], common["state"])
            == (draw["claim"], draw["block"], tip, len(archive["blocks"]), h["daaScore"], inventory["state_root"]),
            "native recovery differs from the signed chain")
    require(common["phase"] == "Final { final_daa: %d }" % common["final_daa"]
            and common["final_daa"] <= common["daa"] < common["certify_by_daa"]
            and common["draws"] == common["approvals"] == common["drawn_daa"] == 0 and common["outcome"] == "None",
            "expected Final waiting for the first draw")
    return common


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("record", "capture", "producer"):
        parser.add_argument(arg, type=Path)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--peer", type=Path, help="one completed native run for this draw")
    group.add_argument("--peers", type=Path, nargs=3, help="three completed native readback logs for the same draw")
    parser.add_argument("--final-suffix", type=Path, help="verify the recovered branch against the same completed native replays")
    args = parser.parse_args()
    if args.final_suffix and not (args.peer or args.peers):
        parser.error("--final-suffix requires native peer logs")
    result = verify(args.record, args.capture, args.producer)
    if args.peer:
        result["reexecution"] = verify_readers(result, [args.peer], 1)
    if args.peers:
        result["reexecution"] = verify_readers(result, args.peers, 3)
    if args.final_suffix:
        result["recovered"] = verify_final_suffix(result, result["reexecution"], args.final_suffix, args.peers or [args.peer])
    print(json.dumps(result, indent=2))
