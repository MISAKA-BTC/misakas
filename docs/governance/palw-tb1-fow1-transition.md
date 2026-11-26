# PALW-TB1 / FOW1 — 関数出力・乱数・仕事会計の統一と移行

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


burn-pauseも全モデル共通とし、Pauseには正規支払者署名・規定停止burnを要する。Resumeはnative登録者正規認証OR専用BFT BurnResume承認に加えて、どちらも規定解除burnを必要とする。登録者不在genesisにも同じBFT経路を使い、通常採用YESを流用しない。旧確定PAUSEDのsequence/前操作を保持し、旧pendingは元policyでdrainする。独立BFT safety stop・原期限・RETIREDはBurnResumeで解除しない。具体的burn額を今回ゼロや任意額に変更しない（RFC0068 §2.4）。

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


<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../adr/0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](qwen-fow1-profile-transition.md)。QF1の具体profileと限定adapter置換を追加する。TB1の用途や停止・freeze前提は変更しない。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用検査・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](../rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。FOW1三gateにMAS1を加え、非報酬評価には独立TB1-MODEL_AUDITを用いる。既知のEXEC/PANEL値から未知の評価seedを作らない。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未検査の費用経路の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 採択根拠: ユーザーが監査の五項目の修正と、添付PALW-TB1への乱数生成変更を明示的に指示した。
- 乱数の正本: [RFC-0061](../rfc/0061-palw-internal-threshold-beacon.md) / [ADR-0229](../adr/0229-palw-internal-threshold-beacon.md)。
- 関数・登録・Ticketの正本: [RFC-0062](../rfc/0062-palw-function-output-and-work-admissibility.md) / [ADR-0230](../adr/0230-palw-function-output-and-work-admissibility.md)。
- 全文書の適用台帳: [BEACON_AND_POLICY_COVERAGE](../maintenance/revisions/2026-11-20-tb1-fow1/BEACON_AND_POLICY_COVERAGE.md)。

## 1. 適用対象と優先関係

本改定は、採用する新PALW採掘・仕事報酬・合意資格の共通契約を**PALW-FOW1＋PALW-TB1**へ統一する。FOW1の採掘値は、登録されたコア推論関数の出力zである。MOR1の認証envelopeの識別値だけが異なることを、zの非衝突性や追加計算の費用保証へ読み替えない。

新profileの実装・有効化に関して、RFC0061/0062、本移行契約、[公開裁定v0.5](../design/palw/consensus/public-adjudication-policy-v0.5.md)を優先する。RFC0059/0060/ADR0227/0228の本日改訂はこの契約へ接続する。既存v2各冊に追記した担当規範と、全旧pathの適用台帳が参照先を一意にする。

旧RFC/ADR、統合版の「原条文の統合収録」「原条文の完全収録」、旧snapshot、過去の測定・レビューの原文は履歴として保持する。これらにある旧beacon、未来anchor hash、seal-reveal、BFT/BLS、free-prompt、quantum、出力envelopeの条文を、FOW1/TB1の新規受付条件として選択してはならない。原文の存在や古いAccepted表示は、新profileへの再採用を意味しない。

本改定は設計方針と規範の採択であり、native実装・暗号合成評価・Qwen用の反復費用検査・公開activationの完了ではない。公開activationの具体的なnetwork、fence、wire ID、suite parameter、TheoremRuleIdを未承認の既定値で補完しない。

## 2. 保証対象を固定する

| 対象 | 新契約 | 代用を認めないもの |
| --- | --- | --- |
| 同一試行の再現性 | 同じ登録context、TB1値、model/profile、数値意味で同じy・z・trace | 実行のたびに異なる値を返す秘密状態 |
| コア推論の採掘値z | 登録された実行グラフ・出力関係に属し、対象系列の非衝突定理の適用条件を満たす | s、nonce、H(s)、外付けreceiptだけをzと改名すること |
| 自然回答y | 同じ登録実行から得る利用者向け回答。yだけの一致で成功しない | yの多様性テストを数学的非衝突性にすること |
| 補助receipt rho | 文脈・Root・zの認証補助 | rhoのhash衝突上界をコア関数・費用の定理にすること |
| 新しい採掘機会 | RFC0062の初回Ticket照会前の仕事と、複数試行に対する費用条件 | 正しい結果だから通常推論費用を負担したとの推定 |
| 正当性 | 全必須関係のPanel再実行とモデル非保有Nodeの局所Court | 局所証拠、署名数、TB1 share certificateだけによるLLM全体承認 |

モデルの保存登録とFOW1採掘適格性を分ける。全permissionless候補へ同じ構造条件を適用し、モデル名、人気、登録者の署名、実測だけを適格性の根拠にしない。証明済みの対象系列と有界な適合検査が未確立なモデルはFOW1採掘を開始できない。現在のQwenに完成済み定理があるとは認定しない。

## 3. 乱数源をTB1へ一本化する

各consumerの型・purpose・固定点は[RFC0061](../rfc/0061-palw-internal-threshold-beacon.md)と[機械可読用途表](../design/palw/consensus/tb1-beacon-consumers.json)に従う。

| 用途 | 新profileの扱い | 旧記述の扱い |
| --- | --- | --- |
| 実入力challenge EXEC | 独立TB1-EXEC。全C/nonce候補・target等を最初の正直share前に安定固定 | 既存work-derived値・block hash・旧seal-revealを新受付へ流用しない |
| Panel選出 PANEL | 独立TB1-PANEL。元Original、pool、DrawPolicy、責任を別に固定 | EXECのhashし直し、固定前に既知のseed、旧委員会値へfallbackしない |
| Court positive / spot challenge | 固定済み元claimと正規TB1-PANELから目的別に決定論的導出 | 新しい未来乱数を作ったとは扱わず、本人の全再実行を証明しない |
| negative・資料case・Court継続 | 新beaconを待たず、元期限・元予約で処理 | 再抽選、再登録、追加D、TB1停止による救済停止を禁止 |
| AN1/FOW1の受付順 | RFC0062のTicketと固定slotによる一意な順序 | 旧ADMISSION抽選を追加して二回抽選にしない |
| 旧preseal順位を持つ別control | 採用する場合は独立TB1-ADMISSIONと専用freezeを必須にする | 旧seed経路を新controlへ暗黙継承しない。未確定ならその経路を無効にする |
| EXEC scheduleの割当 | 独立TB1-SCHEDULE。候補credit、規則、期間を放出前に固定 | future block hashや現在のEXEC値のhashを未来未知性の代用にしない |
| 次holder roster | RFC0061の独立TB1-ROLEと前epochの固定集合 | 自己の未成立roundで自分のrosterを選ばない。bootstrapは別の明示gate |
| 生成sampling | 同じEXEC contextから登録PRNG・消費順で決定論的に導出 | Panel選出用の未来entropyとは扱わない |
| 第三のTicket乱数 | FOW1には設けない | SWC1のR_ticketは旧案の履歴。新Ticketに重ねない |
| 旧モデル審査・延期model improvement | FOW1の定理適合検査を確率的モデル審査で代用しない。延期機能は無効を維持 | 古いbeaconを根拠に廃止・延期経路を復活させない |

TB1は完全BGW VSS、PQ認証私信、署名付きshareの公開復元certificateを使う。`n=3f+1`、秘密復元`t=f+1`、公開certificate`q=2f+1`を区別する。保持者は台帳finality、fork choice、reorg、claim Finalへ投票する新BFT validatorではない。LLM検証用のGroth16も導入しない。

実seedは32byte、context digestは64byteの別型とし、旧64byte beaconの暗黙切詰め、署名bytesや支持者subsetによる値の変更を認めない。固定Qの全dealerを含め、放出しないdealerを後から除外しない。

予測不能性の境界は最初の正直share放出であり、certificate完成や公式公表の時刻ではない。署名片の先行集約、鍵・round・purpose・targetの選び直し、公開拒否、fork上の曝露を扱う。公開済み秘密をDBのundoで未公開に戻さず、別対象へ再割当しない。

## 4. 仕事・再利用・会計の接続

FOW1のTicketは、規定context、モデルと実入力、コア採掘値z、回答とcanonical traceのRootへ一意に束縛する。自由salt、署名乱数、Panel nonce、receipt再包装、後続beaconから別Ticketを作らない。同一の試行・実仕事・用途の再送は新しい報酬を生まない。

保存・配信・認証済み重みの共有と、試行間の演算の償却を区別する。正しいcache、batch、最適化、外注・共同計算は反復費用検査の対象に含める。禁止宣言や本人の申告で計算費用を確保したとしない。正しい計算を安く行ったこと自体へslashを課さない。

旧prefix exportは生成元と責任を認証する契約として維持できるが、その再利用を無料の新FOW1仕事へ変換しない。reuse可能な部分、費用に算入する部分、BaseWorkKey・AttemptUse・creditの消費をRFC0062の同じprofileへ結合する。

モデル二階層1.0/0.3、既存burn-pause/resume、資本上限、全carrier合計1 BPS目標、既受理Court容量、元期限を別の数学的費用保証に読み替えない。

## 5. 旧条項の置換表

| 旧規範・入口 | FOW1/TB1で適用する置換 | 保持する互換範囲 |
| --- | --- | --- |
| RFC59 §2–5、ADR227 | RFC61のTB1 RoundRef/Freeze、RFC62の実入力・z・Ticket・適格性 | 受理済み旧AN1 context、旧署名の意味 |
| RFC60 §1/§4/§8、ADR228 | rhoの定理は補助receiptだけ。コアzと費用はRFC62の別gate | 旧MOR1のy,s,rhoとその定理の限定 |
| RFC0001/0003、ADR0044のfree-prompt F1 | 採掘経路では試行challengeを実入力へ束縛し、登録されたコア関数を評価 | 旧claim、採掘を伴わない旧サービス契約。新採掘への抜け道にしない |
| ADR0074等の量子化された複数draw・旧receipt lottery | RFC62の一試行一Ticketと明示した仕事単位・会計 | 旧版で受理された仕事の消費・権利・負債 |
| RFC0055/0056/ADR0223/0224の旧受付seed | FOW1では追加抽選なし。別controlのseedはTB1-ADMISSION | 旧受付の履歴・原予約 |
| RFC0057/ADR0225 | 正規TB1-PANEL由来の担当別nonce、元claim binding | 旧nonce型・旧署名・旧case終結 |
| RFC0058/ADR0226のBFT/BLS候補・R_ticket | TB1-BGW-PQ、独立EXEC/PANEL。第三Ticket乱数なし | 比較案・DISABLED・未採択の履歴 |
| ADR0185、RFC0007/0010、ADR-v2-0032の旧source | 新profileはTB1だけ。bootstrap未完を旧sourceで補わない | 旧profileの確定済みsource・未終結責任 |
| RFC-v2-0006/0017/0020の抽選 | TB1-PANELと固定DrawPolicy。PublicOneHonest条件を維持 | 旧sampled、多数承認の版固有規則 |
| RFC-v2-0010・PRINCIPLES §14の後続seed | TB1-SCHEDULEとcredit集合固定 | 既発行permit、二重署名防止journal、旧schedule |

## 6. 移行と失敗

1. **現在:** 設計採択のみ。現行ネットワークの受理・報酬をこの文書更新だけで変更しない。
2. **準備:** 有効化するnetwork/profile、TB1全suite/codec、各purpose、安定freeze、holder bootstrap、資源予約、共通実行checker、MAD2の採用basis（AUTOはTheoremRule/反復suite、BFTは正規YES）、全consumer、原子的会計を固定する。
3. **適格性の確認:** 未完成の定理や未対応kernelを仮のPASSにしない。現在Qwenに対する必要な定理・適合検査が未完成なら、そのFOW1入口を開けない。
4. **明示したactivation境界:** 新規PALW採掘・報酬・合意workはFOW1/TB1を必須とする。旧Attempt、FreePrompt、AN1/MOR1、SWC1、別API、import/repair経路を新規採掘fallbackとして受理しない。
5. **旧責任のdrain:** 境界より前の有効なclaim/caseは受理時の署名・型・期限・予約で終える。既払い権利、旧signature、資料保持、再起動・reorg責任を新形式へ黙って変更しない。旧claimから新FOW1 creditを二重発行しない。
6. **TB1未成立:** 未成功終端へ進める。旧beacon、block hash、定数、別round、別suiteへ自動fallbackしない。既受理のnegative・資料case・Courtを継続する。
7. **再編・曝露:** 露出したroundの記録は不可逆な情報漏洩を表す。非canonicalとなったfreezeへ別仕事を付け直さず、元権利・予約のundoと新規の明示回復を区別する。

## 7. 完成判定

文書検査、参照fixture、有限体の小規模算術、native実装、暗号合成評価、実LLM測定、対象instanceでの反復費用検査、公開activationを別々に記録する。添付のTB1参照モデルは完全BGW、PQ暗号、native合意を実装していない。新purposeのSCHEDULE/ROLE/ADMISSIONやFOW1のコア関数定理を、元の29テストの成功で検証済みにしない。

全条件を満たす具体profileが存在し、必要なレビュー・測定・明示的activation記録がそろうまで、新profileを本番対応済みと表示しない。これは旧beaconに戻る許可ではない。
