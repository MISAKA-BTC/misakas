#!/usr/bin/env python3
"""PALW free-prompt v3 worker smoke (ADR-0044 FP-06): one execution yields an ANSWER and a
commitment; the Text arm and the TokenIds (replay) arm reach byte-identical roots; stops are
canonical; every refusal is fail-closed (non-zero exit, empty stdout).

usage: misaka-palw-fp-v3-worker-smoke.py <palw-worker> <gguf>
"""
import hashlib
import json
import pathlib
import struct
import subprocess
import sys
import tempfile

WORKER = sys.argv[1]
GGUF = sys.argv[2]
ENV = {"MISAKA_PALW_GGUF": GGUF, "PATH": "/usr/bin:/bin"}

man = json.loads(subprocess.run([WORKER, "--mode", "v3-manifest"], capture_output=True, check=True).stdout)
H = lambda k: bytes.fromhex(man[k])
assert man["schema"] == "misaka.palw.fp-v3-manifest.v1"

TEMPLATE = "### System:\nYou are a concise assistant.\n\n### User:\nWhat is 2+2? Answer in one short sentence.\n\n### Assistant:\n"


def request(input_arm, decode_limit=24, privacy=1, tamper_manifest=False, max_ctx=4096):
    rmh = bytearray(H("runtime_manifest_hash"))
    if tamper_manifest:
        rmh[0] ^= 1
    out = b""
    out += struct.pack("<H", 3)                       # version
    out += b"\x4e" * 64                               # network_domain
    out += b"\xc1" * 64                               # class_id
    out += b"\xb0" * 64 + struct.pack("<I", 0)        # executor_bond outpoint (txid ‖ index)
    pk = b"\x07" * 32
    out += struct.pack("<I", len(pk)) + pk            # executor_pubkey
    out += b"\xe0" * 64                               # operator_id
    out += b"\xa0" * 64                               # anchor_block
    out += struct.pack("<Q", 5000)                    # anchor_daa
    out += b"\x11" * 32                               # job_nonce
    out += struct.pack("<I", decode_limit)
    out += struct.pack("<I", max_ctx)
    out += struct.pack("B", privacy)
    out += input_arm                                  # PalwFpWorkerInputV3
    out += H("model_profile_id")
    out += bytes(rmh)
    out += H("runtime_class_id")
    out += H("shape_profile_id")
    out += H("trace_scheme_id")
    return out


def text_arm(text: bytes) -> bytes:
    return struct.pack("B", 0) + struct.pack("<I", len(text)) + text


def ids_arm(ids) -> bytes:
    return struct.pack("B", 1) + struct.pack("<I", len(ids)) + b"".join(struct.pack("<I", t) for t in ids)


TRACE_OUT = tempfile.mkdtemp(prefix="palw-fp-trace.")


def run_job(payload, timeout=600):
    frame = struct.pack("<I", len(payload)) + payload
    return subprocess.run([WORKER, "--mode", "v3-job", "--trace-out", TRACE_OUT], input=frame, capture_output=True, env=ENV, timeout=timeout)


def parse_result(stdout, payload):
    (ln,) = struct.unpack_from("<I", stdout, 0)
    body = stdout[4 : 4 + ln]
    assert len(body) == ln and len(stdout) == 4 + ln, "response frame is exact"
    off = 0
    (ver,) = struct.unpack_from("<H", body, off); off += 2
    assert ver == 3
    req_hash = body[off : off + 64]; off += 64
    expect = hashlib.blake2b(
        struct.pack("<Q", len(payload)) + payload, digest_size=64, key=b"misaka-palw/fp-v3/worker-request/v1"
    ).digest()
    assert req_hash == expect, "request_hash echoes the exact request payload under the v3 domain"
    job = {}
    (job["version"],) = struct.unpack_from("<H", body, off); off += 2
    assert job["version"] == 3
    off += 64 * 2                                     # network_domain, class_id (echo-checked below via roots)
    off += 64 + 4                                     # bond outpoint
    (pklen,) = struct.unpack_from("<I", body, off); off += 4 + pklen
    off += 64 * 2                                     # operator, anchor_block
    off += 8 + 32                                     # anchor_daa, nonce
    off += 64                                         # tokenizer_id
    job["prompt_hash"] = body[off : off + 64]; off += 64
    job["prompt_tokens"], job["limit"], job["max_ctx"] = struct.unpack_from("<III", body, off); off += 12
    (job["privacy"],) = struct.unpack_from("B", body, off); off += 1
    (n_ids,) = struct.unpack_from("<I", body, off); off += 4
    job["prompt_ids"] = list(struct.unpack_from(f"<{n_ids}I", body, off)); off += 4 * n_ids
    r = {}
    for name in ("trace_root", "output_root", "schedule_root", "trace_manifest_root"):
        r[name] = body[off : off + 64]; off += 64
    (r["chunks"],) = struct.unpack_from("<I", body, off); off += 4
    r["events"], r["executed"] = struct.unpack_from("<II", body, off); off += 8
    (r["stop"],) = struct.unpack_from("B", body, off); off += 1
    (n_out,) = struct.unpack_from("<I", body, off); off += 4
    r["output_ids"] = list(struct.unpack_from(f"<{n_out}I", body, off)); off += 4 * n_out
    (n_rend,) = struct.unpack_from("<I", body, off); off += 4
    r["rendered"] = body[off : off + n_rend]; off += n_rend
    r["load_ms"], r["exec_ms"] = struct.unpack_from("<QQ", body, off); off += 16
    assert off == ln, f"consumed {off} of {ln}"
    return job, r


