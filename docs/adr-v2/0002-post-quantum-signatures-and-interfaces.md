# ADR-v2-0002 — 耐量子署名・アドレス・RPC／WASM／SDK型

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 3本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0009](../rfc-v2/0009-remote-client-and-sdk.md) / [RFC-v2-0013](../rfc-v2/0013-permissionless-bootstrap-and-release.md)

## 1. この文書の役割

署名方式の移行と、その値を運ぶwallet・RPC・WASM・SDKの型を一体で読む。

## 2. 統合した判断の読み方

### 2.1

旧ML-DSA-65 P2PKHの判断と、ML-DSA-87／64-byte payloadへの後続移行を同じ履歴に置く。古いscript例を新方式の規範としてコピーしない。[旧ADR-0002](../adr/0002-mldsa65-p2pkh.md) [旧ADR-0019](../adr/0019-mldsa87-migration.md)

### 2.2

署名domain、公開鍵・署名サイズ、script制約、アドレス表現を揃え、RPC／wRPC／WASM／SDK間で異なる意味の型を同じものとして扱わない。[旧ADR-0006](../adr/0006-rpc-wasm-sdk-types.md) [旧ADR-0019](../adr/0019-mldsa87-migration.md)

### 2.3

launch時のP2SH／multisig除外は専用の再承認が必要な範囲として残す。CLIや型名に旧方式名が残る記述は、ADR-0019の後続改定と照合する。[旧ADR-0019](../adr/0019-mldsa87-migration.md)

## 3. 維持する境界・後続改定

ADR-0002本文のML-DSA-65／32-byte例は置換された方式の履歴。ADR-0019のfull rename改定と、旧識別子温存案を同時に現行方針としない。

## 4. 隣接する判断との分担

ネットワーク名とfingerprintは[ADR-v2-0001](0001-network-identity-and-fingerprint.md)、秘密鍵の取り扱いと運用操作は[ADR-v2-0029](0029-remote-client-mining-and-host-sandbox.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0002-mldsa65-p2pkh.md](../adr/0002-mldsa65-p2pkh.md) | L1–L172 | L12–L15 |
| [0006-rpc-wasm-sdk-types.md](../adr/0006-rpc-wasm-sdk-types.md) | L1–L289 | L12–L15 |
| [0019-mldsa87-migration.md](../adr/0019-mldsa87-migration.md) | L1–L159 | L12–L13 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0002-mldsa65-p2pkh"></a>

<details>
<summary>0002-mldsa65-p2pkh.md — 原文 L1–L172</summary>

[原文を開く](../adr/0002-mldsa65-p2pkh.md) · SHA-256: `8e883ba80b55cb09e8af5665eacbc20ce0b7fdc475e7eda9a63143fa765d7f29`

<!-- ADR_SOURCE_BEGIN:0002-mldsa65-p2pkh.md -->
~~~~~~~~markdown
# ADR-0002: ML-DSA-65 P2PKH as the only standard script

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Accepted (Phase 1). **Signature scheme superseded by [ADR-0019](0019-mldsa87-migration.md)** — ML-DSA-65 → **ML-DSA-87**, full PQ-only. The P2PKH structure and acceptance criteria here still hold; what changed: the scheme name/sizes (pk 2592 / sig 4627), the tx context `b"kaspa-pq-v1/tx/mldsa87"`, and the address payload (64-byte BLAKE2b-512). Source identifiers still read `MlDsa65` pending a cosmetic rename (design doc §J / ADR-0019 P8, deferred).
Date: 2026-05-28
Supersedes: —
Superseded-by: ADR-0019 (signature scheme)

## Context

The mainline Kaspa transaction script uses Schnorr (secp256k1) or ECDSA
over secp256k1. Both schemes are vulnerable to a sufficiently large
quantum computer running Shor's algorithm against the discrete-log
problem. To make the chain useful for long-term value storage in the
PQ-transition era, we replace the signature scheme with a NIST FIPS 204
standardized scheme.

ML-DSA-65 keys and signatures are large:

- public key: 1952 bytes
- signature: 3309 bytes (+1 byte sighash type when scripted)
- private key: 4032 bytes

This rules out P2PK (pay-to-public-key) as the standard output form,
because P2PK would store the full 1952-byte public key inside every
UTXO, exploding UTXO set size. P2PKH (pay-to-public-key-hash) keeps
outputs at ~36 bytes and pushes the public key to the spending input
where it disappears after confirmation.

## Decision

The **only** standard output script in kaspa-pq is ML-DSA-65 P2PKH:

```
scriptPubKey:
    OP_DUP
    OP_BLAKE2B_256
    OP_DATA32 <BLAKE2b-256(public_key)>
    OP_EQUALVERIFY
    OP_CHECKSIG_MLDSA65
```

A new address `Version::PubKeyHashMlDsa65 = 2` is introduced. Its
32-byte payload is `BLAKE2b-256(public_key)`. The existing
`PAYLOAD_VECTOR_SIZE = 36` SmallVec capacity already accommodates
a 32-byte payload, so the address-bech32 layer does not need a layout
change.

A new opcode `OP_CHECKSIG_MLDSA65` is added. It pops `<public_key>`,
then `<signature || sighash_type>`, length-checks both **before** any
allocation-heavy work, computes the sighash with
`calc_mldsa65_signature_hash`, and calls
`libcrux_ml_dsa::ml_dsa_65::verify` with the fixed context string
`"kaspa-pq-v1/tx/mldsa65"`.

