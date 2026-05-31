# kaspa-pq 量子耐性完了設計書

- 文書版: 1.1（ML-DSA-87 版）。**v2 整合追補 (2026-06-01)**: 署名 context は `kaspa-pq-v2/{tx,sighash}/mldsa87`、address payload は keyed BLAKE2b-512(`kaspa-pq-v2/address/mldsa87`)、caps は 16_384、識別子は `MlDsa87` に rename 済み。本書の該当値は更新済（経緯は ADR-0019 Revision 1.2）。
- 作成日: 2026-05-30 / リポジトリ凍結: 2026-05-31
- 統治 ADR: [docs/adr/0019-mldsa87-migration.md](adr/0019-mldsa87-migration.md)（本書を採用）
- 目的: 既存コードを「ML-DSAを一部導入した実験実装」から、「PQネットワークとして量子耐性を主張できる実装」へ変更するための設計を定義する。

## 1. 結論

現状の実装は PQ 署名検証経路を一部持っているが、secp256k1/Schnorr/ECDSA のトランザクション経路、legacy アドレス生成、P2SH 経由の legacy script、32-byte Schnorr sighash 流用が残っている。本設計では署名方式を ML-DSA-87 に統一する。このため、PQ 化の中心は「ML-DSAを追加する」ことではなく、「PQ ネットワークでは非 PQ 経路を consensus・mempool・wallet/API の全層で実行不能にする」ことである。

本設計では、kaspa-pq を mainline Kaspa 互換ネットワークではなく、genesis から PQ-only の独立ネットワークとして扱う。

## 2. PQ 化の達成条件

### 2.1 必須条件
1. トランザクション認証は `ML-DSA-87` のみを許可する。
2. `OP_CHECKSIG`, `OP_CHECKSIGECDSA`, legacy `OP_CHECKMULTISIG` など secp256k1 依存の署名 opcode は PQ mode で consensus 失敗にする。
3. `PubKey`, `PubKeyECDSA`, `ScriptHash` は parser-only とし、PQ mode の標準送金・block 内 output・mempool input では拒否する。
4. wallet/API は PQ network 上で legacy address を生成・送金・change address として使用できない。
5. ML-DSA 署名対象は `calc_schnorr_signature_hash` ではなく domain-separated な `calc_mldsa87_signature_hash` にする。
6. 仕様書・ADR・実装定数を一致させる（address payload 幅、script size、UTXO commitment 幅、P2SH 可否）。
7. transport confidentiality まで PQ 主張する場合は ML-KEM hybrid 鍵交換を導入。しないなら PQ claim 対象外と明記。

### 2.2 非目標
- mainline Kaspa とのアドレス/wallet/RPC/P2P 互換性維持。
- secp256k1 UTXO の移行。
- legacy wallet import path を PQ-grade と主張。
- ML-DSA multisig/P2SH を launch scope に含める（必要なら別 ADR + hard fork）。

## 3. 標準・暗号選定

### 3.1 署名
- 方式: ML-DSA-87 / FIPS 204 / `libcrux-ml-dsa` / 最小 `0.0.9`（consensus は exact pin + lockfile audit）
- 公開鍵 2592 / 秘密鍵 4896 / 署名 4627 bytes。signature script 上の署名要素 = `signature || sighash_type` = 4628 bytes
- Context: `b"kaspa-pq-v2/tx/mldsa87"` / NIST Category 5

### 3.2 鍵確立・通信
- transport を PQ claim に含めるなら ML-KEM-768 以上を hybrid KEM。含めないなら「transport は PQ scope 外」と明記。

### 3.3 Hash/commitment
- consensus identity と署名 transcript は 64-byte BLAKE2b-512 domain。32-byte hash は非 consensus（debug/legacy parser/cache fingerprint）限定。

## 4. 全体アーキテクチャ

