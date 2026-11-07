# Codex への引き継ぎ 2(2026-10-18)— 赤い試験を通す・受入試験の経路を通す

目的は **drill に早く入ること** です。drill の前提は「全 lane の対象試験が green で、受入試験が実際の経路で通ること」です。この引き継ぎでは、そのうち **どの Claude の lane も持っていないもの** を渡します。

最初に、もう 1 本の Codex 用の引き継ぎ書 [codex-handover-2026-10-18.md](codex-handover-2026-10-18.md) の §0(必ず読むもの)、§3(触らない範囲)、§4(規則)を読んでください。規則はそちらと同じです。

## 1. 作業の基準点

- base:`origin/pre`(最新を取得すること)。
- 作業 branch:`codex/red-tests`(タスク A)と `codex/acceptance-rfc`(タスク B)。
- push してよいのは自分の branch だけです。main・pre・`integ/*` には push しないでください。
- 原因が他の lane の branch にある場合は、その branch を merge して直してかまいません。その場合は、どこを変えたかを commit message に明記してください。

## 2. タスク A — 検証で赤のまま残っている試験を通す

一覧と、これまでの分類は `docs/audit/palw/design-reviews/consolidated-validation-2026-10-15.md` の §3〜§8 にあります。下の試験は、どの lane も担当していません。

| 対象 | 本数 | 分かっている原因 | 求めること |
|---|---|---|---|
| kaspad の view None | 4 | fixture が staged public court を有効にしたまま、FP job を素の payload で送っている。staged court の下では HFP1 envelope(custody pledge 付き)が必須 | fixture に正しい HFP1 envelope を付ける |
| kaspad の network | 3 | 高負荷のときに wall deadline を切る(環境要因と見ているが、未確定) | 原因を確定させる。環境要因なら deadline を負荷に依存しない形にし、本物の不具合なら直す |
| kaspad の parallel lane | 1 | 16 lane +再送 1 の重複を数えていた。試験は直したが、まだ流していない | 流して確認する |
| `t12_a2u_new_kinds_uniform` | 2 | `palw_material_release_recovery_v2` を試験環境で有効にしていない。新しい action 19〜21・80 まで検査が届いていない | 着地済みの fence をすべて試験環境で有効にし、新しい種類を一覧に足す |
| SDK | 2 | pin がずれている | 正しい値かを確かめてから pin を更新する。理由を commit に書く |
| tir-lower | 3 | admission、quant_decode_pins、three_way。在庫の一覧や pin が古い(元の base でも失敗) | 一覧と pin を現状に合わせる |
| base0 | 3 | float_free ×2、prefix cache(元の base でも失敗) | 原因を確定して直す |
| rfc12 | 4 | `evm` feature が要る。まだ build していない | ディスクの空きが 30 GB 以上のときに build して流し、分類して直す |
| RFC-0018 の sibling | 1 | 8 回中 6 回失敗する。`palw_reorg_strict_economic_win` が、同じ blue work の sibling を sink 候補から外すと同時に merge 候補からも外す | 調査し、修正案を書く。live 網の fork choice に関わるので、規則は休眠 fence の下でだけ変える。変える前に報告すること |

**守ること**
- 試験の削除・skip・`#[ignore]` は禁止です。
- 期待値を変える場合は、その期待値が仕様上なぜ正しいのかを commit に書いてください。
- 結果は consolidated-validation の文書に、日付付きの節として追記してください。

## 3. タスク B — 監査の受入試験の経路を通す(RFC19・01・15・17・11)

`docs/design/palw/rfc-completion-audit-2026-10-17/report_ja.md` の該当節が根拠です。これまで drill の lane に割り当てていましたが、drill の lane を最終段に集中させるため、こちらに移します。

1. **RFC19 の R-7/R-8**:`scripts/drills/rfc0019_rolling_upgrade_drill.py` に `model-add` と `local-llm` の mode を足します。
   - 新しい固定の class ID と artifact root を使い、実際の worker と gateway で、H の前後を確かめます。
   - 確かめること:追加されるのは新しい ID だけであること、旧 ID と root が変わらないこと、tier は allocation の roll でしか変わらないこと、旧・新どちらのモデルも実際の prompt に答えること、別のモデルへ fallback しないこと。
   - edition 0 のままの初回 fence の試験は、別にしておきます。
2. **RFC01 の組み合わせ受入**:decode の規則・prefix・batch・公開反証・fence をまたぐ Final を組み合わせます。
   - 正直な token は反証されないこと、偽った token は第三者の証拠で裁かれること、fresh と resume が同じになること、single と batch が同じになること、fence の前後でノードの状態が一致すること。
3. **RFC15 の Panel=0**:通常の登録・実際の profile・公開検証者 1 人で、正直な claim が支払われ、Final の前と後の不正がどちらも実際に徴収されることを、最後まで通します。依存する lane(RFC08・14・16)の branch を merge して進めてください。
4. **RFC17**:tier の係数がちょうど 1 回だけ掛かること、モデルを分割しても登録モデル全体の上限を回避できないことを試験します。年次の追加・allocation の roll・block 権利・実際の報酬を、1 本の chain で確かめます。
5. **RFC11 §18**:固定の対象一覧から、実際の変換 → 登録 → task と context の実行を行い、D_all・D_complete・mining-funded Final を別々に集計します。未対応の architecture は分母に残します。harness を作り、時間や機材が要る部分は報告してください。

機材の上限:ノードは 3 台まで、dense なノードを 4 台同時に動かさないこと。t12 の公開ノードと VPS には触らないでください。

## 4. 報告

タスクごとに、通した試験(名前と件数)、直した原因、残りとその理由を書いてください。試験で示していないことを「完了」と書かないでください。
