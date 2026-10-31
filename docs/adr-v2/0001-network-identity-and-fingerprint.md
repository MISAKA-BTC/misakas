# ADR-v2-0001 — ネットワーク分離・Hash64・識別子・fingerprint

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 4本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0013](../rfc-v2/0013-permissionless-bootstrap-and-release.md) / [RFC-v2-0010](../rfc-v2/0010-dag-consensus-and-native-settlement.md)

## 1. この文書の役割

他ネットワークとの混同を防ぎ、同じ規則を実装したノードだけが同じidentityを名乗るための判断をまとめる。

## 2. 統合した判断の読み方

### 2.1

ネットワークはmainline Kaspa互換クライアントではなく別系統として扱い、genesis・接続先・アドレスprefixなどの分離を記録する。[旧ADR-0001](../adr/0001-network-isolation.md)

### 2.2

合意に見えるblock／transaction／Merkle等のidentityはHash64を境界まで一貫させる。部分的な切り詰めや別型への黙示変換を避ける判断を継承する。[旧ADR-0008](../adr/0008-hash64-consensus-identity.md)

### 2.3

Consensus fingerprintは有効化高さだけでなく意味上のrule manifestも識別する。表示名Misaka／BILIへの変更は、アドレスprefixや既存単位の変更ではない。[旧ADR-0150](../adr/0150-the-fingerprint-must-see-the-rule-not-only-the-height.md) [旧ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)

## 3. 維持する境界・後続改定

初期PoCのネットワーク値と現行releaseの値を混ぜない。旧MSK表記は原文履歴に残し、新しい説明の表示名だけを後続の命名判断に合わせる。

## 4. 隣接する判断との分担

署名・アドレスpayloadは[ADR-v2-0002](0002-post-quantum-signatures-and-interfaces.md)、releaseと有効化の実際の手順は[ADR-v2-0011](0011-release-activation-and-flag-days.md)、新genesisは[ADR-v2-0010](0010-genesis-emission-and-bond-free-bootstrap.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0001-network-isolation.md](../adr/0001-network-isolation.md) | L1–L134 | L12–L14 |
| [0008-hash64-consensus-identity.md](../adr/0008-hash64-consensus-identity.md) | L1–L307 | L12–L20 |
| [0150-the-fingerprint-must-see-the-rule-not-only-the-height.md](../adr/0150-the-fingerprint-must-see-the-rule-not-only-the-height.md) | L1–L187 | L12–L14 |
| [0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md) | L1–L57 | L12–L12 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0001-network-isolation"></a>

<details>
<summary>0001-network-isolation.md — 原文 L1–L134</summary>

[原文を開く](../adr/0001-network-isolation.md) · SHA-256: `05b7099891ef0f6ac156037b59440b7cc96e05ede047b3af9a3fb86162dea659`

<!-- ADR_SOURCE_BEGIN:0001-network-isolation.md -->
~~~~~~~~markdown
# ADR-0001: Network isolation from mainline Kaspa

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

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
~~~~~~~~
<!-- ADR_SOURCE_END:0001-network-isolation.md -->

</details>

<a id="source-0008-hash64-consensus-identity"></a>

<details>
<summary>0008-hash64-consensus-identity.md — 原文 L1–L307</summary>

[原文を開く](../adr/0008-hash64-consensus-identity.md) · SHA-256: `35bc3c5a55ecba2031f6d139ec30f47c34173b100cce6483e8528120785381dc`

<!-- ADR_SOURCE_BEGIN:0008-hash64-consensus-identity.md -->
~~~~~~~~markdown
# ADR-0008: Full Hash64 consensus identity

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Accepted (Phase 9 design freeze; PR-9.1 — PR-9.3 land the foundations)
Date: 2026-05-28
Supersedes: in part, [ADR-0004 §"Scope"](0004-utxo-commitment64.md)
            (UtxoCommitment64 was carved out as the only 64-byte
            field; this ADR widens the scope to all consensus
            identity).
Extends: [ADR-0007](0007-layered-pow.md) (the Layer 0 finalizer is
         the prototypical Hash64 producer; this ADR generalises that
         choice across the identity surface).