```
Wallet/API   → PQ key derivation / ML-DSA signing / PQ address only
Tx generator → pay_to_address_script_pq / calc_mldsa87_signature_hash
Mempool      → PQ standard class only / legacy reject
Consensus    → output class reject / input class reject / PQ script policy
TxScriptEngine → legacy signature opcodes disabled / ML-DSA verify only
```
重要判断: PQ 制約は mempool ではなく **consensus validation と script engine** に置く（miner の block 直投入・P2SH redeem 経由の legacy opcode を防ぐため）。

## 5. 設計 A: PQ enforcement mode（`consensus/core/src/config/params.rs`）

```rust
pub enum PqEnforcementMode { Disabled, PolicyOnly, Consensus }
// Params { pq_enforcement, pq_activation_daa_score, .. }
impl Params {
    pub fn is_pq_active(&self, daa_score: u64) -> bool {
        self.pq_enforcement == PqEnforcementMode::Consensus && daa_score >= self.pq_activation_daa_score
    }
}
```
初期値: kaspa-pq 全ネット = `Consensus`, activation = genesis(0)。upstream-compat tests = `Disabled`。`PolicyOnly` は移行テスト専用。

## 6. 設計 B: legacy signature opcode の consensus 無効化

対象: `crypto/txscript/src/opcodes/mod.rs`, `crypto/txscript/src/lib.rs`, `consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs`

無効化対象（PQ mode で consensus error）: `OP_CHECKSIG`/`VERIFY`, `OP_CHECKSIGECDSA`, `OP_CHECKMULTISIG`/`VERIFY`, `OP_CHECKMULTISIGECDSA`, `ScriptHash` spend。

```rust
pub struct ScriptPolicy { pub pq_only: bool, pub allow_p2sh: bool }
impl ScriptPolicy {
    pub const PQ_ONLY: Self = Self { pq_only: true, allow_p2sh: false };
    pub const LEGACY: Self = Self { pq_only: false, allow_p2sh: true };
}
```
`TxScriptEngine::from_transaction_input` は `policy: ScriptPolicy` を受ける。実行時、legacy 署名 opcode tag (0xa9/0xab/0xac/0xad/0xae/0xaf) を `pq_only` で `LegacySignatureOpcodeDisabled` に。P2SH 分岐前に `pq_only && !allow_p2sh` で `ScriptHashDisabledInPqMode`。

ML-DSA multisig を後で有効化する場合も P2SH 全体を解禁せず、redeem を静的解析して data push + `OP_CHECKMULTISIG_MLDSA87` のみ許可する専用 class を追加する。

## 7. 設計 C: 標準 script class と block output 制限

対象: `script_class.rs`, `standard.rs`, `check_transaction_standard.rs`, `tx_validation_in_isolation.rs`, `tx_validation_in_utxo_context.rs`

`ScriptClass::from_script` は legacy を認識してよいが、policy 判定を分離:
```rust
impl ScriptClass {
    pub fn is_pq_standard_send(&self) -> bool { matches!(self, ScriptClass::PubKeyHashMlDsa87) }
    pub fn is_pq_consensus_allowed_output(&self) -> bool { matches!(self, ScriptClass::PubKeyHashMlDsa87) }
    pub fn is_pq_consensus_allowed_input(&self) -> bool { matches!(self, ScriptClass::PubKeyHashMlDsa87) }
}
```
- Mempool: output/referenced-UTXO class が `PubKeyHashMlDsa87` 以外なら reject。
- Consensus: `tx_validation_in_isolation.rs` で PQ active 時 output class を、`tx_validation_in_utxo_context.rs` で input(referenced UTXO) class を reject してから script 実行。
→ miner が mempool を迂回しても legacy output/spend は無効。

## 8. 設計 D: address payload 幅の確定（**決定: 64-byte**）

