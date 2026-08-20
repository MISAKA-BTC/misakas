#!/usr/bin/env python3
"""PALW free-prompt fleet drill (ADR-0044 FP-09).

One command that runs the whole free-prompt path that exists today, on the real model, and says
plainly which part of it is not yet reachable.

    misaka-palw-fp-fleet-drill.py <gateway> <rail> <worker> <gguf> [--skip-calibrate]

The steps, and what each one is actually evidence of:

  1. worker   — the executor is deterministic and fail-closed on every malformed job shape.
  2. gateway  — one real inference yields an answer AND its commitment inputs, from one run.
  3. rail     — that artifact becomes a signed subnetwork-0x4a transaction, refusing a foreign
                key, an edited outbox, a late retention deadline, and underfunding.
  4. SEAM     — the transaction the rail just built is read by the CONSENSUS extractor and
                accepted by the state machine, producing a free-prompt claim. This is the step
                nothing else in the tree performs: the sidecar tests end at "bytes written" and
                the consensus tests build their own fixtures, so without this the two halves
                agree only by construction.
  5. chain    — the consensus-side V2 wiring: state walk, admission, fork-choice authority, and
                the pruning-point carriage round trip.
  6. price    — the CU weight calibration, re-measured (skippable; it is the slow step).

# What this drill does NOT reach

The claim it produces is `Provisional`. Carrying it to `Final` needs the panel to certify it,
which needs the overlay rounds running on more than one node — and only a `Final` claim can be
spent by a receipt block. So the drill covers *commitment*, not *certification*, and the honest
summary says so rather than implying a full lap.
"""
import json
import os
import pathlib
import subprocess
import sys
import tempfile

if len(sys.argv) < 5:
    sys.exit(__doc__)
GATEWAY, RAIL, WORKER, GGUF = (os.path.abspath(a) for a in sys.argv[1:5])
SKIP_CALIBRATE = "--skip-calibrate" in sys.argv[5:]
HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent
CLASS_ID = "ba" * 64

results = []


def step(name, argv, cwd=None, env=None):
    print(f"\n=== {name} ===", flush=True)
    p = subprocess.run(argv, cwd=cwd, env=env, text=True)
    ok = p.returncode == 0
    results.append((name, ok))
    if not ok:
        print(f"--- {name} FAILED (exit {p.returncode})", flush=True)
    return ok


step("1. worker (real model, fail-closed shapes)",
     [sys.executable, str(HERE / "misaka-palw-fp-v3-worker-smoke.py"), WORKER, GGUF])
step("2. gateway (one inference -> answer + commitment)",
     [sys.executable, str(HERE / "misaka-palw-fp-gateway-smoke.py"), GATEWAY, WORKER, GGUF])
step("3. rail (artifact -> signed 0x4a transaction)",
     [sys.executable, str(HERE / "misaka-palw-fp-rail-smoke.py"), GATEWAY, RAIL, WORKER, GGUF])

