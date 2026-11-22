"""Check the heartbeat branch between the published evaluation and its scheduled PANEL close."""
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

from check_fresh_audit import records
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


def verify(prefix, log):
    published = json.loads((HERE / "qf1-arithmetic-publisher.json").read_bytes())
    text = log.read_text()
    require("test result: ok. 1 passed; 0 failed" in text, "incomplete native prefix run")
    [row] = re.findall(r"^arithmetic PANEL prefix pid=\d+ submission=([a-f0-9]{128}) parent=([a-f0-9]{128}) "
                       r"round=([a-f0-9]{128}) blue=(\d+) daa=(\d+)$", text, re.M)
    submission, parent, panel_round, blue, daa = row
    require(submission == published["submission"], "different evaluation submission")
    qualification = json.loads((HERE / "qf1-arithmetic-qualification.json").read_bytes())
    prepared = json.loads((HERE / "qf1-arithmetic-panel-preparation.json").read_bytes())
    check_assessment_schedule(published, qualification, prepared, panel_round)
    with prefix.open("rb") as stream:
        report = check_prefix(stream, submission, published["assessment_position"], parent)
    require((report["blue"], report["daa"]) == (int(blue), int(daa)), "native position differs from header")
    return dict(submission=submission, native_round=panel_round, **report)


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
            known.append(dict(txid=bond["txid"], index=bond["index"], operator=bond["operator"], card=card))
    return known


def check_submission_state(row, submitted, seed):
    archive = HERE / "qf1-arithmetic-publisher.tar.zst"
    publication = json.loads(subprocess.check_output(["tar", "-xOf", archive, "publication.json"]))
    block, _ = json.loads(subprocess.check_output(["tar", "-xOf", archive, "publication.submission.json"]))
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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    parser.add_argument("native_log", type=Path)
    parser.add_argument("--control-population", action="store_true", help="also verify the cold reader’s live parent population observation")
    args = parser.parse_args()
    result = verify(args.prefix, args.native_log)
    if args.control_population:
        result = control_observations(result, args.native_log)
    print(json.dumps(result, indent=2))
