# RFC-0035 — shared court originalの実原資と役割別同意

Status: **Proposed / Review Required** (2026-10-21)。[ADR-0202](../adr/0202-court-original-funded-contract-and-role-consent.md)の選択待ち。RFC-0026の原子的受付を実装するための未確定な金融・署名契約を具体化する。実装済み・採択済み・有効化済みとは扱わない。

## 1. 問題と既存経路

[RFC-0026](0026-court-policy-master-contract.md)と[RFC-0032 A](0032-shared-court-terminal-material-and-nonblocking-ingress.md)は全合法分岐の容量・依存・保持・費用・原資をoriginalで閉じる。しかし現在のnative originalのrentは実carrier feeからburnされる。frame depositはbond上の名目exposureで、将来のPanel/relay作業料を支出できるescrowではない。既存slashもcollateralとslashedを更新し最終releaseのburn obligationへ接続するため、支払原資と呼び替えない。

旧TFP1 envelopeはversionとEOFを厳密検査する。旧custody pledgeは旧resource planとsource・claim・job・horizonを署名するが、新しい共有全分岐graph、将来の役割、実報酬・返還条件には同意していない。既存wireへoptional tailを足したり、旧署名を新契約の同意に流用しない。

## 2. 選択肢

### A — 実UTXOで全額裏付けた独立運用escrow（推奨候補）

original carrier transactionの実入力から、元のbond/予約資本・既存rent・実transaction feeとは別に運用資金を確保する。新契約に束縛された専用escrow出力/支出規則を設け、実際のUTXO金額・spend script・唯一のoutpointをnative UTXO検査とoriginal state遷移の両方で照合する。state rowに金額を書いたことだけでは資金確保としない。新署名domainでpayerが契約と支出上限に同意する。

各合法分岐の必要支払を、単に互いに排他的と仮定して減額せず、採択する正規化・共有・反復受付の規則から導出する。作業料・carrier fee償還・保持・復旧・終了効果について、誰が何を満たせばいくら支出できるかを明示し、出力の実spendとstateのonce-only accountingを同じ原子遷移で更新する。UTXOが未支出なら支払済みとは記録しない。UTXOとstateのいずれかの検査失敗でoriginal/支払の全体を巻き戻す。

保存則は `実入金 = 実支払 + 実burn + 実返還 + 未解放残高`（すべて同一資産、checked integer arithmetic）である。pledged/nominal金額を入金に含めず、同一outpointの二重割当・二重支出・他claimの原資流用を拒否する。escrow返還には未決作業・復旧・費用・保持・他の明示された責任の解消が必要で、hard到達だけでは返還しない。未確定受取人への恣意的な送金権限をproducerやPanelへ渡さず、採択された決定論的な履行/精算規則とその証拠を支出条件にする。

これは新しい本番資金方式を黙って採択する提案ではない。専用script/UTXO検証・wallet構築・consensus accounting・reorg/IBDの設計と隔離実装が必要であり、作業完了まで有料公開入口を開かない。新しいscript識別子・wire tag・数値料金はこの文書で割り当てない。

### B — 新carrier feeの実徴収額を運用口へ分配

新original専用の実carrier feeから、既存burnと区別して運用原資を徴収し、受益者と未解放残高を追跡する。fee/coinbase collectible/burnの規則に新しい会計経路が必要で、旧rentを遡及して再配分しない。実資金の保持と将来支払の裏付けが成立するまでstate上のcreditを発行しない。既存fee経済への影響がAより広いため、Aを推奨する。

## 3. A/Bに共通の原子的契約

