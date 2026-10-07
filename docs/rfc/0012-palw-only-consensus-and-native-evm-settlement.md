# RFC-0012 — PALW-only consensus and native EVM settlement: retire DNS validators and their reorg veto

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* Status: **Draft, 2026-10-06 — direction requested; not implemented or activated.**
* Source baseline: `3b09a814ed4b2a2f90009c05c1219efc9be46b5f` on `misakas/main`.
* Requested direction: remove DNS validators, DNS finality, DNS attestations, precommits,
  DNS-final and the DNS/stake reorg veto; let PALW block production and settlement carry the EVM bridge.
* Activation: a new coordinated, network-specific consensus fence; **no height assigned**.
  This is not a claim that DAA 5,300 already removed these dependencies.
* Would supersede, at activation only: the retention of DNS validators and their reward pool in
  [ADR-0126](../adr/0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md),
  the DNS BFT authority of
  [ADR-0128](../adr/0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md),
  and the DNS-based EVM `safe` tag / optional pause in
  [ADR-0109](../adr/0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md).
  Historical validation and pre-fence entitlements remain governed by their original rules.
* Builds on: [RFC-0008](0008-palw-claim-backed-consensus-blocks.md),
  [RFC-0010](0010-permissionless-palw-panel-and-claim-completion.md),
  [RFC-0007](0007-palw-verification-certificates-and-algebraic-checks.md),
  [RFC-0011](0011-permissionless-model-and-long-context-onboarding.md),
  [ADR-0127](../adr/0127-palw-settles-on-its-own-and-its-terms-are-not-dns-terms.md),
  [ADR-0129](../adr/0129-a-double-spend-needs-the-anchors-not-the-blocks.md),
  [ADR-0020](../adr/0020-selected-parent-evm-lane.md), and
  [ADR-0089](../adr/0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md).

## 0. Decision

**PALW is the sole live consensus authority. No DNS validator set, bonded-stake vote,
attestation quorum, precommit lock, DNS-final anchor or stake reorg veto may approve, delay,
override or block post-fence PALW chain selection or native UTXO/EVM settlement.**

The normal path is:

`LLM execution → committed claim / uniquely accounted work slices → PALW blocks →`
`Panel verification + challenge/court lifecycle → PALW settlement → native UTXO/EVM state`.

Block inclusion and optimistic EVM execution can precede claim settlement; the arrows do not
require a producer to wait for `Final` before carrying an EVM payload. Safe settlement is the
later reader-visible condition. No separate validator certificate is required at either stage.

“Remove validators” means **remove the DNS finality-validator role**. Deterministic full-node validation, PALW producers, signed computation evidence, courts, public bonds and objective slashing remain. PALW Panel seats remain in the transitional mode; their honest majority is not the arithmetic trust root. A later Panel-free mode is separately deferred under RFC15 until RFC14 and that mode's own gates pass. DNS here is the finality overlay,
not domain-name resolution or DNS peer discovery. Those unrelated facilities remain.

The bridge in this RFC is the **native MISAKA UTXO ↔ MISAKA EVM accounting transition on the
same L1**, not an Ethereum/other-chain bridge. An external chain cannot inherit PALW settlement
without its own proof verification, destination rules and security analysis.

## 1. What must actually change in today's code

