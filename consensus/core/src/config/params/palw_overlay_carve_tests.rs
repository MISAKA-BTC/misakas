//! **`params`'s `palw_overlay_carve_tests` module**, moved out of `params.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).
#![cfg(test)]

use super::*;
use crate::dns_finality::split_block_subsidy;
use crate::palw_reward_v2::{PalwRewardParamsV2, palw_reward_carve_v2};

/// testnet-11's block: `YEAR1_PER_BLOCK_TWO_MINUTE`, 4,445.62 MSK.
const T11_SUBSIDY: u64 = 444_562_014_000;
const T11_HEIGHT: u64 = PALW_RC_PALW_UPGRADE_FENCE_DAA;

fn t11_carve() -> PalwOverlayCarveV1 {
    PalwOverlayCarveV1 { activation: ForkActivation::new(T11_HEIGHT), subsidy_validator_bps: 2_000, worker_carve_permille: 720 }
}

/// **`E`, as the card computes it** (the Activation Pool's P1 derives `b_cap` from it): testnet-12's
/// first-block subsidy through its own overlay carve is `PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI`,
/// 3,200.85 MSK — and testnet-11's carve is the same constant, so the literal left no preset.
#[test]
fn palw_t12_genesis_claim_escrow_is_the_cards() {
    let t12 = palw_t12_shipped_params();
    let carve = t12.palw_overlay_carve.map(|carve| carve.worker_carve_permille);
    assert_eq!(carve, Some(PALW_OVERLAY_WORKER_CARVE_PERMILLE_V1));
    assert_eq!(palw_genesis_block_subsidy_sompi(&t12), PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI);
    assert_eq!(escrow_for_a_genesis_claim_v1(&(palw_genesis_block_subsidy_sompi(&t12), carve)), PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI);
    assert_eq!(PALW_T12_GENESIS_CLAIM_ESCROW_SOMPI, 320_084_650_080);
    assert_eq!(palw_rc_shipped_params().palw_overlay_carve.map(|carve| carve.worker_carve_permille), Some(720));
}

fn armed(carve: PalwOverlayCarveV1) -> Params {
    let mut params = palw_rc_shipped_params();
    params.palw_overlay_carve = Some(carve);
    params
}

fn refusal(params: &Params) -> String {
    match params.validate_palw_v2() {
        Err(why) => why.to_string(),
        Ok(()) => panic!("expected validate_palw_v2 to refuse {:?}", params.palw_overlay_carve),
    }
}

/// The four parts of one subsidy under `split`: (worker base, inclusion, validator, service).
fn parts(subsidy: u64, split: &FeeSplitParams) -> (u64, u64, u64, u64) {
    let s = split_block_subsidy(subsidy, split);
    (s.worker_base_sompi, s.worker_inclusion_sompi, s.validator_sompi, s.service_sompi)
}

