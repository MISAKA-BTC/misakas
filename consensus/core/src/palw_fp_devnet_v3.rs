//! A **buildable, validated** free-prompt devnet bundle (ADR-0044 FP-09, stage 1).
//!
//! ADR-0042's own PR-10 note left the conforming bundle as a `pub(crate)` test fixture, so no
//! crate outside `kaspa-consensus-core` could construct a `ConsensusV2` params set at all — which
//! meant the first real assembly of one would happen under deadline, in a preset, by hand. This
//! module is that assembly done early and in the open: one public constructor, every value
//! commented with WHY it holds, and a test that runs the whole Decision-1 + ADR-0044 startup
//! gate over it.
//!
//! **These are devnet-shaped numbers, and the module name says so.** They are chosen for a
//! machine-paced devnet where a fraud drill must finish inside a coffee break, NOT measured on a
//! fleet: mainnet/RC parameters are soak outputs (ADR-0036), and the fleet drill (FP-09 stage 2)
//! is what replaces them. What this bundle proves today is narrower and still worth having: that
//! the constraints *interlock* — a set of windows, shares, splits and beacon gaps that satisfies
//! every invariant simultaneously EXISTS, and here is one. When the measured numbers arrive they
//! are edits to these constants, not a new discovery that the gate is unsatisfiable.
//!
//! Nothing here is installed on any preset. A network gets this bundle only by an explicit
//! `palw_consensus_mode = ConsensusV2(...)`, and no shipped preset does that.

use crate::Hash64;
use crate::palw_admission_v2::PalwAdmissionParamsV2;
use crate::palw_freeprompt_v3::{PalwFpCuWeightsV3, PalwFreePromptParamsV3};
use crate::palw_mode_v2::{
    PALW_V2_FORK_CHOICE_VERSION, PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS, PALW_V2_TRACE_FORMAT_VERSION, PalwBondParamsV2,
    PalwConsensusParamsV2, PalwCourtParamsV2, PalwModeV2Error, palw_v2_signature_contexts_root,
};
use crate::palw_panel_v2::PalwPanelParamsV2;
use crate::palw_reward_v2::PalwRewardParamsV2;
use crate::palw_state_v2::PalwStateParamsV2;

/// Immature live-weight fraction. 100‰: a fresh tip counts, but a private fork stacking
/// unresolved claims gains a tenth of what resolving them would.
const BETA_PERMILLE: u16 = 100;

/// Lattice windows, in DAA score: bind 600, receipt 600, challenge 1200, court 2400.
///
/// **What these mean in wall-clock changed when the bundle got its cadence field.** They were
/// sized against a 1-DAA-per-block devnet as "a full commit→bind→license→challenge→court cycle
/// in minutes". A `ConsensusV2` network runs the frozen 120 s cadence (ADR-0038 Decision H,
/// enforced by `validate_palw_v2`), so at one DAA per block the same numbers are hours: bind is
/// 20 h and the full cycle about six days. That is the point of putting the cadence inside the
/// ruleset id — the windows never had wall-clock meaning on their own, and this constant block
/// is where the two are read together. A shorter devnet cycle is a decision about these
/// numbers, made in daylight; it is not something the cadence should be bent for.
const WINDOW_BIND: u64 = 600;
const WINDOW_RECEIPT: u64 = 600;
const WINDOW_CHALLENGE: u64 = 1_200;
const WINDOW_COURT: u64 = 2_400;

/// Class-production epoch / retarget span.
const EPOCH_LENGTH: u64 = 1_000;

/// ADR-0044 Decision 1's floor: 150‰ of combined production is the attempt lane, the beacon
/// source. Low enough that free-prompt work dominates the chain's weight; high enough that
/// beacons keep arriving (~1 in 7 blocks) and that `max_beacon_gap` below is a promise the lane
/// can keep.
const ATTEMPT_SHARE_PERMILLE: u16 = 150;

