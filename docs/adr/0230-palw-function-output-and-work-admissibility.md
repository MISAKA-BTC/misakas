# ADR-0230 — 推論関数の採掘出力と費用定理を新規採掘の共通条件にする

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 対応RFC: [RFC-0062 PALW-FOW1](../rfc/0062-palw-function-output-and-work-admissibility.md)
- 乱数契約: [RFC-0061 PALW-TB1](../rfc/0061-palw-internal-threshold-beacon.md)／[ADR-0229](0229-palw-internal-threshold-beacon.md)
- 改訂対象: RFC-0059、RFC-0060、ADR-0227、ADR-0228、および現行の入出力・登録・検証・会計・合意の担当冊

## 背景と今回の明示指示

監査により、AN1は別試行の実token入力を要求していたが、LLM出力の相違や費用下界を要求していないこと、MOR1は`O=(y,s,rho)`の認証出力の非重複性へ限定していることが確認された。ADR0228はユーザーの意図を「正式なプロトコル出力」と解釈していた。これは、LLM関数自体に採掘値を生成させ、その新しい機会に必要な計算を保証するという要求全体の反映ではなかった。

ユーザーは今回、保証対象、モデル登録、beacon/Ticket、仕事会計・再利用、移行規則の五項目を修正し、添付PALW-TB1へ乱数生成を変更するよう明示した。本ADRはその設計変更の採択記録である。過去の署名済みclaim・実測・採択記録を遡及改変する指示とは解釈しない。

## 判断

1. **PALW-FOW1を新しい共通採掘profileとする。** 規範コア実行は`CoreExec(M,P,x)=(y,z,tau)`である。回答y、関数由来の採掘出力z、試行ID s、補助receipt rhoを別型にする。
2. **別試行の非衝突性をzへ要求する。** zは同じ推論の規定状態からReadoutされ、全traceとEnd関係で検証する。s又はH(s)のecho、後付けnonce、receiptの名称変更をモデル関数の保証としない。自然回答yが同じであることだけは不正にしない。
3. **採掘適格性をモデル保存登録と分離する。** 全permissionless候補に同じTheoremRuleと決定論的な適合predicateを適用する。モデル名、評判、品質、取得可否の審査ではない。個別artifactに関する未確認の適合前提又は未知checkerがあれば採掘不適格とする。暗号・腐敗上限・ネットワーク等の外部安全性仮定は明示して条件付き保証とし、Nodeが登録時にそれらの成立を証明したとは扱わない。
4. **三つのgateを必須化する。** 全実行関係と局所反証の完全性、コア採掘値の非衝突性、初回Ticket照会前の複数試行費用下界を別々に示す。MOR1のhash衝突定理、TB1の予測不能性、Panelによる正当性を費用定理へ読み替えない。
5. **乱数生成はTB1に固定する。** EXEC/PANELは独立秘密と別freeze。最初の正直share公開前の固定、全qualified dealer、曝露journal、no fallbackを継承する。BFT台帳finality、BLS候補、旧anchor fold／seal-revealを新規FOW1の代替乱数源にしない。
6. **一試行一Ticketを四Rootへ結合する。** `T=H_FOW1-ticket(Enc(s,R_x,R_y,R_z,R_tau))`。新family/domainを用い、旧三Root codecの意味を変えない。future R_ticket、Final後receipt抽選、旧quantum複数drawを重ねない。
7. **追加計算と再利用を同じ定理へ含める。** cache、batch、prefix、前処理、外注、特殊モデル、入力選別を含む最良アルゴリズムの費用を分析する。正しい最適化を自動的にslashせず、高価な証明・検証・罰金でLLM評価下界を代替しない。
8. **新規受付の移行を閉じる。** 明示したactivation A以後の新規PALW採掘はFOW1＋TB1を必要とし、旧profileの新規受付を閉じる。既受理旧claimは原期限・旧署名・固定drain規則で処理する。旧履歴readerから新規乱数・Ticket・creditを生成させない。

## 証明と未完成事項

同じ固定試行の正規実行・Root・Ticketが一つになることは、決定性とcanonical encodingから示せる。費用Bで作れる相異なる正しい初回Ticket入力の数に上界が証明されれば、明示した古典Ticket ROの下でその数から当選確率の上界を導ける。この接続と、必要な費用下界そのものの証明は別である。

本改訂は、既存Qwenの重み、特定LLM/kernel、実GPU費用についてG-COLLとG-COSTを完成させたと主張しない。現在、FOW1適合を認定できる完成済みTheoremRule・native適合検査器・本番モデルをこの変更で追加したわけではない。未完の間は新profileを有効化せず、適格集合が空である状態を完成済みPoWとして公表しない。

Nodeに一般のプログラムの評価困難性を自動証明する万能機能を仮定しない。外部安全性仮定と、個別artifactへの適合前提を分け、後者をNodeが全モデルを持たずに確認できる有界な登録契約をruleごとに必要とする。登録者の証明書署名、単独Panelの承認又は少数テストをその代替にしない。新しい実行validity SNARKを追加する方針でもない。

## 既存方針に対する限定的置換

| 既存の規則 | 新profileでの変更 |
| --- | --- |
| AN1の実入力のみの束縛 | 保持し、コア採掘出力と費用の必須gateを追加 |
| MOR1の認証出力非重複性 | 補助保証として保持。モデル関数の採掘適格性には不足と明記 |
| モデル無審査登録 | 一般登録は維持。新規採掘には全提出者共通の数学的・構造的適合を要求 |
| 未指定/旧方式の未来beacon | 新規対象は0061のTB1へ統一。旧資料は履歴検査・drainに限定 |
| 旧三Root Ticketとreceipt/quantum抽選 | FOW1四Root・一試行一Ticketへ置換 |
| prefix等の正しい再利用 | 意味・originに加え、採掘費用定理の分析範囲内であることを必要化 |

モデル品質、保有人数、HF取得可否の審査、年次指定以外のtier昇格、mutable Model ID、延期された画像・動画／自動モデル改善は復活させない。既存1.0/0.3、資本・発行量・容量の上限は維持し、政策係数を費用の定理へ読み替えない。

設計凍結に対する今回の変更範囲は、本ADRと0061/0062が列挙する新規採掘・乱数・適合性・移行契約である。旧文書の全条項を無差別に失効させるものではない。具体的な置換表は0062 §9を正本とし、現行冊の参照もこの適用範囲に従う。

## 完成・移行条件

有効化前に、具体TheoremRuleと証明・独立review、全局所checker、TB1の完全VSS/PQ/nativeとの合成、資源・cold取得・Court、全consumer、移行Aと旧pending drain、同一仕事の旧新alias、reorg/曝露/一回消費の統合を検証する。

設計方針の採択は、これらの達成又は公開activationの承認ではない。失敗時は未成功終端・既受理救済・no fallbackを維持し、未証明の仕事や未生成乱数から合意資格を作らない。
