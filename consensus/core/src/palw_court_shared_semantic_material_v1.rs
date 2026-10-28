//! Private all-ID semantic material owner. No public wire, availability/retention
//! promise, funded admission, parser-validity or complete-duty authority.
use super::SharedTirBuilderStoreV1 as Store;
use crate::palw_fp_constraint_job_v1::{PalwTokenTableEogOpeningV1 as Eog, PalwTokenTableOpeningV1 as Rendering};
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1 as E, NativeDemandLeaseV1, NativeHeapV1, NativeRetainedV1, NativeWireScanV1,
    TirNativeRequestsV1, preflight_native_value_v1, reserve_native_requests_v1,
};
use crate::{Hash64, palw_state_v2::PalwChainStateV2};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SemanticMaterialCountsV1 {
    pub vocabulary: u32,
    pub present: u64,
    pub absent: u64,
    pub rendering_bytes: u64,
    pub eog_ids: u64,
    pub opening_siblings: u64,
    pub encoded_opening_bytes: u64,
    pub raw_bytes: u64,
}
/// Opaque actual decoded/authenticated bodies. Ownership includes the raw copy,
/// outer Vec and all proof/body payloads; no unleased extraction or Deserialize.
pub(crate) struct SemanticMaterialInventoryV1 {
    source: Hash64,
    claim: Hash64,
    scope: Hash64,
    semantics: Hash64,
    inventory: Hash64,
    renderings: Vec<Rendering>,
    eog: Eog,
    raw: Vec<u8>,
    counts: SemanticMaterialCountsV1,
}
impl SemanticMaterialInventoryV1 {
    pub(crate) fn counts_v1(&self) -> SemanticMaterialCountsV1 {
        self.counts
    }
    pub(crate) fn rendering_v1(&self, id: u32) -> Option<&Rendering> {
        self.renderings.get(id as usize)
    }
    pub(crate) fn eog_v1(&self) -> &Eog {
        &self.eog
    }
    pub(crate) fn raw_v1(&self) -> &[u8] {
        &self.raw
    }
    pub(crate) fn check_scope_v1(
        &self,
        source: Hash64,
        claim: Hash64,
        scope: Hash64,
        semantics: Hash64,
        inventory: Hash64,
    ) -> Result<(), E<String>> {
        if (self.source, self.claim, self.scope, self.semantics, self.inventory) != (source, claim, scope, semantics, inventory) {
            return Err(bad("semantic retained material from another source/scope/EOF"));
        }
        Ok(())
    }
}
fn bad(s: &str) -> E<String> {
    E::Decode(s.into())
}
fn resource(s: &str) -> E<String> {
    E::Resource(NativeCoverageV1::unsupported::<SemanticMaterialInventoryV1>("private semantic material", s))
}
fn add(a: &mut u64, b: u64) -> Result<(), E<String>> {
    *a = a.checked_add(b).ok_or_else(|| resource("semantic material count overflow"))?;
    Ok(())
}
fn wire_error(e: std::io::Error) -> E<String> {
    match e.kind() {
        std::io::ErrorKind::OutOfMemory | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Unsupported => resource(&e.to_string()),
        _ => E::Decode(e.to_string()),
    }
}
fn shape<T: NativeWireScanV1>(bytes: &[u8]) -> Result<u64, E<String>> {
    preflight_native_value_v1::<T>(bytes, bytes.len() as u64).map(|q| q.logical_requests_v1()).map_err(wire_error)
}
#[derive(Clone, Copy)]
struct Scope {
    source: Hash64,
    claim: Hash64,
    scope: Hash64,
    semantics: Hash64,
    inventory: Hash64,
    table: Hash64,
    tokenizer: Hash64,
    vocab: u32,
    raw_bytes: u64,
}

