# MISAKA/Kaspa L1 Selected-Parent EVM 設計書 v0.4

**版:** v0.4 統合版(v0.2 Audit-Revised + v0.3 DEX/Uniswap Addendum + v0.4 §21 Scaling Addendum を単一文書に統合)
**日付:** 2026-06-10
**対象:** MISAKA/Kaspa L1 における consensus-native EVM 実行レーン。外部ブリッジなし・L2 シーケンサなし。
**関連:** [ADR-0020](adr/0020-selected-parent-evm-lane.md)(コード接地版フリーズ)、[ADR-0019](adr/0019-mldsa87-migration.md)(PQ-only 不変条件)、[ADR-0008](adr/0008-hash64-consensus-identity.md)(Hash64 consensus identity)

## 版履歴と本版の位置づけ

| 版 | 内容 | 状態 |
| --- | --- | --- |
| v0.1 | Selected-parent lane 初版(4-root header) | 廃止(v0.2 が置換) |
| v0.2 | 監査改訂: 単一 `evm_commitment_root`、lazy chain-context validation、2 段階 deposit、withdraw=tx-revert 原則、consensus-safety 6 規則 | 本版に統合 |
| v0.3 | DEX addendum: WMISAKA `0xF001`、withdraw precompile `0xF002`、chain_id/EIP-155、fork 方針(Shanghai 基線)、eth_* RPC | 本版に統合 |
| v0.4 Draft §21 | Scaling addendum: mergeset delayed acceptance、並列実行、deferred root、validity-proof lane | **本版に統合(置換含む)** |

**v0.4 の最重要変更:** v0.3 までの「block B が自身の payload を即時実行する」モデルを廃止し、**mergeset delayed acceptance**(UTXO acceptance と同型)に統一する(§3)。これに伴い header commitment は 2 フィールド化(§4)、skip semantics 5 分類(§6)、timestamp clamp の非減少化(§5.3)が consensus 規則となる。v0.3 の核心資産 — **EvmResult は block hash に対して不変、reorg は pointer 切替のみ** — は全段階で維持する。

---

## §0 結論

EVM レーンは次の 4 段階で構成する。

- **Stage 1(v0.4 hard fork):** EVM user tx の包含を全 DAG block へ開放し、実行を chain block の **mergeset delayed acceptance** として定義する。包含レイテンシは DAG block 間隔(10 BPS で約 100ms)になる。
- **Stage 2(v0.4 実装目標、consensus 変更なし):** GHOSTDAG 順序を固定したまま optimistic 並列実行(Block-STM 系)で実行する。結果は逐次実行と bit-exact に一致しなければならない。
- **Stage 3(v0.5 fork-gated):** state root commitment を k-block 遅延させ、merkle 化を実行クリティカルパスから外す(deferred commitment)。
- **Stage 4(M14+ research):** enshrined validity-proof(zkEVM)レーン。公称 TPS の単一数値競争はこの段階の対象とし、Stage 1–3 では約束しない。

性能目標の正式文言(§18 の主張規律に従う):

> MISAKA は Sui の owned-object lab TPS とは競争しない。UTXO レーンが owned-object/fastpath 相当、EVM レーンが shared-state/consensus path 相当である。設計目標は、EVM-equivalent な shared-state workload において、Sui の shared-object 的コンテンション領域を上回る実効性能を出すことである。297k TPS 級の単一数値競争は、全ノード再実行 EVM ではなく validity-proof レーン(Stage 4)の研究対象とする。

## §1 目的・非目的

### 1.1 目的

- Ethereum EVM を L1 consensus の一部として実行する(外部ブリッジ・L2 シーケンサなし)。Kaspa/MISAKA の DAG consensus と UTXO 台帳は不変に保つ。
- EVM user tx の包含を DAG 全幅に開放し、包含レイテンシと検閲耐性を改善する(Stage 1)。
- EVM 実行スループットを、低〜中コンテンション workload で Ethereum L1 比 1〜2 桁引き上げる(Stage 2–3)。
- 「EvmResult は block hash に対して不変、reorg は pointer 切替のみ」を全 Stage で維持する。
- UTXO レーンの mass 予算・伝播特性・DAG 安定性(λ·D_max ≲ k)を不変に保つ。
- 性能主張を「仕様保証値」と「技術目標値」に分離し、benchmark matrix で裏付ける。
- PQ-only 不変条件([ADR-0019])との整合: EVM レーンは独立 signature domain とし、既定ノードビルドは secp256k1-free を維持する(§20)。

### 1.2 非目的

- 単一 hot pool / 単一 hot contract のコンテンション限界を超えること(逐次等価性の物理限界)。
- Ethereum L1 の block validity 規則との完全互換。本設計は **EVM 実行環境互換** を宣言する(§5.6)。
- Stage 1–3 における公称 TPS の単一数値競争。
- UTXO model の変更、または UTXO tx の EVM acceptance への混入。
- PQ-EVM(secp なし EVM)。明示的にスコープ外。

## §2 アーキテクチャ概要

### 2.1 二レーン構造

```
UTXO レーン: ML-DSA-87 PQ-only。DAG-inclusive acceptance。10 BPS。規則・性能とも不変。
EVM レーン:  secp256k1/ECDSA(独立 signature domain)。selected-parent chain 上の
             mergeset delayed acceptance(§3)。UTXO とは予算・gossip・I/O を完全分離(§14)。
```

### 2.2 EVM parent 規則(v0.2 から不変)

DAG block `B` の EVM parent は GHOSTDAG の **`selected_parent(B)`** である — 直接 parent 集合全体ではなく、現在の virtual selected parent でもない。

```
EVM_PARENT(B) = selected_parent(B)
EvmResult(B)  = ExecuteEVM(
  parent_state = EvmStateRoot(selected_parent(B)),
  system_ops   = B.evm_payload.system_ops,          // B 自身の chain-block-scoped ops
  user_txs     = AcceptedEvmTxs(B),                  // mergeset(B) の payload(§3)
  env          = EvmBlockEnv(B)                      // §5
)
```

この単一規則の帰結:

- `EvmResult(B)` は **B の parents と B.system_ops のみの関数**(invariant I2)。一度計算され `block_hash` で保存され、virtual reorg で**再実行されない**。
- virtual 変更時の EVM 処理は **canonical EVM head pointer の切替のみ**(`latest` / `safe` / `finalized`、Stage 3 以降 `misaka_verified` 追加)。hot path に `execute_evm` / `revert_evm` は存在しない。
- EVM tx が実行されるのは、その payload を包含した block が **chain block の mergeset に取り込まれたとき**(§3)。UTXO tx は既存の DAG-inclusive acceptance を維持する。この非対称は意図的である。
- UTXO ↔ EVM の価値移動は in-consensus の **deposit / withdraw** 機構で行い、結合 native-coin supply を保存する(§9)。

### 2.3 Lazy chain-context validation(v0.2 から不変)

EVM commitment の検証は body validation では行わず、**block が selected chain の chain block(候補)になった時点**で行う(virtual processor の chain-walk 内)。chain block にならない block の EvmResult は計算されない。commitment mismatch は当該 block の `StatusDisqualifiedFromChain`(chain 候補からの失格)であり、block 自体の DAG 上の存在・UTXO 検証には影響しない。

### 2.4 循環依存禁止規則(v0.2 §4.2 から不変)

現在の L1 block hash・現在の EVM block hash は EVM 実行環境の入力にしてはならない(header hash が EVM 結果を commit するため循環する)。`blockhash` / `prevrandao` は `selected_parent` ancestry のみから導出する(§5.4)。

## §3 実行モデル — Mergeset Delayed Acceptance(Stage 1)

### 3.1 設計判断 D1: Kaspa 準拠 delayed acceptance を採用する

v0.3 は「B 自身の evm_payload を B で実行する」即時実行モデルだった。**v0.4 はこれを廃止し**、UTXO acceptance と完全同型の **delayed acceptance** に統一する。

```
mergeset(B) = selected_parent(B) ∪ (anticone(selected_parent(B)) ∩ past(B))

AcceptedEvmTxs(B) = concat(
  for X in sorted_mergeset(B):                       // 既存 consensus の mergeset order
    X.evm_payload.transactions in payload order      // blues と reds の両方を対象にする
)
```

**B 自身の user payload は EvmResult(B) に含まれない。** B の payload は B の selected child が accept する。これが off-by-one の正規解決である。

正当性は mergeset の分割性に依拠する: selected chain 上の連続する chain block の mergeset は互いに素であり、その和は DAG の past を被覆する。したがって全 payload は **exactly-once** で実行される。二重実行も取りこぼしも構造的に発生しない(テスト Y1/Y2)。

追加の利点: AcceptedEvmTxs(B) は B の parents 選択のみで確定するため、producer は**自分の payload を組む前に** mergeset acceptance の実行と commitment 計算を終えられる。EVM 実行が PoW grinding・payload 選択と分離され、template pipeline が単純化する(§15)。

**v0.3 の即時実行モデルと本モデルを混在させてはならない。** v0.3 §2/§6 の該当規則は本節で置換された。

### 3.2 System ops の扱い

system_ops(DepositClaim、§9.2)は **B 自身の payload に留まり、B で実行される**。user payload と異なり producer-selected かつ selected_parent(B) view で検証されるため、delayed acceptance の対象にしない。

実行順(v0.3 §7.4 から不変): **system_ops(claim credit)→ accepted user txs**(テスト Y13)。

### 3.3 Red block の扱い

AcceptedEvmTxs(B) は mergeset の **blues と reds の両方**を対象とする(既存 mergeset order に従う)。red payload の acceptance 除外オプションは DAA/セキュリティ評価後の open decision O11 とする。

## §4 Commitment 構造

### 4.1 Header フィールド(v0.4 で 2 フィールド化)

delayed acceptance では、block は「自身の payload(データ)」と「mergeset acceptance の実行結果」の **2 つを別個に commit** する必要がある。

```rust
pub struct Header {
    // 既存 fields ...
    pub evm_payload_hash: Hash64,      // B 自身の evm_payload バイト列の commitment(データ)
    pub evm_commitment_root: Hash64,   // EvmExecutionHeader(B) の commitment(acceptance 実行結果)
}
```

両フィールドは Hash64(64 バイト)とし、keyed BLAKE2b-512 hasher で domain 分離する(文字列 prefix 連結ではない)。シリアライズは Borsh とする。

```
EvmPayloadHash(B)    = keyed_blake2b_512(key = b"EvmPayload64",    Borsh(B.evm_payload))
EvmCommitmentRoot(B) = keyed_blake2b_512(key = b"EvmCommitment64", Borsh(EvmExecutionHeader(B)))
```

header 増分は 128 バイトであり、header 肥大回避の方針(v0.3 M1)とは緊張するが、データ commitment と実行 commitment の分離は delayed acceptance の構造的要請であり正当化される。

**二層ハッシュ規約:** EVM 内部 root(state_root / receipts_root / transactions_root 等)は keccak-256 由来の 32 バイト(`EvmH256`)のままとする。consensus commitment は Hash64 / keyed BLAKE2b-512 とする。この「EVM 内部 = keccak-256/32B、consensus commitment = Hash64/keyed BLAKE2b」の二層規約が正式仕様である(監査 K-2 解決)。

### 4.2 EvmExecutionHeader(body 側実行ヘッダ)

```rust
pub struct EvmExecutionHeader {
    // v0.3 fields(state_root, transactions_root, receipts_root, logs_bloom,
    // gas_used, base_fee_per_gas, evm_number, evm_timestamp_sec, ...)に加えて:
    pub coinbase: EvmAddress,                  // 監査 AM-3: COINBASE opcode の返り値(§8.2)
    pub accepted_tx_count: u32,                // accept した user tx 数(skip 含まず)
    pub skipped_tx_count: u32,                 // deterministic skip された tx 数
    pub evm_total_native_balance: U256,        // 監査 AM-5: O(1) supply invariant 検証用累積器
    pub evm_burn_accumulator: U256,            // v0.3 から維持(basefee burn 累積)
}
```

`transactions_root` は **accept され実行された tx のみ** の ordered root とする。skip された tx は execution result に痕跡を残さない(receipts にも含めない)。

### 4.3 Version gating(genesis 不変性の根拠)

