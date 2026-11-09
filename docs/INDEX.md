# 文書索引（互換入口）

## 2026-11-09追加採択 — 公開資料とモデル未保有の一人検証

独立選出Panelに少なくとも一人の能動的な正直者がいることを前提とし、その人がモデルを事前保有せずに全必要資料を取得・全体検証できる新profileを採択した。Minerの有限な公開供給、owner許可に依存しない認証付きcache・再配布、cold取得からの異議権、Nodeの小さなcourtを接続する。最低完全承認数はlogicalに **k_min=1** とするが、一件の署名直後にFinalへ進めず、全検査・異議機会、Qの解消、依存・資金・原期限を[20](rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の単一gateで確認する。

[RFC-0050](rfc/0050-public-materials-and-one-honest-panel.md) / [ADR-0218](adr/0218-public-materials-and-one-honest-panel.md) / [公開裁定v0.4](design/palw/consensus/public-adjudication-policy-v0.4.md) / [今回の改訂](maintenance/revisions/2026-11-09-public-panel/README.md) / [今回の検査](maintenance/revisions/2026-11-09-public-panel/VALIDATION.md)。

正常claimの条件付き完了には、正しいMinerの供給・応答、唯一正直なPanelの完全検証、最大合法な虚偽caseの有限終結と実サービス・包含が必要である。承認数1をPanel人数1や性能実測へ読み替えない。旧75／41、旧署名、既存経済値、EXEC 1 BPS、現行20 RFC／50 ADR、過去のレビューを保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。以下の先行採択は当時の範囲で読み、今回の実装・測定・有効化の証拠にはしない。

## 2026-11-09の先行採択 — PALW実行整合性と一人の反証

新claim/profileでは、一人の適格異議をnative未解決状態へ反映し、一人の有効な反証で多数の肯定承認に関係なく元claimを無効化する。必要な検証が未成立なら成功Final・仕事報酬・creditを与えない。Init・Step・Endの全関係、元claimへの認証、同blockの処理順序、保護された救済容量、資料不足、失効と依存を[現行RFC20](rfc-v2/0020-verification-gated-final-and-adversarial-models.md)へ接続した。

[RFC-0049](rfc/0049-palw-execution-integrity-and-unilateral-refutation.md) / [ADR-0217](adr/0217-palw-execution-integrity-and-unilateral-refutation.md) / [公開裁定v0.3](design/palw/consensus/public-adjudication-policy-v0.3.md) / [今回の改訂](maintenance/revisions/2026-11-09-palw-execution-integrity/README.md) / [今回の検査](maintenance/revisions/2026-11-09-palw-execution-integrity/VALIDATION.md)。

通常編集先はRFC20冊・統合ADR50冊のまま。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。条件付きの実行整合性を扱い、Miner/Panel本人のGPU使用、全員共謀・永久検閲下の無条件正当性を証明しない。以下の過去日付の採択・検査はその時点の記録であり、今回の保証・実装状態に読み替えない。今回の入力は `misaka_docs_rfc20_exec_1bps_20261108(2).zip` である。

> **2026-11-08追加採択:** EXEC creditは[RFC07](rfc-v2/0007-claims-exec-and-answer-accounting.md)、事前公開scheduleと通常時1 BPS目標は[RFC10](rfc-v2/0010-dag-consensus-and-native-settlement.md)に反映した。courtの全分岐予約と実モデル完了条件も更新した。[ADR-0216](adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)／[変更記録](maintenance/revisions/2026-11-08-exec-1bps/README.md)／[検査結果](maintenance/revisions/2026-11-08-exec-1bps/VALIDATION.md)。現行RFC20冊・統合ADR50冊を維持し、実装・測定・activationは別のgateとする。

旧 RFC 索引が参照していた `docs/INDEX.md` を案内として復旧しました。総合入口は [README](README.md)、完全一覧は [カタログ](maintenance/catalog.md)。

## 1. 利用・運用

[はじめに](getting-started/README.md) / [運用](operations/README.md) / [リファレンス](reference/README.md)。

## 2. 設計と適用状態

[PRINCIPLES](PRINCIPLES.md) / [状態](STATUS.md) / [仕様](spec/README.md)。

[PALW Architecture Specification](architecture/palw-specification.md) — 0〜9章の統合仕様と、モデル＝minerを前提とするDAG並列ブロック生成。

[2026-11-08 採択設計](rfc-v2/0020-verification-gated-final-and-adversarial-models.md) / [公開裁定の方針 v0.2](design/palw/consensus/public-adjudication-policy-v0.2.md) — 新しい claim 版について、Node のモデル非保有、未共有・モデル内全結託の脅威、非対話 positive、対話 negative、検証成立後の Final を定義します。実装・測定・有効化の完了とは別です。

<a id="3"></a>
## 3. RFC・ADR・仕様の更新

採用済み設計は原則凍結。重大問題時には新規 RFC と新規 ADR の両方、レビュー、明示的採択、対象仕様の改定、試験・移行と別途の有効化判断を必要とします。[変更管理](governance/design-freeze.md)

現行の設計文書: [RFC 20本（RFC-v2-0001〜0020）](rfc-v2/README.md) / [統合 ADR 50本](adr-v2/README.md)。新設計の採択記録 [ADR-0215](adr/0215-model-free-node-noninteractive-positive-and-adversarial-availability.md) は統合後の追加文書です。[RFC対応表](rfc-v2/MAPPING.md) / [ADR対応表](adr-v2/MAPPING.md) / [今回の変更記録](maintenance/revisions/2026-11-08/README.md) / [今回の検査結果](maintenance/revisions/2026-11-08/VALIDATION.md)。
