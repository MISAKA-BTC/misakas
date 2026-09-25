//! **ADR-0152 v3.1 M5 T41: the v22 golden vectors, empty and inhabited (every new map populated).**
//!
//! §6: the v22 layout's landing order (M1 → M2 → S → M3 → M4) is frozen at M5 by T41 — the R-core+
//! root block, the carriage's `0xB4` tail and every new record's Borsh shape — beside the discriminant
//! pins in `palw_state_v2`'s own tests (`delta_entry_discriminants_are_the_ones_on_disk`,
//! `consensus_object_discriminants_are_the_ones_the_chain_carries`, `the_v22_void_reasons_are_pinned`).
//! The style is `palw_state_v2`'s `the_version_22_state_root_golden_vectors` (whose empty root this
//! test re-reads): constants that move with any change to the preimage or the encoding, the visible
//! flag ADR-0043's change rule demands.
//!
//! **The goldens are re-pinned ONCE, on the shipping commit** (the regenesis cut), by the integrator,
//! from the values the failing assertion prints — and only together with a version bump or a written
//! reason. Until then a move is a question to answer, not a number to paste: the inhabited pin is a
//! real testnet-12 fold, so it also moves when a rule (not only a layout) changes what the fold
//! writes. The record pins are hand-built and move only for an encoding change. First pinned on
//! `rcore/m5-tests` over `rcore/int-3` at 7c2850b3.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_m5_v22_golden -- --nocapture

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaClaimV1, PalwDaSessionV1, PalwDaStageV1, PalwDaUnitV1};
use kaspa_consensus_core::palw_held_da_v1::PalwHeldMissingV1;
use kaspa_consensus_core::palw_offence_v1::{PalwConsumedOffenceV1, PalwOffenceKindV1};
use kaspa_consensus_core::palw_panel_var_v1::{PalwPanelLiabilityRecordV1, PalwSlashableLockV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwPayoutV2, PalwPendingRewardV1, PalwReporterCommitV1, PalwReporterCountersV1, PalwRewardWinnerV1, PalwVoidReasonV2,
};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use kaspa_consensus_core::palw_vesting_v1::{PalwVestingCountersV1, PalwVestingRowV1};
use kaspa_consensus_core::tx::TransactionOutpoint;

/// 1 MSK in sompi.
const MSK: u64 = 100_000_000;

/// BLAKE2b-256 of `bytes`, hex.
fn digest(bytes: &[u8]) -> String {
    blake2b_simd::Params::new().hash_length(32).hash(bytes).to_hex().to_string()
}

/// The carriage's Borsh bytes, hashed: what a node persists and serves for `s`.
fn carriage_digest(s: &PalwChainStateV2) -> String {
    digest(&borsh::to_vec(&PalwStateCarriageV2::from_state(s)).expect("the carriage encodes"))
}

