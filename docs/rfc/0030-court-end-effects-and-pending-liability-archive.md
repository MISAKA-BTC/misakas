# RFC-0030 — court終了効果と未解決責任の保存

Status: **Draft / Proposed / Review Required** (2026-10-21)。利用者はAの詳細仕様化・隔離実装を指示。本番採択・全条件の実装済み・有効化を意味しない。

正本は[RFC-0026 §4.3](0026-court-policy-master-contract.md)、[RFC-0014](0014-panel-independent-fraud-prosecution.md)、[RFC-0023](0023-unresolved-claim-liability-and-independent-progress.md)。[ADR-0197](../adr/0197-court-end-effects-and-pending-liability-archive.md)で新候補の方式を比較する。問題調査のsource基準は隔離候補`c54ba38fd`、§8の隔離実装の親は`494361118`。既存の署名責任・料金・wireを変更する採択ではない。

## 1. 問題と実コードの境界

現候補は、未解決serviceの通常court deadline/close組立sweepを保留する。しかし、`objects.rs::CourtClosed`、`convict_close_declarer_v1`、`void_claim`の中立close、`retire_claim`等は`TransitionBuilder::write_court(id, None)`へ集約する。このwriterは、court/sessionの索引とchallenger exposureを外し、残存close groupのdepositを`slash_bond`で徴収し、groupとdissectionを削除する。客観的な反証が受理されても、別の未完groupまで「未提出だった」という理由で同じ処理へ入る。自動期限の保留だけでF02/F08は閉じない。

**現depositは前払escrowではない。** `palw_close_assembly_deposit_v1`の説明と実装では、declaration時に送金せず、宣言者のbond outpointと額を記録し、後で残存collateralから徴収する。`forfeit_close_deposit_v1`は`slash_bond`を呼び、その徴収は残存collateralにclampされる。「保存したdeposit額」が、その額の資金を別勘定に確保したことにはならない。終了後も資金が必ず残ると仮定せず、他claimの最大重複責任・exit・各経済入口へ接続しなければならない。新Account Bondの実装や最低10万による十分性を借りない。

`offence_attribution_active`時の`void_claim`の「中立に他courtを終了する」経路にも共通writerのgroup徴収がある。fence下のvoidが常に全sessionを即削除するという意味ではない。TIRの直接/分割fraud裁定自体を止めてこの相互作用を避けない。既存streamed-fraud試験は、通常courtの残存groupを伴う終了全体の証明ではない。

## 2. 終了効果を分離するfield

候補は次の効果を別々に表す。単一の`closed`/`success`/`resolved`で代用しない。

| field | 意味と禁止する推論 |
| --- | --- |
| `claim_credit` | claimの非credit/void、または別の認証条件を満たすまで保留。courtの棄却だけで`SecurityCertified`、有効Final、一致したlocal replayによる合意creditを生成しない |
| `court_adjudication` | 当該争点の裁定結果。別の宣言・供給・支払義務の履行を意味しない |
| `active_session` | active slotを終了するか、hard内で復旧するか、hard後の未解決保存へ移すか |
| `established_liability` | 既存の検証済み証拠により当事者・対象・理由・額が確定した責任。offence markerで二重徴収しない |
| `pending_liability` | 未決のgroup、署名者義務、保持・費用の責任。無応答・quota・時刻だけで有責/無責へ変えない |
| `collateral_encumbrance` | 役割別・他claimを含む実資本の拘束。archived額を残高と数えず、同じ担保を複数回使わない |
| `material` | 本物の終了証拠、未完prefix、元契約・署名・phaseの保持先。部分prefixを完成証拠全体と呼ばない |
| `shared_frontier` | 確定済み知識と未処理義務を区別して保持。同じrootの別の合法争点を消さない |
| `pruning` | 全未決責任の解消、採択した保持期間、他claimのholdを検査して初めて許可。terminal phaseだけでは許可しない |

## 3. 終了理由ごとの効果表

以下は新service ruleset用の候補で、旧claimの経済規則を後付けで変えない。検証済みの客観的事実と、供給/配送/収録前提の下でしか確定できない欠席を区別する。

