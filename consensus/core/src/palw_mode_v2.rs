//! `PalwConsensusMode` — one switch, or none (ADR-0042 Decision 1, PR-10), and the ruleset
//! fingerprint that makes RC == mainnet a hash a node checks (Decision 11).
//!
//! The five `Option` fences were a machine that invited half-activation: individually
//! flippable, discovered mid-audit to interlock in ways their docs had to warn about. On the V2
//! lineage a network is in exactly ONE mode, and `ConsensusV2` carries **all** of the ruleset or
//! none of it:
//!
//! * every sub-parameter block is constructor-validated (no `Default`s anywhere in the bundle);
//! * [`PalwConsensusParamsV2::validate`] holds the Decision 1 startup invariants — the checks a
//!   node runs before it dials a peer, failing which it does not boot;
//! * [`Params::validate_palw_v2`][crate::config::params::Params] (the config gate) additionally
//!   refuses a MIXED params set: a `ConsensusV2` network may not set any V1 fence and may not
//!   activate any V1 PALW PoW — there is no path that switches on half of two lineages.
//!
//! The fingerprint ([`palw_ruleset_id_v2`]) is the canonical hash of the whole bundle. Network
//! identity is deliberately NOT inside it — the challenge's `network_domain` carries that
//! (Decision 3a), so RC and mainnet can share one ruleset id while a testnet block still cannot
//! replay on mainnet. The P2P handshake exchanges it and drops a mismatched peer early; the
//! mainnet binary reads the RC's canonical ruleset bytes rather than a human re-typing numbers.
//!
//! What this module does NOT do: mint the RC genesis. The class/court catalog ROOTS are
//! committed here; the genesis that registers BASE-0 must hash to them, and the boot path that
//! loads it verifies the preimages and runs `verify_catalog_coverage_v1` against this build's
//! own catalog — those land with the RC genesis itself, which is a network artifact, not a
//! library one.

use crate::Hash64;
use crate::palw_admission_v2::{PalwAdmissionParamsV2, PalwAdmissionV2Error};
use crate::palw_freeprompt_v3::{PalwFpV3Error, PalwFreePromptParamsV3};
use crate::palw_panel_v2::PalwPanelParamsV2;
use crate::palw_reward_v2::PalwRewardParamsV2;
use crate::palw_state_v2::{PalwStateParamsV2, PalwStateV2Error};
use blake2b_simd::Params as Blake2bParams;
use std::collections::BTreeSet;

pub const PALW_RULESET_ID_V2_DOMAIN: &[u8] = b"misaka-palw/ruleset-id-v2/v1";

/// ADR-0038 Decision H: **the block cadence a V2 network runs at is frozen at 120 s**, and
/// Decision H says it is "refused at `Params` construction rather than at sync time".
///
/// It lives here, in the bundle's own module, because of audit H2: `palw_ruleset_id_v2` hashes
/// `PalwConsensusParamsV2` and nothing else, and the cadence was in neither. Every window in the
/// bundle — bind, receipt, challenge, court, the withdrawal delay, the epoch length — is
/// denominated in DAA score, so the cadence is what gives all of them their wall-clock meaning.
/// Two networks could therefore share a `palw_ruleset_id` and run measurably different rules,
/// which is exactly the "RC == mainnet, checkable by machine" promise Decision 11 exists to make.
/// [`crate::config::params::Params::validate_palw_v2`] refuses a V2 network that is not at this
/// cadence, so the id's silence about it is no longer a hole an operator can walk through.
pub const PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS: u64 = 120_000;

/// Version of the fork-choice ORDER a V2 network runs (`compare_palw_candidates_v1`'s key list
/// and their precedence). Decision 11 names it in the ruleset-id preimage; it was not in the
/// bundle, so two networks could share an id while ordering candidates differently — the one
/// disagreement that partitions a chain without either side being wrong about anything else.
pub const PALW_V2_FORK_CHOICE_VERSION: u16 = 1;

/// Version of the trace/step object format the court adjudicates against
/// (`PalwStepBindingV2` and the leaf encodings under it). Also named by Decision 11 and also
/// absent: a network that changed the trace format would still fingerprint identically.
///
/// **2** since `PalwExecutionStepRefutationV1` gained `kv_checkpoint`: a refutation may supply an
/// attention step's KV history as one committed checkpoint instead of one opening per cached
/// position. That changes the object's encoding AND what is adjudicable — a v1 court handed a v2
/// refutation would read the anchor as trailing bytes, and a v2 court accepts material a v1 court
/// would have called `InputSetNotCanonical`. Either way the two disagree about a verdict, which is
/// a fork, so the version is what makes them refuse each other at startup instead.
pub const PALW_V2_TRACE_FORMAT_VERSION: u16 = 2;

/// The ML-DSA-87 signing contexts the V2 ruleset uses, in a fixed order.
///
/// Decision 11 lists "signature_contexts" in the preimage, and this is the honest way to put
/// them there: not a version NUMBER a human bumps, but the bytes themselves. The bundle commits
/// to [`palw_v2_signature_contexts_root`] and the startup gate recomputes it from THIS BINARY'S
/// constants, so a build whose contexts differ from what the network committed to refuses to
/// run — the same shape as the algorithm-finalizer gate. Editing a context string here without
/// re-minting the ruleset id is a startup failure rather than a silent cross-family replay.
pub const PALW_V2_SIGNATURE_CONTEXTS: &[&[u8]] =
    &[crate::palw_attempt_v2::PALW_ATTEMPT_V2_MLDSA87_CONTEXT, crate::palw_panel_v2::PALW_RECEIPT_V2_MLDSA87_CONTEXT];

pub const PALW_V2_SIGNATURE_CONTEXTS_DOMAIN: &[u8] = b"misaka-palw/ruleset-id-v2/signature-contexts/v1";

