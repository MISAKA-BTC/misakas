#!/usr/bin/env python3
"""PALW economic parameter simulation — the B15 gate for ADR-0028 §4 / ADR-0032.

Every number ADR-0028 and ADR-0032 marked "economic-simulation-gated" is derived here from
measured facts, with the inequality each one has to satisfy stated next to it. Nothing is
tuned to taste: each parameter is the tightest value satisfying its constraint, and where a
constraint cannot be satisfied the script says so instead of picking a number.

Measured inputs (all cited, none invented):
  * per-block subsidy, 120 s network — `SUBSIDY_BY_MONTH_TABLE[0]` rescaled by the
    rate-preserving rule (docs: 4445.62 MSK/block at 120 s; genesis-rate cross-check below)
  * replay costs — docs/palw-stage0-fleet-replay-bench-2026-08-16.md (D=512 p99 per host)
  * bond size — 20 000 MSK, live on t10 (t10-bond-registered-2026-08-15)
  * windows — ADR-0028 §3 two-minute defaults (w_replay 30 blocks = 1 h, W_challenge 720 = 24 h)
  * mass/fee floor — mass_per_tx_byte = 1, standard cap 480 000; carriage sizes from ADR-0029 §3

Run: python3 scripts/misaka-palw-economics-sim.py
"""

from dataclasses import dataclass

# --- measured constants ------------------------------------------------------------------

SOMPI = 100_000_000                     # sompi per MSK
GENESIS_SUBSIDY_10BPS_SOMPI = 370_468_345   # params.rs: year-1 per-block at 10 BPS
BLOCKS_PER_SEC_10BPS = 10
SECONDS_PER_BLOCK_120 = 120

# Rate-preserving rescale to the 120 s network: same MSK/second, 1200x fewer blocks.
BASE_SUBSIDY_120_SOMPI = GENESIS_SUBSIDY_10BPS_SOMPI * BLOCKS_PER_SEC_10BPS * SECONDS_PER_BLOCK_120
BASE_SUBSIDY_120_MSK = BASE_SUBSIDY_120_SOMPI / SOMPI

BOND_MSK = 20_000                       # live bonded amount per validator
Q = 2                                   # funded panel size (ADR-0028 §4, Stage 1-2)
RHO_V = 1.0                             # measured replay/primary cost ratio (≈1.0, registered)
LAMBDA = 2.0                            # §4e economic safety factor (λ ≥ 2.0)

W_CHALLENGE_BLOCKS = 720                # 24 h at 120 s
W_REPLAY_BLOCKS = 30                    # 1 h
UNBONDING_BLOCKS = 10_083               # testnet DnsParams (≈14 days wall-clock preserved)

# Replay cost, slowest fleet host at the 512-decode ceiling (ms), and the fleet's capacity.
P99_REPLAY_MS_SLOWEST = 90_716
FLEET_HOSTS = 4

# Carriage sizes (ADR-0029 §3 mass arithmetic; mass == bytes at mass_per_tx_byte = 1).
MASS_OPENING_CALL = 3_300
MASS_OPENING_ANSWER = 152_000
MASS_REFUTATION_LEGS = 15_000
MASS_COMMITMENT_COMPOSITE = 9_100

# The chain's own fee floor: the minimum relay fee rate (sompi per gram of mass).
MIN_RELAY_FEE_PER_GRAM = 1_000          # kaspa default 1000 sompi/gram


@dataclass
class Result:
    name: str
    value: str
    constraint: str
    verdict: str


def msk(sompi: float) -> str:
    return f"{sompi / SOMPI:,.4f} MSK"


results: list[Result] = []


