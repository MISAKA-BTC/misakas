# MISAKA PQルート型EVM資産管理 設計書 v1.1

**文書種別:** セキュリティ・コンセンサス・ウォレット統合設計
**対象コード:** `MISAKA-BTC/misakas` 公開 `main`（snapshot `2e4e04a378c2ddfca5a2428d99cbb4218f280969` 基準）＋ローカル監査remediation差分
**作成日:** 2026-06-23（v1.0）／改訂 2026-06-23（v1.1）
**状態:** Proposed — testnetでの実装・監査・activationが必要
**短縮名:** **PQ-Rooted EVM Account (PREA)**

---

## 0. 結論

採用すべき構成は、次の三層です。

```text
ML-DSA-87 UTXO Vault       長期保管・残高の主たる置き場所
        │
        │ 必要額だけ排他的にlock/claim
        ▼
PQ-Rooted EVM Smart Account  EVM資産・NFT・作業残高
        │
        ├─ ML-DSA Operational Root   無制限権限、PQ
        └─ secp256k1 Session Key     期限・金額・用途制限付き
```

重要な判断は次の4点です。

1. **UTXO残高とEVM残高をミラーしない。** 同じ価値を両側で同時に使用可能にする設計は採用しない。
2. **現行deposit-lock / claim / refund / F002 withdrawの排他的な価値移動を維持する。** 二重使用防止の基礎はすでに妥当である。
3. **EVM側の所有権をsecp256k1 EOAからPQルート型スマートアカウントへ移す。** 無制限権限はML-DSA-87だけが持つ。
4. **secp256k1は通常操作用のSession Keyとして残すが、権限を制限する。** 通常のECDSAトランザクションからPQアカウント残高を直接使用する経路はconsensusで禁止する（**ただし block-invalidating error ではなく決定的 class-2 skip。§8.4 参照**）。

この方式では、量子攻撃でSession Keyが破られても、被害はgrantに設定した期間・金額・コントラクト・function selectorの範囲に限定されます。NFT、長期資産、全額withdraw、root変更、session追加、account移行はML-DSA署名を必須とします。

---

## 0.1 v1.1 変更点（v1.0 → v1.1）

外部レビュー（10エージェントの fact-check + 敵対的健全性レビュー）とユーザー指摘を反映。コンセンサス健全性に関わる修正が中心で、設計の三層構造・不変条件の骨子は不変。

**現状確認の精緻化（§1）**

- 公開 `main` では **F001 は precompile ではなく通常の WMISAKA predeploy**、**F002 は executor へ登録された withdraw handler**。**F003 / F004 は未実装**。（`consensus/core/src/evm/mod.rs`, `kaspa-evm/src/withdraw.rs`）
- 現行で唯一の precompile-style runtime handler は **F002 のみ**。deposit は precompile ではなく **`EvmSystemOp::DepositClaim` という system op**（executor が user tx より先に適用）。
- signer の **`purpose ↔ message_digest variant` 検証は公開 `main` で実装済み**（`kaspa-pq-signer/src/lib.rs`、監査 C-02 相当）。設計はこの domain 分離規律を EVM 側にも適用する。
- **`MAX_WITHDRAWALS_PER_EVM_BLOCK` は公開 `main` にも添付 ZIP にも存在しない**。ローカル実装済みでも「完了」とは表記せず、§31 のトライステート（後述）で `[~]` とする。

**必須修正8点（本文へ反映）**

1. **Direct ECDSA 拒否は block-invalidating error にしない（§8.4 / §24 / I-6 / §28.3）。** `return Err(...)` ではなく決定的 **class-2 skip**。registry 参照は **per-tx 実行時・in-execution CacheDB・recovered sender のみ** に固定。mempool 拒否は最適化でありコンセンサス境界ではない。
2. **Registration と deposit claim を同一 block で成立させない（§11.2 / §11.3 / §25.1）。** executor は claim を user tx より先に処理するため、同一 block の registration tx が作る F004 record をその block の claim から参照できない。strict claim が参照できる registry record は **`selected_parent(B)` の committed EVM state にあるもの** に固定。
3. **F003 ABI を version 化する（§9）。** FSL（generic Hash64 検証）と PREA（key-bound root authorization）が同じ `0xF003` を共有するため、`version 0x01` / `0x02` で明示分岐し、encoding・context ID・不正length戻り値・gas charging開始点・最大input bytes・最大verify回数・activation DAA・output ABI を凍結。任意 caller-context ではなく **consensus 側で context ID を固定文字列へ対応**。
4. **EIP-712 hash の 64-byte 拡張を PQ 強度と表現しない（§9.3）。** `op_hash` が一度 32-byte へ圧縮されるため 64-byte 化しても Keccak-256 以上の commitment 強度にはならない。ML-DSA root 署名対象は **canonical operation bytes から直接導く domain-separated Hash64**（option B）を用い、EIP-712 は表示・ECDSA 互換用に分離。
5. **Session policy は非 native 資産を数量制限する（§14）。** `maxNativeValue` だけでは ERC-20 / NFT を全額移動可能。`Erc20Limit` / `Erc721Limit` / `Erc1155Limit` を規範化。
6. **Approval と router は原則 deny-by-default（§14.2 / §14.4）。** `setApprovalForAll` / `approve(max)` / Permit2 unlimited / `DELEGATECALL` / arbitrary multicall / upgradeable aggregator / unknown calldata を generic session から常時禁止。許可は calldata を完全 decode する router 別 policy module に限定。proxy は code-hash まで確認。
7. **ERC-1271 は purpose 自己申告を信用しない（§15）。** account が claimed-purpose の typed payload から `expected_hash` を再計算し、1271 の hash 引数と一致照合。未知 schema・任意 `Custom`・生 32-byte hash への session 署名は default 拒否。
8. **Direct ECDSA ban の目的を明文化する（§8.4 / §34）。** CREATE2 contract address には通常対応する ECDSA 秘密鍵が存在しないため、本 ban は直接の Shor 防御というより、将来の EOA delegation / address preimage・collision / 誤った registration / 将来の auth 方式追加に対する **defense-in-depth**。

**リリースゲートのトライステート化（§31）** `[x] merged and verified` / `[~] implemented locally, not yet independently verified` / `[ ] not implemented`。

**最初の実装スライス（§33）** P0 を **P0-0（ABI 統一・context ID 凍結・gas/resource 数値決定・activation 方針）→ P0-1（F003 handler + 共通 `register_all_misaka_precompiles()` + 全 net inert fence）** に分割。

---

## 1. 現行設計の評価

### 1.1 二重使用防止は基本的に成立している

現行実装の `EVM_DEPOSIT_LOCK`（公開 `main`、`crypto/txscript/src/script_class.rs` の `EvmDepositLockFields`）は、次を保持します。

```text
evm_address
refund開始DAA score（timeout_daa_score）
claim tip
ML-DSA-87 refund script
```

claimとrefundの有効期間は重なりません（`consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs` の refund timeout 規則で確認済み）。

```text
claim valid  : accepting_daa < timeout_daa_score
refund valid : spending_daa  >= timeout_daa_score
```

claimは未使用lock outpointを検査し、同じchain block内でlockをUTXO diffから消し、EVM残高をcreditします（`consensus/src/pipeline/virtual_processor/processor.rs` が claim view と bridge effect を同一 block へ適用、`consensus/src/processes/evm/mod.rs` が claim 検証 + duplicate 防止）。withdrawはF002でEVM残高をdebitし、同じaccepting blockのUTXO diffへsynthetic UTXOを作ります（`kaspa-evm/src/withdraw.rs`）。これら claim / withdraw の side-effect は EVM state 行と**同一 RocksDB WriteBatch**で commit され、I-4 / I-5 の原子性が成立します。

したがって、現行設計で修正すべき中心は「価値移動の排他性」ではなく、**claim後のEVM残高が通常のsecp256k1アカウントに帰属すること**です。

### 1.2 現行の耐量子境界

現在の境界は次です。

```text
UTXO lane : ML-DSA-87、PQ-only
EVM lane  : standard Ethereum account、secp256k1/ECDSA
```

EVMへclaimされたnative balance、ERC-20、NFT、admin/minter権限は、通常のEOAへ置く限り量子安全ではありません。EVM残高を少額・短期間に限定する運用は有効ですが、NFTのように長期間EVMへ残る資産には不十分です。

### 1.3 F003 / F004 の実装状況（公開 `main` 基準）

