//! **ADR-0132 — the node's end-to-end economics recorder, its per-class telemetry, and the
//! provider that hands both to `getPalwClassEconomics` (op 185).** Shadow: nothing consensus reads.
//!
//! The recorder follows every attempt-lane claim the chain still holds, writes one row per claim
//! into the node's meta database (`PalwEconomicsLedgerV1`), and keeps the row past the chain's
//! retention, so the node can say what a class was actually paid over a window longer than the
//! state remembers — with the payout derived by the rule in force at the claim's `Final`
//! (`palw_ledger_payout_v1`) and the compute the claim cost from the class's own graph and the two
//! lotteries (class draws from the class target, network draws from the accepted block's `bits`).
//! It refreshes every [`PALW_ECONOMICS_REFRESH_SECS`] seconds on its own thread and before every
//! op 185 answer.
//!
//! The telemetry is what THIS node did: its producer's draws (class wins, blocks produced, the
//! milliseconds a forward took, the MiB the process read from storage during it — ADR-0112's
//! number) and its seat's replays and receipts, per class, since the process started.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::palw_economic_compute_v1::{
    PALW_ECONOMIC_COST_TABLE_V1, PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_attempt_economic_compute_v1,
    palw_network_expected_attempts_q32_v1,
};
use kaspa_consensus_core::palw_economics_ledger_v1::{
    PALW_ECONOMICS_LEDGER_VERSION_V1, PalwClaimLedgerRowV1, PalwClassLedgerTotalsV1, PalwLedgerClassFactsV1, PalwLedgerPayoutRuleV1,
    palw_class_ledger_totals_v1, palw_ledger_merge_v1,
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_database::prelude::{CachePolicy, CachedDbAccess, CachedDbItem, DB, DirectDbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash64;
use kaspa_rpc_service::service::{
    PalwClassLedgerCompute, PalwClassLedgerContext, PalwClassLedgerProvider, PalwClassNodeTelemetry, PalwEconomicsLedgerSummary,
};
use log::{debug, info, warn};

use crate::palw_class_context::PalwClassLedgerCellV1;

/// How often the recorder's own thread brings the ledger up to the chain.
pub const PALW_ECONOMICS_REFRESH_SECS: u64 = 60;
/// Two reads inside this many seconds share one refresh.
const PALW_ECONOMICS_REFRESH_MIN_GAP_SECS: u64 = 5;

/// **Per-class counters of what this node did**, shared by the producer, the panel and the RPC.
#[derive(Default, Debug)]
pub struct PalwNodeTelemetryV1 {
    classes: Mutex<BTreeMap<Hash64, PalwClassNodeTelemetry>>,
}

impl PalwNodeTelemetryV1 {
    fn with(&self, class: Hash64, f: impl FnOnce(&mut PalwClassNodeTelemetry)) {
        if let Ok(mut classes) = self.classes.lock() {
            f(classes.entry(class).or_default());
        }
    }

    /// One draw: whether the class ticket won, whether the network draw then won (a block), how
    /// long the forward took, and what the process read from storage during it.
    pub fn producer_draw(&self, class: Hash64, class_won: bool, produced: bool, millis: u64, storage_read_mib: u64) {
        self.with(class, |c| {
            c.draws += 1;
            c.class_wins += u64::from(class_won);
            c.produced += u64::from(produced);
            c.draw_millis = c.draw_millis.saturating_add(millis);
            c.storage_read_mib = c.storage_read_mib.saturating_add(storage_read_mib);
        });
    }

    /// One whole-job replay by this seat, and the leaves it replayed.
    pub fn panel_replay(&self, class: Hash64, millis: u64, leaves: u64) {
        self.with(class, |c| {
            c.replays += 1;
            c.replay_millis = c.replay_millis.saturating_add(millis);
            c.replay_leaves = c.replay_leaves.saturating_add(leaves);
        });
    }

    /// One receipt this seat filed, by the verdict's name.
    pub fn panel_receipt(&self, class: Hash64, verdict: &str) {
        self.with(class, |c| match verdict {
            "Valid" => c.receipts_valid += 1,
            "Unavailable" => c.receipts_unavailable += 1,
            "Incapable" => c.receipts_incapable += 1,
            _ => c.receipts_other += 1,
        });
    }

    /// Openings this seat held when it drew its intervals for a duty.
    pub fn panel_openings_held(&self, class: Hash64, held: u64) {
        self.with(class, |c| c.openings_held = c.openings_held.saturating_add(held));
    }

    pub fn snapshot(&self, class: Hash64) -> Option<PalwClassNodeTelemetry> {
        self.classes.lock().ok()?.get(&class).copied()
    }
}

/// **The ledger's rows in the meta database**, one per claim id, under a schema marker.
pub struct DbPalwEconomicsLedgerStoreV1 {
    access: CachedDbAccess<Hash64, Arc<PalwClaimLedgerRowV1>>,
    schema: CachedDbItem<u32>,
    db: Arc<DB>,
}

impl DbPalwEconomicsLedgerStoreV1 {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            access: CachedDbAccess::new(
                Arc::clone(&db),
                CachePolicy::Count(4_096),
                DatabaseStorePrefixes::PalwEconomicsLedgerV1.into(),
            ),
            schema: CachedDbItem::new(Arc::clone(&db), DatabaseStorePrefixes::PalwEconomicsLedgerSchemaV1.into()),
            db,
        }
    }

    /// Rows written under another layout are dropped: they are a measurement the chain
    /// re-supplies, never a fact only the store holds.
    pub fn ensure_schema(&mut self) -> StoreResult<()> {
        let stored = match self.schema.read() {
            Ok(v) => Some(v),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(e) => return Err(e),
        };
        if stored != Some(PALW_ECONOMICS_LEDGER_VERSION_V1) {
            if stored.is_some() {
                warn!(
                    "[palw-economics] the ledger's rows were written under layout {stored:?}; dropping them for v{PALW_ECONOMICS_LEDGER_VERSION_V1}"
                );
            }
            self.access.delete_all(DirectDbWriter::new(&self.db))?;
            self.schema.write(DirectDbWriter::new(&self.db), &PALW_ECONOMICS_LEDGER_VERSION_V1)?;
        }
        Ok(())
    }

    pub fn rows(&self) -> Vec<PalwClaimLedgerRowV1> {
        self.access.iterator().filter_map(|r| r.ok().map(|(_, row)| (*row).clone())).collect()
    }

    pub fn write(&self, rows: &[PalwClaimLedgerRowV1]) -> StoreResult<()> {
        for row in rows {
            self.access.write(DirectDbWriter::new(&self.db), row.claim_id, Arc::new(row.clone()))?;
        }
        Ok(())
    }
}

