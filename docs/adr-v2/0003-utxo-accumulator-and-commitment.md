# ADR-v2-0003 — UTXO accumulatorと64-byte commitment

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 2本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0012](../rfc-v2/0012-authenticated-state-storage-and-ibd.md)

## 1. この文書の役割

UTXO集合のaccumulatorとheaderに載るcommitmentの型・幅を区別して固定する。

## 2. 統合した判断の読み方

### 2.1

集合の追加・除去を行うLtHash accumulatorと、その状態を最終hashしたUTXO commitmentは別の層として扱う。[旧ADR-0003](../adr/0003-lthash-utxo-accumulator.md) [旧ADR-0004](../adr/0004-utxo-commitment64.md)

### 2.2

ADR-0003の冒頭にあるLtHash32_1024への監査改定を読み、本文に残るLtHash16_1024の初期案をそのまま新規実装へ戻さない。[旧ADR-0003](../adr/0003-lthash-utxo-accumulator.md)

### 2.3

64-byte commitmentの専用型により、通常hashや初期PoCの32-byte仮置きとの取り違えを防ぐ。[旧ADR-0004](../adr/0004-utxo-commitment64.md)

## 3. 維持する境界・後続改定

原文内の初期案と監査後の改定を併録する。ここでaccumulatorの新方式や新しいhash定数を選び直していない。

## 4. 隣接する判断との分担

PALW stateのroot／snapshotは[ADR-v2-0012](0012-state-root-pruning-and-snapshots.md)、行単位の認証stateは[ADR-v2-0049](0049-authenticated-state-map-and-row-stream.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0003-lthash-utxo-accumulator.md](../adr/0003-lthash-utxo-accumulator.md) | L1–L199 | L12–L14 |
| [0004-utxo-commitment64.md](../adr/0004-utxo-commitment64.md) | L1–L169 | L12–L14 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0003-lthash-utxo-accumulator"></a>

<details>
<summary>0003-lthash-utxo-accumulator.md — 原文 L1–L199</summary>

[原文を開く](../adr/0003-lthash-utxo-accumulator.md) · SHA-256: `c8d4d836cbbf0c4ba66a058c0de49f32e332f85ecdec0c4568680ca58921dc79`

<!-- ADR_SOURCE_BEGIN:0003-lthash-utxo-accumulator.md -->
~~~~~~~~markdown
# ADR-0003: LtHash UTXO accumulator (LtHash16_1024 → LtHash32_1024)

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Accepted (Phase 1); REVISED (audit QM-1) — migrated to LtHash32_1024.
Date: 2026-05-28 (rev. 2026-06-03)
Supersedes: —

> **Revision (audit QM-1):** the accumulator was migrated from the 16-bit-lane
> `LtHash16_1024` (~100-bit quantum collision margin — the weakest quantum link
> in the system) to the 32-bit-lane `LtHash32_1024` (4096-byte state, ~170-bit
> quantum collision), the variant §"Alternatives considered" below listed as
> deferred. The serialized state doubled (2048 → 4096 B) and the genesis
> `utxo_commitment` / block hashes were recomputed (re-genesis); the element
> expansion and 64-byte BLAKE2b-512 finalize are otherwise unchanged. The
> 16-bit analysis below is retained for historical context.

## Context

Upstream rusty-kaspa uses a 3072-bit multiplicative MuHash as its UTXO
accumulator. Verified by inspection of [crypto/muhash/src/lib.rs](../../crypto/muhash/src/lib.rs):

```rust
pub const SERIALIZED_MUHASH_SIZE: usize = ELEMENT_BYTE_SIZE; // 384
pub(crate) const ELEMENT_BIT_SIZE: usize = 3072;
pub(crate) const ELEMENT_BYTE_SIZE: usize = ELEMENT_BIT_SIZE / 8;
```

This is **not** ECMH over secp256k1; it is multiplicative MuHash in a
3072-bit RSA-style group. Security rests on a discrete-log-style
assumption in that group, which falls to a sufficiently large quantum
computer running Shor's algorithm.

Replacing it with an additive lattice-style accumulator (LtHash) buys
us:

- A symmetric-key-only security argument (LtHash reduces to SIS
  / lattice problems).
- Cheap component-wise add/remove (the same operation, just with the
  inverse element).
- A simple serialization (a fixed-size byte string of lanes).

The Meta LtHash design uses 1024 lanes of 16 bits each, i.e. a
2048-byte state. The accumulator is a commutative group under
component-wise addition modulo `2^16`. Public references claim ≥200-bit
classical and ≥100-bit quantum security for the 2048-byte instantiation.