- **F003（ML-DSA-87 verify precompile, `0x…F003`）は未実装。** `kaspa-evm/src` にも `consensus/core/src/evm/mod.rs` にも登録・address 定数が存在しない（`docs/misaka-fsl-design-v0.3.md` §4.3 に予約 interface 案があるのみ）。PQスマートアカウントの前提として、F003 を consensus 実装し、executor と simulation の双方へ同じ handler を登録する必要がある（§9.5）。
- **F004（PQ Auth Registry predeploy）は未実装。** `is_pq_account` / `PqAuthRegistry` / direct-ECDSA 拒否いずれも grep ヒットなし。
- **`MAX_WITHDRAWALS_PER_EVM_BLOCK` は公開 `main` に未マージ。** ローカル監査 remediation では実装済み（`consensus/core/src/evm/mod.rs:166`、executor 両 gas path で enforce、ただし `evm_f002_withdraw_cap_activation_daa_score = u64::MAX` で全 net inert）だが、公開 `main` / 添付 ZIP には存在しない。§31 では `[~]` 扱い。
- 既存 portable verifier `kaspa_txscript::verify_mldsa87_with_context`（`crypto/txscript/src/lib.rs`）は実在し、consensus-deterministic な libcrux portable path（per-CPU AVX2/NEON multiplexer ではない）かつ PK 2592 / SIG 4627 の事前 length 拒否を備える。F003 はこれを再利用する。

---

## 2. 目的

### 2.1 必須目的

- 同一native MSKをUTXOとEVMで同時に使用可能にしない。
- native残高の長期保管をML-DSA-87 UTXO側へ寄せる。
- EVM上のNFT・token・native balanceをML-DSAルートで所有できるようにする。
- MetaMask等で扱いやすいsecp256k1鍵を、制限付き日常操作鍵として利用可能にする。
- Session Keyの侵害がaccount全体の所有権喪失にならないようにする。
- 現行ERC-721 / ERC-1155 / ERC-20 / Solidity contractとの互換性を最大限維持する。
- reorg時にUTXOとEVMの価値移動が一体で巻き戻る性質を維持する。
- wallet、CLI、Explorerで各レーンの状態とsecurity levelを明示する。

### 2.2 非目的

- EVM全体からsecp256k1や`ecrecover`を除去すること。
- Ethereum上の全dAppがPQ accountを無修正で扱えること。
- UTXO残高とEVM残高をリアルタイムに二重記録すること。
- arbitrary contract callを、内容解析なしで安全なSession Keyへ許可すること。
- ML-DSA秘密鍵侵害、悪意あるroot owner、脆弱なdAppそのものを完全に防ぐこと。

---

## 3. 採用しない案

### 3.1 UTXO/EVM残高ミラー

```text
UTXOに100 MSKを残したまま
EVMにも100 MSKを表示・使用可能にする
```

この方式は、二つのstate machine間で常に同じ残高を同期しなければならず、reorg、DAG acceptance、partial failure、state root、wallet pending状態の組合せが極端に複雑になります。どちらか一方のdebitが欠けるだけで発行超過になります。

**不採用。** 価値は必ず排他的state transitionで移します。

### 3.2 ML-DSAとECDSAを同格の全権鍵にする

```text
ML-DSA署名でも全額操作可能
ECDSA署名でも全額操作可能
```

この方式は使いやすい一方、アカウント全体の強度がsecp256k1へ低下します。量子攻撃者はECDSA側だけを破ればよく、ML-DSA追加の意味がありません。

**本番では不採用。** ECDSAはSession Keyに限定します。

### 3.3 既存EOAのまま自動sweepだけ追加

作業残高の露出時間は減りますが、NFT、contract admin、allowance、未回収残高はECDSAに残ります。

**短期運用改善としては採用可能ですが、最終設計にはしません。**

### 3.4 PQアドレスを単なる20-byte EOAとして扱う

ML-DSA公開鍵から20-byte EVM addressを作り、そのaddressを通常のECDSA tx senderとしても扱う方式は、Ethereumのsender recovery、Explorer、wallet、nonce semanticsと衝突します。また、contract account addressに一致するECDSA鍵を探索する問題を残します。

**不採用。** PQ accountは明示的なsmart account + auth registryとします。

---

## 4. 脅威モデル

### 4.1 想定する攻撃者

- 大規模量子計算機により、公開済みsecp256k1公開鍵から秘密鍵を回復できる攻撃者。
- Session Key、browser extension、dApp connectionを侵害した攻撃者。
- 悪意あるrelayer / bundler / miner / RPC node。
- 署名replayを試みる攻撃者。
- claimとrefundを同時に成立させようとする攻撃者。
- account registration前後のaddress raceを狙う攻撃者。
- multicall、delegatecall、approval、ERC-1271を使いpolicyを迂回するdApp。
- reorg前のreceiptやside branch eventをfinalと誤認させる攻撃者。

### 4.2 信頼しないもの

- relayerが提示するgas使用量、fee、nonce情報。
- dAppが表示するcalldata説明。
- ECDSA署名が安全であり続けるという仮定。
- `accepted`だけで不可逆finalityに達したという仮定。
- walletが常にonlineでauto-withdrawできるという仮定。

### 4.3 保護対象

- ML-DSA UTXO残高。
- EVM smart accountのnative balance。
- ERC-20 / ERC-721 / ERC-1155。
- NFT collectionのadmin/minter権限。
- bridge withdraw先。
- root/session/recoveryの権限階層。
- native supply invariant。

---

## 5. 規範的不変条件

### I-1: 単一表現

各sompiは、任意のcanonical stateで次のいずれか一つにのみ属します。

```text
A. spendable UTXO
B. unclaimed EVM deposit lock
C. EVM native balance
D. burned balance
```

同じ価値がA/B/Cの複数へ同時に存在してはなりません。

### I-2: 供給保存

wei単位の正確な不変条件を維持します（`EVM_NATIVE_SCALE = 1e10` wei/sompi。コードの supply 恒等式と一致）。

```text
IssuedWei
  = SpendableUtxoSompi * EVM_NATIVE_SCALE
  + DepositLockSompi   * EVM_NATIVE_SCALE
  + EvmTotalNativeBalanceWei
  + BurnAccumulatorWei
```

synthetic withdrawal outputは生成時点からSpendable UTXO側へ含めます。スマートアカウントへの所有権移管（§13）は claim / F002 を経由するため**供給中立**で、I-2 を変えません。

### I-3: claim/refund排他

```text
claim  iff daa < timeout
refund iff daa >= timeout
```

境界DAAを共有してはなりません。

### I-4: claim原子性

deposit lock消費とEVM creditは同一chain blockの同一commit batchで成功または失敗します。

### I-5: withdraw原子性

EVM debitとsynthetic UTXO materializationは同一accepting blockで成功または失敗します。

### I-6: PQ accountのECDSA direct-send禁止（**v1.1: 決定的 class-2 skip**）

`PqAuthRegistry`へ登録されたaddressをsenderとして復元するLegacy/EIP-2930/EIP-1559 txは、**block を無効化せず**、acceptance-time invalidity と同じ**決定的 class-2 skip**として扱います（§8.4）。registry 参照は **per-tx 実行時・in-execution CacheDB・recovered sender のみ**に固定し、template producer と全 verifier で一致させます（c==v）。

### I-7: auth downgrade禁止

account auth typeは次の一方向遷移のみ許可します。

```text
Unregistered / LegacyEcdsa → PqSmartAccountV1
```

`PqSmartAccountV1 → LegacyEcdsa`は存在しません。移行は新accountへ資産を送る方式にします。

### I-8: root権限

全権操作はML-DSA root signatureを必須とします。Session Keyはroot変更、session追加、upgrade、全額withdraw、recovery変更を実行できません。

### I-9: replay防止

全署名は少なくとも次へbindします。

```text
chain_id
network genesis commitment
account address
account version
operation type
nonce space + nonce
valid-after / valid-until
calldata hash
policy/grant id
```

### I-10: account code固定

登録recordはfactory、account version、init code hash、expected account addressをcommitします。登録後に別code hashへ差し替えられません。

### I-11: reorg一体性

registration、claim、withdraw、receipt、registry stateはEVM/UTXOの既存reversed-diffまたはcanonical pointer切替と整合して巻き戻ります（registry は EVM state root 内にあるため pointer-switch で一体に巻き戻る。§8.2 / §25.1）。

### I-12: finality分離

wallet、bridge service、NFT content gatewayは`included`または非canonical `accepted`を不可逆finalとして扱いません。L1/DNS finalityと対応したcanonical finalized stateを使用します。（注: 現行 `safe` タグは sink と同値で k-deep ではない。外部キー解放等の不可逆処理には `finalized`=pruning point 対応の canonical finalized state を用いること。）

---

## 6. 全体アーキテクチャ

