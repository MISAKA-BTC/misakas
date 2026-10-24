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

## Still outside this result

Builder-owned output vectors, scripts, template/block lifetime and their full
reservation coverage remain separate work. The existing grouping algorithm's CPU
cost has not been changed or bounded by this patch. The payout fixture does not
earn or fund rewards, create selected-verifier approval, or accept a native block.
All-reward one-time settlement, production material lifecycle campaigns and the
15-gate acceptance campaign remain incomplete. Activation stays `None`, normal
selected-source/V6 bootstrap remains refused, and UNKNOWN gates remain UNKNOWN.
