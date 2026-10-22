//! ADR-0188 B isolated data-commitment codec. Not a native lifecycle kind,
//! accepting record, model review, availability guarantee or computation credit.
//! The caller authenticates the draw/context, adopted draw cap and literal seat
//! authority before eventual admission. Unknown caps never receive a default.
use super::{PALW_D2_SEAT_READINESS_DIGEST_TAG_V1 as TAG, PalwD2ReadinessVerdictV1, palw_d2_readiness_refute_v1};
use crate::{Hash64, palw_court_scope_v1::RegisteredModelRootsV1, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};
use std::{
    io::{self, Read},
    num::NonZeroU32,
};

const LEAF_DOMAIN: &[u8] = b"MISAKA/PALW/D2/READINESS-ROOT-LEAF/V1";
/// Structural maximum for a tree with a u32 count, not an adopted draw cap.
pub const MAX_PATH: usize = u32::BITS as usize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ReadinessContextV1 {
    pub beacon: Hash64,
    pub class_id: Hash64,
    pub seat: PalwBondKeyV2,
    pub span: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SeatReadinessRootV1 {
    pub beacon: Hash64,
    pub class_id: Hash64,
    pub seat: PalwBondKeyV2,
    pub span: u64,
    pub root: Hash64,
    pub leaf_count: u32,
}
/// One digest opening, bottom-up sibling order; odd nodes are promoted exactly
/// as in the existing artifact tree. No model bytes are requested from the seat.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct ReadinessDigestPathV1 {
    pub leaf_index: u32,
    pub claimed_digest: Hash64,
    pub siblings: Vec<Hash64>,
}
impl BorshDeserialize for ReadinessDigestPathV1 {
    fn deserialize_reader<R: Read>(r: &mut R) -> io::Result<Self> {
        let leaf_index = u32::deserialize_reader(r)?;
        let claimed_digest = Hash64::deserialize_reader(r)?;
        let count = u32::deserialize_reader(r)? as usize;
        if count > MAX_PATH {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "readiness path exceeds u32 tree depth"));
        }
        let mut siblings = Vec::with_capacity(count);
        for _ in 0..count {
            siblings.push(Hash64::deserialize_reader(r)?);
        }
        Ok(Self { leaf_index, claimed_digest, siblings })
    }
}
fn leaf(c: &ReadinessContextV1, index: u32, digest: Hash64) -> Hash64 {
    Hash64::from_bytes(misaka_palw_challenge::hash::object_id(LEAF_DOMAIN, &(c, index, digest)))
}
fn check_draw(drawn: &[u32], cap: Option<NonZeroU32>) -> Result<u32, &'static str> {
    let cap = cap.ok_or("readiness draw cap is unadopted/unknown")?.get();
    let count = u32::try_from(drawn.len()).map_err(|_| "readiness draw count overflow")?;
    if count == 0 || count > cap || drawn.windows(2).any(|w| w[0] >= w[1]) {
        return Err("readiness draw must be nonempty, ascending, unique and inside its explicit cap");
    }
    Ok(count)
}
impl SeatReadinessRootV1 {
    pub fn context(&self) -> ReadinessContextV1 {
        ReadinessContextV1 { beacon: self.beacon, class_id: self.class_id, seat: self.seat, span: self.span }
    }
    /// Separate tagged candidate encoding; does not register tag230 with the
    /// current enum/dispatcher or change historical A-2/fork fingerprints.
    pub fn encode_tagged(&self) -> Result<Vec<u8>, &'static str> {
        if self.leaf_count == 0 {
            return Err("empty readiness root");
        }
        let mut out = vec![TAG];
        self.serialize(&mut out).map_err(|_| "readiness root encoding")?;
        Ok(out)
    }
    /// Fixed-field payload: no peer-controlled allocation. Full consumption is
    /// mandatory. A decoded DTO carries no seat authentication/admission right.
    pub fn decode_tagged(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.first() != Some(&TAG) {
            return Err("wrong readiness root tag");
        }
        let root: Self = borsh::from_slice(&bytes[1..]).map_err(|_| "readiness root shape/trailing bytes")?;
        if root.leaf_count == 0 {
            return Err("empty readiness root");
        }
        Ok(root)
    }
    /// Commitment construction from existing keyed digests; never computes or
    /// verifies a model. Ownership/CPU/IO and the complete draw are caller-funded.
    pub fn from_digests(
        context: ReadinessContextV1,
        digests: &[(u32, Hash64)],
        cap: Option<NonZeroU32>,
    ) -> Result<Self, &'static str> {
        let count = u32::try_from(digests.len()).map_err(|_| "readiness digest count overflow")?;
        let cap = cap.ok_or("readiness draw cap is unadopted/unknown")?.get();
        if count == 0 || count > cap || digests.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err("readiness digests must match one ascending unique bounded draw");
        }
        let mut tree = crate::palw_artifact::PalwArtifactMerkleFrontierV1::new();
        for (i, d) in digests {
            tree.push(leaf(&context, *i, *d));
        }
        Ok(Self {
            beacon: context.beacon,
            class_id: context.class_id,
            seat: context.seat,
            span: context.span,
            root: tree.root().ok_or("empty readiness root")?,
            leaf_count: count,
        })
    }
    /// Establishes membership of a claimed digest only. The expected context and
    /// draw must come from native authority, not from this unauthenticated DTO.
    pub fn check_path(
        &self,
        expected: &ReadinessContextV1,
        drawn: &[u32],
        cap: Option<NonZeroU32>,
        path: &ReadinessDigestPathV1,
    ) -> Result<(), &'static str> {
        if &self.context() != expected || self.leaf_count != check_draw(drawn, cap)? {
            return Err("readiness root substitutes draw/context/count");
        }
        let ordinal = drawn.binary_search(&path.leaf_index).map_err(|_| "readiness leaf is not drawn")? as u32;
        let expected_len =
            crate::palw_artifact::palw_artifact_opening_path_len_v1(self.leaf_count, ordinal).ok_or("readiness path coordinate")?;
        if path.siblings.len() as u64 != expected_len {
            return Err("readiness path length is not canonical");
        }
        let (mut index, mut width) = (u64::from(ordinal), u64::from(self.leaf_count));
        let mut acc = leaf(expected, path.leaf_index, path.claimed_digest);
        let mut siblings = path.siblings.iter();
        while width > 1 {
            if !(index == width - 1 && width % 2 == 1) {
                let sibling = siblings.next().ok_or("readiness sibling missing")?;
                acc = if index % 2 == 0 {
                    crate::palw_artifact::artifact_node_v1(&acc, sibling)
                } else {
                    crate::palw_artifact::artifact_node_v1(sibling, &acc)
                };
            }
            index /= 2;
            width = width.div_ceil(2);
        }
        if siblings.next().is_some() || acc != self.root {
            return Err("readiness root mismatch");
        }
        Ok(())
    }
    /// Uses the verifier's own registered opening and the existing exact digest
    /// predicate. Returns a local result only; no slash, reward or model review.
    pub fn refute(
        &self,
        expected: &ReadinessContextV1,
        roots: &RegisteredModelRootsV1,
        drawn: &[u32],
        cap: Option<NonZeroU32>,
        path: &ReadinessDigestPathV1,
        opening: &crate::palw_artifact::PalwArtifactOpeningV1,
    ) -> Result<PalwD2ReadinessVerdictV1, &'static str> {
        self.check_path(expected, drawn, cap, path)?;
        if opening.leaf_index != path.leaf_index {
            return Err("readiness refutation substitutes inventory coordinate");
        }
        palw_d2_readiness_refute_v1(
            roots,
            expected.beacon,
            expected.class_id,
            expected.seat,
            expected.span,
            drawn,
            path.claimed_digest,
            opening,
        )
    }
}

