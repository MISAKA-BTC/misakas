# ノードを動かす

対象networkとreleaseを先に確認し、同じreleaseのbinaryとCLIを用います。`--help`、Params/fence/fingerprint、nodeが示すchain identityを照合してください。異なるnetworkのデータやbondを取り違えません。

[testnet-12の詳細手順](../operations/mining/testnet12-join-mining.md)は当時の資料です。現在の稼働確認ではありません。起動値や接続先は対象releaseで再確認します。RPCを不要に外部公開しません。

[liveness probe](../operations/node/node-liveness-probe.md) / [hardening](../security/public-deployment-hardening.md) / [状態](../STATUS.md)。

[旧testnet-11起動手順](../history/documentation/2026-10-17/getting-started/run-node.md)は履歴へ分離しました。旧BFT validatorの稼働を設計変更だけで停止する指示ではありません。
