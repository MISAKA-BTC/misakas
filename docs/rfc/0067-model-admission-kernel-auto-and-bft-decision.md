# RFC-0067 — MAD2：kernel自動追加とBFT YES/NOによるモデル採用

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](../adr/0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](../governance/model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

正規BFTによるモデル廃止を可逆Pauseと区別した恒久終端にする。モデル廃止と個人の有罪判定を分け、次のscopeだけを処理する。

| 対象 | BMR1のモデル廃止時 | 個人不正との関係 |
| --- | --- | --- |
| 当該モデルの永久LP内MISAKA | 認証済みactual reserveを100%burn | V・receipt累計・別pool残高を実資産に数えない |
| 当該モデルの登録者責任bond | 対象の責任lotを100%burn | 無関係なwallet/別model資産へ拡張しない |
| 当該モデルのmining bond | MINING用途lotを100%burn | 同じ人がPanelを兼任しても用途を分ける |
| 当該モデルの未確定mining報酬 | 100%失効/burn | 未発行予約失効と実資産burnを二重計上しない |
| 全Panel/Gate C verification bond | **一律にBFT廃止burnから保護** | 不正者も同じ保護。個人不正はCourtの個別slashで処理 |
| 正当に成立した検証報酬 | MISAKAで支払を維持 | 廃止を理由に成立済み役務対価を横取りしない |
| 他モデルの資産・bond・報酬 | 影響なし | 同一address・同一参加者という理由で混ぜない |

兼任者は用途別lotで管理する。MINING6,000/PANEL4,000なら前者6,000だけをburn、検証用途4,000は保護する。同じ元本の重複担保と責任中の用途変更を禁止し、廃止直前の付替えでburn又は保護を迂回しない。

通常の永久LP withdraw/redeem・管理者引出し禁止を維持し、BMR1 canonical廃止だけにactual MISAKA reserveの宛先なし消却を認める。**BFTは没収を受け取らず、廃止bountyは0**。継続5%を廃止件数や没収額に連動させない。RETIREDを通常Resume/BurnResumeで戻さず、台帳finality・fork choice・任意残高処分の権限を追加しない。

旧「BFTは停止だけ」「永久LPを一切burnできない」等は、この明示的な廃止scopeだけで置換する。旧claim/旧profileの義務・履歴・wire・実装/過去試験を保全し、BMR1 native接続・移行実行・公開activationを今回完了扱いしない。

**個別Court slashの実回収額は別会計。** 対象offenceから実際に回収したMISAKAをSとし、Panel/Gate C合計の報酬を`E=floor(490S/1000)`、残り`T=S-E`をburnとする（名目49%/51%、端数はburn）。49%は両役割の合計であり、それぞれ49%ではない。不正発見・立証の資格を認証したplanで一回配分し、不正本人の自己slash受領と同一offenceの二重報酬を禁止する。BFTモデル廃止の100%burn資産をSへ加えず、BFTの没収受取・廃止bountyは0のまま。検証bondがBFT一律burnから保護されることは、本人のCourt個別slash免除ではない。
<!-- BMR1-ADOPTED-20261126-END -->

<!-- MOE3-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — 全モデル統一・小数POSITION・50/20/20/5/5

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0068](0068-fractional-position-miner-rewards.md)／[ADR0236](../adr/0236-fractional-position-miner-rewards.md)／[MOE3適用・移行](../governance/fractional-position-economics-transition.md)を新経済の正本とする。ユーザーの最新指定は、対象coinbase原資Cの**50%をMiner用POSITION購入、20%をLP用POSITION購入、20%をMISAKAのLP、5%をPanel/Gate CへMISAKA、5%をBFTへMISAKA**である。Minerと検証者の受取資産を分け、5%枠内のPanel/Gate C配分は認証済みplanに従う。新しい半々配分等を暗黙設定しない。