- 新しい別envelope/versionと新署名domainを使う。既存TFP1・旧object tag・旧claim identityの意味を変えない。新入口は独立候補profileで、本番presetへ登録しない。
- 元claim/job/source、登録program/layout、materialの閉包、全合法分岐のcanonical相対graph、費用・資本・保持・復旧の規則、実原資、役割別同意を束縛する。callerが作ったgraphや`paid=true`を完全性/支払の証明にしない。
- 通常の事前署名で未知のaccepted DAAへ同意したことにせず、明示したacceptance intervalとcanonical相対graph/必要horizonを署名する。実accepting DAAで一意に絶対化し、元の3,120/10,000と依存契約の既存horizon内に収まる場合だけ受理する。正確なtarget DAA限定の署名を将来サポートする場合も別の明示形式にする。
- producerの署名で将来Panel/custodian/relayへ新義務を課さない。既存契約の明示した範囲で足りるか検査し、不足する新しい役割には本人の署名済みconsent・現在authority・受付/応答/保持期間・資本/報酬条件が必要。未同意の将来actorを仮定して受付を成功させない。
- 将来の外部filerの本人同意はその有料経路を使う時点で検査する。originalでは、任意の適格な一人が使える共通frontier・容量・実費原資を確保する。owner枠/先着不完全bodyを占有権にしない。
- prepareで全検査・必要物の有界導出を行い、普通のFP書込前のcheckpointへ新pairingを含める。installではFP/frame/preparation・rooted全graph・同意・実原資と資本予約を一括更新する。見積り成功だけで書込済みとしない。
- graphをrooted契約へ保持/元sourceから再導出し、load・forward mutation・repair・undo/replay・pruned IBDで同じ拘束を検査する。部分graphを渡したcausal予約成功はoriginal受付成功ではない。

## 4. 未決値と拒否条件

料金・担保額・draw上限・保持期間・配送/収録前提は、本書の構造選択から自動採択しない。RFC-0027/ADR-0193の100,000 MSKは実用運用資本の十分性を保証しない。12,000 MSK/claimを料金へ昇格しない。本契約をその未実装Account Bondへ依存済みとも扱わない。

実fee schedule、全費用の同一profile、必要な署名契約、実UTXO資金/決済規則または原子的予約が未完成なら新入口を拒否する。仮単価・仮資金・nominal deposit・非供給slotを使って合格させない。旧入口の本番規則をこの候補へ置換しない。

## 5. 受入条件

1. 未資金/偽outpoint/他claimの資金/二重割当/非署名actor/不十分horizonをordinary FP書込前に拒否し、FP/frame/新court・escrow会計のdeltaが空である。carrier transaction自体をUTXO検査で拒否した場合だけ、そのtxのUTXO deltaも空となる。PALW objectだけが不成立の場合、実際のordinary fee/rentやpayerの通常fallback outputまで不存在と報告しない。
2. すべての資金入出力で実UTXOとrooted accountingが一致し、保存則・部分支払・一回限りの徴収/返還・reorg・二重支出を検証する。実burnと支払とnominal exposureを分ける。
3. callerに部分graph/安いprofile/偽paid flagを与えても受付できず、全owner共謀・再充填下の一人経路とF02のno-slash復旧を実nodeで通す。
4. 元の期限・他claim責任・資格・現在署名権限・第三者relayを保存し、停止Minerの私鍵なしで承認された処理だけが進む。
5. t12同一性とRFC-0026 T01–T15を同一候補で通す。実モデルdoorと飽和・費用の測定は別途必要で、toy/会計例で代替しない。

実装・検証結果は未記録。A/Bの選択は詳細仕様化・隔離実装に限り、本番採択・旧claim移行・pre統合・武装・配備を含まない。

## 2026-10-21 ユーザー選択 — Aの詳細仕様化・隔離実装

ユーザーは「A：実UTXOで裏付けた独立escrow」を選択した。上の選択待ち表記は選択前の履歴である。実outpoint・支出・保存則と役割別同意を束縛する詳細仕様化・隔離実装に進む。金額・新料金・draw上限・保持期間・本番採択・pre統合・武装・配備を含まない。実UTXO/決済・全費用/資本・原子的original受付の接続と検証は未完であり、この選択だけから資金確保済み・G14完了を導かない。

## 6. Aの実装境界調査 — 未接続のUTXO保護

`crypto/txscript/src/lib.rs`の現engineは未知のscriptPublicKey versionを即座に成功とする。したがって、未対応versionのmarkerを書くだけでlockされたと扱ってはならない。`OP_TRUE`等の通常は自由に支出できる出力へ金額を書いただけでも原資保護は成立しない。

