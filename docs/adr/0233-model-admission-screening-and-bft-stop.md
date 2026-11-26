# ADR-0233 — モデル追加時の構造・実用性検査と限定BFT停止を採択する

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](../governance/model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

正規BFTのモデル恒久廃止で、当該永久LP内actual MISAKA・登録者責任bond・MINING用途bond・未確定mining報酬を100%burn/失効させる。**全Panel/Gate C検証用途bondは、不正者も含め一律にBFT廃止burnから保護**し、個人不正はCourtの個別slashへ分離する。正当に成立した検証報酬の支払を維持し、他モデル資産へ影響させない。兼任は用途別lot（MINING6,000だけburn、PANEL4,000は保護）とし、重複担保・責任中の用途変更は禁止する。

通常のLP撤去禁止を維持し、canonical廃止のactual reserve burnだけを限定例外にする。BFTの没収受取・廃止bountyは0。継続5%報酬を没収額に連動させない。可逆Pause/Resumeと区別し、旧停止専用・LP絶対不処分の記述をこのscopeだけ置換する。旧profile/native/履歴を保持し、BMR1実装・移行・有効化は別工程とする。

**個別Court slashの実回収額は別会計。** 対象offenceから実際に回収したMISAKAをSとし、Panel/Gate C合計の報酬を`E=floor(490S/1000)`、残り`T=S-E`をburnとする（名目49%/51%、端数はburn）。49%は両役割の合計であり、それぞれ49%ではない。不正発見・立証の資格を認証したplanで一回配分し、不正本人の自己slash受領と同一offenceの二重報酬を禁止する。BFTモデル廃止の100%burn資産をSへ加えず、BFTの没収受取・廃止bountyは0のまま。検証bondがBFT一律burnから保護されることは、本人のCourt個別slash免除ではない。
<!-- BMR1-ADOPTED-20261126-END -->

<!-- MOE3-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — 全モデル統一・小数POSITION・50/20/20/5/5

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0068](../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](0236-fractional-position-miner-rewards.md)／[MOE3適用・移行](../governance/fractional-position-economics-transition.md)を新経済の正本とする。ユーザーの最新指定は、対象coinbase原資Cの**50%をMiner用POSITION購入、20%をLP用POSITION購入、20%をMISAKAのLP、5%をPanel/Gate CへMISAKA、5%をBFTへMISAKA**である。Minerと検証者の受取資産を分け、5%枠内のPanel/Gate C配分は認証済みplanに従う。新しい半々配分等を暗黙設定しない。

整数分割は`F=floor(50C/1000)`、`P=floor(50C/1000)`、`A=floor(200C/1000)`、`J=floor(200C/1000)`、`D=C-F-P-A-J`、`B=D+J`。FがBFT、Pが検証者、AがLP用MISAKA、JがLP用POSITION購入原価、DがMiner用購入原価であり、**通貨の全丸め残差はD**へ置く。B（名目70%）を一括購入し、購入qから`ell=floor(q*J/B)`をLP、`m=q-ell`をMinerへ配る（B>0）。単一モデルにC全額を割り当て、かつCが20の倍数の例ならell=floor(2q/7)で名目2/7 LP・5/7 Minerとなるが、端数時も無条件に2q/7としない。`C=B+A+P+F`、`q=ell+m`を保つ。POSITIONの50%/20%相当は取得原価の按分であり、換金額の保証ではない。

全モデルを係数1.0・各モデルの一意市場・MAD2管理へ統一する。新fork特別区分、追加モデル専用κ、coreの市場/報酬免除、genesisのBFT管理免除を継承しない。originは履歴として保存し、genesisの差は正規creatorがいない点だけとする。POSITIONは小数8桁、`1 POSITION=100,000,000 atoms`として固定精度整数で計算する。

通常AMMはburn5%を維持し、creatorありの総手数料1%/名目net94%、正規genesisのcreatorなしは0%/名目net95%とする。不存在creatorの1%を徴収・追加burnしない。内部購入・LP・配布の追加AMM feeは0。通常の永久LP撤去禁止・claim検証・共通発行/資本/責任capを維持し、BMR1正規廃止の限定burnは後続規範に従う。

同日途中の「80%購入・q/4 LP・60%相当Miner」「Wから可変Panel枠を引いてRへ」「BFT10%」は**今回の固定50/20/20/5/5に置換済み**であり、新規受理に使わない。旧MOE1/MOE2のwhole数量・30/20/50・owner3%・旧claim/wire/native/過去試験は元profileの記録として保持する。MOE3のnative接続・移行実行・本番有効化の完了ではない。

一般/複数モデルでは、RFC0068 §4.3で一度分割したCの各legを結合按分し、各モデルの実予算比`floor(q_i J_i/B_i)`でLP数量を求める。Cが20で割り切れるだけでは各モデルの比率が2/7になるとは限らない。
<!-- MOE3-ADOPTED-20261126-END -->

