//! Independent native audit rooted in explicitly pinned child/selected-parent
//! headers and the original consensus state. A pin's chain/fork-choice trust is
//! external; producer flags and pooled payout amounts never mint this type.
use kaspa_consensus_core::{
    Hash64,
    config::params::Params,
    header::Header,
    palw_fork_choice_commitment_v1::{PalwForkChoiceOpeningV1, PalwForkChoicePointV1},
    palw_native_audit_proof_v32::{VerifiedNativeAuxRowsV32, verify_native_aux_v32, verify_native_payouts_v32},
    palw_native_audit_v27::CheckedNativeExecutionAuditViewV27,
    palw_state_proof_v1::{PalwFactProofV1, state_root_of_pinned_header_v1},
};
use misaka_palw_kernel::hash::{Digest, object_id};

pub struct AnchoredNativeExecutionAuditV32<'a> {
    child: Hash64,
    selected_parent: Hash64,
    inner_state: Hash64,
    auxiliary: VerifiedNativeAuxRowsV32<'a>,
    view: CheckedNativeExecutionAuditViewV27,
    payouts: &'a [(Vec<u8>, Vec<u8>)],
    payout_proof: &'a PalwFactProofV1,
    qualified: bool,
}
impl AnchoredNativeExecutionAuditV32<'_> {
    pub fn rfc02_qualified(&self) -> bool {
        self.qualified
    }
    pub fn child(&self) -> Hash64 {
        self.child
    }
    pub fn selected_parent(&self) -> Hash64 {
        self.selected_parent
    }
    pub fn inner_state(&self) -> Hash64 {
        self.inner_state
    }
    pub fn native(&self) -> &CheckedNativeExecutionAuditViewV27 {
        &self.view
    }
    pub fn auxiliary(&self) -> &VerifiedNativeAuxRowsV32<'_> {
        &self.auxiliary
    }
    pub fn payout_proof(&self) -> &PalwFactProofV1 {
        self.payout_proof
    }
    pub fn pending_payout_rows(&self) -> &[(Vec<u8>, Vec<u8>)] {
        self.payouts
    }
    pub fn root(&self) -> Digest {
        object_id(
            b"misaka-palw/sdk/pinned-native-state-audit/v32",
            &(self.child, self.selected_parent, self.inner_state, self.auxiliary.root(), self.view.root()),
        )
    }
}
/// `pinned_selected_parent` must come from the independent validated chain
/// selection, not an untrusted RPC assertion. The child header only proves its
/// parent set and state commitment. The caller's own ruleset determines the
/// envelope fence; raw inner roots are never accepted through an active fence.
pub fn verify_native_execution_audit_v32<'a>(
    child: &Header,
    pinned_child: Hash64,
    parent: &Header,
    pinned_selected_parent: Hash64,
    parameters: &Params,
    fork_opening: Option<&PalwForkChoiceOpeningV1>,
    auxiliary: &'a PalwFactProofV1,
    payouts: &'a PalwFactProofV1,
    native: &CheckedNativeExecutionAuditViewV27,
) -> Result<AnchoredNativeExecutionAuditV32<'a>, String> {
    let inner =
        native_inner_state_of_pinned_headers_v32(child, pinned_child, parent, pinned_selected_parent, parameters, fork_opening)?;
    let verified = verify_native_aux_v32(auxiliary, inner)?;
    let payout_rows = verify_native_payouts_v32(payouts, inner)?;
    let record = native.record();
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        parameters.net.to_string().as_bytes(),
        Some(parameters.genesis.hash),
    )
    .as_bytes();
    let final_fence = parameters.palw_native_final_v27.as_ref().ok_or("native32 original Final27 fence unavailable")?;
    let execution_fence = parameters.palw_funded_execution_v11.as_ref().ok_or("native32 original execution11 fence unavailable")?;
    if !final_fence.activation.is_active(parent.daa_score)
        || !execution_fence.activation.is_active(parent.daa_score)
        || record.network_domain != domain
        || record.original_final_policy != final_fence.policy.root()
        // 11 = original funded v11 Final; 29 = Text29 producer Final (same shared
        // unique credit identity, extracted by the Text29 fold's own row reader).
        || !matches!(record.namespace, 11 | 29)
    {
        return Err("native32 trusted original network/active execution/Final policy differs".into());
    }
    if record.state_root != inner.as_bytes()
        || record.aux_root != verified.root().as_bytes()
        || record.ledger_root != verified.ledger_root().as_bytes()
        || record.daa > parent.daa_score
        || record.established_daa > record.daa
        || record.rows.is_empty()
        || record.rows.len() > 16
        || record.rows.iter().any(|row| verified.row(row.table, &row.key) != Some(row.bytes.as_slice()))
    {
        return Err("native32 Final projection differs from exact original committed rows/state/clock".into());
    }
    let queue_key = borsh::to_vec(&Hash64::from_bytes(record.payout_queue)).map_err(|e| e.to_string())?;
    let observed = payout_rows
        .iter()
        .find(|(key, _)| key == &queue_key)
        .map(|(_, value)| {
            borsh::from_slice::<kaspa_consensus_core::palw_state_v2::PalwPayoutV2>(value)
                .map_err(|_| "native32 canonical payout payload/amount".to_string())
        })
        .transpose()?;
    if observed.as_ref().map(|p| p.amount) != record.observed_queue_amount
        || observed.as_ref().is_some_and(|p| p.payload.as_bytes() != record.payout_payload)
    {
        return Err("native32 pooled queue observation differs from original committed payout rows".into());
    }
    // This type proves Final and its one original native credit. Spending that
    // payout as a real UTXO remains a separately verified transaction proof.
    Ok(AnchoredNativeExecutionAuditV32 {
        child: pinned_child,
        selected_parent: pinned_selected_parent,
        inner_state: inner,
        auxiliary: verified,
        view: native.clone(),
        payouts: payout_rows,
        payout_proof: payouts,
        qualified: parameters.native_final_is_qualified_v34(),
    })
}