既存PALWのobject受付拒否とordinary carrier transactionのUTXO受付は別の段階である。新contractが不成立のcarrierについて、専用資金だけが無保護で残る実装を作らない。資金outpointのnativeなspend拘束を原子的に成立させるか、不成立時もpayerだけが回収できる通常署名のfallbackを維持する必要がある。Miner停止後のrelayには、payerの将来の秘密鍵操作を必要とせず、元の署名済み契約が明示した決定論的な支出だけをnativeに認可する経路が別途必要である。通常inputの署名検査を任意のpayloadや`paid`値で省略しない。

既存bondのUTXO保護は`UtxoProcessingContext`/`BondSpendFilter`、selected-parent stateのlock/burn集合に加え、同mergesetで新規に宣言されたbondも保護する。escrowにもown-bodyだけでなく同block・mergeset・mempool・IBD/reorgに一貫した保護が必要である。単にparentのescrow mapを読むと、資金を作った同block内で横取りできる可能性が残る。

これらの実UTXO拘束・適格支出・不成立時回収が未実装なら、rooted金額や資金DTOを作っても「実escrow確保済み」にはしない。新script/opcodeの番号、engine規則や本番forkはこの調査記録で採択しない。

## 7. 次の隔離実装で必要なnative adapter

以下はAを実装するための検査境界であり、実装済みの支払経路・新script番号・本番forkの採択ではない。

1. **実carrierとのpairing。** 新契約は同carrierのfunding output index・実額・payer/script・元claim/source・canonical graph/価格規則・返還先を署名する。txidをそのtx自身の署名payloadへ自己参照させず、最終carrierの実txidとindexからoutpointをnative側で確定する。callerが別outpointを指定して資金を作れたことにしない。accepted transaction extractionから実output factsを引き継ぎ、通常のUTXO validationと一致させる。
2. **不成立時の資金保護。** 新しいPALW contractが不成立でもcarrierがordinary transactionとして残る場合がある。新outputを無条件支出可能なmarkerにせず、payerだけが回収可能なfallback、またはtransaction自体を拒否する完全なUTXO連携を必須にする。失敗したoriginalの金額をrooted残高へ加えない。
3. **同block/mergeset lock。** parentの確定escrowに加え、新規資金/継続changeを同mergesetでも保護する。未成立contractを根拠に他人の既存outpointをlockできるようにしない。raw carrierによる暫定保護を使う場合は、そのcarrier自身が作る該当outputに限定する。新しい資金はparentで検証可能になるまで支出を拒否する保守的な方法も、元期限内で処理できるかを検証する。
4. **停止Minerに依存しない適格支出。** 元payerの契約同意と確定済み履行・fee receipt・精算規則から、その実transactionのescrow input、支払先/額、burn、残高/changeをnativeに検証する。将来のpayer署名を待つだけの経路では第三者relay要件を満たさない。通常inputの署名を省略する汎用flagは設けない。特殊な支出認可を実装するなら、唯一の実escrow outpoint・確定contract・同じnative stateに束縛された私有の認可結果だけを使い、通常inputや別contractへ流用しない。有効化前はこの認可経路を一切受理しない。
5. **保存則と一回性。** escrow inputの実値を、契約で許可した実payee出力・実burn・実返還・新しい実change出力へchecked arithmeticで割り当てる。同じactual feeを複数operationへ全額ずつ償還しない。relay自身が払った通常feeと、escrowが負担する承認済み費用を分け、callerの過大なfee選択で予約を抜き取れない上限を契約で検査する。数値未決なら受付拒否を維持する。
6. **同じ原子遷移。** 新規funding・支払・changeのUTXO結果と、original/作業receipt/一回限りの精算状態を同じaccepted transaction集合から更新する。契約のescrow作用についてUTXOだけ成功・stateだけ成功の結果を許さず、二重支出、受理順変更、再起動、reorg/undo/replay、pruned IBDで再現する。未成立contractのordinary payer fallback outputは、その契約のfunded escrow成功に数えない。mempool、block構築、mergeset検証で別々の許可条件にしない。

このadapterの原資検査だけができても、全合法分岐・役割別同意・依存horizon・容量予約・実供給・金融/保持効果が閉じなければ、originalの完全受付とは認定しない。

