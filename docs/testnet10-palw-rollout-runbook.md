# 旧 testnet-10 inference PoW rollout（不採用）

algo 4/5 の pinned worker・Ollama 推論を直接 PoW にする方式は使用しない。出力 commitment を実際の推論なしに偽造できるため（[ADR-0021](adr/0021-palw-llm-pow.md)）。旧 driver・node/miner の起動経路・専用 rollout script は削除した。

現行参加方法は [testnet-11 operator runbook](testnet11-node-operator.md) とネットワークの実設定を参照する。
