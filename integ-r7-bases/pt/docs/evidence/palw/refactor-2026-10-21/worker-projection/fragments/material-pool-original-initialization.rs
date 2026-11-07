        let mut materials: HashMap<Hash64, Vec<Vec<u8>>> = HashMap::new();
        // ADR-0077 Decision 8: the interval openings served to THIS node because it asked. Keyed
        // per `(claim, interval)` and bounded by [`Self::pool_interval_opening`]; the transport
        // has already refused anything unsolicited, over-sized, or past a claim's slot ceiling.
        let mut interval_openings: HashMap<(Hash64, u32), Vec<Vec<u8>>> = HashMap::new();
        // Total pooled bytes and per-claim arrival order, maintained with `materials` so the
        // ceiling above is enforced at insertion rather than recomputed by walking the pool
        // (audit3 S-02). The sequence is a plain counter: it only has to order arrivals, and a DAA
        // score is not available inside the drain.
        let mut pool_bytes: usize = 0;
        let mut pool_arrival: HashMap<Hash64, u64> = HashMap::new();
        let mut pool_arrival_seq: u64 = 0;