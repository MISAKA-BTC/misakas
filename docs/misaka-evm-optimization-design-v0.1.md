# MISAKA EVM レーン最適化設計書 v0.1

**版:** Draft v0.1 — EVM Lane Scalability Baseline & Optimization Tracks
**日付:** 2026-06-11
**位置づけ:** [EVM 設計書 v0.4](misaka-evm-design-v0.4.md)(ADR-0020)の実装最適化編。v0.4 の凍結ルール(§3 mergeset delayed acceptance / §4 commitment / §7 二段階 cap)は本書で**変更しない** — 本書の主対象は commitment 値を変えない node-local 最適化であり、consensus 変更を要する項目(O8/O10)は明示的に区別して activation 判断(O13)に接続する。
**規律:** 性能主張は v0.4 §18 に従う — 本書の全数値は (a) 実コードの凍結定数(file:line 付き)、(b) ライブ devnet-EVM メッシュの実測(`be1e998`)、(c) その二つからの算術、のいずれかであり、出所を必ず併記する。推定値は「推定」と明記する。

## 0. 結論

UTXO レーンの単純送金スループットは **transient mass(シリアライズ 4×、実質 125,000 B/block の body 上限)に律速されて ~160 tx/s**。EVM レーンは同等の単純送金で、実測 chain-block レート(~6.94/s)では **gas 律速で ~9,900 tx/s(UTXO 比 ~62×)**、DA 上限では ~11,490 tx/s(~72×)。本質は署名サイズ: ML-DSA-87 の 7,219 B/署名 vs EVM レーンの secp256k1 65 B(レーン分離 = 「facts は PQ、money-now は secp」の設計判断そのものがスケーラビリティ差の源泉)。

ただし EVM レーンの実行能力は **chain-block レートに比例**し(30M gas × chain rate)、敵対的に最大幅の DAG を作られると 10/248 ≈ 0.04 chain block/s → **1.2 Mgas/s ≈ 57 transfer/s まで退化**する(UTXO レーンの 160/s を下回る; UTXO の per-block 予算は DAG 幅で退化しない)。これは GHOSTDAG 固有のトレードオフであり、受容した上で監視 KPI 化する(§3 B7)。

現実装の最大のボトルネックはスループット上限ではなく **状態規模に対するスケーラビリティ**である: (B1) state root を毎 chain block 全アカウント再構築 O(N)、(B2) 全状態スナップショットを毎 chain block 永続化 O(state×blocks)。これらは状態が ~10⁵ アカウント級になると per-block コストとディスク成長の双方で支配的になる。本書の最適化トラック O1–O7 は全て node-local(commitment 不変・fork 不要)で、O4(増分 state root)と O5(diff 永続化)が最優先である。

## 1. スケーラビリティ現状比較(検証済み)

全 4 net 共通の凍結値: 10 BPS(`BlockrateParams::new::<10>()`)、max_block_mass 500,000、mass_per_sig_op 10,000、TRANSIENT_BYTE_TO_MASS_FACTOR 4([constants.rs:35](../consensus/core/src/constants.rs))、EVM payload cap 131,072 B/DAG block・accepted gas cap 30M/chain block([evm/mod.rs:147,150](../consensus/core/src/evm/mod.rs))。

### 1.1 UTXO レーン(単純 ML-DSA-87 P2PKH 送金、1-in/2-out)

```
sig_script   = (3+4627+1) + (3+2592)                = 7,226 B
serialized   = 94 固定 + 7,310 input + 2×87 output   = 7,578 B
compute mass = 7,578×1 + 2×(2+69)×10 + 1×10,000      = 18,998
transient    = 7,578 × 4                              = 30,312  ← 律速
per block    = floor(500,000 / 30,312)                = 16 tx(compute 単独なら 26)
TPS @10BPS   = 160 tx/s
```

注意条件: 出力 ≳2.64 KAS/個でないと storage mass(C=10¹²)が律速軸に変わる(<0.08 KAS は採掘不能)。160/s は per-block inclusion 上限であり、DAG 並行下の重複 inclusion で実効ユニーク TPS はさらに目減りする。

### 1.2 EVM レーン

**DA(inclusion)上限** — 最小 110 B の EIP-1559 送金:

```
可用 payload = 131,072 − 32(空 payload borsh)= 131,040 B
per tx       = 4(borsh len)+ 110              = 114 B
per DAG block = floor(131,040/114)              = 1,149 tx
DA 上限       = 1,149 × 10 BPS                  = 11,490 tx/s
```

**gas(execution)上限** — 30M gas が **chain block 1 個につきその mergeset 全体**へ適用(exactly-once)。chain rate = BPS/(1+λ·D_max)。実測 λ·D_max ≈ 0.44(`be1e998`、payload 満載で B→D 420ms / B→A 859ms)→ **~6.94 chain block/s ≈ 208 Mgas/s**:

| tx 種別(gas) | per chain block | 保守(1 chain-blk/s) | 実測レート(~6.94/s) |
|---|---|---|---|
| 送金(21k) | 1,428 | 1,428 tx/s | **~9,900 tx/s** |
| ERC-20(~50k) | 600 | 600 tx/s | **~4,170 tx/s** |
| Uniswap 系 swap(~130k) | 230 | 230 tx/s | **~1,600 tx/s** |

律速軸: 実測レートでは送金で gas が DA の ~1.16 倍下(ほぼ均衡)、ERC-20/swap では常に gas 律速。供給超過分は class-5 prefix-take で skip され**再 acceptance 可能**(レイテンシ劣化であってロスではない)。

### 1.3 比率

| 比較 | 値 |
|---|---|
| 単純送金 TPS(保守 / 実測 / DA 上限) | **8.9× / ~62× / 71.8×** |
| per-byte 効率(raw tx) | 7,578 / 110 = **68.9×** |
| per-block DA 予算(両レーンは**加算的** — payload は mass 外) | 125,000 vs 131,072 B = 1.05× |
| 敵対的 DAG 幅での EVM 退化床 | 1.2 Mgas/s ≈ **57 tx/s(UTXO 未満)** |

スループット比 ~72× の正体はほぼ全て署名サイズ比(7,219 B vs 65 B ≈ 69×)× DA 予算比(1.05×)である。

## 2. 制約モデル

```
inclusion(DA):   MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK × BPS       — DAG 幅で線形に増える
execution(gas):  MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK × chain_rate — DAG 幅で増え「ない」
chain_rate      = BPS / E[mergeset] = BPS / (1 + λ·D_max)
                  実測 ~6.94/s、敵対床 BPS/mergeset_size_limit = 10/248 ≈ 0.04/s
state(現実装):   per-block CPU  O(total accounts)   ← B1
                  per-block disk O(total state size) ← B2
```

第三の軸「state」が現実装の真のスケーラビリティ制約であり、§3–§4 の主題である。

## 3. ボトルネック分析(実装、ランク順)

| # | ボトルネック | 出所 | コスト式 / 実測 |
|---|---|---|---|
| **B1** | **state root を毎 chain block 全再構築**(keccak MPT を CacheDB 全体から構築: 全アカウント keccak + per-account storage trie + sort) | [state.rs:51-63](../kaspa-evm/src/state.rs)、呼び出し [executor.rs:299](../kaspa-evm/src/executor.rs)。doc 注記「incremental backend は後続」 | O(N_accounts)/block。推定: 10⁵ アカウントで数十 ms〜/block × ~7 chain block/s → 支配的 |
| **B2** | **全状態スナップショットを毎 chain block 永続化**(EvmStateSnapshot = 全 accounts + 全 code + 全 storage の Vec; 実行毎に親 snapshot を全量 CacheDB 再構築) | store 206 [stores/evm.rs:72-95](../consensus/src/model/stores/evm.rs)、[snapshot.rs:25-65](../kaspa-evm/src/snapshot.rs)、§14.1 注記([processes/evm/mod.rs:139-141](../consensus/src/processes/evm/mod.rs)) | disk O(state×chain_blocks) **無限成長・pruning なし**; CPU O(state)/block(serialize+deserialize) |
| **B3** | **revm Evm を per-tx 再構築**(builder+handler 登録+env clone がループ内) | [executor.rs:192-227](../kaspa-evm/src/executor.rs) | per-tx 数 µs〜数十 µs 級の割当/初期化 × ≤1,428 tx/block(推定) |
| **B4** | **同一 tx の signer recovery 多重実行**(キャッシュなし): 検証ノード ≥2 回(body validation + acceptance 実行)、提出ノード ≥5 回(mempool/template 再 admission/template 実行/body/acceptance) | [tx.rs:80,162-164](../kaspa-evm/src/tx.rs)、[body_validation_in_isolation.rs:77](../consensus/src/pipeline/body_processor/body_validation_in_isolation.rs) | k256 recover ~80µs(推定)× 重複回数 × tx 数 |
| **B5** | **class-1 admission が逐次ループ**(body validation、全ノード・全 payload 付き DAG block で実行) | [processes/evm/mod.rs:28-33](../consensus/src/processes/evm/mod.rs) | 満載 payload で 1,149 × ~80µs ≈ **~92ms/block 逐次**(推定)— 10 BPS では並列化必須級 |
| **B6** | **index/receipts/lookup の無限成長**(stores 201/203/204/206 全て pruning なし; 204 は行内 bound 済みだが行数無限) | [stores/evm.rs](../consensus/src/model/stores/evm.rs)、registry 122/127/130/135 | disk 単調増加(206 が支配項、B2 と同根) |
| **B7** | **敵対的 chain-rate 床**(mergeset_size_limit=248 を飽和させる幅広 DAG で gas 供給が 1.2 Mgas/s へ) | [bps.rs:75-86](../consensus/core/src/config/bps.rs)、§2 | 受容済み設計トレードオフ(GHOSTDAG 固有)。攻撃には自前 PoW が必要で、効果は遅延であってロスではない |