/// **Below the height the split is the network's; from it, the validator pool is a fifth and the
/// worker base takes the tenth — on testnet-11's block, to the sompi, summing to the subsidy.**
/// The escrow the tenth funds is 72 % of the block, 3,200.85 MSK, up from the 2,756.28 MSK the
/// bundle's own 620 ‰ escrows; both are the whole worker base, as they are below.
#[test]
fn the_split_is_lowered_from_the_height_and_still_sums_to_the_subsidy() {
    let t11 = palw_rc_shipped_params();
    let dns = t11.dns_params.clone().expect("testnet-11 runs the overlay");
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t11.palw_consensus_mode else {
        panic!("testnet-11 is ConsensusV2")
    };
    let full = dns.reward_params.fee_split.clone();
    assert_eq!(
        (full.subsidy_worker_base_bps, full.subsidy_worker_inclusion_bps, full.subsidy_validator_bps, full.subsidy_service_bps),
        (6_200, 800, 3_000, 0),
        "the fixture is testnet-11's full split"
    );
    assert_eq!(dns.full_reward_split_daa_score, 0, "and it is in force from genesis");
    assert_eq!(bundle.state.worker_carve_permille(), 620, "and its bundle escrows 62 %");
    let carve = t11_carve();
    let at = |daa: u64| palw_overlay_fee_split_at_v1(&dns, Some(carve), daa).expect("Stage 3 from genesis");

    // Below the height: the network's split, byte for byte.
    for daa in [0, 1, T11_HEIGHT - 1] {
        assert_eq!(at(daa), full, "DAA {daa} is below the height");
    }
    assert_eq!(parts(T11_SUBSIDY, &full), (275_628_448_680, 35_564_961_120, 133_368_604_200, 0));

    // At and past it: validator 20 %, worker base 72 %, inclusion and service and both fee splits untouched.
    for daa in [T11_HEIGHT, T11_HEIGHT + 1, u64::MAX] {
        let lowered = at(daa);
        assert_eq!(lowered.subsidy_validator_bps, 2_000, "DAA {daa}: the validator share is the fence's");
        assert_eq!(lowered.subsidy_worker_base_bps, 7_200, "DAA {daa}: the worker base is the remainder");
        assert_eq!(
            FeeSplitParams {
                subsidy_worker_base_bps: full.subsidy_worker_base_bps,
                subsidy_validator_bps: full.subsidy_validator_bps,
                ..lowered.clone()
            },
            full,
            "DAA {daa}: nothing but the subsidy's validator share and its primary moves"
        );
        assert_eq!(parts(T11_SUBSIDY, &lowered), (320_084_650_080, 35_564_961_120, 88_912_402_800, 0));
    }

    // The escrow: 72 % of the block past the height, 62 % below — each the whole worker base.
    let past = palw_reward_carve_v2(T11_SUBSIDY, &carve.escrow_carve().expect("720 is a legal permille")).worker;
    let below = palw_reward_carve_v2(T11_SUBSIDY, &PalwRewardParamsV2::new(bundle.state.worker_carve_permille()).unwrap()).worker;
    assert_eq!((past, below), (320_084_650_080, 275_628_448_680), "3,200.85 MSK from the height, 2,756.28 MSK below it");
    assert_eq!(past - below, 44_456_201_400, "the tenth the validators gave up, escrowed");
    assert_eq!(past, parts(T11_SUBSIDY, &at(T11_HEIGHT)).0, "past the height the escrow is the whole worker base");
    assert_eq!(below, parts(T11_SUBSIDY, &full).0, "as it is below");

    // Every subsidy sums exactly under the lowered split, and the escrow always fits its base.
    for subsidy in [0, 1, 7, 9_999, 10_001, 370_468_345, T11_SUBSIDY, u64::MAX / 3, u64::MAX] {
        let (base, inclusion, validator, service) = parts(subsidy, &at(T11_HEIGHT));
        assert_eq!(base as u128 + inclusion as u128 + validator as u128 + service as u128, subsidy as u128, "subsidy {subsidy}");
        assert!(palw_reward_carve_v2(subsidy, &carve.escrow_carve().unwrap()).worker <= base, "subsidy {subsidy}: the carve fits");
    }

    // No fence: the network's split at every height.
    for daa in [0, T11_HEIGHT, u64::MAX] {
        assert_eq!(palw_overlay_fee_split_at_v1(&dns, None, daa), Some(full.clone()));
    }

    // Only the FULL split is lowered: below the overlay there is no split, and the bootstrap
    // split is never touched, even by a fence that is active there.
    let mut staged = dns.clone();
    staged.dns_activation_daa_score = 10;
    staged.full_reward_split_daa_score = 8_000;
    let from_genesis = PalwOverlayCarveV1 { activation: ForkActivation::always(), ..carve };
    assert_eq!(palw_overlay_fee_split_at_v1(&staged, Some(from_genesis), 9), None, "Stage 1: no split to lower");
    assert_eq!(
        palw_overlay_fee_split_at_v1(&staged, Some(from_genesis), 7_999),
        Some(staged.reward_params.fee_split_bootstrap.clone()),
        "Stage 2: the bootstrap split is never lowered"
    );
    assert_eq!(palw_overlay_fee_split_at_v1(&staged, Some(from_genesis), 8_000).unwrap().subsidy_validator_bps, 2_000, "Stage 3");
}

