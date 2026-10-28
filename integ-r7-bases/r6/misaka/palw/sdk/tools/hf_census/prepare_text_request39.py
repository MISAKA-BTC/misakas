#!/usr/bin/env python3
"""Pinned original text-only frontend -> canonical native39 request adapter.

For the dense text members of the initial seven (Qwen2.5-1.5B PALW A16,
Llama 3.1 8B, Qwen3-14B, Qwen3-32B, Llama 3.3 70B) and the Qwen3.6-35B-A3B
PALWQ361 runtime. The publisher's tokenizer.json and its chat template
(tokenizer_config.json) are independently SHA-256 pinned; the template is
rendered with a sandboxed Jinja environment and tokenized by the pinned
`tokenizers` implementation. EOS ids come from the pinned generation config.

Output schema `misaka.palw.original-text-request-adapter.v39`, consumed by
`bind_original_text_request_v39` (Rust), which re-checks the tokenizer pin,
template string, context bound and vocabulary. No inference, Final,
eligibility or fidelity is asserted; every stage status is NOT_RUN.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import sys

SCHEMA = "misaka.palw.original-text-request-adapter.v39"


def fail(message: str):
    raise ValueError(message)


def pinned(path: pathlib.Path, expected: str, maximum: int) -> bytes:
    if len(expected) != 64:
        fail("TEXT39_PIN_FORMAT")
    if path.stat().st_size > maximum:
        fail("TEXT39_FILE_EXTENT: " + str(path))
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        fail("TEXT39_INDEPENDENT_FILE_SHA_DIFFERS: " + str(path))
    return data


def render(template: str, messages: list, special: dict) -> str:
    try:
        from jinja2.sandbox import ImmutableSandboxedEnvironment
    except ImportError:
        fail("TEXT39_JINJA2_REQUIRED_FOR_PUBLISHER_TEMPLATE")
    env = ImmutableSandboxedEnvironment(trim_blocks=True, lstrip_blocks=True)

    def raise_exception(message):
        raise ValueError("TEXT39_TEMPLATE_RAISED: " + str(message))

    env.globals["raise_exception"] = raise_exception
    return env.from_string(template).render(messages=messages, add_generation_prompt=True, **special)


def main(argv=None) -> int:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--tokenizer-json", required=True)
    p.add_argument("--tokenizer-sha256", required=True)
    p.add_argument("--tokenizer-config", required=True)
    p.add_argument("--tokenizer-config-sha256", required=True)
    p.add_argument("--generation-config", required=True)
    p.add_argument("--generation-config-sha256", required=True)
    p.add_argument("--messages-json", required=True, help="the original task messages (role/content list)")
    p.add_argument("--messages-sha256", required=True)
    p.add_argument("--original-context-max", type=int, required=True)
    p.add_argument("--requested-new-tokens", type=int, required=True)
    p.add_argument("--out", required=True)
    a = p.parse_args(argv)
    out = pathlib.Path(a.out)
    if out.exists():
        fail("TEXT39_FRESH_OUTPUT_REQUIRED")
    if a.original_context_max <= 0 or a.requested_new_tokens <= 0:
        fail("TEXT39_POSITIVE_CONTEXT_AND_REQUEST")
    tok_raw = pinned(pathlib.Path(a.tokenizer_json), a.tokenizer_sha256, 64 << 20)
    config = json.loads(pinned(pathlib.Path(a.tokenizer_config), a.tokenizer_config_sha256, 16 << 20))
    generation = json.loads(pinned(pathlib.Path(a.generation_config), a.generation_config_sha256, 1 << 20))
    messages = json.loads(pinned(pathlib.Path(a.messages_json), a.messages_sha256, 16 << 20))
    template = config.get("chat_template")
    if isinstance(template, list):
        default = [t for t in template if isinstance(t, dict) and t.get("name") == "default"]
        template = default[0]["template"] if default else None
    if not isinstance(template, str) or not template:
        fail("TEXT39_PUBLISHER_CHAT_TEMPLATE_ABSENT")
    special = {k: v for k, v in config.items() if k.endswith("_token") and isinstance(v, str)}
    text = render(template, messages, special)
    try:
        from tokenizers import Tokenizer
    except ImportError:
        fail("TEXT39_TOKENIZERS_REQUIRED")
    tokenizer = Tokenizer.from_str(tok_raw.decode("utf-8"))
    prompt = tokenizer.encode(text, add_special_tokens=False).ids
    eos = generation.get("eos_token_id")
    eos = sorted(set(eos if isinstance(eos, list) else [eos]))
    if not prompt or not eos or any(not isinstance(e, int) or e < 0 for e in eos):
        fail("TEXT39_PROMPT_OR_EOS_ABSENT")
    if len(prompt) + a.requested_new_tokens - 1 > a.original_context_max:
        fail("TEXT39_REQUEST_EXCEEDS_ORIGINAL_CONTEXT")
    record = {
        "schema": SCHEMA,
        "template": "hf-chat-template:" + hashlib.sha256(template.encode("utf-8")).hexdigest(),
        "original_context_max": a.original_context_max,
        "source_pins": {
            "tokenizer": a.tokenizer_sha256,
            "tokenizer_config": a.tokenizer_config_sha256,
            "generation_config": a.generation_config_sha256,
            "messages": a.messages_sha256,
        },
        "prompt": prompt,
        "eos": eos,
        "requested_new_tokens": a.requested_new_tokens,
        "images": [],
        "inference": "NOT_RUN",
        "native_final": "NOT_RUN",
    }
    out.write_text(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
    print(json.dumps({"out": str(out), "sha256": hashlib.sha256(out.read_bytes()).hexdigest(), "prompt_tokens": len(prompt)}))
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except ValueError as e:
        print(str(e), file=sys.stderr)
        sys.exit(1)
