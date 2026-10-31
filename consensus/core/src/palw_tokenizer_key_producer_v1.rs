//! Private local material production. Does not accept an original, supply receipt,
//! signed move, funding, or complete-grammar capability. All paths authenticate data.
use super::*;
use crate::palw_json_key_set_v1::producer::ProducerTree;
use crate::palw_native_resource_v1::{
    NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, TirNativeRequestsV1, btree_node_envelope_bytes_v1,
    reserve_native_requests_v1,
};

type Error = NativeDecodeErrorV1<String>;
#[derive(Clone, Copy)]
pub(crate) struct Budget {
    pub(crate) max_proofs: u64,
    pub(crate) max_nodes: u64,
    pub(crate) max_requests: u64,
    pub(crate) max_hash_calls: u64,
    pub(crate) max_hash_bytes: u64,
}
#[derive(Debug)]
pub(crate) struct Generated {
    artifact_binding: (Hash64, u64),
    signed_key_bound: u64,
    proofs: Vec<PalwJsonKeyCompactProofV1>,
    root: Hash64,
    pub(crate) hash_calls: u64,
    pub(crate) hash_bytes: u64,
    pub(crate) native_proof_calls: u64,
    pub(crate) raw_packet_calls: u64,
}
impl Generated {
    pub(crate) fn artifact_binding_v1(&self) -> (Hash64, u64) {
        self.artifact_binding
    }
    pub(crate) fn key_work_bound_v1(&self) -> u64 {
        self.signed_key_bound
    }
    pub(crate) fn proofs(&self) -> &[PalwJsonKeyCompactProofV1] {
        &self.proofs
    }
    pub(crate) fn root(&self) -> Hash64 {
        self.root
    }
}
fn resource(why: &'static str) -> Error {
    NativeDecodeErrorV1::Resource(NativeCoverageV1::unsupported::<Generated>("tokenizer data proof producer", why))
}
fn decode(e: impl ToString) -> Error {
    NativeDecodeErrorV1::Decode(e.to_string())
}

