# RFC-0006 / RFC-0008 borrowed coinbase validation — 2026-10-24

The actual virtual-state validator previously materialized mergeset partitions,
miner output indices and an expected payload while comparing a received coinbase.
It now shares borrowed blue/red/round iterators and a payload byte iterator with
the actual builder. Partition order, round payee grouping, reward arithmetic,
output order and serialized payload bytes retain their existing meaning.

The remaining comparison scratch is the existing round-payee grouping vector of
borrowed script references. Before rendering, the comparator reserves a checked
envelope of three times `max(round_count, 4)` entries, covering the geometric
old/new vector storage. With no round entries it requests no lease. The lease
lives through rendering and is released on match, mismatch or refusal.

The real `verify_coinbase_transaction` caller now propagates a resource refusal
as `RuleError::LocalNativeCapacityUnavailable`, using the existing local retry
path. It no longer unwraps this result. Payload errors retain their separate
`BadCoinbasePayload` classification. Insufficient local capacity is not a bad
coinbase verdict.

## Verification

The actual coinbase manager integration target passed **18/18** controls:

```sh
CARGO_TARGET_DIR=/tmp/misaka-rfc06-12-build-20261024 \
CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 \
cargo --config 'profile.test.package.kaspa-consensus-core.opt-level=0' \
  test -p kaspa-consensus --test coinbase_manager -- --nocapture
```

The controls cover every hashed transaction field, payload and output order,
blue/red/round rewards, V4 receipt splitting, escrow and inclusion carve rules,
unentitled rewards, payout queue prefix drain and carriage restoration. A child
process exhausts the native capacity budget before a round comparison: it observes
resource refusal, releases the blocker, retries successfully and checks that both
matching and mismatching comparisons release their reservation.

A thread-local allocation-request counter observes zero bytes during comparisons
with zero, one and eight payout scripts and no rounds. Inputs are prepared before
the measurement. This is an allocation-request control of this comparator; it is
not live RSS, a full-node cost bound or a whole-model measurement. Local test
output is `/tmp/misaka-coinbase-stream-controls.log`; the final source rerun also
passed 18/18 in `/tmp/misaka-coinbase-stream-controls-final.log`.

The real private node and all its test targets also passed `cargo check -p kaspad
--lib --tests --features private-integration-config` with the dedicated target and
debug/incremental disabled (7m19s, including the shared-target lock wait). Output is
`/tmp/misaka-consumer-node-check-final.log`. This checks the production error
propagation and the concurrent Memory, remote GC and ETH adapter source connections;
it is type/metadata verification, not native block acceptance. Later paid-claim
reader and current-material response work is outside that completed source snapshot.

## Still outside this result

Builder-owned output vectors, scripts, template/block lifetime and their full
reservation coverage remain separate work. The existing grouping algorithm's CPU
cost has not been changed or bounded by this patch. The payout fixture does not
earn or fund rewards, create selected-verifier approval, or accept a native block.
All-reward one-time settlement, production material lifecycle campaigns and the
15-gate acceptance campaign remain incomplete. Activation stays `None`, normal
selected-source/V6 bootstrap remains refused, and UNKNOWN gates remain UNKNOWN.

## Received-payload input reservation continuation

The earlier comparator measurement prepared its owned miner script before the
observation. The real validator's `deserialize_coinbase_payload` still allocated
that input script outside native admission. It now uses the same allocation-free
borrowed parser for decoding and pre-admission. A script fitting the inline
SmallVec needs no heap reservation. A spilling script reserves a conservative
power-of-two envelope, at most 256 bytes because its wire length is a u8, before
the actual copy. Extra data remains borrowed. The caller retains the ticket until
the comparison has consumed the miner data. Payload errors are propagated as
`BadCoinbasePayload`; local quota refusal stays `LocalNativeCapacityUnavailable`.

The final actual coinbase-manager target passed **21/21** controls (0.23 seconds
runtime), including all earlier reward/queue/comparison controls. New checks
compare actual decoder allocation requests with the admitted envelope for script
lengths 0, 35, 36, 64, 67, 150 and 255. An isolated capacity test exhausts the real
logical budget before admission, checks that malformed bytes remain a decode
error under the same exhausted budget, releases capacity and retries through
decode and comparison. Matching and mismatching outputs both release the input
reservation after the comparison. Log:
`/tmp/misaka-coinbase-input-admission-tests-final-20261024.log`.

The first build found a borrowed-parser lifetime error, corrected by borrowing
both script and extra data from the remaining payload slice. A concurrent core
and consumer API generation mismatch also required a coherent rebuild. Neither
failed build is counted as successful verification.

This closes the input-script copy in this validator call, not every legacy
deserializer caller, returned builder/template ownership, stack cost, physical
RSS or full-model execution cost. The campaign boundaries above still apply.
