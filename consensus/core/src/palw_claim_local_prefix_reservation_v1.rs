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
        export
            .intent
            .verify_signature_v1(&producer, &export.signature)
            .map_err(|_| bad("prefix export signature differs from the admitted producer"))?;
    }
    let claim = builder.state.claims.get(&id).ok_or_else(|| bad("prefix export source was not admitted"))?;
    let liability_sompi = builder.bond_formula_liability_v2(point, claim);
    if liability_sompi == 0 || liability_sompi == u128::MAX {
        return Err(bad("prefix export liability is unsourced"));
    }
    // Budget accounting alone does not reserve the common PALW principal.
    // All work/withdrawal gates share committed_at, which includes existing E rows.
    if liability_sompi > builder.gate_room(&claim.bond, point.daa_score, PalwRcoreGateV1::Work)? {
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
    super::palw_claim_local_prefix_financial_archive_v4::on_e_admission_v4(builder, &row)?;
    Ok(())
}

impl PalwChainStateV2 {
    /// Expiry is a release candidate, not evidence that retained material has
    /// actually been released. The X row remains authoritative until the sweep
    /// observes every existing source material hold discharged.
    pub(super) fn prefix_export_material_pending_v1(
        &self,
        params: &PalwStateParamsV2,
        row: &PalwPrefixExportReservationV1,
        now: u64,
    ) -> Result<bool, PalwStateV2Error> {
        if now <= row.material_until {
            return Ok(true);
        }
        let captured = super::palw_claim_local_material_income_lineage_v7::captured_income_pending_if_present_v7(
            self,
            row.budget_id,
            row.source_claim,
            row.point,
        )
        .map_err(super::palw_claim_local_paid_archive_v2::state_error)?;
        Ok(captured.unwrap_or(false) || self.prefix_claim_material_pending_v1(params, row.source_claim, now))
    }

    fn prefix_claim_material_pending_v1(&self, params: &PalwStateParamsV2, claim: Hash64, now: u64) -> bool {
        self.court_service_holds_claim_v1(&claim)
            || self.material_income_hold_until_v1(params, claim).is_some_and(|until| now <= until)
            || self.staged_claim_contract_v1(&claim).is_some_and(|c| now <= c.terms.material_until_daa)
            || self.staged_tir_source_until_v1(claim).is_some_and(|until| now <= until)
    }

