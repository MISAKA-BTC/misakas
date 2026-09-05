# MISAKA Options

A static web front-end for the MISAKA chain's **model market** (ADR-0087, ADR-0088, ADR-0089): a
Hyperliquid-style trading app for **model positions**, served at https://misakaoptions.com.

A position is bought from a line's protocol curve and sold back to it, never transferred; 5 % of
every MSK leg is burned and 1 % goes to the line's owner. This site reads the market through a node
(the Kaspa-style wRPC and the EVM's read precompiles), quotes with the chain's own curve arithmetic,
and sends trades through the EVM writer from an injected EIP-1193 wallet (MetaMask).

**Every figure on screen is an RPC reply or the ADR-0087 arithmetic applied to a market row, or a
dash.** Nothing is estimated or invented, and there is no server side: four static files.

## Files

| file | what it is |
|---|---|
| `index.html` | the shell: nav, banner, main, footer, toasts |
| `app.js` | everything else, plain ES2020 (no build): the curve arithmetic (BigInt port of `consensus/core/src/palw_model_market_v1.rs`), keccak-256 and BLAKE2b-512 (selectors, event topics, facade addresses, EVM holder ids), a wRPC client (JSON over WebSocket), an EVM JSON-RPC client with hand-rolled ABI encoding for the native doors, the wallet flow, the pages, and a canvas price chart |
| `style.css` | dark theme, teal accent, dense layout; responsive down to 380 px |
| `config.js` | the only file you need to edit: endpoints, chain id, network name, fallback class ids, explorer URL |
| `mock.js` | loaded **only** with `?mock=1`: a simulated node and wallet for demos and screenshots (see below) |
| `screenshots/` | `trade.png`, `models.png`, `portfolio.png`, taken in mock mode at 1440×900 |

No external dependencies, no CDN, no fonts, no build step. `app.js` is about 140 KB unminified.

## Configure

Edit `config.js`:

```js
window.MISAKA_CONFIG = {
  WRPC_URL: "",                 // "" = derive (wss|ws)://<this host>/kaspa from the page's origin
  EVM_RPC_URL: "/evm",          // same-origin path proxied to a node's eth JSON-RPC (port 8545)
  CHAIN_ID: "0x4D534B",         // the MISAKA EVM lane, frozen in ADR-0020
  NETWORK_NAME: "testnet-11",
  CLASS_IDS: [],                // fallback class ids (128 hex) when the registry window is not armed
  EXPLORER_URL: "https://misakascan.com",
  POLL_MS: 10000,               // trade page refresh cadence
  LOG_LOOKBACK_BLOCKS: 5000,    // eth_getLogs window for settlement events (node cap: 10000)
  ADR_URL: "https://github.com/MISAKA-BTC/misakas/tree/main/docs/adr"
};
```

`EVM_RPC_URL` is resolved against the page URL and handed to the wallet by
`wallet_addEthereumChain`, so it must be an `https://` URL (or `http://localhost`) that the user's
browser can reach; a relative path on an https site is fine.

Known class ids on testnet-11 (from the tree, `tools/palw-jobs-export/src/main.rs`), usable as
`CLASS_IDS` while the registry window is dormant:

```
4277d84f7d91528cc04aa366d51ee1c2e4f7902c4f6b16a213dead1c7e227977db732f18ed6183db3d944d44726ebd3feff7b15c48f9dba11cd526684f35f1b7  Qwen2.5 A16 (graph-v5, 512)
5bd9ae3d91df80650caffe3126a38bafb0b4feb9b046a416d353a7c3f71af6eab5aadf9b1ce41650007a980f1cc6044ef218424f4cbb8299ef9e92c97b99ef8e  Qwen3.6-35B-A3B (graph-v3)
```

A class's founding line has the class id as its line id, so each of these is also a line id
(`#/trade/<id>`, `#/line/<id>`).

## Serve

Any static file server works. The site expects two same-origin proxies so that the browser talks
to one host: `/kaspa` (WebSocket, wRPC JSON) and `/evm` (HTTP POST, eth JSON-RPC).

```nginx
server {
    listen 443 ssl http2;
    server_name misakaoptions.com;
    # ssl_certificate / ssl_certificate_key ...

    root /var/www/misaka-options;      # index.html, app.js, style.css, config.js, mock.js
    index index.html;
    location / { try_files $uri /index.html; }

    # wRPC: kaspad --rpclisten-json=127.0.0.1:18110 (JSON encoding over WebSocket)
    location /kaspa {
        proxy_pass http://127.0.0.1:18110;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_read_timeout 3600s;
    }

    # EVM JSON-RPC: the node's eth endpoint on port 8545
    location /evm {
        proxy_pass http://127.0.0.1:8545/;
        proxy_http_version 1.1;
        proxy_set_header Content-Type application/json;
        add_header Cache-Control "no-store";
    }
}
```

Set `Cache-Control: no-cache` (or use versioned filenames) on `app.js` when you deploy updates: the
files carry no cache headers of their own.

For a local look: `cd web/misaka-options && python3 -m http.server 9471`, then open
`http://127.0.0.1:9471/?mock=1`.

## What each page does

Hash routes: `#/trade/<lineId>`, `#/portfolio`, `#/lines`, `#/line/<lineId>`, `#/leaderboard`, `#/docs`.

- **Trade** (default). Market selector (name from the wRPC line row, symbol from the facade's
  `symbol()`, fallback `MP-<8 hex>`), stats (price, 24 h change, reserve, sold/supply, owner, current
  version, roots in force); a canvas chart of curve prices sampled every `POLL_MS` while the page is
  open and folded with the facade's `Bought`/`Sold` events (price after, at the block's timestamp),
  persisted per line in `localStorage`; **curve depth** (cost of 1/10/100/1,000/10,000 positions and
  the net MSK a sell of the same sizes returns, all from the ADR-0087 arithmetic on the market row);
  recent settlements (facade events); the **order entry** (Buy/Sell, amount, slippage → floor,
  quote with positions out, average price, price after, price impact, 5 % burn, 1 % owner leg,
  94 % net, quoted by the node's AMM precompile when the EVM RPC is armed and by local arithmetic
  otherwise); bottom tabs Positions, Order history (transactions sent from this browser, followed
  through `eth_getTransactionReceipt` and then the settlement event at the next block), Settlements,
  Line info.
