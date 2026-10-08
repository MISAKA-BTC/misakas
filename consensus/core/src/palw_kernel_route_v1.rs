//! **G14 lane D — the kernel route (`misaka-palw-kernel`) as consensus state: types, constants, messages, the interim policy.**
//!
//! `misaka-palw-kernel`'s [`KernelLedgerV1`] is a deterministic fold of signed public objects over bonds the consumer owns. This
//! module is the consumer's half that needs no builder: the object tags and their signatures' domains, the one mapping from a
//! V2 bond to the kernel's bond digest, the **interim** ledger policy, the state the fold keeps, and its root. The fold itself
//! (`palw_kernel_route_fold_v1`) is a child of `palw_state_v2` and reads the builder.
//!
//! # Allocation (frozen by the lead, 2026-10-08)
//!
//! object tags **110** ([`PalwConsensusObjectV2::KernelRouteV1`](crate::palw_state_v2::PalwConsensusObjectV2)) and **111**
//! (`KernelConstraintReceiptV1`), delta numbers **160** (a row) and **161** (the header), carriage tail
//! [`PALW_CARRIAGE_KERNEL_ROUTE_TAIL_V1`] `0xEC`, RPC ops 210–219, the dormant fence `palw_probabilistic_constraints_v1`
//! ([`crate::palw_probabilistic_constraints_v1`]) whose validation REFUSES every real height: nothing here can run on a network.
//!
//! # State
//!
//! `PalwChainStateV2.kernel_route: Option<PalwKernelRouteStateV1>` — `None` until the first block at or past the fence. Its rows are
//! the kernel ledger's own `(table, key) → Borsh row` map ([`misaka_palw_kernel::rows`]) plus three consensus tables the ledger
//! does not know: the bond-key map (the kernel's digest → the V2 bond), the **interim** seat assignments and the receipts counted so
//! far. The Some-only root block is `kernel-route/v1 ‖ ledger root ‖ aux root`; the ledger root equals
//! [`KernelLedgerV1::root`] of the ledger the rows describe, so a fresh verifier that rebuilds the ledger from the rows a node serves
//! checks them against the root the chain committed.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::descriptor::{
    KernelScheduleV1, KernelStatusV1, k2_tir_v1_descriptor, k2_tir_v2_descriptor, k2_tir_v3_descriptor,
};
use misaka_palw_kernel::gate::ProsecutionPolicyV1;
use misaka_palw_kernel::hash::Digest;
use misaka_palw_kernel::ledger::{KernelLedgerV1, LedgerPolicyV1};
use misaka_palw_kernel::rows::{LedgerRowsV1, LedgerScalarsV1, config_root_of, root_of_rows};
use misaka_palw_kernel::verify::ScopeV1;

use crate::Hash64;
use crate::constants::SOMPI_PER_KASPA;
use crate::palw_state_v2::PalwBondKeyV2;