```
address payload = keyed BLAKE2b-512("kaspa-pq-v2/address/mldsa87", ML-DSA-87 verification key)   // 64 bytes
scriptPubKey = OP_DUP OP_BLAKE2B_512 OP_DATA64 <payload64> OP_EQUALVERIFY OP_CHECKSIG_MLDSA87
```
実装: `crypto/addresses/src/lib.rs`（`Version::PubKeyHashMlDsa87.public_key_len()` = 64）、`standard.rs`（`pay_to_pub_key_hash_mldsa87` 64B）、`script_class.rs`（`OpData64`）、`wallet/keys/src/kaspa_pq.rs`（`public_key_hash()` を BLAKE2b-512）、各 fixture。`OP_BLAKE2B_512` が未実装なら追加（既存 `OpBlake2b` が 32B 固定のため）。

## 9. 設計 E: ML-DSA 専用 sighash

問題: 現状 ML-DSA は `calc_schnorr_signature_hash`(32B) を署名対象 → scheme/width 双方で弱い。

```rust
pub fn calc_mldsa87_signature_hash(
    verifiable_tx: &impl VerifiableTransaction, input_index: usize,
    hash_type: SigHashType, reused_values: &impl Mldsa87SigHashReusedValues,
) -> Hash64
```
Transcript は Schnorr と同じ semantic fields + `literal domain tag "kaspa-pq-v2/sighash/mldsa87"` + script class tag `PubKeyHashMlDsa87`、全 hash を Hash64 化。`Mldsa87SigHashReusedValues`（previous_outputs/sequences/sig_op_counts/outputs/payload の Hash64 版）を追加。検証側 `check_mldsa87_signature` は `calc_mldsa87_signature_hash` の 64B を `ml_dsa_87::verify(..., MLDSA87_TX_CONTEXT, ..)` に渡し、`secp256k1::Message` を ML-DSA path から除去。

## 10. 設計 F: SigCache key の PQ 化

```rust
pub enum SigAlg { MlDsa87, #[cfg(feature="legacy-secp256k1")] Schnorr, #[cfg(feature="legacy-secp256k1")] Ecdsa }
pub struct SigCacheKey { sig_alg: SigAlg, pub_key_digest: [u8;64], signature_digest: [u8;64], message_digest: [u8;64] }
```
raw key/sig を保持しない（BLAKE2b-512 digest）。capacity は ML-DSA verify cost と block mass で再評価。

## 11. 設計 G: script size と mass policy（**launch = P2PKH only**）

- `MAX_SCRIPT_ELEMENT_SIZE = 8192`（sig 4628 + pk 2592 が収まる）
- `MAX_SCRIPTS_SIZE = 16_384` / `max_signature_script_len = 16_384`（P2PKH unlock ≈ 7.3KB; md2 で 10_000 から引き上げ。`max_script_public_key_len` は 10_000 維持）
- `P2SH = disabled`

ML-DSA multisig を標準化する場合のみ caps を k-of-n で再計算（2-of-3 は 16,384 超の可能性、暫定下限 32_768、実測で確定）+ redeem parser + mass benchmark + ADR。**現時点では scope 外。**

Dust: upstream 148B 想定でなく ML-DSA P2PKH spend size（≈7.3KB）。`mass::estimate_pq_p2pkh_spend_size()` を関数化し dust/fee 見積で共有。

## 12. 設計 H: UTXO commitment 64-byte 化

`utxo_commitment` を `kaspa_hashes::Hash64` に統一（header.rs / hashing/header.rs / utxo_commitment.rs / genesis / DB serialization / RPC・WASM DTO / fixtures）。header preimage writer は 64B。hard fork（新規 PQ network は genesis から）。

## 13. 設計 I: wallet/API の PQ-only 化

`PqKeypair`（ML-DSA-87）に `to_address(network)` / `sign_transaction_input(...)`。PQ network で `Keypair::toAddress(ECDSA)` / `PrivateKey::toAddress(ECDSA)` / secp256k1 BIP32 由来 address を error。WASM は関数名を残し PQ prefix なら `Err("legacy secp256k1 address is disabled on kaspa-pq")`。change address は必ず `PubKeyHashMlDsa87`。tx generator は `pay_to_address_script_pq`（送金先・change 両方を check）。

## 14. 設計 J: dependency と build feature

