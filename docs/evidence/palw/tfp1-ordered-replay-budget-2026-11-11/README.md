# 全prefix再演の失敗位置と累積制限

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前回 `75ab20743` はactual foldと全prefix再演を同じ初期処理・ordered loopへ接続したprogress。今回は[node接続前の残件](../tfp1-ordered-prefix-replay-2026-11-11/README.md)だった拒否位置と反復上限を実装した。Studioの同一claim受付・Final・当選時権利消費・対応blockは **NOT_PROVEN**。

## 実装

[共通ordered prefix](../../../../consensus/core/src/palw_state_v2/ordered_prefix.rs)の再演入口は、親state・params・包含点・四つの実行switchに固定したnon-Clone budgetを必須とする。全passの初期化前に、pass数とordinary/native/raw候補総件数を累積計上する。予算超過は両カウンターを変更せず `NativeResource` として返す。開始したpassが拒否・context不足・初期化失敗で終わっても計上を戻さない。旧unmetered公開入口は残さない。

これは**再演回数・候補訪問数の増幅上限**であり、一回のcourt・state準備・入力走査のCPU/IO上限や料金を証明するものではない。各処理の有限work/IO/heap、profileの累積資源見積もりは引き続き必要。四つのtop-level switchはbudget生成時に固定し、passごとに差し替えられない。extras内のpolicyも同じcanonical状態から導出する責任は呼出元に残る。攻撃者の候補数だけから十分な予算があると推定しない。

失敗にはBudget/Preparation/Order、ordinary object index、Batch/TKRの実accepted ordinal、raw Assignmentのaction indexと実ordinalを付ける。ordinaryの配列indexをphysical ordinalと呼ばない。raw側は失敗までに進んだcursorから位置を取る。nativeの原判定は変更せず、actual foldは元の `PalwStateV2Error` をそのまま返す。

`ordinary_refusal_index_v1` は明示的な重複拒否のallowlistだけを除外候補とする。未知のエラー、Overflow、InvalidParams、NonMonotonicContext、NativeResource・NativeContextUnavailable・DeltaMismatch・CarriageInconsistentは返さない。独立レビューでOverflowを除外候補にしてしまう初期のnegative listを修正した。完全TFO3のsemantic拒否には専用型が必要で、genericなCarriageInconsistentを緩めて対応しない。これは署名やnative受理の代替ではない。候補を除外する呼出元は全carrier/rawの位置とextrasを作り直す必要がある。

[回帰コード](../../../../consensus/core/src/palw_state_v2/tests/ordered_prefix_v1.rs)を新APIへ変更し、重複拒否のindex、失敗後の予算消費、修正版リストの再演、枯渇時のlocal分類、事前課金失敗の原子性、初期化失敗の非refundを記述した。これらは未実行。公開T12、mixed raw/TKR、実LLMの検証結果にはしない。

## 残件と資料契約の確認

node候補filterへの接続、全carrier/raw位置の再構成、完全TFO3 quote/intake planner・共同予約・原子的install、PMR dispatcher以後は未完。新profileの公開gateを開けていない。

独立読解では、旧wide replayの `fetch_public_wide_artifact_v1` と旧Artifact codecがモデル重み全量を扱う一方、ordinary TFP1 replayは既に呼出元のbackendからinventoryを再計算すると確認した。旧署名profileを変更せず、新ordinary profileのArtifactは有限なHF固定版/変換recipe参照descriptorへ分離し、manifest・予約・publisher/replayへ同時接続する必要がある。descriptorはデータ同一性のためのもので、HF取得可否の認定やMinerのモデル全量配信義務を導入しない。このターンではその資料codecは変更していない。

配備・署名送信・新たな公開probeは行っていない。
