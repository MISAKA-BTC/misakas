//! Exact applied-statement checkpoint for a challenger's single in-flight WAL.
//! One fixed row per existing native bond survives material/root retirement.
//! It authenticates application of a signed availability obligation, never
//! computation correctness, fee payment, validator approval or finality.
use crate::{
    Hash64, palw_kernel_material_query_v4::PalwKernelMaterialUnitQueryV4 as Query, palw_kernel_route_v1::PalwKernelRouteStateV1,
    palw_material_release_recovery_v2::PALW_MATERIAL_RELEASE_RECOVERY_TABLE_V2 as TABLE,
    palw_monetary_material_v1::PalwMonetaryMaterialActionV1 as Action, palw_state_v2::PalwBondKeyV2,
};

pub const PALW_MATERIAL_CHALLENGE_CHECKPOINT_PREFIX_V4: u8 = 0xA2;
pub const PALW_MATERIAL_CHALLENGE_CHECKPOINT_KEY_BYTES_V4: usize = 69;
pub const PALW_MATERIAL_CHALLENGE_CHECKPOINT_BYTES_V4: usize = 433;

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwMaterialChallengeCheckpointV4 {
    pub version: u16,
    pub challenger: PalwBondKeyV2,
    pub sequence: u64,
    pub created_daa: u64,
    pub applied_daa: u64,
    pub query: crate::palw_kernel_material_query_v4::PalwKernelMaterialUnitQueryV4,
    pub valid_until_daa: u64,
    pub statement: Hash64,
}

pub fn material_challenge_checkpoint_key_v4(bond: &PalwBondKeyV2) -> [u8; PALW_MATERIAL_CHALLENGE_CHECKPOINT_KEY_BYTES_V4] {
    let mut key = [0; PALW_MATERIAL_CHALLENGE_CHECKPOINT_KEY_BYTES_V4];
    key[0] = PALW_MATERIAL_CHALLENGE_CHECKPOINT_PREFIX_V4;
    borsh::to_writer(&mut &mut key[1..], bond).expect("fixed native bond encoding fits its checkpoint key");
    key
}

fn subject(action: &Action) -> Result<(Query, u64), &'static str> {
    let Action::ChallengeCurrentV4 { root, consent, provider, dependency, stage, position, valid_until_daa } = action else {
        return Err("checkpoint requires an explicitly generation-bound challenge");
    };
    Ok((
        Query {
            version: 4,
            root: *root,
            consent: *consent,
            dependency: *dependency,
            provider: *provider,
            stage: *stage,
            position: *position,
        },
        *valid_until_daa,
    ))
}

fn statement(network: Hash64, challenger: PalwBondKeyV2, query: &Query, until: u64) -> Hash64 {
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
    let mut sink = Stream(blake2b_simd::Params::new().hash_length(64).key(b"MISAKA/PALW/APPLIED-MATERIAL-CHALLENGE/V4").to_state());
    borsh::to_writer(&mut sink, &(network, challenger, query, until)).expect("fixed challenge identity stream cannot fail");
    Hash64::from_bytes(sink.0.finalize().as_bytes().try_into().expect("64-byte digest"))
}

