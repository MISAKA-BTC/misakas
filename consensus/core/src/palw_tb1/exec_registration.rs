//! Model-free reader for the existing private EXEC registration preimage.
//! Branch membership, tokenizer evaluation and mining admissibility are
//! checked by the owning protocol after reading the signed fields.
use crate::Hash64;
use crate::palw_model_market_v1::moe2::settlement::{Moe2ExecSlot, Moe2PreDrawQuote};
use libcrux_ml_dsa::{
    ml_dsa_65::{self, MLDSA65Signature, MLDSA65VerificationKey},
    ml_dsa_87::{self, MLDSA87Signature, MLDSA87VerificationKey},
};
use sha3::{Digest, Sha3_512};
use std::collections::{BTreeMap, BTreeSet};

pub const REGISTRATION_DOMAIN: &[u8] = b"MISAKA/private/FOW1/input-owner/v2";
pub const QF1_LOCAL_CONTEXT_DOMAIN: &[u8] = b"MISAKA/private/FOW1/QF1-Q3-8B-A16-8K-v1/local-inference/context/v1";
pub const TIR_CONTEXT_DOMAIN: &[u8] = b"MISAKA/private/FOW1/TIR-input/012-byte3/v2";
pub const MOE2_QUOTE_DOMAIN: &[u8] = b"MISAKA/private/FOW1/MOE2/pre-draw-quote/v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecRegistrationProfile {
    TirByte3,
    Qf1Local { descriptor: Hash64, source: Hash64 },
}

/// Immutable fields authenticated by the EXEC owner. The caller still
/// establishes the provenance of hashes, positions and registered model data.
pub struct ExecRegistration<'a> {
    pub key: [u8; 64],
    pub profile: ExecRegistrationProfile,
    pub preparation: Hash64,
    pub qualified_rounds: Hash64,
    pub registration_anchor: Hash64,
    pub candidate_set: Hash64,
    pub beneficiary: Hash64,
    pub model_root: Hash64,
    pub class_id: Hash64,
    pub tokenizer: Hash64,
    pub job: Hash64,
    pub policy: Hash64,
    pub difficulty: Hash64,
    pub accounting: Hash64,
    pub owner: &'a [u8; 1952],
    pub nonce: [u8; 32],
    pub target: [u8; 33],
    pub job_tokens: u32,
    pub vocab: u32,
    pub max_positions: u32,
    pub decode_tokens: u32,
    pub max_leaves: u64,
    pub open_daa: u64,
    pub deadline_exclusive_daa: u64,
    pub quote: Option<Moe2PreDrawQuote>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecRegistrationError {
    Encoding,
    Terms,
    OwnerSignature,
    FundingReference,
    FunderKey,
    FunderSignature,
    Count,
    Context,
    Duplicate,
    InputSetRoot,
}

fn take<'a, const N: usize>(bytes: &mut &'a [u8]) -> Result<&'a [u8; N], ExecRegistrationError> {
    let (field, rest) = bytes.split_first_chunk::<N>().ok_or(ExecRegistrationError::Encoding)?;
    *bytes = rest;
    Ok(field)
}

