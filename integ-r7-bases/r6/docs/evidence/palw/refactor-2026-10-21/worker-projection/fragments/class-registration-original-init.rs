        let mut class_registration: Option<PalwConsensusObjectV2> = None;
        // **Not a latch on the mempool's yes.** A registration is judged when a block ACCEPTS it,
        // and it can be dropped there (the target it names must equal the base class's live one,
        // and an epoch retarget can land while the carrier waits behind up to MAX_INFLIGHT_CARRIERS
        // ancestors). A boolean set on submission made that loss permanent AND silent: the retry
        // was gated off forever by the same flag. This remembers the carrier instead, and the tick
        // clears it once the chain has answered — the class appeared (done) or the carrier is gone
        // from the mempool without one (try again).
        let mut class_registration_inflight: Option<(kaspa_consensus_core::tx::TransactionId, u64)> = None;
        let mut class_registration_done = false;
        // When this node first saw the in-flight carrier gone from its mempool, and how many carriers
        // in a row left it without the class (`palw_class_registration_step_v1`).
        let mut class_registration_left_pool: Option<u64> = None;
        let mut class_registration_drops: u32 = 0;
        // The class id this node registers, derived once (`class_registration_id` re-pairs the
        // holdings, which a 2M legacy class pays in minutes) — what tells a restarted registrant its
        // class already stands on the chain.
        let mut class_registration_known_id: Option<Option<Hash64>> = None;