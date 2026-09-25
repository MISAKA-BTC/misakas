//! **A possession proof whose row is about to lapse outranks the court queue** (the 2026-09-25
//! model-registry review, M1) — the one predicate the seat's carrier scheduler
//! (`kaspad::palw_panel`), the pool's carrier index and template lane (`kaspa-mining`) and the
//! virtual processor's tip read all ask, so what a seat hurries is exactly what a miner keeps.
//!
//! **The hole.** A class stays out of HELD only while at least `seat_count` of its seats hold a
//! fresh possession row, and leaves PREFETCHING/HELD only at `seat_count + spare_seats` (5 and 7 on
//! testnet-12). A V2 row stands [`palw_readiness_max_age_daa_v1`] (eight spans — 8 DAA on
//! testnet-12's one-DAA spans) and the seat re-proves at half of it (`palw_readiness_duty_due_v2`).
//! Each proof is one lifecycle carrier of tens of KB. Past R-core+ those proofs rode the panel's
//! Ordinary lane behind the court queue, and P2-9 lets conviction, DA and reporter carriers lead
//! every template and hold a mempool reserve — so a storm of cheap accusations could keep an honest
//! seat's rows past staleness and push its class into HELD, at no cost to the accusers.
//!
//! **The margin** ([`PALW_READINESS_ESCALATION_LANDING_DAA_V1`]): a carrier sent now is included by
//! the next block and accepted by the chain block that merges it — two blocks, two DAA at one block
//! a DAA. A row escalates from `max_age − landing` DAA of age (6 on testnet-12): a proof escalated
//! then is accepted by the last DAA the old row counts, so the seat is never out. Below that age
//! nothing changes: the seat's half-age duty (age 5) sends the proof on the ordinary lane exactly as
//! before, and only a proof that has not landed by age 6 is hurried.
//!
//! **What escalates is a proof, not a key** ([`palw_readiness_proof_escalates_v1`]): the row at the
//! tip is about to lapse AND the proof renews it (the row it writes — dated at the NAMED span's
//! first DAA — would not itself escalate). A proof naming an old span writes an old row; privileging
//! it would let one bond renew a near-stale row with near-stale rows and take the head of every
//! template, so it gets no privilege.
//!
//! **Policy, never a rule.** Nothing in block validation or the fold reads this module; no id, root
//! or fingerprint moves. It decides only which carrier a seat sends first, and which proof a node's
//! pool keeps in a full pool and puts at the head of its template — past R-core+ only (testnet-12).

use crate::palw_heartbeat_carriers_v1::{PalwH1LaneKeyV1, palw_h1_carrier_object_v1};
use crate::palw_lifecycle_objects_v2::palw_lifecycle_objects_from_accepted_txs_v2;
use crate::palw_model_registry_v1::{PalwRegistryGlobalsV1, PalwSeatReadinessRowV1, palw_readiness_max_age_daa_v1};
use crate::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use crate::tx::Transaction;
use kaspa_hashes::Hash64;

/// **How many DAA an escalated proof takes to land**: one block to include its carrier, one for the
/// chain block that merges it to accept it, at one block a DAA (testnet-12). A row escalates once it
/// has no more than this left before the last DAA it counts.
pub const PALW_READINESS_ESCALATION_LANDING_DAA_V1: u64 = 2;

/// **The DAA from which `row` escalates** — `proved_daa + max_age − landing`, or `0` (now) for a
/// row that counts for nothing: none at all, or a V1 row past readiness V2.
pub fn palw_readiness_escalates_from_daa_v1(
    row: Option<&PalwSeatReadinessRowV1>,
    span_daa: u64,
    g: &PalwRegistryGlobalsV1,
    readiness_v2: bool,
) -> u64 {
    match row {
        None => 0,
        Some(row) if readiness_v2 && row.proof_version < 2 => 0,
        Some(row) => row
            .proved_daa
            .saturating_add(palw_readiness_max_age_daa_v1(span_daa, g, readiness_v2))
            .saturating_sub(PALW_READINESS_ESCALATION_LANDING_DAA_V1),
    }
}