#[derive(Clone, Copy)]
struct TableScope {
    source: Hash64,
    claim: Hash64,
    scope: Hash64,
    semantics: Hash64,
    table: Hash64,
    vocab: u32,
}
impl Scope {
    fn table_v1(self) -> TableScope {
        TableScope {
            source: self.source,
            claim: self.claim,
            scope: self.scope,
            semantics: self.semantics,
            table: self.table,
            vocab: self.vocab,
        }
    }
}
/// Table-only retained data. No invented raw EOF, raw length or raw supplier.
/// Rendering correctness is still adjudicated by the existing primitive.
pub(crate) struct SemanticTableMaterialV1 {
    binding: TableScope,
    renderings: Vec<Rendering>,
    eog: Eog,
    counts: SemanticMaterialCountsV1,
}
impl SemanticTableMaterialV1 {
    /// raw_bytes is zero because this owner contains no raw artifact body.
    pub(crate) fn counts_v1(&self) -> SemanticMaterialCountsV1 {
        self.counts
    }
    fn check_scope_v1(&self, source: Hash64, claim: Hash64, scope: Hash64, semantics: Hash64) -> Result<(), String> {
        if (self.binding.source, self.binding.claim, self.binding.scope, self.binding.semantics) != (source, claim, scope, semantics) {
            return Err("retained semantic table from another source/scope/semantics".into());
        }
        Ok(())
    }
}
fn output_requests(scope: Scope, renderings: &[&[u8]], eog: &[u8]) -> Result<u64, E<String>> {
    let mut q = TirNativeRequestsV1::new();
    q.add_v1(table_requests::<SemanticMaterialInventoryV1>(scope.table_v1(), renderings, eog)?).map_err(E::Resource)?;
    q.exact_v1::<u8>(scope.raw_bytes).map_err(E::Resource)?;
    Ok(q.bytes_v1())
}
fn table_requests<T>(scope: TableScope, renderings: &[&[u8]], eog: &[u8]) -> Result<u64, E<String>> {
    if scope.vocab == 0
        || u64::from(scope.vocab).checked_add(1).is_none_or(|n| n > crate::palw_fp_constraint_job_v1::PALW_TOKEN_TABLE_MAX_LEAVES_V1)
        || renderings.len() as u64 != u64::from(scope.vocab)
    {
        return Err(bad("semantic all-ID material cardinality"));
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<T>(1).map_err(E::Resource)?;
    q.exact_v1::<Rendering>(u64::from(scope.vocab)).map_err(E::Resource)?;
    for body in renderings {
        q.add_v1(shape::<Rendering>(body)?).map_err(E::Resource)?;
    }
    q.add_v1(shape::<Eog>(eog)?).map_err(E::Resource)?;
    // EOG stream uses empty relevant/hits Vecs, stack hash continuation, and a
    // constant-size Merkle fold. Existing table leaf hash is streaming as well.
    q.exact_v1::<crate::palw_eog_stream_v1::PalwEogStreamV1>(1).map_err(E::Resource)?;
    Ok(q.bytes_v1())
}
fn decode_material(scope: Scope, renderings: &[&[u8]], eog: &[u8], raw: &[u8]) -> Result<SemanticMaterialInventoryV1, E<String>> {
    if raw.len() as u64 != scope.raw_bytes
        || crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(raw) != scope.tokenizer
    {
        return Err(bad("semantic raw body changes actual EOF length or immutable hash"));
    }
    let (values, eog, mut counts) = decode_table(scope.table_v1(), renderings, eog)?;
    counts.raw_bytes = scope.raw_bytes;
    let mut owned_raw = Vec::new();
    owned_raw.try_reserve_exact(raw.len()).map_err(|_| resource("semantic raw owner allocation"))?;
    owned_raw.extend_from_slice(raw);
    Ok(SemanticMaterialInventoryV1 {
        source: scope.source,
        claim: scope.claim,
        scope: scope.scope,
        semantics: scope.semantics,
        inventory: scope.inventory,
        renderings: values,
        eog,
        raw: owned_raw,
        counts,
    })
}
fn decode_table(
    scope: TableScope,
    renderings: &[&[u8]],
    eog: &[u8],
) -> Result<(Vec<Rendering>, Eog, SemanticMaterialCountsV1), E<String>> {
    let eog: Eog = borsh::from_slice(eog).map_err(wire_error)?;
    let count = u32::try_from(eog.ids.len()).map_err(|_| bad("semantic EOG count does not fit native stream"))?;
    // Do NOT use Eog::verifies alone: it does not enforce each id<vocab.
    let mut stream = crate::palw_eog_stream_v1::PalwEogStreamV1::new(scope.scope, scope.table, scope.vocab, count, &[], &eog.opening)
        .map_err(E::Decode)?;
    for part in eog.ids.chunks(crate::palw_eog_stream_v1::PALW_EOG_STREAM_PART_IDS_V1) {
        stream.push(scope.scope, stream.offset(), part).map_err(E::Decode)?;
    }
    stream.finish().map_err(E::Decode)?;
    let mut values = Vec::new();
    values.try_reserve_exact(renderings.len()).map_err(|_| resource("semantic rendering outer Vec allocation"))?;
    let mut counts = SemanticMaterialCountsV1 {
        vocabulary: scope.vocab,
        present: 0,
        absent: 0,
        rendering_bytes: 0,
        eog_ids: eog.ids.len() as u64,
        opening_siblings: eog.opening.siblings.len() as u64,
        encoded_opening_bytes: borsh::object_length(&eog).map_err(|e| E::Decode(e.to_string()))? as u64,
        raw_bytes: 0,
    };
    for (id, bytes) in renderings.iter().enumerate() {
        let value: Rendering = borsh::from_slice(bytes).map_err(wire_error)?;
        if value.id as usize != id || !value.verifies(&scope.table, scope.vocab) {
            return Err(bad("semantic rendering is missing, duplicate, reordered or not rooted in actual table"));
        }
        if let Some(body) = &value.rendering {
            add(&mut counts.present, 1)?;
            add(&mut counts.rendering_bytes, body.len() as u64)?;
        } else {
            add(&mut counts.absent, 1)?;
        }
        add(&mut counts.opening_siblings, value.opening.siblings.len() as u64)?;
        add(&mut counts.encoded_opening_bytes, bytes.len() as u64)?;
        values.push(value);
    }
    Ok((values, eog, counts))
}
impl Store<'_> {
    /// All inputs are existing inner Borsh bodies, not new public transport tags.
    /// Store/source/raw carrier construction remains separately caller-owned.
    pub(crate) fn retain_semantic_material_v1(
        &self,
        state: &PalwChainStateV2,
        claim: Hash64,
        renderings: &[&[u8]],
        eog: &[u8],
        raw: &[u8],
    ) -> Result<NativeRetainedV1<SemanticMaterialInventoryV1>, E<String>> {
        self.retain_semantic_reserved_v1(state, claim, renderings, eog, raw, reserve_native_requests_v1)
    }
    fn retain_semantic_reserved_v1<F>(
        &self,
        state: &PalwChainStateV2,
        claim: Hash64,
        renderings: &[&[u8]],
        eog: &[u8],
        raw: &[u8],
        reserve: F,
    ) -> Result<NativeRetainedV1<SemanticMaterialInventoryV1>, E<String>>
    where
        F: FnOnce(u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1>,
    {
        let (table, mut lease) = self.semantic_table_context_reserved_v1(state, claim, reserve)?;
        let c = crate::palw_court_shared_tir_state_v1::context(state, claim).map_err(E::Decode)?;
        let sem = state.claim_semantics_v1(&claim).ok_or_else(|| bad("semantic immutable claim snapshot missing"))?;
        let eof = state.staged_tokenizer_inventory_v1(sem.tokenizer_id).ok_or_else(|| bad("semantic actual rooted EOF missing"))?;
        eof.check_v1().map_err(E::Decode)?;
        if eof.artifact_hash != sem.tokenizer_id
            || eof.completed_daa > c.original_accepted_daa
            || sem.raw_tokenizer_bytes != Some(eof.actual_bytes)
        {
            return Err(bad("semantic actual EOF differs from immutable original"));
        }
        let scope = Scope {
            source: table.source,
            claim: table.claim,
            scope: table.scope,
            semantics: table.semantics,
            inventory: eof.inventory_id,
            table: table.table,
            tokenizer: sem.tokenizer_id,
            vocab: table.vocab,
            raw_bytes: eof.actual_bytes,
        };
        let additional = output_requests(scope, renderings, eog)?;
        if let Some(ticket) = lease.as_mut() {
            ticket.grow_memory_v1(additional).map_err(E::Resource)?;
        }
        let value = decode_material(scope, renderings, eog, raw)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(value, lease))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_native_resource_v1::{NativeDemandCheckerV1, NativeSnapshotResourceLimitsV1};
    fn fixture() -> (Scope, Vec<Vec<u8>>, Vec<u8>, Vec<u8>) {
        let raw = b"actual raw bytes; validity is not asserted".to_vec();
        let table = crate::palw_fp_constraint_job_v1::PalwTokenTableV1 {
            entries: vec![Some(vec![1, 2]), Some(vec![]), None],
            eog_token_ids: vec![2],
            tokenizer_id: crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(&raw),
        };
        let h = |x| Hash64::from_bytes([x; 64]);
        let scope = Scope {
            source: h(1),
            claim: h(2),
            scope: h(3),
            semantics: h(4),
            inventory: h(5),
            table: table.root(),
            tokenizer: table.tokenizer_id,
            vocab: 3,
            raw_bytes: raw.len() as u64,
        };
        let bodies = (0..3).map(|id| borsh::to_vec(&table.opening_of(id).unwrap()).unwrap()).collect();
        (scope, bodies, borsh::to_vec(&table.eog_opening().unwrap()).unwrap(), raw)
    }
    #[test]
    fn semantic_table_only_owner_quota_has_no_raw_or_eof_obligation() {
        let (scope, bodies, eog, _raw) = fixture();
        let binding = scope.table_v1();
        let refs = bodies.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let q = table_requests::<SemanticTableMaterialV1>(binding, &refs, &eog).unwrap();
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: q, database_bytes: 0 });
        let lease = checker.reserve_requests_v1(q, 0).unwrap();
        let (renderings, eog, counts) = decode_table(binding, &refs, &eog).unwrap();
        let held = NativeRetainedV1::from_owner_reserved_v1(SemanticTableMaterialV1 { binding, renderings, eog, counts }, Some(lease));
        assert_eq!(held.counts_v1().raw_bytes, 0);
        assert_eq!(held.counts_v1().vocabulary, 3);
        assert_eq!(checker.outstanding(), (q, 0));
        assert!(checker.reserve_requests_v1(1, 0).is_err());
        drop(held);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn semantic_material_retained_all_id_authentication_and_quota_release() {
        let (s, b, e, r) = fixture();
        let refs = b.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let q = output_requests(s, &refs, &e).unwrap();
        assert!(q > r.len() as u64);
        let tiny = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: q - 1, database_bytes: 0 });
        assert!(tiny.reserve_requests_v1(q, 0).is_err());
        assert_eq!(tiny.outstanding(), (0, 0));
        let grow = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: q, database_bytes: 0 });
        let mut prefix = grow.reserve_requests_v1(1, 0).unwrap();
        assert!(prefix.grow_memory_v1(q).is_err());
        assert_eq!(grow.outstanding(), (1, 0));
        prefix.grow_memory_v1(q - 1).unwrap();
        assert_eq!(grow.outstanding(), (q, 0));
        drop(prefix);
        assert_eq!(grow.outstanding(), (0, 0));
        let checker = NativeDemandCheckerV1::new(NativeSnapshotResourceLimitsV1 { memory_bytes: q, database_bytes: 0 });
        // Same preflight quote/decode/owner transfer as real Store; this local
        // test does not authenticate Scope from state (integration below does).
        let lease = checker.reserve_requests_v1(q, 0).unwrap();
        let value = decode_material(s, &refs, &e, &r).unwrap();
        let owner = NativeRetainedV1::from_owner_reserved_v1(value, Some(lease));
        assert_eq!(checker.outstanding(), (q, 0));
        assert_eq!(owner.counts_v1().present, 2);
        assert_eq!(owner.counts_v1().absent, 1);
        assert_eq!(owner.counts_v1().rendering_bytes, 2);
        assert_eq!(owner.rendering_v1(1).unwrap().rendering, Some(vec![]));
        assert_eq!(owner.rendering_v1(2).unwrap().rendering, None);
        assert_eq!(owner.raw_v1(), r);
        assert_eq!(owner.eog_v1().ids, vec![2]);
        owner.check_scope_v1(s.source, s.claim, s.scope, s.semantics, s.inventory).unwrap();
        assert!(owner.check_scope_v1(Hash64::default(), s.claim, s.scope, s.semantics, s.inventory).is_err());
        drop(owner);
        assert_eq!(checker.outstanding(), (0, 0));
    }
    #[test]
    fn semantic_material_rejects_missing_duplicate_tampered_and_range_invalid_eog() {
        let (s, b, e, r) = fixture();
        let refs = b.iter().map(Vec::as_slice).collect::<Vec<_>>();
        assert!(output_requests(s, &refs[..2], &e).is_err());
        assert!(decode_material(s, &[&b[0], &b[0], &b[2]], &e, &r).is_err());
        assert!(decode_material(s, &[&b[1], &b[0], &b[2]], &e, &r).is_err());
        let mut tampered: Rendering = borsh::from_slice(&b[0]).unwrap();
        tampered.rendering.as_mut().unwrap()[0] ^= 1;
        let tampered = borsh::to_vec(&tampered).unwrap();
        assert!(decode_material(s, &[&tampered, &b[1], &b[2]], &e, &r).is_err());
        let mut raw = r.clone();
        raw[0] ^= 1;
        assert!(decode_material(s, &refs, &e, &raw).is_err());
        assert!(decode_material(s, &refs, &e, &r[..r.len() - 1]).is_err());
        // Root-consistent malicious EOG leaf: simple Eog::verifies accepts the
        // out-of-range ID, while actual native EOG stream must reject it.
        let table = crate::palw_fp_constraint_job_v1::PalwTokenTableV1 {
            entries: vec![None, None, None],
            eog_token_ids: vec![3],
            tokenizer_id: s.tokenizer,
        };
        let invalid = table.eog_opening().unwrap();
        assert!(invalid.verifies(&table.root(), 3));
        let bad_scope = Scope { table: table.root(), ..s };
        let bodies = (0..3).map(|id| borsh::to_vec(&table.opening_of(id).unwrap()).unwrap()).collect::<Vec<_>>();
        let bodies = bodies.iter().map(Vec::as_slice).collect::<Vec<_>>();
        assert!(decode_material(bad_scope, &bodies, &borsh::to_vec(&invalid).unwrap(), &r).is_err());
        // Truncated huge declaration refused by schema walk BEFORE decoder.
        assert!(shape::<Eog>(&u32::MAX.to_le_bytes()).is_err());
        assert!(matches!(wire_error(std::io::Error::new(std::io::ErrorKind::TimedOut, "test scanner timeout")), E::Resource(_)));
        assert!(matches!(wire_error(std::io::Error::new(std::io::ErrorKind::OutOfMemory, "test allocation")), E::Resource(_)));
        assert!(matches!(wire_error(std::io::Error::new(std::io::ErrorKind::InvalidData, "test malformed")), E::Decode(_)));
        let mut total = u64::MAX;
        assert!(matches!(add(&mut total, 1), Err(E::Resource(_))));
        assert_eq!(total, u64::MAX);
    }
}

