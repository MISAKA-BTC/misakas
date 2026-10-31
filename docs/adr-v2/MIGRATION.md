# 導入・更新・変更承認

## このパッケージの適用範囲

`misakas-pre(5).zip`と、その文書から作られた`misakas-rfc-v2-15.zip`を基準にした文書overlayです。現行ADRの閲覧・整理先を50本へまとめ、既存15 RFCは変更しません。全repositoryや新しいmainnet候補のバイナリを同梱したものではありません。

同じsnapshotへ適用する場合は、repositoryに新しい`docs/adr-v2/`を追加し、旧ADRのREADME／INDEX／TOPICSに付けたナビゲーションだけを取り込みます。新RFC-v2がまだなければ同梱の`docs/rfc-v2/`も取り込みます。**更新の進んだbranchへ`docs/`全体を無条件に上書きしないでください。** 元資料の変更がある場合、`source-manifest.json`のhashとの不一致を確認し、差分をmergeしてから再生成します。

rootのREADME・PACKAGE-MANIFESTはこの配布物の説明です。repositoryのroot READMEを置き換える指示ではありません。旧navigationの原文は`docs/history/documentation/2026-10-30-adr-consolidation/`に退避しています。

## 参照とCI

新文書は必ず`ADR-v2-0001`のようにnamespaceを含めて参照します。旧`ADR-0001`の意味やcode comment、wire ID、署名domain、既存ADR/RFC本文の番号は一括置換しません。旧204本はファイル名・bytesを固定した履歴です。

現行一覧を使う新ツールは`docs/adr-v2/registry.json`の`documents`を読みます。`docs/adr/*.md`を数える既存CIや全体lintはこのパッケージで改修していません。legacyファイル数を50にするために履歴を消すのではなく、目的に応じてcurrentとlegacyの一覧を分けてください。

## 採択状態を変えない整理

新50本の状態は`EDITORIAL_CONSOLIDATION_SOURCE_STATUS_INHERITED`です。原文のAccepted／Proposed／Withdrawn／Deferred、隔離実装承認、本番schema採択、測定承認、有効化、配備は別々に保ちます。意味が衝突する箇所は既存の明示改定の範囲でのみ整理し、新しい決定を推測して作りません。

通常の誤字訂正・参照整備・説明の改善は新50本の該当文書で行います。原条文の履歴欄と旧原文は変更しません。採択済み判断の変更は旧ADR-0181とRFC-v2-0015の手続きに従い、必要な新RFC・新ADR・レビュー・明示採択を先に記録して、その後に主題別入口へ反映します。**「現在の50本」を維持するために、新規判断の作成義務を省略したり、既存判断を履歴なしで上書きしたりしません。** 新規判断を追加した時点ではmanifestのsource数も更新対象です。

## 内容の分担

仕様の正本は新15 RFC、判断の根拠・理由・却下案の入口は新50 ADRです。CourtはRFC-v2-0014を規範入口にし、ADR-v2-0038〜0047へ理由を分類します。TIRの形式・算術はRFC-v2-0002、Courtの期限・義務は0014、共通会計は0011です。

年次カタログの1.0／追加登録0.3、G14のモデル取得前提、モデル計算の暗号学的証明を使わない方針は、今回変更していません。private modelに関する会話での検討を、新たなprotocol採択として埋め込んでいません。

## 検査

```sh
python3 docs/adr-v2/check_integrity.py
python3 docs/adr-v2/test_integrity.py
python3 docs/rfc-v2/check_integrity.py
```

第一の検査は50本のregistry、204原文のbytes・全行、重複所有と欠落、統合欄の原文、旧番号衝突、既存RFC等の不変性、新しく書いたリンクを検査します。第二は欠落・改変・重複を検査器が検知する回帰試験です。第三は先行RFC整理の検査器です。

これは文書構造の検査であり、各設計の正しさ・採択権限・Rust実装・公開networkの安全性やmainnet readinessを保証するものではありません。原条文中の全ての履歴リンク・証拠file・runtimeコードの存在もこの小型パッケージでは検証対象外です。元repositoryを参照するリンクを残します。
