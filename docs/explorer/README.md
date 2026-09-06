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
