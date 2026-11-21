#!/usr/bin/env python3
"""Check QF1 logical codec/position examples. No model inference or protocol execution."""
from __future__ import annotations

from dataclasses import replace
import hashlib
import json
from pathlib import Path
import struct
import sys

sys.dont_write_bytecode = True
import qf1_reference as q

HERE = Path(__file__).resolve().parent


def main() -> int:
    checks = []

    def check(name, condition):
        if not condition:
            raise AssertionError(name)
        checks.append(name)

    def rejected(name, f):
        try:
            f()
        except ValueError:
            checks.append(name)
        else:
            raise AssertionError(name)

    # Independent examples for the prompt/decode boundary, especially T=1 and EOS.
    check("first EOS causes no extra forward", q.schedule([3, 4, 5], [151645], 8) == {
        "decision_positions": [2], "decision_input_tokens": [5],
        "processed_positions": 3, "last_selected_token_is_forwarded": False, "stop_reason": "EOS"})
    check("last chosen token is not decoder input", q.schedule([3, 4], [100, 101, 102], 3)["decision_input_tokens"] == [4, 100, 101])
    check("decode position alignment", q.schedule([3, 4], [100, 101, 102], 3)["decision_positions"] == [1, 2, 3])
    check("EOS wins over LIMIT", q.schedule([1], [151643], 1)["stop_reason"] == "EOS")
    check("context equality boundary", q.schedule([1] * 8192, [7], 1)["processed_positions"] == 8192)
    rejected("no success before EOS or limit", lambda: q.schedule([1], [7], 2))
    rejected("cannot continue after EOS", lambda: q.schedule([1], [151645, 7], 2))
    rejected("no empty prompt", lambda: q.schedule([], [7], 1))
    rejected("no bool token", lambda: q.schedule([True], [7], 1))
    rejected("no bool limit", lambda: q.schedule([1], [7], True))
    rejected("no float token", lambda: q.schedule([1.0], [7], 1))
    rejected("reject context overflow before execution", lambda: q.schedule([1] * 8192, [151645], 2))

    # A synthetic injective table tests the nibble mapping only, not a Qwen tokenizer.
    table = list(range(16))
    frame = q.hex_frame(bytes(range(32)), table)
    expected = [int(ch, 16) for ch in bytes(range(32)).hex()]
    check("hex order matches canonical lower-case hex", frame == expected and len(frame) == 64)
    all_pairs = {tuple(q.hex_frame(bytes([b]) * 32, table)[:2]) for b in range(256)}
    check("all byte values have distinct hex pairs", len(all_pairs) == 256)
    rejected("duplicate hex token mapping rejected", lambda: q.hex_frame(bytes(32), [1] * 16))

    states = [q.synthetic_step(0)]
    message = b"".join(q.readout_parts(states))
    # Construct bytes independently without the reference's encoder.
    independent = b"misaka/palw/qf1/readout/v1\x00" + b"\x01\x00\x00\x00\x00"
    flat = [value for vector in states[0] for value in vector]
    independent += struct.pack("<151552h", *flat) + b"\xff\x01\x00\x00\x00"
    check("format equals separately packed expected message", message == independent)
    check("readout message length", len(message) == len(q.DOM_Z) + 303109 + 5)
    z = q.readout(states)
    check("streaming and concatenated digest agree", z == hashlib.blake2b(independent).digest())
    varied = hashlib.blake2b()
    for i in range(0, len(message), 997):
        varied.update(message[i:i + 997])
    check("hash update chunking has no meaning", varied.digest() == z)
    moved = [states[0].copy()]
    moved[0][0], moved[0][1] = moved[0][1], moved[0][0]
    check("swapped layer positions change encoded candidate", q.readout(moved) != z)
    bad = [v.copy() for v in states[0]]
    bad[0][0] = -32768
    rejected("forbidden i16 code rejected", lambda: q.encode_states(bad))
    bad[0][0] = 1.0
    rejected("float state rejected", lambda: q.encode_states(bad))
    bad[0][0] = True
    rejected("bool state rejected", lambda: q.encode_states(bad))
    rejected("one missing vector rejected", lambda: q.encode_states(states[0][:-1]))
    rejected("truncated vector rejected", lambda: q.encode_states([v[:-1] for v in states[0]]))

    # Check a separately transcribed compression chain against hashlib, including exact
    # block/chunk endings, where the final flag/counter rules often go wrong.
    for n in (1, 127, 128, 129, 4095, 4096, 4097):
        m = bytes(i % 251 for i in range(n))
        records = q.chunk_records(m)
        check("compression matches hashlib at length " + str(n),
              records[-1].chain_after == hashlib.blake2b(m).digest())
    records = q.chunk_records(message)
    check("full synthetic readout compression agrees", records[-1].chain_after == z)
    check("chunk count is derived", len(records) == (len(message) + 4095) // 4096)
    check("chunk record length and counter", records[0].length == 4096 and records[0].consumed == 4096)
    wrong_counter = records.copy()
    wrong_counter[0] = replace(wrong_counter[0], consumed=4097)
    check("wrong chunk counter rejected", not q.verify_chunks(message, wrong_counter, z))
    wrong_final = records.copy()
    wrong_final[0] = replace(wrong_final[0], final_flag=1)
    check("early final flag rejected", not q.verify_chunks(message, wrong_final, z))
    wrong_chain = records.copy()
    wrong_chain[-1] = replace(wrong_chain[-1], chain_after=bytes(64))
    check("fabricated hash state rejected", not q.verify_chunks(message, wrong_chain, z))

    s = bytes(range(32))
    leaves = [struct.pack("<I", t) for t in (1, 2, 3)]
    rx = q.root(s, 1, leaves)
    proof = q.opening(s, 1, leaves, 2)
    check("canonical padding and opening accepted", q.verify_opening(s, 1, 3, 2, leaves[2], proof, rx))
    check("same value wrong index rejected", not q.verify_opening(s, 1, 3, 1, leaves[2], proof, rx))
    check("same leaf wrong attempt rejected", not q.verify_opening(bytes(32), 1, 3, 2, leaves[2], proof, rx))
    check("same leaf wrong tree kind rejected", not q.verify_opening(s, 2, 3, 2, leaves[2], proof, rx))
    check("wrong leaf count rejected", not q.verify_opening(s, 1, 4, 2, leaves[2], proof, rx))
    one = q.root(s, 3, [z])
    manual = hashlib.blake2b(q.DOM_MERKLE + b"\0" + s + b"\x03" + struct.pack("<Q", 1)
                            + struct.pack("<Q", 0) + struct.pack("<I", 64) + z).digest()
    check("D=1 has no parent and matches direct preimage", one == manual)
    rejected("empty tree rejected", lambda: q.root(s, 1, []))
    rejected("z must be one full leaf", lambda: q.root(s, 3, [z, z]))
    rejected("bool tree kind rejected", lambda: q.root(s, True, leaves))

    roots = [rx, q.root(s, 2, [struct.pack("<I", 151645)]), one,
             q.root(s, 4, [b"SYNTHETIC-NOT-A-VALID-TRACE"])]
    counts = [3, 1, 1, 1]
    t = q.ticket(s, counts, roots)
    ticket_message = q.DOM_TICKET + s + b"".join(struct.pack("<Q", n) + r for n, r in zip(counts, roots))
    check("Ticket uses exact four-root byte order", t == hashlib.blake2b(ticket_message, digest_size=32).digest())
    check("BLAKE2b-256 is not 512-bit truncation", t != hashlib.blake2b(ticket_message).digest()[:32])
    number = int.from_bytes(t, "big")
    check("strict target boundary", not q.is_winner(t, number) and q.is_winner(t, number + 1))
    check("full-width target boundary", not q.is_winner(t, 0) and q.is_winner(t, 1 << 256))
    rejected("target too wide rejected", lambda: q.is_winner(t, (1 << 256) + 1))
    rejected("bool target rejected", lambda: q.is_winner(t, True))
    check("attempt context is length-delimited", q.attempt(b"ab", b"c") != q.attempt(b"a", b"bc"))

    vectors = json.loads((HERE / "conformance-vectors.json").read_text())
    check("published readout vector unchanged", vectors["synthetic_one_step"]["z_hex"] == z.hex())
    check("published Merkle vector unchanged", vectors["synthetic_one_step"]["input_root_hex"] == rx.hex())
    check("published Ticket vector unchanged", vectors["synthetic_one_step"]["ticket_hex"] == t.hex())

    result = {
        "passed": True, "checks": len(checks), "check_names": checks,
        "scope": "logical codec, synthetic positions and BLAKE2b chunk-state examples only",
        "synthetic_message_bytes": len(message), "synthetic_hash_chunks": len(records),
        "qwen_executed": False, "qwen_quality_measured": False,
        "G_COLL_proved": False, "G_COST_proved": False, "native_court_tested": False,
    }
    print(json.dumps(result, ensure_ascii=False, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
