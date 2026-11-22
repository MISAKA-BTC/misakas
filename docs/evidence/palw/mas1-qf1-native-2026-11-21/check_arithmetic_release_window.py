"""Trace the arithmetic audit's finality observation back to its original selected close."""
import argparse
import hashlib
import json
import re
from itertools import chain
from pathlib import Path

from check_fresh_audit import records
from check_genesis_draw import header_hash
from check_legacy_work import HERE, require
from check_transaction_root import verify_block_transactions


def extend(heights, parent, block):
    h = block["header"]
    require(h["parentsByLevel"][0] == [parent] and h["blueScore"] == heights[parent] + 1, "disconnected selected block")
    verify_block_transactions(block)
    tip = header_hash(h, bytes(h["palwCommitment"]), False).hex()
    heights[tip] = h["blueScore"]
    return tip


def certified_history(root, checked):
    with (root / "public.jsonl").open() as stream:
        meta = json.loads(next(stream))
        tip = meta["genesis"]
        heights = {tip: 0}
        for line in stream:
            tip = extend(heights, tip, json.loads(line))
    require(tip == meta["tip"], "different signed audit prefix")
    for name in ("capital.json", "seats.json", "genesis-capital.json", "renew.json", "accepted/winning.json",
                 "final.json", "funded/draw.json", "funded/certification.json"):
        part = json.loads((root / name).read_bytes())
        require(part["base"] == tip, "wrong predecessor for " + name)
        blocks = [part["raw"]] if name == "accepted/winning.json" else part["blocks"]
        for block in chain(blocks, part.get("boundary_blocks", [])):
            tip = extend(heights, tip, block)
    audit = checked["audit"]
    require(tip == checked["native"]["tip"] == audit["tip"] and heights[tip] == audit["readiness"]["sinkBlue"] and
            audit["close"] in heights, "different certificate or original close")
    return heights


