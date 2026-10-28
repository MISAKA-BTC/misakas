#!/usr/bin/env python3
"""FP executor-rail smoke (ADR-0044 FP-08/09): a real chat, answered by the real model, becomes a
SIGNED free-prompt commitment transaction — the whole path a user's inference travels before a
network exists to submit it to.

usage: misaka-palw-fp-rail-smoke.py <gateway> <rail> <palw-worker> <gguf>
"""
import http.client
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time

GATEWAY, RAIL, WORKER, GGUF = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
PORT = 18791

work = pathlib.Path(tempfile.mkdtemp(prefix="palw-fp-rail-smoke."))
outbox = work / "outbox"
identity = work / "identity.json"
anchor = work / "anchor.json"
seed_file = work / "bond.seed"
other_seed = work / "other.seed"
seed_file.write_bytes(bytes(range(32)))
other_seed.write_bytes(bytes(range(32, 64)))

# The gateway must declare the SAME executor key the rail signs with — ask the rail for it, which
# is why --print-bond-pubkey exists (matching the halves by a failed signing attempt is not a
# workflow).
bond = json.loads(subprocess.run(
    [RAIL, "--bond-key-seed", str(seed_file), "--print-bond-pubkey"], capture_output=True, check=True, text=True
).stdout)
assert bond["schema"] == "misaka.palw.fp-rail-bond-key.v1"
CLASS_ID = "ba" * 64

identity.write_text(json.dumps({
    "network_domain": "4e" * 64,
    "class_id": CLASS_ID,
    "bond_txid": "b0" * 64,
    "bond_index": 0,
    "executor_pubkey": bond["executor_pubkey"],
    "operator_id": "e0" * 64,
}))
anchor.write_text(json.dumps({"anchor_block": "a0" * 64, "anchor_daa": 5000}))