整数分割は`F=floor(50C/1000)`、`P=floor(50C/1000)`、`A=floor(200C/1000)`、`J=floor(200C/1000)`、`D=C-F-P-A-J`、`B=D+J`。FがBFT、Pが検証者、AがLP用MISAKA、JがLP用POSITION購入原価、DがMiner用購入原価であり、**通貨の全丸め残差はD**へ置く。B（名目70%）を一括購入し、購入qから`ell=floor(q*J/B)`をLP、`m=q-ell`をMinerへ配る（B>0）。単一モデルにC全額を割り当て、かつCが20の倍数の例ならell=floor(2q/7)で名目2/7 LP・5/7 Minerとなるが、端数時も無条件に2q/7としない。`C=B+A+P+F`、`q=ell+m`を保つ。POSITIONの50%/20%相当は取得原価の按分であり、換金額の保証ではない。

全モデルを係数1.0・各モデルの一意市場・MAD2管理へ統一する。新fork特別区分、追加モデル専用κ、coreの市場/報酬免除、genesisのBFT管理免除を継承しない。originは履歴として保存し、genesisの差は正規creatorがいない点だけとする。POSITIONは小数8桁、`1 POSITION=100,000,000 atoms`として固定精度整数で計算する。

通常AMMはburn5%を維持し、creatorありの総手数料1%/名目net94%、正規genesisのcreatorなしは0%/名目net95%とする。不存在creatorの1%を徴収・追加burnしない。内部購入・LP・配布の追加AMM feeは0。通常の永久LP撤去禁止・claim検証・共通発行/資本/責任capを維持し、BMR1正規廃止の限定burnは後続規範に従う。

同日途中の「80%購入・q/4 LP・60%相当Miner」「Wから可変Panel枠を引いてRへ」「BFT10%」は**今回の固定50/20/20/5/5に置換済み**であり、新規受理に使わない。旧MOE1/MOE2のwhole数量・30/20/50・owner3%・旧claim/wire/native/過去試験は元profileの記録として保持する。MOE3のnative接続・移行実行・本番有効化の完了ではない。

一般/複数モデルでは、RFC0068 §4.3で一度分割したCの各legを結合按分し、各モデルの実予算比`floor(q_i J_i/B_i)`でLP数量を求める。Cが20で割り切れるだけでは各モデルの比率が2/7になるとは限らない。
<!-- MOE3-ADOPTED-20261126-END -->

- 日付: 2026-11-23（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**。ユーザーの今回の指示を設計へ反映する。native実装、投票、モデル承認、公開activationの完了ではない。
- 論理profile: **MAD2（Model Admission, two routes）**。既存MAS1の評価・停止部品を利用する後続の入口規則であり、新しい台帳・乱数・計算証明方式ではない。
- 判断記録: [ADR-0235](../adr/0235-model-admission-kernel-auto-and-bft-decision.md)
- 適用・移行: [MAD2移行契約](../governance/model-admission-dual-route-transition.md)
- 機械可読契約: [mad2-admission-policy.json](../design/palw/models/mad2-admission-policy.json)
- 必要性監査: [PALWの検証責務と簡素化](../design/palw/consensus/verification-responsibilities-and-simplification-2026-11-23.md)

## 0. 今回の決定

genesisに元から含まれるモデルを除き、追加モデルの採掘採用に二つの入口を設ける。fork指定はBFT経路で行う。origin別の権限は§1.1に従う。

1. **KERNEL_AUTO** — 対応kernelへの自動分解・loweringと、採択済み自動審査規則で合格したモデルを追加する。
2. **BFT_DECISION** — 権限を持つ固定集合のYES/NOで追加するかを決める。G-COLLの数学的定理、G-COSTの費用証明・測定証拠、G-USEの評価証明を、承認票の有効条件にしない。

BFT経路は自動経路の全PASSを再要求するfallbackではない。自動判定のUNKNOWN又は採用判断に関するFAILが残っていても、共通の技術条件を満たすモデルはYES承認の対象にできる。登録者は自動解析のtimeoutを待たずBFT経路を選べる。判定の元の値をPASSへ書き換えず、採用根拠をBFT承認として別に保存する。

この変更は、モデルの安全性が数学的に証明されたことを意味しない。BFT担当者の判断と故障前提に依存する採用方針である。各claimの正しい計算、Ticket、Panel、Court、Final、独立Gate C、権利一回消費とブロック検証は維持する。

## 1. 登録・採用・実行を区別する