| reason | claim credit | court/復旧 | 確定責任・担保 | 未決責任・証拠 | frontier/pruning |
| --- | --- | --- | --- | --- | --- |
| 計算不正を客観的証拠で確定 | 当該claimは非credit/void。計算不正を別の署名者へ自動帰責しない | 当該裁定を成立させ、不要になったactive sessionを終了 | 対象となるproducer等の既存確定責任を一度だけ適用 | 他方の未完groupは自動defaultせず、元契約とともにpending保存。false-Validは別の適格証拠が要る | 確定反証と未決義務を別に保持。未決を含む全条件までpruning不可 |
| 合法な申立てを証拠で棄却 | claimの計算正当性・合意creditは未認証のままなら保留 | 当該争点を終了。他の合法争点を妨げない | 当該rulesetで確定するchallenger負担だけ適用。不正入力を受理して無関係な鍵へ課金しない | 別groupや依存義務の解消を推定しない | 元の共有知識・別争点を保持。判決だけで全資料を消さない |
| 開示・応答義務違反 | 計算不正とは別のdefault reason。適格性がない現候補では非credit保留 | 実供給、元の署名者義務、期限、配送/収録前提を全て判定した場合のみ違反終了。それ以外は復旧/未決 | quotaや不在だけでslashしない。将来の採択済みdefault validatorの結果が必要 | pending義務・資料を保持。boolや呼出者申告を許可証にしない | hard外へ時計をresetしない。pruningは全解消後 |
| Panelの放棄 | producerの計算不正・署名者のfalse-Validとは推定しない。非credit保留 | 予約済み合法graph内の復旧。未予約の代替Panelや署名を捏造しない | 客観的に確定した責任以外はpending | 放棄時点の契約・既存応答と残義務を保持 | stage1の共有frontierを保持。stage2独立進行を既実装としない |
| service前提の崩壊 | 非credit保留 | hard内なら合意状態から有料・有界に再割当。hard後は明示的復旧契約待ち | 機会を与えられなかった者へ無応答slashをしない | 未決担保・証拠を保持。供給不足を無責任な自動返還へ変えない | 障害中の`L_class`内前進や無条件の返還期限を保証しない |
| hard deadline到達 | 有効Final、自動無罪、計算不正へ変えない | activeな旧時計を延長せず、未決状態を保存。新復旧は別の採択済み有料契約 | 時刻だけではslash/返還しない。確定済み責任は一度だけ保持/精算 | 新しい外部配送義務を旧署名へ後付けしない。state内保持と提供者の約束を区別 | 全未決を残し、必要な保持とpruning条件を明示 |
| 認証済みの完成assemblyが元の宣言と不整合 | 計算不正の証拠と同一視しない | 完成bytes/宣言scope/rulesetに対する客観的不整合だけを裁く | 当該宣言者の適格な確定責任だけ適用 | 他方の未完groupは別pending。完成していないことを不整合の証拠にしない | 元の完成bytesと不整合箇所を保持。再徴収不可 |

## 4. 推奨A:裁定とpending保存を原子的に分離する

別のversioned namespace `court-end-archive/v1`を候補とする。§8の隔離実装では末尾key 20 / row 26を使用するが、公開schemaの採択ではない。旧key、row、tag230、X3 container、profile identityを読み替えない。

1. 通常native verifierが証拠を検証し、実際のaccepting block/DAAと当事者/claim/original/rulesetを確定する。呼出者が渡した`guilty`や`can_default`だけでarchiveや徴収を作らない。
2. active sessionを削除する前に、元session、embedded phaseと別tableのTIR phase、両sideの残存groupと受理済みchunks、宣言者/outpoint/deposit、署名済み元契約、確定した終了証拠の実bytesまたは同じ保持保証のある認証済み参照を捕捉する。digestだけでは資料の取得可能性を満たさない。
3. 別々の`adjudication`と`pending obligations`を保存し、確定責任のmarker/徴収、archive作成、必要な資本holdの移行、active索引/slot解除を同一checkpointで適用する。archiveや資本の確保が不成立なら全体rollback。ただし受付後に正しいfraudを容量不足で止めないため、次節のoriginal事前予約が必須。
4. 未決の他方groupを保存しただけでdepositをburn/返還したり、amountをescrowと呼んだりしない。既に適格証拠で確定した同一sideの責任は、pendingに戻して二重処理しない。
5. 終了記録は再開可能なactive courtではない。同じsession/side/revisionを上書き・再利用して未決を消さない。新しいsession IDの大量作成や「終了して枠が空いた」ことによるarchive無限増殖を、originalで予約した全合法graphと消費済みidentityで防ぐ。
6. snapshot/restart/pruned IBDは、元claim/frame/service binding、終了point、phase/chunk/署名のscope、確定/未決の排他性、全金融holdと索引を再構築する。undoは元active状態と資本へ、replayは同じarchive/rootへ戻す。受理済みblock全体の証拠bytesを参照するなら、その保存/取得保証もprofileに含める。
7. pruning/closeを、裁定終了・金融精算・外部提供期限・state資料保持の別条件で検査する。全条件を満たすまでarchive削除やbond exitをしない。既存署名の約束より長い外部提供/課金を遡及導入しない。

