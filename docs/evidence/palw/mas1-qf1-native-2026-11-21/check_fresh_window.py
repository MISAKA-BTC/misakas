"""Check the funded branch through the first pruning transition and its recorded template refusal."""
import argparse
import hashlib
import json
import re
import subprocess
from pathlib import Path

from check_fresh_audit import records
from check_genesis_draw import attempt_identity, header_hash
from check_legacy_work import HERE, require


def check_pruning_header(audit, point):
    """Trace the first covering pruning point on the original public registration branch."""
    archive = HERE / "fresh-retained-audit-public.tar.zst"
    def member(name):
        return subprocess.check_output(["tar", "-xOf", archive, name])
    meta, *prefix = map(json.loads, member("public.jsonl").splitlines())
    capital = json.loads(member("capital.json"))
    require(capital["base"] == meta["tip"], "wrong capital branch")
    tips = {meta["genesis"]: 0}
    parent = meta["genesis"]
    for block in prefix + capital["blocks"]:
        h = block["header"]
        require(h["parentsByLevel"][0] == [parent] and h["blueScore"] == tips[parent] + 1, "disconnected audit prefix")
        parent = header_hash(h, bytes(h["palwCommitment"]), False).hex()
        tips[parent] = h["blueScore"]
    require(audit["close"] in tips and point in tips and tips[audit["close"]] <= tips[point],
            "header pruning point does not descend from the signed close")
    return dict(close_blue=tips[audit["close"]], point=point, blue=tips[point])


def verify(branch, log):
    source = json.loads((HERE / "fresh-8k-certificate.json").read_bytes())
    audit = source["audit"]
    start = audit["readiness"]
    tip, blue, daa = start["generation"], start["sinkBlue"], start["sinkDaa"]
    require(tip == audit["tip"] == source["native"]["tip"], "wrong funded certificate")
    [maturity] = [w["readyDaa"] for w in start["blocking"]["waits"] if w["kind"] == "waitingMaturity"]
    log_text = log.read_text()
    [(pid, initial)] = records(log_text, "signed audit release window")
    [binary] = re.findall(r"^test_binary_sha256=([a-f0-9]{64})$", log_text, re.M)
    require((initial["close"], initial["base"], initial["deadline"], initial["tip"], initial["blue"], initial["daa"]) ==
            (audit["close"], audit["tip"], audit["deadline"], tip, blue, daa) and initial["readiness"] == start and
            initial["blocks"] == 0 and initial["publication"] == "held" and binary == source["panel"]["binary_sha256"],
            "expected the original held gate and writer build")
    previous_pruning = start["finalized"]["pruningPoint"]
    changes = []
    digest = hashlib.sha256()
    crossing = None
    count = 0
    with branch.open("rb") as stream:
        for count, raw in enumerate(stream, 1):
            digest.update(raw)
            block = json.loads(raw)
            header = block["header"]
            require(header["parentsByLevel"][0] == [tip] and header["blueScore"] == blue + 1 and
                    daa <= header["daaScore"] <= audit["deadline"], "disconnected or out-of-window header")
            require(header["version"] == 2 and header["powAlgoId"] == 8 and len(block["transactions"]) == 1 and
                    block["transactions"][0]["subnetworkId"] == "01" + "00" * 19, "expected an ordinary heartbeat")
            if count == 1:
                require(header["palwStateRoot"] == source["native"]["state"], "different starting state")
            if crossing and count == crossing["blocks"] + 1:
                crossing["next_header_parent_state"] = header["palwStateRoot"]
            tip = header_hash(header, bytes(header["palwCommitment"]), False).hex()
            blue, daa = header["blueScore"], header["daaScore"]
            if crossing is None and daa >= maturity:
                crossing = dict(blocks=count, tip=tip, daa=daa)
            if header["pruningPoint"] != previous_pruning:
                previous_pruning = header["pruningPoint"]
                changes.append(dict(blocks=count, tip=tip, daa=daa, pruning=previous_pruning))
    reason = "LocalNativeStateContextUnavailable(HistoryUnavailable)"
    require("a template: " + reason in log_text and "test result: FAILED. 0 passed; 1 failed" in log_text,
            "expected initial hold and template refusal before a terminal gate observation")
    require(len(changes) == 1 and changes[0]["blocks"] == count and
            f"Pruning deferred before snapshot readiness at {previous_pruning}:" in log_text, "wrong transition endpoint")
    require(crossing and "next_header_parent_state" in crossing, "missing maturity boundary")
    return dict(base=audit["tip"], close=audit["close"], deadline=audit["deadline"], blocks=count, tip=tip, daa=daa, blue=blue,
                pruning_header=check_pruning_header(audit, previous_pruning), forecast_maturity=maturity, maturity_header=crossing,
                template_refusal=reason, terminal_gate_observed=False, writer=pid, binary_sha256=binary, stream_sha256=digest.hexdigest())




