# Emission audit 2026-10-04 (lane EM; written by the lead from the agent's final report)

Verdict: DESIGN/ACCOUNTING, not an attack. All 26,204 coinbase outputs ≥ 50 MSK are integer multiples of 1/50 of the claim
escrow 3,200.8465 MSK (producer leg 40/50, panel seat legs 2/50 each = 128/256/384 MSK); no miner/validator/buyback payouts.

- H1 CONFIRMED (cause): PALW pays one carve per claim-bearing block; the schedule (calc_block_subsidy) assumes one per DAA tick;
  DAA is a slot clock (ADR-0138/0142, ≤1 tick per slot). Live: 3.68 attempt blocks per DAA tick (17,236 / DAA 4,689), 99.7 % became
  claims. Ratio ≈ 3.68 × 0.72 × 0.767 (DAA 156.4 s) × 0.96 (Final rate) ≈ 1.9. PoC: audit/emission e6b72dd1c,
  consensus/core/tests/audit_emission_per_daa.rs (k attempts in a mergeset → ≤1 tick, k carves).
- H2 CONFIRMED: floor class has no absolute cadence control (bits pinned at max, floor outside the epoch budget, class retarget
  normalises the sole producer); epoch 4 produced 2,431 vs budget 998. Floors = 92.5 % of live vesting rows (3.44 claims/DAA vs real 0.21).
- H3 CONFIRMED (design): a floor claim gets the full escrow (~6.9 GMAC ≈ 62 MSK at 9 MSK/G vs 3,200.85 paid, ≈51×).
- Integrity OK: claim uniqueness/replay (work_ids DuplicateWork, attempt-hash claim id), bond binding (payee read at Final),
  Final paid once (escrow withheld once, vesting row moves once). Sybil bound is collateral only (≈1 claim/DAA per 20k MSK bond).
- Corrections to the lead's first read: 09-25..09-30 zero issuance was NOT voids — Final 16,529 vs void 175 (1 %); rewards vest
  ≈3,000 DAA (5.4 days) after Final. Vesting pipeline: created 52.70 M, moved 18.18 M, still vesting 34.5 M MSK (10,848 rows, last
  expiry DAA 7,705) → keeps minting ≈6.1 M/day whatever the next fence does.
- 3 M "gap": not a flow; circulating − Σcoinbase = 9,996,975,574 MSK (3,024,426 below 10 B), matching within 0.014 % the
  3,023,987.93 MSK four native txs moved to tagged non-P2PKH scripts (DAA 2217/2620/4492/4499) — PLAUSIBLE these are excluded from
  circulating; code path not found.
- 5,300 (0b1c11b87): emission path unchanged except dormant F-EM etc.; F-EM 16 carves/DAA = 51,213.5 MSK/DAA = 11.52 × calc_block_subsidy.
- Next-fence pitfalls (8): escrow fixed at acceptance vs ΣW known at DAA close (withhold full carve, allocate at close/Final,
  collateral on max share); ledger keyed by accepting block's DAA; apply to every subsidy carve (PALW 72 %, validator 20 %,
  inclusion 8 %) from accepting-block rooted state; in-window reds and riders inside the pool; tick rate must stay ≤1/slot;
  panel reserve stays unminted; heartbeat-only DAAs mint nothing; existing 34.5 M vesting rows unaffected (vest or one-time rescale).
- Recommendations: (1) if 5,300 were still editable, PALW_EMISSION_BLOCKS_PER_DAA_V1 16 → 1; (2) next fence per-DAA pool + W_claim
  split; (3) price the floor or make it Idle-only + absolute rate controller; (4) decide the 34.5 M vesting; (5) find the 3.02 M base.
Data captured in the original `lanes/evidence/emission-1004/` directory: blocks2.jsonl, vesting_rows.json, classecon.json, supply_samples.jsonl, poc-test.log, scan2.py, supply_sample.py. The raw captures remain in the original local workspace and are not included in this source snapshot. This report preserves the 2026-10-04 observations; it is not a new measurement.