以上は目標契約である。§8の部分実装はarchive rowと名目exposure/exit holdを接続するが、完全な金融予約移行、public coordinator、解除validatorは未実装である。

## 5. original受付と資本・容量の条件

Aは「後で保管先を探す」案ではない。新rulesetのoriginal受付で、許容する全敵対的分岐、各Court/side/phase、完了・棄却・中立終了・供給不足・hard後未決、重複統合/並行性を含め、activeとarchiveが重なる原子遷移の最大bytes/work/IO/memory/byte-DAA、署名検証、journal、root計算、snapshot/reorgを積算する。

各役割の責任額・最終保持条件・資本holdを新しい署名契約と同じprofileに束縛し、既存のclaim予約資本・fee立替・他claim責任・新Account Bond案の責任を混同しない。各金融入口で同じ資本を再利用できないことが要る。未校正の費用・期間・branch数を0にせず、original受付/武装を拒否する。signature期限が足りなければ新しい適格契約が必要で、旧pledgeの上書きでは補わない。

**保持期間の未被覆を隠さない。** service/復旧が永久に成立しない条件では、hard=10,000からarchiveの有限な解放時刻や有限のbyte-DAA総量は導けない。有限の上限を使うprofileは、その成立に必要な配送・収録・復旧の前提と、前提崩壊時の保持/資金/全体受付制限を別に示す。archiveの未決予約枠をactive session終了だけで再利用し、未決行を無限に積み上げない。保持が無期限になり得る条件を有限額で全費用支払済みと扱わず、未被覆のままなら新original受付/武装を拒否する。10,000を実測の返還時刻や自動pruning時刻へ読み替えない。

ローカルのmemory/IO不足と、合意上の有料容量上限違反も区別する。validな既受理証拠を、あるnodeのローカル資源不足だけで不正と裁定・absence slashする規則にしない。元profileの費用とnative resource policyが未校正なら、その条件を未被覆として残す。

この完了前のprivate adapterは、完全な有料受付やG14を達成したものとしない。上限引上げ、モデルの除外、二階層の再審、禁止した計算証明、新fee/返還期限の仮採択で成立させない。

## 6. 比較案と採択境界

Bはdeclaration時の実費前払と違反罰を別料金にする案。実在する支払/償還財源、全対話費用、署名者の同意、保存費用・未決金融責任、二重課金/rollbackを新規に設計する。既存のdepositを単に「費用だった」と呼んで常時徴収する変更ではない。値は未決で、このRFCは設定しない。

A/Bとも新ruleset/profileの候補で、旧claimの経済規則を変更しない。詳細仕様化・隔離実装の判断、本番採択、pre統合、有効化、配備を別々に記録する。採択まで現行候補の拒否gateを維持する。

## 7. 検証条件（全条件の合格は未達）

以下は既存T01–T15を置換しないF08の追加条件である。