- **ModelRegistered**: artifact、重みroot、program、数値profile等の不変な登録。追加モデルはMOE2の市場を同じ原子遷移で作る。
- **MiningAdmission**: その不変instanceを新規採掘へ使用してよいという認可。元genesisは固定bootstrap、追加モデルは二経路、forkモデルはBFT指定を根拠にする。
- **ClaimEligibility**: 個別の実行が、現行の承認、実行検証、認証、当選、期限、一回会計を満たすか。

市場の存在・Position購入は採掘採用を意味しない。NO・保留・停止は登録済み市場の削除や資産没収を意味しない。経済分類は投票本文へ固定し、BFT fork指定は既存FORK_DESIGNATED規則を選ぶ。genesis免除を付与するものではなく、既存の経済係数・MOE2比率自体は変更しない。

### 1.1 originとBFT権限

ユーザーの追加指示により、**BFTが管理・停止できないモデルは、chain開始時に固定された元のgenesisモデルだけ**とする。経済上のcore分類と、この免除の根拠を分離する。

| 不変origin / 経済分類 | 追加・採用 | BFTによるモデル管理・停止 | Optionと対象worker報酬R |
| --- | --- | --- | --- |
| GENESIS_ORIGINAL / GENESIS | genesis manifestの固定entryで採用 | 不可。追加・差替え・除外・停止・解除・再審の対象にしない | Optionなし、MinerへRの100% |
| ADDITIONAL_REGISTERED / ADDITIONAL | KERNEL_AUTO又はBFT YES | 可。自動追加されたモデルも含む | MOE2の30%購入・20%永久LP・残余 |
| FORK_BFT_DESIGNATED / FORK_DESIGNATED | 対象分類を束縛したBFT YESで追加 | 可。core扱いでも停止・正規解除の対象 | Optionなし、MinerへRの100% |

GENESIS_ORIGINALは、network/genesis hash、genesis manifest entry、model ID、artifact root、初期実行契約の一致から判定する。自己申告、モデル名、後日のcatalog、経済係数、同rootの複製又はBFT票から作れない。chain開始後にGENESIS_ORIGINALを追加する遷移を設けず、既存genesis descriptorの差替えをBFTで承認しない。

ここでのBFT対象外は**モデル管理権限**の境界である。genesisのclaimも正規実行・Panel/Court・Final・Gate C・Ticket・一回会計・ブロック検証を必要とし、不正claimや不整合なブロックを受理しない。BFTはgenesisのモデル採用を再審しないが、客観的なclaim拒否を禁止するものでもない。

MODEL_STOP/RESOLVEの対象がgenesisだけなら、quorumの票数にかかわらず権限外として拒否する。rule/family/kernelをscopeにした停止もGENESIS_ORIGINALを対象集合から除外し、共有ruleのdisableやaliasを使ってBFTの停止禁止を迂回しない。技術上の共通checker故障をBFT票で偽装してgenesisを停止する権限を与えない。これは参加者やネットワークの物理的な停止・検閲まで防ぐ活性保証ではない。

### 1.2 BFTでforkモデルを追加する

BFT proposalはrequested_model_classをADDITIONAL又はFORK_DESIGNATEDとして署名対象に含める。後者のq YESは、既知kernelの技術互換を満たす不変instanceを正式fork catalogへ指定する認可である。数学的定理・費用/品質の証拠はどちらの採用票にも必須にしない。forkの自動指定は行わない。

fork候補は不変descriptorをproposalへ固定し、先にADDITIONALとして市場付き登録することを必須にしない。正規のYES包含でmodel登録・fork catalog指定・採用basisを同じ原子遷移に確定し、Option市場を生成しない。q NO又は未決期限切れから登録済みfork資格・Ticket・報酬を作らない。

forkモデルは既存FORK_DESIGNATEDの価格付け・経済規則を引き継ぐ。genesisと同じR100%帰属とOption対象外を持つが、genesis専用のBFT管理免除を引き継がない。検証役務の原資はRとは別に事前確保する。