| Existing dependency, verified at the source baseline | Required replacement |
| --- | --- |
| `processor.rs::dns_reorg_outcome` calls `dns_bft_gate_refusal` **before** PALW candidate comparison. `dns_bft.rs` rejects abandonment of the DNS-confirmed anchor until its staleness rule releases it. | No DNS veto on post-fence selection, including extension, sibling race, deep reorg and sink search. Keep the applicable PALW comparator and its independently reviewed safety checks. |
| `palw_fork_choice.rs::compare_palw_candidates_v1` orders frontier, safe weight, bounded live total and deterministic tie-break; pipeline fences add further PALW protections. | Reuse the full PALW selection path, not a replacement “highest DAA” or raw header-work selector. Audit every selection/IBD/pruning entry point. |
| `processor.rs::update_evm_canonical_heads` takes `safe` from `last_dns_confirmed_anchor`, otherwise falls back to `sink`; `finalized` uses a pruning result, with an initial sink fallback. | PALW-derived heads with explicit unavailable/bootstrap state; neither missing DNS nor missing history may promote an unconfirmed sink to `safe`/`finalized`. |
| `bridge_finality_is_fresh` reads DNS state. ADR-0109 defaults to `Label`, but retains the `Pause` policy. | No DNS freshness gate in template assembly, claim RPC, wallet or relay. Retire `Pause` as a DNS-dependent policy on the new network rules. |
| `validate_evm_deposit_claims` and `apply_evm_bridge_effects` already enforce lock consumption and withdrawal UTXO effects. | Preserve and test this same-chain accounting, executed against one canonical state generation; do not add a bridge signer committee. |
| ADR-0126's t12 split allocates 20% subsidy to DNS validators and 72% to PALW claim escrow, with 8% inclusion. | Retire future DNS allocations through one reconciled reward/fee schedule; clear historical liabilities without re-minting them (§6). |
| `dns_finality` also houses shared bond, payout, snapshot and historical validation types; PALW uses key-loading utilities under validator-named crates. | Extract shared functions/types before deleting services. A module name is not proof that everything in it is obsolete. |

Primary source paths:

* [Virtual processor](../../consensus/src/pipeline/virtual_processor/processor.rs), especially
  `dns_reorg_outcome`, `update_evm_canonical_heads`, `bridge_finality_is_fresh`, snapshot import
  and template construction.
* [DNS BFT gate](../../consensus/src/pipeline/virtual_processor/dns_bft.rs),
  [PALW comparator](../../consensus/core/src/palw_fork_choice.rs),
  [shared/historical overlay types](../../consensus/core/src/dns_finality.rs).
* [Native EVM accounting](../../consensus/src/processes/evm/mod.rs), especially
  `validate_one_deposit_claim`, `validate_evm_deposit_claims`, `apply_evm_bridge_effects`.

Stopping validator daemons, hiding their explorer tab or setting `Label` alone does **not** implement
this RFC. All live dependencies above must disappear together; historical readers may remain.

## 2. One authority, without an unadvertised replacement committee

1. At the new fence, DNS bond weight, voting snapshots, inactivity leak, attestations, precommit
   locks and `last_dns_confirmed_anchor` contribute **zero authority** to live fork choice.
   Do not retain an emergency `StakeScore` fallback or reintroduce DNS through pruning/IBD.
2. Preserve PALW validity, accepted-transaction provenance, maturity, bounded immature weight,
   strict-win/tie rules and missing-data fail-closed behavior as resolved by the active schedule.
   A candidate lacking evidence is unresolved, not a reason to switch to stake or raw blue work.
3. Header-only work ordering remains a download hint, not a second canonical-chain authority.
   Virtual state, UTXO acceptance, EVM heads, sync and pruning must agree on one validated history.
4. PALW Panel receipts certify computation, not a BFT vote for a preferred chain. No new
   `PALW precommit`, committee checkpoint or “Panel-final reorg veto” may replace the deleted DNS
   gate under a new name. Existing PALW fork-choice protections are not abolished by this rule.
   [RFC07 Part VI](0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)'s
   proposed verification randomness is a fact derived from qualifying future canonical PALW work,
   not validator voting, DNS finality, committee/threshold signatures or a new settlement authority.
   Its branch-relative BeaconLocked predicate neither forbids a valid PALW reorg nor adds a finality overlay.
5. RFC08 multiple blocks per claim must bind `(claim, slice, work budget)` and prevent replay,
   duplicated weight or multiple rewards for the same work. More carrier blocks do not imply
   more independent verified work, independent challenges or settlement confidence.
   Under revised RFC08, both EXEC_TX and EXEC_SLICE have zero fork-choice/DAA contribution;
   slice work settles once at root Final and cannot count as extra chain confirmations.
