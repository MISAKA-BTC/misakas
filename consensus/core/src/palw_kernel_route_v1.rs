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
use misaka_palw_kernel::opv::OpvPolicyV1;
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
/// The block's adjudication budget already spent by earlier objects of the SAME chain block: `[] → (blue score, adjudications, court work)`.
/// Rewritten by each charged object so the budget bounds the block however the fold is split into single-object rehearsals.
pub const PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1: u8 = 35;

/// **C4 F-C4R3-03: the route's OWN chunk lane** — `borsh((opener bond, group)) → PalwKernelChunkGroupV1` (the lead's allocation of
/// 2026-10-08: aux tables 41–42, riding the route's `KernelRouteRow` deltas, tail `0xEC` and root block). A prosecution object larger
/// than one carrier — a `FileProof` or a position `Respond` of a real class, an onboarding refutation (tag 105) — rides
/// `KernelRouteChunkV1` (tag 113) here, never in the certification lane's `pending_chunks` (ONE network-wide table of eight groups
/// that eight junk chunks hold for 4,000 DAA). A group here is keyed by its SIGNING opener's bond, so no bond can occupy another's
/// room: every bond has [`PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1`] groups of its own, each backed by a deposit forfeited if it never
/// completes, and none outliving its target's deadline.
pub const PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1: u8 = 41;
/// Allocated beside table 41 (2026-10-08) and held unused.
pub const PALW_KERNEL_ROUTE_TABLE_RESERVED_42_V1: u8 = 42;
/// The most half-assembled groups one bond may hold open in the route's chunk lane at once.
pub const PALW_KERNEL_CHUNK_GROUPS_PER_BOND_V1: usize = 2;
/// The longest a group may stay half-assembled (it never outlives its target's deadline either).
pub const PALW_KERNEL_CHUNK_TTL_MAX_DAA_V1: u64 = 64;
/// The deposit a group's opener posts per declared part, held from its free collateral until the group completes and FORFEITED
/// (slashed, burned) if it never does: junk pays, honesty is refunded.
pub const PALW_KERNEL_CHUNK_DEPOSIT_PER_PART_SOMPI_V1: u64 = SOMPI_PER_KASPA;
/// The ML-DSA-87 context the opener's bond signs each chunk under.
pub const PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/kernel-route/chunk/v1";
/// The domain of a chunk's signed message (network, the chunk's Borsh).
pub const PALW_KERNEL_CHUNK_DOMAIN_V1: &[u8] = b"misaka-palw/kernel-route/chunk-message/v1";

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
    /// The chain's genesis hash: what an RFC-0013 conformance commitment names as `chain_genesis`.
    pub chain_genesis: Hash64,
    /// Artifact roots the consumer attests public (`KernelLedgerV1::attest_artifact`). **There is no on-chain artifact
    /// availability or conformance fact yet**, so this is empty in production and a test-only hook fills it in the E2E
    /// (`kernel_route_test_attest_artifact_v1` in the processor, `cfg(test)`): GAP — onboarding conformance + availability.
    pub attested_artifacts: Vec<Hash64>,
    /// **Test seam, `None` in production**: a smaller per-block adjudication cap than the policy's (the processor sets it only under
    /// `cfg(test)`, uniformly for every chain of a test process, so a header's policy never changes mid-chain). Lets a test exhaust the
    /// block budget with a handful of carriers instead of sixty-five.
    pub max_adjudications_per_block: Option<u32>,
    /// **RFC-0015 OptimisticPublicVerification**: `Some` exactly where the network declares the OPV policy (a genesis constant — it is
    /// `Some` at every block of a chain or at none). The processor resolves it from `Params::palw_panel_free_v1`.
    pub opv: Option<PalwKernelOpvExtrasV1>,
    /// **RFC-0004 Part II**: the `palw_typed_roots_v1` fence's activation (`None`: absent) — a genesis constant of the route, like the
    /// OPV policy: from this height the route's schedule holds the typed-roots extension `K2-TR-v1` Active.
    pub typed_roots: Option<u64>,
    /// **Lane DA16: `Params::palw_provider_court_v1`'s activation DAA, where that fence is in force at the block** (`None` below it and on
    /// every network). The provider court's objects fold only with it; a claim committed below it can never move its DA responsibility.
    pub provider_court: Option<u64>,
}

/// What the processor hands the fold for the OPV mode (RFC-0015): the network's policy and its admission list, both read from
/// `Params::palw_panel_free_v1` ([`crate::palw_panel_free_v1::PalwPanelFreeFenceV1`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwKernelOpvExtrasV1 {
    /// The OPV policy, a genesis constant of the route (the fence's terms, activating at the fence's height).
    pub policy: OpvPolicyV1,
    /// **The class ids (mode-bound) the NETWORK'S policy admits for OPV** — consensus, never a registrant's choice.
    pub admitted_classes: Vec<Hash64>,
}

/// **The PALW reporter share on the kernel route, permille** (ADR-0032's 2026-10-10 amendment: 4,900 bps): what an accuser, a
/// demander or an onboarding challenger is paid of a slash the chain actually collected; the rest is burned. One constant for every
/// reporter path of the route, so the share cannot drift between them. (`palw_reporter_share_v2` is INTF's fence for the R-core rate;
/// the kernel route is dormant as a whole, so its share needs no fence of its own: it has never been in force at any other rate.)
pub const PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1: u16 = 490;

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
        // ADR-0032 (2026-10-10 amendment): the PALW reporter share is 49 % (4,900 bps) of what a slash actually COLLECTED, the rest
        // burned. The route's accuser (a conviction), its demanders (a default penalty) and — at the same rate — the onboarding
        // challenger are PALW reporters, so the interim share is that ceiling, not the 500‰ it was. A self-reporter recovers at most
        // 49 %: the net loss of a convicted producer is ≥ 51 % of the collected slash, never the gross slash (ADR-0176 D6).
        accuser_reward_permille: PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1,
        default_penalty: 100 * SOMPI_PER_KASPA,
        // GAP-5 (the user's ruling: user-pays escrow): the reward is paid out of the job's ESCROW, reserved from the poster's bond at
        // posting and spent once at the job's first Final — never new money.
        claim_reward: 5 * SOMPI_PER_KASPA,
        // GAP-5: posting a job burns 1 BILI beside its escrow (a self-posted job is never free), and an escrow no claim can still use
        // goes back to its poster 300 DAA after posting (past the seal TTL, the check and challenge windows and a court).
        job_fee: SOMPI_PER_KASPA,
        job_escrow_ttl_daa: 300,
        max_adjudications_per_block: 64,
        // C4 F-C4R3-05 (round 2): half of every block's court runs only a proof may spend.
        prosecution_reserve_permille: 500,
        max_court_work_per_block: 1 << 30,
        // Seal, then reveal: a claim commits over its producer's seal at least one block old; an unrevealed seal lives one check window.
        claim_seal_delay_daa: 1,
        seal_ttl_daa: 100,
        // OPV-BOOT's sealed-source beacon (v3): a claim seal holds 1 BILI until revealed, forfeited if it expires unrevealed.
        seal_deposit: SOMPI_PER_KASPA,
        prosecution: ProsecutionPolicyV1 {
            court_deadline_daa: 20,
            max_sessions_per_claim: 1 << 10,
            max_public_bytes: 1 << 40,
            max_verifier_ram: 1 << 36,
            max_retained_state: 1 << 32,
        },
    }
}

