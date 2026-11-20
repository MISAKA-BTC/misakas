//! Disposable native Manifest request inputs and fee collection for one real
//! UTXO acceptance pass. This does not select a public policy or authorize a
//! request. The complete ordered PALW replay still prepares each request and
//! derives the expected fees. Body refusal drops the trial without publishing
//! candidates or counting any expected effect as consumed.

use kaspa_consensus_core::{
    Hash64,
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, reserve_native_pending_requests_v1},
    palw_positive_contract_v2::{ManifestRequestFeeV1, NativeManifestRequestCandidateV3},
    palw_state_v2::{PalwBlockContextV2, PalwManifestRequestFeeProjectionV3},
    tx::{Transaction, TransactionIndexType, ValidatedTransaction},
};

pub(in crate::pipeline::virtual_processor) struct NativeManifestRequestPassV3 {
    parent_root: Hash64,
    point: PalwBlockContextV2,
    maximum: usize,
    source_activation_daa: u64,
    expected: Option<PalwManifestRequestFeeProjectionV3>,
    // An actual UTXO trial can expose new/moved/refused carriers. It must be
    // reconciled with a fresh complete-prefix projection before header checks.
    replaying: bool,
    relay_reselection: bool,
    matched: usize,
    carriers: Vec<NativeManifestBodyCarrierV3>,
    _lease: Option<NativeDemandLeaseV1>,
}

pub(in crate::pipeline::virtual_processor) struct NativeManifestBodyCarrierV3 {
    pub candidate: NativeManifestRequestCandidateV3,
    effect: Option<ManifestRequestFeeV1>,
    before_fee: u64,
    collectible: u64,
}

pub(in crate::pipeline::virtual_processor) struct NativeManifestBodyV3 {
    carriers: Vec<NativeManifestBodyCarrierV3>,
    _lease: Option<NativeDemandLeaseV1>,
}

fn fault(why: &str) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<NativeManifestRequestPassV3>("Manifest UTXO pass", why)
}

/// Local amplification allowance across actual UTXO replays. This is not a
/// request tariff or a public consensus parameter. Failed attempts consume it.
pub(super) struct NativeManifestReplayBudgetV3 {
    remaining: usize,
}

impl NativeManifestReplayBudgetV3 {
    pub(super) fn new_v3(passes: usize) -> Self {
        Self { remaining: passes }
    }

    pub(super) fn charge_v3(&mut self) -> Result<(), NativeCoverageV1> {
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| fault("UTXO replay allowance exhausted"))?;
        Ok(())
    }
}

/// Neither change flag establishes a finalized candidate. The complete PALW prefix must
/// still reproduce its fees and side effects after the actual UTXO replay.
#[must_use]
pub(super) struct NativeManifestReplayResultV3 {
    pub(super) acceptance_changed: bool,
    pub(super) round_income_changed: bool,
    pub(super) manifest_projection_changed: bool,
}

fn carrier_storage(count: usize) -> Result<(Vec<NativeManifestBodyCarrierV3>, Option<NativeDemandLeaseV1>), NativeCoverageV1> {
    let bytes = count
        .checked_mul(std::mem::size_of::<NativeManifestBodyCarrierV3>())
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| fault("candidate storage overflow"))?;
    let lease = reserve_native_pending_requests_v1(bytes)?;
    let mut carriers = Vec::new();
    carriers.try_reserve_exact(count).map_err(|_| fault("candidate storage allocation refused"))?;
    if carriers.capacity() != count {
        return Err(fault("candidate allocation differs from its reservation"));
    }
    if let Some(lease) = &lease {
        lease.materialized_v1();
    }
    Ok((carriers, lease))
}

impl NativeManifestRequestPassV3 {
    /// Explicit native owner only; shipped presets leave this draft dormant.
    /// `maximum` must come from bounded candidate preflight, while
    /// `expected` comes from the complete ordered fee projection, never wire.
    pub(super) fn new_v3(
        parent_root: Hash64,
        point: PalwBlockContextV2,
        maximum: usize,
        expected: Option<PalwManifestRequestFeeProjectionV3>,
    ) -> Result<Self, NativeCoverageV1> {
        if parent_root == Hash64::default()
            || point.block == Hash64::default()
            || point.block == kaspa_consensus_core::blockhash::ORIGIN
            || expected.as_ref().is_some_and(|fees| fees.len() > maximum)
        {
            return Err(fault("invalid native pass scope or fee count"));
        }
        let (carriers, lease) = carrier_storage(maximum)?;
        Ok(Self {
            parent_root,
            point,
            maximum,
            source_activation_daa: 0,
            expected,
            // Even an externally supplied empty map requires fresh ordered
            // selection: an unpriced carrier may be valid or an omission.
            replaying: true,
            relay_reselection: false,
            matched: 0,
            carriers,
            _lease: lease,
        })
    }

    pub(super) fn with_source_activation_v3(mut self, daa: u64) -> Self {
        self.source_activation_daa = daa;
        self
    }

    pub(in crate::pipeline::virtual_processor) fn accepts_source_v3(&self, daa: u64) -> bool {
        daa >= self.source_activation_daa
    }

