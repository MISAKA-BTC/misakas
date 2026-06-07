// Network presets. The same WASM core serves every network; only these change.
//
// `networkId` is the exact id passed to KaspaPqKeyPair.fromMnemonic / RpcClient.
// `prefix` is the bech32m address HRP and is validated against the recipient before signing.

export const NETWORKS = {
  "testnet-10": {
    id: "testnet-10",
    label: "Testnet",
    prefix: "misakatest",
    symbol: "TMSK",
    // Public testnet node (nginx wRPC-JSON → kaspad :28610). Override in Settings if self-hosting.
    rpc: "wss://misakascan.com/kaspa",
    explorer: "https://misakascan.com",
  },
  mainnet: {
    id: "mainnet",
    label: "Mainnet",
    prefix: "misaka",
    symbol: "MSK",
    // Mainnet is defined but NOT launched yet — set your own node endpoint in Settings.
    rpc: "",
    explorer: "https://misakascan.com",
  },
};

export const DEFAULT_NETWORK = "testnet-10";

export function netConfig(networkId) {
  return NETWORKS[networkId] || NETWORKS[DEFAULT_NETWORK];
}