/// [`palw_kernel_route_template_v1`] over a network that declares an OPV policy.
pub fn palw_kernel_route_template_opv_v1(policy: LedgerPolicyV1, opv: Option<OpvPolicyV1>) -> KernelLedgerV1 {
    let template = palw_kernel_route_template_v1(policy);
    match opv {
        Some(p) => template.with_opv_policy(p).expect("the interim OPV policy validates against the interim ledger policy"),
        None => template,
    }
}

/// [`palw_kernel_route_template_opv_v1`] with the typed-roots extension `K2-TR-v1` Active from `typed` (RFC-0004 Part II). `None` is the
/// template byte for byte — the same schedule, the same `config_root`, the same roots.
pub fn palw_kernel_route_template_typed_v1(policy: LedgerPolicyV1, opv: Option<OpvPolicyV1>, typed: Option<u64>) -> KernelLedgerV1 {
    let mut template = palw_kernel_route_template_opv_v1(policy, opv);
    if let Some(since_daa) = typed {
        let tr = misaka_palw_kernel::spec::k2_tr_v1_descriptor();
        template.schedule = template.schedule.with(tr.digest(), KernelStatusV1::Active { since_daa });
    }
    template
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
    /// RFC-0015: the network's OPV policy, a genesis constant (`None`: the Panel-licensed route alone, and the historical root form).
    pub opv: Option<OpvPolicyV1>,
    /// RFC-0004 Part II: the typed-roots fence's activation, a genesis constant (`None`: the route without typed roots, its historical
    /// schedule and roots).
    pub typed_roots: Option<u64>,
}

/// **The kernel route state** (module doc). `rows` are the ledger's tables; `aux` the consensus tables.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelRouteStateV1 {
    pub header: PalwKernelRouteHeaderV1,
    pub rows: LedgerRowsV1,
    pub aux: BTreeMap<(u8, Vec<u8>), Vec<u8>>,
}

impl PalwKernelRouteStateV1 {
    pub fn new(policy: LedgerPolicyV1, opv: Option<OpvPolicyV1>, scalars: LedgerScalarsV1) -> Self {
        Self::new_typed(policy, opv, None, scalars)
    }

    /// [`Self::new`] under the typed-roots fence's activation (RFC-0004 Part II).
    pub fn new_typed(policy: LedgerPolicyV1, opv: Option<OpvPolicyV1>, typed_roots: Option<u64>, scalars: LedgerScalarsV1) -> Self {
        let template = palw_kernel_route_template_typed_v1(policy, opv, typed_roots);
        let config_root = config_root_of(&template.schedule, &template.known);
        Self {
            header: PalwKernelRouteHeaderV1 { policy, config_root, scalars, opv, typed_roots },
            rows: LedgerRowsV1::new(),
            aux: BTreeMap::new(),
        }
    }

    /// The configuration the rows were folded under (policy, OPV policy, typed roots, schedule, descriptors).
    pub fn template(&self) -> KernelLedgerV1 {
        palw_kernel_route_template_typed_v1(self.header.policy, self.header.opv, self.header.typed_roots)
    }

