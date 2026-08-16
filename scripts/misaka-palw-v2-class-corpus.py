#!/usr/bin/env python3
"""PALW v2 determinism-class probe over a RANDOM job corpus.

    MISAKA_PALW_GGUF=/path/to.gguf \
      scripts/misaka-palw-v2-class-corpus.py ./palw-worker <label> [--jobs N] [--master-seed HEX]

Emits one class line on stdout in the SAME `misaka.palw.v2-class-line.v1` schema the fixed-corpus
probe uses, so `misaka-palw-v2-class-compare.py` consumes it unchanged. Collect one per host and
compare.

WHY THIS EXISTS ALONGSIDE THE FIXED PROBE
-----------------------------------------
`golden_probe_inputs()` is 4 jobs whose longest prefill is 96 tokens, while the shape profile
allows up to 512 — it pins `prefill-single-batch` with `n_batch = n_ubatch = 512`, and the worker
refuses anything longer ("prefill N exceeds the single-batch prefill schedule"). So the reachable
prefill range is 1…512 and the fixed corpus exercises only its first fifth. This corpus covers the
rest, including the 512 boundary itself and the powers-of-two either side of the internal tiling
steps, and varies decode budgets and token-id structure (uniform / repeated / ascending / small-id,
which drive different kernel paths than uniform noise alone).

Worth noting what this means for the divergence the repo previously found: "one seed still flips
arm64-vs-x86 in the batched prefill GEMM" was a MULTI-batch phenomenon, and v2 removed that path
by construction rather than by testing it — `prefill-single-batch` is in the pinned shape string
and out-of-profile jobs fail closed. That is why this corpus stops at 512 instead of straddling it.

Same jobs on every host, derived from a master seed with no clock and no RNG state, so two hosts
provably run the identical corpus — `corpus_digest` proves it and the comparator refuses to compare
hosts whose corpora differ.

FIELD MAPPING onto the shared class-line schema
  jobs        -> one entry per corpus job (expected_root = full_logits_trace_root)
  golden_root -> aggregate digest over every job's full projection, in corpus order
  corpus_digest -> digest over the job INPUTS only; must match across hosts or the run is invalid
  golden_file -> "n/a (corpus mode)"; there is no registered set in this mode
"""
import argparse
import hashlib
import json
import os
import platform
import struct
import subprocess
import sys

AP = argparse.ArgumentParser()
AP.add_argument("worker")
AP.add_argument("label")
AP.add_argument("--jobs", type=int, default=24, help="jobs PER SEED")
AP.add_argument("--master-seed", default="misaka-palw-v2-class-corpus-v1")
AP.add_argument("--seed-count", type=int, default=1,
                help="independent master seeds to iterate; seed i is '<master>#<i>'. Repeating the "
                     "shape set under different content is what turns one corpus into a sample.")
AP.add_argument("--timeout", type=int, default=1800)
AP.add_argument("--network-id", default="misaka-devnet")
A = AP.parse_args()

GGUF = os.environ.get("MISAKA_PALW_GGUF")
if not GGUF:
    sys.exit("MISAKA_PALW_GGUF must point at the pinned GGUF")
ENV = {"MISAKA_PALW_GGUF": GGUF, "PATH": "/usr/bin:/bin:/usr/local/bin"}

# Identity, with no golden registered (registering one moves runtime_manifest_hash).
ENV_NOGOLD = dict(ENV)
man = json.loads(subprocess.run([A.worker, "--mode", "v2-manifest"], capture_output=True, check=True,
                                env=ENV_NOGOLD).stdout)
H = lambda k: bytes.fromhex(man[k])
NID = A.network_id.encode()

N_BATCH = 512          # from the pinned shape string
MAX_CTX = 4096         # must equal the shape profile's n_ctx or the worker rejects the job
N_VOCAB_SAFE = 200_000  # comfortably inside the measured n_vocab (248320)


def kdf(*parts: bytes) -> bytes:
    h = hashlib.blake2b(digest_size=64, key=b"misaka-palw-corpus-kdf")
    for p in parts:
        h.update(struct.pack("<I", len(p)))
        h.update(p)
    return h.digest()


