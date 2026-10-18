//! Fixed native prefix identity for applied material challenge subjects. The
//! prefix binds the existing actor's accepted sequence, not computation
//! correctness, fee payment, selected-verifier approval, or finality. A caller
//! must possess its exact contiguous WAL subjects to reconstruct a prefix;
//! this fixed cell is not an arbitrary historical inclusion witness.
use crate::{
    Hash64,
    palw_kernel_route_v1::PalwKernelRouteStateV1,
    palw_material_challenge_checkpoint_v4::{PalwMaterialChallengeCheckpointV4, material_challenge_checkpoint_from_route_v4},
    palw_material_release_recovery_v2::PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 as TABLE,
    palw_state_v2::PalwBondKeyV2,
};

pub const PALW_MATERIAL_CHALLENGE_HISTORY_PREFIX_V4: u8 = 0xA3;
pub const PALW_MATERIAL_CHALLENGE_HISTORY_KEY_BYTES_V4: usize = 69;
pub const PALW_MATERIAL_CHALLENGE_HISTORY_BYTES_V4: usize = 206;
pub const PALW_MATERIAL_CHALLENGE_HISTORY_ANCHOR_BYTES_V4: usize = 72;

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwMaterialChallengeHistoryAnchorV4 {
    pub sequence: u64,
    pub root: Hash64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwMaterialChallengeHistoryV4 {
    pub version: u16,
    pub challenger: PalwBondKeyV2,
    pub sequence: u64,
    pub previous: Hash64,
    pub root: Hash64,
}

fn digest(domain: &[u8], value: &impl borsh::BorshSerialize) -> Hash64 {
    struct Stream(blake2b_simd::State);
    impl std::io::Write for Stream {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut sink = Stream(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    borsh::to_writer(&mut sink, value).expect("fixed challenge history identity stream cannot fail");
    Hash64::from_bytes(sink.0.finalize().as_bytes().try_into().expect("64-byte history digest"))
}

pub fn material_challenge_history_key_v4(actor: &PalwBondKeyV2) -> [u8; PALW_MATERIAL_CHALLENGE_HISTORY_KEY_BYTES_V4] {
    let mut key = [0; PALW_MATERIAL_CHALLENGE_HISTORY_KEY_BYTES_V4];
    key[0] = PALW_MATERIAL_CHALLENGE_HISTORY_PREFIX_V4;
    borsh::to_writer(&mut &mut key[1..], actor).expect("fixed native bond fits history key");
    key
}

/// Authenticate the starting prefix against the same own-native A2/A3 view.
/// The legacy bridge commits an existing latest A2 only; it does not recover
/// subjects that the earlier single-checkpoint representation already lost.
pub fn material_challenge_history_base_anchor_v4(
    network: Hash64,
    actor: PalwBondKeyV2,
    old_checkpoint: Option<&PalwMaterialChallengeCheckpointV4>,
    old_history: Option<&PalwMaterialChallengeHistoryV4>,
) -> Result<PalwMaterialChallengeHistoryAnchorV4, &'static str> {
    if let Some(history) = old_history {
        let checkpoint = old_checkpoint.ok_or("challenge history has no matching latest applied checkpoint")?;
        history.validate(network, checkpoint)?;
        if history.challenger != actor {
            return Err("challenge history base substitutes actor");
        }
        return Ok(PalwMaterialChallengeHistoryAnchorV4 { sequence: history.sequence, root: history.root });
    }
    if let Some(checkpoint) = old_checkpoint {
        checkpoint.validate(network)?;
        if checkpoint.challenger != actor {
            return Err("challenge history legacy bridge substitutes actor");
        }
        return Ok(PalwMaterialChallengeHistoryAnchorV4 {
            sequence: checkpoint.sequence,
            root: digest(
                b"MISAKA/PALW/MATERIAL-CHALLENGE-LEGACY-BRIDGE/V4",
                &(network, actor, checkpoint.sequence, checkpoint.statement),
            ),
        });
    }
    Ok(PalwMaterialChallengeHistoryAnchorV4 {
        sequence: 0,
        root: digest(b"MISAKA/PALW/MATERIAL-CHALLENGE-GENESIS/V4", &(network, actor)),
    })
}

/// Reconstruct one exact applied-subject link without allocation. The input
/// anchor must originate in an authenticated own-native view or a contiguous
/// local reconstruction ending at that view. A bare anchor grants no authority.
pub fn advance_material_challenge_history_anchor_v4(
    network: Hash64,
    actor: PalwBondKeyV2,
    base: PalwMaterialChallengeHistoryAnchorV4,
    statement: Hash64,
) -> Result<PalwMaterialChallengeHistoryAnchorV4, &'static str> {
    if base.root == Hash64::default() || statement == Hash64::default() {
        return Err("challenge history prefix or applied subject is absent");
    }
    let sequence = base.sequence.checked_add(1).ok_or("challenge history sequence exhausted")?;
    Ok(PalwMaterialChallengeHistoryAnchorV4 {
        sequence,
        root: digest(b"MISAKA/PALW/MATERIAL-CHALLENGE-PREFIX/V4", &(network, actor, sequence, base.root, statement)),
    })
}

impl PalwMaterialChallengeHistoryV4 {
    pub fn next(
        network: Hash64,
        actor: PalwBondKeyV2,
        previous_checkpoint: Option<&PalwMaterialChallengeCheckpointV4>,
        previous_history: Option<&Self>,
        new_checkpoint: &PalwMaterialChallengeCheckpointV4,
    ) -> Result<Self, &'static str> {
        let base = material_challenge_history_base_anchor_v4(network, actor, previous_checkpoint, previous_history)?;
        new_checkpoint.validate(network)?;
        if new_checkpoint.challenger != actor {
            return Err("challenge history next checkpoint substitutes actor");
        }
        let next = advance_material_challenge_history_anchor_v4(network, actor, base, new_checkpoint.statement)?;
        if new_checkpoint.sequence != next.sequence {
            return Err("challenge history next checkpoint skips or repeats a sequence");
        }
        let history = Self { version: 4, challenger: actor, sequence: next.sequence, previous: base.root, root: next.root };
        history.validate(network, new_checkpoint)?;
        Ok(history)
    }

    pub fn validate(&self, network: Hash64, latest_checkpoint: &PalwMaterialChallengeCheckpointV4) -> Result<(), &'static str> {
        latest_checkpoint.validate(network)?;
        if self.version != 4 || self.sequence == 0 || self.challenger != latest_checkpoint.challenger {
            return Err("challenge history version/sequence/actor differs");
        }
        if self.sequence == 1 && self.previous != material_challenge_history_base_anchor_v4(network, self.challenger, None, None)?.root
        {
            return Err("first challenge history link substitutes its genesis");
        }
        let next = advance_material_challenge_history_anchor_v4(
            network,
            self.challenger,
            PalwMaterialChallengeHistoryAnchorV4 { sequence: self.sequence - 1, root: self.previous },
            latest_checkpoint.statement,
        )?;
        if latest_checkpoint.sequence != self.sequence || next.root != self.root {
            return Err("challenge history latest checkpoint/prefix differs");
        }
        Ok(())
    }

    pub fn encoded(&self) -> Result<[u8; PALW_MATERIAL_CHALLENGE_HISTORY_BYTES_V4], &'static str> {
        let mut bytes = [0; PALW_MATERIAL_CHALLENGE_HISTORY_BYTES_V4];
        let mut remaining = bytes.as_mut_slice();
        borsh::to_writer(&mut remaining, self).map_err(|_| "challenge history fixed encoding extent differs")?;
        if !remaining.is_empty() {
            return Err("challenge history fixed encoding has unused bytes");
        }
        Ok(bytes)
    }
}

