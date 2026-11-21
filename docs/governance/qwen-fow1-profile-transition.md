# QF1適用・移行 — Qwen3のFOW1実行具体化

- 日付: 2026-11-20（Asia/Tokyo）
- 状態: **ADOPTED_DESIGN_NOT_ACTIVATED**
- 正本: [RFC0066](../rfc/0066-qwen3-fow1-execution-and-readout.md)／[ADR0234](../adr/0234-qwen3-fow1-execution-and-readout.md)

## 適用先

QF1-Q3-8B-A16-8K-v1を明示する新しいdescriptorと試行だけに適用する。Qwen3-8Bの名前だけで一般登録・既存claim・genesis/forkモデルを切り替えない。旧claimは受付時のprofileと責任でdrainし、新しいReadout・入力・Ticketで再計算して権利を増やさない。

## 優先順位

QF1の具体的な数値・shape・Readout・codecはRFC0066が所有する。FOW1のG-EXEC/G-COLL/G-COST、MAS1のG-USEとUNKNOWN/BFT停止、TB1のfreezeと用途独立性、MOE2経済は継承する。QF1はこれらの必須gateを弱めない。

限定置換は、AN1/RFC0062の35-token byte frameをQF1用HEX64 chat frameに変える点と、未指定の具体Readout/数値/codecを固定する点である。FOW1全体の入力adapterを一括変更するものではない。旧native wire番号やHash64の意味を置換しない。

## 開始条件

instanceの全artifact・TIR・scale・tokenizer・StateMapを固定し、資源上限と完全Court catalog、G-EXEC/G-COLL/G-COST/G-USE、MAS1停止状態、native実装・移行・activationが全て成立するまでは採掘不可とする。公式重みや小規模vectorの成功を条件の代替にしない。

QF1の受入試験では、Minerへの追加質問なしで、正常なclaimの完全再実行、不一致の直接反証、必要資料が欠けたまま元期限に達した場合の未検証終了を確認する。対話による局所化を提供するかは任意で、実装はfenceの開始条件に含めない。[RFC-0062 §3.1](../rfc/0062-palw-function-output-and-work-admissibility.md#fow1-panel-dialogue)

非報酬評価はMAS1のMODEL_AUDITと後続独立PANELを使う。一般登録と追加モデルの市場原子生成はそのままで、評価からcredit/weight/受賞権利/報酬を発行しない。後日合格しても過去の評価に遡って報酬を付与しない。

## 現在の到達点

Qwen3-8Bの固定整数実行について、隔離3ピアで通常のPanel承認、Endの直接裁定、資料欠落時の承認不足と元期限を確認した。QF1-10の一貫検証、変換後の実用性、適合する費用定理、公開有効化は未完了である。各試験が確認した範囲は[RFC-0066 §12](../rfc/0066-qwen3-fow1-execution-and-readout.md#12-受入試験と完成判定)に記録する。