6. Heartbeat/floor are bounded recovery facilities under their active validity rules. They may
   carry state transitions where permitted and advance timeout machinery, but are not substitutes
   for useful-work evidence in the bridge confirmation policy below. This RFC does not silently
   activate RFC08 or claim that today's floor-heavy production is already useful-work dominant.

## 3. What “settled” means after DNS-final is gone

Three different facts MUST remain distinct:

* **Execution validity:** a claim passed the active verification and dispute rules and reached
  `Final`. Under RFC07/RFC11 this can have quantified probabilistic soundness, conditional on
  coverage, commitments, randomness, evidence availability and the stated adversary assumptions.
* **Canonical acceptance:** a block and its UTXO/EVM effects are accepted on the PALW-selected
  history currently verified by a node. A BLUE label or `Final` claim alone is not irreversibility.
* **Settlement confidence:** the relevant accepted effects have sufficient PALW-backed history
  under a published confirmation policy. There is no DNS/BFT certificate after retirement.

Define a versioned, deterministic `PalwSettlementPolicyV1` (proposed type, not an existing API)
and its derivation from a specified canonical state snapshot. Parameters include required
settled-anchor depth `D`, additional **unique eligible matured useful-work** `W`, work-concentration
limits and the applicable challenge/DA lifecycle. Values and their security justification are
activation blockers; this RFC assigns neither arbitrary confirmation counts nor a time promise.

For an executed EVM result at carrier `B`, its safe eligibility requires:

1. `B` and its execution/input provenance belong to the validated selected history; the EVM result
   and corresponding UTXO effects are available and root-verified.
2. `B` is covered by the PALW settlement frontier, with the active provenance/claim-lifecycle rules
   satisfied. Missing data or a pending dispute that prevents settlement cannot be treated as success.
3. Qualifying settled anchors and unique matured work at/after the accepted effect meet `D` and
   `W`. Count shared-claim slices using their bounded work budget, not as independent security
   trials. DNS votes, heartbeat ticks and floor-only work add nothing to these useful-work metrics.
4. Derive a **prefix**, not disconnected safe blocks: if an earlier required dependency is
   unresolved, a later large claim cannot skip it. The mapping from the PALW frontier to eligible
   EVM results must be explicit and identical in live processing, restart and snapshot import.

This settlement policy labels risk; it MUST NOT introduce another tip-selection veto. Its depth
and weight bounds require analysis of the actual frontier-first comparator, not a copied PoW
confirmation formula. A private fork may have colluding Panel seats: “a private fork cannot get
receipts” is not a security proof. Test private maturation, concentrated stake/work, compromised
seats, withheld evidence, partitions and adaptive corruption without relying on DNS to stop them.

Execution-check error, Panel compromise, randomness grinding, DA failure and chain reorganization
are separate failure events. A Freivalds/GKR error bound is **not** a bound on total chain-finality
risk. Publish assumptions and composed risk accounting before choosing the policy parameters.

## 4. Native EVM bridge: one ledger transition, no validator approval

### 4.1 Deposit / withdrawal invariants

Retain ADR-0020's selected-parent execution order. A carrier and the child that executes its payload
must not be conflated by the UI, receipts or confirmation calculation.

* Deposit: an accepted `EVM_DEPOSIT_LOCK` outpoint is found by the reconstructible lock index;
  producers prepare its system operation against the same state generation as their template.
  Validate destination, amount, tip, unspent status and timeout; consume the outpoint once and
  credit the EVM account once. No DNS attestation, second depositor signature or bridge signer is
  required for automatic claiming of an already valid lock.
* Preserve the existing claim/refund boundary: claim acceptance requires DAA **less than** timeout;
  at/after timeout the refund route owns the lock. Preserve the same-block visibility restriction.
  The two paths must be exclusive even across merges, delayed execution and reorgs.
* Withdrawal: deterministic EVM execution debits the account and materializes the protocol-defined
  UTXO output exactly once, with existing destination, scaling, precision and amount checks. It is
  not a request that a validator multisig must honor.