    pub(in crate::pipeline::virtual_processor) fn check_scope_v3(
        &self,
        parent_root: Hash64,
        point: Option<PalwBlockContextV2>,
    ) -> Result<(), NativeCoverageV1> {
        if parent_root != self.parent_root || point != Some(self.point) {
            return Err(fault("UTXO pass differs from its actual parent or accepting point"));
        }
        Ok(())
    }

    pub(in crate::pipeline::virtual_processor) fn rewind_v3(&mut self) {
        self.carriers.clear();
        self.matched = 0;
        // Every replay invalidates earlier reconciliation, even if the next
        // pass eventually reproduces exactly the same acceptance list.
        self.replaying = self.expected.is_some();
        self.relay_reselection = false;
    }

    pub(in crate::pipeline::virtual_processor) fn mark_relay_reselection_v3(&mut self) {
        self.relay_reselection = true;
    }

    /// A complete prefix may price only carriers actually captured by this
    /// pass. Validate every effect before replacing either the old plan or its
    /// candidates. Rebuilt UTXO acceptance must match each effect again.
    pub(in crate::pipeline::virtual_processor) fn install_projection_v3(
        &mut self,
        expected: PalwManifestRequestFeeProjectionV3,
    ) -> Result<(), NativeCoverageV1> {
        if expected.len() > self.maximum {
            return Err(fault("fee projection exceeds candidate preflight"));
        }
        let mut matched = 0usize;
        for row in &self.carriers {
            if let Some(effect) = expected.get(&row.candidate.carrier_v3()) {
                row.candidate.collectible_fee_v3(effect).map_err(|why| match why {
                    NativeDecodeErrorV1::Resource(why) => why,
                    NativeDecodeErrorV1::Decode(_) => fault("projected fee was not derived from this UTXO candidate"),
                })?;
                matched = matched.checked_add(1).ok_or_else(|| fault("projection match count overflow"))?;
            }
        }
        if matched != expected.len() {
            return Err(fault("projection names a carrier outside the captured UTXO pass"));
        }
        self.expected = Some(expected);
        self.rewind_v3();
        Ok(())
    }

    pub(super) fn carriers_v3(&self) -> &[NativeManifestBodyCarrierV3] {
        &self.carriers
    }

    pub(super) fn projection_matches_v3(&self) -> bool {
        !self.relay_reselection
            && self.expected.as_ref().is_some_and(|fees| {
                self.matched == fees.len()
                    && self.carriers.iter().all(|row| row.effect.as_ref() == fees.get(&row.candidate.carrier_v3()))
            })
    }

    /// Only a fresh complete-prefix projection may seal actual collection.
    /// This establishes fee convergence only: strict PALW replay and the
    /// authoritative fold are still required before publishing any state.
    pub(super) fn reconcile_projection_v3(&mut self, fresh: &PalwManifestRequestFeeProjectionV3) -> Result<(), NativeCoverageV1> {
        if !self.projection_matches_v3() || self.expected.as_ref().is_none_or(|expected| expected != fresh) {
            return Err(fault("actual Manifest collection requires fresh selection and projection"));
        }
        self.replaying = false;
        Ok(())
    }

    pub(in crate::pipeline::virtual_processor) fn require_reconciled_v3(&self) -> Result<(), NativeCoverageV1> {
        if self.replaying || !self.projection_matches_v3() {
            return Err(fault("provisional Manifest fees cannot enter header verification"));
        }
        Ok(())
    }

    pub(super) fn check_selected_projection_v3(
        &self,
        selected: &std::collections::BTreeMap<Hash64, ManifestRequestFeeV1>,
    ) -> Result<(), NativeCoverageV1> {
        if !self.projection_matches_v3() || self.expected.as_ref().is_none_or(|expected| &**expected != selected) {
            return Err(fault("fresh selected Manifest fees differ from actual UTXO collection"));
        }
        Ok(())
    }

    /// Used only after full-prefix strict replay checked the selected fees.
    pub(super) fn reconcile_selected_fees_v3(
        &mut self,
        selected: &std::collections::BTreeMap<Hash64, ManifestRequestFeeV1>,
    ) -> Result<(), NativeCoverageV1> {
        self.check_selected_projection_v3(selected)?;
        self.replaying = false;
        Ok(())
    }

    pub(super) fn require_selected_fees_v3(
        &self,
        selected: &std::collections::BTreeMap<Hash64, ManifestRequestFeeV1>,
    ) -> Result<(), NativeCoverageV1> {
        self.require_reconciled_v3()?;
        self.check_selected_projection_v3(selected)
    }

