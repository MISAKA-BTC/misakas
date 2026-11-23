//! RFC-0061 §5. Resolve Q from the entire canonical preparation cut. The
//! owning reader supplies the VSS outcomes and signed Ready rows
//! at that cut, and resolves the reserved resources. This projection
//! verifies the holder signatures but does not implement the private VSS protocol.
use super::{Tb1Field, Tb1PolynomialError, check_qualified, check_roster};
use super::{
    freeze::{Tb1MiningPurpose, Tb1MiningRound},
    signatures,
};
use crate::{
    Hash64,
    block::Block,
    coinbase::{coinbase_payload_extra_data, palw_coinbase_marker},
};
use libcrux_ml_dsa::ml_dsa_65::MLDSA65VerificationKey;
use sha3::{Digest, Sha3_512};
use std::{collections::BTreeSet, io};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tb1VssEnd {
    pub transcript_root: [u8; 64],
    pub end_anchor: [u8; 64],
}

/// Position in the owning profile's canonical preparation order (including
/// intra-block order). This is a resolved ordering value, not a wire field.
#[derive(Clone, Copy)]
pub struct Tb1DealerEnd {
    pub end: Tb1VssEnd,
    pub position: u64,
}

#[derive(Clone, Copy)]
pub struct Tb1Ready<'a> {
    pub round_ref: &'a [u8],
    pub dealer_slot: usize,
    pub holder_slot: usize,
    pub signature: &'a [u8],
    pub position: u64,
    pub end: Tb1VssEnd,
}

/// All terminal dealer rows come from the specified round's native cut, in
/// roster order. None means the dealer did not finish VSS successfully there.
/// Ready rows carry their actual acceptance positions in this same ordering.
pub struct Tb1PreparationCut<'a> {
    pub round_ref: &'a [u8],
    pub deadline_position: u64,
    /// Last position covered by the authenticated preparation read, inclusive.
    pub observed_through: u64,
    pub dealer_ends: &'a [Option<Tb1DealerEnd>],
    pub ready: &'a [Tb1Ready<'a>],
}

/// Coordinates and registered ML-DSA-65 keys in the frozen native roster's slot order.
#[derive(Clone, Copy)]
pub struct Tb1Roster<'a> {
    pub coordinates: &'a [Tb1Field],
    pub holder_keys: &'a [MLDSA65VerificationKey],
    pub f: usize,
}

impl Tb1Roster<'_> {
    pub fn root(self) -> Result<[u8; 64], Tb1PolynomialError> {
        check_roster(self.coordinates, self.f)?;
        if self.holder_keys.len() != self.coordinates.len()
            || self.holder_keys.iter().map(|key| key.as_ref()).collect::<BTreeSet<_>>().len() != self.coordinates.len()
        {
            return Err(Tb1PolynomialError::Roster);
        }
        let mut hash = Sha3_512::new();
        hash.update(b"MISAKA/private/TB1/roster/v1");
        hash.update(u32::try_from(self.coordinates.len()).map_err(|_| Tb1PolynomialError::Roster)?.to_be_bytes());
        for (x, key) in self.coordinates.iter().zip(self.holder_keys) {
            hash.update(x.to_bytes());
            hash.update(key.as_slice());
        }
        Ok(hash.finalize().into())
    }
}

pub struct Tb1Qualification<'a> {
    pub(super) round_ref: &'a [u8],
    pub(super) qual_root: [u8; 64],
    pub(super) roster: Tb1Roster<'a>,
    pub(super) preparation_deadline: u64,
    pub(super) dealers: &'a [usize],
}