## Context

The kaspa-pq plan up to Phase 8 kept `kaspa_hashes::Hash` at its
upstream 32-byte width for everything except the UTXO commitment.
The reasoning at the time was diff-size: widening every consensus
hash would touch ~hundreds of files across header, txid, merkle,
RPC, P2P, database, and wallet code.

That stance leaves a quantum-margin asymmetry. Even with the Layer 0
PoW finalizer producing 512 bits (ADR-0007) and the UTXO commitment
widened to 64 bytes (ADR-0004), the block hash, transaction id,
merkle root, accepted-id merkle root, pruning point, and parent
references stay 256 bits. Grover-style preimage attacks therefore
still impose only a `2^128` cost on those identities — the chain
identity does not actually carry the post-quantum margin we set out
to give it.

For a **new network** (which kaspa-pq is, per ADR-0001), the case
for paying the diff cost up-front is much stronger than for a
mainline upgrade: there is no live chain to migrate; the storage
and wire-format cost is paid once, not retrofitted; and external
tooling that integrates against kaspa-pq pays for kaspa-pq's
semantics, not upstream's.

## Decision

kaspa-pq's consensus-visible hash identity is **64 bytes / 512
bits** end-to-end. Concretely, the following move from `Hash` (32B)
to `Hash64` (64B):

| Identity | Type | Producer |
|---|---|---|
| Block hash | `BlockHash` = `Hash64` | `BlockHash64` keyed BLAKE2b-512 over header preimage |
| Pre-PoW hash | `Hash64` | `BlockPrePowHash64` keyed BLAKE2b-512 |
| Transaction id | `TransactionId` = `Hash64` | `TransactionID64` keyed BLAKE2b-512 |
| Transaction hash | `TransactionHash` = `Hash64` | `TransactionHash64` keyed BLAKE2b-512 |
| Transaction sighash | `Hash64` | `TransactionSigningHash64` keyed BLAKE2b-512 |
| Merkle branch hash | `Hash64` | `MerkleBranchHash64` keyed BLAKE2b-512 |
| `hash_merkle_root` | `MerkleRoot` = `Hash64` | derived from MerkleBranchHash64 over txid64 leaves |
| `accepted_id_merkle_root` | `Hash64` | derived from MerkleBranchHash64 over accepted-id64 leaves |
| UTXO commitment | `UtxoCommitment` = `Hash64` | `UtxoCommitmentHash64` over LtHash 2048-byte state |
| Pruning point | `BlockHash` = `Hash64` | inherits BlockHash64 |
| Parent references | `Vec<BlockHash>` = `Vec<Hash64>` | inherits BlockHash64 |
| PoW final output | `PowHash512` = `Hash64` | `PowFinalHash64` (ADR-0007 §Decision) |
| `pubkey_hash` (P2PKH ML-DSA-65) | `[u8; 64]` payload | `BLAKE2b-512(public_key)` |
| Script-hash (P2SH) payload | `[u8; 64]` payload | `BLAKE2b-512(redeem_script)` |

The following stay at 32 bytes deliberately (no quantum-margin
requirement; the use is incidental or short-lived):

- Network id hash (used only for log-line identification, not
  consensus).
- Short cache keys / debug fingerprints inside the validator
  hot path.
- `kaspa-pq Layer 1 algo_id = 1` (`kHeavyHash v1`) internal
  hashes — Layer 1 is the ASIC-resistance dial and is explicitly
  swappable; its internal hashes are not consensus identity.
- ML-DSA `SigCacheKey` digests (ADR-0002 §7 — these are
  intentionally short to keep the cache memory budget small).
- BLAKE2b-256 over the 1952-byte ML-DSA-65 public key when used
  *as a cache key* (not as the address payload).

### Type layout

```rust
// crypto/hashes/src/lib.rs
pub struct Hash([u8; 32]);                  // existing upstream type
pub type   Hash32 = Hash;                   // documentation alias
pub struct Hash64([u8; 64]);                // new

// consensus/core re-exports
pub type BlockHash             = Hash64;
pub type TransactionId         = Hash64;
pub type TransactionHash       = Hash64;
pub type MerkleRoot            = Hash64;
pub type AcceptedIdMerkleRoot  = Hash64;
pub type UtxoCommitment        = Hash64;
pub type PowHash512            = Hash64;
```

