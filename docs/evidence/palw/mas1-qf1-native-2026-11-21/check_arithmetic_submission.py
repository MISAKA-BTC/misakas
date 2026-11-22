"""Verify the arithmetic audit preimages, native submission and the publisher and cold numerical replays."""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path

from tokenizers import Tokenizer

from check_fresh_audit import coinbase_extra
from check_fresh_audit_seed import arithmetic_seed
from check_genesis_draw import header_hash
from check_transaction_root import verify_block_transactions
from check_g_cost_varied_jobs import input_frame
from check_legacy_work import HERE, require
from check_prediction import qf1
from check_qf1_answers import TOKENIZER


def submission_marker(suite, certificate, rows):
    h = hashlib.sha3_512(b"MISAKA/private/MAS1/arithmetic-submission/v1" + bytes.fromhex(suite) + bytes.fromhex(certificate))
    for row in rows:
        job = row["job"].encode()
        output = row["output"]
        body = row["original"]
        h.update(struct.pack("<I", row["case"]) + bytes(row["challenge"]) + struct.pack("<I", len(job)) + job)
        h.update(struct.pack("<II", row["expected"], len(output)) + struct.pack(f"<{len(output)}I", *output))
        h.update(struct.pack("<?I", row["stopped_at_eos"], row["positions"]) + bytes.fromhex(row["z"]))
        h.update(struct.pack("<4Q", *body["counts"]))
        for root in body["roots"]:
            h.update(bytes.fromhex(root))
        h.update(bytes(body["ticket"]))
    return h.digest()


def verify_case(tokenizer, challenge, case, row, plan):
    seed = bytes.fromhex(challenge)
    a, b = seed[0] % 5, seed[1] % 5
    expr, digit = (f"{a} + {b}", a+b) if case == 0 else (f"{a+b} - {b}", a)
    job = f"Answer with only one digit: what is {expr}?"
    require(row["case"] == case and bytes(row["challenge"]) == seed and row["job"] == job, "different arithmetic input")
    [expected] = tokenizer.encode(str(digit), add_special_tokens=False).ids
    require(expected == row["expected"], "different expected digit")
    prompt = input_frame(tokenizer, job, challenge)
    output = row["output"]
    stop = qf1.schedule(prompt, output, plan["decode_limit"])
    require(row["positions"] == stop["processed_positions"] and row["stopped_at_eos"] is (stop["stop_reason"] == "EOS"),
            "wrong output stop or position")
    body = row["original"]
    counts = body["counts"]
    require(counts[:3] == [len(prompt), len(output), 1] and 0 < counts[3] <= plan["max_leaves"], "wrong Original tree sizes")
    roots = [bytes.fromhex(value) for value in body["roots"]]
    for kind, tokens in [(1, prompt), (2, output)]:
        require(roots[kind-1] == qf1.root(seed, kind, [struct.pack("<I", t) for t in tokens]), "different token tree")
    require(roots[2] == qf1.root(seed, 3, [bytes.fromhex(row["z"])]), "different numerical Readout")
    require(qf1.ticket(seed, counts, roots) == bytes(body["ticket"]), "different evaluation ticket")
    return len(output) == 2 and output[0] == expected and output[1] in qf1.EOS


def verify(directory, tokenizer_path, openssl, *, publisher_only=False):
    seed = arithmetic_seed(directory, openssl, publisher_only=publisher_only)
    plan = json.loads((HERE / "qf1-arithmetic-audit-close.json").read_bytes())
    raw = tokenizer_path.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == TOKENIZER, "wrong registered tokenizer")
    tokenizer = Tokenizer.from_buffer(raw)
    block, rows = json.loads((directory / "publication.submission.json").read_bytes())
    h = block["header"]
    require(len(rows) == plan["cases"] == 2 and h["parentsByLevel"][0] == [seed["certificate"]]
            and h["palwStateRoot"] == seed["state"] and h["daaScore"] < plan["audit"]["panel_deadline"] + 1
            and h["blueScore"] < 2 * plan["audit"]["audit_deadline"] + 100, "submission outside the prepared assessment cut")
    verify_block_transactions(block)
    submitted = header_hash(h, bytes(h["palwCommitment"]), False).hex()
    marker = submission_marker(plan["suite"], seed["certificate"], rows)
    require(coinbase_extra(block) == marker, "different arithmetic submission commitment")
    require(set(seed["cases"]) == {bytes(row["challenge"]).hex() for row in rows} and len(set(seed["cases"])) == 2,
            "missing or duplicate drawn case")
    scores = [verify_case(tokenizer, challenge, i, rows[i], plan) for i, challenge in enumerate(seed["cases"])]
    material = {name: hashlib.sha256((directory / name).read_bytes()).hexdigest()
                for name in ("publication.json", "publication.submission.json")}
    cold_copies = {}
    observed_logs = ["holder-2"] if publisher_only else ["holder-2", "replay-0", "replay-1"]
    for name, pid in zip(observed_logs, seed["publication_readers"], strict=True):
        log = (directory / f"{name}.log").read_text()
        copies = re.findall(r"^cold arithmetic material slot=(\d+) name=([\w.]+) sha256=([0-9a-f]{64})$", log, re.M)
        if name.startswith("replay-"):
            require(copies == [(name[-1], filename, digest) for filename, digest in material.items()],
                    "missing or different cold replay input")
            cold_copies[name] = material
        observations = [json.loads(line) for line in re.findall(r"^fresh arithmetic evaluation (\{.*\})$", log, re.M)]
        require(observations == [dict(pid=pid, certificate=seed["certificate"], evaluation=row, correct=score)
                                for row, score in zip(rows, scores)], "peer numerical results differ")
        require(log.count(f"fresh arithmetic submission pid={pid} certificate={seed['certificate']} block={submitted} marker={marker.hex()}") == 1,
                "peer submission differs")
    return dict(close=seed["close"], certificate=seed["certificate"], submission=submitted, marker=marker.hex(),
                processes=seed["publication_readers"], binary_sha256=seed["binary_sha256"], correct=sum(scores),
                required_correct=plan["required_correct"], results=scores, selected_panel_assessment="pending", cold_material=cold_copies,
                cases=[dict(job=row["job"], expected=tokenizer.decode([row["expected"]]), output_ids=row["output"],
                            decoded=tokenizer.decode(row["output"], skip_special_tokens=False),
                            stopped_at_eos=row["stopped_at_eos"], correct=score) for row, score in zip(rows, scores)])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("tokenizer", type=Path)
    parser.add_argument("--publisher-only", action="store_true", help="check the submitted results before the two cold numerical replays")
    parser.add_argument("--openssl", default="openssl")
    args = parser.parse_args()
    print(json.dumps(verify(args.directory, args.tokenizer, args.openssl, publisher_only=args.publisher_only), indent=2))
