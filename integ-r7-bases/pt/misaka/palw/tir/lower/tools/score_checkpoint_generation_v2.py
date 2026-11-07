#!/usr/bin/env python3
"""Score fixed source/integer greedy jobs using the pinned tokenizer and policy.

There is no benchmark or native acceptance assertion: three short jobs expose
free-running divergence and exact task-answer failures without dropping cases.
"""
import argparse
import json
from pathlib import Path

import hf_checkpoint_reference as pin
from hf_checkpoint_generation_v2 import protocol_check


def score(protocol, source, integer, tokenizer):
    if (source.get("schema") != "misaka.rfc02.independent-greedy-source.v2"
            or integer.get("schema") != "misaka.rfc02.independent-greedy-integer.v2"
            or source.get("jobs") is None or integer.get("jobs") is None
            or len(source["jobs"]) != len(protocol["task_jobs"])
            or len(integer["jobs"]) != len(protocol["task_jobs"])):
        raise ValueError("generation scoring scope/jobs")
    rows, equal, exact, source_passed, integer_passed, comparison_slots = [], 0, 0, 0, 0, 0
    for job, src, own in zip(protocol["task_jobs"], source["jobs"], integer["jobs"]):
        if any(row.get("id") != job["id"] or row.get("task") != job["task"]
               or row.get("prompt_tokens") != job["prompt_tokens"] for row in (src, own)):
            raise ValueError("generation scoring job identity/prompt")
        images = []
        for row in (src, own):
            tokens = row.get("generated_tokens")
            if (not isinstance(tokens, list) or not 1 <= len(tokens) <= protocol["max_new_tokens"]
                    or any(type(t) is not int or not 0 <= t < protocol["vocab"] for t in tokens)):
                raise ValueError("generation scoring output/token bound")
            eos = next((i for i, t in enumerate(tokens) if t in protocol["eos_token_ids"]), None)
            if ((eos is not None and eos != len(tokens)-1)
                    or row.get("actual_generated_length")!=len(tokens)):
                raise ValueError("v2 output after EOS or false actual length")
            text = tokenizer.decode(tokens[:eos], skip_special_tokens=True).strip()
            normalize = lambda s: "".join(s.split()) if job["task"] == "code" else s
            passed = eos is not None and normalize(text) == normalize(job["expected_answer"])
            if (row.get("model_fed_tokens") != job["prompt_tokens"]+tokens[:-1]
                    or row.get("actual_fed_length") != len(job["prompt_tokens"])+len(tokens)-1):
                raise ValueError("v2 actual input IDs/length mismatch")
            images.append({"generated_tokens": tokens, "first_eos_index": eos,
                           "answer_text": text, "predeclared_task_answer_passed": passed})
        if (src.get("first_eos_index") != images[0]["first_eos_index"]
                or src.get("answer_text") != images[0]["answer_text"]
                or src.get("predeclared_task_answer_passed") != images[0]["predeclared_task_answer_passed"]
                or own.get("source_generated_tokens") != src["generated_tokens"]):
            raise ValueError("generation scoring supplied result differs from own decoding")
        eq = sum(a == b for a, b in zip(src["generated_tokens"], own["generated_tokens"]))
        comparison_slots += max(len(src["generated_tokens"]),len(own["generated_tokens"]))
        equal += eq
        exact += int(src["generated_tokens"] == own["generated_tokens"])
        source_passed += int(images[0]["predeclared_task_answer_passed"])
        integer_passed += int(images[1]["predeclared_task_answer_passed"])
        rows.append({"id": job["id"], "task": job["task"], "expected_answer": job["expected_answer"],
                     "source": images[0], "integer": images[1], "equal_generated_tokens": eq,
                     "exact_generated_job": src["generated_tokens"] == own["generated_tokens"],
                     "source_integer_actual_fed_token_ids_equal":src["model_fed_tokens"]==own["model_fed_tokens"],
                     "source_actual_fed_length":src["actual_fed_length"],"integer_actual_fed_length":own["actual_fed_length"]})
    n = len(rows)
    agreement, fraction = equal / comparison_slots, exact / n
    fidelity = agreement >= protocol["token_agreement_min"] and fraction >= protocol["exact_job_fraction_min"]
    if (integer.get("token_agreement") != agreement or integer.get("exact_job_fraction") != fraction
            or integer.get("predeclared_free_running_fidelity_passed") != fidelity):
        raise ValueError("generation scoring supplied aggregate differs from own count")
    answers = min(source_passed/n, integer_passed/n) >= protocol["source_and_integer_task_success_min"]
    return {"jobs": rows, "generated_token_agreement": agreement,"actual_compared_output_slots":comparison_slots, "exact_generated_job_fraction": fraction,
            "source_task_answer_fraction": source_passed/n, "integer_task_answer_fraction": integer_passed/n,
            "predeclared_free_running_fidelity_passed": fidelity, "predeclared_task_answers_passed": answers,
            "predeclared_generation_and_answer_thresholds_met": fidelity and answers}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    for name in ("model", "manifest", "protocol", "source", "integer", "output"):
        ap.add_argument(name, type=Path)
    a = ap.parse_args()
    if a.output.exists():
        raise ValueError("fresh generation score output required")
    protocol = pin.json_file(a.protocol, 64 << 10)
    protocol_check(protocol, pin.json_file(a.model / "config.json"))
    manifest = pin.json_file(a.manifest)
    if pin.sha256_file(a.manifest) != protocol["source_manifest_sha256"]:
        raise ValueError("generation scoring manifest byte pin")
    pins = pin.source_pins(a.model, manifest, protocol["max_source_bytes"])
    pin.verify_pins(a.model, pins)
    if pin.sha256_file(a.model / "tokenizer.json") != protocol["tokenizer_sha256"]:
        raise ValueError("generation scoring tokenizer byte pin")
    source, integer = pin.json_file(a.source, 64 << 10), pin.json_file(a.integer, 64 << 10)
    protocol_hash = pin.sha256_file(a.protocol)
    if (source.get("protocol_sha256") != protocol_hash or integer.get("protocol_sha256") != protocol_hash
            or integer.get("reference_json_sha256") != pin.sha256_file(a.source)
            or integer.get("artifact_sha256") != protocol["artifact_sha256"]):
        raise ValueError("generation scoring source/integer byte pins")
    for doc in (source, integer):
        if (doc.get("repository") != protocol["repository"] or doc.get("revision") != protocol["revision"]
                or doc.get("context") != protocol["context"] or doc.get("max_new_tokens") != protocol["max_new_tokens"]):
            raise ValueError("generation scoring checkpoint/extent")
    from tokenizers import Tokenizer
    tokenizer = Tokenizer.from_file(str(a.model / "tokenizer.json"))
    if any(tokenizer.encode(j["prompt_text"], add_special_tokens=False).ids != j["prompt_tokens"] for j in protocol["task_jobs"]):
        raise ValueError("generation scoring prompt/token identity")
    result = score(protocol, source, integer, tokenizer)
    result.update({"schema": "misaka.rfc02.independent-greedy-task-score.v2",
                   "protocol_sha256": protocol_hash, "source_json_sha256": pin.sha256_file(a.source),
                   "integer_json_sha256": pin.sha256_file(a.integer), "artifact_sha256": protocol["artifact_sha256"],
                   "source_manifest_sha256": protocol["source_manifest_sha256"], "tokenizer_sha256": protocol["tokenizer_sha256"],
                   "repository": protocol["repository"], "revision": protocol["revision"], "context": protocol["context"],
                   "decode_contract": "greedy-first-EOS-no-padding.v2",
                   "comparison_contract": "positional complete terminated outputs; missing positions fail; no padded credit",
                   "requested_max_tokens_per_job":protocol["max_new_tokens"],
                   "original_fixed32_records_and_thresholds_unchanged":True,
                   "complete_task_benchmark_or_maximum_context_proven": False,
                   "native_final_or_a6_a8_proven": False, "full_rfc02_complete": False})
    with a.output.open("x") as out:
        json.dump(result, out, ensure_ascii=False, indent=2)
        out.write("\n")
    print(json.dumps({k: result[k] for k in ("generated_token_agreement", "exact_generated_job_fraction",
                     "source_task_answer_fraction", "integer_task_answer_fraction", "predeclared_generation_and_answer_thresholds_met")}))
    if not result["predeclared_generation_and_answer_thresholds_met"]:
        raise SystemExit("predeclared generation/answer diagnostic failed; result retained")


if __name__ == "__main__":
    main()
