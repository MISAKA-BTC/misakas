//! Bounded, actor-bound seal/reveal contract for a future protected flat route.
//!
//! Every reveal needs the authenticated filer's OWN mature seal, including when
//! another encoding proves the same fault. A byte-digest beneficiary lookup alone
//! cannot enforce that rule. Frozen v3 candidate routes do not call this module.
//! A consumer must authenticate `actor`, charge admission/proof work, reserve
//! original capital, prevent a second payout for a vector, and apply the court
//! result atomically. This component neither transfers funds nor certifies those
//! integration, network capacity, inclusion, retention or liveness requirements.
use crate::flat_court5::{FlatConvictionV5, FlatFaultV5, FlatSegContextV5, verify_flat_fault_v5};
use crate::hash::{Digest, id, object_id};
use borsh::BorshSerialize;
use std::collections::BTreeMap;

pub const FLAT_SEAL_MAX_PROOF_BYTES_V6: usize = 1 << 20;
pub const FLAT_SEAL_MAX_ROWS_V6: u32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatSealPolicyV6 {
    pub maturity_daa: u64,
    pub ttl_daa: u64,
    pub max_rows: u32,
    pub max_rows_per_actor: u32,
}
impl FlatSealPolicyV6 {
    pub fn validate(&self) -> Result<(), FlatSealErrorV6> {
        if self.maturity_daa == 0
            || self.ttl_daa <= self.maturity_daa
            || self.max_rows == 0
            || self.max_rows > FLAT_SEAL_MAX_ROWS_V6
            || self.max_rows_per_actor == 0
            || self.max_rows_per_actor > self.max_rows
        {
            return Err(FlatSealErrorV6::Policy);
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FlatSealErrorV6 {
    Policy,
    ClockOverflow,
    OutsideVectorHorizon,
    Capacity,
    ActorCapacity,
    Existing,
    Missing,
    Immature,
    Expired,
    Spent,
    TooLarge,
    SealMismatch,
    NotExpired,
    InvalidCourt(String),
}

/// Bound raw carrier hashing precedes any variable proof/program court decode.
pub fn flat_proof_digest_v6(proof: &[u8]) -> Result<Digest, FlatSealErrorV6> {
    if proof.len() > FLAT_SEAL_MAX_PROOF_BYTES_V6 {
        return Err(FlatSealErrorV6::TooLarge);
    }
    Ok(id(b"misaka-palw/kernel/flat-proof-digest/v6", proof))
}
/// The caller generates a secret unpredictable salt before posting the seal.
/// It is revealed with the proof; actor, vector and exact bytes cannot be rebound.
pub fn flat_proof_seal_v6(vector: &Digest, actor: &Digest, salt: &Digest, proof: &[u8]) -> Result<Digest, FlatSealErrorV6> {
    Ok(seal_of_digest(vector, actor, salt, &flat_proof_digest_v6(proof)?))
}
fn seal_of_digest(vector: &Digest, actor: &Digest, salt: &Digest, digest: &Digest) -> Digest {
    object_id(b"misaka-palw/kernel/flat-proof-seal/v6", &(*vector, *actor, *salt, *digest))
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct FlatSealRowV6 {
    pub version: u16,
    pub vector: Digest,
    pub actor: Digest,
    pub seal: Digest,
    pub posted_daa: u64,
    pub mature_daa: u64,
    pub expires_daa: u64,
    pub spent: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FlatSealedConvictionV6 {
    pub vector: Digest,
    pub actor: Digest,
    pub proof_digest: Digest,
    pub conviction: FlatConvictionV5,
}

/// Fixed-metadata rows only; neither proof bytes nor arbitrary seal history are
/// retained. A slot cannot be replaced before explicit expiry, so repeated seals
/// cannot push out an honest actor's pending reveal or retain unbounded history.
pub struct FlatSealBookV6 {
    policy: FlatSealPolicyV6,
    rows: BTreeMap<(Digest, Digest), FlatSealRowV6>,
    per_actor: BTreeMap<Digest, u32>,
}
impl FlatSealBookV6 {
    pub fn new(policy: FlatSealPolicyV6) -> Result<Self, FlatSealErrorV6> {
        policy.validate()?;
        Ok(Self { policy, rows: BTreeMap::new(), per_actor: BTreeMap::new() })
    }
    pub fn len(&self) -> usize {
        self.rows.len()
    }
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
    pub fn rows_for(&self, actor: &Digest) -> u32 {
        self.per_actor.get(actor).copied().unwrap_or(0)
    }
    pub fn row(&self, vector: &Digest, actor: &Digest) -> Option<&FlatSealRowV6> {
        self.rows.get(&(*vector, *actor))
    }
    /// `actor` must come from signature authentication, never a proof field.
    /// `vector_until` is the consumer's immutable admitted liability horizon.
    pub fn post(&mut self, vector: Digest, actor: Digest, seal: Digest, now: u64, vector_until: u64) -> Result<(), FlatSealErrorV6> {
        let mature = now.checked_add(self.policy.maturity_daa).ok_or(FlatSealErrorV6::ClockOverflow)?;
        let expires = now.checked_add(self.policy.ttl_daa).ok_or(FlatSealErrorV6::ClockOverflow)?.min(vector_until);
        if mature >= expires {
            return Err(FlatSealErrorV6::OutsideVectorHorizon);
        }
        if self.row(&vector, &actor).is_some() {
            return Err(FlatSealErrorV6::Existing);
        }
        if self.len() >= self.policy.max_rows as usize {
            return Err(FlatSealErrorV6::Capacity);
        }
        let count = self.rows_for(&actor);
        if count >= self.policy.max_rows_per_actor {
            return Err(FlatSealErrorV6::ActorCapacity);
        }
        self.rows.insert(
            (vector, actor),
            FlatSealRowV6 { version: 6, vector, actor, seal, posted_daa: now, mature_daa: mature, expires_daa: expires, spent: false },
        );
        self.per_actor.insert(actor, count + 1);
        Ok(())
    }
    /// Explicit constant-key close; no deadline-wide scan. Anyone may request
    /// expiry, but a consumer supplies authoritative DAA and charges its work.
    pub fn expire(&mut self, vector: &Digest, actor: &Digest, now: u64) -> Result<(), FlatSealErrorV6> {
        let row = self.row(vector, actor).ok_or(FlatSealErrorV6::Missing)?;
        if now < row.expires_daa {
            return Err(FlatSealErrorV6::NotExpired);
        }
        self.rows.remove(&(*vector, *actor));
        let count = self.rows_for(actor);
        if count == 1 {
            self.per_actor.remove(actor);
        } else {
            self.per_actor.insert(*actor, count - 1);
        }
        Ok(())
    }
    /// Fixed metadata/time checks and bounded raw digest authentication precede
    /// the terminal court. No third-party sealer search or filer fallback exists.
    /// The callback is deferred so missing/immature/mismatched seals cannot force
    /// an expensive stored-program decode. It must verify the admitted vector's
    /// context, exact resource ceilings and terminal court. It must NOT pay funds.
    pub fn reveal_with(
        &mut self,
        vector: &Digest,
        actor: &Digest,
        salt: &Digest,
        proof: &[u8],
        now: u64,
        court: impl FnOnce(&[u8]) -> Result<FlatConvictionV5, String>,
    ) -> Result<FlatSealedConvictionV6, FlatSealErrorV6> {
        let row = self.row(vector, actor).ok_or(FlatSealErrorV6::Missing)?;
        if row.spent {
            return Err(FlatSealErrorV6::Spent);
        }
        if now < row.mature_daa {
            return Err(FlatSealErrorV6::Immature);
        }
        if now >= row.expires_daa {
            return Err(FlatSealErrorV6::Expired);
        }
        let digest = flat_proof_digest_v6(proof)?;
        if seal_of_digest(vector, actor, salt, &digest) != row.seal {
            return Err(FlatSealErrorV6::SealMismatch);
        }
        let conviction = court(proof).map_err(FlatSealErrorV6::InvalidCourt)?;
        self.rows.get_mut(&(*vector, *actor)).unwrap().spent = true;
        Ok(FlatSealedConvictionV6 { vector: *vector, actor: *actor, proof_digest: digest, conviction })
    }
    /// Convenience for an already decoded/authenticated flat-v5 context. Native
    /// consumers should use `reveal_with` to defer stored context reconstruction.
    pub fn reveal(
        &mut self,
        vector: &Digest,
        actor: &Digest,
        salt: &Digest,
        proof: &[u8],
        now: u64,
        c: &FlatSegContextV5<'_>,
    ) -> Result<FlatSealedConvictionV6, FlatSealErrorV6> {
        self.reveal_with(vector, actor, salt, proof, now, |bytes| {
            let fault = FlatFaultV5::from_bytes(bytes).map_err(|e| e.to_string())?;
            verify_flat_fault_v5(c, &fault).map_err(|e| format!("{e:?}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat_court5::FlatConvictionKindV5;
    fn book() -> FlatSealBookV6 {
        FlatSealBookV6::new(FlatSealPolicyV6 { maturity_daa: 3, ttl_daa: 12, max_rows: 4, max_rows_per_actor: 2 }).unwrap()
    }
    fn yes(_: &[u8]) -> Result<FlatConvictionV5, String> {
        Ok(FlatConvictionV5 { position: 0, occurrence: 0, node: 1, kind: FlatConvictionKindV5::Whole })
    }
    #[test]
    fn flat_seal_v6_binds_every_coordinate_salt_and_exact_carrier() {
        let a = flat_proof_seal_v6(&[1; 64], &[2; 64], &[3; 64], b"a").unwrap();
        for other in [
            flat_proof_seal_v6(&[0; 64], &[2; 64], &[3; 64], b"a").unwrap(),
            flat_proof_seal_v6(&[1; 64], &[0; 64], &[3; 64], b"a").unwrap(),
            flat_proof_seal_v6(&[1; 64], &[2; 64], &[0; 64], b"a").unwrap(),
            flat_proof_seal_v6(&[1; 64], &[2; 64], &[3; 64], b"a\0").unwrap(),
        ] {
            assert_ne!(a, other);
        }
        assert!(flat_proof_digest_v6(&vec![0; FLAT_SEAL_MAX_PROOF_BYTES_V6]).is_ok());
        assert_eq!(flat_proof_digest_v6(&vec![0; FLAT_SEAL_MAX_PROOF_BYTES_V6 + 1]), Err(FlatSealErrorV6::TooLarge));
    }
    #[test]
    fn flat_seal_v6_copies_alternate_bytes_and_immature_seals_never_enter_court() {
        let (v, a, thief, salt) = ([1; 64], [2; 64], [9; 64], [3; 64]);
        let seal = flat_proof_seal_v6(&v, &a, &salt, b"proof").unwrap();
        let mut b = book();
        b.post(v, a, seal, 10, 100).unwrap();
        let no_court = |_: &[u8]| panic!("refused reveal must not decode/verify the court");
        assert_eq!(b.reveal_with(&v, &thief, &salt, b"proof", 13, no_court), Err(FlatSealErrorV6::Missing));
        b.post(v, thief, seal, 10, 100).unwrap();
        assert_eq!(b.reveal_with(&v, &thief, &salt, b"proof", 13, no_court), Err(FlatSealErrorV6::SealMismatch));
        assert_eq!(b.reveal_with(&v, &a, &salt, b"proof", 12, no_court), Err(FlatSealErrorV6::Immature));
        assert_eq!(b.reveal_with(&v, &a, &salt, b"equivalent proof", 13, no_court), Err(FlatSealErrorV6::SealMismatch));
        b.expire(&v, &thief, 22).unwrap();
        let copied = flat_proof_seal_v6(&v, &thief, &salt, b"equivalent proof").unwrap();
        b.post(v, thief, copied, 22, 100).unwrap();
        assert_eq!(b.reveal_with(&v, &thief, &salt, b"equivalent proof", 22, no_court), Err(FlatSealErrorV6::Immature));
    }
    #[test]
    fn flat_seal_v6_reveal_is_one_use_and_invalid_court_retains_the_slot() {
        let (v, a, salt) = ([1; 64], [2; 64], [3; 64]);
        let mut b = book();
        b.post(v, a, flat_proof_seal_v6(&v, &a, &salt, b"proof").unwrap(), 10, 100).unwrap();
        assert_eq!(
            b.reveal_with(&v, &a, &salt, &vec![0; FLAT_SEAL_MAX_PROOF_BYTES_V6 + 1], 13, |_| panic!()),
            Err(FlatSealErrorV6::TooLarge)
        );
        assert_eq!(
            b.reveal_with(&v, &a, &salt, b"proof", 13, |_| Err("NoFault".into())),
            Err(FlatSealErrorV6::InvalidCourt("NoFault".into()))
        );
        assert!(!b.row(&v, &a).unwrap().spent);
        let result = b.reveal_with(&v, &a, &salt, b"proof", 13, yes).unwrap();
        assert_eq!((result.actor, result.vector), (a, v));
        assert_eq!(b.reveal_with(&v, &a, &salt, b"proof", 14, |_| panic!()), Err(FlatSealErrorV6::Spent));
        assert_eq!(b.post(v, a, [0; 64], 14, 100), Err(FlatSealErrorV6::Existing));
    }
    #[test]
    fn flat_seal_v6_fixed_slots_explicit_expiry_and_aggregate_actor_bounds() {
        let mut b = book();
        let a = [2; 64];
        for v in [1, 2] {
            b.post([v; 64], a, [0; 64], 10, 100).unwrap();
        }
        assert_eq!(b.post([3; 64], a, [0; 64], 10, 100), Err(FlatSealErrorV6::ActorCapacity));
        for v in [3, 4] {
            b.post([v; 64], [v; 64], [0; 64], 10, 100).unwrap();
        }
        assert_eq!(b.post([5; 64], [5; 64], [0; 64], 10, 100), Err(FlatSealErrorV6::Capacity));
        assert_eq!(b.expire(&[1; 64], &a, 21), Err(FlatSealErrorV6::NotExpired));
        assert_eq!(b.len(), 4);
        b.expire(&[1; 64], &a, 22).unwrap();
        assert_eq!(b.rows_for(&a), 1);
        b.post([5; 64], [5; 64], [0; 64], 22, 100).unwrap();
        b.expire(&[2; 64], &a, 22).unwrap();
        assert_eq!(b.rows_for(&a), 0);
        assert!(!b.per_actor.contains_key(&a));
        assert_eq!(b.len(), b.per_actor.values().sum::<u32>() as usize);
    }
    #[test]
    fn flat_seal_v6_policy_horizon_and_overflow_refuse_without_insertion() {
        let p = FlatSealPolicyV6 { maturity_daa: 3, ttl_daa: 12, max_rows: 4, max_rows_per_actor: 2 };
        for q in [
            FlatSealPolicyV6 { maturity_daa: 0, ..p },
            FlatSealPolicyV6 { ttl_daa: 3, ..p },
            FlatSealPolicyV6 { max_rows: 0, ..p },
            FlatSealPolicyV6 { max_rows: FLAT_SEAL_MAX_ROWS_V6 + 1, ..p },
            FlatSealPolicyV6 { max_rows_per_actor: 0, ..p },
            FlatSealPolicyV6 { max_rows_per_actor: 5, ..p },
        ] {
            assert!(FlatSealBookV6::new(q).is_err());
        }
        let mut b = book();
        assert_eq!(b.post([1; 64], [2; 64], [3; 64], u64::MAX - 1, u64::MAX), Err(FlatSealErrorV6::ClockOverflow));
        assert_eq!(b.post([1; 64], [2; 64], [3; 64], 10, 13), Err(FlatSealErrorV6::OutsideVectorHorizon));
        assert!(b.is_empty());
        b.post([1; 64], [2; 64], [3; 64], 10, 15).unwrap();
        assert_eq!(b.row(&[1; 64], &[2; 64]).unwrap().expires_daa, 15);
        assert_eq!(b.reveal_with(&[1; 64], &[2; 64], &[0; 64], b"", 15, |_| panic!()), Err(FlatSealErrorV6::Expired));
    }
}
