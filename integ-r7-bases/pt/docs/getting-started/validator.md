# BFT validator — 旧資料の案内

新しい採用設計はBFT validatorを使用しません。[ADR-0180](../adr/0180-palw-only-consensus-no-bft-validators.md) / [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md)。

[旧DNS-finality validator runbook](../history/operations/dns-validator/validator-runbook.md)と[旧導入手順](../history/documentation/2026-10-17/getting-started/validator.md)を保持しています。これらは新規導入の推奨ではありません。

設計退役と稼働network上の退役は別です。移行・有効化を確認せず既存nodeや債務を停止・解除する指示ではありません。claimごとの抽選検証者は固定BFT集合と別です。[状態](../STATUS.md)。
