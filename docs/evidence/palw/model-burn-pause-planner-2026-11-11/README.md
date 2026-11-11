# 追加モデルburn停止planner — 2026-11-11

[RFC-0053](../../../rfc/0053-additional-model-burn-pause.md)／[実装追補](../../../design/palw/integration/additional-model-burn-pause-2026-11-11.md)。commit `6511a880e`。

coreの未署名plannerを実装し、対象2テストが成功した。[raw log](tests.log)、[source SHA](sources.sha256)、[binary SHA](binary.sha256)、[検証範囲](summary.json)を保存した。既存shared target・opt-level 0・incremental無効のcore lib testを使用。build 2分59秒、テスト0.05秒。

確認したのは、正規class/artifactと登録者、Pause/Resumeの必要認証・異なるburn条件、old sequence/no-op/overflow/別scope/root/登録前DAAの拒否、登録者不明のfail-closed、実genesis objectとのexact root、genesis catalogと年次指定による除外である。既存Frozen制裁は変更せず、年次指定後は既存PAUSED rowを稼働gateへ適用しない。

署名要求の導出と実署名の検証は異なる。本試験は支払・burn・native書込み・reorg・受付/Final/reward gate・公開有効化を検証していない。テストのburn値10/20はfixtureであり、公開金額の提案・採択ではない。直前canonical操作IDを含む署名、carrierの実資金支出との原子的接続は次工程である。