/// The ML-DSA-87 context a signer's bond signs a [`KernelRouteV1`](crate::palw_state_v2::PalwConsensusObjectV2) under.
pub const PALW_KERNEL_ROUTE_OBJECT_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/kernel-route/object/v1";
/// The domain of the signed message (network, signer, the kernel object's bytes).
pub const PALW_KERNEL_ROUTE_OBJECT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel-route/object-message/v1";
/// The domain of the bond-digest mapping: the kernel sees a bond only as this digest.
pub const PALW_KERNEL_ROUTE_BOND_DOMAIN_V1: &[u8] = b"misaka-palw/kernel-route/bond-id/v1";
/// **INTERIM** seat assignment (not a production beacon, grindable): the seed's domain (`H(domain; claim id)`).
pub const PALW_KERNEL_ROUTE_INTERIM_ASSIGNMENT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel-route/interim-assignment/v1";
/// The domain of an interim seat's lottery entry.
pub const PALW_KERNEL_ROUTE_INTERIM_TICKET_DOMAIN_V1: &[u8] = b"misaka-palw/kernel-route/interim-ticket/v1";
/// The domain of the interim challenge anchor and sample seed a seat's receipt is bound to.
pub const PALW_KERNEL_ROUTE_INTERIM_CHALLENGE_DOMAIN_V1: &[u8] = b"misaka-palw/kernel-route/interim-challenge/v1";
/// The domain of the interim challenge policy id the ledger policy names.
pub const PALW_KERNEL_ROUTE_INTERIM_POLICY_DOMAIN_V1: &[u8] = b"misaka-palw/kernel-route/interim-challenge-policy/v1";
/// The domain of a kernel payout's row key (`0xFD` prefix, keyed by payee).
pub const PALW_KERNEL_ROUTE_PAYOUT_DOMAIN_V1: &[u8] = b"misaka-palw/kernel-route/payout/v1";
/// The first byte of every kernel payout row's key: `0xFD`, after the claims' escrows (uniform hashes) and BEFORE the panel seats'
/// `0xFE` and the market's `0xFF`, so a kernel payee never delays a claim's escrow and the panel's and the market's rows never
/// delay a kernel payee.
pub const PALW_KERNEL_ROUTE_PAYOUT_KEY_PREFIX_V1: u8 = 0xFD;

/// The carriage tail of the kernel route state (after Lane MU's `0xEB`).
pub const PALW_CARRIAGE_KERNEL_ROUTE_TAIL_V1: u8 = 0xEC;

/// **Auxiliary tables** (the ledger's own tables are `1..=9`, see [`misaka_palw_kernel::rows`]). Declared numbers.
pub const PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1: u8 = 32;
pub const PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1: u8 = 33;
pub const PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1: u8 = 34;

/// The most seats one claim's interim assignment draws, and the per-segment quorum the tally needs.
pub const PALW_KERNEL_ROUTE_INTERIM_SEATS_V1: usize = 3;
pub const PALW_KERNEL_ROUTE_INTERIM_QUORUM_V1: u8 = 3;

/// **What one kernel route object can be, in encoded bytes** — the bound a node with the real carriers can honour: an object rides
/// one `0x4b` carrier directly, or in `ObjectChunk`s ([`crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES`] ×
/// [`crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_COUNT`]) less the wrapper (the signer, the signature, the length prefix). The
/// registration of a class is refused unless its worst filing, response and commitments fit this
/// ([`misaka_palw_kernel::ledger::carrier_fit_v1`]).
pub const PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1: usize =
    crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES * crate::palw_state_v2::PALW_OBJECT_CHUNK_MAX_COUNT as usize - (16 << 10);

/// **The one mapping from a V2 bond to the kernel's bond digest** — used for the producer, the accuser, the demander and the seat
/// everywhere: `H(domain; txid ‖ index)`.
pub fn palw_kernel_bond_id_v1(bond: &PalwBondKeyV2) -> Digest {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_KERNEL_ROUTE_BOND_DOMAIN_V1).to_state();
    s.update(bond.0.transaction_id.as_byte_slice());
    s.update(&bond.0.index.to_le_bytes());
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    out
}

/// **The message a signer's bond signs**: `H(domain; network ‖ signer ‖ len ‖ bytes)`. The network separates chains, the signer
/// the object from every other bond's, and the bytes ARE the object (strictly decoded by the consumer).
pub fn palw_kernel_route_message_v1(network_domain: Hash64, signer: &PalwBondKeyV2, bytes: &[u8]) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_KERNEL_ROUTE_OBJECT_DOMAIN_V1).to_state();
    s.update(network_domain.as_byte_slice());
    s.update(signer.0.transaction_id.as_byte_slice());
    s.update(&signer.0.index.to_le_bytes());
    s.update(&(bytes.len() as u64).to_le_bytes());
    s.update(bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **What the processor resolves from the ruleset and hands the fold** (`PalwTransitionExtrasV1::kernel_route`): `Some` exactly
/// when `palw_probabilistic_constraints_v1` is active at the block. The network and the ruleset enter the ledger policy, so an
/// object never names its own network.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwKernelRouteExtrasV1 {
    pub network_domain: Hash64,
    pub ruleset_digest: Hash64,
    /// Artifact roots the consumer attests public (`KernelLedgerV1::attest_artifact`). **There is no on-chain artifact
    /// availability or conformance fact yet**, so this is empty in production and a test-only hook fills it in the E2E
    /// (`kernel_route_test_attest_artifact_v1` in the processor, `cfg(test)`): GAP — onboarding conformance + availability.
    pub attested_artifacts: Vec<Hash64>,
}

