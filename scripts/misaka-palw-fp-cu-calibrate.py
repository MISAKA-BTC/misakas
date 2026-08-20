#!/usr/bin/env python3
"""FP CU-weight calibration (ADR-0044 Decision 7, FP-09).

The CU price table says what a job is WORTH: `cu = prompt_tokens·prefill_weight +
decode_tokens·decode_weight`. Those two numbers are consensus parameters, and the safety
direction is one-sided — a mispriced table may only ever UNDER-pay prefill relative to decode.
Over-pricing prefill is a grinding lever: an attacker buys CU with the phase that is cheap for
them and expensive for nobody.

So the weights are not a taste question, they are a measurement of the reference class. This
drives the real gateway with the real model over a grid of shapes and fits

    T(p, d)  =  a  +  b·p  +  c·d

by ordinary least squares, where `a` absorbs per-request fixed cost (HTTP, model already resident,
sampler setup). The measured cost ratio is `c/b`, and the shipped `decode_weight/prefill_weight`
must be at least that — equal is exact, larger is conservative in the safe direction.

usage: misaka-palw-fp-cu-calibrate.py <misaka-palw-gateway> <palw-worker> <gguf> [repeats]

Prints a table and the verdict for the constants in `consensus/core/src/palw_fp_devnet_v3.rs`.
This is a MEASUREMENT script, not a test: it has no pass/fail on the fit itself, because the
answer depends on the machine it runs on. What it does assert is that the run was clean — every
request answered, every shape as requested — so a table nobody can reproduce is not published as
a measurement.
"""
import http.client
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time

GATEWAY, WORKER, GGUF = sys.argv[1], sys.argv[2], sys.argv[3]
REPEATS = int(sys.argv[4]) if len(sys.argv) > 4 else 2
PORT = 18847

# The grid. Prompt lengths span the worker's single-batch prefill range; decode budgets span the
# short-answer regime a chat gateway actually serves. Both axes must vary independently or the
# two coefficients are not separable.
# The worker prefills in ONE batch (`n_batch = 512`) and refuses anything longer, so the grid
# must stay inside that — a sweep that walks off the schedule measures the refusal path, not the
# cost. The filler is sized in repetitions and the top of the range is checked against the cap by
# the assertion below rather than assumed.
FILLER = "The quick brown fox jumps over the lazy dog. "
PROMPT_REPEATS = [1, 8, 20, 34]
DECODE_BUDGETS = [16, 48, 128]
SINGLE_BATCH_PREFILL = 512

work = pathlib.Path(tempfile.mkdtemp(prefix="palw-fp-cu-calibrate."))
outbox = work / "outbox"
identity = work / "identity.json"
anchor = work / "anchor.json"
identity.write_text(json.dumps({
    "network_domain": "4e" * 64,
    "class_id": "c1" * 64,
    "bond_txid": "b0" * 64,
    "bond_index": 0,
    "executor_pubkey": "07" * 32,
    "operator_id": "e0" * 64,
}))
anchor.write_text(json.dumps({"anchor_block": "a0" * 64, "anchor_daa": 5000}))

env = dict(os.environ)
env["MISAKA_PALW_GGUF"] = GGUF
gateway = subprocess.Popen(
    [GATEWAY, "--listen", f"127.0.0.1:{PORT}", "--worker", WORKER, "--outbox", str(outbox),
     "--identity", str(identity), "--anchor", str(anchor), "--quantum-cu", "1000"],
    env=env, stderr=subprocess.PIPE, text=True,
)


def chat(prompt: str, max_tokens: int):
    body = json.dumps({
        "model": "misaka-palw-fp-v3",
        "messages": [{"role": "user", "content": prompt}],
        "max_tokens": max_tokens,
    }).encode()
    conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=1800)
    t0 = time.perf_counter()
    conn.request("POST", "/v1/chat/completions", body=body, headers={"Content-Type": "application/json"})
    payload = json.loads(conn.getresponse().read())
    elapsed = time.perf_counter() - t0
    if "usage" not in payload:
        raise RuntimeError(f"gateway did not answer a chat: {payload}")
    return elapsed, payload["usage"]["prompt_tokens"], payload["usage"]["completion_tokens"], payload["misaka"]["cu"]


try:
    deadline = time.time() + 60
    while True:
        try:
            conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=2)
            conn.request("GET", "/health")
            health = json.loads(conn.getresponse().read())
            assert health["status"] == "ok"
            break
        except (ConnectionRefusedError, OSError):
            if time.time() > deadline:
                raise RuntimeError("gateway did not come up")
            if gateway.poll() is not None:
                raise RuntimeError(f"gateway died: {gateway.stderr.read()}")
            time.sleep(0.3)
    print(f"runtime manifest {health['runtime_manifest_hash'][:16]}…  template {health['template_id']}")

    # One warm-up so the first row does not carry model load / page-in cost into `a`.
    chat("Say ok.", 8)

    rows = []
    for reps in PROMPT_REPEATS:
        prompt = "Summarize the following text in one sentence.\n\n" + FILLER * reps
        for budget in DECODE_BUDGETS:
            for _ in range(REPEATS):
                t, p, d, cu = chat(prompt, budget)
                assert p <= SINGLE_BATCH_PREFILL, f"prompt {p} walked off the single-batch prefill schedule"
                rows.append((p, d, t, cu))
                print(f"  p={p:4d} d={d:4d}  {t:8.3f}s  cu={cu}")

    # OLS on T = a + b·p + c·d, by the normal equations on a 3x3 system.
    n = len(rows)
    sp = sum(r[0] for r in rows)
    sd = sum(r[1] for r in rows)
    st = sum(r[2] for r in rows)
    spp = sum(r[0] * r[0] for r in rows)
    sdd = sum(r[1] * r[1] for r in rows)
    spd = sum(r[0] * r[1] for r in rows)
    spt = sum(r[0] * r[2] for r in rows)
    sdt = sum(r[1] * r[2] for r in rows)
    A = [[n, sp, sd, st], [sp, spp, spd, spt], [sd, spd, sdd, sdt]]
    # Gaussian elimination with partial pivoting.
    for col in range(3):
        piv = max(range(col, 3), key=lambda r: abs(A[r][col]))
        A[col], A[piv] = A[piv], A[col]
        if abs(A[col][col]) < 1e-12:
            raise RuntimeError("the grid is degenerate — prompt and decode did not vary independently")
        for r in range(3):
            if r == col:
                continue
            f = A[r][col] / A[col][col]
            for c in range(col, 4):
                A[r][c] -= f * A[col][c]
    a, b, c = (A[i][3] / A[i][i] for i in range(3))

    print()
    print(f"fit over {n} runs:  T = {a:.4f}s + {b * 1e3:.4f} ms/prompt-token + {c * 1e3:.4f} ms/decode-token")
    if b <= 0 or c <= 0:
        print("REFUSING to publish a ratio: a non-positive per-token cost means the grid was too noisy")
        sys.exit(1)
    ratio = c / b
    print(f"measured cost ratio decode:prefill = {ratio:.1f} : 1")
    print()
    print("verdict for consensus/core/src/palw_fp_devnet_v3.rs:")
    print(f"  CU_DECODE_WEIGHT / CU_PREFILL_WEIGHT must be >= {ratio:.1f}")
    print("  (equal = exact pricing; larger = prefill under-paid, which is the safe direction)")
finally:
    gateway.terminate()
    try:
        gateway.wait(timeout=10)
    except subprocess.TimeoutExpired:
        gateway.kill()
