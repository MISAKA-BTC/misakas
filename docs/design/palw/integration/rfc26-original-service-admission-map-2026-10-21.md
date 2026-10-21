# RFC-0026 — original service admission接続のsource監査

状態: 2026-10-21、実装監査。新規則の採択・完全graphの証明・容量測定ではない。対象候補 `0988d4279`。[source hash](../../../evidence/palw/g14-integration-candidate-2026-10-20/preparation-source-scope-audit.json)。[契約案](rfc26-service-shared-frontier-contract-2026-10-20.md)の原子的original acceptanceを満たすための接続点を、現コードに対応づける。

## 現在の原子的入口

`palw_tir_public_fold_v1::prepare_original_v1` は実登録class/layout/native rules、original署名、custody pledgeとsemantic bundleを検査する。通常FPのnative validation/reservation後に `install_original_v1` が呼ばれ、同じ受理DAA・claim/root/producerを照合し、producerのwork roomから既存full depositを検査する。既存full-SUM rentはtransaction minimum fee側で徴収する前提で、fold自体はfeeを再徴収しない。semantic bundle、prompt保持、preparation、frameの書込みは既存の外側dispatcherの原子性に含まれる。

これを共有protocolの有料受付済みと見なしてはいけない。shared context・query・stream・responseはこのinstallerで作られず、実block supply由来のservice bookも設置されない。従来rent/depositと新費用の二重徴収・未徴収の両方を防ぐには、元のallocation identityから各資源成分を一度だけ導出し、実fee規則・exposureと同じ受理遷移へ結ぶ必要がある。

## 実feeの検査点

`consensus/src/pipeline/virtual_processor/processor/palw_accept.rs` の `palw_tir_public_minimum_rent_v1` と、そのcallerを直接確認した。

| 対象 | owedの実導出 |
| --- | --- |
| original FP envelope | native preclaim quoteの`plan.terms.plan.rent_sompi`。1 carrierにoriginal allocationは1個 |
| `OwnerOpen/Open` | 現在のframe/allocation/sourceを照合し、`owner_terms_v1()?.plan.rent_sompi` |
| `Conviction/Convict`、`ConvictionWitness/WitnessPart(offset=0)` | 同じscope照合後に`frame.conviction_rent_v1()` |
| 他の継続control | この追加rent helperはNone。通常transaction費用まで免除する意味ではない |

callerは実際のouter UTXO由来`carrier_fee`がunpricedまたはowed未満なら、そのallocationをfold前にdropする。control側はOpen/Convict/最初のWitnessPartに非zeroの宣言を要求し、foldは正確な額と一致を検査する。他のprepaid continuationの宣言rentは0。`PalwTirPublicControlActionV1::Convict` の古いコメントに「never carries rent」とあるが、実行される`check_payload_v1`・`conviction_unsigned_v1`・node helper・foldは上記の有料規則で一致しており、そのコメントを規則の根拠にしない。

新しい共有予約のうち何をoriginalで前払いし、どの増分を要求/終端carrierで負担するかは、この実経路と元allocationの各componentを突き合わせて固定する必要がある。元planとnew shared profileの双方をただ加算することも、old rentの支払いだけでnew費用まで予約済みとすることも、まだ正当化されていない。

## controlと費用の対応

| 現在の実体 | 導出/予約へ含める対象 | 接続上の未完 |
| --- | --- | --- |
| PreparationのWorkDebit、PreparationReady、PreparationCollect | whole-passのwork、全restart、durable generation、Readyと欠損経路、literal actor別lane | grant数だけでCPU/IO/公開bytesを置換しない。shared側の費用との再利用境界が未確定 |
| OwnedのDemand、Reacquire | 全合法座標/choice、既公開bodyの再認証・history opening・nonce | owner8枠を共有経路の資格にせず、別要求のcoalescingと全分岐量を元契約から導出する接続が未完 |
| OwnedのBegin、Part、Finish、Collect | actual body bound、canonical fragment、認証、失敗prefix、未応答/欠損時の保持 | old ownedの各actor予算を、新sharedの供給済み記録と同一視しない |
| Shared request | 元source/profileからの閉じたquery、署名・現在bond、最初の受理context | private native関数のみ。実carrier fee・UTXO・timely initial noticeと予約のdispatcher接続が未完 |
| Shared signed Begin/Part/Finish | literal Ready、固定slot、bounded chunk、EOF decode/source認証、全IDsのouter-root検査 | 全Ready actor、合法retry/失敗と再取得、codec/signature/IO/保持費用を全て含むprofileが未完 |
| Shared fresh data response | query block/DAAと認証済みfingerprintに対するliteral signature、receipt、適格起点 | データ段階の応答。semantic turn/witness/court義務の完了・default許可へは未接続 |
| Convictionおよびsplit witness、semantic bundle court | 最終fragmentから実裁定までの全work/bytes、semantic branch、後続責任 | 原子的な全clock更新、依存horizon、終了理由ごとの効果と全legal graphが未完 |