impl<'a> Tb1Qualification<'a> {
    /// Compare the authenticated proposal with every dealer that reached q
    /// distinct holders for its exact VSS end. Resolve the entire cut or fail.
    pub fn resolve(
        roster: Tb1Roster<'a>,
        round_ref: &'a [u8],
        resources_ref: [u8; 64],
        dealers: &'a [usize],
        cut: &Tb1PreparationCut<'_>,
        max_ready_records: usize,
        max_message_bytes: usize,
    ) -> Result<Self, Tb1PolynomialError> {
        use Tb1PolynomialError::*;
        if cut.ready.len() > max_ready_records {
            return Err(PreparationBudget);
        }
        let Tb1Roster { coordinates, holder_keys, f } = roster;
        let roster_root = roster.root()?;
        check_qualified(coordinates, f, dealers)?;
        if round_ref.is_empty()
            || cut.round_ref != round_ref
            || cut.observed_through < cut.deadline_position
            || cut.dealer_ends.len() != coordinates.len()
        {
            return Err(PreparationContext);
        }
        let mut supporters = BTreeSet::new();
        let mut counts = vec![0usize; coordinates.len()];
        for row in cut.ready {
            let Some(Some(dealer)) = cut.dealer_ends.get(row.dealer_slot) else { continue };
            if row.round_ref == round_ref
                && row.holder_slot < coordinates.len()
                && row.end == dealer.end
                && dealer.position < row.position
                && row.position <= cut.deadline_position
                && !supporters.contains(&(row.dealer_slot, row.holder_slot))
                && signatures::ready_message(row, max_message_bytes).is_some_and(|msg| {
                    signatures::verify(&holder_keys[row.holder_slot], row.signature, &msg, signatures::READY_CONTEXT)
                })
            {
                supporters.insert((row.dealer_slot, row.holder_slot));
                counts[row.dealer_slot] += 1;
            }
        }
        let q = coordinates.len() - f;
        let complete = counts.iter().enumerate().filter_map(|(i, &count)| (count >= q).then_some(i));
        if !complete.eq(dealers.iter().copied()) {
            return Err(QualifiedSet);
        }
        // Hash the semantic preparation records in native slot order. Retransmissions
        // and ML-DSA signing randomness do not give the sender another beacon.
        let width = |n: usize| u32::try_from(n).map(u32::to_be_bytes).map_err(|_| PreparationBudget);
        let mut digest = Sha3_512::new();
        digest.update(b"MISAKA/private/TB1/Qualification/v1");
        digest.update(width(round_ref.len())?);
        digest.update(round_ref);
        digest.update(resources_ref);
        digest.update(cut.deadline_position.to_be_bytes());
        digest.update(roster_root);
        for end in cut.dealer_ends {
            match end.filter(|end| end.position <= cut.deadline_position) {
                Some(end) => {
                    digest.update([1]);
                    digest.update(end.position.to_be_bytes());
                    digest.update(end.end.transcript_root);
                    digest.update(end.end.end_anchor);
                }
                None => digest.update([0]),
            }
        }
        digest.update(width(supporters.len())?);
        for (dealer, holder) in supporters {
            digest.update(width(dealer)?);
            digest.update(width(holder)?);
        }
        digest.update(width(dealers.len())?);
        for &dealer in dealers {
            digest.update(width(dealer)?);
        }
        let qual_root = digest.finalize().into();
        Ok(Self { round_ref, qual_root, roster, preparation_deadline: cut.deadline_position, dealers })
    }

    pub fn roster(&self) -> Tb1Roster<'_> {
        self.roster
    }

    pub fn round_ref(&self) -> &[u8] {
        self.round_ref
    }

    pub fn root(&self) -> [u8; 64] {
        self.qual_root
    }
}

pub fn check_panel_schedule(
    purpose: Tb1MiningPurpose,
    first: &Tb1MiningRound,
    first_q: &Tb1Qualification<'_>,
    panel: &Tb1MiningRound,
    panel_q: &Tb1Qualification<'_>,
) -> io::Result<()> {
    if first.purpose != purpose
        || panel.purpose != Tb1MiningPurpose::Panel
        || first.chain_id != panel.chain_id
        || first.close_blue_score >= panel.close_blue_score
        || first.release_from_daa >= panel.release_from_daa
    {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid private TB1 round pair"));
    }
    for (round, q) in [(first, first_q), (panel, panel_q)] {
        if q.round_ref() != round.id()
            || q.roster().root().ok() != Some(round.roster_root)
            || round.release_from_daa >= round.deadline_exclusive_daa
        {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "qualification does not match the scheduled round"));
        }
    }
    Ok(())
}

/// The two signed qualification cuts of the private input profile. Dealer
/// termination and resource references are inputs to qualification. The
/// commitment authenticates those inputs as of the preparation block.
#[derive(Clone, Copy)]
pub struct QualifiedExecPanel {
    exec: Tb1MiningRound,
    panel: Tb1MiningRound,
    exec_qual: [u8; 64],
    panel_qual: [u8; 64],
    marker: [u8; 64],
}

#[derive(Clone, borsh::BorshSerialize, borsh::BorshDeserialize)]
struct RetainedRound {
    round: Tb1MiningRound,
    root: [u8; 64],
    coordinates: Vec<[u8; 66]>,
    holder_keys: Vec<[u8; 1952]>,
    faults: u32,
    deadline_position: u64,
    dealers: Vec<u32>,
}

impl RetainedRound {
    fn capture(round: Tb1MiningRound, qualification: &Tb1Qualification<'_>) -> io::Result<Self> {
        let width = |v| u32::try_from(v).map_err(io::Error::other);
        Ok(Self {
            round,
            root: qualification.qual_root,
            coordinates: qualification.roster.coordinates.iter().map(|coordinate| coordinate.to_bytes()).collect(),
            holder_keys: qualification
                .roster
                .holder_keys
                .iter()
                .map(|key| key.as_slice().try_into().expect("ML-DSA-65 verification key width"))
                .collect(),
            faults: width(qualification.roster.f)?,
            deadline_position: qualification.preparation_deadline,
            dealers: qualification.dealers.iter().copied().map(width).collect::<Result<_, _>>()?,
        })
    }

