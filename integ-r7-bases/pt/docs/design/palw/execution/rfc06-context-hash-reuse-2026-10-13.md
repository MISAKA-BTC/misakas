# RFC06: immutable context hashing cost — 2026-10-13

The actual BASE0/QWEN36/QWEN25-A16 dense rebuild, dense and sparse capture,
checkpoint/interval replay, output validation and held court stream rebuilt the
canonical job context digest for each STEP tile. Some batches also computed an
unused whole-context digest before that loop. This charged context encoding and
hashing once per tile although the batch context is immutable.

`base0::step_tile_hasher::StepTileHasherV1` computes the ordinary job digest and,
when valid, the inherited prefix digest once. It uses the prefix only for call zero
and positions strictly below the inherited length; suffix/decode leaves keep the
job digest. It calls the unchanged core STEP leaf encoder with the chosen digest.
The type immutably borrows its context for its lifetime, so the cached digests cannot
survive a mutation of that context. It retains no preimages, trees, global cache or
new serialization; its measured native size is 200 bytes. Each batch/call drops it.
The historical validator and context/prefix derivation are unchanged.

## Evidence

Two helper tests passed. The canonical-equivalence matrix covers 1,152 combinations
of versions, valid/invalid prefix lengths, call indices and boundary positions, plus
malformed nullifier and missing state root. Additional controls change the job id,
seed and prompt commitment: inherited-prefix hashes remain identical, while suffix
and decode hashes rebind; a changed shape always rebinds.

The ignored comparison was executed three times. Each run hashes 100,000 distinct
coordinates for both context versions 2 and 3, compares every historical/cached leaf,
and compares the resulting Merkle roots. All three executions passed. Median leaf
hashing times were 179,173 / 50,680 microseconds for version 2 and 171,498 / 53,133
microseconds for version 3 (historical / cached). Run 1 recorded peak RSS 33,193,984
bytes for this isolated process. The host was concurrently building/running nodes;
these synthetic tile timings are scoped comparative evidence, not full model CPU,
latency, worst court cost, resource acceptance or approved fees.

The helper was compiled with `rustc --edition=2024 -C opt-level=2 --test` against
existing compiled consensus-core and hashes rlibs in the shared node target, avoiding
another full target. [Machine evidence](../../../evidence/palw/design-records/rfc06-context-hash-reuse-2026-10-13.json)
records every changed Rust source, canonical source, executable and linked-library
SHA256, plus [ordinary results](../../../evidence/palw/design-records/rfc06-context-hash-reuse-2026-10-13-tests.log)
and the three raw measurement logs. A narrow Cargo build encountered a different
core feature graph and was stopped rather than allocating another large core variant.
The integrated BASE0 library subsequently compiled in the central matching-feature
consensus test graph. The existing inherited-prefix end-to-end test passed (1/1,
2.96 seconds): actual floor backend execution, seat replay, public conviction of
a lying prefix, and preservation of version-2 job-bound leaves.
[Raw consumer result](../../../evidence/palw/design-records/rfc06-context-hash-inheritance-2026-10-13-tests.log)
and the linked-library/executable/source hashes are in the machine evidence above.
It was compiled directly against that newly built BASE0 rlib, without another Cargo
feature graph. Broader sparse/dense and all-family backend certification remain
required for integration and do not become model resource/cost approval.

## Remaining RFC06 gates

This fixes one repeated hashing cost. The full [requirements index](../../../audit/palw/design-reviews/rfc-0006-0007-0008-0012-design-audit-2026-10-10.md#完全解消を判定する要件の索引)
and [resource consumer record](rfc6-resource-consumers-2026-10-11.md) still apply.
Whole-claim coverage, worst registered model/time/RSS/court/fee measurements, actual
funded native 100k population/carriage, and genuine multiprocess pruned P2P recovery
remain separate acceptance requirements. No security, economic or activation gate
is promoted by this change.