## 4. 最適化トラック

種別: **[N]** = node-local(commitment 不変、fork 不要、いつでも導入可)/ **[S]** = storage format(node-local だが DB version bump)/ **[C]** = consensus 変更(pre-activation のみ自由)/ **[F]** = fork-gated(activation 後は hard fork)。

| ID | 内容 | 種別 | 対象 | 期待効果 | リスク/検証 |
|---|---|---|---|---|---|
| **O1** | **sender-recovery cache**: `keccak256(raw) → AdmittedEvmTx` の有界 LRU(サイズ ~2×EVM_MEMPOOL_MAX_TXS)。admit_tx_info / decode_tx_to_env が参照 | N | B4 | 検証ノード 2→1 回、提出ノード 5→1 回。純関数のキャッシュなので consensus 同値性は自明 | キャッシュpoisoning 不可(キー=keccak(raw)、値はそこから純導出)。Y8 系 fuzz に cache on/off 同値性を追加 |
| **O2** | **class-1 admission の並列化**: `admit_evm_payload_txs` を rayon par_iter 化(検証は独立・純) | N | B5 | 満載 payload の body validation ~92ms → /cores(8 コアで ~12ms)(推定) | エラー index の決定性維持(最小 index を報告)。既存テストで回帰 |
| **O3** | **Evm インスタンス再利用**: builder+handler 登録をブロックあたり 1 回にし、ループ内は tx env 差し替えのみ | N | B3 | per-tx 割当除去(マイクロベンチで定量化; 推定 数%〜十数%/block) | revm API 上 `modify_tx_env` で可。commitment 同値の回帰(既存決定性テスト) |
| **O4** | **増分 state root**: dirty-account 集合だけ storage root を再計算し、account trie は前 block の trie をキャッシュして touched leaf のみ更新 | N | **B1** | O(N)→O(touched)/block。root **値**は不変なので commitment 影響なし | 実装が最大。中間段階として「per-account storage root の LRU(untouched account の storage root 再利用)」だけでも大半を回収。差分適用 root == 全構築 root の恒等 fuzz(Y12)必須 |
| **O5** | **diff ベース状態永続化**: store 206 を per-block **diff**(touched accounts/slots)+ K block 毎の full snapshot に変更。再構成 = 直近 snapshot + ≤K diffs 適用 | S | **B2**, B6 | disk O(state×blocks) → O(touched×blocks + state×blocks/K)。実行 seed も「snapshot 全量再構築」から「親 CacheDB 再利用(同一 selected chain 上)+ diff 適用」へ | no-replay/原子性は同一バッチ commit のまま維持(§14.1 の原則不変)。reorg 時は nearest-snapshot 再構成にフォールバック。DB version bump + マイグレーション or 再 IBD |
| **O6** | **EVM store retention policy**: 203/204/206 を pruning point 連動で剪定(201 header は commitment 検証の根なので保持 or pruning-point trusted data へ畳む — v0.4 §12.2 の既設計に接続) | S | B6 | disk 成長を pruning window に有界化 | アーカイブノードは全量保持(フラグ)。受理済み設計(EvmTxLocations pruning は §16 から deferred)を実装するだけ |
| **O7** | **Stage 2: Block-STM 並列実行**(v0.4 §11) | N | 実行 ceiling | 低競合 workload で 10–20×(16–32 threads、v0.4 §11 の目標値)。設計目標 20–100 Mgas/s | serial-equivalent が仕様(Y8: parallel==serial bit-exact fuzz)。state root/receipts/logs の順序決定性 |
| **O8** | **cap 再校正**(O13 と同一決定): G_target_sec 導出(v0.4 §5.2)+ Y10 実測に基づき MAX_EVM_PAYLOAD_BYTES / MAX_EVM_ACCEPTED_GAS を activation 前に凍結し直す。実測 λ·D_max=0.44 ≪ k=124 は大きな余裕を示す — ただし引き上げは B1/B2 解消(O4/O5)と Y10 再計測を**前提条件**とする | **C** | 上限そのもの | 例: payload 256 KiB 化で DA 上限 2 倍(伝播再計測必須)。gas は AH-2(= EVM_GAS_LIMIT)維持が既定 | activation 前のみ自由。§14.3 の不等式再検証とセットでなければ動かさない |
| **O9** | **敵対床の監視**: chain_rate / E[mergeset] / accepted-gas 利用率を KPI 化(getBlockDagInfo 拡張 or metrics)。**ルール変更はしない**(per-mergeset-block gas 配分等の代替は決定性・単純性を毀損するため不採用と明記) | N | B7 | 退化の早期検知。攻撃は自前 PoW を要し効果は遅延に限られる(class-5 は再受理可能)ことを運用文書化 | — |
| **O10** | **Stage 3: deferred state root**(k-lag commitment、v0.4 §12) | **F** | B1 を臨界パスから除去 | 設計目標 100–500 Mgas/s | v0.5 fork。O4 はその到達前の node-local 橋渡し |
| **O11** | **Stage 4: zkEVM lane**(v0.4 §13) | research | 全ノード再実行の置換 | §13 の数値目標に従う | research ADR 要件は v0.4 §13 のまま |

