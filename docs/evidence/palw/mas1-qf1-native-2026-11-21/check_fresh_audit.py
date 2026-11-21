"""Verify the fresh registered audit, cold holder observations and its first native source of non-floor work."""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path

from check_genesis_draw import header_hash, verify as verify_draw, verify_readers
from check_legacy_work import HERE, require


def records(log, prefix):
    pattern = rf"^(?:test \S+ \.\.\. )?{re.escape(prefix)} (\d+) (\{{.*\}})$"
    return [(int(pid), json.loads(row)) for pid, row in re.findall(pattern, log, re.M)]


def audit_binding(audit, suite=None):
    """Encode the fixed fixture plan at its original close, before either draw."""
    audit_id, panel_id = map(bytes.fromhex, audit["rounds"])
    audit_q, panel_q = map(bytes.fromhex, audit["qualifications"])
    prepare = bytes.fromhex(audit["preparation"])
    paired = hashlib.sha3_512(b"MISAKA/private/MAS1/qualified-evaluation-rounds/v1" +
                              audit_id + audit_q + panel_id + panel_q).digest()
    # Hash64::from_u64_word stores the final word in little-endian order.
    if suite is None:
        suite = bytes(56) + (0xa501).to_bytes(8, "little")
    reservation = bytes(56) + (0xa502).to_bytes(8, "little")
    plan = struct.pack(">IIQI", 1, 2, audit["panel_deadline"] + 1, 1)
    instance = b"".join(bytes.fromhex(audit[k]) for k in ("class", "inventory", "descriptor"))
    candidates = hashlib.sha3_512(b"MISAKA/private/MAS1/non-rewarding-candidates/v1" +
                                  prepare + paired + suite + reservation + plan + instance + struct.pack(">Q", 1)).digest()
    freeze = hashlib.sha3_512(b"MISAKA/private/TB1/mining-freeze/v1" +
                              audit_id + audit_q + prepare + candidates + suite).digest()
    require(freeze.hex() == audit["freeze"], "candidate, suite, budget or paired round differs from freeze")
    return paired, candidates, freeze


def coinbase_extra(block):
    tx = block["transactions"][0]
    require(tx["subnetworkId"] == "01" + "00" * 19, "missing coinbase")
    payload = bytes.fromhex(tx["payload"])
    require(len(payload) >= 19 and len(payload) >= 19 + payload[18], "short coinbase script")
    return payload[19 + payload[18]:]