/// **The inhabited state: testnet-12's own fold, driven until every v22 item is populated at once.**
/// Two batches of thirty-one floor claims (the second holding one claim on three `Valid`s and two
/// `Unavailable`s, and two on the coverage door), the first batch's rows latched and three of them
/// moved; one Final claim of the second batch defaulted at `FinalRow` (S3: its row burned); an
/// equivocation whose reward nobody revealed (forgone); ten claims seat-accused together, defaulted
/// together (S1 strikes, `DaDefault` records with `collected` and `claim_id`) and swept together in
/// the tip block, which leaves two awards waiting in `reporter_rewards`; and, open at the tip, a DA
/// session on a Final claim, a reporter's commitment, and a revealed equivocation reward pending.
fn inhabited() -> (Tape, Hash64) {
    let mut c = Chain::new(t12());
    c.attribution = true;
    let mut t = Tape::new(c);
    let reporter = bond_key(71);
    t.step(vec![bond_obj(71, 400_000 * MSK)]);
    let seats = t.c.floor_seats();
    let (floor, _, _, _) = genesis_classes(&t.c.p)[0];
    let v1 = |id: Hash64, valid_seats: &[usize], unavailable_seats: &[usize], signed: u64| {
        let mut receipts: Vec<_> = valid_seats.iter().map(|i| valid(id, seats[*i].0, signed)).collect();
        receipts.extend(unavailable_seats.iter().map(|i| unavailable(id, seats[*i].0, signed)));
        PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }
    };
    let mut batch_b = Vec::new();
    let mut finals = Vec::new();
    for b in 0..2u64 {
        for k in 0..31u64 {
            let id = t.attempt(None, 0x4100 + (b << 6) + k);
            let bound = t.bind(id);
            let object = match (b, k) {
                (1, 0) => v1(id, &[0, 1, 2], &[3, 4], bound),
                (1, 1) | (1, 2) => {
                    let anchor = t.c.anchor(&id);
                    PalwConsensusObjectV2::ReceiptLicensedV2 {
                        claim: id,
                        receipts: covered(id, anchor, &seats, &[0, 1, 2, 3, 4], bound),
                    }
                }
                _ => v1(id, &[0, 1, 2, 3, 4], &[], bound),
            };
            t.step(vec![object]);
            if b == 1 {
                batch_b.push(id);
            }
        }
        t.at(t.c.daa + t.c.sp.window_challenge() + 1, vec![]);
        finals.push(t.c.daa);
    }
    // S3: a Final claim of the second batch defaulted at FinalRow (its row burned).
    t.step(vec![da_accuse(batch_b[5], reporter, 0)]);
    // An equivocation nobody reveals for: forgone at its sweep.
    let (forgone_eq, _) = equivocation_of(seats[3].0, floor, 0x4F01);
    t.step(vec![forgone_eq]);
    // Ten claims seat-accused together.
    let wave: Vec<Hash64> = (0..10u64)
        .map(|k| {
            let id = t.attempt(None, 0x4200 + k);
            t.bind(id);
            id
        })
        .collect();
    t.step(wave.iter().map(|id| da_accuse(*id, seats[1].0, 0)).collect());
    let wave_default = t.c.s.da_session(&wave[0], &seats[1].0).unwrap().deadline_daa + 1;
    let s3_default = t.c.s.da_session(&batch_b[5], &reporter).unwrap().deadline_daa + 1;
    for at in [s3_default.min(wave_default), s3_default.max(wave_default)] {
        if at > t.c.daa {
            t.at(at, vec![]);
        }
    }
    let (producer, _, _) = floor_producer(&t.c.p);
    let wave_key = kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1(&producer.0, &wave[0]);
    let sweep = t.c.s.reward_pending(&wave_key).expect("the wave's rewards").reveal_until + 1;
    // The first batch's rows latch past their DAA clock (thirty-one anchors settled since their Final).
    let latch = finals[0] + t.c.sp.window_court() + 1;
    assert!(latch < sweep - 3, "the premise: the latch lands before the tip");
    t.at(latch, vec![]);
    // Open at the tip: a session on a Final claim, a commitment, a revealed equivocation reward.
    t.step(vec![da_accuse(batch_b[9], reporter, 0)]);
    let (eq, evidence) = equivocation_of(seats[4].0, floor, 0x4F02);
    let key = equivocation_key(seats[4].0, evidence);
    t.step(vec![reporter_commit(key, evidence, reporter), reporter_commit(h(0x4F03), h(0x4F04), reporter)]);
    t.step(vec![eq]);
    t.step(vec![reporter_reveal(key, reporter)]);
    t.at(sweep, vec![]);
    (t, key)
}

