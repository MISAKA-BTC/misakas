# ADR-DRAFT-DOOR3200 — A/B/C による bounded objective court の改修

- **Status:** Proposed / Review Required。採択・有効化済みではない。
- **日付:** 2026-10-19（Asia/Tokyo）。
- **番号:** 未割当。最新 branch の ADR-0191 等と番号台帳を照合してから割り当てる。
- **関連 RFC:** [RFC-0024](../0024-court-door-3200-to-ctx-2m.md)(旧 Draft 名 RFC-DRAFT-palw-court-door-3200.md)。
- **変更管理:** `PRINCIPLES §11`、RFC-0021、設計凍結規程に従う。

## 1. 問題

ユーザー提示の未実測の感度分析では、12,500 B/DAA 時の door は、最終裁定の読み込みを含めると 32K が 6,048 DAA、2M が 6,232 DAA。報告中の現行 gate 3,120 に対し、約 2,900〜3,100 DAA 超過する。最新 ADR-0191 の正式式と実装は添付 ZIP に含まれていないため、原因の最終確定・重大問題の実測証拠は統合レビューで補完する。

今回の要求は、全員共謀に対する外部 1 人の客観的訴追を維持しながら、door を 3,200 DAA 以内にすることである。原則や責任を変える可能性を持つため、単なる「性能最適化」という名称だけで設計凍結の例外手続きを省略しない。

## 2. 提案する決定

**A. 最終 history 裁定を 1 位置に局所化する。** 算術は既存の exact reduction を使う。original writer-row の開示を最小変更候補とし、HistTile との整合性・first divergence・完全 witness のサイズを確認する。1 行の bytes を抜いただけの未認証 witness は認めない。

**B. 最大 2 段分の全候補を開示する。** canonical な 16 分岐を維持し、最大 256 候補の frontier から途中の値を復元・検査する。選ばれた branch だけの先送りは最悪値の削減と認めない。増分 bytes/CPU/収録が待機削減を上回る profile は depth=2 で arming しない。

**C. 先行取得と強制開示を併用する。** original root、manifest、index、保持義務を拘束し、任意の先行取得で重複と冷起動を減らす。取得できない場合の既存 on-chain obligation と客観的 DA/default は残す。先行取得成功を最悪上限の前提としない。

**上限の扱い。** 3,200 を新しい提案上限とする。ただし、報告中の現行 3,120 を置き換えるパラメータ決定、関連する liability/retention/Final の再導出、Spec と台帳更新が承認されるまでは、現行 gate を維持する。

## 3. 採用理由

この組合せは、計算の客観的な最終判定をノードに残したまま、運ぶ原本と適応的な通信依存を減らす。TIR の既存 Sum/Max fold、step commitment、annex/DA demand、absolute deadline、SUM、journal を再利用できる部分がある。

一方、単一位置の authentic witness と 2 段 bundle のコスト優位は、profile ごとに確認が必要である。本提案は、数値目標から逆算して未測定の性能を仮定するものではない。

## 4. 不採用案・代替案

| 案 | 判断 |
|---|---|
| door を単に 10,000 にする | 今回の第一選択にしない。最適化後も未達なら、影響評価を伴う別の明示承認の選択肢には残す |
| block には root だけ載せ、取得できなければ諦める | 非協力時の訴追と DA/default が閉じないため不採用 |
| 全員がオンラインで先行取得済みと仮定する | cold verifier・最後だけ withholding・reorg を覆えないため不採用 |
| 新しい計算 validity proof に置き換える | 本 RFC の範囲外かつ現行 PRINCIPLES の禁止対象 |
| deadline を短くし、間に合わない正直な応答を default にする | 正直な実行可能性を損なうため不採用 |
| A のみ採用する | 改善候補として有効だが、2M は terminal 読込なしで既に 3,196。3,200 まで余裕 4 DAA なので、一般に十分とは言えない |
| depth=2 を全 site に強制する | reduction 値が多い site で response bytes が大幅増加するため不採用 |

## 5. 不変条件

計算不正と DA/default を区別する。署名・多数決・未検証 root を exact court の代替にしない。original claim の起点を変えず、追加告発で時計を再開しない。必要な証拠と担保を liability 中に解放しない。未被覆 kernel/fault を検証済みと表示しない。owner 上限・queue・費用で正直な 1 人を排除しない。

## 6. 互換性と置換範囲

本 ADR は、既存文書の未確認節を自動で置き換えない。採択時に最新 ADR-0191、関連 RFC/Spec、parameter register の具体的な変更節を列挙する。

新経路は休眠の versioned path とし、wire tag・codec・fingerprint は統合レビューで割り当てる。fence 前の behavior は維持し、既に開始された session は開始時の規則で完了する。責任保持を途切れさせる rollback は行わない。

## 7. 残余リスクと gate

- 約 12 KB は完全 witness として未検証。writer leaf/opening/metadata で上回り得る。
- 先読みは bytes と検証量を増やす。全候補 disclosure が遅い profile では効果がない。
- 事前準備や verifier replay の時間が元の door に残る場合、C の性能改善が限定される。
- 最新の door 式と tag 245、tag 230、D2/X3 が ZIP 外にある。
- 32K/2M の実ノード・出荷 binary の上限、価格、保持、reorg/restart は未実測。

これらを UNKNOWN のまま残して arming しない。RFC の受入条件、独立レビュー、出荷 binary での record pin、fence-crossing drill がそろわなければ新経路は休眠のままとする。

## 8. 採択・有効化の記録欄

```text
RFC 正式番号: 未割当
ADR 正式番号: 未割当
最新 source commit: 未確認
変更対象の既存 ADR/RFC/Spec 節: 未確定
3,120→3,200 のパラメータ承認: 未記録
独立レビュー / evidence: 未記録
RFC と ADR の明示的採択: 未記録
出荷 binary / cost / arming record: 未記録
activation DAA / fingerprint: 未割当
fence-crossing drill: 未実施
t12 配備承認: 本成果物には含まない
```
