//! **RFC-0007 Part II (consensus half), Part IV.1 and Part IV.2 — the witness manifest, the global audit mesh and staged
//! onboarding** (`Params::palw_witness_manifest_v1`, `palw_audit_mesh_v1`, `palw_capped_onboarding_v1`; each dormant on every
//! shipped preset). Spec `docs/spec/palw/18-verification-certificates.md` is normative; the decisions of 2026-10-03 on RFC-0007's
//! open questions 5 to 10 are recorded in its Decision section.
//!
//! # The witness manifest (Part II)
//!
//! The algebraic checker is seat-local node software; the chain's part is the **producer's duty to serve the witness** and a way
//! for a seat to say *which chunk* went unserved. A class registered at or past `palw_witness_manifest_v1` records a **witness
//! profile** ([`PalwWitnessProfileRowV1`]) read off its program by the canonical serving set (question 10, settled: pinned in the
//! class profile — [`misaka_palw_tir::dataflow::canonical_witness_elements_per_position_v1`] is the one place the set is
//! read): every weight product of contraction at least 64 and every `P·V` from a history of 1,024, at 8 bytes an element, for the
//! class's `max_context` positions, in chunks of [`PALW_WITNESS_CHUNK_BYTES_V1`]. An attempt of that class commits
//! `1 + chunks` trace chunks under [`palw_attempt_trace_manifest_root_v2`]; chunk 0 is the trace, chunks `1..` the witness, so the
//! `Unavailable` verdict's `chunk_index` (already checked against the claim's `trace_chunk_count`) names a witness chunk, and the
//! rule "a producer that withholds gets no licence, the claim redraws and then voids, nobody is slashed for silence" (ADR-0065
//! Decision 4) applies to the witness unchanged. The witness bytes enter the class's **verification-window derivation**
//! ([`palw_witness_ccu_v1`] added to the window's compute term) and **not** the seat's pay (question 5, settled): pay reads the
//! registry's `verification_ccu`, which the profile never touches. A sketching seat's receipt earns exactly what a replay's earns
//! (question 6, settled): the verdict is the same verdict, so no rule here reads *how* a seat checked.
//!
//! # The audit mesh (Part IV.1)
//!
//! A claim of an IR class, at acceptance, draws [`PALW_AUDITS_PER_CLAIM_V1`] auditors from **all bonded seats** by stake weight
//! ([`palw_mesh_audit_draw_v1`]), after the claim has committed. Each draw names a seat and a leaf *ticket* (the auditor maps it onto
//! the claim's committed tiles; the chain does not know the tile count). The auditor reports through a Part I vertex as an
//! `Audited { claim, leaf, result }` leaf — `result` 0 a match, 1 a mismatch; **silence is never a match**. An auditor reserves
//! `trap_penalty` (10 times the audit pay) on its bond while the audit and the trap reveal window stand, and is paid
//! [`PALW_AUDIT_PAY_PERMILLE_V1`] ‰ of the claim's escrowed reward per attested audit out of the panel reserve.
//!
//! **Traps** (the verifier's dilemma): a bonded setter drawn by the slot lottery at [`PALW_TRAP_RATE_BP_V1`] carries
//! `TrapCommitted { H(claim ‖ fault leaf ‖ tiles ‖ salt) }` (tag 93), produces a claim of its own with a fault planted in one of
//! `tiles` committed tiles, and after the audit window carries `TrapRevealed` (tag 94). The fold checks the commitment; an
//! auditor whose draw landed on the planted tile (`ticket % tiles == fault leaf`) and who attested a match is slashed the penalty, one
//! who attested the mismatch earns the bounty, the trap claim is voided **without slashing its setter**, and a setter that never
//! reveals forfeits its deposit.
//!
//! # Staged onboarding (Part IV.2)
//!
//! `PalwModelLifecycleV1::Capped { since_daa }` is appended last in the enum. A class in `Prefetching` that cannot fill a panel
//! (`ready_seats < seat_count`) but whose probe claims' captures a **DA certificate** vouches for (`q` seats' equal `Held` leaves,
//! §I.7) steps to `Capped`; it admits [`PALW_CAPPED_ADMISSION_PERMILLE_V1`] ‰ of its derived admission, never counts toward
//! `panel_drawable`, and all capped classes together hold at most [`PALW_CAPPED_WEIGHT_CAP_PERMILLE_V1`] ‰ of the share table
//! (`w_cap` = 1 %). A capped class's claims are bound by the mesh (the audit draw is the claim's panel), license when every drawn
//! auditor attested a match, and **their reward stays unvested** until the class's holders re-verify: when the class leaves
//! `Capped` the re-verification window opens, **every** capped claim of the class needs [`PALW_CAPPED_REVERIFY_QUORUM_V1`] holder
//! `Valid` leaves inside it, and a claim without them at the window's end is voided and its reward forfeited (a claim a holder
//! convicts in the exact court is slashed as any claim is). Already-accepted blocks are not undone; the cap bounds by how much.
//!
//! Everything here is dormant: the state tables are Some-only and empty until a fence's first write.

use crate::Hash64;
use crate::config::params::{ForkActivation, PalwPostLaunchFenceV1, Params};
use crate::palw_mode_v2::{PalwConsensusMode, PalwModeV2Error};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2};

// ---------------------------------------------------------------------------------------------
// Domains, contexts and constants
// ---------------------------------------------------------------------------------------------