- E01: 全6終了理由と完成不整合のfield別効果。客観的fraudが成立しても無関係な未完groupへabsence slashしない。
- E02: 両side、embedded/別TIR phase、最大合法chunk prefix、完成証拠全体を実native verifier/有料受付からarchiveへ移す。
- E03: 判決/徴収/hold/active解除/archiveの各失敗点が全体rollback。完成sideの責任・他sideの未決・offence markerを二重適用しない。
- E04: 同一originalの全合法branch・同root重複・再開/再投稿・新session IDによるarchive枠の無限生成を防ぎ、正直な外部者の枠を先着者へ渡さない。
- E05: producer/challenger/宣言者/Panel/custodian/relayを区別したexitと他claim重複資本。未知額・短い署名・欠けた資金は拒否。
- E06: hard前後、notice境界、最後の適格応答とsweep順、配送/収録前提崩壊、既存契約の期限超過を区別する。
- E07: reorg/restart/pruned IBDから同じ金融・資料・索引/rootへ復元。改竄/欠落/孤立/archive消去・再利用を拒否。
- E08: 元complete証拠が取得不能なのにdigest/部分prefixだけで保存済みとしない。全保持/pruning条件とtyped heap/wire preflightを同時検査。
- E09: serviceなしの旧claimは既存のW10/ADR-0152等に従い、t12 identity/preset/codecをrepinしない。
- E10: 同じrelease buildでT01–T15・実モデルdoor≤3,120・完全有料graphの条件を満たすまでG14完了にしない。

## 8. A案の隔離実装 — 2026-10-21

親`494361118`から、service bindingのあるcourt削除にprivate archive adapterを加える。新key 20 / row 26を既存enumの末尾へ追加し、`court-end-archive/v1`と元service profileからarchive identityを導く。既存ordinal、X3 key 11 / row 17、service key 12 / row 18、tag230、preset/identity pinは変更しない。このidentityは料金・容量が校正されたprofileの認証ではない。

共通`write_court(None)`は、元session・embedded/別table phase・両sideの残存groupと受理済みchunkを捕捉してから、active索引とphase/groupを除きarchiveを作る。全体をcheckpointで戻せる。正常なnative `CourtClosed`は実objectと受理pointを、直接/分割TIR convictionは最後のfragmentだけでなく検証された全decoded evidenceとpointを保持する。元宣言の全raw署名は旧group rowに保存されておらず、このadapterが署名契約全体の保存を達成したとはしない。

完成した不整合assemblyでは、最終chunkも原子的に保存してから、当該sideの既存徴収を一度適用する。`applied_deposit_debits`は残存collateralでclampされた**実際のdeposit徴収額**であり、名目額の全額確保や他の既存罰の総額ではない。反対sideの未完groupを同時に徴収しない。正常に完成してnative closeを通ったgroupもraw bytesを残し、`fulfilled_groups`で未決depositから区別する。chunk全体のhashと宣言scopeを検査し、typed objectの再encodeが同じbytesになることを新しい受理条件にはしない。旧readerが許す表現を勝手に狭めない。

終了causeをまだ捕捉していない内部経路は`Uncaptured`を明示し、未知の判決・受理pointを補わない。この場合と、当該courtの裁定まで確定しない関連native causeでは、元claimの実reserved court stakeを名目holdへ残す。archiveのread-only dispositionはdefault・credit・解除の許可証ではない。snapshot検査もnative adjudicatorの再実行全体を代替しない。

archiveの未決group名目額を既存accuser exposureへ加え、該当bondのexitを止める。これは同額のescrow口座を新設・入金したことでも、全金融入口の資本保存則を満たしたことでもない。元claim/frame/service bindingと最低material期限をcontext検査し、旧hard/material TTLを過ぎても保持する。未決archiveの上書き・削除・型置換、同じsession IDの再利用を拒否する。料金、署名済み期限、hard、上限値を変更しない。

追加5試験は、小型のverified-half native fixtureで、実際のlocal legacy v2 adjudicatorによるCourtClosed、完成不整合chunk、正常完成groupのraw保存、Uncapturedのstake保持、分割TIR反証の全証拠保存を検査する。nodeはv5でheld context/各fork fence等も検査しており、この小型fixtureがその公開受付を通ったとはしない。将来のarchive resolverも元の適用規則を束縛し、v2への降格で置換しない。各経路でcarriage復元、typed heap/wire reader、期限後保持、元rootへの全undoとreplayを確認する。fixtureのservice cost・carrier署名・元declaration設置は実資金original受付の証拠ではなく、E01–E10全体や最大合法phase群の成功を主張しない。

**次の未完:** 終了後の反対sideが、元宣言に束縛された完成bytesを提示し未決を解消するvalidator/経路が必要。現adapterではactive sessionがないため旧CourtCloseChunkへ出せず、holdが続く。元rulesetのCourtParams/fee/署名scopeを取得できなければ、現在のdefault paramsや仮値で再裁定しない。全金融入口、全合法分岐の予約、実署名の保存、資料提供と保持費用、qualified settlement/pruning、実供給・公開ingress、最大family/phase、T01–T15と実doorは未完。無期限保持を完成と呼ばず、未校正の保持期間/費用を0や10,000で置き換えない。


