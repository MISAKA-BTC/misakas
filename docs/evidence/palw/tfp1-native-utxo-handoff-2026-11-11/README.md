# TFO3の実UTXO handoffとordered prefix捕捉

状態: **IMPLEMENTATION_IN_PROGRESS / Rust build・test NOT_RUN / 公開有効化なし**。

前回の実装commit`bbb820061`は履歴行と有界readerを追加したprogress。直前の方針確認応答は説明のみであり、goal進捗としてはno progress。現在の未コミット差分を再確認し、今回は[元計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md)に従い、実入力をそこへ渡す経路を仕上げた。Studioの同一claimの公開受付・Final・当選時権利消費・対応blockは **NOT_PROVEN**。

## 接続した入力

[UTXO処理](../../../../consensus/src/pipeline/virtual_processor/utxo_validation.rs)は、実ブロックのRound判定を通った`ValidatedTransaction`が存在する場所でTFO3のfixed seedを作る。入力はordered populated UTXO digest/count、actual txid、gross fee。sum(inputs)=sum(outputs)+feeとliteral txidを検査する。元取引がFPR2/FPA1で包まれている場合も、書き換えたdecoder viewではなく実取引を使う。

実accepting point、merged carrier block、selected parent、coinbaseを含む実accepted ordinalを付け、txid keyed mapで保持する。map nodeは挿入前に型別の容量を予約する。再演restartではmapとleaseを破棄し、実UTXOから採取し直す。仮想テンプレートの予測点を受理済みoriginとして使わない。

[PALW抽出](../../../../consensus/src/pipeline/virtual_processor/processor/palw_accept.rs)は同じactual carrierの[funded pairing](../../../../consensus/core/src/palw_funded_original_coordinator_v1.rs)へseedを結び付ける。carrier不一致、別包含点、別の内容での再bindを拒否し、同一再bindは冪等。fixed dataはpairing内に置くため、既存のrehearsal→accepted map→actual extras経路がそのまま運ぶ。wireのpaid/verifiedフラグや別extrasのprepared rowを追加しない。

## 実行直前の位置

[全順序fold](../../../../consensus/core/src/palw_state_v2.rs)だけが親blockを持つ履歴scopeを開く。[FP受付](../../../../consensus/core/src/palw_state_v2/objects.rs)はoriginal admission・全pre-write検査の成功後、最初の`reserve_for_claim`直前に、actual state rootとglobal journal長をprivate tokenへ固定する。claim・carrier・native origin・親を照合し、object処理が成功しても失敗しても一時tokenを外へ持ち越さない。

single-object rehearsalのlocal journal長を、ブロック全体の履歴位置として扱わない。従来のようにraw root/位置/UTXO配列/feeを履歴factoryへ渡すAPIは置換し、private prefix tokenとpairing-bound seedを要求する。現在は`PublicOriginalProfile`拒否がこのcaptureより前にあるため、未完成profileの候補を受けるためだけに全状態rootを新規計算しない。

## 検査と残件

独立担当がnode handoffを実装し、core側を静的レビューした。scopeを誤ってpre-object base helperへ置いた初期差分は修正し、actual full ordered loopへ移動した。captureも公開profile拒否・事前検査より後へ移動した。core回帰はseed fee不一致、別carrier、再bind、singleton scope不足、包含点/親違いを記述する。Rust検証は後段へ留保する。

[node回帰コード](../../../../consensus/src/pipeline/virtual_processor/tests/funded_original_native_candidate.rs)には、実署名した通常取引とTFO3をFull入力検証し、fixtureのselected-parent bodyをproduction UTXO walkへ渡す検査を追加した。coinbase・通常取引・TFO3の順序（TFO3はordinal 2）を保存したacceptance dataから読み戻し、同じcarrierへのhandoff、別block・別ordinalの拒否を検査する。同じ取引列の仮想previewではseedを作らないことも記述した。companionはframing専用のdummyであり、完全profile未設定時の拒否とclaim行の不在を維持する。この回帰コードは未実行であり、公開受付の証拠ではない。

今回、実UTXOからpairingへのproduction handoffは接続した。一方、以下は未完である。

- 完全なquote/intake共通profile、全保護枠・料金・有限な履歴保持予算。
- rehearsalをactualと同じglobal prefix/journal位置に揃えるowner。TKR fenceの偶然の有効化に依存せず、native/raw/ordinary混在の物理順序を揃える。
- 完全coordinatorのinstallからprivate tokenとseedを使って206まで原子的に確定する呼び出し。
- 元予約入力・draw一覧の認証付き復元、history reader→PMR accepted order・料金控除・応答/court・独立承認・Final・receipt spend/block。

公開profileは有効化しておらず、配備・送信・新しい公開probeはない。モデル重み全量配信やHF取得可否認定も追加していない。