    /// The input includes every Full-validated transaction, including ordinary
    /// carriers. Physical body indices do not substitute for accepted ordinals.
    pub(in crate::pipeline::virtual_processor) fn trial_body_v3(
        &self,
        transactions: &[(ValidatedTransaction<'_>, TransactionIndexType)],
        first_ordinal: usize,
        carrier_block: Hash64,
        mut before_fee: impl FnMut(usize, &ValidatedTransaction<'_>) -> Result<u64, NativeCoverageV1>,
    ) -> Result<NativeManifestBodyV3, NativeCoverageV1> {
        let count = transactions.iter().filter(|(tx, _)| tx.tx.payload.starts_with(b"PMR")).count();
        if count > self.maximum.saturating_sub(self.carriers.len()) {
            return Err(fault("candidate preflight bound exhausted"));
        }
        let (mut carriers, lease) = carrier_storage(count)?;
        for (offset, (tx, _)) in transactions.iter().enumerate() {
            if !tx.tx.payload.starts_with(b"PMR") {
                continue;
            }
            let ordinal = first_ordinal.checked_add(offset).ok_or_else(|| fault("accepted ordinal overflow"))?;
            let before = before_fee(ordinal, tx)?;
            let preceding = tx.calculated_fee.checked_sub(before).ok_or_else(|| fault("preceding fees exceed actual UTXO fee"))?;
            let candidate = match NativeManifestRequestCandidateV3::from_validated_v3(
                tx,
                preceding,
                self.parent_root,
                carrier_block,
                self.point,
                ordinal,
            ) {
                Ok(Some(candidate)) => candidate,
                Ok(None) => continue,
                Err(NativeDecodeErrorV1::Resource(why)) => return Err(why),
                Err(NativeDecodeErrorV1::Decode(why)) => {
                    kaspa_core::debug!("[palw-manifest] carrier {} has no native request input: {why}", tx.tx.id());
                    continue;
                }
            };
            let expected = self.expected.as_ref().and_then(|fees| fees.get(&candidate.carrier_v3())).copied();
            let (effect, collectible) = match expected {
                Some(effect) => {
                    let matched = if self.replaying {
                        candidate.matching_collectible_fee_v3(&effect)
                    } else {
                        candidate.collectible_fee_v3(&effect).map(Some)
                    }
                    .map_err(|why| match why {
                        NativeDecodeErrorV1::Resource(why) => why,
                        NativeDecodeErrorV1::Decode(_) => fault("fee projection differs from actual validated carrier"),
                    })?;
                    match matched {
                        Some(fee) => (Some(effect), fee),
                        // Never debit an old position/fee. This whole trial
                        // remains uncommittable until fresh selection prices it.
                        None => (None, before),
                    }
                }
                None => (None, before),
            };
            if collectible > before {
                return Err(fault("projected fee would increase the miner reward"));
            }
            carriers.push(NativeManifestBodyCarrierV3 { candidate, effect, before_fee: before, collectible });
        }
        Ok(NativeManifestBodyV3 { carriers, _lease: lease })
    }

    /// Commit only after that entire native body has been accepted. Check all
    /// rows before moving any ownership, so a wrong pass cannot consume them.
    pub(in crate::pipeline::virtual_processor) fn commit_body_v3(
        &mut self,
        body: NativeManifestBodyV3,
    ) -> Result<(), NativeCoverageV1> {
        if body.carriers.len() > self.maximum.saturating_sub(self.carriers.len()) {
            return Err(fault("committed body exceeds candidate preflight"));
        }
        let mut previous = self.carriers.last().map(|row| row.candidate.accepted_ordinal_v3());
        let mut matched = self.matched;
        for row in &body.carriers {
            row.candidate.check_context_v3(self.parent_root, &self.point).map_err(|_| fault("body belongs to another native pass"))?;
            let ordinal = row.candidate.accepted_ordinal_v3();
            let expected = self.expected.as_ref().and_then(|fees| fees.get(&row.candidate.carrier_v3())).copied();
            if previous.is_some_and(|prior| prior >= ordinal) || (row.effect != expected && !(self.replaying && row.effect.is_none()))
            {
                return Err(fault("body ordinal or projected fee belongs to another pass"));
            }
            previous = Some(ordinal);
            matched = matched.checked_add(usize::from(row.effect.is_some())).ok_or_else(|| fault("matched fee count overflow"))?;
        }
        self.carriers.extend(body.carriers);
        self.matched = matched;
        Ok(())
    }

    pub(in crate::pipeline::virtual_processor) fn finish_v3(&self) -> Result<(), NativeCoverageV1> {
        if !self.replaying && self.expected.as_ref().is_some_and(|fees| fees.len() != self.matched) {
            return Err(fault("projected fee carrier was not accepted by the actual UTXO pass"));
        }
        Ok(())
    }
}

impl NativeManifestBodyV3 {
    /// Both Round preflight and actual reward collection use this exact read.
    pub(in crate::pipeline::virtual_processor) fn collectible_v3(
        &self,
        ordinal: usize,
        tx: &Transaction,
        before: u64,
    ) -> Result<u64, NativeCoverageV1> {
        let Ok(index) = self.carriers.binary_search_by_key(&ordinal, |row| row.candidate.accepted_ordinal_v3()) else {
            return Ok(before);
        };
        let row = &self.carriers[index];
        if row.candidate.carrier_v3() != tx.id() || !tx.id_is_current() || before != row.before_fee {
            return Err(fault("actual reward input differs from its body trial"));
        }
        Ok(row.collectible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        tx::{PopulatedTransaction, ScriptPublicKey, TransactionInput, TransactionOutpoint, TransactionOutput, UtxoEntry},
    };
    use misaka_palw_positive_contract::{
        identity::{Digest, PUBKEY_BYTES, PrincipalId, SIGNATURE_BYTES},
        public_one_honest::{Claimant, ManifestRequest, OriginalId, PanelBond, SignedManifestRequestV1},
    };

    fn point() -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: Hash64::from_u64_word(10), daa_score: 11, blue_score: 12, subsidy: 13 }
    }

    // Literal/UTXO accounting fixture only. It supplies no authenticated Panel
    // approval or protected original, and cannot construct a paid fee effect.
    fn request_tx() -> Transaction {
        let signed = SignedManifestRequestV1 {
            request: ManifestRequest {
                original: OriginalId(Digest([4; 64])),
                manifest_root: Digest([5; 64]),
                claimant: Claimant::DrawnBond(PanelBond { transaction: Digest([6; 64]), index: 0 }),
                signer: PrincipalId(Digest([7; 64])),
                inspection_scope: Digest([8; 64]),
                supply_obligation: Digest([9; 64]),
                collect_until: 10,
                reservation: Digest([11; 64]),
            },
            key: [0; PUBKEY_BYTES],
            signature: [0; SIGNATURE_BYTES],
        };
        let mut payload = b"PMR1\0".to_vec();
        for word in [1, 2, 3] {
            payload.extend_from_slice(&Hash64::from_u64_word(word).as_bytes());
        }
        payload.extend_from_slice(&borsh::to_vec(&signed).unwrap());
        Transaction::new(
            0,
            vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(20), 0), vec![], 0, 0)],
            vec![TransactionOutput::new(80, ScriptPublicKey::default())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload,
        )
    }