/// The panel's anchor delay: 20 DAA after acceptance the claim's beacon slot opens.
const ANCHOR_DELAY: u64 = 20;
/// Declared worst-case wait for the first attempt-class block at or after a slot. At a 150‰
/// floor the expected gap is ~7 blocks; 400 is a wide margin, and the startup gate proves
/// `ANCHOR_DELAY + this < WINDOW_BIND`, so even a very unlucky lull still binds in time.
const MAX_BEACON_GAP: u64 = 400;

/// Draw maturity and use window. Maturity must cover the reorg margin (the draw beacon must sit
/// past the reorgable fringe of the certification it draws for); the use window is generous
/// because a producer that misses it loses the win outright.
const REORG_MARGIN: u64 = 300;
const RECEIPT_MATURITY: u64 = 400;
const RECEIPT_USE_WINDOW: u64 = 600;

/// Measured worst-case honest prosecution time — a PLACEHOLDER shaped like the real thing: the
/// gate demands `window_court > this`, so the constant is what a fleet measurement replaces, and
/// until it does, the ratio here (2400 vs 1200) is the safety factor.
/// The court's SHAPE, from which `worst_case_duration_daa` is derived (ADR-0042 Decision 8:
/// `(ceil(log2(leaves)) + terminal) × turn_deadline`).
///
/// **The whole step space, not a number sized for today's catalog** (ADR-0049 Decision C,
/// `assemble_palw_rc_identity_v2` gate 5). This was `1 << 16`, chosen when BASE-0's graph was
/// eighteen steps per layer; declaring the narrowings the engine performs took its longest job to
/// 366,728 leaves and the ladder could no longer reach its own liveness floor. Sizing a ladder to
/// the class set of the day is how that happens, and the value is inside `palw_ruleset_id_v2`, so
/// it cannot be raised afterwards.
///
/// 2^22 leaves = 22 bisection rounds, +2 terminal, × 60 DAA per turn = 1,440 — still inside
/// `WINDOW_COURT` (2,400). Nothing deeper than the cap is admissible at all, so this ladder cannot
/// fail to reach a class that exists.
const COURT_MAX_STEP_LEAVES: u64 = crate::palw_step::PALW_STEP_MAX_LEAVES;
const COURT_TURN_DEADLINE: u64 = 60;
const COURT_TERMINAL_ROUNDS: u32 = 2;

/// CU pricing (ADR-0044 Decision 7). Prefill is batched and roughly an order of magnitude cheaper
/// per token than decode, and the invariant is that mispricing may only ever UNDER-pay — so the
/// prefill weight starts at the conservative end (1 : 64).
const CU_PREFILL_WEIGHT: u32 = 1;
const CU_DECODE_WEIGHT: u32 = 64;

/// One quantum of certified work. At 1:64 pricing a ~100-token prompt with a 256-token answer is
/// ~16.5k CU ≈ 16 quanta, so an ordinary chat job earns a handful of draws rather than one
/// all-or-nothing ticket — which is the variance the quantization exists to smooth.
const QUANTUM_CU: u128 = 1_000;
/// Chain weight one spent quantum contributes.
const PWU_PER_QUANTUM: u64 = 100;
/// Per-receipt jackpot bound: a single enormous job cannot buy unbounded consecutive blocks.
const MAX_QUANTA_PER_RECEIPT: u32 = 64;

/// Shape caps, inside the worker's own limits (single-batch prefill 512, trace-event cap 4096).
const MAX_PROMPT_TOKENS: u32 = 512;
const MAX_DECODE_TOKENS: u32 = 1_024;

/// Per-bond exposure ceiling, in permille of slashable collateral. 500‰: a bond may back
/// immature claims worth at most half its collateral, so the first slash always lands inside
/// what the bond can actually pay.
const MAX_EXPOSURE_RATIO_PERMILLE: u32 = 500;