Phase 9 ships `Hash32`, `Hash64`, and the 8 keyed BLAKE2b-512
hashers in `crypto/hashes`. Downstream crates migrate their use of
`Hash` to one of the type aliases above piece by piece. This is
the same pattern Phase 7 used to migrate the 32-byte UTXO
commitment to a separate `UtxoCommitment64` newtype before flipping
the header field.

### Hash function choice

All Hash64 producers use **BLAKE2b-512 keyed with a
domain-separator string**, mirroring the upstream `crypto/hashes/src/hashers.rs`
BLAKE2b-256 family pattern (`MuHashFinalizeHash` keyed with
`b"MuHashFinalize"`, etc.). BLAKE2b is RFC 7693, supports 1–64
byte outputs, and `blake2b_simd` (already in the workspace) covers
both widths.

The Layer 1 algo_id = 1 Phase 1 path remains in the Keccak family
(`cSHAKE256("HeavyHash")`), so the kaspa-pq PoW retains the
hash-family diversity property from ADR-0007: a structural break
in BLAKE2 does not invalidate Layer 1, and vice versa.

SHA3-512 was considered as an alternative. It has the "FIPS-202
standardised" advantage; it has the "shares its family with Layer
1" disadvantage. For Phase 9 we choose BLAKE2b-512 for
implementation symmetry with the existing kaspa-pq hash stack and
to preserve Layer 0 / Layer 1 family diversity.

### algo_id = 1 (kHeavyHash) seed derivation

The upstream kHeavyHash signature takes a 32-byte seed. With
`pre_pow_hash` widened to 64 bytes, the algo_id = 1 path
**derives** a 32-byte seed from the 64-byte pre-PoW hash:

```text
l1_seed32 = BLAKE2b-256(
    key   = b"kaspa-pq-l1-kheavyhash-v1-seed",
    input = pre_pow_hash64,
)
L1_tag32 = kHeavyHash_v1(l1_seed32, timestamp, nonce)
```

The kaspa-pq Layer 0 finalizer then consumes `L1_tag32` length-
prefixed (the ADR-0007 self-delimiting layout) so a future
`algo_id = 2` whose tag is a different length cannot collide with
the algo_id = 1 encoding. The seed derivation is domain-separated
on its own keyed BLAKE2b instance so that the 32-byte seed and
the 64-byte pre-PoW hash cannot be substituted for each other in
any other context.

### Address payload width

The two payload-carrying address versions widen:

| Version | Old payload | New payload |
|---|---|---|
| `PubKeyHashMlDsa65` | 32 bytes (`BLAKE2b-256` of public key) | **64 bytes** (`BLAKE2b-512` of public key) |
| `ScriptHash` | 32 bytes (`BLAKE2b-256` of redeem script) | **64 bytes** (`BLAKE2b-512` of redeem script) |

`PAYLOAD_VECTOR_SIZE` in `crypto/addresses/src/lib.rs` grows from
the current 36 to **at least 68** (64-byte payload + 4-byte slack
for future use). The legacy 32-byte `Version::PubKey` /
`Version::PubKeyECDSA` payloads remain representable for parser
completeness but are non-standard (ADR-0002).

A new consensus opcode `OP_BLAKE2B_512` is added so the P2PKH
template can hash the public-key push to a 64-byte value before
the equality check:

```text
OP_DUP
OP_BLAKE2B_512
OP_DATA64 <BLAKE2b-512(public_key)>
OP_EQUALVERIFY
OP_CHECKSIG_MLDSA65
```

`OP_DATA64` is the natural `OpData{N}` extension covering the
length 64 (the existing macro family stops at `OpData75` upstream,
so 64 fits inside the same opcode-number band). `MAX_SCRIPT_ELEMENT_SIZE`
already covers it (4096, set in Phase 4).

