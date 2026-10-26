# M-OPS — public t12 read-only observation (2026-10-26)

`observe-public-wrpc.mjs` requested `getBlockDagInfo` and then `getBlock` for the returned sink over the public JSON wRPC endpoint. The complete request, response, UTC timestamps, command, source commit, Node binary SHA-256, machine identity, and pre/post load are in [public-wrpc-2026-10-26.json](public-wrpc-2026-10-26.json). Exit code: 0. No local or public node was started, stopped, changed, or contacted over SSH.
The raw JSON SHA-256 is `d7890d4f21f973da672bc2b406a1713c7dc9a7d24a17f82ef2eb51ebf8798e86`.

The endpoint reported network `testnet-12`, virtual DAA **15,850**, block count **15,850**, and a sink block at DAA 15,850 with three transactions, including one coinbase-subnetwork transaction with one output of 779,860 sompi. This is **one public observation**, not a producer-income sample. It cannot establish a node's CPU, RAM, power, bandwidth, hosting cost, uptime, bond capital cost, fees over time, or net profitability. Those parts of RFC-0027 remain **NOT_MET**; a participating local public-network node requires separate user approval under the measurement instructions.
