// MISAKA Options - site configuration.
// Plain static file: edit the values and reload. No build step.
window.MISAKA_CONFIG = {
  // Kaspa-style wRPC endpoint (JSON over WebSocket). Leave empty to derive
  // "(wss|ws)://<this host>/kaspa" from the page's own origin, which is what the
  // nginx snippet in README.md proxies to a kaspad --rpclisten-json listener.
  // The public explorer's node answers wRPC JSON over WebSocket; the browser connects to it
  // directly (a WebSocket is not subject to CORS). Set "" to derive "/kaspa" on this origin.
  WRPC_URL: "wss://misakascan.com/kaspa",

  // EVM JSON-RPC endpoint (standard eth_* over HTTP POST). The explorer host's testnet-12
  // node serves it at https://misakascan.com/evm (CORS-open, rate-limited); "/evm" would be a
  // same-origin path for a node on this host instead. This URL is also handed to the wallet by
  // wallet_addEthereumChain, so it must be https and reachable from the user's browser.
  EVM_RPC_URL: "https://misakascan.com/evm",

  // The MISAKA EVM lane's chain id (0x4D534B spells "MSK"; frozen in ADR-0020).
  CHAIN_ID: "0x4D534B",

  // Shown in the network pill and in copy.
  NETWORK_NAME: "testnet-12",

  // Fallback class list (128-hex class ids) used to find lines when the EVM RPC is
  // not reachable or the market fence is dormant. The founding line of a class has
  // the class id as its line id. Known testnet-11 class ids are listed in README.md.
  CLASS_IDS: [
    "ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01", // Qwen2.5-1.5B graph-v7 @8192
    "74c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da409070cb2c98b8e861598db902f7a", // Qwen2.5-1.5B graph-v7 @2097152
    "f1c5635c6e47e96e7af864789c94523335dc56584af297cb8cc19021c228b897bee1a50145597e45f8ca2727349bf4aa352a98cc05274b7f059a176642f623c8", // PALW-BASE-0 (the floor)
  ],

  // **What each registered class actually IS, for a reader who sees an id.**
  //
  // A line's name is a chain fact — `getPalwModelLine` serves it — but only from a node built
  // after ADR-0088, and testnet-11's public node is older than that, so on the live site every
  // name comes back empty and a row reads as its own id twice. This catalogue is the fallback,
  // and the site LABELS it as one: a title from here is this site's word, never the chain's.
  // Keyed by class id (a class's founding line has the class's id) or by any line id.
  // testnet-12's genesis classes (docs/testnet-12-regenesis-2026-09-23.md, and getPalwClasses).
  MODELS: {
    "ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01": {
      title: "Qwen/Qwen2.5-1.5B-Instruct",
      variant: "A16 · graph-v7 · n_ctx 8192",
      params: "1.5B",
      hf: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct",
      artifact: "converted locally with qwen25-convert --a16 — inventory root 88096dc1…",
    },
    "74c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da409070cb2c98b8e861598db902f7a": {
      title: "Qwen/Qwen2.5-1.5B-Instruct",
      variant: "A16 · graph-v7 · n_ctx 2,097,152 (held context)",
      params: "1.5B",
      hf: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct",
      artifact: "2.4 GiB, converted locally with qwen25-convert --a16 — inventory root f63af2c4…",
    },
    "f1c5635c6e47e96e7af864789c94523335dc56584af297cb8cc19021c228b897bee1a50145597e45f8ca2727349bf4aa352a98cc05274b7f059a176642f623c8": {
      title: "PALW-BASE-0",
      variant: "the floor class · deterministic integer model, pure Rust in the node",
      params: "",
      hf: "",
      artifact: "none needed — built into kaspad",
    },
  },

  // The explorer, linked from ids and hashes.
  EXPLORER_URL: "https://misakascan.com",

  // Optional tuning.
  POLL_MS: 10000,             // store page refresh cadence (price samples for the chart)
  LOG_LOOKBACK_BLOCKS: 5000,  // eth_getLogs window for settlement events (node cap: 10000)
  ADR_URL: "https://github.com/MISAKA-BTC/misakas/tree/main/docs/adr",
  // The runbooks the "Add model" page links (register a bond, register a class, certify).
  DOCS_URL: "https://github.com/MISAKA-BTC/misakas/tree/main/docs"
};
