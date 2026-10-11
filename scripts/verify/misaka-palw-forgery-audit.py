#!/usr/bin/env python3
"""PALW algo_id=4 forgery-resistance audit at scale.

Replays the exact PoW tag path (palw_pow_seed_v1 -> palw_pow_prompt_v1 ->
palw-worker --mode verify) over 61 inferences:
  - pow30:     30 seeds via the real keyed-BLAKE2b derivation (nonce x20,
               timestamp x5, pre_pow_hash x5 varied)
  - uniform20: 20 uniform-random 32-byte seeds (deterministic PRNG chain)
  - adv10:     10 adversarial low-entropy seeds (all-zero, all-ff, single-bit,
               counting, and 1-bit-flip pairs for avalanche measurement)
  - repeat1:   the first pow seed re-run (determinism control)

This is the same methodology that caught the algo_id=5 (Ollama) constant-tag
forgery (27/27 identical). Results go to results.jsonl; analysis is separate.
"""
import hashlib, json, os, subprocess, sys, time

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "palw_audit_results.jsonl")
WORKER = os.environ["PALW_WORKER"]
DOMAIN = b"misaka-l1-palw-llm-v1"
NETWORK_ID = b"misaka-testnet-11"  # realistic length-prefixed net id; audit result is net-id-agnostic
N_PREDICT = "128"

def pow_seed(pre_pow_hash: bytes, timestamp: int, nonce: int) -> bytes:
    h = hashlib.blake2b(key=DOMAIN, digest_size=32)
    h.update(b"seed")
    h.update(len(NETWORK_ID).to_bytes(2, "little"))
    h.update(NETWORK_ID)
    h.update(pre_pow_hash)
    h.update(timestamp.to_bytes(8, "little"))
    h.update(nonce.to_bytes(8, "little"))
    return h.digest()

def prng(label: str, n: int) -> bytes:
    """Deterministic uniform bytes: BLAKE2b chain keyed off a fixed audit label."""
    out = b""
    counter = 0
    while len(out) < n:
        out += hashlib.blake2b(f"palw-audit-2026-08-16/{label}/{counter}".encode(), digest_size=32).digest()
        counter += 1
    return out[:n]

def build_seeds():
    seeds = []  # (group, label, seed_bytes)
    h1 = prng("pre-pow-hash-1", 64)
    ts1 = 1_776_000_000_000
    # pow30: the real derivation, nonce-swept (the actual mining loop shape)
    for nonce in range(20):
        seeds.append(("pow", f"nonce{nonce}", pow_seed(h1, ts1, nonce)))
    for i in range(5):
        seeds.append(("pow", f"ts+{i+1}", pow_seed(h1, ts1 + i + 1, 0)))
    for i in range(5):
        seeds.append(("pow", f"hash{i+2}", pow_seed(prng(f"pre-pow-hash-{i+2}", 64), ts1, 0)))
    # uniform20
    for i in range(20):
        seeds.append(("uniform", f"u{i}", prng(f"uniform-{i}", 32)))
    # adv10: low-entropy / structured probes for attractor behavior + avalanche pairs
    adv = [
        ("zero", bytes(32)),
        ("ff", bytes([0xFF] * 32)),
        ("bit0", bytes([0x01] + [0] * 31)),          # 1 bit away from zero
        ("bit255", bytes([0] * 31 + [0x80])),        # 1 bit away from zero, other end
        ("counting", bytes(range(32))),
        ("aa", bytes([0xAA] * 32)),
        ("ascii", b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
    ]
    base = prng("avalanche-base", 32)
    flip1 = bytes([base[0] ^ 0x01]) + base[1:]        # 1-bit flip of base
    flip2 = base[:31] + bytes([base[31] ^ 0x80])      # another 1-bit flip
    adv += [("avalanche-base", base), ("avalanche-flip1", flip1), ("avalanche-flip2", flip2)]
    for label, s in adv:
        seeds.append(("adv", label, s))
    # repeat1: determinism control — byte-identical rerun of the first pow seed
    seeds.append(("repeat", "nonce0-rerun", pow_seed(h1, ts1, 0)))
    return seeds

def run_one(seed: bytes):
    prompt = f"MISAKA PALW proof-of-work v1\nseed: {seed.hex()}\ncontinue:"
    t0 = time.time()
    proc = subprocess.run(
        [WORKER, "--mode", "verify", "--prompt-stdin", "--n-predict", N_PREDICT],
        input=prompt.encode(), capture_output=True, timeout=300,
    )
    elapsed = time.time() - t0
    if proc.returncode != 0:
        return {"error": proc.stderr.decode(errors="replace")[-400:], "secs": round(elapsed, 2)}
    lines = [l for l in proc.stdout.decode(errors="replace").splitlines() if l.strip()]
    doc = json.loads(lines[-1])
    doc["_secs"] = round(elapsed, 2)
    return doc

def main():
    seeds = build_seeds()
    done = set()
    if os.path.exists(OUT):
        with open(OUT) as f:
            for line in f:
                done.add(json.loads(line)["label"])
    with open(OUT, "a") as f:
        for i, (group, label, seed) in enumerate(seeds):
            key = f"{group}/{label}"
            if key in done:
                continue
            doc = run_one(seed)
            rec = {"label": key, "seed": seed.hex(), **doc}
            f.write(json.dumps(rec) + "\n")
            f.flush()
            status = "ERR" if "error" in doc else f"{doc.get('decode_tokens','?')}tok {doc.get('_secs')}s"
            print(f"[{i+1}/{len(seeds)}] {key}: {status}", flush=True)

if __name__ == "__main__":
    main()
