# QF1文書検証 — 2026-11-20

## 判定

**今回の改訂について、入力原本・歴史payloadの保全と、文書の増分検査はPASS。小規模logical codec参照は57項目PASS。旧パッケージ全体のintegrityは既存不整合が残るためPASSではない。**

検査対象は文書、synthetic tensor、位置・byte・hash規則に限定する。Qwen実機、実tokenizer、完全TIR lowering、native Court、G-USEの品質・性能、G-COLL/G-COST、TB1/BFT実動、公開activationは未検証である。

## 保存結果

| 項目 | 結果 |
| --- | --- |
| 既存入力ファイル | 3012件をすべて保持 |
| 同じbytesを保った既存ファイル | 2961件 |
| 更新した既存ファイル | 51件 |
| exact before bytesを保存 | 51件すべて一致 |
| 更新した統合冊の既存本文全体 | 21冊でQF1追記部分以外のbytesが完全一致 |
| 更新した統合冊の歴史payload | 16件すべて不変 |
| 新規の正本・schema・参照ファイル | 9件 |
| 現行統合冊数 | RFC20冊 / ADR50冊 |
| 以前の改訂フォルダ・実測記録の変更 | なし |

## 実行した検査

1. `python3 docs/design/palw/models/qf1/check_qf1_reference.py` — 57項目PASS。EOS/生成位置/最後のtoken非forward、型とi16範囲、vectorの順序、hash更新分割非依存、独立転記BLAKE2b圧縮とhashlibの一致、chunk counter/final flag/chain改竄、別試行・別indexのMerkle opening、木のpadding、四Root Ticket、target境界を確認した。
2. `python3 docs/maintenance/check_docs.py` — 変更前/後とも947件の既存指摘。入力版比の新規指摘は0。欠落doc参照971件・heading2件などは元から存在し、known-link-issues baselineを変更していない。
3. `python3 docs/rfc-v2/check_integrity.py` — 前/後とも旧sourceとの7件の不一致。新規指摘0。
4. `python3 docs/adr-v2/check_integrity.py` — 前159件、後157件。今回編集した現行領域のchecksum更新により2件が解消、新規指摘0。旧source digestや歴史payloadは変更していない。
5. `python3 docs/maintenance/revisions/2026-11-20-qwen-fow1/check_revision.py` — 原本保存・歴史payload・新profileの未完成flag・担当入口・登録冊数の検査PASS。

## 記録

raw結果はvalidation/baseline/、validation/post/、validation/reference-check.json、validation/revision-check.json、差分はvalidation/input-relative-delta.jsonに保存した。参照vectorの1 stepは303141 byte、Readout hash chunkは75件。これは公開の架空データでありQwenの測定値ではない。

リンク検査はofflineであり外部URLの将来可用性を保証しない。ZIPは全ファイルCRCと主要ファイルの元bytes一致を梱包後に確認する。利用者の端末でのダウンロードを実際に試したという意味ではない。

## 完了していないもの

実8Bのimmutable artifact・全scale/table・tokenizer・StateMap、runnerのstage実装、ReadoutとTIRのnative Court、二実装のbit一致、実際の品質/性能/保持費用、適合TheoremRule、MAS1/BFTの実動とactivation。これらが揃うまではQF1の採掘は閉じたままである。
