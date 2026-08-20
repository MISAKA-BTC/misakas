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
use crate::palw_mode_v2::{PalwBondParamsV2, PalwConsensusParamsV2, PalwModeV2Error};
use crate::palw_panel_v2::PalwPanelParamsV2;
use crate::palw_reward_v2::PalwRewardParamsV2;
use crate::palw_state_v2::{PalwClassDaaV2Params, PalwStateParamsV2};

/// Immature live-weight fraction. 100‰: a fresh tip counts, but a private fork stacking
/// unresolved claims gains a tenth of what resolving them would.
const BETA_PERMILLE: u16 = 100;

/// Lattice windows, in DAA score. Sized against a 1-DAA-per-block devnet so a full
/// commit→bind→license→challenge→court cycle is minutes, not days:
/// bind 600, receipt 600, challenge 1200, court 2400.
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

/// Worst-case honest prosecution time. **Still declared, not measured** — and deliberately named
/// as such rather than quietly presented beside the numbers that now are.
///
/// Measuring it needs an end-to-end court: a real refutation opened against a real receipt,
/// bisected to a step, adjudicated. The CU calibration above could be measured because the
/// gateway and worker are the whole of that path; this one cannot until the court runs on a
/// fleet. Until then the gate (`window_court > this`) treats the ratio here — 2400 vs 1200 — as
/// the safety factor, and the constant is the thing a fleet drill replaces first.
const WORST_CASE_COURT: u64 = 1_200;

/// CU pricing (ADR-0044 Decision 7), **measured** — see `scripts/misaka-palw-fp-cu-calibrate.py`.
///
/// The safety direction is one-sided. CU per second of real work is `weight / cost` per phase, so
/// prefill becomes a grinding lever exactly when `prefill_weight / b > decode_weight / c`, where
/// `b` and `c` are the measured seconds per prompt token and per decode token. Rearranged, the
/// rule the table must satisfy is
///
/// ```text
/// CU_DECODE_WEIGHT / CU_PREFILL_WEIGHT  >=  c / b     for every producer's hardware
/// ```
///
/// — and since it must hold for EVERY producer, the binding number is the LARGEST `c/b` anyone
/// can bring, not the average. Measured on the reference model (Qwen3.5-2B-Q4_K_M) by fitting
/// `T(p, d) = a + b·p + c·d` over a 24-run grid, 2026-08-20, Apple M-series:
///
/// ```text
///   CPU build (the registered class):  b = 3.35 ms/tok, c = 26.81 ms/tok  ->  c/b =  8.0
///   Metal build (not a valid class):   b = 0.92 ms/tok, c = 10.95 ms/tok  ->  c/b = 11.9
/// ```
///
/// So 1 : 64 stands, with ~5× headroom over the class's own measurement and ~5× over the fastest
/// backend measured. The headroom is not slack: prefill parallelises across cores while decode is
/// memory-bandwidth bound, so a wide server CPU inside the same class has a materially higher
/// `c/b` than the laptop this was measured on, and that node is the one that would find the
/// lever. Raising the prefill weight toward exact pricing is a consensus change and needs the
/// measurement re-run on the widest CPU in the class — the script exists for that.
const CU_PREFILL_WEIGHT: u32 = 1;
const CU_DECODE_WEIGHT: u32 = 64;

/// One quantum of certified work. Measured against real jobs on the reference model (same run as
/// the weights above), this is what an ordinary chat actually earns:
///
/// ```text
///   p= 33  d= 24  ->  cu =  1_569  ->  1 quantum    (a short question)
///   p=100  d= 16  ->  cu =  1_124  ->  1 quantum
///   p=100  d=128  ->  cu =  8_292  ->  8 quanta     (a paragraph of answer)
///   p=360  d=128  ->  cu =  8_552  ->  8 quanta
/// ```
///
/// A handful of draws rather than one all-or-nothing ticket, which is the variance the
/// quantization exists to smooth — and never so many that one job dominates a window.
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
const MIN_COLLATERAL_SOMPI: u64 = 20_000;
const WITHDRAWAL_DELAY: u64 = 6_000;

/// The worker share of the fixed subsidy (a carve, never an addition).
const WORKER_CARVE_PERMILLE: u16 = 620;