/// **Decision 4: refused at start** — a fence without an overlay or a V2 bundle, below the full
/// split, raising the validator share, past the whole subsidy, or wider than the worker base its
/// lowered split leaves. The exact fit is accepted, and `never()` is absence everywhere.
#[test]
fn an_overlay_carve_that_cannot_fit_refuses_to_start() {
    armed(t11_carve()).validate_palw_v2().expect("testnet-11's carve fits exactly: 720 ‰ into a 7,200 bps base");
    armed(PalwOverlayCarveV1 { worker_carve_permille: 620, ..t11_carve() })
        .validate_palw_v2()
        .expect("a carve below the base fits too");
    armed(PalwOverlayCarveV1 { subsidy_validator_bps: 3_000, worker_carve_permille: 620, ..t11_carve() })
        .validate_palw_v2()
        .expect("the network's own share is not a raise");

    // No overlay.
    let mut no_overlay = armed(t11_carve());
    no_overlay.dns_params = None;
    // testnet-11 schedules ADR-0128's gate beside the carve, and the gate refuses the same absence.
    no_overlay.dns_bft_gate = None;
    assert!(refusal(&no_overlay).contains("runs no validator overlay"), "{}", refusal(&no_overlay));
    assert_eq!(no_overlay.palw_overlay_carve_fence(), None, "and nothing would answer it");

    // No V2 bundle: mainnet's hash lineage runs the overlay and no PALW claims.
    let mut hash = MAINNET_PARAMS.clone();
    assert!(hash.dns_params.is_some() && !matches!(hash.palw_consensus_mode, crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(_)));
    hash.validate_palw_v2().expect("the unarmed preset starts");
    hash.palw_overlay_carve = Some(t11_carve());
    assert!(refusal(&hash).contains("not ConsensusV2"), "{}", refusal(&hash));
    assert_eq!(hash.palw_overlay_carve_fence(), None, "and nothing would answer it");

    // Below the full split.
    let mut staged = armed(PalwOverlayCarveV1 { activation: ForkActivation::new(4_999), ..t11_carve() });
    staged.dns_params.as_mut().unwrap().full_reward_split_daa_score = 5_000;
    assert!(refusal(&staged).contains("below the overlay's full reward split"), "{}", refusal(&staged));

    // A raise.
    let raised = armed(PalwOverlayCarveV1 { subsidy_validator_bps: 3_001, worker_carve_permille: 600, ..t11_carve() });
    assert!(refusal(&raised).contains("raises the validator share"), "{}", refusal(&raised));

    // Past the whole subsidy.
    let whole = armed(PalwOverlayCarveV1 { subsidy_validator_bps: 0, worker_carve_permille: 1_001, ..t11_carve() });
    assert!(refusal(&whole).contains("more than the whole subsidy"), "{}", refusal(&whole));

    // Wider than the base: one permille over, and a carve that fit the old split but not the new.
    let over = armed(PalwOverlayCarveV1 { worker_carve_permille: 721, ..t11_carve() });
    assert!(refusal(&over).contains("exceeds the worker base"), "{}", refusal(&over));
    let unlowered = armed(PalwOverlayCarveV1 { subsidy_validator_bps: 3_000, ..t11_carve() });
    assert!(refusal(&unlowered).contains("exceeds the worker base"), "{}", refusal(&unlowered));
    // The base is what the network's inclusion and service shares leave, not a constant.
    let mut serviced = armed(t11_carve());
    serviced.dns_params.as_mut().unwrap().reward_params.fee_split.subsidy_service_bps = 1;
    assert!(refusal(&serviced).contains("exceeds the worker base"), "{}", refusal(&serviced));

    // `never()` is absence: no refusal on any network.
    let never = PalwOverlayCarveV1 { activation: ForkActivation::never(), subsidy_validator_bps: 9_999, worker_carve_permille: 9_999 };
    let mut dormant_hash = MAINNET_PARAMS.clone();
    dormant_hash.palw_overlay_carve = Some(never);
    dormant_hash.validate_palw_v2().expect("a never() fence is no fence");
}

