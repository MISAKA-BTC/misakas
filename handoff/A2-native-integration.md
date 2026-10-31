# A2 native source / protected local I/O integration

2026-10-31. Implementation candidate for A0/A4/A6; native acceptance is **not yet complete**.

## Implemented connection points

- `consensus/core/src/palw_positive_source_geometry_v2.rs`: one shared existing-G14 value wrapper, bounded registered class/job geometry and raw opening decoder. No new tree, wire discriminator, or admission capability. Exact raw data bytes and bounded serialized bodies are separate axes.
- `consensus/core/src/palw_positive_source_v2.rs`: finite preparation/replay cache for actual op3/op4 literals, canonical interval union across all Panels, source-unit receipt/due dedupe, aggregate usage, pending service-fee reservations, immutable schedule quotes and once-only fee intent DTOs. Public cache construction/preparation does not grant native authority.
- `consensus/core/src/palw_positive_source_v2_history.rs`: bounded literal source histories; source material IDs are actual carrier TxIDs under A0's source-specific rule. Native provenance, full-case sequence/previous/head and accepted-prefix completeness remain mandatory context inputs.
- `consensus/core/src/palw_positive_source_apply_v2.rs`: private `TransitionBuilder::apply_positive_source_prepared_v2`. Requires existing route header, authenticated typed bundle, exact old Case row/usage and replayed old union/receipt. Writes Case/SourceUnion/ServiceReceipt through the existing admitted typed kernel-row writer with checkpoint rollback. A4 must put this call, A5 funds and A6 complete-case journal in the SAME outer native transaction.
- `remote::miner::positive_v2::rocksdb`: default-off `positive-miner-rocksdb` dedicated physical DB. Authenticated responder binds `(network, miner, case)` permanently to Result/Service/process limits. Sync WAL CAS, private ownership marker, logical quotas, exclusive per-case writer lease, no rollback/delete API. DB clones share a writer; case handles cannot clone. Native chain rollback cannot reset spent local I/O.
- `kaspad::palw_positive_miner_source_v2`: default-off `positive-miner-client`. `NativeMinerSourceV2` connects result authentication, protected case lease, durable journal and the actual `ConsensusProxy` source-read API. `NativeSourceDutyV2` has no external DTO/Box constructor. Native per-unit bounds are mandatory. Response release reauthorizes after provider I/O.
- `remote::miner::positive_v2::g14::encode_g14_unit_v2`: one existing raw Borsh artifact opening, Merkle prompt opening, or flat prompt Vec. A4 op4 supplies case/RequestId; input form comes from native geometry.

## Native owner requirements still to connect

A4 must supply `NativeSourceContextV2`: actual accepted carrier/body provenance, current rooted drawn role/key, live funding, reserved opportunity/cutoff, immutable source fee schedule and full A6 case-history completeness. Missing ports return `SourceErrorV2::MissingPort`, never a success token. Use `bind_case`/`refresh_case_usage` with the actual current Case row before preparing; another Panel's usage must not be overwritten. Prepared values are instance/revision/Case bound.

The actual `ConsensusApi::palw_positive_source_duty_v2` implementation and its private current-view constructor are still required. The interface's default returns `Unavailable`. It must fix selected parent/state root/accepted prefix/monotonic generation, reject stale views on every call, reconstruct accepted op3 requests and verify literal op4 receipts. A public `PositiveRowsV2` structural reader alone cannot authorize a duty.

`ConsensusProxy` itself is a public dynamic-interface alias which trusted Rust code can manufacture around a mock. The kaspad consumer and duty constructor are therefore node-private; the alias alone is not an authentication capability. Only the actual native owner can complete the private snapshot/publication path. No production success implementation is inferred from this wrapper.

A6 must persist/replay the COMPLETE case journal with its protected native head, not just these source records or the latest TxID. Reorg rolls back native rows/dues; it must not roll back the separate local spent-I/O DB. No native activation or public policy was added.

## Receipt / fee semantics agreed with A0

`ServiceReceiptRow.bytes` is exact raw data, not opening/carrier bytes. Its `duty` identifies the literal accepted op3 referenced by its op4 response. A receipt from request A also completes an overlapping request B for that unit. Settlement may remain `None`; data completion and payment are separate.

Prepared updates retain touched source/unit coordinates even when they write no receipt or union. The private writer checks the complete old literals before every retry/duplicate path. A5 settlement changes require an actual same-view A5/A6 history/snapshot bridge; until connected, a settlement mismatch is rejected rather than trusting a saved payment flag.

`Case.usage.fee` is only finite source-service dues, bounded by `Service.budget.fee`. Reserve worst-case fresh-unit fees before admitting a request, accrue once at the first valid unit response, and pass `SourceFeeIntentV2` to A5. The once-only payment coordinate is the existing ServiceReceipt key. This cache never moves funds or writes a paid flag.

## Validation in progress

Candidate `3d764a0c1`: remote 246 passed/one existing ignored, focused source 25 passed, default-off 176 passed/one existing ignored. Each recorded run kept the exact source hashes unchanged. The real subprocess checks hold stdin open, verify the paused child is alive and require SIGKILL signal 9.

A further independent raw-journal fixture exposed equal Complete-only status counters merging across two fresh responder instances. The provider-failure variant was already rejected by the monotone meter guard. A local instance seal now binds each file journal to its creating/resuming live responder; this changes no wire, anchor DB or checkpoint schema. Before-fix reproduction failed as expected, and both post-fix controls passed. The final full suite/client checks for this additional fix are being recorded; previous-stage raw logs remain preserved separately.

Real model cold/warm overhead and actual mempool/template/heartbeat source admission, native funds, independent-node replay, canonical reorg and IBD remain unmeasured/unverified until the actual owner connections above run on this same candidate.
