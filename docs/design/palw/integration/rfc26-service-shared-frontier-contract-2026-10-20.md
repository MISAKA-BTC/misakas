# RFC-0026 F02/F03 — 予約サービスと共有 frontier の詳細契約案

**状態:** Proposed / Review Required。2026-10-20 ユーザー指示「推奨案で詳細仕様化を進める」に基づく。[ADR-0192](../../../adr/0192-court-service-opportunity-and-shared-pursuit.md) A/A の詳細化。採択・実装完了・統合・有効化ではない。[RFC-0026](../../../rfc/0026-court-policy-master-contract.md) の 3,120 / 10,000 と旧 claim の ruleset を維持する。

## 1. 原子的な original acceptance

`a` は accepting chain block の DAA。元の署名 frame は変更しない。次の候補を全て検査し、全件成功した場合にだけ同時に記録する。

1. claim/ruleset/source root/profile を固定し、当該 claim について許容する要求の閉じた集合と、その canonical な導出手順を固定する。モデルの審査・取得可能性判定は行わない。
2. 開示・各手番・最終 fragment・検証・回復・保持の最大資源を求める。bytes、検証 work、IO、保持 byte-DAA、control 数を別次元で検査する。未導出の項が一つでもあれば容量保証は未完成。
3. `a + 10,000` と後続責任・証拠保持を最短の署名済み依存 horizon が覆うことを確認する。`max(hard + required_followup, hard + reorg_retention)` を checked arithmetic で導出し、一方が短いことで他方を省かない。bond の別 claim、Gate C、post-Final 告発、monetary material の責任を残す。
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

## 7. 供給会計部品の実装境界

`palw_court_service_v1` の `observe_block` は、予約済み packet の全 quota を専用 court budget から差し引いた後、他の court 操作の実コストが残余に収まることを検査する部品。合意で観測できる accepting DAA と block identity を記録し、同じ DAA の追加 block は供給回数を増やさない。未供給 DAA を飛び越しても走査せず、割当先に記録がない obligation を `MissingSupply` と判定する。予約のない DAA は clock だけを進め、履歴 row を確保しない。これにより supply row 数は予約された packet 数を越えない。

既に最初の block を観測した DAA へ予約を後付けすることを拒否する。新規 original acceptance が同一 DAA の後続 block に載る場合、profile は次の未観測の供給単位以降へ予約し、original acceptance 起点の 3,120 / 10,000 は変更しない。予約への追加と block 観測の失敗はどちらも部品の state を変更しない。

`assess` の `OpportunityProvided` は全 default 条件ではない。署名・手番・応答の適格性・inclusion 前提を確認せず slash する API は追加していない。`MissingSupply` は責任帰属に使えず、実 fold では Final/担保/証拠の保持と復旧へ接続する必要がある。

この部品の `unreserved_court_cost` は producer/Panel の申告値を受理する wire ではない。実 codec からのコスト導出、通常 tx と court quota の物理的分離、予約 packet の scope と実コスト検査、book の候補 row/journal は §8 で追加したが、production admission・供給記録・full snapshot/IBD、補正期限・Final hold の接続は未実装。この接続前に quota が実提供されたと本番で認定してはならず、武装拒否 gate は維持する。clone による部品の rollback 試験を node の reorg / restart / pruned IBD の検証と混同しない。

## 8. Rooted deny-only adapter の実装候補

`palw_court_service_state_v1` は一つの global な `ServiceReservations` と、元の claim/allocation/source/acceptance、固定 `+10,000` hard deadline、packet 集合の binding を保持する候補。予約 packet を複数 claim の保証へ使い回さず、未 binding の packet や残余 quota の不整合を拒否する。復元時の `check` は割当と各資源の残余を再計算し、記録された残余を信用しない。

既存 `staged_courts` へ、key tag 11 / row tag 17 を末尾追加した候補である。既存 tag、lifecycle tag 230、旧 frame codec の意味は変更しない。typed native heap 会計と allocation-free wire shape preflight も追加。native writer の journal は既存 delta 200 を使い、row の書き換え・undo/replay を同じ経路で扱う。これは新 schema の採択ではなく、休眠した実装候補。preset、原子的 original admission、accepting-block 供給会計からこの row を作成・更新する production 経路はまだない。

