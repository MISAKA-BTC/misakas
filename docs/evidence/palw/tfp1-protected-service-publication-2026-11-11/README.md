# 保護予約のnative保存

2026-11-11。基点 `f492965ba`。**実装中・Rust検証未実行**。

## 実装

TFO3 pairingを持つ実foldは、BCT2の設定とは独立に、明示的なnative parent copyの所有枠を取る。既存のpaired checkpointを共通化し、その処理中だけ対象claimを保持する。原受理prefixとこのclaimを、失敗時・成功時とも呼出し前の値へ戻す。

保護予約の結果は、成功時のサービス表全体にallocation-freeの内部digestを付ける。同じpacket graphでも、slot、capacity、既存供給履歴などを変更した別表を保存候補へ渡せない。これはtransient ownerの封印であり、新しいwire IDや供給済み認定ではない。

専用frameとsealed bookingから、全役割・全順位のpacket集合と因果辺を含む `CourtServiceRowV3` 候補を組み立てる。既存のservice profile、live binding、解放履歴、因果辺は維持する。旧bookがない場合のprofile解決は別工程であり、このfactoryは明示的に拒否する。bookとmetadataのメモリ枠を保存処理まで維持する。

専用のサービス保存入口は、同じpaired checkpointと変更されていない旧bookを要求する。専用frameと198/204/206が直前の連続した4挿入であること、live prefixと履歴が一致することを検査する。LastPointがまだ親を指す受付中でも、実受付点で206→204→198を照合する。既存のjournal writerを呼ぶ前に、旧行コピー・検査用索引・終端claim配列の作業領域を予約する。旧のconviction fenceを迂回して新profileを有効化しない。

## 残る接続

完全なnetwork/profile resolverとG14の全工程plannerから、この候補と保存入口を呼び出す本番coordinatorは未接続。初回資料量、完全fee、非Batch義務の予約、finite settlement/release、FpTokenizer route、RPC/Studioからの正常受付が残る。保存部品とfixtureを、原子的な本番受付全体の完成とは扱わない。snapshotとdeltaにおける専用frameとサービス予約の必須同時保存の検査も、本番有効化前に接続する。

公開T12への配備・quote・署名送信は行っていない。同一claimのPanel・Final・当選権利消費・対応ブロック生成も未証明。モデル取得証明、可用性審査、モデル重み全量配信は追加しない。

## 検証

実署名とUTXO handoffを用いる保存fixtureに、共通checkpoint内でclaim・escrow・専用frame・履歴を書いた後にサービス保存の既存fenceで拒否されるケースを追加した。全state/root/journalの巻き戻し、成功時のサービスbindingとdelta適用・逆適用を検査するコードであり、実行結果は未取得（WRITTEN_NOT_RUN）。モデル取得証明や完全な本番受付を代行するfixtureではない。

採択済み[実装計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md) §1に従いCargo check/build/testは縦接続完成後。静的レビュー、整形、差分確認と文書検査の結果を [validation.json](validation.json) に記録する。
