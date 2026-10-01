#!/usr/bin/env python3
"""Golden vectors of RFC-0003's generative job, `PalwGenJobV1` (§I.0), from an implementation that shares no
code with the Rust one: Borsh by hand, BLAKE2b from `hashlib`.

    python3 scripts/palw-gen-job-vectors.py            # rewrites consensus-vectors/gen-v1/job_v1.json
    python3 scripts/palw-gen-job-vectors.py --check    # compares, writes nothing (exit 1 on a difference)

What is pinned (the Rust side holds the same bytes: `consensus/core/tests/palw_gen_job_vectors.rs`):

* the canonical encoding of a job — `version u16 | envelope | seed [u8;32] | body`, the body an enum
  (`0` Image, `1` Embedding), an embedding's input an enum (`0` Text, `1` Image) — and the job id,
  `gen_job_id_v1 = H64(key "misaka-palw/gen-v1/job-id/v1", borsh(job))`;
* `gen_canonical_seed_v1(anchor) = H64(key "misaka-palw/gen-v1/canonical-seed/v1", anchor)[..32]`;
* the encodings that are NOT canonical (a trailing byte, a truncation, an unknown body tag, a known tag
  with the wrong width) — each refused by `decode_canonical`.

Borsh: integers little-endian, `[u8; N]` raw, `Vec<u8>` a `u32` length then the bytes, an enum a `u8` tag then
its fields in declaration order, `Hash64` raw 64 bytes, `TransactionOutpoint` = transaction id (64 bytes) then
`index: u32`.
"""
import hashlib
import json
import struct
import sys
from pathlib import Path

JOB_ID_KEY = b"misaka-palw/gen-v1/job-id/v1"
SEED_KEY = b"misaka-palw/gen-v1/canonical-seed/v1"
VERSION = 1
PRIVACY_PUBLIC_DA = 1
PRIVACY_PANEL_DA = 2
PROMPT_MODE_USER = 0
OUT_IMAGE_RGB8 = 1
OUT_EMBEDDING_I32 = 3
POOLING_CLS, POOLING_MEAN, POOLING_LAST = 1, 2, 3


def h64(key: bytes, data: bytes) -> bytes:
    return hashlib.blake2b(data, digest_size=64, key=key).digest()


def u8(x): return struct.pack("<B", x)
def u16(x): return struct.pack("<H", x)
def u32(x): return struct.pack("<I", x)
def u64(x): return struct.pack("<Q", x)
def vec_u8(b): return u32(len(b)) + b


def filled(n, byte):
    return bytes([byte]) * n


def envelope(class_id, privacy=PRIVACY_PANEL_DA, prompt_mode=PROMPT_MODE_USER, nonce_byte=0x9C):
    return (
        filled(64, 0xD0)                      # network_domain
        + class_id                            # class_id
        + filled(64, 7) + u32(3)              # executor_bond: TransactionOutpoint
        + vec_u8(filled(8, 0xAB))             # executor_pubkey
        + filled(64, 0x0E)                    # operator_id
        + filled(64, 0xA1)                    # anchor_block
        + u64(4242)                           # anchor_daa
        + filled(32, nonce_byte)              # job_nonce
        + u8(privacy) + u8(prompt_mode)
    )


def image_body(prompt_hash, prompt_tokens, neg_hash, neg_tokens, guidance_q, image_index, sampler, steps, width, height, output=OUT_IMAGE_RGB8):
    return (
        u8(0)
        + prompt_hash + u32(prompt_tokens)
        + neg_hash + u32(neg_tokens)
        + u16(guidance_q) + u16(image_index)
        + sampler + u16(steps) + u16(width) + u16(height) + u8(output)
    )


def embedding_text_body(token_hash, tokens, pooling, dims, output=OUT_EMBEDDING_I32):
    return u8(1) + u8(0) + token_hash + u32(tokens) + u8(pooling) + u32(dims) + u8(output)


def embedding_image_body(input_root, h, w, pooling, dims, output=OUT_EMBEDDING_I32):
    return u8(1) + u8(1) + input_root + u32(h) + u32(w) + u8(pooling) + u32(dims) + u8(output)


def job(env, seed, body, version=VERSION):
    return u16(version) + env + seed + body