def expect_die(name, payload):
    p = run_job(payload)
    assert p.returncode != 0, f"{name}: must exit non-zero"
    assert p.stdout == b"", f"{name}: fail-closed means NOTHING on stdout"
    print(f"  fail-closed ok: {name}")


print("[1] text arm executes: answer + commitment from ONE inference")
payload = request(text_arm(TEMPLATE.encode()))
p = run_job(payload)
assert p.returncode == 0, p.stderr.decode()
job, r1 = parse_result(p.stdout, payload)
assert job["privacy"] == 1 and job["limit"] == 24
assert job["prompt_tokens"] == len(job["prompt_ids"]) > 0
assert r1["executed"] == r1["events"] == len(r1["output_ids"]) > 0
assert (r1["stop"] == 0) == (r1["executed"] == job["limit"]), "stop is canonical for the executed count"
answer = r1["rendered"].decode("utf-8", "replace")
print(f"  prompt_tokens={job['prompt_tokens']} executed={r1['executed']}/{job['limit']} stop={'budget' if r1['stop']==0 else 'eog'}")
print(f"  answer: {answer!r}")
assert len(answer.strip()) > 0, "the answer is not empty"

print("[1b] retained trace: chunks on disk, digests and manifest recompute exactly")
job_id_hex = None
for d in pathlib.Path(TRACE_OUT).iterdir():
    if (d / "manifest.json").is_file():
        job_id_hex = d.name
        trace_man = json.loads((d / "manifest.json").read_text())
        break
assert job_id_hex is not None, "the worker retained a trace directory"
assert trace_man["chunk_count"] == r1["chunks"] >= 1
events = b""
for k in range(trace_man["chunk_count"]):
    events += (pathlib.Path(TRACE_OUT) / job_id_hex / f"chunk-{k}.bin").read_bytes()
assert len(events) == r1["events"] * 64, "the retained bytes are the ordered event-hash list"
binding = bytes.fromhex(job_id_hex)
digests = []
for k in range(trace_man["chunk_count"]):
    chunk = events[k * 256 * 64 : (k + 1) * 256 * 64]
    h = hashlib.blake2b(digest_size=64, key=b"misaka-palw/fp-v3/trace-chunk/v1")
    h.update(binding + struct.pack("<I", k) + struct.pack("<I", len(chunk) // 64) + chunk)
    digests.append(h.digest())
    assert digests[k].hex() == trace_man["chunk_digests"][k], f"chunk {k} digest recomputes"
h = hashlib.blake2b(digest_size=64, key=b"misaka-palw/fp-v3/trace-manifest/v1")
h.update(binding + struct.pack("<I", 256) + struct.pack("<I", len(digests)) + b"".join(digests))
assert h.digest() == r1["trace_manifest_root"], "the manifest root in the result is the retained material's"
print(f"  ok: {trace_man['chunk_count']} chunk(s), {r1['events']} events, manifest root recomputed")

print("[2] determinism: the same text twice — every consensus-visible byte identical")
p2 = run_job(payload)
assert p2.returncode == 0, p2.stderr.decode()
job2, r2 = parse_result(p2.stdout, payload)
assert job2 == job, "the bound job is identical"
for k in ("trace_root", "output_root", "schedule_root", "trace_manifest_root", "chunks", "events", "executed", "stop", "output_ids", "rendered"):
    assert r2[k] == r1[k], f"{k} must not move between runs (telemetry may; commitments may not)"
print("  ok")

print("[3] the TokenIds (replay) arm reaches byte-identical roots from chain-carriable data")
replay_payload = request(ids_arm(job["prompt_ids"]))
p3 = run_job(replay_payload)
assert p3.returncode == 0, p3.stderr.decode()
job3, r3 = parse_result(p3.stdout, replay_payload)
assert job3["prompt_ids"] == job["prompt_ids"], "the ids arm echoes its input"
assert job3["prompt_hash"] == job["prompt_hash"]
for k in ("trace_root", "output_root", "schedule_root", "trace_manifest_root", "chunks"):
    assert r3[k] == r1[k], f"{k} must be identical across the two arms"
assert r3["output_ids"] == r1["output_ids"] and r3["rendered"] == r1["rendered"]
print("  ok: text-in and ids-in converge on one execution")

print("[4] a generous ceiling lets EOG stop the run — and the encoding stays canonical")
eog_payload = request(text_arm(TEMPLATE.encode()), decode_limit=192)
p4 = run_job(eog_payload)
assert p4.returncode == 0, p4.stderr.decode()
job4, r4 = parse_result(p4.stdout, eog_payload)
assert (r4["stop"] == 0) == (r4["executed"] == 192), "canonical stop encoding"
print(f"  executed={r4['executed']}/192 stop={'budget' if r4['stop']==0 else 'eog'}")
if r4["stop"] == 1:
    assert r4["executed"] < 192
    print(f"  eog answer: {r4['rendered'].decode('utf-8', 'replace')!r}")

print("[5] fail-closed refusals")
expect_die("tampered runtime identity", request(text_arm(TEMPLATE.encode()), tamper_manifest=True))
expect_die("non-PublicDa privacy mode", request(text_arm(TEMPLATE.encode()), privacy=2))
expect_die("empty text", request(text_arm(b"")))
expect_die("zero decode ceiling", request(text_arm(TEMPLATE.encode()), decode_limit=0))
expect_die("token id outside the vocab", request(ids_arm([2**31 - 1])))
expect_die("prompt over the single-batch prefill cap", request(text_arm(("word " * 900).encode())))
expect_die("budget over max_context", request(text_arm(TEMPLATE.encode()), decode_limit=4096, max_ctx=128))

print("PALW fp-v3 worker smoke: ALL PASS")
