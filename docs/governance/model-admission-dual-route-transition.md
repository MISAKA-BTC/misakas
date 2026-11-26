# MAD2 — 二経路モデル採用の適用と移行

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](../adr/0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

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

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0068](../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](../adr/0236-fractional-position-miner-rewards.md)／[MOE3適用・移行](fractional-position-economics-transition.md)を新経済の正本とする。ユーザーの最新指定は、対象coinbase原資Cの**50%をMiner用POSITION購入、20%をLP用POSITION購入、20%をMISAKAのLP、5%をPanel/Gate CへMISAKA、5%をBFTへMISAKA**である。Minerと検証者の受取資産を分け、5%枠内のPanel/Gate C配分は認証済みplanに従う。新しい半々配分等を暗黙設定しない。

整数分割は`F=floor(50C/1000)`、`P=floor(50C/1000)`、`A=floor(200C/1000)`、`J=floor(200C/1000)`、`D=C-F-P-A-J`、`B=D+J`。FがBFT、Pが検証者、AがLP用MISAKA、JがLP用POSITION購入原価、DがMiner用購入原価であり、**通貨の全丸め残差はD**へ置く。B（名目70%）を一括購入し、購入qから`ell=floor(q*J/B)`をLP、`m=q-ell`をMinerへ配る（B>0）。単一モデルにC全額を割り当て、かつCが20の倍数の例ならell=floor(2q/7)で名目2/7 LP・5/7 Minerとなるが、端数時も無条件に2q/7としない。`C=B+A+P+F`、`q=ell+m`を保つ。POSITIONの50%/20%相当は取得原価の按分であり、換金額の保証ではない。

全モデルを係数1.0・各モデルの一意市場・MAD2管理へ統一する。新fork特別区分、追加モデル専用κ、coreの市場/報酬免除、genesisのBFT管理免除を継承しない。originは履歴として保存し、genesisの差は正規creatorがいない点だけとする。POSITIONは小数8桁、`1 POSITION=100,000,000 atoms`として固定精度整数で計算する。

通常AMMはburn5%を維持し、creatorありの総手数料1%/名目net94%、正規genesisのcreatorなしは0%/名目net95%とする。不存在creatorの1%を徴収・追加burnしない。内部購入・LP・配布の追加AMM feeは0。通常の永久LP撤去禁止・claim検証・共通発行/資本/責任capを維持し、BMR1正規廃止の限定burnは後続規範に従う。

同日途中の「80%購入・q/4 LP・60%相当Miner」「Wから可変Panel枠を引いてRへ」「BFT10%」は**今回の固定50/20/20/5/5に置換済み**であり、新規受理に使わない。旧MOE1/MOE2のwhole数量・30/20/50・owner3%・旧claim/wire/native/過去試験は元profileの記録として保持する。MOE3のnative接続・移行実行・本番有効化の完了ではない。


burn-pauseも全モデル共通とし、Pauseには正規支払者署名・規定停止burnを要する。Resumeはnative登録者正規認証OR専用BFT BurnResume承認に加えて、どちらも規定解除burnを必要とする。登録者不在genesisにも同じBFT経路を使い、通常採用YESを流用しない。旧確定PAUSEDのsequence/前操作を保持し、旧pendingは元policyでdrainする。独立BFT safety stop・原期限・RETIREDはBurnResumeで解除しない。具体的burn額を今回ゼロや任意額に変更しない（RFC0068 §2.4）。

一般/複数モデルでは、RFC0068 §4.3で一度分割したCの各legを結合按分し、各モデルの実予算比`floor(q_i J_i/B_i)`でLP数量を求める。Cが20で割り切れるだけでは各モデルの比率が2/7になるとは限らない。
<!-- MOE3-ADOPTED-20261126-END -->

2026-11-23、ユーザー明示指示による **ADOPTED_DESIGN_NOT_ACTIVATED**。
正本は[RFC-0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)と[ADR-0235](../adr/0235-model-admission-kernel-auto-and-bft-decision.md)。

## 1. 優先順位

