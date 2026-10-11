#!/usr/bin/env python3
"""Gate-3 final analysis: live devnet chain vs the bit-exact DAA replica + sim expectations."""
import re, sys
from palw_daa_sim import next_bits, GENESIS_BITS, WINDOW, target_from_bits

WORKDIR = sys.argv[1] if len(sys.argv) > 1 else "gate3"
BLOCK = re.compile(r"\[(rig\d)\] mined block #\d+ \(nonce=\d+, daa_score=(\d+), bits=(0x[0-9a-f]+), ts=(\d+), blue_score=(\d+)(?:, coinbase0=(\d+), coinbase_outs=(\d+))?\)")
ATTEMPT = re.compile(r"^(\S+ \S+) \[(?:DEBUG|INFO )\] PALW attempt \d+ (rejected|accepted)")

rows, attempts = [], []
for path in [f"{WORKDIR}/miner1.log", f"{WORKDIR}/miner2.log"]:
    try:
        f = open(path)
    except FileNotFoundError:
        continue
    for line in f:
        m = BLOCK.search(line)
        if m:
            rows.append({
                "rig": m.group(1), "daa": int(m.group(2)), "bits": int(m.group(3), 16),
                "ts": int(m.group(4)), "bs": int(m.group(5)),
                "cb0": int(m.group(6)) if m.group(6) else None,
                "cbouts": int(m.group(7)) if m.group(7) else None,
            })
        elif ATTEMPT.search(line):
            attempts.append(line[:23])

rows.sort(key=lambda r: (r["bs"], r["ts"]))
print(f"total mined blocks: {len(rows)} (rig1 {sum(r['rig']=='rig1' for r in rows)}, rig2 {sum(r['rig']=='rig2' for r in rows)})")

# 1. Linearity: find parallel blocks (duplicate or non-consecutive blue scores)
bss = [r["bs"] for r in rows]
dups = len(bss) - len(set(bss))
print(f"blue_score duplicates (parallel blocks): {dups}")

# 2. Bit-exact replay over the linear prefix
chain, mism, checked = [], 0, 0
seen = set()
for r in rows:
    if r["bs"] in seen:
        break  # first parallel block ends the strictly-linear replay
    seen.add(r["bs"])
    pred = next_bits(chain[-WINDOW:])
    checked += 1
    if pred != r["bits"]:
        mism += 1
        if mism <= 5:
            print(f"  MISMATCH bs={r['bs']}: live {r['bits']:#010x} replica {pred:#010x}")
    chain.append((r["ts"], r["bits"]))
print(f"[replica] linear prefix: {checked}/{len(rows)} blocks checked, {mism} mismatches")

# 3. Phase table: intervals + difficulty from live data
def seg_stats(lo, hi):
    seg = [r for r in rows if lo <= r["bs"] < hi]
    if len(seg) < 2:
        return None
    iv = (seg[-1]["ts"] - seg[0]["ts"]) / (len(seg) - 1) / 1000
    dif = target_from_bits(GENESIS_BITS) / target_from_bits(seg[-1]["bits"])
    return f"blocks {lo:>3}-{hi:<4}: interval {iv:6.2f}s  difficulty {dif:5.2f}x  bits {seg[-1]['bits']:#010x}  n={len(seg)}"

p2 = min((r["bs"] for r in rows if r["rig"] == "rig2"), default=None)
phases = [(1, 151), (151, 265), (265, 400)]
if p2:
    last = rows[-1]["bs"] + 1
    phases += [(400, p2), (p2, p2 + 100), (p2 + 100, last)]
else:
    phases += [(400, rows[-1]["bs"] + 1)]
print(f"[phases] second miner first block: bs={p2}")
for lo, hi in phases:
    s = seg_stats(lo, hi)
    if s:
        print("   ", s)

# 4. Attempts per block (all attempt lines vs blocks, whole run + converged tail)
print(f"[attempts] total attempt lines (both rigs): {len(attempts)}, blocks {len(rows)}, "
      f"overall attempts/block {len(attempts)/max(len(rows),1):.1f}")

# 5. Emission: coinbase from rig2 lines (patched binary). Full month-0 subsidy is
# 3_704_683_450 sompi/s x 10 s = 37_046_834_500; the ADR-0018 §F split pays the miner the
# worker BASE share (62%, params.rs fee_split) = 22_969_037_390. Validator 30% is
# don't-minted with no bonded validators; inclusion 8% goes to the §D pool path.
cbs = [(r["cb0"], r["cbouts"]) for r in rows if r["cb0"] is not None]
if cbs:
    vals = set(cbs)
    full = 37_046_834_500
    expect = full * 6200 // 10_000
    ok = all(v == expect and o in (1, 2) for v, o in vals)
    merged = sum(1 for _, o in cbs if o == 2)
    print(f"[emission] {len(cbs)} coinbases observed, distinct {vals} ({merged} merge blocks pay 2 outputs)")
    print("           " + (f"MATCHES worker-base 62% of the month-0 subsidy ({expect} of {full}) per merged block"
                           if ok else f"UNEXPECTED (want {expect} = 62% of {full})"))
else:
    print("[emission] no coinbase-annotated blocks yet (rig2 only)")