/// Keyed-BLAKE2b-512 domain of a trap's commitment.
pub const PALW_MESH_TRAP_COMMITMENT_DOMAIN_V1: &[u8] = b"misaka-palw/mesh/trap-commitment/v1";
/// Keyed-BLAKE2b-512 domain of the message a setter signs over `TrapCommitted`.
pub const PALW_MESH_TRAP_COMMITTED_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/mesh/trap-committed-message/v1";
/// Keyed-BLAKE2b-512 domain of the message a setter signs over `TrapRevealed`.
pub const PALW_MESH_TRAP_REVEALED_MESSAGE_DOMAIN_V1: &[u8] = b"misaka-palw/mesh/trap-revealed-message/v1";
/// Keyed-BLAKE2b-512 domain of the audit draw's seed and tickets.
pub const PALW_MESH_AUDIT_DRAW_DOMAIN_V1: &[u8] = b"misaka-palw/mesh/audit-draw/v1";
/// Keyed-BLAKE2b-512 domain of the trap slot lottery.
pub const PALW_MESH_TRAP_SLOT_DOMAIN_V1: &[u8] = b"misaka-palw/mesh/trap-slot/v1";
/// Keyed-BLAKE2b-512 domain of the v2 trace manifest root (the witness chunks included).
pub const PALW_MESH_TRACE_MANIFEST_V2_DOMAIN: &[u8] = b"misaka-palw/mesh/trace-manifest/v2";
/// ML-DSA-87 signing context of `TrapCommitted`.
pub const PALW_MESH_TRAP_COMMITTED_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/mesh/trap-committed/mldsa87/v1";
/// ML-DSA-87 signing context of `TrapRevealed`.
pub const PALW_MESH_TRAP_REVEALED_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/mesh/trap-revealed/mldsa87/v1";

/// Every keyed-hash domain this module owns (the domain-uniqueness tests read it).
pub const PALW_MESH_V1_ALL_DOMAINS: &[&[u8]] = &[
    PALW_MESH_TRAP_COMMITMENT_DOMAIN_V1,
    PALW_MESH_TRAP_COMMITTED_MESSAGE_DOMAIN_V1,
    PALW_MESH_TRAP_REVEALED_MESSAGE_DOMAIN_V1,
    PALW_MESH_AUDIT_DRAW_DOMAIN_V1,
    PALW_MESH_TRAP_SLOT_DOMAIN_V1,
    PALW_MESH_TRACE_MANIFEST_V2_DOMAIN,
];

/// One witness chunk, in bytes.
pub const PALW_WITNESS_CHUNK_BYTES_V1: u64 = 1 << 20;
/// The most witness chunks a class's profile may carry (a larger witness is cut into larger chunks, never into more).
pub const PALW_WITNESS_MAX_CHUNKS_V1: u32 = 4_096;
/// The width a served `MatMul` element is priced at: its `i64` accumulator.
pub const PALW_WITNESS_BYTES_PER_ELEMENT_V1: u64 = 8;
/// **Witness bytes in the verification window** (question 5): a 100 Mbit/s link moves 12,500 bytes a millisecond and the
/// registry's reference replays 4,000,000 MAC-eq a millisecond, so a served byte costs 320 MAC-eq of window.
pub const PALW_WITNESS_CCU_PER_BYTE_V1: u128 = 320;

/// **Audits drawn per claim** (question 7, settled 2026-10-03).
pub const PALW_AUDITS_PER_CLAIM_V1: usize = 2;
/// **The trap rate**, in basis points of setter slots (question 7: 1 %).
pub const PALW_TRAP_RATE_BP_V1: u64 = 100;
/// The DAA a trap slot lasts: a bond is drawn (or not) once per slot.
pub const PALW_TRAP_SLOT_DAA_V1: u64 = 100;
/// **Audit pay**: this many ‰ of the claim's escrowed reward, per attested audit, out of the panel reserve.
pub const PALW_AUDIT_PAY_PERMILLE_V1: u64 = 4;
/// **The trap penalty** (question 7: a reservation equal to 10 times the audit pay).
pub const PALW_TRAP_PENALTY_MULTIPLE_V1: u128 = 10;
/// The bounty an auditor who attested the mismatch on a planted tile earns, in audit pays.
pub const PALW_TRAP_BOUNTY_MULTIPLE_V1: u64 = 5;
/// An audit stays open this many DAA after its draw; an `Audited` leaf outside it counts for nothing.
pub const PALW_AUDIT_WINDOW_DAA_V1: u64 = 240;
/// A trap may be revealed this many DAA after its audit window closes; an auditor's reservation lives as long.
pub const PALW_TRAP_REVEAL_WINDOW_DAA_V1: u64 = 240;
/// A planted fault names a tile among at most this many (the commitment binds the count).
pub const PALW_TRAP_MAX_TILES_V1: u64 = 64;
/// What a trap setter locks while its trap is open, in sompi (100 MSK); forfeited if it never reveals.
pub const PALW_TRAP_DEPOSIT_SOMPI_V1: u128 = 10_000_000_000;
/// The most traps that may be open at once (a bound on the table, not a rate).
pub const PALW_TRAP_MAX_OPEN_V1: usize = 1_024;
/// The most audit rows that may be open at once. A claim accepted while the table is full is not audited: the mesh is a sensor.
pub const PALW_AUDIT_MAX_OPEN_V1: usize = 20_000;
/// The rows the mesh sweep retires a block.
pub const PALW_MESH_SWEEP_PER_BLOCK_V1: usize = 256;

/// **`w_cap`** (question 8, settled): all capped classes together hold at most this many ‰ of the share table (1 %).
pub const PALW_CAPPED_WEIGHT_CAP_PERMILLE_V1: u32 = 10;
/// **`capped_admission_permille`**: the fraction of the derived admission a `Capped` class admits (probation admits 50).
pub const PALW_CAPPED_ADMISSION_PERMILLE_V1: u32 = 20;
/// The re-verification window: how long after a class leaves `Capped` its holders have to re-verify every capped claim.
pub const PALW_CAPPED_REVERIFY_WINDOW_DAA_V1: u64 = 2_400;
/// The holder `Valid` leaves a capped claim needs inside the window (question 8: every claim is re-verified).
pub const PALW_CAPPED_REVERIFY_QUORUM_V1: usize = 3;
/// The most capped claims a class may hold open (a bound on the table).
pub const PALW_CAPPED_MAX_CLAIMS_V1: usize = 20_000;

