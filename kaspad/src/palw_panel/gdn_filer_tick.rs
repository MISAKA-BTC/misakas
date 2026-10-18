//! **RFC-0006 06-A on the service tick**: the held-GDN filer ([`crate::palw_gdn_held_filer`]) over this node's chain view — the
//! claims its bond may dispute, the served captures it holds (public material; never the producer's heap), the Qwen3.6 hybrid
//! backend of the claim's class (its own recompute kernels, a cold cache each run) and the accepted objects naming the claim. Its
//! objects ride the priority lane (`court_pending`, due now). Off below `palw_memory_routing_court_v2` (every preset) and on a node
//! that does not challenge.
use super::*;
use crate::palw_gdn_held_filer::{PalwGdnHeldBookV1, PalwGdnHeldClaimV1, PalwGdnHeldHostV1, palw_gdn_held_filer_tick_v1};
use misaka_palw_base0::qwen36_backend::Qwen36Backend;

/// The queue round of a GDN filer object, below the dissection's rounds and beside step 6's.
pub(crate) const PALW_GDN_FILER_ROUND_V1: u32 = (1 << 29) | 0x200;

struct ServiceGdnHostV1<'a> {
    entries: Vec<(PalwGdnHeldClaimV1, Box<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1>, Vec<u8>)>,
    objects: HashMap<Hash64, Vec<PalwConsensusObjectV2>>,
    window: u64,
    bond: PalwBondKeyV2,
    domain: Hash64,
    sign: &'a (dyn Fn(&[u8], &[u8]) -> Option<Vec<u8>> + Sync),
}

impl ServiceGdnHostV1<'_> {
    fn q36(&self, claim: &PalwGdnHeldClaimV1) -> Result<(&Qwen36Backend, &[u8]), String> {
        let (_, backend, capture) = self.entries.iter().find(|(c, _, _)| c.claim == claim.claim).ok_or("no capture of this claim")?;
        let q36 = backend.as_any_v1().and_then(|a| a.downcast_ref::<Qwen36Backend>()).ok_or("not a Qwen3.6 hybrid class")?;
        Ok((q36, capture))
    }
}

impl PalwGdnHeldHostV1 for ServiceGdnHostV1<'_> {
    fn accusable_claims(&self) -> Vec<PalwGdnHeldClaimV1> {
        self.entries.iter().map(|(c, _, _)| c.clone()).collect()
    }
    fn material(&self, claim: &PalwGdnHeldClaimV1) -> Result<misaka_palw_base0::gdn_evidence_v2::Base0GdnOwnedMaterialV2, String> {
        let (q36, capture) = self.q36(claim)?;
        q36.gdn_accuser_view_v2(capture, None)
    }
    fn kernels(&self, claim: &PalwGdnHeldClaimV1) -> Result<Box<dyn misaka_palw_base0::fp_recompute::Base0FpRecomputeKernelsV1 + '_>, String> {
        let (q36, _) = self.q36(claim)?;
        Ok(Box::new(q36.gdn_recompute_kernels_v2()?))
    }
    fn objects(&self, claim: &PalwGdnHeldClaimV1) -> Vec<PalwConsensusObjectV2> {
        self.objects.get(&claim.claim).cloned().unwrap_or_default()
    }
    fn openings(
        &self,
        claim: &PalwGdnHeldClaimV1,
    ) -> Result<(Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1>, kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1), String>
    {
        let (q36, _) = self.q36(claim)?;
        let (openings, root) = q36.gdn_inventory_openings_v2()?;
        let oracle =
            kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, root).map_err(|e| format!("{e:?}"))?;
        Ok((openings, oracle))
    }
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
        court_pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,
        court_due: &mut HashMap<(Hash64, u32, bool), u64>,
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
        let targets = session.clone().spawn_blocking(move |c| c.palw_disputable_claims_v2(vec![bond_key])).await;
        let mut entries = Vec::new();
        for d in targets {
            if d.executor_bond == bond_key || book.settled.contains_key(&d.claim_id) {
                continue;
            }
            let Some(capture) = materials.get(&d.claim_id).and_then(|v| v.first()).cloned() else { continue };
            let Ok(backend) = self.resolve_backend(session, d.class_id, d.artifact_root) else { continue };
            if backend.as_any_v1().and_then(|a| a.downcast_ref::<Qwen36Backend>()).is_none() {
                continue;
            }
            let claim = PalwGdnHeldClaimV1 {
                claim: d.claim_id,
                executor: d.executor_bond,
                execution_root: d.execution_root,
                trace_root: d.trace_root,
                shape_profile_id: d.class_id,
            };
            entries.push((d.licensed_daa, claim, backend, capture));
        }
        if entries.is_empty() {
            return;
        }
        let floor = entries.iter().map(|(at, ..)| *at).min().unwrap_or(0);
        let wanted: HashSet<Hash64> = entries.iter().map(|(_, c, ..)| c.claim).collect();
        let found = session
            .clone()
            .spawn_blocking(move |c| {
                let mut found: Vec<PalwConsensusObjectV2> = Vec::new();
                let tip = c.get_sink();
                let _ = walk_accepted_lifecycle_page_v1(c, tip, floor, PALW_GDN_FILER_WALK_BLOCKS_V1, &mut |object| {
                    if disclosure_claim_v1(&object).is_some_and(|claim| wanted.contains(&claim)) {
                        found.push(object);
                    }
                });
                found
            })
            .await;
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
            entries: entries.into_iter().map(|(_, c, b, cap)| (c, b, cap)).collect(),
            objects,
            window,
            bond: bond_key,
            domain: network_domain,
            sign: &sign,
        };
        for (i, (claim, object)) in palw_gdn_held_filer_tick_v1(&host, book, current_daa).into_iter().enumerate() {
            let round = PALW_GDN_FILER_ROUND_V1 + i as u32;
            info!("[{PALW_PANEL}] claim {claim}: the held-GDN filer queues {} (RFC-0006 06-A)", object_name(&object));
            court_pending.push((claim, round, false, object));
            court_due.insert((claim, round, false), current_daa);
        }
    }
}
