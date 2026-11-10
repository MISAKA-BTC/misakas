//! Fee-only TKR1 rehearsal at a candidate's authenticated parent.
//!
//! Open resolves its identity and opener from the original parent and tests
//! only the TKR1 book, session and winner at the prefix. Finish can change
//! those rows. Its current container and completed EOF are immutable, and
//! filling a semantics prefix relation requires a legacy shape-profile ID.
//! An accepted TIR class has a different class-ID hash domain. This relies
//! on normal hash identity assumptions and the authenticated state root,
//! including for a restored parent. No ordinary writer owns the TKR1 rows.
//! Part can depend on an ordinary bond update, but its slot strictly precedes
//! Finish's DAA and Finish requires the original parent's Parts. Its result
//! cannot change another carrier's registration burn in this candidate.
//! The authoritative fold still evaluates all carriers in its full order.

use super::{PalwLocalTransitionFaultV1, VirtualStateProcessor};
use kaspa_consensus_core::{
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, reserve_native_pending_requests_v1},
    palw_state_v2::{
        PalwBlockContextV2, PalwChainStateV2, PalwStateParamsV2, PalwStateV2Error, PalwTransitionExtrasV1,
        TokenizerRegistrationFeeWorkingStateV1,
    },
    palw_tokenizer_finish_derivation_cache_v1::TokenizerFinishDerivationCacheV1,
    palw_tokenizer_registration_carriage_v1::{
        SharedValidatedTokenizerRegistrationCarrierV1, TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1, TokenizerRegistrationCarrierContextV1,
        TokenizerRegistrationFeeV1, ValidatedTokenizerRegistrationCarrierV1,
    },
    palw_tokenizer_registration_v1::TokenizerRegistrationPolicyV1,
    subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
    tx::{Transaction, TransactionIndexType, ValidatedTransaction},
};
use kaspa_hashes::Hash64;

pub(in crate::pipeline::virtual_processor) struct TokenizerRegistrationFeePrefixV1<'parent> {
    parent: &'parent PalwChainStateV2,
    params: &'parent PalwStateParamsV2,
    context: TokenizerRegistrationCarrierContextV1,
    policy: TokenizerRegistrationPolicyV1,
    extras: PalwTransitionExtrasV1,
    state: TokenizerRegistrationFeeWorkingStateV1,
    cache: TokenizerFinishDerivationCacheV1<'parent>,
}

pub(in crate::pipeline::virtual_processor) struct TokenizerRegistrationBodyCarrierV1 {
    pub handoff: SharedValidatedTokenizerRegistrationCarrierV1,
    pub fee: TokenizerRegistrationFeeV1,
}

pub(in crate::pipeline::virtual_processor) struct TokenizerRegistrationBodyCarriersV1 {
    entries: Vec<TokenizerRegistrationBodyCarrierV1>,
    _lease: Option<NativeDemandLeaseV1>,
}

impl TokenizerRegistrationBodyCarriersV1 {
    pub(in crate::pipeline::virtual_processor) fn carriers_v1(&self) -> &[TokenizerRegistrationBodyCarrierV1] {
        &self.entries
    }
}

pub(in crate::pipeline::virtual_processor) struct TokenizerRegistrationBodyTrialV1<'prefix, 'parent> {
    prefix: &'prefix mut TokenizerRegistrationFeePrefixV1<'parent>,
    carriers: TokenizerRegistrationBodyCarriersV1,
}

impl TokenizerRegistrationBodyTrialV1<'_, '_> {
    pub(in crate::pipeline::virtual_processor) fn carriers_v1(&self) -> &[TokenizerRegistrationBodyCarrierV1] {
        self.carriers.carriers_v1()
    }

    /// Commit only after the complete UTXO body passes its acceptance checks.
    /// An abandoned trial is rolled back before the next body starts. Its
    /// returned handoffs and fees are never published by dropping it.
    pub(in crate::pipeline::virtual_processor) fn commit(self) -> TokenizerRegistrationBodyCarriersV1 {
        self.prefix.state.commit_body_v1();
        self.carriers
    }
}

impl<'parent> TokenizerRegistrationFeePrefixV1<'parent> {
    /// Start once per candidate, using the real branch's selected parent and
    /// already resolved rules. All bodies share the same Finish cache.
    pub(in crate::pipeline::virtual_processor) fn begin(
        processor: &VirtualStateProcessor,
        parent: &'parent PalwChainStateV2,
        params: &'parent PalwStateParamsV2,
        point: PalwBlockContextV2,
        policy: TokenizerRegistrationPolicyV1,
    ) -> Result<Self, PalwLocalTransitionFaultV1> {
        let extras = processor.palw_transition_extras_for(&point).into_legacy_extras();
        let network = extras
            .tokenizer_registration_network_domain_v1
            .filter(|_| extras.tokenizer_registration_policy_v1 == Some(policy))
            .ok_or_else(|| native_fault("tokenizer fee prefix rules", "TKR1 policy or domain differs from the accepting rules"))?;
        let policy_id = policy.id_v1().map_err(|why| native_fault("tokenizer fee prefix policy", &why))?;
        let cache = TokenizerFinishDerivationCacheV1::new(parent, point, &policy)?;
        let context = TokenizerRegistrationCarrierContextV1 {
            parent_root: cache.parent_root_v1(),
            at: point,
            accepted_ordinal: 0,
            network,
            policy: policy_id,
        };
        let state = TokenizerRegistrationFeeWorkingStateV1::new(parent, params, &point, &extras).map_err(fold_fault)?;
        Ok(Self { parent, params, context, policy, extras, state, cache })
    }

