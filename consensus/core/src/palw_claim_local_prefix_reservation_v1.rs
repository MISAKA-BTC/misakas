//! Prospective RFC0040 source obligations backed by the native bond budget.
//! A separate zero-credit reservation survives the source claim reaching Final.
use super::palw_bond_budget_fold_v1::PalwBudgetGrowthV1;
use super::*;
use crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportReservationV1;
use crate::palw_claim_local_v1::{PALW_CLAIM_LOCAL_TABLE_V1, decode_claim_local_value_v1};

fn bad(message: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(message.into())
}
pub(super) fn row_key(id: Hash64) -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = b'E';
    key[1..].copy_from_slice(id.as_byte_slice());
    key
}
pub(super) fn expiry_key(until: u64, id: Hash64) -> [u8; 73] {
    let mut key = [0; 73];
    key[0] = b'X';
    key[1..9].copy_from_slice(&until.to_be_bytes());
    key[9..].copy_from_slice(id.as_byte_slice());
    key
}
pub(super) fn reservation_id(point: &PalwBlockContextV2, claim: Hash64) -> Hash64 {
    Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
        b"misaka-palw/prefix-export/reservation/v1",
        &(point.block, point.daa_score, point.blue_score, claim),
    ))
}

pub(super) fn admit(builder: &mut TransitionBuilder<'_>, point: &PalwBlockContextV2, id: Hash64) -> Result<(), PalwStateV2Error> {
    if !builder.params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(());
    }
    let Some(envelope) = builder.extras.prefix_dependencies_v1.get(&id) else { return Ok(()) };
    let claim = builder.state.claims.get(&id).ok_or_else(|| bad("prefix source or consumer was not admitted"))?;
    if claim.accepted_daa != point.daa_score || claim.accepted_blue_score != point.blue_score || claim.accepted_block != point.block {
        return Err(bad("prefix obligation may only be installed at its actual source birth"));
    }
    if envelope.consumed.as_ref().is_some_and(|c| c.intent.material_until < claim.trace_retention_daa) {
        return Err(bad("prefix consumer actual retention exceeds signed upstream reservation"));
    }
    if envelope.exports.is_empty() {
        return Ok(());
    }
    let liability_until = envelope.exports.iter().map(|e| e.intent.liability_until).max().expect("nonempty");
    let material_until = envelope.exports.iter().map(|e| e.intent.material_until).max().expect("nonempty");
    let policy = builder
        .params
        .econ_contracts_v1
        .horizon_extension_at(point.daa_score)
        .ok_or_else(|| bad("prefix export needs finite native horizon policy"))?;
    policy.validate().map_err(bad)?;
    let maximum =
        point.daa_score.checked_add(policy.maximum_claim_horizon_daa).ok_or(PalwStateV2Error::Overflow("prefix export horizon"))?;
    if material_until > maximum || liability_until > material_until || material_until == u64::MAX {
        return Err(bad("prefix export exceeds finite policy horizon"));
    }
    let claim = builder.state.claims.get(&id).ok_or_else(|| bad("prefix export source was not admitted"))?;
    if claim.trace_retention_daa < material_until {
        return Err(bad("prefix export exceeds actual signed material retention"));
    }
    let class_id = claim.class_id;
    let bond_key = claim.bond;
    let producer = builder.state.bonds.get(&bond_key).map(|bond| bond.pubkey.clone()).unwrap_or_default();
    for export in &envelope.exports {
        if producer.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || export.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
        {
            return Err(bad("prefix export signature is missing"));
        }
        export.intent.check_v1().map_err(|_| bad("prefix export contract is incomplete"))?;
        if export.intent.base_commitment != id || export.intent.prefix.class_id != class_id {
            return Err(bad("prefix export is not bound to its admitted source birth"));
        }
        export.intent.verify_signature_v1(&producer, &export.signature)
            .map_err(|_| bad("prefix export signature differs from the admitted producer"))?;
    }
    let claim = builder.state.claims.get(&id).ok_or_else(|| bad("prefix export source was not admitted"))?;
    let liability_sompi = builder.bond_formula_liability_v2(point, claim);
    if liability_sompi == 0 || liability_sompi == u128::MAX {
        return Err(bad("prefix export liability is unsourced"));
    }
    // Budget accounting alone does not reserve the common PALW principal.
    // All work/withdrawal gates share committed_at, which includes existing E rows.
    if liability_sompi > builder.gate_room(&claim.bond, point.daa_score, PalwRcoreGateV1::Work) {
        return Err(bad("prefix export exceeds uncommitted producer collateral"));
    }
    let budget_id = reservation_id(point, id);
    let ask = crate::palw_bond_budget_v1::PalwBudgetAskV1 { block_units: 0, reward_sompi: 0, final_weight: 0, liability_sompi };
    let plan = builder
        .plan_bond_budget_v1(point, claim.bond, None, ask, crate::palw_bond_budget_v1::PalwBudgetOriginV1::FreePrompt, 0)?
        .ok_or_else(|| bad("prefix export requires native collateral reservation"))?;
    let row = PalwPrefixExportReservationV1 {
        source_claim: id,
        point: *point,
        bond: claim.bond,
        budget_id,
        liability_until,
        material_until,
        liability_sompi,
        convicted_daa: None,
    };
    // Block extras carry the route header. They are the in-force stand-in;
    // a missing header is an admission refusal, and a present header is installed here.
    if builder.extras.kernel_route.is_none() {
        return Err(bad("prefix export requires its admitted route header"));
    }
    super::palw_kernel_route_fold_v1::ensure_route_header(builder, point)?;
    builder.commit_bond_budget_v1(point, budget_id, plan)?;
    builder.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &row_key(budget_id), Some(&row))?;
    builder.write_kernel_row_encoded_v1(PALW_CLAIM_LOCAL_TABLE_V1, &expiry_key(material_until, budget_id), Some(&1u8))?;
    Ok(())
}

