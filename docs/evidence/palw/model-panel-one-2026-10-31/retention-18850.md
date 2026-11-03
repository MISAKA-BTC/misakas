# 将来 DAA 18,850 — 受理後の保持 clock 修正

2026-11-01 JST の実装・局所回帰記録。新しい immutable source revision / binary SHA、
配布完了、実 claim の受理・panel=1・Final は **pending**。初回配布の履歴 SHA を
今回の実装 SHA として扱わない。

## 原因と規則

実 Qwen block は公開 node に到着し blue に merge されたが、claim は
`COURT_RETENTION_BELOW_GUARANTEE`（残存保持期間 20,604 DAA / 必要保持期間 20,608 DAA）で未受理だった。
計算前に固定した header の期限から、計算・搬送中に 4 DAA が消費された。
到着・merge と claim 受理は区別する。

G14 全モデル 1 席の元の境界は 18,640。独立した保持 clock fence
`palw_model_panel_one_retention_v1` は将来 DAA **18,850**。
境界以後に初めて受理する ModelOne Attempt に限り、stored obligation を
`max(raw signed deadline, accepted DAA + canonical class retention span)` とする。
同じ実効期限を court admission、検出予算、claim 記録、node の保持判断に使う。
これは protocol が課す追加の保持義務であり、producer に任意の延長や追加抽選を許す規則ではない。

raw signed deadline・header class pin・署名 bytes・ticket・claim identity・計算 commitment は保存する。
full admission でも header から導かれる正確な raw pin を検査し、短い別 pin による追加 ticket を拒否する。
旧 claim の期限は延長せず、境界前の不足期限による拒否も保存する。
既存 rider は受理済み lead の stored obligation を正確に署名し、既存の有効窓を維持する。
raw rider pin と stored lead obligation を混同しない。

全モデルへの G14 1 席移行・node 更新・main / release への反映はユーザーが明示した作業範囲。
確認の終点も実 claim の受理・panel=1・Final と指定済み。
モデル審査・新しい認証制度・モデル係数・報酬・quanta・claim の受付窓・Gate C 期間は変更しない。
Final は Gate C / SecurityCertified や発行の完了を意味しない。

## identity と履歴

| 値 | 初回 18,640 配布版 | 18,850 fence を含む今回の候補 |
|---|---|---|
| params | `edf472294c89dab0e64e625dbdfcd039b91081b05f89eacf8ba2aa25baa3a189` | `bbb4ad66e54695b04f136742902794a6d93aa4cec178e80c185cb58b1c3050a2` |
| schedule | `4024a6e74d7aeb170d99e67c6d8fbe7884d71a716ccb778cd1ee526d01781b04` | `5a84a8d700cae89fad2fb6316cf2033d7c80cecede29e7ccfa00eb3088e8679a` |
| handshake | `5de80e64b63572de0cbf1a09679034e3a1765166e8249d3a88f8e29891215bb5` | 同じ |

fence は全 writer / visitor / fork ID に Some-only で入り、None / Never は旧 identity と一致する。
mainnet・t11・devnet・historical helper は無効。新 field を 18,400 の一括 activation に混ぜない。
ModelOne / court / Gate C の先行と top-level / state mirror の一致を検査する。
無効 top-level に stale active mirror が残る場合も拒否する。

## 検証範囲

- [新 core 回帰 9/9](retention-18850-core-tests.log)。この 9 件には config 4 件が含まれる。
  delayed acceptance、実署名による単独承認と Final、cold load / revert、raw pin anti-grind、
  rider の受理済み期限、max 保持、境界前後・依存条件・None/Never・他 network / 履歴を確認。
- [既存 G14 route 回帰 9/9](retention-18850-existing-route-tests.log)。旧新 claim の分離を確認。
- [identity probe 1/1](retention-18850-identity.log)。上表は同じビルドが実計算した値。
- [node 回帰 3/3](retention-18850-node-tests.log)。rider の exact window / identity、
  実 janitor pressure による旧・自分・他者 material の保持、BASE の 120,000 ms 境界を確認。
  BASE pacing は G14 の BASE のみが対象で、non-BASE と境界前は不変。

core テストは既存 CI と同じ 64 MiB test stack を使用。
テスト成功は fleet 配布・将来 fence 発火・実モデル claim 受理・Final のライブ証跡ではない。
既存の履歴 evidence JSON と配布済み binary の SHA は書き換えていない。
