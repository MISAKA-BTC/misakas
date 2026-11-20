# Studio claim の隔離 T12 観測

2026-11-20、`codex/public-one-honest-vertical` の `3f117d246` から作った Linux ノードで、登録済み DAA 317 のチェックポイントを再開した。3 ピアは同一ホスト上の独立プロセスで、専用 genesis と履歴時計を使った。ノード実行ファイルの SHA-256 は `a954b1ae693a3ea4ed9a337d61c6d68d43c0679a9cf25ae475956f1c6bed621b`。追加統合した AN1・CAD1・CPN1・SWC1 は未採択案として保持し、この実行には既存の protected TFO3 受付を用いた。

## 受付

- Studio が登録済みモデルを検出し、prompt の実行で 85,938 work leaves を記録した。[Studio 応答](studio-run.json)
- Studio job と署名済み rail 取引を claim ID、output root、実行 root で対応づけた。`studio-evidence.json` は照合に使う項目の抜粋。[Studio と rail の対応](studio-evidence.json)
- ノードは carrier 取引 `3a989cfc…a54f08de` を、ブロック `62182f1b…66303ebd` から選択鎖ブロック `c643963c…bb93873` の ordinal 1 で受理した。DAA 318、blue score 638。受付時点の phase は `provisional`、Gate C は `pending`、21 量子の消費数は 0。完全な識別子、両ヘッダー、取引本体は RPC 観測に記録した。[受付観測](original.json)

## Panel 承認

DAA 323 で抽選された seat 7 が、実行者の seat 0 とは別のピアで 85,938 leaves を全再実行し、元の実行 root と回答の一致を確認した。Panel は署名済み取引 `50fbc448…e43498bc` を送信した。[Panel の再実行ログ](panel-worker.log)

この取引は carrier block `00d7bd33…8dc115` に入り、選択鎖ブロック `db95051c…ecccef` の ordinal 1、DAA 325 で受理された。元の受付と claim ID・受付ブロック・元実行 ID が一致し、ネイティブ状態の `fullReplayApprovalRecorded` は `true`。この観測時点で phase は `receipt_licensed`、Gate C は `pending`、消費済み量子は 0。通知期限は DAA 3438、異議申立て延長後の最終期限は DAA 10318。[Panel 承認の包含と状態](panel-approval.json)

## 適用した RFC

この claim は RFC-0054 の protected TFO3 として受理された。以後の Panel、異議、Final、Gate C と receipt quantum は受付時の識別子と期限で追跡する。RFC-0054 は同一 claim の権利消費と対応ブロック受理までを通し確認に含める。[RFC-0054](../../../rfc/0054-public-t12-original-admission-profile.md)

追加マージの CAD1 ゲートは既存 TFO3 に新しい受付 Ticket を要求できる構造だった。また SWC1 用の決済変換は既存の work identity の先頭 32 byte を OriginalId として使っていた。RFC-0058 §2.4 の OriginalId は専用の入力から導出する。両者の native 接続を追加マージ前の実装に戻した。AN1 と MOR1 の規範は新しい family／profile に適用する。分離した草案モジュールはこの実行の合意経路に入らない。[RFC-0058](../../../rfc/0058-staged-work-commitments-and-consensus-eligibility.md)・[RFC-0059 §0](../../../rfc/0059-palw-an1-input-binding-and-panel-court.md)・[RFC-0060 §0](../../../rfc/0060-model-registration-and-attempt-output-binding.md)

## 回帰確認

追加マージ後の `kaspa-consensus-core --lib` は 5,946 件成功、126 件失敗、22 件 ignore。失敗した126件を追加マージ直前の `f721bc681` で個別に再実行し、全件で失敗を再現した。公開設定・DAA・認証を対象にした選択テスト205件は成功した。

受付済みの TFO3 から異議申立て側が PublicDA の入力を取得する修正（`875d1f6aa`）は、上記の実取引の公開 prompt ID 17 個を使って検査した。対象の Rust テスト8件と、同じ claim の再開・観測を含む Python テスト54件が成功した。関連する履歴読み取りテスト全15件では1件が DAA 100 の状態再読込比較で失敗し、この失敗も修正前のバイナリで再現した。

PublicDA 入力をメタデータだけで先読みした場合に、検証用の空き枠で受付済み carrier の本文を再取得する修正（`d16e45988`）を確認した。公開異議の Rust テスト135件が成功した。さらに、実際の受付済み TFO3 の prompt ID 17個と登録時の artifact root に一致するモデルを使い、ローカルで1トークンを再実行した。署名済み job と TIR ヘッダーの job pin が一致し、実行・trace・出力の3つの root がすべて一致した。Panel の署名と包含点は上の「Panel 承認」に記録している。[ローカル再実行出力](accepted-original-local-replay.log)

## 同一 claim の再開

3 ピアを停止して各 DB を保存し、異議申立て側の公開入力読み取りを修正したノードで各自の DB を再開した。各ピアの RPC で元の受付の全識別子と、抽選された seat 7 の承認取引・包含点を照合した。DAA 374 で3ピアとも `receipt_licensed`、Gate C は `pending`、消費済みは 0/21。未応答の DA session は 0 件、存続している公開異議の予約は 7 件だった。[再開後のピア別観測](restart-peers.json)