/// Devnet bond floor and withdrawal delay. The delay must outlast bind+receipt+challenge+court
/// plus the reorg margin (5100 here), so 6000 leaves margin without making a devnet operator
/// wait a week to leave.
/// Sized against what a claim actually RESERVES, which is the number that binds: at
/// `initial_target = u128::MAX/2` the derivation yields pwu = 2 × `pwu_per_inference`, and a claim
/// reserves `pwu × slash_value_per_pwu`. At 4,096 per inference and 5 per pwu that is 40,960 per
/// claim, so a bond must carry at least `40_960 / (500‰)` = 81,920 to make ONE claim — and the
/// floor below funds four concurrent ones.
///
/// The old 20,000 was chosen before the exposure ceiling had a consumer, and it made every
/// attempt on this bundle refusable: `reserved 0 + 40960 > ceiling 10000`. Measured the moment
/// P0-10's check was wired into the pipeline, which is exactly what that check is for.
const MIN_COLLATERAL_SOMPI: u64 = 400_000;
const WITHDRAWAL_DELAY: u64 = 6_000;

/// Per-adjustment retarget clamp (ADR-0038 Decision D) and the ADR-0045 Decision 2 epoch-budget
/// tolerance, in permille of a class's cadence share. Unity is the floor (below it a budget
/// starves its own class); 1000‰ is the devnet's honest "exactly its share" setting.
const CLASS_DAA_MAX_FACTOR: u32 = 4;
const BUDGET_TOLERANCE_PERMILLE: u32 = 1_000;

/// Audit C5's abandon hold, in DAA: how long a free-prompt commitment abandoned at `BindTimeout`
/// keeps its collateral reserved. One bind window (600) — the span the producer had to bind and
/// chose not to use, which is the natural price for declining it, and long enough that a redraw
/// loop needs a fresh reservation per attempt rather than recycling one.
const FP_ABANDON_HOLD: u64 = WINDOW_BIND;

/// The worker share of the fixed subsidy (a carve, never an addition).
const WORKER_CARVE_PERMILLE: u16 = 620;

/// The devnet bundle. `base_class_id` is the caller's — the class id is a genesis artifact (the
/// registered BASE-0 class), and inventing one here would put a second source of that fact in
/// the tree. `class_catalog_root` / `court_catalog_root` likewise come from the genesis that
/// registers them; the boot path verifies their preimages.
///
/// Single-class table (BASE-0 at 1000‰), which is the honest devnet shape: an accelerated class
/// with `coverage < 100%` carries share 0 at RC anyway (ADR-0042 Decision 8).
/// The initial per-class DAA target the genesis registration seeds. Deliberately easy: a devnet
/// that cannot win its own lottery produces nothing, and the retarget moves it from here.
const GENESIS_CLASS_TARGET: u128 = u128::MAX / 2;
/// Slashable value per pwu — what an exposure ceiling is measured in.
const SLASH_VALUE_PER_PWU: u64 = 5;

