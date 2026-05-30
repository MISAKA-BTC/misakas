# ADR-0001: Network isolation from mainline Kaspa

Status: Accepted (Phase 1)
Date: 2026-05-28
Supersedes: —

## Context

The vendored base is rusty-kaspa workspace version `1.1.0`. Out of the box,
this code participates in the mainline Kaspa P2P network (and its testnet /
devnet / simnet variants). For a quantum-resistant fork, **any** accidental
peering with mainline Kaspa is unacceptable for three reasons:

1. The signature scheme is different. A mainline Kaspa transaction is
   not a valid kaspa-pq transaction, and the inverse is also true.
2. The UTXO accumulator is different. Header validation against a
   mainline tip would fail in non-obvious ways and waste validation
   budget.
3. Block/transaction propagation between the two networks would
   pollute the mempool of the kaspa-pq network with malformed traffic.

Mainline Kaspa node identity is established by a combination of:

- `NetworkId` (network kind + suffix).
- Address `Prefix` (`kaspa`, `kaspatest`, etc.).
- Genesis block hash.
- P2P listen port, RPC ports.
- DNS seed list.
- Protocol version / handshake magic.

If any of these is shared, we risk cross-talk.

## Decision

kaspa-pq is a **new network**, not a Kaspa-compatible client.
Concretely, in Phase 2 we change all of:

| Item | New value (PoC) |
|---|---|
| `NetworkId` kind | `KaspaPq` (new variant) |
| Default mainnet suffix | `kaspa-pq-mainnet` |
| Address prefix (mainnet) | `kaspapq` |
| Address prefix (testnet) | `kaspapqtest` |
| Address prefix (devnet) | `kaspapqdev` |
| Address prefix (simnet) | `kaspapqsim` |
| Default P2P port (mainnet) | `+0x4000` offset from upstream |
| Default RPC ports | `+0x4000` offset from upstream |
| DNS seeds | empty (operator-supplied only, no upstream Kaspa seeds) |
| Genesis block | newly generated, distinct hash |
| Initial UTXO commitment | empty LtHash16_1024 final commitment |
| Handshake protocol version | bumped, kaspa-pq-major.minor namespace |

Exact port numbers and protocol-version bytes are deferred to the
Phase 2 implementation PR; the rule is "must not collide with upstream".

## Consequences

### Positive

- A misconfigured kaspa-pq node cannot peer with a Kaspa mainline node.
- A mainline wallet cannot accidentally send funds to a kaspa-pq address
  (address prefix mismatch).
- Block explorers and bridges treat the two as distinct chains.

### Negative

- We lose the ability to test against the upstream live network.
  All integration testing must use kaspa-pq simnet/devnet/testnet
  spun up locally or from operator-provided seeds.
- Upstream rebases require careful audit of any new config defaults
  that might re-introduce mainline values.

### Neutral

- The `Prefix::A` / `Prefix::B` test prefixes used by upstream are kept
  available for cargo-test fixtures; they are non-routable test prefixes,
  not real networks.

## Alternatives considered

1. **Run kaspa-pq as a fork that re-uses Kaspa address prefixes.**
   Rejected: address prefix is the user-visible signal of network
   identity. Re-using it invites cross-network sends.
2. **Re-use the upstream `NetworkId` enum with a new suffix.**
   Rejected: every value of `NetworkId::Kaspa(_)` is still mainline
   in the rest of the code. Tag distinction must be at the enum-variant
   level, not at the suffix level.
3. **Same ports as upstream, rely on handshake magic only.**
   Rejected: this still gets us connection attempts and wastes both
   ends of the dial.

## Implementation notes for Phase 2

Files expected to change:

- `consensus/core/src/config/params.rs` — `Params` defaults per network.
- `consensus/core/src/config/genesis.rs` — new `genesis_block` value
  with empty-state `UtxoCommitment64`.
- `consensus/core/src/config/constants.rs` — magic / version constants.
- `consensus/core/src/network.rs` — `NetworkType` and `NetworkId` enum.
- `crypto/addresses/src/lib.rs` — `Prefix` variants.
- `protocol/p2p` — handshake version.
- `kaspad/src/args.rs`, `daemon/src/*` — default port arguments.
- `rpc/{grpc,wrpc}/*` — default RPC ports.
- `wallet/core/src/account/variants/*` and `wallet/keys` — default
  network prefix in wallet creation.

## Acceptance criteria (Phase 2)

1. Starting a kaspa-pq node with the default mainnet config does not
   peer with any non-kaspa-pq node, even when given an upstream Kaspa
   seed address.
2. A simnet launched from kaspa-pq genesis produces blocks under DAA.
3. A standard send-to-address using a `kaspa:` prefixed address is
   rejected by the wallet (address-parse error or network-mismatch
   error).
4. A handshake from a mainline Kaspa peer is rejected with a
   protocol-version / network-id mismatch error and does not consume
   buffered bytes past the handshake.

## References

- Upstream `consensus/core/src/network.rs` `NetworkType` enum.
- Upstream README §"The Crescendo Hardfork" (10 BPS post-fork is the
  block-rate baseline we inherit).