    pub(in crate::pipeline::virtual_processor) fn restart_v1(&mut self) -> Result<(), PalwLocalTransitionFaultV1> {
        self.state = TokenizerRegistrationFeeWorkingStateV1::new(self.parent, self.params, &self.context.at, &self.extras)
            .map_err(fold_fault)?;
        Ok(())
    }

    pub(in crate::pipeline::virtual_processor) fn parent_root_v1(&self) -> Hash64 {
        self.context.parent_root
    }

    pub(in crate::pipeline::virtual_processor) fn scope_v1<T>(&self, run: impl FnOnce() -> T) -> T {
        self.cache.scope_v1(run)
    }

    /// The body slice contains all UTXO-validated transactions in accepted
    /// order, even those that carry no TKR1. The accompanying index is the
    /// position in the physical body and does not define accepted ordinal.
    pub(in crate::pipeline::virtual_processor) fn trial_body(
        &mut self,
        transactions: &[(ValidatedTransaction<'_>, TransactionIndexType)],
        first_ordinal: usize,
        mut preceding_fee: impl FnMut(&ValidatedTransaction<'_>) -> Result<u64, NativeCoverageV1>,
    ) -> Result<TokenizerRegistrationBodyTrialV1<'_, 'parent>, PalwLocalTransitionFaultV1> {
        let count = transactions.iter().filter(|(validated, _)| is_tokenizer_carrier(validated.tx)).count();
        let bytes = count
            .checked_mul(std::mem::size_of::<TokenizerRegistrationBodyCarrierV1>())
            .and_then(|bytes| u64::try_from(bytes).ok())
            .ok_or_else(|| NativeCoverageV1::overflow::<TokenizerRegistrationBodyCarrierV1>("tokenizer fee trial entries"))?;
        let lease = if bytes == 0 {
            None
        } else {
            Some(
                reserve_native_pending_requests_v1(bytes)?
                    .ok_or_else(|| native_fault("tokenizer fee trial entries", "native reservation unavailable"))?,
            )
        };
        let mut entries = Vec::new();
        entries.try_reserve_exact(count).map_err(|_| native_fault("tokenizer fee trial entries", "allocation refused"))?;
        if entries.capacity() != count {
            return Err(native_fault("tokenizer fee trial entries", "allocation exceeds its exact reservation").into());
        }
        if let Some(lease) = &lease {
            lease.materialized_v1();
        }
        self.state.begin_body_v1(self.params, &self.extras).map_err(fold_fault)?;
        for (offset, (validated, _)) in transactions.iter().enumerate() {
            if !is_tokenizer_carrier(validated.tx) {
                continue;
            }
            let ordinal = first_ordinal
                .checked_add(offset)
                .ok_or_else(|| NativeCoverageV1::overflow::<TokenizerRegistrationBodyCarrierV1>("tokenizer fee trial ordinal"))?;
            let context = TokenizerRegistrationCarrierContextV1 { accepted_ordinal: ordinal, ..self.context };
            let preceding = preceding_fee(validated)?;
            let handoff = match ValidatedTokenizerRegistrationCarrierV1::prepare_shared_v1(validated, context, &self.policy, preceding)
            {
                Ok(Some(handoff)) => handoff,
                Ok(None) => continue,
                Err(NativeDecodeErrorV1::Resource(why)) => return Err(why.into()),
                Err(NativeDecodeErrorV1::Decode(why)) => {
                    kaspa_core::debug!("[palw-tokenizer] carrier {} has no fee projection: {why}", validated.tx.id());
                    continue;
                }
            };
            let fee = if let Some(fee) = handoff.part_fee_v1() {
                fee
            } else {
                let (fee, _) = self
                    .cache
                    .scope_v1(|| {
                        self.state.apply_v1(self.parent, context.parent_root, self.params, &context.at, &handoff, &self.extras)
                    })
                    .map_err(fold_fault)?;
                fee
            };
            entries.push(TokenizerRegistrationBodyCarrierV1 { handoff, fee });
        }
        Ok(TokenizerRegistrationBodyTrialV1 { prefix: self, carriers: TokenizerRegistrationBodyCarriersV1 { entries, _lease: lease } })
    }
}

fn is_tokenizer_carrier(tx: &Transaction) -> bool {
    tx.subnetwork_id == SUBNETWORK_ID_PALW_LIFECYCLE && tx.payload.starts_with(TOKENIZER_REGISTRATION_CARRIER_MAGIC_V1)
}

fn native_fault(component: &'static str, why: &str) -> NativeCoverageV1 {
    NativeCoverageV1::unsupported::<TokenizerRegistrationBodyCarrierV1>(component, why)
}

fn fold_fault(why: PalwStateV2Error) -> PalwLocalTransitionFaultV1 {
    match why {
        PalwStateV2Error::NativeResource(why) => PalwLocalTransitionFaultV1::Resource(why),
        PalwStateV2Error::NativeContextUnavailable(why) => PalwLocalTransitionFaultV1::Context(why),
        why => native_fault("tokenizer fee trial fold", &why.to_string()).into(),
    }
}

pub(super) fn with_prefix_scope_v1<T>(prefix: Option<&TokenizerRegistrationFeePrefixV1<'_>>, run: impl FnOnce() -> T) -> T {
    match prefix {
        Some(prefix) => prefix.scope_v1(run),
        None => run(),
    }
}
