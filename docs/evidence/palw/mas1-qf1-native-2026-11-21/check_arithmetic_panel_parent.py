"""Check the heartbeat branch between the published evaluation and its scheduled PANEL close."""
import argparse
import hashlib
import json
import re
import struct
import subprocess
from pathlib import Path

from check_fresh_audit import audit_binding, coinbase_extra, records
from check_genesis_draw import header_hash
from check_legacy_work import HERE, require
from check_transaction_root import verify_block_transactions


def walk_prefix(stream, tip, position):
    blue = position["submission_blue"]
    daa = position["submission_daa"]
    target = position["panel_close_blue"] - 1
    release_from = position["panel_release_from_daa"]
    require(release_from < position["panel_deadline_exclusive_daa"], "empty PANEL release window")
    digest = hashlib.sha256()
    for line in stream:
        digest.update(line)
        block = json.loads(line)
        h = block["header"]
        require(h["parentsByLevel"][0] == [tip] and h["blueScore"] == blue + 1 <= target,
                "disconnected or excess PANEL prefix")
        require(daa <= h["daaScore"] and h["powAlgoId"] == 8 and h["version"] == 2
                and len(block["transactions"]) == 1 and block["transactions"][0]["subnetworkId"] == "01" + "00"*19,
                "unexpected PANEL heartbeat")
        verify_block_transactions(block)
        tip = header_hash(h, bytes(h["palwCommitment"]), False).hex()
        blue, daa = h["blueScore"], h["daaScore"]
        require(daa < release_from, "parent already at or beyond the first permissible share release")
        yield tip, blue, daa, digest.hexdigest()


def check_prefix(stream, tip, position, expected_parent):
    blue, daa = position["submission_blue"], position["submission_daa"]
    archive_sha256 = hashlib.sha256().hexdigest()
    for tip, blue, daa, archive_sha256 in walk_prefix(stream, tip, position):
        pass
    require(blue == position["panel_close_blue"] - 1 and tip == expected_parent, "missing or different PANEL parent")
    require(daa < position["panel_release_from_daa"], "parent already at or beyond the first permissible share release")
    return dict(parent=tip, blue=blue, daa=daa, blocks=blue-position["submission_blue"], archive_sha256=archive_sha256)


def check_assessment_schedule(published, qualification, prepared, panel_round):
    [panel] = [r for r in qualification["rounds"] if r["purpose"] == "PANEL"]
    require(prepared["close"] == qualification["close"] == published["close"]
            and prepared["round"] == panel["round"] == panel_round
            and prepared["qualification"] == panel["qualification"], "different prequalified evaluation PANEL")
    position = published["assessment_position"]
    require((position["panel_close_blue"], position["panel_release_from_daa"], position["panel_deadline_exclusive_daa"])
            == (prepared["close_blue"], prepared["release_from"], prepared["deadline_exclusive"]),
            "evaluation prefix bounds differ from the qualified PANEL schedule")


def check_native_submission(log_text, pid, published, originals):
    observed = re.findall(r"^fresh arithmetic submission pid=(\d+) certificate=([a-f0-9]{128}) "
                          r"block=([a-f0-9]{128}) marker=([a-f0-9]{128})$", log_text, re.M)
    require(observed == [(pid, published["certificate"], published["submission"], published["marker"])],
            "native process did not replay the original evaluation submission")
    evaluated = [json.loads(raw) for raw in re.findall(r"^fresh arithmetic evaluation (\{.*\})$", log_text, re.M)]
    require(len(originals) == len(published["results"]), "evaluation case count differs from the published report")
    expected = [dict(pid=int(pid), certificate=published["certificate"], evaluation=row, correct=score)
                for row, score in zip(originals, published["results"])]
    require(evaluated == expected, "native numerical replay differs from the archived evaluation cases")


