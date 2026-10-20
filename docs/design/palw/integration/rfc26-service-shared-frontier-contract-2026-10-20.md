# RFC-0026 F02/F03 — 予約サービスと共有 frontier の詳細契約案

**状態:** Proposed / Review Required。2026-10-20 ユーザー指示「推奨案で詳細仕様化を進める」に基づく。[ADR-0192](../../../adr/0192-court-service-opportunity-and-shared-pursuit.md) A/A の詳細化。採択・実装完了・統合・有効化ではない。[RFC-0026](../../../rfc/0026-court-policy-master-contract.md) の 3,120 / 10,000 と旧 claim の ruleset を維持する。

## 1. 原子的な original acceptance

`a` は accepting chain block の DAA。元の署名 frame は変更しない。次の候補を全て検査し、全件成功した場合にだけ同時に記録する。

1. claim/ruleset/source root/profile を固定し、当該 claim について許容する要求の閉じた集合と、その canonical な導出手順を固定する。モデルの審査・取得可能性判定は行わない。
2. 開示・各手番・最終 fragment・検証・回復・保持の最大資源を求める。bytes、検証 work、IO、保持 byte-DAA、control 数を別次元で検査する。未導出の項が一つでもあれば容量保証は未完成。
3. `a + 10,000` と後続責任を最短の署名済み依存 horizon が覆うことを確認する。bond の別 claim、Gate C、post-Final 告発、monetary material の責任を残す。
4. 既存予約を移動せず、全許容分岐と最遅申立てを含む候補予約を追加できることを検査する。全 claim が同時に争われる場合を計上し、期待参加率や確率的 overbooking で代替しない。
5. claim、予約、課金、保持責任、signer、通常窓、hard deadline を一つの状態遷移で記録する。途中失敗では一部を残さない。

容量の数値、turn duration、fee は測定・承認待ち。特定モデルの除外や上限引き上げを達成手段にしない。必要な原資・転送量が期限に収まらない profile は「G14 未達」と報告し、別 profile で達成したことにすり替えない。

## 2. 予約サービスの identity と割当

義務 identity は `(claim, ruleset, source root, dispute family, range/coordinate, turn, response kind)`。owner・送信 peer・relay・受付の競争順を含めない。責任鍵、必要資源、適格応答の認証・数値検査、開始条件、元の期限、hard deadline を義務に固定する。

scheduler は合意状態の予約だけを読み、期限順、original acceptance、claim ID、義務 identity の全順序で割り当てる。後着の課金済み court や別 root によって既存予約を追い出さない。署名者の追加同意・Panel の完了申告・mempool 観測を入力にしない。

DAA 単位の供給を保証する profile では、同じ DAA の最初の accepting chain block だけを供給単位とし、同一 DAA の追加 block を重複計上しない。DAA が飛んだ区間は供給ゼロとして検出する。この供給単位の選択は Proposed であり、現行 block budget の値を無断転用しない。DAA 当たり供給と block 当たり予算が整合することを実測・導出する。

## 3. 「実供給」の意味と inclusion 前提

供給単位の block から、義務ごとの割当、専用の未使用 quota、既収録の適格応答、codec/work 上限を再計算する。予定 quota を通常 tx・他 court が消費した場合や、必要な制御・応答全体が入らない場合は供給済みにしない。無効 codec、偽署名、重複、別 scope の応答は適格な消費に数えない。空の予約枠を別用途に売って供給済みと表示しない。

これは未収録の送信を観測する仕組みではない。quota が確保された block に応答が載らなかったことから、送信者の回線妨害まで否定できない。default の保証範囲には「責任 signer が期限内に適格応答を作成して所定の配送境界まで渡した場合、予約された機会の内側で block producer が公平に収録する」という inclusion 前提が必要。配送境界、最大遅延、機会数、敵対 block producer の許容範囲は未決で、ローカルの送信履歴を合意事実として扱わない。明示的なこの前提なしに安全な無応答処罰が完成したとはしない。