依存関係: O5 は O4 の trie キャッシュと相互補強(diff がそのまま dirty set)。O8 は O4+O5+Y10 再計測が前提。O7 は O3 を包含する形で実装してよい。

## 5. ロードマップ

```
N0(activation 前、node-local 先行):  O1 + O2 + O3(小粒・低リスク・即効)
                                      O9(KPI 計装)
C0(activation 判断と同時):           O8 = O13(cap 凍結し直し or 現値維持の明示決定)
N1(activation 後も随時可):           O4(増分 root)→ O5(diff 永続化)→ O6(retention)
S2(v0.4 §11):                        O7 Block-STM(Y8 fuzz 完備後に default 化)
F1(v0.5 fork):                       O10 deferred root
R(research):                         O11 zkEVM
```

## 6. 計測計画(Y 系列の拡張)

```
Y11 executor profile:   per-block 時間分解(admission / 実行 / root / snapshot serialize /
                        DB write)を criterion + 実 mesh で計測し、B1–B5 の推定値を実測に置換
Y12 root 恒等 fuzz:     O4 の増分 root == 全構築 root(ランダム workload、reorg 込み)
Y13 状態成長 soak:      合成 workload で 10⁵–10⁶ アカウントまで成長させ、O4/O5 前後の
                        per-block CPU・disk 成長率を比較(§14.1 の disk 予算の実測根拠)
Y10 再計測:             O8 で cap を動かす場合は payload 満載の λ·D_max を再計測(§14.3)
```

## 7. 性能主張の規律(v0.4 §18 準拠)

対外的な単一数値の TPS 主張を行わない。主張は次の形に限る: 「単純送金で UTXO レーンの ~62×(実測 chain rate 6.94/s、gas 律速)、敵対的最大幅 DAG では 57 tx/s まで退化(設計上の受容)」のように、**律速軸・前提 chain rate・退化床を必ず併記**する。workload 依存性(低競合 transfer/ERC-20 と単一 hot pool の差)は Stage 2 の数値主張(§11)に従う。
