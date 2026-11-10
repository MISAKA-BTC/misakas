# ADR-0219 — 受付前 tokenizer 資料の有料 native 登録案

Status: **Proposed / Review Required / NOT ACCEPTED**  
Date: 2026-11-11  
RFC: [RFC-0051](../rfc/0051-funded-preclaim-tokenizer-registration.md)

## 問題

公開 T12 で PALWART3 binding を受理した後も、対象 TFP1 候補の quote は FpTokenizer で拒否される。class semantics と raw EOF の先行受理に加え、期限内 Batch に用いる検査済み tokenizer row の production 入力経路がない。旧 EOF は raw を保持せず、既存 family raw の取得には先に claim の受付が必要になる。

このため新しい公開入力・保持・費用契約を、単なる fence 変更や既存 tag の意味変更として導入しない。[設計凍結手続き](../governance/design-freeze.md) に従う提案とする。RFC-0026 §11.3 の tokenizer 記述は実装候補であり、その Draft を本 ADR で暗黙に採択しない。

## 提案する判断

RFC-0051 の有限 Open/Part/Finish を別 version として追加する。先行受理済み class/semantics/binding/EOF を pin し、実手数料と容量予約に基づいて raw を保持する。全 parser/rendering/EOG 検査を行った native Finish だけが immutable registration row を作る。opener は資料搬送だけを認証し、class 登録者の権限を得ない。

料金の分解、重複処理、期限切れ、reorg/restart/IBD を同じ状態遷移で扱う。新 policy の数値、公開 activation、元候補の担保不足の解消は本判断の対象外であり、別の完了条件として残す。

登録 charge は実 Open fee から一回 burn する提案とする。native raw spool の GC は有限の DAA 期限で決め、通常の履歴・journal の保管と物理 pruning を分ける。pruning 停滞時の無期限の追加サービス保証はしない。具体的な料金・保持・復元容量は実装前の設計レビューと実測の対象とする。

## 採用理由の候補と比較

有限 raw stream は、ローカルファイルや第三者の検査申告に依存せず、登録 table との対応を各 Node が再現できる。旧 EOF の昇格、単一巨大 object、keeper receipt、claim 後の経路の前借りでは、この入力・責任・資源契約を満たさない。比較と不採用理由の候補は RFC-0051 §7 に記録する。

## 不変条件と置換関係

モデル品質審査、tier 更新、モデルの資格審査を追加しない。暗号学的な計算証明を導入しない。旧 EOF、旧 claim/署名、claim 単位の検証と認証、元の notice/hard deadline、報酬・bond 制度を維持する。既存 ADR の採択事項の削除・置換は提案しない。追加する範囲は tokenizer 資料登録の入力・保持・費用契約だけである。

## レビューと承認

- 独立レビュー: 2026-11-11、既存担当エージェントによる三回の静的レビューを実施。署名 domain・資格、Open/Finish parent、競合 key、料金の二重計上、保持期限・pruning、同時容量、旧 parser との比較を修正し、料金帰属、pruning 停滞、Finish の再試行も明記した。最終レビューでは採択判断を妨げる重大矛盾なし。追加指摘の「不正・重複 Finish も候補 block の parser 前の総 work 制限へ数える」を RFC に反映した。これは採択や runtime の検証ではない。
- ユーザー／設計責任者の採択: 未取得。
- 数値 policy と公開 activation: 未承認。
- runtime 実装・実測・公開受理: 本案について未実施。

採択前のドキュメント merge は合意規則の変更ではない。採択後も、実装・回帰・測定・移行・activation 承認がそろうまで公開 preset を変更しない。
