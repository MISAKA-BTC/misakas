//! **RFC-0003 §I.4.7, decision 22 (G24): the held leaf challenge** — one generic object, for IR and pipeline
//! classes alike, behind one dormant fence `Params::palw_held_close_chunks_v1`. Spec 04b §15.15.6 is
//! normative.
//!
//! A chain that plays no bisection (the held regime, ADR-0103; testnet-12) convicts only in ONE MOVE: an
//! accusation carries the whole close in one carrier (about 100,000 bytes), so a lie at a leaf whose close
//! exceeds one carrier is refused its licence and never slashed. The court already has the machinery that
//! carries a close up to the carried cap (`CourtCloseDeclared` / `CourtCloseChunk`, up to 32 × 100,000 B):
//! a close GROUP, keyed to a session, with a deposit the declarer's bond backs, an assembly clock and a
//! verdict applied through the one `CourtClosed` arm. What the held regime lacks is a session to key it to —
//! it opens one only by naming a DISSECTED leaf (ADR-0103 Decision 5). So this object generalises that
//! entrance to any leaf, and carries the declaration of the close in the same breath:
//!
//! * the fold opens a session at `Terminal` on the named leaf exactly as Decision 5 does (the session id
//!   derives from the claim, its roots, the two parties and the space — never the leaf — so it is keyed to
//!   (claim, accuser bond) by construction);
//! * and writes the CHALLENGER-SIDE close group the existing declaration would have written, `declarer = the
//!   accuser's bond`, the pinned digests, the deposit `palw_close_assembly_deposit_v1(count)` and the assembly
//!   deadline `daa + 4 · count`;
//! * the close then rides the court's own chunks and is adjudicated at the narrowed leaf by the court's own
//!   function; a conviction applies through the `CourtClosed` arm; a close that never comes (or comes and
//!   does not convict) convicts its DECLARER, the accuser, and leaves the claim alone.
//!
//! The executor is not put on a clock for a close the accuser has pinned and may never deliver: while the
//! challenger's declared close stands at an IR or pipeline terminal the session waits at `Terminal`
//! (`court_session_challenger_declared_at_the_ir_terminal_v1`), the mirror of the clause that makes the
//! executor's own declared close its terminal move.
//!
//! Below the height the acceptance walk drops the object by name (an older build cannot decode it, A-2) and
//! the fold refuses it as the second lock; nothing else in the court moves. Tag 62's and tag 88's semantics
//! are untouched.

use crate::Hash64;
use crate::config::params::{ForkActivation, Params, PalwPostLaunchFenceV1};
use crate::palw_mode_v2::{PalwConsensusMode, PalwCourtParamsV2, PalwModeV2Error};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2};

pub const PALW_HELD_LEAF_CHALLENGE_VERSION_V1: u16 = 1;
/// The key of [`palw_held_leaf_challenge_digest_v1`].
pub const PALW_HELD_LEAF_CHALLENGE_DOMAIN_V1: &[u8] = b"misaka-palw/held-close/challenge/v1";
/// The ML-DSA-87 context the accuser signs the challenge digest under.
pub const PALW_HELD_LEAF_CHALLENGE_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/held-close/challenge/mldsa87/v1";
/// The most chunks a held leaf challenge may declare: the court's structural bound
/// ([`crate::palw_state_v2::PALW_COURT_CLOSE_MAX_CHUNKS`], what its `u64` bitmap addresses), further bounded
/// by the ruleset's own `max_close_chunks` at acceptance.
pub const PALW_HELD_LEAF_CHALLENGE_MAX_CHUNKS_V1: u8 = crate::palw_state_v2::PALW_COURT_CLOSE_MAX_CHUNKS;

