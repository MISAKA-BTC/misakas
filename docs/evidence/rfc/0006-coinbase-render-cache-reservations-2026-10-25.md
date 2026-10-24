# RFC-0006: coinbase render and template-cache reservations (2026-10-25)

## Scope

The owning coinbase renderer now requests capacity before copying each output
and script. Its output vector uses fallible geometric growth, with an envelope
for simultaneous old/new vector requests. Payload and miner-output index
requests are admitted before construction. The borrowed comparison path still
does not copy the payout output collection. Round-payee grouping scratch is
admitted in both construction and comparison.

`CoinbaseAllocationV2` moves with the render into `BlockTemplate`. It has no
implicit clone. Cache `Arc` references share the original owner; an independent
`BlockTemplate::try_clone_v2` requests a separate coinbase/miner-data ticket
before copying. Miner replacement and heartbeat/round reserialization reserve
their simultaneous payload/script requests before copying. Round adaptation
transfers the original ticket into its replacement template.

Capacity failure remains a local retry error. In particular the production
template builder no longer unwraps coinbase generation, and converts capacity
failure to `RuleError::LocalNativeCapacityUnavailable` rather than a bad-block
verdict. Expired, clock-invalid and attestation-bypassed cache entries release
their ownership before replacement construction. External `Arc` holders still
retain their allocation until the last reference drops.

## Limits

These are typed allocation requests for the named coinbase consumers. They do
not establish whole-node RSS, whole-block, header, EVM or RPC-copy coverage.
Extracting a raw `MutableBlock`/`Block` from the public template fields can still
detach its payload from the template ticket. Those exports and the remaining
RPC and raw-block copy consumers require further ownership work. Public fields
also allow other mutation paths; this change covers the explicitly connected
native renderer, cache clone and adaptation paths, not every possible mutation.

No model cost boundary, historical-header consumer, production court, K3
acceptance or RFC-0012 campaign gate is closed by these changes. Public
activation remains unarmed; the 15 real campaign gates remain unexecuted.

## Verification

Scoped verification passed for the coinbase/mining source in this change:

- `processes::coinbase::`: 12/12, including the isolated quota child. It checks
  refusal before output growth, typed propagation from the real renderer,
  borrowed comparison under exhausted quota, retained ownership and capacity
  returned after the last owner drops. Log:
  `/tmp/misaka-rfc-coinbase-render-focused-tests-20261025.log`.
- Cache lifetime: 2/2; miner replacement: 2/2; another miner's cached coinbase
  ID: 1/1. Logs `/tmp/misaka-rfc-coinbase-mining-{cache,modify,cached-id}-tests-20261025.log`.
- `cargo check -p kaspa-consensus -p kaspa-mining --lib --tests`: PASS, before
  subsequent parallel quote-clock/API changes. Log
  `/tmp/misaka-rfc-coinbase-template-check-stable-20261025.log`. A final integrated
  check is still required for those later changes.

Consensus test binary SHA-256:
`b31eb5e6ba00297f69c3acf4b7b62709f270d420aa600621d1c86004563c2ee0`;
mining test binary:
`38b54d54b14ee492a6944bc6b76bfdc0226ebb0e289e92f2c168e6c092db17d3`.
These identify local artifacts only. The binaries do not establish K3 or
campaign acceptance.

The initial broad `coinbase` substring also selected unrelated long pipeline
fixtures. That run exposed the new source-economics test's real own-history
failure, then was interrupted during another long fixture after compilation
completed. It is **not** a passing suite. Its complete log is retained at
`/tmp/misaka-rfc-coinbase-template-tests-20261025.log`; the source-economics
failure is being investigated separately. The focused runs above do not hide
or close that failure.