    /// The ledger's root, computed from the rows alone (equal to [`KernelLedgerV1::root`] of the ledger they describe).
    pub fn ledger_root(&self) -> Hash64 {
        let h = &self.header;
        Hash64::from_bytes(root_of_rows(&h.policy, h.config_root, h.scalars, h.opv.as_ref(), &self.rows))
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
        KernelLedgerV1::from_rows(&self.template(), self.header.scalars, &self.rows)
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

    /// What the route holds against a V2 bond — the ledger's reservations (a claim's collateral, a demand bond) and the deposits of
    /// its open chunk groups: the term V2's committed-collateral ledger adds, so no V2 gate counts it as free.
    pub fn reserved_of(&self, bond: &PalwBondKeyV2) -> u64 {
        self.ledger_reserved_of(bond).saturating_add(self.chunk_deposits_of_v1(bond))
    }

    /// What the kernel LEDGER has reserved against a V2 bond (its `BondRowV1::reserved`). The ledger is synced net of everything
    /// else the bond backs — the chunk deposits included — so a deposit is never also a claim's collateral.
    pub fn ledger_reserved_of(&self, bond: &PalwBondKeyV2) -> u64 {
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

// ---- the public read model (RPC ops 210/211) ------------------------------------------------------------------------------

/// One open position demand, as a reader sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelDemandReadV1 {
    pub stage: u8,
    pub position: u32,
    pub demanders: u32,
    pub filed_daa: u64,
    pub deadline_daa: u64,
    /// The latest rejected response's class name (`malformed`, `wrong_root`, `fake_opening`, ...), if any.
    pub last_rejection: Option<String>,
}

/// **What public discovery returns for an OPV claim** (RFC-0015 §5): the clock facts a fresh verifier plans by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelOpvReadV1 {
    pub admitted_daa: u64,
    /// The latest DAA a fresh verifier may START from and still reach a first filing inside the window.
    pub verifier_start_cutoff_daa: u64,
    /// The earliest Final (the window's end).
    pub final_floor_daa: u64,
    /// The latest Final / decision, whatever is filed.
    pub hard_deadline_daa: u64,
    pub reservation: u64,
    pub max_gain: u64,
    /// What a Final of this claim means (never shown as a proof of correctness).
    pub statement: &'static str,
}

/// **Everything public about one claim** (nothing private exists in the route: every field is chain state): its lifecycle, the
/// public record a fresh verifier is built from, the positions served on chain, the open demands, and the interim assignment while
/// the Panel's receipts are being counted. `ledger_root` and `aux_root` anchor it to the committed state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelClaimReadV1 {
    pub claim_id: Digest,
    /// `program` or `pipeline`.
    pub kind: &'static str,
    /// How the claim's class verifies and finalizes (RFC-0015): `PanelLicensed` or `OptimisticPublicVerification`.
    pub mode: &'static str,
    /// The OPV clock facts a fresh verifier plans by (`None` for a Panel-licensed claim).
    pub opv: Option<KernelOpvReadV1>,
    /// The lifecycle state, spelled out (`Final { final_daa: 7 }`, `Convicted { daa: 9 }`, ...).
    pub state: String,
    pub final_daa: Option<u64>,
    pub convicted: bool,
    pub rewarded: bool,
    pub reserved: u64,
    pub committed_daa: u64,
    pub liability_until: Option<u64>,
    pub producer_bond: Digest,
    pub job_id: Digest,
    pub class_id: Digest,
    /// `PublicClaimRecordV1::to_bytes` (program claims) or `PipelinePublicRecordV1::to_bytes`.
    pub public_record: Vec<u8>,
    /// Borsh of the class header (`EvidenceHeaderV1`) a program claim's verifier is built with (empty for a pipeline claim).
    pub record_header: Vec<u8>,
    /// `(stage, position, borsh ServedPositionV1)`.
    pub served: Vec<(u8, u32, Vec<u8>)>,
    pub demands: Vec<KernelDemandReadV1>,
    /// The interim assignment (`None` once the claim is covered or ended).
    pub seats: Vec<(PalwBondKeyV2, Digest)>,
    pub quorum: u8,
    pub assignment_deadline_daa: u64,
    pub receipts_counted: u32,
    pub ledger_root: Hash64,
    pub aux_root: Hash64,
}

/// One page of the route's rows, in `(table, key)` order (ledger tables first, then the consensus tables).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelRowsPageV1 {
    pub rows: Vec<(u8, Vec<u8>, Vec<u8>)>,
    /// The cursor to resume after (`None` at the end).
    pub next: Option<(u8, Vec<u8>)>,
    pub total_rows: u64,
}

/// One Final of the route, for a reader and for the beacon: the kernel's receipt, how the work reached Final, and — where the route
/// knows every fact the beacon needs — the borsh `WorkFinalEventV1` (RFC-0010's `BeaconFactSource` input).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelFinalReadV1 {
    pub receipt: misaka_palw_kernel::opv::FinalReceiptV1,
    /// `PanelIndependent` (an OPV Final, never anything else) or `PanelLicensed` (the interim route does not know the licensing
    /// Panel's seed and epoch, so it exports no beacon event for it).
    pub final_path: &'static str,
    /// Borsh of `misaka_palw_challenge::WorkFinalEventV1`; `None` for a Panel-licensed Final.
    pub event: Option<Vec<u8>>,
    pub statement: &'static str,
}

impl PalwKernelRouteStateV1 {
    /// **Every Final the ledger holds**, in the kernel's canonical order, each with the beacon's event where the route can state it
    /// (RFC-0015: an OPV Final is `FinalPathV1::PanelIndependent`). Positions are DAA scores; the earliest acceptance of a work
    /// identity is the earliest commitment of any claim of the same job (a convicted or defaulted earlier holder counts).
    pub fn finals_read_v1(&self) -> Result<Vec<KernelFinalReadV1>, String> {
        use misaka_palw_kernel::mode::VerificationModeV1;
        use misaka_palw_kernel::opv::WorkFinalContextV1;
        let ledger = self.ledger()?;
        let mut out = Vec::new();
        for receipt in ledger.final_receipts() {
            let work = ledger.claims.get(&receipt.claim).map(|r| (r.class_binding_id, r.job_id));
            let earliest = ledger
                .claims
                .values()
                .filter(|row| Some((row.class_binding_id, row.job_id)) == work)
                .map(|row| row.committed_daa)
                .min()
                .unwrap_or(receipt.accepted_daa);
            let (final_path, event) = match receipt.mode {
                VerificationModeV1::OptimisticPublicVerification => {
                    let ctx = WorkFinalContextV1 {
                        accepted_position: earliest,
                        settlement_position: receipt.final_daa,
                        occurrence_index: 0,
                        validity_independent: true,
                        depends_on_profiles: Vec::new(),
                        panel: None,
                    };
                    let event = receipt.to_work_final_event(&ctx)?;
                    ("PanelIndependent", Some(borsh::to_vec(&event).map_err(|e| e.to_string())?))
                }
                VerificationModeV1::PanelLicensed => ("PanelLicensed", None),
            };
            let statement = receipt.assurance.statement();
            out.push(KernelFinalReadV1 { receipt, final_path, event, statement });
        }
        Ok(out)
    }

