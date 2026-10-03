//! **ADR-0164 F-M1: the riders' outbox** — node policy. The producer builds a lead's riders (it holds the backends, the key and the
//! retention directory) and the panel service owns the carrier lane (the fee UTXO, the mempool door), so the one hands the other a
//! finished `AttemptRidersV1` object here, in process: a plain queue, drained once a tick by the panel loop into its carrier queue.
//! What a restart loses is the queue (a lead whose riders were not filed inside the window simply keeps its whole carve).

use kaspa_consensus_core::{
    Hash64,
    palw_attempt_v2::PalwAttemptEnvelopeV2,
    palw_state_v2::PalwConsensusObjectV2,
};
use std::sync::Mutex;

/// The court-queue round the riders ride under (lane B's is `u32::MAX − 7`, the audit duty's `− 8`).
pub const PALW_RIDER_QUEUE_ROUND_V1: u32 = u32::MAX - 9;

static OUTBOX: Mutex<Vec<(Hash64, PalwConsensusObjectV2)>> = Mutex::new(Vec::new());

/// Queue the riders `riders` of lead claim `lead`.
pub fn palw_rider_outbox_push_v1(lead: Hash64, riders: Vec<PalwAttemptEnvelopeV2>) {
    if let Ok(mut outbox) = OUTBOX.lock() {
        outbox.push((lead, PalwConsensusObjectV2::AttemptRidersV1 { lead, riders }));
    }
}

/// Everything queued, oldest first (the queue is left empty).
pub fn palw_rider_outbox_drain_v1() -> Vec<(Hash64, PalwConsensusObjectV2)> {
    OUTBOX.lock().map(|mut outbox| std::mem::take(&mut *outbox)).unwrap_or_default()
}

/// Whether a court-queue entry is a rider batch.
pub fn palw_rider_queued_v1(round: u32, responder: bool, object: &PalwConsensusObjectV2) -> bool {
    matches!(object, PalwConsensusObjectV2::AttemptRidersV1 { .. }) && (round, responder) == (PALW_RIDER_QUEUE_ROUND_V1, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pushed_batch_is_drained_once_as_a_riders_object_on_the_riders_round() {
        let lead = Hash64::from_bytes([3; 64]);
        palw_rider_outbox_push_v1(lead, Vec::new());
        let drained = palw_rider_outbox_drain_v1();
        let (id, object) = drained.iter().find(|(id, _)| *id == lead).expect("queued");
        assert_eq!(*id, lead);
        assert!(palw_rider_queued_v1(PALW_RIDER_QUEUE_ROUND_V1, false, object));
        assert!(!palw_rider_queued_v1(0, false, object));
        assert!(palw_rider_outbox_drain_v1().iter().all(|(id, _)| *id != lead), "drained once");
    }
}