```text
┌──────────────────────────────────────────────────────┐
│ ML-DSA UTXO Vault                                    │
│ vault owner key: offline ML-DSA-87                   │
│ recovery SPK: immutable/rotated by explicit process  │
└───────────────┬──────────────────────────────────────┘
                │ EVM_PQ_DEPOSIT_LOCK / claim
                ▼
┌──────────────────────────────────────────────────────┐
│ PQ-Rooted EVM Smart Account                          │
│                                                      │
│ Immutable                                            │
│ - vault_owner_payload64                              │
│ - recovery_spk_hash                                  │
│ - account_version / code_hash / EntryPoint           │
│                                                      │
│ Mutable                                              │
│ - operational_root_payload64 (ML-DSA)                │
│ - root_epoch / root_nonce                            │
│ - freeze flag                                        │
│ - session grants                                     │
│                                                      │
│ Execution paths                                      │
│ - ML-DSA root operation                              │
│ - restricted secp256k1 session operation             │
└───────────────┬──────────────────────────────────────┘
                │ F002 withdraw, root-only by default
                ▼
┌──────────────────────────────────────────────────────┐
│ Synthetic ML-DSA UTXO                                │
└──────────────────────────────────────────────────────┘
```

補助コンポーネント:

```text
F003  ML-DSA-87 verify precompile（versioned: 0x01 FSL generic / 0x02 PREA root）
F004  PQ account auth registry / registration predeploy
Factory  deterministic account deployment
EntryPoint  relayed operation execution
Bundler/Relayer  gas-paying carrier, authorityは持たない
Wallet  auto-fund / auto-sweep / policy UI
```

---

## 7. 鍵階層

### 7.1 Vault Owner Key

- ML-DSA-87。
- UTXO長期残高の所有者。
- cold/offline推奨。
- account登録、emergency freeze、operational root recovery、新accountへのmigrationを認可。
- 日常EVM callには使用しない。

### 7.2 Operational Root Key

- ML-DSA-87。
- Vault Ownerとは別鍵をdefaultとする。
- EVM accountの日常的な高権限操作、session grant、session revoke、NFT移動、withdrawを認可。
- compromise時はVault Ownerがrotate可能。

### 7.3 Session Key

- secp256k1。
- MetaMask互換UX、ゲーム、mint、DEX等の頻繁な操作用。
- 必ず期限、最大value、最大call数、target/selector allowlist、**および非 native 資産の数量上限（§14）** を持つ。
- account所有権を持たない。

### 7.4 推奨security mode

```rust
enum PqAccountSecurityMode {
    PqRootOnly,
    HybridRestricted,
    FrozenRecovery,
}
```

`LegacyDual`は本番仕様へ含めません。

---

## 8. PQ Account addressと登録

### 8.1 deterministic address

account addressは固定FactoryのCREATE2で導出します。

```text
salt = keccak256(
    "MISAKA_PQ_ACCOUNT_V1" ||
    genesis_commitment ||
    vault_owner_payload64 ||
    account_index ||
    recovery_spk_hash ||
    account_version
)

account = CREATE2(factory, salt, init_code_hash(account_version))
```

`genesis_commitment`は64-byte genesis hashそのものではなく、EVMで扱いやすい`keccak256(genesis_hash_bytes)`を使用します。`genesis_commitment` は現状 EVM env に露出していないため、F003/F004 は**per-network 定数として hardcode** する必要があります（未実装プラミング）。

**address-race 対策:** deterministic account address へ攻撃者が registration 前に junk bytecode を CREATE2 deploy すると（その後 CREATE2 が revert）登録不能になり得ます。`account_index` bump による回避経路と「registration relay は permissionless かつ idempotent」を明示規則とします。

### 8.2 native auth registry

EVM state内のsystem predeploy `F004`に、次のrecordを保持します。**state rootに含まれるため、新しいheader fieldは不要**です。これにより registry は既存の state 機構（full-account `EvmStateSnapshot` が `state_root`＝keccak secure-trie を commit、IBD sidecar が verbatim 配送・256 MiB cap、pruning GC、reorg は EVM 側 pointer-switch のみで無 revert）を**追加プラミングゼロで継承**します（`overlay_commitment_root` が専用 header field を要したのと対照的）。

```rust
pub enum EvmAccountAuthType {
    LegacyEcdsa = 0,
    PqSmartAccountV1 = 1,
}

pub struct PqAccountRecord {
    pub account: EvmAddress,
    pub account_version: u16,
    pub factory: EvmAddress,
    pub init_code_hash: EvmH256,

    pub vault_owner_payload64: [u8; 64],
    pub recovery_spk_hash64: [u8; 64],
    pub genesis_commitment: EvmH256,

    pub registered_at_evm_number: u64,
    pub record_hash: EvmH256,
    pub frozen: bool,
}
```

Operational Rootはcontract stateでrotate可能とし、native registryにはimmutableなVault Ownerとaccount identityのみを置きます。

### 8.3 登録フロー

```text
1. walletがaccount addressを計算
2. Vault OwnerがRegistrationをML-DSA署名
3. relayerがFactory.createAndRegisterを送信
4. FactoryがaccountをCREATE2 deploy
5. FactoryがF004へregistrationを書込
6. F004がF003で署名とkey hashを検証
7. code hash/address/configが一致した場合のみregistryへ記録
```

Factory deploy、F004 register、account initializeは一つのEVM tx内でatomicにします。

### 8.4 direct ECDSA transaction拒否（**v1.1: 決定的 class-2 skip**）

**目的（v1.1 で明文化）:** CREATE2 で生成された contract address には通常それに対応する ECDSA 秘密鍵が存在しません。したがってこの ban は通常の Shor 攻撃を直接防ぐというより、

```text
将来の EOA delegation 方式
address preimage / collision 攻撃
誤った account registration
将来の auth 方式追加
```

に対する **defense-in-depth** であり、「account ownership が暗黙に secp256k1 へ fallback する経路を閉じる」ことが本質です。

**規則（block を無効化しない）:** 登録済み PQ account を recovered sender とする Legacy/2930/1559 tx は、現 executor が nonce・残高・fee 等の acceptance-time invalidity を扱うのと同じ**決定的 class-2 skip**にします。`return Err(...)` で block 自体を無効化してはなりません（honest chain で c≠v を引き起こす）。

```rust
// consensus execution 側（kaspa-evm/src/executor.rs の class-2 skip 経路に合わせる）
if registry.is_pq_account(recovered_sender)? {
    outcomes[cand_idx] =
        Some(EvmCandidateOutcome::Skipped {
            class: 2,
            reason: PQ_DIRECT_ECDSA_FORBIDDEN,
        });
    continue;
}
```

**registry 参照の固定（c==v 要件）:**

```text
per-tx 実行時
in-execution CacheDB（同一 block 内で先に実行された registration を後続 tx も参照）
recovered sender のみ（tx.to / access-list address では判定しない）
```

判定サイトは class-5 事前計画パスと v2 gas-pool ループのいずれか**一箇所**に固定し、template producer と全 verifier で一致させます。`tx.to` が PQ account の tx（deposit / EntryPoint relay 先）は valid のまま維持しなければなりません。

**mempool admission はコンセンサス境界ではない。** 現行の admission（`mining/src/evm_mempool.rs` の `admit_tx_info`）は context-free（consensus handle を持たない）であり、PQ 判定は**soft な最適化**に過ぎません。registry-view accessor（`get_evm_account_nonces` / `get_evm_account_states` に倣う）を追加して初めて admission 側チェックが実装可能になりますが、安全境界は常に consensus execution 側です。

これはSession Keyを禁止する規則ではありません。Session Key txのsenderはrelayer EOAであり、account operation内部でSession Key signatureを検証します。

---

## 9. F003 ML-DSA-87 verify precompile（**v1.1: versioned**）

### 9.1 address

```text
0x000000000000000000000000000000000000F003
```

### 9.2 versioned interface

FSL（`docs/misaka-fsl-design-v0.3.md` §4.3）の予約 ABI は

```text
public_key(2592) || message_hash64(64) || signature(4627)   // output 1/0
```

であり、PREA は version / expected payload を加えた 7,348-byte 形式です。**同じ `0xF003` に異なる ABI が定義されているため、実装前に明示的な version 分岐で一本化**します。

```text
共通先頭: version 1 byte

version 0x01: FSL generic Hash64 verification
  input  = 0x01 || public_key(2592) || message_hash64(64) || signature(4627)   = 7284 bytes
  context = (FSL 固定 context ID — 下記凍結対象)
  output = 32-byte ABI bool

version 0x02: PREA key-bound root authorization
  input  = 0x02 || expected_key_payload64(64) || message_hash64(64)
              || public_key(2592) || signature(4627)                            = 7348 bytes
  output = 32-byte ABI bool
```

version 0x02 の検証内容:

```text
1. input lengthが当該versionの固定長に完全一致
2. version == 0x02
3. keyed_blake2b_512(
       "kaspa-pq-v2/address/mldsa87",
       public_key
   ) == expected_key_payload64
4. ML-DSA-87.Verify(
       public_key,
       message_hash64,
       context = "misaka-pq-evm-v1/root/mldsa87",   // 既存7 context と非衝突（確認済み）
       signature
   ) == valid
```

不正length、decode error、key mismatch、invalid signatureはpanicせず`false`を返します（libcrux verify は不正 bytes で Err を返し、`false` へ写像）。