/// **A named-leaf challenge with the declaration of its close.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwHeldLeafChallengeV1 {
    /// [`PALW_HELD_LEAF_CHALLENGE_VERSION_V1`].
    pub version: u16,
    /// The claim challenged, and its committed roots as the accuser read them off the chain.
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    /// The claim's bond — named so the object says whom it prosecutes, and refused if not the claim's.
    pub executor_bond: PalwBondKeyV2,
    /// The bond that stakes on the challenge: Active, at or above the floor, never the claim's. It is the
    /// session's challenger and the close group's declarer.
    pub accuser_bond: PalwBondKeyV2,
    /// The global step leaf the close convicts at (the leaf the session narrows to).
    pub leaf_index: u64,
    /// How many chunks the close rides in, and the digest of each (`palw_court_close_chunk_digest_v1`).
    pub count: u8,
    pub chunk_digests: Vec<Hash64>,
    /// The digest of the assembled close: `borsh(CourtClosed { session_id, verdict: ExecutorGuilty, proof })`.
    pub close_digest: Hash64,
    /// The accuser's ML-DSA-87 over [`palw_held_leaf_challenge_digest_v1`] under
    /// [`PALW_HELD_LEAF_CHALLENGE_MLDSA87_CONTEXT_V1`].
    pub signature: Vec<u8>,
}

/// **What the accuser signs**: the network domain and every field of the challenge but the signature.
pub fn palw_held_leaf_challenge_digest_v1(network_domain: &[u8], c: &PalwHeldLeafChallengeV1) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_HELD_LEAF_CHALLENGE_DOMAIN_V1).to_state();
    s.update(&(network_domain.len() as u32).to_le_bytes());
    s.update(network_domain);
    s.update(&c.version.to_le_bytes());
    s.update(c.claim.as_byte_slice());
    s.update(c.execution_root.as_byte_slice());
    s.update(c.trace_root.as_byte_slice());
    s.update(&borsh::to_vec(&c.executor_bond).expect("a bond key is borsh-serializable"));
    s.update(&borsh::to_vec(&c.accuser_bond).expect("a bond key is borsh-serializable"));
    s.update(&c.leaf_index.to_le_bytes());
    s.update(&[c.count]);
    for digest in &c.chunk_digests {
        s.update(digest.as_byte_slice());
    }
    s.update(c.close_digest.as_byte_slice());
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// **The session the challenge opens**: the id a bisection between the same two parties over the same space
/// would have had — the claim, its trace root, the accuser as challenger and the executor as responder, the
/// step leaves, and the claim's class ladder. It names no leaf: one session per (claim, accuser bond), and
/// the accuser computes it before it files (the assembled close names it).
pub fn palw_held_leaf_challenge_session_id_v1(c: &PalwHeldLeafChallengeV1, class_ladder: u64) -> Hash64 {
    crate::palw_bisect::bisect_session_id_v1(
        &c.claim,
        &c.trace_root,
        &crate::palw_court_v2::court_party_id_v2(&c.accuser_bond),
        &crate::palw_court_v2::court_party_id_v2(&c.executor_bond),
        crate::palw_bisect::PalwBisectSpaceV1::StepLeaves,
        class_ladder,
    )
}

/// **The challenge's own shape** — stateless, asked where it rides and again at acceptance: the version, a
/// signature, between one chunk and the court's structural bound, one digest per chunk, and an executor that
/// is not the accuser.
pub fn palw_held_leaf_challenge_shape_v1(c: &PalwHeldLeafChallengeV1) -> Result<(), &'static str> {
    if c.version != PALW_HELD_LEAF_CHALLENGE_VERSION_V1 {
        return Err("a held leaf challenge is version 1");
    }
    if c.signature.is_empty() {
        return Err("a held leaf challenge must carry the accuser's signature");
    }
    if c.count == 0 || c.count > PALW_HELD_LEAF_CHALLENGE_MAX_CHUNKS_V1 {
        return Err("a held leaf challenge declares between one and 32 chunks");
    }
    if c.chunk_digests.len() != c.count as usize {
        return Err("a held leaf challenge carries one digest per declared chunk");
    }
    if c.executor_bond == c.accuser_bond {
        return Err("an executor does not challenge its own claim");
    }
    Ok(())
}