### Security framing — what we claim and what we don't

Per the user-provided security framing (faithfully reproduced):

> 64-byte hash commitments give a search space of **2^256** against
> Grover-style preimage attacks on a quantum adversary. This is the
> "256-bit quantum preimage margin" we claim.

> Quantum collision resistance under the BHT bound is approximately
> `2^(512/3) ≈ 2^170`, **not** `2^256`. We **do not** claim
> "256-bit quantum collision resistance".

The wording we use in user-facing material is therefore:

- ✅ "512-bit commitment domain"
- ✅ "256-bit quantum preimage margin"
- ✅ "high-margin quantum collision resistance"
- ❌ "256-bit quantum collision"
- ❌ "256-bit post-quantum security across the board"

This precision matters: an external claim of "256-bit post-quantum
security" would be wrong, and getting it wrong publicly is more
damaging than the actual security margin we have.

## Consequences

### Positive

- The chain's identity layer carries the same quantum-preimage
  margin (256-bit) as the PoW finalize. There is no asymmetry where
  the PoW is hardened but the txids are not.
- Single hash-construction choice (keyed BLAKE2b-N) across the
  consensus surface — easier to audit and easier for downstream
  language bindings to implement consistently.
- The "two hash families on the PoW" property (BLAKE2 for Layer 0,
  Keccak for Layer 1) is preserved.

### Negative

- ~2x growth on every RocksDB row that's keyed by or contains a
  hash. Approximate scaling:
  - Block hash: 32 → 64 bytes.
  - Transaction id: 32 → 64 bytes (every UTXO row's outpoint).
  - GHOSTDAG data: many `Hash` fields × 2.
- Wire: every `TransactionInput.outpoint.transaction_id` grows by
  32 bytes. ML-DSA-65 inputs are ~5 KB; the +32-byte outpoint is
  noise.
- RPC payload: every hex-encoded hash field doubles (64 → 128
  hex characters). Block explorer UI / wallets need to handle the
  wider display.
- Diff size: this is a several-thousand-line cascade across
  consensus, RPC, P2P, database, wallet, and SDK.

### Neutral

- Address strings grow by ~50% (32-byte → 64-byte payload, bech32
  is roughly 1.6 chars per byte after the prefix). A
  `kaspapq:` mainnet address goes from ~62 characters to ~100
  characters. Users will copy-paste either way; the change is
  cosmetic.

## Implementation order (revised 9-phase plan)

Phases 1–8 already shipped in this branch (commits `02eb0b9` →
`3d7eb82`) and serve as the kaspa-pq baseline. Phase 9 builds on
that baseline.

| Phase | Title | Status |
|---|---|---|
| 1 | Spec freeze | ✅ landed |
| 2 | Network isolation | ✅ landed |
| 3 | LtHash16_1024 UTXO accumulator (PoC) | ✅ landed |
| 4 | ML-DSA-65 P2PKH script | ✅ landed |
| 5 | Wallet key derivation + minimal CLI | ✅ landed |
| 6 | Mass policy benchmark | ✅ landed |
| 7 | RPC / WASM / SDK | ✅ landed (PR-7.1 – 7.6) |
| 8 | Layered PoW foundation (Layer 0) | ✅ landed (PR-8.1 – 8.3); PR-8.4 / 8.5 / 8.6 deferred |
| **9** | **Hash64 consensus identity** | **this ADR + PR-9.1 – PR-9.5** |

Phase 9 PR sequence:

- **PR-9.1: This ADR.** Design freeze; no code.
- **PR-9.2: crypto/hashes Hash64.** New `Hash64` struct, new
  `blake2b_512_hasher!` macro, 8 domain-separated keyed
  BLAKE2b-512 hashers. `Hash32 = Hash` documentation alias.
  Tests: hex roundtrip, Borsh, serde, fixed-length validation,
  hasher determinism.
- **PR-9.3: pow_layer0 Hash64 + l1_seed32.** Switch
  `pow_finalizer_blake2b_512` to take `Hash64` for `pre_pow_hash`.
  Add the `l1_seed32_for_kheavyhash_v1` helper. Tests: seed
  derivation is deterministic and key-separated from any other
  BLAKE2b-256 use.
- **PR-9.4: Spec update.** Remove the §1.2 "we do not widen Hash
  past 32 bytes" non-goal. Add the new goal. Reorder phases.
  Document the security framing word-for-word.
- **PR-9.5 (deferred): Consensus identity cascade.** Migrate
  `Header`, `Transaction`, `TransactionOutpoint`, merkle, GHOSTDAG,
  pruning, RPC, P2P, database, wallet, SDK call sites from `Hash`
  to the typed `Hash64` aliases. Recompute genesis hashes.
  This is the multi-PR, multi-session cascade — handled separately
  from PR-9.1 – 9.4 so the foundations can land first.

### Relationship to the previously-deferred PR-8.4 / PR-8.5 / PR-8.6

- **PR-8.4 (Header.pow_algo_id)** — still applies. Folded into
  PR-9.5 since the same Header struct changes anyway.
- **PR-8.5 (BlueWorkType: Uint192 → Uint576)** — independent of
  Hash64; still applies in its own PR.
- **PR-8.6 (PoW validator wiring)** — applies. Uses the Hash64
  `pre_pow_hash` produced by `BlockPrePowHash64` (introduced in
  PR-9.2). Implementation lands as part of the Phase 9 validator
  pass.

## References

- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md) (payload
  width discussion; this ADR widens the payload).