EVM header フィールドは `Header` 構造体に常時存在する(ゼロ既定)が、header-hash preimage に入るのは **`header.version >= EVM_HEADER_VERSION`(=2)のときのみ**。genesis header は v0、既存 mined block は v1(いずれも < 2)であるため、preimage と全 digest(legacy-32 / identity-64 / pre-PoW-64)は pre-EVM プロトコルと byte 単位で同一である。`test_genesis_hashes` は定数変更なしで green を維持しなければならない(`merkle::*_pre_crescendo` の version-gating 前例に倣う)。

v≥2 の preimage には `pruning_point` の後に `evm_payload_hash(64)` → `evm_commitment_root(64)` の順で追記する(frozen byte order)。header 変更は pre-PoW hash 順序定義と `hash64-migration-inventory` の更新を伴う(監査 K-11)。

on-disk consensus header は bincode 直列化のため、フィールド追加は `LATEST_DB_VERSION` の bump(旧 DB は open 時拒否 → clean resync、[ADR-0001] 準拠)を伴う。

## §5 EVM 実行環境(EvmBlockEnv)

### 5.1 凍結パラメータ

| 項目 | 値 | 備考 |
| --- | --- | --- |
| `EVM_HEADER_VERSION` | `2` | genesis v0 / 現行 v1 を超える。下げてはならない |
| `EVM_CHAIN_ID` | `0x4D534B`("MSK") | EIP-155。全公開 Ethereum net と非衝突。mainnet 最終値は launch 時決定 |
| EVM fork | revm `SpecId::SHANGHAI` | London+ 基線(Uniswap v2/v3 + 現行 solc が動く)。Cancun/EIP-1153(v4 用)は後続 fork(§19.2 判断)。upstream 自動追従禁止、bump = hard fork |
| `EVM_NATIVE_SCALE` | `10^10` | sompi(8 桁)→ wei(18 桁)。withdraw は正確な倍数のみ |
| `EVM_GENESIS_STATE_ROOT` | `keccak256(rlp(()))` 空 trie root | 空 block が再現することを executor がアサート |
| EVM gas 上限 | `MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK` = `G_limit_block` | §7。per-second 導出(v0.3 §5.2)と同一化(監査 AH-2) |
| EIP-1559 | elasticity 2 / max-change-denominator 8 / initial basefee 1 gwei | 既定値。activation 前に再校正可 |
| Withdraw precompile | `0x…F002`(`MISAKA_WITHDRAW`) | §9.3 |
| WMISAKA predeploy | `0x…F001` | §19.1 |
| Subnetwork ids | `0x20` deposit / `0x21` withdraw-claim(予約) / `0x22` admin(予約) | |
| 活性化 | `Params::evm_activation_daa_score` | **testnet = 0(2026-06-11 genesis-active 化、re-genesis cutover・genesis hash 不変)**、devnet = 0。mainnet/simnet = `u64::MAX`(inert)。activation 以降の header は v2(`EVM_HEADER_VERSION`) |

### 5.2 evm_number

`evm_number(B) = evm_number(selected_parent(B)) + 1`。selected chain 上で連続し、chain block のみが EVM block number を持つ。

### 5.3 設計判断 D6: Timestamp clamp の非減少化(v0.3 §4.1 を置換)

v0.3 の厳密単調 clamp(parent + 1)を **非減少 clamp に置換する**。

```
evm_timestamp_sec(B) = max(header_ts_sec(B), evm_timestamp_sec(selected_parent(B)))
```

厳密単調 clamp は chain block rate > 1/s のとき論理時刻を壁時計から無限乖離させる(監査 AH-3)。非減少 clamp ではドリフトは timestamp deviation tolerance の範囲に有界化され、**EVM 設計と BPS の結合(監査 K-3 の re-genesis 前提)が解消される**。現行 10 BPS パラメータでも timestamp 健全性は成立する(容量・伝播の観点での BPS 選択は §14.3 に従い独立判断)。

Uniswap v2 は `timeElapsed > 0` のときのみ累積価格を更新し、v3 oracle は同一 timestamp で early return するため、等値 timestamp は v2/v3 TWAP と互換である(テスト Y7)。開発者文書に次を明記する:

```
Multiple consecutive EVM blocks may share the same block.timestamp.
Use block.number for per-block sequencing.
Use block.timestamp only as lower-resolution wall-clock time.
```

### 5.4 prevrandao / blockhash

§2.4 の循環依存禁止に従い、`prevrandao(B) = keyed_blake2b(PREVRANDAO domain, selected_parent_hash ‖ blue_work ‖ daa_score)` など **selected_parent ancestry のみ**から導出する。`BLOCKHASH(n)` は selected chain 上の ancestor の EVM block hash を返す。

### 5.5 basefee

EIP-1559 を selected chain 上で適用する(parent の `base_fee_per_gas` + gas_used から決定論的に導出)。basefee は **burn** する(§8.1)。

### 5.6 互換性宣言

本設計は **EVM 実行環境互換**(EVM execution-environment compatible)であり、Ethereum L1 の block validity 互換ではない。既知の開示済み差分: per-tx priority-fee 受取人(§8.2)、等値 timestamp(§5.3)、delayed acceptance による receipt の所在(§16)、Hash64 consensus commitment(§4.1)。

## §6 Validity と Skip Semantics

### 6.1 設計判断 D5: Skip semantics の 5 分類

delayed acceptance では accepting block の producer は payload の内容を選択していない。したがって監査 AC-1 の旧解決(intrinsic invalid → block invalid)は**本モデルでは適用できず、本節が AC-1 を正式に置換する**(§22)。分類は次の 5 クラスとする。

