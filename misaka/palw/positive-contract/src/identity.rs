use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct Digest(pub [u8; 64]);

pub(crate) fn hash<T: BorshSerialize>(domain: &[u8], value: &T) -> Digest {
    Digest(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(domain)
            .hash(&borsh::to_vec(value).expect("fixed contract serialization"))
            .as_bytes()
            .try_into()
            .expect("64"),
    )
}

macro_rules! identifier {
    ($($name:ident),*) => {$(
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
        pub struct $name(pub Digest);
    )*};
}
identifier!(ClaimId, CaseId, ResultId, WitnessId, RequestId, PrincipalId, PlanId);

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ChainScope {
    pub network: Digest,
    pub ruleset: Digest,
    pub branch: Digest,
}

pub const PUBKEY_BYTES: usize = 2592;
pub const SIGNATURE_BYTES: usize = 4627;
pub const RESULT_SIGN_CONTEXT: &[u8] = b"misaka/positive/result/v2/r1";
pub const WITNESS_SIGN_CONTEXT: &[u8] = b"misaka/positive/witness/v2/r1";
pub const REQUEST_SIGN_CONTEXT: &[u8] = b"misaka/positive/request/v2/r1";

pub fn principal(key: &[u8; PUBKEY_BYTES]) -> PrincipalId {
    // Existing MISAKA ML-DSA-87 key identity: unkeyed BLAKE2b-512.
    PrincipalId(Digest(blake2b_simd::Params::new().hash_length(64).hash(key).as_bytes().try_into().expect("64")))
}

/// Actor authentication only; never a computation validity certificate.
pub fn verify_signature(key: &[u8; PUBKEY_BYTES], id: Digest, signature: &[u8; SIGNATURE_BYTES], context: &[u8]) -> bool {
    use libcrux_ml_dsa::ml_dsa_87::{MLDSA87Signature, MLDSA87VerificationKey, portable};
    portable::verify(&MLDSA87VerificationKey::new(*key), &id.0, context, &MLDSA87Signature::new(*signature)).is_ok()
}
