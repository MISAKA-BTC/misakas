#!/usr/bin/env python3
"""New predeclared complete short tasks for a distinct borrowed integer graph.

Source inference and semantic grading remain outside consensus. EOS terminates
this diagnostic and must occur within the fixed cap. This does not alter native
fixed-count generation, earlier failed trials, or the A6/A8 population.
"""
import argparse
import ast
import datetime
import json
import math
import os
from pathlib import Path
import re
import resource
import sys
import time

import hf_checkpoint_reference as pin

SCHEMA = "misaka.rfc02.borrowed-context-complete-task-protocol.v1"
FIELDS = {"schema", "predeclared_utc", "repository", "revision", "source_manifest_sha256",
          "artifact_sha256", "parameter_map_sha256", "new_program_sha256", "tokenizer_sha256",
          "context", "vocab", "max_source_bytes", "max_model_parameter_bytes", "max_total_positions",
          "max_new_tokens", "eos_token_ids", "eos_rule", "answer_rules", "agreement_rule", "task_jobs",
          "token_agreement_min", "exact_job_fraction_min", "source_and_integer_task_success_min", "limits"}
JOB_FIELDS = {"id", "task", "prompt_text", "prompt_tokens", "answer_rule", "expected_answer"}


def protocol_check(doc, config):
    if set(doc) != FIELDS or doc["schema"] != SCHEMA:
        raise ValueError("exact new task protocol fields/schema")
    stamp = datetime.datetime.fromisoformat(doc["predeclared_utc"])
    if stamp.tzinfo is None:
        raise ValueError("predeclared UTC timestamp")
    for k in ("source_manifest_sha256", "artifact_sha256", "parameter_map_sha256", "new_program_sha256", "tokenizer_sha256", "revision"):
        size = 40 if k == "revision" else 64
        if not isinstance(doc[k], str) or len(doc[k]) != size or any(c not in "0123456789abcdef" for c in doc[k]):
            raise ValueError("exact task byte pins")
    for k, lo, hi in [("context", 128, 128), ("vocab", 151936, 151936), ("max_new_tokens", 32, 32),
                      ("max_total_positions", 1, 4096), ("max_source_bytes", 1, 16 << 30), ("max_model_parameter_bytes", 1, 32 << 30)]:
        if type(doc[k]) is not int or not lo <= doc[k] <= hi:
            raise ValueError("task preallocation bounds")
    if config.get("vocab_size") != doc["vocab"] or config.get("max_position_embeddings", 0) < doc["context"]:
        raise ValueError("source task vocabulary/context")
    for k in ("token_agreement_min", "exact_job_fraction_min", "source_and_integer_task_success_min"):
        if type(doc[k]) not in (int, float) or not math.isfinite(doc[k]) or not 0 < doc[k] <= 1:
            raise ValueError("finite fixed task gates")
    eos = doc["eos_token_ids"]
    if not isinstance(eos, list) or not 1 <= len(eos) <= 8 or len(set(eos)) != len(eos) or any(type(t) is not int or not 0 <= t < doc["vocab"] for t in eos):
        raise ValueError("bounded unique task EOS")
    jobs, ids, count = doc["task_jobs"], set(), 0
    if not isinstance(jobs, list) or not 1 <= len(jobs) <= 16:
        raise ValueError("task count")
    for j in jobs:
        if set(j) != JOB_FIELDS or j["id"] in ids or not isinstance(j["id"], str) or not 1 <= len(j["id"]) <= 64:
            raise ValueError("task identity")
        ids.add(j["id"])
        if (j["task"], j["answer_rule"]) not in (("arithmetic", "exact"), ("translation", "english-greeting"), ("code", "python-add-function")):
            raise ValueError("fixed task scoring rule")
        if not isinstance(j["prompt_text"], str) or not 1 <= len(j["prompt_text"]) <= 4096 or not isinstance(j["expected_answer"], str) or not 1 <= len(j["expected_answer"]) <= 256:
            raise ValueError("bounded task text")
        tokens = j["prompt_tokens"]
        if not isinstance(tokens, list) or not tokens or any(type(t) is not int or not 0 <= t < doc["vocab"] for t in tokens):
            raise ValueError("task input ids")
        extent = len(tokens) + doc["max_new_tokens"] - 1
        count += extent
        if extent > doc["context"] or count > doc["max_total_positions"]:
            raise ValueError("task total/context bound")
    return jobs