/// **The recorder**: the store, the chain, and the class facts it prices rows with.
pub struct PalwEconomicsRecorderV1 {
    store: Mutex<DbPalwEconomicsLedgerStoreV1>,
    consensus_manager: Arc<ConsensusManager>,
    config: Arc<Config>,
    ledger: PalwClassLedgerCellV1,
    last_refresh: Mutex<Option<std::time::Instant>>,
}

impl PalwEconomicsRecorderV1 {
    pub fn new(
        store: DbPalwEconomicsLedgerStoreV1,
        consensus_manager: Arc<ConsensusManager>,
        config: Arc<Config>,
        ledger: PalwClassLedgerCellV1,
    ) -> Self {
        Self { store: Mutex::new(store), consensus_manager, config, ledger, last_refresh: Mutex::new(None) }
    }

    /// Bring the ledger up to the chain; `Ok(rows written)`. Two calls inside
    /// [`PALW_ECONOMICS_REFRESH_MIN_GAP_SECS`] share the first.
    pub fn refresh(&self) -> Result<usize, String> {
        {
            let mut last = self.last_refresh.lock().map_err(|_| "the refresh clock is poisoned")?;
            if last.is_some_and(|at| at.elapsed().as_secs() < PALW_ECONOMICS_REFRESH_MIN_GAP_SECS) {
                return Ok(0);
            }
            *last = Some(std::time::Instant::now());
        }
        let session = self.consensus_manager.consensus().unguarded_session_blocking();
        let census = session.palw_class_census_v1().ok_or("no class census: not a ConsensusV2 network, or no state yet")?;
        let observations = session.palw_claim_ledger_observations_v1().ok_or("no claim observations")?;
        let params = &self.config.params;
        let prefill_draw = params.palw_prefill_draw_active_at(census.tip_daa);
        // ADR-0124 Decision 6's unit as the fold reads it: the dearest leaves among the Active,
        // weight-bearing model classes.
        let unit_leaves = census
            .classes
            .iter()
            .filter(|r| !r.is_base_class && r.share_permille.unwrap_or(0) > 0 && r.status == "Active")
            .map(|r| r.pwu_per_inference)
            .max()
            .unwrap_or(0);
        let facts: HashMap<Hash64, (u128, u128, u64, bool)> = census
            .classes
            .iter()
            .map(|row| {
                let draw = match session.palw_registered_class_carriage_v1(row.class_id) {
                    Some((profile, canonical)) => {
                        palw_attempt_economic_compute_v1(&profile, &canonical, prefill_draw, &PALW_ECONOMIC_COST_TABLE_V1).unwrap_or(0)
                    }
                    None => self
                        .ledger
                        .get()
                        .ledger_compute(row.class_id)
                        .map(|c| if prefill_draw { c.draw } else { c.canonical })
                        .unwrap_or(0),
                };
                (row.class_id, (row.expected_attempts_q32, draw, row.pwu_per_inference, row.is_base_class))
            })
            .collect();
        let mut store = self.store.lock().map_err(|_| "the ledger store is poisoned")?;
        store.ensure_schema().map_err(|e| format!("ledger schema: {e}"))?;
        let existing: HashMap<Hash64, PalwClaimLedgerRowV1> = store.rows().into_iter().map(|r| (r.claim_id, r)).collect();
        let mut bits_of: HashMap<BlockHash, u32> = HashMap::new();
        let mut changed = Vec::new();
        for obs in &observations {
            let Some(&(expected_attempts_q32, draw_compute, leaves, is_base)) = facts.get(&obs.class_id) else { continue };
            let previous = existing.get(&obs.claim_id);
            let network_expected_attempts_q32 = match previous {
                Some(row) => row.network_expected_attempts_q32,
                None => {
                    let bits = *bits_of
                        .entry(obs.accepted_block)
                        .or_insert_with(|| session.get_header(obs.accepted_block).map(|h| h.bits).unwrap_or(0));
                    if bits == 0 { PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 } else { palw_network_expected_attempts_q32_v1(bits) }
                }
            };
            let class_facts = PalwLedgerClassFactsV1 { expected_attempts_q32, network_expected_attempts_q32, draw_compute, leaves };
            let row = palw_ledger_merge_v1(previous, obs, census.tip_daa, class_facts, |final_daa| PalwLedgerPayoutRuleV1 {
                work_priced: !is_base && params.palw_work_priced_reward_active_at(final_daa),
                leaves,
                unit_leaves,
                panel_economy: params.palw_panel_economy_active_at(final_daa),
                // ADR-0132 Upgrade C: the merge fills this from the row's own snapshot.
                economic: None,
            });
            if previous != Some(&row) {
                changed.push(row);
            }
        }
        store.write(&changed).map_err(|e| format!("ledger write: {e}"))?;
        debug!("[palw-economics] refreshed at DAA {}: {} of {} claims changed", census.tip_daa, changed.len(), observations.len());
        Ok(changed.len())
    }