# ---------------------------------------------------------------------------------------------
# 4. The seam. Build one real transaction here (so the drill owns it), then hand it to the
#    consensus reader. Everything before this point is the sidecar's own opinion of its output.
# ---------------------------------------------------------------------------------------------
print("\n=== 4. SEAM: the rail's own transaction, read by the consensus extractor ===", flush=True)
seam_ok = False
try:
    import http.client
    import time

    work = pathlib.Path(tempfile.mkdtemp(prefix="palw-fp-drill."))
    outbox = work / "outbox"
    identity = work / "identity.json"
    anchor = work / "anchor.json"
    seed = work / "bond.seed"
    seed.write_bytes(bytes([0x2A]) * 32)
    bond_pubkey = json.loads(subprocess.run(
        [RAIL, "--bond-key-seed", str(seed), "--print-bond-pubkey"], capture_output=True, check=True, text=True,
    ).stdout)["executor_pubkey"]
    identity.write_text(json.dumps({
        "network_domain": "4e" * 64,
        "class_id": CLASS_ID,
        "bond_txid": "b0" * 64,
        "bond_index": 0,
        "executor_pubkey": bond_pubkey,
        "operator_id": "e0" * 64,
    }))
    anchor.write_text(json.dumps({"anchor_block": "a0" * 64, "anchor_daa": 5000}))
    env = dict(os.environ)
    env["MISAKA_PALW_GGUF"] = GGUF
    PORT = 18862
    gw = subprocess.Popen(
        [GATEWAY, "--listen", f"127.0.0.1:{PORT}", "--worker", WORKER, "--outbox", str(outbox),
         "--identity", str(identity), "--anchor", str(anchor), "--quantum-cu", "1000"],
        env=env, stderr=subprocess.PIPE, text=True,
    )
    try:
        deadline = time.time() + 60
        while True:
            try:
                c = http.client.HTTPConnection("127.0.0.1", PORT, timeout=2)
                c.request("GET", "/health")
                json.loads(c.getresponse().read())
                break
            except (ConnectionRefusedError, OSError):
                if time.time() > deadline or gw.poll() is not None:
                    raise RuntimeError("the gateway did not come up")
                time.sleep(0.3)
        body = json.dumps({
            "model": "misaka-palw-fp-v3",
            "messages": [{"role": "user", "content": "Explain a Merkle tree in one sentence."}],
            "max_tokens": 64,
        }).encode()
        c = http.client.HTTPConnection("127.0.0.1", PORT, timeout=1800)
        c.request("POST", "/v1/chat/completions", body=body, headers={"Content-Type": "application/json"})
        payload = json.loads(c.getresponse().read())
        stem = str(pathlib.Path(payload["misaka"]["artifact"]).with_suffix(""))
        print(f"    inference: {payload['choices'][0]['message']['content'].strip()[:90]!r}  cu={payload['misaka']['cu']}")
    finally:
        gw.terminate()
        try:
            gw.wait(timeout=10)
        except subprocess.TimeoutExpired:
            gw.kill()

    built = json.loads(subprocess.run(
        [RAIL, "--artifact", stem, "--bond-key-seed", str(seed), "--funding-outpoint", "aa" * 64 + ":0",
         "--funding-amount", "100000000", "--class-id", CLASS_ID],
        capture_output=True, check=True, text=True,
    ).stdout)
    print(f"    transaction: {built['transaction_bytes']} bytes, cu={built['cu']} -> {built['quanta']} quanta / {built['pwu']} pwu")

    seam = json.loads(subprocess.run(
        [RAIL, "--verify-tx", built["tx_file"], "--class-id", CLASS_ID],
        capture_output=True, check=True, text=True,
    ).stdout)
    assert seam["schema"] == "misaka.palw.fp-rail-seam.v1"
    assert seam["fp_claim_id"] == built["fp_claim_id"], "the claim identity moved across the boundary"
    assert int(seam["quanta"]) == int(built["quanta"]) and int(seam["pwu"]) == int(built["pwu"]), \
        "the chain prices the job differently from the rail"
    assert int(seam["spent_quanta"]) == 0, "a fresh commitment has spent nothing"
    assert int(seam["immature_contribution"]) == 0, "a commitment must not pump live weight (invariant F-commitment)"
    print(f"    consensus: claim {seam['fp_claim_id'][:16]}… accepted — {seam['quanta']} quanta, {seam['pwu']} pwu, "
          f"immature contribution {seam['immature_contribution']}")
    seam_ok = True
except Exception as e:  # noqa: BLE001 — the drill reports, it does not interpret
    print(f"    SEAM FAILED: {e}")
results.append(("4. seam (rail tx -> consensus extractor -> state machine)", seam_ok))

step("5. chain (V2 wiring, walk, authority, pruning carriage)",
     ["cargo", "test", "-p", "kaspa-consensus", "--lib", "palw_state_walk_wiring", "--", "--nocapture", "-q"], cwd=REPO)
step("5b. chain (core: carriage gate, preset, devnet bundle)",
     ["cargo", "test", "-p", "kaspa-consensus-core", "--lib", "palw_fp_", "-q"], cwd=REPO)

if SKIP_CALIBRATE:
    print("\n=== 6. price (skipped) ===")
else:
    step("6. price (CU weight calibration, re-measured)",
         [sys.executable, str(HERE / "misaka-palw-fp-cu-calibrate.py"), GATEWAY, WORKER, GGUF, "2"])

print("\n" + "=" * 78)
for name, ok in results:
    print(f"  {'PASS' if ok else 'FAIL'}  {name}")
print("=" * 78)
print("NOT drilled: certification. The claim this produces is Provisional; carrying it to Final")
print("needs the panel's overlay rounds on more than one node, and only a Final claim can be")
print("spent by a receipt block. This drill covers commitment, not the full lap.")
if not all(ok for _, ok in results):
    sys.exit(1)
print("PALW fp fleet drill: ALL PASS")
