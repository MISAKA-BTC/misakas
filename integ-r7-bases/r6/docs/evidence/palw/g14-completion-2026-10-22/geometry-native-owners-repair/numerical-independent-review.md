# Native numerical owner: independent read-only review

Reviewed /tmp/g14-numerical-native-owner/numerical-owner.patch against actual derive_v3, native Source A/B/C helpers, dissection site/obligation/value functions, interval analyzer and numerical tree inventory. No source edits/builds.

## Finding: captured allocation failures lose Resource cause

`derive_native_reserved_v3` materializes via `Self::derive_v3(...).map_err(E::Decode)?`. The unchanged legacy derive catches actual Vec allocation errors at `out.roots.try_reserve(1)` and `root.paths.try_reserve_exact(...)`, converting them to String("numerical root allocation failed") / String("numerical path allocation failed"). New native wrapper then labels these local capacity failures Decode, despite having a typed Resource branch. Pre-reserving logical quota does not make an actual allocator failure impossible (e.g. address-space/physical contention/capacity overflow).

This is a real cause-classification gap in the new typed API; it is not an argument to change path caps or geometry. Do not classify by matching those strings.

Concrete correction candidate:
- Introduce a private `NumericalMaterializeErrorV3 { Semantic(String), RootAllocation(TryReserveError), PathAllocation(TryReserveError) }` or equivalent private typed enum.
- A shared private materializer contains the existing derive body, preserving all geometry/policy checks and output. String/&str semantic errors convert to Semantic via From; the two try_reserve sites retain RootAllocation/PathAllocation directly.
- Keep old public derive_v3 as wrapper mapping Semantic to the same string and allocation variants to their EXACT old static strings. This preserves its public Result<Self,String> semantics.
- New derive_native_reserved_v3 calls that same private materializer and maps only Semantic to NativeDecodeErrorV1::Decode; allocation variants become NativeDecodeErrorV1::Resource(NativeCoverageV1::unsupported::<Vec<NumericalRootV3>>(...)) / corresponding Vec<NumericalPathV3> with concrete TryReserveError cause. All new resource failures release the owned ticket through normal RAII.
- Prefer a narrow test-only materializer allocation hook if needing a deterministic allocator failure test; do not induce real OOM or fake it as a semantic path-budget exhaustion. Existing independent-checker tests already verify reserve/grow cause, but they do not reach these two allocator branches.

If preserving literal old derive BODY is a review requirement, defer applying this new typed wrapper until the common typed materializer can be reviewed; duplicating the entire enumeration solely to preserve a byte-identity claim would create drift risk.

## Coexisting owners / quotes

No additional undercount was identified in the successful path after examining the following:

- First pass `scan_native_counts_v3` returns only Copy counters. Binding class raw program/layout, Box<Verified>, prepared context and analyze_ranges intervals drop on its return BEFORE derive_v3 constructs its second pass. No two live prepared preludes are justified/needed. One prelude reservation is held across both; retaining it until returned geometry drops is conservative.
- `native_prelude_requests_v3` adds Source construction requests (A codec/validation scratch+B StepSpace+raw program/layout/context/hash writers), plus Box<PalwTirVerifiedBindingV1>, explicit retained interval result outer/inner vectors, and per-site scratch. `analyze_ranges` does run validate while retaining verified.program/info, then interval/operand-type/constant-lane scratch; existing A already includes multiple codec/analysis passes, with the explicit retained interval table additive here.
- Dissection site owns reductions<u16>, folds<PalwTirFoldV1>, bounds<Interval>, counts<u64>. During obligation/value scans these coexist with new computed/seen/derived bool vectors, repeated-reference DFS usize stack, separate obligation reductions and propagated value demand<u64>. Actual parsed structural_entries includes all input reference entries, so total stack pushes+initial root are bounded by n+1. Rank-bounded first operand Vec<usize> and TopK Vec<u64> coexist and are separately quoted.
- `numerical_tree_inventory_v1` uses **BTreeMap<u64,u64>** for both level and next. The patch uses the correct key/value envelope. Both may coexist, including consuming the old map's IntoIter. Distinct positive range sizes at a level are <=initial tiles. Source verified job positions<=layout.max_context and site history_positions<=job positions, so max_context.div_ceil(h_tile) conservatively bounds that map entry count. The quoted two envelopes per possible size include native BTree node minimum capacity via the existing envelope model.
- Retained roots use growing try_reserve(1): q.vec<NumericalRootV3>(roots) covers geometric capacity. Each root path Vec uses try_reserve_exact, so aggregate q.exact<NumericalPathV3>(paths) matches requested payload capacity. Individual byte Vec clone+push growth is bounded by twice path length plus minimum capacity; aggregate path_bytes and per-path one-item envelopes conservatively include that.
- DFS todo retains sibling tuples plus pending path buffers; with legal arity k/depth d its pending frontier is <=k*d+1. q.vec tuples handles growth and q.frontier*vec<u8>(depth) covers pending paths. The popped current path is destined for retained output, whose aggregate byte reservation already includes it; the temporary next child fits the bound on the post-push frontier. Child ranges actually use Vec::with_capacity(arity), matching q.exact<(u64,u64)>(arity), and coexist with pending/output.
- The first-pass output quote is grown onto the SAME ticket before second-pass materialization. `NativeDemandLeaseV1::grow_memory_v1` uses its own checker Arc (including the injected independent checker), not a different global checker. A failed grow doesn't debit the ledger; failure and output mismatch return RAII-drop the prelude lease.

## Source owner / meaning / limits

`&NativeRetained<Source>` is borrowed for derivation; no source ticket is transferred or released. Under active global admission, the existing Source owner reservation coexists with this additional numerical reservation. Test's separate checker explicitly excludes the already constructed Source and should stay labelled that way. Returned graph stores source_id and copied geometry, not references to Source, so no Source lease needs to be retained by the returned graph. Off-mode NativeRetained may have no ticket under the framework's explicit disabled mode; this type alone is not paid/native quota authority.

Old derive body/path traversal and cap constants are unchanged by the proposed patch. New native API preliminarily checks arity/history-tile while deriving its request quote; its refusal ordering can differ from old string API without changing the old API. Firstpass arithmetic-overflow/native-cap/path-budget errors remain semantic Decode. Reserve/quote/grow failures are typed Resource. The actual try_reserve failure classification described above is the remaining inconsistency.

No full semantic closure, raw/proof delivery, F02, complete-original, adopted profile or funding guarantee is established by numerical owner reservation.
