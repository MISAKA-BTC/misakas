#!/usr/bin/env python3
"""Bounded independent HF greedy jobs, with exact pins fixed before execution.

This compares finite text jobs, not a task benchmark, original maximum context,
chain admission or Final. The fixed-length job keeps generating after an EOS;
the first EOS is used only to display and judge the semantic answer. There is
no sampling, tokenizer template inference, remote code or adaptive threshold.
Declared model payload limits do not bound Torch allocator or process RSS.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import resource
import sys
import time

import hf_checkpoint_reference as pin

FIELDS = {
    "schema", "repository", "revision", "source_manifest_sha256",
    "artifact_sha256", "tokenizer_sha256", "context", "vocab",
    "max_source_bytes", "max_model_parameter_bytes", "max_total_positions",
    "max_new_tokens", "eos_token_ids", "task_jobs", "token_agreement_min",
    "exact_job_fraction_min", "source_and_integer_task_success_min", "tokenizer_config_sha256", "generation_config_sha256", "chat_template_sha256", "decode_contract",
}
JOB_FIELDS = {"id", "task", "prompt_text", "prompt_tokens", "expected_answer", "user_text"}


def protocol_check(doc: dict, config: dict) -> list[dict]:
    def integer(key, lo, hi):
        v = doc.get(key)
        if type(v) is not int or not lo <= v <= hi:
            raise ValueError(f"bounded protocol integer: {key}")
        return v

    if set(doc) != FIELDS or doc.get("schema") != "misaka.rfc02.greedy-task-protocol.v2":
        raise ValueError("exact generation protocol fields/schema")
    if doc["decode_contract"] != "greedy-first-EOS-no-padding.v2":
        raise ValueError("explicit first-EOS contract required")
    for key, size in [("tokenizer_config_sha256", 64), ("generation_config_sha256",64), ("chat_template_sha256",64), ("revision", 40), ("source_manifest_sha256", 64),
                      ("artifact_sha256", 64), ("tokenizer_sha256", 64)]:
        v = doc[key]
        if not isinstance(v, str) or len(v) != size or any(c not in "0123456789abcdef" for c in v):
            raise ValueError("generation protocol byte pins")
    if not isinstance(doc["repository"], str) or not doc["repository"]:
        raise ValueError("generation repository")
    context = integer("context", 1, min(4096, config.get("max_position_embeddings", 0)))
    vocab = integer("vocab", 1, 1 << 20)
    if vocab != config.get("vocab_size"):
        raise ValueError("source vocabulary differs from generation protocol")
    generated = integer("max_new_tokens", 1, 128)
    total_cap = integer("max_total_positions", 1, 4096)
    integer("max_source_bytes", 1, 16 << 30)
    integer("max_model_parameter_bytes", 1, 32 << 30)
    for key in ["token_agreement_min", "exact_job_fraction_min", "source_and_integer_task_success_min"]:
        if type(doc[key]) not in (int, float) or not 0 < doc[key] <= 1:
            raise ValueError("predeclared generation threshold")
    eos = doc["eos_token_ids"]
    if (not isinstance(eos, list) or not 1 <= len(eos) <= 8
            or any(type(t) is not int or not 0 <= t < vocab for t in eos)
            or len(set(eos)) != len(eos)):
        raise ValueError("generation EOS ids")
    jobs = doc["task_jobs"]
    if not isinstance(jobs, list) or not 1 <= len(jobs) <= 64:
        raise ValueError("generation job count")
    ids, total = set(), 0
    for job in jobs:
        if not isinstance(job, dict) or set(job) != JOB_FIELDS:
            raise ValueError("exact generation job fields")
        if (not isinstance(job["id"], str) or not 1 <= len(job["id"]) <= 64
                or job["id"] in ids or job["task"] not in ("arithmetic", "translation", "code")
                or not isinstance(job["user_text"], str) or not 1 <= len(job["user_text"]) <= 4096
                or not isinstance(job["prompt_text"], str) or not 1 <= len(job["prompt_text"]) <= 4096
                or not isinstance(job["expected_answer"], str) or not 1 <= len(job["expected_answer"]) <= 256):
            raise ValueError("generation job identity/task/text")
        ids.add(job["id"])
        tokens = job["prompt_tokens"]
        if (not isinstance(tokens, list) or not 1 <= len(tokens) <= context
                or any(type(t) is not int or not 0 <= t < vocab for t in tokens)
                or len(tokens) + generated - 1 > context):
            raise ValueError("generation prompt/output/context extent")
        total += len(tokens) + generated - 1
        if total > total_cap:
            raise ValueError("generation total position budget")
    return jobs


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("source", type=Path)
    ap.add_argument("source_manifest", type=Path)
    ap.add_argument("protocol", type=Path)
    ap.add_argument("output", type=Path)
    a = ap.parse_args()
    if os.environ.get("HF_HUB_OFFLINE") != "1" or a.output.exists():
        raise ValueError("offline mode and fresh output required")
    doc = pin.json_file(a.protocol, 64 << 10)
    config = pin.json_file(a.source / "config.json")
    jobs = protocol_check(doc, config)
    manifest = pin.json_file(a.source_manifest)
    if (pin.sha256_file(a.source_manifest) != doc["source_manifest_sha256"]
            or manifest["repository"] != doc["repository"] or manifest["revision"] != doc["revision"]):
        raise ValueError("generation source manifest byte identity")
    pins = pin.source_pins(a.source, manifest, doc["max_source_bytes"])
    if not any(p["path"] == "tokenizer.json" and p["sha256"] == doc["tokenizer_sha256"] for p in pins):
        raise ValueError("generation tokenizer byte identity")
    payload = pin.model_bytes(a.source, pins, doc["max_model_parameter_bytes"])
    pin.verify_pins(a.source, pins)
    import hashlib
    tc=pin.json_file(a.source / "tokenizer_config.json")
    gc=pin.json_file(a.source / "generation_config.json")
    if (pin.sha256_file(a.source / "tokenizer_config.json") != doc["tokenizer_config_sha256"]
            or pin.sha256_file(a.source / "generation_config.json") != doc["generation_config_sha256"]
            or not isinstance(tc.get("chat_template"),str)
            or hashlib.sha256(tc["chat_template"].encode()).hexdigest()!=doc["chat_template_sha256"]
            or gc.get("eos_token_id")!=doc["eos_token_ids"]):
        raise ValueError("publisher template/generation byte pins")
    from tokenizers import Tokenizer
    import tokenizers
    tokenizer = Tokenizer.from_file(str(a.source / "tokenizer.json"))
    from transformers import AutoTokenizer
    published=AutoTokenizer.from_pretrained(a.source, trust_remote_code=False,local_files_only=True)
    for j in jobs:
        messages=[{"role":"user","content":j["user_text"]}]
        if (published.apply_chat_template(messages,tokenize=False,add_generation_prompt=True)!=j["prompt_text"]
                or published.apply_chat_template(messages,tokenize=True,add_generation_prompt=True,return_dict=False)!=j["prompt_tokens"]):
            raise ValueError("publisher template differs from frozen shared input IDs")
    if any(tokenizer.encode(j["prompt_text"], add_special_tokens=False).ids != j["prompt_tokens"] for j in jobs):
        raise ValueError("generation declared prompt text/token mismatch")
    started = time.monotonic()
    import torch
    import transformers
    from transformers import AutoModelForCausalLM
    model = AutoModelForCausalLM.from_pretrained(
        a.source, torch_dtype=torch.float32, attn_implementation="eager",
        trust_remote_code=False, local_files_only=True,
    ).eval()
    if int(model.get_output_embeddings().weight.shape[0]) != doc["vocab"]:
        raise ValueError("loaded generation model vocabulary")
    results = []
    with torch.inference_mode():
        for job in jobs:
            tokens = list(job["prompt_tokens"])
            generated = []
            for _ in range(doc["max_new_tokens"]):
                logits = model(input_ids=torch.tensor([tokens]), use_cache=False).logits[0, -1]
                if logits.numel() != doc["vocab"] or not bool(torch.isfinite(logits).all()):
                    raise ValueError("generation finite logits/vocabulary")
                # torch.argmax picks the first maximum, matching lowest-id ties.
                token = int(torch.argmax(logits).item())
                generated.append(token)
                if token in doc["eos_token_ids"]:
                    break
                tokens.append(token)
            first_eos = next((i for i, t in enumerate(generated) if t in doc["eos_token_ids"]), None)
            answer = tokenizer.decode(generated[:first_eos], skip_special_tokens=True).strip()
            normalized = "".join(answer.split()) if job["task"] == "code" else answer
            expected = "".join(job["expected_answer"].split()) if job["task"] == "code" else job["expected_answer"]
            results.append({"id": job["id"], "task": job["task"], "prompt_tokens": job["prompt_tokens"],
                            "generated_tokens": generated, "actual_generated_length":len(generated),
                            "model_fed_tokens":job["prompt_tokens"]+generated[:-1], "actual_fed_length":len(job["prompt_tokens"])+len(generated)-1, "first_eos_index": first_eos,
                            "answer_text": answer, "predeclared_task_answer_passed": first_eos is not None and normalized == expected})
            print(f"source greedy job {job['id']}: {len(generated)} tokens", file=sys.stderr, flush=True)
    pin.verify_pins(a.source, pins)
    rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    if sys.platform != "darwin":
        rss *= 1024
    result = {
        "schema": "misaka.rfc02.independent-greedy-source.v2",
        "protocol_sha256": pin.sha256_file(a.protocol), "repository": doc["repository"], "revision": doc["revision"],
        "source_manifest_sha256": doc["source_manifest_sha256"], "tokenizer_sha256": doc["tokenizer_sha256"],
        "context": doc["context"], "max_new_tokens": doc["max_new_tokens"], "vocab": doc["vocab"], "jobs": results,
        "producer": {"implementation": "Transformers built-in source model", "model_class": type(model).__name__,
                     "torch": torch.__version__, "transformers": transformers.__version__, "tokenizers": tokenizers.__version__,
                     "arithmetic": "float32 eager CPU", "source_storage": "BF16 safetensors", "use_cache": False,
                     "trust_remote_code": False, "source_files": pins, "source_before_and_after_sha256_match": True,
                     "declared_float32_parameter_bytes": payload, "elapsed_seconds": time.monotonic()-started,
                     "whole_process_max_rss_bytes": rss, "revision_label_authenticated_by_this_tool": False},
        "fixed_count_generated_after_eos": False, "decode_contract":doc["decode_contract"],
        "actual_generated_length":sum(len(j["generated_tokens"]) for j in results), "requested_max_tokens_per_job":doc["max_new_tokens"], "complete_task_benchmark_or_maximum_context_proven": False,
        "native_final_or_a6_a8_proven": False, "full_rfc02_complete": False,
    }
    with a.output.open("x") as out:
        json.dump(result, out, ensure_ascii=False, indent=2)
        out.write("\n")


if __name__ == "__main__":
    main()
