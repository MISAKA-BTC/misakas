//! Explicit mixed-court seal identity. Frozen v6 seals never authenticate v7.
use crate::hash::{Digest, id, object_id};
use borsh::{BorshDeserialize, BorshSerialize};
pub const FLAT_SEAL_MAX_PROOF_BYTES_V7: usize = 1 << 20;
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FlatSealRowV7 {
    pub version: u16,
    pub vector: Digest,
    pub actor: Digest,
    pub seal: Digest,
    pub posted_daa: u64,
    pub mature_daa: u64,
    pub expires_daa: u64,
    pub spent: bool,
}
pub fn flat_proof_digest_v7(proof: &[u8]) -> Result<Digest, String> {
    if proof.len() > FLAT_SEAL_MAX_PROOF_BYTES_V7 {
        return Err("mixed-court proof exceeds seal carrier".into());
    }
    Ok(id(b"misaka-palw/kernel/flat-proof-digest/v7", proof))
}
pub fn flat_proof_seal_v7(vector: &Digest, actor: &Digest, salt: &Digest, proof: &[u8]) -> Result<Digest, String> {
    Ok(object_id(b"misaka-palw/kernel/flat-proof-seal/v7", &(*vector, *actor, *salt, flat_proof_digest_v7(proof)?)))
}
pub fn check_flat_seal_v7(
    row: &FlatSealRowV7,
    vector: &Digest,
    actor: &Digest,
    salt: &Digest,
    proof: &[u8],
    now: u64,
) -> Result<Digest, String> {
    if row.version != 7 || row.vector != *vector || row.actor != *actor || row.spent || now < row.mature_daa || now >= row.expires_daa
    {
        return Err("mixed-court seal actor, identity or clock is not open".into());
    }
    let digest = flat_proof_digest_v7(proof)?;
    if flat_proof_seal_v7(vector, actor, salt, proof)? != row.seal {
        return Err("mixed-court seal does not bind these exact bytes".into());
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_seal7_binds_actor_exact_proof_and_namespace_before_court() {
        let (v, a, s) = ([1; 64], [2; 64], [3; 64]);
        let row = FlatSealRowV7 {
            version: 7,
            vector: v,
            actor: a,
            seal: flat_proof_seal_v7(&v, &a, &s, b"proof").unwrap(),
            posted_daa: 10,
            mature_daa: 13,
            expires_daa: 110,
            spent: false,
        };
        assert!(check_flat_seal_v7(&row, &v, &a, &s, b"proof", 13).is_ok());
        assert_ne!(row.seal, crate::flat_seal6::flat_proof_seal_v6(&v, &a, &s, b"proof").unwrap());
        assert!(check_flat_seal_v7(&row, &v, &[4; 64], &s, b"proof", 13).is_err());
        assert!(check_flat_seal_v7(&row, &v, &a, &s, b"proof", 12).is_err());
        assert!(check_flat_seal_v7(&row, &v, &a, &s, b"proof", 110).is_err());
        assert!(check_flat_seal_v7(&row, &v, &a, &s, b"proof\0", 13).is_err());
        assert!(check_flat_seal_v7(&FlatSealRowV7 { spent: true, ..row }, &v, &a, &s, b"proof", 13).is_err());
    }
}
