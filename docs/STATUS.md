# 状態と適用範囲

> **未完gate:** 大きな資料の供給・取得後検査を有界なnative根拠で客観終結するprofile別手順は、具体化できていない場合 **PROFILE PROCEDURE NOT ESTABLISHED** として有効化不可である。最低承認数1や公開方針だけでは、この手順の成立を証明しない。実装・実測の未完と区別し、18 §2.4／20 §4.3のgateを維持する。

## 2026-11-09追加採択 — 公開資料とモデル未保有の一人検証

独立選出Panelに少なくとも一人の能動的な正直者がいることを前提とし、その人がモデルを事前保有せずに全必要資料を取得・全体検証できる新profileを採択した。Minerの有限な公開供給、owner許可に依存しない認証付きcache・再配布、cold取得からの異議権、Nodeの小さなcourtを接続する。最低完全承認数はlogicalに **k_min=1** とするが、一件の署名直後にFinalへ進めず、全検査・異議機会、Qの解消、依存・資金・原期限を[20](rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。

[RFC-0050](rfc/0050-public-materials-and-one-honest-panel.md) / [ADR-0218](adr/0218-public-materials-and-one-honest-panel.md) / [公開裁定v0.4](design/palw/consensus/public-adjudication-policy-v0.4.md) / [今回の改訂](maintenance/revisions/2026-11-09-public-panel/README.md) / [今回の検査](maintenance/revisions/2026-11-09-public-panel/VALIDATION.md)。

正常claimの条件付き完了には、正しいMinerの供給・応答、唯一正直なPanelの完全検証、最大合法な虚偽caseの有限終結と実サービス・包含が必要である。承認数1をPanel人数1や性能実測へ読み替えない。旧75／41、旧署名、既存経済値、EXEC 1 BPS、現行20 RFC／50 ADR、過去のレビューを保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。以下の先行採択は当時の範囲で読み、今回の実装・測定・有効化の証拠にはしない。

## 2026-11-09の先行採択 — PALW実行整合性と一人の反証

新claim/profileでは、一人の適格異議をnative未解決状態へ反映し、一人の有効な反証で多数の肯定承認に関係なく元claimを無効化する。必要な検証が未成立なら成功Final・仕事報酬・creditを与えない。Init・Step・Endの全関係、元claimへの認証、同blockの処理順序、保護された救済容量、資料不足、失効と依存を[現行RFC20](rfc-v2/0020-verification-gated-final-and-adversarial-models.md)へ接続した。

[RFC-0049](rfc/0049-palw-execution-integrity-and-unilateral-refutation.md) / [ADR-0217](adr/0217-palw-execution-integrity-and-unilateral-refutation.md) / [公開裁定v0.3](design/palw/consensus/public-adjudication-policy-v0.3.md) / [今回の改訂](maintenance/revisions/2026-11-09-palw-execution-integrity/README.md) / [今回の検査](maintenance/revisions/2026-11-09-palw-execution-integrity/VALIDATION.md)。

通常編集先はRFC20冊・統合ADR50冊のまま。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。条件付きの実行整合性を扱い、Miner/Panel本人のGPU使用、全員共謀・永久検閲下の無条件正当性を証明しない。以下の過去日付の採択・検査はその時点の記録であり、今回の保証・実装状態に読み替えない。今回の入力は `misaka_docs_rfc20_exec_1bps_20261108(2).zip` である。

> **2026-11-08追加採択:** EXEC creditは[RFC07](rfc-v2/0007-claims-exec-and-answer-accounting.md)、事前公開scheduleと通常時1 BPS目標は[RFC10](rfc-v2/0010-dag-consensus-and-native-settlement.md)に反映した。courtの全分岐予約と実モデル完了条件も更新した。[ADR-0216](adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)／[変更記録](maintenance/revisions/2026-11-08-exec-1bps/README.md)／[検査結果](maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。現行RFC20冊・統合ADR50冊を維持し、実装・測定・activationは別のgateとする。

**今回の追加対象は前回のRFC20本更新版、EXEC・1 BPSの指示、添付court予算分析です。** 前回のモデル非保有Nodeと検証先行Finalの採択も維持します。 設計の採択、実装の存在、測定・検証、稼働ネットワークへの有効化、リリース範囲を区別します。この文書改訂で Node・Params・RPC の実態や本番有効化を確認したとは扱いません。

## 2026-11-08 に採択した設計

現行の閲覧入口は [RFC 20本](rfc-v2/README.md) と [統合 ADR 50本](adr-v2/README.md)。新しい RFC-v2-0016〜0020 の採択根拠は、統合後の追加記録 [ADR-0215](adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) です。50本の ADR 統合単位を再採番・増補して置き換えません。

| 項目 | 採択した設計 | 今回の完了範囲 |
| --- | --- | --- |
| 状態 | **ADOPTED_DESIGN_NOT_ACTIVATED** | 文書への反映。実装完了・性能測定・本番有効化を意味しない |
| Node | モデル本体・全推論・GPU を要求せず、承認と上限付き局所証拠を検査 | 再起動・reorg・IBD でも同じ境界を要求する新規則 |
| 脅威モデル | モデル未共有、秘匿、モデル内 Miner／Panel の全結託を想定 | global pool の独立選出と正直な検証者の前提は維持 |
| positive / negative | 通常 positive は固定済み資料から非対話で作成。不一致の局所化には negative の対話を使用 | 局所 positive と claim 全体の承認を区別 |
| Final | 新版では結果固定後の `VerificationPending` で検証し、必須局所 positive と独立肯定承認が成立すると `CertificationReady`、公開異議・依存条件等も満たすと成功 `Final` | 資料不足・検証未成立は成功不可。旧 `Final` 後認証の意味を遡及変更しない |
| version / activation | 新しい claim / profile への限定改定 | wire、fence、有効化高さは未割当。旧署名済み claim は旧規則を保持 |

2026-11-08採択時の入口はRFC-v2-0020と公開裁定v0.2であり、今回の新profileは冒頭の0050／0218・v0.4を適用する。[改訂記録](maintenance/revisions/2026-11-08/README.md) / [文書検査](maintenance/revisions/2026-11-08/VALIDATION.md) を、実装適合性・性能・network activation の証拠と混同しません。

## 初期 mainnet の範囲

| 機能 | 現行方針 | 実際の runtime | 再開 |
| --- | --- | --- | --- |
| RFC-0003 画像経路 | **mainnet 後へ延期・当面無効** | 無効化実装・拒否試験の照合が必要。今回未確認 | 機能別レビューと明示的承認 |
| RFC-0003 動画経路 | **mainnet 後へ延期・当面無効** | 同上 | mainnet 開始で自動有効化しない |
| RFC-0004 モデル改善 | **mainnet 後へ延期・当面無効** | 同上。共通の不変登録まで停止しない | 機能別レビューと明示的承認 |

[正式な延期・無効化方針](governance/post-mainnet-scope.md)。延期機能の肯定的完成を初期 mainnet の条件から外しますが、閉鎖確認と共通機能の安全性・回帰は免除しません。音声・embedding 等を新規承認する表ではありません。

## 設計・実装・検証・有効化

以下は以前の整理から引き継いだ状態記録です。各原資料の対象版・時点に従い、2026-11-08 の新しい claim 版の実装状態へ読み替えません。

| 項目 | 文書上の設計・実装状態 | 有効化の扱い | 根拠 |
| --- | --- | --- | --- |
| PALW のみの合意 | BFT validator 不使用の設計を採用 | 実ネットワークの退役は今回未確認 | [ADR-0180](adr/0180-palw-only-consensus-no-bft-validators.md) |
| V+ | 採用設計・測定待ち・休眠実装、未検証という記載 | 今回有効化なし | [RFC-0016](rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) |
| モデル二階層 | ADR-0179 採用。旧モデル審査は撤回 | 今回有効化なし | [RFC-0017](rfc/0017-model-eligibility-examination-and-renewal.md) |
| Gate C / DAG | RFC全体はDraft、個別採択あり・休眠実装 | 今回有効化なし | [RFC-0018](rfc/0018-dag-parallel-multi-model-consensus.md) |
| 年次カタログ | Draft / 個別決定と試験記録 | 予定高さを稼働済みとしない | [RFC-0019](rfc/0019-annual-model-upgrade.md) |
| 長文脈メモリ | Draft / 部分実装・未検証の記載 | 合意有効化の対象外という記載 | [RFC-0020](rfc/0020-long-context-calibration-memory.md) |

## 稼働版を確認するとき

network、release tag / commit、実際の Params / fence / fingerprint、有効化高さ、node と chain の証拠を照合します。資料の current / today / live / PASS は当時の対象だけに有効です。コードと資料の不一致は隠さず記録します。

以前の整理時のパラメーター記録は次のとおりです。

> 今回、`α=1.5`、30日平均、0.3/1.0 の階層、登録総枠の原文上の設計値、bond 予算・claim 認証は変えていません。拘束の後続決定は Panel=0 7日相当 / Panel=1 1日相当です。[台帳](governance/parameter-register.md)

この引用は過去の記録です。平均期間を含む現行値は [PRINCIPLES](PRINCIPLES.md) と [パラメーター台帳](governance/parameter-register.md) の後続採択に従い、上の旧値を今回再採択しません。

元資料の detailed readiness は [readiness matrix](design/palw/integration/activation-readiness-matrix.md)。その過去の状態を現在の稼働証拠にしていません。
