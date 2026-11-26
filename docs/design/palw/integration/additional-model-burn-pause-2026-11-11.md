# 追加モデルのburn停止・解除 — 実装追補

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](../../../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](../../../adr/0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](../../../governance/model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

正規BFTのモデル恒久廃止で、当該永久LP内actual MISAKA・登録者責任bond・MINING用途bond・未確定mining報酬を100%burn/失効させる。**全Panel/Gate C検証用途bondは、不正者も含め一律にBFT廃止burnから保護**し、個人不正はCourtの個別slashへ分離する。正当に成立した検証報酬の支払を維持し、他モデル資産へ影響させない。兼任は用途別lot（MINING6,000だけburn、PANEL4,000は保護）とし、重複担保・責任中の用途変更は禁止する。

通常のLP撤去禁止を維持し、canonical廃止のactual reserve burnだけを限定例外にする。BFTの没収受取・廃止bountyは0。継続5%報酬を没収額に連動させない。可逆Pause/Resumeと区別し、旧停止専用・LP絶対不処分の記述をこのscopeだけ置換する。旧profile/native/履歴を保持し、BMR1実装・移行・有効化は別工程とする。

**個別Court slashの実回収額は別会計。** 対象offenceから実際に回収したMISAKAをSとし、Panel/Gate C合計の報酬を`E=floor(490S/1000)`、残り`T=S-E`をburnとする（名目49%/51%、端数はburn）。49%は両役割の合計であり、それぞれ49%ではない。不正発見・立証の資格を認証したplanで一回配分し、不正本人の自己slash受領と同一offenceの二重報酬を禁止する。BFTモデル廃止の100%burn資産をSへ加えず、BFTの没収受取・廃止bountyは0のまま。検証bondがBFT一律burnから保護されることは、本人のCourt個別slash免除ではない。
<!-- BMR1-ADOPTED-20261126-END -->

<!-- MOE3-BURN-PAUSE-20261126-BEGIN -->
## 2026-11-26 後続採択 — 全モデル共通のburn停止・解除

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0068 §2.4](../../../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](../../../adr/0236-fractional-position-miner-rewards.md)／[MOE3移行](../../../governance/fractional-position-economics-transition.md)を優先する。MOE3の全モデル統一に伴い、genesis・旧forkのburn-pause対象外を廃止する。誰でも正規支払者署名・期待ACTIVE/sequence/直前操作照合・規定停止burnによりPauseできる。

ResumeはPAUSEDから、**native登録者の正規認証 OR 共通BFTの専用BurnResume承認**と、どちらの経路でも規定解除burnの実支払を必要とする。BFT経路は登録者の有無を問わず全モデル共通とし、creatorなしgenesisが解除不能になることを避ける。fee受取人・contributor・POSITION holderを登録者の代わりにしない。

専用承認はnetwork/genesis・MOE3/pause policy・model/root・期待PAUSED/sequence・直前canonical操作・roster/epoch・purpose・期限・解除burn額/上限を束縛する。通常採用YESや安全停止解除票の流用、支払者署名/実支払の省略は認めない。署名・burn・状態更新を一つのnative遷移で確定する。具体的なburn額は元の版付きパラメーター/有効化条件を継承し、今回ゼロや任意額を設定しない。

BurnResumeはburn理由だけを解除し、**独立BFT safety stop・未採用状態・不正claim・原期限を変更しない**。旧確定PAUSEDはsequenceと直前操作を保持して移行し、自動解除しない。旧未決操作は元policyでdrainし、新Resume権限には新認証domainを用いる。以下の追加モデル限定・genesis/fork対象外・登録者のみ解除という旧条項や過去試験は元profileの履歴として保持する。MOE3 native接続・移行実行・公開有効化の完了を意味しない。
<!-- MOE3-BURN-PAUSE-20261126-END -->

採択: [RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)／[ADR-0221](../../../adr/0221-additional-model-burn-pause.md)。**ADOPTED_DESIGN_NOT_ACTIVATED**。

元の[2026-11-10計画](public-one-honest-vertical-implementation-plan-2026-11-10.md)は原文保持する。[HF取得追補](hugging-face-cold-panel-2026-11-11.md)の独立登録検査・定期可用性認定・有効期限・HF取得不能裁定を実装残件から削除し、本書のburn停止へ置き換える。

独自モデル重み全量配信は廃止候補として依存を棚卸しする。モデル用の消失訂正・分散再配布・Node可用性サンプリングは必須経路から除外する。モデルroot/hash照合、claim固有入力/trace/Merkle opening、Nodeの局所courtは維持する。共用HTTP providerや局所証拠用のモデルopeningを一括削除しない。

新profileのMinerにモデル重み全量の配信義務はない。モデル全体の取得元はHF、Minerの供給対象はclaim固有資料と局所反証に必要なopeningである。残す資料codec・provider・保存リンクを、モデル全量配信の再導入へつなげない。旧契約を保持する互換コードと全量配信専用コードの削除完了は区別する。

## 最小実装と境界

`consensus/core/src/palw_model_pause_v1.rs`の純粋なplannerで、受理済みclass・artifact root・正規登録者、明示genesis/annual指定、期待旧state/sequenceを解決する。Pauseは誰でも、Resumeは登録者の認証を必要とし、別々の正の有限burn条件を導出する。返り値は未署名候補であり、native受理・実支払・状態書込みの権限ではない。

登録者欄がないことだけをgenesis免除の根拠にしない。実genesis登録と明示指定に基づいて除外する。追加モデルが年次指定へ入った時点で旧pause rowは無効となり、対象外化による永久停止を防ぐ。旧Frozen制裁はこのResumeで解除しない。

対象テスト2件が成功した。追加モデルのPause/Resume、異なる必要認証とburn、古いsequence/no-op/overflow/別scope/root、登録前DAA、登録者不明、genesis exact root、genesis catalogと年次指定境界を確認した。これは未署名plannerの試験であり、実署名の拒否や支払・native state保存を試験したものではない。

## 続く接続

1. wire/purposeと署名対象（直前canonical操作IDのpinを含む）、実carrier feeからの一回burn、認証state保存、journal/reorg・snapshot/IBD、資源料金を原子的に接続する。現plannerはscope/state/sequenceの段階であり、署名とbranch認証は未完成。
2. quote/rehearsal/実受付、全成功Final writer、rewardと生成権のconsumerへ同一gateを接続する。旧ModelLineなど別のidentity体系を対象へ含める場合は、共有classのowner/tierを流用せずnative登録者と固定rootを解決する。
3. canonical prefix順を維持する。現行pre-object deadline Finalは同blockの後続Pauseより先行する。post-object kernel tick等はPause後の状態を見る。PAUSED時にdeadline sweepが単にskipしてclaimを索引から落とすことを防ぐ。
4. 原期限と責任を保持して失敗/未検証終結を処理し、反証・供給応答を継続する。停止は計算不正の有罪根拠ではない。
5. 停止/解除burn額、処理上限、全reward境界、公開移行を確定してから有効化する。現時点で公開presetや残高を変更しない。

新TFP1のFpTokenizer/TKR1・共通planner・資料供給・選出Panel承認・公開quote以降も未完であり、burn plannerの実装をそれらの成功証跡にしない。
