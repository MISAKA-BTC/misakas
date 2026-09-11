# MISAKA Options

A static web front-end for the MISAKA chain's **model store** (ADR-0087, ADR-0088, ADR-0089,
**ADR-0090**, **ADR-0095**), served at https://misakaoptions.com. What a model sells is a
**membership**: access its owner declares on chain, and that the fold enforces where it can.

**The site's words and the chain's words.** [ADR-0095](../../docs/adr/0095-a-position-is-a-membership-not-an-income.md)
settled what a position is — a membership, never an income — and the front-end says so, in one
mapping used everywhere:

| the page says | the chain, the CLI and the ABI say |
|---|---|
| store | the line's market (`palw_model_market_v1`) |
| membership | position (`decimals() == 0`) |
| join / leave | `buy(minUnitsOut)` / `sell(unitsIn, minMskOutSompi)` |
| open the store, opening deposit | `seed()`, the seed |
| buy-back reserve | the curve's MSK reserve |

Each mapped word carries the chain's own name in a `title`, and every table of chain events still
prints the facade's event name in its tooltip: the site never becomes a second vocabulary a reader
cannot trace back to the explorer.

A model's store is **opened by somebody paying** at least 100,000 MSK into the model's line
(ADR-0090): the whole deposit becomes the curve's reserve, fee-free, and is locked for good; whoever
pays it receives no membership and nothing back. The store holds **500,000 whole memberships** (no
fraction of one), bought from the line's protocol curve and sold back to it, never transferred; 5 %
of every MSK leg of a join or a leave is burned and 1 % goes to the line's owner; the curve's
product never falls, so the reserve never falls under the opening deposit. **No holder is ever
paid** (ADR-0091). This site reads the store through a node (the Kaspa-style wRPC and the EVM's
read precompiles), quotes with the chain's own curve arithmetic, and sends every move through the
EVM writer from the EIP-1193 wallet the user picks (MISAKA Wallet, MetaMask, or any other the
browser has; see [The wallet](#the-wallet)).

**Every figure on screen is an RPC reply or the ADR-0090 arithmetic applied to a market row, or a
dash.** Nothing is estimated or invented, and there is no server side: four static files.

## Files

| file | what it is |
|---|---|
| `index.html` | the shell: nav (Store, My memberships, Models, Rankings, List a model, How it works), banner, main, footer, toasts |
| `app.js` | everything else, plain ES2020 (no build): the curve arithmetic (BigInt port of `consensus/core/src/palw_model_market_v1.rs` as amended by ADR-0090), keccak-256 and BLAKE2b-512 (selectors, event topics, facade addresses, EVM holder ids), a wRPC client (JSON over WebSocket), an EVM JSON-RPC client with hand-rolled ABI encoding for the native doors, the wallet flow, the opening panel, the membership card, the pages, and a canvas price chart |
| `style.css` | dark theme, teal accent, dense layout; responsive down to 380 px |
| `config.js` | the only file you need to edit: endpoints, chain id, network name, fallback class ids, explorer and docs URLs |
| `mock.js` | loaded **only** with `?mock=1`: a simulated node and wallet for demos and screenshots (see below) |
| `screenshots/` | `store.png` (an open store), `store-not-open.png` (the opening panel), `list-a-model.png` (the checklist), `models.png`, `rankings.png`, `memberships.png`, taken in mock mode at 1440 px wide |

No external dependencies, no CDN, no fonts, no build step. `app.js` is about 230 KB unminified.

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
  POLL_MS: 10000,               // store page refresh cadence
  LOG_LOOKBACK_BLOCKS: 5000,    // eth_getLogs window for settlement events (node cap: 10000)
  ADR_URL: "https://github.com/MISAKA-BTC/misakas/tree/main/docs/adr",
  DOCS_URL: "https://github.com/MISAKA-BTC/misakas/tree/main/docs",  // the runbooks the List a model page links
  WALLET_URL: "https://wallet.misakascan.com"   // optional: where the wallet chooser sends a browser without MISAKA Wallet
};
```

`EVM_RPC_URL` is resolved against the page URL and handed to the wallet by
`wallet_addEthereumChain`, so it must be an `https://` URL (or `http://localhost`) that the user's
browser can reach; a relative path on an https site is fine.

The live deployment (misakaoptions.com, 2026-09-05) reads both endpoints off the explorer host:
`WRPC_URL: "wss://misakascan.com/kaspa"` and `EVM_RPC_URL: "https://misakascan.com/evm"`. The
browser reaches them directly; the site's own host proxies neither.

Known class ids on testnet-11 (from the tree, `tools/palw-jobs-export/src/main.rs`), usable as
`CLASS_IDS` while the registry window is dormant:

```
4277d84f7d91528cc04aa366d51ee1c2e4f7902c4f6b16a213dead1c7e227977db732f18ed6183db3d944d44726ebd3feff7b15c48f9dba11cd526684f35f1b7  Qwen2.5 A16 (graph-v5, 512)
5bd9ae3d91df80650caffe3126a38bafb0b4feb9b046a416d353a7c3f71af6eab5aadf9b1ce41650007a980f1cc6044ef218424f4cbb8299ef9e92c97b99ef8e  Qwen3.6-35B-A3B (graph-v3)
```

A class's founding line has the class id as its line id, so each of these is also a line id
(`#/store/<id>`, `#/line/<id>`, `#/add/<id>`).

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

    # EVM JSON-RPC: only if a node runs on THIS host. The deployment at misakaoptions.com has
    # none, so `EVM_RPC_URL` names the explorer's endpoint (https://misakascan.com/evm) instead
    # and this block is absent there. That endpoint is served by the testnet-11 node started with
    # `--evm-rpc-listen=127.0.0.1:8545`, proxied CORS-open and rate-limited (10 r/s, burst 20).
    location /evm {
        proxy_pass http://127.0.0.1:8545/;
        proxy_http_version 1.1;
        proxy_set_header Content-Type application/json;
        add_header Cache-Control "no-store";
    }
}
```

Set `Cache-Control: no-cache` (or use versioned filenames) on `app.js` and `mock.js` when you
deploy updates: the files carry no cache headers of their own, and a browser that keeps an old
`mock.js` next to a new `app.js` shows a mock world the app does not expect.

For a local look: `cd web/misaka-options && python3 -m http.server 9471`, then open
`http://127.0.0.1:9471/?mock=1`.