<!-- MAD2-ADOPTED-20261123-BEGIN -->
## 2026-11-23 後続採択 — 二経路モデル採用 MAD2

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0067](../rfc/0067-model-admission-kernel-auto-and-bft-decision.md)／[ADR0235](0235-model-admission-kernel-auto-and-bft-decision.md)／[適用・移行](../governance/model-admission-dual-route-transition.md)を、MAD2対象のモデル採用について優先する。KERNEL_AUTOは対応kernelへの自動分解と採択済み審査、BFT_DECISIONは証明提出を要求しない所定quorumのYES/NOである。

G-COLLの定理・G-COST/G-USEの証拠が未完成でも、共通の技術互換とq YESがあればBFT承認を記録できる。元のUNKNOWN/FAILを数学的PASSへ改変しない。未知kernel、壊れた実行仕様、個別claimの不正はYESで免除しない。Panel/Court、Final、独立Gate C、Ticket、一回会計、TB1、MOE2は維持する。

以下の旧「UNKNOWNは一律採掘不可」「BFTは停止専用」「全モデルに定理・費用審査を必須」とする条項は、旧profileの履歴及びMAD2のKERNEL_AUTO側として読む。BFT_DECISIONへ再適用しない。実投票・native実装・公開activationの完了を意味せず、既存QF1のREVIEW_HOLDを文書だけで解除しない。

**モデル管理の範囲。** 元のGENESIS_ORIGINALはBFTの採用・変更・停止・解除の対象外とする。ADDITIONALは自動/BFTの採用経路にかかわらずBFT管理対象。BFT YESでFORK_DESIGNATEDを追加でき、genesis同様のOptionなし・Miner=Rを適用するが、BFT停止・正規解除の対象である。停止免除を後付けGENESIS指定やaliasから作らず、個別claimの検証は全分類で維持する。

<!-- MAD2-ADOPTED-20261123-END -->


<!-- QF1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — QF1のQwen3具体profile

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0066](../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](0234-qwen3-fow1-execution-and-readout.md)／[適用・移行](../governance/qwen-fow1-profile-transition.md)。Qwenの具体例QF1でも、G-USE・構造解析・反復費用検査・現行停止状態の全条件を維持する。

QF1は36層Qwen3-8Bの整数A16実行、各予測位置の37 vector、HEX64実入力、固定Readout/四Root/Ticketを定義する。G-COLL/G-COST/G-USE、MAS1のUNKNOWN・BFT安全停止、Panel＋Courtを維持する。Qwen実機の品質・費用検査・native有効化は未完であり、旧claimやMOE2経済を変更しない。
<!-- QF1-ADOPTED-20261120-END -->


- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 対応RFC: [RFC-0065 MAS1](../rfc/0065-model-admission-screening-and-bft-stop.md)
- 適用・移行: [model-admission-safety-transition](../governance/model-admission-safety-transition.md)
- 根拠: 今回のユーザーによる「安く推測できるモデルを追加時に弾き、自動で弾けない場合はBFTで停止する経路へつなぐ」という明示指示。

## 背景

FOW1はG-EXEC・G-COLL・G-COSTを要求するが、新規採掘のG-COSTは、対象となる重みと規則を固定して複数の入力で費用を比較する。計算graphの形、zの長さ、モデルの評判又は有限testだけで採掘資格を付与すると、定数化、cache、軽いReadout、採掘専用の無用な重い枝、未知の短絡を使うモデルが費用認定をすり抜けるおそれがある。

ユーザーは、実用性を保ったモデルを対象に、登録段階のkernel/graph解析と、不明時のBFT停止を追加するよう指示した。旧「モデル単位の検査をしない」「品質審査をしない」「BFTを使わない」という包括的な記述は、今回採択する限定用途について修正する必要がある。

## 判断

1. **一般登録と採掘資格を分ける。** artifact登録はpermissionlessとし、追加登録時のMOE2市場原子生成を維持する。市場・bond・モデル名から採掘資格を発生させない。
2. **MAS1の自動検査を採掘入口へ追加する。** lowering、kernel、入力、Readout、useful state、trace、cache、MoE、既知の短絡を対象に、PASS/FAIL/UNKNOWNを区別する。未対応や未認証の全weight属性をPASSにしない。
3. **実用性条件G-USEを加える。** 公開・共通・版固定のsuiteと客観的scoreを用いる。軽量Readoutと検証負担を分離し、回答に不要な巨大演算の費用をLLMの有用仕事として認定しない。有限testの合格はG-COSTや全入力の品質の証明ではない。
4. **非報酬評価を分離する。** 未適格モデルの検査はNON_REWARDING_MODEL_EVALUATIONで行い、採掘・credit・weight・受賞権利・MOE2買付を生まない。独立したTB1-MODEL_AUDITと、提出後のTB1-PANEL、既存の完全再実行・局所Courtへ接続する。
5. **不明な新規モデルは閉じたままBFT reviewへ送る。** UNKNOWNはREVIEW_HOLD、期限満了は未適格終端とする。BFTが止まっても仮承認しない。既PASSEDモデルへの未認証の通報一件は永久vetoにしない。
6. **限定BFT停止を導入する。** 事前確定したTB1 holder rosterに別domainのMODEL_STOP権限を付与し、n=3f+1、q=2f+1の署名でartifact/profile又はTheoremRule versionを停止する。claim Panelの一人正直という仮定をBFTの故障仮定へ転用しない。
7. **BFTへ台帳決定権を与えない。** certificateは既存native順序で処理する停止controlである。fork choice、reorg拒否、block finality、claimの算術真偽、既確定残高の改変を権限に含めない。DNS-BFTを復活させない。
8. **解除は全適格性の再確認を必要とする。** artifact/profileの解除にはfreshな全PASSと、現在rosterのq署名による対象StopId集合の解除を必要とする。G-COST不明を多数決でPASSへ変えない。burn Resume、owner変更、core指定、旧解除証明は安全停止を消さない。TheoremRule versionの停止は個別モデルの解除で消さず、別途採択した新rule versionと全適合再検査を必要とする。
9. **全成功consumerへ接続する。** 同batchでは安全停止を新しい成功・発行・受賞権利消費より先に反映する。停止中も元期限・Court・資料・返還を維持する。既確定の効果はBFTだけで遡及没収しない。
10. **未知の安価算法を完全に発見したとはしない。** 検出coverage、未確認の費用条件、誤停止、停止までの遅延、BFTとnativeの活性前提を公表する。正しい高速化そのものをslashしない。

