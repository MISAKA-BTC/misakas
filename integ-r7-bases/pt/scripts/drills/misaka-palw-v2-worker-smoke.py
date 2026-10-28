#!/usr/bin/env python3
"""PALW v2 worker smoke test: framed Borsh IPC, determinism, fail-closed probes."""
import hashlib
import json
import struct
import subprocess
import sys

WORKER = sys.argv[1]
GGUF = sys.argv[2]
ENV = {"MISAKA_PALW_GGUF": GGUF, "PATH": "/usr/bin:/bin"}

# --- 1. identity from v2-manifest (no model load) ---
man = json.loads(subprocess.run([WORKER, "--mode", "v2-manifest"], capture_output=True, check=True).stdout)
H = lambda k: bytes.fromhex(man[k])
assert man["fp_environment_canonical"] is True

def envelope(mode=0, decode=16, max_ctx=4096, tamper_manifest=False, ids=None):
    ids = ids if ids is not None else [1000, 42, 7, 31337, 9999, 5, 88, 12345, 3, 777, 2024, 66]
    rmh = bytearray(H("runtime_manifest_hash_v2"))
    if tamper_manifest:
        rmh[0] ^= 1
    out = b""
    out += struct.pack("<H", 2)                      # version
    nid = b"misaka-devnet"
    out += struct.pack("<I", len(nid)) + nid          # network_id: Vec<u8>
    out += b"\x11" * 64                               # job_id
    out += b"\x22" * 64                               # job_nullifier
    out += struct.pack("B", mode)                     # mode enum
    out += H("model_profile_id")
    out += bytes(rmh)                                 # runtime_manifest_hash
    out += H("runtime_class_id")
    out += H("shape_profile_id_v2")
    out += H("trace_scheme_id_v2")
    out += H("cu_ruleset_id_v2")
    out += b"\xab" * 32                               # execution_seed
    out += struct.pack("<I", len(ids)) + b"".join(struct.pack("<I", t) for t in ids)
    out += struct.pack("<I", decode)                  # exact_decode_tokens
    out += struct.pack("<I", max_ctx)                 # max_context_tokens
    out += b"\x99" * 64                               # assignment_id
    out += struct.pack("<Q", 7)                       # assignment_epoch
    out += struct.pack("<Q", 0)                       # deadline_unix_ms (harness: none)
    return out

def run_job(payload, timeout=300):
    frame = struct.pack("<I", len(payload)) + payload
    p = subprocess.run([WORKER, "--mode", "v2-job"], input=frame, capture_output=True, env=ENV, timeout=timeout)
    return p

def parse_result(stdout, payload):
    (ln,) = struct.unpack_from("<I", stdout, 0)
    body = stdout[4 : 4 + ln]
    assert len(body) == ln and len(stdout) == 4 + ln, "response frame is exact"
    off = 0
    (ver,) = struct.unpack_from("<H", body, off); off += 2
    assert ver == 2
    req_hash = body[off : off + 64]; off += 64
    expect = hashlib.blake2b(payload, digest_size=64, key=b"misaka-palw/job-request/v2").digest()
    assert req_hash == expect, "request_hash echoes the exact request payload"
    job_id = body[off : off + 64]; off += 64
    assert job_id == b"\x11" * 64
    proj = {}
    for name in ("job_context_hash", "root", "output_commitment", "schedule"):
        proj[name] = body[off : off + 64]; off += 64
    (proj["cu"],) = struct.unpack_from("<16s", body, off); off += 16
    proj["prefill"], proj["decode"], proj["events"] = struct.unpack_from("<III", body, off); off += 12
    (proj["stop"],) = struct.unpack_from("B", body, off); off += 1
    tele = {}
    tele["load_ms"], tele["exec_ms"] = struct.unpack_from("<QQ", body, off); off += 16
    (tag,) = struct.unpack_from("B", body, off); off += 1
    tele["eog"] = None
    if tag == 1:
        (tele["eog"],) = struct.unpack_from("<I", body, off); off += 4
    assert off == ln, f"consumed {off} of {ln}"
    return proj, tele

payload = envelope()