/// Every v22 item of `s` populated, and every new field of its records carrying a non-default value
/// somewhere — the premise of the inhabited pin.
fn assert_every_v22_item_is_populated(s: &PalwChainStateV2, open_reward: &Hash64) {
    let c = PalwStateCarriageV2::from_state(s);
    // Claim records: every rcore field, and M2's job identity.
    assert!(c.claims.values().any(|claim| claim.rcore.licence_door == Some(PalwLicenceDoorTagV1::Quorum)), "a Quorum door");
    assert!(c.claims.values().any(|claim| claim.rcore.licence_door == Some(PalwLicenceDoorTagV1::Coverage)), "a Coverage door");
    assert!(
        c.claims.values().any(|claim| claim.rcore.basis_k == 3) && c.claims.values().any(|claim| claim.rcore.basis_k == 2),
        "basis 3 and 2"
    );
    assert!(c.claims.values().any(|claim| claim.rcore.escrow_released), "a released escrow");
    assert!(c.claims.values().any(|claim| claim.rcore.served_mask != 0), "a served mask");
    assert!(c.claims.values().any(|claim| claim.rcore.unserved_seen), "unserved_seen latched");
    assert!(c.claims.values().any(|claim| claim.rcore.g_res_sompi > 0), "a frozen G_res");
    assert!(c.claims.values().any(|claim| claim.job_identity != Hash64::default()), "a job identity (M2)");
    // Locks with masks; liability rows with S's fields; consumed offences with R-2's fields.
    assert!(c.slashable_locks.values().any(|lock| lock.segments > 0 && lock.attested.is_full(lock.segments)), "a full-mask lock");
    assert!(
        c.slashable_locks
            .values()
            .any(|lock| lock.segments > 0 && !lock.attested.is_full(lock.segments) && lock.attested != PalwSegmentMaskV2::NONE),
        "a partial-mask lock"
    );
    assert!(
        c.panel_liabilities
            .values()
            .any(|row| row.licence_door.is_some() && row.basis_k >= 2 && row.g_res_sompi > 0 && row.escrowed_reward > 0),
        "a liability row with its door, basis, G_res and escrow"
    );
    assert!(c.panel_liabilities.values().any(|row| row.job_identity != Hash64::default()), "a liability row with its job identity");
    assert!(
        c.consumed_offences.values().any(|record| record.collected > 0 && record.claim_id != Hash64::default()),
        "collected and claim_id"
    );
    assert!(c.consumed_offences.values().any(|record| record.kind == PalwOffenceKindV1::DaDefault), "a DaDefault record");
    assert!(c.consumed_offences.values().any(|record| record.kind == PalwOffenceKindV1::ExecutorEquivocation), "an Eq record");
    // The R-core+ tail, S's five.
    assert!(!c.withholding_strikes.is_empty(), "withholding_strikes");
    assert!(
        c.reward_pending.get(open_reward).is_some_and(|p| p.best.is_some() && p.evidence_id != Hash64::default()),
        "reward_pending, revealed"
    );
    assert!(!c.reporter_commitments.is_empty(), "reporter_commitments");
    assert!(!c.reporter_rewards.is_empty(), "reporter_rewards (a backlog at the tip)");
    assert!(c.reporter_counters.awarded_sompi > 0 && c.reporter_counters.forgone_sompi > 0, "both reporter counters");
    // The vesting items.
    assert!(c.vesting.values().any(|row| row.matured_at.is_some()), "a latched row");
    assert!(c.vesting.values().any(|row| row.matured_at.is_none()), "an unlatched row");
    assert!(
        c.vesting.values().any(|row| row.job_identity != Hash64::default() && row.basis_k >= 2 && !row.seats.is_empty()),
        "a row's copies"
    );
    assert!(
        c.vesting_counters.created > 0 && c.vesting_counters.moved > 0 && c.vesting_counters.burned > 0,
        "all three vesting counters"
    );
    // M3's two.
    assert!(c.da_sessions.values().any(|session| session.stage == PalwDaStageV1::FinalRow), "an open FinalRow session");
    assert!(c.da_claims.values().any(|record| record.open_sessions() > 0), "a DA record with an open session");
    assert!(
        c.da_claims.values().any(|record| record.last_closed_daa.is_some() && !record.opened_by_seat.is_empty()),
        "a closed seat session"
    );
}

