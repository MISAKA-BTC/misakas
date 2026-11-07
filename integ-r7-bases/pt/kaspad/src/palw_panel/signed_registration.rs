//! The class worker retains its raw declaration for retry/inclusion tracking,
//! and envelopes the actual submitted copy only when the chain requires it.
use super::{CLASS_REGISTRATION_RETRY_DAA, PalwPanelService};
use kaspa_consensus_core::{
    config::params::Params,
    palw_state_v2::{PalwConsensusObjectV2, palw_class_registration_buyer_v1},
    tx::{Transaction, TransactionOutpoint, UtxoEntry},
};
use misaka_palw_sdk::onboarding_chain::{EnvelopeSigner, SignedRegistrationRequestV1};

fn for_submission(
    params: &Params,
    registration: &PalwConsensusObjectV2,
    daa: u64,
    signer: &dyn EnvelopeSigner,
) -> Result<PalwConsensusObjectV2, String> {
    if !params.palw_signed_registration_v1.is_some_and(|fence| fence.is_active(daa)) {
        return Ok(registration.clone());
    }
    let bond = palw_class_registration_buyer_v1(registration)
        .ok_or("the class worker must submit a bought registration of an actual bond")?;
    let deadline = daa.checked_add(CLASS_REGISTRATION_RETRY_DAA).ok_or("signed class registration retry deadline overflow")?;
    SignedRegistrationRequestV1::for_network(registration.clone(), bond, daa, deadline, params)
        .and_then(|request| request.sign(signer))
        .map_err(|e| e.to_string())
}

struct PanelSigner<'a>(&'a PalwPanelService);
impl EnvelopeSigner for PanelSigner<'_> {
    fn public_key(&self) -> Vec<u8> {
        self.0.keypair.as_ref().map(|kp| kp.verification_key.as_ref().to_vec()).unwrap_or_default()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        self.0.sign(message, context).ok_or("the class worker holds no usable bond signer".into())
    }
}

impl PalwPanelService {
    pub(super) fn build_class_registration_tx(
        &self,
        registration: &PalwConsensusObjectV2,
        funding_outpoint: TransactionOutpoint,
        funding: &UtxoEntry,
        daa: u64,
    ) -> Result<Transaction, String> {
        let submitted = for_submission(&self.consensus_config.params, registration, daa, &PanelSigner(self))?;
        self.build_lifecycle_tx(&submitted, funding_outpoint, funding)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::{
        config::params::{ForkActivation, SIMNET_PARAMS},
        palw_state_v2::PalwBondKeyV2,
        palw_tir_admission_v1::palw_tir_post_genesis_registration_v1,
        palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1},
        palw_tir_class_v1::{PalwTirClassV1, PalwTirLayoutV1},
    };
    use kaspa_hashes::Hash64;
    use kaspa_pq_validator_core::ValidatorKey;
    use std::cell::Cell;

    struct Signer {
        key: ValidatorKey,
        calls: Cell<u32>,
    }
    impl EnvelopeSigner for Signer {
        fn public_key(&self) -> Vec<u8> {
            self.key.public_key().to_vec()
        }
        fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.key.sign_with_context(message, context).to_vec())
        }
    }
    fn fixture() -> (PalwConsensusObjectV2, Signer) {
        // Declaration/signature control only; this is not native admission or
        // a trained-model fixture. The node's separate admission still applies.
        let fx = misaka_palw_tir_sketch::fixture::wide128_v1(37);
        let class = PalwTirClassV1 {
            version: 1,
            program: fx.program.encode(),
            tokenizer_id: Hash64::default(),
            layout: PalwTirLayoutV1 {
                version: 1,
                max_context: 64,
                checkpoint_interval: 2,
                h_tile: 2,
                commit_tiles: fx.program.blocks.iter().flat_map(|b| &b.nodes).filter(|n| n.commit).map(|_| 4).collect(),
                state_tiles: vec![],
            },
        };
        let root = Hash64::from_u64_word(201);
        let facts = PalwTirJobFactsV1::of(&class, &fx.program, class.class_id(&root));
        let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&class).unwrap());
        let bond = PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(202), 0));
        let object = palw_tir_post_genesis_registration_v1(class, canonical, root, 0, 1, 1, 0, bond, vec![], u64::MAX).unwrap();
        (object, Signer { key: ValidatorKey::from_seed([47; 32]), calls: Cell::new(0) })
    }

    #[test]
    fn signed_worker_submission_preserves_declaration_and_binds_current_fork() {
        let (raw, signer) = fixture();
        let mut params = SIMNET_PARAMS;
        params.palw_signed_registration_v1 = Some(ForkActivation::new(5));
        assert_eq!(for_submission(&params, &raw, 4, &signer).unwrap(), raw);
        assert_eq!(signer.calls.get(), 0);
        let envelope = for_submission(&params, &raw, 5, &signer).unwrap();
        let PalwConsensusObjectV2::SignedRegistrationV1 { registration, valid_from_daa, valid_until_daa, fork_digest, .. } = &envelope
        else {
            panic!("actual signed envelope required")
        };
        assert_eq!(registration.as_ref(), &raw);
        assert_eq!((*valid_from_daa, *valid_until_daa), (5, 5 + CLASS_REGISTRATION_RETRY_DAA));
        assert_eq!(*fork_digest, misaka_palw_sdk::onboarding_chain::envelope_fork_digest_v1(&params, 5));
        let domain = misaka_palw_sdk::onboarding_chain::network_domain_v1(&params);
        misaka_palw_sdk::onboarding_chain::verify_signed_registration_v1(&envelope, domain, &signer.public_key()).unwrap();
        let mut substituted = envelope.clone();
        let PalwConsensusObjectV2::SignedRegistrationV1 { valid_until_daa, .. } = &mut substituted else { unreachable!() };
        *valid_until_daa += 1;
        assert!(misaka_palw_sdk::onboarding_chain::verify_signed_registration_v1(&substituted, domain, &signer.public_key()).is_err());
        params.palw_signed_registration_v1 = None;
        assert_eq!(for_submission(&params, &raw, u64::MAX, &signer).unwrap(), raw);
        assert_eq!(signer.calls.get(), 1);
    }

    #[test]
    fn signed_worker_overflow_refuses_before_signing() {
        let (raw, signer) = fixture();
        let mut params = SIMNET_PARAMS;
        params.palw_signed_registration_v1 = Some(ForkActivation::new(0));
        assert!(for_submission(&params, &raw, u64::MAX - 1, &signer).unwrap_err().contains("deadline overflow"));
        assert_eq!(signer.calls.get(), 0);
    }
}
