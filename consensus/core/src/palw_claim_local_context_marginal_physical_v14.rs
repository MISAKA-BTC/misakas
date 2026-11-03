// Same module as actual context publication; readonly physical facts only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum MissingFinancialCoverageV14 {
    MissingFinancialCoverage,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct ContextPhysicalAmountsV14 {
    pub new_rows: u64,
    pub new_key_and_value_bytes: u64,
    pub written_key_and_value_bytes: u64,
    pub old_existing_key_and_value_bytes: u64,
    pub present_dedup_rows: u64,
    pub present_dedup_key_and_value_bytes: u64,
    pub repeated_plan_nodes: u64,
    pub changed_existing_rows: u64,
    pub removed_rows: u64,
    pub write_attempts: u64,
    pub unchanged_write_attempts: u64,
    pub delta_entries: u64,
    pub delta_encoded_bytes: u64,
    pub old_payload_bytes_in_undo: u64,
    pub new_payload_bytes_in_undo: u64,
    pub old_current_root_pins_removed: u64,
    pub new_current_root_pins_added: u64,
    pub archive_component_pins_added: u64,
    pub archive_literal_pins_added: u64,
    pub archive_component_pins_removed: u64,
    pub archive_literal_pins_removed: u64,
}
/// No public fields/constructor: native plan/store factory produces binding.
/// Physical presence never authenticates an old payment or funded horizon.
pub(super) struct NativeContextMarginalFactsV14 {
    amounts: ContextPhysicalAmountsV14,
    before: Option<ComponentRootV2>,
    after: Option<ComponentRootV2>,
    mutation_key: Hash64,
    native_scratch_requests: u64,
}
impl NativeContextMarginalFactsV14 {
    pub(super) fn amounts(&self) -> ContextPhysicalAmountsV14 {
        self.amounts
    }
    pub(super) fn before(&self) -> Option<ComponentRootV2> {
        self.before
    }
    pub(super) fn after(&self) -> Option<ComponentRootV2> {
        self.after
    }
    pub(super) fn mutation_key(&self) -> Hash64 {
        self.mutation_key
    }
    pub(super) fn native_scratch_requests(&self) -> u64 {
        self.native_scratch_requests
    }
    pub(super) fn financial_coverage(&self) -> MissingFinancialCoverageV14 {
        MissingFinancialCoverageV14::MissingFinancialCoverage
    }
    pub(super) fn final_gc_undo_and_retention_missing(&self) -> bool {
        true
    }
    pub(super) fn derived_native_index_work_missing(&self) -> bool {
        true
    }
    pub(super) fn owner_horizon_coverage_missing(&self) -> bool {
        true
    }
    pub(super) fn native_reader_peak_missing(&self) -> bool {
        true
    }
    pub(super) fn requires_actual_context_recheck(&self) -> bool {
        true
    }
}
fn marginal_add_v14(a: u64, b: u64) -> Result<u64, ContextErrorV2> {
    a.checked_add(b).ok_or(ContextErrorV2::Resource(crate::palw_native_resource_v1::NativeCoverageV1::overflow::<
        NativeContextMarginalFactsV14,
    >("marginal physical sum")))
}
fn marginal_len_v14(v: &impl BorshSerialize) -> Result<u64, ContextErrorV2> {
    borsh::object_length(v).map(|n| n as u64).map_err(|_| bad("marginal canonical length"))
}
fn marginal_delta_v14(q: &mut ContextPhysicalAmountsV14, key: u64, old: Option<u64>, new: Option<u64>) -> Result<(), ContextErrorV2> {
    q.delta_entries = marginal_add_v14(q.delta_entries, 1)?;
    if let Some(n) = new {
        q.written_key_and_value_bytes = marginal_add_v14(q.written_key_and_value_bytes, marginal_add_v14(key, n)?)?;
    }
    if let Some(n) = old {
        q.old_existing_key_and_value_bytes = marginal_add_v14(q.old_existing_key_and_value_bytes, marginal_add_v14(key, n)?)?;
    }
    // Actual released KernelRouteRow encoding: variant u8, table u8,
    // Vec key, Option<Vec old>, Option<Vec new>. Tests compare actual enum codec.
    let mut n = marginal_add_v14(1 + 1 + 4 + 1 + 1, key)?;
    for v in [old, new].into_iter().flatten() {
        n = marginal_add_v14(n, marginal_add_v14(4, v)?)?;
    }
    q.delta_encoded_bytes = marginal_add_v14(q.delta_encoded_bytes, n)?;
    q.old_payload_bytes_in_undo = marginal_add_v14(q.old_payload_bytes_in_undo, old.unwrap_or(0))?;
    q.new_payload_bytes_in_undo = marginal_add_v14(q.new_payload_bytes_in_undo, new.unwrap_or(0))?;
    Ok(())
}
/// Source-local/path-sized exact prospective publication facts at THIS native
/// store. A later installer must rederive after any intervening mutation.
/// scope is only an I-key selector, not a source/admission/financial authority.
pub(super) fn quote_component_publication_v14(
    state: &PalwChainStateV2,
    plan: &ComponentUpdateV2,
    body: Option<&[u8]>,
    scope: Option<Hash64>,
) -> Result<NativeContextMarginalFactsV14, ContextErrorV2> {
    if state.kernel_route.is_none() {
        return Err(ContextErrorV2::Unavailable("marginal quote needs actual native route"));
    }
    let mut ordinary = [0; 18];
    ordinary[0] = CONTEXT_CURRENT_PREFIX_V2;
    plan.before().spec.serialize(&mut &mut ordinary[1..]).map_err(|_| bad("marginal current spec"))?;
    let owned = scope.map(|id| owned_current_key(id, plan.before().spec)).transpose()?;
    let key = owned.as_ref().map(|k| k.as_slice()).unwrap_or(ordinary.as_slice());
    let reader = StateNodesV2(state);
    if reader.current_at(key, plan.before().spec)? != plan.before() {
        return Err(ContextErrorV2::Stale);
    }
    let mut native = TirNativeRequestsV1::new();
    native
        .exact_v1::<crate::palw_native_resource_v1::BTreeNodeEnvelopeV1<Hash64, &ContextNodeV2>>(plan.new_nodes().len() as u64)
        .map_err(ContextErrorV2::Resource)?;
    let native_scratch_requests = native.bytes_v1();
    let _scratch = reserve_native_requests_v1(native_scratch_requests).map_err(ContextErrorV2::Resource)?;
    let mut seen = std::collections::BTreeMap::<Hash64, &ContextNodeV2>::new();
    let mut q = ContextPhysicalAmountsV14::default();
    if let Some(body) = body {
        let value =
            nodes::RowValueV2 { data: crate::palw_claim_local_v1::palw_liability_material_id_v1(body), bytes: body.len() as u64 };
        if plan.next_value() != Some(value) {
            return Err(bad("marginal supplied body differs from native plan"));
        }
        let physical = marginal_len_v14(&body)?;
        if let Some(raw) = reader.row(&content_key(CONTEXT_BODY_PREFIX_V2, value.data))? {
            crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&body, raw)
                .map_err(|_| bad("marginal body collision"))?;
            q.present_dedup_rows = marginal_add_v14(q.present_dedup_rows, 1)?;
            q.present_dedup_key_and_value_bytes =
                marginal_add_v14(q.present_dedup_key_and_value_bytes, marginal_add_v14(65, physical)?)?;
        } else {
            q.new_rows = marginal_add_v14(q.new_rows, 1)?;
            q.new_key_and_value_bytes = marginal_add_v14(q.new_key_and_value_bytes, marginal_add_v14(65, physical)?)?;
            q.write_attempts = marginal_add_v14(q.write_attempts, 1)?;
            marginal_delta_v14(&mut q, 65, None, Some(physical))?;
        }
    }
    for (id, node) in plan.new_nodes() {
        if nodes::digest(plan.before().spec, node) != *id {
            return Err(bad("marginal plan node identity"));
        }
        if let Some(prior) = seen.insert(*id, node) {
            if prior != node {
                return Err(bad("marginal repeated node collision"));
            }
            q.repeated_plan_nodes = marginal_add_v14(q.repeated_plan_nodes, 1)?;
            continue;
        }
        let physical = marginal_len_v14(node)?;
        if let Some(raw) = reader.row(&content_key(CONTEXT_NODE_PREFIX_V2, *id))? {
            crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(node, raw)
                .map_err(|_| bad("marginal node collision"))?;
            q.present_dedup_rows = marginal_add_v14(q.present_dedup_rows, 1)?;
            q.present_dedup_key_and_value_bytes =
                marginal_add_v14(q.present_dedup_key_and_value_bytes, marginal_add_v14(65, physical)?)?;
        } else {
            q.new_rows = marginal_add_v14(q.new_rows, 1)?;
            q.new_key_and_value_bytes = marginal_add_v14(q.new_key_and_value_bytes, marginal_add_v14(65, physical)?)?;
            q.write_attempts = marginal_add_v14(q.write_attempts, 1)?;
            marginal_delta_v14(&mut q, 65, None, Some(physical))?;
        }
    }
    // Same native publication preconditions for every new leaf and next body.
    // The supplied body is genuine hash/length-checked above; other bodies must
    // already exist and pass their actual native identity decoder.
    let supplied = body.map(|raw| nodes::RowValueV2 {
        data: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw),
        bytes: raw.len() as u64,
    });
    for (_, node) in plan.new_nodes() {
        if let ContextNodeV2::Leaf { value, .. } = node
            && Some(*value) != supplied
        {
            reader.body(*value)?;
        }
    }
    if let Some(value) = plan.next_value()
        && Some(value) != supplied
    {
        reader.body(value)?;
    }
    // Identical I publication still invokes the writer/reservation path but
    // native apply journals ONLY old!=new. Never bill a duplicate actualdelta.
    q.write_attempts = marginal_add_v14(q.write_attempts, 1)?;
    let prior = reader.row(key)?;
    let after = marginal_len_v14(&plan.after())?;
    if prior.is_some() && plan.before() == plan.after() {
        q.unchanged_write_attempts = 1;
    } else {
        match prior {
            Some(_) => q.changed_existing_rows = marginal_add_v14(q.changed_existing_rows, 1)?,
            None => {
                q.new_rows = marginal_add_v14(q.new_rows, 1)?;
                q.new_key_and_value_bytes = marginal_add_v14(q.new_key_and_value_bytes, marginal_add_v14(key.len() as u64, after)?)?;
            }
        }
        marginal_delta_v14(&mut q, key.len() as u64, prior.map(|r| r.len() as u64), Some(after))?;
        if plan.before().node != plan.after().node {
            q.old_current_root_pins_removed = u64::from(plan.before().node.is_some());
            q.new_current_root_pins_added = u64::from(plan.after().node.is_some());
        }
    }
    // No world traversal to claim unchanged subtree byte coverage; root pins
    // describe ownership operations, not an authenticated old-paid byte grant.
    Ok(NativeContextMarginalFactsV14 {
        amounts: q,
        before: Some(plan.before()),
        after: Some(plan.after()),
        mutation_key: crate::palw_claim_local_v1::palw_liability_material_id_v1(key),
        native_scratch_requests,
    })
}
/// Actual generic-A2 extra pin (or removal) after source-local components exist.
/// This is physical liveness only, not signed paid ownership of its referents.
pub(super) fn quote_archive_pin_v14(
    state: &PalwChainStateV2,
    archive: &ArchiveReferencesV2,
    remove: bool,
) -> Result<NativeContextMarginalFactsV14, ContextErrorV2> {
    if state.kernel_route.is_none() {
        return Err(ContextErrorV2::Unavailable("marginal archive quote needs actual route"));
    }
    if archive.version != 2 {
        return Err(bad("marginal archive version"));
    }
    // Existing native referents only. A prospective client without these rows
    // uses max-shape facts, not invented accepting points or old-paid coverage.
    let referents = StateNodesV2(state);
    for component in &archive.components {
        component.check()?;
        if let Some(id) = component.node {
            let node = referents.node(id)?;
            if nodes::digest(component.spec, &node) != id {
                return Err(bad("marginal archive root identity"));
            }
        }
    }
    for literal in &archive.literals {
        referents.body(*literal)?;
    }

    let mut native = TirNativeRequestsV1::new();
    native.exact_v1::<u8>(marginal_len_v14(archive)?).map_err(ContextErrorV2::Resource)?;
    let n = native.bytes_v1();
    let _encoded = reserve_native_requests_v1(n).map_err(ContextErrorV2::Resource)?;
    let mut raw = Vec::with_capacity(usize::try_from(marginal_len_v14(archive)?).map_err(|_| bad("marginal archive host length"))?);
    archive.serialize(&mut raw).map_err(|_| bad("marginal archive codec"))?;
    let id = crate::palw_claim_local_v1::palw_liability_material_id_v1(&raw);
    let key = content_key(b'A', id);
    let reader = StateNodesV2(state);
    let current = reader.row(&key)?;
    if let Some(actual) = current {
        if actual != raw.as_slice() {
            return Err(bad("marginal archive collision"));
        }
    } else if remove {
        return Err(ContextErrorV2::Unavailable("marginal removal required archive absent"));
    }
    let mut q = ContextPhysicalAmountsV14 { write_attempts: 1, ..Default::default() };
    if remove {
        q.removed_rows = 1;
        q.archive_component_pins_removed = archive.components.len() as u64;
        q.archive_literal_pins_removed = archive.literals.len() as u64;
        marginal_delta_v14(&mut q, 65, Some(raw.len() as u64), None)?;
    } else if current.is_some() {
        q.present_dedup_rows = 1;
        q.present_dedup_key_and_value_bytes = marginal_add_v14(65, raw.len() as u64)?;
        q.unchanged_write_attempts = 1;
    } else {
        q.new_rows = 1;
        q.new_key_and_value_bytes = marginal_add_v14(65, raw.len() as u64)?;
        q.archive_component_pins_added = archive.components.len() as u64;
        q.archive_literal_pins_added = archive.literals.len() as u64;
        marginal_delta_v14(&mut q, 65, None, Some(raw.len() as u64))?;
    }
    Ok(NativeContextMarginalFactsV14 { amounts: q, before: None, after: None, mutation_key: id, native_scratch_requests: n })
}