| クラス | 条件 | 扱い | 帰責 |
| --- | --- | --- | --- |
| 1. Payload admission 違反 | RLP/typed encoding 不正、signature 形式不正、chain_id 不一致、tx gas_limit < intrinsic **固定床**(transfer 21k / create 53k)、tx サイズ超過 | **payload block が body validation で invalid**(syntactic check、安価) | payload block producer(自分の payload は自分の責任) |
| 2. Acceptance-time skip | nonce 不一致、balance < upfront cost(gas_limit×max_fee + value)、max_fee < basefee(B)、gas_limit < calldata 込み真 intrinsic(下注) | **deterministic skip**: 実行なし、receipt なし、nonce 不変、gas 課金なし | 誰の fault でもない(basefee(B) は payload 作成時に未知) |
| 3. Duplicate | 同一 tx hash が既に accept 済み | クラス 2 の nonce 規則で自動的に skip。receipt は最初の accepted occurrence を指す | — |
| 4. 実行時失敗 | EVM revert、OOG、precompile validation 失敗(F002 user-input fault 含む §9.3)、slippage、hook revert | **実行済み**: receipt status = 0、gas 課金あり(v0.3 §6.3 から不変) | user |
| 5. Accepted gas cap 超過 | §7 の cap を超えた canonical order 末尾 | **deterministic prefix skip**(D4)。nonce 不変、後続 block で再 accept 可能 | — |

クラス 2/3/5 の skip は EvmResult に不可視である(state、receipts、gas_used のいずれにも影響しない)。`skipped_tx_count` のみが統計として commit される。クラス 2 の skip は nonce を変えないため、同一 tx は後続の accepting block で条件が満たされれば accept される — duplicate 排除はこの nonce 規則から自然に導かれ、専用の seen-set を必要としない。

注(クラス 1/2 境界の正準化、監査 L6): body admission が強制する intrinsic 下限は**固定床**(transfer 21,000 / create 53,000)であり、calldata・initcode 依存の EIP-2028/3860 真 intrinsic ではない — 真 intrinsic の再実装は revm との計算乖離 = consensus split リスクを生むため意図的に避ける。`固定床 ≤ gas_limit < 真 intrinsic` の tx は admission を通過し、acceptance で revm が決定的に拒否してクラス 2 skip となる(全ノード同一 revm のため決定的・安全側: 境界 tx は payload block を invalid にせず無害に skip)。

### 6.2 Block invalid / 失格の到達条件

chain 候補の失格(`StatusDisqualifiedFromChain`)・block invalid に到達するのは次のみ:

1. commitment / root mismatch(`evm_commitment_root` ≠ 再実行結果、または `evm_payload_hash` ≠ payload)
2. system op の consensus rule 違反(claim 上限超過、二重 claim、不一致 claim — §9.2)
3. UTXO diff 不整合(withdraw synthetic output / deposit lock 消込の不一致 — §9)
4. **クラス 1 を含む payload block 自身の syntactic 違反**(body validation、包含時)

accepting producer が選択していない payload 内容によって block が invalid になることは**ない**(クラス 2/3/5 は skip)。

## §7 ガス・予算 — 設計判断 D4: 二段階 cap と over-cap policy

per-DAG-block の payload cap だけでは、大きな mergeset を merge した chain block の実行予算が無制限になる。二段階 cap を導入する。

```
MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK    // 包含側: payload block の body validation で検査
MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK   // 実行側: G_limit_block(per-second 導出 v0.3 §5.2)と一致させる
```

実行側 cap の適用は **canonical order の deterministic prefix-take** とする: AcceptedEvmTxs(B) を順に走査し、tx の gas_limit を加算した累計が cap を超える最初の tx 以降を全てクラス 5 skip とする。**gas_limit 基準で判定する**ことで、実行前に accept 集合が確定し、並列スケジューラ(§11)への入力が固定される(テスト Y6)。

carry(次 block への持ち越し queue)は採用しない。carry は queue root の state 遷移化を要求し、Stage 1 の複雑性を不当に上げる。skip された tx は nonce 不変なので payload 再包含で自然に救済される(テスト Y15)。carry の再検討は open decision O7。

## §8 手数料モデル — 設計判断 D3

### 8.1 構造

```
basefee:        burn(v0.3 §9.2 から不変、evm_burn_accumulator に累積)
priority fee:   tx t ごとに evm_coinbase(payload_block(t)) へ credit
COINBASE opcode: evm_coinbase(B)(accepting chain block の宣言アドレス)を返す
EIP-3651:       warm 対象は accepting coinbase。per-tx beneficiary は各 tx 実行開始時に warm 化
```

選択肢 A(accepting miner 総取り)/ B(payload miner)/ C(分割)のうち **B を COINBASE 定義の明確化付きで採用**した。根拠: 包含という DAG 全幅の希少資源を提供するのは payload miner であり、実行は chain candidacy の必須条件(lazy validation)なので accepting producer に追加インセンティブは不要。C 案への将来移行は O10。

### 8.2 evm_coinbase 宣言

各 DAG block は evm_payload 内に `evm_coinbase: EvmAddress` を宣言する(`evm_payload_hash` が commit する)。帰結:

- 1 つの EVM block 内で tx ごとに priority fee の受取人が異なる。Ethereum との差分として開発者文書に明記する(§5.6)。
- `block.coinbase` へ直接送金するコントラクト(MEV bribe 等)は accepting miner に支払う。EVM env の一貫性(1 block = 1 coinbase)は保たれる。
- RPC: `eth_getBlockByNumber.miner` = accepting coinbase。per-tx beneficiary は receipt 拡張フィールド `misaka_feeRecipient` で公開する。

deposit claim のインセンティブ(監査 AH-1)はこのモデルでも解決しないため、`claim_tip_sompi` を別途維持する(§9.2)。

## §9 UTXO ⇔ EVM ブリッジ(deposit / withdraw)

### 9.1 供給保存の不変条件(v0.3 §9.2 から不変)

```
UTXO_balances + EVM_deposit_locks + EVM_native_balances + burned == issued
```

検証は `evm_total_native_balance` 累積器(§4.2)により O(1) で行う(監査 AM-5 解決)。priority fee 移転 + burn + 残高変化も本 invariant を満たす(invariant I6)。

### 9.2 Deposit(2 段階、v0.2 から不変)