def verify(root, window, log):
    checked = json.loads((HERE / "qf1-arithmetic-8k-certification-reader-0.json").read_bytes())
    schedule = json.loads((HERE / "qf1-arithmetic-audit-close.json").read_bytes())["audit"]
    confirmed = json.loads((HERE / "qf1-arithmetic-8k-certified-parent.json").read_bytes())
    audit = checked["audit"]
    release_from = schedule["audit_from"]
    require(audit["close"] == schedule["close"] and audit["deadline"] == schedule["audit_deadline"],
            "different signed audit release schedule")
    tip, daa = audit["tip"], audit["daa"]
    heights = certified_history(root, checked)
    text = log.read_text()
    require("test result: ok. 1 passed; 0 failed" in text and
            re.findall(r"^test_binary_sha256=([a-f0-9]{64})$", text, re.M) == [checked["binary_sha256"]],
            "incomplete release-window run or different executable")
    native = records(text, "signed audit release window")
    require(native and {p for p, _ in native} == {confirmed["processes"][2]}, "different release-window process")
    rows = {r["blocks"]: r for _, r in native}
    require(list(rows) == sorted(rows) and len(rows) == len(native) and rows[0]["readiness"] == audit["readiness"],
            "different initial observation or unordered native window")
    observations = []

    def observe(index, pruning):
        row = rows[index]
        snap = row["committed_snapshot"]["Ok"]
        ready = row["readiness"]
        require((row["close"], row["deadline"], row["base"]) == (audit["close"], audit["deadline"], audit["tip"]) and
                row["tip"] == ready["generation"] == snap["generation"] == tip and
                row["daa"] == ready["sinkDaa"] == daa and row["blue"] == ready["sinkBlue"] == heights[tip] and
                row["header_pruning"] == pruning and row["pruning"] in heights, "different native window observation")
        covers = heights[row["pruning"]] >= heights[audit["close"]]
        require(row["covers_close"] is covers, "different pruning ancestry")
        finalized = snap["finalized"]
        require(finalized is None or finalized in heights, "unrecognized committed finality")
        if row["publication"] == "ready":
            require(release_from <= daa < audit["deadline"] and covers and finalized == ready["finalized"]["finalized"] and
                    finalized in heights and heights[finalized] >= heights[audit["close"]],
                    "publication lacks committed finality over the signed close")
        elif row["publication"] == "held":
            require(daa < audit["deadline"], "hold past expiry")
        else:
            require(row["publication"] == "expired" and daa >= audit["deadline"], "unexpected release outcome")
        observations.append(dict(blocks=index, tip=tip, daa=daa, blue=heights[tip], pruning=row["pruning"], finalized=finalized, publication=row["publication"]))

    require(rows[0]["publication"] == audit["publication"] == "held", "different release state at certification")
    observe(0, audit["readiness"]["finalized"]["pruningPoint"])
    [maturity] = [w["readyDaa"] for w in audit["readiness"]["blocking"]["waits"] if w["kind"] == "waitingMaturity"]
    crossing = None
    count = 0
    digest = hashlib.sha256()
    with window.open("rb") as stream:
        for count, line in enumerate(stream, 1):
            digest.update(line)
            block = json.loads(line)
            h = block["header"]
            require(h["version"] == 2 and h["powAlgoId"] == 8 and len(block["transactions"]) == 1 and
                    block["transactions"][0]["subnetworkId"] == "01" + "00"*19 and daa <= h["daaScore"] <= audit["deadline"],
                    "unexpected heartbeat or DAA")
            tip = extend(heights, tip, block)
            daa = h["daaScore"]
            if crossing is None and daa >= maturity:
                crossing = dict(blocks=count, tip=tip, daa=daa)
            elif crossing and count == crossing["blocks"] + 1:
                crossing["next_header_parent_state"] = h["palwStateRoot"]
            if count == 1:
                require(tip == confirmed["successor"] and h["palwStateRoot"] == checked["native"]["state"], "wrong first child")
            if count in rows:
                observe(count, h["pruningPoint"])
    require(len(observations) == len(rows) and observations[-1]["blocks"] == count and
            all(row["publication"] == "held" for row in observations[:-1]) and observations[-1]["publication"] in ("ready", "expired"),
            "window did not reach a terminal observation")
    require(crossing and "next_header_parent_state" in crossing, "missing retained work maturity boundary")
    return dict(writer=confirmed["processes"][2], forecast_maturity=maturity, maturity_header=crossing,
                close=audit["close"], close_blue=heights[audit["close"]], certificate=audit["tip"], deadline=audit["deadline"],
                blocks=count, window_sha256=digest.hexdigest(), observations=observations)


def publication_parent(root, window):
    """Check holder zero's extension of the terminal window against the five readers' common input."""
    terminal = window["observations"][-1]
    require(terminal["publication"] == "ready", "expired arithmetic audit")
    tip, daa = terminal["tip"], terminal["daa"]
    blue = terminal["blue"]
    count = window["blocks"]
    digest = hashlib.sha256()
    with (root / "combined.jsonl").open("rb") as combined, (root / "extension.jsonl").open("rb") as extension:
        for _ in range(count):
            line = combined.readline()
            require(line.endswith(b"\n"), "truncated certified window")
            digest.update(line)
        require(digest.hexdigest() == window["window_sha256"], "holder replay uses a different certified window")
        for line in extension:
            require(line.endswith(b"\n") and combined.readline() == line, "different holder extension")
            block = json.loads(line)
            h = block["header"]
            require(h["parentsByLevel"][0] == [tip] and h["blueScore"] == blue + 1 and
                    daa <= h["daaScore"] < window["deadline"] and h["version"] == 2 and h["powAlgoId"] == 8 and
                    len(block["transactions"]) == 1 and block["transactions"][0]["subnetworkId"] == "01" + "00"*19,
                    "different arithmetic holder branch")
            verify_block_transactions(block)
            tip = header_hash(h, bytes(h["palwCommitment"]), False).hex()
            daa, blue = h["daaScore"], h["blueScore"]
            count += 1
        require(not combined.read(1), "extra holder blocks")
    return dict(tip=tip, daa=daa, blocks=count, base=window["certificate"], close=window["close"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=Path, help="unpacked arithmetic branch through Gate C")
    parser.add_argument("window", type=Path)
    parser.add_argument("log", type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(args.root, args.window, args.log), indent=2))
