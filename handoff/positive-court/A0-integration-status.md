# A0 integration checkpoint

2026-10-31。workspace `/Users/wata/.codex/worktrees/positive-court-a0/MISAKA-testnet`、branch `codex/positive-court-a0`。RFC-0044 / ADR-0212の実装調整。A1〜A7への直接連絡はユーザーが明示許可した。元checkoutや各担当のdirty作業を一括コピーしない。

| 担当 | A0へ統合した範囲 | 未完了・保留 |
| --- | --- | --- |
| A0 | v2/r1共通型、ML-DSA署名scope、TIR interchange、frame作者認証、非循環funding scope、176/151/P予約、固定row/typed reader `327393e8e` | native live event・writer、実資金lock、P実draw、全consumer・復旧との結合 |
| A1 | exact checker、PreparedCheck/NodeChecker、全計画・selector・provenance pin、signed ingressと費用計上 | 実LLMの未commit i64/i128 laneへ対応する別derivation、native viewからの登録期待値と資源予約 |
| A2 | 初期r1通常推論・有限原資料応答 `6cf9ee042` | 追加`1b8e4e5c9`は未保護journal tailでquotaを割り引ける反例により保留。修正監査後にA3のstatus_checks追従と一体統合 |
| A3 | 実PanelBuilder、r1 witness/frame、G14 cold資料、A1連結、acyclic funding互換 `fe97a716e` | A2修正版への追従、native受付・実LLM測定 |
| A4 | 共通portと要求条件の調整 | bounded176 body・受付coordinatorは担当作業中。旧prototype全体をnative実装として取り込まない |
| A5 | leaf会計と実A1 terminal/旧escrow checkerへのnode bridge `acc22a5df`、workspace/lock登録 | phase3の新funding/lineage修正、native出力のlock・正規料金/recipient・P承認・全consumerの実経路 |
| A6 | aux151 schema/接続条件を共有 | node-local RocksDB/recoveryのready差分はA4旧prototype依存を確認して選択統合。typed scanner/restore接続を依頼中 |
| A7 | fixed candidateの独立監査結果を受領 | 履歴110件は現A0候補の成功ではない。最新signed ingress/meter、runner外部signal cleanup修正と再実行を依頼 |

共通契約の29テスト・Clippy成功と、各担当の別候補での成功を区別する。frameの署名は作者の認証だけ、FundingConsent署名は支払条件への同意だけ、row/cursor保存は計算検証だけで得られる権限を生成しない。新Pは全LLMで実抽選の`Certified`のみを認め、`PublicCheckCertified`・旧S/K/V・local replay単独を流用しない。

native全経路・実UTXO精算・全consumer・複数Node/reorg/IBD/pruning・実LLM cold/warm測定は未完了。公開presetの有効化は行わない。実装全体の完了や認定profileの成立を宣言しない。