/// **The escrow never outgrows the base it is withheld from, in either order of the two scores.**
/// For every pair of attempt and paying scores around testnet-11's height the carve fits the
/// payer's worker base; the ordered pairs escrow what the fence says (72 % past it, 62 % for a
/// block from below paid past it); and the inverted pair — a merged block past the height paid
/// by a block below it — escrows the 62 % its payer's base holds, where the attempt score alone
/// would have escrowed 72 % against it.
#[test]
fn the_carve_is_resolved_at_the_lower_score_so_it_always_fits_the_payers_base() {
    let t11 = palw_rc_shipped_params();
    let dns = t11.dns_params.clone().expect("testnet-11 runs the overlay");
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t11.palw_consensus_mode else {
        panic!("testnet-11 is ConsensusV2")
    };
    let carve = t11_carve();
    let bundle_carve = PalwRewardParamsV2::new(bundle.state.worker_carve_permille()).unwrap();
    let escrow = |attempt: u64, paying: u64| {
        let resolved = palw_overlay_escrow_carve_at_v1(Some(carve), attempt, paying);
        palw_reward_carve_v2(T11_SUBSIDY, &resolved.unwrap_or(bundle_carve)).worker
    };
    let base = |paying: u64| parts(T11_SUBSIDY, &palw_overlay_fee_split_at_v1(&dns, Some(carve), paying).unwrap()).0;

    let scores = [0, T11_HEIGHT - 2, T11_HEIGHT - 1, T11_HEIGHT, T11_HEIGHT + 1, T11_HEIGHT + 2, u64::MAX];
    for attempt in scores {
        for paying in scores {
            assert!(
                escrow(attempt, paying) <= base(paying),
                "attempt at {attempt}, paid at {paying}: {} escrowed against a base of {}",
                escrow(attempt, paying),
                base(paying)
            );
        }
    }
    assert_eq!(escrow(T11_HEIGHT, T11_HEIGHT + 1), 320_084_650_080, "own work past the height: 72 %, the whole lowered base");
    assert_eq!(escrow(T11_HEIGHT - 1, T11_HEIGHT), 275_628_448_680, "a block from below paid past the height: 62 %, the tenth paid");
    assert_eq!(escrow(T11_HEIGHT, T11_HEIGHT - 1), 275_628_448_680, "a merged block past the height paid from below: 62 %");
    let attempt_score_alone = palw_reward_carve_v2(T11_SUBSIDY, &carve.escrow_carve().unwrap()).worker;
    assert!(
        attempt_score_alone > base(T11_HEIGHT - 1),
        "the inverted pair is the case the lower score exists for: 72 % does not fit a 62 % base"
    );
    assert_eq!(palw_overlay_escrow_carve_at_v1(None, u64::MAX, u64::MAX), None, "no fence, no carve");
}

/// **Admission's collateral gate does not read the fence, so the two are not armed together.**
#[test]
fn the_carve_is_refused_beside_an_armed_escrow_backing_gate() {
    let mut params = armed(t11_carve());
    let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else {
        panic!("testnet-11 is ConsensusV2")
    };
    bundle.state = bundle.state.clone().with_min_slash_permille_of_escrow(1).expect("1 permille is legal");
    assert!(refusal(&params).contains("min_slash_permille_of_escrow"), "{}", refusal(&params));
    params.palw_overlay_carve = None;
    assert!(
        !params.validate_palw_v2().err().is_some_and(|why| why.to_string().contains("palw_overlay_carve")),
        "without the fence this refusal says nothing"
    );
}

