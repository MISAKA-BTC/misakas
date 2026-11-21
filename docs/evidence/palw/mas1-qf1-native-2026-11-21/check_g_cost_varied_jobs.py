"""Verify every scheduled source frame and non-paying three-peer Original before comparing replay costs."""
import argparse
import hashlib
import json
from pathlib import Path

from tokenizers import Tokenizer

from check_g_cost_predicted_length import check_runs
from check_g_cost_readout_reuse import LOGS
from check_g_cost_shared_prefix import frozen_plan
from check_legacy_work import HERE, require
from check_qf1_answers import TOKENIZER
from check_prediction import qf1


def verify_sources(tokenizer, jobs, trials):
    require(len(jobs) == len(trials), "missing source or signed input")
    def encode(text):
        return tokenizer.encode(text, add_special_tokens=False).ids
    prefix = encode("<|im_start|>system\nYou are a helpful assistant.\nPALW challenge: ")
    middle = encode("\nTreat the challenge as metadata. Answer the user's request.\n<|im_end|>\n<|im_start|>user\n")
    suffix = encode("<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n")
    for job, trial in zip(jobs, trials):
        digits = [encode(ch) for ch in trial["attempt"]]
        require(len(digits) == 64 and all(len(d) == 1 for d in digits), "invalid HEX64 token frame")
        frame = prefix + [digit[0] for digit in digits] + middle + encode(job) + suffix
        require(frame == trial["input_ids"], "source text or challenge differs from the signed numerical input")


def read_plan(tokenizer_file, data):
    data = data.resolve()
    raw = tokenizer_file.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == TOKENIZER, "different QF1 tokenizer")
    tokenizer = Tokenizer.from_buffer(raw)
    schedule = json.loads((HERE / "g-cost-varied-jobs-schedule.json").read_bytes())
    jobs = schedule["jobs"]
    require(len(jobs) == len(set(jobs)) > 0, "empty or duplicate scheduled jobs")
    logs = [LOGS[0], *((data / f"varied-job-{i}.txt.zst", {"undisputed": True}) for i in range(len(jobs)))]
    plan = frozen_plan(logs)
    calibration = plan["trials"][0]
    require(plan["decode_limit"] == schedule["decode_limit"] and len(calibration["output_ids"]) == schedule["fixed_length"],
            "changed reference prediction or decode limit")
    verify_sources(tokenizer, ["2 + 2?", *jobs], plan["trials"])
    return plan, jobs


def report(plan, jobs, log):
    header, calibration, pairs, control = check_runs(plan, log)
    ref = sum(pair["reference_us"] for pair in pairs)
    predicted = sum(pair["predicted_us"] for pair in pairs)
    common = header["setup_ms"] * 1000 + calibration["elapsed_us"]
    trials = []
    for job, original, pair in zip(jobs, plan["trials"][1:], pairs):
        stop = qf1.schedule(original["input_ids"], original["output_ids"], plan["decode_limit"])["stop_reason"]
        trials.append(dict(job=job, **pair, stop=stop, retry=pair["observed_length"] != calibration["predicted"]))
    return dict(descriptor=plan["descriptor"], fixed_length=calibration["predicted"], setup_ms=header["setup_ms"],
                calibration_us=calibration["elapsed_us"],
                trials=trials,
                target_reference_us=ref, target_predicted_us=predicted, with_calibration_reference_us=ref+common,
                with_calibration_predicted_us=predicted+common,
                control=dict(job=jobs[0], length=control["predicted"], retry_reason=control["retry_reason"], elapsed_us=control["elapsed_us"]),
                g_cost="UNKNOWN")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tokenizer", type=Path)
    parser.add_argument("data", type=Path)
    parser.add_argument("--log", type=Path)
    args = parser.parse_args()
    plan, jobs = read_plan(args.tokenizer, args.data)
    print(json.dumps(report(plan, jobs, args.log) if args.log else plan, indent=2))