## What each page does

Hash routes: `#/store/<lineId>`, `#/portfolio`, `#/lines`, `#/line/<lineId>`, `#/leaderboard`,
`#/add[/<classId>]`, `#/docs`. `#/trade/<id>` is the store's former name and is redirected
(`location.replace`, no history entry), so links printed before the rename still land.

- **Store** (default, `#/store/<lineId>`). Model selector (name from the wRPC line row, symbol from
  the facade's `symbol()`, fallback `MP-<8 hex>`); a header strip that leads with what the model
  **is and is used for** — membership price, members hold, declared tiers, paid inferences, current
  version — before the market's own numbers (bought by mining, 24 h, buy-back reserve, opening
  deposit, who opened it, owner, roots in force). Three tabs, opening on **Use** (the fold's claim
  counts and the version table), then **Versions**, then **Price history** (a canvas chart of curve
  prices sampled every `POLL_MS` while the page is open and folded with the facade's
  `Bought`/`Sold` events, persisted per line in `localStorage`).
  A **price list** (what 1/10/100/1,000/10,000 memberships cost and what the store pays back for the
  same sizes, all from the ADR-0090 arithmetic on the market row) and recent joins and leaves.
  Beside them, the **membership card** (ADR-0095 §4.10: the tiers in effect, what each grants in
  plain words with the chain's bit name in the tooltip, the lead and tenure each asks for, any
  pending weakening, and the lapse — the reason to join belongs before the join; where a node serves
  no declaration the card says so rather than showing nothing), and under it the **desk**
  (Join/Leave, amount, the floor, the quote's 5 % burn, 1 % owner leg and 94 % net, quoted by the
  node's AMM precompile when the EVM RPC is armed and by local arithmetic otherwise). Bottom tabs:
  My memberships, My activity (transactions sent from this browser, followed through
  `eth_getTransactionReceipt` and then the settlement event at the next block), Recent joins and
  leaves, Model details.
  **For a line with no store the desk is replaced by the "Open this store" panel**: what an opening
  deposit is (at least the network's least seed, locked for good, no membership for whoever pays it,
  the first price deposit / 500,000), an MSK input defaulting to the least seed (`seedMinSompi` from
  the wRPC, else the AMM window's `constants()`, else 100,000), the first-price preview, the class's
  status, and an **Open** button that sends the facade's `seed()` with
  `value = deposit sompi × 10^10 wei`. The price list, the chart and the quote say "this store is
  not open yet" rather than quoting a curve that does not exist.
- **List a model** (`#/add`). The path from a registered class to a store people can join, as a
  checklist with live status where the chain can answer: (1) register the class (the exact `kaspad
  --palw-register-class` / `palw-class preflight` / `--palw-register-bond` commands from the
  runbooks, with links; this step cannot be done from a browser), (2) open the store (the same
  panel, with a line-id input defaulting to the class id, which is the founding line's id), (3)
  approval (the class's status as the chain names it, `Registered { activation_daa }` /
  `Active` / `Frozen` / `Dormant`, the chain's DAA now, and the attempt / free-prompt lane
  certification), (4) members — and the owner's last step, declaring what a membership gets them
  with `misaka palw line-benefits`.
- **Models** (`#/lines`). Every line of every known class, sortable, **use first**; a line with no
  store shows an **Open it** call to action in place of a price.
- **Line** (`#/line/<lineId>`). The row, the tiles (use first), roles, the membership card, versions
  with chain-counted usage and declared hashes/evaluations (labelled *declared*), proposals, roots
  in force.
- **My memberships** (`#/portfolio`). The connected account's EVM-namespace memberships (whole
  numbers) with what the curve would pay back if it left every one right now, MSK balance, joins and
  leaves, activity.
- **Rankings** (`#/leaderboard`). Models ranked by **use** (the default), members, what mining
  bought back, the buy-back reserve, or the opening deposit. No benchmark scores: the chain refuses
  a quality oracle, and declared evaluations are shown only on line pages. A store that is not open
  ranks last.
- **How it works** (`#/docs`). What a membership gets you and what the fold enforces of it
  (ADR-0095), then the mechanism: the opening deposit, whole memberships, the curve without a
  virtual reserve, the deposit floor invariant, one opening a line, no transfer, and that no holder
  is ever paid — with links to the ADRs.

## The arithmetic (ADR-0090)

Ported exactly, in BigInt, from `consensus/core/src/palw_model_market_v1.rs`:

- `PALW_MODEL_POSITION_UNITS_V1 = 1` (a position is the unit; `decimals() == 0`),
  `PALW_MODEL_POSITION_SUPPLY_V1 = 500_000`, `PALW_MODEL_SEED_MIN_SOMPI_V1 = 100,000 MSK`,
  burn 50 ‰, owner leg 10 ‰. There is no virtual reserve.
- A row with `reserve == 0` is **not seeded**: no price, no quote (the fold's own guard). A seed
  makes the row `reserve = seed, positions = 500,000`; price = `reserve / positions`.
- **Buy** of `msk`: `net = msk − 5 % − 1 %`, `K = reserve × positions` of the current row,
  `x′ = reserve + net`, `positions′ = ⌈K / x′⌉`, `out = positions − positions′` (refused when 0).
- **Sell** of `n`: `positions′ = positions + n` (≤ 500,000), `x′ = ⌈K / positions′⌉`,
  `gross = min(reserve − x′, reserve)`, fees on the gross, net paid.
- The depth table's buy side inverts the curve to find the least gross MSK that releases N whole
  positions (verified by re-quoting).

## How it reads the chain

wRPC (JSON over WebSocket, one persistent socket, requests multiplexed by id):
`getBlockDagInfo`, `getPalwModelLines(classId)`, `getPalwModelLine(lineId)`,
`getPalwModelMarket(lineId)` (now with `seedSompi`, `seededBy`, `seedMinSompi`, `classStatus`;
the request also carries `classId` for a pre-ADR-0088 node, see below),
`getPalwModelVersion(lineId, version)`, `getPalwModelProposals(lineId)`,
`getPalwModelPositions(holder)`, and on the List a model page `getPalwProducerFacts(classId)` for the
free-prompt certification. Wire format:
`{"id":n,"method":"getPalwModelMarket","params":{"lineId":"<128 hex>"}}` in,
`{"id":n,"method":"...","params":{...result...}}` out (`error` on failure). Integer literals of 16+
digits are re-quoted before `JSON.parse` so u64 sompi values stay exact.

EVM JSON-RPC (`eth_call` against the native doors, standard `eth_*` otherwise):

| address | used for |
|---|---|
| registry `0x…F010` | `chainDaa()` (32 bytes back = the fence is armed; empty = dormant), `classCount/classAt/classRow` (status code, share, registrant), `certified(class, lane)`, `linesOfCount/lineOfClassAt`, `line`, `rootsInForceCount`, `facadeOf`, `usage` |
| AMM `0x…F011` | `market` (`exists` false and a zero row for an unseeded line), `price`, `quoteBuy`, `quoteSell`, `constants` (its third word is `seedMinSompi`) |
| position `0x…F012` | `balanceOfAddress`, `holderIdOf` |
| the line's facade `0x4d50…` | `symbol()`, and the three writes `buy(minUnitsOut)` payable, `sell(unitsIn, minMskOutSompi)`, `seed()` payable |

64-byte ids travel as two `bytes32` words, high half first; MSK is in sompi everywhere except a
buy's or a seed's `msg.value` (wei, a multiple of 1e10); positions are whole numbers. Settlement
events are read with `eth_getLogs` on the facade over `LOG_LOOKBACK_BLOCKS`, filtered by the
`Bought/Sold/Seeded/Refused` topics and, for an account, the holder topic.

Degradation, in order: a market row comes from the wRPC and, failing that, from the AMM window
(which does not carry the seed or the seeder: those show as a dash); lines come from the wRPC, then
the registry window, then a bare founding line per configured class; the facade address is read
from `facadeOf` and, until confirmed, derived locally (BLAKE2b) and shown as *unconfirmed* (nothing
is sent to an unconfirmed facade); the holder id is read from `holderIdOf` or
derived locally the same way. If neither RPC answers, every figure is a dash and a banner says so;
the layout always renders. A page's poll that fails logs the failure to the console rather than
swallowing it.

## The wallet

**Which wallet.** The site no longer takes whatever sits on `window.ethereum`: that global belongs
to whichever extension claimed it (with Phantom installed, Phantom's own "MetaMask or Phantom?"
prompt, which never lists MISAKA Wallet). It listens for EIP-6963 announcements (`eip6963:announceProvider`,
collected by `info.uuid`, one row per `rdns`) and dispatches `eip6963:requestProvider` at startup and
again whenever the chooser opens. A wallet that announces nothing is still reached through the legacy
globals: `window.misaka` (when `isMisakaWallet`) as "MISAKA Wallet", and `window.ethereum`, when no
announcement is that object, as "Browser wallet". **Connect** goes straight to the wallet when there
is exactly one; with several (or none) it opens the chooser: MISAKA Wallet first, marked *Recommended
— tops up from your UTXO balance automatically*, then the others in the order they announced, each
with the icon it announced (only an image `data:` URI, only ever as an `<img src>`; the name only
ever as text). Without MISAKA Wallet in the browser its row is still first, as a link to
`WALLET_URL` (https://wallet.misakascan.com) instead of a wallet to connect. The wallet the user
picks is asked for accounts and only then becomes the site's wallet for the session (its
`accountsChanged`/`chainChanged` listeners replace the last wallet's); it is remembered by rdns under
`mo:wallet:choice` and taken again without asking on the next visit while it is still announced
(and reconnected, unless the user disconnected). Connected, the header button shows the wallet's
icon (not on a phone) and the address and opens a menu: *Change wallet*, *Copy address*, *Disconnect* (and *Switch to
the MISAKA chain* when the wallet is elsewhere). `wallet.topsUp()` follows the chosen wallet: only a
MISAKA Wallet in use lifts the EVM-balance refusals, not one merely installed next to it.

Connect asks the chosen provider for accounts, then `wallet_switchEthereumChain` to `0x4D534B`
and, on 4902, `wallet_addEthereumChain` (name "MISAKA <network>", currency MSK with 18 decimals,
rpc = the resolved `EVM_RPC_URL`, explorer = `EXPLORER_URL`). A buy is `eth_sendTransaction` to the
facade with `value = gross sompi × 1e10` and `data = buy(minUnitsOut)`; a sell is `sell(unitsIn,
minMskOutSompi)`; a seed is `seed()` with `value = seed sompi × 1e10` (at least the least seed,
else the writer reverts `SeedTooSmall()` at the call). Gas is left to the wallet's estimate. No
private key ever touches this site.

The floor is the quote less a fixed 50 % tolerance (there is no setting; the widest the removed field ever allowed). The fold quotes the action on the
row *as it then stands* after the block's carrier-borne moves, so a floor that is too tight is
refused rather than filled worse; a refused buy or seed is refunded at the next block, and the site
shows the refusal reason from the `Refused` event (1 not armed, 2 line missing, 3 class or line
not active, 4 releases nothing, 5 below your floor, 6 market missing / not seeded, 7 exceeds your
position, 8 pays nothing, 9 other, 10 already seeded, 11 seed too small, 12 class closed).

## Self-test

Open `?selftest=1` (any route): a box at the top of the page and the console report the checks:
keccak-256 and BLAKE2b-512 against known vectors, the facade-address and EVM-holder-id
derivations against values computed independently (Python `hashlib.blake2b` with the node's key
domains), the `seed()` selector and the `Seeded` topic against an independent Keccak, the writer's
action-3 data layout, and the curve against the golden numbers in `palw_model_market_v1.rs`'s
tests (the ADR-0090 §4 table: from a market seeded with 100,000 MSK, first price 0.2 MSK; a buy of
1,000 MSK burns 50, pays 10, puts 940 in the reserve and releases 4,656 positions at 0.20377757
after; a second releases 4,570 (reserve 101,880); selling all 9,226 back pays 187,988,976,000
sompi gross and 176,709,637,440 net, puts 500,000 positions back and leaves the reserve at
100,000.11024 MSK; 0.1 MSK releases nothing and 0.22 MSK releases one; the product never falls and
the reserve never falls under the seed; an unseeded row quotes nothing; a closed market refuses
buys and honours sells), and the wallet chooser's ordering (MISAKA Wallet first, one row per rdns,
the legacy globals as fallbacks, a non-image icon not drawn, a remembered wallet taken only while
it is installed). 67 checks, all passing as shipped.

## Mock mode

`?mock=1` loads `mock.js`, which simulates the wRPC, the EVM RPC and the wallet in the page: three
classes (the two testnet-11 class ids above, both `Active`, and a third, `Registered` with an
activation DAA 300 blocks ahead, that flips to `Active` when the mock's clock reaches it), four
lines (three seeded with real seeds of 250,000, 100,000 and 120,000 MSK, priced by the site's own
curve port; the registered class's founding line with **no store**, so the opening panel and the
List a model checklist have a subject), a tape of joins and leaves over the previous 26 hours, a
block every 6 seconds with background moves and usage, and a wallet account holding 150,000 MSK
(enough to open a store) and two memberships. A buy, a sell or a seed sent in mock mode goes through the real code path: the receipt
appears in the next block, the fold's decision and the `Bought`/`Sold`/`Seeded`/`Refused` event
one block later (a second seed on a seeded line is refused with reason 10; a buy on the registered
class is refused with reason 3 until it activates). Mock data lives under its own `localStorage`
prefix (`mo-mock:`) and never mixes with real data; a new mock world drops the samples and
transactions an older mock world left there. The banner and the footer say "mock" whenever it is
on. Nothing in mock mode is a chain fact; it exists for demos, screenshots and UI work.

`?mock=1&mockwallet=misaka` makes the mock wallet pose as **MISAKA Wallet**: 5 MSK on the EVM account,
the rest "on the post-quantum lane", and a send that tops the EVM account up by the shortfall plus a
10,000-sompi gas reserve before it goes out — what the extension's automatic top-up does. It exercises
`wallet.topsUp()`: for MISAKA Wallet the store does not refuse a join or an opening deposit on the
EVM balance alone (the wallet moves what is missing inside the same confirmation and says so), and
the desk says how much will be moved and that the claim takes about two blocks. Any other wallet
(MetaMask) is still refused with "Insufficient MSK balance in the EVM account", because it cannot top up.

`?mockwallet=` also takes a comma list, so the wallet chooser can be exercised without extensions:
`misaka` (MISAKA Wallet, announced with EIP-6963 as `com.misakachain.wallet`), `misaka-legacy` (MISAKA
Wallet reachable only through its global), `phantom` and `metamask` (announced as `app.phantom` and
`io.metamask`, with plain lettered icons, not the real marks; neither tops up), `injected` (a wallet
on `window.ethereum` that announces nothing: the "Browser wallet" row) and `none`. For example
`?mock=1&mockwallet=misaka,phantom,metamask` lists three wallets, MISAKA Wallet first;
`?mock=1&mockwallet=phantom,metamask` shows the "MISAKA Wallet — not installed" row; `none` shows the
chooser with no wallet at all. All of them sign for the one mock account (5 MSK on the EVM lane when
MISAKA Wallet is in the list, else 150,000), and in mock mode the chooser lists mock.js's wallets
only: a real extension would sign on a real chain for a simulated one.

## What is not available on the public network yet

- On testnet-11 today the `palw_model_market`, `palw_model_lines` and `palw_model_evm` fences are
  **dormant** (`None` on every preset). Every read is empty or synthesised: the wRPC answers
  `getPalwModelMarket` for every registered class with an **unseeded** row (reserve 0, 500,000
  positions, no price, `seedMinSompi` = 100,000 MSK), `getPalwModelLines` with the synthesised
  founding line, and the EVM doors are empty accounts (`chainDaa()` returns no data). The site
  detects this and shows "Market not armed on this network yet" while rendering the layout. When
  the wRPC answers, every line reads as having no store and the opening panel appears with its
  button disabled ("the store is not armed on this network: the facade is an empty account"), so a
  deposit sent by hand would be refused or lost; when no market row can be read at all (the wRPC
  down), the desk shows dashes and its button is disabled. Until the fences are armed (a release:
  they enter the fingerprint) no store can be opened and nobody can join one.
- **ADR-0095 has a fence of its own** (`palw_model_benefits`, `None` on every preset today), so no
  line can declare what its memberships grant yet and `getPalwModelLine` serves no `benefits` word.
  The membership card says exactly that instead of rendering an empty promise; it fills in on its
  own once a node serves the field.
- The public node behind misakascan.com (probed 2026-09-06) still runs a build from **before
  ADR-0088/0090**: its `getPalwModelMarket` is keyed by `classId` (a request with only `lineId`
  is answered with "request deserialization error") and its row carries a virtual reserve of
  1,000 MSK, 100,000 positions of 10^6 units and no seed. The site sends both keys (`lineId` and
  `classId`, the founding line's id being the class id; each build ignores the key it does not
  know), reads such a row as **unseeded** (reserve 0, no price) and marks it `legacy`: position
  counts and supply are shown as a dash rather than in the wrong unit, and the banner says so.
  A node of this tree answers the ADR-0090 shape and everything above applies.
- Below the fences there is no way to enumerate classes from the chain (the registry window is
  the enumeration), hence `CLASS_IDS` in `config.js`.
- **Names.** A line's name is a chain fact (`getPalwModelLine` serves it), but only from an
  ADR-0088 node, and testnet-11's public node is older — so on the live site every name comes back
  empty and a row used to read as its own derived symbol twice (`MP-4277d84f MP-4277d84f`).
  `config.js`'s `MODELS` is the fallback catalogue: a Hugging Face-style repository title, what the
  chain registered of it (`A16 dense · graph-v5 · n_ctx 512`), the parameter count and the
  artifact. A title from there is marked ⓘ and says, on hover, that it is the site's word and the
  id below is the chain's. A chain-served name always wins; a line the catalogue does not know
  still reads as its symbol.
- ADR-0091: the market row carries `buybackSompi` (MSK the mining reward has bought into the pair)
  and `retiredUnits` (positions the chain holds for good). Both travel on the wRPC row and as the
  AMM window's two appended words, so a node from before ADR-0091 answers nine words and the site
  shows a dash rather than a zero. The site never computes them: `curve.buyback` exists only so the
  self-test can check the chain's arithmetic and so `mock.js` can simulate a mined pair.
- The AMM window's `market()` does not carry the seed or the seeder; when the wRPC is down those
  two cells are a dash even for a seeded line.
- Attempt-lane certification is served by the registry window only (`certified(class, 0)`); the
  wRPC serves the free-prompt lane (`getPalwProducerFacts.fpCertified`). With the window dormant
  the attempt lane is a dash on the List a model page.
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
- Prices are shown in MSK per position with up to 8 decimals in the header and 6 in dense tables;
  position counts are integers everywhere (an input with a decimal point is refused with a
  message, not rounded).
- "Members hold" shows memberships currently outside the curve (supply minus the curve's units);
  the cumulative `soldUnits` is in the tooltip and in Model details.
- "Not open" is decided by `reserve == 0` (the fold's own quote guard), never by the absence of
  a `seedSompi` field, so an old node and the AMM window read the same way.
- The opening panel defaults to the least seed and refuses (before the wallet) a value under it, an
  already-open store, a frozen or dormant class, an unconfirmed facade, and an insufficient balance;
  the same panel serves the store page and the List a model page. An opening is recorded in My
  activity as its own action kind and settles on the `Seeded` event.
- The List a model page reads the class status from the wRPC market row's `classStatus` string
  (parsed for `activation_daa`) and, when the doors are armed, from `classRow`; `#/add/<classId>`
  opens the checklist on a class, and the last class checked is remembered per browser.
- A join on a line whose class is not `Active` is explained by the class status ("Registered,
  activates at DAA n: buys wait for Active") rather than by the RPC's `closedToBuys` flag, which
  the node sets for both a retired line and a class that is not Active.
- The store page polls every 10 s; models/rankings every 30 s; the List a model page every 15 s;
  pending transactions every 12 s. A poll that fails is logged, not swallowed.
- `screenshots/list-a-model.png` is taken 1440 px wide and taller than the others so the whole
  checklist is in one image.
