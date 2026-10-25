# RFC-0012 production market parent ownership — dedicated controls verified

This change connects the actual producer and validator's existing market settlement inputs to retained native state and typed local failures. It supplies no new policy, native39 settlement mapping, mature-work fact, model examination/approval, filled-market premise, K3 material or campaign result. Public activation/bootstrap refusals, model tiers and claim authentication remain unchanged.

## Direct settlement consumer defect

ADR-0089 Decision 6 requires the selected child to carry the complete settlement list decided by its selected parent's fold. `VirtualStateProcessor::build_evm_template` instead loaded the PALW tip with legacy `load_tip(params).ok().flatten()`, filtered wrong-parent state away, and converted any absence into an empty expected settlement list and missing market view. A failed local read could therefore omit real already-decided settlement operations from the producer's payload. This is an actual template consumer, not an RPC-only presentation defect.

The defect also occurs under real native admission with otherwise valid bytes: `load_tip` calls `into_unreserved_legacy_v1`, which refuses to detach a retained allocation ticket whenever native capacity is enabled. Silently discarding that error removes the market window even when the retained tip is valid and affordable. Merely changing the load to retained while cloning a bare view would not close that consumer.

## Production connection and scope

The template now loads the retained tip, requires the exact captured selected parent, and distinguishes capacity from unavailable history/store context. A missing, unreadable or mismatched active parent stops template construction through existing LocalNative RuleError categories. The dormant market path performs no additional native state load.

A shared producer/validator projection checks the real parent's point, admits the existing Core owned view and exact ordered settlement copy before their allocations, and retains both owners through execution and payload construction. A valid genesis with no last point remains permitted by the existing genesis contract. The child DAA chooses the same existing fences/gate inputs as the old projection. This uses the already factored market predicate; it does not introduce model review or adopt legacy lifecycle admission as new authorization.

`EvmMarketInput` adds a non-wire optional retained view. The ordinary snapshot/executor entry passes that owner to the same retained MarketHandlers used by replay, whose cloned callbacks retain the actual setup/view tickets. The legacy view path remains available. Simultaneous raw and owned inputs are refused before both the full executor and empty-acceptance snapshot shortcut. The explicit retained replay sibling also refuses a second supplied owned view. This does not newly change the legacy empty-acceptance settlement-validation contract: actual production validation checks exact system operations before that shortcut, as before.

The driver preserves `EvmExecError::NativeResource` as a typed local Resource instead of `Exec(String)`. A missing EVM-active parent's state snapshot is local history unavailable. The O12 worker channel now retains the error enum rather than erasing it into a String; inline and worker results share the same local/semantic classification. Actual EVM payload/store failures are local context faults. The main virtual walk returns local failures for retry before it can mark a candidate disqualified; commitment mismatch and existing semantic execution refusal remain the invalid-block path. No same-batch UTXO, receipt, PALW settlement clearing or persistence order is changed.

## Registered verification scope

Six new parents are registered and passed in the coordinated retry3 described below. No worker Cargo or runtime was launched. Static parsing of all 12 owned referenced Rust files and `git diff --check` passed; new leaves were formatted without formatting the shared large files.

Consensus `evm_market_production_` has three parents:

- The real existing private signed-pipeline harness is constructed with `parts(None)` and its full `validate_palw_v2` check. It installs no DNS-retirement settlement policy. An actual signed deposit lock is accepted/automatically consumed, two actual signed sell calls reach the fold and decide MARKET_MISSING refusals, and a signed F002 withdrawal is carried. The actual next template must contain the exact two real queued settlements; native acceptance must execute the withdrawal into exactly one UTXO. A later block and a fresh Consensus instance over the same database must preserve the receipt/UTXO/balance and carry no repeated settlement. There is no prefilled settlement queue, seeded market or fabricated fill.
- Real accepted BASE context plus cold malformed/missing tip inputs exercise the actual template entry: neither fault can manufacture an empty market context, and the accepted parent's status/sink are unchanged. The fault inputs are explicitly local storage fixtures.
- An isolated logical-quota child verifies a real exact parent versus wrong parent, the old bare extractor's actual refusal, owned view lifetime across clones, and release. OnceLock is armed once at a declared 512 MiB test limit; a retained blocker occupies the actual remaining request-ledger room without rearming. An actual acceptance-driver call with a native parent and a separately identified unaccepted signed-payload storage row must preserve handler Resource as a typed driver error. That row is execution input only, not an accepted block, deposit or credit. The dormant market adds no state work; releasing the blocker permits the same configured limit to retry.

EVM `production_market_owned_` has three parents:

- Real signed interpreter calls compare ordinary retained versus legacy execution roots, state, receipts, actions and outcomes against the same hypothetical window.
- Raw+owned input is refused by full execution and empty snapshot fast path; supplying an additional retained owner to the replay sibling is also refused.
- An isolated zero-budget child expects actual retained handler setup to return NativeResource while the preexisting raw compatibility entry remains executable. Earlier hypothetical window/snapshot construction is not claimed preadmitted or authenticated native work.