**context domain 分離（必須）:** `"misaka-pq-evm-v1/root/mldsa87"` は `MLDSA87_TX_CONTEXT`（`kaspa-pq-v2/tx/mldsa87`）・attestation/unbond/takeover/audit-ckpt・address payload key のいずれとも**衝突してはなりません**（衝突すると UTXO/attestation 署名が EVM root operation へ cross-protocol replay 可能になる。署名側 C-02 修正と同一クラス）。`version 0x01` の FSL context もこれらと分離します。**任意の caller-provided context は許さず、consensus 側が version → 固定 context ID を対応**させます。

**凍結対象（activation 前に numeric/byte レベルで確定。activation 後の変更は hard fork）:**

```text
入力 encoding（各 version の固定長）
各 version の固定 context ID
不正 length 時の戻り値（false）
gas charging 開始点
最大 input bytes
最大 verify 回数（§9.4）
activation DAA
output ABI（32-byte bool）
```

### 9.3 message_hash64（**v1.1: EIP-712 拡張を PQ 強度と表現しない**）

EIP-712 hash は表示・ECDSA 互換用に有用ですが、

```text
op_hash   = EIP712Hash(domain, PqOperation)   // 32 bytes（ここで一度圧縮される）
message64 = keccak256(0x00 || op_hash) || keccak256(0x01 || op_hash)
```

は元情報が 32-byte `op_hash` へ一度圧縮されているため、64-byte 化しても元の Keccak-256 以上の commitment 強度にはなりません。対応は二択です。

```text
A. 量子時 128-bit 相当の hash 境界として明示的に受容する
B. ML-DSA root 署名には、canonical operation bytes から直接導出する
   domain-separated Hash64 を別途使用する
```

**MISAKA が Hash64 で PQ 強度を維持する方針であるため、本設計は B を採用**します。すなわち:

```text
ML-DSA root 署名対象:
  message_hash64 = keyed_blake2b_512(
      "misaka-pq-evm-v1/op/mldsa87",         // op-digest 専用 domain
      canonical_operation_bytes(PqOperation)  // 圧縮前の正準バイト列
  )

EIP-712 op_hash:
  表示用・ECDSA(session) 互換用に併存させるが、ML-DSA 署名の commitment 強度の根拠にはしない
```

EIP-712 の domain には chain ID、verifying contract、account version、genesis commitment を含めます。EIP-712 単独は replay protection を完成させないため、nonce と期限を operation 本体（I-9）へ必須で入れます。F003 の signer（CLI/remote signer）と verifier は `canonical_operation_bytes` の正準 encoding と op-digest domain に合意し、**この構成専用の KAT**（UTXO tx sighash ベクタの流用ではない）を用意します。

### 9.4 gasとresource cap

固定値を先に決めず、portable backendのworst-case benchmark（ML-DSA-87 verify ≈ 数十µs、入力 ≈ 7.3 KB）に安全率を掛けて設定します。call-override seam は内部フレームからも到達可能なため、flat な per-call gas だけでは block 全体の verify CPU を bound できません。

必須制限（数値を §9.2 の凍結対象として確定）:

```text
MAX_MLDSA_VERIFY_PER_EVM_BLOCK
MAX_MLDSA_AUTH_BYTES_PER_EVM_BLOCK
MAX_MLDSA_VERIFY_PER_TX
```

deposit-claim の per-block cap（=256, `consensus/core/src/evm/mod.rs`）が先例です。これらは**F003 の登録と同時に**入れ、activation DAA と rollback plan をセットにします（F002 cap が inert のまま残っている轍を踏まない）。root operation は大きいため頻繁な操作には使用せず、Session Key grant/revoke・高権限操作に限定します。

### 9.5 実装要件

- `kaspa_txscript::verify_mldsa87_with_context`（`crypto/txscript/src/lib.rs`）と同じ portable verify path を再利用。
- executor と `eth_call` / `eth_estimateGas` simulation へ**単一の共通 register 関数** `register_all_misaka_precompiles()` を用意し、両側から呼ぶ（`estimate_gas` は `simulate_call` を経由するため自動的にカバー）。**executor / sim parity の差分テストを F003 と同時に出す**（現状 F002 にも parity assertion は無い）。
- FIPS 204 KAT、libcrux differential test、malformed input fuzz を必須化。
- CPU architecture 間で accept/reject が一致すること。

---

## 10. F004 PQ Auth Registry predeploy

### 10.1 address

```text
0x000000000000000000000000000000000000F004
```

### 10.2 interface概念

```solidity
interface IMisakaPqAuthRegistry {
    function register(
        Registration calldata r,
        bytes calldata ownerPublicKey,
        bytes calldata ownerSignature
    ) external;

    function isPqAccount(address account) external view returns (bool);
    function record(address account) external view returns (PqAccountRecord memory);
    function setFrozen(address account, bool frozen, bytes calldata pqAuth) external;
}
```

### 10.3 registration preimage

```text
MISAKA_PQ_ACCOUNT_REGISTRATION_V1
chain_id
genesis_commitment
factory
account
account_version
init_code_hash
vault_owner_payload64
operational_root_payload64
recovery_spk_hash64
account_index
registration_nonce
valid_until_evm_block
```

### 10.4 規則

- addressとCREATE2 derivationが一致すること。
- code hashとversionがallowlistにあること。
- Vault Owner payloadとF003検証結果が一致すること。
- registration nonceを再利用しないこと。
- recordは削除不可。
- auth type downgrade不可。
- freeze解除にはVault Ownerまたは現Operational RootのML-DSA認可を要求。

---

## 11. Deposit v2

### 11.1 互換モード

testnet初期実装では現行`EVM_DEPOSIT_LOCK`を変更せず、walletがdestinationとして登録済みPQ accountだけを選ぶ方式を利用できます。

```text
既存lock → registered PQ smart account address
```

この方式はfork範囲を減らしますが、raw RPC利用者はlegacy EOAへdepositできます。

### 11.2 strict mode（**v1.1: registry record の参照点を固定**）

mainnet向けには新しいscript classを追加します。

```rust
pub struct EvmPqDepositLockFields {
    pub pq_account: EvmAddress,
    pub pq_account_record_hash: EvmH256,
    pub timeout_daa_score: u64,
    pub claim_tip_sompi: u64,
    pub refund_script_public_key: ScriptPublicKey,
}
```

claim時に次を検証します。

```text
- lock outpointがunspent
- accepting_daa < timeout
- amount/tip/addressがlockと一致
- F004 registryにpq_accountが存在
- registry.record_hash == lock.pq_account_record_hash
- auth type == PqSmartAccountV1
```

**参照点の固定（c==v 要件）:** UTXO validator が曖昧な virtual EVM state を参照すると node 間で claim 妥当性が割れます。deposit lock は `account + record_hash` を commit し、claim 実行時には

```text
strict claim が参照できる registry record
    = selected_parent(B) の committed EVM state に存在する record
```

に固定して照合します（block B 自身の EVM 結果は claim より後に計算されるため、B 内 registration を同 block claim から参照してはならない。§11.3）。この read-point を spec で pin するまで、strict-mode deposit（Phase C）は activation しません。

legacy depositはactivation後も既存lockのsettle/refundを許可しますが、新規作成はnetwork parameterで段階的に停止可能にします。

```rust
pub enum EvmDepositPolicy {
    LegacyAndPq,
    PqPreferred,
    PqRequired,
}
```

### 11.3 claim順序とregistration分離（**v1.1**）

現 executor の system operation 順序は次で固定されています。

```text
1. account registration / freeze system effects
2. deposit claims
3. accepted user EVM txs
```

claim は user tx より先に処理されるため、**同一 chain block 内の registration tx で作成された F004 record を、その block の deposit claim から参照することはできません**。したがって規則は「strict claim は `selected_parent(B)` の committed EVM state の record のみ参照可」（§11.2）とし、wallet フローは次を normative とします。

```text
register
→ canonical accepted 確認
→ 必要なら finalized 確認
→ deposit lock 作成
→ claim
```

**liveness 注意:** record_hash H を commit した strict lock は、その registration が claimer の selected-parent EVM state に入るまで claim 不能です。registration が reorg out されると refund-timeout まで資金が固着し得るため、wallet は**未 finalized な registration に対して deposit してはなりません**。

---

## 12. Withdraw設計

### 12.1 F002再利用

現行F002は次を満たすため再利用します。

- EVM callerからvalueをjournal上で移動。
- committed logだけをWithdrawOpへ変換。
- EVM残高からburnし、synthetic ML-DSA UTXOを生成。
- destination scriptをPQ標準classへ制限（公開 `main` で `MAX_WITHDRAW_SCRIPT_BYTES=128` を無条件 enforce）。

F002 は contract の msg.sender でも動作し（intercept は target/bytecode address のみ判定、EOA 前提なし）、スマートアカウントからの呼び出しでも supply 中立です。

### 12.2 account policy

PQ smart accountからF002を呼ぶ経路を制限します。