    /// **The public read of one claim** (see [`KernelClaimReadV1`]); `Ok(None)` for an unknown claim, `Err` only if the stored rows do
    /// not rebuild (corruption, not an input).
    pub fn claim_read_v1(&self, claim: &Digest) -> Result<Option<KernelClaimReadV1>, String> {
        let ledger = self.ledger()?;
        let Some(row) = ledger.claims.get(claim) else { return Ok(None) };
        let (kind, public_record, record_header) = match &row.body {
            misaka_palw_kernel::ledger::ClaimBodyV1::Program { .. } => {
                let (record, header) = ledger.public_record(claim).ok_or("a stored program claim has no public record")?;
                ("program", record.to_bytes(), borsh::to_vec(&header).map_err(|e| e.to_string())?)
            }
            misaka_palw_kernel::ledger::ClaimBodyV1::Pipeline { .. } => {
                let (record, _header, _binding) =
                    ledger.pipeline_public_record(claim).ok_or("a stored pipeline claim has no public record")?;
                // A pipeline header has no wire form of its own: a reader rebuilds it from the class record.
                ("pipeline", record.to_bytes(), Vec::new())
            }
            // RFC-0004 Part II: the typed claim's stored body is its public record, and the class's specification its header.
            misaka_palw_kernel::ledger::ClaimBodyV1::Spec(body) => {
                let spec = ledger.typed.classes.get(&row.class_binding_id).ok_or("a stored typed claim has no class")?;
                (
                    body.claim.kind(),
                    borsh::to_vec(body).map_err(|e| e.to_string())?,
                    borsh::to_vec(&spec.spec).map_err(|e| e.to_string())?,
                )
            }
        };
        let mut served = Vec::new();
        for ((c, stage, position), sp) in ledger.served.iter() {
            if c == claim {
                served.push((*stage, *position, borsh::to_vec(sp).map_err(|e| e.to_string())?));
            }
        }
        let demands = ledger
            .demands
            .iter()
            .filter(|((c, ..), _)| c == claim)
            .map(|((_, stage, position), d)| KernelDemandReadV1 {
                stage: *stage,
                position: *position,
                demanders: d.demanders.len() as u32,
                filed_daa: d.filed_daa,
                deadline_daa: d.deadline_daa,
                last_rejection: d.last_class().map(str::to_string),
            })
            .collect();
        let assignment = self.assignment_of(claim);
        let final_daa = match row.life.state {
            misaka_palw_kernel::lifecycle::ClaimStateV1::Final { final_daa } => Some(final_daa),
            _ => None,
        };
        let opv = ledger.opv_claim_view(claim).map(|v| KernelOpvReadV1 {
            admitted_daa: v.admitted_daa,
            verifier_start_cutoff_daa: v.verifier_start_cutoff_daa,
            final_floor_daa: v.final_floor_daa,
            hard_deadline_daa: v.hard_deadline_daa,
            reservation: v.reservation,
            max_gain: v.max_gain,
            statement: v.assurance.statement(),
        });
        Ok(Some(KernelClaimReadV1 {
            claim_id: *claim,
            kind,
            mode: ledger.mode_of_class(&row.class_binding_id).name(),
            opv,
            state: format!("{:?}", row.life.state),
            final_daa,
            convicted: row.convicted,
            rewarded: row.rewarded,
            reserved: row.reserved,
            committed_daa: row.committed_daa,
            liability_until: row.liability_until,
            producer_bond: row.producer,
            job_id: row.job_id,
            class_id: row.class_binding_id,
            public_record,
            record_header,
            served,
            demands,
            seats: assignment.as_ref().map(|a| a.seats.iter().map(|s| (s.bond, s.kernel_bond)).collect()).unwrap_or_default(),
            quorum: assignment.as_ref().map(|a| a.quorum).unwrap_or(0),
            assignment_deadline_daa: assignment.as_ref().map(|a| a.deadline_daa).unwrap_or(0),
            receipts_counted: self.receipts_of(claim).len() as u32,
            ledger_root: self.ledger_root(),
            aux_root: self.aux_root(),
        }))
    }

    /// **A page of rows**, `(table, key, row)` in order, starting after `after` (exclusive; `None` = the beginning) and stopping once
    /// `max_bytes` of keys and rows are gathered (at least one row, so a page always makes progress). A reader that collects every page
    /// and rebuilds a ledger from them must reach [`Self::ledger_root`].
    pub fn rows_page_v1(&self, after: Option<(u8, Vec<u8>)>, max_bytes: usize) -> KernelRowsPageV1 {
        use std::ops::Bound::{Excluded, Unbounded};
        let total_rows = (self.rows.len() + self.aux.len()) as u64;
        let start = match after {
            Some(cursor) => Excluded(cursor),
            None => Unbounded,
        };
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        let mut next = None;
        // The ledger tables are numbered below the consensus tables, so the two maps concatenate in key order.
        let all = self.rows.range((start.clone(), Unbounded)).chain(self.aux.range((start, Unbounded)));
        for ((table, key), row) in all {
            if !rows.is_empty() && bytes + key.len() + row.len() > max_bytes {
                break;
            }
            bytes += key.len() + row.len();
            rows.push((*table, key.clone(), row.clone()));
            next = Some((*table, key.clone()));
        }
        // `next` is a resume point only if something follows it.
        if let Some(cursor) = &next {
            let after_last = self.rows.range((Excluded(cursor.clone()), Unbounded)).next().is_some()
                || self.aux.range((Excluded(cursor.clone()), Unbounded)).next().is_some();
            if !after_last {
                next = None;
            }
        }
        KernelRowsPageV1 { rows, next, total_rows }
    }
}

// ---- the route's own chunk lane (C4 F-C4R3-03; tag 113, aux table 41) --------------------------------------------------------------

/// **What a chunk group serves** — fixed at its first chunk, and what its TTL is bounded by: a group never outlives the last DAA at
/// which its object could still matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwKernelChunkTargetV1 {
    /// A kernel claim: the assembled object is a `FileProof` against it or a `Respond` to a demand on it (tag 110).
    Claim(Digest) = 0,
    /// An onboarding artifact binding: the assembled object is its refutation (tag 105).
    Binding { v2_class: Hash64, kernel_param_root: Hash64 } = 1,
    /// **OPV-BOOT #1: the conformance attempt of V2 class `v2_class`** — the assembled object is that class's conformance evidence
    /// (tag 109: a Post by the class's registrant, opening the group itself, or a Refute by anyone), applied by its own arm. Its
    /// deadline is OPV-BOOT's `palw_conformance_chunk_target_v1(route, v2_class, daa)` (the agreed signature: the attempt's evidence
    /// deadline for a Post, its refutation window's end for a Refute; `None` when no attempt is open).
    Conformance { v2_class: Hash64 } = 2,
}

/// **One chunk of a prosecution object in the route's own lane** (the body of `KernelRouteChunkV1`, tag 113). `group` is
/// `palw_object_chunk_group_id_v1` of the assembled object's Borsh (a `PalwConsensusObjectV2`), the opener's bond signs every chunk
/// ([`palw_kernel_chunk_message_v1`]), and `target`/`count` are fixed by the group's first chunk.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelChunkV1 {
    pub opener: PalwBondKeyV2,
    pub group: Hash64,
    pub target: PalwKernelChunkTargetV1,
    pub index: u8,
    pub count: u8,
    pub bytes: Vec<u8>,
}