MODEL_STOPは追加モデルとforkモデルの両方に適用し、正規の停止・解除record、期限、世代、現在の承認を検査する。BFTが既知のkernel/profile内でモデルを指定することは、Nodeの新primitive、台帳finality、fork choice又はソフトウェア互換性を投票だけで変更する権限ではない。

既存のADDITIONALをforkへ変える場合は、MOE2の永久LP付きID・同rootのcore指定禁止、旧市場と義務のdrain、既存停止を継承する。分類変更で停止を消さず、genesisへ昇格する経路は常に拒否する。新しい別forkモデルを追加する場合も、旧市場・権利・債務を勝手に移さない。

旧catalogによるfork指定は、MAD2の明示移行recordで元の指定根拠を保持して扱う。移行後もgenesis免除は与えず、forkモデルはBFT管理対象とする。旧claimの署名意味・元期限・確定資産は遡及変更しない。

## 2. 共通の技術条件

二経路とも、Nodeが同じ計算仕様を解釈し、個別claimを検証できることを必要とする。この条件を **BaseCompatible** と呼ぶ。

| 条件 | 必須内容 |
| --- | --- |
| 同一性 | artifact、program、tokenizer、入力規則、Readout、数値profileの正規digest。欠落・placeholder・別modelへの差替えは不可 |
| 実行仕様 | shape/type、丸め、overflow、sampling、停止、資源上限を一意に固定。秘密状態・未固定外部API・非決定的意味を含めない |
| kernel対応 | 必須の全演算・状態遷移を、有効化済みkernel/TIRと有界checkerが解釈できる。未知primitiveを無視しない |
| 入出力とtrace | 実challenge、回答、採掘値、四Root、trace、元claimの位置・境界が対応する形式を持つ |
| 規約適合 | コアReadoutを単なる試行IDのechoへ置換しない。任意saltや可変符号化で追加Ticketを作らない |
| 現行状態 | 対象policy・kernel版・有効化条件が一致し、hard protocol failureや停止を無視しない |

これは既知の実行契約への互換性検査である。モデルごとの計算困難性、数値状態の非衝突性又は最低費用の証明ではない。全重みの未確認の数学的性質を、共通技術条件の名でBFT経路へ再導入しない。

**違反を分ける。** 未対応演算、壊れたcodec、非決定的仕様、認証root不一致、既知の実行仕様違反は技術的拒否でありYESで免除しない。一方、G-COLL根拠の欠如、安価な正しい算法の懸念、実用scoreの不足は採用判断の項目であり、BFT経路では自動判定と異なる採否判断を許す。既知の安価経路も履歴から消さない。

## 3. KERNEL_AUTO — 自動追加

### 3.1 処理

登録artifactを解析し、graphを対応TIR/kernelへloweringし、認証されたLoweringManifest・StateMap・Readout仕様を作る。Nodeは登録された既存checker、必要な公開評価とその認証結果から、固定policyの判定を再現する。解析workerの「成功した」という署名だけで全重みの性質を真としない。

自動審査は従来MAS1の範囲を継承する。

- G-EXECに対応する実行・局所反証契約。
- G-COLLの採択済みTheoremRuleとinstanceの適合条件。
- G-COSTの事前固定条件による反復比較。全算法の数学的費用下界は要求しない。
- G-USE、構造解析、現行世代・rule・停止状態。

全条件PASSでAUTO_APPROVEDへ進む。UNKNOWNは自動追加しない。採用判断のFAIL/UNKNOWNからは、明示的なBFT proposalを作れる。共通技術条件が不成立なら、その欠陥を解消するまで採掘不可である。

既存のkernel系列の共通規則に適合すれば、各モデルに新しい手書きの定理又は人の投票を要求しない。適合する自動規則が一件もないことを理由に、自動PASSを捏造しない。

### 3.2 「kernelの自動追加」の範囲

| 追加対象 | 扱い |
| --- | --- |
| 既知primitiveで表現できる新モデル・新graph・新shape | 資源とcoverageを満たせば自動lowering・登録の対象 |
| 既知primitiveを組み合わせた生成kernel・高速化backend | 規範意味は既存TIRで固定。最適化codeはホスト実装であり、独立した合意意味を追加しない |
| 既知の実行kernelだが、困難性等の自動審査に未対応 | BFT_DECISIONを選択できる |
| Nodeが意味又は裁定を知らない新primitive | 両経路とも実行可能にしない。checker・codec・実装・明示upgradeを先に用意する |