native TIR fold では、pure default transition が返った後、row の書き換え・経済効果の前に、この ledger に binding のある claim を検査する。供給欠損・未完了・不整合では default を拒否する。全 packet が供給済みでも、duty ごとの適格応答と採択済み inclusion 前提が未実装なので default を許可しない。wire の bool や Panel の申告でその拒否を解除する経路はない。binding のない既存 claim は従来の規則を維持する。

この段階の対象は **default を実行しない検査と、その参照 state の永続化**。現在の owner demand と owner 非依存の義務 identity の導出、Final/担保/証拠/pruning の保留、残余容量内の再割当、hard 到達後の未解決・非 credit 状態、全 claim の admission、実ノードの restart/pruned IBD は未完了である。default 拒否だけで Final 保留や P0 完了まで実装したとは表示しない。原子的 admission と専用 fence/profile の採択前にこの schema を本番で生成・受理するルールへ接続しない。

別 branch `codex/rfc14-x3-reorg-clock` / `4f41081ee` の transport 会計も未統合。そちらは bytes/work/memory、この予約部品は bytes/work/IO/control の独立次元である。memory を IO に読み替えず、両方の費用・保持 byte-DAA と物理容量を含む profile を導出する必要がある。

## 9. 未解決 binding の保持候補

`CourtServiceState` に binding がある claim は、この未採択候補では unresolved のままとする。quota 提供だけで解除する bool、時刻経過による解除、hard deadline を最大値へ書き換える処理はない。claim の自動終了 deadline を `None` とし、native Final/retirement の入口、期限 sweep、非 fraud の pending-accusation 判定を同じ rooted predicate で保持する。元の notice / hard / signed frame の期限値は動かさない。他 claim の deadline を書き直さない。

元 frame の qualified custody signer は exit-held のままにし、関係しない owner / 未 credit の seat をその claim の責任者へ加えない。panel liability / Valid lock の時刻 pruning も unresolved binding を飛ばす。TIR frame、preparation、acquisition、raw chunk、accepted body、staged witness の TTL sweep と直接削除を保持する。期限切れの保持 row を sweep の 64 件枠より先に除き、別 claim の回収枠を保持 row だけで埋めない。これは state 内の証拠保持であり、署名済み horizon を超えて外部 provider に新しい配送義務を作る規則ではない。

book の writer と state の整合性検査は、binding が元 frame と claim を失った場合、元の allocation/source/acceptance を替えた場合、unresolved のまま Final になっている場合を拒否する。snapshot からの復元時も同じ条件と derived deadline を再計算する。新規 field/tag や preset の変更は追加していない。

これは保持を接続する実装候補で、正常終了時の解除や不足分の再割当を完成したものではない。解除の責任判定、正当な裁定ごとの解消、hard 後の明示的な復旧、actual original admission での契約 horizon 検査と profile/fence の固定、node の restart/pruned IBD、費用・保持上限の計測は残る。永久に止めることを G14 の達成へすり替えない。production writer は依然なく、旧 claim に新しい署名責任を後付けする入口は設けない。

保持中の分割 witness も、全体を assemble/decode したら従来どおり deterministic fraud 裁定へ進む。保持を理由に裁定そのものを拒否しない。新 codec へ黙って読み替えず、V1 が禁止する completed witness row は生成しない。保持中の既存 partial prefix は残し、完成した evidence を裁定へ渡す。binding のない旧 claim では従来どおり row を消費する。裁定後の prefix の archive/解除は未採択の復旧・終了効果契約に残し、partial prefix が完成証拠全体の永続保存であるとは表示しない。

## 10. 供給欠損の再割当候補

`ServiceReservations::repair_missing` は book の観測 DAA より前に割り当てられ、供給 row のない packet だけを対象にする。現在/将来の予約と供給済みの機会は移動せず、供給記録も消さない。再割当先は観測 DAA の次以降で、元の `release_daa..service_before` 内、かつ original hard 以下。packet の identity、費用、適格区間、hard を変更しない。過去の未供給 quota を戻しても `reserve` の後付け禁止は変わらず、過去枠を再販売する経路にはならない。

