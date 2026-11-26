# モデル追加はBFTのみ

規範：[RFC-0007](../rfc/0007-bft-model-admission.md)。kernel解析・lowering・資源推定は技術準備。全自動PASS、期限切れ、fallbackによる採用はない。新規登録と市場作成は正規BFT決定に基づき原子的に行う。既受理claimの元署名・元期限は変更しない。

採択・実装・検証・公開有効化は別とし、今回のdocs更新で公開設定を変更しない。
