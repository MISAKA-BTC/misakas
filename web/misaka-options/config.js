// MISAKA Options - site configuration.
// Plain static file: edit the values and reload. No build step.
window.MISAKA_CONFIG = {
  // Kaspa-style wRPC endpoint (JSON over WebSocket). Leave empty to derive
  // "(wss|ws)://<this host>/kaspa" from the page's own origin, which is what the
  // nginx snippet in README.md proxies to a kaspad --rpclisten-json listener.
  // The public explorer's node answers wRPC JSON over WebSocket; the browser connects to it
  // directly (a WebSocket is not subject to CORS). Set "" to derive "/kaspa" on this origin.
  WRPC_URL: "wss://misakascan.com/kaspa",

  // EVM JSON-RPC endpoint (standard eth_* over HTTP POST). "/evm" is a same-origin
  // path nginx proxies to a node's port 8545. This URL is also handed to the wallet
  // by wallet_addEthereumChain, so it must be reachable from the user's browser.
  EVM_RPC_URL: "/evm",

  // The MISAKA EVM lane's chain id (0x4D534B spells "MSK"; frozen in ADR-0020).
  CHAIN_ID: "0x4D534B",

  // Shown in the network pill and in copy.
  NETWORK_NAME: "testnet-11",

  // Fallback class list (128-hex class ids) used to find lines when the EVM RPC is
  // not reachable or the market fence is dormant. The founding line of a class has
  // the class id as its line id. Known testnet-11 class ids are listed in README.md.
  CLASS_IDS: [
    "4277d84f7d91528cc04aa366d51ee1c2e4f7902c4f6b16a213dead1c7e227977db732f18ed6183db3d944d44726ebd3feff7b15c48f9dba11cd526684f35f1b7", // Qwen2.5 A16 (graph-v5)
    "5bd9ae3d91df80650caffe3126a38bafb0b4feb9b046a416d353a7c3f71af6eab5aadf9b1ce41650007a980f1cc6044ef218424f4cbb8299ef9e92c97b99ef8e", // Qwen3.6-35B-A3B (graph-v3)
  ],

  // The explorer, linked from ids and hashes.
  EXPLORER_URL: "https://misakascan.com",

  // Optional tuning.
  POLL_MS: 10000,             // trade page refresh cadence (price samples for the chart)
  LOG_LOOKBACK_BLOCKS: 5000,  // eth_getLogs window for settlement events (node cap: 10000)
  ADR_URL: "https://github.com/MISAKA-BTC/misakas/tree/main/docs/adr"
};
