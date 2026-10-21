# Source-material graph native owner: independent review

Read-only review of /tmp/g14-source-graph-native-owner/source-graph-owner.patch and NOTES.md against actual frame, query universe, graph builder and check_v3. No repository edit/build. No additional successful-path undercount or captured-allocator classification bug was found.

## Phase 0 / structural validation

`frame.check_structure_v1` calls plan.check_v1, whose accepted_id_v1 clones the plan and Borsh-hashes the clone. The full frame NativeHeap request bounds that plan clone, including semantic stage vectors. The clone drops when accepted_id returns; it does not survive into custody checks or graph expansion. Custody checks iterate existing panel/grant/credit collections and serialize existing pledge/receipt operands for their digests; no additional whole-frame clone is made.

FP claim ID serializes the original commitment; job pin serializes/hashes its job and scalar identity; header_digest serializes the header; scope/request IDs serialize fixed scope tuples; the final frame byte-bound check serializes the full frame. These writers are sequential. The plan clone may coexist with its own plan writer, so clone + writer are correctly additive. max(actual full frame encoding, scope exemplar,1024)*2 bounds these successful Borsh writers and their pinned minimum/growth. Actual full frame contains every variable-size encoded subobject; fixed request tuples are also below1024. stream_digest for node/relative/absolute control IDs uses a streaming sink, not one per-node encoded Vec.

Prelude quote's object_length and NativeHeap traversal use borrowed operands and counters; no copied plan or allocating parser is invoked before reservation on the successful path. Source is already borrowed and is not reconstructed here. Error formatting allocation is outside the successful construction claim, as documented.

## Extent and simultaneous graph/index owners

The grouped native transport upper bounds every complete request family/stream/control, including one shared FullIds stream per actual role. Exact graph construction can be smaller for ragged bodies; the existing <= upper rule is preserved. Extent's R-squared finish-predecessor response count is the actual builder semantics, not one signer-only edge. Roles are the same maximum_custodians as the native upper (no +1 change).

Retained graph BTreeMap<Hash64,SourceMaterialNodeV3> inline node envelopes cover fixed request/guard fields and predecessor Vec descriptors. Dynamic predecessor buffers receive aggregate E growth plus one independent minimum envelope per possible node. Transient response `finishes.to_vec(); push(query)` is the next retained node's predecessor allocation and is covered before insertion; it is not a second simultaneous retained copy. Unstable sort is in-place.

Query map is actually BTreeMap<u64,Hash64> with Q entries; FullIds ordinal list is <=Q; one live streams() finishes Vec has <=R entries. These can coexist with graph nodes. FullIds list is consumed/dropped before check_v3 but remaining quota deliberately stays conservative.

check_v3 allocates indegree BTreeMap<Hash64,usize> (64-bit profile explicitly required), children BTreeMap<Hash64,Vec<Hash64>> (<=V keys), child Vec buffers totaling E with per-key minimum, and ready Vec <=V. These all coexist with retained graph and possibly query map; quote sums them, rather than reusing the retained parent buffers. No packet-ID sets, scheduling projection/reservation or service book clones are created by derive_v3; those later APIs remain explicitly excluded.

First phase's frame/set/upper/extent are fixed or temporary after their writers/plan clone drop. The same ticket grows with graph output/index quote BEFORE derive_v3 expansion. Repeated structural checking by old derive does not create two simultaneous plan clones or writers; the retained prelude quota covers each pass. Graph and source tickets coexist under active common checker; source ticket is borrowed, not transferred.

## Typed errors

Unlike the numerical materializer, this graph derive has NO captured try_reserve/try_reserve_exact allocator errors. It uses BTreeMap insertion, Vec push/to_vec/collect and Borsh to_vec's ordinary allocator APIs. Thus mapping its String semantic errors to Decode does not currently swallow a caught TryReserveError. Existing Strings are graph/budget/shape/hash/codec refusals. The new reserve/grow/quote/unsupported paths retain NativeDecodeErrorV1::Resource. This is NOT a guarantee that physical allocator OOM/abort becomes catchable Resource; NOTES already says that explicitly.

No need to add string matching or a typed materializer just to mirror the numerical repair. If fallible allocator APIs are later introduced, map their actual causes at those exact sites.

## Unsupported tail scope

Native prelude explicitly refuses job.tail Some as Resource(Unsupported inventory). That is a local missing resource inventory, not a new semantic/model rejection and not a change to old derive. This broad limitation includes currently bounded Constraint/prefix/eval/etc carried tails rather than pretending their parsers are all inventoried. Old native original preparation already refuses carried tails; no dormant fence is armed. The private native graph owner therefore remains SourceMaterialOnly and must not be used as evidence that Tokenizer/Constraint/full-original closure or paid admission exists.

The old derive body and graph logical/absolute IDs are unchanged by this patch. The added tests compare actual nodes/scope/edges and independent quota lifecycle; they should remain labelled capacity/identity tests, not adopted profile or public transport success.