A known footgun: with 16-bit lanes, adding the same element 2^16 times
returns to the empty state (each lane wraps). Therefore each element
must be uniquely tagged.

## Decision

kaspa-pq replaces the MuHash UTXO accumulator with **LtHash16_1024**:

- `LTHASH_LANES = 1024`, `LTHASH_LANE_BYTES = 2`,
  `LTHASH_STATE_BYTES = 2048`.
- The accumulator type is `LtHashUtxoAccumulator`, internally
  `lanes: [u16; 1024]`.
- The set operation is component-wise addition mod `2^16`. Remove is
  component-wise subtraction mod `2^16`.
- The element-to-lane-vector map is BLAKE3 (XOF) over the element
  serialization, expanded to `LTHASH_STATE_BYTES`. BLAKE3 is chosen
  for its native XOF interface; BLAKE2b-512 is an acceptable
  fallback and the final choice is locked in at Phase 3.

### Element serialization

```
element_bytes =
    "kaspa-pq-utxo-v1"           ||  // 16-byte literal domain tag
    txid                          ||  // 32 bytes
    output_index                  ||  // 4 bytes, little-endian
    amount                        ||  // 8 bytes, little-endian
    script_public_key_version     ||  // 2 bytes, little-endian
    script_public_key_len         ||  // 4 bytes, little-endian
    script_public_key             ||  // variable
    daa_score                     ||  // 8 bytes, little-endian
    is_coinbase                       // 1 byte (0x00 or 0x01)
```

Every UTXO element therefore embeds its `(txid, output_index)`, which
is unique by definition of a confirmed UTXO set. This rules out the
2^16-duplicate collision.

### Migration path inside crypto/muhash

To keep the diff bounded in Phase 3, we keep the upstream API surface
during the PoC and replace the **implementation**:

- `new()`
- `add_element_byte_hash(...)` / equivalent
- `remove_element_byte_hash(...)`
- `combine(other)`
- `serialize()`
- `deserialize(bytes)`
- `finalize()` → 32-byte digest in PoC, 64-byte `UtxoCommitment64`
  in production (see [ADR-0004](0004-utxo-commitment64.md))

When the API has settled, rename the crate from `kaspa-muhash` to
`kaspa-utxo-accumulator` and the type from `MuHash` to
`UtxoAccumulator`. Old type names remain as deprecated aliases for
exactly one release.

### Header field

The block header's UTXO commitment field changes meaning, not name,
during the PoC. It carries the LtHash final commitment (32-byte PoC
mode or 64-byte production mode). See
[ADR-0004](0004-utxo-commitment64.md) for the 64-byte type.

## Consequences

### Positive

- Quantum-resistant UTXO commitment.
- 2048-byte state is small in absolute terms and trivially
  serializable.
- Add and remove are the same primitive operation, so DAG-merge logic
  ("apply this block's diff") and rollback ("undo this block's diff")
  are symmetric.
- Order independence falls out of commutativity, so DAG ordering
  edge cases that bit upstream MuHash work also won't bite here.

### Negative

- Each block must touch 2048 bytes of state instead of 384. In
  practice this is dominated by RocksDB I/O and is not a hotpath
  concern, but it is a real bytes-on-disk increase per UTXO set
  snapshot.
- A bug in the lane-vector derivation (e.g. forgetting to bind the
  outpoint, or padding the XOF wrong) is silent — it produces a
  valid-looking 2048-byte state that disagrees with everyone else.
  Property tests are therefore non-optional.

### Neutral

- The 16-bit lane wrap is a defended attack surface, not a residual
  risk: as long as elements carry their outpoint, the attacker
  cannot construct 2^16 copies of the same element.

## Alternatives considered

1. **Keep MuHash, swap the underlying group.** Rejected: any
   discrete-log-style group falls to Shor's algorithm.
2. **Use a hash-based Merkle accumulator with per-block delta
   commitments.** Rejected for the PoC: implementation cost is
   higher, and the DAG structure of Kaspa makes per-block delta
   replay harder than the commutative LtHash design.
3. **LtHash16_512 (1024-byte state).** Rejected: the size win is
   small and the security margin is reduced.
4. **LtHash32_1024 (4096-byte state).** Deferred: a future ADR may
   bump to 32-bit lanes if the 2^16 collision class proves to be
   harder to defend than expected.

## Implementation notes for Phase 3

Files expected to change:

- `crypto/muhash/src/lib.rs` — replace `MuHash` implementation with
  `LtHashUtxoAccumulator`. Keep `MuHash` type alias for the PoC.
- `crypto/muhash/src/u3072.rs` — delete or move to an `attic/`
  module; the multiplicative group is not used.
