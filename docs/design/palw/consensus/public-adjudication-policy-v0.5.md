# 公開裁定の方針 v0.5 — TB1乱数とFOW1コア出力

<!-- MAS1-ADOPTED-20261120-BEGIN -->
## 2026-11-20 後続採択 — MAS1の構造・実用性検査とBFT安全停止

**ADOPTED_DESIGN_NOT_ACTIVATED**。[RFC0065](../../../rfc/0065-model-admission-screening-and-bft-stop.md)／[ADR0233](../../../adr/0233-model-admission-screening-and-bft-stop.md)／[MAS1適用・移行](../../../governance/model-admission-safety-transition.md)を、この新規採掘profileの追加正本とする。MAS1対象の公開裁定は後続v0.6へ接続する。v0.5の既存保証を強めたことを、LLM費用定理の完成と扱わない。

構造検査はPASS/FAIL/UNKNOWNを区別し、UNKNOWNは採掘不可のままBFT reviewへ接続する。G-EXEC/G-COLL/G-COSTを維持し、公開・共通の実用性G-USEと停止状態をAND条件へ追加する。BFTはモデル/ruleの停止と条件付き解除に限り、未知の費用定理の承認・台帳finality・fork choice・reorgの権限を持たない。burnやowner変更で安全停止を解除しない。

旧「モデル品質・モデル単位の検査を一切しない」という文言は、MAS1の採掘資格に関する今回の限定検査へ置換する。一般登録とMOE2市場原子生成、通常取引、coreの経済配分、Panel＋Court、計算validity proof不使用、旧claimの元責任は維持する。旧採択ブロック・原文・実測はその版の履歴であり、今回の数学的証明・native実装・公開activationが完成したという意味ではない。
<!-- MAS1-ADOPTED-20261120-END -->

- 2026-11-20、**ADOPTED_DESIGN_NOT_ACTIVATED**。
- 正本: [RFC0061](../../../rfc/0061-palw-internal-threshold-beacon.md)、[RFC0062](../../../rfc/0062-palw-function-output-and-work-admissibility.md)、[共通移行契約](../../../governance/palw-tb1-fow1-transition.md)。
- v0.4および受理済み旧claimは、その版の意味を保持する。

## 1. 適用する新規仕事

明示されたFOW1 activation境界以後の新規PALW採掘・報酬・合意workには、登録されたコア関数の採掘値z、その非衝突性の適合条件、複数試行費用条件を要求する。MOR1の認証出力識別だけで新profileを満たしたとしない。自然回答y、コア採掘値z、補助receipt rhoを分ける。

モデル保存登録の自由、immutableなモデルID、既存1.0/0.3、additional modelのburn停止・登録者解除は維持する。停止・未適格・未対応を、旧profileや別APIへ付け替えて新規成功にしない。

## 2. 乱数の唯一の新規source

実行入力はTB1-EXEC、Panel選出は独立したTB1-PANELから得る。登録Cと全候補はEXECの最初の正直share放出前に安定固定し、Minerの元Original・pool・DrawPolicy・責任はPANELの放出前に別に固定する。certificate完成時刻を事前固定の境界に使わない。

TB1のshare保持者はLLMの正しさや台帳Finalを投票で決めない。q=2f+1の公開share certificateと、能動的な正直Panelが少なくとも一人いるという前提は別である。TB1のfault boundから後者を導かない。

旧work-derived、seal-reveal、BFT/BLS、block hash、公開済みEXEC値の再hashへfallbackしない。FOW1のTicketへ第三の未来乱数を追加しない。

## 3. 正当性・反証・資料

Panelは登録された実入力、y、z、canonical traceの全必須関係を再実行して照合する。Nodeは元claimと固定programへ認証された有界な局所関係を検査する。局所positive、rho一致、share certificate、署名一件だけで成功Finalにしない。

[RFC-v2-0016](../../../rfc-v2/0016-miner-execution-commit-and-material-supply.md)〜[0020](../../../rfc-v2/0020-verification-gated-final-and-adversarial-models.md)の資料・完全承認・一人の有効反証優先・未解決caseによる成功停止を維持する。最低完全承認数1と、選出Panel人数や正直者の実在は別の条件である。

negative、資料要求・応答、局所Court、終端は新beaconを待たない。元期限・元予約・元の署名責任を使い、TB1の準備負荷が既受理の救済容量を消費しない。取得不能は計算不正の自動判定ではない。

## 4. 一回会計・取消・保存

slot、試行、実仕事、用途別BaseWorkKeyの消費とFinal・credit発行を原子的に行う。別salt、owner表示、再署名、receipt、carrier、後続beaconで同じ仕事を増幅しない。正しい最適化をslashで排除せず、費用定理の攻撃者モデルへ含める。

原則の一つの鎖、PALW-only、Node非保有、計算validity proof不使用、既存のCourt責任と期限を維持する。既受理の旧claimはdrainし、曝露roundはDB undoや深いreorgでfreshに戻さない。

## 5. 有効化

TB1の完全VSS・PQ channel・native stable broadcast・曝露journal・各purposeの実装と合成評価、およびFOW1の具体的なモデル系列と定理適合checkerが必要である。現在の文書・fixtureの成功を、それらの完了としない。条件未成立の新profileは有効化しない。