* Commit UTXO diffs, EVM state/results, consumed-lock and withdrawal identities, market settlement
  effects and canonical head/index changes atomically or through a crash-safe transaction protocol
  with equivalent recovery guarantees. Reorgs undo/reapply **both** accounting domains together.
* Preserve EVM nonce/gas/replay protection and ADR-0089 order → fold → child settlement ordering.
  Removing DNS is not permission to spend unsatisfied market orders or change membership pricing.

Conservation tests must cover native units and EVM scaling: deposits/withdrawals transfer backing,
not create subsidy. Include locked funds, credits, fees/tips, refunds, escrow, sinks and burned units
in the ledger reconciliation. A retry, restart, duplicate carrier or alternative branch cannot
increase the combined spendable supply.

### 4.2 Progress and reader-visible heads

| Surface | New meaning |
| --- | --- |
| `latest` | Latest fully executed, canonical EVM result under PALW; optimistic/reorgable. Never just an unexecuted advertised tip. |
| `safe` | Deepest canonical EVM result in the prefix satisfying §3's published PALW policy. No DNS lookup; no fallback to an unsafe sink. |
| `finalized` | Conservative PALW-settled prefix also satisfying validated pruning/checkpoint safety. A pruning snapshot by itself is not evidence of settlement. This is protocol/economic finality under stated assumptions, not a validator certificate or unconditional mathematical finality. |
| Native bridge status | Lock seen → accepted → claim included → EVM executed / withdrawal materialized → PALW safe → finalized policy satisfied; also refund, reorged and failed states. Include carrier/result hashes and precise unmet conditions. |

Where a tag has no qualifying EVM result, return an explicitly documented unavailable result/error
for that RPC method and expose the reason; do not substitute `latest`. Extend internal head types
to represent absence. Whenever present, `finalized` is an ancestor of `safe`, which is an ancestor
of `latest`, in **executed EVM-result order**.

If useful work or verification stops, optimistic inclusion can continue under the permitted
recovery rules, but useful-work settlement waits and reports why. No DNS quorum can unpause it and
no number of heartbeats fabricates safe work. Timeouts may still advance for refunds/courts.
Wallets must distinguish an optimistic spendable balance from a safe balance, without promising
that low block latency equals external-payment finality.

Recompute `safe` on legal reorgs and emit changed/removed logs and transaction status. Do not pin
it by an RPC-local checkpoint. A conflict below `finalized`/pruned history requires the documented
resync/safety-failure procedure and an explicit alarm, never a silent relabel or DNS veto revival.

## 5. Retire the overlay without erasing history or locking users' bonds

Introduce a proposed `palw_dns_retirement_v1` fence, scheduled only for a compatible PALW network.
Its consensus parameters, reward schedule, transaction retirement matrix and snapshot version are
part of the fingerprint/schedule commitment. Changing a CLI flag is not activation.

Before scheduling, specify the fence predicate at **each** call site: earning block, paying block,
accepting transaction view, incumbent chain, candidate chain, EVM execution and snapshot boundary.
No candidate may choose its own activation state to bypass a live rule. Crossing/forking around
the fence must converge independently of arrival order; the exact transition algorithm and tests
are release prerequisites, not left to a generic `if daa >= H` patch.

| Operation/state | At and after the fence |
| --- | --- |
| New DNS validator bonds, activations, attestations and precommits | Retired; no duties, admission to new live participation, vote weight or new reward accrual. Define explicit mempool rejection and consensus treatment for each legacy transaction kind, including merged transactions and crossing-boundary carriers. |
| Pre-fence DNS votes, anchors, snapshots and historical blocks | Retain sufficient versioned data/readers to replay and verify old history. They confer no post-fence veto or RPC safety label. |
| Legacy validator bond exits | Keep a bounded, deterministic exit/withdrawal path. Retirement never confiscates principal or demands a now-impossible fresh attestation/quorum to exit. Preserve existing liabilities and applicable cooldowns. |
| Pre-fence misconduct | Preserve a finite evidence/contest/settlement horizon with the original offense rules. Historical slashing cannot become a new live finality vote. State exactly when residual exposure ends. |
| Shared PALW bonds / keys / services | Separate by explicit role and ownership. Do not unlock a PALW producer/seat exposure merely because it shares a key, crate or record type with a DNS validator. |
| Remaining DNS reward/quality reserves | Snapshot and settle lawful pre-fence entitlements through deterministic accounting (§6), without requiring post-retirement participation. |

