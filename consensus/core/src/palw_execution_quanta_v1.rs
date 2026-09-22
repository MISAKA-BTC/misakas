//! Execution-round quanta — 1 verified CanonicalWork → N unique spend-once permit tickets.
//!
//! Free-prompt already splits a job into receipt/reward quanta (`palw_freeprompt_v3`). Those are
//! **not** these. An execution quantum is an execution *right*: it converts into at most one
//! algo-10 round permit. A free-prompt quantum is a receipt/weight right. Same CanonicalWork may
//! spawn both; they are distinct objects and cannot settle each other.
//!
//! ```text
//! Verified Final (Attempt work)
//!         │
//!         ▼
//! CanonicalWork credit
//!         │
//!         ▼
//! N unique execution quanta   quantum_id = H(canonical_work_id, final_id, i)
//!         │
//!         ▼
//! future seed assigns each to one later 1-second round
//!         │
//! unused → scheduled → consumed (tombstone; does not revive)
//! ```
//!
//! The global BPS1 cap remains 1 permit / round. A heavy job earns more *candidates* for future
//! rounds, never 20 blocks in the same second. QWEN36 is not special-cased: a larger verified
//! CanonicalWork simply yields more quanta.

use crate::Hash64;
use crate::palw_execution_lane_v1::PalwExecFinalV1;
use crate::palw_state_v2::PalwBondKeyV2;
use std::collections::BTreeSet;

/// Domain of `quantum_id = H(canonical_work_id ‖ final_id ‖ i)`.
pub const PALW_EXEC_QUANTUM_ID_DOMAIN: &[u8] = b"misaka-palw/exec-lane/quantum-id/v1";
/// Domain of the fractional-rounding bit.
pub const PALW_EXEC_QUANTUM_FRAC_DOMAIN: &[u8] = b"misaka-palw/exec-lane/quantum-frac/v1";
/// Domain of a quantum's preferred round.
pub const PALW_EXEC_QUANTUM_ROUND_DOMAIN: &[u8] = b"misaka-palw/exec-lane/quantum-round/v1";
/// Domain of CanonicalWork identity (claim execution, not the model name).
pub const PALW_EXEC_CANONICAL_WORK_ID_DOMAIN: &[u8] = b"misaka-palw/exec-lane/canonical-work-id/v1";

pub const PALW_EXEC_QUANTUM_ALL_DOMAINS: &[&[u8]] =
    &[PALW_EXEC_QUANTUM_ID_DOMAIN, PALW_EXEC_QUANTUM_FRAC_DOMAIN, PALW_EXEC_QUANTUM_ROUND_DOMAIN, PALW_EXEC_CANONICAL_WORK_ID_DOMAIN];

/// One execution quantum of CanonicalWork, in the same units `PalwExecFinalV1::credit` is stored
/// in (exposure pwu, capped). A ~1.6M-pwu QWEN25-scale job is about 16 tickets; a ~2.7M-pwu
/// QWEN36-scale job is about 27. The constant is a protocol unit, not a model name.
pub const PALW_EXECUTION_QUANTUM_V1: u64 = 100_000;

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

fn lead_u64(hash: &Hash64) -> u64 {
    let mut lead = [0u8; 8];
    lead.copy_from_slice(&hash.as_byte_slice()[..8]);
    u64::from_le_bytes(lead)
}

/// Identity of the verified job a Final certified — execution root, not class name, so two
/// packagings of one run collide and copies of one inference do not mint a second ticket set.
pub fn palw_execution_canonical_work_id_v1(execution_root: Hash64) -> Hash64 {
    let mut state = keyed(PALW_EXEC_CANONICAL_WORK_ID_DOMAIN);
    state.update(execution_root.as_byte_slice());
    finish(state)
}

/// `quantum_id_i = H(canonical_work_id ‖ final_id ‖ i)`.
pub fn palw_execution_quantum_id_v1(canonical_work_id: Hash64, final_id: Hash64, index: u32) -> Hash64 {
    let mut state = keyed(PALW_EXEC_QUANTUM_ID_DOMAIN);
    state.update(canonical_work_id.as_byte_slice());
    state.update(final_id.as_byte_slice());
    state.update(&index.to_le_bytes());
    finish(state)
}