この表はsourceに存在する入口の監査であり、全合法graphが閉じたというcertificateではない。duplicate/invalid carrierの負荷も無限の予約義務と混同せず、実blockの受理前検査予算・通常費用・予約済み正規義務の各々で処理する規則が必要。callerが任意のpacket集合や費用を渡して「complete」とする入口を作らない。

## 次の実装が通すべき条件

採択済みの元class/layout/source/rulesを読み、preparationから最終裁定・回復までの全合法分岐の閉包とactual codec費用を導出する。未導出の項は0にせず拒否する。全claimの同時負荷、既存予約を移動しない追加予約、実際のfees/funding/exposure、最短の署名済み依存horizon、hard+followupとhard+retentionの最大、signerとholdを一つのoriginal受理遷移で検査・設置する。失敗では全てを戻す。

source-data upper、quota book、データreceiptの各々だけではこの条件を満たさない。first-blockの物理/専用/無効入力予算と明示したdelivery/inclusion前提も別途必須。全owner占有下の外部1人の実node訴追、T01–T15、tag230、実モデルdoor≤3,120の元の受入条件は維持する。

## 確認記録

既存current-clock release回帰5件と、G14 gate12・activation5・release identity9の31件が成功した。[実コマンド・生ログhashとsource hash](../../../evidence/palw/g14-integration-candidate-2026-10-20/preparation-source-scope-audit.json)。この回帰はclock version/serial witness依存/既存turn span/queue/cost recordを対象とし、この表の全対話予約・実fee統合やphysical capacityを実行したものではない。production・test source、旧wire、数値、presetは未変更。

## 最低material期限の接続修正

service bindingの`check_context`が署名custodyをhardだけに照合していた不足を補う候補。元frameのdescriptorに固定されたnative reorg保持期間を読み、`max(frame.terms.material_until_daa, hard + native_reorg_retention_daa)` をchecked arithmeticで導出し、既存のTIR custodyとsemantic history custodyの検査へ渡す。hard自体も元受理DAA+10,000との一致を要求する。

これは現在の元契約から確定できる最低値。全後続semantic turn・裁定後責任・delivery/inclusion・最終follow-up graphの導出を完了したものではない。別claimのbond holdをこの署名契約の代わりにしない。短い既存pledgeを書き換えず、新service bindingのwriteとcarriage context検査を拒否する。serviceを持たない旧claimの処理や全default拒否は変更しない。wire/schema/preset/実測値の変更ではない。

semantic回帰は、既存のguard-only synthetic semantic rowを使い、hardへ届くpledgeでも最低materialへ1 DAA足りなければ拒否し、一致/+1なら同じcontext検査が通ることを確認する。元frameやhistory署名のnative admissionを実行したという証拠ではない。[検証・hash・native回帰の正確な範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/service-material-floor-artifact.json)。

未解決後の保持・終了効果は別途閉じる必要がある。`d4896737c`時点の`court_service_holds_material_v1`はTIR frame/acquisition/preparation/witness/raw bytes等を扱うが、この最低horizon検査だけではClaimBundleやsemantic ownerの全保持・pruning・正当なclose後のarchiveを実装したことにならない。semantic行のあらゆる削除を一律拒否して正しい裁定まで止める修正も、その代替にしない。実契約を使うsemantic admission/close/reorg/pruned IBDと、必要な全material/担保責任を含む終了graphは未完として残す。

