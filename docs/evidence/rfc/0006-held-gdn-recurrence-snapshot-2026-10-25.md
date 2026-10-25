# RFC-0006 held-GDN recurrence snapshot admission — 2026-10-25

Status: scoped compilation and runtime PASS. The new snapshot controls below are distinct from the earlier root-workspace controls; neither closes the whole replay boundary.

The actual `ServiceGdnHostV1::kernels` supplies Qwen36 recompute kernels through the boxed GDN interface. On a retained predecessor, `transition_with_owned_predecessor_v3` calls `honest_chunks_root_retained_v3`, then `base0_fp_recompute_state_at_covered_root_retained_v3`. After the real teacher-forced prefix, that entry previously invoked raw `state_chunks`. Qwen36 immediately constructed `qwen36_recurrence_state_v1`: outer layer/state vectors and deep copies of the cache's per-head matrices and convolution rows. This happened before the existing admitted root workspace and also on leaves whose map carries only attention at that position.

The new typed `state_chunks_snapshot_retained_v4` admits that temporary snapshot from the actual borrowed cache before copying. Qwen36 and the production boxed delegator implement it. The default is a retryable `NativeDecodeErrorV1::Resource` for missing local coverage, with no raw fallback, model-unsupported classification, honest completion, or accusation. The raw method and raw constructor remain compatible. Both snapshot constructors feed one private serialization body: map dispatch, shape validation, gathered channel ordering, historical roots and serialized bytes are shared.

For L actual linear-attention layers, the request is the checked sum of:

- L `u16` layer indices and L `Base0GdnLayerStateV1` entries;
- each actual selected cache head vector's `Qwen36GdnStateV1` entries and each head's actual `i32` matrix length;
- each selected convolution vector's `Vec<i32>` row entries and each row's actual `i32` length.

It uses source lengths, not spare capacities, a profile-size multiplier, an invented prefix limit, or serialized bytes as a RAM ratio. Every destination vector uses `try_reserve_exact` before population; a returned capacity different from the admitted slot count or allocation failure is typed Resource. Source entries missing from the cache retain the old empty/default spelling, with semantic validation left to the shared serializer. The old native layer-index spelling is preserved.

The owner stores layers and nested states before its lease. During partial construction, local partial states retire before the outer owner; on normal return, semantic refusal, capacity refusal or unwind, all snapshot values retire before the lease. The actual shared serializer runs while that owner is live. Serialized output chunks have separate ownership and are deliberately not included in this snapshot ticket.

## Validation — actual pinned binaries

- Base0 unit filter `fp_recompute::snapshot_retained::tests`: actual native artifact/cache snapshot equality with the historical collector; all destination capacities; quota refusal directly in snapshot construction; retained live request and complete retirement; retry and checked arithmetic overflow. The isolated child is part of its parent test, not another independent test.
- Node filter `state_root_tests::snapshot_tests`: real registered Q36 prefix and raw/typed chunk Borsh/root parity at positions 15 (attention only) and 16 (recurrence carrying). A separate isolated parent invokes the actual retained held predecessor with quota occupied only after its real 16 forwards, requires typed Resource with unchanged cursor/demands/answers/deadline and no signature, then tests the raw-only kernel's explicit local-coverage refusal. After capacity release the same case retries through the production boxed delegator, native court, retained output and actual signing.
- Existing `state_root_tests` post-serialization root quota and `leaf_leg_tests` delegators explicitly forward the new typed method. The full held-GDN regression passed on the same Node binary, as recorded below.

The initial coordinated four-package no-run build passed in 4m48s. The 20 frozen Rust/config identities matched after compilation in `/tmp/misaka-rfc-head-harvest-gdn-wave-source-20261025/record.json`. Build time includes shared dependencies and is not a model cost measurement. No snapshot compilation or runtime failure preceded these results. An unrelated owner-harvest fixture failed in the same broader wave and was assigned separately; its subsequent test-only correction does not change this verified GDN source or the results below.

| Actual target/filter | Result | Log |
| --- | --- | --- |
| Base0 `fp_recompute::snapshot_retained::tests` | 1/1 PASS, 0.02s harness / 1.63s wall; isolated child included | `/tmp/misaka-rfc-head-harvest-gdn-snapshot-tests-20261025.log` |
| Node `state_root_tests::snapshot_tests` | 2/2 PASS, 3.34s harness / 3.39s wall; isolated child included | `/tmp/misaka-rfc-head-harvest-gdn-gdn-snapshot-tests-20261025.log` |
| Same Node full `palw_gdn_held_filer::tests` | 36/36 PASS, 66.46s harness; includes the new two and existing root/leaf controls | `/tmp/misaka-rfc-head-harvest-gdn-gdn-held-all-regression-20261025.log` |
| Base0 existing recompute regression | 9/9 PASS, 0.51s harness | `/tmp/misaka-rfc-head-harvest-gdn-recompute-legacy-regression-20261025.log` |

The new Node two are members of the full 36, and child executions are members of their parents; these are not additive independent counts. Elapsed times are fixture observations, not throughput or full-model fee/RSS evidence.

Immutable Base0 binary: `/tmp/misaka-rfc-head-harvest-gdn-base0-20261025`, SHA-256 `212f0537fc152acca5b9ac9d4a7198d21a72fe649a98ffead5d4677fe3d45ba8`. Immutable Node binary: `/tmp/misaka-rfc-head-harvest-gdn-node-20261025`, SHA-256 `8e09f1f565a73ca37190764e6992092c138cb76526e017dabbb9b42ed53652e1`. These identify the initial passing GDN source before the unrelated owner-harvest fixture retry; no GDN Rust change was required.

## Remaining boundaries

The passing controls cover only the recurrence snapshot input copy. Kernel/cache initialization, arbitrary prefix execution and its growing cache, the serializer's geometry/scratch and output chunks, root workspace's separate ticket, callback leaf clones, other court/payout/aux consumers, physical allocator/RSS and all-model cost remain separate. It does not claim complete replay, model validation, formal K3 artifacts, multi-process campaign completion, or the 15 real campaign gates. Activation remains unarmed, model tiers and selected-verifier authentication unchanged. Matching bytes/root and local court results do not replace randomly selected verifier approval. The blocked experimental P2P fixture is unrelated and remains NOT PASS.