The two child processes are inclusive within their parents, not additional parent passes. The immutable retry3 results below verify this registered scope; related regressions and private/non-EVM metadata checks remain pending.

## Limits and subsequent work

The native pipeline's model line is not asserted approved; its orders produce genuine refusals. This closes the exact settlement-list consumer and real withdrawal once/reopen path, not market fill eligibility or the formally unprovided native39 mapping. The existing MutableBlock harness export is not a complete allocation-ownership or memory campaign. Reopening a Consensus instance over the same database tests cold application caches and durable rows, not OS-crash recovery or a cold physical page cache. No actual pruning, pruned IBD, Safe/Finalized maturity or large-model campaign is asserted here.

General interpreter state/snapshot/raw-transaction allocations remain broader accounting debt. Existing semantic invariant errors are not reclassified wholesale. ADR-0109 deposit-index rebuild restart marker behavior was audited separately: current wholesale import uses an incomplete-import barrier and rebuilds before stable publication, so this evidence does not assert an exposed committed restart defect or claim that index work is fixed by this change.

## Initial compile history

The first coordinated five-package no-run build ended with exit 101 after 84.66s, with frozen identities matching. Two E0277 errors in the producer/validator settlement slice projections came from `.as_ref().unwrap_or(&[])` inferring the empty-array AsRef target. Both were corrected to the exact Vec `.as_slice()` projection; parsing and diff checks passed and Rust was frozen again for the parent retry. This was a compile failure, not a runtime failure or settlement pass. Original log: `/tmp/misaka-rfc-payout-provider-market-wave-build-20261025.log`. No runtime had been executed at that compile checkpoint.

The first retry no-run ended with exit 101 after 112.92s, again with source identities matching; the Core unit artifact completed. Its sole market test error was E0382: the unavailable-template diagnostic matched a non-Copy context by value and then attempted to print the partly moved Result. The fixture now matches `&actual` and compares `&expected`. Production code and test conditions were unchanged. Parsing/diff checks passed and Rust was frozen for retry2. An earlier read-only concern about a nested closure capture was not a reported compile error and caused no edit. No runtime result is inferred from either compile failure.


## Actual dedicated verification

The first successful coordinated retry2 no-run passed in 146.78s with 31 frozen identities matching, and all six market parents passed (native 3/3 at 0.77s, EVM 3/3 at 0.05s). The same initial shared runtime had an unrelated remote-provider fixture expectation fail (3/4 there), not a market failure. After pinning, two stale Rust comments were corrected to describe local retry rather than universal disqualification; no market behavior or test changed. The parent corrected its separate remote test assertion and rebuilt the shared generation.

The final coordinated five-package retry3 no-run passed in 158.27s with all 31 frozen identities matching `/tmp/misaka-rfc-payout-provider-market-wave-source-retry3-20261025/record.json`. The source comments and the separate remote test correction are included in this generation. The two preceding compile failures above remain preserved; neither is represented as runtime evidence.

Consensus `evm_market_production_` passed 3/3 in 0.74s harness time (4.40s controller wall), including its 0.22s quota child within the parent count. Immutable binary `/tmp/misaka-rfc-payout-provider-market-kaspa_consensus-retry3-20261025`, SHA-256 `9df50188cbc33e1c49570cd50641d4e842d88983ab0b740b6f19e64ab7ae3de1`. Log: `/tmp/misaka-rfc-payout-provider-market-market-native-retry3-20261025.log`.

EVM `production_market_owned_` passed 3/3 in 0.05s harness time (0.64s controller wall), including its 0.01s child within the parent count. Immutable binary `/tmp/misaka-rfc-payout-provider-market-kaspa_evm-retry3-20261025`, SHA-256 `ed3750056029ab456c843d0564d648057a0d6dcb855d30b539f79d71f5431b73`. Log: `/tmp/misaka-rfc-payout-provider-market-market-executor-retry3-20261025.log`. Controller results: `/tmp/misaka-rfc-payout-provider-market-focused-results-retry3-20261025.json`; pins: `/tmp/misaka-rfc-payout-provider-market-pins-retry3-20261025.json`.

These six passing parents establish the exact actual producer/validator settlement-list connection, signed F002 withdrawal/once/in-process reopen path, native retained-parent/handler admission and typed driver failure, and ordinary-executor compatibility/dual-input contracts described above. The shared final runtime also passed its separate Core4/remote4, for 14 new parents overall; those eight are not added to this worker's six. No filled market, native39 certification or campaign completion is established. Related regressions/private-bin/non-EVM checks are still pending at this checkpoint.

## Final coordinated checkpoint

Retry4 compiled with 33 identities matching and all 14 dedicated parents passed. All 13 related groups passed, with 375 distinct passing parents across overlapping selections; four ignored parents are excluded. Private-node and non-EVM checks passed before only the two Remote test-only repairs. Earlier pending statements describe earlier checkpoints. [Final pins, failures, counts and scope](0006-0008-0012-payout-provider-market-verification-2026-10-25.md).