// =================================================================================================================================
// Tag 230 as a lifecycle kind (`PalwConsensusObjectV2::SeatReadinessRootV1`, ADR-0188 B / ADR-0200 C1). Dormant behind
// `palw_court_model_scope_v1`; past it, admission still refuses while the draw cap is unadopted (typed-missing, never zero/default).
// =================================================================================================================================

/// **The adopted readiness draw cap — NOT adopted** (ADR-0188 "ユーザーに決めてほしいこと"; ADR-0188 B approval 2026-10-21 says the
/// cap is not chosen). `None` is the typed-missing value: every admission and every fold refuses with it. Changing it is a user decision.
pub const PALW_D2_READINESS_DRAW_CAP_V1: Option<NonZeroU32> = None;

/// ML-DSA-87 context of the seat's signature over a readiness root (the literal seat authority).
pub const PALW_D2_READINESS_ROOT_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/d2-readiness-root/mldsa87/v1";
const MESSAGE_DOMAIN: &[u8] = b"MISAKA/PALW/D2/READINESS-ROOT-MESSAGE/V1";
const REGISTRY_KEY_DOMAIN: &[u8] = b"MISAKA/PALW/D2/READINESS-ROOT-KEY/V1";

/// **What the seat's bond signs**: the network domain and the whole root object (context, root, count).
pub fn palw_d2_readiness_root_message_v1(network_domain: Hash64, root: &SeatReadinessRootV1) -> Hash64 {
    Hash64::from_bytes(misaka_palw_challenge::hash::object_id(MESSAGE_DOMAIN, &(network_domain, root)))
}

