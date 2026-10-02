# The explorer's copy of what it may show

`misakascan.com` shows testnet-12. Its page (`app.js`, `index.html`), the wRPC probe and the deploy
steps are in [`contrib/misakascan-t12/`](../../contrib/misakascan-t12/DEPLOY.md); a change to the
page is made there and deployed with that directory's `DEPLOY.md` and `deploy.sh`. The testnet-11-era
patches, the nginx change of 2026-09-06 and the old `jobs_textify.py` step are kept as a record in
[`docs/archive/explorer/`](../archive/explorer/README.md).

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

The page renders the sealed form (`llmSealed` in `contrib/misakascan-t12/app.js`). The exporter in
`tools/palw-jobs-export` derives only the anchor prompt and opens nothing the executor retains; its
class constants are still the testnet-11 rows, so check it before using it to build a testnet-12
`llm-jobs.json`.

**What this does not do.** A `PublicDa` commitment carries its prompt ids in the block payload, so
anyone who decodes the transaction can read them. Hiding them here is not privacy for those claims;
it is this site declining to be the place that publishes them. Privacy for a claim comes from filing
it as `PanelDa` (`palw_panel_da`, in force from genesis on testnet-12).