Stopping the DNS-only service is safe only after the fence and its duties are retired. Extract shared
ML-DSA key loaders and bond/payout helpers first. Keep historical codecs and replay as long as supported
networks need them; delete live voter/aggregator scheduling, not consensus history indiscriminately.

## 6. Monetary policy: no DNS reward, no extra issuance

The proposed normal **eligible PALW block** split for the current 72/8/20 t12 baseline is:
**92% PALW claim escrow, 8% inclusion, 0% DNS validator subsidy**. The removed 20 percentage points
join the existing claim escrow; its producer/Panel allocation and work-price/market rules remain
those of the active PALW economy unless separately amended. This is an explicit proposal, not a
statement of the currently shipped split.

The released share MUST NOT become an immediately spendable unconditional producer reward.
It follows verified work, escrow release at the proper lifecycle stage, and void/burn rules.
Heartbeat/floor do not acquire a 92% useful-work escrow by default: preserve their separately bounded
recovery issuance and do not mint the redirected share where no eligible claim can back it. Publish
the complete block-kind matrix before activation, including treatment of any unallocated amount.

Resolve all subsidy, ordinary/finality-fee, inclusion bounty, deferred quality, reserve-drip and
slashing allocations together. No fresh fee stream may accrue to a retired validator pool. Redirect
new DNS fee shares into the versioned PALW/inclusion fee schedule, distinct from the subsidy split;
exact integer rounding and destinations must be frozen before release. Pre-fence balances are not
fresh fees: settle their recorded liabilities once, then handle residual reserves under an explicit
auditable rule. Do not silently sweep users' old claims into the new budget.

For cross-fence merged/attempt rewards, preserve the invariant `escrow <= actually withheld base`
at the payer. ADR-0126's paired earning/paying-score rule is a required regression case, not a license
to retrospectively raise an old claim's escrow from 72% to 92%. Test every score pairing, delayed
release, slash, void, replay and rounding remainder. The emission ceiling never increases.

## 7. RPC, tooling and operator migration

* Add a versioned settlement-status response with ruleset/policy id, canonical generation, executed
  head, PALW frontier, safe/finalized results or absence, unique work/depth and unmet conditions.
  This is derived data, not an oracle. Cache and index state must be rebuildable from verified history.
* Retire `getPrecommitDuty` and DNS attestation duties explicitly (`retired`, fence and reason).
  Keep historical queries distinct from live capabilities. Old clients must not treat empty duties
  as node failure or interpret legacy DNS-final data as current safety.
* Remove DNS validator onboarding/current quorum panels from SDK, CLI, explorer and wallet normal
  flows. Keep historical bond exit/reward/evidence tools through their migration horizon. Use
  “PALW Panel” for compute verifiers and name the remaining trust assumptions plainly.
* After the fence, reject or clearly retire DNS-dependent `Pause` configuration rather than leaving
  an EVM lane stuck waiting for nonexistent votes. No operator override may change block validity.
* Snapshot import/restart must derive or verify heads, settlement summaries, legacy liabilities and
  EVM backing against committed history. Never initialize `safe` or `finalized` from the sink merely
  because a field is missing. Old snapshot/peer versions require explicit compatibility negotiation.

## 8. Implementation sequence and release gates

