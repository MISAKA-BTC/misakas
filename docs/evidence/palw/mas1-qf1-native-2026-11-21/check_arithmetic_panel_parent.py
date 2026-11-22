"""Check the heartbeat branch between the published evaluation and its scheduled PANEL close."""
import argparse
import hashlib
import json
import re
from pathlib import Path

from check_genesis_draw import header_hash
from check_legacy_work import HERE, require
from check_transaction_root import verify_block_transactions


def check_prefix(stream, tip, position, expected_parent):
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
    require(blue == target and tip == expected_parent, "missing or different PANEL parent")
    require(daa < release_from, "parent already at or beyond the first permissible share release")
    return dict(parent=tip, blue=blue, daa=daa, blocks=blue-position["submission_blue"], archive_sha256=digest.hexdigest())


def verify(prefix, log):
    published = json.loads((HERE / "qf1-arithmetic-publisher.json").read_bytes())
    text = log.read_text()
    require("test result: ok. 1 passed; 0 failed" in text, "incomplete native prefix run")
    [row] = re.findall(r"^arithmetic PANEL prefix pid=\d+ submission=([a-f0-9]{128}) parent=([a-f0-9]{128}) "
                       r"round=([a-f0-9]{128}) blue=(\d+) daa=(\d+)$", text, re.M)
    submission, parent, panel_round, blue, daa = row
    require(submission == published["submission"], "different evaluation submission")
    qualification = json.loads((HERE / "qf1-arithmetic-qualification.json").read_bytes())
    [panel] = [r for r in qualification["rounds"] if r["purpose"] == "PANEL"]
    require(qualification["close"] == published["close"] and panel_round == panel["round"],
            "different prequalified evaluation PANEL")
    with prefix.open("rb") as stream:
        report = check_prefix(stream, submission, published["assessment_position"], parent)
    require((report["blue"], report["daa"]) == (int(blue), int(daa)), "native position differs from header")
    return dict(submission=submission, native_round=panel_round, **report)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    parser.add_argument("native_log", type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.prefix, args.native_log), indent=2))
