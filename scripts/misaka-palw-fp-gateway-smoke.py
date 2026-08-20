#!/usr/bin/env python3
"""FP gateway smoke (ADR-0044 FP-07): an OpenAI-style chat request answered by the real model,
with the commitment inputs in the response and the artifact in the outbox — one inference.

usage: misaka-palw-fp-gateway-smoke.py <misaka-palw-gateway> <palw-worker> <gguf>
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
PORT = 18790

work = pathlib.Path(tempfile.mkdtemp(prefix="palw-fp-gateway-smoke."))
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
try:
    # Wait for /health.
    deadline = time.time() + 30
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
    print(f"[1] health ok — manifest {health['runtime_manifest_hash'][:16]}…, template {health['template_id']}")

    body = json.dumps({
        "model": "misaka-palw-fp-v3",
        "messages": [
            {"role": "system", "content": "You are a concise assistant."},
            {"role": "user", "content": "What is 2+2? Answer in one short sentence."},
        ],
        "max_tokens": 24,
    })
    conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=600)
    conn.request("POST", "/v1/chat/completions", body, {"Content-Type": "application/json"})
    response = conn.getresponse()
    payload = json.loads(response.read())
    assert response.status == 200, payload
    answer = payload["choices"][0]["message"]["content"]
    usage = payload["usage"]
    misaka = payload["misaka"]
    print(f"[2] chat answered: {answer!r}")
    print(f"    usage: {usage}")
    print(f"    fp_job_id={misaka['fp_job_id'][:16]}… trace_root={misaka['trace_root'][:16]}… cu={misaka['cu']}")
    assert len(answer.strip()) > 0
    assert usage["completion_tokens"] > 0 and usage["prompt_tokens"] > 0
    assert len(misaka["trace_root"]) == 128 and len(misaka["output_root"]) == 128

    artifact = pathlib.Path(misaka["artifact"])
    assert artifact.is_file(), "the artifact JSON exists"
    summary = json.loads(artifact.read_text())
    assert summary["schema"] == "misaka.palw.fp-v3-gateway-artifact.v1"
    assert summary["trace_root"] == misaka["trace_root"]
    assert summary["cu"] == misaka["cu"]
    assert int(summary["quanta_at_configured_quantum"]) >= 1, "the job earned at least one draw at quantum 1000"
    borsh_artifact = artifact.with_suffix("").with_suffix(".result.borsh")
    assert borsh_artifact.is_file() and borsh_artifact.stat().st_size > 0, "the framed result rides beside the summary"
    commitment_artifact = artifact.with_suffix("").with_suffix(".commitment-unsigned.borsh")
    assert commitment_artifact.is_file() and commitment_artifact.stat().st_size > 0, "the unsigned commitment rides too"
    assert summary["trace_manifest_root"] and int(summary["trace_chunk_count"]) >= 1
    trace_dir = pathlib.Path(summary["trace_dir"])
    assert (trace_dir / "manifest.json").is_file() and (trace_dir / "chunk-0.bin").is_file(), "the retained trace is where the summary says"
    assert summary["pending_for_chain_submission"] and all("trace" not in item for item in summary["pending_for_chain_submission"]),         "retention is no longer pending; the signer and the rail are"
    print(f"[3] artifact ok: {artifact.name} + unsigned commitment + retained trace ({summary['trace_chunk_count']} chunk)")

    # Same conversation again. Two properties, both load-bearing and easy to conflate:
    # * F1: the ANSWER is identical — the fresh nonce never touches the model's input.
    # * Anti-replay: the trace root DIFFERS — every event binds the job id (nonce included), so
    #   one job's trace can never be replayed as another's. Asserting root equality here was
    #   this script's first draft, and it was wrong about the design on purpose of the design.
    conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=600)
    conn.request("POST", "/v1/chat/completions", body, {"Content-Type": "application/json"})
    second = json.loads(conn.getresponse().read())
    assert second["choices"][0]["message"]["content"] == answer, "same conversation, same ANSWER — F1 at the gateway boundary"
    assert second["usage"] == usage
    assert second["misaka"]["fp_job_id"] != misaka["fp_job_id"], "a fresh nonce is a fresh job identity"
    assert second["misaka"]["trace_root"] != misaka["trace_root"], "the trace binds the job id — anti-replay, by design"
    print("[4] re-ask: same answer (F1), different job id and trace root (anti-replay binding)")

    # Refusals: streaming, and a body with no user message.
    conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=60)
    conn.request("POST", "/v1/chat/completions", json.dumps({"messages": [{"role": "user", "content": "x"}], "stream": True}),
                 {"Content-Type": "application/json"})
    assert conn.getresponse().status == 400, "a stream request is refused, not silently downgraded"
    conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=60)
    conn.request("POST", "/v1/chat/completions", json.dumps({"messages": [{"role": "system", "content": "x"}]}),
                 {"Content-Type": "application/json"})
    assert conn.getresponse().status == 400, "no user message is refused"
    print("[5] refusals ok (stream, userless chat)")

    print("PALW fp gateway smoke: ALL PASS")
finally:
    gateway.terminate()
    try:
        gateway.wait(timeout=5)
    except subprocess.TimeoutExpired:
        gateway.kill()