/// **The INTERIM ledger policy.** Windows are short so a drill crosses them; the amounts are sompi. Values are consensus constants
/// of the (never-armed) fence, written once here: a real activation would revisit every one.
pub fn palw_kernel_route_policy_v1(network_domain: Hash64, ruleset_digest: Hash64) -> LedgerPolicyV1 {
    let policy_id = {
        let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_KERNEL_ROUTE_INTERIM_POLICY_DOMAIN_V1).to_state();
        s.update(network_domain.as_byte_slice());
        let mut out = [0u8; 64];
        out.copy_from_slice(s.finalize().as_bytes());
        out
    };
    LedgerPolicyV1 {
        network_domain: network_domain.as_bytes(),
        ruleset_digest: ruleset_digest.as_bytes(),
        challenge_policy_id: policy_id,
        claim_collateral: 1_000 * SOMPI_PER_KASPA,
        demand_bond: SOMPI_PER_KASPA,
        check_window_daa: 100,
        challenge_window_daa: 50,
        court_deadline_daa: 20,
        proof_grace_daa: 10,
        liability_daa: 200,
        exit_delay_daa: 30,
        dismissed_proof_fee: SOMPI_PER_KASPA / 10,
        accuser_reward_permille: 500,
        default_penalty: 100 * SOMPI_PER_KASPA,
        // INTERIM: a reward needs a funded payout path that does not exist (GAP); the mapping to the coinbase queue is exercised.
        claim_reward: 5 * SOMPI_PER_KASPA,
        max_adjudications_per_block: 64,
        max_court_work_per_block: 1 << 30,
        prosecution: ProsecutionPolicyV1 {
            court_deadline_daa: 20,
            max_sessions_per_claim: 1 << 10,
            max_public_bytes: 1 << 40,
            max_verifier_ram: 1 << 36,
            max_retained_state: 1 << 32,
        },
    }
}

/// The ledger configuration the fold starts every block from (policy, the schedule that arms the K2 descriptors, the descriptors
/// this binary implements). Rows are loaded over it.
pub fn palw_kernel_route_template_v1(policy: LedgerPolicyV1) -> KernelLedgerV1 {
    let (v1, v2, v3) = (k2_tir_v1_descriptor(), k2_tir_v2_descriptor(), k2_tir_v3_descriptor());
    let schedule = KernelScheduleV1::default()
        .with(v1.digest(), KernelStatusV1::Active { since_daa: 0 })
        .with(v2.digest(), KernelStatusV1::Active { since_daa: 0 })
        .with(v3.digest(), KernelStatusV1::Active { since_daa: 0 });
    KernelLedgerV1::genesis(policy, schedule, vec![v1, v2, v3]).expect("the interim kernel route policy validates")
}

/// One seat of an interim assignment.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KernelSeatV1 {
    pub bond: PalwBondKeyV2,
    pub kernel_bond: Digest,
    pub scope: ScopeV1,
}

/// **INTERIM seat assignment of one claim** — deterministic from the chain state at its commitment, grindable, NOT a production
/// beacon: G14 must hold whatever these seats sign (the public court convicts a claim every assigned seat covered).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct KernelAssignmentV1 {
    pub seats: Vec<KernelSeatV1>,
    pub assignment_root: Digest,
    pub challenge_anchor: Digest,
    pub sample_seed: Digest,
    /// The receipts' deadline: the end of the claim's check window.
    pub deadline_daa: u64,
    pub quorum: u8,
}

