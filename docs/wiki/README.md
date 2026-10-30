# Wiki pages (testnet-12 sync)

This directory mirrors the GitHub wiki (<https://github.com/MISAKA-BTC/misakas/wiki>) after it was
re-checked against the public testnet-12 (release `0e8ec984e`) on 2026-09-25. The session that made
the change could not push to `misakas.wiki.git`, so the pages are staged here.

To publish them:

```bash
git clone https://github.com/MISAKA-BTC/misakas.wiki.git
cd misakas.wiki
git am /path/to/misakas/docs/wiki/wiki-t12-sync.patch   # or: cp /path/to/misakas/docs/wiki/*.md . (not README.md)
git push origin master
```

The patch applies on wiki commit `bd205c8`. Once the wiki carries these pages, this directory can
be deleted.