// ---------------------------------------------------------------------------------------------
// Part II: the witness profile
// ---------------------------------------------------------------------------------------------

/// **A class's witness profile**, recorded once at its registration past `palw_witness_manifest_v1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwWitnessProfileRowV1 {
    /// Elements of the canonical serving set per position at history `max_context`.
    pub elements_per_position: u64,
    /// The class layout's `max_context`: the positions a job may touch.
    pub max_context: u32,
    /// `elements_per_position × max_context × 8`: the most the witness of one job holds.
    pub bytes: u64,
    /// The witness chunks an attempt of the class commits (beyond chunk 0, the trace).
    pub chunks: u32,
}

/// **The profile of a class's program** — `None` where the program serves nothing (the class keeps the v1 manifest) or where the
/// arithmetic overflows (an absurd program is refused by admission long before; this never wraps).
pub fn palw_witness_profile_v1(program: &misaka_palw_tir::TirProgramV1, max_context: u32) -> Option<PalwWitnessProfileRowV1> {
    if max_context == 0 {
        return None;
    }
    let elements = misaka_palw_tir::dataflow::canonical_witness_elements_per_position_v1(program, max_context as usize)?;
    let elements = u64::try_from(elements).ok()?;
    if elements == 0 {
        return None;
    }
    let bytes = elements.checked_mul(u64::from(max_context))?.checked_mul(PALW_WITNESS_BYTES_PER_ELEMENT_V1)?;
    let chunks = bytes.div_ceil(PALW_WITNESS_CHUNK_BYTES_V1).clamp(1, u64::from(PALW_WITNESS_MAX_CHUNKS_V1)) as u32;
    Some(PalwWitnessProfileRowV1 { elements_per_position: elements, max_context, bytes, chunks })
}

/// **The trace chunk count an attempt of a class must commit** under `palw_witness_manifest_v1`: the trace, plus the witness
/// chunks of the class's profile (just the trace for a class with none).
pub fn palw_witness_canonical_chunk_count_v1(profile: Option<&PalwWitnessProfileRowV1>) -> u32 {
    1u32.saturating_add(profile.map_or(0, |p| p.chunks))
}

/// **The v2 trace manifest root**: `H(domain ‖ trace_root ‖ chunk_count)`. A distinct domain from the v1 root, so a v1 root can never
/// be read as a witness-bearing one. The witness root rides inside `trace_root` (the producer's own commitment over trace and
/// witness), which the panel replays; consensus pins the count and this derivation, and nothing a producer chooses.
pub fn palw_attempt_trace_manifest_root_v2(trace_root: Hash64, chunk_count: u32) -> Hash64 {
    let mut state = keyed(PALW_MESH_TRACE_MANIFEST_V2_DOMAIN);
    state.update(trace_root.as_byte_slice());
    state.update(&chunk_count.to_le_bytes());
    finish(state)
}

/// **Does an `Unavailable` verdict name a witness chunk?** Chunk 0 is the trace; the chunks beyond it are the witness.
pub fn palw_unavailable_names_witness_chunk_v1(chunk_index: u32, trace_chunk_count: u32) -> bool {
    chunk_index >= 1 && chunk_index < trace_chunk_count
}

/// **The window term of the witness**: the compute-equivalent of moving `bytes` at the reference link
/// ([`PALW_WITNESS_CCU_PER_BYTE_V1`]), added to the class's `verification_ccu` where the window is derived — and nowhere else.
pub fn palw_witness_ccu_v1(bytes: u64) -> u128 {
    u128::from(bytes).saturating_mul(PALW_WITNESS_CCU_PER_BYTE_V1)
}

/// **The stateless half of the manifest pin** past `palw_witness_manifest_v1`: a count of 1 keeps the v1 root, a count of
/// `2..=1 + PALW_WITNESS_MAX_CHUNKS_V1` carries the v2 root. Which count a class owes is the stateful half
/// ([`palw_witness_canonical_chunk_count_v1`]).
pub fn palw_witness_manifest_shape_ok_v1(trace_root: Hash64, count: u32, manifest_root: Hash64) -> Result<(), WitnessPinRefusalV1> {
    if count == 0 || count > 1 + PALW_WITNESS_MAX_CHUNKS_V1 {
        return Err(WitnessPinRefusalV1::CountOutOfRange { count });
    }
    let derived = if count == 1 {
        crate::palw_attempt_v2::attempt_trace_manifest_root_v1(trace_root, 1)
    } else {
        palw_attempt_trace_manifest_root_v2(trace_root, count)
    };
    if manifest_root != derived {
        return Err(WitnessPinRefusalV1::ManifestNotDerived { claimed: manifest_root, derived });
    }
    Ok(())
}

/// Why a witness-bearing manifest pin is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WitnessPinRefusalV1 {
    #[error("a trace chunk count of {count}: it is 1 (no witness) or 1 + the class's witness chunks, at most {}", 1 + PALW_WITNESS_MAX_CHUNKS_V1)]
    CountOutOfRange { count: u32 },
    #[error("the trace manifest root {claimed} is not the derived one {derived}")]
    ManifestNotDerived { claimed: Hash64, derived: Hash64 },
}

// ---------------------------------------------------------------------------------------------
// Hashing helpers
// ---------------------------------------------------------------------------------------------

fn keyed(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// Part IV.1: the audit
// ---------------------------------------------------------------------------------------------

/// What an auditor attested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwAuditOutcomeV1 {
    /// `true` for `result == 0` (the leaf matched), `false` for a mismatch.
    pub matched: bool,
    pub signed_daa: u64,
}

/// **One drawn audit**: an auditor, the leaf ticket it audits, what it reserved, and what it said.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwAuditAssignmentV1 {
    pub auditor: PalwBondKeyV2,
    /// A uniform `u64`; the auditor audits committed tile `ticket % tiles`.
    pub ticket: u64,
    /// The penalty reserved on the auditor's bond (0 once released).
    pub reserved: u128,
    pub outcome: Option<PalwAuditOutcomeV1>,
}