/// The consumer's part of the header: the configuration the rows were folded under. Part of the state, so a state is self-contained
/// and a fold under another policy is refused.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelRouteHeaderV1 {
    pub policy: LedgerPolicyV1,
    pub config_root: Digest,
    pub scalars: LedgerScalarsV1,
}

/// **The kernel route state** (module doc). `rows` are the ledger's tables; `aux` the consensus tables.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelRouteStateV1 {
    pub header: PalwKernelRouteHeaderV1,
    pub rows: LedgerRowsV1,
    pub aux: BTreeMap<(u8, Vec<u8>), Vec<u8>>,
}

impl PalwKernelRouteStateV1 {
    pub fn new(policy: LedgerPolicyV1, scalars: LedgerScalarsV1) -> Self {
        let template = palw_kernel_route_template_v1(policy);
        let config_root = config_root_of(&template.schedule, &template.known);
        Self { header: PalwKernelRouteHeaderV1 { policy, config_root, scalars }, rows: LedgerRowsV1::new(), aux: BTreeMap::new() }
    }

    /// The ledger's root, computed from the rows alone (equal to [`KernelLedgerV1::root`] of the ledger they describe).
    pub fn ledger_root(&self) -> Hash64 {
        let h = &self.header;
        Hash64::from_bytes(root_of_rows(&h.policy, h.config_root, h.scalars, &self.rows))
    }

    /// The consensus tables' root: every aux row, in key order.
    pub fn aux_root(&self) -> Hash64 {
        let mut s = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/kernel-route/aux-root/v1").to_state();
        s.update(&(self.aux.len() as u64).to_le_bytes());
        for ((table, key), row) in &self.aux {
            s.update(&[*table]);
            s.update(&(key.len() as u32).to_le_bytes());
            s.update(key);
            s.update(&(row.len() as u32).to_le_bytes());
            s.update(row);
        }
        let mut out = [0u8; 64];
        out.copy_from_slice(s.finalize().as_bytes());
        Hash64::from_bytes(out)
    }

    /// The kernel ledger these rows describe (over the state's own configuration).
    pub fn ledger(&self) -> Result<KernelLedgerV1, String> {
        let template = palw_kernel_route_template_v1(self.header.policy);
        KernelLedgerV1::from_rows(&template, self.header.scalars, &self.rows)
    }

    /// A decoded aux row.
    pub fn aux_row<T: BorshDeserialize>(&self, table: u8, key: &[u8]) -> Option<T> {
        borsh::from_slice(self.aux.get(&(table, key.to_vec()))?).ok()
    }

    /// The V2 bond a kernel bond digest stands for (the route has seen it sign or sit).
    pub fn bond_key_of(&self, kernel_bond: &Digest) -> Option<PalwBondKeyV2> {
        self.aux_row(PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, &borsh::to_vec(kernel_bond).expect("a digest serializes"))
    }

    /// A claim's interim assignment, while its receipts are still being counted.
    pub fn assignment_of(&self, claim: &Digest) -> Option<KernelAssignmentV1> {
        self.aux_row(PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1, &borsh::to_vec(claim).expect("a digest serializes"))
    }

    /// The receipts counted so far for a claim (dropped once the claim is covered).
    pub fn receipts_of(&self, claim: &Digest) -> Vec<misaka_palw_kernel::receipt::PalwConstraintReceiptV1> {
        self.aux_row(PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1, &borsh::to_vec(claim).expect("a digest serializes")).unwrap_or_default()
    }