impl PalwMaterialChallengeCheckpointV4 {
    pub fn next(
        previous: Option<Self>,
        network: Hash64,
        challenger: PalwBondKeyV2,
        action: &Action,
        now: u64,
    ) -> Result<Self, &'static str> {
        let (query, until) = subject(action)?;
        let (created_daa, sequence) = if let Some(old) = previous {
            old.validate(network)?;
            if old.challenger != challenger || old.applied_daa > now {
                return Err("challenge checkpoint actor/clock differs");
            }
            (old.created_daa, old.sequence.checked_add(1).ok_or("challenge checkpoint sequence exhausted")?)
        } else {
            (now, 1)
        };
        let value = Self {
            version: 4,
            challenger,
            sequence,
            created_daa,
            applied_daa: now,
            query,
            valid_until_daa: until,
            statement: statement(network, challenger, &query, until),
        };
        value.validate(network)?;
        Ok(value)
    }
    pub fn validate(&self, network: Hash64) -> Result<(), &'static str> {
        if self.version != 4
            || self.sequence == 0
            || self.created_daa > self.applied_daa
            || self.applied_daa > self.valid_until_daa
            || self.query.version != 4
            || [self.query.root, self.query.consent, self.query.dependency].contains(&Hash64::default())
            || self.challenger == self.query.provider
            || self.statement != statement(network, self.challenger, &self.query, self.valid_until_daa)
        {
            return Err("challenge checkpoint version/subject/clock differs");
        }
        Ok(())
    }
    pub fn matches(&self, network: Hash64, challenger: PalwBondKeyV2, action: &Action) -> Result<bool, &'static str> {
        self.validate(network)?;
        let (query, until) = subject(action)?;
        Ok(self.challenger == challenger
            && self.query == query
            && self.valid_until_daa == until
            && self.statement == statement(network, challenger, &query, until))
    }
    pub fn encoded(&self) -> Result<[u8; PALW_MATERIAL_CHALLENGE_CHECKPOINT_BYTES_V4], &'static str> {
        let mut bytes = [0; PALW_MATERIAL_CHALLENGE_CHECKPOINT_BYTES_V4];
        let mut remaining = bytes.as_mut_slice();
        borsh::to_writer(&mut remaining, self).map_err(|_| "challenge checkpoint fixed encoding extent differs")?;
        if !remaining.is_empty() {
            return Err("challenge checkpoint fixed encoding has unused bytes");
        }
        Ok(bytes)
    }
}

/// Own accepted route only. Absence is unknown, not an accepted statement; a
/// superseding same-key operation does not authorize rebinding an older WAL.
pub fn material_challenge_checkpoint_from_route_v4(
    route: &PalwKernelRouteStateV1,
    actor: PalwBondKeyV2,
) -> Result<Option<PalwMaterialChallengeCheckpointV4>, &'static str> {
    let key = material_challenge_checkpoint_key_v4(&actor);
    let Some(raw) = route.aux.iter().find(|((table, k), _)| *table == TABLE && k.as_slice() == key.as_slice()).map(|(_, raw)| raw)
    else {
        return Ok(None);
    };
    let row = decode_material_challenge_checkpoint_v4(raw)?;
    row.validate(Hash64::from_bytes(route.header.policy.network_domain))?;
    if row.challenger != actor {
        return Err("challenge checkpoint key substitutes actor");
    }
    Ok(Some(row))
}