`MAX_SCRIPT_ELEMENT_SIZE` is widened from `520` to `4096` to admit
the 3310-byte signature item and 1952-byte public-key item.

ML-DSA P2PK, legacy `Version::PubKey`, legacy `Version::PubKeyECDSA`,
legacy multisig, and `Version::ScriptHash` wrapping any of the above
are explicitly **non-standard** in kaspa-pq. Whether to remove them
from the consensus opcode table outright or to keep them disabled at
mempool admission is a Phase 4 decision; the design rule is that
no PoC wallet can produce or accept them.

The `libcrux-ml-dsa` dependency is pinned exactly:

```toml
libcrux-ml-dsa = "=0.0.9"
```

with `Cargo.lock` committed and `cargo build --locked` in CI. RustSec
advisory monitoring is mandatory.

## Consequences

### Positive

- Standard outputs become quantum-resistant.
- UTXO entries stay small (~36 bytes scriptPubKey) — the giant key
  material only appears on the input side.
- One single output template means simpler wallets, simpler mass
  policy, and simpler test surface.

### Negative

- Inputs are large (~5.3 KB per spend) — transaction byte mass and
  signature-op mass both have to be retuned (see
  [ADR-0005](0005-mass-policy.md)).
- We lose multisig at PoC time. Reintroducing it requires either
  threshold ML-DSA (not yet standardized) or a script-hash style
  composition, deferred to a follow-up ADR.
- We are temporarily dependent on a `<0.1` library
  (`libcrux-ml-dsa 0.0.9`). The library is high-assurance but
  pre-release; mitigations are exact pinning, lockfile commitment,
  differential KAT testing against a second implementation, and
  RustSec monitoring.

### Neutral

- We deliberately use BLAKE2b-256 for the public-key hash because the
  consensus opcode table already contains `OP_BLAKE2B_256`. Switching
  to BLAKE3 would require adding a new consensus opcode; that is a
  larger change than the PoC needs.

## Alternatives considered

1. **SLH-DSA (FIPS 205) instead of ML-DSA-65.** Rejected: signatures
   are an order of magnitude larger again, and verify cost is
   higher. ML-DSA is the right operating point for an L2-style
   chain where verify throughput matters.
2. **Falcon / FN-DSA.** Rejected for the PoC: less mature
   constant-time implementations in Rust, floating-point
   verification subtleties.
3. **Hybrid Schnorr + ML-DSA.** Rejected for the PoC: doubles the
   signature footprint with little PQ benefit if the Schnorr half
   is required to verify.
4. **P2PK ML-DSA-65 (no hash).** Rejected for UTXO-set size reasons.
5. **Custom BLAKE3-based P2PKH.** Rejected for PoC; revisit in Phase 4
   if benchmarks justify a new consensus opcode.

## Implementation notes for Phase 4

Files expected to change:

- `crypto/addresses/src/lib.rs` — add `Version::PubKeyHashMlDsa65`.
- `crypto/txscript/src/opcodes/mod.rs` — add `OP_CHECKSIG_MLDSA65`.
- `crypto/txscript/src/standard.rs` — add
  `pay_to_pub_key_hash_mldsa65`; route from `Version::PubKeyHashMlDsa65`.
- `crypto/txscript/src/script_class.rs` — new `ScriptClass` variant.
- `crypto/txscript/src/lib.rs` — bump `MAX_SCRIPT_ELEMENT_SIZE` to
  `4096`.
- New module `crypto/txscript/src/mldsa65.rs` — verify wrapper,
  length pre-checks, context constant.
- `consensus/core/src/sig_hash.rs` (or equivalent) —
  `calc_mldsa65_signature_hash`.
- `consensus/core/src/sigcache.rs` (or equivalent) —
  `Mldsa65SigCacheKey`.
- `Cargo.toml` of the workspace — `libcrux-ml-dsa = "=0.0.9"`.

## Acceptance criteria (Phase 4)

1. A well-formed ML-DSA-65 P2PKH spend on simnet verifies and the
   tx is accepted into a block.
2. Each of the following malformations is rejected **without**
   reaching `verify`:
   - wrong public-key length,
   - wrong signature length,
   - wrong sighash type byte,
   - non-canonical push,
   - script element > `MAX_SCRIPT_ELEMENT_SIZE`.
3. A spend with a public key whose `BLAKE2b-256` does not match the
   address payload is rejected.
4. Context-mismatch (e.g. a signature made with a different `ctx`)
   is rejected.
5. The signature-cache stores `Mldsa65SigCacheKey` (96 bytes plus
   tag) and not the raw signature/public key.

## References

- FIPS 204 (ML-DSA).
- `libcrux-ml-dsa` 0.0.9 on docs.rs.
- RustSec advisory regarding pre-0.0.9 AVX2 verify acceptance of
  invalid signatures.
~~~~~~~~
<!-- ADR_SOURCE_END:0002-mldsa65-p2pkh.md -->

</details>

<a id="source-0006-rpc-wasm-sdk-types"></a>

<details>
<summary>0006-rpc-wasm-sdk-types.md — 原文 L1–L289</summary>

[原文を開く](../adr/0006-rpc-wasm-sdk-types.md) · SHA-256: `bcf2f67fe5745ebe90ffaff03044791ffa4efc2f3060969b3a01e5e7b8c44911`