- **Models** (`#/lines`). Every line of every known class, sortable; click a row to trade.
- **Line**. The row, roles (owner/developer/maintainer bonds and payout payloads), versions with
  chain-counted usage and declared hashes/evaluations (labelled *declared*), proposals, roots in force.
- **Portfolio**. The connected account's EVM-namespace positions with a mark value (net MSK of a
  sell-all right now), MSK balance, settlement events, order history.
- **Leaderboard**. Lines ranked by reserve, positions sold, or chain-counted usage. No benchmark
  scores: the chain refuses a quality oracle, and declared evaluations are shown only on line pages.
- **Docs**. The explainer, with links to the ADRs.

## How it reads the chain

wRPC (JSON over WebSocket, one persistent socket, requests multiplexed by id):
`getBlockDagInfo`, `getPalwModelLines(classId)`, `getPalwModelLine(lineId)`,
`getPalwModelMarket(lineId)`, `getPalwModelVersion(lineId, version)`,
`getPalwModelProposals(lineId)`, `getPalwModelPositions(holder)`. Wire format:
`{"id":n,"method":"getPalwModelMarket","params":{"lineId":"<128 hex>"}}` in,
`{"id":n,"method":"...","params":{...result...}}` out (`error` on failure). Integer literals of 16+
digits are re-quoted before `JSON.parse` so u64 sompi values stay exact.

EVM JSON-RPC (`eth_call` against the native doors, standard `eth_*` otherwise):

| address | used for |
|---|---|
| registry `0x…F010` | `chainDaa()` (32 bytes back = the fence is armed; empty = dormant), `classCount/classAt/classRow`, `linesOfCount/lineOfClassAt`, `line`, `rootsInForceCount`, `facadeOf`, `usage` |
| AMM `0x…F011` | `market`, `price`, `quoteBuy`, `quoteSell`, `constants` |
| position `0x…F012` | `balanceOfAddress`, `holderIdOf` |
| the line's facade `0x4d50…` | `symbol()`, and the two writes `buy(minUnitsOut)` payable / `sell(unitsIn, minMskOutSompi)` |

64-byte ids travel as two `bytes32` words, high half first; MSK is in sompi everywhere except a
buy's `msg.value` (wei, a multiple of 1e10); positions are in units (10^6 = one position).
Settlement events are read with `eth_getLogs` on the facade over `LOG_LOOKBACK_BLOCKS`, filtered by
the `Bought/Sold/Refused` topics and, for an account, the holder topic.

Degradation, in order: a market row comes from the wRPC and, failing that, from the AMM window;
lines come from the wRPC, then the registry window, then a bare founding line per configured class;
the facade address is read from `facadeOf` and, until confirmed, derived locally (BLAKE2b) and
shown as *unconfirmed* (trades are not sent to an unconfirmed facade); the holder id is read from
`holderIdOf` or derived locally the same way. If neither RPC answers, every figure is a dash and a
banner says so; the layout always renders.

