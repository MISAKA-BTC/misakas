//! **RFC-0006 06-A on the service tick**: the held-GDN filer ([`crate::palw_gdn_held_filer`]) over this node's chain view — the
//! claims its bond may dispute, the served captures it holds (public material; never the producer's heap), the Qwen3.6 hybrid
//! backend of the claim's class (its own recompute kernels, a cold cache each run) and the accepted objects naming the claim. Its
//! objects ride the priority lane (`court_pending`, due now). Off below `palw_memory_routing_court_v2` (every preset) and on a node
//! that does not challenge.
use super::*;
use crate::palw_gdn_held_filer::{PalwGdnHistoryWalkV1, PalwGdnHeldBookV1, PalwGdnHeldClaimV1, PalwGdnHeldHostV1, palw_gdn_held_filer_tick_v1};
use crate::palw_panel::court_outbox::CourtOutboxV1;
use misaka_palw_base0::qwen36_backend::Qwen36Backend;
use kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1;

#[path = "gdn_filer_capture_owner_v3.rs"]
mod capture_owner;

/// The queue round of a GDN filer object, below the dissection's rounds and beside step 6's.
pub(crate) const PALW_GDN_FILER_ROUND_V1: u32 = (1 << 29) | 0x200;

struct ServiceGdnHostV1<'a> {
    entries: Vec<(PalwGdnHeldClaimV1, Box<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1>, NativeRetainedV1<Vec<u8>>, NativeRetainedV1<Vec<u32>>)>,
    objects: HashMap<Hash64, Vec<PalwConsensusObjectV2>>,
    history_daa: Option<u64>,
    window: u64,
    bond: PalwBondKeyV2,
    domain: Hash64,
    sign: &'a (dyn Fn(&[u8], &[u8]) -> Option<Vec<u8>> + Sync),
}

impl ServiceGdnHostV1<'_> {
    fn q36(&self, claim: &PalwGdnHeldClaimV1) -> Result<(&Qwen36Backend, &[u8], &[u32]), String> {
        let (_, backend, capture, prompt) =
            self.entries.iter().find(|(c, _, _, _)| c.claim == claim.claim).ok_or("no capture of this claim")?;
        let q36 = backend.as_any_v1().and_then(|a| a.downcast_ref::<Qwen36Backend>()).ok_or("not a Qwen3.6 hybrid class")?;
        Ok((q36, capture, prompt))
    }
}

impl PalwGdnHeldHostV1 for ServiceGdnHostV1<'_> {
    fn accusable_claims(&self) -> Vec<PalwGdnHeldClaimV1> {
        self.entries.iter().map(|(c, _, _, _)| c.clone()).collect()
    }
    fn material(&self, claim: &PalwGdnHeldClaimV1) -> Result<NativeRetainedV1<misaka_palw_base0::gdn_evidence_v2::Base0GdnOwnedMaterialV2>, String> {
        let (q36, capture, prompt) = self.q36(claim)?;
        let owned = q36.gdn_accuser_view_retained_v3(capture, Some(prompt), claim.execution_root, claim.shape_profile_id).map_err(|e|e.to_string())?;
        if owned.binding.committed_execution_root != claim.execution_root
            || owned.binding.shape_profile.shape_profile_id() != claim.shape_profile_id
        {
            return Err("the served capture does not name this claim's binding".into());
        }
        Ok(owned)
    }
    fn kernels(
        &self,
        claim: &PalwGdnHeldClaimV1,
    ) -> Result<Box<dyn misaka_palw_base0::fp_recompute::Base0FpRecomputeKernelsV1 + '_>, String> {
        let (q36, _, _) = self.q36(claim)?;
        Ok(Box::new(q36.gdn_recompute_kernels_v2()?))
    }
    fn objects(&self, claim: &PalwGdnHeldClaimV1) -> Vec<PalwConsensusObjectV2> {
        self.objects.get(&claim.claim).cloned().unwrap_or_default()
    }
    fn openings(&self, claim: &PalwGdnHeldClaimV1) -> Result<misaka_palw_base0::GdnInventoryOracleV2, String> {
        let (q36, _, _) = self.q36(claim)?;
        let (openings, root) = q36.gdn_inventory_openings_v2()?;
        openings.into_oracle_v2(root)
    }
    fn observed_through_daa(&self, _now: u64) -> Option<u64> { self.history_daa }
    fn disclose_window_daa(&self) -> u64 {
        self.window
    }
    fn bond(&self) -> PalwBondKeyV2 {
        self.bond
    }
    fn network_domain(&self) -> Hash64 {
        self.domain
    }
    fn sign(&self, message: &[u8], context: &[u8]) -> Option<Vec<u8>> {
        (self.sign)(message, context)
    }
}

