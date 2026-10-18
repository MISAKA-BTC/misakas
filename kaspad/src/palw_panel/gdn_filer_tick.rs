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
    entries: Vec<(PalwGdnHeldClaimV1, Box<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1>, Vec<u8>, Vec<u32>)>,
    objects: HashMap<Hash64, Vec<PalwConsensusObjectV2>>,
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
    fn material(&self, claim: &PalwGdnHeldClaimV1) -> Result<misaka_palw_base0::gdn_evidence_v2::Base0GdnOwnedMaterialV2, String> {
        let (q36, capture, prompt) = self.q36(claim)?;
        let owned = q36.gdn_accuser_view_v2(capture, Some(prompt))?;
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
    fn openings(
        &self,
        claim: &PalwGdnHeldClaimV1,
    ) -> Result<
        (Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOpeningV1>, kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1),
        String,
    > {
        let (q36, _, _) = self.q36(claim)?;
        let (openings, root) = q36.gdn_inventory_openings_v2()?;
        let oracle = kaspa_consensus_core::palw_artifact::PalwProvenOperandsV1::from_openings_v1(&openings, root)
            .map_err(|e| format!("{e:?}"))?;
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
            let Some(pooled) = materials.get(&d.claim_id) else { continue };
            let Ok(backend) = self.resolve_backend(session, d.class_id, d.artifact_root) else { continue };
            if backend.as_any_v1().and_then(|a| a.downcast_ref::<Qwen36Backend>()).is_none() {
                continue;
            }
            // Read the prompt from the accepted block, or the hash-bound free-prompt job.
            // A held capture need not carry prompt IDs; its job_id is not the block anchor.
            let (capture, prompt) = if d.free_prompt {
                let form = self.class_prompt_ids_form(d.class_id);
                let Some((capture, prompt)) = pooled.iter().find_map(|bytes| {
                    let payload = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_capture_decode_v1(bytes, form)?;
                    if payload.material.job.class_id != d.class_id || payload.material.job.executor_bond != d.executor_bond.0 {
                        return None;
                    }
                    let prompt = Self::fp_prompt_for_job(backend.as_ref(), &payload.material, form)?;
                    Some((payload.capture, prompt))
                }) else {
                    continue;
                };
                (capture, prompt)
            } else {
                let Some((job, prompt)) = self.attempt_job_for_claim(
                    session,
                    backend.as_ref(),
                    network_domain,
                    d.accepted_block,
                    d.class_id,
                    &d.executor_bond,
                ) else {
                    continue;
                };
                let prompt: Vec<u32> = prompt.into_iter().map(|id| id as u32).collect();
                let Some(capture) = pooled
                    .iter()
                    .find(|bytes| {
                        misaka_palw_base0::produce::base0_material_decode_any_v1(bytes)
                            .is_ok_and(|retained| retained.binding().job_context == job)
                    })
                    .cloned()
                else {
                    continue;
                };
                (capture, prompt)
            };
            let claim = PalwGdnHeldClaimV1 {
                claim: d.claim_id,
                executor: d.executor_bond,
                execution_root: d.execution_root,
                trace_root: d.trace_root,
                shape_profile_id: d.class_id,
            };
            entries.push((d.licensed_daa, claim, backend, capture, prompt));
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
            entries: entries.into_iter().map(|(_, c, b, cap, prompt)| (c, b, cap, prompt)).collect(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gdn_service_host_uses_the_claim_prompt_and_rejects_another_binding() {
        let w = crate::palw_gdn_held_filer::tests::world(0);
        let mut claim = crate::palw_gdn_held_filer::tests::claim_of(&w.honest, &w);
        let prompt = w.honest.prompt_token_ids.clone();
        assert!(!prompt.is_empty());
        let sign = |_: &[u8], _: &[u8]| None;
        let mut host = ServiceGdnHostV1 {
            entries: vec![(claim.clone(), Box::new(w.backend), w.capture, prompt.clone())],
            objects: HashMap::new(),
            window: 8,
            bond: claim.executor,
            domain: Hash64::from_u64_word(0),
            sign: &sign,
        };
        let owned = host.material(&claim).expect("the actual claim prompt decodes the held capture");
        assert_eq!(owned.prompt_token_ids, prompt);
        claim.execution_root = Hash64::from_u64_word(0xBAD);
        host.entries[0].0 = claim.clone();
        assert!(host.material(&claim).is_err(), "a capture of another execution cannot supply evidence");
        host.entries[0].0.execution_root = owned.binding.committed_execution_root;
        host.entries[0].3[0] ^= 1;
        assert!(host.material(&host.entries[0].0).is_err(), "a different prompt cannot supply evidence");
    }
}