## Semantic material・署名契約・無応答課金の補強候補

`d4896737c`後の候補では、semanticを宣言した元frameに結び付く`Key::Claim`契約と`Key::Owner` materialを未解決serviceの保持対象へ加える。TTL/default sweepで削除・処罰する入口を保留し、nativeの通常更新は止めない。finished rowも未解決の間はその型のまま保持する。この保持をもって終了理由ごとの精算・archive規則の完成とはしない。

保持中のClaimBundle更新は、既存rowの構造検査、元の署名済みhistory pledgeとoriginal bindingの同一性、元frameのscopeと最低material horizonを検査する。長い別pledgeへ置き換えて契約を延長したことにしない。通常のPanel/qualification更新は可能で、native Panel記録callbackを回帰で実行する。署名済みpromiseの固定と、行全体の不変化は別である。

semantic/historyの無応答課金は`staged_selected_default_charge_v1`へ集約される。現在のserviceは実供給・inclusion・duty適格性を採択していないため、service bindingがあるclaimについてはproducer不応答分岐とsignerだけの不応答分岐の両方を、状態・journal変更前に拒否する。客観的な計算反証やfalse-Validの証拠裁定をこの無応答guardへ読み替えない。serviceを持たない既存claimの経済規則は維持する。

回帰fixtureは既存のguard-only originalを使う。semantic契約/OwnerBundle自体は構造検査を通る形にし、同じ行の通常TTL削除とservice下の保持、破壊的write/型変更/短い・長い別pledge/別original bindingの拒否、Panel更新、両無応答課金分岐の無変更を検査する。これは実署名のsemantic original受付、実fees、全対話、全familyの正しいclose、node/pruned IBDを証明しない。追加の構造検査・保持・比較・index/journal等の費用は、完成profileへ含める未校正項であり0ではない。

**`7aa640a8f`時点の残P0経路:** `palw_state_v2/deadlines.rs::sweep_court_deadlines`の通常court ladder/backstopは、このsemantic共通課金関数を通らない。現時点ではservice保留guardがなく、F02全体のdefault拒否完成とは表示できない。held sessionを保ったまま後続のunheld deadlineを処理できる索引走査、responding/challenging/terminalの各欠席、通常caseとorphan cleanup、正しい客観的closeを検証する必要がある。単にwhileの先頭をcontinueして索引を残すと無限反復になるため、その実装はしない。

また、現original quote/prepareは`job.tail.is_some()`を拒否する。Constraint/Prefixもそのtail enumを使うため、全semantic jobのnative original経路を既に通したという根拠はない。既存の別ClaimBundle経路の成功と混同しない。未知のtailを通す一括解除や、延期済みV5画像/Gen/Evalの解放で迂回せず、対象text経路を各々の本物の契約・費用・courtで検証する必要がある。モデルの除外や数値上限変更でG14達成にしない。

[検証・source/hash・範囲](../../../evidence/palw/g14-integration-candidate-2026-10-20/semantic-contract-retention-artifact.json)。原子的な全分岐予約・公開有料ingress・実供給・全依存/bond/終了graph、T01–T15、tag230と実モデルdoor≤3,120の条件は全て残る。


## 通常courtの自動期限処理の保留候補

`7aa640a8f`後の隔離候補で、`deadlines::sweep_court_deadlines`と、それより先に実行される`sweep_court_close_deadlines`の両方に未解決serviceのguardを追加する。前者はladder/phase/backstop、後者はclose組立窓の無応答を扱う。合意された実供給・配送/収録前提・義務の適格性が未接続の現bookを、帰責の許可にしない。現存するclaimにservice bindingがあれば自動欠席処理を保留し、元の期限、session、close group、depositを保持する。quota達成やhard通過で解除しない。

保留した索引要素を上書き・延長せず、呼出し内のcursorで次の要素へ進む。後続の未保留要素やmissing-claim/sessionの既存掃除を止めない。索引全体のVec化や新しい永続cursorは追加しない。通常courtの既存refresh分岐は現在DAA以降のbackstopへ再索引するため、処理した未保留行がcursor前の期限へ再挿入される経路ではない。close sweepの通常処理はsession/groupsを削除する。走査の追加比較・lookup・保持/再訪の費用は完成profileに含める未校正項で、0や実測容量とはしない。