- `crypto/muhash/src/element_hash.rs` — replace with a BLAKE3-XOF
  expansion to 2048 bytes.
- All `add_element` / `remove_element` callers — should not need to
  change because the public surface is preserved.
- Genesis: recompute the initial UTXO commitment as the empty-state
  serialization of `LtHashUtxoAccumulator`. Document the expected
  empty-state hex string in a test fixture.

## Acceptance criteria (Phase 3)

1. `add(e)` then `remove(e)` returns the empty-state commitment.
2. The accumulator is order-independent: for any two permutations
   of the same set, the commitments are equal.
3. `combine(self, other) == add_all(self, other.elements)` whenever
   the two sets are disjoint.
4. Serialized state is exactly `LTHASH_STATE_BYTES = 2048`.
5. Deserialization of 2048-byte state and then re-serialization is
   the identity.
6. The 2^16 duplication attack is **not** possible against unique
   UTXOs (property test: random UTXO sets never collide under the
   set-of-`(txid, index)` partial order).
7. An invalid-block rollback (apply diff D, detect failure, apply
   −D) restores the previous commitment exactly.

## References

- Meta LtHash design paper and OSS implementation.
- Upstream [crypto/muhash/src/lib.rs](../../crypto/muhash/src/lib.rs).
- [ADR-0004 — 64-byte UTXO commitment](0004-utxo-commitment64.md).
~~~~~~~~
<!-- ADR_SOURCE_END:0003-lthash-utxo-accumulator.md -->

</details>

<a id="source-0004-utxo-commitment64"></a>

<details>
<summary>0004-utxo-commitment64.md — 原文 L1–L169</summary>

[原文を開く](../adr/0004-utxo-commitment64.md) · SHA-256: `0edbbbd7537f86908d13966049557de77f9e00b23e8e7ef6f2066fc2b1e1a26e`

<!-- ADR_SOURCE_BEGIN:0004-utxo-commitment64.md -->
~~~~~~~~markdown
# ADR-0004: 64-byte UTXO commitment via a dedicated `UtxoCommitment64` type

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Accepted (Phase 1 freeze; PR-7.6 introduced the type + finalize path)
Date: 2026-05-28
Supersedes: —

## Phase 7 PR-7.6 delivery

PR-7.6 (commit on the kaspa-pq branch) introduced the type, the finalize
path, and the RPC wire form, but **kept the header field at 32 bytes**.
The header switch is the last mechanical step and is split out so it can
land on its own once any consumer that reads the header field is ready
for the type change.

What landed in PR-7.6:

| Location | Item |
|---|---|
| [consensus/core/src/utxo_commitment.rs](../../consensus/core/src/utxo_commitment.rs) | `UtxoCommitment64` newtype, `UTXO_COMMITMENT_64_BYTES = 64`. No `From`/`Into<Hash>` conversion (type discipline). |
| [crypto/muhash/src/lib.rs](../../crypto/muhash/src/lib.rs) | `MuHash::finalize_64() -> [u8; 64]` (BLAKE2b-512 of the 2048-byte LtHash state); `EMPTY_MUHASH_64` constant. |
| [rpc/core/src/model/kaspa_pq.rs](../../rpc/core/src/model/kaspa_pq.rs) | `RpcUtxoCommitment64` newtype, Borsh + serde JSON hex encoding, `From`/`Into<UtxoCommitment64>` conversions. |
| `EMPTY_MUHASH_64` | `2938eeb12521e01d…b8eb820d6b` (LtHash empty-state BLAKE2b-512 digest, asserted by `test_empty_hash_64`). |

What is deferred to a follow-up "header-switch PR":

- `Header::utxo_commitment` field switch from `Hash` to
  `UtxoCommitment64`. This touches ~30 files (genesis, header hashing,
  RPC header conversion, p2p convert, bridge, testing/integration,
  WASM optional_header). The type, finalize path, and RPC wire form
  already exist in the branch, so the switch is mechanical.
- Genesis hash recompute (4 networks) once the header field changes.

## Context

LtHash16_1024 (see [ADR-0003](0003-lthash-utxo-accumulator.md))
materializes as a 2048-byte state. For storage inside the block
header we must finalize that state into a fixed-size digest. The two
realistic choices are:

- **32-byte finalize.** Cheap. Compatible with reusing
  `kaspa_hashes::Hash` (which is 32 bytes everywhere in the
  codebase). However, 32 bytes (256 bits) cannot honestly claim the
  ≥200-bit security level of the underlying LtHash16_1024 state once
  you account for collision security ≈ half-output.
- **64-byte finalize.** Preserves the full security margin of the
  underlying state. Requires a dedicated type because everything
  else in the header uses 32-byte hashes.