```toml
[features]
default = ["pq-only"]
pq-only = []
legacy-secp256k1 = ["dep:secp256k1"]
```
legacy opcode 実装 / legacy wallet API / legacy tests は `legacy-secp256k1` のみ。release/node binary では無効。CI: `cargo deny check advisories`, `cargo audit`, `cargo tree -p kaspa-consensus | grep secp256k1 && exit 1`。`libcrux-ml-dsa >= 0.0.9` 必須。

**実装済 (Phase 8 — S8a `b278817` / S8b `a1d93f5` / S8c):** `secp256k1` は `kaspa-txscript(-errors)` / `kaspa-consensus-core` / `kaspa-consensus` で `optional = true` 化し `legacy-secp256k1` feature に gate（default = `pq-only`）。`cargo tree -p kaspa-consensus -e normal` は secp256k1 ゼロ（`scripts/pq-ci-guard.sh` の `HARD_SECP_GATE=1` で hard gate、CI `lints` job に組込済）。SigCache key は secp-free（`SigAlg {MlDsa87, #[cfg(legacy)] Schnorr/Ecdsa}` + 3×`[u8;64]` BLAKE2b digest, §10）。legacy sign helper を呼ぶ wallet-core / simpa / rothschild / testing-integration は `kaspa-consensus-core/legacy-secp256k1` を有効化（いずれも kaspad の依存外）。**S9 で RPC/SDK 層も secp 除去**: `rpc-core → consensus-wasm → consensus-client` の secp を optional 化し legacy signer/error を `legacy-secp256k1` に gate（consensus-client の `wasm32-sdk` profile が legacy を引き込むので WASM SDK は従来通り、native node は `wasm32-sdk` 非有効で secp ゼロ）。`cargo tree -p kaspad -e normal` も secp ゼロ、guard は `kaspa-consensus` と `kaspad` の両方を検査。残（非ブロッキング）: cosmetic な `MlDsa65`→`87` rename と wallet/WASM-SDK crate 群（`wallet-keys`/`bip32`/`wallet-core`、wasm32-sdk build）の secp（node/consensus 経路外）。

## 15. 設計 K: transport PQ scope

初回は **transport を PQ claim に含めない**（README/spec に「PQC claim は tx authorization と consensus identity に限定」）。ML-KEM hybrid は別 ADR。

## 16. 仕様書・ADR と固定値

修正対象: `docs/kaspa-pq-spec.md`, `docs/adr/0002-mldsa87-p2pkh.md`(rename), `0005-mass-policy.md`, `0008-hash64-consensus-identity.md`, README。

| 項目 | 値 |
|---|---|
| Signature | ML-DSA-87 |
| Tx signature context | `kaspa-pq-v2/tx/mldsa87` |
| Address version | `PubKeyHashMlDsa87` only |
| Address payload | keyed BLAKE2b-512 (`kaspa-pq-v2/address/mldsa87`) 64 bytes |
| Standard script | ML-DSA P2PKH only |
| P2SH | disabled |
| Legacy secp256k1 opcode | consensus disabled |
| ML-DSA sighash | `calc_mldsa87_signature_hash` 64 bytes |
| UTXO commitment | Hash64 |
| MAX_SCRIPT_ELEMENT_SIZE | 8192 |
| MAX_SCRIPTS_SIZE / max_signature_script_len | 10,000 |

許可表現: 「tx authorization uses ML-DSA-87」「secp256k1 signing disabled in PQ consensus mode」「64-byte BLAKE2b-512 consensus identity」「transport は ML-KEM hybrid 有効時以外 PQ claim 外」。
禁止表現: 「all cryptography is post-quantum」「256-bit PQ security across the board」「legacy Kaspa addresses are quantum-resistant」「P2SH scripts are PQ-safe（redeem 制限なし）」。

