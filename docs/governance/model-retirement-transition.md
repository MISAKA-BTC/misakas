# BMR1モデル恒久廃止・用途別bond・Court 49/51の移行

- 日付: 2026-11-26（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 正本: [RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](../adr/0237-bft-model-retirement-and-role-bonds.md)
- 通常報酬: [RFC0068](../rfc/0068-fractional-position-miner-rewards.md)／[MOE3移行](fractional-position-economics-transition.md)

## 1. 設計採択と資産処分を区別する

ユーザーの指示を設計採択として記録する。今回実際のBFT投票、pool burn、bond没収、slash配分変更、公開activationは行わない。元のruntime/旧private MOE2と過去結果を保存する。

有効化にはnetwork、移行anchor、新目的署名domain、新native状態、認証対象集合、完全な資産移行root、case期限・quorum、実回収と受給planを固定する。既存資産をどの新規caseからBMR1対象にするかを明示した版付き移行契約を必要とし、旧契約の名前だけを同じまま変更しない。

## 2. 旧bondを用途lotへ分ける

原資・model・purpose・owner・残額・元liability期限・holdを認証snapshotから移行する。同じwalletの検証資格を、wallet全額の保護又は没収の根拠にしない。

旧担保が複数役割・複数モデルの重なるexposureに使われている場合、単純に全roleへ残高を複写しない。原資の一意なsub-allocationを解決し、合計保存と旧責任を確認できるまで新BMR1対象への移行を未commitにする。callerによる「検証用」という自己申告、元責任の削除、早期解放によって通さない。

旧責任が終わるまで元model/purposeタグを保持する。将来の用途変更は責任・hold・認証case freezeのすべてが終わってから。既に適法に解放された旧資産へ無限の遡及権を追加しない。

## 3. poolと支払原資を分離する

poolの現在real MISAKA、virtual、seed履歴、累積LP、pending order escrow、採掘reward escrow、検証/BFT役務予約を別台帳で照合する。receiptの累計を新しい残高として入金しない。

移行後の正規MODEL_RETIREだけがcurrent real reserveをburnできる。通常withdraw/owner引出しは禁止のまま。これは過去LP promiseへの明示改定であり、対象版と資産集合を認証してから適用する。未移行の旧poolを旧MODEL_STOP票で処分しない。

既確定holder POSITIONと元の権利確定記録を保存する。RETIRED後は当該AMMを閉鎖し、pool在庫を凍結する。未確定POSITION awardをretireする際はholder又は専用escrowの同じ量を一度だけ減らす。

## 4. 未確定と正当な検証報酬

各producerの旧/新awardを、未発行予約、実MISAKA escrow、実POSITION escrow、既確定出力へ明示分類する。未解禁であるだけの既確定支払を未確定扱いにしない。分類できない経路があれば有効化しない。

Panel/Gate Cの成立報酬と係争中の役務予約は採掘rewardの集合から外す。旧claimが停止・廃止されても、正当に成立した検証役務のMISAKA支払、Court応答、資料供給、元責任を維持する。BFTが各検証者の善悪を選別して保護額を決めない。

未発行取消は `CancelledUnissuedReward`、実通貨消却は `BurnedMISAKA`、POSITION消却は `RetiredPositionAtoms`。burn統計と供給変動を混同しない。

## 5. 新coinbase5%とCourt49%の版

通常coinbaseの最新分割50/20/20/5/5を、認証された新配分recordから適用する。BFT5%・Panel/Gate C合計5%を一回だけ予約し、旧10%/15%案を実装上のfallbackにしない。

旧offence・旧signed liability・旧collectionは元のslash配分契約でdrainする。新Court49/51は、新policyを束縛したoffenceの実回収原資へ適用する。古い回収を再登録して49%を再請求できない。受取identity、資格、発見・立証record、plan、期限、一回キーを認証する。

分割回収をする新offenceは累積S/Qと一意collection IDを保持する。Qの支払/専用予約とburnを同じjournalへ結合する。BFT恒久廃止purposeの資金をCourt recovery purposeへ付け替えず、100%burnから報酬を作らない。

## 6. ケース開始・終端・再生

正規case開始から対象market・採掘原資・登録者/採掘lotをfreezeするが、検証資金の通常期限を延ばす権限にはしない。単なる受信提案ではfreezeしない。未決・却下・期限切れはcase固有freezeを解除し、元の独立停止・liabilityは残す。

RETIREDを不変model/rootの認証tombstoneとして保持する。同rootのalias/別登録・旧fork指定・genesis名・BurnResumeで再開しない。pruning後も資産一回性、旧責任、Court・回収履歴、tombstoneの必要summaryを保持する。

native reorgは市場・lot・escrow・支払・burn・tombstoneを厳密な前状態へ戻す。これは通常のモデル再採用又はLP償還ではない。旧purposeの署名と旧stateを新purposeへ再解釈しない。

## 7. 受入検査

最低限、兼任6000/4000、悪性フラグを問わない検証lot保護、他model非干渉、資金重複・用途替え拒否、case期限、current reserveと履歴累計の区別、未発行取消、POSITION retire、既確定支払保護、49/51と分割回収、自己責任identity除外、重複collection、後段失敗、実reorg/IBDを検査する。

今回のPython参照はこのうち抽象lot会計・数量・一回性・原子性を検査する。BFT署名、Court判定、実UTXO、役務資格、native永続化・全consumer接続を完了したとはしない。