明示的にMAD2を有効化する新規採掘profileについて、モデル採用規則は本契約とRFC0067を優先する。旧MAS1の「UNKNOWNは全経路で採掘不可」「BFTは停止専用」「全モデルにG-COLL定理とG-COST/G-USE合格を必須」はKERNEL_AUTOに限定する。BFT_DECISIONでは証拠提出なしのYES/NO決定を使用できる。

現在の旧profile、署名済みclaim、snapshot、過去の検査世代を遡及変更しない。過去のUNKNOWN測定はUNKNOWNのまま残る。新規承認recordは元の採用判断を参照し、数学的PASSを偽装しない。

## 1.1 originと管理権限

GENESIS_ORIGINALは不変genesis manifestで特定し、BFTは採用・変更・削除・停止・解除へ関与できない。追加モデルは自動又はBFT YESで採用し、いずれもBFT管理・停止対象とする。FORK_DESIGNATEDはBFT YESでも追加でき、Optionなし・Miner=Rを継承するが、BFT停止・解除の対象である。

genesis同様の経済扱いを、genesis由来の停止免除へ読み替えない。BFTによるGENESIS再分類、rule/family経由の停止、同root複製での免除付与を拒否する。個別claimの不正拒否・既存検証はgenesisにも適用する。

## 2. 引き継ぐ条件

共通技術互換、モデルと重みの認証、決定的な実行とkernel coverage、Panel/Court、独立Gate C、TB1、Ticket一意性、一回会計、停止・元期限・責任を引き継ぐ。暗号学的な計算validity proof、DNS-BFT、HF取得証明制度を復活させない。

追加モデル登録と市場原子生成、MOE2の30%買付・20%永久LP・余剰burn・残余、通常取引5%/3%、coreの対象worker報酬100%は変更しない。

## 3. 入口と解除

全consumerは次を共通参照する。

    BaseCompatible AND 現行実行profile
    AND (
        元genesisの不変bootstrapと一致
        OR (
            MAD2有効化 AND 現行世代
            AND (追加モデルのAUTO又はBFT YES OR forkモデルのBFT YES)
            AND 適用停止なし
        )
    )
    AND 個別claimの既存gate

BFT承認を記録する場合、TheoremRuleと評価証拠は必須fieldにしない。旧MAS1の全PASS predicateを最後に重ねない。現在のSTOPをYESで消さず、BFT経路に適合した明示解除を必要とする。ROSTER権限、投票domain、proposal・generation・期限を固定し、承認と停止を同じnative順序で適用する。

## 4. 実装・activation前の確認

1. 新policy/codec/admission basisを版付きで実装する。旧enum・旧署名意味を変えない。
2. 既存kernelへの自動lowering・審査入口と、技術互換だけを共通要件にするBFT入口を接続する。
3. MODEL_ADMISSION権限を明示付与し、署名済みYES/NO、重複・別scope・期限・reorgを検査する。
4. RPC、Studio、miner、Panel、Node、Final、Gate C、未使用権利消費、reward、import/IBDで同じeffective gateを使う。
5. 技術失敗は両経路で拒否し、未証明の採用判断はq YESで通せることを短時間試験で確認する。
6. 実モデルに正規の投票を行い、その結果を記録する。文書を追加しただけでQF1 REVIEW_HOLDを解除しない。
7. genesisへのBFT制御拒否、BFT forkの追加と停止、追加モデルの両経路への停止、originのsnapshot/IBD復元を確認する。
8. 受賞・決済・再起動・reorgを通常ブロックで検証し、公開activationを別途決定する。

BFT経路の開始条件へ、G-COLLの定理や全G-COST/G-USE測定の完成を再挿入しない。自動経路の開始条件はその規則に従う。非報酬評価は任意の判断資料として再利用できるが、評価traceに後付け報酬を与えない。

## 5. 文書と履歴

現在入口のRFC/ADR-v2は20冊/50冊を維持し、新規記録は旧名前空間のRFC0067/ADR0235として追記する。元の統合payload、実測ログ、runtime、公開fence、配備、支払いを本改訂で変更しない。

[改訂記録と検査](../maintenance/revisions/2026-11-23-dual-route-admission/README.md)と[必要性監査](../design/palw/consensus/verification-responsibilities-and-simplification-2026-11-23.md)を参照する。
