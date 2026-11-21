"""Verify the two stored public replies against the native QF1 replay and its accepted marker."""
import argparse
import json
import re
import tempfile
from pathlib import Path

from check_fresh_qualification import h512, verify_signature
from check_legacy_work import HERE, require
from check_prediction import shared_rows
from check_real_lifecycle import check

DOMAIN = b"MISAKA/private/TB1/QF1/complete-replay/v1"


def verify(log, openssl, *, checkpoints=False, cold_readers=()):
    full = json.loads((HERE / "qf1-native-tile-artifacts.json").read_bytes())["full"]
    claim = check(log, {"class_id": full["class_id"], "artifact_root": full["inventory"]},
                  decode_limit=16, recorded_replies=True)
    panel = claim["panel"]
    pids, (block, marker, data) = shared_rows(log, r"QF1 public replies pid=(\d+) block=(\w+) marker=(\w+) data=([0-9a-f]+)")
    require(sorted(map(int, pids)) == panel["pids"] and block == panel["accepted_replies"], "wrong receipt or peer")
    field = lambda name: bytes.fromhex(panel[name])
    anchor = h512(b"MISAKA/private/TB1/PANEL/draw-anchor/v1", field("panel_marker"), field("panel_seed"))
    duty = h512(DOMAIN, field("panel_marker"), field("original"), field("receipt"), field("panel_certificate"), anchor)
    record = bytes.fromhex(data)
    width = 1952 + 3309
    require(len(record) == len(panel["selected"]) * width, "wrong number of public replies")
    pieces = [b"MISAKA/private/TB1/PANEL/reply-set/v1", duty]
    with tempfile.TemporaryDirectory() as temp:
        for i, actor in enumerate(panel["selected"]):
            key = record[i * width:i * width + 1952]
            signature = record[i * width + 1952:(i+1) * width]
            signer = h512(b"MISAKA/private/TB1/PANEL/actor/v1", key)
            require(signer.hex() == actor, "wrong signer or draw order")
            require(verify_signature(openssl, key, h512(duty, signer), signature, DOMAIN.decode(), Path(temp)),
                    "invalid full-replay signature")
            pieces.extend([signer, signature])
    require(h512(*pieces).hex() == marker, "native reply-set commitment mismatch")
    result = dict(binary=panel["binary_sha256"], processes=panel["pids"], accepted=block, daa=panel["accepted_daa"],
                deadline=panel["deadline"], selected=panel["selected"], marker=marker, public_bytes=len(record))
    if checkpoints or cold_readers:
        readers, (observed, deadline_tip, included) = shared_rows(
            log, r"QF1 reopened checkpoints pid=(\d+) observed=([0-9a-f]{128}) deadline_tip=([0-9a-f]{128}) accepted=([0-9a-f]{128})")
        require(readers == pids and deadline_tip == panel["deadline_block"] and included == block and
                observed not in {included, deadline_tip, panel["panel_batch"], panel["panel_certificate"]},
                "checkpoint generations or acceptance disagree")
        result.update(observation_checkpoint=observed, deadline_checkpoint=deadline_tip)
    if cold_readers:
        require(len(cold_readers) == 3, "expected three cold reader logs")
        expected = dict(observed=observed, deadline_tip=deadline_tip, accepted=block, marker=marker, deadline=panel["deadline"],
                        close=panel["panel_close"], batch=panel["panel_batch"], certificate=panel["panel_certificate"],
                        close_marker=panel["panel_marker"])
        mapping = {}
        domains = set()
        for reader_log in cold_readers:
            require("test result: ok. 1 passed; 0 failed;" in reader_log, "incomplete cold reader")
            rows = re.findall(r"QF1 cold reader pid=(\d+) receipt=(\{[^\n]*\}) replies=([0-9a-f]+)", reader_log)
            require(len(rows) == 1, "missing or duplicate cold reader result")
            pid, meta, stored = rows[0]
            meta = json.loads(meta)
            writer = meta["writer"]
            require(all(meta.get(k) == v for k, v in expected.items()) and stored == data, "cold evidence differs from signed receipt")
            require(writer in panel["pids"] and writer not in mapping and int(pid) not in panel["pids"] and int(pid) not in mapping.values(),
                    "expected distinct original peers and fresh reader processes")
            mapping[writer] = int(pid)
            domains.add(meta["domain"])
        require(len(domains) == 1, "inconsistent native chain domain")
        result["cold_readers_by_writer"] = mapping
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    parser.add_argument("--openssl", default="openssl")
    parser.add_argument("--checkpoints", action="store_true")
    parser.add_argument("--cold-readers", nargs=3, type=Path, metavar="LOG")
    args = parser.parse_args()
    print(json.dumps(verify(args.log.read_text(), args.openssl, checkpoints=args.checkpoints,
                            cold_readers=[p.read_text() for p in args.cold_readers or []]), indent=2))