```text
Vault Owner            : recovery SPKへのwithdraw可
Operational Root       : allowlisted PQ SPKへのwithdraw可
Session Key default    : F002禁止
Session Key optional   : immutable recovery SPKのみ、少額cap付き
```

### 12.3 全額回収

```solidity
function withdrawAllToRecovery(PqRootAuth calldata auth) external;
```

- exact sompi multiple部分をF002へ送る。
- 1 sompi未満のwei dustはaccountに残る。
- root operationとしてnonce/replay protectionを適用。
- arbitrary destinationは別の高権限operationに分離。

### 12.4 withdrawal resource cap

F002には独立した件数・bytes・UTXO write capを追加します。

```text
MAX_WITHDRAWALS_PER_EVM_BLOCK          [~] ローカル実装済み・公開 main 未マージ・inert
MAX_WITHDRAW_SCRIPT_BYTES (per-op)     [x] 公開 main で active
MAX_SYNTHETIC_UTXO_WRITES_PER_EVM_BLOCK [ ] 未実装（per-block count cap が代替的に bound）
```

EVM gasだけでなく、RocksDB、MuHash、UTXO index、serializationのworst-case benchmarkで料金を決めます。`MAX_WITHDRAWALS_PER_EVM_BLOCK` は activation DAA を設定するまで本番では inert である点に注意。

---

## 13. PQ Smart Account contract

### 13.1 immutable fields

```solidity
bytes64  public immutable vaultOwnerPayload;
bytes32  public immutable recoverySpkHash;
address  public immutable entryPoint;
uint16   public immutable accountVersion;
bytes32  public immutable registryRecordHash;
```

実際のrecovery script bytesは69 bytesをstorageへ保持してもよいですが、immutable hashとの一致を検査します。

### 13.2 mutable fields

```solidity
bytes64 public operationalRootPayload;
uint64  public rootEpoch;
uint256 public rootNonce;
bool    public frozen;

mapping(bytes32 grantId => SessionGrant) public sessions;
mapping(bytes32 grantId => uint256 nonce) public sessionNonce;
```

### 13.3 operation interface

```solidity
struct PqOperation {
    uint8   operationType;
    uint64  rootEpoch;
    uint192 nonceKey;
    uint64  nonce;
    uint64  validAfterEvmBlock;
    uint64  validUntilEvmBlock;

    address target;
    uint256 value;
    bytes   callData;

    uint64  callGasLimit;
    uint256 maxRelayerFee;
    bytes32 grantId;
    bytes32 policyHash;
}
```

### 13.4 root execution

```solidity
function executeRoot(
    PqOperation calldata op,
    bytes calldata publicKey,
    bytes calldata signature
) external returns (bytes memory result);
```

検査順:

```text
1. account frozen policy
2. rootEpoch
3. nonce
4. validAfter/validUntil
5. op domain / chain / account binding
6. public key payload == current Operational Root または Vault Owner
7. F003 verify（version 0x02、§9.3 の op-digest Hash64 を message に）
8. operation-specific authority
9. nonce increment
10. external CALL
```

nonceは外部call前にincrementし、revert時はEVM journalにより戻します。

### 13.5 session execution

```solidity
function executeSession(
    PqOperation calldata op,
    SessionProof calldata proof,
    bytes calldata ecdsaSignature
) external returns (bytes memory result);
```

- EIP-712 operation hashをECDSA検証。
- recovered keyがgrant keyと一致。
- grant、nonce、expiry、累積counter、**および非 native 資産 limit（§14）** を検査。
- `CALL`のみ許可し、`DELEGATECALL`を提供しない。
- state counterを外部call前に更新し、revert時は巻き戻す。

### 13.6 upgrade方針

proxy upgradeをdefaultにしません。

```text
v1 accountはimmutable
upgradeはv2 accountを新規deploy
ML-DSA rootでnative/ERC-20/NFTをv2へ移送
```

これにより、upgrade admin compromiseとimplementation差替えriskを減らします。

---

## 14. Session Grant（**v1.1: 非 native 資産の数量モデルを規範化**）

```solidity
struct SessionGrant {
    address sessionKey;

    uint64 validAfterEvmBlock;
    uint64 validUntilEvmBlock;
    uint64 maxCalls;

    uint128 maxNativeValuePerCall;
    uint128 maxNativeValueTotal;
    uint64  maxGasPerCall;

    bytes32 targetMerkleRoot;     // address だけでなく code-hash も pin（§14.5）
    bytes32 selectorMerkleRoot;
    bytes32 tokenPolicyRoot;      // 下記 Erc20/721/1155 limit set を commit

    uint32 flags;
    uint64 rootEpoch;
}
```

### 14.1 必須制約

- `validUntil`必須。無期限grantは禁止。
- `maxCalls`必須。
- native value cap必須。
- **非 native 資産を扱う grant は §14.6 の数量 limit を必須**（`maxNativeValue` は native MSK しか縛らない）。
- targetまたはpolicy module必須。任意target grantは禁止。
- grantはrootEpochへbindし、root recovery後に旧grantを一括失効。
- session自身によるgrant追加・延長・cap拡大は禁止。

### 14.2 常時禁止する操作（deny-by-default）

generic session からは次を**常時禁止**します（個別 policy module でのみ明示許可）。

```text
account root変更
Vault Owner変更
新session追加
session cap拡大
account code upgrade
DELEGATECALL
arbitrary F002 withdraw
setApprovalForAll
approve(max) / 無制限 ERC-20 approve
Permit2 unlimited approval
arbitrary multicall / aggregator forwarding
upgradeable aggregator target
unknown calldata（decode 不能）
任意ERC-1271署名
freeze解除
```

### 14.3 NFT用policy

```solidity
struct NftSessionPolicy {
    bytes32 allowedCollectionsRoot;
    bytes32 allowedMarketplacesRoot;
    bool canMint;
    bool canList;
    bool canCancelListing;
    bool canTransfer;
    bool canApproveSingleToken;
    bool canSetApprovalForAll; // default false
    uint64 maxTransferCount;
    uint128 maxListingPrice;
}
```

`setApprovalForAll`はcollection全体を失う可能性があるため、default falseとします。

### 14.4 DEX / router policy（**v1.1: 内部 sub-call を完全 decode**）

「内部 sub-call を一般的に再検証する」のはスマートアカウント単体では困難です。session policy はトップレベル target/selector だけでなく、**許可は特定 router ごとの policy module に限定**し、その module が calldata を完全 decode して次を検査します。

```text
router address allowlist
function selector allowlist
対象 token / recipient / amount / deadline / slippage（min-out）
max native / token amount（§14.6 の limit と整合）
```

walletは min-out/slippage を clear-sign します。aggregator / 任意 multicall を allowlisted target にしてはなりません（selector allowlist の万能バイパスになる）。

### 14.5 target の code-hash pin（**v1.1**）

`targetMerkleRoot` がアドレスのみを pin すると、upgradeable proxy の実装が grant 署名後に差し替わり挙動が変わります。proxy の code-hash だけでも不十分なため、**実装差し替えを許す proxy は session 対象外**にするか、実行時に implementation address / code hash まで確認します。

### 14.6 非 native 資産の数量 limit（**v1.1 新設**）

`maxNativeValue` だけでは ERC-20 / NFT を全額移動可能です。`tokenPolicyRoot` は最低限次の limit set を commit します。

```rust
struct Erc20Limit {
    token: Address,
    max_per_call: U256,
    max_total: U256,
    max_allowance_delta: U256,   // approve 増分の上限（spender allowlist と併用）
}

struct Erc721Limit {
    collection: Address,
    allowed_token_ids_root: Bytes32,
    max_transfers: u64,
}

struct Erc1155Limit {
    collection: Address,
    token_id: U256,
    max_per_call: U256,
    max_total: U256,
}
```

approve 系は原則 §14.2 で禁止し、必要時のみ `max_allowance_delta` + spender allowlist 付きで限定許可します。

---

## 15. ERC-1271（**v1.1: purpose 自己申告を信用しない**）

PQ smart accountは`isValidSignature(bytes32,bytes)`を実装します。

### 15.1 default policy

- ML-DSA root signatureのみgeneral ERC-1271 valid。
- Session Keyはgeneral message signing不可。
- Session signatureを許可する場合、signature envelopeへ元の typed payload と `grantId`、purpose、deadline、domainを含めます。

```solidity
enum SignaturePurpose {
    Login,
    NftListing,
    Permit,
    Order,
    Custom
}
```

### 15.2 purpose の再計算照合（必須）

`isValidSignature(hash, signature)` は opaque な 32-byte hash しか受け取らないため、session が `purpose = Login` と名乗るだけでは Permit/Permit2/marketplace order の hash を Login と偽れます。account は claimed-purpose の typed payload から hash を**再計算して照合**しなければなりません。

```text
expected_hash = hashKnownSchema(
    purpose, domain, collection, tokenId, amount, deadline, grantId
)
require(expected_hash == ERC1271_hash_argument)
```