/// How many execution quanta `credited_work` mints, with deterministic stochastic rounding of the
/// remainder against the span's future seed. `quantum == 0` mints nothing rather than dividing.
///
/// A 1.3-quantum job is 1 ticket plus a 30% chance of a second; an 18.7-quantum job is 18 plus a
/// 70% chance of a 19th. The chance is a function of `(seed, final_id)`, which did not exist when
/// the job was chosen.
pub fn palw_execution_quantum_count_v1(credited_work: u128, quantum: u128, seed: Hash64, final_id: Hash64) -> u32 {
    if quantum == 0 || credited_work == 0 {
        return 0;
    }
    let whole = credited_work / quantum;
    let frac = credited_work % quantum;
    let extra = if frac == 0 {
        0u128
    } else {
        let mut state = keyed(PALW_EXEC_QUANTUM_FRAC_DOMAIN);
        state.update(seed.as_byte_slice());
        state.update(final_id.as_byte_slice());
        if u128::from(lead_u64(&finish(state))) % quantum < frac { 1 } else { 0 }
    };
    whole.saturating_add(u128::from(extra)).min(u128::from(u32::MAX)) as u32
}

/// One issued execution quantum, assigned to a future round. Distinct from a free-prompt receipt
/// quantum: consuming this ticket produces an algo-10 permit, never weight or a receipt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExecQuantumV1 {
    pub quantum_id: Hash64,
    pub final_id: Hash64,
    pub index: u32,
    pub bond: PalwBondKeyV2,
    pub operator_id: Hash64,
    pub domain: Hash64,
    /// Global 1-second round this quantum may occupy. Unique among siblings of one mint.
    pub scheduled_round: u64,
}

/// Lifecycle of one execution quantum. Retire/reorg/restart restore the *record*, not a spent
/// ticket: consumed is a tombstone on the chain that recorded the spend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwExecQuantumPhaseV1 {
    Unused,
    Scheduled,
    Consumed,
}

/// Mint unique execution quanta from a span's Finals once the ADR-0130 future seed exists.
///
/// * Finals are first collapsed by `execution_root` so 100 copies of one inference are one job.
/// * Each surviving Final yields `palw_execution_quantum_count_v1` tickets.
/// * Tickets are assigned to distinct rounds at or after `open_round` by
///   `H(seed ‖ quantum_id)` with linear probe, so one round never holds two quanta of this mint
///   (the 1 permit / round cap).
pub fn palw_execution_mint_quanta_v1(
    finals: &[PalwExecFinalV1],
    seed: Hash64,
    quantum: u128,
    open_round: u64,
) -> Vec<PalwExecQuantumV1> {
    palw_execution_mint_quanta_matured_v1(finals, seed, quantum, open_round, 0, &std::collections::BTreeSet::new())
}