1. **Dependency and security inventory.** Enumerate live DNS reads/writes, stake fallback paths,
   transaction kinds, payouts and snapshot formats. Analyze PALW without the veto: private maturation,
   adversarial Panel concentration, bootstrap randomness, DA/court censorship, long-range attacks and
   partitions. State the genesis/trusted-sync assumptions; no unproved “PALW is equally safe” claim.
2. **Separation with legacy equivalence.** Move shared helpers, freeze old-history golden vectors,
   add settlement policy types and reconstructible summaries. Below-fence roots, accepted sets,
   fees, payouts and EVM results must remain byte-identical.
3. **Dormant coordinated implementation.** Implement retirement, bond/reward wind-down, all fork-choice
   entry points, bridge heads and client capability handling behind the same reviewed schedule.
   Do not deploy a fork-choice-only patch and leave accounting or RPC semantics inconsistent.
4. **Adversarial multi-node drill.** Run full replay, pruning/IBD and crash recovery with zero DNS
   validators. Publish artifacts/results and settle policy parameters and resource bounds.
5. **Schedule only after the gates pass.** Publish fingerprint, height, compatibility matrix,
   bond exit procedure and release notes. Old nodes diverge after the fork and must not participate
   as compatible peers. After activation, reverting binaries is not a safe rollback; any corrective
   protocol transition needs a coordinated plan preserving all accepted financial effects.

Mandatory acceptance matrix:

| Test | Required result |
| --- | --- |
| Zero DNS validators, healthy PALW work/verification | Claims settle, canonical chain progresses, native deposit/withdrawal and market orders complete without DNS services or votes. |
| DNS-final anchor opposed to valid PALW candidate; absent/all-equivocating DNS votes | Post-fence PALW selection/heads are unchanged by DNS state; historical evidence can affect only legacy liabilities. |
| Sibling races, deep forks, strict-win/tie rules, private mature claims, partitions | Identical eligible histories converge under the specified PALW rules; security failures cannot be masked by reinstating stake veto. |
| One claim copied into many blocks; repeated work slices | No duplicate useful-work budget, reward or artificial independent settlement depth. |
| Heartbeat/floor only, withheld receipts, missing DA, open court | Permitted recovery proceeds; `safe` does not advance without its eligible PALW requirements. No fabricated latest-as-safe fallback. |
| Reorg before/after deposit claim, refund boundary, withdrawal and market settlement | Root/state equality after undo/replay; no double credit, double refund, missing debit, repeated withdrawal or supply inflation. |
| Child executes parent payload; pending/noncanonical payload | Correct execution provenance; unexecuted or detached effects never labeled safe. |
| Old/new earn-pay score pairs; unbond/slash/reward backlog | Coinbases agree with construction; no locked-forever principal, lost lawful entitlement or unbacked escrow. |
| Restart, crash between store writes, pruned sync and legacy snapshot migration | Same canonical state, backing and head tags as full replay, or explicit unavailable/resync status—not silent promotion. |
| Removal audit | No live DNS finality dependency in PALW selection, EVM bridge or wallet safety; allowlisted legacy readers and shared utilities are justified individually. |

## 9. Completion and non-claims

RFC12 is implemented only when DNS validators are unnecessary **and incapable of exercising a live
veto**, pre-fence funds/history remain correct, and PALW alone supplies the documented native EVM
settlement path across replay, reorg and pruning. A renamed API or a stopped daemon is insufficient.

This document does not prove PALW common-prefix security, set confirmation parameters, remove the
need for independent computation verification, make arbitrary external bridges trustless, or promise
safe finality while useful work is absent. Those limits are not grounds to restore DNS secretly;
they are measurable security and liveness conditions that must be addressed before activation.

## Mission alignment amendment — 2026-10-07

DNS authorityの撤廃とPALW computation accountabilityは維持する。PALW Panelを残す記述は移行中の通常処理を指し、正直な多数派を安全性前提にしたり永久の固定Panelを義務づけたりしない。計算の客観的裁定はDNS vote、Panel vote、EVM executionで代替しない。RFC14全gateとRFC15固有のsettlement/monitoring/migration条件が揃うまではPanel=0を有効化しない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。