未知 schema・任意 `Custom`・生 32-byte hash への session 署名は **default 拒否**。grant が purpose を明示許可しない場合も invalid です。

### 15.3 理由

Session KeyのECDSA署名を無条件でERC-1271 validにすると、on-chain call capを迂回して、off-chain order、permit、NFT listing、asset authorizationを作成できます（§14 の数量 cap を実質迂回する経路になる）。

---

## 16. EntryPoint / Relayer

### 16.1 原則

relayerはgasを支払うcarrierであり、account authorityを持ちません。

```text
Session/Rootがoperation署名
      ↓
任意relayerがEntryPointへsubmit
      ↓
Accountが署名・policyを検証
      ↓
AccountからtargetへCALL
```

### 16.2 ERC-4337互換性

初期版は小さなMISAKA専用EntryPointでよいですが、operation model、bundler RPC、account validationをERC-4337へ寄せます。

将来RPC:

```text
eth_sendUserOperation
eth_estimateUserOperationGas
eth_getUserOperationReceipt
```

MISAKA独自root auth dataは`signature` field内のtyped envelopeとして扱えます。

### 16.3 fee reimbursement

relayerへの支払は署名済み上限を超えてはなりません。

```text
maxRelayerFee
maxGasPrice
callGasLimit
verificationGasLimit
```

EntryPointがgas before/afterを測定し、実費と上限の小さい方だけを支払います。relayer提供値を無条件に信頼しません。

### 16.4 availability

特定bundlerを必須にしません。誰でもoperationをrelayでき、walletは複数relayerへ送信可能にします。

---

## 17. 残高運用

### 17.1 one-shot mode

単発mint、購入、contract call向けです（account は登録済みが前提。未登録なら §11.3 の `register → finalized` を先に行う）。

```text
estimate
→ 必要額のみUTXOからdeposit
→ claim finalized
→ operation実行
→ receipt finalized
→ 余剰nativeをPQ UTXOへwithdraw
```

必要額:

```text
call value
+ estimated gas * max fee
+ claim tip
+ relayer fee
+ small safety margin
```

### 17.2 session mode

ゲーム、複数mint、DEX等向けです。

```text
max EVM working balance
session expiry
per-call cap
aggregate cap
target/selector allowlist
非 native 資産の per-token / per-collection limit
auto-sweep threshold
```

Session Keyが量子攻撃で破られた場合も、grant残量（native + 非 native limit）以上は使用できません。

### 17.3 長期EVM資産

NFTやERC-20はPQ smart accountが保有できます。native working balanceは小さく保ちますが、NFTはEVMに残ってもroot ownershipがML-DSAであるため、現行EOAより耐量子性が高くなります。

---

## 18. 状態機械

```text
UTXO_AVAILABLE
    │ create deposit lock
    ▼
DEPOSIT_LOCKED
    ├─ daa < timeout: claim
    │      ▼
    │   EVM_ACTIVE
    │      ├─ EVM calls / NFT ownership
    │      └─ F002 withdraw
    │             ▼
    │         UTXO_AVAILABLE
    │
    └─ daa >= timeout: ML-DSA refund
           ▼
       UTXO_AVAILABLE
```

account auth state:

```text
UNREGISTERED
    │ ML-DSA registration + fixed code commitment
    ▼
PQ_ACTIVE
    ├─ freeze → PQ_FROZEN
    ├─ operational root rotation → PQ_ACTIVE(root_epoch+1)
    └─ migration → old account remains PQ, assets move to new PQ account
```

`PQ_ACTIVE → LEGACY_ECDSA`は存在しません。

---

## 19. NFTとの統合

### 19.1 ownership

```text
ERC-721 ownerOf(tokenId) = PQ smart account address
```

contract addressとして通常のNFTを保有できます。

### 19.2 mint

- low-value mintはSession Grantで許可可能（§14.6 の Erc721Limit 付き）。
- collection admin/minter roleはOperational Rootまたは別PQ accountへ置く。
- collection contractの`DEFAULT_ADMIN_ROLE`はML-DSA root operation経由でのみ使用。

### 19.3 marketplace

- ERC-1271対応marketplaceを優先。
- EOA署名のみ対応marketplaceはPQ rootと互換でない。
- Session listingを許可する場合、collection、tokenId、price、expiry、marketplace domainを署名へbind（§15.2 の再計算照合を適用）。

### 19.4 Logic Capsule NFT

holder-gated `.logicx` download keyの解放は、canonical finalized ownershipを確認します。side branch receiptまたは非finalized transferでは鍵を解放しません。

---

## 20. RPC設計

### 20.1 account

```text
misaka_getPqAccount(address)
misaka_getPqAccountAddress(config)
misaka_registerPqAccount(registration)
misaka_getPqAccountSecurity(address)
misaka_getPqAccountSessions(address)
```

### 20.2 operation

```text
misaka_estimatePqOperation(operation)
misaka_sendPqOperation(operation, auth)
misaka_getPqOperationStatus(hash)
misaka_waitPqOperation(hash)
```

status例:

```json
{
  "state": "included",
  "canonical": true,
  "finalized": false,
  "account": "0x...",
  "authScheme": "ML_DSA_87_ROOT",
  "rootEpoch": 3,
  "nonce": 18,
  "includedIn": ["..."],
  "acceptedIn": "...",
  "failureReason": null
}
```

### 20.3 bridge

```text
misaka_getDepositState(outpoint)
misaka_getWithdrawalState(evmTxHash, opIndex)
misaka_getCombinedBalance(pqAddress, pqAccount)
```

combined balanceは内訳を必ず表示します。

```text
PQ spendable
Deposit locked
EVM native
Pending withdrawal
Burn/fee
```

単一の曖昧な`total`だけを表示しません。

---

## 21. CLI設計

```bash
misaka pq-account create \
  --vault-key-file vault.mldsa \
  --operational-key-file evm-root.mldsa \
  --recovery-address misakatest:... \
  --account-index 0

misaka pq-account register --account pq-account.json --sponsor auto

misaka pq-account fund \
  --from-wallet main.wallet \
  --account pq-account.json \
  --amount auto \
  --for-call call.json

misaka pq-account session grant \
  --account pq-account.json \
  --session-key session.json \
  --expires 1h \
  --max-value 10 \
  --allow-target 0x... \
  --allow-selector 0x... \
  --erc20-limit 0xToken:perCall:total

misaka pq-account call \
  --account pq-account.json \
  --auth session \
  --to 0x... \
  --data 0x... \
  --wait-finalized

misaka pq-account withdraw \
  --account pq-account.json \
  --all \
  --to-recovery \
  --auth root

misaka pq-account freeze --auth vault-owner
misaka pq-account recover-root --new-root ... --auth vault-owner
```

### 21.1 安全なdefault

- raw EOA depositはadvanced option。
- `--allow-legacy-evm-address`がない限り拒否。
- secretsをCLI引数へ直接渡さない。
- key fileは0600、暗号化keystore、OS keyring、remote signerを支援。
- operationはhuman-readable clear-sign画面を表示。
- network name、chain id、genesis commitmentを表示。
- `accepted`と`finalized`を分ける。

---

## 22. Wallet UX

### 22.1 security表示

```text
Account type             PQ-Rooted Smart Account
Unrestricted authority   ML-DSA-87
Daily session authority  secp256k1, restricted
Direct ECDSA tx           consensus-forbidden (skipped)
PQ UTXO recovery          enabled
Current EVM exposure      2.40 MSK
Session expiry            43 minutes
```

### 22.2 操作確認

walletは署名前に次を表示します。

```text
Target contract
Function name / selector
Native value
Token/NFT movement estimate
Approval change
Session remaining cap（native + 非 native）
Expiry
Maximum relayer fee
Final recovery destination
```

calldataをdecodeできない操作はSession Keyでdefault拒否し、ML-DSA root clear-signを要求します。

### 22.3 auto-sweep

auto-sweepは利便機能でありsecurity invariantにはしません。walletがofflineでも資産はPQ smart accountに残るため、ECDSA EOAより安全です。

---

## 23. Consensus変更一覧

### 23.1 必須

1. F003 ML-DSA verify precompile（versioned 0x01/0x02、§9）。
2. F004 PQ Auth Registry system predeploy。
3. PQ registered senderからのdirect ECDSA tx **class-2 skip**（block 無効化でない、§8.4）。
4. executorとsimulationのhandler parity（共通 `register_all_misaka_precompiles()` + parity test）。
5. registry stateのsnapshot/IBD/pruning/reorg対応（EVM state root 内に置くため既存機構を継承）。
6. 新error/skip-reason codeとRPC reason。

### 23.2 strict deposit向け

1. `ScriptClass::EvmPqDepositLock`。
2. `EvmPqDepositLockFields`。
3. PQ registry record hashを検証するclaim variant（参照点 = `selected_parent(B)` committed EVM state、§11.2）。
4. mempool standardnessとUTXO-context rule。
5. network parameter `EvmDepositPolicy`。