// Select by the chain's commitment before taking the first held capture: an unrelated
// payload in the pool must not starve a later capture of this claim.
fn capture_names_claim_v1(capture: &[u8], execution_root: Hash64, class_id: Hash64) -> bool {
    misaka_palw_base0::qwen36_backend::gdn_capture_owner_v3::binding_from_capture_v3(capture).is_ok_and(|binding| {
        binding.committed_execution_root == execution_root && binding.shape_profile.shape_profile_id() == class_id
    })
}

/// The claim an accepted object answers for, if it is a held DA disclosure.
fn disclosure_claim_v1(object: &PalwConsensusObjectV2) -> Option<Hash64> {
    match object {
        PalwConsensusObjectV2::MaterialDisclosedV2 { claim, .. } => Some(*claim),
        PalwConsensusObjectV2::MaterialDisclosedHeld { disclosure } => Some(disclosure.claim),
        _ => None,
    }
}

/// How many chain blocks one tick reads back for disclosures, at most.
const PALW_GDN_FILER_WALK_BLOCKS_V1: usize = 4096;

/// Collect one strict native-history page. A read failure discards the partial
/// collection; a continuation identifies an unfinished interval, not absence.
pub(super) fn gdn_disclosure_page_v1(
    consensus: &dyn kaspa_consensus_core::api::ConsensusApi,
    start: Hash64,
    floor: u64,
    blocks: usize,
    wanted: &HashSet<Hash64>,
) -> Result<(Option<Hash64>, Vec<PalwConsensusObjectV2>), String> {
    let mut found = Vec::new();
    let next = walk_accepted_lifecycle_page_v1(consensus, start, floor, blocks, &mut |object| {
        if disclosure_claim_v1(&object).is_some_and(|claim| wanted.contains(&claim)) {
            found.push(object);
        }
    })?;
    Ok((next, found))
}

pub(super) fn gdn_history_branch_v1(c: &dyn kaspa_consensus_core::api::ConsensusApi, anchor: Hash64) -> Result<bool, String> {
    let tip = c.get_sink();
    if tip == anchor { return Ok(true); }
    c.is_chain_ancestor_of(anchor, tip).map_err(|e| format!("GDN history branch: {e}"))
}

/// Continue a frozen cohort at a fixed native anchor. New claims wait for the
/// next scan, so arrival traffic cannot restart an unfinished old interval.
pub(super) fn gdn_history_next_page_v1(
    c: &dyn kaspa_consensus_core::api::ConsensusApi,
    previous: Option<PalwGdnHistoryWalkV1>, floor: u64, wanted: HashSet<Hash64>, blocks: usize,
) -> Result<(PalwGdnHistoryWalkV1, Vec<PalwConsensusObjectV2>), String> {
    if let Some(previous) = &previous {
        if !gdn_history_branch_v1(c, previous.anchor)? { return Err("GDN history anchor changed branch".into()); }
    }
    let mut walk = match previous.filter(|walk| walk.next.is_some()) {
        Some(walk) => walk,
        None => {
            let anchor = c.get_sink();
            let anchor_daa = c.get_header(anchor).map_err(|e| format!("GDN history header: {e}"))?.daa_score;
            PalwGdnHistoryWalkV1 { anchor, anchor_daa, floor, next: Some(anchor), wanted }
        }
    };
    let (next, found) = gdn_disclosure_page_v1(c, walk.next.unwrap_or(walk.anchor), walk.floor, blocks, &walk.wanted)?;
    if !gdn_history_branch_v1(c, walk.anchor)? { return Err("GDN history branch changed during page".into()); }
    walk.next = next;
    Ok((walk, found))
}

/// One claim a GDN filer may pursue, with the class backend and the served capture (public material) that names it.
pub(super) struct GdnFilerEntryV1 {
    pub(super) d: kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2,
    pub(super) backend: Box<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1>,
    pub(super) capture: NativeRetainedV1<Vec<u8>>,
    pub(super) prompt: NativeRetainedV1<Vec<u32>>,
}

