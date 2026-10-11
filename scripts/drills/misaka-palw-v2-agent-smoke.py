#!/usr/bin/env python3
"""PALW v2 agent smoke test: boot golden gate, health, one job over UDS, admission rejections,
and the quarantine path with a tampered golden set.

Usage: misaka-palw-v2-agent-smoke.py <palw-agent> <palw-worker> <gguf> <golden-set>
"""
import hashlib
import json
import os
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import time

AGENT, WORKER, GGUF, GOLDEN = sys.argv[1:5]
SOCK = "/tmp/misaka-palw-agent-smoke.sock"
ENV = {"MISAKA_PALW_GGUF": GGUF, "MISAKA_PALW_GOLDEN": GOLDEN, "PATH": "/usr/bin:/bin"}

man = json.loads(subprocess.run([WORKER, "--mode", "v2-manifest"], capture_output=True, check=True, env=ENV).stdout)
assert man["golden_registered"] is True
H = lambda k: bytes.fromhex(man[k])

def envelope(job_seed=0x11, tamper_manifest=False):
    ids = [1000, 42, 7, 31337, 9999, 5]
    rmh = bytearray(H("runtime_manifest_hash_v2"))
    if tamper_manifest:
        rmh[0] ^= 1
    out = b""
    out += struct.pack("<H", 2)
    nid = b"misaka-devnet"
    out += struct.pack("<I", len(nid)) + nid
    out += bytes([job_seed]) * 64            # job_id
    out += b"\x22" * 64                      # job_nullifier
    out += struct.pack("B", 0)               # mode Execute
    out += H("model_profile_id") + bytes(rmh) + H("runtime_class_id")
    out += H("shape_profile_id_v2") + H("trace_scheme_id_v2") + H("cu_ruleset_id_v2")
    out += b"\xab" * 32
    out += struct.pack("<I", len(ids)) + b"".join(struct.pack("<I", t) for t in ids)
    out += struct.pack("<I", 16) + struct.pack("<I", 4096)
    out += b"\x99" * 64 + struct.pack("<Q", 7) + struct.pack("<Q", 0)
    return bytes(out)

def request(payload, timeout=400):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.settimeout(timeout)
    s.connect(SOCK)
    s.sendall(struct.pack("<I", len(payload)) + payload)
    # Protocol contract (misaka-palw-agent-borsh/v1): one request per connection — the client
    # half-closes its write side so the agent can verify no trailing bytes follow the frame.
    s.shutdown(socket.SHUT_WR)
    def read_exact(n):
        buf = b""
        while len(buf) < n:
            chunk = s.recv(n - len(buf))
            assert chunk, "connection closed mid-frame"
            buf += chunk
        return buf
    (ln,) = struct.unpack("<I", read_exact(4))
    body = read_exact(ln)
    s.close()
    return body

def parse_string(body, off):
    (ln,) = struct.unpack_from("<I", body, off)
    return body[off + 4 : off + 4 + ln].decode(), off + 4 + ln

def wait_for_sock(proc, deadline_s=600):
    start = time.time()
    while time.time() - start < deadline_s:
        if proc.poll() is not None:
            raise AssertionError(f"agent exited early with {proc.returncode}")
        if os.path.exists(SOCK):
            try:
                socket.socket(socket.AF_UNIX, socket.SOCK_STREAM).connect(SOCK)
                return
            except OSError:
                pass
        time.sleep(0.5)
    raise AssertionError("agent socket never became connectable")

AGENT_LOG = "/tmp/misaka-palw-agent-smoke.log"

def start_agent(extra_env=None, args=()):
    env = dict(ENV, **(extra_env or {}))
    return subprocess.Popen([AGENT, "--listen", SOCK, "--worker", WORKER, *args],
                            env=env, stdout=subprocess.DEVNULL, stderr=open(AGENT_LOG, "ab"))