/// **A half-assembled group** (aux table 41): its target, its declared part count, its clock and the deposit its opener posted.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwKernelChunkGroupV1 {
    pub target: PalwKernelChunkTargetV1,
    pub count: u8,
    pub opened_daa: u64,
    /// The last DAA a part may arrive: `min(opened + PALW_KERNEL_CHUNK_TTL_MAX_DAA_V1, the target's deadline)`. Past it the group is
    /// dropped and its deposit forfeited.
    pub expires_daa: u64,
    pub deposit: u64,
    pub parts: BTreeMap<u8, Vec<u8>>,
}

/// **The message a chunk's opener signs**: `H(domain; network ‖ len ‖ borsh(chunk))` — every field, the target and the bytes included.
pub fn palw_kernel_chunk_message_v1(network_domain: Hash64, chunk: &PalwKernelChunkV1) -> Hash64 {
    let bytes = borsh::to_vec(chunk).expect("a chunk serializes");
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_KERNEL_CHUNK_DOMAIN_V1).to_state();
    s.update(network_domain.as_byte_slice());
    s.update(&(bytes.len() as u64).to_le_bytes());
    s.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(s.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// A group's row key: `borsh((opener, group))`.
pub fn palw_kernel_chunk_group_key_v1(opener: &PalwBondKeyV2, group: &Hash64) -> Vec<u8> {
    borsh::to_vec(&(*opener, *group)).expect("a bond key and a digest serialize")
}

/// **Cut `object` into the route lane's chunks** of at most `cap` bytes (unsigned: the opener signs each). At least one chunk, even
/// when the object fits one carrier; refused past `PALW_OBJECT_CHUNK_MAX_COUNT` chunks or a cap past one carrier.
pub fn palw_kernel_chunks_v1(
    object: &crate::palw_state_v2::PalwConsensusObjectV2,
    opener: PalwBondKeyV2,
    target: PalwKernelChunkTargetV1,
    cap: usize,
) -> Result<Vec<PalwKernelChunkV1>, String> {
    use crate::palw_state_v2::{PALW_OBJECT_CHUNK_MAX_BYTES, PALW_OBJECT_CHUNK_MAX_COUNT, palw_object_chunk_group_id_v1};
    if cap == 0 || cap > PALW_OBJECT_CHUNK_MAX_BYTES {
        return Err(format!("a chunk holds 1..={PALW_OBJECT_CHUNK_MAX_BYTES} bytes"));
    }
    let bytes = borsh::to_vec(object).map_err(|e| e.to_string())?;
    let count = bytes.len().div_ceil(cap).max(1);
    if count > PALW_OBJECT_CHUNK_MAX_COUNT as usize {
        return Err(format!("{} bytes need {count} chunks of {cap}, past {PALW_OBJECT_CHUNK_MAX_COUNT}", bytes.len()));
    }
    let group = palw_object_chunk_group_id_v1(&bytes);
    Ok(bytes
        .chunks(cap)
        .enumerate()
        .map(|(i, part)| PalwKernelChunkV1 { opener, group, target, index: i as u8, count: count as u8, bytes: part.to_vec() })
        .collect())
}

/// **Is the assembled object a prosecution of the group's target?** A `KernelRouteV1` whose strictly decoded kernel object is a
/// `FileProof` against, or a `Respond` to a demand on, the target claim; a refutation (tag 105) of the target binding. Nothing else
/// rides this lane (other objects keep the certification lane's `ObjectChunk`).
pub fn palw_kernel_chunk_inner_matches_target_v1(
    inner: &crate::palw_state_v2::PalwConsensusObjectV2,
    target: &PalwKernelChunkTargetV1,
) -> bool {
    use crate::palw_state_v2::PalwConsensusObjectV2 as Obj;
    use misaka_palw_kernel::route::KernelRouteObjectV1 as K;
    match (inner, target) {
        (Obj::KernelRouteV1 { bytes, .. }, PalwKernelChunkTargetV1::Claim(id)) => {
            matches!(K::decode(bytes), Ok(K::FileProof { claim, .. } | K::Respond { claim, .. }) if claim == *id)
        }
        (
            Obj::ArtifactBindingChallengedV1 { v2_class, kernel_param_root, .. },
            PalwKernelChunkTargetV1::Binding { v2_class: class, kernel_param_root: root },
        ) => v2_class == class && kernel_param_root == root,
        (Obj::ConformanceEvidenceV1 { v2_class, .. }, PalwKernelChunkTargetV1::Conformance { v2_class: class }) => v2_class == class,
        _ => false,
    }
}

/// **OPV-BOOT #1's deadline, until OPV-BOOT's own lands**: the agreed `palw_conformance_chunk_target_v1(route, v2_class, daa) ->
/// Option<u64>` (the last DAA a part may arrive; `None`: no attempt open) is OPV-BOOT's to implement over its attempt rows. Until it
/// is wired here this answers `None`, so every conformance group is refused at its first chunk — the lane adds no acceptance before
/// the rule that bounds it exists. (The integrator replaces this body with the call.)
pub fn palw_conformance_chunk_target_pending_v1(route: &PalwKernelRouteStateV1, v2_class: &Hash64, daa: u64) -> Option<u64> {
    let _ = (route, v2_class, daa);
    None
}

/// **The last DAA a proof against (or a response on) a claim in `state` could still matter** — an upper bound read from the claim
/// row and the policy: its liability horizon once Final or defaulted; before that, the latest Final its clock allows plus the
/// liability horizon. `None`: the claim is decided (convicted, timed out) and nothing it receives can change that.
pub fn palw_kernel_claim_horizon_bound_v1(
    policy: &LedgerPolicyV1,
    committed_daa: u64,
    liability_until: Option<u64>,
    state: &misaka_palw_kernel::lifecycle::ClaimStateV1,
) -> Option<u64> {
    use misaka_palw_kernel::lifecycle::ClaimStateV1 as S;
    let tail = policy.court_deadline_daa.saturating_add(policy.proof_grace_daa).saturating_add(policy.liability_daa);
    match state {
        S::Final { .. } | S::Unavailable { .. } => liability_until,
        S::TimedOut { .. } | S::Convicted { .. } => None,
        S::Checking { deadline_daa, .. } => Some(deadline_daa.saturating_add(policy.challenge_window_daa).saturating_add(tail)),
        S::ProbabilisticPass { window_end_daa, .. } | S::Challengeable { window_end_daa, .. } | S::WindowClosed { window_end_daa } => {
            Some(window_end_daa.saturating_add(tail))
        }
        S::Disputed { resume, .. } => palw_kernel_claim_horizon_bound_v1(policy, committed_daa, liability_until, resume),
        S::Committed | S::ChallengeBound { .. } => Some(
            committed_daa.saturating_add(policy.check_window_daa).saturating_add(policy.challenge_window_daa).saturating_add(tail),
        ),
    }
}

impl PalwKernelRouteStateV1 {
    /// One group of the route's chunk lane.
    pub fn chunk_group_v1(&self, opener: &PalwBondKeyV2, group: &Hash64) -> Option<PalwKernelChunkGroupV1> {
        self.aux_row(PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1, &palw_kernel_chunk_group_key_v1(opener, group))
    }

    /// Every half-assembled group, `(opener, group, row)`, in key order.
    pub fn chunk_groups_v1(&self) -> Vec<(PalwBondKeyV2, Hash64, PalwKernelChunkGroupV1)> {
        let t = PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1;
        self.aux
            .range((t, Vec::new())..(t + 1, Vec::new()))
            .filter_map(|((_, key), row)| {
                let (opener, group) = borsh::from_slice::<(PalwBondKeyV2, Hash64)>(key).ok()?;
                Some((opener, group, borsh::from_slice::<PalwKernelChunkGroupV1>(row).ok()?))
            })
            .collect()
    }

    /// How many groups `opener` holds open.
    pub fn chunk_groups_of_v1(&self, opener: &PalwBondKeyV2) -> usize {
        self.chunk_groups_v1().iter().filter(|(o, _, _)| o == opener).count()
    }

    /// The deposits `bond`'s open groups hold against its collateral (part of what V2's committed-collateral ledger and both
    /// withdrawal gates read as the route's reservation).
    pub fn chunk_deposits_of_v1(&self, bond: &PalwBondKeyV2) -> u64 {
        self.chunk_groups_v1().iter().filter(|(o, _, _)| o == bond).fold(0u64, |acc, (_, _, g)| acc.saturating_add(g.deposit))
    }

    /// **The last DAA at which an object for `target` could still change anything** (`None`: nothing can — the claim is decided or
    /// unknown, the binding refuted or unknown). A group's TTL is never longer.
    pub fn chunk_target_deadline_v1(&self, target: &PalwKernelChunkTargetV1, daa: u64) -> Option<u64> {
        match target {
            PalwKernelChunkTargetV1::Claim(id) => {
                let key = borsh::to_vec(id).expect("a digest serializes");
                let row: misaka_palw_kernel::ledger::ClaimRowV1 =
                    borsh::from_slice(self.rows.get(&(misaka_palw_kernel::rows::TABLE_CLAIMS_V1, key))?).ok()?;
                if row.convicted {
                    return None;
                }
                palw_kernel_claim_horizon_bound_v1(&self.header.policy, row.committed_daa, row.liability_until, &row.life.state)
            }
            PalwKernelChunkTargetV1::Binding { v2_class, kernel_param_root } => {
                let row = self.artifact_binding_v1(v2_class, kernel_param_root)?;
                // Refutable while `daa < final_daa`.
                (!row.refuted).then(|| row.final_daa.saturating_sub(1))
            }
            PalwKernelChunkTargetV1::Conformance { v2_class } => palw_conformance_chunk_target_pending_v1(self, v2_class, daa),
        }
    }

    /// **The object `chunk` completes, if it completes its group**: every part present (the stored ones and this one), coherent with
    /// the stored group, the assembled bytes hashing to the group id and decoding to one object. `None` otherwise — the fold refuses
    /// what this cannot assemble. (The acceptance layer checks the assembled object's own signature at this chunk.)
    pub fn chunk_completion_v1(&self, chunk: &PalwKernelChunkV1) -> Option<crate::palw_state_v2::PalwConsensusObjectV2> {
        if chunk.count == 0 || chunk.index >= chunk.count || chunk.bytes.is_empty() {
            return None;
        }
        let stored = self.chunk_group_v1(&chunk.opener, &chunk.group);
        let mut parts: BTreeMap<u8, &[u8]> = BTreeMap::new();
        if let Some(stored) = &stored {
            if stored.count != chunk.count || stored.target != chunk.target || stored.parts.contains_key(&chunk.index) {
                return None;
            }
            parts.extend(stored.parts.iter().map(|(i, p)| (*i, p.as_slice())));
        }
        parts.insert(chunk.index, chunk.bytes.as_slice());
        if parts.len() != chunk.count as usize {
            return None;
        }
        let mut whole = Vec::with_capacity(parts.values().map(|p| p.len()).sum());
        for i in 0..chunk.count {
            whole.extend_from_slice(parts.get(&i)?);
        }
        if crate::palw_state_v2::palw_object_chunk_group_id_v1(&whole) != chunk.group {
            return None;
        }
        borsh::from_slice(&whole).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **C4 F-C4R4-16 (ADR-0032 49 %)**: the route's interim accuser share — and so the demanders' share of a default and the
    /// onboarding challenger's — is at most 490‰; a self-reporter keeps at most 49 % of a slash, a net loss of at least 51 %.
    #[test]
    fn palw_kernel_route_the_interim_reporter_share_is_adr_0032s_49_percent() {
        let p = palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        assert_eq!(p.accuser_reward_permille, 490);
        assert_eq!(crate::palw_onboarding_v1::PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1, 490);
        let kept = p.claim_collateral - p.claim_collateral * u64::from(p.accuser_reward_permille) / 1000;
        assert!(kept * 100 >= p.claim_collateral * 51, "net loss {kept} of {}", p.claim_collateral);
    }

    #[test]
    fn the_interim_policy_validates_and_the_bond_digest_separates_indices() {
        let p = palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        p.validate().unwrap();
        let b = |i| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(9), i));
        assert_ne!(palw_kernel_bond_id_v1(&b(0)), palw_kernel_bond_id_v1(&b(1)));
        let state = PalwKernelRouteStateV1::new(p, None, LedgerScalarsV1::default());
        assert_eq!(state.ledger().unwrap().root(), state.ledger_root().as_bytes(), "an empty state roots like an empty ledger");
    }

    #[test]
    fn the_interim_opv_policy_validates_against_the_interim_ledger_policy_and_roots_like_its_ledger() {
        let p = palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        let opv = crate::palw_panel_free_v1::PalwPanelFreeFenceV1::at(crate::config::params::ForkActivation::new(7)).opv_policy();
        opv.validate(&p).unwrap();
        // The test node's four-adjudication block must still satisfy the OPV relations.
        let mut small = p;
        small.max_adjudications_per_block = 4;
        opv.validate(&small).unwrap();
        let state = PalwKernelRouteStateV1::new(p, Some(opv), LedgerScalarsV1::default());
        assert_eq!(
            state.ledger().unwrap().root(),
            state.ledger_root().as_bytes(),
            "an empty OPV state roots like an empty OPV ledger"
        );
        let plain = PalwKernelRouteStateV1::new(p, None, LedgerScalarsV1::default());
        assert_ne!(state.ledger_root(), plain.ledger_root(), "the OPV root form is not the historical one");
        // OPV-BOOT GAP-B1a: with tables 25 and 26 empty there is no root extension — both forms are what they were before them.
        let (opv_ledger, plain_ledger) = (state.ledger().unwrap(), plain.ledger().unwrap());
        assert!(
            opv_ledger.claim_beacon_salts.is_empty()
                && opv_ledger.forfeited_claim_seals.is_empty()
                && opv_ledger.job_posters.is_empty()
        );
        assert_eq!(state.ledger_root().as_bytes(), opv_ledger.root_parts_v2().root(), "the OPV form, unextended");
        assert_eq!(plain.ledger_root().as_bytes(), plain_ledger.root_parts().root(), "the historical form, unextended");
        // The v3 beacon's window against the interim seal TTL: OPV-BOOT's interim W = 40 needs 2·W ≤ 100.
        assert!(misaka_palw_kernel::ledger::seal_ttl_admits_beacon_window_v1(&p, 40));
        assert!(!misaka_palw_kernel::ledger::seal_ttl_admits_beacon_window_v1(&p, p.seal_ttl_daa / 2 + 1));
    }

    /// **RFC-0004 Part II**: an unarmed route (no `palw_typed_roots_v1`) has the historical schedule, `config_root` and root byte for
    /// byte; an armed one schedules `K2-TR-v1` from the fence's height and is another configuration.
    #[test]
    fn the_unarmed_typed_roots_fence_leaves_the_schedule_config_root_and_root_unchanged() {
        let p = palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        let opv = crate::palw_panel_free_v1::PalwPanelFreeFenceV1::at(crate::config::params::ForkActivation::new(7)).opv_policy();
        for o in [None, Some(opv)] {
            let historical = palw_kernel_route_template_opv_v1(p, o);
            let unarmed = PalwKernelRouteStateV1::new(p, o, LedgerScalarsV1::default());
            assert_eq!(unarmed.header.typed_roots, None);
            assert_eq!(
                unarmed.header.config_root,
                config_root_of(&historical.schedule, &historical.known),
                "the historical config root"
            );
            assert_eq!(unarmed.template().schedule, historical.schedule, "the historical schedule");
            assert_eq!(unarmed.ledger().unwrap().root(), historical.root(), "the historical root");
            let armed = PalwKernelRouteStateV1::new_typed(p, o, Some(11), LedgerScalarsV1::default());
            assert_ne!(armed.header.config_root, unarmed.header.config_root, "arming is another configuration");
            let l = armed.ledger().unwrap();
            assert_eq!(armed.ledger_root().as_bytes(), l.root(), "an armed state roots like its ledger");
            let tr = misaka_palw_kernel::spec::k2_tr_v1_descriptor().digest();
            assert_eq!(
                l.schedule.standing_at(&tr, 10),
                misaka_palw_kernel::descriptor::KernelStandingV1::NotActive(KernelStatusV1::Active { since_daa: 11 })
            );
            assert_eq!(l.schedule.standing_at(&tr, 11), misaka_palw_kernel::descriptor::KernelStandingV1::Active);
        }
    }

    #[test]
    fn the_tail_and_tables_are_the_allocated_ones() {
        assert_eq!(PALW_CARRIAGE_KERNEL_ROUTE_TAIL_V1, 0xEC);
        assert_eq!(
            (PALW_KERNEL_ROUTE_TABLE_BOND_KEYS_V1, PALW_KERNEL_ROUTE_TABLE_ASSIGNMENTS_V1, PALW_KERNEL_ROUTE_TABLE_RECEIPTS_V1),
            (32, 33, 34)
        );
        assert_eq!(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, 35);
        assert!(PALW_KERNEL_ROUTE_MAX_OBJECT_BYTES_V1 > 1_000_000);
        // C4 F-C4R3-03: the route's own chunk lane (the lead's allocation of 2026-10-08), above onboarding's 36–40.
        assert_eq!((PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1, PALW_KERNEL_ROUTE_TABLE_RESERVED_42_V1), (41, 42));
    }

    fn chunk_bond(i: u32) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(77), i))
    }

    /// **C4 F-C4R3-03 (the lane's pure half)**: the splitter, the completion (every part, the group id, coherence with the stored
    /// group), the per-opener room, the deposits that join the route's reservation, the target check and the target's deadline.
    #[test]
    fn the_routes_chunk_lane_assembles_only_coherent_groups_and_bounds_them_by_their_target() {
        use crate::palw_state_v2::PalwConsensusObjectV2 as Obj;
        use misaka_palw_kernel::route::{KernelRouteObjectV1 as K, ProsecutionV1};
        let p = palw_kernel_route_policy_v1(Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        let mut state = PalwKernelRouteStateV1::new(p, None, LedgerScalarsV1::default());
        let (opener, other) = (chunk_bond(0), chunk_bond(1));
        let claim = [7u8; 64];
        let proof = K::FileProof { accuser: [1; 64], claim, proof: ProsecutionV1::Kernel(vec![9; 3000]) };
        let inner = Obj::KernelRouteV1 { bytes: proof.encode(), signer: opener, signature: vec![5; 64] };
        let target = PalwKernelChunkTargetV1::Claim(claim);
        let chunks = palw_kernel_chunks_v1(&inner, opener, target, 1024).unwrap();
        assert!(chunks.len() >= 3);
        assert!(palw_kernel_chunks_v1(&inner, opener, target, 64).is_err(), "past the chunk count");
        assert!(palw_kernel_chunk_inner_matches_target_v1(&inner, &target));
        assert!(!palw_kernel_chunk_inner_matches_target_v1(&inner, &PalwKernelChunkTargetV1::Claim([8; 64])), "another claim");
        let demand = Obj::KernelRouteV1 {
            bytes: K::FileDemand { demander: [1; 64], claim, stage: 0, position: 0 }.encode(),
            signer: opener,
            signature: vec![5; 64],
        };
        assert!(!palw_kernel_chunk_inner_matches_target_v1(&demand, &target), "a demand is not a prosecution object of this lane");
        // A lone last chunk completes nothing; with the others stored it completes exactly the object.
        let last = chunks.last().unwrap().clone();
        assert!(state.chunk_completion_v1(&last).is_none());
        let mut group =
            PalwKernelChunkGroupV1 { target, count: last.count, opened_daa: 1, expires_daa: 9, deposit: 3, parts: BTreeMap::new() };
        for c in &chunks[..chunks.len() - 1] {
            group.parts.insert(c.index, c.bytes.clone());
        }
        state.aux.insert(
            (PALW_KERNEL_ROUTE_TABLE_CHUNK_GROUPS_V1, palw_kernel_chunk_group_key_v1(&opener, &last.group)),
            borsh::to_vec(&group).unwrap(),
        );
        assert_eq!(state.chunk_completion_v1(&last), Some(inner.clone()));
        // The same bytes under another opener are another group (rooms are per bond); another target or count is incoherent.
        assert!(state.chunk_completion_v1(&PalwKernelChunkV1 { opener: other, ..last.clone() }).is_none());
        assert!(
            state
                .chunk_completion_v1(&PalwKernelChunkV1 { target: PalwKernelChunkTargetV1::Claim([8; 64]), ..last.clone() })
                .is_none()
        );
        let mut tampered = last.clone();
        tampered.bytes[0] ^= 1;
        assert!(state.chunk_completion_v1(&tampered).is_none(), "the assembled bytes must hash to the group id");
        // The open group is the opener's room and its deposit is part of what the route holds against the opener.
        assert_eq!((state.chunk_groups_of_v1(&opener), state.chunk_groups_of_v1(&other)), (1, 0));
        assert_eq!((state.reserved_of(&opener), state.ledger_reserved_of(&opener)), (3, 0));
        // An unknown claim and a refuted / unknown binding have no deadline; a claim's is its horizon bound.
        assert_eq!(state.chunk_target_deadline_v1(&target, 0), None);
        let binding =
            PalwKernelChunkTargetV1::Binding { v2_class: Hash64::from_u64_word(3), kernel_param_root: Hash64::from_u64_word(4) };
        assert_eq!(state.chunk_target_deadline_v1(&binding, 0), None);
        // OPV-BOOT #1: a conformance attempt's deadline is OPV-BOOT's (pending: no attempt is ever open here yet).
        let conformance = PalwKernelChunkTargetV1::Conformance { v2_class: Hash64::from_u64_word(3) };
        assert_eq!(state.chunk_target_deadline_v1(&conformance, 0), None);
        assert!(!palw_kernel_chunk_inner_matches_target_v1(&inner, &conformance), "a proof is not conformance evidence");
        {
            use crate::palw_conformance_evidence_v1::{ConformanceEvidenceActionV1 as A, ConformanceFaultV1 as F};
            let refute = |class: u64| Obj::ConformanceEvidenceV1 {
                v2_class: Hash64::from_u64_word(class),
                action: Box::new(A::Refute {
                    evidence_id: Hash64::from_u64_word(9),
                    fault: Box::new(F::VectorTokens { check: 0, kernel_claim: Hash64::from_u64_word(5) }),
                }),
                signer: other,
                signature: vec![5; 64],
            };
            assert!(palw_kernel_chunk_inner_matches_target_v1(&refute(3), &conformance), "the class's evidence");
            assert!(!palw_kernel_chunk_inner_matches_target_v1(&refute(4), &conformance), "another class's evidence");
            assert!(!palw_kernel_chunk_inner_matches_target_v1(&refute(3), &binding), "evidence is not a binding's refutation");
            assert!(!palw_kernel_chunk_inner_matches_target_v1(&refute(3), &target), "evidence is not a claim's prosecution");
        }
        use misaka_palw_kernel::lifecycle::ClaimStateV1 as S;
        let bound = |state: &S, liability: Option<u64>| palw_kernel_claim_horizon_bound_v1(&p, 10, liability, state);
        let tail = p.court_deadline_daa + p.proof_grace_daa + p.liability_daa;
        let pass = S::ProbabilisticPass { passed_daa: 20, window_end_daa: 70 };
        assert_eq!(bound(&pass, None), Some(70 + tail), "a passed claim: its latest Final plus the liability horizon");
        assert_eq!(bound(&S::Disputed { open: 1, resume: Box::new(pass) }, None), Some(70 + tail));
        assert_eq!(bound(&S::Checking { anchor_daa: 10, deadline_daa: 110 }, None), Some(110 + p.challenge_window_daa + tail));
        assert_eq!(
            bound(&S::Unavailable { daa: 40, producer_defaulted: true }, Some(240)),
            Some(240),
            "a defaulted claim (F-C4R3-02)"
        );
        assert_eq!(bound(&S::Final { final_daa: 60 }, Some(260)), Some(260));
        assert_eq!(bound(&S::TimedOut { daa: 111 }, None), None, "a claim that ended without passing takes nothing more");
        assert_eq!(bound(&S::Convicted { daa: 30 }, None), None);
    }
}
