# 2026-11-26 MWA1設計追加記録

状態: PROPOSED_DESIGN_NOT_ACTIVATED。入力は `LLM-chain-MOE3-20261126(1).zip` とユーザー提示のMWA1説明。既存のMOE3/BMR1採択を変更せず、旧MOE2やgenesis特例を復活させない。今回追加する [RFC0070](../../../rfc/0070-model-work-accounting-and-exec-credit.md)／[ADR0238](../../../adr/0238-model-work-accounting-and-exec-credit.md) は共通計算量・FOW1報酬・EXEC生成権の設計案。[詳細](../../../design/palw/economics/mwa1-implementation.md) と [機械可読案](../../../design/palw/economics/mwa1-policy-proposal.json) を含む。

採番は提供treeの未使用を確認した範囲。旧主文書、runtime、署名・wire、合意設定、過去evidence、既知link baselineを保持する。変更は文書、案のJSON、索引生成、参考算術試験に限定する。

確認コマンド（repo root）:

```sh
python3 docs/maintenance/revisions/2026-11-26-mwa1/check_reference.py
python3 docs/maintenance/generate_indexes.py
python3 docs/maintenance/check_docs.py
```

参考算術試験8件は終了コード0。共通target境界、整数quote、期待原資比、最大予約、結合按分、MOE3購入、累積49/51、EXEC端数の期待値を確認。GPU/LLM、native consensus、BFT投票、devnet、1 BPS、600秒確定の試験は未実行。具体的なモデル別CCUと報酬単価、公開target、動的retarget、EXEC量子は未確定。既存の文書参照切れはbaselineを変更して隠さない。

文書検査実測: 提供treeは1061 Markdown、既知baseline外1343 link issue occurrencesで終了1。更新後は1065 Markdown、同1343 occurrencesで終了1。報告されたissue行のmultiset差分（増加・減少）はともに0。既存baselineを変更していない。索引生成は305主文書（ADR234、RFC71。既存重複番号は元のまま）を収録した。これらは文書の構造・リンク検査で、規範実装や安全性の検証ではない。
