# ADR-0153 (draft) — 2M verification by shards: the implementation plan

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

* Status: **Draft plan** (rcore/cap-s1, stage 7 of the user's capacity plan, 2026-09-27). **Decides
  nothing and arms nothing**: the 2M class stays capped — `c_2M = 1` held to `Final` (its row's static
  `max_inflight_claims`, which no ρ scales), on the C7 list (ADR-0152 D7, ADR-0160 D-13), uncredited:
  its escrow `E`, its seats' duty and lock and its `G` today's at every ρ (F1); only its weight
  reservation is `⌈w/ρ⌉` like every attempt's under F-W, the whole bond liable behind it (AG-2) — until
  the gates below pass and the user decides.
* Supersedes nothing. Related: ADR-0152 §4-quater (held attention, the 2M deadline redesign of
  2026-09-24), ADR-0160 (capacity; 2M is excluded from every ρ step's credit), ADR-0151 (liveness is
  structural), the kv-codec resource profile (2026-09-23).

## 1. Why 2M is capped, in physical numbers

Verification capacity is physical — replay time and memory on the verifier's hardware — and no
collateral or protocol rule buys it:

| quantity (2M context, 1.5B dense, A16-KV-i16) | measured / derived |
|---|---|
| K/V resident for one execution | 7.00 GiB (i32: 14.00 GiB) |
| one replay's need (artifact, K/V, attention and trace scratch, Fold capture, leg) | 12.42 GiB |
| prefill throughput on the reference host | ≈ 4 positions/s → 262,143 positions ≥ 18 h |
| one full replay | ≈ 9.7 days ≈ 6,984 DAA at 120 s/DAA — longer than the court window (3,000 DAA) |
| an S1 partial seat's replay hashes (dense capture) | ≈ 607 GiB at segment 0 — S1 is structurally infeasible for 2M |
| attempt step space | 27,002,845,160 leaves (dense capture 1.7 TB; held classes Fold) |
| uncovered loss per 2M claim at launch (C) | ≤ 63,599 MSK, `c_2M = 1` (≤ 9.35 M MSK / month) |

So one honest verifier cannot prove a 2M fraud inside today's deadlines, and ρ multiplies claims, not
verifiers. The cap is the only sound rule until verification itself is split.

## 2. The idea: a 2M execution verified as independent shards

A 2M execution is split at `K` checkpoints (every `P` positions, per class, from measurement) into
shards; shard `j` is replayable from checkpoint `j−1`'s state alone. A panel verifies a claim by
assigning shards to seats (each seat replays ≤ `P` positions — hours, not days), and a court localises
a dispute to one shard. Capacity then scales with seats × GPUs, not with one host's 9.7 days.

## 3. Work items (each a lane with its own fence; order is the dependency order)

1. **Measure (no fence).** On the reference GPU host and the fleet's: positions/s by context depth; the
   time to build, serialise and load a checkpoint (K/V at a boundary: ≈ 7 GiB i16 at depth 2M, scaling
   with depth); the time from detection to a complete court evidence object; per-shard replay at `P` ∈
   {16k, 32k, 65k}. Deliverable: the table the deadlines are derived from (the user's rule of
   2026-09-24: deadlines per class from measured cost + margin, never one global number).
2. **Checkpoint commitments (consensus).** The attempt commits a root over the shard boundary states
   (the Fold capture already derives the roots dense capture does; add the boundary state digests as
   leaves). A claim's material names them; a shard is replayed from its boundary.
3. **Checkpoint availability (DA).** A boundary state is 1–7 GiB: the producer must serve it on demand
   within the plain-response deadline (42 DAA), or default (an existing DA unit kind, sized). Retention
   until the claim's liability window closes.
4. **Shard assignment in the panel (consensus).** Each seat is drawn for a shard subset (the segment
   mask machinery, `PalwSegmentMaskV2`, generalised from positions to checkpoint shards), with
   `k ≥ 2` coverage per shard; the licence's coverage basis counts shards, not whole replays.
5. **Shard courts (consensus).** A dissection localises to one shard, whose re-execution fits the
   compute-bearing deadline; a forged boundary state is refuted by the previous shard's replay
   (chain of custody). The held-attention (A-held) procedure is the template.
6. **Three deadline kinds, per class, derived** (the user's split): plain response, compute-bearing
   verification (from item 1's shard time + margin), economic liability (never released before
   verification can conclude).
7. **Economic `Final` for 2M re-enabled** only when items 2–6 are live and an honest verifier has proven
   a planted 2M fraud inside the derived deadline on a drill (the user's condition).
8. **Only then**: `c_2M ≥ 2`, 2M credit and 2M in a capacity step — each its own flag day, never by ρ.

## 4. Gates before any 2M cap moves

* G-A: item 1's table published; shard replay ≤ the compute-bearing deadline with ≥ 2× margin.
* G-B: a drill proves a planted 2M fraud (one forged shard) and the forger's bond is taken, with the
  checkpoint served by DA.
* G-C: the verifier fleet's shard throughput covers the target 2M claim rate with k ≥ 2 per shard.
* G-D: the user's decision.

## 5. What stage 7 of the capacity plan ships instead

ρ 1000 as its own package for the floor only (ADR-0160 §9.1), with every capacity fence at or below it
and `k_aud = 2`; 2M stays at `c_2M = 1`, uncredited, off the capacity list's credit
(`palw_capacity_class_creditable_v1`: the floor only), and a test pins that no package moves it.
