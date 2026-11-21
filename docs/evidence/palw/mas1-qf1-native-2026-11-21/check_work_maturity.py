"""Check the native work observations against the original branch and its successor headers."""
import argparse
import hashlib
import json
import re
from pathlib import Path

from check_genesis_draw import header_hash
from check_legacy_work import HERE, require


def verify(original, extension, native, writer):
    cert = json.loads((HERE / "genesis-funded-certificate-replicas.json").read_bytes())
    window = json.loads((HERE / "genesis-audit-release-window.json").read_bytes())
    audit = cert["cold_readback"]["signed_audit"]
    log = native.read_text()
    require("test result: ok. 1 passed; 0 failed" in log, "native run did not finish")
    [binary] = re.findall(r"^test_binary_sha256=([a-f0-9]{64})$", log, re.M)

    def record(label):
        [(pid, line)] = re.findall(rf"^{label} (\d+) (\{{.*\}})$", log, re.M)
        r = json.loads(line)
        require((r["close"], r["base"], r["claim"]) == (audit["close"], audit["tip"], cert["claim"]), "substituted history")
        return pid, r

    pid, retirement = record("signed work retirement")
    other, m = record("signed work maturity")
    require(pid == other and (m["work"], m["scheduled"], m["start"]) ==
            (retirement["work"], retirement["scheduled"], retirement["start"]), "different work or native process")
    require(retirement["deadline"] == audit["deadline"] and audit["deadline"] == window["deadline"], "different release deadline")
    start = m["start"]
    height = m["maturity"]
    require(start["state"] == audit["state"] and start["daa"] == audit["daa"] and
            height == start["daa"] + start["earliest_in"] == window["initial_earliest_maturity_daa"], "different starting state or maturity")
    producer_log = writer.read_text()
    [(writer_pid, measured)] = re.findall(r"^signed work maturity (\d+) (\{.*\})$", producer_log, re.M)
    [writer_binary] = re.findall(r"^test_binary_sha256=([a-f0-9]{64})$", producer_log, re.M)
    require(pid != writer_pid and json.loads(measured) == m and "test result: ok. 1 passed; 0 failed" in producer_log,
            "writer and independent reader observed different maturity histories")
    at = retirement["blocks"]
    n = window["blocks"]
    require(0 < at < n and m["blocks"] > 0, "invalid observation offsets")
    snapshots = {0: start, at-1: retirement["before"], at: retirement["after"], n: m["expired"], n+m["blocks"]: m["observed"]}
    require(len(snapshots) == 5, "observations must be distinct")
    tip, daa, blue = audit["tip"], audit["daa"], audit["readiness"]["sinkBlue"]
    offset = 0
    depth = window["pruning_depth"]
    blue_at_maturity = None
    hashes = []
    for path in (original, extension):
        digest = hashlib.sha256()
        with path.open("rb") as source:
            for raw in source:
                digest.update(raw)
                block = json.loads(raw)
                h = block["header"]
                if offset == n + m["blocks"]:
                    blue_at_maturity = blue
                if offset in snapshots:
                    row = snapshots[offset]
                    require((row["tip"], row["daa"], row["state"]) == (tip, daa, h["palwStateRoot"]),
                            "observed native state differs from the successor header commitment")
                require(h["parentsByLevel"][0] == [tip] and h["blueScore"] == blue + 1 and h["daaScore"] >= daa and
                        h["pruningPoint"] == window["pruning"] and h["powAlgoId"] == 8, "disconnected heartbeat or changed pruning point")
                require(len(block["transactions"]) == 1 and block["transactions"][0]["subnetworkId"] == "01"+"00"*19,
                        "unexpected non-coinbase activity")
                tip = header_hash(h, bytes(h["palwCommitment"]), False).hex()
                daa, blue = h["daaScore"], h["blueScore"]
                offset += 1
        hashes.append(digest.hexdigest())
        if path == original:
            require(offset == n and tip == window["tip"] and daa == window["deadline"] and
                    hashes[0] == window["archive_sha256"], "different original audit window")
    require(tip == m["witness"] and offset == n + m["blocks"] + 1 and m["observed"]["daa"] == height, "incomplete maturity witness")
    require(blue_at_maturity is not None and blue_at_maturity < depth, "genesis pruning-depth bound no longer explains this branch")
    before, after = retirement["before"], retirement["after"]
    require(before["daa"] == retirement["scheduled"] < after["daa"] and before["due"] == retirement["scheduled"] and
            before["claim_live"] and after["deleted"] and not after["claim_live"] and after["due"] is None,
            "adjacent states do not show the original scheduled retirement")
    require(all(row["bond_live"] and row["finalized"] is None for row in snapshots.values()), "bond lost or unexpected finality")
    require(all(row["blocking"] == audit["readiness"]["blocking"]["block"] and row["safe"] is None
                for row in (start, before, after, m["expired"])), "blocking effect changed during the original signed window")
    require(all(row["pending_source"] and row["closed_maturity"] is None for row in (start, before, after, m["expired"])),
            "canonical pending source was lost before maturity")
    require(before["evidence"]["pendingFacts"] == 1 and after["evidence"]["pendingFacts"] == 0 and
            before["evidence"]["pendingWork"] == start["evidence"]["pendingWork"] and after["evidence"]["pendingWork"] == "0",
            "missing live-claim tally transition")
    mature = m["observed"]
    require(mature["pending_source"] is False and mature["closed_maturity"] == height and mature["safe"] == cert["accepted"] and
            not mature["claim_live"] and mature["finalized"] is None, "work failed to mature on the original branch")
    return dict(binary_sha256=binary, process=int(pid), writer_binary_sha256=writer_binary, writer_process=int(writer_pid),
                original_sha256=hashes[0], extension_sha256=hashes[1],
                extension_blocks_including_witness=m["blocks"]+1, witness=tip, claim=m["claim"], work=m["work"], close=m["close"],
                blue_at_release_deadline=window["observations"][-1]["blue"], blue_at_work_maturity=blue_at_maturity,
                minimum_blue_to_advance_from_genesis=depth,
                remaining_blue_to_genesis_depth_at_maturity=depth-blue_at_maturity,
                original_deadline=window["deadline"], scheduled_retirement=retirement["scheduled"], forecast_maturity=height,
                local_at_acceptance=retirement["local_at_acceptance"], local_at_retirement=retirement["local_at_observation"],
                start=start, before_retirement=before, retirement=after, at_release_deadline=m["expired"], at_maturity=m["observed"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("original", "extension", "native_log", "writer_log"):
        parser.add_argument(name, type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.original, args.extension, args.native_log, args.writer_log), indent=2))
