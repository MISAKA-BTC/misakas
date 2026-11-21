# ADR-0221 — 追加モデルの取得可否認定をburn停止・解除に置き換える

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