モデルを登録するだけでNodeに任意のnative binaryをロードさせない。BFTのYESも、未配布kernelの意味をNodeへ魔法のように追加するものではない。既存のversioned-kernel方針を維持する。

## 4. BFT_DECISION — 証明提出を要求しないYES/NO

### 4.1 採否

権限を持つ担当者は、同一の固定proposalに **YES（採用する）** 又は **NO（採用しない）** を署名する。判断理由、測定、利用経験などを任意で添付できるが、数学的証明・費用測定一式・品質評価証明・反証の提出を投票又は承認の成立条件にしない。任意資料が欠けても有効票である。投票の前にモデルidentityと技術互換を固定し、採否の根拠を得るために任意資料を読む判断と、Nodeが検証する票の有効条件を区別する。

Nodeが検証するのは、対象の同一性、署名、権限、定足数、世代、期限、canonicalな起点と現在状態である。モデルが「賢い」「推測困難」という判断そのものをNodeが証明し直すことはない。

BFT票の署名検証は本人確認であり、LLM計算の暗号学的validity proofではない。証明不要とは、無署名のYES一文字や任意の第三者票を受理することではない。

### 4.2 集合と定足数

同じproposalの開始前にnativeで固定したrosterに、MODEL_ADMISSION権限を明示付与する。既存TB1 holder rosterを共用できるが、TB1 share鍵、MODEL_STOP、claim Panel、Gate Cの署名を流用しない。新しいauthority、署名domain、codecは有効化前に固定する。

論理条件はn=3f+1、f>=1、故障者は高々f、q=2f+1の相異なる正規票とする。実際の人数・epoch・交替・容量は今回割り当てない。一人の正直Panelという前提からBFTの故障上限を導かない。モデル登録者が自分でrosterを選ぶことも認めない。

- q YESと共通技術条件が成立: BFT_APPROVED。
- q NO: REJECTED_BY_VOTE。
- 期限までにどちらもqへ到達しない: EXPIRED_UNDECIDED。採掘を許可しない。
- 少数のNOは単独vetoにしない。単なる無応答をYES又はNOに変換しない。

これは既存native台帳で順序付ける限定採用投票であり、別のBFT blockchain/finalityを導入しない。正直な担当者が採否で分かれれば決定できない場合がある。定足数だけから審査の活性や判断の正しさを保証しない。

### 4.3 署名する対象

ModelAdmissionBallotは少なくとも次をcanonicalに束縛する。

    domain / version / action=MODEL_ADMISSION
    chain_id / genesis_hash
    proposal_id / canonical_open_anchor
    immutable_descriptor_hash / active_kernel_contract_digest
    requested_model_class={ADDITIONAL,FORK_DESIGNATED} / immutable_origin
    policy_id / policy_version / admission_generation
    roster_epoch / roster_root
    deadline / decision={YES,NO}

自由な説明文は署名対象本文の必須fieldにせず、任意の別資料として扱う。TheoremRule、EvidenceRoot、G-COST/G-USE結果はBFT票の必須添付ではない。対象不明の「Qwenを承認」やownerだけへの包括承認を禁止する。

同じVoteKeyについて、正直な担当者は片方だけに署名し、再起動・reorgでも署名済み決定を保持する。roster交替や世代変更で既存caseの票を使い回さない。再審は正規終端後の新しいproposalで行い、旧結果と停止履歴を残す。

## 5. 一つの採用record、一つの実効判定

追加モデルの論理的なAdmissionBasisを次の識別付きunionにする。fork指定はBFT_DECISIONとrequested_model_class=FORK_DESIGNATEDを使用する。元genesisの固定bootstrapはモデル追加の第三経路ではなく、起動時の不変registryを参照する。

    KERNEL_AUTO { policy, generation, automatic_record_digest }
    BFT_DECISION { policy, generation, proposal_id, quorum_record_digest }