/// BASE-0's registered artifact root on a devnet.
///
/// A PLACEHOLDER, and named one: the real value is the hash of the registered BASE-0 artifact
/// set, which arrives with the class registration itself. A devnet drill exercises the wiring,
/// not the arithmetic, and a made-up value that pretended to be measured would be worse than one
/// that says what it is.
fn base0_artifact_root_placeholder() -> Hash64 {
    Hash64::from_u64_word(0xBA5E_0A27)
}
/// Sompi of collateral one pwu puts at stake.
const SLASH_VALUE_PER_PWU: u64 = 5;
/// The class rule's ceiling (ADR-0039's real PWU derivation is still its own record).
const MAX_PWU_PER_ATTEMPT: u64 = 1_000_000;
/// The class's initial difficulty target: the whole 128-bit space, so a fresh chain produces
/// before its DAA has measured anything. The retarget takes over from the first closed epoch.
const INITIAL_CLASS_TARGET: u128 = u128::MAX;
/// What a genesis bond stakes.
const GENESIS_BOND_COLLATERAL: u64 = 1_000_000;

/// The devnet bundle with its catalog root DERIVED — for tests and drills that want a valid
/// bundle without hand-typing a hash.
///
/// A preset must NOT use this: it should state the root it believes in and let construction
/// refuse a mismatch (carrying both the root and its preimage is what makes that refusal
/// possible). This exists so a test is not forced to hard-code a value that moves whenever a
/// class constant does.
pub fn palw_fp_devnet_bundle_derived_root_v3(
    base_class_id: Hash64,
    court_catalog_root: Hash64,
    genesis_bond: crate::tx::TransactionOutpoint,
    genesis_bond_pubkey: Vec<u8>,
    genesis_operator_id: Hash64,
) -> Result<PalwConsensusParamsV2, PalwModeV2Error> {
    let classes = vec![crate::palw_mode_v2::PalwGenesisClassV2 {
        class_id: base_class_id,
        artifact_root: base0_artifact_root_placeholder(),
        slash_value_per_pwu: SLASH_VALUE_PER_PWU,
        pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(MAX_PWU_PER_ATTEMPT),
        initial_target: INITIAL_CLASS_TARGET,
    }];
    let root = crate::palw_mode_v2::palw_class_catalog_root_v2(&classes);
    palw_fp_devnet_bundle_with_genesis_bond_v3(
        base_class_id,
        root,
        court_catalog_root,
        genesis_bond,
        genesis_bond_pubkey,
        genesis_operator_id,
    )
}

/// The devnet bundle. `base_class_id` is the caller's — the class id is a genesis artifact (the
/// registered BASE-0 class), and inventing one here would put a second source of that fact in
/// the tree. `class_catalog_root` / `court_catalog_root` likewise come from the genesis that
/// registers them; the boot path verifies their preimages.
///
/// Single-class table (BASE-0 at 1000‰), which is the honest devnet shape: an accelerated class
/// with `coverage < 100%` carries share 0 at RC anyway (ADR-0042 Decision 8).
pub fn palw_fp_devnet_bundle_v3(
    base_class_id: Hash64,
    class_catalog_root: Hash64,
    court_catalog_root: Hash64,
) -> Result<PalwConsensusParamsV2, PalwModeV2Error> {
    // A bundle with no usable genesis bond cannot produce a block; this convenience form is for
    // tests of the PARAMETERS, so it registers a placeholder key. A network takes the
    // `_with_genesis_bond` form and supplies the real one.
    palw_fp_devnet_bundle_with_genesis_bond_v3(
        base_class_id,
        class_catalog_root,
        court_catalog_root,
        crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(0xB0), 0),
        vec![0x11; 32],
        Hash64::from_u64_word(0xE0),
    )
}