/// All fields are fixed sized; decode never constructs a Vec/map or history.
pub(crate) fn decode_material_challenge_checkpoint_v4(raw: &[u8]) -> Result<PalwMaterialChallengeCheckpointV4, &'static str> {
    if raw.len() != PALW_MATERIAL_CHALLENGE_CHECKPOINT_BYTES_V4 {
        return Err("challenge checkpoint fixed row extent differs");
    }
    borsh::from_slice(raw).map_err(|_| "challenge checkpoint fixed row does not decode")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_bytes([n; 64]), 0))
    }
    fn action(consent: u8) -> Action {
        Action::ChallengeCurrentV4 {
            root: Hash64::from_bytes([2; 64]),
            consent: Hash64::from_bytes([consent; 64]),
            provider: bond(3),
            dependency: Hash64::from_bytes([4; 64]),
            stage: 255,
            position: 7,
            valid_until_daa: 100,
        }
    }
    #[test]
    fn exact_subject_has_fixed_codec_and_cannot_match_other_consent_deadline_actor_or_network() {
        let network = Hash64::from_bytes([1; 64]);
        let a = action(5);
        let first = PalwMaterialChallengeCheckpointV4::next(None, network, bond(6), &a, 10).unwrap();
        let raw = first.encoded().unwrap();
        assert_eq!(borsh::to_vec(&first).unwrap(), raw);
        assert_eq!(decode_material_challenge_checkpoint_v4(&raw).unwrap(), first);
        assert_eq!(material_challenge_checkpoint_key_v4(&bond(6)).len(), 69);
        assert!(first.matches(network, bond(6), &a).unwrap());
        assert!(!first.matches(network, bond(6), &action(7)).unwrap());
        let mut changed = a.clone();
        let Action::ChallengeCurrentV4 { valid_until_daa, .. } = &mut changed else { unreachable!() };
        *valid_until_daa += 1;
        assert!(!first.matches(network, bond(6), &changed).unwrap());
        assert!(!first.matches(network, bond(8), &a).unwrap());
        assert!(first.matches(Hash64::from_bytes([9; 64]), bond(6), &a).is_err());
        assert!(decode_material_challenge_checkpoint_v4(&raw[..raw.len() - 1]).is_err());
        let next = PalwMaterialChallengeCheckpointV4::next(Some(first), network, bond(6), &action(7), 20).unwrap();
        assert_eq!((next.created_daa, next.applied_daa, next.sequence), (10, 20, 2));
        assert!(!next.matches(network, bond(6), &a).unwrap());
        assert!(PalwMaterialChallengeCheckpointV4::next(Some(next), network, bond(6), &a, 19).is_err());
        let mut exhausted = next;
        exhausted.sequence = u64::MAX;
        assert!(PalwMaterialChallengeCheckpointV4::next(Some(exhausted), network, bond(6), &a, 21).is_err());
        let Action::ChallengeCurrentV4 { root, provider, dependency, stage, position, valid_until_daa, .. } = a else {
            unreachable!()
        };
        let legacy = Action::Challenge { root, provider, dependency, stage, position, valid_until_daa };
        assert!(PalwMaterialChallengeCheckpointV4::next(None, network, bond(6), &legacy, 10).is_err());
    }
    #[test]
    fn current_root_checkpoint_survives_cold_route_without_live_material_and_rolls_back_as_a_row() {
        use crate::palw_kernel_route_v1::palw_kernel_route_policy_v1;
        let network = Hash64::from_bytes([1; 64]);
        let actor = bond(6);
        let first = PalwMaterialChallengeCheckpointV4::next(None, network, actor, &action(5), 10).unwrap();
        let key = (TABLE, material_challenge_checkpoint_key_v4(&actor).to_vec());
        let mut route =
            PalwKernelRouteStateV1::new(palw_kernel_route_policy_v1(network, Hash64::from_bytes([9; 64])), None, Default::default());
        assert!(material_challenge_checkpoint_from_route_v4(&route, actor).unwrap().is_none());
        route.aux.insert(key.clone(), first.encoded().unwrap().to_vec());
        let cold: PalwKernelRouteStateV1 = borsh::from_slice(&borsh::to_vec(&route).unwrap()).unwrap();
        assert_eq!(material_challenge_checkpoint_from_route_v4(&cold, actor).unwrap(), Some(first));
        let second = PalwMaterialChallengeCheckpointV4::next(Some(first), network, actor, &action(7), 20).unwrap();
        route.aux.insert(key.clone(), second.encoded().unwrap().to_vec());
        assert_eq!(material_challenge_checkpoint_from_route_v4(&route, actor).unwrap(), Some(second));
        route.aux.insert(key.clone(), first.encoded().unwrap().to_vec());
        assert_eq!(material_challenge_checkpoint_from_route_v4(&route, actor).unwrap(), Some(first));
        route.aux.insert((TABLE, material_challenge_checkpoint_key_v4(&bond(8)).to_vec()), first.encoded().unwrap().to_vec());
        assert!(material_challenge_checkpoint_from_route_v4(&route, bond(8)).is_err());
        route.aux.insert(key, vec![0]);
        assert!(material_challenge_checkpoint_from_route_v4(&route, actor).is_err());
    }
}
