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
