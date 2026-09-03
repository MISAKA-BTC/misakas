#!/usr/bin/env python3
"""FP gateway smoke (ADR-0044 FP-07, ADR-0077 Decisions 1-3): an OpenAI-style chat request
answered by the real model, with the commitment inputs in the response and the artifact in the
outbox — one inference, one resident worker, and the SSE form of the same answer.

Runs the OFFLINE form (`--anchor`): the four chain facts read `unknown` and `/health` says so by
name, and the CHAIN raises no objection to committing — a source that cannot submit cannot read an
unknown as a yes. What decides whether an UNSIGNED commitment is written is then ADR-0077 SA-1's
exposure rule alone (`PublicJobBudget::may_commit`): with no `--rpc` to read the numbers from, the
operator must DECLARE the bond's exposure room and one claim's exposure, so this smoke declares
them (`--bond-exposure-room-sompi` / `--claim-exposure-sompi`) and step [3] asserts the writer
ran. Without the two flags the gateway answers and withholds the commitment by design
(`not_committed_because` names the reason) — which is what this script used to claim could not
happen, until the first run that reached step [3] (2026-09-03). The live form is `--rpc
<host:port>`; the devnet drill exercises that.

The identity is REAL where the worker checks it: `palw-a16-fp-worker` pins the request's
`class_id` to the class its artifact derives (`fp_worker.rs`, "the request declares a runtime this
worker is not"), so `class_id` here is taken from `palw-class ledger` for `MISAKA_PALW_MODEL_ID`
(default `Qwen/Qwen2.5-1.5B/graph-v5@512`) — or from `MISAKA_PALW_CLASS_ID` — never a synthetic
`c1c1…`, which the worker refuses.

usage: misaka-palw-fp-gateway-smoke.py <misaka-palw-gateway> <family-fp-worker> <artifact.palwart>
env:   MISAKA_PALW_TOKENIZER (tokenizer.json for the artifact — REQUIRED)
       MISAKA_PALW_NETWORK_ID (the worker's network — REQUIRED, e.g. testnet-11)
       MISAKA_PALW_MODEL_ID (the catalog row; default Qwen/Qwen2.5-1.5B/graph-v5@512)
       MISAKA_PALW_CLASS_ID (skip the ledger lookup and use this 128-hex class id)
       PALW_CLASS_BIN (the palw-class binary; default: beside the gateway binary)
"""
import http.client
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import time

if len(sys.argv) != 4:
    sys.exit(__doc__)
GATEWAY, WORKER, ARTIFACT = sys.argv[1], sys.argv[2], sys.argv[3]
PORT = 18790


def required_env(name):
    value = os.environ.get(name)
    if not value:
        sys.exit(f"{name} is not set — this smoke runs the real worker, which needs it (see the usage in this file)")
    return value


TOKENIZER = required_env("MISAKA_PALW_TOKENIZER")
NETWORK_ID = required_env("MISAKA_PALW_NETWORK_ID")
MODEL_ID = os.environ.get("MISAKA_PALW_MODEL_ID", "Qwen/Qwen2.5-1.5B/graph-v5@512")
for label, path in (("artifact", ARTIFACT), ("tokenizer", TOKENIZER)):
    if not pathlib.Path(path).is_file():
        sys.exit(f"the {label} {path!r} is not a file — relative paths resolve against the CALLER's cwd, use absolute ones")


def class_id_for(model_id):
    """The class id this build derives for `model_id`, from `palw-class ledger` — the same id the
    worker's manifest carries, so the request's identity and the runtime agree by construction."""
    explicit = os.environ.get("MISAKA_PALW_CLASS_ID")
    if explicit:
        assert len(explicit) == 128, "MISAKA_PALW_CLASS_ID must be the 128-hex class id"
        return explicit
    palw_class = os.environ.get("PALW_CLASS_BIN") or str(pathlib.Path(GATEWAY).resolve().parent / "palw-class")
    if not pathlib.Path(palw_class).is_file():
        sys.exit(f"{palw_class} is not a file — build it (cargo build --release -p misaka-palw-sdk) or set MISAKA_PALW_CLASS_ID")
    ledger = subprocess.run([palw_class, "ledger", "--network", NETWORK_ID], capture_output=True, text=True, check=True).stdout
    found = False
    for line in ledger.splitlines():
        words = line.split()
        if found and len(words) >= 3 and words[0] == "class" and words[1] == "id":
            return words[2]
        found = found or (len(words) >= 1 and words[0] == model_id)
    sys.exit(f"palw-class ledger names no class id for {model_id!r} on {NETWORK_ID}")


CLASS_ID = class_id_for(MODEL_ID)

work = pathlib.Path(tempfile.mkdtemp(prefix="palw-fp-gateway-smoke."))
outbox = work / "outbox"
identity = work / "identity.json"
anchor = work / "anchor.json"
identity.write_text(json.dumps({
    "network_domain": "4e" * 64,
    "class_id": CLASS_ID,
    "bond_txid": "b0" * 64,
    "bond_index": 0,
    "executor_pubkey": "07" * 32,
    "operator_id": "e0" * 64,
}))
anchor.write_text(json.dumps({"anchor_block": "a0" * 64, "anchor_daa": 5000}))