- [ADR-0003 — LtHash16_1024](0003-lthash-utxo-accumulator.md)
  (UTXO accumulator state; UtxoCommitmentHash64 finalises it).
- [ADR-0004 — UtxoCommitment64](0004-utxo-commitment64.md)
  (precursor of the same widening, now generalised).
- [ADR-0007 — Layered PoW](0007-layered-pow.md) (Layer 0 finalize
  is the prototypical Hash64 producer).
- RFC 7693 (BLAKE2). FIPS 202 (SHA-3 — alternative considered
  and rejected for hash-family-diversity reasons).
~~~~~~~~
<!-- ADR_SOURCE_END:0008-hash64-consensus-identity.md -->

</details>

<a id="source-0150-the-fingerprint-must-see-the-rule-not-only-the-height"></a>

<details>
<summary>0150-the-fingerprint-must-see-the-rule-not-only-the-height.md — 原文 L1–L187</summary>

[原文を開く](../adr/0150-the-fingerprint-must-see-the-rule-not-only-the-height.md) · SHA-256: `4ab5832c63ad2710c11cac41f9c6cee3a9b74b2af1f1916561d2506fa48df3e5`

<!-- ADR_SOURCE_BEGIN:0150-the-fingerprint-must-see-the-rule-not-only-the-height.md -->
~~~~~~~~markdown
# ADR-0150 — The fingerprint must see the RULE, not only the height

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** ACCEPTED, 2026-09-20. Implemented behind no fence: the manifest is a description of the
build, and adding it re-pins every preset's fingerprint exactly once, on testnet, before mainnet
genesis freezes the format.

> Two builds whose `consensus_params_id` is the same must return the same VALID/INVALID verdict for
> every block. Today they can differ, and the identity cannot say so.

## 1. The hole, as it was walked into

`consensus_params_id` hashes `Params`. For a fenced rule it hashes the fence's **height** and
nothing else — the code says so at each arm:

```rust
// ADR-0133 §11.2 readiness V2: the height only, Some-only.
if let Some(activation) = palw_readiness_v2 {
    h.write(b"palw_readiness_v2");
    h.write(activation.daa_score().to_le_bytes());
}
```

On 2026-09-20 the possession rule that fence turns on changed (ADR-0133 §11.2 as amended: the V2
challenge opens the prefix of its draw that one carrier's budget buys, because all sixteen of its
leaves serialized to 184,037 bytes for the shipped class and no transaction carries that). The
height did not move, no field of `Params` moved, and so:

```
old build fingerprint = ABC          new build fingerprint = ABC
       the same block:  old → VALID          new → INVALID
```

