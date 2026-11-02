# RFC-0042 (CourtSupplierConsent) — Court全分岐の供給者同意・実原資・native予約

> **並行割当衝突:** 同番号の [RFC-0042 (AllModelG14PanelOne)](0042-all-model-g14-panel-one.md) は別の主文書です。両方の元番号・完全パス・採択内容・wire/署名domainを保持し、修飾名と完全パスで区別します。[番号台帳](../maintenance/numbering.md#2026-11-02-並行割当衝突)。

Status: **Adopted / 実装中・統合未検証・未有効化** (2026-10-30)。[ADR-0210 (CourtSupplierConsent)](../adr/0210-court-family-supplier-consent-and-native-reservation.md)。
ユーザー回答「新契約を採択して接続まで進める」を記録する。
[具体案](../design/palw/integration/g14-17000/family-supplier-contract-proposal-2026-10-30.md)と
[RFC-0035](0035-court-original-funded-contract-and-role-consent.md)の役割別同意をnative受付へ接続する。
公開ネットワークの有効化・配備、料金の新しい測定値承認を含まない。

## 1. 問題と最小変更

既存7-family SourceMaterialOnly graphは、parameter/prompt、semantic table、tokenizer raw材料、
Prefix生成後のselection/stopの供給義務を閉じていない。これらを無予約のままoriginalへ含めると、
裁定に必要な材料・資本・役務が欠け、受付後の進行不能または実費不足を生じる。
旧custody pledgeの範囲を変更せず、新しい双方署名契約と全分岐予約を追加する。

四役割は `ParameterAndPrompt`、`SemanticTable`、`TokenizerArtifact`、`PrefixGeneratedSelectionAndStop`。
新versionのoriginal envelopeは旧envelopeのliteral bytesを保持し、新契約集合を別に束縛する。
旧wire tag、旧署名domain、旧IDの意味を変更しない。

## 2. nativeなscopeと双方同意

契約はnetwork、ruleset、claim/job/source、canonical相対graph、役割、認証済みtarget inventory、
供給者・sponsor、byte上限、受付interval、応答・材料保持horizon、料金・資本・保持・精算profile、
金額、nonceを署名する。供給者とsponsorが同じcanonical messageに専用domainで署名する。
native受付は現在の鍵権限・適格性と、実際の登録program/layout・immutable semantics・元jobから
導出したscope/profile/上限/金額の完全一致を検査する。peer由来expected DTOだけを根拠にしない。

未知のaccepted DAAへの署名を仮定しない。受付intervalと相対service graphを署名し、実受付点で
一意に絶対化し、署名済みhorizon内に収まる場合だけ受理する。絶対期限を正確に固定するversionでは、
その期限を満たす実受付点を限定し、別DAAへの移動を拒否する。既存署名の意味を途中で変えない。

各材料はclaim・source・inventory・役割・target identityに束縛する。全parameter union、prompt形とtile、
vocabulary/EOG/rendering、tokenizerの認証済みEOFとraw byte上限、生成IDとselection/stop依存を導出する。
protocol上の上限を使用する場合は保守的な最大値と明記し、実byte数・取得済み・供給済みとは扱わない。
導出不能・超過・不足scopeをゼロ費用または「供給済み」で通さない。

semantic table の file 上限256 MiBをそのまま全originalの必須供給量にすると、既存の16 KiB part幅・
1 packet/DAA・10,000 DAA hardでは正当な小tableまで受付不能になる。このためTFO2は、既存outer上限内で
canonical table literalのinventory witnessを運ぶ。受付前に全vocabulary/EOG/tokenizerをimmutable semanticsの
rootへ照合し、実際のcanonical encoded幅をnative inventoryと双方署名のbyte boundへ拘束する。
認証済みinventoryは元originalのrooted状態に保存し、restore時にも再照合する。peerが申告したlengthや
単なる未認証hashを実サイズの根拠にしない。元の256 MiBはdecoderの上限として残し、認証済み実byte数と
区別する。TFO2の既存outer上限に収まらないwitnessや既存service予算内に収まらないoriginalを、容量を増やして
受理しない。inventoryの検査はデータ同一性の検査であり、モデルの審査・計算の正しさの証明ではない。

## 3. 既存単価からの実額と予約

新しい単価・期間・担保率を発明しない。nativeに導出したtargetごとに、既存16 KiB part幅で
Begin、全Part、Finishを数える。署名とouter framingを含む最大encoded control bytesを導出する。

- feeはcontrol bytes×既存carriage単価と、材料byte上限×既存V5 storage単価のchecked和。
- 役務資本はpacket数×既存source-node資本係数と、既存の参加・担保下限を用いる。
- profile IDは導出versionと使用した既存scheduleを拘束する。未承認の測定値へ読み替えない。
- originalの全合法分岐の役務・材料・復旧packet、費用、供給者、資本、保持をまとめて検査・予約する。
  排他的分岐の共有は既存のcanonical共有規則で認めるものだけとし、callerの重複除去を信用しない。

元originalのheap/retention holding需要と新supplierの保持需要も、既存のservice予算に収まる有限segmentへ
checkedに分割する。元の総需要を減らさず、segmentの和がその需要に正確に一致する。新TFO2専用の導出versionと
ID domainを使い、旧V1のsingleton packetやIDの意味を変更しない。各segmentのCPU/資本は既存scheduleから導出する。
holding segmentを送信済みwireや供給者の応答機会に数えず、feeへ架空のtransferを追加しない。
これは決定論的な需要・予約であり、実測hardware性能とは区別する。実際のcodec・材料構築・DB/spillには別途、
構築前のNativeDemand・storage予約と、生存期間を覆うleaseを必要とする。

新しい供給feeの原資は、既存material fundingと同じ**sponsor bondの実free collateral principal**経路を用いる。
原資・service collateral・既存Court予約を同じnative stateで照合し、feeを一度だけdebitし、
専用のheld principal・SourceIncome tranche・material receiptへ結ぶ。
元のUTXO運用escrowから同じfeeを重ねてdebitしない。名目exposureを実入金としない。
既存locked bond UTXOと後続native burn obligationによるprincipal拘束を維持し、支払queueだけを増やして
未拘束のprincipalを残さない。資金・供給・予約の一つでも失敗すれば、同じcheckpointで全installを戻す。

### 3.1 actor catalog の未充足項目との対応

`CanonicalWholeBranchActorAssignment`、`SharedServiceOperatorConsent`、
`FeeCapitalRetentionRecoveryBodies` をscheduleの許可bitで無条件に解消しない。TFO2では、
全native inventoryの双方署名を照合した具体的なsupplier planと、元の実signer/source/retentionの
予約、実escrow/fee原資、復旧・保持horizonをそろえた時だけ、構造的なcatalogの未充足を解消できる。
元producer/custodian/panelの責任はliteralな既存署名契約の範囲のままである。新しい材料の供給責任は、
各新supplierが署名した正確な役割・target inventory・graphに従う。匿名のpotential slotを同意済みactorに
数えない。shared service bookは合意の決定論的な容量予約であり、別の無署名の人間operatorを仮定しない。
将来のoutsiderは元受付時のactorへ捏造せず、既存standing bondと署名付きopeningを実際の参加時に検査する。
old TFO1について未充足だった役割をこの新契約によって遡及充足させず、新native pathは保存後・restore時にも
同じ具体的なactor/scope/capitalと全予約の対応を検査する。

### 3.2 新contract固有のnative時計・復元

新TFO2のoriginal allocationには専用のclock version 3を用いる。既存V1/V2の署名、
queue待ち、idle grace、ID、wireの意味を変更しない。新fenceと双方署名の役割集合がない
originalへversion 3を導入せず、tableがない場合も明示的な新contract経路だけを使用する。
新rules/profile IDはこの導出version、実native Court contextと認証済みtable幅を拘束する。

material/reorg retentionの10,000 DAAをownerの操作待ちへ転用しない。新ownerのidle graceは、
実native acquisition planの最大応答body、literal custodian数とowner枠、既存のpart幅・
response/inclusion/parallel容量から既存`tir_public_turn_span_v1`で導出する。
queue quoteと実際のidle evictionは同じ値を読む。進行中ownerの強制交代tenureは、これに加えて
全native localization/acquisition pathと最後のcollection carrierの既存inclusion marginを下回らない。
このtenureもqueue quoteとruntimeで一致させる。conviction filingはowner slotを必要とせず、別のdoorで全量を予約する。
未応答のresponse deadline、準備完了期限、
最後の進捗を含む既存`idle_since`を短縮せず、材料・accepted body・MMR・reacquisitionの保持と費用を維持する。
filingは実nodeのaccepted-offset待ちを含むV2と同じ逐次marginを数える。
既存3,120 DAA door、単価、参加資格、物理容量を増やして受付を通さない。

新TIR Prefix stageは実際の`TirPrefix`のnative slotと認証済みleafを予約する。
旧`PublicPrefix`だけで生成・受付されるhistory recordの復旧経路は、TIRでは到達不能なので
この新contractのTIR quoteへ重ねない。layout公開を供給済みとは扱わず、全native Source query、
StepLeaf/run/tree、FullIds等の材料義務と実rooted bodyの認証を維持する。
TIR PublicOwned自身のMMR履歴とreacquisitionは別の実経路なので削減しない。

restoreでは保存した実native Court/dissection/prompt context、元program/layout、immutable semantics、
認証済みtable literal/EOFからoriginalの全allocationとpacket universeを再導出する。
署名済み元allocationとの完全一致を要求し、現在のActive statusや消費済みfree principalを再受付条件にしない。
version 3のframeが残る間は対応する署名済みcohortの出生metadataを保持する。
paid後のこのmetadata保持は、署名済みfee/collateral horizonを延長せず、frameのGC後に回収する。

### 3.3 署名済みmetadataと完了journalの有限予約

AUX192の内部storage version 2は、immutableな双方署名・native target catalogを一度だけ保存し、
fee/default statusとtargetごとの完了digestを固定幅sidecarへ分離する。外部contract ID、署名message、
tag 250と既存料金率は変えない。target完了ごとに大きな署名付きcatalogと成長する完了mapを再記録しない。
queryはtypedな全行検査と構築前NativeDemandでlogical viewを再構成し、status欠損や孤立sidecarを拒否する。

予約は材料body/partだけでなく、双方の実signature、catalogのpacket ID全件、status/完了行、aux-tree node、
最大query再構成、create/default/queue/paid/deleteまでのundo journal上限も数える。
N targetの完了ごとに全行を複製する旧構造では、base B byteに対しcodecだけで
`2*N*B + 128*N²` byteになる。新構造ではimmutable create/delete、各固定幅completion create/delete、
有限回のstatus遷移をそれぞれcheckedに加算し、target数に線形の上限とする。
このbyte需要と既存quantaからholding node・service/capitalを導出し、料金率や容量上限を増やして補わない。
実codec/query/checkpointのallocationは各境界のNativeDemandとleaseでも別に検査する。


## 4. 応答時計、default、有限fee

fee cutoff `H` は元originalのnative hardに束縛した `respond_until_daa`。
個々の供給義務は既存の予約済みservice packetのdue、service span、inclusion delayと実際の処理機会を使う。
Hまで全default処理を待たず、各packetの既存時計に従って記録する。返金のために期限やinclusion前提を短縮しない。

fee全額をHまで保留し、当該供給者の客観的な役務defaultが選択chain上で**H以内に実際に確定**した場合に
元sponsorへ全額返金する。それ以外はH後に全額を供給者の物理支払queueへ入れる。
未使用枠、未解決告発、ServiceHoldだけでfeeの保留を無期限化しない。
Hより後のdefaultによってfeeを遡及回収しない。feeとservice bond・材料保持責任は別である。
packetのdueがHで最初のdefault処理がH+1になる場合は、この新fee契約でも返金対象外になる。
これは[RFC-0039](0039-finite-fee-consent-and-provider-payment-bounds.md)と同じlate-default制限である。

未供給・未取得・未確定や、実処理機会がない状態だけでslashしない。既存の明示的inclusion前提・
役務の客観的違反・帰責を維持し、no-slash hold/recoveryへ分ける。材料不開示を計算不正へ読み替えない。
供給完了だけでfeeを早期解放せず、既存default履歴も消さない。
TFO2自体が運んだ完全なcanonical tableを受付で認証・rooted保持した場合は、その正確なsemantic targetの
非排他的供給として同じ原子遷移で完了を記録する。既に公開され取得可能な同一原本を重複uploadしないことだけで
役務defaultにしない。出所は元carrier/outpoint/frameとして保持し、実際と異なるuploaderの署名receiptを作らない。
未使用の予約枠とfeeの有限cutoffはそのままで、他roleの材料や単なるlengthの申告を完了へ流用しない。

## 5. 認証済み供給と精算の原子性

材料はboundedなBegin/Part/Finishでstagingし、native上限・署名・offset・重複・checksumを検査する。
Finishで元のparameter/prompt opening、semantic root/cardinality、tokenizer EOF/raw digest、
生成IDとnative selection/stop依存を再認証して初めて、その正確なtargetを完了とする。
別claim/inventoryの材料、申告 `complete=true`、単なるEOF certificateでraw保持を代用しない。
同じ認証済み材料を共有できても、元の役割・generation・保持horizonを勝手に消さない。

支払・返金はheld principalを一回だけ消費し、SourceIncome、物理queue、receiptの保持・解放と一致させる。
queue満杯・合算overflow時は未消費のまま再試行する。reorg/undo、再起動、pruned IBDで同じ義務・原資・
完了・default・一回性を復元する。保存則は実担保debit = 未解放held + 実支払/返金予約と、
対応するlocked principal/native burn accountingの組で検査し、二重発行・二重回収を許さない。

## 6. 検証と適用範囲

新規入口は専用のoptional `palw_court_family_supplier_v1` activationで区別し、既定はNoneとする。
既存の17400等のfenceを再利用して、過去に受理したTFO1の意味を変更しない。
有効化後の新規original受付は新envelopeと全役割を必要とし、旧envelopeへのfallbackで迂回しない。
有効化前に受理済みのoriginalと署名は元の契約で読み取り・処理する。新しい本番起動高は設定しない。
新fenceがNoneのpresetの既存fingerprintを保存し、有効時だけ新しい名前付きidentityを拘束する。

四役割の実材料、誤署名、scope差替え、不足資金、同じsponsor/供給者の多重契約、短いhorizon、
資源不足での全rollback、timely/late default、未使用枠、物理queue混雑、reorg、native carriage restoreを検査する。
helper成功やコードmergeを完成・有効化と扱わない。認証はデータ同一性と署名者の同意であり、
計算の正しさの暗号学的証明を使用しない。claim単位のGate C/court/抽選検証者認証は維持する。
モデル二階層・モデル単位の無審査を変更しない。画像・動画とモデル改善は初期mainnetの対象外のままとする。
