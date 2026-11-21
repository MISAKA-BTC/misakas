2026-11-20 QF1追加版 — Qwen3のFOW1実行を具体化

Qwen3-8Bの36層、整数A16実行、HEX64 challenge入力、各回答tokenの予測位置で37本の4096要素vectorを読む仕様を追加しました。
Readoutのbyte順、4096-byte検証chunk、四Root、Ticket、Panel/CourtをRFC0066で固定しています。
MAS1の未証明・UNKNOWN停止、TB1、MOE2の経済条件、旧claimは維持しています。

状態: ADOPTED_DESIGN_NOT_ACTIVATED。
実Qwen推論・品質・native実装・G-COLL/G-COST・公開有効化が完了した意味ではありません。

入口:
 docs/rfc/0066-qwen3-fow1-execution-and-readout.md
 docs/adr/0234-qwen3-fow1-execution-and-readout.md
 docs/design/palw/models/qf1-qwen3-8b-profile.json
 docs/design/palw/models/qf1/README.md
 docs/maintenance/revisions/2026-11-20-qwen-fow1/VALIDATION.md

以下は前版の説明を保存したものです。

MISAKA MAS1 — モデル追加時の検査とBFT安全停止 — 2026-11-20

設計採択済み・未有効化（ADOPTED_DESIGN_NOT_ACTIVATED）。
新RFC0065 / ADR0233。FOW1のG-EXEC/G-COLL/G-COSTへ構造検査、公開suiteのG-USE、停止状態を追加。
UNKNOWNは採掘不可のままBFT reviewへ送り、timeoutや多数決で仮承認しない。
BFTはモデル/ruleの安全停止に限定。台帳finality・fork choice・reorg・既確定資産の変更権限はない。
burn Resume、owner変更、古い解除証明で安全停止を消せない。
モデル登録と市場作成は原子的。審査待ちを市場未準備とは扱わない。
MOE2の30%購入・20%永久LP・50%残余、coreの対象R100% Minerは維持。
Qwenの費用定理、自動解析の完全性、native実装・実機評価・公開activationを完成したとはしない。

正本:
 docs/rfc/0065-model-admission-screening-and-bft-stop.md
 docs/adr/0233-model-admission-screening-and-bft-stop.md
 docs/governance/model-admission-safety-transition.md
 docs/design/palw/models/mas1-admission-policy.json
 docs/design/palw/consensus/public-adjudication-policy-v0.6.md

変更・検証・保存原本:
 docs/maintenance/revisions/2026-11-20-model-admission-safety/README.md
 docs/maintenance/revisions/2026-11-20-model-admission-safety/VALIDATION.md

以下は入力MOE2版の案内（今回の限定変更以外を維持）。

MISAKA / MisakaOption MOE2 — 2026-11-20

設計採択済み・未有効化（ADOPTED_DESIGN_NOT_ACTIVATED）。
追加モデルの対象報酬を30% Position購入、20% BILI永久LP、残余50%へ分配。
購入Positionの必要分だけを同じ市場へLPとして戻し、余剰だけをburn。
GENESIS / FORK_DESIGNATEDはOption対象外、対象worker報酬Rの100%をMinerへ。
追加モデル登録時に一意市場が原子的に生成される。

主な正本:
 docs/adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md
 docs/adr/0232-reward-buy-and-permanent-liquidity.md
 docs/rfc/0064-reward-buy-and-permanent-liquidity.md
 docs/rfc-v2/0011-bond-budgets-fees-and-payments.md
 docs/governance/model-option-economics-transition.md

検証・変更一覧:
 docs/maintenance/revisions/2026-11-20-permanent-liquidity/README.md
 docs/maintenance/revisions/2026-11-20-permanent-liquidity/VALIDATION.md
 docs/maintenance/revisions/2026-11-20-permanent-liquidity/MATH_PROOF.md

旧MOE1の既存2,817ファイルはすべて保持。既存75ファイルを改訂し、変更前原本はbefore/に保存。
旧履歴中の50%全購入や全retireは過去規則であり、新profileにはMOE2を優先する。
整数dust時の元期限内保留・未成功終端を規定。全状態で正のLPが成立する保証ではない。
永久LP記録後のmodel ID / artifact rootはcore指定不可。
永久LPは非償還のnative拠出履歴であり、実資産は通常swapに使用可能。
今回の納品は文書・数学的参照検査であり、native/EVM実装や稼働ルールの変更ではない。
