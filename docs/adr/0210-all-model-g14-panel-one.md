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

## 実装検証の記録

[2026-10-31 の検証記録](../evidence/palw/model-panel-one-2026-10-31/README.md): 関連回帰 283 件と EVM を含む全 target の型検査に成功。配備・有効化は未実施。

## 実装追補 — 受理後の保持 clock（2026-11-01 JST）

G14 の 1 席移行境界 18,640 は維持する。別の将来 fence
`palw_model_panel_one_retention_v1` を t12 DAA **18,850** に設定した。
遅いモデル計算では header 時点の署名済み保持期限が、受理時点に必要な court / Gate C
の期間より先に不足するため、境界以後に初めて受理する ModelOne Attempt の
stored obligation を `max(raw signed deadline, acceptance DAA + canonical class retention span)`
として決定する。producer が自由に期限を選ぶ規則ではない。
署名対象の raw deadline、header ごとの class pin、抽選 ticket と execution commitment
は変更せず、full admission でも従来の正確な class pin を検査する。
境界前の claim の期限を延長せず、その claim を後の高さで再解釈しない。

ユーザーが指定した全モデルの G14 1 席移行・node 更新・実 claim の受理と Final の確認を
行う範囲で、遅延による保持期間不足を修正する。モデル審査、係数、報酬、quanta、
公開訴追期間は変更しない。詳細な前後値・試験範囲・未完事項は
[保持 clock 修正の検証記録](../evidence/palw/model-panel-one-2026-10-31/retention-18850.md)を参照。
実ネットワークでこの新境界以後の claim の受理・panel=1・Final はまだ確認していない。