/// **A claim's audit row**: written at acceptance, kept through the audit window and the trap reveal window.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwAuditRowV1 {
    pub drawn_daa: u64,
    /// `drawn_daa + PALW_AUDIT_WINDOW_DAA_V1`: an `Audited` leaf signed after it counts for nothing.
    pub audit_end_daa: u64,
    /// `audit_end_daa + PALW_TRAP_REVEAL_WINDOW_DAA_V1`: the row and every reservation leave here.
    pub row_end_daa: u64,
    /// The audit pay: [`palw_mesh_audit_pay_v1`] of the claim's escrowed reward, snapshotted.
    pub pay: u64,
    /// The trap penalty: `pay × PALW_TRAP_PENALTY_MULTIPLE_V1`, snapshotted.
    pub penalty: u128,
    pub assignments: Vec<PalwAuditAssignmentV1>,
    /// Set once a `TrapRevealed` has settled this claim (a second reveal of it changes nothing).
    pub trap_settled: bool,
}

impl PalwAuditRowV1 {
    /// The assignment of `auditor`, if it was drawn.
    pub fn assignment_of(&self, auditor: &PalwBondKeyV2) -> Option<&PalwAuditAssignmentV1> {
        self.assignments.iter().find(|a| a.auditor == *auditor)
    }

    /// How many drawn auditors attested a match.
    pub fn matches(&self) -> usize {
        self.assignments.iter().filter(|a| a.outcome.is_some_and(|o| o.matched)).count()
    }

    /// Whether every drawn auditor attested a match (what licenses a capped claim).
    pub fn all_matched(&self) -> bool {
        !self.assignments.is_empty() && self.matches() == self.assignments.len()
    }
}

/// **The audit pay of a claim**: [`PALW_AUDIT_PAY_PERMILLE_V1`] ‰ of its escrowed reward.
pub fn palw_mesh_audit_pay_v1(escrowed_reward: u64) -> u64 {
    ((u128::from(escrowed_reward) * u128::from(PALW_AUDIT_PAY_PERMILLE_V1)) / 1_000) as u64
}

/// **The trap penalty and the reservation an auditor takes**: ten audit pays.
pub fn palw_mesh_trap_penalty_v1(pay: u64) -> u128 {
    u128::from(pay).saturating_mul(PALW_TRAP_PENALTY_MULTIPLE_V1)
}

/// **The trap bounty**: [`PALW_TRAP_BOUNTY_MULTIPLE_V1`] audit pays.
pub fn palw_mesh_trap_bounty_v1(pay: u64) -> u64 {
    pay.saturating_mul(PALW_TRAP_BOUNTY_MULTIPLE_V1)
}

/// **The audit draw's seed**: bound to the claim's carrying block and execution root (which it cannot choose after the fact), the
/// claim id and the DAA of the draw.
pub fn palw_mesh_audit_seed_v1(claim_id: &Hash64, accepted_block: &Hash64, execution_root: &Hash64, drawn_daa: u64) -> Hash64 {
    let mut state = keyed(PALW_MESH_AUDIT_DRAW_DOMAIN_V1);
    state.update(b"seed");
    state.update(claim_id.as_byte_slice());
    state.update(accepted_block.as_byte_slice());
    state.update(execution_root.as_byte_slice());
    state.update(&drawn_daa.to_le_bytes());
    finish(state)
}

/// A candidate auditor: its bond, its operator (one seat per operator), and its stake weight in whole MSK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwAuditCandidateV1 {
    pub bond: PalwBondKeyV2,
    pub operator_id: Hash64,
    pub weight_msk: u64,
}

/// **The audit draw** over `candidates` (the caller's eligibility filter: Active, mature, not the claim's producer, free
/// collateral for the penalty): every candidate holds an Efraimidis–Spirakis key `-ln(u)/weight` from a ticket
/// `H(seed ‖ bond)`, the lowest keys win (the panel race's own comparison, [`crate::palw_panel_v2::palw_draw_key_cmp_v1`]), one seat
/// per operator, `count` of them. Each winner draws its leaf ticket from the seed and its position. Deterministic, pure.
pub fn palw_mesh_audit_draw_v1(seed: &Hash64, candidates: &[PalwAuditCandidateV1], count: usize) -> Vec<(PalwBondKeyV2, u64)> {
    let mut keyed_candidates: Vec<(u128, &PalwAuditCandidateV1)> = candidates
        .iter()
        .map(|candidate| {
            let mut state = keyed(PALW_MESH_AUDIT_DRAW_DOMAIN_V1);
            state.update(b"ticket");
            state.update(seed.as_byte_slice());
            state.update(&borsh::to_vec(&candidate.bond).expect("a bond key serializes"));
            let u = crate::palw_panel_v2::palw_draw_ticket_u64_v1(&finish(state));
            (crate::palw_panel_v2::palw_draw_neg_log2_q64_v1(u), candidate)
        })
        .collect();
    keyed_candidates.sort_by(|(l_i, c_i), (l_j, c_j)| {
        crate::palw_panel_v2::palw_draw_key_cmp_v1(*l_i, c_i.weight_msk.max(1), &c_i.operator_id, *l_j, c_j.weight_msk.max(1), &c_j.operator_id)
            .then_with(|| c_i.bond.cmp(&c_j.bond))
    });
    let mut operators: Vec<Hash64> = Vec::new();
    let mut out = Vec::new();
    for (_, candidate) in keyed_candidates {
        if out.len() >= count {
            break;
        }
        if operators.contains(&candidate.operator_id) {
            continue;
        }
        operators.push(candidate.operator_id);
        let mut state = keyed(PALW_MESH_AUDIT_DRAW_DOMAIN_V1);
        state.update(b"leaf");
        state.update(seed.as_byte_slice());
        state.update(&(out.len() as u64).to_le_bytes());
        state.update(&borsh::to_vec(&candidate.bond).expect("a bond key serializes"));
        let leaf_ticket = crate::palw_panel_v2::palw_draw_ticket_u64_v1(&finish(state));
        out.push((candidate.bond, leaf_ticket));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Part IV.1: traps
// ---------------------------------------------------------------------------------------------

/// **`TrapCommitted`** (object tag 93): a setter commits to a planted fault before it carries the claim.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTrapCommittedV1 {
    pub setter_bond: PalwBondKeyV2,
    /// [`palw_trap_commitment_v1`] of the claim, the fault tile, the tile count and the salt.
    pub commitment: Hash64,
    /// ML-DSA-87 over [`palw_trap_committed_message_v1`] under [`PALW_MESH_TRAP_COMMITTED_MLDSA87_CONTEXT_V1`].
    pub signature: Vec<u8>,
}

/// **`TrapRevealed`** (object tag 94): the claim, the planted tile and the salt that open a commitment.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTrapRevealedV1 {
    pub setter_bond: PalwBondKeyV2,
    pub claim: Hash64,
    /// Which of the claim's `tiles` committed tiles carries the planted fault.
    pub fault_leaf: u64,
    /// How many committed tiles the trap claim has (at most [`PALW_TRAP_MAX_TILES_V1`]).
    pub tiles: u64,
    pub salt: [u8; 32],
    /// ML-DSA-87 over [`palw_trap_revealed_message_v1`] under [`PALW_MESH_TRAP_REVEALED_MLDSA87_CONTEXT_V1`].
    pub signature: Vec<u8>,
}

