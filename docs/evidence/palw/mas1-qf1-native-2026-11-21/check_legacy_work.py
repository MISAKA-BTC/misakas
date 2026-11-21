"""Compare one capitalized Studio branch producer with two independent native replays."""
import argparse
import hashlib
import json
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
REFUSAL = "the model registry admits no new claim of this class now"


def require(ok, message):
    if not ok:
        raise ValueError(message)


def read_peer(path):
    log = path.read_text()
    digest = re.findall(r"^test_binary_sha256=([0-9a-f]{64})$", log, re.M)
    require(len(digest) == 1 and log.count("test result: ok. 1 passed; 0 failed") == 1, "missing binary or completed test")
    records = {}
    pids = set()
    for kind, pid, raw in re.findall(r"^legacy-work (?:(inventory|result) )?(\d+) (\{.*\})$", log, re.M):
        data = json.loads(raw)
        name = data["model"] if kind == "inventory" else kind or data["stage"]
        require(name not in records, "duplicate observation")
        records[name] = data
        pids.add(int(pid))
    require(len(pids) == 1 and len(records) == 8, "missing peer, stage or model")
    requested = "legacy-work registration requested_share=1 native_preflight=NotEndToEndCertified"
    return digest[0], pids.pop(), records, log.count(requested)


def verify(paths, branch_path):
    peers = [read_peer(p) for p in paths]
    binary, _, rows, _ = peers[0]
    require([n for _, _, _, n in peers] == [1, 0, 0], "expected one producer followed by two readers")
    pids = [pid for _, pid, _, _ in peers]
    require(len(set(pids)) == 3 and all((b, data) == (binary, rows) for b, _, data, _ in peers), "distinct peers must agree")
    source = json.loads((HERE / "registered-certified-frontier.json").read_text())
    reg, assigned, mature, result = (rows[key] for key in ("registered", "assigned", "seasoned", "result"))
    require([r["daa"] for r in (reg, assigned, mature)] == [3, 4, 5040], "unexpected registration or maturity")
    amount = reg["collateral"] // 2
    require(amount > 0 and reg["effective"] is None and reg["pending"] is None, "registration half collateral")
    require(assigned["effective"] is None and assigned["pending"] == amount, "pending assignment")
    require(mature["effective"] == amount and mature["pending"] is None and int(mature["capital"]) > 0, "mature allocation")
    require(mature["opportunity"] == 720_000_000 and mature["status"] == "Active" and mature["fp_certified"] is False, "mature readiness")
    require(all((r["class"], r["artifact"], r["bond"], r["collateral"]) ==
                (reg["class"], reg["artifact"], reg["bond"], reg["collateral"]) for r in (assigned, mature)), "changed assignment identity")
    inventory = {key: r for key, r in rows.items() if "cards" in r}
    require(len(inventory) == 4 and inventory[reg["class"]]["root"] == reg["artifact"], "Studio inventory")
    qwen = source["class_id"]
    require(inventory[qwen]["root"] == json.loads((HERE / "class-check.json").read_text())["artifact_root"], "QF1 inventory")
    lifecycle = {}
    for model, r in inventory.items():
        expected_daa = 3 if model == reg["class"] else 1 if model == qwen else 0
        status = "Candidate" if expected_daa else "Prefetching"
        require(r["registered_daa"] == expected_daa and r["tip"] == mature["tip"], "inventory history")
        require(r["cards"] == [dict(card=i, verdict=REFUSAL, class_reason=f"class {model} is {status} under the model registry and admits no new claims")
                               for i in range(8)], "missing card or different producer refusal")
        lifecycle[model] = status
    readiness = result["readiness"]
    require(result["close"] == source["close"] and result["deadline"] == source["deadline"], "original audit")
    require(result["tip"] == mature["tip"] == readiness["generation"] and result["state"] == mature["root"], "tip or state mismatch")
    require(result["claims"] == 0 and result["publication_refusal"] == "WouldBlock" and readiness["stop"] == "frontierNotCovered", "native release")
    require(readiness["safe"] is None and readiness["finalized"]["finalized"] is None and
            readiness["finalized"]["pruningPoint"] == source["finalized"]["pruningPoint"], "native finality")
    raw = branch_path.read_bytes()
    branch = json.loads(raw)
    require(all(branch[k] == result[k] for k in ("close", "registration", "assignment", "seasoned", "tip")), "wrong fork archive")
    headers = [b["header"] for b in branch["blocks"]]
    require(len(headers) == 10076 and headers[0]["parentsByLevel"][0] == [result["close"]] and
            headers[-1]["blueScore"] == readiness["sinkBlue"] and headers[-1]["daaScore"] == result["daa"] == 5040, "raw fork bounds")
    for stage, r in (("registration", reg), ("assignment", assigned)):
        children = [i for i, h in enumerate(headers) if h["parentsByLevel"][0] == [r["tip"]]]
        require(r["tip"] == result[stage] and len(children) == 1 and children[0] > 0 and
                headers[children[0] - 1]["daaScore"] == r["daa"], f"wrong raw {stage}")
    return dict(binary_sha256=binary, processes=pids, archive_sha256=hashlib.sha256(raw).hexdigest(), blocks=len(headers),
                close=result["close"], registration=reg["tip"], assignment=assigned["tip"], tip=mature["tip"], state_root=mature["root"],
                assigned_sompi=amount, average_capital_sompi=mature["capital"], available_microblocks=mature["opportunity"],
                lifecycle=lifecycle, refused_per_peer=sum(len(r["cards"]) for r in inventory.values()), claims=result["claims"],
                publication=result["publication_refusal"], safe=readiness["safe"], finalized=readiness["finalized"]["finalized"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("peer_logs", type=Path, nargs=3)
    args = parser.parse_args()
    print(json.dumps(verify(args.peer_logs, args.archive), indent=2))