def ids_from(seed: bytes, n: int, pattern: str) -> list:
    """Deterministic token ids. `pattern` varies the STRUCTURE, not just the values —
    repeats and runs stress different kernel paths than uniform noise does."""
    out = []
    stream = b""
    i = 0
    while len(stream) < n * 4 + 4:
        stream += kdf(seed, b"ids", struct.pack("<I", i))
        i += 1
    if pattern == "uniform":
        for k in range(n):
            (v,) = struct.unpack_from("<I", stream, k * 4)
            out.append(v % N_VOCAB_SAFE)
    elif pattern == "repeat":
        (v,) = struct.unpack_from("<I", stream, 0)
        out = [v % N_VOCAB_SAFE] * n
    elif pattern == "ascending":
        (base,) = struct.unpack_from("<I", stream, 0)
        out = [(base % 1000 + k) % N_VOCAB_SAFE for k in range(n)]
    elif pattern == "low":  # small ids cluster in the embedding table's first rows
        for k in range(n):
            (v,) = struct.unpack_from("<I", stream, k * 4)
            out.append(v % 256)
    else:
        raise AssertionError(pattern)
    return out


def build_corpus(master: bytes, count: int) -> list:
    """Prefill lengths spanning the whole reachable 1…512 single-batch range.

    The fixed corpus stops at 96. Everything from 97 to 512 — four fifths of the legal range,
    including the boundary itself — was never measured on any host before this."""
    lengths = [1, 2, 7, 32, 63, 64, 65, 96, 127, 128, 200, 255, 256, 257, 300, 400, 500, 511, 512]
    decodes = [1, 2, 8, 16]
    patterns = ["uniform", "repeat", "ascending", "low"]
    jobs = []
    for k in range(count):
        seed = kdf(master, b"job", struct.pack("<I", k))
        plen = lengths[k % len(lengths)]
        dec = decodes[(k // len(lengths)) % len(decodes)] if k >= len(lengths) else decodes[k % len(decodes)]
        pat = patterns[k % len(patterns)]
        assert plen <= N_BATCH, f"{plen} exceeds the single-batch prefill schedule; the worker would refuse it"
        jobs.append({
            "name": f"c{k:02d}-p{plen}-d{dec}-{pat}",
            "prefill_len": plen,
            "decode": dec,
            "pattern": pat,
            "beyond_fixed_corpus": plen > 96,
            "ids": ids_from(seed, plen, pat),
            "execution_seed": kdf(seed, b"exec")[:32],
            "job_id": kdf(seed, b"job-id"),
            "job_nullifier": kdf(seed, b"nullifier"),
            "assignment_id": kdf(seed, b"assignment"),
        })
    return jobs


def envelope(job: dict) -> bytes:
    out = b""
    out += struct.pack("<H", 2)                                   # job wire version
    out += struct.pack("<I", len(NID)) + NID
    out += job["job_id"]
    out += job["job_nullifier"]
    out += struct.pack("B", 0)                                    # mode = Execute
    out += H("model_profile_id")
    out += H("runtime_manifest_hash_v2")
    out += H("runtime_class_id")
    out += H("shape_profile_id_v2")
    out += H("trace_scheme_id_v2")
    out += H("cu_ruleset_id_v2")
    out += job["execution_seed"]
    out += struct.pack("<I", len(job["ids"])) + b"".join(struct.pack("<I", t) for t in job["ids"])
    out += struct.pack("<I", job["decode"])
    out += struct.pack("<I", MAX_CTX)
    out += job["assignment_id"]
    out += struct.pack("<Q", 0)                                   # assignment_epoch
    out += struct.pack("<Q", 0)                                   # deadline_unix_ms
    return out


def run_job(payload: bytes):
    frame = struct.pack("<I", len(payload)) + payload
    p = subprocess.run([A.worker, "--mode", "v2-job"], input=frame, capture_output=True,
                       env=ENV, timeout=A.timeout)
    if p.returncode != 0:
        return None, p.stderr.decode(errors="replace")[-400:]
    (ln,) = struct.unpack_from("<I", p.stdout, 0)
    body = p.stdout[4:4 + ln]
    off = 0
    (ver,) = struct.unpack_from("<H", body, off); off += 2
    assert ver == 2, f"result version {ver}"
    req_hash = body[off:off + 64]; off += 64
    expect = hashlib.blake2b(payload, digest_size=64, key=b"misaka-palw/job-request/v2").digest()
    assert req_hash == expect, "request_hash must echo the exact request payload"
    off += 64                                                     # job_id
    proj = {}
    for name in ("job_context_hash", "root", "output_commitment", "schedule"):
        proj[name] = body[off:off + 64].hex(); off += 64
    proj["cu"] = int.from_bytes(body[off:off + 16], "little"); off += 16
    proj["prefill"], proj["decode"], proj["events"] = struct.unpack_from("<III", body, off); off += 12
    (proj["stop"],) = struct.unpack_from("B", body, off); off += 1
    return proj, None


corpus = []
for _s in range(A.seed_count):
    sub = build_corpus(f"{A.master_seed}#{_s}".encode(), A.jobs)
    for j in sub:
        j["seed_index"] = _s
        j["name"] = f"s{_s}-{j['name']}"
    corpus += sub

corpus_digest = hashlib.blake2b(digest_size=64, key=b"misaka-palw-corpus-inputs")
for j in corpus:
    corpus_digest.update(kdf(j["name"].encode(), struct.pack("<I", j["seed_index"]), j["execution_seed"],
                             b"".join(struct.pack("<I", t) for t in j["ids"]),
                             struct.pack("<II", j["decode"], MAX_CTX)))
corpus_digest = corpus_digest.hexdigest()

shapes = sorted({j['prefill_len'] for j in corpus})
print(f"[corpus] {A.label}: {len(corpus)} jobs = {A.seed_count} seed(s) x {A.jobs}, "
      f"{sum(1 for j in corpus if j['beyond_fixed_corpus'])} with prefill > 96 "
      f"(beyond anything the fixed corpus reaches), {len(shapes)} distinct prefill lengths, "
      f"max {max(shapes)}", file=sys.stderr)

results, failures = {}, {}
agg = hashlib.blake2b(digest_size=64, key=b"misaka-palw-corpus-results")
for j in corpus:
    proj, err = run_job(envelope(j))
    if proj is None:
        failures[j["name"]] = err
        print(f"[corpus] {j['name']}: FAILED — {err}", file=sys.stderr)
        continue
    results[j["name"]] = {"expected_root": proj["root"], "expected_cu": str(proj["cu"])}
    agg.update(j["name"].encode())
    for f in ("job_context_hash", "root", "output_commitment", "schedule"):
        agg.update(bytes.fromhex(proj[f]))
    agg.update(struct.pack("<QIII", proj["cu"], proj["prefill"], proj["decode"], proj["events"]))
    print(f"[corpus] {j['name']:<26} prefill={proj['prefill']:>4} decode={proj['decode']:>2} "
          f"events={proj['events']:>2} cu={proj['cu']:<8} root={proj['root'][:16]}…", file=sys.stderr)

line = {
    "schema": "misaka.palw.v2-class-line.v1",
    "mode": "random-corpus",
    "label": A.label,
    "selftest": "pass" if not failures else "FAIL",
    "golden_file": "n/a (corpus mode)",
    "corpus_digest": corpus_digest,
    "corpus_jobs": len(corpus),
    "corpus_seed_count": A.seed_count,
    "corpus_jobs_per_seed": A.jobs,
    "corpus_failures": failures,
    "corpus_master_seed": A.master_seed,
    "host": {"machine": platform.machine(), "system": platform.system(), "release": platform.release()},
    "runtime_class_id": man["runtime_class_id"],
    "runtime_manifest_hash_v2": man["runtime_manifest_hash_v2"],
    "model_profile_id": man["model_profile_id"],
    "shape_profile_id_v2": man["shape_profile_id_v2"],
    "tokenizer_id_v2": man["tokenizer_id_v2"],
    "trace_scheme_id_v2": man["trace_scheme_id_v2"],
    "worker_binary_sha256": man["worker_binary_sha256"],
    "cmake_cache_sha256": man["cmake_cache_sha256"],
    "llama_static_library_sha256": man["llama_static_library_sha256"],
    "ggml_flags": man["ggml_flags"],
    "fp_environment_probe": man.get("fp_environment_probe"),
    "fp_environment_canonical": man.get("fp_environment_canonical"),
    "golden_root": agg.hexdigest(),
    "jobs": results,
}
print(json.dumps(line, sort_keys=True))