impl Store<'_> {
    /// The evidence borrows the actual retained owner, so its decoded table/raw
    /// payload ticket cannot be dropped while evidence is live. Proof owners
    /// remain separately borrowed/admitted. Existing native builder still checks
    /// current source, rooted EOF, actor/signature, rules, parser and material ID.
    /// No new source/builder scratch or signed-intent clone is admitted here.
    pub(crate) fn build_tokenizer_terminal_retained_v1<'a>(
        &self,
        state: &PalwChainStateV2,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
        intent: &super::tokenizer_terminal::SignedTokenizerTerminalIntentV1,
        material: &'a NativeRetainedV1<SemanticMaterialInventoryV1>,
        proofs: &'a [crate::palw_json_key_set_v1::PalwJsonKeyCompactProofV1],
        budget: crate::palw_tokenizer_table_stream_v1::canonical_replay::Budget,
    ) -> Result<super::tokenizer_terminal::TokenizerTerminalEvidenceV1<'a>, String> {
        material
            .check_scope_v1(self.source.source_id_v1(), intent.claim, intent.scope_id, intent.semantics_id, intent.inventory_id)
            .map_err(|e| e.to_string())?;
        let rendering = material.rendering_v1(intent.token_id).ok_or("retained tokenizer target outside actual vocabulary")?;
        self.build_tokenizer_terminal(
            state,
            params,
            rules,
            intent,
            super::tokenizer_terminal::TokenizerMaterialV1 { raw: material.raw_v1(), proofs, rendering },
            budget,
        )
    }
}

