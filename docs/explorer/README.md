# The explorer's copy of what it may show

`misakascan.com` is a hand-patched `app.js` on `.113` (`/var/www/misaka-explorer`), not a build
product of this tree, so the tree keeps the two things that decide what a reader sees:

* `2026-09-06-da-visibility.patch` — the page change of 2026-09-06, against the `app.js` that was
  live before it (`?v=llmview2`, now `?v=da2`).
* `jobs_textify.py` — the step between `misaka-palw-jobs-export` and the published
  `llm-jobs.json`, which turns the anchor-derived prompt's ids into text and adds nothing else.

## The rule

A claim puts COMMITMENTS on chain — the roots over its trace, its output and its execution — and
the bytes behind them stay with the executor, reaching the claim's five drawn seats over an
authenticated pull (ADR-0077 Decision 16). A piece becomes public exactly when somebody **demands**
it: a data-availability accusation the executor answers on chain (ADR-0062), which costs the
accuser if the answer comes. So:

| shown | why |
|---|---|
| the attempt lane's input | a pure function of the block's anchor — any reader recomputes it without this site |
| counts, work, class, claim, block, DAA | the claim carries them on chain |
| ADR-0078 derived artifacts | the kind, transformer, id and size are on chain: what a free prompt published |
| a disclosure | the chain carries it because a demand forced it — marked `disclosed on demand` |

| not shown | why |
|---|---|
| a person's prompt | the author's; `PanelDa` puts none on chain at all |
| any answer | the chain carries `output_root`; the ids ride the envelope served to seats |

Until 2026-09-06 the feed was built by reading the producers' retention directories and printing
what it found, which put the text in the one place the network's own rules do not. The exporter no
longer reads those files (`tools/palw-jobs-export`), and the page renders the sealed form.

**What this does not do.** A `PublicDa` commitment carries its prompt ids in the block payload, so
for claims filed before testnet-11 crosses DAA 1,900 the ids remain readable by anyone who decodes
the transaction. Hiding them here is not privacy for those claims; it is this site declining to be
the place that publishes them. Privacy for a claim comes from filing it as `PanelDa`, which the
fence at 1,900 makes possible.

## Deploying a change

    scp app.js root@169.58.232.113:/tmp/ && ssh … 'cd /var/www/misaka-explorer && cp -a app.js app.js.bak-<tag>-<ts> && install -m644 /tmp/app.js app.js'

then bump BOTH `?v=` tokens in `index.html` (the hash router does not reload, and a stale `app.js`
against a fresh feed is the failure mode this bump exists for).

## 2026-09-06 — "Recent blocks" was in consensus order, not time order

A reader on a phone reported that the block list is sometimes not in time order. It was, routinely.

`renderRecent()` painted `recent`, which `updateRecent()` keeps sorted **blueScore desc, daaScore
desc** — a consensus order, chosen deliberately ("Mirror the official kaspa-explorer feed … stable
under reorgs/orphaning"). In a DAG that is not time order. Measured against the live chain the same
day: **482 of 1,723 adjacent pairs had the upper row OLDER than the row beneath it**, the worst by
1 h 50 m. The table has an `Age` column, so the reader is right and the code was answering a
different question than the one the column asks.

The stored order could not simply change: `refreshHomeInner` reads `recent[0].blueScore` to decide
whether the cached window has fallen behind the live sink and must be re-anchored. Sorting `recent`
by time would have fed that check the newest *timestamp* instead of the highest *blueScore* and
broken the self-heal that keeps the page from freezing in the past. So the display sorts a **copy**
(`recent.slice().sort(...)`, by timestamp desc, then blueScore, then daaScore) and no control path
moves. Verified live at a 375×812 viewport: 25 rows, 0 inversions.

Patch: `2026-09-06-recent-blocks-time-order.patch`.

## 2026-09-06 — the page shipped uncompressed and uncacheable, over HTTP/1.1

Same report: "slow on a phone". Three separate causes, all in nginx, none in the app.

| | before | after |
|---|---|---|
| `app.js` | 212,663 B, `no-store` | 68,689 B gzip, `max-age=300` |
| `style.css` | 19,838 B, `no-store` | 5,479 B gzip |
| `sha3.min.js` | 9,850 B | 3,893 B gzip |
| `llm-jobs.json` | 213,903 B | 120,421 B gzip |
| protocol | HTTP/1.1 | HTTP/2 |

* **gzip**: `nginx.conf` ships `gzip on` with `gzip_types` **commented out** (the Debian default),
  so only `text/html` was ever compressed. Set at `server` level here, so the other vhosts on the
  host are untouched.
* **HTTP/2**: the `listen 443 ssl` lines had no `http2`. At this host's ~260 ms RTT each extra
  connection costs a TCP + TLS handshake before a byte moves; HTTP/1.1 opens one per parallel
  asset. Added on the first block for each address:port only — nginx 1.24 takes the socket option
  from whichever block declares it and warns if a second repeats it. `wallet.misakascan.com` shares
  the socket and got HTTP/2 with it.
* **caching**: `max-age=300, must-revalidate`, deliberately **not** `immutable`. `app.js` carries a
  self-update watcher whose own comment says it works "with the no-store cache policy": it HEADs
  `/app.js` every 60 s and reloads when the ETag moves. Under `immutable`, a reload would keep
  serving the year-old cached copy whenever a deploy forgot to bump `?v=` in `index.html` — turning
  a hand-maintained rule into a silent year-long staleness bug. At 300 s a revalidation costs one
  304 with an empty body and a deploy always lands within five minutes on its own.

**What is left, and is not fixable here:** nginx answers in **2 ms** locally. The ~950 ms a phone
sees is `tcp 0.26 s → tls 0.69 s → ttfb 0.95 s`: round trips to a host that is ~260 ms away. TLS
1.3 is already in force (1-RTT). Only moving the origin closer, or putting a CDN in front, changes
that number.

The front-page operator banner was also three flag days stale — it named DAA 5,000, fingerprint
`0533c8ee…`, a malformed `&nbsp;`, and the claim that "old and new builds stay peers until it
fires", which the 2026-09-06 measurement disproves. Replaced with the current notice.

Deploy: `app.js` first, then `index.html` (the reverse order 404s the versioned asset for a moment).
Backups on `.113`: `app.js.bak-ord-<ts>`, `index.html.bak-ord-<ts>`,
`/root/misakascan-nginx.bak-{,h2-,rv-}<ts>`.
