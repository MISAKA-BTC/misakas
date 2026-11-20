# 検証記録 — RFC-0057 Court nonce

対象: [RFC-0057](../../../rfc/0057-court-nonce-and-panel-challenges.md) / [ADR-0225](../../../adr/0225-court-nonce-and-panel-challenges.md)。

状態: PROPOSED_DESIGN_NOT_ACTIVATED。設計・抽象契約・文書の検査を、native実装・実機・公開activationへ昇格させない。

## 検証の入口

- [ソース監査](SOURCE_AUDIT.md)と[source照合](source-provenance.json)。
- [独立レビュー](REVIEW.md)。本文の4指摘を修正したことを確認済み。
- [抽象モデル](check_nonce_model.py)と[保存結果](model-results.json)。
- [文書保全](check_revision.py)、[保存結果](revision-results.json)、[標準checker前後比較](validation-results.json)。

## 結果

| 検査 | 結果 | 範囲 |
| --- | --- | --- |
| CPN1抽象モデル | 49 PASS / 0 failure / 0 error / 0 skip | nonce・selector・輸送chunk木・toy整数局所検査・一回性・承認/Q・有限責任 |
| 今回の文書保全checker | PASS | 元1,975件のうち1,950件byte一致＋25許可変更、削除0 |
| 元metadata・追記 | 5件のpreimage認証、front pointer 12件、末尾追記2件 | 追記除去で元本文復元。旧採択状態不変 |
| 主題別文書とparameter | 20 RFC / 50 ADR、旧70項目不変＋新8提案 | 0057/0225と追加項目はPROPOSED、runtime設定変更なし |
| 過去revision | 103ファイルbyte一致 | 先行0055/0056の試験結果・baseline・checkerを再認証した意味ではない |
| 生成索引・catalog | 6件の生成結果とbytes一致 | 認証済みgeneratorをメモリ内で実行し、生成物を上書きしない |
| 新規/追記のローカルリンク | 対象103件すべて解決 | 限定parser、external URLと既存歴史リンクは別範囲 |
| 標準docs checker | 947 failures → 947 failures | 正確なfailure多重集合が一致、追加0 |
| 標準RFC-v2 checker | 7 failures → 7 failures | 同じ歴史source不一致、追加0 |
| 標準ADR-v2 checker | 86 errors → 86 errors | 正確なerror多重集合が一致、追加0 |

モデル実行は2026-11-17 21:31:12 JST。現在のRFC SHA-256は4dac02f381f65d467cfad39d3bd061093eafd5c949895f5f24b93d96fd301875、script SHA-256は8038b9e3be5f77ce26751c7695353ad6a490a2386dd5c5c69888efdb4d0ba5c6で、保存結果と一致する。

outer CPN1のcodec・SHA-256・nonce・selector・chunk木は記述した構成で実行する。元traceのMerkle木と局所演算は合成整数fixtureであり、nativeのLLM kernelではない。native署名・役務資格・選出・資本/料金/責任・clock・全工程plannerは明示的な外部前提である。

全員が虚偽の完全承認を出し、sample外の不正に有効な反証が届かない場合、nonceだけでは誤ったFinalを防げない反例も検査した。同じモデルで有効な反証を投入すると取消へ進む。この負の保証を「独立全体再実行が証明された」というPASSへ読み替えない。

標準docs checkerのmissing-doc-fileは971、missing-headingは2で前後同数。古いknown-issues baseline上の「新規930件」も入力時点から存在しており、今回の増分ではない。947 failuresにはリンク以外の失敗も含むため、missing target数と同じ指標ではない。旧baselineや歴史sourceを書き換えて合格化していない。今回の保全PASSは既存全checkerのPASSを意味しない。

## 再検査

repository rootで保存結果を上書きせず実行する。

~~~sh
python3 docs/maintenance/revisions/2026-11-17-court-nonce/check_revision.py --stdout-only
python3 docs/maintenance/revisions/2026-11-17-court-nonce/check_nonce_model.py --stdout-only
python3 docs/maintenance/check_docs.py
python3 docs/rfc-v2/check_integrity.py
python3 docs/adr-v2/check_integrity.py
~~~

標準checkerの前後比較はdocs-onlyの同じ範囲で実施した。repository rootのREADMEなどdocs外の入力が増えた場合、対象範囲が変わるため上記件数をそのまま適用しない。

## 未検証

nativeの全route・実鍵・資本/責任担保・DB/journal/snapshot・beacon予測不能性と偏り耐性・全対象index/codecの完全性・最低NodeのCPU/RAM/I/O/混合負荷・実モデル1万/5万registry・Studioから完全承認/単独反証/Final/権利消費・実時間1 BPSは未検証。

nonceと局所資料から各Panel本人の物理的な全体再実行・非共謀・外注不在を証明したとはしない。公開された正しい資料の合法共有を拒否するための試験にもしていない。