def answer_passed(job, text):
    if not isinstance(text, str) or len(text) > 4096:
        return False
    text = text.strip()
    if job["answer_rule"] == "exact":
        return text == job["expected_answer"]
    if job["answer_rule"] == "english-greeting":
        return text in ("Hello", "Hello.")
    if job["answer_rule"] != "python-add-function":
        return False
    if text.startswith("```"):
        fence = re.fullmatch(r"```(?:python)?\r?\n(.*?)\r?\n```", text, re.S)
        if fence is None:
            return False
        text = fence.group(1)
    try:
        tree = ast.parse(text)
    except (SyntaxError, ValueError, RecursionError):
        return False
    if len(tree.body) != 1 or not isinstance(tree.body[0], ast.FunctionDef):
        return False
    f = tree.body[0]
    a = f.args
    if (f.name != "add" or f.decorator_list or f.returns is not None or f.type_comment is not None or f.type_params
            or a.posonlyargs or a.vararg or a.kwarg or a.kwonlyargs or a.defaults or a.kw_defaults
            or [x.arg for x in a.args] != ["a", "b"] or any(x.annotation is not None for x in a.args)
            or len(f.body) != 1 or not isinstance(f.body[0], ast.Return)):
        return False
    x = f.body[0].value
    return (isinstance(x, ast.BinOp) and isinstance(x.op, ast.Add)
            and isinstance(x.left, ast.Name) and x.left.id == "a"
            and isinstance(x.right, ast.Name) and x.right.id == "b")