/// `H(count ‖ (len ‖ context)*)` over [`PALW_V2_SIGNATURE_CONTEXTS`]. Length-prefixed per entry
/// so no two different context lists can concatenate to the same preimage.
pub fn palw_v2_signature_contexts_root() -> Hash64 {
    let mut state = Blake2bParams::new().hash_length(64).key(PALW_V2_SIGNATURE_CONTEXTS_DOMAIN).to_state();
    state.update(&(PALW_V2_SIGNATURE_CONTEXTS.len() as u64).to_le_bytes());
    for context in PALW_V2_SIGNATURE_CONTEXTS {
        state.update(&(context.len() as u64).to_le_bytes());
        state.update(context);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

pub const PALW_MODE_V2_ALL_DOMAINS: &[&[u8]] = &[PALW_RULESET_ID_V2_DOMAIN];

/// `H(network_id_bytes)` — the value every V2-lineage object binds as its `network_domain`
/// (ADR-0042 Decision 3a, ADR-0044).
///
/// **One derivation, re-exported, not a second one.** The integration found this concept defined
/// TWICE under two different domain keys — here and in `palw_attempt_v2` — with the live header
/// path reading this one and the envelope's own validation binding the other. Two derivations of
/// a consensus identity are two identities: a correctly-formed attempt would have been refused by
/// the validator that computed the domain the other way. `palw_attempt_v2` owns the derivation
/// (its domain key is the one the frozen challenge preimage already binds), and this path is that
/// function under the name the pipeline calls it by.
pub use crate::palw_attempt_v2::palw_network_domain_v2;

/// The court's shape, from which the worst-case honest prosecution is DERIVED (ADR-0042
/// Decision 8: `rounds = ceil(log2(max_step_leaf_count)) + terminal/opening rounds`).
///
/// This replaces a bare `worst_case_court_duration_daa` field. The audit's objection to that
/// field was not that it was wrong but that it was **self-certifying**: the one startup
/// invariant standing between the ruleset and an un-prosecutable fraud window compared
/// `window_court` against a number the same operator typed, with nothing deriving it. Splitting
/// it into the three quantities the ADR's formula actually takes does not make them
/// unfalsifiable — an operator can still understate `max_step_leaf_count` — but each is now a
/// NAMED fact with a place to be checked: the leaf count is a property of the registered class
/// catalog, which the RC genesis loader verifies against the catalog preimage, and the turn
/// deadline is a protocol constant a reader can compare against the ladder's own measurement.
/// **Sized from what a court close can be CARRIED in, not from a number that sounded generous** —
/// see below.
///
/// # What one court close may cost to carry, in bytes
///
/// ADR-0049 Decision C, restated to the quantity the ADR's own table names ("per refutation and
/// per court close ... plus Merkle paths") rather than to the weight bytes alone.
///
/// ## Why this is a carriage number and not a taste number
///
/// A close is one transaction on `SUBNETWORK_ID_PALW_LIFECYCLE`. There is no chunked-evidence path
/// for a `PalwConsensusObjectV2` — `palw_carriage`'s four-chunk envelope is the Stage-0 v1 format
/// and carries no V2 object — so whatever a close weighs, it weighs in one payload. Two rules then
/// decide the largest close that can exist:
///
/// * `transient_mass = size × TRANSIENT_BYTE_TO_MASS_FACTOR` (4), and the mempool refuses a
///   transaction whose transient mass exceeds `MAXIMUM_STANDARD_TRANSACTION_MASS` (480,000), so a
///   RELAYABLE transaction is at most **120,000 bytes**;
/// * body validation refuses a block whose total transient mass exceeds `max_block_mass`
///   (500,000), so even a hand-delivered one is at most 125,000 bytes.
///
/// A close that cannot be relayed is a dispute only a friendly miner could raise, so 120,000 is the
/// bound. Against it: a measured carrier (one ML-DSA-87 input and a change output) is 7,457 bytes
/// and a worst-case standard one is ~18,000; the encoded object runs about 1.2x the bytes this
/// ceiling counts, because every opening carries its own coordinate and length prefixes. So
///
/// ```text
/// ceiling x 1.20 + 18,000 <= 120,000   =>   ceiling <= 85,000
/// ```
///
/// and the value below is 80 KiB, the round number under it.
///
/// ## What a close is made of, measured on the shipped floor
///
/// Three arms, and the two that grow with the job are the ones a weight-bytes ceiling never saw:
///
/// | arm | grows with | floor at the old `n_ctx` 512 / vocab 4,096 |
/// |---|---|---|
/// | the artifact opening | tile x row | 32,768 |
/// | the disputed step's KV history | `n_ctx` — one step opening per position per ref, each with its own path; ADR-0030's checkpoint anchor makes this cheap, but no anchor exists at a prefill position or the first decode call | ~3 MB |
/// | the generated-token pin (Decision E) | `decode_tokens x vocabulary` — `base0_logits_trace_root_v1` is a flat hash, so no single row can be opened | 65,536 at the canonical job, 8 MB at the longest |
///
/// Measured whole closes, assembled from real executions before the anchor landed: 90,893 bytes at
/// `n_ctx` 16, 139,437 at 24, and **750,716 at the floor's own declared 64/64 job** — against a
/// derived 32,768. That gap is what this ceiling exists to make visible at admission instead of at
/// dispute time.
///
/// ## What it admits
///
/// `PALW_RC_BASE0_GEOMETRY` is `vocab_size` 1,024 and `n_ctx` 12 BECAUSE of this number: its worst
/// close is 61,040, or 75% of the ceiling. `n_ctx` 16 reaches 97% and 20 exceeds it; `vocab_size`
/// 2,048 exceeds it at every context. No Qwen2.5 geometry fits — its
/// cheapest artifact opening alone is 143,360 — so a larger ceiling would not buy that class, only
/// admit one whose disputes nobody could raise. Carrying a model at that scale needs bisection
/// WITHIN a step's reduction and an OPENABLE logits commitment, not a bigger number here.
///
/// ## It is frozen with the network
///
/// This is a `PalwConsensusParamsV2` field and therefore inside `palw_ruleset_id_v2`. A class that
/// exceeds it cannot join a running chain, so it is chosen once, at genesis, for every class the
/// network will ever admit — which is why `assemble_palw_rc_identity_v2` refuses an RC identity
/// carrying any other value.
pub const DEFAULT_MAX_CLOSE_BYTES: u64 = 80 * 1024;

/// **The mempool's standard-transaction mass, mirrored** — the number
/// [`DEFAULT_MAX_CLOSE_BYTES`] is derived from.
///
/// The constant itself is `mining::mempool::check_transaction_standard`'s and private to that
/// crate, which `kaspa-consensus-core` does not depend on. Mirroring it is a second source, so it
/// is guarded from the side that owns it: `the_palw_close_ceiling_mirror_is_the_real_limit` in that
/// module fails if the two ever differ. A mirror nobody compares is how a derived number quietly
/// stops being derived from anything.
pub const PALW_MIRRORED_STANDARD_TX_MASS: u64 = 480_000;

/// The largest STANDARD transaction, in bytes: transient mass is `size x 4`, and a transaction
/// whose transient mass exceeds the standard limit is refused before it is relayed.
pub const PALW_STANDARD_TX_BYTES: u64 = PALW_MIRRORED_STANDARD_TX_MASS / crate::constants::TRANSIENT_BYTE_TO_MASS_FACTOR;
/// The floor's widest step is 32,768 multiply-accumulates. This is 512 times that: generous for a
/// class of the floor's shape, and still milliseconds of scalar `int8` CPU, which is the budget the
/// court was always sized for. A close is bounded to 80 KiB of payload, and a `MatMulQuant`'s
/// multiply-accumulate count equals its opened weight bytes, so only a `KvScaled` node can
/// approach this at all — the floor's own widest is 131,072.
pub const DEFAULT_MAX_TERMINAL_MACS: u64 = 16 * 1024 * 1024;
/// A step reads its `input_refs` and at most one weight operand. Eight is well past any node in the
/// BASE-0 or Qwen graphs (the widest is the gated-delta-net recurrence's five rows; the floor's is
/// two).
pub const DEFAULT_MAX_OPERAND_COUNT: u32 = 8;

/// One opaque number becomes three checkable ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwCourtParamsV2 {
    /// Worst-case step-tree leaf count over every registered class — what the bisection ladder
    /// must be able to walk. A property of the class catalog, not of the operator's opinion.
    max_step_leaf_count: u64,
    /// DAA budget one bisection turn gets before the silent party defaults.
    turn_deadline_daa: u64,
    /// Rounds beyond the bisection itself: the terminal one-step adjudication and its opening.
    terminal_rounds: u32,
    /// **ADR-0049 Decision C: what one round may COST.** `max_step_leaf_count` bounds how many
    /// rounds a dispute takes and nothing bounded the size of a round, so the terminal
    /// adjudication's price was the model's — ~223 MiB of opening for Qwen2.5-1.5B's unembed
    /// against ADR-0046's 152 KB court-close budget. These three are compared against
    /// `palw_class_admission_v2::derive_court_cost_v1`, which reads them off a class's graph, and
    /// against the close that actually arrives (`palw_court_v2::check_close_cost_v2`).
    ///
    /// `max_close_bytes` counts **the whole close** — the artifact openings, the refutation's own
    /// step leaves, and every Merkle path element on both — because that is what has to fit in one
    /// transaction. It was named `max_opening_bytes` and counted the weight bytes alone, which
    /// understated the shipped floor's most expensive close by 23x: 32,768 derived against 750,716
    /// real, the difference being a KV history that the ceiling never looked at.
    ///
    /// They are here, inside `palw_ruleset_id_v2`, for the same reason the ladder is: a class that
    /// exceeds them cannot join a running chain, so the ceiling is chosen once, at genesis, for
    /// every class the network ever intends to admit.
    max_close_bytes: u64,
    max_terminal_macs: u64,
    max_operand_count: u32,
}

impl PalwCourtParamsV2 {
    /// The pre-Decision-C constructor, kept so existing callers state only what they meant to.
    /// It installs the DEFAULT cost ceilings, which are deliberately generous rather than absent:
    /// a zero ceiling would refuse every class, and no ceiling is what this ADR exists to end.
    pub fn new(max_step_leaf_count: u64, turn_deadline_daa: u64, terminal_rounds: u32) -> Result<Self, PalwModeV2Error> {
        Self::with_cost_ceilings(
            max_step_leaf_count,
            turn_deadline_daa,
            terminal_rounds,
            DEFAULT_MAX_CLOSE_BYTES,
            DEFAULT_MAX_TERMINAL_MACS,
            DEFAULT_MAX_OPERAND_COUNT,
        )
    }

    /// Every court parameter, including ADR-0049 Decision C's three cost ceilings.
    pub fn with_cost_ceilings(
        max_step_leaf_count: u64,
        turn_deadline_daa: u64,
        terminal_rounds: u32,
        max_close_bytes: u64,
        max_terminal_macs: u64,
        max_operand_count: u32,
    ) -> Result<Self, PalwModeV2Error> {
        if max_close_bytes == 0 || max_terminal_macs == 0 || max_operand_count == 0 {
            return Err(PalwModeV2Error::Invalid("a zero court cost ceiling admits no class at all"));
        }
        if max_step_leaf_count < 2 {
            return Err(PalwModeV2Error::Invalid("a trace with fewer than two step leaves cannot be bisected"));
        }
        if turn_deadline_daa == 0 {
            return Err(PalwModeV2Error::Invalid("a zero turn deadline defaults whichever party the block order reaches first"));
        }
        if terminal_rounds == 0 {
            return Err(PalwModeV2Error::Invalid("the terminal adjudication is a round; zero of them never reaches a verdict"));
        }
        Ok(Self { max_step_leaf_count, turn_deadline_daa, terminal_rounds, max_close_bytes, max_terminal_macs, max_operand_count })
    }

    pub fn max_close_bytes(&self) -> u64 {
        self.max_close_bytes
    }

    pub fn max_terminal_macs(&self) -> u64 {
        self.max_terminal_macs
    }

    pub fn max_operand_count(&self) -> u32 {
        self.max_operand_count
    }

    pub fn max_step_leaf_count(&self) -> u64 {
        self.max_step_leaf_count
    }

    pub fn turn_deadline_daa(&self) -> u64 {
        self.turn_deadline_daa
    }

    pub fn terminal_rounds(&self) -> u32 {
        self.terminal_rounds
    }

    /// `ceil(log2(max_step_leaf_count))` — the bisection depth needed to isolate one step.
    pub fn bisection_rounds(&self) -> u32 {
        // `next_power_of_two` is exact for powers of two, so this is ceil(log2(n)) for n >= 2.
        self.max_step_leaf_count.next_power_of_two().trailing_zeros()
    }

    /// ADR-0042 Decision 8's formula, in DAA units. `None` on overflow, which the startup gate
    /// treats as a refusal rather than a saturation — a court window that cannot be represented
    /// is not a long window.
    pub fn worst_case_duration_daa(&self) -> Option<u64> {
        let rounds = u64::from(self.bisection_rounds()).checked_add(u64::from(self.terminal_rounds))?;
        rounds.checked_mul(self.turn_deadline_daa)
    }
}

/// Bond-side network constants (ADR-0042 Decision 6's withdrawal-delay clause).
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwBondParamsV2 {
    /// Minimum slashable collateral a bond registers with, in sompi.
    min_collateral_sompi: u64,
    /// DAA delay between a retirement request and withdrawal. The startup invariant demands it
    /// exceed the whole liability period, so a bond cannot commit fraud and leave before it is
    /// provable.
    withdrawal_delay_daa: u64,
}

impl PalwBondParamsV2 {
    pub fn new(min_collateral_sompi: u64, withdrawal_delay_daa: u64) -> Result<Self, PalwModeV2Error> {
        if min_collateral_sompi == 0 {
            return Err(PalwModeV2Error::Invalid("a zero minimum collateral bonds nothing"));
        }
        if withdrawal_delay_daa == 0 {
            return Err(PalwModeV2Error::Invalid("a zero withdrawal delay lets a bond leave mid-liability"));
        }
        Ok(Self { min_collateral_sompi, withdrawal_delay_daa })
    }

    pub fn min_collateral_sompi(&self) -> u64 {
        self.min_collateral_sompi
    }

    pub fn withdrawal_delay_daa(&self) -> u64 {
        self.withdrawal_delay_daa
    }
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwModeV2Error {
    #[error("invalid V2 bundle: {0}")]
    Invalid(&'static str),
    #[error("invalid V2 bundle: {0}")]
    State(#[from] PalwStateV2Error),
    #[error("invalid V2 bundle: {0}")]
    Admission(#[from] PalwAdmissionV2Error),
    #[error("invalid V2 bundle: {0}")]
    FreePrompt(#[from] PalwFpV3Error),
    #[error("invalid V2 bundle: {0}")]
    Panel(#[from] crate::palw_panel_v2::PalwPanelV2Error),
    /// The genesis ARTIFACT is the thing at fault, not the bundle: the catalog does not match the
    /// root, a registration disagrees with the catalog, or a bond names collateral the genesis
    /// UTXO set does not hold. Carried whole rather than flattened to a `&'static str`, because
    /// every one of these failures is an operator holding two artifacts who needs to be told which
    /// of them is wrong.
    #[error("the genesis artifact does not load: {0}")]
    Genesis(#[from] crate::palw_genesis_v2::PalwGenesisV2Error),
}

/// The whole V2 ruleset, or none of it. Field order is part of the fingerprint preimage —
/// reordering is a different ruleset.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwConsensusParamsV2 {
    /// == [`crate::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION`]; a bundle claiming another
    /// protocol is another ruleset.
    pub protocol_version: u16,
    /// == [`crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2`]; the only algorithm a V2 network
    /// demands or accepts.
    pub algorithm_id: u8,
    /// The permanently-Active liveness floor (ADR-0039 W6′): PALW-BASE-0's class id.
    pub base_class_id: Hash64,
    /// Commitment to the genesis-registered class set. The genesis loader verifies preimages.
    pub class_catalog_root: Hash64,
    /// Commitment to the adjudicable primitive set. The boot path runs
    /// `verify_catalog_coverage_v1` against this build's own catalog.
    pub court_catalog_root: Hash64,
    pub state: PalwStateParamsV2,
    pub admission: PalwAdmissionParamsV2,
    pub panel: PalwPanelParamsV2,
    pub reward: PalwRewardParamsV2,
    pub bond: PalwBondParamsV2,
    /// ADR-0044: the free-prompt receipt lane — a REQUIRED part of the bundle, not a fence. A
    /// ruleset without it is a different ruleset (a different `palw_ruleset_id_v2`), never this
    /// one with a switch off.
    pub freeprompt: PalwFreePromptParamsV3,
    /// Reorg safety margin added to the liability period in the withdrawal-delay invariant.
    pub reorg_margin_daa: u64,
    /// The court's shape. Its [`PalwCourtParamsV2::worst_case_duration_daa`] is what the court
    /// backstop window must exceed, DERIVED rather than asserted.
    pub court: PalwCourtParamsV2,
    /// The cadence every DAA-denominated window in this bundle is measured against
    /// (ADR-0038 Decision H's frozen 120 s). Inside the bundle — and therefore inside the
    /// ruleset id — because without it two networks could share an id and run every window at a
    /// different wall-clock length, which is the exact opposite of what Decision 11 promises.
    /// [`crate::config::params::Params::validate_palw_v2`] additionally requires the network's
    /// own `target_time_per_block` to equal it, so the commitment and the behaviour agree.
    pub cadence_target_time_per_block_ms: u64,
    /// == [`PALW_V2_FORK_CHOICE_VERSION`]. Named by Decision 11's preimage.
    pub fork_choice_version: u16,
    /// == [`PALW_V2_TRACE_FORMAT_VERSION`]. Named by Decision 11's preimage.
    pub trace_format_version: u16,
    /// == [`palw_v2_signature_contexts_root`] as THIS BINARY computes it. Named by Decision 11's
    /// preimage, and committed as the context bytes' own digest rather than a version number, so
    /// a build whose contexts differ from the network's refuses to start.
    pub signature_contexts_root: Hash64,
    /// **The genesis registration list — the RC genesis artifact's own objects.**
    ///
    /// A `ConsensusV2` network has no class and no bond until something registers them, and the
    /// only block that can is genesis: an attempt is refused by admission for naming a bond the
    /// chain does not have, so a network whose genesis registers nothing can never produce its
    /// first block. Measured, not reasoned about — the harness wedged exactly there once
    /// admission was wired.
    ///
    /// They live in the BUNDLE because the ruleset id must cover them: two networks that share a
    /// ruleset id and register different classes are two rulesets, which is the property
    /// Decision 11 exists to make checkable. `palw_genesis_v2::verify_palw_genesis_v2` checks
    /// this list against the catalog preimage at load.
    pub genesis_objects: Vec<crate::palw_state_v2::PalwConsensusObjectV2>,
}

impl PalwConsensusParamsV2 {
    /// The Decision 1 startup invariants. A node holding a `ConsensusV2` mode whose bundle fails
    /// any of these does not boot — there is no degraded mode, because a degraded mode is a
    /// half-flip with a friendlier name.
    pub fn validate(&self) -> Result<(), PalwModeV2Error> {
        self.validate_ruleset_shape()?;
        // **A network that never retires a claim has an unbounded state** (launch blockers §8,
        // third bullet). The claim map grows by one entry per attempt-lane block, `state_root`
        // re-hashes every collection on every block and the tip row re-serializes the registry on
        // every walk — so with retirement off the per-block cost grows with the chain and never
        // comes down (`measure_claim_growth_cost`: 8.2 ms/5.4 MB at 10k claims, 467 ms/538 MB at
        // 1M). It is a runnable-network property, so it is checked where every other one is,
        // rather than left to whoever writes the next bundle.
        if self.state.claim_retirement_daa() == 0 {
            return Err(PalwModeV2Error::Invalid(
                "a ConsensusV2 ruleset must retire terminal claims — with no retirement span the PALW state grows by one claim per block forever",
            ));
        }
        // **Audit C1 — last, because it is about this BINARY rather than the ruleset.**
        //
        // Four pipeline gates now demand `algorithm_id` on a `ConsensusV2` network. Nothing
        // checked that the demanded algorithm has a finalizer arm, so a node booted (every
        // invariant above held), accepted its parentless genesis, and then rejected every block
        // after it — its own miner's included — as `InvalidPoW`, with no fallback id accepted and
        // no pruning proof importable. A total, unrecoverable liveness failure at block 1,
        // reachable purely by configuration.
        //
        // `check_algo_id_known` is the list of ids `kaspa_pow::StateLayer0::calculate_l1_tag`
        // actually implements. Reading it here means the mode can only demand what the binary can
        // compute: the day the V2 finalizer arm lands, this gate opens in the same commit, and
        // until then a V2 ruleset refuses to boot instead of stalling silently. (That day came —
        // the algo-6 arm and its carrier landed with `palw_v2_commitment_mutation_invalidates_pow`
        // — so this clause now passes a well-formed V2 bundle; it stays as the same tripwire for
        // any future id a ruleset names before its arm exists.)
        crate::pow_layer0::check_algo_id_known(self.algorithm_id).map_err(|_| {
            PalwModeV2Error::Invalid(
                "this binary has no Layer-0 finalizer for the ruleset's algorithm_id — it would accept genesis and reject every block after it",
            )
        })?;
        Ok(())
    }

    /// Every Decision 1 invariant that is a property of the RULESET rather than of this binary.
    ///
    /// Split out from [`Self::validate`] so the ruleset invariants stay testable while the
    /// runnability gate above is closed — a bundle can be well-formed and still be one this
    /// build cannot execute, and those are different failures with different fixes.
    pub fn validate_ruleset_shape(&self) -> Result<(), PalwModeV2Error> {
        if self.protocol_version != crate::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION {
            return Err(PalwModeV2Error::Invalid("protocol_version is not the V2 attempt version"));
        }
        if self.algorithm_id != crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2 {
            return Err(PalwModeV2Error::Invalid("algorithm_id is not the committed-V2 id"));
        }
        if self.class_catalog_root == Hash64::default() || self.court_catalog_root == Hash64::default() {
            return Err(PalwModeV2Error::Invalid("a zero catalog root commits to nothing adjudicable"));
        }

        // ADR-0045 Decision 3: the share table is chain state now, so the old boot-time checks
        // (BASE-0 funded, exactly-1000 allocation, share↔budget coherence) have no params-side
        // subject left to check. What replaced them is stronger, not absent: the transition
        // refuses any first registration that is not the base class at the whole 1000‰, the
        // donation arithmetic conserves the denominator at every mutation, and
        // `assert_internal_consistency` re-derives both facts on every carriage load. What the
        // BUNDLE must still guarantee is that the id the ruleset commits to and the id the
        // machine enforces are the same id (the C5 pattern, applied to the liveness floor).
        if self.state.base_class_id() != self.base_class_id {
            return Err(PalwModeV2Error::Invalid(
                "the liveness floor the bundle names is not the one the state machine enforces registrations against",
            ));
        }

        // The bundle's minimum collateral and the state machine's must be the same number: the
        // bundle is what the ruleset id commits to, the state params are what registrations are
        // actually checked against, and a network where they disagree enforces the one nobody
        // audited (audit C5).
        if self.state.min_collateral_sompi() != self.bond.min_collateral_sompi() {
            return Err(PalwModeV2Error::Invalid(
                "the bond floor the bundle commits to is not the one registrations are checked against",
            ));
        }

        // Same rule, same reason, for the producer carve: `reward` is what the ruleset id commits
        // to and what a reader audits, `state.worker_carve_permille` is what actually lands in
        // every claim's escrow. A network where they differ pays the number nobody read.
        if self.state.worker_carve_permille() != self.reward.worker_carve_permille() {
            return Err(PalwModeV2Error::Invalid("the producer carve the bundle commits to is not the one claims actually escrow"));
        }

        // And once more for the rung window (P0-9). `court` is what Decision 8's worst-case ladder
        // duration is computed from — the check a few lines below — while `state` is what the
        // sweep actually measures silence against. A bundle where they differ is audited against
        // one ladder and run against another.
        if self.state.turn_deadline_daa() != self.court.turn_deadline_daa() {
            return Err(PalwModeV2Error::Invalid("the rung window the bundle commits to is not the one the sweep enforces"));
        }

        // The anchor slot sits strictly inside the bind window (PR-06's cross-check).
        self.panel
            .validate_against_state_params(&self.state)
            .map_err(|_| PalwModeV2Error::Invalid("the anchor slot does not sit inside the bind window"))?;

        // Decision 11's preimage, checked against what this binary implements. Each of these was
        // named in the ADR's `palw_ruleset_id` formula and absent from the bundle (audit H2), so
        // "RC and mainnet share a ruleset id" said less than it appeared to.
        if self.fork_choice_version != PALW_V2_FORK_CHOICE_VERSION {
            return Err(PalwModeV2Error::Invalid("fork_choice_version is not the order this binary implements"));
        }
        if self.trace_format_version != PALW_V2_TRACE_FORMAT_VERSION {
            return Err(PalwModeV2Error::Invalid("trace_format_version is not the format this binary adjudicates"));
        }
        if self.signature_contexts_root != palw_v2_signature_contexts_root() {
            return Err(PalwModeV2Error::Invalid("signature_contexts_root is not the context set this binary signs under"));
        }
        if self.cadence_target_time_per_block_ms != PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS {
            return Err(PalwModeV2Error::Invalid("the bundle's cadence is not the frozen 120 s (ADR-0038 Decision H)"));
        }

        // The court backstop exceeds the worst-case honest prosecution — DERIVED from the
        // court's shape by ADR-0042 Decision 8's formula, not asserted as a lone number.
        let worst_case = self
            .court
            .worst_case_duration_daa()
            .ok_or(PalwModeV2Error::Invalid("the worst-case court duration overflows the DAA score"))?;
        if self.state.window_court() <= worst_case {
            return Err(PalwModeV2Error::Invalid("window_court does not fit the worst-case honest prosecution"));
        }
        // ADR-0042 Decision 1 also states "challenge window > worst-case court duration", and the
        // audit (H2) flagged its absence. It is deliberately NOT added: the ADR's clause assumes a
        // design where a court has to finish inside the window that decides maturity, and this
        // implementation chose a stronger one — an OPEN court suspends `ReceiptLicensed → Final`
        // entirely (`open_courts_by_claim` gates the edge, and `rearm_after_challenger_side_close`
        // re-arms the deadline afterwards), so the court is bounded by `window_court` and never
        // races the challenge window at all. Requiring `window_challenge > worst_case` on top
        // would force every honest claim to wait a full worst-case prosecution before maturing,
        // for no safety gained. ADR-0042 Decision 1 should be amended to say which mechanism
        // carries the guarantee.
        //
        // `court.max_step_leaf_count` is a number the bundle ASSERTS and this gate cannot read off
        // the catalog — only `class_catalog_root` is here, and the preimage travels with the
        // genesis artifact. That is by construction, not a residual: the catalog is deliberately
        // outside the bundle so RC and mainnet share one ruleset id. The assertion is checked
        // against the fact one altitude up, at `verify_against_catalog`, which
        // `palw_genesis_v2::verify_palw_genesis_v2` runs at genesis load — the same landing that
        // now also checks each registration's declared `pwu_per_inference` against the catalog's
        // counted one (ADR-0045 Decision 1). A node whose court is shallower than its catalog
        // refuses to start; it does not start and stall.

        // Withdrawal outlasts the whole liability period plus the reorg margin: a bond cannot
        // commit fraud and leave before it is provable.
        let liability = self
            .state
            .window_bind()
            .checked_add(self.state.window_receipt())
            .and_then(|x| x.checked_add(self.state.window_challenge()))
            .and_then(|x| x.checked_add(self.state.window_court()))
            .and_then(|x| x.checked_add(self.reorg_margin_daa))
            .ok_or(PalwModeV2Error::Invalid("the liability period overflows the DAA score"))?;
        if self.bond.withdrawal_delay_daa() <= liability {
            return Err(PalwModeV2Error::Invalid("the withdrawal delay does not outlast the liability period"));
        }

        // ---- ADR-0044: the free-prompt lane's startup invariants ----

        // **The split must match which lanes can actually produce** (launch blockers §6).
        //
        // This used to demand `1..=999` unconditionally — "both lanes must exist" — on the reading
        // that a zero attempt share has no beacons (F16) and a full one has no receipts. That is
        // right for a network where BOTH lanes are reachable, and this one is not: `algorithm_id`
        // above pins it to algo-6, and the header gate rejects every algo-7 header before its
        // admission path is reached. The receipt lane exists on paper and can never produce a block.
        //
        // Two rules therefore contradicted each other, and the one that had to give is this one:
        // holding a lane open in the SPLIT does not make it producible, while giving it a permille
        // does make the retarget expect blocks from it. The retarget measures each lane against the
        // COMBINED census, so an unproducible lane's permille stays in the expectation while its
        // blocks are missing from the total — the attempt lane then holds 100% of what happened
        // while being expected to hold 15% of it, is judged a 6.67x over-producer at every epoch
        // boundary, and has its target divided until it reaches the floor of 1, where the class
        // lottery refuses every attempt. A chain that stops with no path back.
        //
        // So: the whole cadence to the attempt lane while algo-7 is unreachable, and the
        // `1..=999` range the moment it is not. When the receipt lane becomes producible this
        // becomes a two-sided check again — and it will be a ruleset change, which is where a
        // change of this shape belongs.
        let split = self.state.fp_attempt_share_permille();
        if split != crate::palw_class_daa::PALW_CLASS_SHARE_DENOMINATOR {
            return Err(PalwModeV2Error::Invalid(
                "the receipt lane holds a cadence share on a network whose algorithm_id makes an algo-7 block                  impossible — every epoch would measure the attempt lane as an over-producer and divide its target                  to the floor, stopping the chain with no path back",
            ));
        }

        // The per-class half of this clause retired with its subject. It used to walk a params
        // share TABLE and refuse any class whose composed lane share rounds to zero; ADR-0045
        // Decision 3 moved shares to chain state (granted at registration, conserved to 1000‰),
        // so there is no table here to walk — and the retarget renormalizes each lane over the
        // classes that actually competed in it, which makes a composed zero a SKIPPED span (the
        // price holds still for the span) rather than a silently frozen price. The startup fact
        // that remains is the one above: both lanes must exist.

        // A late beacon must still bind inside the bind window: the panel's anchor is the FIRST
        // attempt-class block at the slot, and the declared worst-case gap to one is part of the
        // ruleset. Without this a thin floor quietly turns every FP claim into a BindTimeout.
        let worst_anchor = self
            .panel
            .anchor_delay()
            .checked_add(self.freeprompt.max_beacon_gap_daa())
            .ok_or(PalwModeV2Error::Invalid("the anchor slot plus the beacon gap overflows the DAA score"))?;
        if worst_anchor >= self.state.window_bind() {
            return Err(PalwModeV2Error::Invalid("anchor_delay + max_beacon_gap must sit inside the bind window"));
        }

        // The draw beacon sits past the reorgable fringe of the certification it draws for.
        if self.freeprompt.receipt_maturity_daa() < self.reorg_margin_daa {
            return Err(PalwModeV2Error::Invalid("receipt maturity must cover the reorg margin"));
        }
        Ok(())
    }

    /// Does this bundle accept the given header algorithm? A V2+FP network runs exactly two
    /// block kinds: the attempt id and the receipt id (ADR-0044 Decision 1). This is the
    /// two-id acceptance the FP-08 seam swap wires into the header/pruning gates; until then
    /// the wired seam still demands the attempt id exclusively, and no live network carries a
    /// bundle at all.
    pub fn accepts_algo_id(&self, algo_id: u8) -> bool {
        algo_id == self.algorithm_id || algo_id == self.freeprompt.receipt_algorithm_id()
    }
}

// ---------------------------------------------------------------------------------------------
// The class catalog: the preimage behind `class_catalog_root`
// ---------------------------------------------------------------------------------------------

/// One registered execution class, as the catalog describes it.
///
/// The bundle commits to a ROOT over these; the entries themselves are a network artifact that
/// travels with the RC genesis. Keeping them out of `PalwConsensusParamsV2` is deliberate — the
/// ruleset id must be the same value on the RC and on mainnet, and the catalog is exactly the
/// kind of thing that would otherwise tempt someone to differ.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClassCatalogEntryV2 {
    pub class_id: Hash64,
    /// What `palw_artifact` openings prove against, and what an attempt's `artifact_root` must
    /// equal (admission item 5).
    pub artifact_root: Hash64,
    /// Worst-case step-tree leaf count for this class — the depth the bisection ladder has to
    /// walk. This is the quantity `PalwCourtParamsV2::max_step_leaf_count` ASSERTS; the catalog
    /// is where it is a fact.
    pub max_step_leaf_count: u64,
    /// The COUNTED step-leaf count of this class's canonical inference — the quantity ADR-0045
    /// Decision 1 makes `pwu_per_inference` normatively equal to.
    ///
    /// A registration DECLARES `pwu_per_inference`; only this says what it should be. Without the
    /// number living here the declaration was self-certifying, and since Decision 1 turned pwu
    /// into `palw_pwu_v1(class_target, pwu_per_inference)` an overstated declaration is a direct
    /// multiplier on the class's fork-choice weight. It is a catalog fact for the same reason
    /// `max_step_leaf_count` is: the catalog root is committed to the genesis, so the number an
    /// operator would have to lie about is one the chain already hashed.
    ///
    /// Bounded above by `max_step_leaf_count` — a canonical run cannot be deeper than the class's
    /// own worst case — which is what keeps "work worth paying for" and "work the ladder can
    /// walk" the same quantity.
    pub canonical_step_leaf_count: u64,
    /// Every `kernel_semantics_id` this class's shape profile can reach at adjudication time.
    /// `verify_catalog_coverage_v1` compares it against THIS BUILD's adjudicable catalog.
    pub reachable_kernels: BTreeSet<Hash64>,
    /// **What prosecuting this class costs, DERIVED at mint** — the same
    /// `derive_court_cost_v1(profile)` the post-genesis admission gate runs, carried here so the
    /// genesis path can enforce the same ceilings. The catalog already holds the derived
    /// `max_step_leaf_count` for exactly this reason: with `admission: None` a genesis
    /// registration carries no profile, so nothing at boot can re-derive a number — the catalog
    /// asserts it and the mint is the one place the derivation runs. A second, genesis-only cost
    /// computation would be a second metric to drift from the admission one (the defect class the
    /// mint/admission same-function rule exists for).
    ///
    /// Without this field a class minted into a genesis bypassed ADR-0049 Decision C entirely:
    /// coverage-clean, ladder-deep enough, and unprosecutable — the ceilings gated only the
    /// post-genesis door (found on the two-class genesis's first review).
    pub court_cost: crate::palw_class_admission_v2::PalwCourtCostV1,
}

/// The registered class set, ordered and unique by class id.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwClassCatalogV2 {
    entries: Vec<PalwClassCatalogEntryV2>,
}

pub const PALW_CLASS_CATALOG_V2_DOMAIN: &[u8] = b"misaka-palw/class-catalog-v2/root/v1";

impl PalwClassCatalogV2 {
    /// Ascending by class id, no duplicates, non-empty. The order is part of the root, so a
    /// catalog that could be reordered would be a catalog with two roots.
    pub fn new(entries: Vec<PalwClassCatalogEntryV2>) -> Result<Self, PalwModeV2Error> {
        if entries.is_empty() {
            return Err(PalwModeV2Error::Invalid("an empty class catalog registers nothing"));
        }
        if entries.windows(2).any(|w| w[0].class_id >= w[1].class_id) {
            return Err(PalwModeV2Error::Invalid("the class catalog must be ascending and unique by class id"));
        }
        if entries.iter().any(|e| e.max_step_leaf_count < 2) {
            return Err(PalwModeV2Error::Invalid("a class whose trace has fewer than two step leaves cannot be bisected"));
        }
        if entries.iter().any(|e| e.canonical_step_leaf_count == 0) {
            return Err(PalwModeV2Error::Invalid("a canonical inference of zero steps is work worth nothing and priced as something"));
        }
        if entries.iter().any(|e| e.canonical_step_leaf_count > e.max_step_leaf_count) {
            return Err(PalwModeV2Error::Invalid(
                "a class's canonical inference is deeper than its own worst case — the ladder could not walk what the price claims",
            ));
        }
        Ok(Self { entries })
    }

    pub fn entries(&self) -> &[PalwClassCatalogEntryV2] {
        &self.entries
    }

    /// `H(count ‖ borsh(entries))` — what `PalwConsensusParamsV2::class_catalog_root` commits to.
    pub fn root(&self) -> Hash64 {
        let bytes = borsh::to_vec(&self.entries).expect("catalog entries are borsh-serializable");
        let mut state = Blake2bParams::new().hash_length(64).key(PALW_CLASS_CATALOG_V2_DOMAIN).to_state();
        state.update(&(self.entries.len() as u64).to_le_bytes());
        state.update(&(bytes.len() as u64).to_le_bytes());
        state.update(&bytes);
        let mut out = [0u8; 64];
        out.copy_from_slice(state.finalize().as_bytes());
        Hash64::from_bytes(out)
    }

    /// The deepest trace any registered class can produce.
    pub fn max_step_leaf_count(&self) -> u64 {
        self.entries.iter().map(|e| e.max_step_leaf_count).max().expect("a catalog is non-empty by construction")
    }

    /// The entry for `class_id`, if the catalog registers it.
    pub fn entry(&self, class_id: &Hash64) -> Option<&PalwClassCatalogEntryV2> {
        self.entries.iter().find(|e| e.class_id == *class_id)
    }
}

impl PalwConsensusParamsV2 {
    /// **The half of Decision 1 that needs the catalog PREIMAGE**, which is why it is a separate
    /// entry point: `validate` runs on the bundle alone and boots the node, while this runs where
    /// the genesis artifact is loaded and the class set is actually in hand.
    ///
    /// Three invariants the ADR lists as boot conditions and the bundle alone cannot check
    /// (audit H2/C1):
    ///
    /// * the catalog is the one the ruleset committed to — its root recomputes to
    ///   `class_catalog_root`, so "we registered these classes" is a hash rather than a claim;
    /// * **court coverage is 100%** for every registered class — every kernel a class can reach
    ///   at adjudication time is one this build can actually adjudicate. Without it a class
    ///   activates whose disputes end `Unadjudicable`, which is a class that cannot be policed;
    /// * the court's asserted `max_step_leaf_count` covers the catalog's real worst case. This is
    ///   what turns the last self-certified number in the bundle into a checked one: an operator
    ///   who understates it to shrink `window_court` now contradicts the catalog its own genesis
    ///   committed to.
    pub fn verify_against_catalog(&self, catalog: &PalwClassCatalogV2) -> Result<(), PalwModeV2Error> {
        if catalog.root() != self.class_catalog_root {
            return Err(PalwModeV2Error::Invalid("the class catalog is not the one this ruleset's root commits to"));
        }
        if !catalog.entries().iter().any(|e| e.class_id == self.base_class_id) {
            return Err(PalwModeV2Error::Invalid("PALW-BASE-0 is not in the class catalog — the liveness floor is unregistered"));
        }
        // ADR-0045 Decision 3: shares are granted by `ClassRegistered` and conserved by the
        // transition, so "every share-bearing class exists" is a per-registration fact now —
        // the genesis loader checks each registration object against this same catalog, and a
        // class cannot hold a share without having been registered. No params-side table is
        // left to sweep here.
        for entry in catalog.entries() {
            let reachable = crate::palw_catalog_coverage::PalwReachableKernelSetV1 {
                execution_class_id: entry.class_id,
                kernel_ids: entry.reachable_kernels.clone(),
            };
            crate::palw_catalog_coverage::verify_catalog_coverage_v1(&reachable)
                .map_err(|_| PalwModeV2Error::Invalid("a registered class reaches kernels this build cannot adjudicate"))?;
        }
        if self.court.max_step_leaf_count() < catalog.max_step_leaf_count() {
            return Err(PalwModeV2Error::Invalid(
                "the court's worst-case trace depth is shallower than the catalog's — the ladder cannot reach the deepest class",
            ));
        }
        // The three cost ceilings, beside the ladder and by the same discipline: the catalog
        // asserts the derived cost, the ruleset asserts what it will pay for, and a class whose
        // disputes cannot ride a carrier must fail the BOOT gate, not its first challenger.
        for entry in catalog.entries() {
            if entry.court_cost.max_opening_bytes > self.court.max_opening_bytes() {
                return Err(PalwModeV2Error::Invalid(
                    "a registered class's terminal opening exceeds the ceiling this ruleset pays for",
                ));
            }
            if entry.court_cost.max_terminal_macs > self.court.max_terminal_macs() {
                return Err(PalwModeV2Error::Invalid(
                    "a registered class's terminal recomputation exceeds the ceiling this ruleset pays for",
                ));
            }
            if entry.court_cost.max_operand_count > self.court.max_operand_count() {
                return Err(PalwModeV2Error::Invalid(
                    "a registered class's operand count exceeds the ceiling this ruleset pays for",
                ));
            }
        }
        Ok(())
    }
}

/// A network is in exactly one PALW mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwConsensusMode {
    /// Hash networks: PALW machinery inert. Every shipped preset.
    Disabled,
    /// The ADR-0035 algo-4 soak, exactly as the existing legacy fields describe it — this
    /// variant marks the mode; the legacy knobs stay where they live, because duplicating them
    /// here would be a second source for the same facts.
    LegacyTn11,
    /// The RC / mainnet ruleset — all of it.
    ConsensusV2(PalwConsensusParamsV2),
}

impl PalwConsensusMode {
    /// The algorithm a network in this mode DEMANDS. `None` leaves the decision to the existing
    /// (V1 / hash) rules; `Some` is exclusive — a V2 network accepts nothing else.
    pub fn required_algo_id(&self) -> Option<u8> {
        match self {
            PalwConsensusMode::Disabled | PalwConsensusMode::LegacyTn11 => None,
            PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.algorithm_id),
        }
    }

    /// **Does this network accept a header declaring `algo_id`?**
    ///
    /// [`Self::required_algo_id`] answers a narrower question — which id a producer must DECLARE
    /// when it builds an attempt — and a gate that used it to decide acceptance refused every
    /// block on the second lane a V2 bundle admits. `accepts_algo_id` is the bundle's own answer
    /// and it has always been two: the committed-attempt id and the free-prompt receipt id.
    ///
    /// `None` means "this network has no V2 opinion", and the caller falls back to the V1 fork
    /// cascade exactly as before — every shipped non-V2 preset is unaffected.
    pub fn accepts_algo_id(&self, algo_id: u8) -> Option<bool> {
        match self {
            PalwConsensusMode::Disabled | PalwConsensusMode::LegacyTn11 => None,
            PalwConsensusMode::ConsensusV2(bundle) => Some(bundle.accepts_algo_id(algo_id)),
        }
    }
}

/// Decision 11: `H(canonical(bundle))`. Everything that decides consensus is inside; network
/// identity is not (the challenge's `network_domain` carries it), so the RC and mainnet share
/// one id and a node can CHECK sameness instead of trusting a release note.
impl PalwConsensusParamsV2 {}

pub fn palw_ruleset_id_v2(bundle: &PalwConsensusParamsV2) -> Hash64 {
    let bytes = borsh::to_vec(bundle).expect("the V2 bundle is borsh-serializable");
    let mut state = Blake2bParams::new().hash_length(64).key(PALW_RULESET_ID_V2_DOMAIN).to_state();
    state.update(&(bytes.len() as u64).to_le_bytes());
    state.update(&bytes);
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// **The gate fires** (launch blockers §8, third bullet). A guard nothing has been shown to
    /// refuse is a comment; the bundle every other test in this module uses is the one that must
    /// still pass.
    #[test]
    fn a_ruleset_that_never_retires_a_claim_is_refused() {
        let good = conforming_bundle();
        good.validate().expect("the conforming bundle retires and boots");
        let mut bad = conforming_bundle();
        bad.state = bad.state.clone().with_claim_retirement_daa(0).unwrap();
        let err = bad.validate().expect_err("a ruleset with no retirement span is not runnable");
        assert!(format!("{err}").contains("retire terminal claims"), "and it says why: {err}");
    }

    fn state_params_with_min_collateral(min_collateral: u64) -> PalwStateParamsV2 {
        // Carries the same carve `conforming_bundle`'s `reward` declares: this helper is used to
        // REPLACE that bundle's state, and dropping the carve on the way would trip the coherence
        // check instead of testing the thing the caller named.
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, min_collateral, 1000, 0)
            .unwrap()
            .with_worker_carve_permille(620)
            .unwrap()
            .with_turn_deadline_daa(20)
            .unwrap()
            // §8: a runnable ruleset retires terminal claims; a fixture is one a node starts on.
            .with_claim_retirement_daa(200)
            .unwrap()
    }

    pub(crate) fn conforming_freeprompt() -> PalwFreePromptParamsV3 {
        PalwFreePromptParamsV3::new(
            crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
            1_000,
            10,
            crate::palw_freeprompt_v3::PalwFpCuWeightsV3 { prefill_weight: 1, decode_weight: 64 },
            64,
            4_096,
            512,
            150,
            200,
            5,
        )
        .unwrap()
    }

    pub(crate) fn conforming_bundle() -> PalwConsensusParamsV2 {
        let base = h64(1);
        PalwConsensusParamsV2 {
            protocol_version: crate::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION,
            algorithm_id: crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
            base_class_id: base,
            class_catalog_root: h64(0xCA7),
            court_catalog_root: h64(0xC0517),
            // Split 1000‰: the attempt lane holds the whole cadence, because `algorithm_id` makes
            // an algo-7 block impossible and a lane that cannot produce must not be expected to.
            // The carve is set on BOTH, because `validate` requires them equal — the fixture has
            // to be a bundle a node would actually start on, or the tests below prove nothing.
            state: PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, base, 4, 1000, 100, 1000, 0)
                .unwrap()
                .with_worker_carve_permille(620)
                .unwrap()
                .with_turn_deadline_daa(20)
                .unwrap()
                .with_claim_retirement_daa(200)
                .unwrap(),
            admission: PalwAdmissionParamsV2::new(500).unwrap(),
            panel: PalwPanelParamsV2::new(3, 2, 4).unwrap(),
            reward: PalwRewardParamsV2::new(620).unwrap(),
            bond: PalwBondParamsV2::new(100, 2_000).unwrap(),
            freeprompt: conforming_freeprompt(),
            reorg_margin_daa: 100,
            // 2^20 step leaves -> 20 bisection rounds, +2 terminal, x20 DAA per turn = 440,
            // which must fit strictly inside the fixture's `window_court` of 500.
            court: PalwCourtParamsV2::new(1_048_576, 20, 2).unwrap(),
            cadence_target_time_per_block_ms: PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS,
            fork_choice_version: PALW_V2_FORK_CHOICE_VERSION,
            trace_format_version: PALW_V2_TRACE_FORMAT_VERSION,
            signature_contexts_root: palw_v2_signature_contexts_root(),
            genesis_objects: vec![
                crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                    class_id: base,
                    artifact_root: h64(11),
                    slash_value_per_pwu: 5,
                    pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                    initial_target: u128::MAX / 2,
                    share_permille: 1000,
                    activation_daa: 0,
                    admission: None,
                },
                crate::palw_state_v2::PalwConsensusObjectV2::BondRegistered {
                    bond: crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint {
                        transaction_id: crate::tx::TransactionId::from_u64_word(0xB0),
                        index: 0,
                    }),
                    pubkey: vec![7; 4],
                    operator_pubkey: vec![21; 8],
                    collateral: 100_000,
                    payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                    signature: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn a_conforming_bundle_validates_and_fingerprints_deterministically() {
        let bundle = conforming_bundle();
        bundle.validate_ruleset_shape().expect("the fixture bundle holds every startup invariant");
        let id = palw_ruleset_id_v2(&bundle);
        assert_eq!(id, palw_ruleset_id_v2(&bundle.clone()), "the fingerprint is a pure function of the bundle");
        // The bundle accepts exactly its two block kinds (ADR-0044 Decision 1) — and nothing else.
        assert!(bundle.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2));
        assert!(bundle.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3));
        for other in [0u8, 1, 2, 3, 4, 5, 8, 0xff] {
            assert!(!bundle.accepts_algo_id(other), "algo {other} is neither lane");
        }
        assert_eq!(PalwConsensusMode::ConsensusV2(bundle).required_algo_id(), Some(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2));
        assert_eq!(PalwConsensusMode::Disabled.required_algo_id(), None);
        assert_eq!(PalwConsensusMode::LegacyTn11.required_algo_id(), None);
    }

    /// Every startup invariant refuses its own violation — the Decision 1 list, executable.
    #[test]
    fn every_startup_invariant_refuses_its_violation() {
        let cases: Vec<(&str, Box<dyn Fn(&mut PalwConsensusParamsV2)>)> = vec![
            ("protocol", Box::new(|b| b.protocol_version = 1)),
            ("algorithm", Box::new(|b| b.algorithm_id = 4)),
            ("class root", Box::new(|b| b.class_catalog_root = Hash64::default())),
            ("court root", Box::new(|b| b.court_catalog_root = Hash64::default())),
            ("bundle names a floor the machine does not enforce", Box::new(|b| b.base_class_id = h64(9))),
            (
                // ADR-0045 Decision 3: the bundle names one liveness floor and the state machine
                // enforces another — the C5 disagreement shape, applied to the base id.
                "base id disagreement",
                Box::new(|b| {
                    b.state = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(2), 4, 1000, 100, 800, 0)
                        .unwrap()
                        .with_claim_retirement_daa(200)
                        .unwrap();
                }),
            ),
            (
                // ADR-0044 Decision 1: a bundle whose freeprompt lane is live must hold BOTH
                // lanes open — split 1000 has no receipts (and the params gate already refuses
                // 0). The share TABLE cases that once sat beside this one retired with their
                // subject (shares are chain state now); the split is still a params fact.
                "one-lane split (1000‰ has no receipts)",
                Box::new(|b| {
                    b.state = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 0)
                        .unwrap()
                        .with_claim_retirement_daa(200)
                        .unwrap();
                }),
            ),
            (
                "beacon gap outside the bind window",
                Box::new(|b| {
                    b.freeprompt = PalwFreePromptParamsV3::new(
                        crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
                        1_000,
                        10,
                        crate::palw_freeprompt_v3::PalwFpCuWeightsV3 { prefill_weight: 1, decode_weight: 64 },
                        64,
                        4_096,
                        512,
                        100,
                        200,
                        6, // anchor_delay 4 + gap 6 = 10 ≥ window_bind 10
                    )
                    .unwrap()
                }),
            ),
            (
                "receipt maturity inside the reorg margin",
                Box::new(|b| {
                    b.freeprompt = PalwFreePromptParamsV3::new(
                        crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
                        1_000,
                        10,
                        crate::palw_freeprompt_v3::PalwFpCuWeightsV3 { prefill_weight: 1, decode_weight: 64 },
                        64,
                        4_096,
                        512,
                        99, // reorg_margin_daa is 100
                        200,
                        5,
                    )
                    .unwrap()
                }),
            ),
            // The two table-coherence cases ("budgetless share", "shareless budget") retired
            // with their subject: shares are chain state (ADR-0045 Decision 3) and the epoch
            // budget is derived at each boundary from them (Decision 2), so there are no two
            // params tables left to drift apart. The FP "composed share rounds to zero" startup
            // case retired the same way — with shares granted at registration and the retarget
            // renormalizing over the classes that competed, a lane whose composition rounds to
            // zero is SKIPPED for the span (its price holds still), not silently frozen.
            ("anchor outside bind window", Box::new(|b| b.panel = PalwPanelParamsV2::new(3, 2, 10).unwrap())),
            // A deeper ladder needs more rounds than the backstop window can hold…
            ("ladder deeper than the court window", Box::new(|b| b.court = PalwCourtParamsV2::new(1 << 40, 20, 2).unwrap())),
            // …and so does a slower turn, at the same depth.
            (
                "turn slower than the court window",
                Box::new(|b| {
                    b.court = PalwCourtParamsV2::new(1_048_576, 30, 2).unwrap();
                    b.state = b.state.clone().with_turn_deadline_daa(30).unwrap();
                }),
            ),
            ("fork choice version", Box::new(|b| b.fork_choice_version += 1)),
            ("trace format version", Box::new(|b| b.trace_format_version += 1)),
            ("signature contexts", Box::new(|b| b.signature_contexts_root = h64(0xBAD))),
            ("cadence", Box::new(|b| b.cadence_target_time_per_block_ms /= 2)),
            ("withdrawal inside liability", Box::new(|b| b.bond = PalwBondParamsV2::new(20_000, 640).unwrap())),
        ];
        for (name, mutate) in cases {
            let mut bundle = conforming_bundle();
            mutate(&mut bundle);
            assert!(bundle.validate().is_err(), "{name}: the mutated bundle must not validate");
        }
    }

    /// Every shipped preset is `Disabled` — the V2 lineage's dormancy is one enum arm, checked
    /// as a fact, and the config gate holds the atomicity rules the arm implies.
    #[test]
    fn every_shipped_preset_is_disabled_and_the_gate_refuses_mixed_lineages() {
        use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS};
        for params in [&MAINNET_PARAMS, &TESTNET_PARAMS, &DEVNET_PARAMS, &SIMNET_PARAMS] {
            assert_eq!(params.palw_consensus_mode, PalwConsensusMode::Disabled, "{} ships Disabled", params.net);
            params.validate_palw_v2().expect("Disabled has nothing to validate");
        }
        // …except the PALW staging net, which says what it is since the Relaunch-2 re-genesis.
        assert_eq!(TESTNET11_PARAMS.palw_consensus_mode, PalwConsensusMode::LegacyTn11, "t11 marks the legacy lineage");
        TESTNET11_PARAMS.validate_palw_v2().expect("LegacyTn11 adds no new constraints");

        // A ConsensusV2 params set with a conforming bundle and no V1 residue validates.
        // SIMNET is the clean base: its `pow_palw_activation` is `never()`.
        let mut v2 = SIMNET_PARAMS.clone();
        v2.palw_consensus_mode = PalwConsensusMode::ConsensusV2(conforming_bundle());
        // …and is refused ANYWAY, today, because this binary has no finalizer for algo 6 (audit
        // C1). The mixed-lineage clauses below are what this test is about, so it asserts the
        // ruleset half separately from the runnability half.
        let PalwConsensusMode::ConsensusV2(ref bundle) = v2.palw_consensus_mode else { unreachable!() };
        bundle.validate_ruleset_shape().expect("a pure V2 set is a well-formed ruleset");
        assert!(v2.validate_palw_v2().is_err(), "and still refuses to boot: no V2 finalizer exists yet");

        // …a broken bundle does not…
        let mut broken = conforming_bundle();
        broken.court = PalwCourtParamsV2::new(1_048_576, 30, 2).unwrap();
        broken.state = broken.state.clone().with_turn_deadline_daa(30).unwrap();
        let mut bad_bundle = SIMNET_PARAMS.clone();
        bad_bundle.palw_consensus_mode = PalwConsensusMode::ConsensusV2(broken);
        assert!(bad_bundle.validate_palw_v2().is_err(), "the startup invariants gate the config");

        // …and mixing lineages does not — in BOTH shapes. DEVNET is a live V1 PALW network, so
        // declaring a V2 mode on it is refused outright…
        let mut mixed_pow = DEVNET_PARAMS.clone();
        mixed_pow.palw_consensus_mode = PalwConsensusMode::ConsensusV2(conforming_bundle());
        assert!(mixed_pow.validate_palw_v2().is_err(), "a V1 PALW PoW activation under a V2 mode is half of two lineages");
        // …and so is a V1 fence smuggled under a V2 mode on a clean base.
        let mut mixed_fence = SIMNET_PARAMS.clone();
        mixed_fence.palw_consensus_mode = PalwConsensusMode::ConsensusV2(conforming_bundle());
        mixed_fence.palw_ramp = Some(crate::palw_weight::PalwWeightParamsV1 { receipt_quorum: 2, rho_r_permille: 250 });
        assert!(mixed_fence.validate_palw_v2().is_err(), "a V1 fence under a V2 mode is the five-fences defect reborn");
    }

    /// **The PR-08 seam is inert on every shipped network.** For every preset, at a sweep of DAA
    /// scores, the mode-aware required-algo answer equals the V1 cascade's answer byte for byte —
    /// so threading the mode through the header, virtual and pruning-proof gates changed nothing
    /// any node accepts today. Only a `ConsensusV2` network answers differently, and no shipped
    /// preset is one.
    #[test]
    fn the_mode_seam_changes_no_shipped_networks_required_algo() {
        use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS};
        use crate::pow_layer0::{
            POW_ALGO_ID_PALW_COMMITTED_V2, check_algo_id, check_algo_id_for_mode, required_algo_id, required_algo_id_for_mode,
        };

        for params in [&MAINNET_PARAMS, &TESTNET_PARAMS, &TESTNET11_PARAMS, &DEVNET_PARAMS, &SIMNET_PARAMS] {
            let mode_required = params.palw_consensus_mode.required_algo_id();
            assert_eq!(mode_required, None, "{} demands no V2 id", params.net);
            for daa in [0u64, 1, 1_000, 1_000_000, u64::MAX - 1] {
                let (o, l, s) = (
                    params.pow_palw_ollama_activation.is_active(daa),
                    params.pow_palw_activation.is_active(daa),
                    params.pow_blake2b_sha3_activation.is_active(daa),
                );
                let v1 = required_algo_id(o, l, s);
                assert_eq!(
                    required_algo_id_for_mode(mode_required, o, l, s),
                    v1,
                    "{} @ {daa}: the seam moved a live network",
                    params.net
                );
                assert_eq!(check_algo_id_for_mode(v1, mode_required, o, l, s), check_algo_id(v1, o, l, s));
                assert!(
                    check_algo_id_for_mode(POW_ALGO_ID_PALW_COMMITTED_V2, mode_required, o, l, s).is_err(),
                    "{} @ {daa} must still refuse a V2 header",
                    params.net
                );
            }
        }
    }

    /// The mode is in the P2P consensus fingerprint — through the ruleset id, so the handshake
    /// commitment and the ruleset commitment cannot drift — and `Disabled` leaves the
    /// fingerprint exactly where it was before the field existed.
    #[test]
    fn the_mode_moves_the_consensus_fingerprint() {
        use crate::config::params::DEVNET_PARAMS;
        let disabled = DEVNET_PARAMS.consensus_params_id();
        let mut legacy = DEVNET_PARAMS.clone();
        legacy.palw_consensus_mode = PalwConsensusMode::LegacyTn11;
        let mut v2 = DEVNET_PARAMS.clone();
        v2.palw_consensus_mode = PalwConsensusMode::ConsensusV2(conforming_bundle());
        let legacy_id = legacy.consensus_params_id();
        let v2_id = v2.consensus_params_id();
        assert_ne!(disabled, legacy_id, "the legacy mark separates at handshake");
        assert_ne!(disabled, v2_id, "a V2 network separates at handshake");
        assert_ne!(legacy_id, v2_id);

        // And two V2 networks with different bundles separate too, through the ruleset id.
        let mut other = conforming_bundle();
        other.reward = crate::palw_reward_v2::PalwRewardParamsV2::new(621).unwrap();
        other.state = other.state.clone().with_worker_carve_permille(621).unwrap();
        let mut v2b = DEVNET_PARAMS.clone();
        v2b.palw_consensus_mode = PalwConsensusMode::ConsensusV2(other);
        assert_ne!(v2_id, v2b.consensus_params_id(), "a different ruleset is a different handshake");
    }

    /// **Audit C1: a ruleset this binary cannot compute must refuse to boot, not stall at block 1
    /// — and the gate must OPEN in the commit that lands the arm.**
    ///
    /// `a460cdd7` wired `required_algo_id_for_mode` into the header processor, the virtual
    /// processor's template stamp and the pruning-proof gate, so a `ConsensusV2` network demands
    /// `pow_algo_id == 6` and refuses every other id. While
    /// `kaspa_pow::StateLayer0::calculate_l1_tag` had no arm for 6, nothing connected those two
    /// facts, so the node started, accepted its parentless genesis, and then rejected every block
    /// after it — its own miner's included — as `InvalidPoW`. This test then asserted that
    /// `validate()` REFUSES a well-formed V2 ruleset, and its doc promised the assertion would
    /// flip in the same commit as the arm.
    ///
    /// That commit is this one. The algo-6 arm and its wire carrier landed
    /// (`palw_v2_commitment_mutation_invalidates_pow` is the test that holds them together), the
    /// finalizer's own list (`check_algo_id_known`) names 6 again, and the gate — still the last
    /// clause of `validate`, still reading that list — now passes the same bundle it refused. It
    /// stays in place as the tripwire for any FUTURE id a ruleset might name before its arm exists.
    #[test]
    fn the_runnability_gate_opened_with_the_finalizer_arm() {
        let bundle = conforming_bundle();
        // Well-formed as a ruleset…
        bundle.validate_ruleset_shape().expect("the fixture is a well-formed ruleset");
        // …and runnable by this binary: the arm exists, so the ruleset boots.
        bundle.validate().expect("a well-formed V2 ruleset whose algorithm this binary finalizes must boot");
        assert!(
            crate::pow_layer0::check_algo_id_known(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2).is_ok(),
            "the gate reads the finalizer's list, and that list now carries the V2 arm"
        );
        // The gate is not a hardcoded "V2 is on": shape still comes first, so a bundle naming an
        // algorithm this binary implements for OTHER lanes fails the ruleset clause — a different
        // failure with a different fix.
        let mut legacy_algo = conforming_bundle();
        legacy_algo.algorithm_id = crate::pow_layer0::POW_ALGO_ID_PALW_LLM;
        assert!(crate::pow_layer0::check_algo_id_known(legacy_algo.algorithm_id).is_ok(), "algo 4 has a finalizer arm");
        match legacy_algo.validate() {
            Err(PalwModeV2Error::Invalid(msg)) => {
                assert!(!msg.contains("finalizer"), "algo 4 must fail on the ruleset clause, not the runnability one: {msg}")
            }
            other => panic!("expected the algorithm-id ruleset refusal, got {other:?}"),
        }
    }

    /// **Audit H2: Decision 11's preimage, item by item.**
    ///
    /// The id used to hash a bundle that named none of `cadence`, `fork_choice_version`,
    /// `trace_format_version` or `signature_contexts` — so "the RC and mainnet ship the same
    /// ruleset" was checkable only for the fields that happened to be in the struct. Two networks
    /// could share an id and still order candidates differently, adjudicate a different trace
    /// format, sign under different contexts, or run every DAA window at half the wall-clock
    /// length. They are in the struct now, and the id is `H(borsh(bundle))`, so each one moves it.
    ///
    /// `every_bundle_byte_moves_the_ruleset_id` cannot cover them: it requires each mutation to
    /// remain a VALID other ruleset, and these four are pinned to what this binary implements —
    /// mutating one is an invalid bundle by construction. That is the stronger property, not a
    /// gap: they are in the preimage AND they cannot vary. This test asserts both halves.
    #[test]
    fn the_ruleset_id_covers_every_component_decision_11_names() {
        let base = conforming_bundle();
        let base_id = palw_ruleset_id_v2(&base);

        let pinned: Vec<(&str, Box<dyn Fn(&mut PalwConsensusParamsV2)>)> = vec![
            ("cadence", Box::new(|b: &mut PalwConsensusParamsV2| b.cadence_target_time_per_block_ms /= 2)),
            ("fork_choice_version", Box::new(|b: &mut PalwConsensusParamsV2| b.fork_choice_version += 1)),
            ("trace_format_version", Box::new(|b: &mut PalwConsensusParamsV2| b.trace_format_version += 1)),
            ("signature_contexts_root", Box::new(|b: &mut PalwConsensusParamsV2| b.signature_contexts_root = h64(0xBAD))),
        ];
        for (name, mutate) in pinned {
            let mut bundle = base.clone();
            mutate(&mut bundle);
            assert_ne!(palw_ruleset_id_v2(&bundle), base_id, "{name} is not inside the ruleset id");
            assert!(bundle.validate_ruleset_shape().is_err(), "{name} is pinned to this binary and must not validate when moved");
        }

        // The signature contexts are committed as their own BYTES, not as a version number, so
        // editing a context string is a startup failure rather than a silent cross-family replay.
        assert_eq!(base.signature_contexts_root, palw_v2_signature_contexts_root());
        assert_eq!(
            PALW_V2_SIGNATURE_CONTEXTS,
            [crate::palw_attempt_v2::PALW_ATTEMPT_V2_MLDSA87_CONTEXT, crate::palw_panel_v2::PALW_RECEIPT_V2_MLDSA87_CONTEXT],
            "the committed context set is the one the V2 objects actually sign under"
        );
    }

    /// **Audit H2: the worst-case court duration is DERIVED, not asserted.**
    ///
    /// It used to be one operator-typed number that the only invariant standing between the
    /// ruleset and an un-prosecutable fraud window compared itself against — self-certifying.
    /// It is now ADR-0042 Decision 8's formula over three named quantities.
    #[test]
    fn the_worst_case_court_duration_follows_the_adr_formula() {
        // ceil(log2(n)) is exact on powers of two and rounds up in between.
        assert_eq!(PalwCourtParamsV2::new(2, 1, 1).unwrap().bisection_rounds(), 1);
        assert_eq!(PalwCourtParamsV2::new(1_024, 1, 1).unwrap().bisection_rounds(), 10);
        assert_eq!(PalwCourtParamsV2::new(1_025, 1, 1).unwrap().bisection_rounds(), 11, "a non-power-of-two rounds UP");
        assert_eq!(PalwCourtParamsV2::new(1 << 40, 1, 1).unwrap().bisection_rounds(), 40);

        // rounds = ceil(log2(leaves)) + terminal, duration = rounds * turn deadline.
        let court = PalwCourtParamsV2::new(1_048_576, 20, 2).unwrap();
        assert_eq!(court.worst_case_duration_daa(), Some((20 + 2) * 20));

        // Overflow is a refusal, not a saturation: a window that cannot be represented is not a
        // long window.
        let huge = PalwCourtParamsV2::new(1 << 40, u64::MAX, 1).unwrap();
        assert_eq!(huge.worst_case_duration_daa(), None);
        let mut bundle = conforming_bundle();
        bundle.court = huge;
        assert!(bundle.validate_ruleset_shape().is_err(), "an unrepresentable court duration must refuse the bundle");

        // Shapes that cannot host a bisection at all are refused at construction.
        assert!(PalwCourtParamsV2::new(1, 20, 2).is_err(), "one leaf cannot be bisected");
        assert!(PalwCourtParamsV2::new(1_024, 0, 2).is_err(), "a zero turn deadline defaults by block order");
        assert!(PalwCourtParamsV2::new(1_024, 20, 0).is_err(), "zero terminal rounds never reaches a verdict");
    }

    fn catalog_entry(class_id: Hash64, leaves: u64) -> PalwClassCatalogEntryV2 {
        PalwClassCatalogEntryV2 {
            class_id,
            artifact_root: h64(11),
            max_step_leaf_count: leaves,
            // Half the worst case: distinct from it on purpose, so a fixture cannot pass by the
            // two being interchangeable.
            canonical_step_leaf_count: (leaves / 2).max(1),
            // The BASE-0 kernels this build adjudicates — the honest reachable set for a class
            // whose shape profile is BASE-0's.
            reachable_kernels: crate::palw_step_refute::catalogued_kernel_ids_v1(),
            court_cost: crate::palw_class_admission_v2::PalwCourtCostV1 { max_opening_bytes: 1, max_terminal_macs: 1, max_operand_count: 1 },
        }
    }

    /// **A class the ruleset cannot pay to prosecute fails the BOOT gate, not its first
    /// challenger.** The ceilings gated only the post-genesis door until the catalog carried the
    /// derived cost; a genesis-minted class sailed past them — coverage-clean, ladder-deep
    /// enough, unprosecutable (ADR-0049 Decision C's exact refusal condition, at the one entry
    /// point that did not test it).
    #[test]
    fn a_class_whose_close_cannot_ride_a_carrier_fails_at_boot() {
        let mut entry = catalog_entry(h64(1), 1 << 10);
        // A close bigger than what the ruleset pays for — one byte past the ceiling.
        entry.court_cost.max_opening_bytes = crate::palw_mode_v2::DEFAULT_MAX_OPENING_BYTES + 1;
        let catalog = PalwClassCatalogV2::new(vec![entry]).expect("well-formed");
        let mut bundle = conforming_bundle();
        bundle.base_class_id = h64(1);
        bundle.class_catalog_root = catalog.root();
        let err = bundle.verify_against_catalog(&catalog).expect_err("an unprosecutable class must not boot");
        assert!(format!("{err:?}").contains("terminal opening"), "refused by the cost gate, by name: {err:?}");

        // And the same class inside the ceilings boots — the gate separates.
        let mut entry = catalog_entry(h64(1), 1 << 10);
        entry.court_cost.max_opening_bytes = crate::palw_mode_v2::DEFAULT_MAX_OPENING_BYTES;
        let catalog = PalwClassCatalogV2::new(vec![entry]).expect("well-formed");
        bundle.class_catalog_root = catalog.root();
        bundle.verify_against_catalog(&catalog).expect("a class at the ceiling is a class the ruleset pays for");
    }

    /// **Audit C1/H2: the invariants that need the catalog PREIMAGE, not just its root.**
    ///
    /// Decision 1 lists "BASE-0 court coverage == 100%" and the catalog-root preimage check as
    /// boot conditions, and `validate` implemented neither — it could not: the bundle carries the
    /// ROOT, and only a genesis artifact carries the classes. `verify_against_catalog` is the
    /// entry point for the place that does hold them, and it is also what finally checks the last
    /// self-certified number in the bundle: an operator who understates
    /// `court.max_step_leaf_count` to shrink `window_court` now contradicts the catalog its own
    /// genesis committed to.
    #[test]
    fn the_catalog_gate_checks_what_the_bundle_alone_cannot() {
        let base = h64(1);
        let catalog = PalwClassCatalogV2::new(vec![catalog_entry(base, 1_048_576)]).unwrap();
        let mut bundle = conforming_bundle();
        bundle.class_catalog_root = catalog.root();
        bundle.verify_against_catalog(&catalog).expect("the committed catalog verifies");

        // A catalog that is not the one the ruleset committed to.
        let other = PalwClassCatalogV2::new(vec![catalog_entry(base, 1_048_575)]).unwrap();
        assert!(bundle.verify_against_catalog(&other).is_err(), "the root is a commitment, not a label");

        // BASE-0 absent: the liveness floor is unregistered.
        let without_base = PalwClassCatalogV2::new(vec![catalog_entry(h64(7), 1_024)]).unwrap();
        let mut b2 = conforming_bundle();
        b2.class_catalog_root = without_base.root();
        assert!(b2.verify_against_catalog(&without_base).is_err(), "BASE-0 must be registered");

        // A class that reaches a kernel this build cannot adjudicate: coverage is not 100%, so
        // its disputes would end `Unadjudicable` — a class that cannot be policed.
        let mut uncovered = catalog_entry(base, 1_024);
        uncovered.reachable_kernels.insert(h64(0xDEAD));
        let gapped = PalwClassCatalogV2::new(vec![uncovered]).unwrap();
        let mut b3 = conforming_bundle();
        b3.class_catalog_root = gapped.root();
        assert!(b3.verify_against_catalog(&gapped).is_err(), "a coverage gap must refuse the class set");

        // The court's asserted depth must cover the catalog's real one.
        let deeper = PalwClassCatalogV2::new(vec![catalog_entry(base, 1 << 30)]).unwrap();
        let mut b4 = conforming_bundle();
        b4.class_catalog_root = deeper.root();
        assert!(
            b4.verify_against_catalog(&deeper).is_err(),
            "asserting a shallower ladder than the catalog needs is the understatement this gate exists to catch"
        );

        // "A share-bearing class missing from the catalog" is no longer a params-side case:
        // shares are granted by `ClassRegistered` (ADR-0045 Decision 3), so a class cannot hold
        // a share without having been registered, and the genesis loader checks each
        // registration object against this same catalog.

        // Catalog shapes that cannot be a catalog.
        assert!(PalwClassCatalogV2::new(Vec::new()).is_err(), "an empty catalog registers nothing");
        assert!(
            PalwClassCatalogV2::new(vec![catalog_entry(h64(2), 1_024), catalog_entry(base, 1_024)]).is_err(),
            "unordered entries would give one catalog two roots"
        );
        assert!(PalwClassCatalogV2::new(vec![catalog_entry(base, 1)]).is_err(), "one leaf cannot be bisected");
    }

    /// **Audit H2: the frozen cadence is outside the ruleset id, so it is refused at the params.**
    ///
    /// `palw_ruleset_id_v2` hashes the bundle, and the bundle has no cadence field — yet every
    /// window in it is denominated in DAA score, so the cadence is what gives bind/receipt/
    /// challenge/court/withdrawal their wall-clock meaning. Two networks sharing a ruleset id
    /// could run measurably different rules, which is the exact opposite of what Decision 11
    /// promises. Until the cadence is inside the id, `validate_palw_v2` refuses the
    /// configurations where the silence would matter.
    #[test]
    fn a_v2_network_must_run_the_frozen_cadence() {
        use crate::config::params::SIMNET_PARAMS;
        let mut v2 = SIMNET_PARAMS.clone();
        v2.palw_consensus_mode = PalwConsensusMode::ConsensusV2(conforming_bundle());
        v2.blockrate.target_time_per_block = PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS / 2;
        // The finalizer gate (C1) fires first today, so aim the assertion at the clause under test
        // by checking the cadence predicate the gate uses.
        assert_ne!(v2.blockrate.target_time_per_block, PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS);
        assert!(v2.validate_palw_v2().is_err(), "an off-cadence V2 network must not boot");

        // And the constant is the decision's own number, not a stray default.
        assert_eq!(PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS, 120_000, "ADR-0038 Decision H: one block per 120 seconds");

        // Disabled and LegacyTn11 networks are unaffected — the clause is scoped to ConsensusV2,
        // so t11's own cadence is none of its business.
        let mut legacy = SIMNET_PARAMS.clone();
        legacy.palw_consensus_mode = PalwConsensusMode::LegacyTn11;
        legacy.blockrate.target_time_per_block = 1_000;
        legacy.validate_palw_v2().expect("the cadence freeze does not reach the legacy lineage");
    }

    /// The network domain is a pure function of the network id bytes, and different networks get
    /// different domains — which is what keeps a testnet object off mainnet even when the two
    /// share a ruleset id (Decision 11's deliberate omission).
    #[test]
    fn the_network_domain_separates_networks() {
        let mainnet = palw_network_domain_v2(b"mainnet");
        assert_eq!(mainnet, palw_network_domain_v2(b"mainnet"), "a pure function of the bytes");
        for other in [&b"testnet-11"[..], b"testnet-1", b"devnet", b"simnet", b""] {
            assert_ne!(mainnet, palw_network_domain_v2(other), "{other:?} must not share mainnet's domain");
        }
        // Length-prefixed, so no two ids concatenate into one another's preimage.
        assert_ne!(palw_network_domain_v2(b"testnet-1"), palw_network_domain_v2(b"testnet-11"));
    }

    /// Decision 11's property: any consensus-deciding byte moves the id, and network identity is
    /// not in the preimage at all (there is no field for it — RC and mainnet share the id by
    /// construction, and the challenge's network_domain keeps their blocks apart).
    #[test]
    fn every_bundle_byte_moves_the_ruleset_id() {
        let base_id = palw_ruleset_id_v2(&conforming_bundle());
        let mutations: Vec<(&str, Box<dyn Fn(&mut PalwConsensusParamsV2)>)> = vec![
            // Both halves move together: this row asks "does the carve reach the ruleset id",
            // and moving only `reward` would be refused by the coherence check before the
            // fingerprint is ever computed — a different question with the same name.
            (
                "reward carve",
                Box::new(|b| {
                    b.reward = PalwRewardParamsV2::new(621).unwrap();
                    b.state = b.state.clone().with_worker_carve_permille(621).unwrap();
                }),
            ),
            ("panel quorum", Box::new(|b| b.panel = PalwPanelParamsV2::new(3, 3, 4).unwrap())),
            // A different floor is a different ruleset — and the state params must follow it, or
            // the coherence clause refuses the bundle.
            (
                "bond floor",
                Box::new(|b| {
                    b.bond = PalwBondParamsV2::new(101, 2_000).unwrap();
                    b.state = state_params_with_min_collateral(101);
                }),
            ),
            ("reorg margin", Box::new(|b| b.reorg_margin_daa += 1)),
            // Still a valid bundle (22 rounds x 19 = 418 < 500), and a different one.
            (
                "court shape",
                Box::new(|b| {
                    b.court = PalwCourtParamsV2::new(1_048_576, 19, 2).unwrap();
                    b.state = b.state.clone().with_turn_deadline_daa(19).unwrap();
                }),
            ),
            ("exposure ratio", Box::new(|b| b.admission = PalwAdmissionParamsV2::new(501).unwrap())),
            (
                "free-prompt quantum",
                Box::new(|b| {
                    b.freeprompt = PalwFreePromptParamsV3::new(
                        crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
                        1_001,
                        10,
                        crate::palw_freeprompt_v3::PalwFpCuWeightsV3 { prefill_weight: 1, decode_weight: 64 },
                        64,
                        4_096,
                        512,
                        100,
                        200,
                        5,
                    )
                    .unwrap()
                }),
            ),
            (
                "free-prompt cu price",
                Box::new(|b| {
                    b.freeprompt = PalwFreePromptParamsV3::new(
                        crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
                        1_000,
                        10,
                        crate::palw_freeprompt_v3::PalwFpCuWeightsV3 { prefill_weight: 2, decode_weight: 64 },
                        64,
                        4_096,
                        512,
                        100,
                        200,
                        5,
                    )
                    .unwrap()
                }),
            ),
        ];
        // **A lane that cannot produce holds no cadence** (launch blockers §6) — asserted here
        // rather than only in the shipped bundle, so a future edit that reintroduces a receipt
        // permille fails a test instead of shipping a chain that stops.
        //
        // The retarget measures each lane against the COMBINED census. An unproducible lane's
        // permille stays in the expectation while its blocks are missing from the total, so the
        // attempt lane holds 100% of what happened while being expected to hold `split` of it —
        // an over-producer verdict at EVERY epoch boundary, each dividing the target by up to
        // `class_daa_max_factor`, until it reaches the floor of 1 and the class lottery refuses
        // every attempt. The shipped bundle carried 150‰ and would have stopped ~63 epochs in.
        for bad_split in [1u16, 150, 800, 999] {
            let mut bundle = conforming_bundle();
            bundle.state = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, bundle.base_class_id, 4, 1000, 100, bad_split, 0)
                .unwrap()
                .with_worker_carve_permille(620)
                .unwrap()
                .with_turn_deadline_daa(20)
                .unwrap()
                .with_claim_retirement_daa(200)
                .unwrap();
            assert!(
                bundle.validate().is_err(),
                "a {bad_split}permille attempt share leaves the receipt lane a cadence it can never produce"
            );
        }

        for (name, mutate) in mutations {
            let mut bundle = conforming_bundle();
            mutate(&mut bundle);
            assert!(bundle.validate_ruleset_shape().is_ok(), "{name}: this mutation is a VALID other ruleset");
            assert_ne!(palw_ruleset_id_v2(&bundle), base_id, "{name}: a consensus byte moved and the id did not");
        }
    }
}
