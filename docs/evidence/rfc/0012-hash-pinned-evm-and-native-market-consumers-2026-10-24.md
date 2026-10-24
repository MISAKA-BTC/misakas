# RFC-0012 hash-pinned EVM and native market consumers — 2026-10-24

This connects existing adapter capabilities to real acceptance-driver callers. It is
not a real node, bridge transfer, market fill, 15-gate campaign, or security acceptance
run. K3 formal inputs remain uncreated. No public network, deployment, signed transfer,
activation height, consensus rule or predeclared acceptance threshold is changed.

## EVM balance and bridge consumer

The ETH [parser](../../../rpc/eth/src/lib.rs) already accepts EIP-1898 selectors and the
native [provider](../../../kaspad/src/eth_rpc.rs) resolves a hash to its own EVM/L1 block
and checks `requireCanonical`. The acceptance [balance reader](../../../scripts/drills/rfc12_acceptance_observe.py)
previously selected a latest hash but queried accounts by its mutable numeric height.
A same-height reorg could therefore attach another block's balances to the old hash.

`evm_value` now queries every account with `{blockHash, requireCanonical: true}` using
the observed latest hash. It re-reads ETH latest after the accounts and refuses a changed
hash/number or disappearance with ContextMoved. Hashes must have the adapter's 32-byte
shape, and block numbers/balances must be canonical unsigned ETH quantities within
u64/U256 respectively. An unsupported/noncanonical selector
remains an error; there is no numeric or latest-state fallback. The existing bridge
`evm_set` caller additionally requires one native sink and DAA across participating
processes before it combines balances, and requires the same ETH number and identical
account balances across all those nodes rather than checking only the first row. Existing Latest/Safe/Finalized publication
checks are retained; they are not newly certified by this change.
The withdrawal reconciliation caller now retains the before-balance hash as well as
its number. The gas interval is hash-walked backwards from the after-balance hash,
requiring every block number to decrease by exactly one and the final parent to equal
the original before-balance hash. The frozen endpoint is checked against the canonical
numeric mapping before and after the scan. Numeric height no longer selects fee data.
A receipt must match that block hash/number and transaction hash/index. Missing bodies
or receipts never become zero gas, duplicate/cyclic paths refuse, and canonical
unsigned quantities/status are checked. The existing maximum-block bound is enforced
before any RPC. Each read and transaction loop checks the remaining workflow/phase
budget; per-request timeouts are clamped to that remaining budget.

The actual withdrawal caller combines its JOIN gas observation with the same JOIN
balance generation (sink, DAA, network and import stamp), inside another native bracket.
A moved generation or incomplete read retries only the balance/gas read set in the
existing bounded wait, without repeating the submitted withdrawal. Gas and balance rows
are preserved together in evidence. Full bridge economic/security acceptance still
requires a real campaign and all of the other native/outpoint/fee checks.
The ETH adapter's `full_txs_for` previously skipped a transaction when its body lookup
returned None. It now returns an explicit server error rather than rendering a partial
full block; a lookup returning another transaction hash or another/missing
block hash, number or transaction index is also an error. Both
`eth_getBlockByNumber` and `eth_getBlockByHash` use this path. Hash-only requests remain
available and do not request full bodies. The gas consumer propagates unavailable full
block data as a retryable incomplete observation, never a zero-fee interval.

## Market consumer

The native `getPalwModelPositions` response already carries holder, tipHash and tipDaa.
The acceptance [market phase](../../../scripts/drills/misaka-rfc12-acceptance.py) now calls
`model_position_value`, checking that response against the same role's public RPC
bracket. Missing rows do not become zero holdings; an explicitly empty list still
means zero at that tip. Wrong holders, malformed identities/units, out-of-u64 values and
repeated lines are unavailable observations. A stale tip is retryable.

The phase checks all process baselines and fill reads at the same native sink/DAA and
requires equal holder units before issuing its existing market commands. It then uses
the existing exact buy/sell delta predicate. These checks do not authenticate a market
order, fee, price, or fill on their own and do not authorize running transfers here.

## Verification

[Nineteen new controls](../../../scripts/drills/tests/test_rfc12_evm_market_reads.py) run the
actual `evm_value` over an owned loopback HTTP server and exercise the actual `evm_set`
and `phase_market` callers with oracle transports. They cover hash selectors, a
same-height reorg, changed/disappeared latest, selector refusal without fallback,
different native sinks despite the same ETH hash, stale cached positions, unavailable
and duplicate rows, bad units, contradictory baselines, and exact buy/sell deltas.
Additional gas controls cover ordered hash walks, skipped numbers, cycles, missing
blocks/receipts, wrong receipt identity/position, wrong before endpoint, endpoint reorg,
empty intervals, predeclared span/deadline limits, and the actual reconciliation caller
refusing a JOIN generation mismatch before fee RPCs. The HTTP server is a transport
control, not the MISAKA node or a real ETH adapter run.

```text
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc12_evm_market_reads.py' -v
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc12_acceptance_observe.py' -v
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc12_acceptance_driver.py' -v
python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc12_acceptance_judgement.py' -v
```

Results: 19 new + 12 observer + 4 driver + 36 judgement controls passed. No real
multiprocess campaign, historical signature replay, bridge payment, or market settlement
is claimed; the corresponding unmeasured gates remain UNKNOWN/unexecuted.

## ETH full-block adapter dispatcher controls

The actual JSON-RPC dispatcher controls in
[full_block_tests.rs](../../../rpc/eth/src/full_block_tests.rs) exercise both block lookup
methods: missing bodies return an error instead of a partial result, hash-only replies
need no full body reads, complete bodies retain their committed order, wrong transaction
identity refuses, and another/missing block hash/number/index refuses. These use a typed
oracle EthProvider, not a live native node or campaign.

```text
CARGO_TARGET_DIR=/tmp/misaka-rfc06-12-build-20261024 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test -p kaspa-eth-rpc --lib full_block_tests -- --nocapture
```

The five actual dispatcher controls passed (build 7m54s; runtime 0.01s), including
another/missing block context across both block methods. Combined with the 71 Python
controls above, 76 controls passed. The Rust adapter production path was built and run;
its synthetic provider inputs do not establish any real-node campaign gate.
