//! Correspondence for a PALW state item.
//!
//! Adding a field still touches genesis, carriage, restore and the resource account. This table is
//! the place that names those duties before the edit, and [`PalwStateItemDutyV1::gaps`] is the
//! check that a row which omits one of them is refused. The probe below is not a consensus field
//! and is not written into the carriage.

/// One row of the state-item correspondence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwStateItemDutyV1 {
    pub item: &'static str,
    pub owner: &'static str,
    pub primary_or_derived: &'static str,
    pub lifecycle: &'static str,
    pub writer_and_delta: &'static str,
    pub equal_update: &'static str,
    pub root_order: &'static str,
    pub wire_tag: &'static str,
    pub version: &'static str,
    pub restore: &'static str,
    pub rollback: &'static str,
    pub resource_estimate: &'static str,
    pub activation_clock: &'static str,
    pub fixture: &'static str,
}

impl PalwStateItemDutyV1 {
    /// Names of duties this row left empty. A shipped row has none.
    pub fn gaps(&self) -> Vec<&'static str> {
        let fields = [
            ("owner", self.owner),
            ("primary_or_derived", self.primary_or_derived),
            ("lifecycle", self.lifecycle),
            ("writer_and_delta", self.writer_and_delta),
            ("equal_update", self.equal_update),
            ("root_order", self.root_order),
            ("wire_tag", self.wire_tag),
            ("version", self.version),
            ("restore", self.restore),
            ("rollback", self.rollback),
            ("resource_estimate", self.resource_estimate),
            ("activation_clock", self.activation_clock),
            ("fixture", self.fixture),
        ];
        fields.into_iter().filter(|(_, value)| value.is_empty()).map(|(name, _)| name).collect()
    }
}

/// Epoch counters already on the carriage. The const-generic writer binds the map and the delta
/// variant together; this row is the place a later item is copied from.
pub const EPOCH_COUNTER_DUTY_V1: PalwStateItemDutyV1 = PalwStateItemDutyV1 {
    item: "epoch_counters",
    owner: "primary state; the deadline index is derived and is not in the delta",
    primary_or_derived: "primary",
    lifecycle: "create and update by write_epoch; delete by writing None; no retention expiry",
    writer_and_delta: "JournaledMapWriterV1 on epoch_counters records PalwDeltaEntryV2::Epoch",
    equal_update: "an equal value is still recorded; the writer does not skip it",
    root_order: "included in the carriage map order already pinned by PALW_STATE_V2_VERSION",
    wire_tag: "existing Epoch delta variant; no new tag",
    version: "carriage version 22",
    restore: "state to carriage and carriage to state through the existing epoch map",
    rollback: "delta apply and revert of PalwDeltaEntryV2::Epoch",
    resource_estimate: "the existing native row reservation of epoch_counters",
    activation_clock: "block time of the transition that writes the counter",
    fixture: "the epoch writer tests already in palw_state_v2",
};

/// Receipt epoch counters. Same key and value type as epoch counters, different map and delta.
pub const RECEIPT_EPOCH_COUNTER_DUTY_V1: PalwStateItemDutyV1 = PalwStateItemDutyV1 {
    item: "receipt_epoch_counters",
    owner: "primary state",
    primary_or_derived: "primary",
    lifecycle: "create and update by write_receipt_epoch; delete by writing None",
    writer_and_delta: "JournaledMapWriterV1 on receipt_epoch_counters records PalwDeltaEntryV2::ReceiptEpoch",
    equal_update: "an equal value is still recorded; the writer does not skip it",
    root_order: "included in the carriage map order already pinned by PALW_STATE_V2_VERSION",
    wire_tag: "existing ReceiptEpoch delta variant; no new tag",
    version: "carriage version 22",
    restore: "state to carriage and carriage to state through the existing receipt epoch map",
    rollback: "delta apply and revert of PalwDeltaEntryV2::ReceiptEpoch",
    resource_estimate: "the existing native row reservation of receipt_epoch_counters",
    activation_clock: "block time of the transition that writes the counter",
    fixture: "the receipt epoch writer tests already in palw_state_v2",
};

/// Rows a new consensus item is checked against. The probe used by the gap test is not here.
pub const SHIPPED_STATE_ITEM_DUTIES_V1: &[PalwStateItemDutyV1] = &[EPOCH_COUNTER_DUTY_V1, RECEIPT_EPOCH_COUNTER_DUTY_V1];

/// A row someone might add, with the rollback and root duties left blank. It is not shipped.
pub fn probe_state_item_missing_rollback_and_root_v1() -> PalwStateItemDutyV1 {
    PalwStateItemDutyV1 {
        item: "probe_not_a_consensus_field",
        owner: "not on the carriage",
        primary_or_derived: "neither; this probe is not stored",
        lifecycle: "none",
        writer_and_delta: "none",
        equal_update: "none",
        root_order: "",
        wire_tag: "none",
        version: "not a carriage version",
        restore: "none",
        rollback: "",
        resource_estimate: "none",
        activation_clock: "none",
        fixture: "this module's gap test",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_state_v2::{PALW_STATE_V2_VERSION, palw_carriage_version_refusal_v1};

    #[test]
    fn shipped_rows_name_every_duty_and_a_gap_is_reported() {
        for row in SHIPPED_STATE_ITEM_DUTIES_V1 {
            assert!(row.gaps().is_empty(), "{} left {}", row.item, row.gaps().join(", "));
        }
        let probe = probe_state_item_missing_rollback_and_root_v1();
        assert!(!SHIPPED_STATE_ITEM_DUTIES_V1.iter().any(|row| row.item == probe.item));
        let gaps = probe.gaps();
        assert!(gaps.contains(&"rollback"));
        assert!(gaps.contains(&"root_order"));
        assert_eq!(PALW_STATE_V2_VERSION, 22);
        assert!(palw_carriage_version_refusal_v1(&21u16.to_le_bytes()).is_some());
        assert!(palw_carriage_version_refusal_v1(&PALW_STATE_V2_VERSION.to_le_bytes()).is_none());
    }
}