**終了効果はなお未完:** `TransitionBuilder::write_court(id, None)`は、正しい客観的裁定を含むsession削除時に、残存close groupのdepositを徴収してgroup/dissectionを消す。自動期限のguardだけでは、この経路の配送欠如と経費/違反帰責の区別、未解決material/責任の精算・archiveは閉じない。正しいfraudのclose全体を禁止したり、徴収だけ飛ばして責任を削除したり、親を失ったgroupを残したりして解決済みとしない。既存のstreamed TIR fraud検証は別の経路の回帰であり、この通常courtの全終了graphの証拠ではない。F02/F03やG14完成、武装可能とは表示しない。


## 未解決bookの置換による保持消去

`c54ba38fd`の`write_staged_row_v1(CourtService)`は、既存のshared context/duty clockがあれば逆参照を検査する。しかし、それらの作成前には、元bindingを消した構造/context-validな空bookで置換できる。元の未解決claimをFinal・担保・資料保持のpredicateから外せるため、archive方式の選択に関係なく閉じる必要がある。

追加候補の`CourtServiceStateV2::check_retains_unresolved`は、元profile/予算、bindingとpacketの契約、供給記録、前向きclockを保存する。旧slotへの事後供給の挿入を拒否する。既存assignmentは、観測clockより前に供給されず、再割当先がそのclockより後の場合だけ動かせる。新packetは元観測clock後のreleaseを要する。これはbookの前向きwriteの不変条件で、供給の真正性・有料original受付・終了許可を生成するvalidatorではない。bookを縮小する汎用更新をterminal resolverの代わりにしない。journalによるundoは前向きwriterを経由せず、元の有効な状態へ戻す。

新archive候補については[RFC-0030](../../../rfc/0030-court-end-effects-and-pending-liability-archive.md)を参照。現depositの名目額は前払escrowではなく、残存collateralと他claim責任を考慮する必要がある。Aの隔離実装指示を、未決責任の解除や新Account Bondの実装・価格採択へ広げない。


## F08 A案のprivate archive adapter