def recovery_prefix_start(text, pid, submission, position):
    """Resolve the first parent of the trailing heartbeat file after native work replay."""
    lines = [line for line in text.splitlines() if line.startswith("arithmetic PANEL recovery work replay ")]
    if not lines:
        return submission, position, {}
    require(len(lines) == 1, "ambiguous recovery branch")
    replay = json.loads(lines[0].split("replay ", 1)[1])
    pinned = json.loads((HERE / "round16-recovery-inputs.json").read_bytes())
    base = pinned["branch_parent"]
    require(replay["pid"] == int(pid) and pinned["submission"] == submission
            and pinned["assessment_position"] == position and replay["base"] == base["block"],
            "recovery belongs to a different process, submission, parent or schedule")
    for field in ("claim", "accepted", "tip"):
        require(re.fullmatch(r"[a-f0-9]{128}", replay[field]) and int(replay[field], 16) > 0,
                "missing native recovery identity")
    require(replay["tip"] != replay["base"]
            and base["blue"] < replay["blue"] < position["panel_close_blue"]
            and base["daa"] <= replay["certified_daa"] <= replay["daa"] < position["panel_release_from_daa"]
            and replay["approvals"] > 0, "recovery outside certified pre-close history")
    start = dict(position, submission_blue=replay["blue"], submission_daa=replay["daa"])
    return replay["tip"], start, dict(prefix_start=replay["tip"], recovery={k: v for k, v in replay.items() if k != "pid"})


def verify(prefix, log):
    published = json.loads((HERE / "qf1-arithmetic-publisher.json").read_bytes())
    text = log.read_text()
    require("test result: ok. 1 passed; 0 failed" in text, "incomplete native prefix run")
    [row] = re.findall(r"^arithmetic PANEL prefix pid=(\d+) submission=([a-f0-9]{128}) parent=([a-f0-9]{128}) "
                       r"round=([a-f0-9]{128}) blue=(\d+) daa=(\d+)$", text, re.M)
    pid, submission, parent, panel_round, blue, daa = row
    require(submission == published["submission"], "different evaluation submission")
    _, originals = read_submission_archive()
    check_native_submission(text, pid, published, originals)
    qualification = json.loads((HERE / "qf1-arithmetic-qualification.json").read_bytes())
    prepared = json.loads((HERE / "qf1-arithmetic-panel-preparation.json").read_bytes())
    check_assessment_schedule(published, qualification, prepared, panel_round)
    start, position, recovery = recovery_prefix_start(text, pid, submission, published["assessment_position"])
    with prefix.open("rb") as stream:
        report = check_prefix(stream, start, position, parent)
    require((report["blue"], report["daa"]) == (int(blue), int(daa)), "native position differs from header")
    return dict(submission=submission, native_round=panel_round, **recovery, **report)


def check_control_identity(row, registered):
    for field in ("class", "base_class", "registrant", "registrant_key", "registrant_operator"):
        require(row[field] == registered[field], "control population differs from the signed registration")
    bonds = row["bonds"]
    outpoints = {(b["txid"], b["index"]) for b in bonds}
    require(len(bonds) == len(outpoints) and tuple(registered["registrant"]) not in outpoints
            and all(b["operator"] != registered["registrant_operator"] and b["key_id"] != registered["registrant_key"]
                    and type(b["collateral"]) is int and b["collateral"] > 0 for b in bonds)
            and row["operator_count"] == len({b["operator"] for b in bonds}), "invalid or self-selected control bond")


def isolated_card_keys():
    keys = (HERE / "isolated-card-public-keys.bin").read_bytes()
    width = 2592  # ML-DSA-87 verification key
    require(len(keys) == 8 * width, "incomplete isolated verification keys")
    identities = {}
    for card in range(8):
        key = keys[card*width:(card+1)*width]
        key_id = hashlib.blake2b(key).hexdigest()
        identities[key_id] = card
    require(len(identities) == 8, "duplicate isolated key")
    return identities


def identify_control_cards(snapshot, identities):
    # A bond may use a key outside this fixture. Preserve those rows in the native snapshot.
    known = []
    for bond in snapshot["bonds"]:
        card = identities.get(bond["key_id"])
        if card is not None:
            known.append(dict(txid=bond["txid"], index=bond["index"], operator=bond["operator"], card=card, key_id=bond["key_id"]))
    return known


def read_submission_archive():
    archive = HERE / "qf1-arithmetic-publisher.tar.zst"
    return json.loads(subprocess.check_output(["tar", "-xOf", archive, "publication.submission.json"]))