impl<'a> ExecRegistration<'a> {
    fn decode(mut bytes: &'a [u8], key: [u8; 64]) -> Result<Self, ExecRegistrationError> {
        use ExecRegistrationError::{Encoding, Terms};
        let profile = if let Some(rest) = bytes.strip_prefix(QF1_LOCAL_CONTEXT_DOMAIN) {
            bytes = rest;
            ExecRegistrationProfile::Qf1Local {
                descriptor: Hash64::from_bytes(*take(&mut bytes)?),
                source: Hash64::from_bytes(*take(&mut bytes)?),
            }
        } else {
            bytes = bytes.strip_prefix(TIR_CONTEXT_DOMAIN).ok_or(Encoding)?;
            ExecRegistrationProfile::TirByte3
        };
        let mut hash = || take(&mut bytes).map(|value| Hash64::from_bytes(*value));
        let preparation = hash()?;
        let qualified_rounds = hash()?;
        let registration_anchor = hash()?;
        let candidate_set = hash()?;
        let beneficiary = hash()?;
        let model_root = hash()?;
        let class_id = hash()?;
        let tokenizer = hash()?;
        let job = hash()?;
        let policy = hash()?;
        let difficulty = hash()?;
        let accounting = hash()?;
        let owner = take(&mut bytes)?;
        let nonce = *take(&mut bytes)?;
        let target = *take::<33>(&mut bytes)?;
        let job_tokens = u32::from_be_bytes(*take(&mut bytes)?);
        let vocab = u32::from_be_bytes(*take(&mut bytes)?);
        let max_positions = u32::from_be_bytes(*take(&mut bytes)?);
        let decode_tokens = u32::from_be_bytes(*take(&mut bytes)?);
        let max_leaves = u64::from_be_bytes(*take(&mut bytes)?);
        let open_daa = u64::from_be_bytes(*take(&mut bytes)?);
        let deadline_exclusive_daa = u64::from_be_bytes(*take(&mut bytes)?);
        let quote = if bytes.is_empty() {
            None
        } else {
            let bytes = bytes.strip_prefix(MOE2_QUOTE_DOMAIN).ok_or(Encoding)?;
            let quote = borsh::from_slice::<Moe2PreDrawQuote>(bytes).map_err(|_| Encoding)?;
            if quote.model != class_id
                || quote.artifact_root != model_root
                || quote.deadline == u64::MAX
                || quote.deadline < quote.valid_from
                || quote.price_limits.max_output_slippage_bps > 10_000
                || quote.limits.min_out > quote.limits.max_purchase_units
            {
                return Err(Terms);
            }
            Some(quote)
        };
        if target[0] != 0 && (target[0] != 1 || target[1..].iter().any(|&part| part != 0))
            || vocab < 259
            || max_positions == 0
            || decode_tokens == 0
            || max_leaves == 0
            || open_daa >= deadline_exclusive_daa
        {
            return Err(Terms);
        }
        match profile {
            ExecRegistrationProfile::TirByte3
                if job_tokens == 0
                    || job_tokens.checked_add(34).and_then(|n| n.checked_add(decode_tokens)).is_none_or(|n| n > max_positions) =>
            {
                return Err(Terms);
            }
            ExecRegistrationProfile::Qf1Local { .. }
                if vocab != crate::palw_qf1::VOCAB
                    || max_positions != crate::palw_qf1::CONTEXT
                    || decode_tokens > crate::palw_qf1::MAX_NEW =>
            {
                return Err(Terms);
            }
            _ => {}
        }
        Ok(Self {
            key,
            profile,
            preparation,
            qualified_rounds,
            registration_anchor,
            candidate_set,
            beneficiary,
            model_root,
            class_id,
            tokenizer,
            job,
            policy,
            difficulty,
            accounting,
            owner,
            nonce,
            target,
            job_tokens,
            vocab,
            max_positions,
            decode_tokens,
            max_leaves,
            open_daa,
            deadline_exclusive_daa,
            quote,
        })
    }

    /// Read exactly one preimage and authenticate it. Neither of the existing
    /// private context domains supplies authority for a mining reservation.
    pub fn verify(bytes: &'a [u8], signature: &MLDSA65Signature) -> Result<(Self, [u8; 64]), ExecRegistrationError> {
        let key = registration_key(bytes);
        let row = Self::decode(bytes, key)?;
        ml_dsa_65::verify(&MLDSA65VerificationKey::new(*row.owner), &key, REGISTRATION_DOMAIN, signature)
            .map_err(|_| ExecRegistrationError::OwnerSignature)?;
        Ok((row, key))
    }
}

pub fn registration_key(bytes: &[u8]) -> [u8; 64] {
    Sha3_512::new().chain_update(b"MISAKA/private/FOW1/reg-key/v1").chain_update(bytes).finalize().into()
}