## 旧規範の置換範囲

| 対象 | 今回の限定改定 |
| --- | --- |
| RFC0062 §0/§2/§4/§7/§8、ADR0230 | FOW1の三gateを維持してMAS1検査・G-USE・停止状態を追加。数学的費用保証の欠如はBFTで補完しない |
| RFC-v2-0002/0003/0005/0006/0007/0010/0011/0016〜0020 | kernel、登録、評価、control、Final、権利消費・会計・資料・reorgの担当規範へ接続 |
| ADR0180、PRINCIPLESのBFT・モデル無審査条項 | 台帳BFTは禁止を維持。モデル安全停止と共通実用性条件だけを例外として採択 |
| RFC0053、ADR0221 | burn停止の独立性を維持。burnだけでMAS1停止を解除しない |
| RFC0061/ADR0229、TB1用途表 | MODEL_AUDITを独立purposeとして追加。MODEL_STOPは署名権限であり乱数sourceではない |
| MOE2、追加モデル市場・core配分 | 率・原子市場生成・永久LP・通常市場取引を変更しない。採掘停止を市場未準備にしない |

MAD2では経済上のcoreとBFT停止権限を分離する。元genesisはBFT採用・変更・停止・解除の対象外であり、forkモデルはOption対象外でもBFT停止対象である。全分類の個別claim検証は維持する。既存旧profileのcore/追加modelへこの文書を遡及適用しない。

## 検討した代替案

- 全ての未知モデルをBFT承認だけで許す案は不採用。必要な反復費用検査を署名で省くため。
- benchmark合格又は非衝突testだけで採掘可能とする案は不採用。過学習・短絡・共有・安い入力選別を排除できないため。
- 採掘専用の大きなhash/行列演算、Groth16の証明費用を仕事根拠にする案は不採用。今回の「実用的なLLM推論を使う」という目的を満たさないため。
- 単独の未検証通報で既PASSEDモデルを停止する案は不採用。許可された通報の大量投入が無期限vetoになるため。
- BFTが過去の正しい計算や保有資産を没収する案は不採用。危険管理と計算不正を区別できなくなるため。

## レビューと残る条件

本変更で示す抽象性質は、UNKNOWN/FAILから成功しない状態不変条件、q署名に必要な正直署名数、理由別停止とstale解除拒否である。一般のモデル解析の完全性やQwenのG-COSTを証明したものではない。

今回の文書レビューでは、モデル非保有Nodeに不可能な全weightの推定を要求しないこと、BFTによる仮承認を置かないこと、低rank等を自動的な詐欺扱いにしないこと、非報酬評価の循環を避けること、停止対象とnative順序、burn/owner/alias/reorgによる迂回を確認する。独立外部レビュー・実装・実機試験を実施済みとはしない。

停止集合に少なくともf+1人の正直者が含まれることは、危険を必ず正しく発見するという保証ではない。既PASSEDモデルの未知攻撃には停止までの窓があり、false positiveは可用性を損なう。権限を与える実roster、fault budget、control包含、suite・threshold、bounded checker、FOW1定理、旧pending drainがそろうまで公開有効化しない。

## 実装・有効化

新しい型は論理設計である。公開設定・配備・支払は変更しない。新しいruntime flag、wire ID、BFT署名者、品質閾値を今回の文書で本番値として割り当てない。一般登録と市場生成、採掘資格、非報酬評価、risk control、消費・Finalの全入口をnative実装で接続し、旧履歴を保持して試験した後に明示activationを行う。

保存原本・変更一覧・文書検査結果は[改訂記録](../maintenance/revisions/2026-11-20-model-admission-safety/README.md)に保存する。以前のテスト成功や今回のリンク検査を本番の安全性・実装完成に読み替えない。
