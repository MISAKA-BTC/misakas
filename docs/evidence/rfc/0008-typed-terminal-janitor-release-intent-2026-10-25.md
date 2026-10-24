# RFC-0008: Typed terminal receipt and exact janitor Release intent

## Status

The final combined `rfc08_typed_` regression passed **4/4 public parent tests in 32.72s** on 2026-10-25. The separate new filter is `rfc08_typed_terminal_release_finalized_janitor_gc_all_generations_and_cold_wal_replay`; its writer passed in **20.87s**, including six reader handoffs (0.40/0.48/0.39/0.36/0.55/1.17s). These child runs are included in their parent, not additional independent acceptance cases. The earlier initial-publication/replacement/reorg writers passed in 2.23/3.68/5.66s and keep their scopes. No policy, wire, public preset, normal validator, model rule or activation value changes.

The final executable was `/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspa_consensus-53c349c8ce803ba3`, with complete runtime evidence in `/tmp/misaka-rfc-typed-terminal-native-tests-20261025.log`. It came from the coordinated reserve/native test rebuild, which completed in 3m59s after an unrelated first test-only E0308 assertion mismatch was corrected; the original compiler diagnostics remain in `/tmp/misaka-rfc-reserve-read-fault-tests-20261025.log`. No terminal fixture runtime failure was hidden or replaced by a synthetic result.

## Actual consumer defect

`MonetaryMaterialGcJanitorV1::tick` receives `(finalized_snapshot_pin, terminal_receipt)` from either native authority or the op242 proof adapter. Its terminal path used the snapshot pin as the deletion intent's `release_block`. That is different from the original accepted Release block in the receipt. The strict native and remote GC witness correctly refuses the substituted tuple, so receipt-based discovery could retain material indefinitely even after native finality.

The minimal production correction stores `receipt.release_block`. The finalized pin still authenticates receipt presence; it does not change the saved Release tuple. No consensus interpretation of the informational receipt field or new authority is introduced.

## Mechanism and controls

The new isolated public filter opts into the existing role driver after actual TypedKernelV4 initial material publication, both providers' durable preparation before signatures, native FundedEnroll, the real claim, signed replacement and all four old/new exact cold reads. It waits for the actual monetary row's payment/liability tail, signs namespace242 Release using the existing producer key and requires own native rehearsal to accept before sending the real funded carrier. The accepted native state must contain a terminal receipt covering both old and new generation consents.

Immediately after the signed Release, native finalized receipt authority must still be absent for both generations. The writer closes the DB and all four provider stores reopen in an acquisition-only child. Exact real GC witnesses must refuse and material/context snapshot bytes must remain unchanged.

The writer then mines actual heartbeats until native finalized receipt presence is available at a pin **different from the original Release block**. This difference makes the old janitor bug fail the saved-intent assertion. In the actual run at DAA 377, the finalized pin began `d4a0ef4050f7…` and the original Release block began `5d769a31c6cf…`; their complete distinct hashes are printed in the terminal log. All mandatory positive/negative native and remote controls, four saved intents, interruption/reopen/deletion checks and final accepted-state-root assertion completed successfully. A subsequent cold reader obtains the actual terminal receipt separately for each stored consent and checks both generations. Both the native adapter and the remote op242 proof pager must accept the exact tail/Release tuple and reject substituting the finalized pin as either field.

For providers 2/4 the actual janitor uses native authority; providers 3/5 use the real remote proof adapter. A fixture read-fault wrapper interrupts only witness revalidation, after the real janitor has fsynced its deletion intent. It supplies no positive authority. The test requires unchanged material/context bytes, the exact root/stored consent/release consent/tail/Release block in the persisted intent, then closes the store and janitor. On cold reopen the real witness revalidates and the janitor replays that intent, removes physical material and Typed context metadata, persists its completion receipt and removes its intent marker. Another store/archive/janitor reopen checks durable absence and completion, then metadata-cleanup acknowledgement itself is persisted. Native accepted state root remains unchanged across provider GC and the subsequent native writer restart.

## Limits

These are tiny deterministic fixture materials, not K3 or a selected-verifier/campaign acceptance result. Existing artifact/OPV fixture seams, explicit private unapproved fences, skip-PoW and simulated time remain visible. The native writer does not arm global resource admission; acquisition-only reader processes open and warm the existing DB before arming their local metadata budget, submit no native block/object and close all native DB owners before the writer reopens it. The shared heartbeat harness still exports via MutableBlock. This is not whole-node physical allocation, hard disk quota, OS-crash or StrictDB coverage.

The read-fault control is a recoverable janitor interruption, not a power-loss campaign. No delta is manually deleted and no pruning depth, safe frontier or policy is changed. Actual pruning, pruned IBD and proof import after such pruning remain separate work. No UNKNOWN campaign gate is promoted.