/// **Shape admission of a carried root against an explicit cap.** `None` (the unadopted cap) is always a refusal; a zero count or a
/// count above the cap is refused. Grants nothing: no ready credit, slash or model review (ADR-0200: no root-only ready/slash door).
pub fn palw_d2_readiness_root_admit_v1(root: &SeatReadinessRootV1, cap: Option<NonZeroU32>) -> Result<(), &'static str> {
    let cap = cap.ok_or("readiness draw cap is unadopted/unknown")?.get();
    if root.leaf_count == 0 || root.leaf_count > cap {
        return Err("readiness root count must be nonzero and inside the explicit draw cap");
    }
    Ok(())
}

/// The registry key of one root: `(beacon, class, seat, span)` — one root per seat per draw.
pub fn palw_d2_readiness_root_key_v1(context: &ReadinessContextV1) -> Hash64 {
    Hash64::from_bytes(misaka_palw_challenge::hash::object_id(REGISTRY_KEY_DOMAIN, context))
}

/// Recorded readiness roots, keyed by [`palw_d2_readiness_root_key_v1`].
pub type PalwD2ReadinessRootRegistryV1 = std::collections::BTreeMap<Hash64, SeatReadinessRootV1>;

/// **The fold of a tag-230 object**: admitted against `cap`, written once per `(beacon, class, seat, span)`; a second root for the
/// same key is refused (no replacement after a refuter could have read the first). Pure; the caller supplies the fence gate.
pub fn palw_d2_readiness_root_fold_v1(
    registry: &mut PalwD2ReadinessRootRegistryV1,
    root: &SeatReadinessRootV1,
    cap: Option<NonZeroU32>,
) -> Result<Hash64, &'static str> {
    palw_d2_readiness_root_admit_v1(root, cap)?;
    let key = palw_d2_readiness_root_key_v1(&root.context());
    if registry.contains_key(&key) {
        return Err("a readiness root for this seat and draw is already recorded");
    }
    registry.insert(key, root.clone());
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn h(n: u32) -> Hash64 {
        let mut x = [0; 64];
        x[..4].copy_from_slice(&n.to_le_bytes());
        Hash64::from_bytes(x)
    }
    fn ctx() -> ReadinessContextV1 {
        ReadinessContextV1 {
            beacon: h(1),
            class_id: h(2),
            seat: PalwBondKeyV2(crate::tx::TransactionOutpoint {
                transaction_id: crate::tx::TransactionId::from_u64_word(3),
                index: 0,
            }),
            span: 4,
        }
    }
    // Test-only reference tree walk. No production draw cap is selected here.
    fn path(c: &ReadinessContextV1, ds: &[(u32, Hash64)], ordinal: usize) -> ReadinessDigestPathV1 {
        let mut level: Vec<_> = ds.iter().map(|(i, d)| leaf(c, *i, *d)).collect();
        let mut i = ordinal;
        let mut siblings = vec![];
        while level.len() > 1 {
            let partner = i ^ 1;
            if partner < level.len() {
                siblings.push(level[partner]);
            }
            level = level
                .chunks(2)
                .map(|p| if p.len() == 2 { crate::palw_artifact::artifact_node_v1(&p[0], &p[1]) } else { p[0] })
                .collect();
            i /= 2;
        }
        ReadinessDigestPathV1 { leaf_index: ds[ordinal].0, claimed_digest: ds[ordinal].1, siblings }
    }
    #[test]
    fn bounded_codec_and_every_promoted_path_preserve_exact_draw_identity() {
        let c = ctx();
        for count in 1..=65 {
            let ds: Vec<_> = (0..count).map(|i| (i * 2, h(100 + i))).collect();
            let draw: Vec<_> = ds.iter().map(|p| p.0).collect();
            let cap = NonZeroU32::new(count);
            let root = SeatReadinessRootV1::from_digests(c, &ds, cap).unwrap();
            let bytes = root.encode_tagged().unwrap();
            assert_eq!(bytes[0], 230);
            assert_eq!(SeatReadinessRootV1::decode_tagged(&bytes).unwrap(), root);
            for i in 0..count as usize {
                let p = path(&c, &ds, i);
                root.check_path(&c, &draw, cap, &p).unwrap();
                assert_eq!(borsh::from_slice::<ReadinessDigestPathV1>(&borsh::to_vec(&p).unwrap()).unwrap(), p);
                let mut bad = p.clone();
                bad.siblings.push(h(99));
                assert!(root.check_path(&c, &draw, cap, &bad).is_err());
                let mut bad = p.clone();
                bad.claimed_digest = h(999);
                assert!(root.check_path(&c, &draw, cap, &bad).is_err());
                if !p.siblings.is_empty() {
                    let mut bad = p.clone();
                    bad.siblings[0] = h(999);
                    assert!(root.check_path(&c, &draw, cap, &bad).is_err());
                }
            }
            for cut in [0, 1, bytes.len() - 1] {
                assert!(SeatReadinessRootV1::decode_tagged(&bytes[..cut]).is_err());
            }
            let mut trailing = bytes.clone();
            trailing.push(0);
            assert!(SeatReadinessRootV1::decode_tagged(&trailing).is_err());
            let mut wrong_tag = bytes.clone();
            wrong_tag[0] = 229;
            assert!(SeatReadinessRootV1::decode_tagged(&wrong_tag).is_err());
            let mut empty = root.clone();
            empty.leaf_count = 0;
            assert!(empty.encode_tagged().is_err());
            let mut empty_bytes = vec![230];
            empty.serialize(&mut empty_bytes).unwrap();
            assert!(SeatReadinessRootV1::decode_tagged(&empty_bytes).is_err());
            if count > 1 {
                assert!(root.check_path(&c, &draw, NonZeroU32::new(count - 1), &path(&c, &ds, 0)).is_err());
            }
            for change in 0..3 {
                let mut wrong = c;
                match change {
                    0 => wrong.beacon = h(999),
                    1 => wrong.class_id = h(999),
                    _ => wrong.seat.0.index += 1,
                };
                assert!(root.check_path(&wrong, &draw, cap, &path(&c, &ds, 0)).is_err());
            }
            let mut undrawn = path(&c, &ds, 0);
            undrawn.leaf_index = 1;
            assert!(root.check_path(&c, &draw, cap, &undrawn).is_err());
            assert!(root.check_path(&c, &draw, None, &path(&c, &ds, 0)).is_err());
            assert!(SeatReadinessRootV1::from_digests(c, &ds, None).is_err());
            let mut wrong = c;
            wrong.span += 1;
            assert!(root.check_path(&wrong, &draw, cap, &path(&c, &ds, 0)).is_err());
            let mut wrong = root.clone();
            wrong.leaf_count += 1;
            assert!(wrong.check_path(&c, &draw, cap, &path(&c, &ds, 0)).is_err());
        }
        let mut hostile = borsh::to_vec(&ReadinessDigestPathV1 { leaf_index: 0, claimed_digest: h(1), siblings: vec![] }).unwrap();
        hostile[68..72].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(borsh::from_slice::<ReadinessDigestPathV1>(&hostile).is_err());
        assert!(SeatReadinessRootV1::from_digests(c, &[(1, h(1)), (1, h(2))], NonZeroU32::new(2)).is_err());
    }
    #[test]
    fn only_an_authenticated_wrong_digest_is_refuted_with_verifiers_own_bytes() {
        use crate::palw_artifact::*;
        use crate::palw_court_model_scope_routes_v1::{PalwD2ReadinessDigestInputV1, palw_d2_readiness_digest_v1};
        let c = ctx();
        let leaves: Vec<_> = (0..3)
            .map(|i| PalwArtifactOperandV1 { tensor_name: format!("t{i}"), layer: None, row_start: 0, bytes: vec![i as u8; 8] })
            .collect();
        let roots = RegisteredModelRootsV1 {
            artifact_root: artifact_root_v1(&leaves.iter().map(artifact_leaf_v1).collect::<Vec<_>>()),
            artifact_leaf_count: Some(3),
            ..Default::default()
        };
        let draw = [0, 2];
        let cap = NonZeroU32::new(2);
        let opens = open_artifact_leaves_v1(&leaves, &draw).unwrap();
        let mut ds: Vec<_> = draw
            .iter()
            .map(|i| {
                (
                    *i,
                    palw_d2_readiness_digest_v1(&PalwD2ReadinessDigestInputV1 {
                        beacon: c.beacon,
                        class_id: c.class_id,
                        seat: c.seat,
                        span: c.span,
                        leaf_index: *i,
                        leaf: leaves[*i as usize].clone(),
                    }),
                )
            })
            .collect();
        let honest = SeatReadinessRootV1::from_digests(c, &ds, cap).unwrap();
        assert_eq!(honest.refute(&c, &roots, &draw, cap, &path(&c, &ds, 1), &opens[1]), Ok(PalwD2ReadinessVerdictV1::DigestHolds));
        ds[1].1 = h(999);
        let liar = SeatReadinessRootV1::from_digests(c, &ds, cap).unwrap();
        assert_eq!(liar.refute(&c, &roots, &draw, cap, &path(&c, &ds, 1), &opens[1]), Ok(PalwD2ReadinessVerdictV1::SeatRefuted));
        assert!(honest.refute(&c, &roots, &draw, cap, &path(&c, &ds, 1), &opens[1]).is_err());
        let mut forged = opens[1].clone();
        forged.operand.bytes[0] ^= 1;
        assert!(liar.refute(&c, &roots, &draw, cap, &path(&c, &ds, 1), &forged).is_err());
        assert!(liar.refute(&c, &roots, &draw, cap, &path(&c, &ds, 1), &opens[0]).is_err());
    }
    fn root_of(count: u32) -> SeatReadinessRootV1 {
        let ds: Vec<_> = (0..count).map(|i| (i, h(500 + i))).collect();
        SeatReadinessRootV1::from_digests(ctx(), &ds, NonZeroU32::new(count)).unwrap()
    }
    /// The unadopted cap (the shipped constant) refuses every admission and every fold: `None` is never success.
    #[test]
    fn tag230_unadopted_draw_cap_refuses_admission_and_fold() {
        assert_eq!(PALW_D2_READINESS_DRAW_CAP_V1, None);
        let root = root_of(3);
        assert!(palw_d2_readiness_root_admit_v1(&root, PALW_D2_READINESS_DRAW_CAP_V1).is_err());
        let mut reg = PalwD2ReadinessRootRegistryV1::new();
        assert!(palw_d2_readiness_root_fold_v1(&mut reg, &root, PALW_D2_READINESS_DRAW_CAP_V1).is_err());
        assert!(reg.is_empty());
    }
    /// With an explicit test cap: count bounds, one root per seat and draw, a different span is a different key.
    #[test]
    fn tag230_fold_writes_once_per_seat_and_draw_inside_the_cap() {
        let root = root_of(3);
        assert!(palw_d2_readiness_root_admit_v1(&root, NonZeroU32::new(2)).is_err());
        let mut zero = root.clone();
        zero.leaf_count = 0;
        assert!(palw_d2_readiness_root_admit_v1(&zero, NonZeroU32::new(3)).is_err());
        let mut reg = PalwD2ReadinessRootRegistryV1::new();
        let key = palw_d2_readiness_root_fold_v1(&mut reg, &root, NonZeroU32::new(3)).unwrap();
        assert_eq!(reg.get(&key), Some(&root));
        let mut again = root.clone();
        again.root = h(77);
        assert!(palw_d2_readiness_root_fold_v1(&mut reg, &again, NonZeroU32::new(3)).is_err());
        assert_eq!(reg.get(&key), Some(&root));
        let mut other = root.clone();
        other.span += 1;
        palw_d2_readiness_root_fold_v1(&mut reg, &other, NonZeroU32::new(3)).unwrap();
        assert_eq!(reg.len(), 2);
    }
    /// The lifecycle variant's borsh is the tag-230 codec followed by the signature: round-trip, and boundary decodes refuse.
    #[test]
    fn tag230_lifecycle_object_round_trips_and_refuses_boundary_bytes() {
        use crate::palw_state_v2::PalwConsensusObjectV2;
        let root = root_of(5);
        let object = PalwConsensusObjectV2::SeatReadinessRootV1 { root: root.clone(), signature: vec![7; 4] };
        let bytes = borsh::to_vec(&object).unwrap();
        assert_eq!(bytes[0], TAG);
        let codec = root.encode_tagged().unwrap();
        assert_eq!(&bytes[..codec.len()], codec.as_slice());
        assert_eq!(borsh::from_slice::<PalwConsensusObjectV2>(&bytes).unwrap(), object);
        for cut in [1, codec.len() - 1, codec.len(), bytes.len() - 1] {
            assert!(borsh::from_slice::<PalwConsensusObjectV2>(&bytes[..cut]).is_err(), "cut at {cut}");
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(borsh::from_slice::<PalwConsensusObjectV2>(&trailing).is_err());
        let m = palw_d2_readiness_root_message_v1(h(1), &root);
        assert_ne!(m, palw_d2_readiness_root_message_v1(h(2), &root));
        let mut moved = root.clone();
        moved.leaf_count += 1;
        assert_ne!(m, palw_d2_readiness_root_message_v1(h(1), &moved));
    }
}