/// Read the fixed row from an own accepted route. Missing history is unknown;
/// present history must match the same route's latest A2 checkpoint exactly.
pub fn material_challenge_history_from_route_v4(
    route: &PalwKernelRouteStateV1,
    actor: PalwBondKeyV2,
) -> Result<Option<PalwMaterialChallengeHistoryV4>, crate::palw_native_resource_v1::NativeDecodeErrorV1<String>> {
    use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;
    let key = material_challenge_history_key_v4(&actor);
    let Some(raw) =
        crate::palw_material_challenge_checkpoint_v4::material_challenge_fixed_row_v4(route, TABLE, &key).map_err(E::Resource)?
    else {
        return Ok(None);
    };
    let history = decode_material_challenge_history_v4(raw).map_err(|why| E::Decode(why.into()))?;
    let checkpoint = material_challenge_checkpoint_from_route_v4(route, actor)?
        .ok_or_else(|| E::Decode("challenge history own route lacks matching latest applied checkpoint".into()))?;
    history.validate(Hash64::from_bytes(route.header.policy.network_domain), &checkpoint).map_err(|why| E::Decode(why.into()))?;
    if history.challenger != actor {
        return Err(E::Decode("challenge history key substitutes actor".into()));
    }
    Ok(Some(history))
}

/// Every field is fixed sized; malformed lengths are refused before decoding.
pub(crate) fn decode_material_challenge_history_v4(raw: &[u8]) -> Result<PalwMaterialChallengeHistoryV4, &'static str> {
    if raw.len() != PALW_MATERIAL_CHALLENGE_HISTORY_BYTES_V4 {
        return Err("challenge history fixed row extent differs");
    }
    borsh::from_slice(raw).map_err(|_| "challenge history fixed row does not decode")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as Action;
    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_bytes([n; 64]), 0))
    }
    fn checkpoint(previous: Option<PalwMaterialChallengeCheckpointV4>, position: u32) -> PalwMaterialChallengeCheckpointV4 {
        PalwMaterialChallengeCheckpointV4::next(
            previous,
            Hash64::from_bytes([1; 64]),
            bond(2),
            &Action::ChallengeCurrentV4 {
                root: Hash64::from_bytes([3; 64]),
                consent: Hash64::from_bytes([4; 64]),
                provider: bond(5),
                dependency: Hash64::from_bytes([3; 64]),
                stage: 255,
                position,
                valid_until_daa: 100,
            },
            10 + u64::from(position),
        )
        .unwrap()
    }
    #[test]
    fn genesis_and_legacy_bridge_are_separate_and_contiguous_subjects_reproduce_native_prefix() {
        let network = Hash64::from_bytes([1; 64]);
        let actor = bond(2);
        let first = checkpoint(None, 1);
        let genesis = material_challenge_history_base_anchor_v4(network, actor, None, None).unwrap();
        assert_eq!(genesis.sequence, 0);
        let initial = PalwMaterialChallengeHistoryV4::next(network, actor, None, None, &first).unwrap();
        assert_eq!(advance_material_challenge_history_anchor_v4(network, actor, genesis, first.statement).unwrap().root, initial.root);
        let bridge = material_challenge_history_base_anchor_v4(network, actor, Some(&first), None).unwrap();
        assert_eq!(bridge.sequence, 1);
        assert_ne!(bridge.root, initial.root);
        let second = checkpoint(Some(first), 2);
        let continuous = PalwMaterialChallengeHistoryV4::next(network, actor, Some(&first), Some(&initial), &second).unwrap();
        let migrated = PalwMaterialChallengeHistoryV4::next(network, actor, Some(&first), None, &second).unwrap();
        assert_eq!(continuous.sequence, 2);
        assert_eq!(continuous.previous, initial.root);
        assert_eq!(migrated.previous, bridge.root);
        assert_ne!(continuous.root, migrated.root);
        assert!(PalwMaterialChallengeHistoryV4::next(network, actor, Some(&second), Some(&continuous), &second).is_err());
        assert!(PalwMaterialChallengeHistoryV4::next(network, actor, None, Some(&initial), &second).is_err());
    }
    #[test]
    fn fixed_codec_and_little_endian_sequence_preserve_all_206_bytes() {
        let network = Hash64::from_bytes([1; 64]);
        let first = checkpoint(None, 1);
        let history = PalwMaterialChallengeHistoryV4::next(network, bond(2), None, None, &first).unwrap();
        let raw = history.encoded().unwrap();
        assert_eq!(borsh::to_vec(&history).unwrap(), raw);
        assert_eq!(&raw[..2], &4u16.to_le_bytes());
        assert_eq!(&raw[70..78], &1u64.to_le_bytes());
        assert_eq!(decode_material_challenge_history_v4(&raw).unwrap(), history);
        assert_eq!(
            borsh::to_vec(&material_challenge_history_base_anchor_v4(network, bond(2), None, None).unwrap()).unwrap().len(),
            72
        );
        assert_eq!(material_challenge_history_key_v4(&bond(2))[0], 0xA3);
        assert_eq!(material_challenge_history_key_v4(&bond(2)).len(), 69);
        assert!(decode_material_challenge_history_v4(&raw[..205]).is_err());
        let mut larger = raw.to_vec();
        larger.push(0);
        assert!(decode_material_challenge_history_v4(&larger).is_err());
    }
    #[test]
    fn network_actor_reorg_and_exhausted_sequence_cannot_rebind_an_old_prefix() {
        let network = Hash64::from_bytes([1; 64]);
        let actor = bond(2);
        let first = checkpoint(None, 1);
        let initial = PalwMaterialChallengeHistoryV4::next(network, actor, None, None, &first).unwrap();
        let second = checkpoint(Some(first), 2);
        let next = PalwMaterialChallengeHistoryV4::next(network, actor, Some(&first), Some(&initial), &second).unwrap();
        assert!(next.validate(network, &first).is_err());
        assert!(initial.validate(network, &second).is_err());
        let mut foreign_genesis = initial;
        foreign_genesis.previous = material_challenge_history_base_anchor_v4(network, bond(8), None, None).unwrap().root;
        foreign_genesis.root = advance_material_challenge_history_anchor_v4(
            network,
            actor,
            PalwMaterialChallengeHistoryAnchorV4 { sequence: 0, root: foreign_genesis.previous },
            first.statement,
        )
        .unwrap()
        .root;
        assert!(foreign_genesis.validate(network, &first).is_err());
        assert!(next.validate(Hash64::from_bytes([9; 64]), &second).is_err());
        assert!(material_challenge_history_base_anchor_v4(network, bond(8), Some(&second), Some(&next)).is_err());
        assert!(material_challenge_history_base_anchor_v4(network, bond(8), Some(&second), None).is_err());
        assert!(
            advance_material_challenge_history_anchor_v4(
                network,
                actor,
                PalwMaterialChallengeHistoryAnchorV4 { sequence: u64::MAX, root: initial.root },
                second.statement,
            )
            .is_err()
        );
    }
    #[test]
    fn cold_route_requires_matching_latest_checkpoint_and_rolls_back_together() {
        use crate::{
            palw_kernel_route_v1::palw_kernel_route_policy_v1,
            palw_material_challenge_checkpoint_v4::material_challenge_checkpoint_key_v4,
        };
        let network = Hash64::from_bytes([1; 64]);
        let actor = bond(2);
        let first = checkpoint(None, 1);
        let initial = PalwMaterialChallengeHistoryV4::next(network, actor, None, None, &first).unwrap();
        let second = checkpoint(Some(first), 2);
        let next = PalwMaterialChallengeHistoryV4::next(network, actor, Some(&first), Some(&initial), &second).unwrap();
        let mut route =
            PalwKernelRouteStateV1::new(palw_kernel_route_policy_v1(network, Hash64::from_bytes([8; 64])), None, Default::default());
        let key = (TABLE, material_challenge_history_key_v4(&actor).to_vec());
        let checkpoint_key = (TABLE, material_challenge_checkpoint_key_v4(&actor).to_vec());
        assert!(material_challenge_history_from_route_v4(&route, actor).unwrap().is_none());
        route.aux.insert(key.clone(), initial.encoded().unwrap().to_vec());
        assert!(material_challenge_history_from_route_v4(&route, actor).is_err());
        route.aux.insert(checkpoint_key.clone(), first.encoded().unwrap().to_vec());
        let cold: PalwKernelRouteStateV1 = borsh::from_slice(&borsh::to_vec(&route).unwrap()).unwrap();
        assert_eq!(material_challenge_history_from_route_v4(&cold, actor).unwrap(), Some(initial));
        route.aux.insert(key.clone(), next.encoded().unwrap().to_vec());
        assert!(material_challenge_history_from_route_v4(&route, actor).is_err());
        route.aux.insert(checkpoint_key.clone(), second.encoded().unwrap().to_vec());
        assert_eq!(material_challenge_history_from_route_v4(&route, actor).unwrap(), Some(next));
        route.aux.insert(key.clone(), initial.encoded().unwrap().to_vec());
        route.aux.insert(checkpoint_key, first.encoded().unwrap().to_vec());
        assert_eq!(material_challenge_history_from_route_v4(&route, actor).unwrap(), Some(initial));
        route.aux.insert((TABLE, material_challenge_history_key_v4(&bond(8)).to_vec()), initial.encoded().unwrap().to_vec());
        assert!(material_challenge_history_from_route_v4(&route, bond(8)).is_err());
        route.aux.insert(key, vec![0]);
        assert!(material_challenge_history_from_route_v4(&route, actor).is_err());
    }
}