1. **Lock:** user が UTXO レーンで `EVM_DEPOSIT_LOCK` output(subnet `0x20`、`ScriptClass::EvmDepositLock`)を作る。lock は宛先 `EvmAddress`・`timeout_daa_score`・`claim_tip_sompi`(claim 包含インセンティブ、AH-1)を持つ。
2. **Claim:** producer が `DepositClaim` system op を自 block B の payload に入れる。claim は **selected_parent(B) の UTXO view** で検証される(同一 block 内の lock は claim 不可)。成功時、宛先 `evm_address` に `(amount_sompi − claim_tip_sompi) × EVM_NATIVE_SCALE` を credit し、`claim_tip_sompi × EVM_NATIVE_SCALE` を accepting block B の `evm_coinbase` に credit する(AH-1 の包含インセンティブ分割、供給中立)。lock UTXO は B の per-block UTXO diff で消込む。consensus は作成時に `claim_tip_sompi ≤ value` を強制する(監査 F3: claim 不能な lock の発行を拒否)。
3. **Refund:** timeout 経過後は user が lock を通常 spend で回収できる。**排他ウィンドウ(監査 AC-2): claim valid iff accepting_block_daa_score < timeout_daa_score**。claim と refund が同時有効になる DAA 領域は存在しない。

有界性: claim 数 ≤ `MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK`(256)、bytes ≤ 64 KiB、system gas ≤ `MAX_SYSTEM_GAS_PER_EVM_BLOCK`(10M、@25,000 gas/claim)。違反は §6.2-(2) の失格。

### 9.3 Withdraw(F002 precompile)

accepted user tx が precompile `0x…F002` を呼ぶ: `withdraw(bytes script_public_key, uint256 amount_wei)`。

- **user-input fault**(calldata 不正 / amount==0 / `amount_wei % EVM_NATIVE_SCALE != 0` / script 規則違反 / 残高不足)→ **tx revert(クラス 4)**。block は valid のまま。
- 成功 → EVM 残高 debit + `WithdrawOp` emit。

**materialization(v0.3 §8.3 invariant を delayed acceptance 下で維持):** WithdrawOp を emit するのは accepted user tx(payload は他 block 由来)だが、**実行は accepting block B で行われる**ため、synthetic UTXO output は **B 自身の per-block UTXO diff に materialize** される(invariant I7、テスト Y12)。outpoint は frozen domain(`MISAKA_EVM_SYNTHETIC_OUTPOINT_V2` 系)から決定論的に導出し、同一 block 内では unspendable。per-block diff に載るため **reorg 時の巻き戻しは既存 reversed-diff 経路で自動**である。

## §10 Reorg 処理(invariant、v0.2 から不変)

- virtual 変更時の EVM 処理は **pointer 切替のみ**(invariant I3)。EvmResult(B) は B の parents の関数なので、どの chain に乗っても再計算不要(テスト Y11)。
- canonical heads: `latest`(現 sink 系)/ `safe` / `finalized`(blue_work confirmation depth・DNS-finality 連動)。Stage 3 で `misaka_verified` を追加(§12)。
- USDC/CEX 級の用途は `finalized` を参照する。

## §11 Stage 2: Serial-Equivalent Optimistic Parallel Execution

consensus 変更を伴わない実装最適化。仕様としての要求は**逐次等価性のみ**である。

```
canonical_order  = AcceptedEvmTxs(B)(§3.1 の確定順序、prefix-take 適用後)
parallel_result  MUST equal serial EVM execution in canonical_order
conflict granularity = account / storage-slot level
failed speculative execution MUST be invisible
gas_used / refunds / logs / receipts MUST equal serial execution(bit-exact)
```

EIP-2930 access list は **scheduler hint(prefetch / 依存推定)としてのみ**使用する。追加の gas 割引は fork rule になるため Stage 2 では導入しない(O8)。

性能期待値は workload 依存である: 低コンテンション(独立 sender の transfer / ERC-20)で 16–32 thread あたり 10–20 倍、単一 hot pool・同一 vault 連打では伸びない。表記は §18 の主張規律に従う。検証はテスト Y8(parallel == serial fuzz)。

## §12 Stage 3(v0.5 fork-gated): Deferred State Root Commitment

Stage 3 は v0.4 に**含めない**。execution・RPC・state sync・light client・fee market を同時に変更するため、独立 fork とする。本節は v0.5 設計の要求事項を確定する。

### 12.1 モデル

Monad の delayed Merkle root と同系だが、本設計では **execution は引き続き chain-context validation 時に行い、遅延させるのは root の merkle 化と commitment のみ**(deferred commitment)。Monad が consensus を execution に先行させるのと異なり、gas 支払可能性は acceptance 時点で正確に検査済み(§6.1 クラス 2)であり、Reserve Balance 相当の機構は不要である。この差分は仕様に明記する。

```
EvmExecutionHeader(B).state_root = state_root(ancestor(B, k))   // k-lag commitment
```

### 12.2 ノード状態と RPC

```
canonical_tip: GHOSTDAG 上の現在 canonical chain head
executed_tip:  local node が EVM 実行済みの先端
verified_tip:  k-lag root で検証済みの先端 = canonical_tip − k
```

`misaka_verified` tag を導入し、state proof が k block 遅れることを RPC 仕様と light client 文書に明記する。root mismatch の発見が k block 遅延するため、mismatch 検出時の rollback / branch disqualification protocol(`StatusDisqualifiedFromChain` への遡及適用)を定義する(テスト Y14)。

### 12.3 State backend

trie ネイティブ store を廃し、flat KV + 非同期 merkleization(Erigon/MonadDB 系)へ移行する。G_target_sec の引き上げ可能量は次の関係式に拘束されることを ADR に明記する。

```
G_target_sec_max = parallel_replay_speed / safety_margin(10–20x)
```

replay 速度(anchor → tip、IBD)は benchmark workload 10(§18.2)で実測し、G_target_sec 改定の根拠とする。

## §13 Stage 4(M14+ research): Enshrined Validity-Proof Lane

公称 TPS の単一数値競争を行う唯一の経路として、全ノード再実行を proof 検証に置換する zkEVM レーンを研究項目とする。research ADR は次を数値目標として扱わなければならない(スローガン化の禁止)。

```
proof latency:        realtime proving 目標 ≤ 10s 級(EF L1 zkEVM 目標に準拠)
security:             128-bit、proof size ≤ 300 KiB 級
prover liveness:      proof 停止時に verified finality が止まる。full execution fallback の有無を仕様化
prover decentralization: 専用 GPU/ASIC/cluster 依存の集中リスク評価
DA:                   tx data 可用性が state 再構成の前提。DAG 包含を DA として使う場合の保持要件
forced inclusion:     prover/sequencer 検閲への対抗経路
```

