"""Compare stop-length prediction to the measured online one-pass baseline on six original Tickets."""
import argparse
import hashlib
import json
from pathlib import Path

from tokenizers import Tokenizer

from check_g_cost_predicted_length import check_runs, verify as predicted_report
from check_g_cost_shared_prefix import frozen_plan
from check_g_cost_varied_jobs import online_report, verify_sources
from check_legacy_work import HERE, require
from check_qf1_answers import TOKENIZER


def scheduled_inputs(tokenizer_file):
    schedule = json.loads((HERE / "g-cost-repeated-job-one-pass-schedule.json").read_bytes())
    plan = frozen_plan()
    digest = hashlib.sha256((json.dumps(plan, indent=2) + "\n").encode()).hexdigest()
    trials = [{k: row[k] for k in ("attempt", "ticket")} for row in plan["trials"]]
    require(schedule["plan_sha256"] == digest and schedule["trials"] == trials and
            schedule["methods"] == ["online-stop", "predict-length"] and schedule["calibration_index"] == 0,
            "different experiment or signed trial order")
    raw = tokenizer_file.read_bytes()
    require(hashlib.sha256(raw).hexdigest() == TOKENIZER, "different registered tokenizer")
    jobs = [schedule["job"]] * len(trials)
    verify_sources(Tokenizer.from_buffer(raw), jobs, plan["trials"])
    return schedule, plan, jobs


def compare(tokenizer_file, directory):
    schedule, plan, jobs = scheduled_inputs(tokenizer_file)
    online = online_report(plan, jobs, directory / "online-stop.jsonl")
    prediction = predicted_report(plan, directory / "predict-length.jsonl")
    calibration, *targets = online["trials"]
    first_online = 1000 * online["setup_ms"] + calibration["online_us"]
    first_prediction = 1000 * prediction["setup_ms"] + prediction["calibration_us"]
    pairs = []
    for trial, original, pred in zip(targets, plan["trials"][1:], prediction["pairs"], strict=True):
        require(trial["answer_length"] == pred["observed_length"] and original["attempt"] == pred["attempt"],
                "different calibrated target")
        pairs.append(dict(attempt=pred["attempt"], ticket=trial["ticket"], online_us=trial["online_us"],
                          predicted_us=pred["predicted_us"], retry_reason=pred.get("retry_reason")))
    online_sum = sum(p["online_us"] for p in pairs)
    pred_sum = sum(p["predicted_us"] for p in pairs)
    return dict(plan_sha256=schedule["plan_sha256"], descriptor=plan["descriptor"], job=schedule["job"],
                first_ticket=calibration["ticket"], setup_us=dict(online=1000 * online["setup_ms"],
                prediction=1000 * prediction["setup_ms"]), first_ticket_including_setup_us=dict(online=first_online,
                prediction=first_prediction), targets=pairs, five_targets_us=dict(online=online_sum, prediction=pred_sum),
                six_tickets_including_setup_us=dict(online=first_online+online_sum, prediction=first_prediction+pred_sum),
                forced_miss_control=prediction["miss_control"], g_cost="UNKNOWN")


def calibrated_comparison(tokenizer_file, raw):
    schedule, plan, _ = scheduled_inputs(tokenizer_file)
    mixed = json.loads((HERE / "g-cost-online-calibrated-schedule.json").read_bytes())
    expected = [dict(ticket=row["ticket"], predict_first=i % 2 == 0) for i, row in enumerate(plan["trials"][1:])]
    require(mixed["plan_sha256"] == schedule["plan_sha256"] and mixed["method"] == "online-calibrated"
            and mixed["calibration_ticket"] == plan["trials"][0]["ticket"] and mixed["targets"] == expected
            and mixed["forced_miss_target"] == 0, "changed online calibration plan")
    header, calibration, pairs, control = check_runs(plan, raw, method=mixed["method"])
    common = 1000 * header["setup_ms"] + calibration["elapsed_us"]
    require(control["retry_reason"] is not None, "forced miss control did not retry")
    totals = dict(online_us=common + sum(p["reference_us"] for p in pairs),
                  mixed_us=common + sum(p["predicted_us"] for p in pairs))
    targets = [dict(attempt=row["attempt"], ticket=trial["ticket"], online_us=row["reference_us"],
                    mixed_us=row["predicted_us"], retry_reason=row.get("retry_reason"))
               for row, trial in zip(pairs, plan["trials"][1:], strict=True)]
    return dict(plan_sha256=schedule["plan_sha256"], descriptor=plan["descriptor"], job=schedule["job"],
                first_ticket=plan["trials"][0]["ticket"], setup_ms=header["setup_ms"],
                first_ticket_generation_us=calibration["elapsed_us"], observed_length=calibration["predicted"],
                targets=targets, six_tickets_including_setup=totals, forced_miss_including_retry_us=control["elapsed_us"],
                forced_miss_reason=control["retry_reason"], g_cost="UNKNOWN")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tokenizer", type=Path)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--online-calibrated", action="store_true", help="compare both paths with one shared online calibration")
    args = parser.parse_args()
    result = calibrated_comparison(args.tokenizer, args.directory / "online-calibrated.jsonl") if args.online_calibrated else compare(args.tokenizer, args.directory)
    print(json.dumps(result, indent=2))