/// `H(commitment-domain ‖ claim ‖ le64(fault_leaf) ‖ le64(tiles) ‖ salt)`.
pub fn palw_trap_commitment_v1(claim: &Hash64, fault_leaf: u64, tiles: u64, salt: &[u8; 32]) -> Hash64 {
    let mut state = keyed(PALW_MESH_TRAP_COMMITMENT_DOMAIN_V1);
    state.update(claim.as_byte_slice());
    state.update(&fault_leaf.to_le_bytes());
    state.update(&tiles.to_le_bytes());
    state.update(salt);
    finish(state)
}

/// What a setter signs over `TrapCommitted`.
pub fn palw_trap_committed_message_v1(network_domain: Hash64, setter: &PalwBondKeyV2, commitment: &Hash64) -> Hash64 {
    let mut state = keyed(PALW_MESH_TRAP_COMMITTED_MESSAGE_DOMAIN_V1);
    state.update(network_domain.as_byte_slice());
    state.update(&borsh::to_vec(setter).expect("a bond key serializes"));
    state.update(commitment.as_byte_slice());
    finish(state)
}

/// What a setter signs over `TrapRevealed`.
pub fn palw_trap_revealed_message_v1(
    network_domain: Hash64,
    setter: &PalwBondKeyV2,
    claim: &Hash64,
    fault_leaf: u64,
    tiles: u64,
    salt: &[u8; 32],
) -> Hash64 {
    let mut state = keyed(PALW_MESH_TRAP_REVEALED_MESSAGE_DOMAIN_V1);
    state.update(network_domain.as_byte_slice());
    state.update(&borsh::to_vec(setter).expect("a bond key serializes"));
    state.update(claim.as_byte_slice());
    state.update(&fault_leaf.to_le_bytes());
    state.update(&tiles.to_le_bytes());
    state.update(salt);
    finish(state)
}

impl PalwTrapCommittedV1 {
    /// Build and sign `TrapCommitted` for a trap about `claim` (the node and the tests build here).
    pub fn sign_v1(
        network_domain: Hash64,
        setter_bond: PalwBondKeyV2,
        claim: &Hash64,
        fault_leaf: u64,
        tiles: u64,
        salt: &[u8; 32],
        sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
    ) -> Option<Self> {
        let commitment = palw_trap_commitment_v1(claim, fault_leaf, tiles, salt);
        let message = palw_trap_committed_message_v1(network_domain, &setter_bond, &commitment);
        let signature = sign(message.as_byte_slice(), PALW_MESH_TRAP_COMMITTED_MLDSA87_CONTEXT_V1)?;
        Some(Self { setter_bond, commitment, signature })
    }
}

impl PalwTrapRevealedV1 {
    /// Build and sign `TrapRevealed`.
    pub fn sign_v1(
        network_domain: Hash64,
        setter_bond: PalwBondKeyV2,
        claim: Hash64,
        fault_leaf: u64,
        tiles: u64,
        salt: [u8; 32],
        sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
    ) -> Option<Self> {
        let message = palw_trap_revealed_message_v1(network_domain, &setter_bond, &claim, fault_leaf, tiles, &salt);
        let signature = sign(message.as_byte_slice(), PALW_MESH_TRAP_REVEALED_MLDSA87_CONTEXT_V1)?;
        Some(Self { setter_bond, claim, fault_leaf, tiles, salt, signature })
    }

    /// The commitment this reveal opens.
    pub fn commitment(&self) -> Hash64 {
        palw_trap_commitment_v1(&self.claim, self.fault_leaf, self.tiles, &self.salt)
    }
}

/// **The trap slot lottery**: is `bond` drawn to set a trap in the slot `daa` falls in? A uniform ticket per `(bond, slot)` against
/// [`PALW_TRAP_RATE_BP_V1`] of 10,000. A bond that is not drawn may not commit a trap. (A bond is an outpoint of one chain, so no
/// network domain is needed to keep two chains' lotteries apart.)
pub fn palw_trap_slot_drawn_v1(bond: &PalwBondKeyV2, daa: u64) -> bool {
    let mut state = keyed(PALW_MESH_TRAP_SLOT_DOMAIN_V1);
    state.update(&borsh::to_vec(bond).expect("a bond key serializes"));
    state.update(&(daa / PALW_TRAP_SLOT_DAA_V1).to_le_bytes());
    crate::palw_panel_v2::palw_draw_ticket_u64_v1(&finish(state)) % 10_000 < PALW_TRAP_RATE_BP_V1
}