A blanket widening of `kaspa_hashes::Hash` to 64 bytes is a
non-starter for this PoC. It would touch:

- every database key in `database/`,
- every RPC message and gRPC proto in `rpc/grpc/*` and `rpc/wrpc/*`,
- every WASM binding in `wasm/`, `crypto/txscript/src/wasm/`,
- every wallet on-disk format,
- every merkle-tree node, every txid, every block hash.

That diff is the size of a separate project.

## Decision

Introduce a dedicated newtype:

```rust
#[derive(Copy, Clone, Eq, PartialEq, Hash, Default)]
pub struct UtxoCommitment64([u8; 64]);
```

placed in `consensus/core/src/header.rs` (or a new
`consensus/core/src/utxo_commitment.rs`, decided in Phase 3/4
implementation).

- The block header's UTXO-commitment field switches to
  `UtxoCommitment64` (production target).
- For the PoC, a 32-byte commitment may be used to keep the diff
  small. If used, the spec line in `kaspa-pq-spec.md` §3.3 must
  flag this as a "PoC shortcut, not the production security claim".
- All other hash-typed fields (`txid`, block hash, merkle root,
  accepted-id merkle root, RPC `Hash`) remain `kaspa_hashes::Hash`
  (32 bytes).
- The finalize function is `BLAKE2b-512(state_bytes)` or
  `BLAKE3-XOF(state_bytes, 64)`. The final choice is locked in
  alongside [ADR-0003](0003-lthash-utxo-accumulator.md).

### Type discipline

- `UtxoCommitment64` must not have a `From<Hash>` impl.
- `Hash` must not have a `From<UtxoCommitment64>` impl.
- RPC serializers must encode `UtxoCommitment64` as a 64-byte
  hex/byte field whose JSON / proto name is distinguishable from
  any 32-byte hash field.
- The wallet, indexer, and explorer should display
  `UtxoCommitment64` as 128 hex chars; a 64-hex display would
  silently misrepresent a truncated value.

## Consequences

### Positive

- Honest security claim. The block-header commitment matches the
  underlying accumulator's claimed security level.
- Bounded blast radius. Only the header, the accumulator, the
  validator, and the RPC encoding for that one field have to learn
  about the 64-byte type.
- Optionality at PoC time. We can ship a 32-byte commitment PoC and
  flip the type later without a second consensus-level rewrite,
  because the rest of the system already knows about
  `UtxoCommitment64` from Phase 1.

### Negative

- Two hash widths in the codebase. Reviewers must be alert to
  `Hash` ↔ `UtxoCommitment64` confusion. Mitigation: no `From`
  conversions, distinct field names, distinct hex display widths.
- Header byte size grows by 32 bytes per block when we move from
  PoC (32) to production (64). At 10 BPS this is ~28 MB/year. Not
  a concern given block-data dominance.

### Neutral

- WASM and TypeScript SDKs grow a new `UtxoCommitment64` type. They
  do not break the existing `Hash` type.

## Alternatives considered

1. **Widen `kaspa_hashes::Hash` to 64 bytes globally.** Rejected:
   diff size, see §Context.
2. **Stuff 64 bytes into two `Hash` fields.** Rejected: footgun,
   downstream tooling will assume each field is independently
   meaningful.
3. **Keep 32-byte commitment in production too.** Rejected for the
   production spec, allowed only as a PoC shortcut. We would not
   be able to claim the LtHash16_1024 security level honestly.

## Implementation notes for Phase 3/4

- The type is introduced in Phase 3 alongside the accumulator,
  even if Phase 3 still emits a 32-byte commitment in the PoC,
  so that downstream code can already depend on
  `UtxoCommitment64`'s existence.
- A `to_be_truncated_hash(&self) -> Hash` helper is **not**
  provided. If a downstream component needs a 32-byte view, it must
  define and name its own truncation explicitly.

## Acceptance criteria

- `UtxoCommitment64` exists as a distinct Rust type, with no
  cross-conversion to `Hash`.
- Block header serializes the UTXO-commitment field with the
  width selected by the active build (32 PoC / 64 production).
- RPC and CLI display widths match the active build, and the
  documentation flags PoC mode whenever the 32-byte variant is
  in use.
- The empty-state finalize value is committed as a fixture and
  matches the genesis-block UTXO-commitment field.

## References

- [ADR-0003 — LtHash16_1024](0003-lthash-utxo-accumulator.md).
- BLAKE2b-512 (RFC 7693). BLAKE3 specification.
~~~~~~~~
<!-- ADR_SOURCE_END:0004-utxo-commitment64.md -->

</details>