## §14 UTXO レーン保護(予算分離と伝播安全性)

### 14.1 予算の完全分離

EVM payload を `max_block_mass`(500,000)の共有予算に入れてはならない(監査 K-10 解決)。次の予算を独立に管理する。

```
UTXO mass budget               // 既存。不変
EVM payload bytes budget       // MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK
EVM accepted gas budget        // MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK
P2P bandwidth budget           // gossip 優先度で保護(§14.2)
disk write budget              // EVM state DB の flush/compaction を UTXO-set I/O と別 queue 化
mempool RAM budget             // UTXO mempool / EVM mempool の上限を独立設定
```

### 14.2 ネットワーク規則

```
EVM payload gossip は UTXO tx / block header gossip より低優先度とする。
UTXO block validation path は EVM execution の完了を待ってはならない。
EVM payload relay は signature / basic format precheck 通過後のみ行う(クラス 1 の事前遮断)。
過大 mergeset 時も実行予算は MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK で有界(D4)。
```

### 14.3 伝播安全性の再検証

EVM payload は block サイズを増やし、伝播遅延 D を押し上げ、W_eff と orphan/merge pressure に跳ねる。テストネットで確立した安全不等式に payload 追加後の実測 D を代入して再検証することを **activation の前提条件**とする。

```
λ · D_max(payload 込み実測値) ≲ k
```

検証は benchmark workload 10 および専用の伝播試験(Y10)で行い、不等式を満たさない場合は `MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK` を縮小する。

## §15 Mempool / Block template

```
build_template(parent_tip):
  1. choose selected_parent candidate P and parents
  2. compute AcceptedEvmTxs(B) from mergeset(B)         // parents のみで確定
  3. execute acceptance(system_ops + accepted txs)      // EVM 実行は payload 選択と独立
  4. build EvmExecutionHeader / evm_commitment_root
  5. build UTXO tx list(従来通り)
  6. select own evm_payload from EVM mempool            // 包含のみ。実行しない
  7. compute evm_payload_hash
  8. assemble header / body and mine PoW
```

EVM mempool は skip 救済を前提に再包含(rebroadcast)を実装する: クラス 2/5 で skip された tx は drop せず、nonce が有効である限り再包含候補に保持する。

## §16 RPC

```
eth_getTransactionReceipt(h):
  h が accept・実行済み      -> receipt(accepting chain block を blockHash として返す)
  h が包含済みだが未 accept   -> null(latest 基準)。misaka_getTxInclusionStatus で DA 包含を確認可能
  h が skip された           -> null。skip は consensus 上の最終状態ではない(再 accept があり得る)

misaka_getTxInclusionStatus(tx_hash) -> {
  included_in: [payload block hashes],     // DA visibility
  accepted_in: chain block hash | null,    // execution receipt の所在
  skip_class:  2 | 5 | null,               // 直近 acceptance 試行での skip 理由(参考情報)
}
```

`included_in` の存在は実行を保証しない。wallet UI は §18.1 のレイテンシ 3 層(included / accepted / safe / finalized)を区別して表示する義務を負う。`eth_*` 標準面(getLogs、subscriptions、call/estimateGas 等)は v0.3 の RPC 章を維持し、block tag に `safe` / `finalized`(Stage 3 以降 `misaka_verified`)を追加する。

## §17 Consensus invariants

```
I1: mergeset 分割性 — 任意の selected chain について、各 EVM user tx は高々 1 回実行される。
I2: EvmResult(B) は B の parents・B.system_ops のみの関数である(B の user payload に依存しない)。
I3: virtual 変更時の EVM 処理は pointer update のみ(v0.2 から不変)。
I4: skip(クラス 2/3/5)は state / receipts / gas_used / nonce に影響しない。
I5: 並列実行結果は canonical_order の逐次実行と bit-exact に一致する。
I6: priority fee の合計 + burn + 残高変化は supply invariant(§9.1)を満たす。
I7: withdrawal synthetic output は accepting block B 自身の UTXO diff にのみ現れる。
I8: evm_timestamp_sec は selected chain 上で非減少である。
I9: 現在の L1 block hash / EVM block hash は EVM 実行環境の入力にならない(§2.4)。
I10: 既定ノードビルド(evm feature OFF)は secp256k1 を一切リンクしない(§20)。
```

## §18 性能主張の規律(Performance Claims Policy)

### 18.1 仕様保証値と技術目標値の分離

**仕様保証値**は consensus パラメータから導出可能なもののみ(G_target_sec、cap、レイテンシ 3 層)。**技術目標値**は benchmark matrix の実測を引用条件付きで示す。

レイテンシは「包含 100ms」の単独広告を禁止し、次の 3 層に分離して表記する:

```
data inclusion / DA visibility:  ~ DAG block 間隔(10 BPS で約 100ms)
EVM execution receipt:           accepting chain block への到達(≈ selected-chain interval)
probabilistic finality:          blue_work confirmation depth 依存(safe / finalized tag)
```

RPC・公式 UI・マーケティング資料はこの 3 層を区別する義務を負う。

| 項目 | 区分 | 値 | 条件 |
| --- | --- | --- | --- |
| DA 包含レイテンシ | 仕様保証 | ≈ DAG block 間隔 | BPS 設定依存 |
| receipt レイテンシ | 仕様保証 | ≈ selected-chain interval | W_eff 依存 |
| Stage 2 実行スループット | 技術目標 | 20–100 Mgas/s | 低〜中コンテンション、workload 1–3/5 |
| Stage 3 実行スループット | 技術目標 | 100–500 Mgas/s | flat state backend、NVMe、replay margin 充足 |
| 送金 TPS | 技術目標 | gas/s ÷ 21,000 | 多数独立 sender |
| Uniswap swap TPS | 技術目標 | **多数 pool 分散時のみ** gas/s ÷ 120–150k | 単一 hot pool では保証しない |

禁止事項: 単一 hot pool / 単一 hot object の数値を一般 TPS として表記すること。Sui 比較は §0 の正式文言のみを使用し、「Sui の shared-object は桁で落ちる」という一般断定をしてはならない — 限定は「単一 hot shared object、DEX pool、清算 queue のような高コンテンション shared-state」に対してのみ行う。