def image(doc, job, tokens, tokenizer):
    if not isinstance(tokens, list) or not 1 <= len(tokens) <= doc["max_new_tokens"] or any(type(t) is not int or not 0 <= t < doc["vocab"] for t in tokens):
        raise ValueError("bounded generated task ids")
    eos = next((i for i, t in enumerate(tokens) if t in doc["eos_token_ids"]), None)
    if eos is not None and eos != len(tokens) - 1 or eos is None and len(tokens) != doc["max_new_tokens"]:
        raise ValueError("exact task EOS/cap stopping rule")
    text = tokenizer.decode(tokens[:eos], skip_special_tokens=True).strip()
    return {"generated_tokens": tokens, "first_eos_index": eos, "answer_text": text,
            "completed": eos is not None, "task_answer_passed": eos is not None and answer_passed(job, text)}


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("mode", choices=("source", "score"))
    for name in ("model", "manifest", "protocol", "output"):
        ap.add_argument(name, type=Path)
    ap.add_argument("--source", type=Path)
    ap.add_argument("--integer", type=Path)
    a = ap.parse_args()
    if a.output.exists() or os.environ.get("HF_HUB_OFFLINE") != "1":
        raise ValueError("offline and fresh output required")
    doc = pin.json_file(a.protocol, 64 << 10)
    jobs = protocol_check(doc, pin.json_file(a.model / "config.json"))
    manifest = pin.json_file(a.manifest)
    if pin.sha256_file(a.manifest) != doc["source_manifest_sha256"] or manifest["repository"] != doc["repository"] or manifest["revision"] != doc["revision"]:
        raise ValueError("task manifest byte identity")
    pins = pin.source_pins(a.model, manifest, doc["max_source_bytes"])
    payload = pin.model_bytes(a.model, pins, doc["max_model_parameter_bytes"])
    pin.verify_pins(a.model, pins)
    if pin.sha256_file(a.model / "tokenizer.json") != doc["tokenizer_sha256"]:
        raise ValueError("task tokenizer identity")
    from tokenizers import Tokenizer
    tokenizer = Tokenizer.from_file(str(a.model / "tokenizer.json"))
    if any(tokenizer.encode(j["prompt_text"], add_special_tokens=False).ids != j["prompt_tokens"] for j in jobs):
        raise ValueError("task text/ids identity")
    ph = pin.sha256_file(a.protocol)
    if a.mode == "source":
        started = time.monotonic()
        import torch
        import transformers
        model = transformers.AutoModelForCausalLM.from_pretrained(a.model, torch_dtype=torch.float32, attn_implementation="eager", trust_remote_code=False, local_files_only=True).eval()
        rows = []
        with torch.inference_mode():
            for job in jobs:
                fed, generated = list(job["prompt_tokens"]), []
                for _ in range(doc["max_new_tokens"]):
                    logits = model(input_ids=torch.tensor([fed]), use_cache=False).logits[0, -1]
                    if logits.numel() != doc["vocab"] or not bool(torch.isfinite(logits).all()):
                        raise ValueError("finite source task logits")
                    token = int(torch.argmax(logits).item())
                    generated.append(token)
                    if token in doc["eos_token_ids"]:
                        break
                    fed.append(token)
                rows.append({"id": job["id"], "task": job["task"], "prompt_tokens": job["prompt_tokens"], **image(doc, job, generated, tokenizer)})
                print(f"source complete task {job['id']}: {len(generated)} output ids", file=sys.stderr, flush=True)
        pin.verify_pins(a.model, pins)
        rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * (1 if sys.platform == "darwin" else 1024)
        result = {"schema": "misaka.rfc02.borrowed-context-task-source.v1", "jobs": rows,
                  "producer": {"model_class": type(model).__name__, "torch": torch.__version__, "transformers": transformers.__version__,
                               "arithmetic": "float32 eager CPU", "trust_remote_code": False, "use_cache": False,
                               "source_files": pins, "source_before_and_after_sha256_match": True, "declared_float32_parameter_bytes": payload,
                               "elapsed_seconds": time.monotonic() - started, "max_rss_bytes": rss, "revision_label_authenticated": False}}
    else:
        if a.source is None or a.integer is None:
            raise ValueError("source and integer task results required")
        src, own = pin.json_file(a.source, 64 << 10), pin.json_file(a.integer, 64 << 10)
        for x in (src, own):
            if x.get("protocol_sha256") != ph or len(x.get("jobs", [])) != len(jobs):
                raise ValueError("task result policy/count")
        if src.get("schema") != "misaka.rfc02.borrowed-context-task-source.v1" or own.get("schema") != "misaka.rfc02.borrowed-context-task-integer.v1" or own.get("reference_json_sha256") != pin.sha256_file(a.source):
            raise ValueError("task result source/byte identities")
        rows, equal, denom, exact, good_s, good_i = [], 0, 0, 0, 0, 0
        for job, s, i in zip(jobs, src["jobs"], own["jobs"]):
            if any(x.get("id") != job["id"] or x.get("task") != job["task"] or x.get("prompt_tokens") != job["prompt_tokens"] for x in (s, i)):
                raise ValueError("task result job identity")
            si, ii = image(doc, job, s["generated_tokens"], tokenizer), image(doc, job, i["generated_tokens"], tokenizer)
            if any(s.get(k) != v for k, v in si.items()):
                raise ValueError("source answer independently decoded/scored differs")
            if i.get("first_eos_index") != ii["first_eos_index"] or i.get("completed") != ii["completed"]:
                raise ValueError("integer completion differs from independently decoded EOS")
            eq = sum(a == b for a, b in zip(si["generated_tokens"], ii["generated_tokens"]))
            equal += eq
            denom += max(len(si["generated_tokens"]), len(ii["generated_tokens"]))
            exact += si["generated_tokens"] == ii["generated_tokens"]
            good_s += si["task_answer_passed"]
            good_i += ii["task_answer_passed"]
            rows.append({"id": job["id"], "task": job["task"], "source": si, "integer": ii, "equal_ids": eq})
        n = len(jobs)
        fidelity = equal / denom >= doc["token_agreement_min"] and exact / n >= doc["exact_job_fraction_min"]
        answers = min(good_s / n, good_i / n) >= doc["source_and_integer_task_success_min"]
        if own.get("raw_id_agreement") != equal / denom or own.get("exact_job_fraction") != exact / n or own.get("free_running_fidelity_passed") != fidelity:
            raise ValueError("integer aggregate differs from independent output-id count")
        result = {"schema": "misaka.rfc02.borrowed-context-task-score.v1", "jobs": rows, "raw_id_agreement": equal / denom,
                  "exact_job_fraction": exact / n, "source_task_success_fraction": good_s / n, "integer_task_success_fraction": good_i / n,
                  "free_running_fidelity_passed": fidelity, "both_task_answers_passed": answers,
                  "all_predeclared_gates_passed": fidelity and answers, "source_sha256": pin.sha256_file(a.source), "integer_sha256": pin.sha256_file(a.integer)}
    if pin.sha256_file(a.protocol) != ph:
        raise ValueError("task policy changed during run")
    result.update({"protocol_sha256": ph, "repository": doc["repository"], "revision": doc["revision"], "context": doc["context"],
                   "new_program_sha256": doc["new_program_sha256"], "max_new_tokens": doc["max_new_tokens"],
                   "native_fixed_count_semantics_replaced": False, "full_benchmark_maxcontext_native_final_or_cohort_proven": False})
    with a.output.open("x") as out:
        json.dump(result, out, indent=2, ensure_ascii=False)
        out.write("\n")
    if a.mode == "score" and not result["all_predeclared_gates_passed"]:
        raise SystemExit("predeclared complete task diagnostic failed; result retained")


if __name__ == "__main__":
    main()
