# Wiki pages (testnet-12)

This directory is the source of the GitHub wiki (<https://github.com/MISAKA-BTC/misakas/wiki>). Edit the
pages here; the wiki is a copy. Links between pages use the wiki's page names (`[Quick Start](Quick-Start)`),
so they resolve on the wiki, not in this tree.

To publish them, copy every page except this README and remove the pages that no longer exist here:

```bash
git clone https://github.com/MISAKA-BTC/misakas.wiki.git
cd misakas.wiki
for f in /path/to/misakas/docs/wiki/*.md; do [ "$(basename "$f")" = README.md ] || cp "$f" .; done
# pages merged or retired here (2026-10-02): Testnet-12-Operator-UI-JA → Quick-Start,
# PALW-Roles-and-Network-Scope-JA → PALW-Participation-JA, and the testnet-11 pages
git rm -q --ignore-unmatch Testnet-12-Operator-UI-JA.md PALW-Roles-and-Network-Scope-JA.md \
  Testnet-11-Operator-UI-JA.md Testnet-11-Verification-Participation-JA.md
git add -A
git commit -m "wiki: sync from docs/wiki"
git push origin master
```
