# MISAKA セキュリティ監査 修正対応報告書（再監査提出用）

**対応日:** 2026-06-22 / 2026-06-23
**対象ブランチ:** `pr-19-s5f-generator-mldsa-recalibration`
**修正後 HEAD:** `117f915909987a3c3b9be5941efb4d7b70c54703`
**対象 2 監査:**

| 監査 | パッケージ | 対象ZIP SHA-256 | 件数 |
|---|---|---|---|
| EVM・NFT 監査 | `MISAKA-EVM-NFT-Audit-Package-2026-06-22.zip` | `749482ad3eb54878d02c2950cb89e3c7555a21e3c8e12f89753a54960be090aa` | 7 High / 8 Medium / 3 Low / 3 Info |
| Kaspa差分 監査 | `MISAKA-Kaspa-Diff-Audit-Package-2026-06-22.zip`（base kaspa `e97070f`） | 同上 | 2 Critical / 11 High / 12 Medium / 4 Low/Info |

> **重要:** 2 つの監査は重なる。Kaspa差分監査の **H-04〜H-09 / M-05〜M-11** は EVM・NFT 監査の **H-01〜H-06 / M-01〜M-08** と同一所見であり、EVM・NFT 監査の修正コミット（`39e5f6a..3650a66`）で既に閉じている。本書はその対応関係も明示する。

---

## 0. エグゼクティブサマリー

- **Critical 2 件: 両方クローズ。** C-02（remote signer の context↔purpose 非結合 = 鍵を出さない double-sign）は実装で修正＋回帰テスト。C-01（sidecar 署名digest非結合）は検証で「強制slashではなく self-DoS」と再評価したうえで修正。
- **High 11 件（Kaspa差分）: 全件対応。** うち 6 件（H-04〜H-09）は EVM・NFT 監査で既修正、5 件（H-01/02/03/10/11）を本対応で修正（H-10 は経済スコア部分を既修正、残りは bounded-DoS フォローアップ）。
- **EVM・NFT 監査 21 件:** High 7 / Medium 8 / Low 3 = 全 18 実装、Info 3 は開示で対応。
- **検証方式:** 各所見をソースに対して **並列の敵対的検証 Workflow**（EVM 18件 + 差分 12件、計 30 エージェント）でレビューし「実在 / 既緩和 / 誤読 / 降格」を判定。コンセンサスクリティカルな F002 cap（M-03）は **独立サブエージェントによる敵対的レビュー（7/7 違反なし）** を実施。
- **コンセンサス安全性:** consensus 変更は F002 cap のみで、**有効化fence付き・inert（全ネット u64::MAX）・byte-identical 証明済み**（genesis ハッシュ不変）。H-11 は config のみ（dns_params は genesis 入力でない → 再genesis不要）。
- **未デプロイ:** push / mesh swap / mainnet launch はいずれも未実施（「デプロイ前に確認」方針）。

---

## 1. 修正方針と検証

1. 提供 ZIP の差分境界を再構成し、各所見の引用 file:line を**現ツリーに対して**確認（EVM・NFT 監査は 4 ファイルの SHA-256 が現 HEAD と完全一致）。
2. 所見ごとに敵対的検証エージェントを起動し、(a) 記述コードが現在も存在し主張通り動くか、(b) 監査が見落とした既存の緩和があるか、(c) 誤読か、(d) 最小修正と consensus 影響、を判定。
3. 修正は **crate 単位でビルド + テスト**。consensus 変更は inert 化（byte-identical 証明）＋独立敵対的レビュー。
4. 各修正を 1 コミット = 1 所見群で記録（下表の commit hash）。

---

## 2. Critical（Kaspa差分監査）

| ID | 所見 | 判定 | 対応 | commit | 証跡 |
|---|---|---|---|---|---|
| **C-02** | remote signer の `strict`/denylist を cross-purpose + caller制御 context で迂回（鍵を出さず attestation を偽造 → equivocation slash） | **確認(実害あり) → 修正** | `try_sign` で ML-DSA context を purpose に結合（3 overlay context をそれぞれの purpose に予約、Transaction は借用不可）。違反は PolicyViolation | `987d8ba` | 回帰テスト `rejects_cross_purpose_context_borrowing_c02`（Unbond×attestation-ctx と Transaction×unbond-ctx を Strict でも拒否、正規 Unbond は署名）/ signer 10/10 |
| **C-01** | sidecar が RPC 供給の digest を再計算せず署名、guard が署名後、metadata 由来で記録 | **降格(self-DoS) → 修正** | genesis_hash を load 時に pin → `stake_attestation_message` をローカル再計算 → RPC `message` と fail-closed 照合 → **再計算値を署名**、equivocation guard + dry-run を**署名前**へ | `9ca86dd` | validator 4/4。**降格根拠**: consensus が genesis_hash で digest を再計算し非canonicalを拒否（採掘不能）/ 同一target署名は非incompatible / 別target は既に `Block` |