/// **Is `ticket` the planted tile?** The auditor audits tile `ticket % tiles`.
pub fn palw_trap_ticket_hits_v1(ticket: u64, fault_leaf: u64, tiles: u64) -> bool {
    tiles != 0 && ticket % tiles == fault_leaf
}

/// What a trap row remembers.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTrapRowV1 {
    pub setter: PalwBondKeyV2,
    pub committed_daa: u64,
    /// What the setter reserved (0 once released).
    pub deposit: u128,
    /// `Some(claim)` once revealed.
    pub revealed_claim: Option<Hash64>,
}

/// **Why a trap move is refused**, by name.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwMeshErrorV1 {
    #[error("a mesh move below its fence ({0})")]
    Dormant(&'static str),
    #[error("bond {0:?} is not registered on this chain")]
    UnknownBond(PalwBondKeyV2),
    #[error("bond {0:?} is not Active: a retiring or retired bond sets no trap")]
    BondNotActive(PalwBondKeyV2),
    #[error("a trap commitment already on the chain: one commitment, one trap")]
    TrapCommitmentKnown,
    #[error("bond {0:?} already has a trap open: one at a time")]
    TrapAlreadyOpen(PalwBondKeyV2),
    #[error("bond {0:?} is not drawn to set a trap in this slot")]
    TrapSlotNotDrawn(PalwBondKeyV2),
    #[error("the open trap table is full ({PALW_TRAP_MAX_OPEN_V1})")]
    TrapTableFull,
    #[error("bond {bond:?} has {free} sompi free: a trap deposit is {PALW_TRAP_DEPOSIT_SOMPI_V1}")]
    TrapDepositUnaffordable { bond: PalwBondKeyV2, free: u128 },
    #[error("a trap object whose signature is {got} bytes, not the ML-DSA-87 {expected}")]
    SignatureLength { got: usize, expected: usize },
    #[error("the trap object's signature does not verify under bond {0:?}'s registered key")]
    BadSignature(PalwBondKeyV2),
    #[error("a trap reveal that opens no commitment this chain holds")]
    TrapNotCommitted,
    #[error("a trap revealed by bond {got:?} that committed it as {committed:?}")]
    TrapSetterDiffers { got: PalwBondKeyV2, committed: PalwBondKeyV2 },
    #[error("the trap is already revealed")]
    TrapAlreadyRevealed,
    #[error("a trap over {tiles} tiles with planted tile {fault_leaf}: tiles are 1..={PALW_TRAP_MAX_TILES_V1} and the tile is below the count")]
    TrapTilesInvalid { tiles: u64, fault_leaf: u64 },
    #[error("claim {0} is not a claim this chain holds an audit row for")]
    TrapClaimNotAudited(Hash64),
    #[error("claim {0} is not the setter's own claim: a trap is a claim its setter produced")]
    TrapClaimNotTheSetters(Hash64),
    #[error("a trap revealed at DAA {at} before the audit window closes at {audit_end}")]
    TrapRevealedEarly { at: u64, audit_end: u64 },
    #[error("a trap revealed at DAA {at}, past its reveal window ending at {row_end}")]
    TrapRevealedLate { at: u64, row_end: u64 },
    #[error("an Audited leaf's result {0} is neither 0 (match) nor 1 (mismatch)")]
    AuditResultUnknown(u8),
}

/// The ML-DSA-87 signature length every mesh object carries.
fn signature_length_ok(signature: &[u8]) -> Result<(), PalwMeshErrorV1> {
    let expected = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
    if signature.len() != expected {
        return Err(PalwMeshErrorV1::SignatureLength { got: signature.len(), expected });
    }
    Ok(())
}

/// **`TrapCommitted`'s shape** (stateless): the signature's length.
pub fn palw_trap_committed_shape_v1(object: &PalwTrapCommittedV1) -> Result<(), PalwMeshErrorV1> {
    signature_length_ok(&object.signature)
}

/// **`TrapRevealed`'s shape** (stateless): the signature's length, and a tile count and tile the commitment can have bound.
pub fn palw_trap_revealed_shape_v1(object: &PalwTrapRevealedV1) -> Result<(), PalwMeshErrorV1> {
    signature_length_ok(&object.signature)?;
    if object.tiles == 0 || object.tiles > PALW_TRAP_MAX_TILES_V1 || object.fault_leaf >= object.tiles {
        return Err(PalwMeshErrorV1::TrapTilesInvalid { tiles: object.tiles, fault_leaf: object.fault_leaf });
    }
    Ok(())
}

/// **`TrapCommitted`'s signature**, under the setter's registered key.
pub fn palw_trap_committed_verify_v1<V>(
    state: &PalwChainStateV2,
    network_domain: Hash64,
    object: &PalwTrapCommittedV1,
    verify_mldsa87: V,
) -> Result<(), PalwMeshErrorV1>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    signature_length_ok(&object.signature)?;
    let bond = state.bond(&object.setter_bond).ok_or(PalwMeshErrorV1::UnknownBond(object.setter_bond))?;
    let message = palw_trap_committed_message_v1(network_domain, &object.setter_bond, &object.commitment);
    if verify_mldsa87(&bond.pubkey, message.as_byte_slice(), &object.signature, PALW_MESH_TRAP_COMMITTED_MLDSA87_CONTEXT_V1) {
        Ok(())
    } else {
        Err(PalwMeshErrorV1::BadSignature(object.setter_bond))
    }
}

/// **`TrapRevealed`'s signature**, under the setter's registered key.
pub fn palw_trap_revealed_verify_v1<V>(
    state: &PalwChainStateV2,
    network_domain: Hash64,
    object: &PalwTrapRevealedV1,
    verify_mldsa87: V,
) -> Result<(), PalwMeshErrorV1>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    signature_length_ok(&object.signature)?;
    let bond = state.bond(&object.setter_bond).ok_or(PalwMeshErrorV1::UnknownBond(object.setter_bond))?;
    let message = palw_trap_revealed_message_v1(
        network_domain,
        &object.setter_bond,
        &object.claim,
        object.fault_leaf,
        object.tiles,
        &object.salt,
    );
    if verify_mldsa87(&bond.pubkey, message.as_byte_slice(), &object.signature, PALW_MESH_TRAP_REVEALED_MLDSA87_CONTEXT_V1) {
        Ok(())
    } else {
        Err(PalwMeshErrorV1::BadSignature(object.setter_bond))
    }
}