pub(crate) fn generate(base: &PalwTokenizerTableStreamV1, raw: &[u8], budget: Budget) -> Result<NativeRetainedV1<Generated>, Error> {
    generate_reserved(base, raw, budget, reserve_native_requests_v1)
}
/// Same production construction path, injectable only by owner-local tests.
pub(super) fn generate_reserved<R>(
    base: &PalwTokenizerTableStreamV1,
    raw: &[u8],
    budget: Budget,
    reserve: R,
) -> Result<NativeRetainedV1<Generated>, Error>
where
    R: FnOnce(u64) -> Result<Option<crate::palw_native_resource_v1::NativeDemandLeaseV1>, NativeCoverageV1>,
{
    base.check().map_err(decode)?;
    if base.tokenizer_offset() != 0
        || base.key_events() != 0
        || base.pending_key().is_some()
        || base.parser.is_sealed_v1()
        || base.rendering_offset() != 0
        || raw.len() as u64 != base.operand_raw_length
    {
        return Err(decode("key production requires pristine actual native Court and exact operand length"));
    }
    let eof = crate::palw_tokenizer_json_stream_v1::palw_tokenizer_key_event_byte_bound_v1(raw.len() as u64);
    let p = eof.min(base.signed_key_bound);
    let nodes = p.checked_mul(513).ok_or_else(|| resource("sparse node cardinality overflow"))?;
    if p > budget.max_proofs || nodes > budget.max_nodes {
        return Err(resource("local proof/node budget exhausted"));
    }
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<Generated>(1).map_err(NativeDecodeErrorV1::Resource)?;
    // Court clone plus packet rollback parser; stacks/lex bounded in their owning module.
    q.add_v1(
        crate::palw_tokenizer_json_stream_v1::producer_parser_requests_v1()
            .map_err(NativeDecodeErrorV1::Resource)?
            .checked_mul(2)
            .ok_or_else(|| resource("parser clone request overflow"))?,
    )
    .map_err(NativeDecodeErrorV1::Resource)?;
    q.exact_v1::<PalwTokenizerTableStreamV1>(1).map_err(NativeDecodeErrorV1::Resource)?;
    q.vec_v1::<PalwJsonKeyCompactProofV1>(p).map_err(NativeDecodeErrorV1::Resource)?;
    let mut siblings = TirNativeRequestsV1::new();
    siblings.vec_v1::<Hash64>(512).map_err(NativeDecodeErrorV1::Resource)?;
    q.add_v1(
        siblings
            .bytes_v1()
            .checked_mul(p.checked_add(3).ok_or_else(|| resource("proof count overflow"))?)
            .ok_or_else(|| resource("proof heap overflow"))?,
    )
    .map_err(NativeDecodeErrorV1::Resource)?;
    q.add_v1(
        nodes
            .checked_mul(btree_node_envelope_bytes_v1::<(usize, [u8; 64]), Hash64>() as u64)
            .ok_or_else(|| resource("sparse map request overflow"))?,
    )
    .map_err(NativeDecodeErrorV1::Resource)?;
    q.vec_v1::<PalwJsonKeyCompactProofV1>(1).map_err(NativeDecodeErrorV1::Resource)?;
    // Actual packet codec scratch is <=90000; a raw atom contains one byte.
    q.vec_v1::<u8>(90000).map_err(NativeDecodeErrorV1::Resource)?;
    q.vec_v1::<u8>(1).map_err(NativeDecodeErrorV1::Resource)?;
    let requests = q.bytes_v1();
    if requests > budget.max_requests {
        return Err(resource("local owner request budget exhausted"));
    }
    let lease = reserve(requests).map_err(NativeDecodeErrorV1::Resource)?;
    let mut c = base.clone();
    let mut tree = ProducerTree::new(budget.max_hash_calls, budget.max_hash_bytes);
    if tree.root() != c.key_set_root() {
        return Err(decode("nonempty initial native key set"));
    }
    let mut proofs = Vec::new();
    proofs
        .try_reserve_exact(usize::try_from(p).map_err(|_| resource("proof address-space overflow"))?)
        .map_err(|_| resource("proof vector allocation refused"))?;
    let mut raw_calls = 0u64;
    loop {
        c.pending_overrun().map_err(decode)?;
        if let Some((namespace, key)) = c.pending_key() {
            if proofs.len() as u64 >= p {
                return Err(resource("actual proof count exceeds derived bound"));
            }
            if tree.root() != c.key_set_root() {
                return Err(decode("rolling native data root diverged"));
            }
            let (path, proof) = tree.proof(namespace, key).map_err(resource)?;
            let before = c.parser.key_insertions();
            c.packet(
                c.context,
                &PalwTokenizerStreamPacketV1 { offset: c.tokenizer_offset(), bytes: Vec::new(), proofs: vec![proof.clone()] },
            )
            .map_err(decode)?;
            if c.parser.key_insertions() != before {
                tree.insert_path(path).map_err(resource)?;
            }
            if tree.root() != c.key_set_root() {
                return Err(decode("native authenticated key transition diverged"));
            }
            if tree.nodes() as u64 > nodes {
                return Err(resource("actual sparse node count exceeds bound"));
            }
            proofs.push(proof);
        } else {
            let at = usize::try_from(c.tokenizer_offset()).map_err(|_| decode("raw offset overflow"))?;
            if at == raw.len() {
                break;
            }
            if at > raw.len() {
                return Err(decode("raw offset exceeds operand"));
            }
            // One-byte construction atoms avoid an independently duplicated parser pause grammar.
            // The terminal builder separately repacks into its existing maximal mixed packets.
            c.packet(c.context, &PalwTokenizerStreamPacketV1 { offset: at as u64, bytes: vec![raw[at]], proofs: Vec::new() })
                .map_err(decode)?;
            raw_calls = raw_calls.checked_add(1).ok_or_else(|| resource("raw packet count overflow"))?;
        }
    }
    c.seal_tokenizer(c.context).map_err(decode)?;
    // Flat EOF authentication is mandatory even after duplicate/overrun/invalid JSON.
    let _authenticated_json_verdict = c.parser.finish().map_err(decode)?;
    let result = Generated {
        artifact_binding: c.parser.sealed_artifact_binding_v1().map_err(decode)?,
        signed_key_bound: c.signed_key_bound,
        native_proof_calls: proofs.len() as u64,
        proofs,
        root: tree.root(),
        hash_calls: tree.hash_calls,
        hash_bytes: tree.hash_bytes,
        raw_packet_calls: raw_calls,
    };
    Ok(NativeRetainedV1::from_owner_reserved_v1(result, lease))
}