> C-02 の修正で README の「compromised node cannot double-sign」保証が回復（署名は purpose 固有 context でのみ生成され、attestation context は purpose=Attestation のみ → equivocation guard を必ず通過）。

---

## 3. High（Kaspa差分監査）— 11 件

| ID | 所見 | 判定 | 対応 / commit |
|---|---|---|---|
| **H-01** | DNS seeder が未検証 address を配布（eclipse） | 確認→修正 | seeder が publicly-routable のみ配布（bogon/private/CGNAT Sybil 除外、anchor は常時）+ addressmanager `get_verified_addresses()`。`117f915` |
| **H-02** | legacy `getUtxosByAddresses` 無制限 materialize（DoS） | 確認→修正 | 250k 件で hard cap → serialize 前に Err、page/balance API へ誘導。`373938d` |
| **H-03** | EVM/DNS pruning snapshot 単一巨大message（IBD DoS） | 部分確認→修正 | deserialize 前バイト上限（header 1MiB / EVM state 256MiB / overlay 64MiB）。chunk/stream manifest は follow-up。`373938d` |
| **H-04** | EVM 全状態 block 単位保存・pruning欠如 | **既修正** | = EVM・NFT 監査 **H-01** → `a087aec`（pruning processor が EVM state/header/payload/receipts も削除、inert-safe） |
| **H-05** | Ethereum HTTP RPC 資源枯渇 | **既修正** | = EVM・NFT **H-02** → `fa564e5`（conn semaphore 512 / 30s timeout / batch 100 / resp 16MiB / loopback bind warn） |
| **H-06** | `eth_call`/`estimateGas` 不一致・empty-state fail-open | **既修正** | = EVM・NFT **H-03** → `fa564e5` + `1e30a64`（fail-closed + estimate hard-error + historical block selector 拒否） |
| **H-07** | safe/finalized/historical 不正確 | **既修正** | = EVM・NFT **H-04** → `a32c37f`（canonical heads resolver、safe/finalized 実解決、account_at(selector)、full-tx bool、parentHash） |
| **H-08** | `eth_getLogs` 無通知 10k 打ち切り・logIndex不整合 | **既修正** | = EVM・NFT **H-05** → `fa564e5` + `8553a12`（>10k で Err、block-global logIndex、実 EIP-234 bloom） |
| **H-09** | tx status が side-branch を accepted 扱い | **既修正** | = EVM・NFT **H-06** → `fa564e5`（canonical receipt 基準、orphaned 分離） |
| **H-10** | EVM mempool state非依存 admission・score不整合（Sybil占有） | 部分確認→部分修正 | eviction/selection を effective-tip に統一 + replacement atomic 化（= EVM・NFT **H-07/M-01** → `cf10535`）。残: stateful 残高/nonce 与信（**bounded DoS**、follow-up） |
| **H-11** | mainnet DNS finality が単一validatorで active | 確認→修正(config) | `PRODUCTION_DNS_PARAMS.min_active_validators` 1→3、testnet は 1 に明示固定。genesis 不変。`7cbfab9` |

---

## 4. Medium

### Kaspa差分監査
| ID | 対応 | commit |
|---|---|---|
| M-01 EVM replacement 非atomic | = EVM・NFT M-01、atomic swap | `cf10535` |
| M-02 validator seed 緩いperm | fail-closed（symlink拒否 + group/world読取拒否） | `2d558bc` |
| M-03 signer thread/idle/global mutex | 同時接続 64 上限 | `959058a` |
| **M-04 signer audit log 外部anchorなし** | **未対応（follow-up）**: forensic のみ。double-sign 防止は SignedEpochStore guard。periodic signed checkpoint を TPM/transparency log へ | — |
| M-05 custom Borsh receipt root | = EVM・NFT M-02、custom commitment と明示（doc） | `6f5cd85` |
| M-06 F002 件数/gas cap | = EVM・NFT M-03、`MAX_WITHDRAWALS_PER_EVM_BLOCK` enforcement（fenced inert） | `09e5a4d` |
| M-07/M-08/M-09/M-10/M-11 | = EVM・NFT M-04/05/06/07/08（NFT seal/strict ctor/solc pin、CLI key perms+zeroize、RPC fields） | `39e5f6a` `6f5cd85` `a32c37f` |
| **M-12 ML-DSA mass 校正が Apple Silicon中心** | **未対応（運用）**: mainnet前に低スペック x86/ARM no-SIMD で既存 `cargo bench -p kaspa-txscript` を実行・再校正。現状 ~25-50× ブロック予算余裕 | — |