Nothing in the handshake distinguishes them. They peer, agree about every block until the height,
and split there — a partition between nodes that each believe they are on the same network, with no
event anywhere that says which rule anybody is running. This tree already has a name for that shape
(the 2026-09-17 note: *a cleanup that removes history rules without moving the fingerprint is a
silent fork*), and this is the same shape arrived at from the other direction: not by deleting a
rule, but by redefining one the fingerprint only knows by its address.

The readiness change is safe to ship **this time** because `palw_readiness_v2` has never been in
force anywhere — `None` on every shipped preset, scheduled on testnet-11 at a height that chain has
not reached — so the deployment rule is operational: every node carries the new build before that
height. That is a true mitigation for one instance and no answer at all to the class.

## 1.5 The goal, stated so it can be argued with

> **Same fingerprint ⇒ same consensus-relevant semantics.**

Not "same source", not "same binary": same *meaning* for every question a node answers about a
block. That sentence is what makes the rest of this ADR checkable, and it names the subsystems it
covers — header validity, the state transition, fork choice and weight, the DAA, reward accounting,
PALW admission, FreePrompt accounting, pruning and IBD interpretation, and the activation schedule.
A ruleset outside that list is a place the goal does not hold, which is why §2.1 lists them all.

### The ways a fingerprint fails, and what this does about each

The realistic danger is not a broken hash. At 512 bits the collision question is not the one to
spend attention on; **omission** is.

| failure | what it looks like | what this ADR does |
|---|---|---|
| **coverage** — a rule that decides validity is not hashed | the 2026-09-20 readiness change: same height, different verdict, same id | every subsystem is a manifest entry (§2.1); a rule not listed cannot be versioned, so the list is asserted by a test |
| **canonicalisation** — one meaning, two byte strings (or two meanings, one) | map iteration order, optional fields, integer widths | the manifest is a const array in its own order, every field length-prefixed; `the_manifest_encoding_is_canonical_and_unambiguous` |
| **domain separation** — `A=12,B=3` and `A=1,B=23` share a preimage | ambiguous concatenation | the digest leads with a format tag and the entry count, and each name is length-prefixed; same test |
| **rollback / downgrade** | a peer negotiates an older fingerprint format | nothing here is negotiated: the manifest is local, read from the build's own constants, and there is no compatibility mode to select. R1's silence is an encoding property, not a mode — it cannot be *asked for* |
| **partial coverage** | headers hashed, state transition or pruning not | §2.1's list is the whole of the consensus surface this tree has, including `pruning_ibd`, which is the quietest one: two builds can reconstruct different states without either rejecting a block |
| **manifest vs implementation** | the build says `READINESS_R2` and the code is R3 | §3.5's stored corpus: a verdict that moves without a revision is a red build, with the message saying which file to change |
| **over-trust** | same fingerprint, different verdict from UB, platform, overflow, endianness | out of scope for a hash and said so: the fingerprint is a *claim about intent*, and the corpus is the only part that observes behaviour. This ADR does not promise more |
| **hash strength** | collision on the id | BLAKE2b-512, keyed and domain-separated; the least likely of these and the only one that is purely cryptographic |

## 2. Decision

**The fingerprint hashes a rule manifest beside the params and the schedule.**

```
ConsensusFingerprint = H(
    network identity,
    consensus params,
    activation schedule,
    rule manifest:  header_validity, state_transition, fork_choice, pruning_ibd, daa,
                    palw_clock, palw_accounting, palw_freeprompt, palw_admission,
                    palw_readiness, palw_payout, palw_court, palw_verification,
                    palw_work_target, palw_independence
)
```

Each entry is a **semantic revision** of one ruleset: a small integer a developer raises when the
meaning of VALID changes inside that ruleset, whatever height it takes effect at.
`palw_readiness_v2` with `activation = 7,200, semantics = READINESS_R2` is a different fingerprint
from the same height at `READINESS_R1`, which is the property §1 lacked.

**Not the binary and not the commit.** A build hash would make a log line, an optimisation or a
compiler bump a consensus incompatibility, which teaches operators to ignore the field — the
failure mode this decision exists to avoid. The manifest is written by hand, in one file, and the
cost of that is the point: a change that alters validity must be accompanied by a developer saying
so.