    pub fn totals(&self, class_id: Hash64) -> Option<PalwClassLedgerTotalsV1> {
        let rows = self.store.lock().ok()?.rows();
        Some(palw_class_ledger_totals_v1(&rows, class_id))
    }

    pub fn summary(&self) -> Option<PalwEconomicsLedgerSummary> {
        let rows = self.store.lock().ok()?.rows();
        let (first, last) = rows.iter().fold((u64::MAX, 0u64), |(f, l), r| (f.min(r.accepted_daa), l.max(r.accepted_daa)));
        Some(PalwEconomicsLedgerSummary {
            claims: rows.len() as u64,
            first_daa: if rows.is_empty() { 0 } else { first },
            last_daa: last,
        })
    }
}

/// The recorder's own thread: a refresh every [`PALW_ECONOMICS_REFRESH_SECS`] seconds, for the life
/// of the process. Errors are logged at debug (a node that is still syncing has no state to read).
pub fn spawn_recorder_thread(recorder: Arc<PalwEconomicsRecorderV1>) {
    let spawned = std::thread::Builder::new().name("palw-economics".to_string()).spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(PALW_ECONOMICS_REFRESH_SECS));
            match recorder.refresh() {
                Ok(n) if n > 0 => info!("[palw-economics] ledger: {n} claim row(s) written"),
                Ok(_) => {}
                Err(e) => debug!("[palw-economics] refresh skipped: {e}"),
            }
        }
    });
    if spawned.is_err() {
        warn!("[palw-economics] the recorder thread could not be started; op 185 refreshes the ledger on read only");
    }
}

/// **What op 185 reads on this node**: the build's class ledger, the recorder, and the telemetry.
pub struct PalwEconomicsNodeProviderV1 {
    pub cell: PalwClassLedgerCellV1,
    pub recorder: Option<Arc<PalwEconomicsRecorderV1>>,
    pub telemetry: Arc<PalwNodeTelemetryV1>,
}

impl PalwClassLedgerProvider for PalwEconomicsNodeProviderV1 {
    fn class_context(&self, class_id: Hash64) -> Option<PalwClassLedgerContext> {
        self.cell.get().ledger_context(class_id)
    }

    fn class_economic_compute(&self, class_id: Hash64) -> Option<PalwClassLedgerCompute> {
        self.cell.get().ledger_compute(class_id)
    }

    fn refresh_economics_ledger(&self) {
        if let Some(recorder) = &self.recorder
            && let Err(e) = recorder.refresh()
        {
            debug!("[palw-economics] refresh on read skipped: {e}");
        }
    }

