# QF1適用・移行 — Qwen3のFOW1実行具体化

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](../adr/0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

正規BFTのモデル恒久廃止で、当該永久LP内actual MISAKA・登録者責任bond・MINING用途bond・未確定mining報酬を100%burn/失効させる。**全Panel/Gate C検証用途bondは、不正者も含め一律にBFT廃止burnから保護**し、個人不正はCourtの個別slashへ分離する。正当に成立した検証報酬の支払を維持し、他モデル資産へ影響させない。兼任は用途別lot（MINING6,000だけburn、PANEL4,000は保護）とし、重複担保・責任中の用途変更は禁止する。

通常のLP撤去禁止を維持し、canonical廃止のactual reserve burnだけを限定例外にする。BFTの没収受取・廃止bountyは0。継続5%報酬を没収額に連動させない。可逆Pause/Resumeと区別し、旧停止専用・LP絶対不処分の記述をこのscopeだけ置換する。旧profile/native/履歴を保持し、BMR1実装・移行・有効化は別工程とする。

**個別Court slashの実回収額は別会計。** 対象offenceから実際に回収したMISAKAをSとし、Panel/Gate C合計の報酬を`E=floor(490S/1000)`、残り`T=S-E`をburnとする（名目49%/51%、端数はburn）。49%は両役割の合計であり、それぞれ49%ではない。不正発見・立証の資格を認証したplanで一回配分し、不正本人の自己slash受領と同一offenceの二重報酬を禁止する。BFTモデル廃止の100%burn資産をSへ加えず、BFTの没収受取・廃止bountyは0のまま。検証bondがBFT一律burnから保護されることは、本人のCourt個別slash免除ではない。
<!-- BMR1-ADOPTED-20261126-END -->

<!-- MOE3-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — 全モデル統一・小数POSITION・50/20/20/5/5

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0068](../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](../adr/0236-fractional-position-miner-rewards.md)／[MOE3適用・移行](fractional-position-economics-transition.md)を新経済の正本とする。ユーザーの最新指定は、対象coinbase原資Cの**50%をMiner用POSITION購入、20%をLP用POSITION購入、20%をMISAKAのLP、5%をPanel/Gate CへMISAKA、5%をBFTへMISAKA**である。Minerと検証者の受取資産を分け、5%枠内のPanel/Gate C配分は認証済みplanに従う。新しい半々配分等を暗黙設定しない。

整数分割は`F=floor(50C/1000)`、`P=floor(50C/1000)`、`A=floor(200C/1000)`、`J=floor(200C/1000)`、`D=C-F-P-A-J`、`B=D+J`。FがBFT、Pが検証者、AがLP用MISAKA、JがLP用POSITION購入原価、DがMiner用購入原価であり、**通貨の全丸め残差はD**へ置く。B（名目70%）を一括購入し、購入qから`ell=floor(q*J/B)`をLP、`m=q-ell`をMinerへ配る（B>0）。単一モデルにC全額を割り当て、かつCが20の倍数の例ならell=floor(2q/7)で名目2/7 LP・5/7 Minerとなるが、端数時も無条件に2q/7としない。`C=B+A+P+F`、`q=ell+m`を保つ。POSITIONの50%/20%相当は取得原価の按分であり、換金額の保証ではない。

全モデルを係数1.0・各モデルの一意市場・MAD2管理へ統一する。新fork特別区分、追加モデル専用κ、coreの市場/報酬免除、genesisのBFT管理免除を継承しない。originは履歴として保存し、genesisの差は正規creatorがいない点だけとする。POSITIONは小数8桁、`1 POSITION=100,000,000 atoms`として固定精度整数で計算する。

通常AMMはburn5%を維持し、creatorありの総手数料1%/名目net94%、正規genesisのcreatorなしは0%/名目net95%とする。不存在creatorの1%を徴収・追加burnしない。内部購入・LP・配布の追加AMM feeは0。通常の永久LP撤去禁止・claim検証・共通発行/資本/責任capを維持し、BMR1正規廃止の限定burnは後続規範に従う。

同日途中の「80%購入・q/4 LP・60%相当Miner」「Wから可変Panel枠を引いてRへ」「BFT10%」は**今回の固定50/20/20/5/5に置換済み**であり、新規受理に使わない。旧MOE1/MOE2のwhole数量・30/20/50・owner3%・旧claim/wire/native/過去試験は元profileの記録として保持する。MOE3のnative接続・移行実行・本番有効化の完了ではない。

一般/複数モデルでは、RFC0068 §4.3で一度分割したCの各legを結合按分し、各モデルの実予算比`floor(q_i J_i/B_i)`でLP数量を求める。Cが20で割り切れるだけでは各モデルの比率が2/7になるとは限らない。
<!-- MOE3-ADOPTED-20261126-END -->

<!-- MAD2-ADOPTED-20261123-BEGIN -->
## 2026-11-23 後続採択 — 二経路モデル採用 MAD2

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)／[ADR0235](../adr/0235-model-admission-kernel-auto-and-bft-decision.md)／[適用・移行](model-admission-dual-route-transition.md)を、MAD2対象のモデル採用について優先する。KERNEL_AUTOは対応kernelへの自動分解と採択済み審査、BFT_DECISIONは証明提出を要求しない所定quorumのYES/NOである。

