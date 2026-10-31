# RFC-0042 — 全モデルの producer を G14 Panel=1 に統一する

Status: **ユーザー指定の移行方針 / 実装・検証中、未有効化**（2026-10-31）。
[ADR-0210](../adr/0210-all-model-g14-panel-one.md)。

## 対象

2026-10-31 のユーザー指示「全てのモデルの経路をproducerをG14の1席経路に切り替えて」に従う。
Qwen 8K だけに限定せず、既存モデルと後から登録されるモデルの新規 claim を共通の G14 1 席経路へ移す。
新規 claim の `legacyPanelAttempt`、複数席の segmented licence、optimistic licence へのフォールバックを廃止する。
画像・動画・改善など、[PRINCIPLES §12](../PRINCIPLES.md#post-mainnet-scope) が延期した機能の有効化は含まない。

モデル単位の検証・審査、保持者数による採否、昇格・更新・失効は導入しない。
genesis / 年次フォーク指定は 1.0、後発登録は登録時から審査なしで 0.3 という二階層と、登録モデル全体の発行上限を維持する。

## 移行境界と履歴

別の fence `palw_model_panel_one_v1` を使用する。既に発火した kernel slice の DAA 18,400 を全モデル移行の高さに読み替えない。
t12 の配備候補は DAA **18,640**。params / schedule identity、fork visitor、state params の mirror に含める。mainnet 等の fence は無効のまま。

経路を選ぶ基準は claim の **最初の acceptance DAA**。再抽選、receipt、Final、再起動、reorg、pruned restore の高さでは切り替えない。
境界前の claim とその責任記録は従来の席数・scope・経済条件を維持する。
既存のモデル ID と execution commitment の envelope は保存し、計算 backend と公開 court を再利用する。
新規 claim の割当・承認・Final の制御は本経路が担当し、旧 `PanelBound` carrier から割り当てさせない。

## 1 席の割当と承認

G14 kernel route と同じ commit 後の round seed anchor と抽選を使用する。
producer 自身、同じ operator / 公開鍵、資格・担保を満たさない bond を除き、global bonded pool から 1 席を選ぶ。
モデル別の審査結果や ready holder 数でモデルの採否を判定しない。
旧モデル別の Panel room / 5 席の possession floor は新規 claim に適用しない。静的な class の最大同時 claim 数、bond ごとの担保 ceiling、共有ネットワークの搬送・検証量と未解決 claim 上限は維持する。
旧 permissionless Panel の追跡も新規 claim には起動せず、旧 claim のみ処理する。
選ばれた席の自分のモデルコピーによる **claim 全体を scope とする承認**を必要とする。
コピーを取得できない、実行できない、データがない場合は承認を与えない。

割当は fold が一度だけ決め、実際の担保を予約する。抽選対象の bond は commit 時点までに登録されているものに限る。空の eligible pool は claim 単位の非 fraud 終了とし、旧 5 席経路へ戻さない。
固定済みの ORIGINAL custody が新しい panel を受け入れられない再抽選は `PanelUnavailable` で非 fraud 終了し、producer を slash しない。旧 custodian の保持契約は書き換えず、次のブロックを無効にしない。
既存の署名認証を通る whole-claim receipt、または 1 席が担当する全 scope の receipt だけを認める。
部分 scope、`Sampled`、S2 単独 licence、同一署名の重複を全 scope の承認に数えない。

## 記録・担保・Final

新しい stored licence door `G14PanelOne = 4` を追加する。既存 tag 0–3 の意味・符号は変えない。
新経路は実際の全 scope 承認 1 件を basis とし、旧 segmented claim は各 segment 2 件という条件を維持する。
1 件を 2 件として記録しない。単独の責任署名者の lock は全 gain に対して計算し、旧複数署名者向けの除数を流用しない。
新経路の escrow を旧経路の early release 条件で払い出さない。

Panel=1 の licence / Final と `SecurityCertified` は別。
claim ごとの Gate C の抽選・承認条件、公開訴追期間、court、DA default、post-Final の責任・失効、経済的抑止を維持する。
正直な抽選 stake の割合・応答・取得可能性を前提とする承認であり、計算の正しさの暗号学的証明ではない。
検出率が未測定の対象を検証済みと扱わない。

## 検証と配備の完了条件

- 境界の前・当日・後、混在する旧新 claim、再抽選で経路が不変。
- 異なる model ID / tier、後発登録、通常モデル・TIR / shard profile で full scope の 1 席割当を確認。
- 正常な承認と、不正 scope・署名・重複・unavailable・虚偽承認を区別。第三者の公開 court が責任を追及できる。
- 実際の予約・licence・Final・Gate C・支払・post-Final 処分、cold restore と delta revert が一致。
- producer、seat、collector、RPC / Explorer が同じ claim-local route を表示し、モデル単位の旧審査を新規採掘条件にしない。
- 同じ候補の binary / params identity を fleet で確認して段階配備し、同期と新経路の実処理を確認する。

この文書の追加や局所テストだけでは、上記の検証・稼働完了を意味しない。

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