BFT recordにはassurance=BFT_POLICY_APPROVALを保存する。G-COLL=UNKNOWN等の元測定値は保持し、G-COLL_PROVEDやG-COST_PASSへ書き換えない。AUTO_APPROVEDとBFT_APPROVEDを同じ数学的保証として表示しない。

全consumerは次の一つの述語を使う。ImmutableGenesisBootstrapMatchはBFTが更新できない元genesisの正確な一致であり、任意のcoreフラグではない。

    EffectiveModelAdmission =
        ExecutionProfileActive
        AND BaseCompatible
        AND (
            ImmutableGenesisBootstrapMatch
            OR (
                MAD2Active AND CurrentDescriptorPolicyAndGeneration
                AND (
                    (ADDITIONAL AND (ValidAutomaticApproval OR ValidBftYesApproval))
                    OR (FORK_DESIGNATED AND ValidBftForkYesApproval)
                )
                AND NoApplicableTechnicalFailureOrStop
            )
        )

    EligibleClaimEffect =
        EffectiveModelAdmission
        AND ExistingClaimVerificationAndCertification
        AND ExistingTicketAndOneUseAccounting

共通gateの外側へG-COLL/G-COST/G-USE全PASSをANDし直してBFT経路を実質的に無効にしない。逆に、BFT_APPROVEDだけで個別claimをFinal又はSecurityCertifiedへ昇格させない。

採用recordの再署名・署名subset・補足説明の違いで、新しいTicket、仕事キー、モデル市場、報酬を発生させない。proof/署名をTicketの自由fieldへ入れない。現行policyとgenerationへの有効な参照を、通常claimの不変contextで固定する。

## 6. 停止・解除・二経路間の移行

既存MAS1の停止集合、burn停止との分離、同batch停止優先、元期限・責任の保持を、ADDITIONALとFORK_DESIGNATEDへ適用する。GENESIS_ORIGINALはBFT採用・停止・解除の対象外とする。承認やfork指定は既存停止の消去ではない。

- 客観的technical failureや未対応checkerはYESで正常化しない。BFTのrule停止はgenesisへ作用させず、genesisの元実行契約を差し替えない。
- BFT安全停止は、現在rosterによる明示の解除と、正確なStopId集合・直前control record・世代への束縛を必要とする。
- 自動経路の解除はその経路の現在の自動条件を必要とする。
- BFT経路の解除では、共通技術条件、現在のYES決定、正規解除を確認する。G-COLL定理やG-COST/G-USEの証拠を解除の隠れた必須条件にしない。
- 同batchの新しい停止は承認・解除より優先する。未知の別停止まで一括消去しない。
- TheoremRuleだけが失効した場合、自動承認は無効になる。BFTへの切替には新しい明示proposalを必要とし、既存停止のscopeを確認する。実行checkerの失効はどちらの経路にも効く。
- owner変更、alias、再登録、世代increment、timeout又はburnだけで停止を回避しない。

NOは当該proposalの不採用であり、別の有効な既存承認を自動的に取り消す停止票ではない。稼働中モデルを止める場合は専用のMODEL_STOPを使用する。承認・停止・解除はnative包含で初めて効き、mempool到着順でNode間の状態を変えない。

旧版で正規受理されたclaimは元の規則・期限・責任で処理する。新policyを、過去claimの救済、落選Ticketの再抽選、失効権利の復活に用いない。

## 7. claim検証に与える影響

モデル追加投票は、個別のMinerの計算を承認するものではない。

- Panelは採用経路にかかわらず同じ元claimを完全再実行する。
- Courtは元のmodel/trace commitmentへ結び付いた局所関係を裁定する。
- 資料不足・未解決case・停止・元期限違反は成功を保留又は既定終端へ送る。
- Finalと独立Gate Cの意味を維持する。
- 唯一Ticket、当選条件、権利の一回消費、原子的会計とブロック検証を維持する。

BFT_APPROVEDでも、正しい計算を安くできる可能性は残る。それは採用方針のリスクであり、正しい結果を算術詐欺として自動slashする理由にはしない。悪い採用判断への対応は新規採掘の停止・再審とする。過去の正当な確定残高をYES/NOで没収しない。