# --- 1. boot with a valid golden set: selftest must pass before the socket serves ---
agent = start_agent()
try:
    wait_for_sock(agent)

    body = request(struct.pack("B", 1))  # Health
    assert body[0] == 3, "Health response tag"
    state, selftest = body[1], body[2]
    assert state == 0 and selftest == 1, f"expected Ready+selftest_passed, got state={state} selftest={selftest}"
    print("health ok: Ready, selftest_passed")

    # --- 2. one real job through the agent ---
    env_bytes = envelope(job_seed=0x11)
    body = request(struct.pack("B", 0) + env_bytes)
    assert body[0] == 0, f"expected JobOk, got tag {body[0]}: {body[1:200]}"
    off = 1
    (ver,) = struct.unpack_from("<H", body, off); off += 2
    assert ver == 2
    req_hash = body[off : off + 64]; off += 64
    assert req_hash == hashlib.blake2b(env_bytes, digest_size=64, key=b"misaka-palw/job-request/v2").digest(), \
        "agent-forwarded request hash must equal the canonical envelope encoding's hash"
    job_id = body[off : off + 64]; off += 64
    assert job_id == bytes([0x11]) * 64
    off += 64  # job_context_hash
    root = body[off : off + 64]; off += 64
    print(f"job ok: root={root[:8].hex()}…")

    # --- 3. duplicate job id -> rejected ---
    body = request(struct.pack("B", 0) + envelope(job_seed=0x11))
    assert body[0] == 1, "duplicate must be JobRejected"
    code, _ = parse_string(body, 1)
    assert code == "duplicate_job", code
    print("probe ok: duplicate_job")

    # --- 4. wrong runtime identity -> rejected without a spawn ---
    t0 = time.time()
    body = request(struct.pack("B", 0) + envelope(job_seed=0x12, tamper_manifest=True))
    assert body[0] == 1
    code, _ = parse_string(body, 1)
    assert code == "runtime_identity_mismatch", code
    assert time.time() - t0 < 5, "identity rejection must not load the model"
    print("probe ok: runtime_identity_mismatch (fast)")

    # --- 5. malformed request -> rejected ---
    body = request(b"\x09garbage")
    assert body[0] == 1
    code, _ = parse_string(body, 1)
    assert code == "invalid_request", code
    print("probe ok: invalid_request")

    # --- 6. health counters ---
    body = request(struct.pack("B", 1))
    off = 1 + 1 + 1 + 64 + 64 + 4
    total, ok_count, rejected, failed_count, timeouts = struct.unpack_from("<QQQQQ", body, off)
    assert (total, ok_count, failed_count, timeouts) == (3, 1, 0, 0), (total, ok_count, failed_count, timeouts)
    assert rejected >= 2
    print(f"health counters ok: total={total} ok={ok_count} rejected={rejected}")
finally:
    agent.send_signal(signal.SIGKILL)
    agent.wait()

# --- 7. tampered golden set -> agent boots QUARANTINED (and rebinds the stale socket) ---
with tempfile.NamedTemporaryFile(suffix=".golden", delete=False) as tf:
    data = bytearray(open(GOLDEN, "rb").read())
    data[-20] ^= 1
    tf.write(bytes(data))
    tampered = tf.name
try:
    agent = start_agent(extra_env={"MISAKA_PALW_GOLDEN": tampered})
    try:
        wait_for_sock(agent)
        body = request(struct.pack("B", 1))
        assert body[0] == 3 and body[1] == 2, f"expected Quarantined(2), got state={body[1]}"
        body = request(struct.pack("B", 0) + envelope(job_seed=0x21))
        assert body[0] == 1
        code, _ = parse_string(body, 1)
        assert code == "quarantined", code
        print("probe ok: tampered golden -> QUARANTINED, jobs rejected, stale socket rebound")
    finally:
        agent.send_signal(signal.SIGKILL)
        agent.wait()
finally:
    os.unlink(tampered)

# --- 8. no golden registered and no --allow-ungated -> refuses to serve ---
env8 = dict(ENV)
env8.pop("MISAKA_PALW_GOLDEN")
agent = subprocess.Popen([AGENT, "--listen", SOCK, "--worker", WORKER],
                         env=env8, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
rc = agent.wait(timeout=120)
assert rc != 0, "an ungated boot must refuse without --allow-ungated"
print("probe ok: ungated boot refused (fail closed)")

if os.path.exists(SOCK):
    os.unlink(SOCK)
print("\nALL AGENT SMOKE TESTS PASSED")