impl PalwPanelService {
    /// **The claims a GDN filer (held checkpoint, 06-A; or step, G14 ③) may pursue** — disputable by `bond_key`, of a Qwen3.6 hybrid
    /// class, with a pooled served capture that names the claim's execution and its job's prompt. Shared by both filers.
    pub(super) async fn gdn_filer_entries_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        skip: impl Fn(&Hash64) -> bool,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
    ) -> Vec<GdnFilerEntryV1> {
        let targets = session.clone().spawn_blocking(move |c| c.palw_disputable_claims_v2(vec![bond_key])).await;
        let targets: Vec<_> = targets.into_iter().filter(|d| !skip(&d.claim_id)).collect();
        self.gdn_filer_entries_from_v1(session, targets, materials, network_domain, bond_key)
    }

    /// The material half of [`Self::gdn_filer_entries_v1`] over targets the caller already discovered (the step filer discovers
    /// every claim inside tag 243's window, not only `ReceiptLicensed` ones — lane gdn-filer round 2). No await: nothing borrowed
    /// by the caller is held across a suspension point.
    pub(super) fn gdn_filer_entries_from_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        targets: Vec<kaspa_consensus_core::palw_producer_v2::PalwDisputableClaimV2>,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
    ) -> Vec<GdnFilerEntryV1> {
        let mut entries = Vec::new();
        for d in targets {
            if d.executor_bond == bond_key {
                continue;
            }
            let Some(pooled) = materials.get(&d.claim_id) else { continue };
            let Ok(backend) = self.resolve_backend(session, d.class_id, d.artifact_root) else { continue };
            if backend.as_any_v1().and_then(|a| a.downcast_ref::<Qwen36Backend>()).is_none() {
                continue;
            }
            let (capture,prompt)=match self.gdn_capture_for_claim_v3(session,&d,backend.as_ref(),pooled,network_domain) {
                Ok(Some(material))=>material,
                Ok(None)=>continue,
                Err(why)=>{ debug!("[{PALW_PANEL}] claim {}: GDN capture admission withheld: {why}",d.claim_id); continue; }
            };
            entries.push(GdnFilerEntryV1 { d, backend, capture, prompt });
        }
        entries
    }
}

impl PalwPanelService {
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn gdn_held_filer_tick_v1(
        &self,
        session: &kaspa_consensusmanager::ConsensusProxy,
        book: &mut PalwGdnHeldBookV1,
        materials: &HashMap<Hash64, Vec<Vec<u8>>>,
        current_daa: u64,
        network_domain: Hash64,
        bond_key: PalwBondKeyV2,
        court_outbox: &mut CourtOutboxV1,
    ) {
        use kaspa_consensus_core::config::params::ForkActivation;
        let armed = self
            .consensus_config
            .params
            .palw_memory_routing_court_v2_fence()
            .is_some_and(|f| f != ForkActivation::never() && f.is_active(current_daa));
        if !armed || !self.config.challenge {
            return;
        }
        let Some(kp) = self.keypair.clone() else { return };
        if let Some(previous) = &book.history {
            let anchor = previous.anchor;
            match session.clone().spawn_blocking(move |c| gdn_history_branch_v1(c, anchor)).await {
                Ok(true) => {},
                Ok(false) => { book.cases.clear(); book.settled.clear(); book.history = None; book.history_failed_claims.clear(); },
                Err(why) => { warn!("[{PALW_PANEL}] GDN history unavailable: {why}; filer tick waits"); return; }
            }
        }
        let mut entries: Vec<_> = self
            .gdn_filer_entries_v1(session, |claim| book.settled.contains_key(claim), materials, network_domain, bond_key)
            .await
            .into_iter()
            .map(|e| {
                let claim = PalwGdnHeldClaimV1 {
                    claim: e.d.claim_id,
                    executor: e.d.executor_bond,
                    execution_root: e.d.execution_root,
                    trace_root: e.d.trace_root,
                    shape_profile_id: e.d.class_id,
                };
                (e.d.licensed_daa, claim, e.backend, e.capture, e.prompt)
            })
            .collect();
        if entries.is_empty() {
            return;
        }
        let floor = entries.iter().map(|(at, ..)| *at).min().unwrap_or(0);
        let wanted: HashSet<Hash64> = entries.iter().map(|(_, c, ..)| c.claim).collect();
        let previous = book.history.clone();
        let new_scan = previous.as_ref().is_none_or(|walk| walk.next.is_none());
        let page = session.clone().spawn_blocking(move |c| {
            gdn_history_next_page_v1(c, previous, floor, wanted, PALW_GDN_FILER_WALK_BLOCKS_V1)
        }).await;
        let (walk, found) = match page {
            Ok(page) => page,
            Err(why) => {
                warn!("[{PALW_PANEL}] held-GDN disclosure history unavailable: {why}; cursor unchanged");
                return;
            }
        };
        if new_scan { book.history_failed_claims.clear(); }
        entries.retain(|(_, claim, ..)| walk.wanted.contains(&claim.claim));
        let history_daa = walk.next.is_none().then_some(walk.anchor_daa);
        book.history = Some(walk);
        let mut objects: HashMap<Hash64, Vec<PalwConsensusObjectV2>> = HashMap::new();
        for object in found {
            if let Some(claim) = disclosure_claim_v1(&object) {
                objects.entry(claim).or_default().push(object);
            }
        }
        let window = match &self.consensus_config.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(b) => {
                kaspa_consensus_core::palw_state_v2::palw_da_disclose_window_daa_v1(&b.state)
            }
            _ => return,
        };
        let sign = move |message: &[u8], context: &[u8]| Self::sign_hedged(&kp.signing_key, message, context);
        let host = ServiceGdnHostV1 {
            entries: entries.into_iter().map(|(_, c, b, cap, prompt)| (c, b, cap, prompt)).collect(),
            objects,
            history_daa,
            window,
            bond: bond_key,
            domain: network_domain,
            sign: &sign,
        };
        gdn_emit_into_outbox_v2(&host, book, current_daa, court_outbox);
    }
}