def check_submission_state(row, submitted, seed):
    archive = HERE / "qf1-arithmetic-publisher.tar.zst"
    publication = json.loads(subprocess.check_output(["tar", "-xOf", archive, "publication.json"]))
    block, _ = read_submission_archive()
    certificate = publication["certificate"]
    for raw, expected in ((certificate, seed["certificate"]), (block, submitted)):
        verify_block_transactions(raw)
        h = raw["header"]
        require(header_hash(h, bytes(h["palwCommitment"]), False).hex() == expected,
                "archive differs from the original evaluation branch")
    h = block["header"]
    require(row["block"] == submitted and h["parentsByLevel"][0] == [row["source"]] == [seed["certificate"]]
            and row["source_daa"] == certificate["header"]["daaScore"]
            and row["state"] == h["palwStateRoot"] == seed["state"],
            "population differs from the state committed by the submitted block")


def control_observations(report, log):
    text = log.read_text()
    [(pid, submission)] = records(text, "arithmetic control population")
    [(parent_pid, parent)] = records(text, "arithmetic assessment parent state")
    require(pid == parent_pid and f"arithmetic PANEL prefix pid={pid} submission={report['submission']} " in text,
            "population observation belongs to another process")
    seed = json.loads((HERE / "qf1-arithmetic-publisher-seed.json").read_bytes())
    registered = json.loads((HERE / "qf1-arithmetic-registration-population.json").read_bytes())["snapshot"]
    for row in (submission, parent):
        check_control_identity(row, registered)
    check_submission_state(submission, report["submission"], seed)
    require(parent["source"] == report["parent"] and parent["source_daa"] == report["daa"]
            and "block" not in parent and re.fullmatch(r"[0-9a-f]{128}", parent["state"]),
            "different live pre-close state observation")
    identities = isolated_card_keys()
    require(identities.get(registered["registrant_key"]) == 2, "different signing card for the original registration")
    return dict(**report, process=pid, at_submission=submission, at_parent=parent,
                known_cards_at_submission=identify_control_cards(submission, identities),
                known_cards_at_parent=identify_control_cards(parent, identities))


def control_operator_weights(bonds):
    sompi_per_msk = 100_000_000
    max_weight_msk = 1_000_000  # ADR-0152's V1 operator cap
    posted = {}
    for bond in bonds:
        operator = bond["operator"]
        posted[operator] = posted.get(operator, 0) + bond["collateral"] // sompi_per_msk
    return [(operator, max(1, min(weight, max_weight_msk))) for operator, weight in sorted(posted.items())]


def assessment_commitment(report, published, plan, seats):
    audit = plan["audit"]
    _, origin, _ = audit_binding(audit, bytes.fromhex(plan["suite"]))
    state = report["at_parent"]["state"]
    weights = control_operator_weights(report["at_parent"]["bonds"])
    require(len(weights) >= seats, "insufficient eligible control operators")
    roster = hashlib.sha3_512(b"MISAKA/private/MAS1/arithmetic-panel-roster/v1" + bytes.fromhex(state)
                              + struct.pack(">I", len(weights)))
    for operator, weight in weights:
        roster.update(bytes.fromhex(operator) + struct.pack(">Q", weight))
    consumer = hashlib.sha3_512(b"MISAKA/private/MAS1/arithmetic-panel-inputs/v1" + origin
                                + bytes.fromhex(published["certificate"]) + struct.pack(">I", 0)
                                + bytes.fromhex(published["submission"]) + bytes.fromhex(published["marker"])
                                + roster.digest()).digest()
    policy = hashlib.sha3_512(b"MISAKA/private/MAS1/arithmetic-panel/stake-ticket/full-replay/v1"
                              + struct.pack(">I", seats)).digest()
    freeze = hashlib.sha3_512(b"MISAKA/private/TB1/mining-freeze/v1" + bytes.fromhex(audit["rounds"][1])
                              + bytes.fromhex(audit["qualifications"][1]) + bytes.fromhex(report["parent"])
                              + consumer + policy).hexdigest()
    return policy.hex(), freeze, weights