### 23.3 推奨file map

```text
consensus/core/src/evm/mod.rs
  EvmAccountAuthType / PqAccountRecord / F003,F004 address 定数 / caps

crypto/txscript/src/script_class.rs
  EvmPqDepositLock parser/builder

kaspa-evm/src/mldsa_verify.rs
  F003（versioned）

kaspa-evm/src/pq_auth_registry.rs
  F004 handler / registry access

kaspa-evm/src/executor.rs
  direct ECDSA sender class-2 skip / register_all_misaka_precompiles 登録

kaspa-evm/src/sim.rs
  simulation parity（同一 register_all 呼び出し）

consensus/src/processes/evm/mod.rs
  PQ deposit claim validation（selected-parent registry view）

mining/src/evm_mempool.rs
  soft auth admission（最適化）/ reason codes

contracts/pq-account/
  Account / Factory / EntryPoint / policy modules / tests

misaka-cli/src/pq_account.rs
wallet-apps/*
  account UX / session / bridge orchestration
```

---

## 24. Mempool・template規則

- registered PQ accountをECDSA senderとするtxは admission で**soft reject（最適化）**。安全境界は consensus execution 側の class-2 skip（§8.4）。
- consensus executorで registry を per-tx・in-execution CacheDB・recovered sender で判定し class-2 skip。
- skip reasonを`PQ_DIRECT_ECDSA_FORBIDDEN`として公開。
- EntryPoint txはrelayer senderのstate nonceで通常処理。
- same account operation nonce重複はEntryPoint validationで拒否。
- root operationの巨大auth bytes（version 0x02 = 7,348 B）をpayload byte cap・§9.4 verify capへ正確に反映。
- Session operationは通常65-byte ECDSA signatureで高頻度利用。

---

## 25. Reorg・finality

### 25.1 registration reorg

registrationを含むEVM blockがcanonicalから外れた場合、registry recordもstate rootとともにcanonical pointerから外れます（EVM state は BlockHash keyed で無 revert、reorg は `CanonicalEvmHeads` pointer 切替のみ）。そのaccountへのdeposit claimはcanonical（selected-parent）registry viewを使用します。

### 25.2 deposit reorg

claimされたblockがreorgされた場合、lock消費とEVM creditを一体で戻します。

### 25.3 withdrawal reorg

withdraw accepting blockがreorgされた場合、synthetic UTXOとEVM debitを一体で戻します。

### 25.4 external service

NFT download、商品発送、CEX credit等はcanonical finalized mappingを使用します。`included_in`やside-branch `accepted_in`を根拠に不可逆処理を行いません。`safe` タグは現状 sink 同値（k-deep でない）ため、不可逆処理の根拠には `finalized` を用います。

---

## 26. 移行計画

### Phase A: testnet smart-account MVP

- F003実装（versioned、inert fence）。
- immutable PQ Account / Factory / EntryPoint。
- 現行deposit lockを登録済みaccountへ送るwallet default。
- ML-DSA root + restricted Session Key。
- account/NFT/withdraw E2E。

この段階ではraw legacy depositを完全には禁止しません。

### Phase B: strict PQ auth

- F004 registry。
- direct ECDSA sender class-2 skip。
- root recovery/freeze。
- ERC-1271 purpose binding（再計算照合）。
- multiple relayer support。

### Phase C: strict deposit

- `EvmPqDepositLock`（参照点固定済み）。
- registry record hash commitment。
- `PqPreferred` activation。
- legacy wallet警告。

### Phase D: production policy

- `PqRequired` activationをmainnetで検討。
- 既存legacy locksはclaim/refund可能。
- 新規legacy depositだけ停止。

### Phase E: optional native PQ operation

将来、ML-DSA operationをEIP-2718相当のMISAKA独自tx typeとしてnativeに扱い、bundler依存とF003 calldata costを減らせます。ただしconsensus・RPC・Explorer互換性への影響が大きいため、v1ではsmart account方式を採用します。

---

## 27. 既存EOA資産の移行

### Native MSK

```text
legacy EOA → PQ account
または
legacy EOA → F002 → PQ UTXO → PQ accountへ必要額だけ再deposit
```

後者はUTXO Vaultへ一度戻すため、推奨です。

### NFT / ERC-20

- legacy EOAが安全なうちにPQ accountへtransfer。
- collection admin/minter roleもPQ accountへgrantし、旧EOA roleをrevoke。
- ERC-20 allowanceをlegacy EOAで残さない。

### 量子緊急時

ECDSA公開鍵が既に露出し、攻撃者が署名可能になった後の移行は競争になります。mainnet前からPQ accountをdefaultにし、長期資産をEOAへ置かないことが必要です。

---

## 28. 試験計画

### 28.1 Bridge property tests

- claim前refund失敗。
- timeout境界でclaim失敗/refund成功。
- duplicate claim失敗。
- mergeset spend済みlockのclaim失敗。
- claim creditとlock消費のatomicity。
- withdraw debitとsynthetic UTXOのatomicity。
- reorgで双方が同時rollback。
- supply invariantをstateful fuzz。

### 28.2 F003

- FIPS 204 known-answer tests（version 0x01 / 0x02 双方）。
- valid/invalid signature。
- public key length 2591/2592/2593。
- signature length 4626/4627/4628。
- key payload mismatch（0x02）。
- version byte 取り違え / 未知 version。
- context ID 取り違え（version → 固定 context の対応）。
- message bit flip。
- portable/SIMD backend differential。
- op-digest Hash64（§9.3 option B）専用 KAT。
- executor / eth_call / estimateGas parity（差分テスト）。
- block gas / §9.4 cap 上限で verify 数が必ず bounded。
- activation直前・直後（fence 切替で genesis/state root 不変）。
- fuzzでpanicなし。

### 28.3 Registry

- CREATE2 address mismatch拒否。
- code hash mismatch拒否。
- registration replay拒否。
- auth downgrade拒否。
- registration reorg（pointer 切替で record も巻き戻る）。
- registered addressからdirect ECDSA txが **class-2 skip**（block は valid）。
- unregistered EOA txは従来どおり成功。
- 同一block内 registration を同block claim から参照しない（selected-parent 参照点）。

### 28.4 Account

- root operation success/failure。
- cross-chain replay拒否。
- cross-account replay拒否。
- rootEpoch replay拒否。
- expired operation拒否。
- nonce race。
- malicious relayerによるfee増額拒否。
- reentrancy。
- delegatecall不在。
- frozen account behavior。
- recovery root rotation（旧 epoch + 全旧 session 失効）。

### 28.5 Session bypass

- arbitrary target拒否。
- selector proof偽造拒否。
- multicall / aggregator 内の禁止call。
- proxy target / upgradeable implementation を使ったpolicy bypass（code-hash 確認）。
- `approve(type(uint256).max)`拒否。
- `setApprovalForAll`拒否。
- Permit2 unlimited approval 拒否。
- ERC-20 / NFT 数量 limit 超過拒否（非 native drain）。
- F002 arbitrary destination拒否。
- ERC-1271 purpose confusion 拒否（再計算照合 / 生 hash 拒否）。
- grant expiry/counter/cumulative value（native + 非 native）。

### 28.6 NFT

- mint/list/transfer policy。
- ERC-1271 marketplace flow。
- non-1271 marketplace warning。
- session compromise時に高価値NFTを移動できないこと。
- collection admin roleがPQ rootにあること。

### 28.7 Soak/DoS

- ML-DSA root operation burst（§9.4 cap で bounded）。
- large auth calldata。
- registry account大量作成。
- state snapshot / IBD / pruning。
- public RPC simulation rate limiting。
- F002 withdrawal count cap（activation 後）。

---

## 29. 監視KPI

```text
pq_accounts_registered_total
pq_accounts_frozen_total
pq_root_operations_total
pq_root_verify_failures_total
pq_session_operations_total
pq_session_policy_rejections_total
pq_direct_ecdsa_skips_total
pq_deposit_locked_sompi
pq_evm_native_balance_wei
pq_withdrawal_sompi
pq_account_exposure_p50/p95
pq_operation_finality_latency
pq_registry_state_bytes
mldsa_verify_cpu_seconds
```

security event:

```text
DIRECT_ECDSA_FROM_PQ_ACCOUNT (skipped)
REGISTRATION_REPLAY
ROOT_EPOCH_MISMATCH
SESSION_POLICY_DENIED
NON_NATIVE_LIMIT_EXCEEDED
UNEXPECTED_ACCOUNT_CODE_HASH
FINALITY_ROLLBACK_AFTER_EXTERNAL_ACTION
```

---

## 30. 残留リスク

### 30.1 EVM全体がPQになるわけではない

