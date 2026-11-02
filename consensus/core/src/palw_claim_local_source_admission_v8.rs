//! ISOLATED state_v2 child: actual source-only Claim admission archive.
//! No wholeworld snapshot, fake full root, approval or model validity assertion.
use super::palw_claim_local_context_store_v2::{self as store, StateNodesV2};
use super::*;
use crate::palw_claim_local_context_nodes_v2::{
    self as nodes, ComponentRootV2, ComponentSpecV2, ContextErrorV2, RowValueV2,
};
use crate::palw_claim_local_v1::{
    PALW_CLAIM_LOCAL_TABLE_V1, PalwClaimLocalInputV1 as Input,
    PalwClaimLocalPolicyContextV1 as Policy,
};
use crate::palw_native_resource_v1::{
    NativeRetainedV1, NativeWireScanV1, TirNativeRequestsV1, reserve_native_requests_v1,
};
use borsh::{BorshDeserialize, BorshSerialize};
const DIRECTORY: u32 = 1;
const STORED: u32 = 2;
fn spec(table: u32) -> ComponentSpecV2 {
    ComponentSpecV2 {
        owner: 9,
        table,
        schema: 8,
        key_bytes: 64,
        value_bytes: u64::MAX,
    }
}
fn bad(s: &'static str) -> ContextErrorV2 {
    ContextErrorV2::Malformed(s)
}
fn err(e: ContextErrorV2) -> PalwStateV2Error {
    super::palw_claim_local_paid_archive_v2::state_error(e)
}
fn native_err(e: PalwStateV2Error) -> ContextErrorV2 {
    match e {
        PalwStateV2Error::NativeResource(e) => ContextErrorV2::Resource(e),
        _ => bad("source admission journal refused"),
    }
}
fn encode<T: BorshSerialize>(v: &T) -> Result<NativeRetainedV1<Vec<u8>>, ContextErrorV2> {
    let n = borsh::object_length(v).map_err(|_| bad("source admission encoded length"))?;
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u8>(n as u64)
        .map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut out = Vec::with_capacity(n);
    v.serialize(&mut out)
        .map_err(|_| bad("source admission encode"))?;
    Ok(NativeRetainedV1::from_owner_reserved_v1(out, lease))
}
fn decode<T: BorshDeserialize + NativeWireScanV1>(
    raw: &[u8],
) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    crate::palw_native_resource_v1::retain_native_value_decode_classified_v1(raw, || {
        borsh::from_slice(raw)
    })
    .map_err(|e| match e {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(r) => {
            ContextErrorV2::Resource(r)
        }
        _ => bad("source admission typed body codec"),
    })
}
fn hash<T: BorshSerialize>(domain: u8, v: &T) -> Result<Hash64, ContextErrorV2> {
    // Distinct typed domain is part of the streamed identity, not an allocation.
    struct Writer(blake2b_simd::State);
    impl std::io::Write for Writer {
        fn write(&mut self, x: &[u8]) -> std::io::Result<usize> {
            self.0.update(x);
            Ok(x.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut w = Writer(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka-palw/source-admission/v8")
            .to_state(),
    );
    domain
        .serialize(&mut w)
        .map_err(|_| bad("source admission domain"))?;
    v.serialize(&mut w)
        .map_err(|_| bad("source admission identity"))?;
    let mut bytes = [0; 64];
    bytes.copy_from_slice(w.0.finalize().as_bytes());
    Ok(Hash64::from_bytes(bytes))
}
fn value(raw: &[u8]) -> RowValueV2 {
    RowValueV2 {
        data: crate::palw_claim_local_v1::palw_liability_material_id_v1(raw),
        bytes: raw.len() as u64,
    }
}
fn root(
    s: &PalwChainStateV2,
    table: u32,
    scope: Option<Hash64>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let r = StateNodesV2(s);
    match scope {
        Some(id) => r.owned_current(id, spec(table)),
        None => r.current(spec(table)),
    }
}
fn lookup<T: BorshDeserialize + NativeWireScanV1>(
    s: &PalwChainStateV2,
    table: u32,
    scope: Option<Hash64>,
    key: Hash64,
) -> Result<Option<NativeRetainedV1<T>>, ContextErrorV2> {
    let r = StateNodesV2(s);
    nodes::lookup(&r, root(s, table, scope)?, key.as_byte_slice())?
        .map(|v| {
            let body = r.body(v)?;
            decode(&body)
        })
        .transpose()
}
fn put<T: BorshSerialize>(
    b: &mut TransitionBuilder<'_>,
    table: u32,
    scope: Option<Hash64>,
    key: Hash64,
    row: Option<&T>,
) -> Result<ComponentRootV2, ContextErrorV2> {
    let raw = row.map(encode).transpose()?;
    let r = StateNodesV2(&b.state);
    let before = root(&b.state, table, scope)?;
    let old = nodes::lookup(&r, before, key.as_byte_slice())?;
    let plan = nodes::update(
        &r,
        before,
        key.as_byte_slice(),
        old,
        raw.as_ref().map(|r| value(r)),
    )?;
    match scope {
        Some(id) => {
            store::publish_owned_component_v2(b, id, &plan, raw.as_deref().map(Vec::as_slice))?
        }
        None => store::publish_component_v2(b, &plan, raw.as_deref().map(Vec::as_slice))?,
    };
    Ok(plan.after())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Birth {
    block: BlockHash,
    daa: u64,
    blue: u64,
}
crate::palw_native_resource_v1::wire::wire_struct!(Birth;BlockHash,u64,u64);
impl Birth {
    fn of(c: &PalwClaimStateV2) -> Self {
        Self {
            block: c.accepted_block,
            daa: c.accepted_daa,
            blue: c.accepted_blue_score,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct Dependency {
    source: Hash64,
    birth: Birth,
}
crate::palw_native_resource_v1::wire::wire_struct!(Dependency;Hash64,Birth);
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
struct SourceV8 {
    version: u16,
    id: Hash64,
    point: PalwBlockContextV2,
    stored: Birth,
    network: [u8; 32],
    forks: [u8; 32],
    journal: u64,
    ordinal: Option<u64>,
    original: RowValueV2,
    inputs: RowValueV2,
    control: Option<RowValueV2>,
    header: Option<RowValueV2>,
    policy: RowValueV2,
    params: RowValueV2,
    dependencies: RowValueV2,
}
crate::palw_native_resource_v1::wire::wire_struct!(SourceV8;u16,Hash64,PalwBlockContextV2,Birth,[u8;32],[u8;32],u64,Option<u64>,RowValueV2,RowValueV2,Option<RowValueV2>,Option<RowValueV2>,RowValueV2,RowValueV2,RowValueV2);
fn exact_key(id: Hash64, p: PalwBlockContextV2) -> Result<Hash64, ContextErrorV2> {
    hash(1, &(id, p.block, p.daa_score, p.blue_score))
}
fn stored_key(id: Hash64, b: Birth) -> Result<Hash64, ContextErrorV2> {
    hash(2, &(id, b))
}
fn body<T: BorshDeserialize + NativeWireScanV1>(
    s: &PalwChainStateV2,
    v: RowValueV2,
) -> Result<NativeRetainedV1<T>, ContextErrorV2> {
    let raw = StateNodesV2(s).body(v)?;
    decode(&raw)
}
fn put_literal<T: BorshSerialize>(
    b: &mut TransitionBuilder<'_>,
    v: &T,
) -> Result<RowValueV2, ContextErrorV2> {
    // A temporary CURRENT literal component uses existing shared publication
    // admission/collision checks and is cleared inside the batch. Each
    // source A2 inventory independently roots its full immutable literal closure.
    let raw = encode(v)?;
    let rv = value(&raw);
    put(b, 3, None, Hash64::default(), Some(v))?;
    put::<T>(b, 3, None, Hash64::default(), None)?;
    Ok(rv)
}
fn akey(id: Hash64) -> [u8; 65] {
    let mut k = [0; 65];
    k[0] = b'A';
    k[1..].copy_from_slice(id.as_byte_slice());
    k
}
fn inventory(
    m: &SourceV8,
    manifest: RowValueV2,
) -> Result<NativeRetainedV1<store::ArchiveReferencesV2>, ContextErrorV2> {
    let count = 6 + usize::from(m.control.is_some()) + usize::from(m.header.is_some());
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<RowValueV2>(count as u64)
        .map_err(ContextErrorV2::Resource)?;
    let lease = reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
    let mut literals = Vec::with_capacity(count);
    literals.extend([
        manifest,
        m.original,
        m.inputs,
        m.policy,
        m.params,
        m.dependencies,
    ]);
    if let Some(v) = m.control {
        literals.push(v)
    }
    if let Some(v) = m.header {
        literals.push(v)
    }
    Ok(NativeRetainedV1::from_owner_reserved_v1(
        store::ArchiveReferencesV2 {
            version: 2,
            components: Vec::new(),
            literals,
        },
        lease,
    ))
}
fn input_is_for(x: &Input, id: Hash64, c: &PalwClaimStateV2) -> Result<bool, ContextErrorV2> {
    Ok(match x {
        Input::Attempt {
            block, envelope, ..
        } => {
            let a = &envelope.attempt;
            if *block != c.accepted_block
                || a.class_id != c.class_id
                || a.executor_bond != c.bond.0
                || a.trace_root != c.trace_root
                || a.output_root != c.output_root
                || a.execution_root != c.execution_root
            {
                false
            } else {
                let n =
                    borsh::object_length(a).map_err(|_| bad("source attempt identity length"))?;
                let mut q = TirNativeRequestsV1::new();
                q.vec_v1::<u8>((n as u64).max(1024))
                    .map_err(ContextErrorV2::Resource)?;
                let _lease =
                    reserve_native_requests_v1(q.bytes_v1()).map_err(ContextErrorV2::Resource)?;
                crate::palw_attempt_v2::attempt_id_v2(a) == id
            }
        }
        Input::ReceiptSpend { spend, .. } => spend.claim_id == id,
        Input::History { claim, .. }
        | Input::PublicBinding { claim, .. }
        | Input::CustodyDelegation { claim, .. }
        | Input::TirOriginal { claim, .. }
        | Input::FreePromptNative { claim, .. }
        | Input::PrefixDependency { claim, .. } => *claim == id,
        Input::ConsumedCarrier { consumer, .. } => *consumer == id,
    })
}
fn prefix_use<'a>(
    id: Hash64,
    control: Option<&PalwConsensusObjectV2>,
    inputs: &'a [Input],
) -> Result<Option<&'a crate::palw_claim_local_v1::prefix_v1::PalwSignedPrefixUseV1>, ContextErrorV2>
{
    let mut matching = inputs.iter().filter_map(|x| match x {
        Input::PrefixDependency { claim, envelope } if *claim == id => Some(envelope),
        _ => None,
    });
    let Some(envelope) = matching.next() else {
        return Ok(None);
    };
    if matching.next().is_some() {
        return Err(bad("duplicate source prefix metadata"));
    }
    envelope
        .check_v1()
        .map_err(|_| bad("invalid source prefix envelope"))?;
    let Some(used) = &envelope.consumed else {
        return Ok(None);
    };
    let Some(PalwConsensusObjectV2::FreePromptCommitted {
        claim,
        consumed_prefix_state,
        ..
    }) = control
    else {
        return Ok(None);
    };
    if *claim != id {
        return Err(bad("source prefix control identity"));
    }
    let Some(job) = inputs.iter().find_map(|x| match x {
        Input::FreePromptNative {
            claim,
            public_job: Some(job),
            ..
        } if *claim == id => Some(job),
        _ => None,
    }) else {
        return Ok(None);
    };
    if used.intent.base_commitment != id
        || used.intent.network != job.job.network_domain
        || used.intent.prefix != *consumed_prefix_state
        || crate::palw_freeprompt_v3::fp_claim_id_v3(job) != id
        || used
            .intent
            .verify_signature_v1(&job.job.executor_pubkey, &used.signature)
            .is_err()
    {
        return Ok(None);
    }
    Ok(Some(used))
}
/// Opaque leased typed source. This has no frame()/fullstate/root API.
pub(super) struct SourceAdmissionViewV8 {
    source: NativeRetainedV1<SourceV8>,
    original: NativeRetainedV1<PalwClaimStateV2>,
    inputs: NativeRetainedV1<Vec<Input>>,
    control: Option<NativeRetainedV1<PalwConsensusObjectV2>>,
    policy: NativeRetainedV1<Policy>,
    params: NativeRetainedV1<Vec<u8>>,
    dependencies: NativeRetainedV1<Vec<Dependency>>,
}
impl SourceAdmissionViewV8 {
    pub(super) fn original_reference_v18(&self)->RowValueV2{self.source.original}
    pub(super) fn original_claim(&self) -> &PalwClaimStateV2 {
        &self.original
    }
    pub(super) fn accepting_point(&self) -> PalwBlockContextV2 {
        self.source.point
    }
    pub(super) fn policy(&self) -> &Policy {
        &self.policy
    }
    pub(super) fn inputs(&self) -> &[Input] {
        &self.inputs
    }
    pub(super) fn original_params_bytes(&self) -> &[u8] {
        &self.params
    }
    pub(super) fn prefix_use(
        &self,
    ) -> Result<Option<&crate::palw_claim_local_v1::prefix_v1::PalwSignedPrefixUseV1>, ContextErrorV2>
    {
        prefix_use(self.source.id, self.control.as_deref(), &self.inputs)
    }

    pub(super) fn inputs_known(&self) -> Result<bool, ContextErrorV2> {
        Ok(match self.original.source {
            PalwClaimSourceV2::Attempt => {
                self.inputs
                    .iter()
                    .try_fold(false, |found, x| -> Result<bool, ContextErrorV2> {
                        Ok(found
                            || input_is_for(x, self.source.id, &self.original)?
                                && matches!(x, Input::Attempt { .. }))
                    })?
            }
            PalwClaimSourceV2::FreePrompt { .. } => {
                matches!(self.control.as_deref(),Some(PalwConsensusObjectV2::FreePromptCommitted{claim,consumed_prefix_state,..})if *claim==self.source.id&&consumed_prefix_state.is_genesis())
                    || prefix_use(self.source.id, self.control.as_deref(), &self.inputs)?.is_some()
            }
        })
    }
    pub(super) fn visit_dependencies(
        &self,
        mut f: impl FnMut(Hash64, BlockHash, u64, u64) -> Result<(), PalwStateV2Error>,
    ) -> Result<(), PalwStateV2Error> {
        for d in self.dependencies.iter() {
            f(d.source, d.birth.block, d.birth.daa, d.birth.blue)?
        }
        Ok(())
    }
}
fn read(s: &PalwChainStateV2, rv: RowValueV2) -> Result<SourceAdmissionViewV8, ContextErrorV2> {
    let m = body::<SourceV8>(s, rv)?;
    if m.version != 8
        || m.network == [0; 32]
        || m.forks == [0; 32]
        || m.stored.daa != m.point.daa_score
        || m.stored.blue != m.point.blue_score
    {
        return Err(bad("source admission point/rules"));
    }
    let reader = StateNodesV2(s);
    if nodes::lookup(
        &reader,
        reader.current(spec(DIRECTORY))?,
        exact_key(m.id, m.point)?.as_byte_slice(),
    )? != Some(rv)
        || nodes::lookup(
            &reader,
            reader.current(spec(STORED))?,
            stored_key(m.id, m.stored)?.as_byte_slice(),
        )? != Some(rv)
    {
        return Err(bad("source accepting/stored directories disagree"));
    }
    let original = body::<PalwClaimStateV2>(s, m.original)?;
    if Birth::of(&original) != m.stored {
        return Err(bad("source original stored birth differs"));
    }
    let inputs = body::<Vec<Input>>(s, m.inputs)?;
    let control = m
        .control
        .map(|v| body::<PalwConsensusObjectV2>(s, v))
        .transpose()?;
    let policy = body::<Policy>(s, m.policy)?;
    let dependencies = body::<Vec<Dependency>>(s, m.dependencies)?;
    let params = body::<Vec<u8>>(s, m.params)?;
    for x in inputs.iter() {
        if !input_is_for(x, m.id, &original)? {
            return Err(bad("source archive contains unrelated original input"));
        }
    }
    match original.source {
        PalwClaimSourceV2::Attempt => {
            if !inputs.iter().any(|x| matches!(x, Input::Attempt { .. })) {
                return Err(bad("source attempt original absent"));
            }
        }
        PalwClaimSourceV2::FreePrompt { .. } => {
            let Some(PalwConsensusObjectV2::FreePromptCommitted {
                claim,
                class_id,
                bond,
                trace_root,
                output_root,
                execution_root,
                ..
            }) = control.as_deref()
            else {
                return Err(bad("source FP actual control absent"));
            };
            if *claim != m.id
                || *class_id != original.class_id
                || *bond != original.bond
                || *trace_root != original.trace_root
                || *output_root != original.output_root
                || *execution_root != original.execution_root
                || m.ordinal.is_none()
            {
                return Err(bad("source FP control subject differs"));
            }
            if !inputs
                .iter()
                .any(|x| matches!(x,Input::FreePromptNative{claim,..}if *claim==m.id))
            {
                return Err(bad("source FP native original absent"));
            }
        }
    }
    if let Some(v) = m.header {
        let header = body::<Vec<u8>>(s, v)?;
        let actual = decode::<crate::header::Header>(&header)?;
        if actual.hash != m.stored.block {
            return Err(bad("source carrying header differs"));
        }
    }
    let a = inventory(&m, rv)?;
    let encoded = encode(&*a)?;
    let id = value(&encoded).data;
    let _key = reserve_native_requests_v1(65).map_err(ContextErrorV2::Resource)?;
    let route = s
        .kernel_route
        .as_ref()
        .ok_or(ContextErrorV2::Unavailable("source archive route absent"))?;
    let stored = route
        .aux
        .get(&(PALW_CLAIM_LOCAL_TABLE_V1, akey(id).to_vec()))
        .ok_or(ContextErrorV2::Unavailable(
            "source archive inventory absent",
        ))?;
    crate::palw_claim_local_v1::palw_check_canonical_liability_bytes_v1(&*a, stored)
        .map_err(|_| bad("source inventory differs"))?;
    Ok(SourceAdmissionViewV8 {
        source: m,
        original,
        inputs,
        control,
        policy,
        params,
        dependencies,
    })
}
pub(super) fn for_stored_claim_v8(
    s: &PalwChainStateV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<Option<SourceAdmissionViewV8>, PalwStateV2Error> {
    let key = stored_key(id, Birth::of(c)).map_err(err)?;
    let r = StateNodesV2(s);
    let Some(rv) = nodes::lookup(
        &r,
        r.current(spec(STORED)).map_err(err)?,
        key.as_byte_slice(),
    )
    .map_err(err)?
    else {
        return Ok(None);
    };
    let v = read(s, rv).map_err(err)?;
    if v.source.id != id
        || v.source.stored != Birth::of(c)
        || v.original.execution_root != c.execution_root
        || v.original.bond != c.bond
        || v.original.class_id != c.class_id
        || v.original.output_root != c.output_root
        || v.original.trace_root != c.trace_root
        || !matches!(
            (&v.original.source, &c.source),
            (PalwClaimSourceV2::Attempt, PalwClaimSourceV2::Attempt)
        ) && !matches!((&v.original.source,&c.source),(PalwClaimSourceV2::FreePrompt{quanta:a,..},PalwClaimSourceV2::FreePrompt{quanta:b,..})if a==b)
    {
        return Err(err(bad("source stored directory identity changed")));
    }
    Ok(Some(v))
}
fn claim_before<'a>(
    b: &'a TransitionBuilder<'_>,
    parent: &'a PalwChainStateV2,
    end: usize,
    id: Hash64,
) -> Option<&'a PalwClaimStateV2> {
    for e in b.entries[..end].iter().rev() {
        if let PalwDeltaEntryV2::Claim { key, new, .. } = e {
            if *key == id {
                return new.as_ref();
            }
        }
    }
    parent.claims.get(&id)
}
/// Capture ONLY actual successful None->Some Claim events. The inputs slice is
/// the original vector already constructed from authentic block_work/merged
/// work/extras by retain_changed_claim_context, never decoded from F1.
pub(super) fn capture_admissions_v8(
    b: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
    objects: &[PalwConsensusObjectV2],
    inputs: &[Input],
) -> Result<(), PalwStateV2Error> {
    if !b.params.claim_local_settlement_active_at(ctx.daa_score) {
        return Ok(());
    }
    let end = b.entries.len();
    for index in 0..end {
        let (id, claim) = match &b.entries[index] {
            PalwDeltaEntryV2::Claim {
                key,
                old: None,
                new: Some(c),
            } => (*key, c),
            _ => continue,
        };
        if claim.accepted_daa != ctx.daa_score || claim.accepted_blue_score != ctx.blue_score {
            return Err(err(bad("fresh source claim differs from executing clock")));
        }
        let original_raw = encode(claim).map_err(err)?;
        let original = decode::<PalwClaimStateV2>(&original_raw).map_err(err)?;
        let mut count = 0;
        for x in inputs {
            if input_is_for(x, id, &original).map_err(err)? {
                count += 1
            }
        }
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<&Input>(count as u64)
            .map_err(ContextErrorV2::Resource)
            .map_err(err)?;
        let _refs = reserve_native_requests_v1(q.bytes_v1())
            .map_err(ContextErrorV2::Resource)
            .map_err(err)?;
        let mut selected = Vec::with_capacity(count);
        for x in inputs {
            if input_is_for(x, id, &original).map_err(err)? {
                selected.push(x)
            }
        }
        let source_inputs_raw = encode(&selected).map_err(err)?;
        let source_inputs = decode::<Vec<Input>>(&source_inputs_raw).map_err(err)?;
        let object = super::palw_claim_local_object_spans_v2::object_for_entry(b, objects, index)?;
        let control_raw = object.map(encode).transpose().map_err(err)?;
        let control = control_raw
            .as_ref()
            .map(|raw| decode::<PalwConsensusObjectV2>(raw))
            .transpose()
            .map_err(err)?;
        let ordinal = b
            .claim_local_object_spans
            .iter()
            .find(|s| s.first <= index && index < s.end)
            .map(|s| s.ordinal as u64);
        if matches!(original.source, PalwClaimSourceV2::FreePrompt { .. })
            && !matches!(control.as_deref(),Some(PalwConsensusObjectV2::FreePromptCommitted{claim,..})if *claim==id)
        {
            return Err(err(bad(
                "fresh FP source lacks actual accepted control span",
            )));
        }
        if let Some(object @ PalwConsensusObjectV2::FreePromptCommitted{..})=object {
            super::palw_tir_public_fold_v1::recheck_source_input_generation_v12(b,ctx,id,object,&source_inputs)?;
        }
        let used = prefix_use(id, control.as_deref(), &source_inputs).map_err(err)?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<Dependency>((source_inputs.len() + 1) as u64)
            .map_err(ContextErrorV2::Resource)
            .map_err(err)?;
        let _deps = reserve_native_requests_v1(q.bytes_v1())
            .map_err(ContextErrorV2::Resource)
            .map_err(err)?;
        let mut dependencies = Vec::with_capacity(source_inputs.len() + 1);
        if let Some(used) = used {
            let s = used.intent.source;
            if s.source_claim != id {
                if let Some(old) = claim_before(b, parent, index, s.source_claim) {
                    if (
                        old.accepted_block,
                        old.accepted_daa,
                        old.accepted_blue_score,
                    ) == (s.accepted_block, s.accepted_daa, s.accepted_blue_score)
                        && old.accepted_daa <= original.accepted_daa
                        && old.accepted_blue_score <= original.accepted_blue_score
                    {
                        dependencies.push(Dependency {
                            source: s.source_claim,
                            birth: Birth::of(old),
                        })
                    }
                }
            }
        }
        // Raw tag8 does not identify its source birth. Exact carrier edges are
        // captured separately from actual spends and parent born authority.
        dependencies.sort_by_key(|d| (d.source, d.birth.block, d.birth.daa, d.birth.blue));
        dependencies.dedup();
        let (network, forks) = b
            .extras
            .claim_local_rule_context_v1
            .ok_or_else(|| err(bad("source original network rules absent")))?;
        let mut q = TirNativeRequestsV1::new();
        q.exact_v1::<crate::palw_detection_policy_v1::PalwDetectionPolicyV1>(
            Policy::detection_at_v1(b.params, ctx.daa_score).len() as u64,
        )
        .map_err(ContextErrorV2::Resource)
        .map_err(err)?;
        let _policy = reserve_native_requests_v1(q.bytes_v1())
            .map_err(ContextErrorV2::Resource)
            .map_err(err)?;
        let policy = Policy::from_rules_v1(b.params, ctx.daa_score);
        let original_params = encode(b.params).map_err(err)?;
        // Header bytes are independently owned before builder mutation. Preserve
        // only this admitted Attempt's actual carrying header (including PAV3).
        let header = if matches!(original.source, PalwClaimSourceV2::Attempt) {
            b.extras
                .exec_v2_origin_headers
                .get(&original.accepted_block)
                .map(encode)
                .transpose()
                .map_err(err)?
        } else {
            None
        };
        atomic(b, |b| {
            let key = exact_key(id, *ctx)?;
            if lookup::<SourceV8>(&b.state, DIRECTORY, None, key)?.is_some() {
                return Err(bad("duplicate source admission point"));
            }
            if lookup::<SourceV8>(
                &b.state,
                STORED,
                None,
                stored_key(id, Birth::of(&original))?,
            )?
            .is_some()
            {
                return Err(bad("source stored birth locator already exists"));
            }
            let original_v = put_literal(b, &*original)?;
            let inputs_v = put_literal(b, &*source_inputs)?;
            let control_v = control.as_ref().map(|c| put_literal(b, &**c)).transpose()?;
            let header_v = header
                .as_ref()
                .map(|raw| {
                    let decoded = decode::<Vec<u8>>(raw)?;
                    put_literal(b, &*decoded)
                })
                .transpose()?;
            let policy_v = put_literal(b, &policy)?;
            let params_v = put_literal(b, &original_params.as_slice())?;
            let dependency_v = put_literal(b, &dependencies)?;
            let m = SourceV8 {
                version: 8,
                id,
                point: *ctx,
                stored: Birth::of(&original),
                network,
                forks,
                journal: index as u64,
                ordinal,
                original: original_v,
                inputs: inputs_v,
                control: control_v,
                header: header_v,
                policy: policy_v,
                params: params_v,
                dependencies: dependency_v,
            };
            let raw = encode(&m)?;
            let rv = value(&raw);
            put(b, DIRECTORY, None, key, Some(&m))?;
            put(b, STORED, None, stored_key(id, m.stored)?, Some(&m))?;
            let a = inventory(&m, rv)?;
            #[cfg(test)]
            if matches!(original.source,PalwClaimSourceV2::FreePrompt{..}) {
                let geometry=fresh_fp_source_codec_v18(ctx).map_err(native_err)?;
                assert!(raw.len() as u64<=geometry.manifest_bytes_v18());
                assert!(borsh::object_length(&dependencies).unwrap() as u64<=geometry.dependency_bytes_max_v18());
                assert_eq!(borsh::object_length(&*a).unwrap() as u64,geometry.archive_bytes_v18());
            }
            let raw = encode(&*a)?;
            b.write_kernel_row_encoded_v1(
                PALW_CLAIM_LOCAL_TABLE_V1,
                &akey(value(&raw).data),
                Some(&*a),
            )
            .map_err(native_err)?;
            read(&b.state, rv)?;
            super::palw_claim_local_carrier_lineage_v9::capture_consumer_facts_v9(b,parent,*ctx,id,&original,rv).map_err(native_err)?;
            Ok(())
        })
        .map_err(err)?;
    }
    Ok(())
}

fn atomic<T>(
    b: &mut TransitionBuilder<'_>,
    f: impl FnOnce(&mut TransitionBuilder<'_>) -> Result<T, ContextErrorV2>,
) -> Result<T, ContextErrorV2> {
    let first=b.entries.len();let gc_point=b.claim_local_gc_sequence_v3;let answer=f(b);
    if answer.is_err(){super::palw_claim_local_context_gc_v3::rollback_aux_v3(b,first,gc_point,|table,key|table==PALW_CLAIM_LOCAL_TABLE_V1&&matches!(key.first().copied(),Some(b'N'|b'B'|b'I'|b'A')))?;}
    answer
}

pub(super) fn validate_source_admissions_v8(s: &PalwChainStateV2) -> Result<(), ContextErrorV2> {
    let reader = StateNodesV2(s);
    for table in [DIRECTORY, STORED] {
        nodes::complete_rows(&reader, reader.current(spec(table))?, |key, rv| {
            let view = read(s, rv)?;
            let expected = if table == DIRECTORY {
                exact_key(view.source.id, view.source.point)?
            } else {
                stored_key(view.source.id, view.source.stored)?
            };
            if key != expected.as_byte_slice() {
                return Err(bad("source admission directory leaf identity"));
            }
            Ok(())
        })?;
    }
    if reader.current(spec(3))?.rows != 0 {
        return Err(bad("source temporary literal index not cleared"));
    }
    if let Some(route) = &s.kernel_route {
        for ((table, key), raw) in &route.aux {
            if *table != PALW_CLAIM_LOCAL_TABLE_V1 || key.first() != Some(&b'I') {
                continue;
            }
            let rooted = decode::<ComponentRootV2>(raw)?;
            if rooted.spec.owner != 9 || rooted.spec.schema == 9 {
                continue;
            }
            if key.len() != 18
                || !matches!(rooted.spec.table, DIRECTORY | STORED | 3)
                || rooted.spec != spec(rooted.spec.table)
            {
                return Err(bad("source admission allocated root specification"));
            }
        }
    }
    Ok(())
}
pub(super) fn validate_source_network_v8(
    s: &PalwChainStateV2,
    global: &crate::config::params::Params,
) -> Result<(), PalwStateV2Error> {
    let reader = StateNodesV2(s);
    nodes::complete_rows(
        &reader,
        reader.current(spec(DIRECTORY)).map_err(err)?,
        |_, rv| {
            let view = read(s, rv)?;
            let crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) =
                &global.palw_consensus_mode
            else {
                return Err(bad("source admission requires configured V2 network"));
            };
            if view.source.network != global.consensus_identity_id().as_bytes()
                || view.source.forks
                    != crate::palw_onboarding_v1::palw_envelope_fork_digest_v1(
                        global.genesis.hash,
                        &global.fence_schedule_v1(),
                        view.source.point.daa_score,
                    )
                    .as_bytes()
                || !bundle
                    .state
                    .claim_local_settlement_active_at(view.source.point.daa_score)
            {
                return Err(bad("source admission configured network/fork differs"));
            }
            Ok(())
        },
    )
    .map_err(err)
}
/// Only an actual source-duty owner may call after authentic closure/fee/undo
/// retention authorization. No new unsigned storage fee or expiry is inferred.
pub(super) fn release_source_admission_v8(
    b: &mut TransitionBuilder<'_>,
    id: Hash64,
    point: PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    atomic(b, |b| {
        let key = exact_key(id, point)?;
        let reader = StateNodesV2(&b.state);
        let Some(rv) = nodes::lookup(
            &reader,
            reader.current(spec(DIRECTORY))?,
            key.as_byte_slice(),
        )?
        else {
            return Ok(());
        };
        let view = read(&b.state, rv)?;
        if view.source.id != id || view.source.point != point {
            return Err(bad("source release substituted accepting cohort"));
        }
        if super::palw_claim_local_legacy_terminal_v18::has_native_ledger_refs_v18(&b.state,id,view.original_claim()).map_err(native_err)?{return Ok(())}
        let a = inventory(&view.source, rv)?;
        let raw = encode(&*a)?;
        let aid = value(&raw).data;
        super::palw_claim_local_carrier_lineage_v9::release_consumer_facts_v9(b,id,point).map_err(native_err)?;
        put::<SourceV8>(b, DIRECTORY, None, key, None)?;
        put::<SourceV8>(b, STORED, None, stored_key(id, view.source.stored)?, None)?;
        b.write_kernel_row_encoded_v1::<store::ArchiveReferencesV2>(
            PALW_CLAIM_LOCAL_TABLE_V1,
            &akey(aid),
            None,
        )
        .map_err(native_err)?;
        Ok(())
    })
    .map_err(err)
}

pub(super) fn known_inputs_v8(
    s: &PalwChainStateV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<Option<bool>, PalwStateV2Error> {
    for_stored_claim_v8(s, id, c)?
        .map(|view| view.inputs_known().map_err(err))
        .transpose()
}
/// A cheap branch selector only. The actual typed reader validates the complete
/// source and fails on malformed present data; only true absence uses old F1.
pub(super) fn has_source_admission_v8(
    s: &PalwChainStateV2,
    id: Hash64,
    c: &PalwClaimStateV2,
) -> Result<bool, PalwStateV2Error> {
    let reader = StateNodesV2(s);
    let key = stored_key(id, Birth::of(c)).map_err(err)?;
    Ok(nodes::lookup(
        &reader,
        reader.current(spec(STORED)).map_err(err)?,
        key.as_byte_slice(),
    )
    .map_err(err)?
    .is_some())
}

/// Read by the actual accepting tuple without fabricating subsidy or a prefix
/// identity. The returned lease contains the original native point and body.
/// Stored carrying provenance (notably Attempt RED work) is a different index.
pub(super) fn for_birth_point_v8(
    state: &PalwChainStateV2,
    id: Hash64,
    block: BlockHash,
    daa: u64,
    blue: u64,
) -> Result<Option<SourceAdmissionViewV8>, PalwStateV2Error> {
    let key = hash(1, &(id, block, daa, blue)).map_err(err)?;
    let reader = StateNodesV2(state);
    let Some(rv) = nodes::lookup(
        &reader,
        reader.current(spec(DIRECTORY)).map_err(err)?,
        key.as_byte_slice(),
    ).map_err(err)? else {
        return Ok(None);
    };
    let view = read(state, rv).map_err(err)?;
    let point = view.accepting_point();
    if view.source.id != id || (point.block, point.daa_score, point.blue_score) != (block, daa, blue) {
        return Err(err(bad("source historical lookup changed actual birth tuple")));
    }
    Ok(Some(view))
}

/// Exact historical read for source factories; no current raw claim lookup.
pub(super) fn for_accepting_point_v8(
    state: &PalwChainStateV2,
    id: Hash64,
    point: PalwBlockContextV2,
) -> Result<Option<SourceAdmissionViewV8>, PalwStateV2Error> {
    let Some(view) = for_birth_point_v8(state, id, point.block, point.daa_score, point.blue_score)? else {
        return Ok(None);
    };
    if view.accepting_point() != point {
        return Err(err(bad("source historical lookup changed actual point")));
    }
    Ok(Some(view))
}

/// Exact source point from scoped directory; no subsidy=0 context fabrication.
/// Prefix identity does not carry subsidy, so the authenticated stored point wins.
pub(super) fn point_for_prefix_identity_v8(
    state: &PalwChainStateV2,
    identity: &crate::palw_claim_local_v1::prefix_v1::PalwPrefixExportIdentityV1,
) -> Result<Option<PalwBlockContextV2>, PalwStateV2Error> {
    let Some(view) = for_birth_point_v8(state, identity.source_claim, identity.accepted_block,
        identity.accepted_daa, identity.accepted_blue_score)? else {
        return Ok(None);
    };
    if !matches!(view.original.source, PalwClaimSourceV2::FreePrompt { .. }) {
        return Err(err(bad("scoped prefix source actual point differs")));
    }
    Ok(Some(view.accepting_point()))
}

/// Exact born consumer manifest binding for the carrier-fact native owner.
pub(super) fn manifest_reference_v9(state:&PalwChainStateV2,id:Hash64,point:PalwBlockContextV2)->Result<Option<RowValueV2>,PalwStateV2Error>{
 let reader=StateNodesV2(state);let key=exact_key(id,point).map_err(err)?;
 let Some(rv)=nodes::lookup(&reader,reader.current(spec(DIRECTORY)).map_err(err)?,key.as_byte_slice()).map_err(err)?else{return Ok(None)};
 let view=read(state,rv).map_err(err)?;if view.source.id!=id||view.source.point!=point{return Err(err(bad("carrier consumer manifest substituted born admission")))}Ok(Some(rv))
}

#[cfg(test)]
pub(super) fn assert_stored_birth_alias_rejected_v8(
    state:&PalwChainStateV2,params:&PalwStateParamsV2,extras:&PalwTransitionExtrasV1,
    id:Hash64,original:&PalwClaimStateV2,
){
    // The caller supplies an actual native-fold/cold-restored admission. Add an
    // EXTRA validly hashed directory key without changing its genuine body or
    // either genuine index. The old body/root equality checks alone accept it.
    let source=for_stored_claim_v8(state,id,original).unwrap().unwrap();
    let mut other=original.clone();other.accepted_daa+=1;other.accepted_blue_score+=1;
    let key=stored_key(id,Birth::of(&other)).unwrap();
    let mut b=TransitionBuilder::new(state,params,false,false,false,false,extras);
    put(&mut b,STORED,None,key,Some(&*source.source)).unwrap();
    assert!(for_stored_claim_v8(&b.state,id,original).unwrap().is_some(),"genuine canonical birth remains readable");
    assert!(matches!(for_stored_claim_v8(&b.state,id,&other),Err(PalwStateV2Error::CarriageInconsistent(_))),
        "an extra stored key must not alias another birth with identical roots/bond/class");
    assert!(validate_source_admissions_v8(&b.state).is_err(),"cold validation also rejects the extra alias");
}

/// Complete source-local admission material. All literals are bound by the
/// actual A8 descriptor and its rooted archive inventory. No complete state or
/// computation/paid authority is constructed from this projection.
pub(super) fn visit_checkpoint_material_v10(
    state: &PalwChainStateV2,
    id: Hash64,
    current: &PalwClaimStateV2,
    mut visit: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
) -> Result<Option<PalwBlockContextV2>, PalwStateV2Error> {
    let Some(view) = for_stored_claim_v8(state, id, current)? else { return Ok(None) };
    let point = view.accepting_point();
    let reader = StateNodesV2(state);
    let stored = stored_key(id, Birth::of(current)).map_err(err)?;
    let manifest = nodes::lookup(&reader, reader.current(spec(STORED)).map_err(err)?, stored.as_byte_slice())
        .map_err(err)?.ok_or_else(||err(ContextErrorV2::Unavailable("scoped checkpoint source manifest absent")))?;
    // Use precisely the descriptor opened by the validated STORED alias. A
    // different directory coordinate must never substitute another body.
    let refs = inventory(&view.source, manifest).map_err(err)?;
    for literal in &refs.literals {
        // Keep the genuine decoded B owner through the callback. Neither a
        // partial world nor a dangling borrowed temporary is exposed.
        let body = reader.body(*literal).map_err(err)?;
        visit(body.as_slice())?;
    }
    Ok(Some(point))
}

/// Stream every retained exact accepting cohort, including retired consumers.
/// DIRECTORY is unique per (claim, accepting block/DAA/blue), unlike STORED's
/// carrying provenance. The existing reader authenticates each scoped manifest,
/// original and literal inputs; no live raw-ID claim or full frame is synthesized.
/// Trie traversal and each decoded view retain their own native preflight leases.
pub(super) fn visit_retained_admissions_v8(
    state: &PalwChainStateV2,
    mut visit: impl FnMut(Hash64, SourceAdmissionViewV8) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    let reader = StateNodesV2(state);
    let mut callback_error = None;
    let result = nodes::complete_rows(&reader, reader.current(spec(DIRECTORY)).map_err(err)?, |key, rv| {
        let view = read(state, rv)?;
        if key != exact_key(view.source.id, view.source.point)?.as_byte_slice() {
            return Err(bad("source admission visitor changed exact accepting identity"));
        }
        visit(view.source.id, view).map_err(|e| {
            callback_error = Some(e);
            bad("source admission visitor refused")
        })
    });
    match result {
        Ok(()) => Ok(()),
        Err(e) => Err(callback_error.unwrap_or_else(|| err(e))),
    }
}
/// Exact directory-presence check used only AFTER the complete authenticated
/// scoped visitor, to avoid visiting its unchanged legacy F1 copy a second time.
pub(super) fn retained_accepting_point_present_v8(
    state: &PalwChainStateV2, id: Hash64, point: PalwBlockContextV2,
) -> Result<bool, PalwStateV2Error> {
    let reader = StateNodesV2(state);
    let key = exact_key(id, point).map_err(err)?;
    Ok(nodes::lookup(&reader, reader.current(spec(DIRECTORY)).map_err(err)?, key.as_byte_slice())
        .map_err(err)?.is_some())
}

// Readonly quote binding: no caller-constructible source or paid permit.
impl SourceAdmissionViewV8 {
    pub(super) fn archive_quote_claim_id_v12(&self)->Hash64 {self.source.id}
    pub(super) fn archive_quote_record_bytes_v12(&self)->Result<u64,PalwStateV2Error> {
        borsh::object_length(&*self.source).map(|n|n as u64).map_err(|_|err(bad("source quote record codec")))
    }
    pub(super) fn archive_quote_record_id_v12(&self)->Result<Hash64,PalwStateV2Error> {
        hash(251,&*self.source).map_err(err)
    }
}

/// Private-owner codec geometry only: this does not mint an admission, a paid
/// receipt or permission to collect retained source/dependency duties.
pub(super) struct NativeFreshFpSourceCodecV18 {
    manifest:u64,dependencies:u64,archive:u64,
}
impl NativeFreshFpSourceCodecV18 {
    pub(super) fn manifest_bytes_v18(&self)->u64 {self.manifest}
    pub(super) fn dependency_bytes_max_v18(&self)->u64 {self.dependencies}
    pub(super) fn archive_bytes_v18(&self)->u64 {self.archive}
    pub(super) fn directory_spec_v18(&self)->ComponentSpecV2 {spec(DIRECTORY)}
    pub(super) fn stored_spec_v18(&self)->ComponentSpecV2 {spec(STORED)}
    pub(super) fn temporary_literal_spec_v18(&self)->ComponentSpecV2 {spec(3)}
    // Six temporary literals: original, inputs, control, policy, params, deps.
    // Each creates/removes its temporary root; this is separate from B dedup.
    pub(super) fn temporary_literal_publications_v18(&self)->u64 {6}
}
pub(super) fn fresh_fp_source_codec_v18(
    point:&PalwBlockContextV2,
)->Result<NativeFreshFpSourceCodecV18,PalwStateV2Error> {
    // Fixed schema serialization using this owner's private types. No future
    // SourceV8/Claim is constructed or exposed as acceptance evidence.
    let width=|v:Result<usize,std::io::Error>|v.map(|n|n as u64).map_err(|_|err(bad("source codec geometry")));
    let birth=Birth{block:point.block,daa:point.daa_score,blue:point.blue_score};
    let rv=RowValueV2{data:Hash64::default(),bytes:0};
    let manifest=width(borsh::object_length(&(8u16,Hash64::default(),*point,birth,[0u8;32],[0u8;32],0u64,Some(0u64),rv,rv,Some(rv),None::<RowValueV2>,rv,rv,rv)))?;
    // capture_admissions_v8 pushes at most the one exact native prefix use.
    let dependencies=4+width(borsh::object_length(&Dependency{source:Hash64::default(),birth}))?;
    // inventory(SourceV8): manifest + original + inputs + policy + params +
    // deps + FP control; no Attempt carrying header in this FP cohort.
    let archive=2+4+4+7*width(borsh::object_length(&rv))?;
    Ok(NativeFreshFpSourceCodecV18{manifest,dependencies,archive})
}

/// Exact per-source immutable references and discovery keys. This visitor reads
/// the authenticated native source; it grants no financial/cleanup authority.
pub(super) fn visit_native_archive_references_v19(
    s:&PalwChainStateV2,id:Hash64,p:PalwBlockContextV2,
    visit:impl FnOnce(&store::ArchiveReferencesV2,RowValueV2,Hash64,Hash64,Hash64)->Result<(),PalwStateV2Error>,
)->Result<bool,PalwStateV2Error> {
    let Some(view)=for_accepting_point_v8(s,id,p)? else{return Ok(false)};
    let key=exact_key(id,p).map_err(err)?;
    let reader=StateNodesV2(s);
    let rv=nodes::lookup(&reader,reader.current(spec(DIRECTORY)).map_err(err)?,key.as_byte_slice()).map_err(err)?
        .ok_or_else(||err(bad("source archive directory disappeared")))?;
    let refs=inventory(&view.source,rv).map_err(err)?;
    let raw=encode(&*refs).map_err(err)?;
    visit(&refs,rv,key,stored_key(id,view.source.stored).map_err(err)?,value(&raw).data)?;
    Ok(true)
}

/// Actual stored/carrying tuple lookup, distinct from accepting-point birth.
/// Returns the owner-decoded original; no synthetic Claim or guessed subsidy.
pub(super) fn for_stored_birth_v24(s:&PalwChainStateV2,id:Hash64,block:BlockHash,daa:u64,blue:u64)->Result<Option<SourceAdmissionViewV8>,PalwStateV2Error>{
 let born=Birth{block,daa,blue};let key=stored_key(id,born).map_err(err)?;let reader=StateNodesV2(s);let Some(rv)=nodes::lookup(&reader,reader.current(spec(STORED)).map_err(err)?,key.as_byte_slice()).map_err(err)?else{return Ok(None)};let view=read(s,rv).map_err(err)?;
 if view.source.id!=id||view.source.stored!=born||Birth::of(view.original_claim())!=born{return Err(err(bad("source stored coordinates substituted native birth")))}Ok(Some(view))
}