/// **T41: the v22 golden vectors — the empty state and the inhabited one.**
///
/// * empty: `PalwChainStateV2::genesis()` — the R-core+ block is Some-only, so the empty root is the
///   one `palw_state_v2`'s own golden pins (cross-checked here) — and its carriage bytes;
/// * inhabited: [`inhabited`]'s tip, with [`assert_every_v22_item_is_populated`] as its premise: the
///   state root (every root section in landing order) and the carriage bytes (the `0xB4` tail in
///   landing order, every record's shape).
#[test]
fn t41_the_v22_golden_vectors_empty_and_inhabited() {
    let empty = PalwChainStateV2::genesis();
    let (tape, open_reward) = inhabited();
    let full = &tape.c.s;
    assert_every_v22_item_is_populated(full, &open_reward);
    // The inhabited carriage loads under its root (the premise that it is a state, not bytes).
    tape.restart_at(tape.len());
    let got = [
        ("empty root", empty.state_root().to_string()),
        ("empty carriage", carriage_digest(&empty)),
        ("inhabited root", full.state_root().to_string()),
        ("inhabited carriage", carriage_digest(full)),
    ];
    // re-pin 2026-09-25 @57c1fe323c44: t12 shipping re-pin 2026-09-25 (rcore/int-3 57c1fe32): mainnet values (λ 5, 1,620 s tolerance, level 225; mainnet takes t12's DNS set, carve, Decision A), the beat lead cap (132 s past the receiver clock), the execution-quantum maturity 120 DAA, the readiness-memory / duties / mempool node fixes; T41 v22 golden moves because R1 (palw_activation_pool) excludes genesis rows from silence reclamation. Genesis a27f8f44 and premine txid 5e0d5f1b unchanged. (was cd18ed25…, f5b6e2c2…)
    let want = [
        // `palw_state_v2`'s own v22 empty root (`the_version_22_state_root_golden_vectors`), re-read.
        "63e5e4480416252619a5ee106aeeb32fb884bba58b5660c990396970e6f2f5fa26d8780b2901567db8c539291aaca64eb4c0a8dab75d0436238d8fbe65553529",
        "d1daaf1d50b5b7b368ceeb8096316c6109d7aea07016a146c5525db43560a2a4",
        "6afd3a7c573f5113daae9ae420eb8044f56262aa5f9269b51f9639e890bf2549b013be87812112b899b703db1a6631cc8eac562c21487fe486c89426e794f678",
        "7590901d4673f96b9111c0a1570e0b8640af9662ce93c59b59b2b3ba8dac6e0a",
    ];
    let mut moved = Vec::new();
    for ((what, value), want) in got.iter().zip(want) {
        println!("T41 {what}: {value}");
        if value != want {
            moved.push(*what);
        }
    }
    assert!(moved.is_empty(), "v22 goldens moved: {moved:?} (re-pinned only on the shipping commit)");
}

