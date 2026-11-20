# 2026-11-17 — 全モデル共通受付・動的資源予約・総搬送1 BPS

- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。設計の記述と文書/抽象モデルの検証であり、native実装・実機測定・公開activationの完了ではない。
- source: MISAKA-BTC/misakas の pre@c3da7c6eaf5bf87f8e032d5b122c603d720a8398。
- 最新指示: 一・三・五件と30秒の配分は説明例とし、court件数を固定せず過大負荷からNodeを保護する。
- 補足入力: 2026-11-17添付の共通bounded checker、未知model自動受付、多段上限、全合法継続の予約に関するテキスト。

## 成果物

| 文書 | 内容 |
| --- | --- |
| [RFC-0055](../../../rfc/0055-global-admission-court-calendar-and-one-bps.md) | 共通形式、全新規routeのD、資源予約、Node内部の有限性、全carrier1 BPS、期限・Final・精算・移行 |
| [ADR-0223](../../../adr/0223-global-admission-court-calendar-and-one-bps.md) | 最新指示の採択範囲、旧固定案の撤去、経済条件との分離、未有効化 |
| [実装計画](../../../design/palw/integration/unified-admission-implementation-plan-2026-11-17.md) | nativeの接続順、既存コードとの境界、正常/不正の実経路、共通qualification |
| [レビュー](REVIEW.md) | 旧案からの是正、独立した設計レビュー、未実証の条件 |
| [検証記録](VALIDATION.md) | 抽象モデル、文書の増分検査、既存checkerの失敗を分けた結果 |

## 決定の要点

1. 120秒は計画・集計epochとする。court/claim件数、30秒専用窓、通常一件＋反証三件を規範にしない。
2. 共通checker/codec/cost meterとNode共通経路を版・機器・容量ごとにqualificationする。対応kernelで表せる未知modelの個別benchmarkやallowlistを要求しない。
3. 全itemの非ゼロ固定費、10次元flowと占有期間付きstockを計上する。全model共通の残容量、原期限、因果順序から実際の受付量を導く。
4. 原受付時に全actorの適格救済、有限第三者、防御、cold取得・全体検証・provider供給、終端・精算、保持・復旧、control/carrierを予約する。
5. 全新規routeの共通Dは需要調整器、hard budgetは安全上限とする。受理済み救済へ新しいDを要求しない。
6. allocation前の上界検査、全process budget、実終了まで所有するlease、bounded queue、backpressureをNode実装の必須契約とする。
7. 全kindの外側carrierを共通1 permit/秒へ統合し、DAG並列生成とcanonical一回性を両立する。既存P_m/CCU/権利規則とFinal条件は維持する。

## 検査の入口

~~~bash
python3 docs/maintenance/revisions/2026-11-17-global-admission-one-bps/check_protocol_model.py
python3 docs/maintenance/revisions/2026-11-17-global-admission-one-bps/check_revision.py
python3 docs/maintenance/check_docs.py
python3 docs/rfc-v2/check_integrity.py
python3 docs/adr-v2/check_integrity.py
~~~

[抽象モデル](check_protocol_model.py) / [保存結果](model-results.json) は有限の契約モデルである。nativeコード、real RPC/Studio、実LLM、メモリ安全性、最低Node性能、実時間1 BPSを証明しない。一万/五万modelの検査もIDを増やして予算を増やさない抽象則であり、実registry性能の測定ではない。

[source baseline](source-baseline.json) は元commitで確認したdocsのテキストblobと既存parameterを記録する。[変更台帳](changes.json) は今回の許可変更を区別する。過去revision・旧numbered RFC/ADR原文・歴史付録・旧checkerのbaselineは変更しない。現在の主題別20 RFC・50 ADRを維持し、0055/0223は統合後の変更記録として登録する。

既存checkerは過去snapshotのbytesや既に削除された証拠参照で失敗する。その失敗を隠すために古い基準を書き換えず、元preで同じ検査を実行した結果と今回の増分を[検証記録](VALIDATION.md)へ分けて記載する。