env = dict(os.environ)
env["MISAKA_PALW_ARTIFACT"] = ARTIFACT
env["MISAKA_PALW_TOKENIZER"] = TOKENIZER
env["MISAKA_PALW_NETWORK_ID"] = NETWORK_ID
gateway = subprocess.Popen(
    [GATEWAY, "--listen", f"127.0.0.1:{PORT}", "--worker", WORKER, "--outbox", str(outbox),
     "--identity", str(identity), "--anchor", str(anchor), "--class-leaves", "7708",
     # SA-1: declared, because offline there is no chain to read them from — without these two
     # the gateway answers and writes NO commitment, by design (see the header).
     "--bond-exposure-room-sompi", "1000000", "--claim-exposure-sompi", "50000"],
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
    # ADR-0077 Decision 3: all four names are present in every answer, including the unknown one.
    for name in ("registered", "fp_certified", "bond_active", "exposure_room"):
        assert name in health["chain"], f"/health must name {name} (ADR-0077 Decision 3)"
        assert health["chain"][name] == "unknown", f"the offline form must say unknown, never a yes: {name}"
    assert health["can_submit"] is False, "a gateway with no --rpc cannot submit, and says so"
    print(f"[1] health ok — manifest {health['runtime_manifest_hash'][:16]}…, template {health['template_id']}, "
          f"n_ctx {health['n_ctx']}, chain {health['chain']['source']}")

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
    print(f"    fp_job_id={misaka['fp_job_id'][:16]}… trace_root={misaka['trace_root'][:16]}… "
          f"work_leaves={misaka['work_leaves']} stream_checked={misaka['answer_stream_checked']}")
    assert len(answer.strip()) > 0
    assert usage["completion_tokens"] > 0 and usage["prompt_tokens"] > 0
    assert len(misaka["trace_root"]) == 128 and len(misaka["output_root"]) == 128

    artifact = pathlib.Path(misaka["artifact"])
    assert artifact.is_file(), "the artifact JSON exists"
    summary = json.loads(artifact.read_text())
    # The commitment writer ran because the exposure was DECLARED; a refusal names its reason.
    assert summary["committed"] is True, f"the gateway withheld the commitment: {summary.get('not_committed_because')}"
    assert summary["not_committed_because"] is None
    assert summary["schema"] == "misaka.palw.fp-v3-gateway-artifact.v1"
    assert summary["trace_root"] == misaka["trace_root"]
    assert summary["work_leaves"] == misaka["work_leaves"]
    assert int(summary["quanta_at_configured_quantum"]) >= 1, "the job earned at least one draw at the class's quantum"
    # ADR-0077 SA-3: the prompt side of F1 is checked on every job that produced a commitment.
    assert summary["prompt_ids_checked"] is True, "the committed prompt ids were checked against the plan (SA-3)"
    borsh_artifact = artifact.with_suffix("").with_suffix(".result.borsh")
    assert borsh_artifact.is_file() and borsh_artifact.stat().st_size > 0, "the framed result rides beside the summary"
    commitment_artifact = artifact.with_suffix("").with_suffix(".commitment-unsigned.borsh")
    assert commitment_artifact.is_file() and commitment_artifact.stat().st_size > 0, "the unsigned commitment rides too"
    assert summary["trace_manifest_root"] and int(summary["trace_chunk_count"]) >= 1
    trace_dir = pathlib.Path(summary["trace_dir"])
    assert (trace_dir / "manifest.json").is_file() and (trace_dir / "chunk-0.bin").is_file(), "the retained trace is where the summary says"
    assert summary["pending_for_chain_submission"] and all("trace" not in item for item in summary["pending_for_chain_submission"]),         "retention is no longer pending; the signer and the rail are"
    print(f"[3] artifact ok: {artifact.name} + unsigned commitment (exposure declared, SA-1) + retained trace ({summary['trace_chunk_count']} chunk)")

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

    # ADR-0077 Decision 2: the answer STREAMS as SSE and the commitment does not. W5 is asserted
    # by the gateway itself — a stream whose rendering is not the committed one writes no
    # commitment — and what this script checks is that the two surfaces agree about the answer.
    conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=600)
    conn.request("POST", "/v1/chat/completions", json.dumps(json.loads(body) | {"stream": True}),
                 {"Content-Type": "application/json"})
    sse = conn.getresponse()
    assert sse.status == 200, "stream: true is served, not refused"
    assert sse.getheader("content-type") == "text/event-stream"
    streamed, tail = "", None
    for raw in sse.read().decode().split("\n\n"):
        raw = raw.strip()
        if not raw.startswith("data: "):
            continue
        payload_text = raw[len("data: "):]
        if payload_text == "[DONE]":
            break
        event = json.loads(payload_text)
        if "misaka" in event:
            tail = event
            continue
        streamed += event["choices"][0]["delta"].get("content", "")
    assert tail is not None, "the terminal event carries the misaka object — an SSE client is told whether it made a claim"
    assert streamed.strip() == answer.strip(), f"the streamed answer is the buffered one: {streamed!r} vs {answer!r}"
    assert tail["misaka"]["answer_stream_checked"] is True, "W5 was actually exercised on the streamed run"
    print(f"[5] SSE ok — {len(streamed)} bytes streamed, same answer, W5 checked")

    # Refusals: a body with no user message.
    conn = http.client.HTTPConnection("127.0.0.1", PORT, timeout=60)
    conn.request("POST", "/v1/chat/completions", json.dumps({"messages": [{"role": "system", "content": "x"}]}),
                 {"Content-Type": "application/json"})
    assert conn.getresponse().status == 400, "no user message is refused"
    print("[6] refusals ok (userless chat)")

    print("PALW fp gateway smoke: ALL PASS")
finally:
    gateway.terminate()
    try:
        gateway.wait(timeout=5)
    except subprocess.TimeoutExpired:
        gateway.kill()