def check_maturity(window, native_log):
    source = json.loads((HERE / "fresh-8k-certificate.json").read_bytes())["native"]
    log = native_log.read_text()
    [binary] = re.findall(r"^test_binary_sha256=([a-f0-9]{64})$", log, re.M)
    [(pid, row)] = records(log, "fresh audit maturity")
    [(cert_pid, cert)] = records(log, "legacy genesis certification")
    require(pid == cert_pid and pid != window["writer"] and cert["claim"] == source["claim"] and
            cert["state"] == source["state"] and cert["tip"] == source["tip"], "different cold certification")
    accepted = json.loads(subprocess.check_output(["tar", "-xOf", HERE / "fresh-retained-audit-public.tar.zst", "accepted/winning.json"]))
    header = accepted["raw"]["header"]
    wire, _, claim, _, work = attempt_identity(header)
    require(header_hash(header, wire, False).hex() == source["accepted"] and claim == source["claim"] == accepted["claim"] and
            row["work"] == work.hex(), "matured fact belongs to a different native execution")
    cross = window["maturity_header"]
    require((row["tip"], row["daa"], row["blocks"], row["state"]) ==
            (cross["tip"], cross["daa"], cross["blocks"], cross["next_header_parent_state"]), "wrong mature state or block")
    require(row["claim"] == source["claim"] and row["base"] == source["tip"] and row["close"] == window["close"] and
            row["maturity"] == window["forecast_maturity"] == row["daa"],
            "maturity is not tied to the original work, claim and audit")
    ready = row["readiness"]
    require(ready["safe"] == source["accepted"] and ready["generation"] == row["tip"] and
            ready["finalized"]["finalized"] is None and row["publication"] == "held", "wrong safe prefix or release state")
    return dict(reader=pid, binary_sha256=binary, slot=row["slot"], claim=row["claim"], work=row["work"], close=row["close"],
                blocks=row["blocks"], daa=row["daa"], state=row["state"], tip=row["tip"], safe=ready["safe"],
                finalized=ready["finalized"]["finalized"], publication=row["publication"])


def check_held_endpoint(window, native_log):
    maturity = json.loads((HERE / "fresh-work-maturity.json").read_bytes())
    log = native_log.read_text()
    [(pid, end)] = records(log, "fresh audit window end")
    [binary] = re.findall(r"^test_binary_sha256=([a-f0-9]{64})$", log, re.M)
    require(pid == maturity["reader"] and binary == maturity["binary_sha256"] and end["slot"] == maturity["slot"],
            "different holder process or build")
    require(all(end[k] == window[k] for k in ("base", "close", "tip", "blocks", "daa")) and end["daa"] < window["deadline"],
            "wrong branch or endpoint")
    readiness = end["readiness"]
    point = window["pruning_header"]["point"]
    require(readiness["generation"] == end["tip"] and readiness["sinkDaa"] == end["daa"] and
            readiness["sinkBlue"] == window["blue"] and readiness["safe"] == maturity["safe"] and
            readiness["finalized"]["pruningPoint"] == readiness["finalized"]["finalized"] == point,
            "unexpected advisory observation")
    require(end["publication"] == "WouldBlock: native finalized head unavailable" and
            "sign only on an independently finalized close:" in log and
            "test result: FAILED. 0 passed; 1 failed" in log and "fresh audit signed holder=" not in log,
            "expected the native custody refusal")
    return dict(reader=pid, slot=end["slot"], tip=end["tip"], daa=end["daa"], deadline=window["deadline"],
                state=end["state"], safe=readiness["safe"], advisory_finalized=point, holder_gate=end["publication"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("branch", type=Path)
    parser.add_argument("writer_log", type=Path)
    group = parser.add_mutually_exclusive_group()
    group.add_argument("--maturity-log", type=Path, help="cold holder log through the first native closed-fact row")
    group.add_argument("--held-endpoint-log", type=Path, help="holder refusal at the first covering pruning point")
    args = parser.parse_args()
    window = verify(args.branch, args.writer_log)
    if args.held_endpoint_log:
        window = check_held_endpoint(window, args.held_endpoint_log)
    elif args.maturity_log:
        window = check_maturity(window, args.maturity_log)
    print(json.dumps(window, indent=2))
