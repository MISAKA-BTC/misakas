# Wiki pages (testnet-12 sync)

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。

[新方針・廃止範囲](../adr/0177-model-bond-allocation-without-availability-consensus.md) / [文書チェックと未完了事項](../adr/evidence/0177-model-distribution-policy-alignment-2026-10-10.md)。


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

The patch applies on wiki commit `bd205c8`. It predates the 2026-09-26 node update and the 2026-09-27 DAA-750 fence release: `Home.md`, `Quick-Start.md`, `Operations-Notes.md`, `Testnet-12-Operator-UI-JA.md`, `Testnet-12-Verification-Participation-JA.md` and `PALW-Roles-and-Network-Scope-JA.md` here were updated afterwards to name `c3dbaee3c`, so publish with the `cp` form (or apply the patch, then copy those pages). Once the wiki carries these pages, this directory can
be deleted.
