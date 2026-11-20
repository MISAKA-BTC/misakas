# PALW-AN1 — whitepaper・RFC改訂

日付: 2026-11-18（Asia/Tokyo）  
状態: **DRAFT_NOT_ACTIVATED**

## 方針と成果物

ユーザーの最新指示に従い、実入力へowner・nonceを結合するPALW-AN1へ変更した。検証方式は**Panelの完全再実行＋Nodeの局所Court**を維持する。Groth16その他の計算validity proofを追加していない。

1. [新RFC-0059](../../../rfc/0059-palw-an1-input-binding-and-panel-court.md)を主規範入口とする。
2. [ADR-0227](../../../adr/0227-palw-an1-input-binding-and-panel-court.md)に変更理由・採用しない推論・状態を記した。
3. RFC-0056 §17、0057 §17、0058 §16をAN1の受付・Court・段階別一回性へ接続した。
4. [whitepaper改訂PDF](whitepaper/PALW_AN1_whitepaper_revision_v04_20261118.pdf)、[編集用TeX](whitepaper/PALW_AN1_whitepaper_revision_v04_20261118.tex)、[本文HTML](whitepaper/PALW_AN1_whitepaper_revision_v04_20261118.html)を同梱した。
5. [独立参照実装](reference_impl/README.md)と反例検査、[検証記録](VALIDATION.md)、[変更点](CHANGES.md)、[脅威表](THREAT_MATRIX.md)、[source監査](SOURCE_AUDIT.md)を追加した。

現行RFC-v2は20冊、ADR-v2は50冊のまま。各担当冊へAN1の適用範囲付き案内を追加した。旧採択本文、原則、過去検証・digestを新案の完成証拠へ読み替えない。

## 査読で補強した点

- 同一canonical Cに別資金・別slotを与えず、登録を一意化。Finalでslot・s・実仕事を原子的に消費する。
- slot_idと候補集合のclosed rootをCへ逆流させない。job rootとs導出後のinput rootを分ける。
- tokenizerを通す文字列nonceではなく、実tokenの単射表・固定位置・全32 byteを使う。切詰めやUNKへの置換を拒否する。
- 元claimを未来Panel選出より前に固定し、正しい再実行結果を見てRootを後出しできないようにする。
- model rootの属性だけを信用せず、実bytesから認証し直す。Nodeは期待weight indexを固定命令・token・座標から導出する。
- 正しい局所Courtと全体正当性を分離し、完全再実行・有効反証・資料caseの規範を維持する。
- 同batchのnotice/資料hold・反証・取消をFinalより先に適用する。古い認証cacheで現在stateを代替しない。
- AN1入力はbeacon依存なので、旧FULL1の固定入力の費用定理を流用しない。

## 検証と未完了部分

独立実装は、小さな決定論的整数モデルを実際に実行し、input/model/program/trace/outputの認証と局所Court、簡略Finalを検査する。ユーザーの元AN1 ZIPは取得しておらず、元の11テスト・7,226レコードを再実行したものではない。今回の確定件数・model規模・限界は[VALIDATION](VALIDATION.md)と[results.json](reference_impl/results.json)を参照する。

元whitepaper v0.3のPDF/TeX原文は未取得である。今回のwhitepaperは、本会話で作成した29ページの統合補完稿からのAN1改訂であり、未取得原本の逐語差分ではない。一方、RFCは添付ZIPを実際に展開し、本文を編集した。

実署名・beacon、native runtime、cold取得、ネットワークのCourt包含、DB並行性、実LLM、最悪負荷、DAG安全性、通常LLM実費の下界は完成済みにしない。一人正直の条件は、取得・完全再実行・反証の包含と裁定を含む。

## 再実行

`current`を作業ディレクトリとして実行する。

~~~bash
python3 docs/maintenance/revisions/2026-11-18-an1/reference_impl/check_an1.py
python3 docs/maintenance/revisions/2026-11-18-an1/check_revision.py
python3 docs/maintenance/check_docs.py
python3 docs/rfc-v2/check_integrity.py
python3 docs/adr-v2/check_integrity.py
~~~

最後の三つは添付入力時点で既存失敗を持つ。元失敗とAN1変更による回帰を分けて報告し、公式baselineを失敗隠しのため更新しない。文書検査の成功はprotocol安全性や性能の証明ではない。

whitepaperの再組版は`whitepaper`内で`python3 build_an1_report.py`を実行する。ReportLab経由のPDF、MathML HTML、編集用TeXを生成する。TeXの直接コンパイルは行っていない。
