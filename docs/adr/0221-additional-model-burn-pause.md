# ADR-0221 — 追加モデルの取得可否認定をburn停止・解除に置き換える

<!-- BMR1-ADOPTED-20261126-BEGIN -->
## 2026-11-26 最終訂正 — BFT継続5%・モデル恒久廃止と用途別bond

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0069](../rfc/0069-bft-model-retirement-and-role-bonds.md)／[ADR0237](0237-bft-model-retirement-and-role-bonds.md)／[BMR1移行](../governance/model-retirement-transition.md)をBFTの継続報酬・恒久廃止・用途別bond処理の正本とする。最新配分は同一coinbase原資Cの50/20/20/5/5であり、BFTの通常継続報酬は**`F=floor(50C/1000)`（5%）**、Panel/Gate CのMISAKA枠は**`P=floor(50C/1000)`（5%）**。同日途中の10%と可変W/P前段を撤回する。残るPOSITION購入70%とMISAKA LP20%はRFC0068の同じ一回分割・残差規則に従い、5%をさらに二重控除しない。検証者5%内の役割配分は既存の正規planで決める。

正規BFTのモデル恒久廃止で、当該永久LP内actual MISAKA・登録者責任bond・MINING用途bond・未確定mining報酬を100%burn/失効させる。**全Panel/Gate C検証用途bondは、不正者も含め一律にBFT廃止burnから保護**し、個人不正はCourtの個別slashへ分離する。正当に成立した検証報酬の支払を維持し、他モデル資産へ影響させない。兼任は用途別lot（MINING6,000だけburn、PANEL4,000は保護）とし、重複担保・責任中の用途変更は禁止する。

通常のLP撤去禁止を維持し、canonical廃止のactual reserve burnだけを限定例外にする。BFTの没収受取・廃止bountyは0。継続5%報酬を没収額に連動させない。可逆Pause/Resumeと区別し、旧停止専用・LP絶対不処分の記述をこのscopeだけ置換する。旧profile/native/履歴を保持し、BMR1実装・移行・有効化は別工程とする。

**個別Court slashの実回収額は別会計。** 対象offenceから実際に回収したMISAKAをSとし、Panel/Gate C合計の報酬を`E=floor(490S/1000)`、残り`T=S-E`をburnとする（名目49%/51%、端数はburn）。49%は両役割の合計であり、それぞれ49%ではない。不正発見・立証の資格を認証したplanで一回配分し、不正本人の自己slash受領と同一offenceの二重報酬を禁止する。BFTモデル廃止の100%burn資産をSへ加えず、BFTの没収受取・廃止bountyは0のまま。検証bondがBFT一律burnから保護されることは、本人のCourt個別slash免除ではない。
<!-- BMR1-ADOPTED-20261126-END -->

<!-- MOE3-BURN-PAUSE-20261126-BEGIN -->
## 2026-11-26 後続採択 — 全モデル共通のburn停止・解除

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0068 §2.4](../rfc/0068-fractional-position-miner-rewards.md)／[ADR0236](0236-fractional-position-miner-rewards.md)／[MOE3移行](../governance/fractional-position-economics-transition.md)を優先する。MOE3の全モデル統一に伴い、genesis・旧forkのburn-pause対象外を廃止する。誰でも正規支払者署名・期待ACTIVE/sequence/直前操作照合・規定停止burnによりPauseできる。

ResumeはPAUSEDから、**native登録者の正規認証 OR 共通BFTの専用BurnResume承認**と、どちらの経路でも規定解除burnの実支払を必要とする。BFT経路は登録者の有無を問わず全モデル共通とし、creatorなしgenesisが解除不能になることを避ける。fee受取人・contributor・POSITION holderを登録者の代わりにしない。

専用承認はnetwork/genesis・MOE3/pause policy・model/root・期待PAUSED/sequence・直前canonical操作・roster/epoch・purpose・期限・解除burn額/上限を束縛する。通常採用YESや安全停止解除票の流用、支払者署名/実支払の省略は認めない。署名・burn・状態更新を一つのnative遷移で確定する。具体的なburn額は元の版付きパラメーター/有効化条件を継承し、今回ゼロや任意額を設定しない。

BurnResumeはburn理由だけを解除し、**独立BFT safety stop・未採用状態・不正claim・原期限を変更しない**。旧確定PAUSEDはsequenceと直前操作を保持して移行し、自動解除しない。旧未決操作は元policyでdrainし、新Resume権限には新認証domainを用いる。以下の追加モデル限定・genesis/fork対象外・登録者のみ解除という旧条項や過去試験は元profileの履歴として保持する。MOE3 native接続・移行実行・公開有効化の完了を意味しない。
<!-- MOE3-BURN-PAUSE-20261126-END -->

<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](../rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](../governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。burn停止の解除はburn理由に限る。MAS1の安全停止・未適格・rule失効は別条件として保持する。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未検査の費用経路の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

Status: **Accepted / ADOPTED_DESIGN_NOT_ACTIVATED**
Date: 2026-11-11
RFC: [RFC-0053](../rfc/0053-additional-model-burn-pause.md)
Partially supersedes: [ADR-0220](0220-hugging-face-model-acquisition-and-claim-materials.md)

ユーザーはHF取得証明・第三者検査・多数決を使わず、誰でもburnで停止、モデル登録者だけがburnで解除する方式へ変更するよう明示指示した。genesisモデルと年次fork指定モデルを除外し、後から登録された追加モデルだけを対象とする。この指示を採択として記録し、取得可否認定の設計を続けない。

HF固定版の取得・hash照合とclaim固有資料の供給を分離する方針は維持する。ただし取得結果をモデル停止・解除の合意入力にしない。登録時の独立全量検査、定期可用性record、期限、HF取得不能の裁定という旧必須条件は廃止する。Panelが実際の再計算に必要なデータを取得することと、チェーンが取得可能性を認定することを区別する。

ユーザーの機能別指示に従い、独自モデル重み全量配信は廃止候補、モデル用の消失訂正・分散再配布とNode可用性サンプリングは必須経路から除外する。モデルroot/hash、claim固有の入力/trace/Merkle opening、Nodeの局所courtは維持する。共有providerや旧profileを無差別に削除しない。

PAUSEDは新規受付・成功Final・新規報酬確定を停止する。既存claimの反証・責任・原期限・保管は継続する。登録classやtierは変えず、追加の稼働stateとする。年次指定によって対象外となればpauseを適用しない。独立した旧制裁の解除には使わない。

停止・解除は認証済み署名と実支払に基づき、canonicalな状態・sequence・burnを一つのcheckpointで更新する。先行状態、競合順、reorgとburnの一回性を検査する。HFへのアクセス証明や他者の許可を条件にしない。固定burnでDoSが防げるとは主張しない。

PRINCIPLES §9の可逆停止に関する限定改定を明示する。品質審査、class取消、1.0/0.3変更、計算の暗号学的証明は導入しない。金額・有効化高さ・全reward/生成権consumerの確定境界は別の実装・数値台帳に残す。未設定の値を推測で出荷しない。

番号は現在checkoutと取得済みlocal refsで同番号が未使用であることを確認した。旧RFC/ADRの本文は履歴として残し、新しい規範入口から本決定への優先関係を示す。
