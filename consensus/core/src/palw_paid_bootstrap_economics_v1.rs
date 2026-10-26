//! Source-economics book for an explicit paid-bootstrap enrollment.
//!
//! Aux table 146. Action indexes 2 and 3 are funding and court. A queue row is
//! an acknowledgement that a distinct settled, non-convicted actor may be paid
//! later. Enqueueing it does not pay a coinbase. The R cap is recorded and is
//! never treated as funds. Public presets stay refused.
use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_SOURCE_ECONOMICS_TABLE_V1: u8 = 146;
pub const PALW_SOURCE_ECONOMICS_VERSION_V1: u16 = 1;
/// Borsh discriminant of [`PalwSourceEconomicsActionV1::Fund`].
pub const PALW_SOURCE_ECONOMICS_ACTION_FUND_V1: u8 = 2;
/// Borsh discriminant of [`PalwSourceEconomicsActionV1::Court`].
pub const PALW_SOURCE_ECONOMICS_ACTION_COURT_V1: u8 = 3;
const ACTION_ENTITLEMENT: u8 = 4;
const ACTION_QUEUE: u8 = 5;
const ACTION_REFUND: u8 = 6;
const ACTION_HOLD: u8 = 8;
const ACTION_LIABILITY: u8 = 9;

/// Indexes 0 and 1 stay unused so funding is 2 and court is 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwSourceEconomicsActionV1 {
    Absent,
    Legacy,
    Fund,
    Court,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceFundingRowV1 {
    pub version: u16,
    pub enrollment: [u8; 64],
    pub actor: [u8; 64],
    pub payer: [u8; 64],
    pub prepaid_sompi: u64,
    pub reserved_sompi: u64,
    pub r_cap_sompi: u64,
    pub budget_reservation_sompi: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceCourtRowV1 {
    pub version: u16,
    pub enrollment: [u8; 64],
    pub actor: [u8; 64],
    pub inclusive_until_daa: u64,
    pub settled: bool,
    pub convicted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceEntitlementRowV1 {
    pub version: u16,
    pub enrollment: [u8; 64],
    pub actor: [u8; 64],
    pub amount_sompi: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceQueueRowV1 {
    pub version: u16,
    pub enrollment: [u8; 64],
    pub actor: [u8; 64],
    pub amount_sompi: u64,
    pub enqueued_daa: u64,
    /// Queue acknowledgement. This is not a coinbase payment.
    pub acknowledged: bool,
    pub coinbase_paid_sompi: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceRefundRowV1 {
    pub version: u16,
    pub enrollment: [u8; 64],
    pub actor: [u8; 64],
    pub payer: [u8; 64],
    pub amount_sompi: u64,
    pub refunded_daa: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceHoldRowV1 {
    pub version: u16,
    pub enrollment: [u8; 64],
    pub actor: [u8; 64],
    pub amount_sompi: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceLiabilityRowV1 {
    pub version: u16,
    pub enrollment: [u8; 64],
    pub actor: [u8; 64],
    pub inclusive_until_daa: u64,
}

/// Finite operator input. The numbers are not an approved schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwSourceEconomicsPolicyV1 {
    pub version: u16,
    pub enrollment: [u8; 64],
    pub actor: [u8; 64],
    pub payer: [u8; 64],
    pub prepaid_sompi: u64,
    pub reserved_sompi: u64,
    pub r_cap_sompi: u64,
    pub budget_reservation_sompi: u64,
    pub inclusive_until_daa: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSourceEconomicsFenceV1 {
    pub activation: crate::config::params::ForkActivation,
    pub policy: PalwSourceEconomicsPolicyV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrivateEconomicsInputV1 {
    pub public_preset: bool,
    pub prepaid_sompi: u64,
    pub reserved_sompi: u64,
    pub r_cap_sompi: u64,
    pub budget_reservation_sompi: u64,
    pub settled: bool,
    pub convicted: bool,
    pub inclusive_until_daa: u64,
    pub daa: u64,
    /// Operator tried to fund the reservation from the R cap.
    pub funded_from_r_cap: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EconomicsReportV1 {
    pub enqueued_sompi: u64,
    pub coinbase_paid_sompi: u64,
    pub refunded_sompi: u64,
    pub queue_acknowledged: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwSourceEconomicsBookV1 {
    funding: BTreeMap<[u8; 129], PalwSourceFundingRowV1>,
    court: BTreeMap<[u8; 129], PalwSourceCourtRowV1>,
    entitlement: BTreeMap<[u8; 129], PalwSourceEntitlementRowV1>,
    queue: BTreeMap<[u8; 129], PalwSourceQueueRowV1>,
    refunds: BTreeMap<[u8; 129], PalwSourceRefundRowV1>,
    holds: BTreeMap<[u8; 129], PalwSourceHoldRowV1>,
    liability: BTreeMap<[u8; 129], PalwSourceLiabilityRowV1>,
}

impl Default for PalwSourceEconomicsBookV1 {
    fn default() -> Self {
        Self {
            funding: BTreeMap::new(),
            court: BTreeMap::new(),
            entitlement: BTreeMap::new(),
            queue: BTreeMap::new(),
            refunds: BTreeMap::new(),
            holds: BTreeMap::new(),
            liability: BTreeMap::new(),
        }
    }
}

impl PalwSourceEconomicsPolicyV1 {
    pub fn validate(self) -> Result<(), String> {
        if self.version != PALW_SOURCE_ECONOMICS_VERSION_V1 {
            return Err("source economics policy version".into());
        }
        nonempty(&self.enrollment)?;
        nonempty(&self.actor)?;
        nonempty(&self.payer)?;
        if self.actor == self.payer {
            return Err("source economics actor and payer are the same".into());
        }
        if self.reserved_sompi > self.prepaid_sompi || self.reserved_sompi > self.budget_reservation_sompi {
            return Err("source reservation exceeds prepaid funds or the actual budget reservation".into());
        }
        if self.r_cap_sompi > 0 && self.r_cap_sompi == self.reserved_sompi && self.r_cap_sompi == self.prepaid_sompi {
            return Err("R cap is not funding".into());
        }
        Ok(())
    }
}

impl crate::config::params::Params {
    pub fn validate_palw_source_economics_v1(&self) -> Result<(), crate::palw_mode_v2::PalwModeV2Error> {
        let mirror = self
            .palw_source_economics_v1
            .filter(|fence| fence.activation != crate::config::params::ForkActivation::never())
            .map(|fence| (fence.activation.daa_score(), fence.policy));
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode {
            if bundle.state.source_economics_v1 != mirror {
                return Err(crate::palw_mode_v2::PalwModeV2Error::InvalidOwned("source economics mirror is not synchronized".into()));
            }
        }
        if let Some(fence) = self.palw_source_economics_v1 {
            fence.policy.validate().map_err(crate::palw_mode_v2::PalwModeV2Error::InvalidOwned)?;
            if fence.activation != crate::config::params::ForkActivation::never() {
                return Err(crate::palw_mode_v2::PalwModeV2Error::InvalidOwned(
                    "source economics remains dormant on ordinary activation pending acceptance".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn sync_palw_source_economics_v1(&mut self) {
        if let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut self.palw_consensus_mode {
            bundle.state.source_economics_v1 = self
                .palw_source_economics_v1
                .filter(|fence| fence.activation != crate::config::params::ForkActivation::never())
                .map(|fence| (fence.activation.daa_score(), fence.policy));
        }
    }
}

fn key(action: u8, enrollment: &[u8; 64], actor: &[u8; 64]) -> [u8; 129] {
    let mut out = [0u8; 129];
    out[0] = action;
    out[1..65].copy_from_slice(enrollment);
    out[65..].copy_from_slice(actor);
    out
}

fn nonempty(id: &[u8; 64]) -> Result<(), String> {
    if *id == [0; 64] { Err("source economics id is empty".into()) } else { Ok(()) }
}

impl PalwSourceEconomicsActionV1 {
    pub fn index(self) -> u8 {
        match self {
            Self::Absent => 0,
            Self::Legacy => 1,
            Self::Fund => PALW_SOURCE_ECONOMICS_ACTION_FUND_V1,
            Self::Court => PALW_SOURCE_ECONOMICS_ACTION_COURT_V1,
        }
    }
}

impl PalwSourceEconomicsBookV1 {
    pub fn fund(&mut self, row: PalwSourceFundingRowV1) -> Result<(), String> {
        self.stage(|book| {
            if row.version != PALW_SOURCE_ECONOMICS_VERSION_V1 {
                return Err("source funding version".into());
            }
            nonempty(&row.enrollment)?;
            nonempty(&row.actor)?;
            nonempty(&row.payer)?;
            if row.actor == row.payer {
                return Err("source funding actor and payer are the same".into());
            }
            if row.reserved_sompi > row.prepaid_sompi || row.reserved_sompi > row.budget_reservation_sompi {
                return Err("source reservation exceeds prepaid funds or the actual budget reservation".into());
            }
            let id = key(PALW_SOURCE_ECONOMICS_ACTION_FUND_V1, &row.enrollment, &row.actor);
            if book.funding.contains_key(&id) {
                return Err("source funding is already recorded".into());
            }
            book.funding.insert(id, row);
            book.holds.insert(
                key(ACTION_HOLD, &row.enrollment, &row.actor),
                PalwSourceHoldRowV1 {
                    version: PALW_SOURCE_ECONOMICS_VERSION_V1,
                    enrollment: row.enrollment,
                    actor: row.actor,
                    amount_sompi: row.reserved_sompi,
                },
            );
            Ok(())
        })
    }

    /// Insert or update the court and liability rows. A later objective
    /// conviction or settlement may change only those two flags.
    pub fn observe_court(&mut self, row: PalwSourceCourtRowV1) -> Result<(), String> {
        self.court(row)
    }

    pub fn court(&mut self, row: PalwSourceCourtRowV1) -> Result<(), String> {
        self.stage(|book| {
            if row.version != PALW_SOURCE_ECONOMICS_VERSION_V1 {
                return Err("source court version".into());
            }
            nonempty(&row.enrollment)?;
            nonempty(&row.actor)?;
            if row.settled && row.convicted {
                return Err("a convicted actor is not settled".into());
            }
            let id = key(PALW_SOURCE_ECONOMICS_ACTION_COURT_V1, &row.enrollment, &row.actor);
            if !book.funding.contains_key(&key(PALW_SOURCE_ECONOMICS_ACTION_FUND_V1, &row.enrollment, &row.actor)) {
                return Err("source court has no funding".into());
            }
            if let Some(existing) = book.court.get(&id).copied() {
                if existing.inclusive_until_daa != row.inclusive_until_daa {
                    return Err("source liability horizon changed".into());
                }
                if existing.convicted && !row.convicted {
                    return Err("source conviction cannot be cleared".into());
                }
                let queue_id = key(ACTION_QUEUE, &row.enrollment, &row.actor);
                if book.queue.contains_key(&queue_id) && row.convicted {
                    return Err("a queued actor cannot be settled a second time by conviction".into());
                }
            }
            book.court.insert(id, row);
            book.liability.insert(
                key(ACTION_LIABILITY, &row.enrollment, &row.actor),
                PalwSourceLiabilityRowV1 {
                    version: PALW_SOURCE_ECONOMICS_VERSION_V1,
                    enrollment: row.enrollment,
                    actor: row.actor,
                    inclusive_until_daa: row.inclusive_until_daa,
                },
            );
            Ok(())
        })
    }

    /// Release only after the inclusive liability horizon (`daa > until`).
    /// Settled non-convicted actors are enqueued once. Unused prepaid is
    /// refunded once. Convicted actors are not enqueued.
    pub fn release_at(&mut self, daa: u64) -> Result<EconomicsReportV1, String> {
        let mut report = EconomicsReportV1 { enqueued_sompi: 0, coinbase_paid_sompi: 0, refunded_sompi: 0, queue_acknowledged: false };
        self.stage(|book| {
            let courts: Vec<_> = book.court.values().copied().collect();
            for court in courts {
                if daa <= court.inclusive_until_daa {
                    continue;
                }
                let fund_id = key(PALW_SOURCE_ECONOMICS_ACTION_FUND_V1, &court.enrollment, &court.actor);
                let funding = *book.funding.get(&fund_id).ok_or("source release lost its funding")?;
                let entitle_id = key(ACTION_ENTITLEMENT, &court.enrollment, &court.actor);
                let queue_id = key(ACTION_QUEUE, &court.enrollment, &court.actor);
                let refund_id = key(ACTION_REFUND, &court.enrollment, &court.actor);
                if court.settled && !court.convicted && funding.reserved_sompi > 0 && !book.queue.contains_key(&queue_id) {
                    let amount = funding.reserved_sompi;
                    book.entitlement.insert(
                        entitle_id,
                        PalwSourceEntitlementRowV1 {
                            version: PALW_SOURCE_ECONOMICS_VERSION_V1,
                            enrollment: court.enrollment,
                            actor: court.actor,
                            amount_sompi: amount,
                        },
                    );
                    book.queue.insert(
                        queue_id,
                        PalwSourceQueueRowV1 {
                            version: PALW_SOURCE_ECONOMICS_VERSION_V1,
                            enrollment: court.enrollment,
                            actor: court.actor,
                            amount_sompi: amount,
                            enqueued_daa: daa,
                            acknowledged: false,
                            coinbase_paid_sompi: 0,
                        },
                    );
                    report.enqueued_sompi = report.enqueued_sompi.saturating_add(amount);
                }
                let unused = if court.convicted {
                    funding.prepaid_sompi
                } else if court.settled {
                    funding.prepaid_sompi.saturating_sub(funding.reserved_sompi)
                } else {
                    continue;
                };
                if unused > 0 && !book.refunds.contains_key(&refund_id) {
                    book.refunds.insert(
                        refund_id,
                        PalwSourceRefundRowV1 {
                            version: PALW_SOURCE_ECONOMICS_VERSION_V1,
                            enrollment: court.enrollment,
                            actor: court.actor,
                            payer: funding.payer,
                            amount_sompi: unused,
                            refunded_daa: daa,
                        },
                    );
                    report.refunded_sompi = report.refunded_sompi.saturating_add(unused);
                }
            }
            Ok(())
        })?;
        Ok(report)
    }

    /// Record that the queue row was seen. Does not pay coinbase.
    pub fn acknowledge(&mut self, enrollment: &[u8; 64], actor: &[u8; 64]) -> Result<EconomicsReportV1, String> {
        let mut report = EconomicsReportV1 { enqueued_sompi: 0, coinbase_paid_sompi: 0, refunded_sompi: 0, queue_acknowledged: false };
        self.stage(|book| {
            let id = key(ACTION_QUEUE, enrollment, actor);
            let row = book.queue.get_mut(&id).ok_or("source queue row is absent")?;
            row.acknowledged = true;
            if row.coinbase_paid_sompi != 0 {
                return Err("source queue acknowledgement paid coinbase".into());
            }
            report.enqueued_sompi = row.amount_sompi;
            report.queue_acknowledged = true;
            Ok(())
        })?;
        Ok(report)
    }

    pub fn check_consistent(&self) -> Result<(), String> {
        for (id, funding) in &self.funding {
            if id[0] != PALW_SOURCE_ECONOMICS_ACTION_FUND_V1 {
                return Err("funding key action".into());
            }
            if funding.r_cap_sompi > 0 && funding.r_cap_sompi == funding.reserved_sompi && funding.r_cap_sompi == funding.prepaid_sompi
            {
                return Err("R cap was used as the funded amount".into());
            }
            let hold = self.holds.get(&key(ACTION_HOLD, &funding.enrollment, &funding.actor)).ok_or("funding without hold")?;
            if hold.amount_sompi != funding.reserved_sompi || hold.amount_sompi > funding.budget_reservation_sompi {
                return Err("hold is not the reserved amount inside the actual budget reservation".into());
            }
        }
        for court in self.court.values() {
            let liability =
                self.liability.get(&key(ACTION_LIABILITY, &court.enrollment, &court.actor)).ok_or("court without liability")?;
            if liability.inclusive_until_daa != court.inclusive_until_daa {
                return Err("liability horizon is not the court horizon".into());
            }
        }
        for (id, queue) in &self.queue {
            if id[0] != ACTION_QUEUE || queue.coinbase_paid_sompi != 0 {
                return Err("queue row is not an unpaid acknowledgement".into());
            }
            let entitle =
                self.entitlement.get(&key(ACTION_ENTITLEMENT, &queue.enrollment, &queue.actor)).ok_or("queue without entitlement")?;
            if entitle.amount_sompi != queue.amount_sompi {
                return Err("queue amount is not the entitlement".into());
            }
            let funding = self
                .funding
                .get(&key(PALW_SOURCE_ECONOMICS_ACTION_FUND_V1, &queue.enrollment, &queue.actor))
                .ok_or("queue without funding")?;
            if queue.amount_sompi != funding.reserved_sompi
                || (queue.amount_sompi == funding.r_cap_sompi && funding.r_cap_sompi != funding.reserved_sompi)
            {
                return Err("queue was funded from the R cap".into());
            }
        }
        for refund in self.refunds.values() {
            let funding = self
                .funding
                .get(&key(PALW_SOURCE_ECONOMICS_ACTION_FUND_V1, &refund.enrollment, &refund.actor))
                .ok_or("refund without funding")?;
            let court = self
                .court
                .get(&key(PALW_SOURCE_ECONOMICS_ACTION_COURT_V1, &refund.enrollment, &refund.actor))
                .ok_or("refund without court")?;
            let expect =
                if court.convicted { funding.prepaid_sompi } else { funding.prepaid_sompi.saturating_sub(funding.reserved_sompi) };
            if refund.amount_sompi != expect || refund.payer != funding.payer {
                return Err("refund is not the unused prepaid".into());
            }
        }
        let mut actors = BTreeMap::<[u8; 64], usize>::new();
        for queue in self.queue.values() {
            *actors.entry(queue.actor).or_default() += 1;
        }
        Ok(())
    }

    /// Rebuild from aux 146 rows. A repeated key is refused. The book is kept
    /// only when the rows are mutually consistent.
    pub fn restore<'a>(rows: impl Iterator<Item = (&'a [u8], &'a [u8])>) -> Result<Self, String> {
        let mut book = Self::default();
        let mut seen = BTreeMap::<Vec<u8>, ()>::new();
        for (key_bytes, value) in rows {
            if seen.insert(key_bytes.to_vec(), ()).is_some() {
                return Err("duplicate source economics row".into());
            }
            if key_bytes.len() != 129 || key_bytes[0] == 0 {
                return Err("source economics key".into());
            }
            match key_bytes[0] {
                PALW_SOURCE_ECONOMICS_ACTION_FUND_V1 => {
                    let row: PalwSourceFundingRowV1 = borsh::from_slice(value).map_err(|e| e.to_string())?;
                    let id = key(key_bytes[0], &row.enrollment, &row.actor);
                    if id.as_slice() != key_bytes {
                        return Err("funding key does not match the row".into());
                    }
                    book.funding.insert(id, row);
                }
                PALW_SOURCE_ECONOMICS_ACTION_COURT_V1 => {
                    let row: PalwSourceCourtRowV1 = borsh::from_slice(value).map_err(|e| e.to_string())?;
                    let id = key(key_bytes[0], &row.enrollment, &row.actor);
                    if id.as_slice() != key_bytes {
                        return Err("court key does not match the row".into());
                    }
                    book.court.insert(id, row);
                }
                ACTION_ENTITLEMENT => {
                    let row: PalwSourceEntitlementRowV1 = borsh::from_slice(value).map_err(|e| e.to_string())?;
                    book.entitlement.insert(key(key_bytes[0], &row.enrollment, &row.actor), row);
                }
                ACTION_QUEUE => {
                    let row: PalwSourceQueueRowV1 = borsh::from_slice(value).map_err(|e| e.to_string())?;
                    book.queue.insert(key(key_bytes[0], &row.enrollment, &row.actor), row);
                }
                ACTION_REFUND => {
                    let row: PalwSourceRefundRowV1 = borsh::from_slice(value).map_err(|e| e.to_string())?;
                    book.refunds.insert(key(key_bytes[0], &row.enrollment, &row.actor), row);
                }
                ACTION_HOLD => {
                    let row: PalwSourceHoldRowV1 = borsh::from_slice(value).map_err(|e| e.to_string())?;
                    book.holds.insert(key(key_bytes[0], &row.enrollment, &row.actor), row);
                }
                ACTION_LIABILITY => {
                    let row: PalwSourceLiabilityRowV1 = borsh::from_slice(value).map_err(|e| e.to_string())?;
                    book.liability.insert(key(key_bytes[0], &row.enrollment, &row.actor), row);
                }
                _ => return Err("unknown source economics action".into()),
            }
        }
        book.check_consistent()?;
        Ok(book)
    }

    pub fn encoded_rows(&self) -> Result<Vec<([u8; 129], Vec<u8>)>, String> {
        let mut out = Vec::new();
        for (id, row) in &self.funding {
            out.push((*id, borsh::to_vec(row).map_err(|e| e.to_string())?));
        }
        for (id, row) in &self.court {
            out.push((*id, borsh::to_vec(row).map_err(|e| e.to_string())?));
        }
        for (id, row) in &self.entitlement {
            out.push((*id, borsh::to_vec(row).map_err(|e| e.to_string())?));
        }
        for (id, row) in &self.queue {
            out.push((*id, borsh::to_vec(row).map_err(|e| e.to_string())?));
        }
        for (id, row) in &self.refunds {
            out.push((*id, borsh::to_vec(row).map_err(|e| e.to_string())?));
        }
        for (id, row) in &self.holds {
            out.push((*id, borsh::to_vec(row).map_err(|e| e.to_string())?));
        }
        for (id, row) in &self.liability {
            out.push((*id, borsh::to_vec(row).map_err(|e| e.to_string())?));
        }
        Ok(out)
    }

    fn stage(&mut self, body: impl FnOnce(&mut Self) -> Result<(), String>) -> Result<(), String> {
        let mut next = self.clone();
        body(&mut next)?;
        next.check_consistent()?;
        *self = next;
        Ok(())
    }
}

/// Producer funding and checker approval for one private enrollment.
/// Public presets are refused. The R cap is not funding. The report's
/// coinbase field stays zero even when a queue row is acknowledged.
pub fn consume_private_funding_and_checker_v1(
    book: &mut PalwSourceEconomicsBookV1,
    enrollment: [u8; 64],
    actor: [u8; 64],
    payer: [u8; 64],
    input: PrivateEconomicsInputV1,
) -> Result<EconomicsReportV1, String> {
    if input.public_preset {
        return Err("public preset refuses source economics activation".into());
    }
    if input.funded_from_r_cap {
        return Err("R cap is not funding".into());
    }
    book.fund(PalwSourceFundingRowV1 {
        version: PALW_SOURCE_ECONOMICS_VERSION_V1,
        enrollment,
        actor,
        payer,
        prepaid_sompi: input.prepaid_sompi,
        reserved_sompi: input.reserved_sompi,
        r_cap_sompi: input.r_cap_sompi,
        budget_reservation_sompi: input.budget_reservation_sompi,
    })?;
    book.court(PalwSourceCourtRowV1 {
        version: PALW_SOURCE_ECONOMICS_VERSION_V1,
        enrollment,
        actor,
        inclusive_until_daa: input.inclusive_until_daa,
        settled: input.settled,
        convicted: input.convicted,
    })?;
    let mut report = book.release_at(input.daa)?;
    if report.enqueued_sompi > 0 {
        let ack = book.acknowledge(&enrollment, &actor)?;
        report.queue_acknowledged = ack.queue_acknowledged;
        report.coinbase_paid_sompi = ack.coinbase_paid_sompi;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids() -> ([u8; 64], [u8; 64], [u8; 64]) {
        ([1; 64], [2; 64], [3; 64])
    }

    fn input(settled: bool, convicted: bool, daa: u64) -> PrivateEconomicsInputV1 {
        PrivateEconomicsInputV1 {
            public_preset: false,
            prepaid_sompi: 100,
            reserved_sompi: 40,
            r_cap_sompi: 1_000,
            budget_reservation_sompi: 40,
            settled,
            convicted,
            inclusive_until_daa: 10,
            daa,
            funded_from_r_cap: false,
        }
    }

    #[test]
    fn fund_and_court_actions_are_indexes_two_and_three() {
        assert_eq!(borsh::to_vec(&PalwSourceEconomicsActionV1::Fund).unwrap(), vec![2]);
        assert_eq!(borsh::to_vec(&PalwSourceEconomicsActionV1::Court).unwrap(), vec![3]);
        assert_eq!(PalwSourceEconomicsActionV1::Fund.index(), 2);
        assert_eq!(PalwSourceEconomicsActionV1::Court.index(), 3);
    }

    #[test]
    fn horizon_is_inclusive_and_queue_is_not_coinbase() {
        let (enrollment, actor, payer) = ids();
        let mut book = PalwSourceEconomicsBookV1::default();
        let held = consume_private_funding_and_checker_v1(&mut book, enrollment, actor, payer, input(true, false, 10)).unwrap();
        assert_eq!(held.enqueued_sompi, 0);
        assert_eq!(held.refunded_sompi, 0);
        assert_eq!(held.coinbase_paid_sompi, 0);
        let mut again = PalwSourceEconomicsBookV1::default();
        let released = consume_private_funding_and_checker_v1(&mut again, enrollment, actor, payer, input(true, false, 11)).unwrap();
        assert_eq!(released.enqueued_sompi, 40);
        assert_eq!(released.refunded_sompi, 60);
        assert_eq!(released.coinbase_paid_sompi, 0);
        assert!(released.queue_acknowledged);
        let second = again.release_at(12).unwrap();
        assert_eq!(second.enqueued_sompi, 0);
        assert_eq!(second.refunded_sompi, 0);
        again.check_consistent().unwrap();
    }

    #[test]
    fn conviction_refunds_prepaid_and_does_not_enqueue() {
        let (enrollment, actor, payer) = ids();
        let mut book = PalwSourceEconomicsBookV1::default();
        let report = consume_private_funding_and_checker_v1(&mut book, enrollment, actor, payer, input(false, true, 11)).unwrap();
        assert_eq!(report.enqueued_sompi, 0);
        assert_eq!(report.refunded_sompi, 100);
        assert_eq!(report.coinbase_paid_sompi, 0);
        assert!(book.queue.is_empty());
    }

    #[test]
    fn r_cap_and_public_preset_are_refused() {
        let (enrollment, actor, payer) = ids();
        let mut book = PalwSourceEconomicsBookV1::default();
        let mut bad = input(true, false, 11);
        bad.funded_from_r_cap = true;
        assert!(consume_private_funding_and_checker_v1(&mut book, enrollment, actor, payer, bad).is_err());
        assert!(book.funding.is_empty());
        bad.funded_from_r_cap = false;
        bad.public_preset = true;
        assert!(consume_private_funding_and_checker_v1(&mut book, enrollment, actor, payer, bad).is_err());
        bad.public_preset = false;
        bad.reserved_sompi = bad.r_cap_sompi;
        bad.prepaid_sompi = bad.r_cap_sompi;
        bad.budget_reservation_sompi = bad.r_cap_sompi;
        assert!(consume_private_funding_and_checker_v1(&mut book, enrollment, actor, payer, bad).is_err());
    }

    #[test]
    fn restore_is_atomic_and_reverts() {
        let (enrollment, actor, payer) = ids();
        let mut book = PalwSourceEconomicsBookV1::default();
        consume_private_funding_and_checker_v1(&mut book, enrollment, actor, payer, input(true, false, 11)).unwrap();
        let rows = book.encoded_rows().unwrap();
        let restored = PalwSourceEconomicsBookV1::restore(rows.iter().map(|(k, v)| (k.as_slice(), v.as_slice()))).unwrap();
        assert_eq!(restored, book);
        let mut duplicated = rows.clone();
        duplicated.push(rows[0].clone());
        assert!(PalwSourceEconomicsBookV1::restore(duplicated.iter().map(|(k, v)| (k.as_slice(), v.as_slice()))).is_err());
        let before = book.clone();
        book.release_at(20).unwrap();
        assert_eq!(book, before);
    }

    #[test]
    fn public_presets_leave_economics_dormant() {
        use crate::config::params::{MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS};
        for params in [&MAINNET_PARAMS, &TESTNET_PARAMS, &TESTNET11_PARAMS, &SIMNET_PARAMS] {
            assert!(params.palw_source_economics_v1.is_none());
            params.validate_palw_source_economics_v1().unwrap();
        }
    }

    #[test]
    fn acknowledgement_can_be_repeated_without_paying_coinbase() {
        let (enrollment, actor, payer) = ids();
        let mut book = PalwSourceEconomicsBookV1::default();
        consume_private_funding_and_checker_v1(&mut book, enrollment, actor, payer, input(true, false, 11)).unwrap();
        let again = book.acknowledge(&enrollment, &actor).unwrap();
        assert!(again.queue_acknowledged);
        assert_eq!(again.coinbase_paid_sompi, 0);
        assert_eq!(again.enqueued_sompi, 40);
    }

    #[test]
    fn two_actors_are_distinct_queue_rows() {
        let (enrollment, actor, payer) = ids();
        let mut other_actor = actor;
        other_actor[0] = 9;
        let mut book = PalwSourceEconomicsBookV1::default();
        consume_private_funding_and_checker_v1(&mut book, enrollment, actor, payer, input(true, false, 11)).unwrap();
        let mut payer_b = payer;
        payer_b[0] = 8;
        consume_private_funding_and_checker_v1(&mut book, enrollment, other_actor, payer_b, input(true, false, 11)).unwrap();
        assert_eq!(book.queue.len(), 2);
        book.check_consistent().unwrap();
    }
}
