# RFC-0057 Court nonce — 実装接続表

<!-- TB1-FOW1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 新規profileの適用範囲

**ADOPTED_DESIGN_NOT_ACTIVATED**。新規positive nonce/challengeのsourceは認証済みTB1-PANELと元claim・actor・義務である。旧WorkBeacon branch-relative lockの接続を新profileへそのまま継承しない。negative・資料case・Court継続・終端へ新beacon待ちを追加しない。

[TB1](../../../rfc/0061-palw-internal-threshold-beacon.md) / [FOW1](../../../rfc/0062-palw-function-output-and-work-admissibility.md) / [優先順位と移行](../../../governance/palw-tb1-fow1-transition.md) / [全適用台帳](../../../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。
<!-- TB1-FOW1-ADOPTED-20261120-END -->

日付: 2026-11-17（Asia/Tokyo）  
状態: **PROPOSED_DESIGN_NOT_ACTIVATED**  
規範案: [RFC-0057](../../../rfc/0057-court-nonce-and-panel-challenges.md) / [ADR-0225](../../../adr/0225-court-nonce-and-panel-challenges.md)  
監査基準: `main@2d8212b634880b0967fbf24b00b41936c2322c14` / [ソース監査](../../../maintenance/revisions/2026-11-17-court-nonce/SOURCE_AUDIT.md)

## 1. 接続の目的

元claim・元Miner execution root・選出主体・round・scope・認証済み乱数を束縛したCourt nonceを、新しい版のpositive承認と局所challengeへ導入する。同じ義務の再提出で、承認・coverage・報酬・返還・制裁・資源予算を増やさない。

既存negative、資料要求、応答、終端のactor/phase/stepにも一回性を接続するが、完成した反証に新しい乱数待ちを課さない。正しい資料の共有と第三者搬送は維持する。

本表は設計から実装へ渡す完了条件であり、下記の新経路を実装済み・試験済みとする記録ではない。

## 2. 入口別の接続

| 入口/機能 | 現行sourceの足場 | 追加する接続 | 終了条件 |
| --- | --- | --- | --- |
| 元claim・execution commit | native original、既存rooted frame、RFC-v2-0016の契約 | 新policyとscope、元root群、nonce/challenge計画、必要な全工程予算を受理時に固定 | packetの自己申告から期待subjectを作らず、元stateと一致する |
| 選出Panelのpositive承認 | RFC-v2-0017の採択schema。旧PanelWitness型は意味が異なる | 新署名scope、canonicalな選出主体、round、nonce、個別challenge集合、承認scope、本文digestを束縛 | 他人/他claim/他round/旧profileの署名を移植できない |
| 認証済み公開乱数 | challenge/seed.rsのcontext/subject照合、WorkBeaconのbranch相対lock | 適用profileに対応するpost-commit anchorと有効な乱数recordを解決 | ユーザー選択seed、古いanchor、障害時のlocal乱数fallbackを拒否 |
| positive局所証拠 | 元rootに対するopeningと既存bounded court checker | 該当nonceから期待targetを再導出し、型・位置・scope・opening・局所演算を照合 | 本人の承認条件にのみ反映し、局所passを全体再実行の証明にしない |
| negative/直接反証 | TIR public control、rooted source/turn、objective conviction | 確定済みcaseのactor/phase/step identity、提出一回性、元subjectとの結合 | 個別positive challenge外の反証も扱い、seed待ち・多数票を追加しない |
| 資料要求・応答 | shared material intake、SourceRequest、literal signer検査 | nonce付き提出wrapperと共有material identityを分離 | 同じbytesを再利用でき、他人の履行・defaultへ誤変換しない |
| third-party relay / split parts | 既存carrierとsigned action、Begin/Part/Finish | immutable bundle/part index/全長/digestを拘束し、重複partを無効果化 | relayerが報酬先や義務者を変更できず、part差替えと無限再試行を拒否 |
| native受理・決済 | accepted-prefix、checkpoint、once engine、false-Valid offence | nonce scope、accepted marker、coverage、支払、実lock/費用を同じatomic境界へ接続 | 途中失敗・再送・同batchでpartialな効果を残さない |
| restart/reorg/IBD | journal、source/row復元、既存nonce counterの先例 | nonce根拠、draw/seed、承認、case、効果の一回性と残予約を認証付きで復元 | 孤立枝の結果を混ぜず、欠損を空集合・未使用nonceと推定しない |

実在する足場の限界・行番号・固定commitリンクは[ソース監査](../../../maintenance/revisions/2026-11-17-court-nonce/SOURCE_AUDIT.md)を参照する。旧ExactStream positive contractの存在を新版の完全承認native接続と数えない。

## 3. P0 — 型・scope・profile契約を確定する

新profileだけに適用し、旧署名・旧claim・旧wire意味は維持する。採択時には次を一体で定める。

- 元subject、選出主体、round、役割、義務、phase/step、challenge計画、乱数anchorのcanonical encoding。
- 新digestと既存native IDの型・幅・domain。既存64-byte ID等を32-byteに切り詰めない。
- nonceを自由な探索値にせず、認証stateから期待値を一意に導く規則。鍵・relay・署名bytes・重複draw位置の変更で義務を増やさない。
- Panel固有challenge集合と、共通の必須coverageの区別。同じtargetの重複を別coverageとして数えない。
- positive承認、局所証拠、直接反証、共有資料、資料供給、役務receiptの別scope。
- 各kindのsize/count/depth/work/heap、challenge抽出・重複処理・sortの有限上限、全工程のclockと責任窓。

wire tag、署名domain、経済purposeの割当が未完ならactive新profileで受理しない。共通codec/kernel/cost checkerの適合をモデル別の公開性・品質・保有者数の審査へ変えない。

**完了証拠:** 独立実装が同じnative参照から同じscope/nonce/targetsを導き、全field変更・encoding曖昧性・overflow・最大長の反例を拒否する固定vector。

## 4. P1 — 乱数とPanel別challengeを接続する

適用profileが認証するpost-commit乱数を使う。現行WorkBeaconをBFT committeeの乱数として説明せず、利用するadapterのcontext、anchor、branch、元subject、policy、到達期限を検査する。

Minerの元commit、選出主体の固定、challengeの確定、証拠作成・包含の順序をRFCへ合わせる。round増加、鍵rotation、Panel交代で自由に再抽選しない。正規交代を許すprofileでも旧義務・予算・期限・一回性を継承し、その条件をstateから認証する。

乱数が成立しなければpositive条件を未完として扱う。期限内の既存回復/未検証終了へ進め、ゼロseed、local RNG、任意のblock hash、提出者選択seedで合格を作らない。成立済みのnegativeや既受理救済は別に継続する。

**完了証拠:** post-commit以前のseed、他subjectのanchor、別branch、偏りを生む選択的再試行、期限切れ、seed障害、restart/reorgを含むnative vectorと因果順序試験。beaconの予測不能性・bias resistanceは別のreview gateとする。

## 5. P2 — 主体別onceと共有materialを分離する

受理対象の期待identityをnative stateから解決し、重い資料decodeの前にbounded header、scope、資格、nonce、署名、現在の義務を検査する。不正packetで被害者のonce marker、case、手番、保護予約を消費しない。

同じ有効な提出の再送は、canonical state効果を追加しない。署名bytesやcarrierを変えても同じunsigned subjectなら新しい義務・新しい報酬とはしない。異なる内容を同じ義務へ再提出した場合は、そのkindに許された更新/矛盾規則へ従い、単なるhash差を即座に計算有罪としない。

資料storeは、元root、relation、座標、認証規則に結び付くmaterialを共有できる。主体別nonceをServiceReceiptやsource-unitのidentityへ加えて二重課金・二重報酬を作らない。共通cacheのhitから、別Panelの完全再実行を認定しない。

**完了証拠:** 同一提出・別署名・別relay・split再送・別Panel・別claim・別round・鍵変更・同bond重複draw、shared opening衝突/再利用、invalid-firstからvalidへの進行で、必要な効果だけが一回起きる試験。

## 6. P3 — 反証・誤った肯定・Finalを同じ状態遷移へ接続する

反証は元Minerのsubject/rootに対する客観的な局所不整合を必要とする。Panelの比較traceが異なること、不正opening、nonce不一致、資料欠損、通信遅延だけではMinerの計算有罪を生成しない。

完全な肯定承認を出した主体への責任は、当該署名、scope、元subject、反証の結論、責任窓、実際の担保と費用同意から判定する。既存false-Valid offenceのscopeとonce ruleへ接続する場合は、新承認から既存責任への明示adapterを必要とし、選出されただけの人、局所scopeだけを署名した人、relayerを一律に徴収しない。

同一offenceの別証拠・別nonce・別申立人による再提出は、二重の制裁・報酬を生まない。既存責任と新nonce offenceを別名にして同じ違反を二重徴収しない。報酬recipientの取得や上書きも、提出の先着だけでは許可しない。

同canonical処理単位では、既存の反証・取消・期限・Final優先順を維持する。nonce、受理、未解決case、局所coverage、承認、credit資格、支払/返還/制裁のonce markerを原子的に更新し、エラーでpartial stateを残さない。

PublicOneHonestについて、全Panelの個別challenge完了を新しいFinal条件にしない。本人のchallenge未完はその本人の肯定承認を数えない条件とし、それだけで他人の未解決caseを生成しない。能動的な正直者1人の全体検証・公開資料・提出包含・期限内反証の前提を継続し、nonceでその前提が証明されたとは扱わない。

**完了証拠:** 多数positiveと単独有効反証の同batch、誤った肯定と正しい局所positiveの併存、正しいclaimで残り全員不署名、反証対象外scopeの署名者、自己署名を別鍵へ付替え、二重offence/二重報酬/二重返還、各checkpointの故障注入。

## 7. P4 — RFC-0055/0056の容量・受付へ接続する

nonce追加は新しい無償のCourt送信枠を作らない。原claimの予約へ、全選出主体の合法な個別challenge、必要な共有資料、署名/nonce検査、再送・復旧、反証、終端、保持を計上する。

[RFC-0055](../../../rfc/0055-global-admission-court-calendar-and-one-bps.md)のglobal資源上限と、[RFC-0056](../../../rfc/0056-global-claim-admission-difficulty.md)の新規受付Dを使う。既受理caseの正規反証・応答に新しい受付Dを要求しない。model・Panel別nonce数の増加でglobal budgetを増やさない。

必要なsample数・Panel数・round数・byte/work/stock上界は共通profileから導く。便宜的な1/3/5件を新しい固定仕様へ置換しない。Nodeはモデル本体や全traceを取得せず、有界なopeningと局所演算を検査する。profileを満たせない場合は新規受付/activationを拒否し、Nodeの全モデル再実行へfallbackしない。

**完了証拠:** invalid nonce洪水、重複packet、正規候補・反証・共有資料の混合、1万/5万model ID、長いbatch/part、quote経由の重い処理、live-state/once索引・GC・復旧を含む最低Nodeの実測。ID不変性の抽象試験を実registry/CPU/RAM負荷試験と表示しない。

## 8. P5 — 永続化・reorg・古いclaimとの共存

受理済みnonceと効果identity、根拠となる元subject/draw/seed/round、case、責任窓、残予約を認証state・journal・snapshotに保存する。資料をGCしても、未使用扱いに戻るmarkerを作らない。保存行や成功flagだけを信用し、証拠を省略した復元をしない。

canonical reorgでは関連stateを同じcheckpointでundo/replayする。通常のcancel・retention終了と、正規reorgで孤立した受理のundoを区別する。別branchのseed・局所pass・署名・支払をそのまま現在branchへ移植しない。reorgが無条件に再抽選不可能であるという保証も追加しない。

旧claimは旧規則で検査し、新nonceを遡及要求しない。新profileへ旧carrierをdowngradeしてnonce検査を飛ばす経路は閉じる。未対応schema/経路から新しい責任を作れないようにする。

**完了証拠:** process restart、DB reopen、複数peer IBD、pruned snapshot、index欠損、世代競合worker、reorg後の再包含、同時支払、古いprofileとの共存試験。欠損を復元できない場合は未検証/保留となり、成功や新しい予算にならない。

## 9. P6 — Studio/Panel/Nodeの実経路を確認する

同一の実LLM結果から、元commit、Panel選出、nonce/target導出、完全再実行、本人のpositive bundle、Nodeの局所検査、Final、権利の一回消費まで接続する。不正な元traceでは、適格な一人がchallenge割当外も含む反証を提出し、元claimと誤った肯定の責任へ正しく反映する。

正常時のPanelがnonceを選び直したり、Studioが結果・署名者・roundを変更して再送したりしても、新予算を得られないことを確認する。モデル未保有のcold Panelの取得・全体検証時間と、Node側の局所検査時間を別々に測る。1 BPSは外側carrier全体の持続目標として扱い、1秒一claim Finalやnonce提出ごとの採掘権を作らない。

**完了証拠:** 実プロセスと実資金を用いた記録、accepted-prefixと永続state、Node再起動後の同じ結果、原期限内の正直な単独反証、既存Final/credit/onceとのE2E。fixture上のdigest/署名一致や抽象モデルPASSで代用しない。

## 10. 有効化判定

設計の明示採択、版と署名domain登録、共通profile・容量・料金・clock/beacon、上記native全入口・永続復旧・経済効果・実機/E2Eの証拠を揃えてから、新claim用profileのactivationを別途判断する。

nonceは、使い回しに伴うscope偽装・同じ義務の二重効果を防ぐ構成要素である。証拠内容の共有、外注、共謀、局所だけの計算から、本人の独立全体再実行を判定する保証へ拡張しない。非選出者の署名や局所証拠だけで完全承認を補完せず、計算正当性の暗号学的証明やモデル単位の審査を導入しない。