def check_close(report, native_log, raw_close, seats):
    published = json.loads((HERE / "qf1-arithmetic-publisher.json").read_bytes())
    pos = published["assessment_position"]
    [(pid, row)] = records(native_log.read_text(), "arithmetic PANEL close")
    require(pid == report["process"], "closure from another process")
    block = json.loads(raw_close.read_bytes())
    verify_block_transactions(block)
    h = block["header"]
    close = header_hash(h, bytes(h["palwCommitment"]), False).hex()
    require(h["parentsByLevel"][0] == [report["parent"]] == [row["parent"]]
            and h["blueScore"] == row["blue"] == pos["panel_close_blue"]
            and report["daa"] <= h["daaScore"] == row["daa"] < pos["panel_release_from_daa"]
            and row["block"] == close and row["round"] == report["native_round"],
            "closure is outside the confirmed PANEL schedule")
    require(h["palwStateRoot"] == row["state"] == report["at_parent"]["state"]
            and coinbase_extra(block).hex() == row["commitment"],
            "closure does not commit to the observed parent state and marker")
    require(len(report["known_cards_at_parent"]) == len(report["at_parent"]["bonds"]),
            "control bond without a verification key in the original isolated inventory")
    plan = json.loads((HERE / "qf1-arithmetic-audit-close.json").read_bytes())
    policy, marker, weights = assessment_commitment(report, published, plan, seats)
    require((row["policy"], row["commitment"]) == (policy, marker), "different control roster, policy or assessment commitment")
    return dict(**report, assessment_close=row, control_operator_weights_msk=weights)


def prefix_run(log):
    text = log.read_text()
    [pid] = re.findall(r"^arithmetic PANEL prefix pid=(\d+) ", text, re.M)
    [binary] = re.findall(r"^test_binary_sha256=([a-f0-9]{64})$", text, re.M)
    return dict(pid=int(pid), binary_sha256=binary)


def agree_on_parent(prefix, writer_log, reader_log):
    writer = prefix_run(writer_log)
    reader = prefix_run(reader_log)
    require(writer["pid"] != reader["pid"], "writer and reader report the same process")
    report = verify(prefix, reader_log)
    require(verify(prefix, writer_log) == report, "writer and reader reproduced different prefixes")
    return dict(**report, writer=writer, reader=reader)


def agree_on_close(prefix, raw_close, logs, seats):
    runs = [prefix_run(log) for log in logs]
    require(len({run["pid"] for run in runs}) == len(logs), "duplicate native process in close evidence")
    require(len({run["binary_sha256"] for run in runs}) == 1, "assessment close was not replayed by the same build")
    agreed = None
    for log in logs:
        report = check_close(control_observations(verify(prefix, log), log), log, raw_close, seats)
        del report["process"]
        if agreed is None:
            agreed = report
        require(report == agreed, "peers accepted different assessment close or control state")
    return dict(**agreed, close_processes=[run["pid"] for run in runs], close_binary_sha256=runs[0]["binary_sha256"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    parser.add_argument("native_log", type=Path)
    parser.add_argument("--control-population", action="store_true", help="also verify the cold reader’s live parent population observation")
    parser.add_argument("--writer-log", type=Path, help="also require agreement with a separate native writer")
    parser.add_argument("--close", type=Path, help="verify the archived child and its pinned control state")
    parser.add_argument("--peer-log", type=Path, action="append", default=[], help="additional native reader of the same close")
    parser.add_argument("--seats", type=int, default=2, help="seat count fixed for the isolated assessment close")
    args = parser.parse_args()
    if not 0 < args.seats < 1 << 32:
        parser.error("--seats must be a positive u32")
    if args.peer_log and not args.close:
        parser.error("--peer-log requires --close")
    if args.close:
        logs = [args.native_log, *args.peer_log]
        if args.writer_log:
            logs.insert(0, args.writer_log)
        print(json.dumps(agree_on_close(args.prefix, args.close, logs, args.seats), indent=2))
        raise SystemExit(0)
    result = (agree_on_parent(args.prefix, args.writer_log, args.native_log) if args.writer_log
              else verify(args.prefix, args.native_log))
    if args.control_population:
        result = control_observations(result, args.native_log)
    print(json.dumps(result, indent=2))