### 2.1 The subsystems the manifest covers

```
header_validity   state_transition   fork_choice   pruning_ibd   daa
palw_clock   palw_accounting   palw_freeprompt   palw_admission   palw_readiness
palw_payout   palw_court   palw_verification   palw_work_target   palw_independence
```

The activation schedule is the tenth item of §1.5's list and is already hashed as heights, by the
fence arms and by `consensus_schedule_id`; the manifest does not repeat it.

**R1 is silent.** A ruleset at revision 1 — "the rule as this tree first wrote it" — contributes no
bytes, so adding entries does not move any fingerprint and a build that has changed nothing
fingerprints exactly as the build before the manifest existed. The first bump is the first time a
number moves, and today there is exactly one: `palw_readiness` R2. That is also what let this land
without partitioning a fleet mid-rollout.

### 2.2 Which id each revision enters, and why it is not all of them

This tree already separates three ids, and the manifest follows the same discipline rather than
inventing a fourth:

| id | what it is for | what the manifest adds |
|---|---|---|
| `consensus_identity_id` | the handshake's gate: two nodes that differ here are different networks and do not peer | the revisions of rulesets **in force** (unfenced, or fenced at genesis) |
| `consensus_params_id` | the full fingerprint an operator reads and a release is pinned to | **every** revision |
| `consensus_schedule_id` | reported, never gated, so a mismatch can be named | unchanged |

A ruleset whose fence is scheduled in the future is normalised out of the identity exactly as its
height is (`for_each_fence` rewrites a future height to "not yet"). That is what lets a fleet roll
out a new rule before the height it takes effect at: the builds are one network until the fence
fires, which is the property ADR-0066 SA-4 and audit3 H1 protect and which this must not break.

The consequence is the honest one: **a rollout across a scheduled rule change is still an
operational deadline**, and the node now prints the deadline and the rule tag beside its
fingerprint so the operator can compare two nodes by reading two lines. What the manifest removes
is the case where nobody could have known at all — a rule in force today, changed silently.

## 3. The invariants, pinned as tests

1. **A change to consensus validity moves the fingerprint.** Concretely: the corpus test below, plus
   `a_ruleset_revision_moves_the_params_id`.
2. **Comments, logs, RPC shapes and UI do not move it.** The manifest is hand-written and mentions
   none of them; `the_manifest_is_revisions_and_nothing_else` pins its shape.
3. **An activation height change moves it** — already pinned by
   `every_fence_the_visitor_reaches_moves_the_fingerprint`.
4. **A ruleset revision change moves it**, and moves the IDENTITY only where that ruleset is in
   force: `a_scheduled_rulesets_revision_leaves_the_identity_alone`.
5. **Same fingerprint ⇒ same verdict.** A stored adversarial corpus of consensus objects is replayed
   through the validity path and the verdicts are digested; the digest is pinned beside the
   manifest. A change that moves a verdict fails that test, and the failure says: raise the
   ruleset's revision and re-pin. This is not a proof — a corpus is a sample — but it converts
   "somebody should have noticed" into "the build does not compile green".

## 4. What this does not do

* It does not make an old node validate a new rule. It makes the difference **visible before the
  height** rather than discoverable after it.
* It does not version the wire protocol, the RPC surface or the database layout.
* It does not replace a flag day. A rule that changes what an in-force ruleset accepts still needs
  a height, a drill that crosses it, and a fleet that updates — the manifest is what makes a build
  that skipped any of those refuse to be mistaken for one that did not.
* It is not a substitute for reading the diff. A developer who raises no revision on a validity
  change defeats it; §3.5's corpus is the backstop that makes that a red build rather than a quiet
  one.

## 5. Order of work

1. The manifest, the two ids, the invariant tests, the node's printed line — this ADR's commit.
2. **Testnet-11 carries it first.** Every preset's pinned fingerprint moves once, here, and the
   pins are re-taken in the same commit; the identity of a network with no in-force manifest entry
   does not move, so the rollout is a rollout and not a partition.