pub fn verify_quote_consent(
    key: [u8; 64],
    slot: u64,
    funding_ref: [u8; 64],
    registered_funder: &[u8],
    signature: &MLDSA87Signature,
) -> Result<(), ExecRegistrationError> {
    let expected = Moe2ExecSlot::consent_ref(key, slot);
    if expected != funding_ref {
        return Err(ExecRegistrationError::FundingReference);
    }
    let funder = MLDSA87VerificationKey::new(registered_funder.try_into().map_err(|_| ExecRegistrationError::FunderKey)?);
    ml_dsa_87::verify(&funder, &expected, Moe2ExecSlot::CONSENT_DOMAIN, signature).map_err(|_| ExecRegistrationError::FunderSignature)
}

/// Commit to the complete verified allocation sequence in numerical slot order.
pub fn input_set_root(
    preparation: Hash64,
    qualified_rounds: [u8; 64],
    allocations: &BTreeMap<u64, ([u8; 64], [u8; 64])>,
) -> Result<[u8; 64], ExecRegistrationError> {
    let count = u32::try_from(allocations.len()).map_err(|_| ExecRegistrationError::Count)?;
    let mut h = Sha3_512::new()
        .chain_update(b"MISAKA/private/FOW1/exec-input-set/v2")
        .chain_update(qualified_rounds)
        .chain_update(preparation.as_bytes())
        .chain_update(count.to_be_bytes());
    for (&slot, (key, funding)) in allocations {
        h.update(slot.to_be_bytes());
        h.update(key);
        h.update(funding);
    }
    Ok(h.finalize().into())
}

/// Preimages supplied for an existing EXEC close commitment. A block reader
/// must bound the outer carrier before borrowing these fixed-format records.
pub struct SignedExecRegistration<'a> {
    pub context: &'a [u8],
    pub owner_signature: &'a MLDSA65Signature,
    pub slot: u64,
    pub funding_ref: [u8; 64],
    pub quote_consent: Option<&'a MLDSA87Signature>,
}

pub struct ExecInputSetBinding {
    pub preparation: Hash64,
    pub qualified_rounds: Hash64,
    pub consumer_set_root: [u8; 64],
}

impl ExecInputSetBinding {
    /// Reconstruct the committed set from signed preimages without consulting
    /// a model or the current tip. Obtain funder keys from the authenticated
    /// preparation state. Other branch, timing and eligibility rules still
    /// apply before the owning block fold can reserve any of these slots.
    pub fn verify<'a, 'state>(
        &self,
        witnesses: &[SignedExecRegistration<'a>],
        max_records: usize,
        mut funder_at_preparation: impl FnMut(&Moe2PreDrawQuote) -> Option<&'state [u8]>,
    ) -> Result<BTreeMap<u64, ExecRegistration<'a>>, ExecRegistrationError> {
        use ExecRegistrationError::*;
        if witnesses.len() > max_records {
            return Err(Count);
        }
        let mut rows = BTreeMap::new();
        let mut allocations = BTreeMap::new();
        let mut keys = BTreeSet::new();
        for witness in witnesses {
            let (row, key) = ExecRegistration::verify(witness.context, witness.owner_signature)?;
            if row.preparation != self.preparation || row.qualified_rounds != self.qualified_rounds {
                return Err(Context);
            }
            if !keys.insert(key) || allocations.insert(witness.slot, (key, witness.funding_ref)).is_some() {
                return Err(Duplicate);
            }
            match (&row.quote, witness.quote_consent) {
                (Some(quote), Some(consent)) => {
                    let funder = funder_at_preparation(quote).ok_or(FunderKey)?;
                    verify_quote_consent(key, witness.slot, witness.funding_ref, funder, consent)?;
                }
                (None, None) => {}
                _ => return Err(FundingReference),
            }
            rows.insert(witness.slot, row);
        }
        if input_set_root(self.preparation, self.qualified_rounds.as_bytes(), &allocations)? != self.consumer_set_root {
            return Err(InputSetRoot);
        }
        Ok(rows)
    }
}