/// **Is `row` about to lapse at `now_daa`?** — within [`PALW_READINESS_ESCALATION_LANDING_DAA_V1`]
/// of its last fresh DAA, already stale, or absent.
pub fn palw_readiness_row_escalates_v1(
    row: Option<&PalwSeatReadinessRowV1>,
    now_daa: u64,
    span_daa: u64,
    g: &PalwRegistryGlobalsV1,
    readiness_v2: bool,
) -> bool {
    now_daa >= palw_readiness_escalates_from_daa_v1(row, span_daa, g, readiness_v2)
}

/// **Does a proof for `proof_span` (of `proof_version`) escalate at `now_daa`, against the tip's
/// `row`?** The row is about to lapse ([`palw_readiness_row_escalates_v1`]) and the row the proof
/// writes — dated at the named span's first DAA, as the fold dates it — would not.
#[allow(clippy::too_many_arguments)]
pub fn palw_readiness_proof_escalates_v1(
    row: Option<&PalwSeatReadinessRowV1>,
    proof_span: u64,
    proof_version: u8,
    now_daa: u64,
    span_daa: u64,
    g: &PalwRegistryGlobalsV1,
    readiness_v2: bool,
) -> bool {
    let renewed = PalwSeatReadinessRowV1 {
        proved_daa: proof_span.saturating_mul(span_daa.max(1)),
        proved_span: proof_span,
        leaf_index: 0,
        proof_version,
        chunks: 0,
    };
    palw_readiness_row_escalates_v1(row, now_daa, span_daa, g, readiness_v2)
        && !palw_readiness_row_escalates_v1(Some(&renewed), now_daa, span_daa, g, readiness_v2)
}

/// **A possession proof, as the pool and the tip read name it**: the row it writes and the span it
/// names. Its lane key ([`Self::lane_key`]) is `(bond, class)` — one reserved place and one lane
/// slot per row, however many copies of a proof a bond files.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PalwReadinessCarrierV1 {
    pub bond: PalwBondKeyV2,
    pub class_id: Hash64,
    pub span: u64,
    /// `1` for a one-leaf `SeatReadinessProved`, `2` for a `SeatReadinessProvedV2` multiproof.
    pub proof_version: u8,
}

impl PalwReadinessCarrierV1 {
    /// The proof `object` is, if it is one.
    pub fn of_object(object: &PalwConsensusObjectV2) -> Option<Self> {
        match object {
            PalwConsensusObjectV2::SeatReadinessProved { bond, class_id, span, .. } => {
                Some(Self { bond: *bond, class_id: *class_id, span: *span, proof_version: 1 })
            }
            PalwConsensusObjectV2::SeatReadinessProvedV2 { bond, class_id, span, .. } => {
                Some(Self { bond: *bond, class_id: *class_id, span: *span, proof_version: 2 })
            }
            _ => None,
        }
    }

    /// The proof `tx` carries — the extraction walk the fold reads, never a tag peek; any other
    /// subnetwork is answered without decoding.
    pub fn of_tx(tx: &Transaction) -> Option<Self> {
        if tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE {
            return None;
        }
        palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(tx))
            .objects
            .first()
            .and_then(|carried| Self::of_object(&carried.object))
    }

    /// One place per `(bond, class)` row.
    pub fn lane_key(&self) -> PalwH1LaneKeyV1 {
        PalwH1LaneKeyV1::Readiness(self.bond, self.class_id)
    }

    /// [`palw_readiness_proof_escalates_v1`] for this proof against the tip's `row`.
    pub fn escalates(
        &self,
        row: Option<&PalwSeatReadinessRowV1>,
        now_daa: u64,
        span_daa: u64,
        g: &PalwRegistryGlobalsV1,
        readiness_v2: bool,
    ) -> bool {
        palw_readiness_proof_escalates_v1(row, self.span, self.proof_version, now_daa, span_daa, g, readiness_v2)
    }
}