3. Before mainnet genesis: freeze the manifest's field set and its encoding. Adding a ruleset after
   that is itself a fingerprint move, which is correct, but the FORMAT should stop changing.

## 6. The first entry

`palw_readiness = R2` records 2026-09-20's change (`9ef0d326`): the V2 possession challenge is
spent in bytes — the prefix of the draw one carrier's budget buys — because sixteen leaves did not
fit a transaction. R1 is the rule as ADR-0133 §11.2 first wrote it. No chain has run either, and
testnet-11 arms the fence at 7,200; the fleet's deadline is that height.
~~~~~~~~
<!-- ADR_SOURCE_END:0150-the-fingerprint-must-see-the-rule-not-only-the-height.md -->

</details>

<a id="source-0174-token-name-misaka-ticker-bili-address-prefix-unchanged"></a>

<details>
<summary>0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md — 原文 L1–L57</summary>

[原文を開く](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md) · SHA-256: `1ca8afa02713035f825760a6e902ebfdd095090a4eff6c439d8363c6d97ae8bd`

<!-- ADR_SOURCE_BEGIN:0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md -->
~~~~~~~~markdown
# ADR-0174 — Token name Misaka, ticker BILI; address prefixes stay unchanged

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Accepted naming and compatibility decision, 2026-10-07.

## Decision

The native token's display name is **Misaka** and its ticker is **BILI**.
Current ADR/RFC explanations, documentation, wallet balances, CLI help and human-readable
operator screens use these names. `MISAKA` may still identify the project, network,
repository, product or an existing external asset; it is not the native token's display name.

The amount represented by one coin does not change: **1 BILI = 100,000,000 sompi** on
the native UTXO ledger, and **1 BILI = 10^18 wei** in the EVM lane. These are the existing
lane-specific accounting units, not a new conversion or an economic parameter change.
Wallets keep their existing network-label convention: `BILI` on mainnet, `TBILI` on
testnet, `SBILI` on simnet, and `DBILI` on devnet. These prefixes identify environments;
they do not introduce different canonical token names or a claim about market value.

## Compatibility boundaries

- Address prefixes stay `misaka`, `misakatest`, `misakasim`, and `misakadev` for their
  existing networks. The address codec, payload, checksum and key rules are unchanged.
- Existing chain IDs, genesis commitments, network identities and signed domain strings
  are unchanged. In particular, the `MSK` mnemonic for `EVM_CHAIN_ID = 0x4D534B` remains
  a description of those fixed bytes, not the current display ticker.
- Binary names, subcommands, existing flags such as `--msk`, Rust identifiers such as
  `SOMPI_PER_MSK`/`parse_msk_amount`, and API/JSON fields such as `balanceMsk` stay compatible.
  Renaming a human-readable unit is not authorization to break a command or wire schema.
- The validator's amount parser accepts `BILI` as the current coin suffix while retaining
  existing `MSK` and `KAS` input aliases. Bare integers and `sompi` retain their previous
  meaning; accepted precision and overflow limits do not change.
- Historical measurements, audit records, literal historical quotations, and captured
  command output retain `MSK` as recorded, with a naming note. Their values and conclusions
  are not rewritten. Preserved code/output examples are legacy-compatible examples, not
  a second current ticker. Document filenames and link targets containing `msk` remain stable.

## Scope

This decision changes presentation and terminology. It does not alter issuance, rewards,
bond sizes, fees, signatures, addresses, fork choice, activation heights or consensus state.
It does not change an external token's on-chain metadata or deploy website changes.
The public-verifier objective in [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md)
and the completion gates in [RFC-0014](../rfc/0014-panel-independent-fraud-prosecution.md)
and [RFC-0015](../rfc/0015-panel-free-permissionless-verification.md) still govern.

Current explanatory prose in earlier ADRs and RFCs adopts BILI. Retained historical
evidence and fixed compatibility identifiers remain individually scoped exceptions under
this decision; their legacy label must not be used to redefine the token's current identity.
~~~~~~~~
<!-- ADR_SOURCE_END:0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md -->

</details>