// ---------------------------------------------------------------------------------------------
// The mesh's state (three tables, one Some-only root block)
// ---------------------------------------------------------------------------------------------

/// **What a capped claim remembers**: the class it belongs to, when it was accepted, and the end of its re-verification window once
/// the class has left `Capped` (`None` while the class is still capped: the claim waits, owing no bind deadline).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCappedClaimRowV1 {
    pub class_id: Hash64,
    pub accepted_daa: u64,
    /// `Some(daa)`: the re-verification window ends here (set when the class leaves `Capped`).
    pub window_end_daa: Option<u64>,
}

/// **The mesh's tables**: witness profiles by class, audit rows by claim, trap rows by commitment, capped claim rows by claim.
/// All empty on every network until a fence's first write; the root block, the carriage tail and the delta entry are Some-only.
#[derive(Clone, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwMeshStateV1 {
    pub witness: std::collections::BTreeMap<Hash64, PalwWitnessProfileRowV1>,
    pub audits: std::collections::BTreeMap<Hash64, PalwAuditRowV1>,
    pub traps: std::collections::BTreeMap<Hash64, PalwTrapRowV1>,
    pub capped: std::collections::BTreeMap<Hash64, PalwCappedClaimRowV1>,
}

impl PalwMeshStateV1 {
    pub fn is_empty(&self) -> bool {
        self.witness.is_empty() && self.audits.is_empty() && self.traps.is_empty() && self.capped.is_empty()
    }
}

/// **What a node reads of the mesh** (status RPC and the kit): table sizes, the fence heights, and one seat's open audits.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwMeshStatusV1 {
    pub witness_fence_daa: Option<u64>,
    pub audit_fence_daa: Option<u64>,
    pub capped_fence_daa: Option<u64>,
    pub witness_profiles: u64,
    pub audit_rows: u64,
    pub traps_open: u64,
    pub capped_claims: u64,
    /// `(claim, ticket, answered)` for each audit assigned to the asked seat that is still open.
    pub own_audits: Vec<(Hash64, u64, bool)>,
}

/// **An audit this seat owes** (what a node reads of the mesh to do its duty): the claim it was drawn on, the class and roots it must
/// judge it against, the producer, the carrying block (the job's anchor derives from it), the leaf ticket it audits, and the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwMeshAuditDutyV1 {
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub accepted_block: Hash64,
    pub trace_root: Hash64,
    pub execution_root: Hash64,
    /// The DAA the audit was drawn at (a compact reference to the claim names it).
    pub drawn_daa: u64,
    pub audit_end_daa: u64,
    pub ticket: u64,
}

// ---------------------------------------------------------------------------------------------
// Part IV.2: capped onboarding
// ---------------------------------------------------------------------------------------------

/// **The class's panel is the mesh while it is `Capped`.** True where `state` is a lifecycle state of that name.
pub fn palw_lifecycle_is_capped_v1(state: &crate::palw_model_registry_v1::PalwModelLifecycleV1) -> bool {
    matches!(state, crate::palw_model_registry_v1::PalwModelLifecycleV1::Capped { .. })
}

/// **Has a class just left `Capped`?** (holders seated: its waiting claims get their re-verification window.)
pub fn palw_lifecycle_left_capped_v1(
    before: &crate::palw_model_registry_v1::PalwModelLifecycleV1,
    after: &crate::palw_model_registry_v1::PalwModelLifecycleV1,
) -> bool {
    palw_lifecycle_is_capped_v1(before) && !palw_lifecycle_is_capped_v1(after)
}

/// **The weight cap's arithmetic**: may a class holding `share_permille` of the table enter `Capped` when the capped classes hold
/// `capped_total_permille` already? All capped classes together stay within [`PALW_CAPPED_WEIGHT_CAP_PERMILLE_V1`].
pub fn palw_capped_weight_admits_v1(capped_total_permille: u32, share_permille: u32) -> bool {
    capped_total_permille.checked_add(share_permille).is_some_and(|total| total <= PALW_CAPPED_WEIGHT_CAP_PERMILLE_V1)
}

// ---------------------------------------------------------------------------------------------
// The fences
// ---------------------------------------------------------------------------------------------

/// The entry a flag day (or a drill) arms the witness manifest with.
pub const PALW_WITNESS_MANIFEST_ENTRY_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_witness_manifest_v1",
    set: |params, at| {
        params.palw_witness_manifest_v1 = at;
        params.sync_palw_witness_manifest_v1();
    },
};
/// The entry a flag day (or a drill) arms the audit mesh with.
pub const PALW_AUDIT_MESH_ENTRY_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_audit_mesh_v1",
    set: |params, at| {
        params.palw_audit_mesh_v1 = at;
        params.sync_palw_audit_mesh_v1();
    },
};
/// The entry a flag day (or a drill) arms capped onboarding with.
pub const PALW_CAPPED_ONBOARDING_ENTRY_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_capped_onboarding_v1",
    set: |params, at| {
        params.palw_capped_onboarding_v1 = at;
        params.sync_palw_capped_onboarding_v1();
    },
};

/// The drill's one-entry lists (`--palw-drill-witness-at`, `--palw-drill-audit-mesh-at`, `--palw-drill-capped-at`).
pub const PALW_DRILL_WITNESS_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_WITNESS_MANIFEST_ENTRY_V1];
pub const PALW_DRILL_AUDIT_MESH_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_AUDIT_MESH_ENTRY_V1];
pub const PALW_DRILL_CAPPED_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_CAPPED_ONBOARDING_ENTRY_V1];