/// Why a challenge is refused by the acceptance layer (the fold's refusals are its own).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwHeldCloseErrorV1 {
    #[error("{0}")]
    Shape(&'static str),
    #[error("the challenge declares {count} chunks; this ruleset's court carries at most {ceiling}")]
    TooManyChunks { count: u64, ceiling: u64 },
    #[error("the challenge names bond {0:?} this chain does not have")]
    UnknownBond(PalwBondKeyV2),
    #[error("the challenge is not signed by the accuser bond's registered key")]
    BadSignature,
}

/// **The acceptance layer's check of a challenge** — the shape, the ruleset's own carriage count (the fold has
/// no court parameters and enforces only the structural bound), and the accuser's signature under the key
/// the registry holds at this chain point. Everything about the claim (live, its class, its roots, the leaf,
/// the open sessions, the accuser's standing) is the fold's, where the state is in hand.
pub fn check_held_leaf_challenge_acceptance_v1<V>(
    state: &PalwChainStateV2,
    network_domain: &[u8],
    c: &PalwHeldLeafChallengeV1,
    court: &PalwCourtParamsV2,
    verify_mldsa87: V,
) -> Result<(), PalwHeldCloseErrorV1>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    palw_held_leaf_challenge_shape_v1(c).map_err(PalwHeldCloseErrorV1::Shape)?;
    if u64::from(c.count) > court.max_close_chunks() {
        return Err(PalwHeldCloseErrorV1::TooManyChunks { count: u64::from(c.count), ceiling: court.max_close_chunks() });
    }
    let bond = state.bond(&c.accuser_bond).ok_or(PalwHeldCloseErrorV1::UnknownBond(c.accuser_bond))?;
    let digest = palw_held_leaf_challenge_digest_v1(network_domain, c);
    if !verify_mldsa87(&bond.pubkey, digest.as_byte_slice(), &c.signature, PALW_HELD_LEAF_CHALLENGE_MLDSA87_CONTEXT_V1) {
        return Err(PalwHeldCloseErrorV1::BadSignature);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The fence
// ---------------------------------------------------------------------------------------------

/// **The entry a flag day (or a drill) arms the held leaf challenge with**, through its own `set`, which
/// writes the bundle's mirror. In NO testnet-12 flag-day list today: the fence is dormant on every network
/// and arms with the next flag day (the generative, decode or improvement one).
pub const PALW_HELD_CLOSE_CHUNKS_ENTRY_V1: PalwPostLaunchFenceV1 = PalwPostLaunchFenceV1 {
    name: "palw_held_close_chunks_v1",
    set: |params, at| {
        params.palw_held_close_chunks_v1 = at;
        params.sync_palw_held_close_chunks_v1();
    },
};

/// The drill's one-entry list (`--palw-drill-held-chunks-at`, [`crate::config::drill::palw_drill_held_close_chunks_at_v1`]).
pub const PALW_DRILL_HELD_CLOSE_CHUNKS_FENCES_V1: &[PalwPostLaunchFenceV1] = &[PALW_HELD_CLOSE_CHUNKS_ENTRY_V1];

impl Params {
    /// `palw_held_close_chunks_v1`, resolved: `Some` only on a `ConsensusV2` network that armed it with a
    /// real height (a `never()` value is dormant).
    pub fn palw_held_close_chunks_fence(&self) -> Option<ForkActivation> {
        match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(_) => self.palw_held_close_chunks_v1.filter(|f| *f != ForkActivation::never()),
            _ => None,
        }
    }

    /// **Is the held leaf challenge in force at `daa_score`?** `false` on every shipped preset.
    pub fn palw_held_close_chunks_active_at(&self, daa_score: u64) -> bool {
        self.palw_held_close_chunks_fence().is_some_and(|f| f.is_active(daa_score))
    }

    /// **The fence's mirror** on the V2 bundle's state params (`held_close_chunks_from_daa`), which the fold
    /// reads. Written here and nothing else; `None` where the fence is not armed (or is `never()`). Call it
    /// wherever the fence is set on an assembled ruleset; [`Self::validate_palw_held_close_chunks_v1`]
    /// refuses a ruleset whose copy disagrees.
    pub fn sync_palw_held_close_chunks_v1(&mut self) {
        let from_daa = self.palw_held_close_chunks_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if let PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state = bundle.state.clone().with_held_close_chunks_from_daa(from_daa);
        }
    }

    /// **The held leaf challenge's own refusals**, asked by [`Params::validate_palw_v2`]:
    ///
    /// * a V2 bundle whose mirror of the height is not the fence's;
    /// * arming on a ruleset that is not `ConsensusV2`;
    /// * arming without `palw_tir_v1` in force at or below it — the classes the object serves are IR and
    ///   pipeline classes, and an IR object below `palw_tir_v1` is dropped by name whatever this fence says;
    /// * arming without `palw_held_context` in force at or below it — the object is the held regime's
    ///   (where bisection is played the ladder reaches the leaf, and the fold refuses it by name).
    ///
    /// A `Some(never())` value is dormant and passes (it collapses out of the identity).
    pub fn validate_palw_held_close_chunks_v1(&self) -> Result<(), PalwModeV2Error> {
        let mirror = match &self.palw_consensus_mode {
            PalwConsensusMode::ConsensusV2(bundle) => bundle.state.held_close_chunks_from_daa(),
            _ => None,
        };
        let armed = self.palw_held_close_chunks_v1.filter(|f| *f != ForkActivation::never()).map(|f| f.daa_score());
        if mirror != armed {
            return Err(PalwModeV2Error::Invalid(
                "palw_held_close_chunks_v1 disagrees with the V2 bundle's mirror: mirror it with \
                 Params::sync_palw_held_close_chunks_v1 after the bundle is assembled",
            ));
        }
        let Some(at) = armed else { return Ok(()) };
        if !matches!(self.palw_consensus_mode, PalwConsensusMode::ConsensusV2(_)) {
            return Err(PalwModeV2Error::Invalid("palw_held_close_chunks_v1 is a ConsensusV2 rule: this ruleset has no V2 bundle"));
        }
        let tir_ok = self.palw_tir_v1.is_some_and(|f| f.activation != ForkActivation::never() && f.activation.daa_score() <= at);
        if !tir_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_held_close_chunks_v1 needs palw_tir_v1 in force at or below it: it serves IR and pipeline classes",
            ));
        }
        let held_ok = self.palw_held_context.is_some_and(|f| f != ForkActivation::never() && f.daa_score() <= at);
        if !held_ok {
            return Err(PalwModeV2Error::Invalid(
                "palw_held_close_chunks_v1 needs palw_held_context in force at or below it: the object is the held regime's",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn challenge() -> PalwHeldLeafChallengeV1 {
        let bond = |i: u8| {
            PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_bytes([i; 64]), u32::from(i)))
        };
        PalwHeldLeafChallengeV1 {
            version: PALW_HELD_LEAF_CHALLENGE_VERSION_V1,
            claim: Hash64::from_bytes([1; 64]),
            execution_root: Hash64::from_bytes([2; 64]),
            trace_root: Hash64::from_bytes([3; 64]),
            executor_bond: bond(4),
            accuser_bond: bond(5),
            leaf_index: 7,
            count: 2,
            chunk_digests: vec![Hash64::from_bytes([8; 64]), Hash64::from_bytes([9; 64])],
            close_digest: Hash64::from_bytes([10; 64]),
            signature: vec![1],
        }
    }

    /// **The shape, rule by rule**, and the digest's coverage: every field but the signature moves it, the
    /// network domain moves it, the signature does not.
    #[test]
    fn the_shape_refuses_what_it_names_and_the_digest_covers_every_field() {
        let base = challenge();
        assert_eq!(palw_held_leaf_challenge_shape_v1(&base), Ok(()));
        let mut bad = base.clone();
        bad.version = 2;
        assert!(palw_held_leaf_challenge_shape_v1(&bad).is_err());
        let mut bad = base.clone();
        bad.signature.clear();
        assert!(palw_held_leaf_challenge_shape_v1(&bad).is_err());
        for count in [0u8, 33] {
            let mut bad = base.clone();
            bad.count = count;
            assert!(palw_held_leaf_challenge_shape_v1(&bad).is_err(), "count {count}");
        }
        let mut bad = base.clone();
        bad.chunk_digests.pop();
        assert!(palw_held_leaf_challenge_shape_v1(&bad).is_err(), "one digest per chunk");
        let mut bad = base.clone();
        bad.accuser_bond = bad.executor_bond;
        assert!(palw_held_leaf_challenge_shape_v1(&bad).is_err(), "an executor does not challenge itself");

        let domain = b"testnet-12";
        let digest = palw_held_leaf_challenge_digest_v1(domain, &base);
        assert_ne!(digest, palw_held_leaf_challenge_digest_v1(b"another-network", &base), "the network");
        let mut moved = base.clone();
        moved.signature = vec![2, 3];
        assert_eq!(digest, palw_held_leaf_challenge_digest_v1(domain, &moved), "the signature is not signed over itself");
        let touch: [Box<dyn Fn(&mut PalwHeldLeafChallengeV1)>; 9] = [
            Box::new(|c| c.claim = Hash64::from_bytes([11; 64])),
            Box::new(|c| c.execution_root = Hash64::from_bytes([11; 64])),
            Box::new(|c| c.trace_root = Hash64::from_bytes([11; 64])),
            Box::new(|c| c.leaf_index += 1),
            Box::new(|c| c.close_digest = Hash64::from_bytes([11; 64])),
            Box::new(|c| c.chunk_digests[1] = Hash64::from_bytes([11; 64])),
            Box::new(|c| c.chunk_digests.swap(0, 1)),
            Box::new(|c| std::mem::swap(&mut c.executor_bond, &mut c.accuser_bond)),
            Box::new(|c| c.version += 1),
        ];
        for (k, f) in touch.iter().enumerate() {
            let mut moved = base.clone();
            f(&mut moved);
            assert_ne!(digest, palw_held_leaf_challenge_digest_v1(domain, &moved), "field {k}");
        }
    }

    /// **The session id names no leaf and one accuser**: two challenges that differ only in the leaf (or the
    /// close) open the same session; another accuser, another ladder or another claim another one.
    #[test]
    fn the_session_is_keyed_to_the_claim_and_the_accuser_not_the_leaf() {
        let base = challenge();
        let id = palw_held_leaf_challenge_session_id_v1(&base, 1_000);
        let mut other_leaf = base.clone();
        other_leaf.leaf_index = 99;
        other_leaf.close_digest = Hash64::from_bytes([77; 64]);
        assert_eq!(id, palw_held_leaf_challenge_session_id_v1(&other_leaf, 1_000), "no leaf in the id");
        assert_ne!(id, palw_held_leaf_challenge_session_id_v1(&base, 2_000), "the ladder");
        let mut other_accuser = base.clone();
        other_accuser.accuser_bond = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(
            crate::tx::TransactionId::from_bytes([6; 64]),
            6,
        ));
        assert_ne!(id, palw_held_leaf_challenge_session_id_v1(&other_accuser, 1_000), "the accuser");
        let mut other_claim = base.clone();
        other_claim.claim = Hash64::from_bytes([12; 64]);
        assert_ne!(id, palw_held_leaf_challenge_session_id_v1(&other_claim, 1_000), "the claim");
    }
}