impl Store<'_> {
    fn semantic_table_context_reserved_v1<F>(
        &self,
        state: &PalwChainStateV2,
        claim: Hash64,
        reserve: F,
    ) -> Result<(TableScope, Option<NativeDemandLeaseV1>), E<String>>
    where
        F: FnOnce(u64) -> Result<Option<NativeDemandLeaseV1>, NativeCoverageV1>,
    {
        let c = crate::palw_court_shared_tir_state_v1::context(state, claim).map_err(E::Decode)?;
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(state, claim)
            .ok_or_else(|| bad("semantic original frame missing"))?;
        let job = &frame.original_v1().job;
        let record = state.tir_class_v1(&job.class_id).ok_or_else(|| bad("semantic registered class missing"))?;
        let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, job.class_id)
            .ok_or_else(|| bad("semantic layout missing"))?;
        let mut q = TirNativeRequestsV1::new();
        q.add_v1(crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::native_construction_requests_v1(
            record,
            layout,
            frame.header_v1(),
        )?)
        .map_err(E::Resource)?;
        let clone = frame.native_heap_v1("semantic current original identity clone");
        q.add_v1(clone.bytes().ok_or(E::Resource(clone))?).map_err(E::Resource)?;
        let encoded = borsh::object_length(frame).map_err(|e| E::Decode(e.to_string()))? as u64;
        q.exact_v1::<u8>(encoded.max(1024).checked_mul(2).ok_or_else(|| resource("semantic frame hash writer overflow"))?)
            .map_err(E::Resource)?;
        let lease = reserve(q.bytes_v1()).map_err(E::Resource)?;
        {
            let current = c.source(state).map_err(E::Decode)?;
            if current.source_id_v1() != self.source.source_id_v1() || current.header_v1() != self.source.header_v1() {
                return Err(bad("semantic Store from another current original source"));
            }
        }
        let sem = state.claim_semantics_v1(&claim).ok_or_else(|| bad("semantic immutable claim snapshot missing"))?;
        if !sem.matches_context(&self.source.header_v1().job_context)
            || !sem.well_formed_for(&job.class_id, &self.source.header_v1().artifact_root)
            || sem.bound_from_daa > c.original_accepted_daa
            || sem.token_table_root.is_none()
            || crate::palw_fp_constraint_job_v1::palw_fp_constraint_table_root_v1(job)
                .is_some_and(|root| Some(root) != sem.token_table_root)
        {
            return Err(bad("semantic current original table/context mismatch"));
        }
        Ok((
            TableScope {
                source: self.source.source_id_v1(),
                claim,
                scope: c.scope_id,
                semantics: sem.commitment(),
                table: sem.token_table_root.unwrap(),
                vocab: self.source.vocab_v1(),
            },
            lease,
        ))
    }
    /// Table-only means no raw bytes are supplied or required. Current original
    /// table scope and every actual opening are authenticated by the same code
    /// used by the full raw+EOF owner, without a fabricated inventory certificate.
    pub(crate) fn retain_semantic_table_material_v1(
        &self,
        state: &PalwChainStateV2,
        claim: Hash64,
        renderings: &[&[u8]],
        eog: &[u8],
    ) -> Result<NativeRetainedV1<SemanticTableMaterialV1>, E<String>> {
        let (binding, mut lease) = self.semantic_table_context_reserved_v1(state, claim, reserve_native_requests_v1)?;
        let additional = table_requests::<SemanticTableMaterialV1>(binding, renderings, eog)?;
        if let Some(ticket) = lease.as_mut() {
            ticket.grow_memory_v1(additional).map_err(E::Resource)?;
        }
        let (renderings, eog, counts) = decode_table(binding, renderings, eog)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(SemanticTableMaterialV1 { binding, renderings, eog, counts }, lease))
    }
    pub(crate) fn build_constraint_terminal_retained_v1<'a>(
        &self,
        state: &PalwChainStateV2,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
        intent: &super::constraint_terminal::SignedConstraintTerminalIntentV1,
        material: &'a NativeRetainedV1<SemanticTableMaterialV1>,
        budget: crate::palw_constraint_stream_court_v1::canonical_replay::WorkBudget,
    ) -> Result<super::constraint_terminal::ConstraintTerminalEvidenceV1<'a>, String> {
        material.check_scope_v1(self.source.source_id_v1(), intent.claim, intent.scope_id, intent.semantics_id)?;
        self.build_constraint_terminal(
            state,
            params,
            rules,
            intent,
            super::constraint_terminal::ConstraintMaterialV1 { eog: &material.eog, renderings: &material.renderings },
            budget,
        )
    }
}

impl Store<'_> {
    pub(crate) fn build_constraint_stop_terminal_retained_v1<'a>(
        &self,
        state: &PalwChainStateV2,
        params: &crate::palw_state_v2::PalwStateParamsV2,
        rules: &crate::palw_court_close_receipt_v1::NativeCourtRulesV1,
        intent: &super::constraint_stop::SignedConstraintStopIntentV1,
        material: &'a NativeRetainedV1<SemanticTableMaterialV1>,
        budget: crate::palw_constraint_stream_court_v1::canonical_replay::WorkBudget,
    ) -> Result<super::constraint_stop::ConstraintStopEvidenceV1<'a>, String> {
        material.check_scope_v1(self.source.source_id_v1(), intent.claim, intent.scope_id, intent.semantics_id)?;
        self.build_constraint_stop_terminal_v1(
            state,
            params,
            rules,
            intent,
            super::constraint_terminal::ConstraintMaterialV1 { eog: &material.eog, renderings: &material.renderings },
            budget,
        )
    }
}