fn gdn_emit_into_outbox_v2<H: PalwGdnHeldHostV1>(host: &H, book: &mut PalwGdnHeldBookV1, current_daa: u64, court_outbox: &mut CourtOutboxV1) {
        let mut queued = 0u32;
        crate::palw_gdn_held_filer::palw_gdn_held_filer_emit_v2(host, book, current_daa, |claim, object| {
            let round = PALW_GDN_FILER_ROUND_V1.checked_add(queued).ok_or_else(||
                kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::overflow::<CourtOutboxV1>("held GDN outbox round"))?;
            let next = queued.checked_add(1).ok_or_else(||
                kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1::overflow::<CourtOutboxV1>("held GDN outbox count"))?;
            court_outbox.try_push_clone_with_due((claim, round, false), object, current_daa)?;
            queued = next;
            info!("[{PALW_PANEL}] claim {claim}: the held-GDN filer queues {} (RFC-0006 06-A)", object_name(object));
            Ok(())
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gdn_service_host_uses_the_claim_prompt_and_rejects_another_binding() {
        let w = crate::palw_gdn_held_filer::tests::world(0);
        let mut claim = crate::palw_gdn_held_filer::tests::claim_of(&w.honest, &w);
        assert!(capture_names_claim_v1(&w.capture, claim.execution_root, claim.shape_profile_id));
        assert!(!capture_names_claim_v1(&w.capture, Hash64::from_u64_word(0xBAD), claim.shape_profile_id));
        let forged_claim = crate::palw_gdn_held_filer::tests::claim_of(&w.forged, &w);
        let forged_capture = w.forged_capture.clone();
        let prompt = w.honest.prompt_token_ids.clone();
        assert!(!prompt.is_empty());
        let mut outsider = claim.executor;
        outsider.0.index += 1;
        let sign = |_: &[u8], _: &[u8]| None;
        let mut host = ServiceGdnHostV1 {
            entries: vec![(claim.clone(), Box::new(w.backend), NativeRetainedV1::retain_existing_v1(w.capture).unwrap(), NativeRetainedV1::try_from_clone_v1(&prompt).unwrap())],
            objects: HashMap::new(),
            history_daa: Some(1),
            window: 8,
            bond: outsider,
            domain: Hash64::from_u64_word(0),
            sign: &sign,
        };
        let owned = host.material(&claim).expect("the actual claim prompt decodes the held capture");
        assert_eq!(owned.prompt_token_ids, prompt);
        let mut honest_book = PalwGdnHeldBookV1::default();
        assert!(palw_gdn_held_filer_tick_v1(&host, &mut honest_book, 1).is_empty());
        assert_eq!(honest_book.settled.get(&claim.claim), Some(&"honest"));
        host.entries[0].0 = forged_claim.clone();
        host.entries[0].2 = NativeRetainedV1::retain_existing_v1(forged_capture).unwrap();
        let forged = host.material(&forged_claim).expect("checkpoint mismatch is readable for GDN accusation");
        assert_ne!(forged.binding.checkpoint_merkle_root, owned.binding.checkpoint_merkle_root);
        let mut expected_step_binding = owned.binding.clone();
        expected_step_binding.checkpoint_merkle_root = forged.binding.checkpoint_merkle_root;
        expected_step_binding.committed_execution_root =
            kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(&expected_step_binding);
        assert_eq!(forged.binding, expected_step_binding);
        let mut forged_book = PalwGdnHeldBookV1::default();
        host.history_daa = None;
        assert!(palw_gdn_held_filer_tick_v1(&host, &mut forged_book, 1).is_empty());
        assert!(forged_book.cases.contains_key(&forged_claim.claim), "the service host opens the forged checkpoint case");
        assert!(forged_book.cases[&forged_claim.claim].demanded.is_none(), "partial history emits no demand");
        forged_book.cases.get_mut(&forged_claim.claim).unwrap().demanded = Some((0, 1));
        assert!(palw_gdn_held_filer_tick_v1(&host, &mut forged_book, 100).is_empty());
        assert!(!forged_book.settled.contains_key(&forged_claim.claim), "partial history cannot finalize a timeout");
        forged_book.history = Some(PalwGdnHistoryWalkV1 {
            anchor: Hash64::from_u64_word(9), anchor_daa: 100, floor: 0, next: Some(Hash64::from_u64_word(8)),
            wanted: HashSet::from([forged_claim.claim]),
        });
        let unavailable = host.entries.pop().unwrap();
        assert!(palw_gdn_held_filer_tick_v1(&host, &mut forged_book, 101).is_empty());
        host.entries.push(unavailable);
        host.history_daa = Some(102);
        assert!(palw_gdn_held_filer_tick_v1(&host, &mut forged_book, 102).is_empty());
        assert!(forged_book.history_failed_claims.contains(&forged_claim.claim),
            "a claim absent during one page cannot use the cohort's completion as its own complete history");


        let sign_demand = |_: &[u8], _: &[u8]| Some(vec![1]);
        host.sign = &sign_demand;
        host.history_daa = Some(1);
        let mut backfilled = PalwGdnHeldBookV1::default();
        assert_eq!(palw_gdn_held_filer_tick_v1(&host, &mut backfilled, 100).len(), 1);
        assert_eq!(backfilled.cases[&forged_claim.claim].demanded.unwrap().1, 100,
            "a newly queued demand uses the current clock, not the historical watermark");
        assert!(palw_gdn_held_filer_tick_v1(&host, &mut backfilled, 101).is_empty());
        assert!(!backfilled.settled.contains_key(&forged_claim.claim), "an old snapshot cannot time out a fresh demand");
        claim.execution_root = Hash64::from_u64_word(0xBAD);
        host.entries[0].0 = claim.clone();
        assert!(host.material(&claim).is_err(), "a capture of another execution cannot supply evidence");
        host.entries[0].0.execution_root = forged.binding.committed_execution_root;
        let mut foreign_prompt = prompt.clone();
        foreign_prompt[0] ^= 1;
        host.entries[0].3 = NativeRetainedV1::retain_existing_v1(foreign_prompt).unwrap();
        assert!(host.material(&host.entries[0].0).is_err(), "a different prompt cannot supply evidence");
    }

    #[test]
    fn retained_service_gdn_demand_uses_atomic_outbox_handoff_before_advancing_book() {
        let w = crate::palw_gdn_held_filer::tests::world(0);
        let claim = crate::palw_gdn_held_filer::tests::claim_of(&w.forged, &w);
        let prompt = w.forged.prompt_token_ids.clone();
        let mut outsider = claim.executor; outsider.0.index += 1;
        let sign = |_: &[u8], _: &[u8]| Some(vec![1]);
        let host = ServiceGdnHostV1 {
            entries: vec![(claim.clone(), Box::new(w.backend), NativeRetainedV1::retain_existing_v1(w.forged_capture).unwrap(), NativeRetainedV1::retain_existing_v1(prompt).unwrap())],
            objects: HashMap::new(), history_daa: Some(1), window: 8, bond: outsider,
            domain: Hash64::from_u64_word(0), sign: &sign,
        };
        let mut book = PalwGdnHeldBookV1::default();
        let mut outbox = CourtOutboxV1::default();
        gdn_emit_into_outbox_v2(&host, &mut book, 100, &mut outbox);
        assert_eq!(outbox.pending().len(), 1);
        assert!(matches!(&outbox.pending()[0].3, PalwConsensusObjectV2::DefaultAccusedHeld { .. }));
        assert_eq!(book.cases[&claim.claim].demanded.unwrap().1, 100);
        assert!(!book.settled.contains_key(&claim.claim));
        assert_eq!(outbox.forget_due(&(claim.claim, PALW_GDN_FILER_ROUND_V1, false)), Some(100));
    }

}
