# A0 → 各担当への接続調整案

2026-10-31。以下は当初の送信案。後にユーザーからA1〜A7への直接送信・調整の明示許可を受け、接続依頼と後続commitを各担当へ送信した。このファイル自体を送信権限の根拠にはしない。現時点の統合範囲は[A0 checkpoint](A0-integration-status.md)を参照。

共通基準はA0 branch `codex/positive-court-a0`、commit `d615f62b6`。A0初期契約`a85152edc`、A1検査器`40cbcb559`の選択統合`52c621411`を含む。`d615f62b6`単独のcherry-pickはNode moduleの先行依存に注意し、全基準commitを取り込む。全体buildやnative統合の成功はまだ宣言していない。

## A1

A0の `TIR-INTERCHANGE-R1.md` / `positive-contract/src/tir.rs` を共通正本とする。NodeChecker adapterの独自DERIVATION_NAME/8-byte payload prefixを共通helperへ接続してほしい。`exact_derivation_id`、`greedy_selector_id`、`provenance_id`を照合し、`ExactPart::decode_frame`を使う。full frame capとraw cone/traceの差は300 bytes（284+16）。A0はA1の確定済み検査器を選択統合しmodule登録した。進行中のhardening/API adapterはcommit後に統合する。

## A2

実装`3b2e05bd0`をA0でreview/選択統合する。TIR結果fieldは共通annexのjob/context_hash/登録root/greedy selector recipeへ揃える。署名・case・有限資料契約は既存r1のまま。`CanonicalDataDutyV2`のproduction adapterと復旧meterはA4/A6へ接続が必要で、自己申告DTOをnative dutyにしない。

## A3

`158128d03`の42件は旧prototype checker経路であり、A0/A1との結合成功には数えない。A0 `PanelBuilder`、Result/Witness型、r1署名context、TIR annnex payloadへ明示接続してほしい。旧4KiBサービス木はG14 inventory/prompt unitへ置き換える。最初のjoint fixtureは新しく登録した全node-commit i32 programで、旧登録programを無言で変更しない。

## A4

snapshot `c4749c861`全体は統合しない。新しいA4固有commitを提出してほしい。A0/A1のchecked-terminalを、署名result・rooted current branch/state・全試行capacity・A2 dutyへ結び付ける。A5のprivate foundationはnative fundingではなく、A4/A5/A6で同じatomic commit境界へ接続が必要。先に受け付けたsource義務はPositive完成だけで消さない。

## A5

`56ef2750a`はA0でread-only review済み。private foundationとして選択統合し、A0所有のworkspace登録を行う。全consumerのnative binding・money engine・Gate Cが未接続であることは維持する。A1/A4の計算事実を任意booleanや旧claim IDへ置換しない。

## A6

`32287dc73`とA0 merge後のdirty adapterは、commitとraw試験証拠が揃ってから選択統合する。TrustedAnchorにはcurrent branch/state世代、期待journal head、正規carrier/accepted DAA/順序を結び付ける。読み取り境界でBoundedJournalを制限する。private履歴replay・整数fee ledgerはnative DB/支払/IBDの完成ではない。journal再encodeの費用もquoteへ含める。

## A7

A0/A1新candidateの期待登録class/artifact/input/jobは、受信packetから独立に固定する。現Packet::scopeのように候補から期待値を作る試験では、自己整合した全モデル差替えを検出できない。全fieldを一貫して置換した攻撃を追加してほしい。runner timeoutはprocess group全体を停止・終了確認してからlog/hashを確定する。4×4 warm例を実LLM・cold/native性能の実績には数えない。