def case(name, note, raw, **fields):
    return {
        "name": name,
        "note": note,
        "borsh_hex": raw.hex(),
        "job_id_hex": h64(JOB_ID_KEY, raw).hex(),
        **fields,
    }


def build():
    class_a = filled(64, 0xC1)
    class_b = filled(64, 0xC2)
    seed = bytes(range(32))
    zero_seed = bytes(32)
    ph = filled(64, 0x11)
    sampler = filled(64, 0x5A)
    cases = [
        case(
            "image-guided",
            "an image job: a prompt, no negative prompt, guidance 28/16, the second offered step count, image 3 of a batch",
            job(envelope(class_a), seed, image_body(ph, 5, bytes(64), 0, 28, 3, sampler, 20, 512, 512)),
            profile="image",
            seed_hex=seed.hex(),
        ),
        case(
            "image-true-cfg",
            "an image job with a negative prompt (the class offers true CFG) over PublicDa",
            job(envelope(class_a, privacy=PRIVACY_PUBLIC_DA), seed, image_body(ph, 5, filled(64, 0x22), 3, 112, 0, sampler, 28, 1024, 1024)),
            profile="image",
            seed_hex=seed.hex(),
        ),
        case(
            "embedding-text",
            "an embedding job over text ids: mean pooling, 384 dimensions, a zero seed (a class that draws no randomness)",
            job(envelope(class_b), zero_seed, embedding_text_body(filled(64, 0x33), 7, POOLING_MEAN, 384)),
            profile="embedding",
            seed_hex=zero_seed.hex(),
        ),
        case(
            "embedding-image",
            "an embedding job over one canonical image: the reference only (input root, height, width), CLS pooling",
            job(envelope(class_b), zero_seed, embedding_image_body(filled(64, 0x44), 224, 224, POOLING_CLS, 768)),
            profile="embedding",
            seed_hex=zero_seed.hex(),
        ),
    ]
    good = job(envelope(class_b), zero_seed, embedding_text_body(filled(64, 0x33), 7, POOLING_MEAN, 384))
    not_canonical = [
        {"name": "trailing-byte", "note": "a byte after the job", "borsh_hex": (good + b"\x00").hex()},
        {"name": "truncated", "note": "the job's last byte missing", "borsh_hex": good[:-1].hex()},
        {
            "name": "unknown-body-tag",
            "note": "a body tag past the built profiles (audio and video are not built)",
            "borsh_hex": job(envelope(class_b), zero_seed, u8(2) + b"\x00" * 16).hex(),
        },
        {
            "name": "unknown-input-tag",
            "note": "an embedding input tag that is neither Text (0) nor Image (1)",
            "borsh_hex": job(envelope(class_b), zero_seed, u8(1) + u8(2) + filled(80, 1)).hex(),
        },
        {"name": "empty", "note": "no bytes", "borsh_hex": ""},
    ]
    anchors = [bytes(64), filled(64, 0xA1), bytes(range(64))]
    seeds = [{"anchor_hex": a.hex(), "seed_hex": h64(SEED_KEY, a)[:32].hex()} for a in anchors]
    return {
        "format": "palw-gen-v1/job-vectors/1",
        "spec": "docs/rfc/0003-palw-generative-model-classes.md §I.0; docs/spec/palw/04b-tensor-ir.md §15",
        "generator": "scripts/palw-gen-job-vectors.py",
        "job_id_key": JOB_ID_KEY.decode(),
        "canonical_seed_key": SEED_KEY.decode(),
        "cases": cases,
        "not_canonical": not_canonical,
        "canonical_seeds": seeds,
    }


def main():
    out = Path(__file__).resolve().parent.parent / "consensus-vectors" / "gen-v1" / "job_v1.json"
    text = json.dumps(build(), indent=2) + "\n"
    if "--check" in sys.argv:
        if not out.exists() or out.read_text() != text:
            print(f"{out} differs from the generator's output", file=sys.stderr)
            sys.exit(1)
        print(f"{out} is the generator's output")
        return
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text)
    print(f"wrote {out} ({len(build()['cases'])} jobs, {len(build()['not_canonical'])} refusals)")


if __name__ == "__main__":
    main()