/// **T41, the record half: every new v22 record's Borsh shape, hand-built** (fold-independent: moves
/// only for an encoding change). Each record carries a distinct non-default value in every field the
/// v22 layout added — the claim's `rcore` (incl. `g_res_sompi`), the vesting row (incl. the latch) and
/// counters, the pending reward with its winner, the commitment row, the reporter counters, the DA
/// session (every stage) and claim record (every field; their unit lists carry an event AND every
/// held unit — `PalwDaUnitV1::Held` with each `PalwHeldMissingV1` variant, the 8k A-held gate's
/// encoding, which no fold on the floor writes), the lock's mask and cut, the liability row's
/// door, basis, `G_res` and escrow, and the consumed offence's `collected` and `claim_id` — and the
/// digest of its bytes is pinned.
#[test]
fn t41_the_v22_record_encodings_are_pinned() {
    let bond = |n: u64| bond_key(n);
    let rcore = PalwClaimRcoreV1 {
        licence_door: Some(PalwLicenceDoorTagV1::Coverage),
        basis_k: 2,
        escrow_released: true,
        served_mask: 0b1_0110,
        unserved_seen: true,
        g_res_sompi: 0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10,
    };
    let row = PalwVestingRowV1 {
        claim_id: h(0x5601),
        producer_bond: bond(1),
        class_id: h(0x5602),
        execution_root: h(0x5603),
        artifact_root: h(0x5604),
        job_identity: h(0x5605),
        free_prompt: true,
        trace_root: h(0x5606),
        segment_count: 4,
        licence_door: PalwLicenceDoorTagV1::Quorum,
        basis_k: 3,
        escrowed_reward: 320_000,
        buyback_bound: 16_000,
        producer: PalwPayoutV2 { payload: h(0x5607), amount: 250_000 },
        seats: vec![
            (bond(2), PalwPayoutV2 { payload: h(0x5608), amount: 10_000 }),
            (bond(3), PalwPayoutV2 { payload: h(0x5609), amount: 11_000 }),
        ],
        reserve: 33,
        final_daa: 1_234,
        expiry_daa: 4_234,
        settled_at_final: 77,
        matured_at: Some(10_234),
    };
    let counters = PalwVestingCountersV1 { created: 3, moved: 2, burned: 1 };
    let pending = PalwPendingRewardV1 {
        amount: 96_028,
        reveal_until: 1_810,
        evidence_id: h(0x5701),
        best: Some(PalwRewardWinnerV1 { committed_daa: 1_200, commitment: h(0x5702), reporter: bond(4), payload: h(0x5703) }),
    };
    let commit = PalwReporterCommitV1 { reporter: bond(5), committed_daa: 1_201 };
    let reporter_counters = PalwReporterCountersV1 { awarded_sompi: 100_000, forgone_sompi: 96_028 };
    let every_unit = vec![
        PalwDaUnitV1::Event { row: 7, tile: 2 },
        PalwDaUnitV1::Held(PalwHeldMissingV1::PromptIdsTile { tile: 3 }),
        PalwDaUnitV1::Held(PalwHeldMissingV1::StateChunk { checkpoint: 2, chunk: 9 }),
        PalwDaUnitV1::Held(PalwHeldMissingV1::StepRange { first: 1_024, count: 16 }),
        PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { leaf: 4_097 }),
    ];
    let sessions: Vec<PalwDaSessionV1> = [PalwDaStageV1::Live, PalwDaStageV1::Licensed, PalwDaStageV1::FinalRow]
        .into_iter()
        .map(|stage| PalwDaSessionV1 {
            opened_daa: 2_000,
            deadline_daa: 3_200,
            accuser_is_seat: stage != PalwDaStageV1::FinalRow,
            exposure: 32_009_640_274,
            // Every unit kind: an event, and each held unit (the 8k A-held gate's encoding,
            // discriminant 1 and its four inner variants).
            units: every_unit.clone(),
            stage,
        })
        .collect();
    let record = PalwDaClaimV1 {
        open_seat_sessions: 2,
        open_other_sessions: 1,
        opened_non_seat_total: 3,
        opened_by_seat: [(bond(6), 1), (bond(7), 2)].into_iter().collect(),
        paused_since: Some(2_000),
        last_closed_daa: Some(1_999),
        answered: every_unit.iter().copied().chain([PalwDaUnitV1::Event { row: 0, tile: 0 }]).collect(),
        flat_answered: true,
        refuted_held: vec![(bond(8), 320)],
    };
    let lock = PalwSlashableLockV1 {
        claim: h(0x5801),
        amount: 16_011_000_000,
        expiry_daa: 4_234,
        settled_at_final: 77,
        attested: PalwSegmentMaskV2(0b0100),
        segments: 4,
    };
    let liability = PalwPanelLiabilityRecordV1 {
        claim_id: h(0x5901),
        work_id: h(0x5902),
        class_id: h(0x5903),
        execution_root: h(0x5904),
        output_root: h(0x5905),
        executor_bond: bond(1),
        voided_daa: Some(3_300),
        void_reason: Some(PalwVoidReasonV2::ProducerWithholding),
        valid_signers: vec![(bond(2).0, h(0x5906))],
        locked_sompi: 16_011_000_000,
        expiry_daa: 4_234,
        settled_at_final: 77,
        job_identity: h(0x5907),
        free_prompt: false,
        trace_root: h(0x5908),
        segment_count: 4,
        licence_door: Some(PalwLicenceDoorTagV1::Coverage),
        basis_k: 2,
        g_res_sompi: 11_752_660,
        escrowed_reward: 320_084_650_080,
    };
    let offence = PalwConsumedOffenceV1 {
        kind: PalwOffenceKindV1::DaDefault,
        accused: TransactionOutpoint { transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(0x5A01), index: 3 },
        amount: 320_096_402_740,
        accepted_daa: 2_221,
        execution_root: Hash64::default(),
        collected: 320_096_402_740,
        claim_id: h(0x5A02),
    };
    // The unit's tags byte for byte: `Event` is 0 and `Held` 1, and `Held`'s four inner variants
    // 0..=3 in declaration order (appended-last `StepLeaf` is 3).
    let tags: Vec<Vec<u8>> = every_unit.iter().map(|u| borsh::to_vec(u).unwrap()).collect();
    assert_eq!(tags[0], [vec![0u8], 7u32.to_le_bytes().to_vec(), vec![2u8]].concat(), "Event {{ row, tile }}");
    assert_eq!(tags[1], [vec![1u8, 0], 3u32.to_le_bytes().to_vec()].concat(), "Held(PromptIdsTile)");
    assert_eq!(tags[2], [vec![1u8, 1], 2u32.to_le_bytes().to_vec(), 9u32.to_le_bytes().to_vec()].concat(), "Held(StateChunk)");
    assert_eq!(tags[3], [vec![1u8, 2], 1_024u64.to_le_bytes().to_vec(), 16u32.to_le_bytes().to_vec()].concat(), "Held(StepRange)");
    assert_eq!(tags[4], [vec![1u8, 3], 4_097u64.to_le_bytes().to_vec()].concat(), "Held(StepLeaf)");
    // Each record decodes back to itself (a shape the encoder and decoder agree on).
    fn round<T: borsh::BorshSerialize + borsh::BorshDeserialize + PartialEq + std::fmt::Debug>(value: &T) {
        assert_eq!(&borsh::from_slice::<T>(&borsh::to_vec(value).unwrap()).unwrap(), value);
    }
    round(&rcore);
    round(&row);
    round(&counters);
    round(&pending);
    round(&commit);
    round(&reporter_counters);
    round(&sessions);
    round(&record);
    round(&lock);
    round(&liability);
    round(&offence);
    let got: Vec<(&str, String)> = vec![
        ("PalwClaimRcoreV1", digest(&borsh::to_vec(&rcore).unwrap())),
        ("PalwVestingRowV1", digest(&borsh::to_vec(&row).unwrap())),
        ("PalwVestingCountersV1", digest(&borsh::to_vec(&counters).unwrap())),
        ("PalwPendingRewardV1", digest(&borsh::to_vec(&pending).unwrap())),
        ("PalwReporterCommitV1", digest(&borsh::to_vec(&commit).unwrap())),
        ("PalwReporterCountersV1", digest(&borsh::to_vec(&reporter_counters).unwrap())),
        ("PalwDaSessionV1 (Live, Licensed, FinalRow; every unit kind)", digest(&borsh::to_vec(&sessions).unwrap())),
        ("PalwDaClaimV1", digest(&borsh::to_vec(&record).unwrap())),
        ("PalwSlashableLockV1", digest(&borsh::to_vec(&lock).unwrap())),
        ("PalwPanelLiabilityRecordV1", digest(&borsh::to_vec(&liability).unwrap())),
        ("PalwConsumedOffenceV1", digest(&borsh::to_vec(&offence).unwrap())),
    ];
    let want = [
        "4b0f4d363b1671ab2fe2af51f437343a6ceed3d753ced3116ec2b93fefd9e9bf", // PalwClaimRcoreV1
        "ba1362b92e60de699a2b1bcfd26b322ce07827b27abbad52c28d4c6fe99f1964", // PalwVestingRowV1
        "1158b81247235faed7b1f7f2497c6b9259f44b67beb05b7892e37bf354e54bdc", // PalwVestingCountersV1
        "c7723e6229ed367fc0afd2f72e5712eda7d941b32a33a8b9ba82db840eb7478d", // PalwPendingRewardV1
        "cc0c43765c2bebb37288716a8851254e34abe6a534eb20d85b60ddd5a14570b2", // PalwReporterCommitV1
        "211813b42cca0fc007a86c07d2b767c61e2e0fa50b4470fee07323dfb3eac7da", // PalwReporterCountersV1
        "65c5638eaa8dec4f482a6dae13c7b490eb9ec0fec8d13b8cd004b9c23f77914f", // PalwDaSessionV1
        "da62e85b79b466150f9ac97f53e53aafe70d3ff5b788cf005a6fd37781c27ba8", // PalwDaClaimV1
        "74f585bc9a1d8887fc2e92feba1387db5ae96a3a35f645beee02025a150be9b6", // PalwSlashableLockV1
        "3d319ff29614dc063db42a7e9aa7a8ff7767cee6519a310d654e9f09bfbd86a0", // PalwPanelLiabilityRecordV1
        "e8e4d8837819bb9984f1561da937f8b18ba64045c1fd552bd91cb2dee4d576f5", // PalwConsumedOffenceV1
    ];
    // Every record is printed before anything is compared (`scripts/t12-repin.sh` reads these lines),
    // and a record without its pinned digest — or a digest without its record — fails by count, not
    // by a `zip` that stops at the shorter side.
    for (what, value) in &got {
        println!("T41 record {what}: {value}");
    }
    assert_eq!(got.len(), want.len(), "one pinned digest per record, in order");
    let moved: Vec<&str> = got.iter().zip(want).filter(|((_, value), want)| value != want).map(|((what, _), _)| *what).collect();
    assert!(moved.is_empty(), "v22 record encodings moved: {moved:?} (re-pinned only on the shipping commit)");
}