/// **Some-only, and a scheduled height is reported, never gated.** An unset fence writes nothing,
/// so every shipped preset keeps its pinned fingerprint (`shipped_presets_have_pinned_fingerprints`);
/// set, its height and both numbers move the params id and the schedule id and leave the identity
/// until the height is genesis, and the fork-id gate reads it by name.
#[test]
fn the_overlay_carve_is_hashed_some_only_and_scheduled_like_a_fence() {
    let shipped = palw_rc_shipped_params();
    assert_eq!(shipped.palw_overlay_carve, Some(t11_carve()), "testnet-11 schedules it on its 6,001 flag day");
    for other in [&MAINNET_PARAMS, &TESTNET_PARAMS, &SIMNET_PARAMS, &DEVNET_PARAMS] {
        assert_eq!(other.palw_overlay_carve, None, "{}: no other shipped preset arms it", other.net);
    }
    let mut none = shipped.clone();
    none.palw_overlay_carve = None;
    let base = armed(t11_carve());
    let later = armed(PalwOverlayCarveV1 { activation: ForkActivation::new(T11_HEIGHT + 1), ..t11_carve() });
    let share = armed(PalwOverlayCarveV1 { subsidy_validator_bps: 2_100, ..t11_carve() });
    let smaller = armed(PalwOverlayCarveV1 { worker_carve_permille: 700, ..t11_carve() });
    for (what, other) in
        [("the height", &later), ("the validator share", &share), ("the escrow carve", &smaller), ("the fence", &none)]
    {
        assert_ne!(base.consensus_params_id(), other.consensus_params_id(), "{what} reaches the fingerprint");
        assert_ne!(base.consensus_schedule_id(), other.consensus_schedule_id(), "{what} is reported in the schedule id");
        assert_eq!(
            base.consensus_identity_id(),
            other.consensus_identity_id(),
            "{what} of a height that has not fired leaves the identity"
        );
    }
    // `never()` is absence in the identity.
    let never = armed(PalwOverlayCarveV1 { activation: ForkActivation::never(), ..t11_carve() });
    assert_eq!(never.consensus_identity_id(), none.consensus_identity_id());
    // Armed at genesis it is a rule about block one, and its numbers with it.
    let genesis = armed(PalwOverlayCarveV1 { activation: ForkActivation::always(), ..t11_carve() });
    let genesis_smaller =
        armed(PalwOverlayCarveV1 { activation: ForkActivation::always(), worker_carve_permille: 700, ..t11_carve() });
    assert_ne!(genesis.consensus_identity_id(), none.consensus_identity_id(), "a carve in force at genesis separates networks");
    assert_ne!(genesis.consensus_identity_id(), genesis_smaller.consensus_identity_id(), "and so does its number");

    // The height reaches the schedule through the carve alone: the build before the flag day, with
    // and without it.
    let mut bare = none.clone();
    palw_rc_clear_flag_day_6001_for_tests(&mut bare);
    let mut carve_alone = bare.clone();
    carve_alone.palw_overlay_carve = Some(t11_carve());
    assert!(carve_alone.fence_schedule_v1().contains(&T11_HEIGHT), "the fork-id schedule names the height");
    assert!(!bare.fence_schedule_v1().contains(&T11_HEIGHT));
    assert!(
        base.palw_fences_v1()
            .iter()
            .any(|(name, fence)| *name == "palw_overlay_carve" && *fence == Some(ForkActivation::new(T11_HEIGHT))),
        "the named fence list the gate reads carries it"
    );
    assert!(none.palw_fences_v1().iter().any(|(name, fence)| *name == "palw_overlay_carve" && fence.is_none()));
    assert_eq!(base.palw_overlay_carve_fence(), Some(t11_carve()), "a ConsensusV2 network with an overlay answers it");
    assert_eq!(none.palw_overlay_carve_fence(), None);
}

/// **ADR-0134: the compute overlay retires at its own height on testnet-11, from genesis on a
/// card, and nowhere else** — and the fence is a bare `Option<ForkActivation>` the fork-id gate
/// names, so a build without it is refused past 6,201 rather than forked silently.
#[test]
fn adr0134_the_compute_overlay_retires_at_its_own_height() {
    let rc = palw_rc_shipped_params();
    assert_eq!(rc.palw_compute_overlay_retired, Some(ForkActivation::new(PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA)));
    assert!(!rc.palw_compute_overlay_retired_at(PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA - 1));
    assert!(rc.palw_compute_overlay_retired_at(PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA));
    assert_ne!(PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA, PALW_RC_PALW_UPGRADE_FENCE_DAA, "its own height, so the gate sees it");
    assert_ne!(
        PALW_RC_EXECUTION_SPAN_SHORT_FENCE_DAA, PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA,
        "the span-short fence is one before retirement, so each failure names its cause"
    );
    assert!(rc.palw_fences_v1().iter().any(|(name, fence)| *name == "palw_compute_overlay_retired" && fence.is_some()));
    for (name, preset) in [
        ("mainnet", MAINNET_PARAMS.clone()),
        ("testnet", TESTNET_PARAMS.clone()),
        ("simnet", SIMNET_PARAMS.clone()),
        ("devnet", devnet_shipped_params()),
    ] {
        assert!(preset.palw_compute_overlay_retired.is_none(), "{name}: no retirement scheduled");
    }
    // A card arms it from genesis (`mainnet_card_base_v1`); `a_carded_mainnet_arms_every_fence_testnet_11_arms`
    // is the pin, since the card fixtures live in that module.
    let mut never = rc.clone();
    never.palw_compute_overlay_retired = Some(ForkActivation::never());
    assert!(!never.palw_compute_overlay_retired_at(u64::MAX - 1), "never is never active");
}
