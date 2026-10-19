# RFC-0024 — ctx 2M・大きいモデルでも door ≤ 3,200 DAA に収める court 設計(単一位置裁定・先読み開示・先行取得)

- **Status:** Draft / Review Required。実装完了・採択済み・有効化可能という宣言ではない。
- **作成日:** 2026-10-19。ユーザー指示により RFC-0024 として登録。
- **対となる ADR(草案):** [ADR-DRAFT-palw-court-door-3200](0024-assets/ADR-DRAFT-palw-court-door-3200.md)。正式な ADR 番号は採択時に割り当てる。
- **設計目標:** モデルの大きさ(≤10B は model size の制限であり ctx の制限ではない)と ctx(最大 2,097,152)に関係なく、すべての合法 job について TIR の公開 door を **≤ 3,200 DAA** に収める。producer・Panel・custodian の協力を安全性の前提とせず、適格な外部検証者 1 人による訴追を維持する。
- **採る変更:** A=最終 history 裁定の 1 位置化、B=最大 2 段の先読み開示、C=認証済み material の先行取得と強制開示の併用。
- **非変更:** モデルの算術、検出 policy、正しい署名・root の検証、計算不正と DA/default の区別、担保・責任保持、Gate C、客観的 exact court。ZK・SNARK/STARK・zkVM・PCS によるモデル計算の validity proof は導入しない([PRINCIPLES](../PRINCIPLES.md) §8)。
- **上限値:** 従来の `maximum_door_daa = 3,120` から 3,200 への変更は**明示的な承認対象**であり、本 RFC の登録だけでは gate を変えない。
- **有効化:** 新経路は休眠。wire tag、fingerprint、DAA 16,000、稼働 t12 の設定は本書で割り当て・変更しない。
- **Related:** ADR-0189(door 候補)、ADR-0190(bounded rounds)、ADR-0191(dispute-local decomposition)、[g14 実測 evidence](../evidence/palw/g14-measure-2026-10-18/)。

> **提案する決定:** court の判定をノード上に残したまま、裁定に必要な原本の範囲を小さくし、適応的な要求を前もって網羅し、待機の依存を減らす。3,200 を満たしたことにするために既存タイマーを切り詰めたり、オフチェーンの好条件を最悪時の保証に読み替えたりしない。

## 最新 branch との照合(2026-10-19 時点、g14/door-design)

本文は `misakas-pre` の ZIP を基に書かれており、ADR-0191 系の最新実装を監査していない(§0.1)。登録時点で branch `g14/door-design` にある内容は次のとおり。いずれも休眠・fence 未武装で、door の値は**式による derived_upper であり実測ではない**。

| 項目 | commit | 状態 |
|---|---|---|
| history tile の b=16 二分探索(⌈log16 tiles⌉ 往復) | 16555f312 | 純関数・試験通過 |
| A に相当する tile 内 1 位置への追加往復(⌈log16 tiles⌉+1 往復) | 16458bbb1 | 純関数・試験通過。終端 ≈12 KB は**見込み**で認証証拠込みの上限ではない(本文 §4 の条件どおり) |
| 指名 custodian 回答の登録簿(tag 245)を state field 化・processor 接続・block 試験 | ec2c4ae94 / c718fa12e / 5d7905bdf | 休眠 |
| door 導出表 | bb4daa746 / b6e8a3d16 | `docs/evidence/palw/g14-measure-2026-10-18/door-bisection-derived-1019.md` |

1 位置化後の derived_upper(DAA):

| | 32K, 125,000 B/DAA | 2M, 125,000 B/DAA | 32K, 12,500 B/DAA | 2M, 12,500 B/DAA |
|---|---:|---:|---:|---:|
| 最終裁定の読み込みを含めない | 1,519 | 1,703 | 3,058 | 3,242 |
| 1 位置の最終裁定を含める | 1,635 | 1,819 | 3,408 | 3,592 |

125,000 B/DAA は楽観的な上限で感度分析用、12,500 はその 1/10 の感度値であり、どちらも実測ではない。低帯域時の主因は 17,408 B の step block の搬送が段ごとに重なることで、B(先読み開示)による段数削減と C(先行取得)が、本文 §8 の `S ≥ X − 4`(2M)を満たせるかが採否を決める。