def verify(root, arithmetic_close=False):
    archive, *blocks = [json.loads(x) for x in (root / "public.jsonl").read_text().splitlines()]
    require(archive["floor"] is None and archive["count"] == len(blocks) and
            [len(rows) for rows in archive["signatures"]] == [9, 9], "incomplete public audit")
    require(all(len(sig) == 3309 for group in archive["signatures"] for sig in group), "invalid READY encoding")
    custody = archive["custody"]
    require([len(role) for role in custody] == [3, 3]
            and all(re.fullmatch(r"[0-9a-f]{128}", seal) for role in custody for seal in role)
            and len({seal for role in custody for seal in role}) == 6, "invalid holder/role commitments")
    writer = root / "writer.log"
    readers = [root / f"reader-{slot}.log" for slot in range(3)]
    pids, binaries = [], set()
    common = None
    renewed = None
    for i, path in enumerate([writer, *readers]):
        log = path.read_text()
        [binary] = re.findall(r"^test_binary_sha256=([0-9a-f]{64})$", log, re.M)
        require("test result: ok. 1 passed; 0 failed" in log, "incomplete native run")
        [(pid, row)] = records(log, "fresh retained audit")
        slot = 0 if i == 0 else i-1
        require(row.pop("cold") == (i > 0) and row.pop("slot") == slot and
                row.pop("seals") == [role[slot] for role in archive["custody"]], "different slot or custody")
        require(row["publication"] == row["signing"] == "held" and len(set(row["rounds"])) == len(set(row["qualifications"])) == 2,
                "overlapping purposes or unexpected release")
        pids.append(pid)
        binaries.add(binary)
        common = row if common is None else common
        require(common == row, "native registered cut differs")
        if arithmetic_close:
            continue
        [(seat_pid, seats), (renew_pid, later)] = records(log, "legacy genesis boundary")
        [(capital_pid, capital)] = records(log, "legacy genesis capital")
        [(initial_pid, initial)] = records(log, "legacy-work result")
        require(pid == seat_pid == capital_pid == renew_pid == initial_pid, "mixed native receipts")
        facts = dict(initial=initial, seats=seats, capital=capital, renewed=later)
        renewed = facts if renewed is None else renewed
        require(renewed == facts, "forked native seat or capital history")
    require(len(set(pids)) == 4 and len(binaries) == 1, "expected one producer and three cold processes on the same build")
    candidate = json.loads((HERE / "qf1-native-tile-artifacts.json").read_bytes())["full"]
    scope = json.loads((HERE / "g-cost-shared-prefix-run.jsonl").read_text().splitlines()[0])
    require((common["class"], common["inventory"], common["descriptor"]) ==
            (candidate["class_id"], candidate["inventory"], scope["descriptor"]), "different integer model")
    previous = json.loads((HERE / "genesis-work-maturity.json").read_bytes())
    require(common["close"] != previous["close"] and common["audit_deadline"] > previous["forecast_maturity"] and
            common["audit_from"] <= common["now"] < common["audit_deadline"] < common["panel_from"] < common["panel_deadline"],
            "stale close or incompatible windows")

    tip, blue = archive["genesis"], 0
    checkpoints = {}
    wanted = {common[k] for k in ("registered", "preparation", "close", "tip")}
    def walk(blocks):
        nonlocal tip, blue
        for b in blocks:
            h = b["header"]
            require(h["parentsByLevel"][0] == [tip] and h["blueScore"] == blue+1, "disconnected selected branch")
            tip, blue = header_hash(h, bytes(h["palwCommitment"]), False).hex(), h["blueScore"]
            if tip in wanted:
                checkpoints[tip] = (b, blue)
    walk(blocks)
    require(tip == archive["tip"] == common["tip"] and archive["registered"] == common["registered"] and
            len(checkpoints) == 4, "audit prefix not fully covered")
    suite = None
    if arithmetic_close:
        spec = (b"MISAKA/private/MAS1/arithmetic/v1\0HEX64\0a=c[0]%5,b=c[1]%5\0add(a,b),sub(a+b,b)\0"
                b"Answer with only one digit: what is \0 + \0 - \0?\0canonical-digit-token,one-QF1-EOS\0required-correct=2\0")
        suite = hashlib.sha3_512(spec + struct.pack(">IIQ", 2, 16, 32_000_000)).digest()
        legacy = json.loads((HERE / "fresh-retained-audit.json").read_bytes())["audit"]
        require(set(legacy["rounds"]).isdisjoint(common["rounds"]), "reused legacy audit or panel round")
    paired, _, freeze = audit_binding(common, suite)
    require(coinbase_extra(checkpoints[common["preparation"]][0]) == paired, "different qualified evaluation rounds")
    close = checkpoints[common["close"]][0]
    require(close["header"]["parentsByLevel"][0] == [common["preparation"]] and
            coinbase_extra(close) == freeze, "different close commitment")

    if arithmetic_close:
        return dict(audit=common, suite=suite.hex(), required_correct=2, cases=2, decode_limit=16, max_leaves=32_000_000,
                    holder_processes=pids, binary_sha256=binaries.pop(), public_prefix_blocks=archive["count"],
                    archive_sha256=hashlib.sha256((root/"public.jsonl").read_bytes()).hexdigest())
    state = common["state"]
    for name, stage in (("capital", "initial"), ("seats", "seats"), ("genesis-capital", "capital"), ("renew", "renewed")):
        ext = json.loads((root / f"{name}.json").read_bytes())
        require(ext["base"] == tip, "wrong extension basis")
        require(ext["blocks"][0]["header"]["palwStateRoot"] == state, "wrong parent state for " + name)
        row = renewed[stage]
        if stage == "initial":
            require(ext["close"] == row["close"] == common["close"] and ext["tip"] == row["tip"], "wrong audit origin")
            readiness = row["readiness"]
            require(row["publication_refusal"] == "WouldBlock" and readiness["finalized"]["finalized"] is None
                    and readiness["finalized"]["pruningPoint"] == archive["genesis"], "unexpected release or finalized block")
        else:
            require(row["base"] == tip, "detached native receipt for " + name)
        walk(ext["blocks"])
        walk(ext.get("boundary_blocks", []))
        require(tip == row["tip"], f"wrong tip for {name}")
        state = row["state"]
    require(renewed["renewed"]["ready"] == 8 and renewed["renewed"]["available_microblocks"] > 0
            and renewed["capital"]["assigned_sompi"] == renewed["capital"]["collateral"] // 2 > 0,
            "missing native capacity or paid seats")
    draw = verify_draw(root / "accepted/winning.json", root / "accepted/capture.bin", writer, renewed["renewed"])
    manifest = HERE.parents[3] / "consensus/core/src/config/class-manifests/qwen25-1.5b-a16-8k.palwmanifest"
    [genesis_model] = json.loads(manifest.read_text())["classes"]
    require((draw["class_id"], draw["inventory"]) == (genesis_model["class_id"], genesis_model["inventory_root"]),
            "different genesis model")
    require(draw["base"] == tip and draw["class_id"] != common["class"] and draw["daa"] < common["audit_deadline"],
            "wrong 8K work basis or timing")
    replay = verify_readers(draw, readers, 3)
    require(replay["processes"] == pids[1:] and replay["phase"] == "Provisional"
            and replay["binary_sha256"] in binaries, "different cold readers or claim phase")
    return dict(audit=common, holder_processes=pids, binary_sha256=next(iter(binaries)),
                public_prefix_blocks=archive["count"], ready_signatures=sum(map(len, archive["signatures"])),
                holder_commitments=archive["custody"], capital_sompi=renewed["capital"]["assigned_sompi"],
                available_microblocks=renewed["renewed"]["available_microblocks"], ready_seats=renewed["renewed"]["ready"],
                native_8k_draw=draw, cold_8k_replay=replay,
                archive_sha256=hashlib.sha256((root/"public.jsonl").read_bytes()).hexdigest())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, help="extracted public evidence")
    parser.add_argument("--arithmetic-close", action="store_true")
    args = parser.parse_args()
    print(json.dumps(verify(args.root, args.arithmetic_close), indent=2))
