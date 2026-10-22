# ADR-0205 — decode 済みの回答は二度支払わない

Status: **Proposed / Draft** (2026-10-22, compute-claim audit lane compute-exec)。[RFC-0038](../rfc/0038-palw-fp-answer-accounting.md)。
監査: compute-exec CX-01 / CX-02。ADR-0145 §6 の「支払済み prefix は再び支払わない」の対象を prompt から
`prompt ‖ answer` に広げる。

## 決定(提案)

1. `palw_fp_answer_accounting_v1`(dormant)以降、compute-priced free-prompt commitment は回答 id を運び、chain は
   `prompt ‖ answer` を class の支払済み列として記録する。
2. claim の credit は、有効な支払済み row が同じトークンで既に計算した位置より後ろの位置だけ。新しい位置が無い
   claim は受理しない(`ZeroQuanta`)。
3. 回答の真正性は seat の replay(claim の `output_root`)で拘束する。暗号的計算証明は使わない(PRINCIPLES §8)。
4. 保守側の帰結(明示): 他者が既に支払われた列をコールドで再計算した生産者は、その位置の credit を得ない。
   chain はキャッシュ再利用とコールド再計算を区別できず、区別できない方向で「起きていないかもしれない計算」に
   払わない(ADR-0145 §6 の既存の立場と同じ)。

## 代替案

| 案 | 不採用の理由 |
| --- | --- |
| 回答を運ばず「P を拡張する prompt は D_P 位置まで既払い」とみなす | 安価な偽 claim 1 本で、その prefix を持つ全ての正直な claim の decode credit を潰せる(griefing) |
| work id を bond 非依存にする | 同一 prompt の先行 commit で正直な生産者を締め出せる。継続(CX-01)は塞がらない |
| seat の node policy のみ | consensus の価格が変わらず、seat を持つ側が自分の claim を通せる |

## 実装状況

consensus-core の fold 規則・価格・row・carriage の encode/decode と fence 配線は実装済み(dormant)。
processor ingress、isolation door、seat の回答照合、producer の FPA1 出力は未実装で、arming record の前提
(`validate_palw_fp_answer_accounting_v1` が arming を拒否)。