def main() -> None:
    print("PALW economic parameters — derived, not chosen\n" + "=" * 78)
    print(f"base(C) per credited block  : {BASE_SUBSIDY_120_MSK:,.2f} MSK  "
          f"({BASE_SUBSIDY_120_SOMPI:,} sompi, rate-preserved from the 10 BPS genesis rate)")
    print(f"cross-check vs the docs' 4 445.62 MSK/blk: "
          f"{'MATCH' if abs(BASE_SUBSIDY_120_MSK - 4445.62) < 0.5 else 'MISMATCH — investigate'}")
    print()

    # --- 1. The issuance split (ADR-0028 §4a) --------------------------------------------
    issuance = (1 + Q * RHO_V) * BASE_SUBSIDY_120_SOMPI
    attester_fee = RHO_V * BASE_SUBSIDY_120_SOMPI
    print("1. Issuance split  issuance(C) = (1 + q·ρ_v)·base(C)")
    print(f"   q = {Q}, ρ_v = {RHO_V}")
    print(f"   total issuance        : {msk(issuance)}   (= {1 + Q * RHO_V:.1f}× base — the verification tax, visible)")
    print(f"   miner                 : {msk(BASE_SUBSIDY_120_SOMPI)}")
    print(f"   each on-time attester : {msk(attester_fee)}  (× {Q})")
    results.append(Result("attester fee ρ_v·base", msk(attester_fee),
                          "= ρ_v · base(C), ρ_v measured per class", "derived"))

    # --- 2. Verifier's dilemma (ADR-0028 §4b) --------------------------------------------
    # Replaying is dominant when S_a · P(fraud) · P_refute > c_replay.
    # c_replay: one p99 replay of CPU. Price it generously at a cloud rate.
    cpu_hour_usd = 0.10                 # a shared EPYC vCPU-hour, generous
    msk_usd = 0.001                     # a deliberately tiny assumed price; the ratio is what matters
    c_replay_usd = (P99_REPLAY_MS_SLOWEST / 3_600_000) * cpu_hour_usd
    c_replay_msk = c_replay_usd / msk_usd
    print("\n2. Verifier's dilemma  replay dominant iff  S_a · P(fraud) · P_refute > c_replay")
    print(f"   c_replay (p99 {P99_REPLAY_MS_SLOWEST/1000:.1f}s at ${cpu_hour_usd}/cpu-h) = {c_replay_msk:,.6f} MSK-equivalent")
    print(f"   S_a (bond at risk)    = {BOND_MSK:,} MSK")
    breakeven = c_replay_msk / BOND_MSK
    print(f"   ⇒ replaying is dominant while P(fraud)·P_refute > {breakeven:.3e}")
    print(f"   i.e. slack by ~{1/breakeven:,.0f}× against even a 1-in-1 fraud/refute product")
    results.append(Result("verifier-dilemma slack", f"{1/breakeven:,.0f}x",
                          "S_a·P(fraud)·P_refute > c_replay", "slack by orders of magnitude"))

    # --- 3. F_call, the opening-call fee (ADR-0032 E1) -----------------------------------
    # Constraint: a call must cost the caller MORE than answering costs the answerer, or
    # calls are a griefing lever. Answer cost = one replay + the answer's own mass.
    answer_mass_fee = MASS_OPENING_ANSWER * MIN_RELAY_FEE_PER_GRAM
    call_mass_fee = MASS_OPENING_CALL * MIN_RELAY_FEE_PER_GRAM
    answerer_cost = answer_mass_fee + c_replay_msk * SOMPI
    print("\n3. F_call (opening-call fee)   constraint: F_call ≥ answerer's total cost")
    print(f"   answerer pays: answer-tx mass fee {msk(answer_mass_fee)} + replay {c_replay_msk:,.6f} MSK")
    print(f"                = {msk(answerer_cost)}")
    print(f"   caller's own mass fee at the floor: {msk(call_mass_fee)} — NOT enough by itself")
    f_call = round_up_nice(answerer_cost)
    print(f"   ⇒ F_call = {msk(f_call)}  (mandatory minimum tx fee on an opening-call carriage)")
    print(f"     = {f_call / answerer_cost:.2f}× the answerer's cost")
    results.append(Result("F_call", msk(f_call), "≥ answerer mass fee + replay cost", "derived"))

    # --- 4. No-show slash floor (ADR-0028 §4c) -------------------------------------------
    # Constraint: griefing must have negative ROI. The griefer's gain from stranding a job
    # is at most the miner's re-mine cost (one block's work); the pair's loss is 2 slashes.
    forgone_fee = attester_fee
    noshow_floor = 100 * RHO_V * BASE_SUBSIDY_120_SOMPI     # the ADR's placeholder form
    miner_loss_from_strand = BASE_SUBSIDY_120_SOMPI          # one orphan-equivalent
    print("\n4. No-show slash floor   constraint: 2·slash > griefer's gain (one orphan-equivalent)")
    print(f"   forgone fee per no-show : {msk(forgone_fee)}")
    print(f"   floor at 100·ρ_v·base   : {msk(noshow_floor)}")
    print(f"   miner's loss if stranded: {msk(miner_loss_from_strand)}")
    print(f"   ⇒ griefing ROI = {miner_loss_from_strand / (2 * noshow_floor):.4f} (must be < 1)  "
          f"{'OK' if miner_loss_from_strand < 2 * noshow_floor else 'FAIL'}")
    # Also: the floor must not exceed the bond (a slash you cannot collect is theatre).
    print(f"   floor vs bond: {noshow_floor/SOMPI:,.0f} MSK vs {BOND_MSK:,} MSK — "
          f"{'collectible' if noshow_floor/SOMPI <= BOND_MSK else 'EXCEEDS BOND — cap at bond'}")
    results.append(Result("no-show floor", msk(noshow_floor),
                          "2·floor > orphan-equivalent AND ≤ bond", "OK" if noshow_floor/SOMPI <= BOND_MSK else "CAP AT BOND"))

    # --- 5. B_cap, the challenger bounty (ADR-0027 §4 / ADR-0032) ------------------------
    # Constraint: ≤10% of slash, and small enough that manufacturing offenses is unprofitable
    # (a self-slash costs 100% to gain ≤10%).
    bounty_pct = 0.10
    max_slash = BOND_MSK * SOMPI
    b_cap = bounty_pct * max_slash
    print("\n5. Challenger bounty   constraint: ≤ 10% of slash; self-slash must be unprofitable")
    print(f"   B_cap = {msk(b_cap)} (10% of a full {BOND_MSK:,} MSK bond)")
    print(f"   self-slash ROI = {bounty_pct:.2f} (must be < 1) OK")
    print(f"   remainder burned: {msk(max_slash - b_cap)}")
    results.append(Result("B_cap", msk(b_cap), "≤ 10% slash; self-slash ROI < 1", "derived"))

    # --- 6. §4e admission caps -----------------------------------------------------------
    print("\n6. Admission caps (both registration-time)")
    # Physical: R_jobs · q ≤ Σ capacity. Capacity per host = slots per W_replay window.
    w_replay_ms = W_REPLAY_BLOCKS * SECONDS_PER_BLOCK_120 * 1000
    slots_per_host = w_replay_ms // P99_REPLAY_MS_SLOWEST
    fleet_capacity = slots_per_host * FLEET_HOSTS
    max_credited_jobs_per_window = fleet_capacity // Q
    blocks_per_window = W_REPLAY_BLOCKS
    credit_every_n_blocks = blocks_per_window / max_credited_jobs_per_window if max_credited_jobs_per_window else float("inf")
    print(f"   physical: {slots_per_host} replay slots/host/window × {FLEET_HOSTS} hosts = {fleet_capacity} slots")
    print(f"             ⇒ ≤ {max_credited_jobs_per_window} credited jobs per {W_REPLAY_BLOCKS}-block window at q={Q}")
    print(f"             ⇒ credit at most 1 job every {credit_every_n_blocks:.1f} blocks "
          f"({credit_every_n_blocks * SECONDS_PER_BLOCK_120 / 60:.1f} min) on today's 4-host fleet")
    results.append(Result("max credit rate (4-host)", f"1 job / {credit_every_n_blocks:.1f} blocks",
                          "R_jobs·q ≤ Σ capacity(p99)", "derived"))

    # Economic: P_check · S_eff ≥ λ · G_max, with max_leverage ≤ 1.
    #
    # FINDING (2026-08-16): ADR-0028 §4e admits two readings of G_max, and they differ by four
    # orders of magnitude at the live parameters. Both are computed here; the strict one governs
    # because it is the one the `max_leverage ≤ 1` sentence states.
    s_eff = BOND_MSK * SOMPI
    print(f"   economic: S_eff = {msk(s_eff)} (one bond reachable by refutation)")

    #   Reading A (per-commitment): "G_max = credit mintable from THE dishonest commitment".
    g_max_single = BASE_SUBSIDY_120_SOMPI
    need_single = LAMBDA * g_max_single
    print(f"   [A] per-commitment G_max = {msk(g_max_single)} ⇒ need S_eff ≥ {msk(need_single)} at P_check=1.0")
    print(f"       {msk(s_eff)} available ⇒ {'OK' if s_eff >= need_single else 'VIOLATED'}"
          f" (margin {s_eff / need_single:.2f}×)")

    #   Reading B (aggregate): "credit mintable WITHIN ONE UNBONDING PERIOD must not exceed
    #   S_eff" — a repeat offender mints repeatedly against the same bond.
    jobs_per_unbonding = UNBONDING_BLOCKS / credit_every_n_blocks
    g_max_aggregate = jobs_per_unbonding * BASE_SUBSIDY_120_SOMPI
    print(f"   [B] aggregate G_max over one unbonding period ({UNBONDING_BLOCKS} blocks, crediting")
    print(f"       every physically-allowed slot: {jobs_per_unbonding:,.0f} jobs) = {msk(g_max_aggregate)}")
    for p_check in (1.0, 0.5, 0.2):
        need = LAMBDA * g_max_aggregate / p_check
        print(f"       P_check={p_check:.1f} ⇒ need S_eff ≥ {msk(need)}  "
              f"{'OK' if s_eff >= need else f'VIOLATED by {need/s_eff:,.0f}x'}")

    # The actionable resolution: a per-validator credited-job rate cap, registration-time.
    max_credit_per_unbonding = s_eff / LAMBDA
    max_jobs_per_bond = max_credit_per_unbonding / BASE_SUBSIDY_120_SOMPI
    blocks_between = UNBONDING_BLOCKS / max_jobs_per_bond
    print(f"\n   ⇒ RESOLUTION (reading B governs — it is what max_leverage ≤ 1 says):")
    print(f"     a per-validator credited-job cap of {max_jobs_per_bond:.1f} jobs per unbonding period")
    print(f"     (1 job per {blocks_between:,.0f} blocks ≈ {blocks_between*SECONDS_PER_BLOCK_120/86400:.1f} days per validator)")
    print(f"     at the FULL block subsidy as base(C). Alternatives, equivalent under the same")
    print(f"     inequality — pick at registration, not at runtime:")
    target_rate_blocks = 10  # a plausible operational target: one credited job every 10 blocks
    implied_base = max_credit_per_unbonding / (UNBONDING_BLOCKS / target_rate_blocks)
    print(f"       (i)  keep base(C) = the block subsidy and cap the rate (above), or")
    print(f"       (ii) credit every {target_rate_blocks} blocks with base(C) ≤ {msk(implied_base)}")
    print(f"            — i.e. PALW credit is {implied_base/BASE_SUBSIDY_120_SOMPI*100:.4f}% of the subsidy, not all of it, or")
    implied_bond = LAMBDA * g_max_aggregate / SOMPI
    print(f"       (iii) raise the bond to {implied_bond:,.0f} MSK per validator (not credible), or")
    print(f"       (iv) shorten the unbonding period — bounded below by the dispute window")
    print(f"            (W_challenge {W_CHALLENGE_BLOCKS} blocks), so at best a {UNBONDING_BLOCKS/W_CHALLENGE_BLOCKS:.0f}× reduction.")
    results.append(Result("per-validator credit cap", f"{max_jobs_per_bond:.1f} jobs / unbonding",
                          "P_check·S_eff ≥ λ·G_max (aggregate)", "REQUIRED — see finding"))

    # --- 7. Summary ----------------------------------------------------------------------
    print("\n" + "=" * 78)
    print("REGISTRATION VALUES (Stage-1 candidates — every one derived above, none chosen)")
    print("=" * 78)
    for r in results:
        print(f"  {r.name:32s} {r.value:>24s}   [{r.constraint}]")
    print("\nNote: ρ_v, P_check and the per-class p99 are MEASURED inputs. This script is the")
    print("derivation, not the measurement — re-run it whenever a measured input moves.")


def round_up_nice(sompi: float) -> float:
    """Round up to a clean 0.01 MSK so a fee minimum is human-checkable."""
    step = SOMPI // 100
    return float(((int(sompi) + step - 1) // step) * step)


if __name__ == "__main__":
    main()