供給を示す署名付き第三者証明や Panel vote は追加しない。供給の記録は受理 block と資源規則から導出した会計であり、通信履歴やモデル計算の正しさの証明ではない。

## 4. 手番と default の判定順

同一 block では、まず block 開始時の義務・供給会計から期限と適格区間を固定する。応答自身による deadline 更新より先に、各 object がその元の適格区間に入ることを検査する。次に canonical object 順で適格応答を適用し、共有開示の再利用と手番の完了を判定し、その後に期限 sweep を行う案とする。遅い応答で次手番の期限を先に作って default を逃れない。既存 fold は `sweep_deadlines` / `sweep_court_deadlines` を object 適用前に実行するので、この順序を旧 claim に適用しない。新しい profile/fence の受理規則として境界試験を通す。各期限の等号は別の境界表で採択する。既存 `collect_until` の排他的境界を notice の等号へ黙って流用しない。

default に必要なのは (i) 受理済み義務と鍵・資源・予約・完了条件、(ii) 合意で再計算した十分な供給、(iii) 明示した inclusion 前提の保証範囲内、(iv) 適格な応答不在と期限超過の全て。clock は応答不在でも進む。既に義務を果たした Miner を Panel の最終証拠不提出で slash しない。

合意で検出できる供給欠損では、slash せず Final を保留し、未供給分を既存予約の内側で再割当する。新しい deadline は original hard deadline 以下。残余で完了できなければ未解決・非 credit の復旧状態に移し、担保・証拠・依存責任を保持する。hard 到達だけで無罪/Final/返還/pruning 許可へ変えない。共有 frontier 段階 1 の blocking は残り、段階 2 の独立進行を借用しない。観測不能な個別検閲を判定できるとは主張しない。

## 5. owner から独立した要求と共有開示

original acceptance で固定した閉じた要求集合について、誰でも同じ identity の要求を提出できる。既存 owner 8 枠は、この経路の資格条件や共有回答の取得条件にしない。requester の fee/bond は搬送・濫用費用のためであり、claim の開示権を独占する権利ではない。

同じ identity の要求は一つの義務に統合する。指定 custodian の body は claim/source root/coordinate/turn に認証して保存し、誰でも再利用できる。異なる claim/root/ruleset の body は再利用しない。署名済み応答は第三者が relay でき、新しい適応的応答には引き続き責任 signer の署名が要る。資金鍵の委譲は行わない。

異なる合法 choice は全て、original に固定した許容分岐の集合の中で扱う。最初の owner が選ばなかった争点を消さない。新しい owner や Sybil の発生で予約を増減せず、要求集合・重複統合・固定割当で制限する。「最初の reserve row」「外部者専用の 1 枠」だけではこの条件を満たさない。

許容分岐が tree の全 node に広がるなら、全 node の worst case を計上する必要がある。実際には一人が一経路しか使わないという仮定で全 owner 共謀時の容量を保証しない。固定集合の導出、共有回答の state/保持上限、全分岐の費用と 3,120 / 10,000 への収容可能性はまだ実装・測定が必要であり、現段階の gate は解除しない。

## 6. 接続と検証の順序

まず義務 identity / 閉じた要求集合の導出、予約と供給の state、同一 DAA・DAA skip・同一 block の処理順を実装する。次に予約会計と実 codec bytes、専用 block budget、local pre-auth queue/work 上限を接続する。最後に default / 保留 / 復旧、Final・bond・証拠・pruning、snapshot / IBD を一貫させる。

T02/T03 で永久沈黙・偽署名・架空完了を分離し、T04 で通常 tx・有料 court・不正 codec flood を分離する。T05 は全 owner 枠の共謀占有・埋め直し・全許容 choice・重複 root のもとで、未完成証拠の外部者が開示から裁定まで進めることを実証する。T06/T07 は最大合法遅延と各境界、T09/T13/T14/T15 は契約 horizon・責任・復旧・旧 claim を検証する。単体 scheduler の試験を node-level 達成と表示しない。
