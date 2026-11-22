"""Compare complete trace replay with a stop-length prediction on signed Originals."""
import argparse
import json
from pathlib import Path

from check_g_cost_shared_prefix import frozen_plan
from check_legacy_work import require


def check_retry(guess, actual, reason):
    if guess == actual:
        require(reason is None, "unexpected retry at the correct length")
    elif guess > actual:
        require(reason == "QF1 executed trace geometry differs from Original", "missing short-answer trace diagnostic")
    else:
        require(reason in ("Operand: QF1 extra leaf", "QF1 extra leaf", "QF1 decision outside original trace length"),
                "missing overlength trace diagnostic")


def check_runs(plan, raw, *, method="predict-length"):
    first, *targets = plan["trials"]
    header, calibration, *runs, control = map(json.loads, raw.read_text().splitlines())
    require(header["method"] == method and header["trials"] == len(targets) + 1 and
            all(header[k] == plan[k] for k in ("class_id", "descriptor", "inventory", "decode_limit", "max_leaves")),
            "different model, budget, method or sample")
    timings = [row["elapsed_us"] for row in [calibration, *runs, control]]
    require(all(type(us) is int and us > 0 for us in timings) and
            type(header["setup_ms"]) is int and header["setup_ms"] >= 0, "invalid timing")
    guess = len(first["output_ids"])
    require(calibration["predicted"] == guess and bytes(calibration["calibration_ticket"]).hex() == first["ticket"],
            "different calibration original")
    require(len(runs) == 2 * len(targets), "incomplete paired measurement")
    records = []
    for index, original in enumerate(targets):
        actual = len(original["output_ids"])
        pair = dict(attempt=original["attempt"], observed_length=actual)
        for predict, row in zip((index % 2 == 0, index % 2 != 0), runs[2*index:2*index+2]):
            require(row["predict"] is predict and row["target"] == index and
                    bytes(row["verified_ticket"]).hex() == original["ticket"], "wrong order, target, timing or Ticket")
            reason = row["retry_reason"]
            if predict:
                check_retry(guess, actual, reason)
            else:
                require(reason is None, "unexpected retry on the reference route")
            if reason is not None:
                pair["retry_reason"] = reason
            pair["predicted_us" if predict else "reference_us"] = row["elapsed_us"]
        records.append(pair)
    forced = 2 if guess == 1 else guess-1
    reason = control["retry_reason"]
    require(control["control_target"] == 0 and control["predicted"] == forced and
            bytes(control["verified_ticket"]).hex() == targets[0]["ticket"], "wrong shifted prediction or Ticket")
    check_retry(forced, records[0]["observed_length"], reason)
    return header, calibration, records, control


def verify(plan, raw):
    first, *targets = plan["trials"]
    header, calibration, records, control = check_runs(plan, raw)
    guess, forced = calibration["predicted"], control["predicted"]
    require(len(targets) == 5 and control["retry_reason"] is not None, "original six-ticket comparison needs its miss control")
    setup_us = 1000 * header["setup_ms"]
    shared = setup_us + calibration["elapsed_us"]
    independent = sum(p["reference_us"] for p in records)
    predicted = sum(p["predicted_us"] for p in records)
    return dict(descriptor=plan["descriptor"], fixed_prediction=guess, calibration_attempt=first["attempt"],
                setup_ms=header["setup_ms"], calibration_us=calibration["elapsed_us"], pairs=records,
                target_reference_us=independent, target_predicted_us=predicted,
                six_ticket_reference_us=shared+independent, six_ticket_predicted_us=shared+predicted,
                miss_control=dict(predicted=forced, attempt=targets[0]["attempt"], retry_reason=control["retry_reason"],
                                  including_retry_us=control["elapsed_us"]), g_cost="UNKNOWN")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path)
    args = parser.parse_args()
    print(json.dumps(verify(frozen_plan(), args.log), indent=2))