fn armed(fence: Option<ForkActivation>) -> Option<u64> {
    fence.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score())
}

fn in_force_at(fence: Option<ForkActivation>, at: u64) -> bool {
    fence.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at)
}

impl Params {
    /// `palw_witness_manifest_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it with a real height.
    pub fn palw_witness_manifest_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_witness_manifest_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// `palw_audit_mesh_v1`, resolved.
    pub fn palw_audit_mesh_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_audit_mesh_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// `palw_capped_onboarding_v1`, resolved.
    pub fn palw_capped_onboarding_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_capped_onboarding_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// Whether the witness manifest is in force at `daa_score`. `false` on every shipped preset.
    pub fn palw_witness_manifest_active_at(&self, daa_score: u64) -> bool {
        self.palw_witness_manifest_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Whether the audit mesh is in force at `daa_score`. `false` on every shipped preset.
    pub fn palw_audit_mesh_active_at(&self, daa_score: u64) -> bool {
        self.palw_audit_mesh_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// Whether capped onboarding is in force at `daa_score`. `false` on every shipped preset.
    pub fn palw_capped_onboarding_active_at(&self, daa_score: u64) -> bool {
        self.palw_capped_onboarding_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The three fences' mirrors** on the V2 bundle's state params, which the fold reads. Written here and nowhere else.
    pub fn sync_palw_witness_manifest_v1(&mut self) {
        let from_daa = armed(self.palw_witness_manifest_v1);
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_witness_manifest_from_daa(from_daa);
        }
    }

    pub fn sync_palw_audit_mesh_v1(&mut self) {
        let from_daa = armed(self.palw_audit_mesh_v1);
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_audit_mesh_from_daa(from_daa);
        }
    }

    pub fn sync_palw_capped_onboarding_v1(&mut self) {
        let from_daa = armed(self.palw_capped_onboarding_v1);
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_capped_from_daa(from_daa);
        }
    }

    /// **The witness manifest's refusals**, asked by [`Params::validate_palw_v2`]: a mirror that disagrees with the fence; arming off
    /// ConsensusV2; arming without `palw_tir_v1` (the witness is an IR class's) and `palw_unavailable_abstains` (an `Unavailable`
    /// naming a witness chunk abstains) in force at or below it. `Some(never())` is dormant and passes.
    pub fn validate_palw_witness_manifest_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.witness_manifest_from_daa(),
            _ => None,
        };
        let at = armed(self.palw_witness_manifest_v1);
        if mirror != at {
            return Err(PalwModeV2Error::Invalid(
                "palw_witness_manifest_v1 disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_witness_manifest_v1 after the bundle is assembled",
            ));
        }
        let Some(at) = at else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_witness_manifest_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        if !self.palw_tir_v1.is_some_and(|f| f.activation != ForkActivation::never() && f.activation.daa_score() <= at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_witness_manifest_v1 needs palw_tir_v1 in force at or below it: the witness is an IR class's",
            ));
        }
        if !in_force_at(self.palw_unavailable_abstains, at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_witness_manifest_v1 needs palw_unavailable_abstains in force at or below it: an Unavailable naming a witness chunk abstains",
            ));
        }
        Ok(())
    }

    /// **The audit mesh's refusals**: the mirror; ConsensusV2; and `palw_verification_vertex_v1` (the `Audited` leaf rides a
    /// vertex), `palw_tir_v1` (the audit is the court's leaf check) and `palw_panel_economy` (the audit pay comes out of the panel
    /// reserve) in force at or below it.
    pub fn validate_palw_audit_mesh_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.audit_mesh_from_daa(),
            _ => None,
        };
        let at = armed(self.palw_audit_mesh_v1);
        if mirror != at {
            return Err(PalwModeV2Error::Invalid(
                "palw_audit_mesh_v1 disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_audit_mesh_v1 after the bundle is assembled",
            ));
        }
        let Some(at) = at else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_audit_mesh_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        if !in_force_at(self.palw_verification_vertex_v1, at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_audit_mesh_v1 needs palw_verification_vertex_v1 in force at or below it: an Audited leaf rides a vertex",
            ));
        }
        if !self.palw_tir_v1.is_some_and(|f| f.activation != ForkActivation::never() && f.activation.daa_score() <= at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_audit_mesh_v1 needs palw_tir_v1 in force at or below it: the audit is the court's leaf check",
            ));
        }
        if !in_force_at(self.palw_panel_economy, at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_audit_mesh_v1 needs palw_panel_economy in force at or below it: audit pay comes out of the panel reserve",
            ));
        }
        Ok(())
    }

    /// **Capped onboarding's refusals**: the mirror; ConsensusV2; and `palw_audit_mesh_v1`, `palw_admission_independence` (the
    /// `Candidate` state the walk starts from) and `palw_registry_resilience` (the registry's probation memory) in force at or
    /// below it.
    pub fn validate_palw_capped_onboarding_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.capped_from_daa(),
            _ => None,
        };
        let at = armed(self.palw_capped_onboarding_v1);
        if mirror != at {
            return Err(PalwModeV2Error::Invalid(
                "palw_capped_onboarding_v1 disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_capped_onboarding_v1 after the bundle is assembled",
            ));
        }
        let Some(at) = at else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_capped_onboarding_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        if !in_force_at(self.palw_audit_mesh_v1, at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_capped_onboarding_v1 needs palw_audit_mesh_v1 in force at or below it: capped claims are checked by the mesh",
            ));
        }
        if !in_force_at(self.palw_admission_independence, at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_capped_onboarding_v1 needs palw_admission_independence in force at or below it: the walk starts from Candidate",
            ));
        }
        if !in_force_at(self.palw_registry_resilience, at) {
            return Err(PalwModeV2Error::Invalid(
                "palw_capped_onboarding_v1 needs palw_registry_resilience in force at or below it: the probation memory holds a capped class's return",
            ));
        }
        Ok(())
    }
}