    fn economics_ledger_summary(&self) -> Option<PalwEconomicsLedgerSummary> {
        self.recorder.as_ref()?.summary()
    }

    fn class_ledger_totals(&self, class_id: Hash64) -> Option<PalwClassLedgerTotalsV1> {
        self.recorder.as_ref()?.totals(class_id)
    }

    fn class_node_telemetry(&self, class_id: Hash64) -> Option<PalwClassNodeTelemetry> {
        self.telemetry.snapshot(class_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::palw_economics_ledger_v1::PalwClaimLedgerRowV1;
    use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;

    fn row(n: u64, class: u64) -> PalwClaimLedgerRowV1 {
        PalwClaimLedgerRowV1 {
            version: PALW_ECONOMICS_LEDGER_VERSION_V1,
            claim_id: Hash64::from_u64_word(n),
            class_id: Hash64::from_u64_word(class),
            producer_bond: TransactionOutpoint::new(TransactionId::from_u64_word(1), 0),
            accepted_daa: 4_000 + n,
            accepted_block: Hash64::from_u64_word(9_000 + n),
            escrow_sompi: 320_084_650_080,
            pwu: 1,
            expected_attempts_q32: PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1,
            network_expected_attempts_q32: 2 * PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1,
            draw_compute: 83_102_171_136,
            leaves: 6_630_544,
            first_seen_daa: 4_001 + n,
            last_seen_daa: 4_001 + n,
            bound_daa: Some(4_020 + n),
            rebound_daa: None,
            licensed_daa: None,
            final_daa: None,
            voided_daa: None,
            void_reason: String::new(),
            seats: 5,
            credited_seats: 0,
            producer_paid_sompi: 0,
            panel_paid_sompi: 0,
            reserve_sompi: 0,
            burned_sompi: 0,
            paid_at_acceptance: false,
            economic_snapshotted: false,
            economic_rate_sompi_per_giga: 0,
            economic_panel_share_permille: 0,
        }
    }

    /// **The store round-trips rows by claim id, upserts, and drops rows of another layout.**
    #[test]
    fn adr0132_the_ledger_store_round_trips_and_drops_a_foreign_layout() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPalwEconomicsLedgerStoreV1::new(db.clone());
        store.ensure_schema().unwrap();
        store.write(&[row(1, 7), row(2, 7), row(3, 8)]).unwrap();
        let mut rows = store.rows();
        rows.sort_by_key(|r| r.accepted_daa);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], row(1, 7));
        let mut updated = row(2, 7);
        updated.final_daa = Some(6_020);
        updated.producer_paid_sompi = 1;
        store.write(std::slice::from_ref(&updated)).unwrap();
        let rows = store.rows();
        assert_eq!(rows.len(), 3, "an upsert, not a second row");
        assert!(rows.iter().any(|r| r == &updated));
        let totals = palw_class_ledger_totals_v1(&rows, Hash64::from_u64_word(7));
        assert_eq!((totals.claims, totals.finals, totals.bound), (2, 1, 2));
        // Another layout: the marker moves, the rows go.
        store.schema.write(DirectDbWriter::new(&db), &(PALW_ECONOMICS_LEDGER_VERSION_V1 + 1)).unwrap();
        store.ensure_schema().unwrap();
        assert!(store.rows().is_empty());
        assert_eq!(store.schema.read().unwrap(), PALW_ECONOMICS_LEDGER_VERSION_V1);
    }

    /// **The telemetry counts per class and answers nothing for a class it never touched.**
    #[test]
    fn adr0132_the_telemetry_counts_per_class() {
        let t = PalwNodeTelemetryV1::default();
        let (dense, hybrid) = (Hash64::from_u64_word(1), Hash64::from_u64_word(2));
        t.producer_draw(hybrid, true, false, 120_000, 9_700);
        t.producer_draw(hybrid, true, true, 118_000, 9_650);
        t.producer_draw(hybrid, false, false, 121_000, 9_800);
        t.panel_replay(dense, 20_100, 6_508_520);
        t.panel_receipt(dense, "Valid");
        t.panel_receipt(dense, "Unavailable");
        t.panel_receipt(hybrid, "Incapable");
        t.panel_openings_held(dense, 4);
        let h = t.snapshot(hybrid).unwrap();
        assert_eq!((h.draws, h.class_wins, h.produced, h.draw_millis, h.storage_read_mib), (3, 2, 1, 359_000, 29_150));
        assert_eq!(h.receipts_incapable, 1);
        let d = t.snapshot(dense).unwrap();
        assert_eq!(
            (d.replays, d.replay_millis, d.replay_leaves, d.receipts_valid, d.receipts_unavailable, d.openings_held),
            (1, 20_100, 6_508_520, 1, 1, 4)
        );
        assert!(t.snapshot(Hash64::from_u64_word(3)).is_none());
    }
}