## The wallet

Connect asks the injected provider for accounts, then `wallet_switchEthereumChain` to `0x4D534B`
and, on 4902, `wallet_addEthereumChain` (name "MISAKA <network>", currency MSK with 18 decimals,
rpc = the resolved `EVM_RPC_URL`, explorer = `EXPLORER_URL`). A buy is `eth_sendTransaction` to the
facade with `value = gross sompi × 1e10` and `data = buy(minUnitsOut)`; a sell is `sell(unitsIn,
minMskOutSompi)`. Gas is left to the wallet's estimate. No private key ever touches this site.

The floor is the quote less the slippage tolerance (default 2 %). The fold quotes the action on the
row *as it then stands* after the block's carrier-borne trades, so a floor that is too tight is
refused rather than filled worse; a refused buy is refunded at the next block, and the site shows
the refusal reason from the `Refused` event.

## Self-test

Open `?selftest=1` (any route): a box at the top of the page and the console report the checks:
keccak-256 and BLAKE2b-512 against known vectors, the facade-address and EVM-holder-id
derivations against values computed independently (Python `hashlib.blake2b` with the node's key
domains), and the curve against the golden numbers in `palw_model_market_v1.rs`'s tests (the
ADR-0087 §4 table: 48,453 and 16,824 positions out, 1,880 MSK gross and 1,767.2 MSK net on the
sell-all, the product never below K, a round trip at most 0.94² of the gross, a closed market
refusing buys and honouring sells). 27 checks, all passing as shipped.

## Mock mode

`?mock=1` loads `mock.js`, which simulates the wRPC, the EVM RPC and the wallet in the page: two
classes (the two testnet-11 class ids above), three lines (two founding lines and a founded
`QWEN25-B` with an adopted proposal and declared evaluations), a trade tape over the previous 26
hours priced by the site's own curve port, a block every 6 seconds with background trades and
usage, and a wallet account holding 5,000 MSK and two positions. A buy or sell sent in mock mode
goes through the real code path: the receipt appears in the next block, the fold's decision and the
`Bought`/`Sold`/`Refused` event one block later. Mock data lives under its own `localStorage`
prefix (`mo-mock:`) and never mixes with real data. The banner and the footer say "mock" whenever
it is on. Nothing in mock mode is a chain fact; it exists for demos, screenshots and UI work.

## What is not available on the public network yet

- On testnet-11 today the `palw_model_market`, `palw_model_lines` and `palw_model_evm` fences are
  **dormant** (`None` on every preset). The wRPC answers `getPalwModelMarket` for every registered
  class with an **unopened** market (the whole supply in the curve, price V/supply = 0.01 MSK),
  `getPalwModelLines` with the synthesised founding line, and the EVM doors are empty accounts
  (`chainDaa()` returns no data). The site detects this and shows "Market not armed on this
  network yet" while rendering the layout with the unopened rows; no trade can be sent.
- Below the fences there is no way to enumerate classes from the chain (the registry window is
  the enumeration), hence `CLASS_IDS` in `config.js`.
- The `CLASSICAL-ECC` label for EVM-held positions (ADR-0089 E12) is not served by the RPC yet;
  the site states the namespace in copy instead.
- Historical `eth_call` against the doors is not supported by the node (the fold is kept at the
  tip), so price history is sampled by the browser and completed from settlement events; the
  24 h change is computed from those samples and is a dash until the browser has seen enough.
- Explorer links: `EXPLORER_URL` is linked as a site; per-transaction deep links are not built
  because the explorer's EVM routes are not pinned anywhere in the tree.

## Choices made without asking

- One persistent wRPC socket with requests multiplexed by id (reconnects lazily) rather than one
  socket per request; both shapes are accepted by the node.
- Prices are shown in MSK per position with up to 8 decimals in the header and 6 in dense tables.
- "Sold / Supply" shows positions currently outside the curve (supply minus the curve's units);
  the cumulative `soldUnits` is in the tooltip and in Line info.
- The depth table's buy side inverts the curve to find the least gross MSK that releases N whole
  positions (verified by re-quoting); the sell side is a straight `sellQuote` and is a dash when N
  exceeds the positions outside the curve.
- The trade page polls every 10 s; models/leaderboard every 30 s; pending transactions every 12 s.
- A refused action is reported with the fold's reason code mapped to words (1 not armed, 2 line
  missing, 3 line not active, 4 releases nothing, 5 below your floor, 6 market missing, 7 exceeds
  your position, 8 pays nothing, 9 other).