/// **[`palw_execution_mint_quanta_v1`] under ADR-0151's economic-safety bundle**: the mint that
/// matures its tickets and skips the Finals whose rights are forfeit.
///
/// Two additions, and they are the two halves of "a fraudulent Final's rights are recoverable":
///
/// * **`maturity_rounds`** — no ticket is scheduled before `open_round + maturity_rounds`, so a
///   right minted by a Final cannot be spent until its conviction window has had that long to run.
///   Expressed in ROUNDS rather than DAA because a round is what a ticket occupies; the caller
///   converts, and the conversion errs by at most one span (the Final's own offset inside it), which
///   is 120 rounds against a maturity of 144,000.
/// * **`forfeited_roots`** — a Final whose `execution_root` was convicted mints nothing. Keyed by
///   the ROOT and not the claim, for the same reason the dedup below is: the right belongs to the
///   work, so a second claim over the same execution is the same right and forfeits with it.
///
/// With `maturity_rounds == 0` and an empty forfeiture set this is the pre-bundle mint, ticket for
/// ticket — which is what [`palw_execution_mint_quanta_v1`] is, and why no existing network moves.
pub fn palw_execution_mint_quanta_matured_v1(
    finals: &[PalwExecFinalV1],
    seed: Hash64,
    quantum: u128,
    open_round: u64,
    maturity_rounds: u64,
    forfeited_roots: &std::collections::BTreeSet<Hash64>,
) -> Vec<PalwExecQuantumV1> {
    let open_round = open_round.saturating_add(maturity_rounds);
    let mut by_work: Vec<&PalwExecFinalV1> = finals
        .iter()
        .filter(|f| f.credit > 0)
        .filter(|f| !crate::palw_economic_safety_v1::palw_exec_rights_are_forfeit_v1(forfeited_roots, &f.execution_root))
        .collect();
    by_work.sort_by(|a, b| a.execution_root.cmp(&b.execution_root).then(a.claim_id.cmp(&b.claim_id)));
    by_work.dedup_by(|a, b| a.execution_root == b.execution_root);

    let mut taken: BTreeSet<u64> = BTreeSet::new();
    let mut issued = Vec::new();
    for f in by_work {
        let work_id = palw_execution_canonical_work_id_v1(f.execution_root);
        let n = palw_execution_quantum_count_v1(u128::from(f.credit), quantum, seed, f.claim_id);
        for index in 0..n {
            let quantum_id = palw_execution_quantum_id_v1(work_id, f.claim_id, index);
            let scheduled_round = assign_round(seed, quantum_id, open_round, &mut taken);
            issued.push(PalwExecQuantumV1 {
                quantum_id,
                final_id: f.claim_id,
                index,
                bond: f.bond,
                operator_id: f.operator_id,
                domain: f.domain,
                scheduled_round,
            });
        }
    }
    issued.sort_by_key(|q| (q.scheduled_round, q.quantum_id));
    issued
}

fn assign_round(seed: Hash64, quantum_id: Hash64, open_round: u64, taken: &mut BTreeSet<u64>) -> u64 {
    let mut state = keyed(PALW_EXEC_QUANTUM_ROUND_DOMAIN);
    state.update(seed.as_byte_slice());
    state.update(quantum_id.as_byte_slice());
    // Probe a 2^16-round (~18 hour) horizon so a busy mint still finds a free second without
    // wrapping back onto an earlier occupied round of this span.
    let horizon = 1u64 << 16;
    let prefer = open_round.saturating_add(lead_u64(&finish(state)) % horizon);
    for step in 0..horizon {
        let round = prefer.saturating_add(step);
        if taken.insert(round) {
            return round;
        }
    }
    let mut round = open_round.saturating_add(horizon);
    while !taken.insert(round) {
        round = round.saturating_add(1);
    }
    round
}

/// The (at most one) quantum this round may convert into a permit. Width stays 1: a second
/// quantum of the same round does not exist because minting probed away from collisions.
pub fn palw_execution_quantum_for_round_v1(issued: &[PalwExecQuantumV1], round: u64) -> Option<&PalwExecQuantumV1> {
    issued.iter().find(|q| q.scheduled_round == round)
}

/// Whether `quantum_id` is one of the issued tickets. A consumed tombstone is the caller's
/// ledger (`round_permit_used` / a per-quantum spent set); this only names the ticket.
pub fn palw_execution_quantum_issued_v1(issued: &[PalwExecQuantumV1], quantum_id: Hash64) -> bool {
    issued.iter().any(|q| q.quantum_id == quantum_id)
}