同じ claim の不足 packet は全件成功時だけ移動する。一部だけ仮割当できても残りが収まらなければ候補を捨てる。`NoCapacity` / `DeadlineExhausted` は未解決のまま保持するための結果で、Miner/Panel の勝敗・guilt・無罪・返還を表さない。他の予約を追い出したり、区間や物理予算を広げて成功扱いにしない。

`CourtServiceState::repair_missing_supply` は全 binding を original hard、original acceptance、claim ID の順、各 claim の不足 packet を `service_before`、packet ID の順に処理する候補。group ごとの原子性と保守的な first-fit を用い、最適な充填や全義務の期限内回復を保証するアルゴリズムとは表示しない。一つの claim の回復失敗後も、他 claim の既存予約は保持し、残余に入る回復だけを記録できる。これは quota 会計の更新であり、RFC-0023 段階 2 の独立決済や shared prefix の解放ではない。この group 優先順位は Proposed の詳細案で、global な packet EDF の最適性を採択したものではない。

native の `recover_court_service_supply_v1(ctx)` は book の観測 DAA が native accepting context と一致しなければ拒否する。古い supply clock から、実際には過ぎた枠を将来枠と見なさない。成功時の global book 書き換えは一つの既存 delta 200 を使い、変化がなければ新しい journal row を出さない。report は wire の権限や completion flag ではない。production block/admission からの caller はまだなく、actual admitted codec/resource から供給を導出する接続も残る。

この候補は **既存の適格区間内** の retry だけを扱う。応答 deadline の補正・全時計の原子的な更新、完成済みの適格応答との対応、署名済み依存 horizon、old/new ruleset の fence、hard 後の recovery と終了効果は未完了。元の区間内で不可能な場合に deadline を黙って延長しない。成功しても binding・Final・担保・証拠の hold と default 拒否は解除しない。実供給/inclusion の全条件と全 owner 共謀下の経路を完成したとは扱わない。

current book の assignment は再割当先を表す。変更前後は native journal の旧/新 row で保持するが、current snapshot だけが全 retry 履歴を格納する schema ではない。履歴の保持期間、node restart/pruned IBD、費用と物理容量の検証は引き続き必要で、fixture の round-trip をその代替にしない。

## 11. 依存 horizon の受付検査に向けた監査

[実コード参照の監査](rfc26-dependency-horizon-audit-2026-10-20.md)で、producer/main custody、semantic history custody、EXEC の最短 path 制約、provider lease の live 状態と exact subject を分離した。`CourtClockBounds` の保持分の検査不足は修正したが、単一の最短 horizon 引数が全依存契約を列挙した証拠にはならない。報酬・担保用の最大 horizon や、ordinary path の欠落時成功を新規 admission の保証に流用しない。全依存集合の canonical 導出、実契約の認証と live 検査、原子的な受付への接続、T09 は未完了。

## 12. Binding の署名済み custody 下限

候補 row の context 検査へ、main custody pledge が original hard を覆う条件を追加した。semantic component がある場合は、別の受理済み ClaimBundle/history pledge の存在・claim/source scope・署名済み horizon も検査する。欠落を「依存なし」と扱わず拒否する。binding の書き込みと復元が通るための最小条件であり、実 original admission の完了や followup/retention 全体の保証ではない。既存 unbound claim の規則と署名済みの値は変更しない。実コードの対象と残件は [依存監査](rfc26-dependency-horizon-audit-2026-10-20.md)を参照。

## 13. Owner 非依存の query 集合の候補

[Native TIR 要求集合](rfc26-shared-tir-request-set-2026-10-20.md)を、実 source の全許可 query shape から導出する部品として追加した。全 interior level、全 leaf、全合法 run の start/count、row/tile/IDs を列挙可能な ordinal にし、claim/source/native rules/shared profile の identity に固定する。owner/relay/先着順は入力にしない。

これは全応答義務・service packet・合法対話の全遷移 graph の完成ではない。元 source validator より狭い reachable set を採るなら、実 protocol から全敵対 choice を含めて導出する必要がある。重なる query の公開データ再利用を計上してよいが、変換・opening・署名・保持費用を省かない。実費用、応答者/turn/fragment、予約・ingress・期限/供給会計への接続と T05 は未完了であり、gate を解除しない。