## 17. 実装フェーズ
- P1: Spec freeze + CI guard（`PqEnforcementMode`、advisory/secp256k1 tree check）
- P2: Consensus PQ policy（output/input class reject、`ScriptPolicy`、legacy opcode error、P2SH disable）
- P3: ML-DSA sighash64（`Mldsa87SigHashReusedValues`、`calc_mldsa87_signature_hash`、検証/wallet 切替、negative test）
- P4: address/script payload 固定（64B、`OP_BLAKE2B_512`、premine P2PKH、caps、genesis 再生成）
- P5: wallet/API PQ-only
- P6: UTXO commitment64
- P7: Mass/DoS calibration
- P8 (done): Release hardening — `secp256k1` を `legacy-secp256k1` feature に gate（release/consensus tree は secp ゼロ、`cargo tree -p kaspa-consensus -e normal` で確認）、CI hard gate（`scripts/pq-ci-guard.sh`）、docs。残: cosmetic rename と RPC/SDK 層 secp（consensus 検証外）

## 18. 受け入れ基準（要約）
- Consensus: PQ active で legacy output/input/opcode/P2SH を含む block・tx が invalid、ML-DSA P2PKH は valid。
- Mempool: `PubKey`/`PubKeyECDSA`/`ScriptHash` output reject、referenced UTXO 非 PQ class reject、dust は ML-DSA spend size。
- Wallet/API: PQ net で `toAddress(ECDSA)` が legacy を返さない、legacy 宛て tx 生成失敗、change は `PubKeyHashMlDsa87`、signing は `calc_mldsa87_signature_hash`。
- Dependency/CI: release consensus path に secp256k1 が無い、libcrux advisory clear、cargo audit/deny green。
- Docs: address 幅・script limit が spec/ADR/code 一致、transport PQ claim 範囲明記。

## 19. テスト設計（要約）
- Unit: `is_pq_standard_send()` は `PubKeyHashMlDsa87` のみ true、legacy opcode は `PQ_ONLY` で `LegacySignatureOpcodeDisabled`、`calc_mldsa87_signature_hash` 固定 vector、context binding、長さ pre-check が libcrux 前に失敗。
- Integration: PQ wallet create→sign→submit→mine→spend、legacy Schnorr は mempool+consensus reject、P2SH redeem 内 legacy opcode reject、UTXO commitment 64B roundtrip、release feature に secp256k1 不在。
- Negative: 32B Schnorr sighash の ML-DSA 署名は検証失敗、address payload 長不正は decode/script 作成で失敗、oversized signature script は params/txscript 両方で拒否。

## 20. 主な変更対象（要約）
params.rs(PqEnforcementMode) / txscript lib.rs(ScriptPolicy, legacy reject, sighash64) / opcodes(mod.rs) / script_class.rs / standard.rs / sighash.rs(calc_mldsa87_signature_hash) / check_transaction_standard.rs / transaction_validator/* / addresses lib.rs / wallet keys・tx / header・UTXO / Cargo+CI。

## 21. 残リスク
1. ML-DSA は大きく、mass 不足だと DoS。benchmark 必須。
2. P2SH を安易に残すと redeem 経由で legacy opcode 復活。launch 禁止。
3. transport を PQ scope 外にする場合も operator 文書に明記。
4. 配布済み 32-byte PQ address があれば 64-byte 切替は migration 問題（未 launch なら即変更）。
5. dependency PQ 安全性は固定でない。libcrux advisory を CI 継続監視。

## 22. 最小パッチ順序
1. consensus と script engine で legacy signature opcode 無効化
2. mempool と wallet で legacy address 拒否
3. ML-DSA sighash64 実装
4. address/script/UTXO commitment の幅を固定
5. mass/DoS と dependency CI
6. README と public claim 修正

最初の 2 steps で「secp256k1 が使えるため PQ ではない」最大リスクを閉じられる。

## 23. 参考資料
- NIST FIPS 203/204/205
- RustSec RUSTSEC-2026-0125 / -0126（libcrux-ml-dsa AVX2）※ローカル advisory-db には 0076/0077（patched ≥0.0.8）が存在。0.0.9 は clear。
