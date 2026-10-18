//! Versioned bootstrap cohort mechanics; no native authority or release approval.
//!
//! A native consumer must authenticate the frozen population, canonical future
//! anchor and policy against its own chain before using this selection. The
//! anchor's unpredictability/bias, honest locked-weight fraction, participation
//! and material availability are explicit external assumptions. These routines
//! establish none of them and prove no model computation.
//!
//! One cohort serves a finite epoch. No work/claim ID enters its draw. Repeated
//! work therefore cannot reroll the cohort, but its failures are correlated for
//! the whole epoch: per-work failure probabilities must not be multiplied as if
//! each work drew a new honest committee. Bond identities are not independence.
//! Slots sample locked-weight mass with replacement. A repeated checker may own
//! several slots; these are weighted voting positions, not independent people or
//! repeated computational checks. The native receipt must bind its slot index,
//! and payment must not multiply a check just because its signer owns more slots.
//! Splitting weight between frozen keys does not add total ticket mass; it does
//! not preserve the exact per-seed output and proves no controller independence.
use crate::hash::{Digest, h};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BootstrapPolicyV1 {
    pub version: u16,
    pub seats: u16,
    pub quorum: u16,
    pub maximum_population: u32,
    pub maximum_draw_attempts: u16,
    pub maximum_source_works: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapRefusalV1 {
    Policy,
    Epoch,
    Population,
    Overflow,
    Codec,
    DrawUnavailable { seat: u16 },
}
#[derive(Debug)]
pub enum BootstrapSelectionErrorV1<E> {
    Contract(BootstrapRefusalV1),
    Capacity(E),
}
impl BootstrapPolicyV1 {
    pub fn validate(&self) -> Result<(), BootstrapRefusalV1> {
        if self.version != 1
            || self.seats == 0
            || self.quorum == 0
            || self.quorum > self.seats
            || u32::from(self.quorum) * 2 <= u32::from(self.seats)
            || self.maximum_population == 0
            || self.maximum_draw_attempts == 0
            || self.maximum_source_works == 0
        {
            return Err(BootstrapRefusalV1::Policy);
        }
        Ok(())
    }
    pub fn id(&self) -> Result<Digest, BootstrapRefusalV1> {
        fixed_id::<16, _>(b"MISAKA/PALW/BOOTSTRAP-POLICY/V1", self)
    }
}

/// Identity of a candidate from the native, pre-anchor locked population.
/// A split bond does not assert a different controller or honest participant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BootstrapCandidateV1 {
    pub bond: Digest,
    pub locked_weight: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BootstrapEpochV1 {
    pub chain_genesis: Digest,
    pub ruleset: Digest,
    pub policy: Digest,
    pub population_root: Digest,
    pub anchor: Digest,
    pub epoch: u64,
    pub population_frozen_at: u64,
    pub anchor_position: u64,
    pub starts_at: u64,
    pub ends_at: u64,
}
impl BootstrapEpochV1 {
    pub fn id(&self) -> Result<Digest, BootstrapRefusalV1> {
        fixed_id::<360, _>(b"MISAKA/PALW/BOOTSTRAP-EPOCH/V1", self)
    }
    pub fn validate(&self, policy: &BootstrapPolicyV1) -> Result<(), BootstrapRefusalV1> {
        policy.validate()?;
        if self.policy != policy.id()?
            || [self.chain_genesis, self.ruleset, self.population_root, self.anchor].contains(&[0; 64])
            || self.population_frozen_at >= self.anchor_position
            || self.anchor_position >= self.starts_at
            || self.starts_at >= self.ends_at
        {
            return Err(BootstrapRefusalV1::Epoch);
        }
        Ok(())
    }
}
fn fixed_id<const N: usize, T: BorshSerialize>(domain: &[u8], value: &T) -> Result<Digest, BootstrapRefusalV1> {
    let mut bytes = [0u8; N];
    let mut output = bytes.as_mut_slice();
    value.serialize(&mut output).map_err(|_| BootstrapRefusalV1::Codec)?;
    if !output.is_empty() {
        return Err(BootstrapRefusalV1::Codec);
    }
    Ok(h(domain, &bytes))
}
/// Canonical sorted population identity, hashed directly without an encoded
/// population Vec. This checks shape; own native state must authenticate it.
pub fn bootstrap_population_root_v1(candidates: &[BootstrapCandidateV1]) -> Result<Digest, BootstrapRefusalV1> {
    let count = u32::try_from(candidates.len()).map_err(|_| BootstrapRefusalV1::Overflow)?;
    if count == 0
        || candidates.iter().any(|c| c.bond == [0; 64] || c.locked_weight == 0)
        || candidates.windows(2).any(|w| w[0].bond >= w[1].bond)
    {
        return Err(BootstrapRefusalV1::Population);
    }
    let length = u64::from(count).checked_mul(72).and_then(|n| n.checked_add(4)).ok_or(BootstrapRefusalV1::Overflow)?;
    let mut state = blake2b_simd::Params::new().hash_length(64).key(b"MISAKA/PALW/BOOTSTRAP-POPULATION/V1").to_state();
    state.update(&length.to_le_bytes());
    state.update(&count.to_le_bytes());
    for c in candidates {
        state.update(&c.bond);
        state.update(&c.locked_weight.to_le_bytes());
    }
    let mut root = [0u8; 64];
    root.copy_from_slice(state.finalize().as_bytes());
    Ok(root)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BootstrapSeatV1 {
    pub candidate_index: u32,
    pub bond: Digest,
    pub locked_weight: u64,
}
/// A deterministic selection with an allocation owner, not a verified approval.
/// Native quorum receipt admission and source lineage are separate consumers.
pub struct BootstrapCommitteeV1<O> {
    epoch_id: Digest,
    seats: Vec<BootstrapSeatV1>,
    _owner: O,
}
impl<O> BootstrapCommitteeV1<O> {
    pub fn epoch_id(&self) -> Digest {
        self.epoch_id
    }
    pub fn seats(&self) -> &[BootstrapSeatV1] {
        &self.seats
    }
}
pub fn bootstrap_committee_workspace_v1(policy: &BootstrapPolicyV1) -> Result<u64, BootstrapRefusalV1> {
    policy.validate()?;
    u64::from(policy.seats).checked_mul(std::mem::size_of::<BootstrapSeatV1>() as u64).ok_or(BootstrapRefusalV1::Overflow)
}
/// Locked-weight sampling with replacement, using bounded rejection for each
/// exact integer ticket. No fallback, changed anchor, or fresh work ID is used
/// if the fixed attempt bound is exhausted. Selection is conditional on the
/// stated entropy assumptions; this function does not attest that entropy.
pub fn select_bootstrap_committee_v1<O, E>(
    policy: &BootstrapPolicyV1,
    epoch: &BootstrapEpochV1,
    population: &[BootstrapCandidateV1],
    reserve: impl FnOnce(u64) -> Result<O, E>,
) -> Result<BootstrapCommitteeV1<O>, BootstrapSelectionErrorV1<E>> {
    select_with_tickets(policy, epoch, population, reserve, |id, seat, attempt| {
        let mut input = [0u8; 68];
        input[..64].copy_from_slice(id);
        input[64..66].copy_from_slice(&seat.to_le_bytes());
        input[66..].copy_from_slice(&attempt.to_le_bytes());
        let digest = h(b"MISAKA/PALW/BOOTSTRAP-TICKET/V1", &input);
        u128::from_le_bytes(digest[..16].try_into().expect("fixed ticket prefix"))
    })
}
fn select_with_tickets<O, E>(
    policy: &BootstrapPolicyV1,
    epoch: &BootstrapEpochV1,
    population: &[BootstrapCandidateV1],
    reserve: impl FnOnce(u64) -> Result<O, E>,
    mut ticket: impl FnMut(&Digest, u16, u16) -> u128,
) -> Result<BootstrapCommitteeV1<O>, BootstrapSelectionErrorV1<E>> {
    use BootstrapSelectionErrorV1::{Capacity, Contract};
    epoch.validate(policy).map_err(Contract)?;
    if population.len() > policy.maximum_population as usize {
        return Err(Contract(BootstrapRefusalV1::Population));
    }
    if bootstrap_population_root_v1(population).map_err(Contract)? != epoch.population_root {
        return Err(Contract(BootstrapRefusalV1::Population));
    }
    let epoch_id = epoch.id().map_err(Contract)?;
    let owner = reserve(bootstrap_committee_workspace_v1(policy).map_err(Contract)?).map_err(Capacity)?;
    let mut seats: Vec<BootstrapSeatV1> = Vec::with_capacity(usize::from(policy.seats));
    let total_weight = population
        .iter()
        .try_fold(0u128, |n, c| n.checked_add(u128::from(c.locked_weight)))
        .ok_or(Contract(BootstrapRefusalV1::Overflow))?;
    for seat in 0..policy.seats {
        let threshold = total_weight.wrapping_neg() % total_weight;
        let mut draw = None;
        for attempt in 0..policy.maximum_draw_attempts {
            let value = ticket(&epoch_id, seat, attempt);
            if value >= threshold {
                draw = Some(value % total_weight);
                break;
            }
        }
        let mut draw = draw.ok_or(Contract(BootstrapRefusalV1::DrawUnavailable { seat }))?;
        let mut chosen = None;
        for (index, c) in population.iter().enumerate() {
            if draw < u128::from(c.locked_weight) {
                chosen = Some((index, c));
                break;
            }
            draw -= u128::from(c.locked_weight);
        }
        let (index, c) = chosen.ok_or(Contract(BootstrapRefusalV1::Population))?;
        seats.push(BootstrapSeatV1 { candidate_index: index as u32, bond: c.bond, locked_weight: c.locked_weight });
    }
    Ok(BootstrapCommitteeV1 { epoch_id, seats, _owner: owner })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (BootstrapPolicyV1, BootstrapEpochV1, [BootstrapCandidateV1; 3]) {
        // Mechanics-only test values; no approved stake or bias parameters.
        let policy = BootstrapPolicyV1 {
            version: 1,
            seats: 2,
            quorum: 2,
            maximum_population: 3,
            maximum_draw_attempts: 2,
            maximum_source_works: 4,
        };
        let population = [
            BootstrapCandidateV1 { bond: [1; 64], locked_weight: 1 },
            BootstrapCandidateV1 { bond: [2; 64], locked_weight: 2 },
            BootstrapCandidateV1 { bond: [3; 64], locked_weight: 3 },
        ];
        let epoch = BootstrapEpochV1 {
            chain_genesis: [4; 64],
            ruleset: [5; 64],
            policy: policy.id().unwrap(),
            population_root: bootstrap_population_root_v1(&population).unwrap(),
            anchor: [6; 64],
            epoch: 7,
            population_frozen_at: 10,
            anchor_position: 20,
            starts_at: 30,
            ends_at: 40,
        };
        (policy, epoch, population)
    }
    #[test]
    fn identities_keep_exact_borsh_framing_and_bound_network_policy_and_population() {
        let (policy, epoch, population) = fixture();
        assert_eq!(policy.id().unwrap(), crate::hash::object_id(b"MISAKA/PALW/BOOTSTRAP-POLICY/V1", &policy));
        assert_eq!(epoch.id().unwrap(), crate::hash::object_id(b"MISAKA/PALW/BOOTSTRAP-EPOCH/V1", &epoch));
        assert_eq!(epoch.population_root, crate::hash::object_id(b"MISAKA/PALW/BOOTSTRAP-POPULATION/V1", &population.to_vec()));
        let mut other = epoch;
        other.chain_genesis = [9; 64];
        assert_ne!(other.id().unwrap(), epoch.id().unwrap());
        let first = select_bootstrap_committee_v1(&policy, &epoch, &population, |_| Ok::<_, ()>(())).unwrap();
        let again = select_bootstrap_committee_v1(&policy, &epoch, &population, |_| Ok::<_, ()>(())).unwrap();
        assert_eq!(first.epoch_id(), again.epoch_id());
        assert_eq!(first.seats(), again.seats(), "no work/claim attempt parameter can reroll this epoch");
        let mut changed = population;
        changed[0].locked_weight += 1;
        assert!(matches!(
            select_bootstrap_committee_v1(&policy, &epoch, &changed, |_| Ok::<_, ()>(())),
            Err(BootstrapSelectionErrorV1::Contract(BootstrapRefusalV1::Population))
        ));
    }
    #[test]
    fn rejection_has_no_modulo_or_anchor_fallback_and_repeated_slots_keep_their_weight() {
        let (policy, epoch, population) = fixture();
        assert!(matches!(
            select_with_tickets(&policy, &epoch, &population, |_| Ok::<_, ()>(()), |_, _, _| 0),
            Err(BootstrapSelectionErrorV1::Contract(BootstrapRefusalV1::DrawUnavailable { seat: 0 }))
        ));
        let chosen = select_with_tickets(&policy, &epoch, &population, |_| Ok::<_, ()>(()), |_, _, _| 4).unwrap();
        assert_eq!(chosen.seats().iter().map(|s| s.candidate_index).collect::<Vec<_>>(), vec![2, 2]);
        let mut late = epoch;
        late.population_frozen_at = late.anchor_position;
        assert_eq!(late.validate(&policy), Err(BootstrapRefusalV1::Epoch));
    }
    #[test]
    fn splitting_frozen_keys_does_not_add_coalition_ticket_mass() {
        let (policy, mut epoch, _) = fixture();
        let joined =
            [BootstrapCandidateV1 { bond: [1; 64], locked_weight: 3 }, BootstrapCandidateV1 { bond: [3; 64], locked_weight: 3 }];
        let split = [
            BootstrapCandidateV1 { bond: [1; 64], locked_weight: 1 },
            BootstrapCandidateV1 { bond: [2; 64], locked_weight: 2 },
            BootstrapCandidateV1 { bond: [3; 64], locked_weight: 3 },
        ];
        let mut captured = [0usize; 2];
        // Exactly one full accepted residue cycle for each slot, not a
        // statistical experiment or a statement about actual anchor entropy.
        for (case, population) in [joined.as_slice(), split.as_slice()].into_iter().enumerate() {
            epoch.population_root = bootstrap_population_root_v1(population).unwrap();
            for a in 4..10 {
                for b in 4..10 {
                    let committee = select_with_tickets(
                        &policy,
                        &epoch,
                        population,
                        |_| Ok::<_, ()>(()),
                        |_, slot, _| if slot == 0 { a } else { b },
                    )
                    .unwrap();
                    if committee.seats().iter().all(|s| s.bond != [3; 64]) {
                        captured[case] += 1;
                    }
                }
            }
        }
        assert_eq!(captured, [9, 9], "distinct key count cannot become extra independent stake");
    }
    #[test]
    fn refusal_precedes_draw_and_selected_storage_retains_its_owner() {
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        struct Owner(Arc<AtomicUsize>);
        impl Drop for Owner {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let (policy, epoch, population) = fixture();
        let mut draws = 0;
        let result = select_with_tickets(
            &policy,
            &epoch,
            &population,
            |_| Err::<(), _>("local capacity"),
            |_, _, _| {
                draws += 1;
                4
            },
        );
        assert!(matches!(result, Err(BootstrapSelectionErrorV1::Capacity("local capacity"))));
        assert_eq!(draws, 0);
        let dropped = Arc::new(AtomicUsize::new(0));
        let committee = select_with_tickets(
            &policy,
            &epoch,
            &population,
            |bytes| {
                assert_eq!(bytes, 2 * std::mem::size_of::<BootstrapSeatV1>() as u64);
                Ok::<_, ()>(Owner(dropped.clone()))
            },
            |_, _, _| 4,
        )
        .unwrap();
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        drop(committee);
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }
}
