# 入力資料と監査境界

## 入力

- 添付名: `misaka_docs_latest.zip`
- 入力ZIPのSHA-256: `e5e6a3f89b3e1adcdb6b621fb77a7e12b662b6c0107eeecbb65c2ad122d7a6f2`
- ZIP bytes: 19,783,374
- 通常ファイル: 2,606件
- 展開後bytes合計: 60,733,352
- 絶対パス、親ディレクトリへの逸脱、重複パス、symlinkは検出されなかった。

この添付を今回の変更基準とした。過去の別ZIP、白書PDF、旧toy実装のbyte定義を最新AN1へ混入させない。

## 設計照合

1. docsの変更規則とfreeze方針を確認し、新RFC・ADRを草案として追加した。
2. RFC-v2-0003／ADR-v2-0015に基づき、共通の技術適合と廃止済みの裁量的資格審査を分離した。
3. RFC-0053によるburn停止・解除が、それ以前の取得可否認定を置き換えたことを確認した。
4. RFC-0059／ADR-0227の完全C、RegKey、slot、s、実token、Ticket、署名、Final、一回会計に接続した。
5. 新rhoを既存Ryや同じRtauへ戻さず、元claimの署名からreceiptへも戻さない順序を確認した。
6. 非重複性の定理を完全contextの単射符号化とhashの確率論証に限定し、LLM実費の下界と分離した。

## 検証の意味

文書checkerは入力時点ですでに失敗を報告した。そのため、更新前後のエラー文字列と重複回数を比較する。元のbaselineを変更して失敗を消した扱いにしない。

参照scriptはreceiptの型・長さ・完全context束縛とbirthday上界の算術を検査する小さな例である。実モデル、native codec、Panel/Court、DAG、経済的費用を実装・実測した証拠ではない。詳細は[検証結果](VALIDATION.md)に記録する。