/// **What the node's carrier gate puts to the fold** (the virtual processor's
/// `palw_mempool_h1_carrier_refusal`): an H-1 carrier, and — because a proof can buy a reserved place
/// and the head of a template once its row nears staleness, and whether it does moves with the DAA
/// after admission — every possession proof. One decode; any other subnetwork is answered without
/// decoding.
pub fn palw_gated_carrier_object_of_tx_v1(tx: &Transaction) -> Option<PalwConsensusObjectV2> {
    if tx.subnetwork_id != SUBNETWORK_ID_PALW_LIFECYCLE {
        return None;
    }
    palw_lifecycle_objects_from_accepted_txs_v2(std::slice::from_ref(tx))
        .objects
        .into_iter()
        .next()
        .map(|carried| carried.object)
        .filter(|object| palw_h1_carrier_object_v1(object) || PalwReadinessCarrierV1::of_object(object).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use crate::palw_model_registry_v1::{PALW_READINESS_V2_MAX_AGE_SPANS_V1, PALW_REGISTRY_GLOBALS_V1};
    use crate::tx::{ScriptPublicKey, TransactionId, TransactionOutpoint, TransactionOutput};

    const G: PalwRegistryGlobalsV1 = PALW_REGISTRY_GLOBALS_V1;

    fn row(proved_daa: u64, proof_version: u8) -> PalwSeatReadinessRowV1 {
        PalwSeatReadinessRowV1 { proved_daa, proved_span: proved_daa, leaf_index: 0, proof_version, chunks: 16 }
    }

    /// **The margin on testnet-12's one-DAA spans: a row escalates at age 6 of 8** — the seat's own
    /// half-age duty (age 5) is not hurried, a proof escalated at 6 lands at 8, the last DAA the row
    /// counts, and a stale, absent or V1 row escalates at once.
    #[test]
    fn a_row_escalates_two_landing_daa_before_its_last_fresh_daa() {
        let max_age = palw_readiness_max_age_daa_v1(1, &G, true);
        assert_eq!(max_age, PALW_READINESS_V2_MAX_AGE_SPANS_V1 as u64, "eight spans of one DAA");
        let r = row(100, 2);
        for age in 0..=5 {
            assert!(!palw_readiness_row_escalates_v1(Some(&r), 100 + age, 1, &G, true), "age {age}: the ordinary lane");
        }
        for age in 6..=20 {
            assert!(palw_readiness_row_escalates_v1(Some(&r), 100 + age, 1, &G, true), "age {age}: escalated");
        }
        assert_eq!(max_age - PALW_READINESS_ESCALATION_LANDING_DAA_V1, 6);
        assert!(palw_readiness_row_escalates_v1(None, 0, 1, &G, true), "no row: nothing counts yet");
        assert!(palw_readiness_row_escalates_v1(Some(&row(100, 1)), 100, 1, &G, true), "a V1 row counts for nothing past V2");
        // Below readiness V2 the thirty-span age, same margin.
        assert!(!palw_readiness_row_escalates_v1(Some(&row(100, 1)), 127, 1, &G, false));
        assert!(palw_readiness_row_escalates_v1(Some(&row(100, 1)), 128, 1, &G, false));
        // Five-DAA spans: eight spans are forty DAA.
        assert!(!palw_readiness_row_escalates_v1(Some(&row(100, 2)), 137, 5, &G, true));
        assert!(palw_readiness_row_escalates_v1(Some(&row(100, 2)), 138, 5, &G, true));
    }

    /// **A proof escalates only if it renews the row**: this span's proof on a near-stale row does;
    /// the same proof on a fresh row does not; a proof naming an old span — which writes an old row —
    /// never does, so one bond cannot take the head of every template with stale renewals.
    #[test]
    fn only_a_proof_that_renews_a_lapsing_row_escalates() {
        let r = row(100, 2);
        assert!(palw_readiness_proof_escalates_v1(Some(&r), 106, 2, 106, 1, &G, true), "this span's proof at age 6");
        assert!(!palw_readiness_proof_escalates_v1(Some(&r), 105, 2, 105, 1, &G, true), "at age 5: the ordinary lane");
        assert!(palw_readiness_proof_escalates_v1(None, 7, 2, 7, 1, &G, true), "a first proof");
        assert!(!palw_readiness_proof_escalates_v1(Some(&r), 100, 2, 106, 1, &G, true), "a proof as old as the row renews nothing");
        assert!(!palw_readiness_proof_escalates_v1(Some(&r), 104, 2, 110, 1, &G, true), "a renewal that is itself lapsing");
        assert!(palw_readiness_proof_escalates_v1(Some(&r), 105, 2, 110, 1, &G, true), "a renewal with room to count");
        assert!(!palw_readiness_proof_escalates_v1(Some(&r), 106, 1, 106, 1, &G, true), "a V1 proof renews nothing past V2");
    }

    fn lifecycle_tx(object: PalwConsensusObjectV2) -> Transaction {
        let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap();
        Transaction::new(
            0,
            vec![],
            vec![TransactionOutput::new(1, ScriptPublicKey::from_vec(0, vec![0x51]))],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        )
    }

    /// The proof a transaction carries, its lane key, and the gate's decode: every possession proof
    /// and every H-1 carrier is put to the fold, nothing else.
    #[test]
    fn a_proof_is_named_by_its_row_and_put_to_the_gate() {
        let bond = PalwBondKeyV2(TransactionOutpoint::new(TransactionId::from_u64_word(7), 0));
        let class_id = Hash64::from_u64_word(9);
        let proof = crate::palw_artifact::PalwArtifactMultiproofV1 {
            leaf_count: 1,
            opened: vec![(
                0,
                crate::palw_artifact::PalwArtifactOperandV1 { tensor_name: String::new(), layer: None, row_start: 0, bytes: vec![1] },
            )],
            siblings: vec![],
        };
        let v2 = PalwConsensusObjectV2::SeatReadinessProvedV2 { bond, class_id, span: 11, proof: Box::new(proof), signature: vec![1] };
        let tx = lifecycle_tx(v2);
        let carrier = PalwReadinessCarrierV1::of_tx(&tx).expect("a proof");
        assert_eq!(carrier, PalwReadinessCarrierV1 { bond, class_id, span: 11, proof_version: 2 });
        assert_eq!(carrier.lane_key(), PalwH1LaneKeyV1::Readiness(bond, class_id));
        assert!(palw_gated_carrier_object_of_tx_v1(&tx).is_some(), "the gate asks the fold about every proof");
        let accused = lifecycle_tx(PalwConsensusObjectV2::DefaultAccused {
            claim: Hash64::from_u64_word(1),
            missing_event_index: 0,
            accuser: bond,
            signature: vec![1],
        });
        assert!(PalwReadinessCarrierV1::of_tx(&accused).is_none());
        assert!(palw_gated_carrier_object_of_tx_v1(&accused).is_some(), "and about every H-1 carrier, as before");
        let licence = lifecycle_tx(PalwConsensusObjectV2::ReceiptLicensedV2 { claim: class_id, receipts: vec![] });
        assert!(PalwReadinessCarrierV1::of_tx(&licence).is_none() && palw_gated_carrier_object_of_tx_v1(&licence).is_none());
        let mut native = tx.clone();
        native.subnetwork_id = crate::subnets::SUBNETWORK_ID_NATIVE;
        assert!(PalwReadinessCarrierV1::of_tx(&native).is_none(), "another subnetwork is not decoded");
    }
}