<!-- ADR_SOURCE_BEGIN:0006-rpc-wasm-sdk-types.md -->
~~~~~~~~markdown
# ADR-0006: RPC / WASM / SDK types for kaspa-pq

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Accepted (Phase 7 scope freeze)
Date: 2026-05-28
Supersedes: —
Depends on: [ADR-0002](0002-mldsa65-p2pkh.md), [ADR-0003](0003-lthash-utxo-accumulator.md), [ADR-0004](0004-utxo-commitment64.md)

## Context

Phases 1–6 changed the kaspa-pq consensus layer in three ways that the
RPC / WASM / SDK surface has to follow:

1. **Address payload version** — a new `Version::PubKeyHashMlDsa65 = 2`
   replaces the legacy `PubKey` / `PubKeyECDSA` variants as the only
   standard send template.
2. **Signature primitive** — ML-DSA-65 (FIPS 204) byte-blob fields
   (1952-byte public key, 3309-byte signature) replace 32 / 64-byte
   secp256k1 fields on the input side.
3. **UTXO commitment** — `UtxoCommitment64` is the production header
   field (PoC PR shipped 32 bytes for diff control; ADR-0004 commits
   to widening this to 64 bytes for the production launch).

The on-disk consensus encoding for these is now finalized
(crate `kaspa-muhash`, `kaspa-txscript`, `kaspa-addresses`). The RPC
side still encodes everything as if upstream Kaspa fields apply, which
is wrong at the byte-length level: an RPC client that asks the node for
a transaction's input scripts will receive ML-DSA-65-sized pushes but
the proto field is still typed as `bytes` (untagged), so language
bindings emit no compile-time signal that this is a new wire format.

Phase 7's job is to make that signal explicit at the RPC, WASM, and TS
layers, without re-litigating the on-chain decisions of Phases 1–6.

## Decision

### 1. Scope

In scope for Phase 7:

- **gRPC proto** updates (new field options / variants where the
  meaning changed, new top-level fields where new info is exposed).
- **wRPC** model + Borsh/JSON encoding additions for the same.
- **WASM** bindings: kaspa-pq address class, ML-DSA-65 type wrappers,
  `UtxoCommitment64` type.
- **TypeScript** type declarations matching the WASM bindings.
- **kaspa-rpc-core** model types.
- One example app per language flavour (`rust`, `wasm/js`,
  `wasm/python` if applicable) demonstrating: build → sign with
  ML-DSA-65 → submit.

Out of scope for Phase 7 (deferred):

- gRPC service backward compatibility with upstream Kaspa nodes —
  kaspa-pq is a separate network (see ADR-0001), so the gRPC service
  name and version are explicitly different.
- Header-version bump to expose `UtxoCommitment64` as a 64-byte field
  on the RPC. Phase 7 starts by RPC-exposing the 32-byte PoC width;
  ADR-0004 already commits to widening to 64 bytes, and that switch
  will be a single point-update to the proto + wRPC + WASM in a
  follow-up.
