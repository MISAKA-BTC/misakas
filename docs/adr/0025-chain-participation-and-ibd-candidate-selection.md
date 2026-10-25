# ADR-0025 — Chain participation and IBD candidate selection

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Status:** proposed
**Date:** 2026-08-09
**Supersedes nothing. Constrains:** IBD source selection, the sync-status RPCs, every signing path.

## What went wrong

Testnet-22 split. A node ran IBD against the first peer that relayed to it, adopted that peer's
chain wholesale, and then mined and attested on it. Other peers offering a heavier chain were
discarded at the relay guard — unfetched, uncounted, unremembered. Which chain a node ended up on
was decided by arrival order.

Two failures stacked. The node had no way to compare chains, and no way to withhold participation
while it worked out which one was right. Either alone is survivable; together they turn one
unlucky race into a fork with miners and validators committed to both sides.

## What this changes

**A node that adopts someone else's chain does not participate until it has reviewed it.** One
gate — `ChainParticipationGate` — with four states (Ready, IbdRunning, CandidateReview,
Quarantined), consulted by mining, both validator paths, compute, and the three sync-status RPCs.
It closes when an IBD starts, not when one finishes, because `staging.commit()` happens partway
through: waiting for success leaves a window where the node is already running the new chain and
still signing on it. It is persisted, because a quarantine a restart clears is not a quarantine.

**Chains are compared on verified evidence, never on claims.** Peers' offers are collected in a
registry keyed by chain rather than by peer, so losing a connection is a source failover rather
than a reason to redo the decision. What a peer says about its chain arrives as `ClaimedBlueWork`,
a newtype with no ordering against verified work: the compiler enforces that a claim may decide
what order to CHECK candidates in and nothing else. Adoption requires a validated pruning proof
and compares verified tip work under the canonical `(blue_work, hash)` fork-choice order.

**Two permits, not one.** `CandidateValidationPermit` authorises spending resources to check a
candidate — reversible, cheap evidence. `CandidateAdoptionPermit` authorises replacing the chain —
issued only against verified superiority, and binding on the defender it was compared with, so a
permit cannot be redeemed after the situation has moved.

**DNS scores never rank chains.** A verified trusted checkpoint (`--trusted-checkpoint`) is a hard
admissibility constraint: a chain that does not descend from it is refused outright. Within what
is admissible, GHOSTDAG decides. A DNS seed can therefore restrict the set of acceptable chains
and cannot choose among them.

## What this does not claim

This is convergence after a fork, not prevention of one. It does not address UTXO, overlay, or
registry consensus determinism — if two nodes disagree about the result of applying the same
blocks, everything here still runs, and everything here still converges them onto the same
*chain* while they continue to disagree about its *state*. That is a separate problem and this
ADR does not touch it.

## Consequences

**A node held back is a node not mining.** The gate fails closed, so every bug in it costs
availability rather than safety. That is the intended direction and it is not free: the review
floor is 180 s after an IBD, extended while any proof-backed challenger is still being weighed.

**Restarting mid-IBD quarantines the node.** An interrupted IBD leaves the active consensus in a
state nothing can vouch for, so the node comes back Quarantined and stays there until an operator
looks. This is deliberate and it is operator-visible toil; the alternative is guessing.

**The switch cap is 5 and it persists.** Two branches trading the latch is a different failure
from being on the wrong one, and a node cannot tell them apart alone. At the cap it quarantines.

## What convinced us

Unit tests cover the policy. They are not what found the defects.

The defects were found by a randomized soak (20 rounds, seeded per round, latency 5–500 ms,
peer order and arrival gap randomized, link cut and healed mid-flight) against two independently
mined chains past pruning depth — and by one round on real infrastructure across a 267 ms
intercontinental hop, which crashed the node within a minute through a state loopback is too fast
to produce.

Five defects in this work shared one shape: **the information needed to make the decision existed,
and nothing was driving the step that would have used it.** Discovery, expiry, adoption, and
verification each had an edge-triggered path with no level-triggered fallback. The fourth instance
sharpened the rule enough to state it: a driver placed where work is *scheduled* rather than where
the resource is *free* never runs for the peer whose resource is busiest — which is reliably the
peer that matters. That rule is now recorded next to the tick it explains.
