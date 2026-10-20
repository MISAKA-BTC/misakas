# RFC-0030 — court終了効果と未解決責任の保存

Status: **Draft / Proposed / Review Required** (2026-10-21)。利用者はAの詳細仕様化・隔離実装を指示。本番採択・実装済み・有効化を意味しない。

正本は[RFC-0026 §4.3](0026-court-policy-master-contract.md)、[RFC-0014](0014-panel-independent-fraud-prosecution.md)、[RFC-0023](0023-unresolved-claim-liability-and-independent-progress.md)。[ADR-0197](../adr/0197-court-end-effects-and-pending-liability-archive.md)で新候補の方式を比較する。source基準は隔離候補`c54ba38fd`。既存の署名責任・料金・wireを変更する採択ではない。

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

別のversioned namespace `court-end-archive/v1`を候補とする。wire ordinal/table IDは本RFCでは未割当。旧key、row、tag230、X3 container、profile identityを読み替えない。

1. 通常native verifierが証拠を検証し、実際のaccepting block/DAAと当事者/claim/original/rulesetを確定する。呼出者が渡した`guilty`や`can_default`だけでarchiveや徴収を作らない。
2. active sessionを削除する前に、元session、embedded phaseと別tableのTIR phase、両sideの残存groupと受理済みchunks、宣言者/outpoint/deposit、署名済み元契約、確定した終了証拠の実bytesまたは同じ保持保証のある認証済み参照を捕捉する。digestだけでは資料の取得可能性を満たさない。
3. 別々の`adjudication`と`pending obligations`を保存し、確定責任のmarker/徴収、archive作成、必要な資本holdの移行、active索引/slot解除を同一checkpointで適用する。archiveや資本の確保が不成立なら全体rollback。ただし受付後に正しいfraudを容量不足で止めないため、次節のoriginal事前予約が必須。
4. 未決の他方groupを保存しただけでdepositをburn/返還したり、amountをescrowと呼んだりしない。既に適格証拠で確定した同一sideの責任は、pendingに戻して二重処理しない。
5. 終了記録は再開可能なactive courtではない。同じsession/side/revisionを上書き・再利用して未決を消さない。新しいsession IDの大量作成や「終了して枠が空いた」ことによるarchive無限増殖を、originalで予約した全合法graphと消費済みidentityで防ぐ。
6. snapshot/restart/pruned IBDは、元claim/frame/service binding、終了point、phase/chunk/署名のscope、確定/未決の排他性、全金融holdと索引を再構築する。undoは元active状態と資本へ、replayは同じarchive/rootへ戻す。受理済みblock全体の証拠bytesを参照するなら、その保存/取得保証もprofileに含める。
7. pruning/closeを、裁定終了・金融精算・外部提供期限・state資料保持の別条件で検査する。全条件を満たすまでarchive削除やbond exitをしない。既存署名の約束より長い外部提供/課金を遡及導入しない。

これは方式の具体化であり、現実装の紹介ではない。現在の候補にarchive row、financial migration、public coordinator、解除validatorは存在しない。

## 5. original受付と資本・容量の条件

Aは「後で保管先を探す」案ではない。新rulesetのoriginal受付で、許容する全敵対的分岐、各Court/side/phase、完了・棄却・中立終了・供給不足・hard後未決、重複統合/並行性を含め、activeとarchiveが重なる原子遷移の最大bytes/work/IO/memory/byte-DAA、署名検証、journal、root計算、snapshot/reorgを積算する。

各役割の責任額・最終保持条件・資本holdを新しい署名契約と同じprofileに束縛し、既存のclaim予約資本・fee立替・他claim責任・新Account Bond案の責任を混同しない。各金融入口で同じ資本を再利用できないことが要る。未校正の費用・期間・branch数を0にせず、original受付/武装を拒否する。signature期限が足りなければ新しい適格契約が必要で、旧pledgeの上書きでは補わない。

**保持期間の未被覆を隠さない。** service/復旧が永久に成立しない条件では、hard=10,000からarchiveの有限な解放時刻や有限のbyte-DAA総量は導けない。有限の上限を使うprofileは、その成立に必要な配送・収録・復旧の前提と、前提崩壊時の保持/資金/全体受付制限を別に示す。archiveの未決予約枠をactive session終了だけで再利用し、未決行を無限に積み上げない。保持が無期限になり得る条件を有限額で全費用支払済みと扱わず、未被覆のままなら新original受付/武装を拒否する。10,000を実測の返還時刻や自動pruning時刻へ読み替えない。

ローカルのmemory/IO不足と、合意上の有料容量上限違反も区別する。validな既受理証拠を、あるnodeのローカル資源不足だけで不正と裁定・absence slashする規則にしない。元profileの費用とnative resource policyが未校正なら、その条件を未被覆として残す。

この完了前のprivate adapterは、完全な有料受付やG14を達成したものとしない。上限引上げ、モデルの除外、二階層の再審、禁止した計算証明、新fee/返還期限の仮採択で成立させない。

## 6. 比較案と採択境界

Bはdeclaration時の実費前払と違反罰を別料金にする案。実在する支払/償還財源、全対話費用、署名者の同意、保存費用・未決金融責任、二重課金/rollbackを新規に設計する。既存のdepositを単に「費用だった」と呼んで常時徴収する変更ではない。値は未決で、このRFCは設定しない。

A/Bとも新ruleset/profileの候補で、旧claimの経済規則を変更しない。詳細仕様化・隔離実装の判断、本番採択、pre統合、有効化、配備を別々に記録する。採択まで現行候補の拒否gateを維持する。

## 7. 検証条件（未実施）

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
