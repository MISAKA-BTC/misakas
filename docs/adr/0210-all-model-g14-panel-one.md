# ADR-0210 — 全モデルの新規 producer claim を G14 の 1 席経路へ移す

Status: **ユーザー指定方針 / 実装・検証中、未有効化**（2026-10-31）。
Specification: [RFC-0042](../rfc/0042-all-model-g14-panel-one.md)。

## 決定

対象を Qwen 8K に限定せず、既存・後発の全モデルの新規 producer claim を G14 の 1 席経路に統一する。
新規 claim が `legacyPanelAttempt` の複数席経路へフォールバックする実装を残さない。
モデル単位の検証・審査は行わず、二階層係数、claim 単位の court、Gate C、担保・責任を維持する。

最初の acceptance DAA で経路を固定する独立した移行 fence を設け、既存 claim の処理を保存する。
全 scope の単独承認、対応する全 gain の署名者 lock、専用の licence 記録を一緒に接続する。
既存の席数定数だけを 1 に置換する実装は、この決定の完了としない。

## 根拠と状態

ユーザーは 2026-10-31 に旧経路の廃止と 1 席経路への切り替えを指示し、続いて対象が全モデルであることを明示した。
これは [PRINCIPLES §11](../PRINCIPLES.md#design-freeze) に従って記録する移行範囲であり、モデル単位の審査を再導入する判断ではない。
実装・試験結果・有効化の実績は区別して記録する。現在は稼働完了を宣言しない。