    fn validated(tx: &Transaction, fee: u64) -> ValidatedTransaction<'_> {
        ValidatedTransaction::new(PopulatedTransaction::new(tx, vec![UtxoEntry::new(100, ScriptPublicKey::default(), 1, false)]), fee)
    }

    fn no_effects() -> PalwManifestRequestFeeProjectionV3 {
        kaspa_consensus_core::palw_native_resource_v1::NativeRetainedV1::try_from_clone_v1(&std::collections::BTreeMap::new()).unwrap()
    }

    #[test]
    fn manifest_selection_positions_come_from_complete_utxo_acceptance() {
        use super::super::{PalwCarriedObjectV1, palw_accept::NativeManifestRequestSelectionV3};
        use crate::{model::stores::ghostdag::GhostdagData, pipeline::virtual_processor::utxo_validation::UtxoProcessingContext};
        use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
        use kaspa_muhash::MuHash;
        let parent = Hash64::from_u64_word(30);
        let source = Hash64::from_u64_word(31);
        let ghostdag = GhostdagData { selected_parent: parent, ..Default::default() };
        let mut ctx = UtxoProcessingContext::new((&ghostdag).into(), MuHash::new());
        ctx.positive_accepting_point = Some(point());
        let first = request_tx();
        let mut last = first.clone();
        last.lock_time += 1;
        last.finalize();
        let mut ordinary = first.clone();
        ordinary.payload = b"ordinary".to_vec();
        ordinary.finalize();
        let mut middle = ordinary.clone();
        middle.lock_time += 1;
        middle.finalize();
        let mut pass = NativeManifestRequestPassV3::new_v3(parent, point(), 2, None).unwrap();
        let transactions =
            [(validated(&ordinary, 20), 8), (validated(&first, 20), 2), (validated(&middle, 20), 19), (validated(&last, 20), 0)];
        let body = pass.trial_body_v3(&transactions, 1, source, |_, _| Ok(12)).unwrap();
        pass.commit_body_v3(body).unwrap();
        ctx.palw_manifest_requests_v3 = Some(pass);
        ctx.accepted_tx_ids = vec![Hash64::from_u64_word(32), ordinary.id(), first.id(), middle.id(), last.id()];
        let carried = |id| PalwCarriedObjectV1 {
            object: PalwConsensusObjectV2::BondRetireRequested {
                bond: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(33), 0)),
                signature: Vec::new(),
            },
            native_carrier: id,
            carrier_fee: 0,
            fp_answer: None,
            prefix_dependency: None,
            history_pledge: None,
            tir_public_original: None,
            funded_original: None,
            original_binding: None,
            custody_delegation: None,
            refund: None,
            unrefundable: None,
            winning_carrier: None,
            input_spends: Vec::new(),
        };
        // Derived ordinary objects count as positions; physical body indices
        // and accepted tx ordinals are deliberately different in this fixture.
        let objects = [carried(None), carried(Some(ordinary.id())), carried(Some(middle.id()))];
        let mut quote = |_, _, _| panic!("indexing does not price or authenticate requests");
        let mut selection = NativeManifestRequestSelectionV3::new_v3(&ctx, 1024, 1024, &mut quote).unwrap();
        assert!(selection.index_v3(source, &point(), &ctx.accepted_tx_ids, &objects).is_err());
        selection.index_v3(parent, &point(), &ctx.accepted_tx_ids, &objects).unwrap();
        assert_eq!(selection.candidate_positions_v3(), vec![(2, first.id(), 2), (3, last.id(), 4)]);
        assert!(selection.index_v3(parent, &point(), &ctx.accepted_tx_ids, &objects).is_err());
        drop(selection);
        for bad in [vec![carried(Some(middle.id())), carried(Some(ordinary.id()))], vec![carried(Some(first.id()))]] {
            let mut selection = NativeManifestRequestSelectionV3::new_v3(&ctx, 1024, 1024, &mut quote).unwrap();
            assert!(selection.index_v3(parent, &point(), &ctx.accepted_tx_ids, &bad).is_err());
        }
        let mut selection = NativeManifestRequestSelectionV3::new_v3(&ctx, 1024, 1024, &mut quote).unwrap();
        assert!(selection.index_v3(parent, &point(), &ctx.accepted_tx_ids[1..], &objects).is_err());
        drop(selection);
        assert!(NativeManifestRequestSelectionV3::new_v3(&ctx, 0, 1024, &mut quote).is_err());
    }

    #[test]
    fn manifest_projection_restart_discards_candidates_and_preserves_replay_budget() {
        let parent = Hash64::from_u64_word(30);
        let source = Hash64::from_u64_word(31);
        let mut pass = NativeManifestRequestPassV3::new_v3(parent, point(), 1, None).unwrap();
        let request = request_tx();
        let transactions = [(validated(&request, 20), 4)];
        let body = pass.trial_body_v3(&transactions, 1, source, |_, _| Ok(12)).unwrap();
        pass.commit_body_v3(body).unwrap();
        assert_eq!(pass.carriers_v3().len(), 1);
        let mut budget = NativeManifestReplayBudgetV3::new_v3(2);
        budget.charge_v3().unwrap();
        // No request is authenticated by this fixture; an empty projection
        // is not a paid receipt and must not preserve earlier candidate rows.
        pass.install_projection_v3(no_effects()).unwrap();
        assert!(pass.carriers_v3().is_empty());
        let rebuilt = pass.trial_body_v3(&transactions, 1, source, |_, _| Ok(12)).unwrap();
        pass.commit_body_v3(rebuilt).unwrap();
        pass.finish_v3().unwrap();
        budget.charge_v3().unwrap();
        pass.install_projection_v3(no_effects()).unwrap();
        assert!(pass.carriers_v3().is_empty());
        assert!(budget.charge_v3().is_err());
        assert!(budget.charge_v3().is_err());
        assert!(NativeManifestReplayBudgetV3::new_v3(0).charge_v3().is_err());
    }

    #[test]
    fn manifest_reselection_cannot_verify_headers_until_fresh_fee_reconciliation() {
        let parent = Hash64::from_u64_word(30);
        let source = Hash64::from_u64_word(31);
        let request = request_tx();
        let transactions = [(validated(&request, 20), 4)];
        let mut pass = NativeManifestRequestPassV3::new_v3(parent, point(), 1, None).unwrap();
        assert!(pass.require_reconciled_v3().is_err());
        assert!(pass.require_selected_fees_v3(&std::collections::BTreeMap::new()).is_err());
        let body = pass.trial_body_v3(&transactions, 1, source, |_, _| Ok(12)).unwrap();
        pass.commit_body_v3(body).unwrap();
        pass.install_projection_v3(no_effects()).unwrap();

        // A new/moved raw transport is a candidate for fresh selection, never
        // authority for a paid request. No opaque paid effect is fabricated.
        let body = pass.trial_body_v3(&transactions, 3, source, |_, _| Ok(11)).unwrap();
        assert_eq!(body.collectible_v3(3, &request, 11).unwrap(), 11);
        pass.commit_body_v3(body).unwrap();
        pass.finish_v3().unwrap();
        // No debit is consistent with a semantic omission, but matching
        // fees alone does not authorize header verification. The production
        // caller must re-check that omission in the complete strict prefix.
        assert!(pass.projection_matches_v3());
        assert!(pass.require_reconciled_v3().is_err());
        pass.check_selected_projection_v3(&std::collections::BTreeMap::new()).unwrap();
        assert!(pass.require_reconciled_v3().is_err());

        // A refused body publishes neither its candidates nor old debits.
        // Even a stable empty projection requires explicit reconciliation.
        pass.install_projection_v3(no_effects()).unwrap();
        let refused = pass.trial_body_v3(&transactions, 3, source, |_, _| Ok(11)).unwrap();
        drop(refused);
        pass.finish_v3().unwrap();
        assert!(pass.projection_matches_v3());
        assert!(pass.require_reconciled_v3().is_err());
        pass.reconcile_projection_v3(&no_effects()).unwrap();
        pass.require_reconciled_v3().unwrap();
        pass.require_selected_fees_v3(&std::collections::BTreeMap::new()).unwrap();
        pass.rewind_v3();
        assert!(pass.require_selected_fees_v3(&std::collections::BTreeMap::new()).is_err());
        assert!(pass.projection_matches_v3());
        assert!(pass.require_reconciled_v3().is_err(), "replay invalidates the previous reconciliation");
        // Comparing the new selected map does not itself seal the candidate.
        // The production caller reconciles only after strict full-prefix replay.
        pass.check_selected_projection_v3(&std::collections::BTreeMap::new()).unwrap();
        assert!(pass.require_reconciled_v3().is_err());
        pass.reconcile_selected_fees_v3(&std::collections::BTreeMap::new()).unwrap();
        pass.require_selected_fees_v3(&std::collections::BTreeMap::new()).unwrap();
        pass.mark_relay_reselection_v3();
        assert!(!pass.projection_matches_v3());
        assert!(pass.require_reconciled_v3().is_err(), "an unchanged Manifest map cannot hide a stale relay proposal");
        assert!(pass.reconcile_selected_fees_v3(&std::collections::BTreeMap::new()).is_err());
        pass.rewind_v3();
        assert!(pass.require_reconciled_v3().is_err(), "restart still requires a fresh strict replay");
        pass.reconcile_selected_fees_v3(&std::collections::BTreeMap::new()).unwrap();

        // An externally supplied empty plan is not a reconciled selection.
        let mut strict = NativeManifestRequestPassV3::new_v3(parent, point(), 1, Some(no_effects())).unwrap();
        let body = strict.trial_body_v3(&transactions, 1, source, |_, _| Ok(12)).unwrap();
        strict.commit_body_v3(body).unwrap();
        assert!(strict.require_reconciled_v3().is_err());
    }

    #[test]
    fn manifest_context_restart_clears_derived_outputs_and_keeps_parent_inputs() {
        use crate::{model::stores::ghostdag::GhostdagData, pipeline::virtual_processor::utxo_validation::UtxoProcessingContext};
        use kaspa_consensus_core::{
            acceptance_data::{AcceptedTxEntry, MergesetBlockAcceptanceData},
            coinbase::BlockRewardData,
            palw_bond_budget_v1::PalwRoundFeeIncomeV1,
            palw_state_v2::PalwBondKeyV2,
        };
        use kaspa_muhash::MuHash;
        let parent = Hash64::from_u64_word(30);
        let source = Hash64::from_u64_word(31);
        let ghostdag = GhostdagData { selected_parent: parent, ..Default::default() };
        let mut ctx = UtxoProcessingContext::new((&ghostdag).into(), MuHash::new());
        ctx.positive_accepting_point = Some(point());
        let request = request_tx();
        let lock = TransactionOutpoint::new(Hash64::from_u64_word(32), 0);
        ctx.palw_v2_locked_bonds.insert(lock);
        ctx.accepted_tx_ids.push(request.id());
        ctx.mergeset_acceptance_data.push(MergesetBlockAcceptanceData {
            block_hash: source,
            accepted_transactions: vec![AcceptedTxEntry { transaction_id: request.id(), index_within_block: 4 }],
        });
        ctx.mergeset_rewards.insert(source, BlockRewardData::new(1, 2, 3, ScriptPublicKey::default()));
        ctx.mergeset_diff.add.insert(lock, UtxoEntry::new(80, ScriptPublicKey::default(), 1, false));
        ctx.multiset_hash.add_element(b"old candidate");
        ctx.reserve_accrual = 9;
        ctx.palw_v2_accepted_tx_fees.insert(request.id(), 20);
        ctx.palw_v2_rent_available_fees.insert(request.id(), 12);
        let income = PalwRoundFeeIncomeV1 { block: source, bond: PalwBondKeyV2(lock), maximum_sompi: 12, actual_sompi: 12 };
        ctx.palw_round_fee_incomes.push(income);
        // Refusing a missing original owner must precede any reset.
        assert!(ctx.restart_with_manifest_projection_v3(MuHash::new(), no_effects(), None).is_err());
        assert_eq!(ctx.accepted_tx_ids, vec![request.id()]);
        assert_eq!(ctx.palw_round_fee_incomes, vec![income]);
        let mut pass = NativeManifestRequestPassV3::new_v3(parent, point(), 1, None).unwrap();
        let body = pass.trial_body_v3(&[(validated(&request, 20), 4)], 1, source, |_, _| Ok(12)).unwrap();
        pass.commit_body_v3(body).unwrap();
        ctx.palw_manifest_requests_v3 = Some(pass);
        let (old, incomes) = ctx.restart_with_manifest_projection_v3(MuHash::new(), no_effects(), None).unwrap();
        assert_eq!(old.len(), 1);
        assert_eq!(old[0].accepted_transactions[0].transaction_id, request.id());
        assert_eq!(incomes, vec![income]);
        assert!(ctx.accepted_tx_ids.is_empty());
        assert!(ctx.mergeset_acceptance_data.is_empty());
        assert!(ctx.mergeset_rewards.is_empty());
        assert!(ctx.mergeset_diff.add.is_empty());
        assert!(ctx.mergeset_diff.remove.is_empty());
        assert_eq!(ctx.multiset_hash.finalize(), MuHash::new().finalize());
        assert_eq!(ctx.reserve_accrual, 0);
        assert!(ctx.palw_v2_accepted_tx_fees.is_empty());
        assert!(ctx.palw_v2_rent_available_fees.is_empty());
        assert!(ctx.palw_round_fee_incomes.is_empty());
        assert!(ctx.palw_manifest_requests_v3.as_ref().unwrap().carriers_v3().is_empty());
        assert_eq!(ctx.positive_accepting_point, Some(point()));
        assert!(ctx.palw_v2_locked_bonds.contains(&lock));
        assert_eq!(ctx.selected_parent(), parent);
    }

    #[test]
    fn manifest_restart_discards_bridge_effects_and_header_outputs() {
        use crate::{
            model::stores::ghostdag::GhostdagData, pipeline::virtual_processor::utxo_validation::UtxoProcessingContext,
            processes::evm::apply_evm_bridge_effects,
        };
        use kaspa_consensus_core::{evm::WithdrawOp, muhash::MuHashExtensions};
        use kaspa_muhash::MuHash;
        let parent = Hash64::from_u64_word(30);
        let ghostdag = GhostdagData { selected_parent: parent, ..Default::default() };
        let lock = TransactionOutpoint::new(Hash64::from_u64_word(32), 0);
        let entry = UtxoEntry::new(80, ScriptPublicKey::default(), 1, false);
        let mut parent_multiset = MuHash::new();
        parent_multiset.add_utxo(&lock, &entry);
        let mut ctx = UtxoProcessingContext::new((&ghostdag).into(), parent_multiset.clone());
        ctx.positive_accepting_point = Some(point());
        ctx.palw_manifest_requests_v3 = Some(NativeManifestRequestPassV3::new_v3(parent, point(), 0, None).unwrap());
        let consumed = [(lock, entry)];
        let withdrawals = [WithdrawOp {
            receipt_index: 0,
            op_index: 0,
            evm_tx_hash: Default::default(),
            from: Default::default(),
            script_public_key: ScriptPublicKey::default(),
            amount_sompi: 9,
        }];
        apply_evm_bridge_effects(&mut ctx.mergeset_diff, &mut ctx.multiset_hash, point().daa_score, &consumed, &withdrawals).unwrap();
        let staged_multiset = ctx.multiset_hash.clone().finalize();
        assert_ne!(staged_multiset, parent_multiset.clone().finalize());
        assert_eq!(ctx.mergeset_diff.add.len(), 1);
        assert_eq!(ctx.mergeset_diff.remove.len(), 1);
        ctx.pruning_sample_from_pov = Some(Hash64::from_u64_word(33));
        ctx.validator_rewarded_keys.push((lock, 4));
        ctx.validator_quality_subpool = 5;
        ctx.reserve_balance_after = 6;
        ctx.reserve_accrual = 7;
        ctx.require_no_evm_stage_v3().unwrap();
        // Ownership fixture, not an authenticated EVM execution result: the
        // actual bridge effects above are what the reset must discard with it.
        ctx.evm_staged = Some(crate::processes::evm::EvmStaged {
            result: kaspa_consensus_core::evm::EvmExecutionResult { withdrawals: withdrawals.to_vec(), ..Default::default() },
            snapshot: Default::default(),
            candidate_meta: Vec::new(),
            trace_body: None,
            state_diff: None,
        });
        assert!(ctx.require_no_evm_stage_v3().is_err());
        let pass = ctx.palw_manifest_requests_v3.take();
        assert!(ctx.restart_with_manifest_projection_v3(parent_multiset.clone(), no_effects(), None).is_err());
        assert_eq!(ctx.evm_staged.as_ref().unwrap().result.withdrawals, withdrawals);
        assert_eq!(ctx.multiset_hash.clone().finalize(), staged_multiset);
        assert!(ctx.pruning_sample_from_pov.is_some());
        ctx.palw_manifest_requests_v3 = pass;
        ctx.restart_with_manifest_projection_v3(parent_multiset.clone(), no_effects(), None).unwrap();
        assert!(ctx.evm_staged.is_none());
        ctx.require_no_evm_stage_v3().unwrap();
        assert!(ctx.pruning_sample_from_pov.is_none());
        assert!(ctx.validator_rewarded_keys.is_empty());
        assert_eq!(ctx.validator_quality_subpool, 0);
        assert_eq!(ctx.reserve_balance_after, 0);
        assert_eq!(ctx.reserve_accrual, 0);
        assert!(ctx.mergeset_diff.add.is_empty());
        assert!(ctx.mergeset_diff.remove.is_empty());
        assert_eq!(ctx.multiset_hash.clone().finalize(), parent_multiset.finalize());
        assert_eq!(ctx.positive_accepting_point, Some(point()));
        assert_eq!(ctx.selected_parent(), parent);
        assert!(ctx.palw_manifest_requests_v3.as_ref().unwrap().require_reconciled_v3().is_err());
        // The new candidate can apply the real bridge effects once, from the
        // original parent, without retaining or duplicating the earlier diff.
        apply_evm_bridge_effects(&mut ctx.mergeset_diff, &mut ctx.multiset_hash, point().daa_score, &consumed, &withdrawals).unwrap();
        assert_eq!(ctx.multiset_hash.finalize(), staged_multiset);
        assert_eq!(ctx.mergeset_diff.add.len(), 1);
        assert_eq!(ctx.mergeset_diff.remove.len(), 1);
        assert!(ctx.pruning_sample_from_pov.is_none());
    }

    #[test]
    fn manifest_utxo_trials_publish_only_committed_bodies_and_rewind() {
        let parent = Hash64::from_u64_word(30);
        let source = Hash64::from_u64_word(31);
        let mut pass = NativeManifestRequestPassV3::new_v3(parent, point(), 2, None).unwrap();
        let request = request_tx();
        let mut ordinary = request.clone();
        ordinary.payload = b"ordinary".to_vec();
        ordinary.finalize();
        let transactions = [(validated(&ordinary, 20), 8), (validated(&request, 20), 99)];
        let trial = pass.trial_body_v3(&transactions, 4, source, |_, _| Ok(12)).unwrap();
        assert_eq!(trial.carriers[0].candidate.accepted_ordinal_v3(), 5);
        assert_eq!(trial.collectible_v3(5, &request, 12).unwrap(), 12);
        assert!(trial.collectible_v3(5, &request, 11).is_err());
        assert!(trial.collectible_v3(5, &ordinary, 12).is_err());
        drop(trial); // rejected whole body, not an accepted request
        assert!(pass.carriers_v3().is_empty());
        pass.finish_v3().unwrap();
        let accepted = pass.trial_body_v3(&transactions, 4, source, |_, _| Ok(12)).unwrap();
        pass.commit_body_v3(accepted).unwrap();
        assert_eq!(pass.carriers_v3().len(), 1);
        assert_eq!(pass.carriers_v3()[0].candidate.carrier_v3(), request.id());
        let duplicate = pass.trial_body_v3(&transactions, 4, source, |_, _| Ok(12)).unwrap();
        assert!(pass.commit_body_v3(duplicate).is_err());
        assert_eq!(pass.carriers_v3().len(), 1);
        let mut later = request.clone();
        later.lock_time += 1;
        later.finalize();
        let later_body = [(validated(&later, 20), 1)];
        let accepted = pass.trial_body_v3(&later_body, 6, source, |_, _| Ok(12)).unwrap();
        pass.commit_body_v3(accepted).unwrap();
        assert_eq!(pass.carriers_v3().len(), 2);
        assert!(pass.trial_body_v3(&later_body, 7, source, |_, _| Ok(12)).is_err());
        pass.rewind_v3();
        assert!(pass.carriers_v3().is_empty());
        let retried = pass.trial_body_v3(&transactions, 4, source, |_, _| Ok(12)).unwrap();
        pass.commit_body_v3(retried).unwrap();
        assert_eq!(pass.carriers_v3().len(), 1);
        pass.finish_v3().unwrap();
    }

    #[test]
    fn manifest_utxo_body_and_scope_cannot_move_to_another_pass() {
        let parent = Hash64::from_u64_word(30);
        let source = Hash64::from_u64_word(31);
        let pass = NativeManifestRequestPassV3::new_v3(parent, point(), 1, None).unwrap();
        pass.check_scope_v3(parent, Some(point())).unwrap();
        assert!(pass.check_scope_v3(parent, None).is_err());
        assert!(pass.check_scope_v3(source, Some(point())).is_err());
        assert!(pass.check_scope_v3(parent, Some(PalwBlockContextV2 { subsidy: 14, ..point() })).is_err());
        let request = request_tx();
        let transactions = [(validated(&request, 20), 1)];
        let body = pass.trial_body_v3(&transactions, 4, source, |_, _| Ok(12)).unwrap();
        let mut foreign = NativeManifestRequestPassV3::new_v3(source, point(), 1, None).unwrap();
        assert!(foreign.commit_body_v3(body).is_err());
        assert!(foreign.carriers_v3().is_empty());
        assert!(pass.trial_body_v3(&transactions, 4, source, |_, _| Ok(21)).is_err());
        let zero = NativeManifestRequestPassV3::new_v3(parent, point(), 0, None).unwrap();
        assert!(zero.trial_body_v3(&transactions, 4, source, |_, _| Ok(12)).is_err());
        let mut ordinary = request.clone();
        ordinary.payload.clear();
        ordinary.finalize();
        let overflow = [(validated(&ordinary, 20), 1), (validated(&request, 20), 2)];
        assert!(pass.trial_body_v3(&overflow, usize::MAX, source, |_, _| Ok(12)).is_err());
    }

    #[test]
    fn malformed_or_wrong_fee_manifest_transport_retains_no_candidate() {
        let parent = Hash64::from_u64_word(30);
        let source = Hash64::from_u64_word(31);
        let mut pass = NativeManifestRequestPassV3::new_v3(parent, point(), 1, None).unwrap();
        let good = request_tx();
        let mut unknown = good.clone();
        unknown.payload[..4].copy_from_slice(b"PMR9");
        unknown.finalize();
        for (tx, fee) in [(&unknown, 20), (&good, 19)] {
            let body = pass.trial_body_v3(&[(validated(tx, fee), 1)], 4, source, |_, _| Ok(12)).unwrap();
            assert!(body.carriers.is_empty());
            assert_eq!(body.collectible_v3(4, tx, 12).unwrap(), 12);
            pass.commit_body_v3(body).unwrap();
            assert!(pass.carriers_v3().is_empty());
        }
        pass.finish_v3().unwrap();
    }
}