env = dict(os.environ)
env["MISAKA_PALW_GGUF"] = GGUF
gateway = subprocess.Popen(
    [GATEWAY, "--listen", f"127.0.0.1:{PORT}", "--worker", WORKER, "--outbox", str(outbox),
     "--identity", str(identity), "--anchor", str(anchor), "--class-leaves", "7708"],
    env=env, stderr=subprocess.PIPE, text=True,
)
try:
    deadline = time.time() + 30
    while True:
        try:
            conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=2)
            conn.request("GET", "/health")
            assert json.loads(conn.getresponse().read())["status"] == "ok"
            break
        except (ConnectionRefusedError, OSError):
            if gateway.poll() is not None:
                raise RuntimeError(f"gateway died: {gateway.stderr.read()}")
            if time.time() > deadline:
                raise RuntimeError("gateway did not come up")
            time.sleep(0.3)

    body = json.dumps({
        "messages": [{"role": "user", "content": "Name one property of a hash function, in one sentence."}],
        "max_tokens": 48,
    })
    conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=900)
    conn.request("POST", "/v1/chat/completions", body, {"Content-Type": "application/json"})
    payload = json.loads(conn.getresponse().read())
    answer = payload["choices"][0]["message"]["content"]
    misaka = payload["misaka"]
    print(f"[1] the user's inference: {answer!r}")
    print(f"    cu={misaka['cu']} trace_root={misaka['trace_root'][:16]}…")
    stem = str(pathlib.Path(misaka["artifact"]).with_suffix(""))

    # --print-claim needs no key: it emits exactly the digest a signer sidecar signs.
    claim = json.loads(subprocess.run([RAIL, "--artifact", stem, "--print-claim"], capture_output=True, check=True, text=True).stdout)
    assert claim["schema"] == "misaka.palw.fp-rail-claim.v1" and claim["signing_purpose"] == "PalwFpCommitmentV3"
    assert claim["cu"] == misaka["cu"], "the rail prices the artifact exactly as the gateway did"
    print(f"[2] claim id for the signer: {claim['fp_claim_id'][:16]}… (purpose {claim['signing_purpose']})")

    # The real thing: sign and build the funded overlay transaction.
    built = json.loads(subprocess.run(
        [RAIL, "--artifact", stem, "--bond-key-seed", str(seed_file),
         "--funding-outpoint", "aa" * 64 + ":0", "--funding-amount", "100000000", "--class-id", CLASS_ID],
        capture_output=True, check=True, text=True,
    ).stdout)
    assert built["schema"] == "misaka.palw.fp-rail-tx.v1"
    assert built["fp_claim_id"] == claim["fp_claim_id"], "signing did not move the claim identity"
    assert built["subnetwork"].startswith("0x4a")
    assert int(built["quanta"]) >= 1 and int(built["pwu"]) % int(built["quanta"]) == 0, "uniform quanta, as the state machine demands"
    tx_file = pathlib.Path(built["tx_file"])
    assert tx_file.is_file() and tx_file.stat().st_size > int(built["payload_bytes"]), "the transaction rides on disk"
    assert built["not_done_here"], "the rail says what it did not do"
    print(f"[3] signed commitment tx: {built['transaction_bytes']} bytes "
          f"(payload {built['payload_bytes']}), cu={built['cu']} → {built['quanta']} quanta / {built['pwu']} pwu")

    # Refusal 1: a key that is not the commitment's executor key. Holding A valid key is not
    # holding THIS job's key.
    refused = subprocess.run(
        [RAIL, "--artifact", stem, "--bond-key-seed", str(other_seed),
         "--funding-outpoint", "aa" * 64 + ":0", "--funding-amount", "100000000"],
        capture_output=True, text=True,
    )
    assert refused.returncode != 0 and "does not match the commitment" in refused.stderr, refused.stderr
    print("[4] a foreign bond key is refused — the rail cannot sign another operator's job")

    # Refusal 2: an edited outbox. Flip a byte INSIDE the retained-trace manifest root — a field
    # the worker result also carries, so the pair cross-check catches it.
    #
    # (The first draft flipped the last 8 bytes and expected the same refusal. Those bytes are
    # `trace_retention_daa`, the ONE commitment field with no counterpart in the result: it is a
    # chain-time promise the caller makes, so a different value is a different, still-honest
    # promise under a different claim id. The rail checks what it can there — that the deadline
    # is not already past the job's own anchor — and signs what it is told otherwise.)
    unsigned = pathlib.Path(f"{stem}.commitment-unsigned.borsh")
    original = unsigned.read_bytes()
    tampered = bytearray(original)
    tampered[-20] ^= 0x01  # inside trace_manifest_root
    unsigned.write_bytes(bytes(tampered))
    edited = subprocess.run([RAIL, "--artifact", stem, "--print-claim"], capture_output=True, text=True)
    assert edited.returncode != 0 and "does not match the execution" in edited.stderr, edited.stderr
    print("[5] an edited outbox is refused — the artifact pair is cross-checked before signing")

    # …and the one field the pair cannot cross-check is still bounded: a retention deadline at or
    # before the job's own anchor promises to serve nothing.
    broken_promise = bytearray(original)
    broken_promise[-8:] = (0).to_bytes(8, "little")
    unsigned.write_bytes(bytes(broken_promise))
    hollow = subprocess.run([RAIL, "--artifact", stem, "--print-claim"], capture_output=True, text=True)
    assert hollow.returncode != 0 and "promise to serve nothing" in hollow.stderr, hollow.stderr
    print("[5b] a retention deadline at or before the anchor is refused")
    unsigned.write_bytes(original)

    # Refusal 3: funding that does not cover the fee.
    broke = subprocess.run(
        [RAIL, "--artifact", stem, "--bond-key-seed", str(seed_file),
         "--funding-outpoint", "aa" * 64 + ":0", "--funding-amount", "1000", "--class-id", CLASS_ID],
        capture_output=True, text=True,
    )
    assert broke.returncode != 0 and "does not cover the fee" in broke.stderr, broke.stderr
    print("[6] underfunded builds are refused before they become a transaction")

    print("PALW fp rail smoke: ALL PASS")
finally:
    gateway.terminate()
    try:
        gateway.wait(timeout=5)
    except subprocess.TimeoutExpired:
        gateway.kill()