pub fn palw_fp_devnet_bundle_v3(
    base_class_id: Hash64,
    class_catalog_root: Hash64,
    court_catalog_root: Hash64,
    // The catalog's counted canonical step-leaf count for `base_class_id` — the value
    // `verify_palw_genesis_v2` demands the registration declare.
    genesis_pwu_per_inference: u64,
    genesis_artifact_root: Hash64,
    genesis_bond: crate::palw_state_v2::PalwBondKeyV2,
    genesis_bond_pubkey: Vec<u8>,
    genesis_operator_pubkey: Vec<u8>,
    // Where the genesis bond's matured rewards are paid — the 64-byte P2PKH-ML-DSA-87 owner
    // payload. An argument rather than a value derived from `genesis_operator_pubkey`, because
    // paying rewards to a different key than the one that signs is an ordinary operational
    // choice (a cold payout address), and deriving it would quietly forbid it.
    genesis_payout_payload: Hash64,
) -> Result<PalwConsensusParamsV2, PalwModeV2Error> {
    // ADR-0045 Decision 3: no share table here. The chain grants shares at registration — the
    // first registration on the chain must be `base_class_id` at the whole 1000‰, which is the
    // single-class devnet shape this bundle used to spell out as a params constant.
    let state = PalwStateParamsV2::new(
        BETA_PERMILLE,
        WINDOW_BIND,
        WINDOW_RECEIPT,
        WINDOW_CHALLENGE,
        WINDOW_COURT,
        EPOCH_LENGTH,
        base_class_id,
        CLASS_DAA_MAX_FACTOR,
        BUDGET_TOLERANCE_PERMILLE,
        MIN_COLLATERAL_SOMPI,
        ATTEMPT_SHARE_PERMILLE,
        FP_ABANDON_HOLD,
    )?
    // The SAME constants the `reward` and `court` fields below declare — `validate()` requires
    // each pair to agree, so these are not second sources, they are the one source reaching both
    // readers. `COURT_TURN_DEADLINE` here is what turns the interactive ladder ON: it is strictly
    // inside `WINDOW_COURT`, which is what makes a rung deadline able to fire at all.
    .with_worker_carve_permille(WORKER_CARVE_PERMILLE)?
    .with_turn_deadline_daa(COURT_TURN_DEADLINE)?;
    // The epoch budget: what one class may produce per epoch, in pwu. Sized so a full epoch of
    // receipt blocks at `PWU_PER_QUANTUM` fits with headroom — a budget that binds before the
    // difficulty does would make the DAA a decoration.
    let admission = PalwAdmissionParamsV2::new(MAX_EXPOSURE_RATIO_PERMILLE)?;
    let freeprompt = PalwFreePromptParamsV3::new(
        crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3,
        QUANTUM_CU,
        PWU_PER_QUANTUM,
        PalwFpCuWeightsV3 { prefill_weight: CU_PREFILL_WEIGHT, decode_weight: CU_DECODE_WEIGHT },
        MAX_QUANTA_PER_RECEIPT,
        MAX_PROMPT_TOKENS,
        MAX_DECODE_TOKENS,
        RECEIPT_MATURITY,
        RECEIPT_USE_WINDOW,
        MAX_BEACON_GAP,
    )?;
    let bundle = PalwConsensusParamsV2 {
        protocol_version: crate::palw_attempt_v2::PALW_ATTEMPT_V2_VERSION,
        algorithm_id: crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2,
        base_class_id,
        class_catalog_root,
        court_catalog_root,
        state,
        admission,
        panel: PalwPanelParamsV2::new(5, 3, ANCHOR_DELAY)?,
        reward: PalwRewardParamsV2::new(WORKER_CARVE_PERMILLE)?,
        bond: PalwBondParamsV2::new(MIN_COLLATERAL_SOMPI, WITHDRAWAL_DELAY)?,
        freeprompt,
        reorg_margin_daa: REORG_MARGIN,
        court: PalwCourtParamsV2::new(COURT_MAX_STEP_LEAVES, COURT_TURN_DEADLINE, COURT_TERMINAL_ROUNDS)?,
        cadence_target_time_per_block_ms: PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS,
        fork_choice_version: PALW_V2_FORK_CHOICE_VERSION,
        trace_format_version: PALW_V2_TRACE_FORMAT_VERSION,
        signature_contexts_root: palw_v2_signature_contexts_root(),
        // The genesis artifact's own registrations: the liveness floor at the whole 1000‰, and
        // one bond to execute under. Without them the network cannot produce its first block —
        // admission refuses an attempt naming a bond the chain does not have — so a bundle
        // carrying none is a bundle that boots and then stalls.
        //
        // `pwu_per_inference` is the catalog's counted canonical step-leaf count, which
        // `verify_palw_genesis_v2` checks: the declaration is not the fact.
        genesis_objects: vec![
            crate::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                class_id: base_class_id,
                artifact_root: genesis_artifact_root,
                slash_value_per_pwu: SLASH_VALUE_PER_PWU,
                pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference: genesis_pwu_per_inference },
                initial_target: GENESIS_CLASS_TARGET,
                share_permille: 1000,
                activation_daa: 0,
            },
            crate::palw_state_v2::PalwConsensusObjectV2::BondRegistered {
                bond: genesis_bond,
                pubkey: genesis_bond_pubkey,
                operator_pubkey: genesis_operator_pubkey,
                collateral: MIN_COLLATERAL_SOMPI,
                payout_payload: genesis_payout_payload,
            },
        ],
    };
    // Validated HERE, so the constructor cannot hand back a bundle that would refuse to boot:
    // a caller holding an `Ok` holds a bundle a node will start on.
    bundle.validate()?;
    Ok(bundle)
}