親`494361118`からの候補。[RFC-0030 §8](../../../rfc/0030-court-end-effects-and-pending-liability-archive.md#8-a案の隔離実装--2026-10-21)と[ADR-0197](../../../adr/0197-court-end-effects-and-pending-liability-archive.md)に従い、`write_court(None)`のservice分岐で元session・phase・両group・全終了証拠を保存し、他sideの未完groupを自動徴収しない。直接close、完成chunk、分割TIR convictionをnative fixtureで部分検査する。正常完成groupのraw bytesを消さずfulfilledと区別し、不整合完成側は既存collateralから実際に徴収したdepositを記録する。

archive key 20 / row 26は新しいprivate末尾ordinalで、既存X3/service/tag230の意味を変えない。名目pending chargeを既存accuser exposureとbond exit holdへ加える。終了causeが未接続、または単に関連するnative処理であるだけなら元court stakeも保持する。実費入金・新Account Bondの資本額・全財源の確保を意味しない。未校正のコピー/hash/snapshot/IO/byte-DAAと全archive走査の仕事を追加profileへ含める必要がある。

未完groupの終了後提出・元CourtParams/署名scopeでの再検査・qualified settlement/pruningは未実装で、現行のholdだけをG14完成にしない。旧group rowは元declarationの全署名を保持していない。全actor金融入口、実署名のoriginal全分岐予約、実供給/公開ingress、最大family/phaseとE01–E10全条件、T01–T15、実doorは別の未完である。hardや最低material horizonを責任の自動解放時刻へ変えない。


## Archive v5検査と有料originalの境界

親`c153ecc02`の後のprivate検査は、保存した署名・native条件・session/phaseから既存v5を呼び、元state rootに束縛した局所結果を返す。[RFC-0030 §10](../../../rfc/0030-court-end-effects-and-pending-liability-archive.md#10-保存した入力によるprivate-v5再検査候補)。旧公開受付の適用時点を置換せず、wire/fee/financial writerへ接続していない。quote/cloneする全state検査scratchはglobal state量に依存するため、全original graphのper-claim上限が閉じたとはしない。借用contextによる共通native v5 kernelへの接続、内側のlegacy allocation/CPU、署名/hash、原子性と物理容量を含めて原本受付へ積算する必要がある。局所resource不足は不正/defaultにせずNativeResourceとして扱う。


## 借用recordによるarchive v5 kernel

親`54c0081d6`後の候補は、上記の検査scratch全state copyを除き、認証した元session/phaseを共通v5 kernelへ借用する。public active入口と同じ裁定本文を使い、費用検査の順序を維持する。group/message/assembly/decodeのquoteは残す。元state rootのhashや内側のlegacy v5 allocation/CPU等は未校正で、全有料graphのper-claim上限が閉じたとはしない。[実装範囲と最終検証](../../../evidence/palw/g14-integration-candidate-2026-10-20/borrowed-v5-artifact.json)。金融writer/公開受付/期限採択は接続しない。
## Signed data envelope component

Parent `49892b675`. `TirSharedRequestSetV1::signed_data_transport_upper` derives envelope bytes and control counts from the original pledge's custodian upper, grouped native source-unit bounds and actual Borsh signed codecs. It includes unique query openings, all allowed literal signer streams and fresh data responses. IDs have one FullIds stream per signer, while each logical query retains its response. No caller-provided participant number or unit price supplies this component.

The output is an input to a future full profile, not permission to instantiate a funded `CourtServiceStateV2`. Conversion into work/IO/service budget and actual transaction fees is not yet derived. Repeated/coalesced/rejected packets, outer carrier wrappers, preparation/restart, semantic/court/archive, retention and physical supply still require independent accounting. The dispatcher remains absent and F02/F03 default/arming refusal stays in place. [Source/codec tests, hashes and limits](../../../evidence/palw/g14-integration-candidate-2026-10-20/shared-transport-artifact.json).
The source audit also identifies an unpriced read path: `authenticate_body` and `read_shared_data_v1` retain/recheck the complete IDs whenever that row exists, even for StepNode/StepLeaf/StepRun. Those native source-authentication branches do not consume the rows-root/IDs closure. This is a source-level observation, not a measured CPU reduction. Required trace/ID anchoring and loaded-row integrity must stay authenticated; eliminating unneeded step-only copies and accounting remaining reauthentication is a next step before full funded admission.
## Query-local borrowed rows identity

Parent `52c20b99e`. The previously identified complete-ID work on step-only shared reads is removed. Trace reads still validate exact IDs/rows against the original outer root but do not clone an owned ID Vec. Native authentication, complete-ID validation, loaded-ID context and owned FullIds EOF output stay unchanged. [Audit/tests and limits](../../../evidence/palw/g14-integration-candidate-2026-10-20/borrowed-rows-artifact.json).

This reduces a known source-level duplication before full-profile accounting; it is not measured CPU/RSS or a change to native resource bounds. Trace hash scratch, duplicate native reauthentication, EOF allocations, repeated ingress, transaction fees, whole branching/retention and supplied capacity still need costing and funded admission. Partial data transport plus local read optimization cannot authorize default or a production original installer.
## Nonexclusive numerical ranges beyond shared data

Parent `65dd205cd`. The new per-fixed-root numerical frontier preserves all pinned children and shares the native shape/interval/numerical fold validators. It has no serialized/rooted authority, signer or notice/funding transition; projections retain the root's historic clock and cannot be installed as live phases. [Implementation/test scope](../../../evidence/palw/g14-integration-candidate-2026-10-20/numerical-frontier-artifact.json).

The next native connection must authenticate original/source/program/leaf and finalize under actual rules, establish one canonical root/signer scope independent of actor session races, and retain every admissible branch. It must not seed a public shared right by reinterpreting a legacy actor/session signature or by creating a fake challenger/session. Per-root `N-1` disclosures cannot stand in for all-leaf/all-root/all-actor cost, actual fees and atomic original reservation. The notice endpoint is separately proposed in [RFC-0031](../../../rfc/0031-shared-court-initial-notice-boundary.md)/[ADR-0198](../../../adr/0198-shared-court-initial-notice-boundary.md); no equality, funding or wire rule is adopted here.