## 8. quorumで示せることと示せないこと

署名が偽造困難で不正者が高々fなら、q=2f+1票には少なくともf+1人の正直な署名者が含まれる。同じ固定rosterの二つのq集合は2q-n=f+1人以上で重なる。正直な署名者が同一VoteKeyへ両方署名しない条件の下では、相反するYES/NOの二つの有効なquorumを作れない。

これで保証できるのは、定義された前提下の認可・非二重決定である。正直な人の誤判断、未知の高速算法、非衝突性、計算費用下界、実用性、BFTへの賄賂やfault budget超過を数学的に排除したことにはならない。

相反する有効quorumを観測した場合は、前提破れ又は実装不備として対象proposalの新規成功を保留し、証拠を残す。より早くRPCへ届いた方を任意採用しない。native合意自体をBFTで書き換えない。

## 9. 簡素化の境界

実装は二つの採用システムを並立させず、一つのadmission recordとreducerにbasisだけを分岐させる。追加のBFT乱数、別Final、別報酬台帳、別の採掘Ticketを作らない。

claim側は共通のContextBinding、ExecutionChecker、ClaimEligibility、WorkUse/Settlementの責務へ集約できる。Panel・Court・Gate Cが共有するdescriptorや署名検査を重複実装しない。ただし論理的な条件と独立性は維持する。

独立Gate Cの廃止、Panelとの抽選統合、期限短縮又は全Nodeによるモデル再実行への変更は今回採択しない。それらは安全性・資源・移行への影響を比較する別の設計変更である。任意対話は従来どおり初回公開の必須機能にしない。

## 10. migrationと実装境界

対象network、policy版、署名domain/codec、roster権限、提案受付量・料金・期限、journal、保護容量、activationを明示してから新規MAD2を有効にする。今回wire ID又は本番数値を割り当てない。

既存MAS1 recordは旧意味で読む。新たなBFT_DECISIONを旧enumのPASSEDへ黙って格納しない。旧QF1 REVIEW_HOLDは自動解除されず、実装・有効化・そのinstanceへの正規投票が成立して初めて新経路を使える。

originの移行は、chain開始時のgenesis manifestを根拠に行う。既存genesis向けBFT停止recordがある場合も過去の履歴から削除せず、MAD2のgenesisモデル管理には適用しない。旧claimは元のdrain規則を保持する。旧fork指定は元catalog根拠を残して非genesisの管理対象へ移行する。originを確認できないrecordをGENESISへ推定分類してはならない。

RPC/Studioはtechnical compatibility、automatic assessments、admission basis、vote result、stops、claim certificationを区別する。既登録の市場は追加作成しない。snapshot/IBD/reorgは同じrecord・policy・roster・停止集合を復元する。

## 11. 受入条件

1. 自動全PASSでBFT票なしに自動承認できる。
2. G-COLL/G-COST/G-USEがUNKNOWNでも、BaseCompatibleとq YESでBFT承認できる。証拠添付が空でも票を受理する。
3. 自動の採用判断FAILをBFT承認した場合も元FAILを保存し、保証表示を分ける。
4. hard technical failure、未知kernel、q未満、別model/network/世代/roster、期限切れ、重複票を拒否する。
5. q NOと未決期限切れは新規採掘を許可しない。
6. 同batchの停止、古い解除、alias、reorg、snapshotで承認の迂回がない。
7. BFT採用済みモデルの不正claimはPanel/Courtで拒否し、Gate Cなし・落選・二重消費も拒否する。
8. 評価専用のtraceを後から採掘creditへ転用しない。
9. 実装、3ピア試験、資料/署名の保持、公開activationを別々に確認する。
10. genesisへBFT STOP/RESOLVE、catalog差替え、rule経由停止、後付けGENESIS指定を行っても拒否する。
11. BFT fork追加はOptionなし・Miner=Rで登録し、同じモデルをBFT STOPで停止・正規解除で再開できる。
12. 自動追加モデルもBFT停止対象であり、分類・alias・永久LP消去で停止と経済義務を迂回できない。

文書上の状態表・抽象試験だけで、本番の署名検証・合意・費用・モデル安全性を検証済みとはしない。