### EVM・NFT 監査の独自番号 → commit（重複の対応表）
EVM・NFT 監査 H-01=`a087aec` / H-02=`fa564e5` / H-03=`fa564e5`+`1e30a64` / H-04=`a32c37f` / H-05=`fa564e5`+`8553a12` / H-06=`fa564e5` / H-07=`cf10535`。
M-01=`cf10535` / M-02=`6f5cd85`(doc) / M-03=`39b1057`+`09e5a4d`+`3650a66` / M-04=`39e5f6a` / M-05=`39e5f6a` / M-06=`39e5f6a` / M-07=`8ac2646` / M-08=`6f5cd85`+`a32c37f`。
L-01=`39e5f6a`(test) / L-02=`b60a9d5` / L-03=`6f5cd85`。I-01/I-02/I-03 = 開示（secp≠PQ 明記、ERC-2981 非強制、Logic Capsule 未実装）。

---

## 5. ビルド・テスト証跡

| 対象 | 結果 |
|---|---|
| `kaspad --features evm`（統合） | ビルド緑 |
| `kaspad`（default, non-evm） | ビルド緑 |
| `kaspa-pq-signer` | 10/10（C-02 回帰含む） |
| `kaspa-pq-validator` / `-core` | 4/4 / 2/2（seed loader） |
| `kaspa-consensus-core` | genesis hash 255/0 **不変**（H-11/F002 fence の inert 証明） |
| `kaspa-consensus`（+ EVM/store） | evm 7/7、store delete-batch 2/2 |
| `kaspa-evm` | 15/15（F002 cap テスト含む v1+v2 active/inert） |
| `kaspa-mining`（+evm） | mempool 17/17 |
| `kaspa-eth-rpc` / `kaspa-rpc-service` / `kaspa-p2p-flows` / `kaspa-addressmanager` | ビルド緑 |
| NFT（Foundry, OZ v5.0.2, solc 0.8.28） | forge 30/30、bytecode 再現一致（`contracts/nft/BUILD.md`） |

**独立敵対的レビュー（F002 cap, consensus-critical）:** サブエージェントが revm 14.0.3 の `transact_commit` ソースまで照合し 7/7 プロパティ PASS（inert byte-identical、cap-skip の supply 中立、決定性、count/materialize 一致、他経路の見落としなし）。

---

## 6. 残フォローアップ（いずれも 鍵安全 / consensus / リリースブロッカーではない）

| 項目 | 種別 | 内容 |
|---|---|---|
| **H-10 残** | bounded DoS | mempool の stateful 残高/nonce 与信（ready/parked）。squat は effective-tip eviction で既に解消。`ConsensusApi` 残高 accessor が必要 |
| **M-04** | forensic | signer 監査ログの外部 anchor（periodic signed checkpoint → TPM/transparency log） |
| **M-12** | 運用 | ML-DSA mass を低スペック実機で再校正（mainnet checklist） |
| **H-01 深部** | bootstrap gate | verified-only RPC 配線 + ASN/prefix diversity + UDP RRL + QNAME 検証 |
| **H-03 深部** | IBD | chunk/stream manifest + element cap（巨大正規 state 対応） |
| **F002 cap 有効化** | consensus deploy | `evm_f002_withdraw_cap_activation_daa_score` を finite DAA に（gas-pool-v2 同様の協調デプロイ、要サインオフ） |
| **H-11 mainnet gate** | governance | 最終 validator 数(3-5)、work-threshold の live 校正、parameter freeze/sign-off |
| **動的検証**（両監査の §動的検証計画） | QA | C-01/C-02 PoC、IBD adversarial、DNS eclipse sim、UTXO 150k/1M、EVM state soak、RPC DoS、reorg finality、PQ cross-platform、bridge invariant fuzz、reproducible build |

---

## 7. 再監査提出物チェックリスト（監査の「再監査提出物」項目への対応）

- [x] 修正 commit hash — 本書 §2-4 + `MISAKA-Audit-Remediation-Matrix-2026-06-23.csv`
- [x] source ブランチ/HEAD — `pr-19-s5f-…` / `117f915`
- [x] `cargo test` 結果（該当 crate） — §5
- [x] `forge test` 結果 + reproducible bytecode manifest — §5 / `contracts/nft/BUILD.md`
- [x] consensus-critical 変更の独立 adversarial レビュー — §5（F002 cap 7/7）
- [ ] `cargo clippy` / `cargo audit` / `cargo deny` — CI で再実行（本環境では未実行）
- [ ] RPC adversarial load / EVM state-growth soak / DNS eclipse sim — §6 動的検証（未実施）
- [ ] deployed address/chain ID/constructor args/admin/minter/royalty — デプロイ未実施
- [ ] reproducible release SBOM/SLSA/signed SHA256SUMS — リリース時

> 本対応は静的修正 + crate 単位テスト + 限定的な独立レビューであり、動的 adversarial 検証・CI フルパス・独立第三者再監査は §6 のとおり未完。Critical 2 件・主要 High の修正版 commit を対象に、§6 の動的検証と差分再監査を推奨する。