### 7.1 同じcarrierの実feeの一回割当

新adapterは `F = Σ実input UTXO額 − Σ実output額` をchecked arithmeticで一度だけ求める。専用escrowの新output額はoutput側に既に含まれるため、同じ額をfee原資にも加えない。旧bond burn・旧rent burn・新escrowの実burn/承認feeを同carrierで併用する場合、それぞれが単独で`F ≥ 自分の額`を満たすだけでは不足する。すべての割当の合計を`F`と照合し、miner collectibleから差し引く実burn総額、実支出、rooted escrow debitを一致させる。`palw_bond_burn_paid`等の既存単独検査を新複合会計の保存則に流用しない。

## 8. Aの隔離実装の第一段階

新しい`TFO1` transportは旧`TFP1`のliteral bytesを保持し、新payer consentを別署名domainで束縛する。全wire幅は旧TFP1のouter ceiling以内とし、新metadataを理由に上限を増やさない。署名は元claim/job/execution/source、canonical相対graphと各profileの識別子、受付interval、必要material horizon、実資金output index/amount、payerと返還鍵、nonceを束縛する。最終carrier txidを自己参照で署名せず、実transactionからoutpointを取得する。

実出力との照合は既存version0の厳密なpayer P2PKH-MLDSA87 fallbackだけを認める。未知script version、OP_TRUE、別出力、異なる額/返還鍵を準備成功としない。payer署名とnative expected scopeを検査したopaque preparationも、UTXO検証済み・lock済み・全graph導出済み・actor同意済み・funded original受付済みではない。expected scopeのDTOをpeer入力から作るだけではnative導出にならない。

同carrierのpopulated input/outputからchecked differenceを求め、一つのfee budget上でallocation identityの重複と合計超過を拒否する。これは実UTXO viewの供給と通常script検証、nativeでのallocation適格性、burn/collectibleの更新を代替しない。個別helperを呼び直して別budgetを作る経路で保存則を迂回させないことが、将来coordinatorの責務として残る。

accepted transactionのobject extractionでは、既存history/TIR fenceとは独立してTFO1を拒否し、内包TFP1を旧originalとして受理しない。通常carrierのUTXO/feeは残り得るが、rooted escrow残高を作らない。新rooted row、同mergeset lock、停止Minerに依存しない適格支出、全分岐の原子的受付・精算は未接続である。第一段階の署名・codec・実出力検査を、Aの実escrow確保やG14完了とは扱わない。

## 9. Native feeとsource graphの接続 — 2026-10-22

既存carrierでも、実fee `F`に対するbond burn `B`とrent `R`を独立に`F>=B`・`F>=R`と判定すると、同じfeeを二重に充当できる。native accepted transactionのraw feeはそのまま保持し、同じ検証済みcarrierとburn obligationからrent利用可能額`F-B`を別に導出して価格検査へ渡す。finality分類も実collectibleの範囲内とし、burnされた額を別carrierの通常feeへ転嫁しない。既存の金額・価格・fence条件を変更するものではない。

source資料のv3候補は全7query familyから導出するopaqueなSourceMaterialOnly graphである。相対logical IDと実受付allocationに束縛したpacket IDを分離し、source全packet集合・辺・元時計の内側の予約を検査する。短bodyの未使用tail、他signerが供給した認証済み資料を使う応答、FullIds共有を維持する。semantic・数値裁定・parameter・役割別同意・資本・実費まで閉じた全original graphではなく、旧v2のcodecやrooted schemaも変更しない。

実escrow支出の次段階では、ordinary UTXO受付より前に、同じselected-chain state/accepting pointに束縛した決定論的なUTXO/金融state planを検査しなければならない。後段のPALW object dropへ会計を付加するだけでは、実支出とstate更新が分離する。認可結果は同じ実input entries・全transaction bytes・contract/pointへ束縛し、txidだけで署名検査を省略するcacheに流用しない。

[実装範囲・検証記録・残りの接続](../evidence/palw/g14-completion-2026-10-22/native-fee-and-source-graph.md)を参照。これらを実escrowの確保、公開有料入口、G14全体の完了とは扱わない。
