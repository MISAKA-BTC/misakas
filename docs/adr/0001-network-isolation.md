# ADR-0001: Network isolation from mainline Kaspa

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


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
