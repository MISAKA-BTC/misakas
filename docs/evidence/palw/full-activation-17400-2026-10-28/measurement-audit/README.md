# RFC06/RFC08 measurement preparation — 2026-10-28

Status: collector implementation and synthetic controls only. No measured-policy record, economics approval, live router-court acceptance, or activation is claimed.

## Implemented in this change

`rfc06_native_router_observation.py` connects the existing native `getPalwLegacyDispute` (op 204) and `getPalwRouterBindingV2` (op 245) to the integration campaign. A frozen `reader-router-court` requirement can pin `classId`, `producerBond`, `bindingSha256`, `table`, `reader`, and `router`. The collector brackets both reads with the native sink/DAA/genesis/Params/import context, checks the exact claim identity, requires an active binding declared no later than claim acceptance, validates bounded canonical Borsh framing and the exact requested reader/router tuple, and preserves the bytes and context.

Binding alone produces `routerBindingObserved=true` but **status UNKNOWN**. Neither the binding nor the generic `courtConvicted` boolean identifies the actual route. Legacy requirements with empty expected scope remain UNKNOWN without making new RPC calls.

The implementation now also adds `getPalwMemoryRoutingOutcomeV1` (op 249; protobuf 1314/1315). A thread-local, non-Send guard captures exact tag 243/244 verdicts only around the synchronous authoritative fold, after the individual object's economic effects succeed. The processor publishes the bounded result only after the atomic UTXO/PALW commit returns successfully. Internal builder checkpoints also retain the telemetry generation, row length and overflow bit; branch restoration restores these with the state. Nested guards isolate their captures and restore the outer scope. The cache retains at most 64 blocks of at most 64 outcomes each; overflow discards the whole observation. A restart or eviction returns unavailable. Reads require a fully validated block, exact header hash, current selected-chain membership, matching stored post-state root, bounded DAA lag and stable selected context. This is node-local observability, not new consensus state or a computation proof.

A frozen requirement may additionally specify `court: {blockHash, objectId, objectTag, route, leafIndex, verdict}`. The collector queries the new endpoint inside the same context bracket and requires the exact committed tag-244 `RoutedStep` outcome and its immutable binding. Only that exact positive observation makes this one court fact PASS. Missing cache, old binaries, another object, another class/claim, another verdict or changed context cannot close it. All-model boundaries, economic evidence, restart reproduction and full campaign acceptance remain separate obligations. `objectId` is the existing kernel `object_id` hash of `(object_tag, accusation)` under `MISAKA/LOCAL-COURT-OBSERVATION/V1`; it binds the complete accusation including its signature. It must be computed from the actual submitted carrier, not discovered and retrospectively changed in the frozen plan.

The outcome optionally includes `adjudicationAndSettlementMicros`, measured with the local monotonic clock around actual court verification and successful economic writes. It excludes RPC latency and object-hash serialization. No capture means no timing. This duration is telemetry, not a consensus cost, approved economic price, or all-class upper bound. The collector validates an observed duration as u64; absent timing stays unavailable.

The SDK now exports practical frozen inputs without querying outcomes:

```sh
palw-class router-binding-v2 court-expectation \
  --object actual-signed-accusation.borsh --binding-row actual-bound-row.borsh \
  --block-hash ACTUAL_BLOCK_HASH --expected-verdict ExecutorGuilty \
  --id reader-court --role A --out court-requirement.json
```

This reads the actual bounded tag-244 RoutedStep carrier and row, derives its full signed object identity, claim/class/producer, reader/router, leaf, and row SHA256, and writes one requirement for the campaign. The expected verdict is declared by the scenario; it is never read back and changed to match an outcome. The command does not verify native acceptance, generate a signed accusation, or emit any observation/PASS. Existing output files are preserved. Supply a block from the controlled fixture's native block construction/submission and freeze these identities before observation. The collector still independently requires the exact committed native result. The existing real-node GDN registration fixture does not yet provide a complete admitted RoutedStep accusation scenario; that positive node scenario remains explicitly unexecuted.

The Rust changes have been handed to the parent for the consolidated build/test batch. No live run of this new endpoint has yet been performed. Source tests cover guard discard/overflow, wrong roots/claims, reorg exclusion, restart and eviction, plus a real GDN fold's actual guilty/false-accusation outcomes and unchanged state root with telemetry enabled. Cache lifecycle controls are not a substitute for the upcoming real node acceptance run. No numeric measured-policy record is created by this feature.

## Executed controls

- Existing RFC08 cost harness: 16 tests passed (`rfc08-cost-harness-tests.log`).
- Existing campaign before modification: 19 tests passed (`campaign-tests.log`).
- Router collector counterexamples and coherent-read positive control: 6 tests passed (`router-observation-tests.log`).
- Campaign after integration: 19 tests passed (`campaign-after-tests.log`).

These are synthetic parser/collector controls, not live cost measurements. The new Rust observer/API is pending consolidated build and native execution validation.

## Monetary policy inputs and derivation

The shipping activation declaration still has an all-zero monetary policy. `kaspad/src/private_integration/paid_claim_factory.rs` contains a nonzero **unapproved private probe**: 1,024 contracts, 32 MiB/contract, 256 MiB/provider, 100 sompi collateral/fee, 20 DAA windows, 10,000 DAA enrollment, and an explicitly unapproved scope digest. It is not a measured public policy.

The existing `misaka-rfc08-cost-measure.py` already measures accepted native transaction fees/payments by their exact funding address, actual collateral collection after conviction, and sampled capital-time. Missing polls remain UNKNOWN. It does not approve a fee or safety threshold. No complete measured production plan/report was found in the searched evidence files.

A candidate public policy should be derived from a real run covering the small/large model × below/at-cap × unsaturated/saturated budget-window matrix, including real conviction collection and provider replacement/recovery. Pin the resulting raw report and independent review before choosing bounds. The following inputs remain necessary:

| Input | How to obtain it |
|---|---|
| Candidate source, binaries, network/Params and exact class IDs | Freeze the completed candidate and owned private network |
| Actual work, enrollment/replacement and conviction commands | Existing signed native fixture/SDK flows with exact ownership |
| Capacity bounds (contracts and bytes) | Measure allocation/retention under intended hardware and concurrency; choose the supported operating envelope |
| Capital rate and its DAA period | Explicit economic policy decision; this cannot be inferred from test duration |
| Retention cost per DAA | Actual storage/network/court/recovery cost attribution and a stated conversion basis |
| Material/budget/response/enrollment windows | Measured recovery and inclusion conditions plus explicit service/lifetime policy |
| Collection lower bound | Actual conviction collection evidence plus stated participation/economic assumptions |
| Per-class cost upper bounds | Complete scenario measurements and the chosen coverage/safety assumptions |
| Minimum collateral and fee | Derive from those bounds and approve the economic terms; private 100-sompi probe values are not evidence |
| Cost report root and measured gate record | Hash the actual reviewed report, exact policy and network; do not fill with arbitrary nonzero digests |

No approved numerical public monetary policy was found. Missing economic choices are distinct from missing runtime implementation; neither is closed by making the record list nonempty.