/// [`palw_fp_devnet_bundle_v3`] with the genesis bond named explicitly — the form a preset uses,
/// because the bond's PUBLIC KEY is a network artifact (some operator holds the secret) and a
/// library cannot invent one.
///
/// `class_catalog_root` is still passed in and still CHECKED against the class list this builds:
/// the caller states what it believes the catalog is, and construction refuses if the belief and
/// the classes disagree. Deriving it silently would turn a mismatched preset into a working one.
pub fn palw_fp_devnet_bundle_with_genesis_bond_v3(
    base_class_id: Hash64,
    class_catalog_root: Hash64,
    court_catalog_root: Hash64,
    genesis_bond: crate::tx::TransactionOutpoint,
    genesis_bond_pubkey: Vec<u8>,
    genesis_operator_id: Hash64,
) -> Result<PalwConsensusParamsV2, PalwModeV2Error> {
    let class_daa = PalwClassDaaV2Params::new([(base_class_id, 1000u16)].into_iter().collect(), 4)?;
    let state = PalwStateParamsV2::new(
        BETA_PERMILLE,
        WINDOW_BIND,
        WINDOW_RECEIPT,
        WINDOW_CHALLENGE,
        WINDOW_COURT,
        EPOCH_LENGTH,
        ATTEMPT_SHARE_PERMILLE,
        class_daa,
    )?;
    // The epoch budget: what one class may produce per epoch, in pwu. Sized so a full epoch of
    // receipt blocks at `PWU_PER_QUANTUM` fits with headroom — a budget that binds before the
    // difficulty does would make the DAA a decoration.
    let admission = PalwAdmissionParamsV2::new(
        MAX_EXPOSURE_RATIO_PERMILLE,
        [(base_class_id, (EPOCH_LENGTH as u128) * (PWU_PER_QUANTUM as u128) * 4)].into_iter().collect(),
    )?;
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
    // The one class a devnet registers: BASE-0, at the initial target the whole space admits so a
    // fresh chain can actually produce its first blocks before the DAA has measured anything.
    let genesis = crate::palw_mode_v2::PalwGenesisRegistrationsV2 {
        classes: vec![crate::palw_mode_v2::PalwGenesisClassV2 {
            class_id: base_class_id,
            artifact_root: base0_artifact_root_placeholder(),
            slash_value_per_pwu: SLASH_VALUE_PER_PWU,
            pwu_rule: crate::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(MAX_PWU_PER_ATTEMPT),
            initial_target: INITIAL_CLASS_TARGET,
        }],
        bonds: vec![crate::palw_mode_v2::PalwGenesisBondV2 {
            bond: genesis_bond,
            pubkey: genesis_bond_pubkey,
            operator_id: genesis_operator_id,
            collateral: GENESIS_BOND_COLLATERAL,
        }],
    };
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
        genesis,
        reorg_margin_daa: REORG_MARGIN,
        worst_case_court_duration_daa: WORST_CASE_COURT,
    };
    // Validated HERE, so the constructor cannot hand back a bundle that would refuse to boot:
    // a caller holding an `Ok` holds a bundle a node will start on.
    bundle.validate()?;
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_mode_v2::{PalwConsensusMode, palw_ruleset_id_v2};

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bundle() -> PalwConsensusParamsV2 {
        devnet_bundle_for_test(h64(0xBA5E))
    }

    /// The catalog root is checked against the class list, so a test that wants a valid bundle
    /// must derive it — which is the point: a preset that types the wrong root does not boot.
    fn devnet_bundle_for_test(base_class_id: Hash64) -> PalwConsensusParamsV2 {
        // A catalog root that does not open must not validate — the check this module makes.
        assert!(
            palw_fp_devnet_bundle_v3(base_class_id, h64(0xCA7), h64(0xC0757)).is_err(),
            "a catalog root that does not open must not validate"
        );
        palw_fp_devnet_bundle_derived_root_v3(
            base_class_id,
            h64(0xC0757),
            crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(0xB0), 0),
            vec![0x11; 32],
            h64(0xE0),
        )
        .expect("the devnet bundle validates")
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
        assert!(s.window_court() > b.worst_case_court_duration_daa, "an honest prosecution fits its window");
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
        assert_ne!(id, palw_ruleset_id_v2(&devnet_bundle_for_test(h64(0xBA5F))), "a different base class is a different ruleset");
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
        v2.validate_palw_v2().expect("a pure V2 params set carrying the devnet bundle validates");

        // …and the mixed-lineage refusal still bites: DEVNET runs V1 PALW PoW, so a V2 mode on it
        // is half of two lineages.
        let mut mixed = DEVNET_PARAMS.clone();
        mixed.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle());
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