- A `kaspa-pq-wallet` GUI integration. The minimal CLI in
  `wallet/pq-cli/` (Phase 5') is the supported entry point.

### 2. gRPC proto changes

The kaspa-pq gRPC service is **renamed** to make the chain identity
explicit on the wire:

```proto
package protowire.kaspapq;

service KaspaPqRpcService {
    ...
}
```

(upstream Kaspa uses `package protowire;` `service RPC`.) Renaming
guarantees no mixed-chain client will accidentally connect.

For each existing RPC message that carried a script blob, version, or
hash, the kaspa-pq proto adds **per-field comments** documenting the
new lengths and a new `kaspa_pq` extension subfield where the
semantics differ:

| Upstream proto field | kaspa-pq treatment |
|---|---|
| `bytes script_public_key` | Encoded as **kaspa-pq scriptPubKey**. For the standard send template this is a 37-byte ML-DSA-65 P2PKH template (see ADR-0002 §"scriptPubKey (output)"). |
| `bytes signature_script` | Encoded as kaspa-pq signatureScript. For a standard spend this is `push(sig\|\|sighash_type)` followed by `push(public_key)` — approximately 5.3 KB. |
| `bytes utxo_commitment` | 32 bytes in Phase 7 (matches PoC final commitment), to be widened to a separate `bytes utxo_commitment_64` field when ADR-0004's production switch flips. |
| `string address` (RPC display) | Carries the `kaspapq*` prefix family. RPC servers must not emit `kaspa*`-prefixed strings. |

New top-level enum (Phase 7 only — not in upstream proto):

```proto
enum AddressVersion {
    PUB_KEY                  = 0;  // legacy, parse-only in kaspa-pq
    PUB_KEY_ECDSA            = 1;  // legacy, parse-only in kaspa-pq
    PUB_KEY_HASH_ML_DSA_65   = 2;  // kaspa-pq standard
    SCRIPT_HASH              = 8;
}
```

`reserved` blocks around the deleted upstream message-type numbers
where the kaspa-pq proto removes legacy messages, so future field
additions can't accidentally re-use a tag.

### 3. wRPC (Borsh + JSON)

`kaspa-rpc-core` adds three new types:

```rust
/// 1952-byte ML-DSA-65 public key. Borsh: fixed-size byte array.
/// JSON: hex string of length 3904.
pub struct RpcMlDsa65PublicKey(pub [u8; 1952]);

/// 3309-byte ML-DSA-65 signature.
pub struct RpcMlDsa65Signature(pub [u8; 3309]);

/// 32-byte kaspa-pq PoC UTXO commitment. Will be replaced by
/// RpcUtxoCommitment64 when the production switch happens.
pub struct RpcUtxoCommitment(pub [u8; 32]);
```

For Borsh, all three derive `BorshSerialize` + `BorshDeserialize`
natively (Borsh supports primitive byte arrays of any length).

For JSON (wRPC), each type implements `Serialize` / `Deserialize` as
hex-encoded strings with a fixed length validated at deserialize time.
The serde encoding **must** be hex strings — not `serde_bytes` — so
that JSON clients can read them with no special framing.

### 4. WASM bindings

`crypto/addresses/src/wasm.rs` Phase 2 TODO is resolved: the
`#[ignore]`-marked tests get re-enabled with regenerated `kaspapq:`
test vectors. The `prefix` JS-side field accepts the kaspa-pq prefix
strings (`"kaspapq"`, `"kaspapqtest"`, `"kaspapqsim"`, `"kaspapqdev"`).

A new module `wasm/src/kaspa_pq.rs` exposes:

```rust
#[wasm_bindgen]
pub struct MlDsa65PublicKey(...);

#[wasm_bindgen]
pub struct MlDsa65Signature(...);

#[wasm_bindgen]
pub struct UtxoCommitment(...); // 32-byte PoC width
```

Each with `from_hex`, `to_hex`, `to_bytes`, and `from_bytes`
constructors / accessors. Length checks are enforced and surfaced as
`JsValue`-typed errors.

`KaspaPqKeyPair` is the WASM-side mirror of
`kaspa_wallet_keys::kaspa_pq::KaspaPqMlDsa65KeyPair`. Exposed
methods: `from_seed`, `from_mnemonic`, `address`,
`public_key_bytes`, `sign`.

### 5. TypeScript declarations

`wasm/types/kaspa-pq.d.ts` is auto-generated from the WASM bindings.
Notable additions:

```typescript
export type KaspaPqNetwork =
    | "mainnet"
    | "testnet-10"
    | "simnet"
    | "devnet";

export type KaspaPqAddressPrefix =
    | "kaspapq"
    | "kaspapqtest"
    | "kaspapqsim"
    | "kaspapqdev";

export class MlDsa65PublicKey {
    static fromHex(hex: string): MlDsa65PublicKey;
    toHex(): string;
    toBytes(): Uint8Array;   // length 1952
}

export class MlDsa65Signature {
    static fromHex(hex: string): MlDsa65Signature;
    toHex(): string;
    toBytes(): Uint8Array;   // length 3309
}

export class KaspaPqKeyPair {
    static fromMnemonic(
        phrase: string,
        passphrase: string,
        network: KaspaPqNetwork,
        account: number,
        change: number,
        index: number,
    ): KaspaPqKeyPair;
    address(): Address;
    publicKeyBytes(): Uint8Array; // length 1952
    sign(message: Uint8Array, randomness: Uint8Array): Uint8Array; // length 3309
}
```

The on-chain RPC types (`RpcMlDsa65PublicKey`, etc.) are emitted with
`Uint8Array` typing plus a comment naming the required length.

### 6. SDK examples

One example per supported language:

- `rpc/wrpc/examples/kaspa_pq_send.rs` — sign + submit a tx on simnet.
- `rpc/wrpc/examples/vcc_v2_kaspa_pq.ts` — same as above from
  TypeScript (assumes a node available at the kaspa-pq default ports
  per ADR-0001).
- `wasm/examples/kaspa-pq-address-derivation.html` — minimal HTML +
  WASM demo of mnemonic → kaspa-pq address.

Each example carries a `// kaspa-pq Phase 7` comment at the top
pointing back at this ADR.

## Consequences

### Positive

- Cross-language clients can compile-time-check the kaspa-pq wire
  format. A TypeScript caller that tries to put a 64-byte Schnorr
  signature into an `MlDsa65Signature` slot fails at the type layer.
- The gRPC service-name change makes mixed-chain client errors loud
  rather than silent.
- The 32-byte → 64-byte UtxoCommitment switch becomes a single,
  trackable proto change in the future instead of a cross-cutting
  rewrite.

### Negative

- Phase 7 touches many files (rpc/core model, rpc/grpc proto, all
  three wRPC encoders, WASM bindings, TS declarations, examples).
  Realistic delivery is one focused PR per surface, gated on the
  preceding one merging.
- `protoc` and the wasm-bindgen toolchain become hard build deps for
  the kaspa-pq node release artefact. The upstream README already
  documents `protoc` for gRPC build; the wasm-bindgen path is new for
  some downstream packagers.

### Neutral

- Removing legacy `PubKey` / `PubKeyECDSA` from the kaspa-pq RPC
  serialization entirely is **out of scope** for Phase 7. They remain
  representable for parsers, but no RPC method emits them as standard
  output addresses.

## Implementation order (Phase 7 PR sequence)

1. **PR-7.1: rpc/core types.** Add `RpcMlDsa65PublicKey`,
   `RpcMlDsa65Signature`, `RpcUtxoCommitment`; thread them through the
   relevant model structs. Tests: roundtrip Borsh + JSON encoding.
2. **PR-7.2: gRPC proto rename + AddressVersion.** Rename service to
   `KaspaPqRpcService`, package to `protowire.kaspapq`, add the new
   `AddressVersion` enum, add `reserved` blocks. Tests: proto compile +
   one gRPC roundtrip.
3. **PR-7.3: wRPC encoders.** Plumb the new core types through Borsh
   and JSON wRPC paths; add hex (de)serialization with length
   validation.
4. **PR-7.4: WASM bindings + TS declarations.** New
   `wasm/src/kaspa_pq.rs`, regenerate `kaspa_pq.d.ts`. Re-enable the
   wasm-bindgen tests in `crypto/addresses/src/wasm.rs` with
   `kaspapq:`-prefixed vectors.
5. **PR-7.5: SDK examples.** Rust, TypeScript, HTML.
6. **PR-7.6 (separate, gated): UtxoCommitment 32 → 64 switch.** Bump
   header version, add `RpcUtxoCommitment64`. This is the production
   commitment cut per ADR-0004 §"Decision".

## References

- [ADR-0001 — Network isolation](0001-network-isolation.md)
- [ADR-0002 — ML-DSA-65 P2PKH](0002-mldsa65-p2pkh.md)
- [ADR-0004 — UtxoCommitment64](0004-utxo-commitment64.md)
- [kaspa-pq-spec.md](../spec/pq/kaspa-pq-spec.md) §12 (ADR index, kept in sync with this ADR.)
~~~~~~~~
<!-- ADR_SOURCE_END:0006-rpc-wasm-sdk-types.md -->

</details>

<a id="source-0019-mldsa87-migration"></a>

<details>
<summary>0019-mldsa87-migration.md — 原文 L1–L159</summary>

[原文を開く](../adr/0019-mldsa87-migration.md) · SHA-256: `eb2aea507e9d4ea52f21d3557329363e817b0a56bd40b80ba709aa7047febc09`

<!-- ADR_SOURCE_BEGIN:0019-mldsa87-migration.md -->
~~~~~~~~markdown
# ADR-0019: Migrate Signature Scheme from ML-DSA-65 to ML-DSA-87

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


## Status
Accepted (2026-05-30); **Revised 1.1 — 2026-05-31** (scope expanded to PQ-only completion); **Revised 1.2 — 2026-06-01** (md2 alignment: identifier rename, v2 contexts, keyed address payload, 16_384 caps).

> **Two dangling references (noted 2026-08-20).** `0008-pq-genesis-premine.md` and
> `0003-pq-address-format.md` do not exist on this lineage, and the numbers 0003 and 0008 are held
> here by [ADR-0003 (LtHash UTXO accumulator)](0003-lthash-utxo-accumulator.md) and
> [ADR-0008 (Hash64 consensus identity)](0008-hash64-consensus-identity.md) — different decisions
> entirely. Follow the number and you read the wrong ADR, which is the hazard
> [ADR-0036](0036-palw-mainnet-activation-model.md) Decision 5 exists to stop. The claims those
> links carried (this migration forces a new genesis; the address format is unchanged) stand on
> their own and are restated in §"Consequences"; the links are de-linked rather than repointed,
> because there is no live ADR to point them at.

Supersedes the signature-level choice in [ADR-0002](0002-mldsa65-p2pkh.md). Forces a new genesis (see `ADR-0008 (pq-genesis-premine)` — **not resident**; see the note below); **not compatible** with any prior kaspa-pq chain state, UTXO set, or address.

---

## Revision 1.2 — md2 (PQ-only design v2.0) alignment (2026-06-01)

Revision 1.1 deferred the identifier rename and used v1 signing contexts, an *unkeyed* address payload, and 10_000 caps. The **md2** design pass closed these. The current code governs; where 1.1 / 1.0 conflict with the items below, **1.2 governs**.

- **Naming — DONE (S10).** The deferred `MlDsa65`→`MlDsa87` rename has landed: `MlDsa87` / `MLDSA87_*` / `mldsa87` across all crates, the address version `PubKeyHashMlDsa87`, opcode idents, and the WASM `signTransactionMlDsa87` binding. (Only ADR-0002's *filename* keeps `mldsa65`.) The 1.0/1.1 "keep `MlDsa65` identifiers" policy is **withdrawn**.
- **Signing contexts v1→v2 — DONE (S11b).** Tx context = `b"kaspa-pq-v2/tx/mldsa87"`, sighash domain = `b"kaspa-pq-v2/sighash/mldsa87"`. Signer + verifier move in lock-step; no genesis change.
- **Address payload now KEYED — DONE (S11c).** The 64-byte ML-DSA-87 P2PKH payload is a **keyed** BLAKE2b-512 of the verification key under `b"kaspa-pq-v2/address/mldsa87"` (was *unkeyed*). The `OP_BLAKE2B_512` opcode recomputes this keyed hash at spend time; the opcode, the wallet/premine/validator derivations all share `kaspa_hashes::blake2b_512_address_payload` so a P2PKH stays spendable. This **re-genesised** the chain (new premine payload + `utxo_commitment` + all 5 genesis block hashes; `hash_merkle_root`s unchanged since the coinbase is unchanged).
- **Script caps 10_000→16_384 — DONE (S11a).** `MAX_SCRIPTS_SIZE`, the mempool `MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE`, and the 4 `Params` presets' `max_signature_script_len` are now **16_384**. `MAX_SCRIPT_ELEMENT_SIZE` stays 8192; `max_script_public_key_len` stays 10_000.
- **Validator-attestation crash fix (`67dfbb0`).** The DNS-overlay `rewarded_epochs_store` (per-block `(bond, epoch)` reward keys) reused the `tracked_bytes` utxo_diffs cache policy, but its `Vec` value implements only `estimate_mem_units` (not `_bytes`) → the first reward-bearing coinbase panicked the `virtual-processor` (`not implemented` @ `utils/src/mem_size.rs:40`). Fixed to an untracked `Count` policy (+ regression test).

The normative table in *Revision 1.1* below is updated in place to these values.

---

## Revision 1.1 — PQ-only completion (2026-05-31)

The original 1.0 decision was a minimal value/variant swap (ML-DSA-65→87) that *kept* legacy paths. Per the team design doc **`docs/kaspa-pq-design-mldsa87.md`**, the scope is now a full **PQ-only** completion: legacy secp256k1/Schnorr/ECDSA, legacy addresses, and P2SH are made **unrepresentable at the consensus, mempool, and wallet layers** — not merely "ML-DSA added". This revises several 1.0 decisions.

### Locked decisions (this revision)
1. **Address payload = 64-byte BLAKE2b-512** of the ML-DSA-87 verification key (was 32-byte). Needs an `OP_BLAKE2B_512` / `OpData64` opcode. `scriptPubKey = OP_DUP OP_BLAKE2B_512 OP_DATA64 <payload64> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87`.
2. **Premine → single-key ML-DSA-87 P2PKH** (was 2-of-3 multisig P2SH). P2SH/multisig is out of launch scope, so the premine cannot be a P2SH UTXO.
3. **P2SH disabled in launch scope.** Re-enabling ML-DSA multisig is a separate ADR + redeem-script static-analysis class.

### Revised from 1.0
- **Naming:** full rename to `*87*` including the tx context string `b"kaspa-pq-v1/tx/mldsa87"`. The 1.0 "keep `MlDsa65` identifiers" minimal-churn policy is **withdrawn**.
- **Script caps:** P2PKH-only → `MAX_SCRIPT_ELEMENT_SIZE = 8192`, `MAX_SCRIPTS_SIZE = 10_000`, `max_signature_script_len = 10_000`. (Revises the 1.0 multisig-sized 16384/32768.)

### Net-new consensus/wallet work (8 phases; design doc §17, §22 order)
All eight phases have landed on branch `pr-19-…` (not yet pushed to `main`):

- **P1 (done):** `PqEnforcementMode {Disabled, PolicyOnly, Consensus}` + `pq_activation_daa_score` + `Params::is_pq_active(daa)` in `params.rs`; all kaspa-pq nets = `Consensus` @ genesis. (Inert until consumed in P2.)
- **P2 (done):** `ScriptPolicy` in `TxScriptEngine`; legacy signature opcodes → consensus error; P2SH disabled; PQ-only script-class reject at mempool **and** consensus (output + input).
- **P3 (done):** `calc_mldsa87_signature_hash` → `Hash64` + `Mldsa87SigHashReusedValues`; context `mldsa87`; ML-DSA verify/sign in lockstep on the 64-byte digest.
- **P4 (done):** 64-byte address + premine P2PKH + caps; genesis regenerated once.
- **P5 (done):** wallet/API PQ-only (legacy address reject, PQ change address); native ML-DSA wallet-core.
- **P6 (done):** `utxo_commitment` → `Hash64`.
- **P7 (done):** mass/DoS calibration (`mass_per_sig_op` = 10000 from the measured ML-DSA-87/Schnorr verify ratio).
- **P8 (secp256k1 feature-gate + SigCache PQ key + docs done — S8a `b278817`, S8b `a1d93f5`, S8c):**
  `secp256k1` is now an **optional** dependency of `kaspa-txscript(-errors)`, `kaspa-consensus-core`, and `kaspa-consensus`, gated behind a `legacy-secp256k1` feature (default = `pq-only`). The SigCache key is secp-free — `SigAlg {MlDsa87, #[cfg(legacy)] Schnorr/Ecdsa}` + three `[u8; 64]` BLAKE2b digests (design §10), dropping the S3b `secp256k1::Message` fold. `cargo tree -p kaspa-consensus -e normal` now links **zero** secp256k1, enforced as a hard gate by `scripts/ci/pq-ci-guard.sh` (`HARD_SECP_GATE=1` default), wired into the CI `lints` job. **S9 extension (RPC/SDK → node binary):** secp256k1 is also gated out of the **kaspad node binary** — the RPC/SDK path (`kaspa-rpc-core → kaspa-consensus-wasm → kaspa-consensus-client`) now makes secp optional and gates its legacy schnorr signer (`sign_with_multiple_v3`) + error variants behind `legacy-secp256k1`. consensus-client's `wasm32-sdk` profile pulls `legacy-secp256k1` in (the WASM SDK still ships the legacy signer), but the native node never enables `wasm32-sdk`, so `cargo tree -p kaspad -e normal` links **zero** secp256k1. The CI guard now asserts **both** `kaspa-consensus` and `kaspad` are secp-free. **Deferred (non-blocking):** the cosmetic `MlDsa65`→`87` identifier rename. (The wallet-stack secp256k1 gating deferred here is now **done** — see P10.)
- **P10 (wallet-stack secp256k1 feature-gate — audit QL-1):** the `legacy-secp256k1` fence now extends through the entire wallet stack. `secp256k1` is an **optional** dependency of `kaspa-bip32`, `kaspa-wallet-keys`, `kaspa-wallet-pskt`, and `kaspa-wallet-core` (all `default = ["pq-only"]`). The classical key infrastructure — BIP32 extended-key (xprv/xpub) derivation, the secp `PrivateKey`/`PublicKey`/`Keypair`/`XPrv`/`XPub` types, the classical account variants (`bip32`/`multisig`/`legacy`/`watchonly`/`bip32watch`/`keypair`/`resident`), HD + deterministic address derivation, secp message signing, and the whole PSKT/PSKB crate — is gated behind `legacy-secp256k1`, leaving only the ML-DSA-87 (`mldsa`) account, BIP39, and the shared wallet/UTXO/storage infra in the PQ-only build. The single-key ML-DSA account reuses the existing address-*set* UTXO scan path (not the HD `AddressManager`), and the generator's `Signer` routes through the native ML-DSA path (`try_pq_keypair` → `sign_transaction_inputs_mldsa87`); the secp key-map signing path is gated. `kaspa-cli`/`kaspa-daemon`/`kaspa-wallet` default to pq-only and opt into the classical wallet via `legacy-secp256k1`; the WASM SDK keeps the classical signer via `wasm32-sdk → legacy-secp256k1` (the consensus-client precedent). **Net result:** `cargo tree -e normal -i secp256k1` links **zero** secp256k1 for **every** production binary — `kaspad`, `kaspa-pq-cli`, `kaspa-wallet`, `kaspa-cli`, `kaspa-daemon`, `misaminer`, `kaspa-pq-miner`, `kaspa-pq-validator` — with only the `simpa`/`rothschild` sim/load-test tools opting back in. The classical wallet (`--features legacy-secp256k1`) still builds and passes its full suite (wallet-core 49/0/3, wallet-keys 21/0/7, bip32 6/0, pskt 5/0); the PQ-only build is warning-free and passes its subset (wallet-core 30/0, wallet-keys 18/0, bip32 5/0).

### Security note (libcrux advisories)
Local `advisory-db` lists `RUSTSEC-2026-0076` / `-0077` for `libcrux-ml-dsa`, both `patched >= 0.0.8`; we pin `=0.0.9` → **clear**. The design doc's cited `RUSTSEC-2026-0125/0126` are not present in the local advisory-db snapshot. `cargo audit` / `cargo deny check advisories` are wired into the P1 CI guard (`scripts/ci/pq-ci-guard.sh`); as of P8 the secp256k1-tree check is a **hard** gate (`HARD_SECP_GATE=1` default), run by the CI `lints` job.

### Normative launch-scope values (design doc §16.2)
| Item | Value |
|---|---|
| Signature | ML-DSA-87 (FIPS 204, NIST cat 5) |
| Tx signature context | `kaspa-pq-v2/tx/mldsa87` |
| Address version | `PubKeyHashMlDsa87` only |
| Address payload | keyed BLAKE2b-512 (`kaspa-pq-v2/address/mldsa87`), 64 bytes |
| Standard script | ML-DSA-87 P2PKH only |
| P2SH | disabled |
| Legacy secp256k1 opcodes | consensus-disabled |
| ML-DSA sighash | `calc_mldsa87_signature_hash`, 64 bytes (domain `kaspa-pq-v2/sighash/mldsa87`) |
| UTXO commitment | `Hash64` (64 bytes) |
| `MAX_SCRIPT_ELEMENT_SIZE` | 8192 |
| `MAX_SCRIPTS_SIZE` / `max_signature_script_len` | 16_384 |
| `mass_per_sig_op` | 10000 |

The 1.0 body below is retained for history; where it conflicts with this revision (naming, caps, P2SH/premine, address width), **this revision governs**.

---

## Context

Kaspa-pq adopted **ML-DSA-65** (FIPS 204, NIST security category 3) as its sole signature scheme (ADR-0002). Category 3 targets roughly AES-192-equivalent post-quantum security.

We migrate to **ML-DSA-87** (FIPS 204, NIST security category 5) — the highest ML-DSA parameter set, targeting roughly AES-256-equivalent security — to maximize the long-term quantum-resistance margin of the chain before mainnet and before significant value or tooling accretes on the category-3 parameters.

Size impact:

| | ML-DSA-65 (cat 3) | ML-DSA-87 (cat 5) | Δ |
|---|---|---|---|
| Public key | 1952 B | 2592 B | +640 |
| Signature | 3309 B | 4627 B | +1318 |
| Secret key | 4032 B | 4896 B | (derived from a 32-byte seed; never stored) |

The address layer is unaffected in **format** (addresses store a 32-byte BLAKE2b-256 hash of the pubkey — ADR-0003), but every address **value** changes because the hashed pubkey is different.

There is no central switch for the ML-DSA parameter set: the variant is selected by `use libcrux_ml_dsa::ml_dsa_65` repeated across ~9 crates, and the byte sizes are independently hardcoded as named constants in three crates (txscript, consensus-core, rpc-core). This is therefore a **scattered migration**, not a one-line config change.

## Decision

### 1. Switch the ML-DSA parameter set to 87
Change every `libcrux_ml_dsa::ml_dsa_65` import/call site to `ml_dsa_87`: `crypto/txscript` (`lib.rs`, `standard/multisig.rs`, `benches/bench.rs`), `wallet/keys` (`kaspa_pq.rs`, `kaspa_pq_wasm.rs`), `kaspa-pq-validator-core`, `consensus` virtual-processor tests, `wallet/pq-cli`, `rpc/wrpc/examples/kaspa_pq_send`.

### 2. Update size constants (three independent copies)
- `crypto/txscript/src/lib.rs`: `MLDSA65_PK_LEN` 1952→**2592**, `MLDSA65_SIG_LEN` 3309→**4627**
- `consensus/core/src/dns_finality.rs`: `STAKE_VALIDATOR_PUBKEY_LEN` 1952→**2592**, `STAKE_ATTESTATION_SIG_LEN` 3309→**4627**
- `rpc/core/src/model/kaspa_pq.rs`: `RPC_MLDSA65_PK_LEN` 1952→**2592**, `RPC_MLDSA65_SIG_LEN` 3309→**4627**

Existing parity asserts (rpc↔txscript, dns_finality↔txscript) cross-check these and will catch drift at test time. Bare literals (test fixtures, WASM length-check error strings, derivation comments) are updated to match.

### 3. Raise size-derived caps and recalibrate mass
- Raise `MAX_SCRIPT_ELEMENT_SIZE` / `MAX_SCRIPTS_SIZE` (txscript) and `MAXIMUM_STANDARD_SIGNATURE_SCRIPT_SIZE` (mining) to admit the larger 87 pushes. A 2-of-3 ML-DSA-87 multisig redeem script (~7.8 KB) and its unlock script (~16.5 KB) exceed the current 16 384-byte caps.
- Recalibrate `mass_per_sig_op` (`consensus/core/src/config/params.rs`, currently 6000, calibrated to ML-DSA-65 verify cost). ML-DSA-87 verification is slower; re-benchmark and raise so sigops mass still reflects real verify cost.

### 4. New genesis (forced)
The 15 B premine is locked to a 2-of-3 ML-DSA multisig whose redeem-script hash is baked into `MISAKA_PREMINE_P2SH_SCRIPT`, which feeds the genesis `utxo_commitment` and thence all genesis block hashes. ML-DSA-87 keys produce a different redeem script, so we regenerate, in order:
1. multisig keys + address (`gen_misaka_devnet_multisig`) → `misaka-devnet-multisig-keys.json`
2. the premine P2SH constant `MISAKA_PREMINE_P2SH_SCRIPT` (`consensus/core/src/config/premine.rs`)
3. the premine commitment (`print_premine_commitment`)
4. all genesis block hashes (`gen_kaspa_pq_genesis_hashes`, `consensus/core/src/config/genesis.rs`)

### 5. Naming policy (minimal-churn for this pass)
Identifiers keep their existing `MlDsa65` / `MLDSA65_*` spelling for this migration; only **values and behavior** become ML-DSA-87. Each changed size constant carries a clarifying comment. A full identifier rename to `*MlDsa87*` (key types, the WASM `signTransactionMlDsa65` binding, opcode idents, the address-version variant name) is a tracked **cosmetic follow-up**, sequenced separately to keep the behavioral migration reviewable and low-risk.

Unchanged on purpose (variant-agnostic / opaque): opcode byte tags `0xa6` (`OP_CHECKSIGMLDSA65`) and `0xa7` (`OP_CHECKMULTISIGMLDSA65`), the address version byte (2), the 32-byte validator/wallet seed length, and the signing domain-separation context strings (`b"kaspa-pq-v1/tx/mldsa65"`, `.../att/mldsa65`, `.../takeover/mldsa65`) — kept identical so signer and verifier stay in lock-step.

## Consequences

### Positive
- Maximum NIST category-5 post-quantum security margin for the chain.

### Negative / operational
- **New chain.** No compatibility with any existing kaspa-pq genesis, UTXO set, or address. A full from-genesis redeploy of all devnet hosts plus a WASM wallet rebuild is required.
- All keypairs/addresses are functionally new (format identical, values different).
- External consumers (misakascan, misaka-api, wallet.misakascan) must regenerate addresses and update the WASM SDK.
- Larger transactions and higher mass for every PQ spend (+640 B pubkey, +1318 B signature revealed at spend time).
- `MlDsa65`-spelled identifiers temporarily denote ML-DSA-87 until the rename follow-up lands.

## Alternatives Considered
- **Stay on ML-DSA-65.** Rejected per the migration directive; category 5 chosen for long-term margin while the ecosystem is still small.
- **Full identifier rename in this same pass.** Deferred: high churn across 9 crates and it would break the external WASM JS API name mid-migration. Sequenced as a separate cosmetic pass.
- **Parameterize the variant behind a feature/const (single switch).** Rejected for now: a larger refactor than the migration itself; the scattered `ml_dsa_NN` imports plus per-crate size constants would all need to route through one module. Revisit if a third parameter set is ever contemplated.

## References
- FIPS 204 (ML-DSA)
- Supersedes [ADR-0002](0002-mldsa65-p2pkh.md); forces new genesis per `ADR-0008 (pq-genesis-premine)` — **not resident**; see the note below; address format unchanged per `ADR-0003 (pq-address-format)` — **not resident**; see the note below; mass policy per [ADR-0005](0005-mass-policy.md)
- `libcrux-ml-dsa` crate (`ml_dsa_87` submodule)
~~~~~~~~
<!-- ADR_SOURCE_END:0019-mldsa87-migration.md -->

</details>

