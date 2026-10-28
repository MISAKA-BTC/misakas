#!/usr/bin/env python3
"""Exact replica of MISAKA's SampledDifficultyManager (consensus/src/processes/difficulty.rs)
plus a generative model of PALW mining, for Track A gate 3.

  replay <miner.log...>  verify the replica reproduces every live block's bits bit-for-bit
  simulate               generative trajectories (ramp, convergence, multi-miner, testnet)

Replica scope: linear chains (single miner). Multi-miner live data is analyzed behaviorally.
"""
import random, re, sys

MAX_TARGET = (1 << 255) - 1
GENESIS_BITS = 0x207FFFFF
# Genesis is EXCLUDED from the DAA window (window.rs:152: fixed timestamp) — verified bit-exact vs live.
WINDOW = 264            # devnet difficulty_window_size (sample rate 1)
MIN_WINDOW = 150        # MIN_DIFFICULTY_WINDOW_SIZE
TARGET_TIME_MS = 10_000 # devnet 0.1 bps

def target_from_bits(bits: int) -> int:
    unshifted = bits >> 24
    if unshifted <= 3:
        mant, expt = (bits & 0xFFFFFF) >> (8 * (3 - unshifted)), 0
    else:
        mant, expt = bits & 0xFFFFFF, 8 * (unshifted - 3)
    return 0 if mant > 0x7FFFFF else mant << expt

def bits_from_target(t: int) -> int:
    size = (t.bit_length() + 7) // 8
    compact = (t << (8 * (3 - size))) if size <= 3 else t >> (8 * (size - 3))
    if compact & 0x00800000:
        compact >>= 8
        size += 1
    return (compact | (size << 24)) & 0xFFFFFFFF