| 条件 | 現在の検証範囲 | 残る条件 |
| --- | --- | --- |
| E01 | direct close、完成不整合、streamed TIR fraudの小型native fixture | 全6理由と全actorのfield別効果 |
| E02 | 両groupと完成raw bytes、full decoded TIR evidence | 実署名・実資金受付、最大prefix、embedded/別TIR phase全family |
| E03 | archive改変/削除/再利用拒否、重複chunkのrollback | 各書込・金融移行点の資源失敗を注入した原子性 |
| E04 | 同じsession IDの再利用拒否 | 全合法branch/並行性/重複統合の予約、無限archive生成対策 |
| E05 | pending nominalのexposureとexit hold、実deposit debit | 全役割/他claim/全金融入口と資金保存則 |
| E06 | hard/material後の保持とTTL sweep | notice/最終応答/配送収録/元契約境界、適格な解除 |
| E07 | 小型fixtureのcarriage復元・全undo/replay/root一致 | 実DB restart/pruned IBDのarchive経路、各孤立・欠損の否定試験 |
| E08 | 実raw chunk/全decoded evidence、typed heapとwire scanner | 完全な元署名契約、全原資/最大資源/保持・取得保証 |
| E09 | 既存ordinary courtとt12同一性の回帰対象 | 公開service受付導入時にも旧claim分離を検証 |
| E10 | 有効化拒否を維持 | T01–T15、実door、全有料graphの元の完了条件 |

「全family/全合法分岐」は当面有効な経路の範囲であり、延期中のRFC-0003画像・動画とRFC-0004モデル改善を有効化・迂回開放する意味ではない。


## 9. 宣言時の署名・native裁定入力の保存候補

親`e2e367e98`。終了後の未決解消validatorに先立ち、新しいprivate `CourtCloseReceipt`（末尾key 21 / row 27）でnative受付時の契約資料を保持する。archive key 20 / row 26を含む既存ordinalは変更しない。Aの隔離実装の続きであり、公開schemaや終了後受付の本番採択ではない。

serviceに束縛された`CourtCloseDeclared`が通常のnative署名検査を通る経路で、完全な署名済みobject、宣言者の当時の公開鍵、実point、元session、embedded/別table phase、original/service scope、最低material horizonを保存する。receiptは署名のdomainを既存のdeclaration contextで再確認し、元groupのcount・各digest・whole digest・verdict・時刻・当事者と一致させる。active courtとarchiveのいずれか一方に対応するgroupが必要で、別のnative処理や任意row置換で過去のreceiptを捏造しない。receiptは前向き上書き・削除・retypeを許さず、service未解決の間はTTL後も保持する。

native processorは、実際のv5呼出しが読むCourtParams、leaf ladder、prompt形、held regime、offence-attribution、FP decode/cell closure/memory routingのfork境界と、TIR/各proof-familyの有効条件を、その受理pointとnetwork domainからextrasへ渡す。carrierの自己申告を使わない。元の証拠を異なる条件で後から裁定しないための履歴入力であり、この記録だけで新しい配送義務・無応答default・遡及罰・返還を発生させない。旧nodeの「後日のblockで証拠を検査する」規則を新しい固定条件へ置換する採択でもない。

native入力がないfixture/内部経路は`rules=None`を明示し、current/default paramsへfallbackしない。旧groupに署名や条件を後付けし、完全なreceiptとして扱う経路は作らない。条件の保存と、その条件を使う権限ある終了後validator、実費予約・決済、pruningは別の段階である。元宣言資料の保存に加え、将来のresolverでphase・class/claim binding・native feature gate・v5裁定・全金融効果を一体で検査する必要がある。

追加の署名再検査、署名/公開鍵/session/phaseのcloneとserialized保存、root/journal、snapshot/reorg、保持byte-DAAは全分岐profileの追加費用で、0としない。料金・上限・署名期限・preset/identity pinを変更せず、全original予約/公開ingress・E01–E10全条件・T01–T15・実doorが閉じるまで有効化保留を維持する。

