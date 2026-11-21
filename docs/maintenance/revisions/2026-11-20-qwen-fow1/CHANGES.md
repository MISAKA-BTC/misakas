# QF1の変更内容

## 新規規範

RFC0066とADR0234に、Qwen3-8Bの36層dense graph、4096 hidden width、12288 FFN、QK norm、GQA、整数A16、固定入力frame、生成・停止・Readout・四Root・Ticketを一体で記述した。

| 決定 | 内容 |
| --- | --- |
| 数値実行 | 既存PALW-TIR/C1を使い、weights/scale/epsilon/gain/table/programを登録instanceへ固定 |
| 入力 | QF1専用HEX64 chat frame。AN1の35-token frameからの限定置換を明記 |
| 生成 | non-thinking frame＋greedy、tieは最小ID、EOSをcanonical出力に含む |
| Readout | 各token予測位置の全36第二残差後とfinal norm後、37×4096 i16 |
| 時系列 | j番目の予測はL−1+j、処理位置数L+T−1、最後の選択tokenの余分なforwardなし |
| digest | 数値streamのBLAKE2b-512。s/nonceをzへ直付けしない |
| 局所検査 | 4096-byte hash chunk、最大32 compression。元TIR stateのopeningと連鎖を認証 |
| trace | Init/TirValue/Decision/Stop/ReadoutChunk/Endの論理schema |
| Root/Ticket | 固定木形・64-byte root・32-byte Ticket・field順・domain・target境界 |
| 未完gate | G-COLL/G-COST/G-USE・nativeは未完。MAS1 UNKNOWN停止を継承 |

## 担当入口

現行の入出力、TIR、登録、資源、Panel、会計、元claim、positive/negative Court、モデル非保有Node、FinalへQF1の参照と責務を追加した。FOW1/MAS1の親RFC/ADR、現行統合ADR、モデルSDKの案内、適用・移行、索引・番号台帳も更新した。

追加モデル登録時の市場生成、MOE2の30/20/50・通常売買5/3、genesis/forkの経済配分、TB1乱数方式・用途・BFT権限、旧claimの受付条件は変更していない。

## 追加した参照と境界

小規模Python参照・否定例・公開synthetic vectorを同梱した。実Qwen weights/tokenizerは使用していない。hash chunkの圧縮転記はhashlibと比較し、token位置・EOS・型・vector順・Merkle opening・target境界を確認する。これらを実LLMのG-COLL/G-COST又はnative Courtの成功へ昇格させない。

変更前の3012ファイルは全て保持する。変更件数・exact before・歴史payloadの検査結果はVALIDATIONとchange-manifestに記録する。以前の検査ログ・source digest・欠落参照baselineは更新しない。