impl PalwChainStateV2 {
    /// Prospective source export capital is independent of the source's own
    /// claim reservation, and remains committed through its signed material tail.
    pub fn prefix_export_reserved_at_checked_v1(
        &self,
        bond: &PalwBondKeyV2,
        now_daa: u64,
    ) -> Result<u128, PalwStateV2Error> {
        let Some(route) = &self.kernel_route else { return Ok(0) };
        let mut total = 0u128;
        for ((table, key), bytes) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'E') { continue; }
            let row = decode_claim_local_value_v1::<PalwPrefixExportReservationV1>(bytes).map_err(|e| match e {
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => bad("malformed prefix capital reservation"),
            })?;
            if key.len() != 65 || key[1..] != *row.budget_id.as_byte_slice()
                || row.budget_id != reservation_id(&row.point, row.source_claim)
                || row.liability_until > row.material_until {
                return Err(bad("prefix capital reservation identity differs"));
            }
            if row.bond == *bond && now_daa <= row.material_until {
                total = total.checked_add(row.liability_sompi)
                    .ok_or(PalwStateV2Error::Overflow("prefix export committed collateral"))?;
            }
        }
        Ok(total)
    }

    /// Compatibility with the common infallible headroom reader: failure must
    /// consume all headroom, never silently make corrupt obligations spendable.
    pub fn prefix_export_reserved_at_v1(&self, bond: &PalwBondKeyV2, now_daa: u64) -> u128 {
        self.prefix_export_reserved_at_checked_v1(bond, now_daa).unwrap_or(u128::MAX)
    }

    pub(crate) fn prefix_export_reservation_v1(
        &self,
        point: &PalwBlockContextV2,
        claim: Hash64,
    ) -> Result<Option<PalwPrefixExportReservationV1>, PalwStateV2Error> {
        let id = reservation_id(point, claim);
        let key = row_key(id);
        let Some(bytes) = self.kernel_route.as_ref().and_then(|route| {
            route.aux.iter().find_map(|((table, k), v)| (*table == PALW_CLAIM_LOCAL_TABLE_V1 && k.as_slice() == key).then_some(v))
        }) else {
            return Ok(None);
        };
        let row = decode_claim_local_value_v1::<PalwPrefixExportReservationV1>(bytes).map_err(|e| match e {
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => bad("malformed prefix reservation"),
        })?;
        if row.source_claim != claim || row.point != *point || row.budget_id != id || row.liability_until > row.material_until {
            return Err(bad("prefix reservation changed its original source"));
        }
        // A recorded conviction stays readable after the source claim closes.
        // Dependent credit uses this flag; the budget row is not that record.
        if row.convicted_daa.is_some() {
            return Ok(Some(*row));
        }
        if self.last_point.is_some_and(|p| p.daa_score <= row.material_until) {
            let budget = self
                .bond_budget
                .as_ref()
                .and_then(|b| b.claim_row(&id))
                .ok_or_else(|| bad("prefix source lost its actual collateral reservation"))?;
            if !budget.open
                || budget.bond != row.bond
                || budget.liability_sompi != row.liability_sompi
                || budget.accepted_daa != row.point.daa_score
                || budget.reserved
                    != (crate::palw_bond_budget_v1::PalwBudgetVectorV1 {
                        claims: 1,
                        ..crate::palw_bond_budget_v1::PalwBudgetVectorV1::ZERO
                    })
            {
                return Err(bad("prefix source reservation differs from actual budget"));
            }
        }
        Ok(Some(*row))
    }
}

pub(super) fn sweep(builder: &mut TransitionBuilder<'_>, point: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    if !builder.params.claim_local_settlement_active_at(point.daa_score) {
        return Ok(());
    }
    let due = |table: u8, key: &[u8]| {
        table == PALW_CLAIM_LOCAL_TABLE_V1
            && key.len() == 73
            && key[0] == b'X'
            && u64::from_be_bytes(key[1..9].try_into().expect("fixed expiry")) < point.daa_score
    };
    let count =
        builder.state.kernel_route.as_ref().map_or(0, |route| route.aux.keys().filter(|(table, key)| due(*table, key)).count());
    if count == 0 {
        return Ok(());
    }
    let mut quote = native_writers::NativeWriteQuoteV1::new("prefix reservation expiries");
    quote.exact::<[u8; 73]>(count as u64)?;
    builder.admit_native_write_v1(quote)?;
    let keys: Vec<[u8; 73]> = builder
        .state
        .kernel_route
        .as_ref()
        .expect("counted")
        .aux
        .keys()
        .filter(|(table, key)| due(*table, key))
        .map(|(_, key)| key.as_slice().try_into().expect("fixed key"))
        .collect();
    for key in keys {
        let id = Hash64::from_bytes(key[9..].try_into().expect("fixed id"));
        // This synthetic reservation has no ordinary claim retirement hook.
        // Mark it non-live as well as closed; the budget owns any remaining
        // rolling-window release and removes the row when that window expires.
        builder
            .bond_budget_op(PalwBudgetGrowthV1::point(), |budget, _, journal| budget.forget(&id, journal))?
            .ok_or_else(|| bad("prefix expiry lost its native budget"))?;
        builder.write_kernel_row_encoded_v1::<u8>(PALW_CLAIM_LOCAL_TABLE_V1, &key, None)?;
    }
    Ok(())
}