署名が認証するのは既存declarationのfieldであり、保存したnative rulesは合意上の受理履歴である。宣言者が料金・fork条件・終了後の新しい外部義務へ追加署名したと見なさない。gen/improvement等の条件を記録しても、延期中の機能を有効化する効果はない。


## 10. 保存した入力によるprivate v5再検査候補

親`c153ecc02`。`verify_archived_close_v1`は、元archiveの未決side、署名済みreceiptとnative入力が存在する場合だけ完成groupを検査する。count、各digest、whole digest、宣言者、outpoint、verdict、deposit、元の期限を変更できず、既に受理したchunk prefixを消去・差替えできない。完成bitmap/全index・各chunkの既存サイズ上限を確認する。native入力があるreceiptには、既存nodeと同じ宣言chunk数の上限検査も追加する。上限の変更ではない。

完全なraw bytesを署名済みhashと照合し、native wire readerでheap shapeを調べてからdecodeする。typed objectの再encode一致を新しい条件にはしない。元のscopeとverdict、TIR/各familyの有効条件を検査し、宣言時のsessionとembedded/別table phaseを一時viewへ戻して、保存した全入力で**既存のadjudicate_court_close_v5**を呼ぶ。v2へfallbackせず、現在のextrasで欠けた入力を補わない。返った裁定が宣言と異なる場合は拒否する。

一時viewはlocal inspection用で、active/archiveが共存し得る検査scratchである。新しい合意状態としてcommit・journal化せず、元state、担保、claim credit、service/frontier、責任の未決/確定状態を変更しない。結果はprivate constructorだけを持つ非Borsh・非Cloneの型で、元state root、session/side、raw group、decoded close、実v5結果を束縛する。将来の金融writerはstate rootの再確認に加え、独立した有料契約・実費/原資・一度だけの終了効果を検査しなければならない。この型や一致したlocal replayだけでSecurityCertified・credit・default・解除を許可しない。

メッセージ/組立buffer・group/session/phase cloneとBTree membership、decoded proof、全stateの検査用copyを事前quoteし、native trackingが有効な場合は既存のconstruction ownerへadmitする。実readerのCPU予算超過・shape算術/メモリ不足は`NativeResource`として伝播し、証拠不正やabsence slashへ変えない。syntax/truncationは構造上の拒否である。RSS/allocator/全familyの物理上限や実CPU費用が校正されたという主張ではなく、全有料profileへの費用追加が必要である。

これは宣言時の条件による**privateな履歴再検査**であり、旧公開nodeが後日の検証blockの条件を読む規則を置換する採択ではない。終了後の公開受付で適用する時点/規則、遡及しない署名責任、精算・pruning・資本経路は未決のまま残す。wire、schema ordinal、cap、fee、署名期限、preset/identityを変えず、武装/統合/配備を行わない。


## 11. 借用recordを使う共通v5 kernel

親`54c0081d6`。§10の初版で検査用に全stateをcopyした部分を、session/別table phaseの借用へ置換する。通常の公開`adjudicate_court_close_v5`入口は既存のcost gateとsession/claim解決を維持する。private借用入口も同じcost gateを先に実行し、session/phaseのscopeとclaimを確認する。両入口は一つのv5裁定本体へ進み、TIR/Gen/Eval dissectionの3箇所だけ、phaseの取得先を引数へ置き換える。embedded attention phaseは元sessionから読む。延期中のfamilyを有効化する変更ではない。

archive adapterは、署名済みRooted receiptを認証した後にそのsession/phaseを借用し、全state copy・一時active行・session/phase cloneを作らない。宣言raw bytes、受理済みprefix、元のnative条件、Rootに束縛した結果と無金融効果の条件は維持する。借用入口自体はadmissionや署名責任の認証器ではなく、呼出元がRooted recordのscopeを検査する。callerの任意recordを金融権限へ変えない。

初版のv5本体と現共通kernelを、3箇所のphase lookupを戻してbyte単位で照合するsource auditを残す。これは全familyの実行試験や物理容量の代替ではない。group/メッセージ/組立/decodeの事前quoteは残し、Root hashing、署名/IO、内部のlegacy v5 allocationとCPUの全費用は依然として校正・全分岐予約へ含める必要がある。金融解除、公開late規則の適用時点、pruning、有料original全graphを採択したものではない。