/// The devnet bundle with the fixture identities every caller in this tree used before the
/// genesis registrations became part of the ruleset. One helper rather than eight arguments
/// repeated at every call site, and `pub(crate)` so the fixture is the SAME one everywhere — two
/// fixtures for one bundle is how two tests come to disagree about a ruleset id.
#[cfg(test)]
pub(crate) fn palw_fp_devnet_bundle_for_tests(
    base_class_id: Hash64,
    class_catalog_root: Hash64,
    court_catalog_root: Hash64,
) -> Result<PalwConsensusParamsV2, PalwModeV2Error> {
    palw_fp_devnet_bundle_v3(
        base_class_id,
        class_catalog_root,
        court_catalog_root,
        4_096,
        Hash64::from_u64_word(0xA7),
        crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint {
            transaction_id: crate::tx::TransactionId::from_u64_word(0xB0),
            index: 0,
        }),
        vec![7; 4],
        vec![21; 8],
        Hash64::from_u64_word(0x9A11),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_mode_v2::{PalwConsensusMode, palw_ruleset_id_v2};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bundle() -> PalwConsensusParamsV2 {
        palw_fp_devnet_bundle_v3(
            h64(0xBA5E),
            h64(0xCA7),
            h64(0xC0757),
            4_096,
            h64(0xA7),
            crate::palw_state_v2::PalwBondKeyV2(crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_u64_word(0xB0),
                index: 0,
            }),
            vec![7; 4],
            vec![21; 8],
            h64(0x9A11),
        ).expect("the devnet bundle validates")
    }

    /// **The interlock exists.** Every Decision-1 and ADR-0044 startup invariant holds on one
    /// concrete parameter set — the claim this module was written to make checkable.
    #[test]
    fn the_devnet_bundle_boots() {
        let b = bundle();
        b.validate().expect("validated at construction, and again here");

        // Spot-check the interlocks by hand, so a future edit that satisfies `validate()` by
        // accident still has to face the arithmetic that made these numbers a set.
        let s = &b.state;
        assert!(b.panel.anchor_delay() + b.freeprompt.max_beacon_gap_daa() < s.window_bind(), "a late beacon still binds");
        let worst_case = b.court.worst_case_duration_daa().expect("the court shape has a finite worst case");
        assert!(s.window_court() > worst_case, "an honest prosecution fits its window");
        assert!(b.freeprompt.receipt_maturity_daa() >= b.reorg_margin_daa, "the draw beacon sits past the reorgable fringe");
        let liability = s.window_bind() + s.window_receipt() + s.window_challenge() + s.window_court() + b.reorg_margin_daa;
        assert!(b.bond.withdrawal_delay_daa() > liability, "a bond cannot leave before its fraud is provable");
        assert!((1..=999).contains(&s.fp_attempt_share_permille()), "both lanes exist");
    }

    /// The bundle is a *ruleset*: it fingerprints, it demands the V2 attempt id, and it accepts
    /// exactly the two lanes' ids.
    #[test]
    fn the_devnet_bundle_is_one_checkable_ruleset() {
        let b = bundle();
        let id = palw_ruleset_id_v2(&b);
        assert_eq!(id, palw_ruleset_id_v2(&bundle()), "the fingerprint is a pure function of the values");
        assert_ne!(id, palw_ruleset_id_v2(&palw_fp_devnet_bundle_for_tests(h64(0xBA5E), h64(0xCA8), h64(0xC0757)).unwrap()));
        assert_eq!(
            PalwConsensusMode::ConsensusV2(b.clone()).required_algo_id(),
            Some(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2)
        );
        assert!(b.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2));
        assert!(b.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_PALW_RECEIPT_V3));
        assert!(!b.accepts_algo_id(crate::pow_layer0::POW_ALGO_ID_KHEAVYHASH));
    }

    /// A params set carrying this bundle passes the config gate — the mixed-lineage rule
    /// included — so the first real preset assembly is a value edit, not a discovery.
    #[test]
    fn a_params_set_carrying_it_passes_the_config_gate() {
        use crate::config::params::{DEVNET_PARAMS, SIMNET_PARAMS};
        let mut v2 = SIMNET_PARAMS.clone();
        v2.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle());
        // ADR-0038 Decision H: a ConsensusV2 network runs the frozen cadence, and the bundle's
        // own `cadence_target_time_per_block_ms` must equal the network's — the two are one fact
        // in two places, which is what the H2 fix made checkable.
        v2.blockrate.target_time_per_block = PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS;
        v2.validate_palw_v2().expect("a pure V2 params set carrying the devnet bundle validates");

        // …and the mixed-lineage refusal still bites: DEVNET runs V1 PALW PoW, so a V2 mode on it
        // is half of two lineages.
        let mut mixed = DEVNET_PARAMS.clone();
        mixed.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle());
        mixed.blockrate.target_time_per_block = PALW_V2_FROZEN_TARGET_TIME_PER_BLOCK_MS;
        assert!(mixed.validate_palw_v2().is_err());
    }

    /// The economics the numbers encode, asserted rather than assumed: an ordinary chat job earns
    /// several draws (variance smoothing), a tiny job earns none but still certifies, and the
    /// per-receipt cap bounds a huge one.
    #[test]
    fn the_pricing_lands_where_the_comments_claim() {
        use crate::palw_freeprompt_v3::{fp_cu_v3, fp_quanta_v3};
        let b = bundle();
        let w = b.freeprompt.cu_weights();

        let chat = fp_cu_v3(100, 256, w);
        let chat_quanta = fp_quanta_v3(chat, b.freeprompt.quantum_cu(), b.freeprompt.max_quanta_per_receipt());
        assert_eq!(chat, 100 + 256 * 64);
        assert!((8..=32).contains(&chat_quanta), "an ordinary chat job earns a handful of draws, got {chat_quanta}");

        let tiny = fp_cu_v3(8, 4, w);
        assert_eq!(fp_quanta_v3(tiny, b.freeprompt.quantum_cu(), b.freeprompt.max_quanta_per_receipt()), 0, "sub-quantum draws nothing");

        let huge = fp_cu_v3(b.freeprompt.max_prompt_tokens(), b.freeprompt.max_decode_tokens(), w);
        assert_eq!(
            fp_quanta_v3(huge, b.freeprompt.quantum_cu(), b.freeprompt.max_quanta_per_receipt()),
            b.freeprompt.max_quanta_per_receipt(),
            "the largest admissible job is capped, not unbounded"
        );

        // The derivation the acceptance layer runs: quanta and total pwu together, uniform by
        // construction (the state machine refuses a non-uniform commitment).
        let (quanta, pwu) = b.freeprompt.derive_quanta_and_pwu(chat).expect("a chat job derives");
        assert_eq!(quanta, chat_quanta);
        assert_eq!(pwu, (quanta as u64) * b.freeprompt.pwu_per_quantum());
        assert!(pwu % (quanta as u64) == 0 && pwu / (quanta as u64) > 0, "uniform quanta, as the state machine demands");
        assert!(b.freeprompt.derive_quanta_and_pwu(tiny).is_none(), "a sub-quantum job never enters the chain");
    }
}