- relayer、他EOA、dApp admin、oracle、marketplaceはECDSAのままの場合があります。
- `ecrecover`依存contractは量子耐性を持ちません。
- PQ accountが他protocolへdepositした資産は、そのprotocolの安全性へ依存します。

### 30.2 Session Keyは量子攻撃対象

Session Keyは破られる前提で、被害をcapします。期限・金額・非 native 数量を無制限にすると本設計の安全性は失われます。

### 30.3 Smart contract risk

account、EntryPoint、Factory、policy module、F003/F004は新しいcritical surfaceです。immutable設計、最小code、独立監査、formal property testingが必要です。

### 30.4 dApp互換性

contract wallet、ERC-1271、relayed operationを理解しないdAppは利用できない場合があります。walletは互換性を事前検査します。

### 30.5 finality/RPC

RPCがcanonicalityやfinalized tagを誤ると、off-chain serviceが誤動作します。NFT access gatewayやbridge外部処理の前に、canonical finality APIの修正をrelease gateとします。

---

## 31. Production release gate（**v1.1: トライステート**）

凡例: `[x] merged and verified` ／ `[~] implemented locally, not yet independently verified` ／ `[ ] not implemented`。
以下が全て `[x]` になるまで、`PQ-protected EVM account`という製品保証を出しません。

```text
[ ] F003がコード実装され、FIPS 204 KAT（0x01/0x02）とcross-backend testを通過
[ ] F004 registryがstate root、reorg、snapshot、IBD、pruningへ統合
[ ] direct ECDSA sender class-2 skip が admission(soft)/execution(authoritative) 双方に存在
[ ] Account / Factory / EntryPointが独立監査済み
[ ] Session policy bypass fuzz（非 native limit / approval / 1271 含む）が完了
[ ] claim/refund/withdraw/supply stateful fuzzが完了
[~] canonical finalized / tx status が正確（ローカル実装、要独立検証。safe=sink の注記つき）
[~] EVM state pruning/retentionが実装済み（ローカル DB-GC、要独立検証）
[~] public RPC connection/batch/timeout/response-size制限が実装済み（ローカル、要独立検証）
[~] F002 withdrawal count/bytes/write cap が実装済み（ローカル・公開 main 未マージ・inert。activation 未設定）
[~] wallet secrets が暗号化keystore/secure signerで管理（部分: 0600+zeroize+CLI非露出。暗号化keystore/keyring は未）
[ ] legacy EOA depositがUI defaultから除外
[ ] mainnet activation DAAとrollback planが確定（F002 cap / F003 を含む）
```

---

## 32. Acceptance criteria

設計完了の判定基準:

1. 10,000回のランダムdeposit/claim/refund/withdraw/reorg sequenceでsupply invariantが一度も破れない。
2. 登録済みPQ accountをsenderとする標準ECDSA txが全nodeで同じ理由（`PQ_DIRECT_ECDSA_FORBIDDEN`）で **class-2 skip** され、それを含む block は valid のまま。
3. Operational Rootを知らず、Session Keyだけを持つ攻撃者がgrant cap（native + 非 native）を1 weiでも超えられない。
4. Session Keyがroot変更、grant追加、arbitrary withdraw、`setApprovalForAll`、非 native limit 超過を実行できない。
5. Vault OwnerがOperational Rootをrotateすると、旧root epochと全旧sessionが失効する。
6. F003のvalid/invalid判定が全supported CPU backendで一致する（0x01/0x02）。
7. deposit claim blockのreorgでEVM creditとlock消費が同時に戻る。
8. withdrawal blockのreorgでEVM debitとsynthetic UTXOが同時に戻る。
9. ERC-721をPQ accountが所有し、ML-DSA rootでtransferできる。
10. ECDSA Session Keyで許可されたmintはできるが、禁止されたNFT transferはできない。
11. walletがcombined balanceをlane別に正しく表示する。
12. `accepted`と`finalized`を混同せず、external key releaseはfinalized後だけ行われる。

---

## 33. 実装優先順位（**v1.1: P0 を P0-0/P0-1 に分割**）

### P0-0（仕様凍結 — コード前）

1. PREA/FSL 文書の F003 ABI 統一（version 0x01/0x02、§9.2）。
2. context ID 凍結（version → 固定 context、§9.2 / §9.3 op-digest domain）。
3. gas / resource 数値決定（§9.4 caps を numeric 化）。
4. activation 方針決定（DAA + rollback plan）。

### P0-1（F003 inert 実装 — self-contained, genesis 不変）

1. F003 handler（versioned）。
2. executor / simulation 共通 `register_all_misaka_precompiles()`。
3. activation fence = `u64::MAX`（全 net inert、public network 未有効化）。
4. §28.2 の必須テスト一式。

必須テスト（再掲・要点）:

```text
FIPS 204 KAT / 既存 portable verifier 一致
valid / invalid / malformed length
version / context ID 取り違え
key payload mismatch（0x02）
executor / eth_call / estimateGas parity
block gas / §9.4 cap で verify 数が必ず bounded
activation 直前・直後（genesis/state root 不変）
異種 CPU で同一 accept/reject
fuzz で panic なし
```

**F003 をマージしても、F004 / account が完成するまでは製品上「PQ EVM account 対応」と表示してはなりません**（F003 単体は primitive であり、`executeRoot` が消費するまで dead code）。

### P0-2（直後）

immutable PQ Account / Factory / EntryPoint + `executeRoot`（F003 version 0x02 を消費）+ restricted Session Key + 現行 bridge を使った registered account への deposit/withdraw E2E + CLI `create/register/fund/call/withdraw/status`。

### P1

1. F004 auth registry。
2. direct ECDSA sender class-2 skip（admission soft + execution authoritative）。
3. Vault Owner recovery/freeze。
4. ERC-1271 purpose binding（再計算照合）。
5. NFT policy module + 非 native 数量 limit。
6. multiple relayer / fee cap。

### P2

1. strict `EvmPqDepositLock`（参照点固定）。
2. legacy deposit deprecation。
3. ERC-4337 RPC互換。
4. hardware/remote ML-DSA signer。
5. native PQ operation typeの研究。

---

## 34. 最終判断

現行のbridgeは、lockを消費してからEVMへcreditし、EVMをdebitしてからUTXOをmaterializeするため、二重使用防止の基礎として再利用できます。

変更すべきなのは、EVM残高のownershipです。

```text
現行:
  UTXOはPQ
  EVMへ移した瞬間にECDSA全権

提案:
  UTXOはPQ
  EVMへ移してもML-DSA rootが全権
  ECDSAは制限付きSession Keyのみ
```

また、単にcontract walletを作るだけではなく、登録済みPQ accountからのdirect ECDSA txをconsensusで（block を無効化しない **class-2 skip** として）禁止することが重要です。CREATE2 contract address には通常対応する ECDSA 秘密鍵が存在しないため、この ban は直接の Shor 防御というより、将来の EOA delegation / address preimage・collision / 誤った registration / 将来の auth 方式追加に対する **defense-in-depth** であり、「account ownership が暗黙に secp256k1 へ fallback する経路を閉じ、ML-DSA ルートを実際の最上位 authority にする」ためのものです。

この設計は、残高ミラーを導入せず、現行の供給保存・claim/refund排他・F002原子性を維持しながら、NFTとEVM作業残高の耐量子性を現行設計より大きく引き上げます。

---

## 参考資料

### MISAKA source snapshot

- `docs/misaka-evm-design-v0.4.md` — §9 deposit/withdraw、§20 PQ境界。
- `crypto/txscript/src/script_class.rs` — current `EvmDepositLockFields`（strict `EvmPqDepositLockFields` は未実装）。
- `consensus/src/processes/transaction_validator/tx_validation_in_utxo_context.rs` — refund timeout rule。
- `consensus/src/processes/evm/mod.rs` — claim validationとduplicate防止。
- `consensus/src/pipeline/virtual_processor/processor.rs` — claim viewとbridge effectの同一block適用。
- `kaspa-evm/src/withdraw.rs` — F002 journal/log/withdraw invariant（唯一の precompile-style handler）。
- `kaspa-evm/src/executor.rs` — deposit system op、class-2 skip 経路、handler 登録。
- `kaspa-evm/src/sim.rs` — eth_call/estimateGas simulation（同一 handler 登録）。
- `crypto/txscript/src/lib.rs` — `verify_mldsa87_with_context`（F003 再利用先、portable path）。
- `kaspa-pq-signer/src/lib.rs` — signer purpose↔digest binding（C-02、EVM 1271/op 側へ転用する規律）。
- `docs/misaka-fsl-design-v0.3.md` — reserved F003 interface案（§9.2 で version 0x01 として統一）。

### 外部標準

- NIST FIPS 204 — Module-Lattice-Based Digital Signature Standard。
- ERC-4337 — Account Abstraction Using Alt Mempool。
- ERC-1271 — Standard Signature Validation Method for Contracts。
- EIP-712 — Typed structured data hashing and signing。
- EIP-155 — chain-id replay protection。