/// Header/own-fence checks shared with the stateless remote auditor. An RPC
/// committed_form flag never selects the hash interpretation.
pub fn native_inner_state_of_pinned_headers_v32(
    child: &Header,
    pinned_child: Hash64,
    parent: &Header,
    pinned_selected_parent: Hash64,
    parameters: &Params,
    fork_opening: Option<&PalwForkChoiceOpeningV1>,
) -> Result<Hash64, String> {
    let committed = state_root_of_pinned_header_v1(child, pinned_child).map_err(|e| e.to_string())?;
    if kaspa_consensus_core::hashing::header::hash(parent) != pinned_selected_parent
        || !child.parents_by_level.get(0).is_some_and(|parents| parents.contains(&pinned_selected_parent))
        || parent.daa_score > child.daa_score
    {
        return Err("native32 pinned child/independent selected-parent header relation differs".into());
    }
    let inner = if parameters.palw_fork_choice_commitment_active_at(parent.daa_score) {
        // Op203's current public opening is V1. Never parse a future V2 leaf
        // as V1 or grant a raw-root fallback through an active envelope.
        if parameters.palw_fork_choice_rule_e_active_at(parent.daa_score) {
            return Err("native32 requires the explicit rule-E V2 opening verifier at this fence".into());
        }
        let opening = fork_opening.ok_or("native32 active fork-choice envelope opening missing")?;
        opening
            .verify(
                &committed,
                &PalwForkChoicePointV1 { block: pinned_selected_parent, daa_score: parent.daa_score, blue_score: parent.blue_score },
                parameters.palw_fork_choice_commitment_v1,
            )
            .map_err(|e| e.to_string())?;
        opening.inner_root
    } else {
        if fork_opening.is_some() {
            return Err("native32 unexpected envelope below original fence".into());
        }
        committed
    };
    Ok(inner)
}
/// Build the private native view directly from authenticated original rows,
/// then compare the untrusted optional RPC DTO in full. Raw Borsh decode
/// cannot mint either the Core checked view or this SDK anchored view.
pub fn derive_native_execution_audit_v32<'a>(
    child: &Header,
    pinned_child: Hash64,
    parent: &Header,
    pinned_selected_parent: Hash64,
    parameters: &Params,
    fork_opening: Option<&PalwForkChoiceOpeningV1>,
    auxiliary: &'a PalwFactProofV1,
    payouts: &'a PalwFactProofV1,
    job: Digest,
    supplied: Option<&kaspa_consensus_core::palw_native_audit_v27::NativeExecutionAuditRecordV27>,
) -> Result<AnchoredNativeExecutionAuditV32<'a>, String> {
    use kaspa_consensus_core::palw_native_audit_proof_v32::{
        derive_native_execution_audit_from_proofs_v32, verify_native_payout_rows_v32,
    };
    let inner =
        native_inner_state_of_pinned_headers_v32(child, pinned_child, parent, pinned_selected_parent, parameters, fork_opening)?;
    let aux = verify_native_aux_v32(auxiliary, inner)?;
    let payouts_checked = verify_native_payout_rows_v32(payouts, inner)?;
    let native = derive_native_execution_audit_from_proofs_v32(&aux, &payouts_checked, parameters, parent.daa_score, job)?;
    if supplied.is_some_and(|record| record != native.record()) {
        return Err("native32 remote DTO differs from independently rederived original rows".into());
    }
    verify_native_execution_audit_v32(
        child,
        pinned_child,
        parent,
        pinned_selected_parent,
        parameters,
        fork_opening,
        auxiliary,
        payouts,
        &native,
    )
}