    fn restore(self) -> Option<PreparedTb1Round> {
        let coordinates = self.coordinates.iter().map(Tb1Field::from_bytes).collect::<Option<Vec<_>>>()?;
        let holder_keys = self.holder_keys.into_iter().map(MLDSA65VerificationKey::new).collect();
        let dealers = self.dealers.into_iter().map(|slot| slot as usize).collect();
        Some(PreparedTb1Round {
            round: self.round,
            id: self.round.id(),
            root: self.root,
            coordinates,
            holder_keys,
            faults: self.faults as usize,
            deadline_position: self.deadline_position,
            dealers,
        })
    }
}

/// An authenticated preparation restored from the selected branch's state.
pub struct PreparedTb1Round {
    round: Tb1MiningRound,
    id: [u8; 64],
    root: [u8; 64],
    coordinates: Vec<Tb1Field>,
    holder_keys: Vec<MLDSA65VerificationKey>,
    faults: usize,
    deadline_position: u64,
    dealers: Vec<usize>,
}

impl PreparedTb1Round {
    pub fn round(&self) -> Tb1MiningRound {
        self.round
    }

    pub fn qualification(&self) -> Tb1Qualification<'_> {
        Tb1Qualification {
            round_ref: &self.id,
            qual_root: self.root,
            roster: Tb1Roster { coordinates: &self.coordinates, holder_keys: &self.holder_keys, f: self.faults },
            preparation_deadline: self.deadline_position,
            dealers: &self.dealers,
        }
    }
}

#[derive(Clone, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub(crate) struct RetainedExecPanelPreparation {
    exec: RetainedRound,
    panel: RetainedRound,
}

use crate::palw_native_resource_v1::wire::wire_struct;
wire_struct!(RetainedRound; Tb1MiningRound, [u8; 64], Vec<[u8; 66]>, Vec<[u8; 1952]>, u32, u64, Vec<u32>);
wire_struct!(RetainedExecPanelPreparation; RetainedRound, RetainedRound);

impl RetainedExecPanelPreparation {
    pub(crate) fn restore(self) -> Option<(PreparedTb1Round, PreparedTb1Round)> {
        Some((self.exec.restore()?, self.panel.restore()?))
    }
}

/// The owner supplies the two authenticated cuts resolved against their
/// frozen rosters. The candidate must commit the exact qualification pair.
#[derive(Clone)]
pub struct PreparedExecPanelCandidate {
    pub(crate) block: Hash64,
    pub(crate) parent: Hash64,
    pub(crate) rounds: RetainedExecPanelPreparation,
}

impl PreparedExecPanelCandidate {
    pub fn verify(
        chain: [u8; 64],
        parent: Hash64,
        candidate: &Block,
        exec: Tb1MiningRound,
        exec_q: &Tb1Qualification<'_>,
        panel: Tb1MiningRound,
        panel_q: &Tb1Qualification<'_>,
    ) -> io::Result<Self> {
        let rounds = QualifiedExecPanel::new(exec, exec_q, panel, panel_q)?;
        let marker = candidate
            .transactions
            .first()
            .filter(|tx| tx.is_coinbase())
            .and_then(|tx| coinbase_payload_extra_data(&tx.payload))
            .and_then(palw_coinbase_marker);
        if chain != exec.chain_id
            || candidate.header.blue_score >= exec.close_blue_score
            || candidate.header.daa_score >= exec.release_from_daa
            || marker != Some(rounds.commitment())
        {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "missing joint qualification before EXEC close"));
        }
        Ok(Self {
            block: candidate.hash(),
            parent,
            rounds: RetainedExecPanelPreparation {
                exec: RetainedRound::capture(exec, exec_q)?,
                panel: RetainedRound::capture(panel, panel_q)?,
            },
        })
    }
}

impl QualifiedExecPanel {
    pub fn new(
        exec: Tb1MiningRound,
        exec_q: &Tb1Qualification<'_>,
        panel: Tb1MiningRound,
        panel_q: &Tb1Qualification<'_>,
    ) -> io::Result<Self> {
        check_panel_schedule(Tb1MiningPurpose::Exec, &exec, exec_q, &panel, panel_q)?;
        let marker = Sha3_512::new()
            .chain_update(b"MISAKA/private/TB1/qualified-exec-panel/v1")
            .chain_update(exec.id())
            .chain_update(exec_q.root())
            .chain_update(panel.id())
            .chain_update(panel_q.root())
            .finalize()
            .into();
        Ok(Self { exec, panel, exec_qual: exec_q.root(), panel_qual: panel_q.root(), marker })
    }

    pub fn exec(&self) -> &Tb1MiningRound {
        &self.exec
    }

    pub fn panel(&self) -> &Tb1MiningRound {
        &self.panel
    }

    pub fn exec_qual(&self) -> [u8; 64] {
        self.exec_qual
    }

    pub fn panel_qual(&self) -> [u8; 64] {
        self.panel_qual
    }

    pub fn commitment(&self) -> [u8; 64] {
        self.marker
    }
}