G-COLLの定理・G-COST/G-USEの証拠が未完成でも、共通の技術互換とq YESがあればBFT承認を記録できる。元のUNKNOWN/FAILを数学的PASSへ改変しない。未知kernel、壊れた実行仕様、個別claimの不正はYESで免除しない。Panel/Court、Final、独立Gate C、Ticket、一回会計、TB1、MOE2は維持する。

以下の旧「UNKNOWNは一律採掘不可」「BFTは停止専用」「全モデルに定理・費用審査を必須」とする条項は、旧profileの履歴及びMAD2のKERNEL_AUTO側として読む。BFT_DECISIONへ再適用しない。実投票・native実装・公開activationの完了を意味せず、既存QF1のREVIEW_HOLDを文書だけで解除しない。

**モデル管理の範囲。** 元のGENESIS_ORIGINALはBFTの採用・変更・停止・解除の対象外とする。ADDITIONALは自動/BFTの採用経路にかかわらずBFT管理対象。BFT YESでFORK_DESIGNATEDを追加でき、genesis同様のOptionなし・Miner=Rを適用するが、BFT停止・正規解除の対象である。停止免除を後付けGENESIS指定やaliasから作らず、個別claimの検証は全分類で維持する。

<!-- MAD2-ADOPTED-20261123-END -->


2026-11-21改訂。新規採掘のG-COSTには、現世代の固定条件での反復費用検査を適用する。未検査はUNKNOWN、再現された安価な正規TicketはFAILとし、合格の範囲と未検査条件を記録する。[RFC-0062 §4.3](../rfc/0062-palw-function-output-and-work-admissibility.md#g-cost-review)


- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 正本: [RFC0066](../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../adr/0234-qwen3-fow1-execution-and-readout.md)

## 適用先

QF1-Q3-8B-A16-8K-v1を明示する新しいdescriptorと試行だけに適用する。Qwen3-8Bの名前だけで一般登録・既存claim・genesis/forkモデルを切り替えない。旧claimは受付時のprofileと責任でdrainし、新しいReadout・入力・Ticketで再計算して権利を増やさない。

## 優先順位

QF1の数値・shape・Readout・codecはRFC0066が所有する。MAD2の採用basisを利用し、AUTOの数学・反復検査条件とBFTの採否判断を分ける。TB1、全実行checker、Panel/Court、Gate CとMOE2は維持する。

限定置換は、AN1/RFC0062の35-token byte frameをQF1用HEX64 chat frameに変える点と、未指定の具体Readout/数値/codecを固定する点である。FOW1全体の入力adapterを一括変更するものではない。旧native wire番号やHash64の意味を置換しない。

## 開始条件

instanceのartifact・TIR・scale・tokenizer・StateMap、全有界checkerと資源条件を固定する。その技術互換に加え、AUTOの現行合格又はBFTの正規YES、停止なし、native実装・activationを必要とする。公式重みの名称だけでいずれかの承認を推定しない。

QF1の受入試験では、Minerへの追加質問なしで、正常なclaimの完全再実行、不一致の直接反証、必要資料が欠けたまま元期限に達した場合の未検証終了を確認する。対話による局所化を提供するかは任意で、実装はfenceの開始条件に含めない。[RFC-0062 §3.1](../rfc/0062-palw-function-output-and-work-admissibility.md#fow1-panel-dialogue)

非報酬評価はMAS1のMODEL_AUDITと後続独立PANELを使う。一般登録と追加モデルの市場原子生成はそのままで、評価からcredit/weight/受賞権利/報酬を発行しない。後日合格しても過去の評価に遡って報酬を付与しない。

## 現在の到達点

Qwen3-8Bの固定整数実行について、隔離3ピアで通常のPanel承認、Endの直接裁定、資料欠落時の承認不足と元期限を確認した。QF1-10の一貫検証、G-COLLの適合確認、変換後の実用性、公開有効化は未完了である。[RFC-0066 §12](../rfc/0066-qwen3-fow1-execution-and-readout.md#12-受入試験と完成判定)

G-COSTの予備検査では、署名済み6試行を3組に分けて共通prefixの計算を共有した。各組で独立実行と同じTicketを再現し、合計の経過時間が5.87%短くなった。[共通prefixの費用比較](../evidence/palw/mas1-qf1-native-2026-11-21/g-cost-shared-prefix.json)

同じ仕事で停止位置を予測した試験では、独立した5試行すべてで全traceを1回だけ生成して元のTicketを再現できた。基準の2回実行に比べて対象5件の合計時間は47.99%短かった。予測を1 token短くした対照には再実行の費用もかかった。公開採掘の評価では、初回Ticketの生成費用にこの経路を含め、異なる仕事での予測失敗と再試行も計上する。費用比の閾値と測定誤差の扱いは事前に固定する。現世代は `UNKNOWN` で新規採掘を保留する。[停止位置の費用比較](../evidence/palw/mas1-qf1-native-2026-11-21/g-cost-predicted-length.json)

抽選前に固定した異なる3問では、同じ長さ9の予測は全て外れた。再試行を含む合計は368.101秒、基準経路は244.377秒。各問1回の観測として記録し、以前の同一仕事の費用短縮も回帰対象に含める。[仕事別の費用](../evidence/palw/mas1-qf1-native-2026-11-21/g-cost-varied-jobs.json)
