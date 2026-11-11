# 専用TFP1の予約同時保存と復元検査

2026-11-11。基点 `b8499182b`。**実装中・Rust検証未実行**。

## 接続した検査

専用frameの最終状態にはV3サービス表と、同じclaim・frame identity・Source・原受理DAA・hardを持つliveまたはreleased bindingを要求する。共通表の構造を専用frame全体で一回検査し、観測時計がnative状態点を超えることを拒否する。releasedの時計も照合するが、これはサービス会計であり、claimの成功終端・資金決済の権限ではない。

新frameを使うaux-204 version 2のdeltaは、frame・198・204・206・CourtServiceV3の5件連続の保存を必要とする。forwardとinverseは同じ正方向の旧新行を検査する。サービス保存は対象bindingを一件追加し、既存のlive/released binding、packet、割当、供給履歴、因果辺、profileを維持する。原受理時点より未来の観測時計を用いた挿入も拒否する。既存のaux-204 version 1の3行履歴は変更しない。

サービス行だけのdelta変更でも専用frameとの整合性を再検査する。受付の5行直後だけを同時保存として検査するので、その後に同ブロック内の通常サービス観測が続くことを妨げない。検査用の一時索引はnativeの所有枠を予約してから構築する。

## 回帰コード

旧4行fixtureは中間writerの検査へ限定し、最終状態としては拒否する。実署名・実UTXO handoffを使う5行fixtureに、snapshot保存・復元、実pairingの再読込、delta適用・逆適用、各行の欠落、原履歴prefixの自己包含、予約欠落・空binding・別Source・未来時計、claim/class/旧frameの差替え、後続サービス観測を追加した。**WRITTEN_NOT_RUN**であり、通過を報告していない。

## 残る仕事

この変更で完全な予約グラフの認証まで済んだとは扱わない。snapshotで同じscopeを保ったままpacket/cost/window/因果辺を差し替えることを排除するには、claim単位のcanonical予約内容とframeの束縛、およびrelease時の保存規約を本番plannerと同時に完成させる必要がある。全役割のgraphを実plannerから導出する処理、完全fee・資源・期限、初回service profile解決、本番受付coordinatorと有限settlement/release、FpTokenizer routeも未完。

公開配備・quote・署名送信は行っていない。同一Studio claimのPanel・Final・当選権利消費・対応block生成は未証明。モデル取得証明は実装しない。

採択済み[実装計画](../../../design/palw/integration/public-one-honest-vertical-implementation-plan-2026-11-10.md) §1に従いCargo check/build/testは縦接続完成後とする。静的レビュー・整形・差分確認・文書検査を [validation.json](validation.json) に記録する。