    /// What the kernel has reserved against a V2 bond (a claim's collateral, a demand bond): the term V2's committed-collateral
    /// ledger adds, so no V2 gate counts kernel-reserved collateral as free.
    pub fn reserved_of(&self, bond: &PalwBondKeyV2) -> u64 {
        let key = borsh::to_vec(&palw_kernel_bond_id_v1(bond)).expect("a digest serializes");
        self.rows
            .get(&(misaka_palw_kernel::rows::TABLE_BONDS_V1, key))
            .and_then(|b| borsh::from_slice::<misaka_palw_kernel::ledger::BondRowV1>(b).ok())
            .map(|b| b.reserved)
            .unwrap_or(0)
    }
}

/// **The payout key of a kernel payee**: `H(domain; payload)` under the `0xFD` prefix. One row per payee, accumulating (as the
/// panel's seat pay does).
pub fn palw_kernel_payout_key_v1(payload: &Hash64) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_KERNEL_ROUTE_PAYOUT_DOMAIN_V1).to_state();
    s.update(payload.as_byte_slice());
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    out[0] = PALW_KERNEL_ROUTE_PAYOUT_KEY_PREFIX_V1;
    Hash64::from_bytes(out)
}

/// The interim challenge anchor and sample seed of a claim (grindable: derived from the claim id alone — the receipts' bindings).
pub fn palw_kernel_interim_challenge_v1(claim: &Digest) -> (Digest, Digest) {
    let derive = |tag: u8| {
        let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_KERNEL_ROUTE_INTERIM_CHALLENGE_DOMAIN_V1).to_state();
        s.update(&[tag]);
        s.update(claim);
        let mut out = [0u8; 64];
        out.copy_from_slice(s.finalize().as_bytes());
        out
    };
    (derive(0), derive(1))
}

/// The interim lottery seed of a claim: `H("misaka-palw/kernel-route/interim-assignment/v1"; claim id)`.
pub fn palw_kernel_interim_seed_v1(claim: &Digest) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_KERNEL_ROUTE_INTERIM_ASSIGNMENT_DOMAIN_V1).to_state();
    s.update(claim);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// An interim seat's lottery key: `−log2(u) / W` (smaller wins), `u` the first 8 bytes of `H(domain; seed ‖ operator)`.
pub fn palw_kernel_interim_key_v1(seed: &Hash64, operator: &Hash64, weight: u128) -> u128 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_KERNEL_ROUTE_INTERIM_TICKET_DOMAIN_V1).to_state();
    s.update(seed.as_byte_slice());
    s.update(operator.as_byte_slice());
    let mut u = [0u8; 8];
    u.copy_from_slice(&s.finalize().as_bytes()[..8]);
    crate::palw_panel_v2::palw_draw_neg_log2_q64_v1(u64::from_le_bytes(u)) / weight.max(1)
}

/// The assignment root: `H(seats in order ‖ quorum)`.
pub fn palw_kernel_assignment_root_v1(seats: &[KernelSeatV1], quorum: u8) -> Digest {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/kernel-route/assignment-root/v1").to_state();
    s.update(&[quorum]);
    for seat in seats {
        s.update(&seat.kernel_bond);
        s.update(&borsh::to_vec(&seat.scope).expect("a scope serializes"));
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interim_policy_validates_and_the_bond_digest_separates_indices() {
        let p = palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        p.validate().unwrap();
        let b = |i| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(9), i));
        assert_ne!(palw_kernel_bond_id_v1(&b(0)), palw_kernel_bond_id_v1(&b(1)));
        let state = PalwKernelRouteStateV1::new(p, LedgerScalarsV1::default());
        assert_eq!(state.ledger().unwrap().root(), state.ledger_root().as_bytes(), "an empty state roots like an empty ledger");
    }

    #[test]
    fn the_tail_and_tables_are_the_allocated_ones() {
        assert_eq!(PALW_CARRIAGE_KERNEL_ROUTE_TAIL_V1, 0xEC);
        assert_eq!(
            (PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1, PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1),
            (32, 33, 34)
        );
        assert!(PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 > 1_000_000);
    }
}