# --- 2. determinism: Execute twice, Replay once — projections byte-identical ---
runs = []
for i, mode in enumerate((0, 0, 1)):
    p = run_job(envelope(mode=mode))
    assert p.returncode == 0, f"run {i} failed: {p.stderr.decode()[-800:]}"
    proj, tele = parse_result(p.stdout, envelope(mode=mode))
    runs.append((proj, tele))
    print(f"run{i} mode={mode}: root={proj['root'][:8].hex()}… prefill={proj['prefill']} decode={proj['decode']} "
          f"events={proj['events']} cu={int.from_bytes(proj['cu'], 'little')} stop={proj['stop']} "
          f"load={tele['load_ms']}ms exec={tele['exec_ms']}ms eog@{tele['eog']}")
assert runs[0][0] == runs[1][0] == runs[2][0], "projections must be byte-identical across reruns and modes"
assert runs[0][0]["prefill"] == 12 and runs[0][0]["decode"] == 16 and runs[0][0]["events"] == 16
assert int.from_bytes(runs[0][0]["cu"], "little") == 12 + 8 * 16

# --- 3. input sensitivity: a different seed must move the root (context-bound) even with the
# same token ids, and different token ids must move it too ---
alt = bytearray(envelope());
# flip one bit of execution_seed: locate it = after 2+4+13+64+64+1+6*64 = 532 offset
seed_off = 2 + 4 + 13 + 64 + 64 + 1 + 6 * 64
assert alt[seed_off] == 0xAB
alt[seed_off] ^= 1
p = run_job(bytes(alt))
assert p.returncode == 0, p.stderr.decode()[-500:]
proj_alt, _ = parse_result(p.stdout, bytes(alt))
assert proj_alt["root"] != runs[0][0]["root"], "a different execution_seed must change the trace root"
assert proj_alt["job_context_hash"] != runs[0][0]["job_context_hash"]

p = run_job(envelope(ids=[15, 2, 900, 4444, 21, 6]))
assert p.returncode == 0
proj_ids, _ = parse_result(p.stdout, envelope(ids=[15, 2, 900, 4444, 21, 6]))
assert proj_ids["root"] != runs[0][0]["root"], "different prompt token ids must change the trace root"

# --- 4. fail-closed probes: no stdout, nonzero exit ---
probes = {
    "tampered runtime_manifest_hash": envelope(tamper_manifest=True),
    "zero decode budget": envelope(decode=0),
    "context profile mismatch": envelope(max_ctx=2048),
    "budget overflow": envelope(decode=4096),
    "unknown mode": envelope(mode=9),
    "empty prompt": envelope(ids=[]),
}
for name, bad in probes.items():
    p = run_job(bad, timeout=60)
    assert p.returncode != 0, f"{name}: must be rejected"
    assert p.stdout == b"", f"{name}: nothing may be written to stdout on failure"
    print(f"probe ok: {name} -> exit={p.returncode}, stdout empty")

# out-of-vocab token (n_vocab is model-dependent; 2**31-1 is certainly out)
bad = envelope(ids=[1, 2, 2**31 - 1])
p = run_job(bad)
assert p.returncode != 0 and p.stdout == b"", "out-of-vocab token must be rejected with no output"
print("probe ok: out-of-vocab token")

# truncated frame
p = subprocess.run([WORKER, "--mode", "v2-job"], input=struct.pack("<I", 100) + b"xx", capture_output=True, env=ENV, timeout=60)
assert p.returncode != 0 and p.stdout == b""
print("probe ok: truncated frame")

# trailing bytes after the frame
frame = struct.pack("<I", len(payload)) + payload + b"\x00"
p = subprocess.run([WORKER, "--mode", "v2-job"], input=frame, capture_output=True, env=ENV, timeout=60)
assert p.returncode != 0 and p.stdout == b""
print("probe ok: trailing bytes after frame")

# --n-predict is not a v2 interface
p = subprocess.run([WORKER, "--mode", "v2-job", "--n-predict", "128"], input=struct.pack("<I", len(payload)) + payload,
                   capture_output=True, env=ENV, timeout=60)
assert p.returncode != 0 and p.stdout == b""
print("probe ok: --n-predict rejected on v2-job")

print("\nALL SMOKE TESTS PASSED")
print("root:", runs[0][0]["root"].hex())