/// Lifecycle of one issued quantum against the rounds the chain has already accepted.
///
/// * **Unused** — the id is not in this mint (still sitting in `round_finals`, or never earned).
/// * **Scheduled** — minted onto a unique future round, permit not yet accepted.
/// * **Consumed** — that round's permit bit is set; the ticket does not revive.
pub fn palw_execution_quantum_phase_v1(
    issued: &[PalwExecQuantumV1],
    consumed_rounds: &BTreeSet<u64>,
    quantum_id: Hash64,
) -> PalwExecQuantumPhaseV1 {
    match issued.iter().find(|q| q.quantum_id == quantum_id) {
        None => PalwExecQuantumPhaseV1::Unused,
        Some(q) if consumed_rounds.contains(&q.scheduled_round) => PalwExecQuantumPhaseV1::Consumed,
        Some(_) => PalwExecQuantumPhaseV1::Scheduled,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_state_v2::PalwBondKeyV2;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bond(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
    }

    fn final_of(claim: u64, root: u64, credit: u64, domain: u64) -> PalwExecFinalV1 {
        PalwExecFinalV1 {
            domain: h(domain),
            bond: bond(domain),
            operator_id: h(domain + 50),
            claim_id: h(claim),
            execution_root: h(root),
            credit,
        }
    }

    #[test]
    fn the_quantum_domains_are_distinct() {
        let mut seen = BTreeSet::new();
        for d in PALW_EXEC_QUANTUM_ALL_DOMAINS {
            assert!(seen.insert(*d), "duplicate domain {d:?}");
        }
    }

    #[test]
    fn same_work_same_seed_same_final_same_count() {
        let seed = h(7);
        let id = h(11);
        let a = palw_execution_quantum_count_v1(1_300, 1_000, seed, id);
        let b = palw_execution_quantum_count_v1(1_300, 1_000, seed, id);
        assert_eq!(a, b);
        assert!(a == 1 || a == 2, "1.3 quanta is 1 or 2, got {a}");
    }

    #[test]
    fn the_fraction_is_deterministic_and_uses_the_future_seed() {
        let id = h(11);
        let with = palw_execution_quantum_count_v1(1_300, 1_000, h(1), id);
        let without = palw_execution_quantum_count_v1(1_000, 1_000, h(1), id);
        assert_eq!(without, 1, "an exact multiple does not roll");
        assert!(with == 1 || with == 2);
        // Different seeds can flip the extra bit; the function remains a pure map.
        let mut seen = BTreeSet::new();
        for s in 0..64u64 {
            seen.insert(palw_execution_quantum_count_v1(1_300, 1_000, h(s), id));
        }
        assert!(seen.contains(&1) && seen.contains(&2), "a 30% remainder is not stuck on one side of the coin, got {seen:?}");
    }

    #[test]
    fn a_heavier_canonical_job_mints_more_quanta_than_a_lighter_one() {
        let seed = h(99);
        let light = palw_execution_quantum_count_v1(1_300, 1_000, seed, h(1));
        let heavy = palw_execution_quantum_count_v1(18_700, 1_000, seed, h(2));
        assert!(heavy > light, "18.7q ({heavy}) must out-mint 1.3q ({light})");
        assert!(heavy == 18 || heavy == 19);
        assert!(light == 1 || light == 2);
    }

    #[test]
    fn representation_does_not_enter_the_count_the_scalar_does() {
        // Two packagings of one job that already hashed to one CanonicalWork scalar.
        let seed = h(3);
        let id = h(4);
        assert_eq!(palw_execution_quantum_count_v1(18_700, 1_000, seed, id), palw_execution_quantum_count_v1(18_700, 1_000, seed, id));
    }

    #[test]
    fn copies_of_one_execution_do_not_multiply_quanta() {
        let seed = h(5);
        let one = palw_execution_mint_quanta_v1(&[final_of(1, 0xE0, 10_000, 1)], seed, 1_000, 100);
        let copies = palw_execution_mint_quanta_v1(
            &[final_of(1, 0xE0, 10_000, 1), final_of(2, 0xE0, 10_000, 1), final_of(3, 0xE0, 10_000, 1)],
            seed,
            1_000,
            100,
        );
        assert_eq!(one.len(), copies.len(), "100 copies of one execution_root are one CanonicalWork");
        assert_eq!(one.iter().map(|q| q.quantum_id).collect::<Vec<_>>(), copies.iter().map(|q| q.quantum_id).collect::<Vec<_>>());
    }

    #[test]
    fn genuine_separate_jobs_do_mint_separately() {
        let seed = h(5);
        let two = palw_execution_mint_quanta_v1(&[final_of(1, 0xE0, 10_000, 1), final_of(2, 0xE1, 10_000, 1)], seed, 1_000, 100);
        let one = palw_execution_mint_quanta_v1(&[final_of(1, 0xE0, 10_000, 1)], seed, 1_000, 100);
        assert!(two.len() > one.len(), "two distinct execution roots are two jobs");
    }

    #[test]
    fn each_quantum_has_a_unique_id_and_a_unique_round() {
        let issued = palw_execution_mint_quanta_v1(&[final_of(1, 0xE0, 18_700, 36), final_of(2, 0xE1, 1_300, 25)], h(8), 1_000, 1_000);
        let mut ids = BTreeSet::new();
        let mut rounds = BTreeSet::new();
        for q in &issued {
            assert!(ids.insert(q.quantum_id), "quantum ids collide");
            assert!(rounds.insert(q.scheduled_round), "two quanta share round {}", q.scheduled_round);
            assert!(q.scheduled_round >= 1_000);
        }
        assert_eq!(ids.len(), issued.len());
        assert!(issued.len() >= 19, "18.7 + 1.3 is at least 19 tickets, got {}", issued.len());
    }

    #[test]
    fn n_quanta_are_consumable_at_most_n_times() {
        let issued = palw_execution_mint_quanta_v1(&[final_of(1, 0xE0, 5_000, 1)], h(1), 1_000, 0);
        assert!(!issued.is_empty());
        let mut spent = BTreeSet::new();
        let mut consumed_rounds = BTreeSet::new();
        for q in &issued {
            assert_eq!(palw_execution_quantum_phase_v1(&issued, &consumed_rounds, q.quantum_id), PalwExecQuantumPhaseV1::Scheduled);
            assert!(spent.insert(q.quantum_id), "a quantum spent twice");
            consumed_rounds.insert(q.scheduled_round);
            assert_eq!(palw_execution_quantum_phase_v1(&issued, &consumed_rounds, q.quantum_id), PalwExecQuantumPhaseV1::Consumed);
        }
        assert!(!spent.insert(issued[0].quantum_id), "the tombstone holds: the first ticket cannot be spent again");
        assert_eq!(spent.len(), issued.len());
        assert_eq!(
            palw_execution_quantum_phase_v1(&issued, &consumed_rounds, h(0xDEAD)),
            PalwExecQuantumPhaseV1::Unused,
            "an id that was never minted stays unused"
        );
    }

    #[test]
    fn one_round_yields_at_most_one_permit_candidate() {
        let issued = palw_execution_mint_quanta_v1(&[final_of(1, 0xE0, 20_000, 1)], h(2), 1_000, 50);
        for q in &issued {
            let hits = issued.iter().filter(|o| o.scheduled_round == q.scheduled_round).count();
            assert_eq!(hits, 1);
            assert_eq!(palw_execution_quantum_for_round_v1(&issued, q.scheduled_round).map(|h| h.quantum_id), Some(q.quantum_id));
        }
        assert!(palw_execution_quantum_for_round_v1(&issued, 0).is_none());
    }

    #[test]
    fn a_qwen36_scale_job_out_mints_a_qwen25_scale_job_because_of_work_not_the_name() {
        // The numbers are CanonicalWork scalars, not class ids. Renaming the rows does not
        // change the mint; swapping the work does.
        let seed = h(36);
        let qwen25_work = 1_589_424u64;
        let qwen36_work = 2_685_360u64;
        let unit = 1_589_424u128;
        let light = palw_execution_mint_quanta_v1(&[final_of(25, 0xA, qwen25_work, 25)], seed, unit, 0);
        let heavy = palw_execution_mint_quanta_v1(&[final_of(36, 0xB, qwen36_work, 36)], seed, unit, 0);
        assert!(heavy.len() > light.len(), "heavier CanonicalWork ({}) must out-mint lighter ({})", heavy.len(), light.len());
        let swapped = palw_execution_mint_quanta_v1(&[final_of(25, 0xB, qwen36_work, 25)], seed, unit, 0);
        assert_eq!(swapped.len(), heavy.len(), "the class name is not an input; the work scalar is");
    }

    #[test]
    fn zero_work_or_zero_unit_mints_nothing() {
        assert!(palw_execution_mint_quanta_v1(&[final_of(1, 1, 0, 1)], h(1), 1_000, 0).is_empty());
        assert!(palw_execution_mint_quanta_v1(&[final_of(1, 1, 9, 1)], h(1), 0, 0).is_empty());
        assert_eq!(palw_execution_quantum_count_v1(0, 1_000, h(1), h(1)), 0);
    }
}