    fn prefix_export_release_indexed_v1(&self, row: &PalwPrefixExportReservationV1) -> Result<bool, PalwStateV2Error> {
        let key = expiry_key(row.material_until, row.budget_id);
        let Some(bytes) = self.kernel_route.as_ref().and_then(|route| {
            route.aux.iter().find_map(|((table, k), v)| (*table == PALW_CLAIM_LOCAL_TABLE_V1 && k.as_slice() == key).then_some(v))
        }) else {
            return Ok(false);
        };
        if bytes.as_slice() != [1] {
            return Err(bad("malformed prefix release index"));
        }
        Ok(true)
    }
    /// Prospective source export capital is independent of the source's own
    /// claim reservation, and remains committed through its signed material tail.
    pub fn prefix_export_reserved_at_checked_v1(&self, bond: &PalwBondKeyV2, now_daa: u64) -> Result<u128, PalwStateV2Error> {
        let Some(route) = &self.kernel_route else { return Ok(0) };
        let mut total = 0u128;
        for ((table, key), bytes) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'E') {
                continue;
            }
            let row = decode_claim_local_value_v1::<PalwPrefixExportReservationV1>(bytes).map_err(|e| match e {
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
                crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => bad("malformed prefix capital reservation"),
            })?;
            if key.len() != 65
                || key[1..] != *row.budget_id.as_byte_slice()
                || row.budget_id != reservation_id(&row.point, row.source_claim)
                || row.liability_until > row.material_until
            {
                return Err(bad("prefix capital reservation identity differs"));
            }
            if row.bond == *bond && (now_daa <= row.material_until || self.prefix_export_release_indexed_v1(&row)?) {
                total =
                    total.checked_add(row.liability_sompi).ok_or(PalwStateV2Error::Overflow("prefix export committed collateral"))?;
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
        if self.last_point.is_some_and(|p| p.daa_score <= row.material_until) || self.prefix_export_release_indexed_v1(&row)? {
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
        let raw = builder
            .state
            .kernel_route
            .as_ref()
            .and_then(|route| {
                route.aux.iter().find_map(|((table, k), value)| {
                    (*table == PALW_CLAIM_LOCAL_TABLE_V1 && k.as_slice() == row_key(id)).then_some(value)
                })
            })
            .ok_or_else(|| bad("prefix expiry lost its source reservation"))?;
        let row = decode_claim_local_value_v1::<PalwPrefixExportReservationV1>(raw).map_err(|e| match e {
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
            crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_) => bad("malformed expiring prefix reservation"),
        })?;
        if row.budget_id != id
            || row.budget_id != reservation_id(&row.point, row.source_claim)
            || expiry_key(row.material_until, id) != key
        {
            return Err(bad("prefix expiry changed its source reservation"));
        }
        if builder.state.prefix_export_material_pending_v1(builder.params, &row, point.daa_score)? {
            continue;
        }
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

// All four coordinates belong to the actual admitted cohort. A re-admitted
// raw claim ID is never an alias of an earlier source or consumer.
type PrefixBirthV1 = (Hash64, Hash64, u64, u64);
fn birth_v1(claim: Hash64, point: &PalwBlockContextV2) -> PrefixBirthV1 {
    (claim, point.block, point.daa_score, point.blue_score)
}

/// Decreasing blue scores let one descending pass retain every transitive
/// ancestor. Callers reserve both edges and at most two set nodes per edge.
fn retain_prefix_ancestors_v1(
    edges: &mut [(PrefixBirthV1, PrefixBirthV1)],
    needed: &mut std::collections::BTreeSet<PrefixBirthV1>,
) -> Result<(), PalwStateV2Error> {
    if edges.iter().any(|(consumer, source)| source.2 > consumer.2 || source.3 >= consumer.3) {
        return Err(bad("prefix collection encountered a noncausal birth edge"));
    }
    edges.sort_unstable_by_key(|(consumer, _)| std::cmp::Reverse(consumer.3));
    for (consumer, source) in edges {
        if needed.contains(consumer) {
            needed.insert(*source);
        }
    }
    Ok(())
}

/// Preserve a producer's later conviction when its ordinary claim already
/// moved to the existing closed-source ledger. No consumer is slashed here.
pub(super) fn deny_closed_source_v1(
    builder: &mut TransitionBuilder<'_>,
    row: &PalwPrefixExportReservationV1,
) -> Result<(), PalwStateV2Error> {
    // A later raw-ID S1 row cannot substitute for this accepted source birth.
    // The shared A5 reader retains the exact legacy-S1 fallback and its native
    // closure/approval checks; denial remains monotonic for this E only.
    super::palw_claim_local_exact_closed_source_v5::deny_exact_closed_source_v5(builder, row)
}

/// Called by the existing compact-source collector after it records terminal
/// outcomes and before it collects full birth frames. No checkpoint codec or
/// owner projection is replaced. A terminal export is discarded only when its
/// finite material duties and every still-needed transitive consumer are closed.
pub(super) fn collect_closed_exports_v1(
    builder: &mut TransitionBuilder<'_>,
    point: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    use super::palw_claim_local_rewards_v1 as rewards;
    use super::palw_claim_local_source_origin_v8::{SourceOriginV8, visit_retained_prefix_origins_v8};
    let exports = builder.state.kernel_route.as_ref().map_or(0, |route| {
        route.aux.keys().filter(|(table, key)| *table == PALW_CLAIM_LOCAL_TABLE_V1 && key.first() == Some(&b'E')).count()
    });
    if exports == 0 {
        return Ok(());
    }
    // Without a live FP consumer's authenticated birth we cannot tell whether
    // it is independent or uses one of these sources. Preserve all exports;
    // missing provenance never establishes absence of a dependency.
    for (id, claim) in &builder.state.claims {
        if matches!(claim.source, PalwClaimSourceV2::FreePrompt { .. })
            && SourceOriginV8::for_claim(&builder.state, builder.params, *id, claim)?.is_none()
        {
            return Ok(());
        }
    }
    let mut edge_count = 0u64;
    let mut incomplete = false;
    visit_retained_prefix_origins_v8(&builder.state, builder.params, |origin| {
        let consumed = origin.prefix_use()?;
        // A retained declaration whose actual consumer key/control is absent
        // cannot establish independence or closure. Preserve all E in this pass.
        if consumed.is_none()
            && origin.inputs().iter().any(|input| {
                matches!(input,
            crate::palw_claim_local_v1::PalwClaimLocalInputV1::PrefixDependency { claim, envelope }
                if *claim == origin.claim_id() && envelope.consumed.is_some())
            })
        {
            incomplete = true;
        }
        if consumed.is_some() {
            edge_count = edge_count.checked_add(1).ok_or(PalwStateV2Error::Overflow("prefix collection edges"))?;
        }
        Ok(())
    })?;
    if incomplete {
        return Ok(());
    }
    let mut quote = native_writers::NativeWriteQuoteV1::new("closed prefix export dependency closure");
    quote.exact::<(PrefixBirthV1, PrefixBirthV1)>(edge_count)?;
    quote.member::<PrefixBirthV1, ()>(edge_count.checked_mul(2).ok_or(PalwStateV2Error::Overflow("prefix closure nodes"))?)?;
    quote.exact::<[u8; 65]>(exports as u64)?;
    builder.admit_native_write_v1(quote)?;
    let mut edges = Vec::with_capacity(usize::try_from(edge_count).map_err(|_| bad("prefix edge count exceeds address space"))?);
    let mut needed = std::collections::BTreeSet::new();
    visit_retained_prefix_origins_v8(&builder.state, builder.params, |origin| {
        let Some(consumed) = origin.prefix_use()? else {
            return Ok(());
        };
        let claim = origin.claim_id();
        let consumer = birth_v1(claim, &origin.accepting_point());
        let source = consumed.intent.source;
        let source = (source.source_claim, source.accepted_block, source.accepted_daa, source.accepted_blue_score);
        if edges.len() as u64 >= edge_count {
            return Err(bad("retained prefix origins exceeded admitted edge count"));
        }
        edges.push((consumer, source));
        let original = origin.original_claim();
        let live = builder.state.claims.get(&claim).is_some_and(|row| {
            row.accepted_block == original.accepted_block
                && row.accepted_daa == original.accepted_daa
                && row.accepted_blue_score == original.accepted_blue_score
        });
        let closed = super::palw_claim_local_exact_closed_source_v5::read_exact_closed_source_v5(
            &builder.state,
            builder.params,
            claim,
            &origin.accepting_point(),
        )?;
        if live
            || point.daa_score <= consumed.intent.material_until
            || builder.state.prefix_claim_material_pending_v1(builder.params, claim, point.daa_score)
            || closed.as_ref().is_none_or(|row| birth_v1(row.claim, &row.birth) != consumer || point.daa_score <= row.material_until)
        {
            needed.insert(consumer);
        }
        Ok(())
    })?;
    if edges.len() as u64 != edge_count {
        return Err(bad("retained prefix origins changed admitted edge count"));
    }
    retain_prefix_ancestors_v1(&mut edges, &mut needed)?;
    let mut keys = Vec::<[u8; 65]>::with_capacity(exports);
    for ((table, key), _) in builder.state.kernel_route.as_ref().expect("counted exports").aux.iter() {
        if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'E') {
            continue;
        }
        keys.push(key.as_slice().try_into().map_err(|_| bad("invalid prefix collection reservation key"))?);
    }
    for key in keys {
        let raw = builder
            .state
            .kernel_route
            .as_ref()
            .expect("counted exports")
            .aux
            .iter()
            .find_map(|((table, k), value)| (*table == PALW_CLAIM_LOCAL_TABLE_V1 && k.as_slice() == key).then_some(value))
            .ok_or_else(|| bad("prefix collection lost its reservation"))?;
        let row = rewards::decoded::<PalwPrefixExportReservationV1>(raw)?;
        if row_key(row.budget_id) != key || row.budget_id != reservation_id(&row.point, row.source_claim) {
            return Err(bad("prefix collection changed its admitted source"));
        }
        if row.convicted_daa.is_some() {
            deny_closed_source_v1(builder, &row)?;
        }
        if builder.state.prefix_export_material_pending_v1(builder.params, &row, point.daa_score)?
            || builder.state.prefix_export_release_indexed_v1(&row)?
            || needed.contains(&birth_v1(row.source_claim, &row.point))
        {
            continue;
        }
        let Some(closed) = super::palw_claim_local_exact_closed_source_v5::read_exact_closed_source_v5(
            &builder.state,
            builder.params,
            row.source_claim,
            &row.point,
        )?
        else {
            continue;
        };
        if birth_v1(closed.claim, &closed.birth) != birth_v1(row.source_claim, &row.point)
            || closed.original.bond != row.bond
            || point.daa_score <= closed.material_until
            || builder.state.claims.get(&row.source_claim).is_some_and(|claim| {
                (claim.accepted_block, claim.accepted_daa, claim.accepted_blue_score)
                    == (row.point.block, row.point.daa_score, row.point.blue_score)
            })
        {
            continue;
        }
        // The ordinary closed-source row now preserves the source's terminal
        // outcome. Future consumers cannot use an expired, unreserved export.
        super::palw_claim_local_material_income_lineage_v7::drop_closed_population_v7(builder, row.budget_id)?;
        super::palw_claim_local_prefix_financial_archive_v4::collect_e_v4(builder, &row)?;
        builder.write_kernel_row_encoded_v1::<PalwPrefixExportReservationV1>(PALW_CLAIM_LOCAL_TABLE_V1, &key, None)?;
        super::palw_claim_local_born_tir_source_v16::on_native_archive_ownership_changed_v19(builder, row.source_claim, row.point)?;
    }
    Ok(())
}

#[cfg(test)]
mod prefix_collection_tests {
    use super::*;
    fn birth(id: u64, block: u64, blue: u64) -> PrefixBirthV1 {
        (Hash64::from_u64_word(id), Hash64::from_u64_word(block), blue, blue)
    }
    #[test]
    fn rfc40_prefix_gc_retains_only_exact_birth_transitive_ancestors() {
        let a = birth(1, 10, 1);
        let c = birth(3, 30, 2);
        let d = birth(4, 40, 3);
        let independent_b = birth(2, 20, 4);
        let readmitted_a = birth(1, 11, 5);
        let mut edges = [(c, a), (d, c)];
        let mut needed = [d, independent_b, readmitted_a].into_iter().collect();
        retain_prefix_ancestors_v1(&mut edges, &mut needed).unwrap();
        assert_eq!(needed, [a, c, d, independent_b, readmitted_a].into_iter().collect());
        let mut closed = [independent_b, readmitted_a].into_iter().collect();
        retain_prefix_ancestors_v1(&mut edges, &mut closed).unwrap();
        assert!(!closed.contains(&a));
        assert!(!closed.contains(&c));
    }
    #[test]
    fn rfc40_prefix_gc_refuses_cycle_or_future_source() {
        let a = birth(1, 10, 1);
        let c = birth(3, 30, 2);
        let mut needed = [c].into_iter().collect();
        assert!(retain_prefix_ancestors_v1(&mut [(c, a), (a, c)], &mut needed).is_err());
        let future_daa = (a.0, a.1, 3, a.3);
        assert!(retain_prefix_ancestors_v1(&mut [(c, future_daa)], &mut needed).is_err());
    }
}

#[cfg(test)]
#[path = "palw_claim_local_prefix_gc_source_origin_tests_v8.rs"]
mod source_origin_collection_tests_v8;

/// Material-reader closure only. Fixed Source8/Carrier9 eligibility metadata
/// remains under its existing one-time ledger owner (RFC42 section3.2); a live
/// Claim or absent A5 does not renew supplier duties or retain all21 TIR bodies.
pub(super) fn native_other_archive_material_closed_v19(
    s: &PalwChainStateV2,
    id: Hash64,
    point: PalwBlockContextV2,
    now: u64,
) -> Result<bool, PalwStateV2Error> {
    if s.court_service_holds_claim_v1(&id) {
        return Ok(false);
    }
    if s.staged_tir_by_claim.get(&id).is_some_and(|rows| {
        rows.iter().any(|(until, key)| *key != crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirClaim(id) && now <= *until)
    }) {
        return Ok(false);
    }
    if s.prefix_export_reservation_v1(&point, id)?.is_some_and(|e| now <= e.material_until) {
        return Ok(false);
    }
    Ok(true)
}
pub(super) fn native_source_archive_duties_closed_v19(
    s: &PalwChainStateV2,
    _params: &PalwStateParamsV2,
    id: Hash64,
    point: PalwBlockContextV2,
    now: u64,
) -> Result<bool, PalwStateV2Error> {
    if crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(s, id).is_some() {
        return Ok(false);
    }
    native_other_archive_material_closed_v19(s, id, point, now)
}
