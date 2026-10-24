# RFC-0012 committed reserve reads and local retry

## Consumer gap

The real template, expected-UTXO validation and overlay snapshot consumers used
`reserve_balance_store.get(...).unwrap_or(0)`. An actual missing row represents
zero in this live per-block store, but a failed decode or I/O read does not. The
same fallback could construct a zero-reserve coinbase or persist a zero-reserve
pruning snapshot from a failed read. Separately, the sink walk treated every
`verify_expected_utxo_state` error as a block rule violation, including its
existing local native capacity errors.

The shared checked reader now maps only `KeyNotFound` to zero. Typed store
capacity remains `LocalNativeCapacityUnavailable`; other store errors use the
existing local `StoreUnreadable` context channel and log the exact reserve point
and original store error. That channel's human-readable message is generalized
from bootstrap to native context; enum layouts and tags are unchanged.

Construction, validation and overlay snapshot use this reader. Local native
capacity/context errors from expected-UTXO validation return through the sink
walk's local fault channel before any disqualification status is written.
Snapshot capture returns a failed read before its database batch; the actual
pruner defers destructive pruning when capture fails. Existing missing-row
semantics, reserve arithmetic, payouts, network parameters and activation rules
remain unchanged.

This does not authenticate arbitrary validly encoded tampering or deletion of a
previously nonzero row. It is not independent accrual/drip reconciliation or a
complete economic store audit. Other overlay/header readers and snapshot write
failure handling are separate scopes. A capture fault may occur after pruning
metadata/UTXO advancement; retained history is kept, and no claim of whole
pruning-workflow atomicity is made here.

## Validation

Two native controls in `rfc12_reserve_read_faults` use actual malformed RocksDB
reserve rows. One requires template/snapshot refusal and unchanged saved
snapshot, then successful construction after repair. The other holds the real
virtual gate while header/body workers admit a genuine candidate, invokes the
actual sink walk against the failed read, requires pending status and unchanged
sink, repairs the store, then lets the original queued candidate finish normally.
The fixture keeps reserve economics dormant so genuine absence is zero; it does
not assert a successful real reserve payout or campaign acceptance.

The final coordinated consensus build and native controls pass. The first shared node build stopped at two
`E0004` errors after an unnecessary new local error variant was added without its
generated inventory. That variant was removed in favor of the existing typed
context channel; generator-only edits were restored, preserving existing private
adapters. No wire tags changed. Failed log:
`/tmp/misaka-rfc-source-response-carrier-tests-20261025.log`.

The first consensus test compile then stopped on a test-only `E0308`: the helper
returns `BlockStatus`, not `Option<BlockStatus>`. The assertion was corrected
without a production change. Failed log:
`/tmp/misaka-rfc-reserve-read-fault-tests-20261025.log`.

K3 inputs remain uncreated and the 15 campaign gates remain UNKNOWN. This local
fault boundary does not establish OS-crash, 100k, pruning/IBD or all-fee acceptance.


Final commands used the coordinated target, debug info disabled, incremental
compilation disabled, 64 MiB Rust thread stack and the existing core test-profile
opt-level override. `/tmp/misaka-rfc-reserve-read-fault-tests-retry1-20261025.log`
records **2/2 PASS, 0.12 s**, build **3m59s**. The actual final binary is
`/tmp/misaka-rfc06-12-build-20261024/debug/deps/kaspa_consensus-53c349c8ce803ba3`.
Against that same binary, existing active DNS overlay construction/validation
passes **1/1, 0.03 s** (`/tmp/misaka-rfc-reserve-overlay-native-regression-20261025.log`)
and native local-capacity selection/retry passes **1/1, 0.50 s**, including its
child **0.39 s** (`/tmp/misaka-rfc-reserve-capacity-selection-regression-20261025.log`).
The current Typed initial/replacement/reorg/terminal controls also pass **4/4,
32.72 s** (`/tmp/misaka-rfc-typed-terminal-native-tests-20261025.log`); those are
the same four controls documented by the Typed evidence, not four extra tests.

An attempted historical bond regression matched **zero tests** in this non-EVM
binary (`/tmp/misaka-rfc-reserve-overlay-positive-regression-20261025.log`), since
that module is EVM-feature gated. It is no runtime evidence. EVM caller metadata
compilation separately passes: `cargo check -p kaspa-consensus --tests --features
evm` with the same coordinated environment/profile override, **3m33s**,
`/tmp/misaka-rfc-reserve-evm-callers-check-20261025.log`. This type-checks both
changed feature-gated snapshot call sites; it is no EVM runtime result.
Real EVM campaign gates remain UNKNOWN.