def next_bits(window: list[tuple[int, int]], target_time_ms: int = TARGET_TIME_MS) -> int:
    """window: [(timestamp_ms, bits)] of the POV block's up-to-264 most recent ancestors."""
    blocks = list(window)
    if len(blocks) < MIN_WINDOW:
        return blocks[-1][1] if blocks else GENESIS_BITS
    ts = [b[0] for b in blocks]
    min_i = ts.index(min(ts))
    min_ts, max_ts = min(ts), max(ts)
    blocks[min_i] = blocks[-1]  # swap_remove of the min-timestamp block
    blocks.pop()
    n = len(blocks)
    avg_target = sum(target_from_bits(b[1]) for b in blocks) // n
    measured = max(max_ts - min_ts, 1)
    expected = target_time_ms * 1 * n   # sample_rate = 1
    new_target = min(avg_target * measured // expected, MAX_TARGET)
    return bits_from_target(new_target)

# ── replay: live log -> bit-exact verification ────────────────────────────────

def replay(paths: list[str]) -> None:
    pat = re.compile(r"daa_score=(\d+), bits=(0x[0-9a-f]+), ts=(\d+), blue_score=(\d+)")
    rows = []
    for p in paths:
        for line in open(p):
            m = pat.search(line)
            if m:
                rows.append((int(m.group(4)), int(m.group(3)), int(m.group(2), 16)))
    rows.sort()
    heights = [r[0] for r in rows]
    if heights != list(range(heights[0], heights[0] + len(rows))):
        print(f"[replay] NOTE: non-contiguous blue_scores (parallel blocks?) — {len(rows)} rows, "
              f"range {heights[0]}..{heights[-1]}; bit-exact replay requires the linear prefix only")
    chain = []  # window.rs:152 — genesis never enters the DAA window
    mismatches = checked = 0
    first_real_adjust = None
    for bs, ts, bits in rows:
        win = chain[-WINDOW:]
        predicted = next_bits(win)
        checked += 1
        if predicted != bits:
            mismatches += 1
            if mismatches <= 5:
                print(f"  MISMATCH at blue_score {bs}: live {bits:#010x} vs replica {predicted:#010x}")
        if first_real_adjust is None and bits != GENESIS_BITS:
            first_real_adjust = bs
        chain.append((ts, bits))
    print(f"[replay] {len(rows)} blocks checked, {mismatches} mismatches"
          + (f"; first bits change at blue_score {first_real_adjust}" if first_real_adjust else "; bits never moved"))
    if len(rows) > 1:
        span = (rows[-1][1] - rows[0][1]) / 1000
        print(f"[replay] wall span {span:.0f}s, mean interval {span / (len(rows) - 1):.2f}s, "
              f"final bits {rows[-1][2]:#010x} = {target_from_bits(GENESIS_BITS) / max(target_from_bits(rows[-1][2]), 1):.2f}x genesis difficulty")
        # interval trajectory in 50-block segments
        for s in range(0, len(rows) - 1, 50):
            seg = rows[s:s + 51]
            if len(seg) > 1:
                iv = (seg[-1][1] - seg[0][1]) / (len(seg) - 1) / 1000
                print(f"    blocks {seg[0][0]:>3}-{seg[-1][0]:<3}: interval {iv:6.2f}s  bits {seg[-1][2]:#010x}")

# ── generative model ──────────────────────────────────────────────────────────

def simulate(attempt_ms: float, n_blocks: int, *, seed: int = 7, genesis_age_ms: float = 0.0,
             second_miner_at: int | None = None, target_time_ms: int = TARGET_TIME_MS,
             label: str = "", quiet: bool = False):
    """Each attempt costs attempt_ms wall-clock; p = target/2^256 per attempt; with two miners
    a round of parallel attempts still costs attempt_ms. Timestamps = discovery time."""
    rng = random.Random(seed)
    t = float(genesis_age_ms)
    chain = []  # window.rs:152 — genesis never enters the DAA window
    out = []
    for h in range(1, n_blocks + 1):
        active = 2 if (second_miner_at is not None and h >= second_miner_at) else 1
        bits = next_bits(chain[-WINDOW:], target_time_ms)
        p = target_from_bits(bits) / (1 << 256)
        p_round = 1 - (1 - p) ** active
        rounds = 1
        while rng.random() >= p_round:
            rounds += 1
        t += rounds * attempt_ms
        chain.append((int(t), bits))
        out.append((h, int(t), bits, rounds * active))
    if not quiet:
        last = out[-50:]
        iv = (last[-1][1] - last[0][1]) / (len(last) - 1) / 1000
        att = sum(x[3] for x in last) / len(last)
        print(f"[sim {label}] attempt={attempt_ms/1000:.1f}s T={target_time_ms/1000:.0f}s"
              f"{f' 2nd-miner@{second_miner_at}' if second_miner_at else ''}"
              f" genesis_age={genesis_age_ms/86_400_000:.0f}d: {n_blocks} blocks in {t/60000:.1f}min wall,"
              f" last-50 interval {iv:.2f}s, attempts/block {att:.1f},"
              f" final difficulty {target_from_bits(GENESIS_BITS)/target_from_bits(out[-1][2]):.1f}x genesis")
    return out

def phase_table(out, phases):
    for lo, hi in phases:
        seg = [o for o in out if lo <= o[0] < hi]
        if len(seg) > 1:
            iv = (seg[-1][1] - seg[0][1]) / (len(seg) - 1) / 1000
            att = sum(o[3] for o in seg) / len(seg)
            print(f"    blocks {lo:>3}-{hi:<4}: interval {iv:7.2f}s  attempts/block {att:5.1f}  bits {seg[-1][2]:#010x}")

if __name__ == "__main__":
    if len(sys.argv) >= 3 and sys.argv[1] == "replay":
        replay(sys.argv[2:])
        sys.exit(0)
    print("=== devnet (T=10s): Metal 0.97s attempts, single miner ===")
    out = simulate(968, 620, label="devnet-1m")
    phase_table(out, [(1, 151), (151, 265), (265, 400), (400, 620)])
    print("=== devnet: second miner joins at 400 ===")
    out = simulate(968, 620, second_miner_at=400, label="devnet-2m@400")
    phase_table(out, [(265, 400), (400, 500), (500, 620)])
    print("=== public testnet (T=120s, window 264/min 150), fleet attempt times ===")
    for ms, lbl in [(6100, "B-epyc-6c"), (7800, "A-broadwell"), (15700, "ibm-slowest")]:
        out = simulate(ms, 700, seed=11, target_time_ms=120_000, label=f"testnet/{lbl}")
        phase_table(out, [(1, 151), (151, 400), (400, 700)])