### 18.2 Benchmark matrix(ADR 必須要件)

性能主張はすべて次の 10 workload の実測に紐づける。

```
 1. native transfer(多数独立 sender/receiver)
 2. ERC-20 transfer, many independent senders/receivers
 3. ERC-20 transfer, hot token contract but disjoint balances
 4. Uniswap v2 single hot pool
 5. Uniswap v2 many pools
 6. Uniswap v3 concentrated liquidity, many ticks
 7. liquidation / vault hotspot
 8. MEV bundle 様の依存 tx 列
 9. adversarial スパム(クラス 2 skip flood)— skip 処理コストと payload 予算の攻撃耐性
10. reorg / anchor→tip replay — G_target_sec 上限と Stage 3 の前提検証
```

## §19 DEX / アプリケーション互換(v0.3 から維持)

### 19.1 WMISAKA

`0x…F001` に WMISAKA(WETH9 互換 wrapped native)を predeploy する。deposit/withdraw は native EVM 残高と等価交換。

### 19.2 Fork 方針

初期 fork は **revm `SpecId::SHANGHAI`**(PUSH0 + 現行 solc 出力 + Uniswap v2/v3 が動く)。Uniswap v4(EIP-1153 transient storage)・blob 系は **Cancun を別 fork として後日 activation** する。upstream への自動追従はしない(fork bump = hard fork)。

### 19.3 TWAP / oracle 注意

§5.3 の等値 timestamp 許容により、v2 `timeElapsed > 0` ガード・v3 observation early-return が前提どおり機能する(テスト Y7)。oracle 連携文書に `block.number` ベースの sequencing を推奨として記載する。

## §20 PQ-only との整合(secp 開示)

EVM レーンは secp256k1/ECDSA(ecrecover + 署名検証)を**独立 signature domain** として再導入する。native UTXO レーンの ML-DSA-87 PQ-only 不変条件([ADR-0019])との整合は次で担保する:

- EVM の**型**(consensus-core)は常時コンパイルされ secp-free。
- EVM の **executor**(revm/k256)は cargo `evm` feature(default OFF)の背後。既定 `kaspad` は secp-free のまま(`scripts/pq-ci-guard.sh` が 9 production binaries で強制)。
- `--features evm` ビルドのみ EVM レーンを実行できる。evm-active なネットに non-evm バイナリで参加した場合は silent fork ではなく明示的に停止する。
- EVM レーンの量子リスクは Ethereum 本体と同等であることを利用者向けに開示する(PQ 保証は UTXO レーンのみ)。

## §21 実装状況と v0.3 実装からの移行

2026-06-10 時点で `pr-19-s5f-…` に **v0.2/v0.3 ベースの P0–P3 が実装済み**(ADR-0020 参照): 単一 `evm_commitment_root` の version-gated header(P1)、revm Shanghai executor `kaspa-evm`(P2)、state snapshot + append-only chaining + consensus stores + `evm_validate_and_persist` driver(P3 データ層)。全ネット `evm_activation_daa_score = u64::MAX`(inert)。

v0.4 が既実装に要求する差分(activation 前の改修であり、inert のため既存ネット影響なし):

1. **Header:** `evm_payload_hash` フィールド追加 + v≥2 preimage 順序の更新(§4.3)。
2. **Commitment domain:** §4.1 の keyed domain(`EvmPayload64` / `EvmCommitment64`)へ統一(現行 `MISAKA_EVM_COMMITMENT_V2` を置換)。
3. **Executor 入力:** `user_txs` を「B 自身の payload」から `AcceptedEvmTxs(B)`(mergeset 走査 + prefix-take)へ変更。executor 本体(env/状態遷移/MPT root)は再利用可能。
4. **EvmExecutionHeader:** `coinbase` / `accepted_tx_count` / `skipped_tx_count` / `evm_total_native_balance` フィールド追加(§4.2)。
5. **Skip 分類器**(クラス 2/3/5)と **payload admission check**(クラス 1、body validation)の新設。
6. **Timestamp clamp:** 厳密単調(parent+1)→ 非減少 max() へ変更(§5.3)。
7. **Fee routing:** priority fee の payload-miner credit + `evm_coinbase` payload フィールド(§8)。

## §22 監査トレーサビリティ

| 監査 ID | v0.4 での扱い |
| --- | --- |
| AC-1(intrinsic validity) | **置換**: delayed acceptance では accepting producer が payload を選択しないため block invalid 化は不可。§6.1 クラス 1(payload block invalid)+ クラス 2(deterministic skip)が正式解決となる |
| AC-2(claim/refund 競合) | 維持: 排他ウィンドウ規則を §9.2 で再確認 |
| AH-1(claim incentive) | 維持: `claim_tip_sompi`。D3 の fee model は claim incentive を解決しない旨を明記 |
| AH-2(tau_sc 決定論) | 維持: MAX_EVM_ACCEPTED_GAS = G_limit_block の同一化により per-second 導出の厳密仕様要求は不変 |
| AH-3(timestamp drift) | **置換**: D6 非減少 clamp によりドリフトは deviation tolerance に有界化 |
| AM-3(coinbase 出所) | 解決: EvmExecutionHeader.coinbase + payload の evm_coinbase(D3) |
| AM-5(supply invariant O(1)) | 解決: evm_total_native_balance 累積器(§4.2) |
| K-2(Hash64/Borsh) | 適用: §4.1 の二層規約 |
| K-3(10 BPS 依存) | **緩和**: D6 により timestamp 起因の BPS≤1 制約は消滅。容量・伝播起因の BPS 判断は §14.3 に従い独立に行う |
| K-10(mass 統合) | 解決: §14.1 予算分離(共有 mass に入れない判断を正式化) |
| K-11(header 変更) | 適用: §4.3 に pre-PoW hash 順序・inventory 更新義務を記載 |
| K-15(activation) | open decision O9 へ: Stage 1 fork を re-genesis に同梱するか |

## §23 テスト計画

v0.3 の試験(genesis 不変・version gating・deposit/withdraw・supply invariant・no-replay)を維持した上で、v0.4 追加分:

```
Y1:  mergeset 分割性 — 合成 DAG 上で全 payload が exactly-once 実行される
Y2:  off-by-one — B の payload は EvmResult(B) に不在、selected child の result に存在
Y3:  skip 5 分類 — 各クラスが §6.1 の表の通りに処理される(クラス 1 は payload block invalid)
Y4:  duplicate — 同一 tx hash の receipt は最初の accepted occurrence を指す
Y5:  fee routing — priority fee は payload miner へ、COINBASE opcode は accepting coinbase を返す
Y6:  prefix-take 決定性 — gas cap 超過時の accept 集合が実装間で一致する
Y7:  等値 timestamp — v2 TWAP(timeElapsed == 0 スキップ)/ v3 observation(early return)が正しく動作
Y8:  並列等価性 fuzz — workload 1–8 で parallel == serial(state root / receipts / logs bit-exact)
Y9:  予算独立性 — EVM payload 満載時に UTXO スループットが低下しない
Y10: 伝播安全性 — payload 込み実測 D で λ·D_max ≲ k が成立する
Y11: reorg — pointer 切替のみで EVM 再実行が発生しない(delayed acceptance 版)
Y12: withdrawal — accepted user tx の WithdrawOp が accepting block B の diff に materialize される
Y13: 実行順 — system_ops(claim)が accepted user txs より先に適用される
Y14: (v0.5)k-lag root — mismatch 検出と遡及 disqualification が機能する
Y15: skip 救済 — クラス 2/5 skip 後の再包含・再 acceptance が成功する
```

## §24 Implementation milestones

```
M10(v0.4 fork): Stage 1 consensus
  - evm_payload_hash / evm_commitment_root(Hash64)header activation
  - mergeset acceptance executor、skip 分類、prefix-take
  - fee routing(payload miner)+ COINBASE 定義 + misaka_feeRecipient
  - 非減少 timestamp clamp
  - claim_tip_sompi(AH-1)
  - 既実装 P0–P3 の v0.4 改修(§21 の 7 項目)

M11(v0.4 実装): Stage 2 executor
  - Block-STM 系並列実行(serial-equivalence 検証付き)
  - access list prefetch hint(割引なし)
  - benchmark matrix 1–9 の CI 化

M12(v0.4 運用): 予算・ネットワーク
  - 予算分離、gossip 優先度、I/O queue 分離
  - 伝播試験(Y10)と λ·D_max ≲ k 再検証
  - mempool skip 救済 / misaka_getTxInclusionStatus

M13(v0.5 fork): Stage 3
  - k-lag deferred commitment、verified_tip RPC、flat state backend
  - mismatch rollback protocol、light client proof delay 文書

M14(research): Stage 4 validity-proof lane ADR
```

## §25 Open decisions

```
O7:  accepted gas cap 超過分の carry queue(queue root の state 遷移化)を将来導入するか
O8:  access list への MISAKA 固有 gas 割引 fork
O9:  Stage 1 fork を re-genesis に同梱するか(K-15)
O10: priority fee の C 案(P/B 分割)への将来移行可否
O11: red block payload の acceptance 除外オプション(DAA/セキュリティ評価後)
O12: Stage 4 の proof system / prover market 設計
O13: MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK / MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK の数値確定
     (G_limit_block per-second 導出 + Y10 伝播実測に基づき activation 前に凍結)
```

## §26 References

- Sui Blog, Sui Performance Update(297k TPS の測定条件). https://blog.sui.io/sui-performance-update/
- Sui Docs, Consensus(owned/shared object の実行パス分離). https://docs.sui.io/develop/sui-architecture/consensus
- Kaspa WIKI, Merging and Rewards(mergeset と accepting block モデル). https://wiki.kaspa.org/merging-and-rewards
- Uniswap v2-core, UniswapV2Pair.sol(timeElapsed > 0 条件). https://github.com/Uniswap/v2-core/blob/master/contracts/UniswapV2Pair.sol
- Aptos Documentation, Execution(Block-STM の preset order 等価性). https://aptos.dev/network/blockchain/execution
- Block-STM: Scaling Blockchain Execution(arXiv:2203.06871). https://arxiv.org/abs/2203.06871
- EIP-2930: Optional access lists. https://eips.ethereum.org/EIPS/eip-2930
- Monad Documentation, Asynchronous Execution / Block States(delayed Merkle root、Reserve Balance). https://docs.monad.xyz/monad-arch/consensus/asynchronous-execution
- ethereum.org, Zero-knowledge rollups(validity proof モデル). https://ethereum.org/developers/docs/scaling/zk-rollups/
- Ethereum Foundation Blog, Shipping an L1 zkEVM #1: Realtime Proving(proof latency / size 目標). https://blog.ethereum.org/2025/07/10/realtime-proving
- Sei Blog, Sei v2 — The First Parallelized EVM(100 Mgas/s の文脈). https://blog.sei.io/sei-v2-the-first-parallelized-evm/

## §27 最終設計判断(v0.4)

1. EVM user tx の包含は全 DAG block に開放し、実行は chain block の mergeset delayed acceptance(Kaspa 準拠、off-by-one なし)とする。
2. EvmResult(B) は parents と B.system_ops のみの関数であり、reorg 処理は pointer 切替のみという核心を維持する。
3. skip semantics 5 分類により、accepting producer が選択していない payload 内容で block が invalid になることはない。
4. priority fee は payload miner へ、COINBASE は accepting miner とし、Ethereum との差分は EVM 実行環境互換の範囲として明示開示する。
5. 実行予算は per-DAG-block bytes と per-chain-block accepted gas の二段階 cap で有界化し、超過は deterministic prefix skip とする。
6. timestamp は非減少 clamp とし、EVM 設計の BPS 結合を解消する。
7. 並列実行は逐次等価性を唯一の仕様要求とする実装最適化に位置づける。
8. deferred root(Stage 3)は v0.5 独立 fork、validity-proof lane(Stage 4)は research とし、v0.4 では公称 TPS 競争を約束しない。
9. UTXO レーンは予算・gossip 優先度・I/O queue の完全分離と λ·D_max ≲ k の実測再検証で保護する。
10. 性能主張は仕様保証値と技術目標値を分離し、benchmark matrix の実測のみを根拠とする。
11. 既定ノードビルドの secp-free(PQ-only)保証は維持し、EVM executor は `evm` feature の背後に置く。
