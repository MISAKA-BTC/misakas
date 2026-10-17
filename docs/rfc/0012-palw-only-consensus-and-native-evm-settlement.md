# RFC-0012 — PALW-only consensus and native EVM settlement: retire DNS validators and their reorg veto

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* Status: **Draft — dormant implementation; no activation or production settlement parameters assigned.**
* Spec closures, 2026-10-15: [R12-* requirement groups](#spec-closure-2026-10-15) — normative rules and named parameters; no PASS or activation implied.
* Revision, 2026-10-10後続改定: §2.1 withdraws model-availability consensus and adds model-bond coinbase allocation under unchanged total issuance and individual ceilings. Rule E and claim-specific adjudication remain separate gates; the dormant DNS-retirement work does not implement this amendment.
* Implementation record: [unactivated fence, native heads, monetary transition and remaining release gates](../design/palw/integration/rfc-0012-dormant-implementation.md).
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

### 2.1 Model acquisition non-interference, allocation and Rule E — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
旧必須Torrent、独立Bonded Seeder、PoR/Full Fetch/lease/TRDC/FPRと取得不履行に基づく資格/weight停止を撤回する。
model配布は任意のTorrent/mirror/共有契約で行い、取得失敗・非公開だけでSlash/Final延長を起動しない。

固定Model ID/root、weights/tokenizer/config/specは不変とし、変更は新Model IDとして登録する。
取得bytesは認証と全量復元rootで照合し、claimも同じroot/specをbindする。
任意のinfohash対応を登録する場合はimmutable bindingとし、必須取得経路にしない。
claim固有state/trace/output/witnessの有限な証拠責任は維持するが、modelの全量/反復range公開へ転用しない。
必要operandの認証と許可unit/累積scopeをADR177 D2で確定し、modelを得たfresh non-seat verifierの
計算反証/localization/exact courtを試験する。全model保有者拒否時の取得保証は撤回する。

重複しないminer拘束元本`S_m=sum_b C_{b,m}`からmodelのcoinbase予算を`f(S_m)`で配分する。
同じ元本をclaim数/rho/複数modelで増幅せず、総発行予算と個別bondのQ/B/R/F cap・共通DAA拘束を維持する。
資本預託だけでは支払わない。model別配分はblock頻度・DAA・fork choiceを自動変更しない。
公開参加資本が閉鎖自己資本より圧倒的に有利になることは倍率式・経済評価の未立証目標であり、
同額資本の所有者独立性をchainが識別できるとはしない。既存minerの実収入増も無条件保証しない。
non-interference、distinct-capital、三層予算、公開/閉鎖比較、回復/移行のgateはADR177 §3を用いる。
既存実装/測定・旧claim/activation記録は保持し、新配分・Panel=0は未実装/未有効化である。

EXEC_TX/EXEC_SLICEのweight/DAA=0、unique-work、root単位一回精算を維持する。
適用modeのclaim固有Final/court条件・暫定creditとsafe settlement・規則Eの全writer/reader/undoを検証する。
model配布不能だけの停止を起動せず、閉鎖時の未検出workを必ず排除できるとは主張しない。

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
*(2026-10-15: the parameters, their invariants, the 2026-10-08/10-11 drill-provisional rulings and the
open-court/EXEC-horizon/maturity rules are specified in [Spec closures — 2026-10-15](#spec-closure-2026-10-15), R12-SETTLE.
The drill values remain USER DECISION REQUIRED for production.)*

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
are separate failure events. A direct randomized checker's conditional miss rate does not
bound total chain-finality risk. Consensus credit authenticates selected verifier approvals
under an explicit honest-fraction assumption (PRINCIPLES §8); cryptographic model-computation
proofs are prohibited. Publish checking scope, participation, availability, randomness and
reorganization assumptions before choosing the policy parameters.

### 3.1 Publication history is separate from the current certificate

A recomputed `finalized` may retreat to `null` when evidence no longer certifies it. That MUST NOT erase
which head the node previously published as finalized. Retain a versioned local publication record,
including its ruleset, policy, retirement fence and generation, in the same durable batch as the native
snapshot and EVM heads. Advance its conflict anchor only to an equal or descendant finalized head.
This record is incident history: it cannot certify a head, supply useful work, or veto fork choice.

Every post-retirement evaluation MUST check the selected sink against the retained publication, including
after an ordinary withdrawal, restart, or rollback across the retirement fence. Abandoning that head
produces sticky `FinalizedConflict`, with safe/finalized absent. A canonical withdrawal is named by
`readiness.finalized.withdrawnFrom` after restart; a conflict has its own alarm. Current labels remain
subject to root, provenance, generation and ordered-prefix checks, independently of this history.

Only a validated pruning-point execution-state import/resync may clear both publication history and the
sticky snapshot, atomically with imported execution state and absent safe/finalized heads. Invalid imports
preserve them. The readiness memo MUST be invalidated even when the imported generation is unchanged.
Corrupt or incompatible history is preserved and makes readers fail closed. An older positive snapshot
can bootstrap a missing publication row; an older negative snapshot cannot distinguish no publication
from a forgotten withdrawal and requires validated recovery. An upgrade must not silently assume that
such a node never published a finalized head. The dormant public fence provides no deployed migration
evidence; local store tests and in-process fork tests do not establish crash/P2P/pruned-IBD guarantees.

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

### 4.3 Pruning import authority and repeatable recovery

An EVM header and account snapshot reproducing a block's commitments establish execution data for that
block; they do not authorize resetting settlement incidents at an arbitrary block. EVM pruning import
MUST target the node's established pruning point, installed by the existing validated headers-proof or
catch-up path. Check that target before expensive validation, then recheck and hold it through the import
batch. An unreadable target/header is an explicit error, not a panic or an assumed empty state. These
checks do not replace proof validation or change branch selection.

Retrying a committed import, including after restart before IBD's stable flag was set, MUST succeed for
identical canonical execution headers, snapshots and checkpoints. Complete missing rows independently;
do not treat an existing row as proof of success or overwrite contradictory/unreadable data. Verify all
existing rows before staging flat-state/head changes or clearing publication history. A checkpoint's
block, EVM number, root, payload and checksum must agree with the validated input. Current snapshot
canonicality and code/hash checks remain required. Storage corruption requires validated recovery;
ignoring a read error or deleting a conflicting row merely to make a retry succeed is forbidden.

The pruning-history context import must also accept identical existing samples. Validate nonempty history,
network genesis, uniqueness, monotonic blue score, immutable sample consistency and required header shapes
before changing history. Keep its target/retention update coherent under the context lock. The durable EVM
import batch and explicit IBD stable flag still form a recovery protocol; this subsection does not claim one
transaction spanning all UTXO/PALW/EVM sidecars. Real transport, interrupted imports, proof adoption and
cross-ledger recovery remain acceptance requirements under §§4.1 and 8.

A checkpoint-only chain row has no delta evidence and MUST NOT be cached as an ordinary row keyed only
by block hash. Evict previously ordinary evidence when that block becomes the current checkpoint;
read its root-verified execution separately. If context later moves below it, require its own delta
again and stop at missing history rather than inherit an empty contribution from the former boundary.
Readiness memo reuse requires both sink and pruning point to match. A successful import still resets
the memo after commit; context freshness is not permission to clear publication history or incidents.

The RPC response combines committed settlement and a current advisory evaluation. Equality of sink
alone does not establish agreement if pruning context moved between publications. Attach readiness only
when its version, policy and shared certificate statements (generation, safe, finalized and stop) agree
with the returned snapshot. Otherwise omit the explanation until a coherent publication exists; do not
substitute optimistic heads or invent a certificate. This coherence filter verifies agreement between
already validated local reads, not cryptographic soundness or a transaction across all sidecars.

## 5. Retire the overlay without erasing history or locking users' bonds

Introduce a proposed `palw_dns_retirement_v1` fence, scheduled only for a compatible PALW network.
Its consensus parameters, reward schedule, transaction retirement matrix and snapshot version are
part of the fingerprint/schedule commitment. Changing a CLI flag is not activation.

Before scheduling, specify the fence predicate at **each** call site (2026-10-15: the resolved call sites are listed in
[Spec closures — 2026-10-15](#spec-closure-2026-10-15), R12-AUTHORITY item 2; EVM execution and snapshot boundary remain open): earning block, paying block,
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

*(2026-10-15: this split is **not adopted**. The dormant code commits it only inside the unarmed fence's policy values;
production split and fee schedule are USER DECISION REQUIRED — see [Spec closures — 2026-10-15](#spec-closure-2026-10-15), R12-MONEY.)*

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

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](0015-panel-free-permissionless-verification.md)を適用する。

PALW-only/native EVMの将来経路にも同じbond/DAAのblock/reward/Final weight予算を適用する。Finalというphase名だけを未予約full creditやbridge safeの根拠にしない。
会計上の配分、保留dispute、残存責任、経済Finalを全head/writer/readerで一致させる。旧DNS/clockの撤去は別upgradeのままで、本原則から新weight/DAAを暗黙に有効化しない。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。

## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

## 改定 — 2026-10-12（モデル配分とブロック生成機会の接続：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §3A.5。

ADR-0177 2026-10-12改定のモデル配分（Miner＋Panelの適格claim bondと1.3〜2倍の係数、提案）をブロック生成機会へ接続する場合、coinbase配分だけの変更として済ませず、ブロック機会の帰属・上限・移行を本RFCで明示する。物理blockの総頻度、DAA/難易度、fork choice、Final weightを同時に変更するとは決定しておらず、特にFinal weightへ係数を自動乗算しない。必要な変更は別のversioned review・試験・activationを要する。

**X5:** `palw_dns_retirement_v1` は設計上のfence識別子、`Params::palw_dns_retirement` はRustの内部フィールド名であり、異なる階層の名前である。

<a id="spec-closure-2026-10-15"></a>

## Spec closures — 2026-10-15

**Scope and reading rule.** This section closes specification gaps that the
[design audit's requirement index](../audit/palw/design-reviews/rfc-0006-0007-0008-0012-design-audit-2026-10-10.md#完全解消を判定する要件の索引)
lists for R12-AUTHORITY, R12-SETTLE, R12-BRIDGE, R12-HEADS, R12-MONEY and R12-OPS. Where this RFC's body is silent
or contradicts a later design record, ADR or user ruling, this section governs; superseded body text is kept
and annotated, not deleted. [PRINCIPLES](../PRINCIPLES.md) and ADR-0173/0176/0177/0178 rank above it. Nothing
here is an implementation PASS, an activation, a fence height or a production parameter assignment; every
public preset keeps `palw_dns_retirement_v1` absent. "USER DECISION REQUIRED" marks a named parameter whose
value needs the user's decision; a recommendation appears only where an existing record justifies one.
Mechanics results and private-process acceptance are not independent review or economic evidence.

### R12-AUTHORITY — every selection entry point, ordering and assumptions

1. **Rule E ordering (ADR-0178 §4.6).** A ruleset MUST NOT arm `palw_dns_retirement_v1` unless
   `palw_fork_choice_rule_e_v1` is armed at or below the same height. The public validator continues to refuse both
   until their gates close.
2. **Fence predicate per call site (closes §5's "specify at each call site").** Fork choice resolves retirement at
   the **incumbent** sink (the candidate cannot select its own rule). Transaction admission and template selection
   resolve it at the **accepting** DAA. Claim escrow uses the **lower** of the earning and paying DAA. Historical
   equivocation evidence is admissible only within the accepting-DAA horizon and only when both signed targets lie
   strictly before retirement. The EVM-execution and snapshot-boundary predicates MUST be specified in the same
   form before scheduling; until then this item remains open (it is not left to a generic `daa >= H` test).
3. **No replacement authority.** After retirement no DNS state, Panel receipt, committee/threshold signature,
   RFC07 `BeaconLocked` predicate, settlement label, publication-history row or operator override may veto or
   reorder PALW selection. `safe`/`finalized` labels are outputs of selection, never inputs to it.
4. **Removal inventory as a release artifact.** The release MUST carry an inventory of every live DNS read/write,
   stake fallback, transaction kind, payout and snapshot path, each marked removed or allow-listed with a reason
   (§8 removal audit). A renamed module is not evidence of removal.
5. **Trusted-sync and genesis assumptions.** The release MUST publish a versioned statement of what a new node
   trusts (genesis identity, headers-proof/pruning-point import, any checkpoint) and of the adversary assumptions
   under which PALW-only selection is analysed (private maturation, Panel concentration, bootstrap randomness,
   DA/court censorship, long-range and partition attacks). This RFC does not supply that analysis; absence of the
   statement blocks scheduling.
6. **Model availability is not an input** (ADR-0177, ADR-0178 §10): selection, settlement and readiness read no
   model distribution, peer reachability, acquisition result or lease.
7. Remaining (observations, not verified): multi-process zero-DNS fork/partition/long-range drill, attack tests of
   the certified effect, the EVM/snapshot predicates in item 2.

### R12-SETTLE — `PalwSettlementPolicyV1` parameters and invariants

1. **Open dispute inventory.** Evidence filtering, lifecycle closure and readiness MUST read one shared inventory of
   open DA sessions **and** open computation courts (`native_open_sessions_v1`). An expired retention clock never
   bypasses an open court. A claim with an open dispute contributes no D or W.
2. **Unique work.** W counts each `(canonical work id, quantum index)` once, read from the PALW delta of the block
   that carried the transition (not from rows the sink still holds). EXEC slice work counts once, under the root's
   identity, at root native Final. Carrier copies, heartbeat, floor, execution Round and unspent FP grants add zero.
   A later `Final → Voided` conviction retracts what the `Final` counted.
3. **EXEC consumer horizon.** The Final journal, the native facts and the node lifecycle/readiness MUST carry the
   EXEC consumer horizon of every counted root, so a dependency still inside its liability window is not mature.
4. **Maturity invariant.** A counted fact is mature no earlier than
   `max(accepted + M_maturity, Final + claim_retirement, trace_retention)` and, additionally, no earlier than the
   last applicable court/DA/liability endpoint of that claim (including any G14/OPV/EXEC consumer path). A longer
   new G14 path therefore lengthens maturity automatically (2026-10-11 ruling).
5. **Prefix, not islands.** §3 item 4 stands; an unresolved earlier dependency stops the safe prefix.
6. **Named parameters** (all committed in the fence's policy and fingerprint; all **value: USER DECISION REQUIRED**):

   | Parameter | Invariant | Recommendation and source |
   | --- | --- | --- |
   | `settled_anchor_depth` D | integer ≥ 1; counts distinct qualifying grant/attempt anchors, never slices | recommended **3** — 2026-10-11 drill-provisional ruling; with a 400 ‰ operator cap three operators is the minimum that can satisfy it ([policy proposal](../design/palw/execution/rfc-0012-policy-proposal.md)) |
   | `unique_mature_work` W (pwu) | > 0; distinct from ADR-0176's budget window | recommended **6,331,093,440** as a drill floor (= 3 × smallest REAL claim); production value from a census `⌈k × matured REAL work per window⌉`, which does not exist yet |
   | `operator_concentration_cap_permille` | 1..1000 | recommended **400** (2026-10-11 drill ruling) |
   | `class_concentration_cap_permille` | 1..1000 | recommended **1000** for now; re-evaluate **600** once ≥ 3 REAL classes have mature work (2026-10-11 ruling) |
   | `M_maturity` (DAA) | ≥ accepted+bind+receipt+challenge+court of the counted rules | recommended **5,400** (DERIVED in the policy proposal); per the 2026-10-08 ruling it is neither shortened nor treated as final without the formula derivation, the 5,400/3,000/1,000/600 comparison and the attack tests |
   | `legacy_evidence_horizon_daa` | ≥ the DNS `evidence_window_blocks` it preserves | recommended **10,080** (DERIVED from `evidence_window_blocks`) |

   The 2026-10-11 values were adopted for the **drill only**; they are not production values and do not arm anything.

   **Status (2026-10-15, lead record): closed-as-provisional.** The specification of R12-SETTLE is closed with these
   values marked **drill-provisional (user ruling 10-11), production values pending census/measurement**:
   D = 3; W = 6,331,093,440 pwu (revisit with the census); operator concentration 400 ‰; class concentration
   1000 ‰ for now, re-evaluated at 600 ‰ once at least three REAL classes have mature work;
   maturity = `max(accepted + 5,400 DAA, the later retirement/liability deadline)`, so any longer G14 deadline
   lengthens maturity; legacy DNS evidence horizon 10,080 DAA. No preset carries them and no fence is armed.
7. Remaining: census of operators/classes/pwu per window, native court-extension and pruned-provenance paths,
   private-fork attack evidence (validation-only).

### R12-BRIDGE — native accounting

The §4.1 invariants are complete as specification. Added rule: claim/refund exclusivity is evaluated at the
accepting DAA of the executing block, so a merge, delayed execution or reorg cannot make both paths succeed; market
settlement effects belong to the same state generation as the UTXO/EVM effects and are undone with them. Remaining
items are validation-only: reorg/refund boundary rollback, actual market fills, and equality of both ledger roots
after undo/replay.

### R12-HEADS — durability and reader coherence

1. Every import session/recovery marker, each verified sidecar install, the marker preceding virtual UTXO
   replacement and the final stable publication MUST pass a synchronous WAL barrier before the next step. Intrusive
   target resets use the same barrier.
2. Native-retirement virtual commits MUST persist publication history synchronously with their UTXO/selected-chain
   generation before releasing reader guards (extends §3.1).
3. Validated snapshot declarations MUST be installed with the PALW tip, the servable pruning snapshot and the
   incomplete marker in one batch; whole-set validation precedes staging. Genesis PALW delta, initial tip, UTXO-valid
   rows and accepted class-carriage index rows are staged in one batch after the transition is fully derived.
4. An incomplete intermediate virtual copy MUST NOT publish canonical ETH or native tags.
5. Remaining (observations): power-loss durability, atomic initialization of every header/body/virtual store, crash
   between generations under real pruning, finalized-conflict external evidence.

### R12-MONEY — split, fees and retained liabilities

1. **The §6 split is not adopted.** `92 / 8 / 0` is a proposal. The dormant implementation commits it inside the
   unarmed fence's policy values only so the mechanics can be tested; that is not adoption. The same holds for the
   dormant rule that normal and native-settlement transaction fees go 100% to the carrier.
   Named parameters `retirement_subsidy_split` (claim-escrow ‰, inclusion ‰, DNS ‰ = 0; sum = 1000) and
   `retirement_fee_schedule` (per-recipient fee shares; sum = 1000; no DNS/service share): **value: USER DECISION REQUIRED**. No recommendation is made here, because the
   model-bond allocation of the claim escrow under ADR-0177 D4 (and its 2026-10-12 proposal) is itself undecided.
2. Invariants that hold for any value: the emission ceiling never increases; escrow ≤ actually withheld base at the
   payer; the retired DNS share is not minted where no eligible claim backs it; old claims keep their recorded
   entitlement (no retroactive 72%→92%); pre-fence liabilities are settled once and residual reserves follow an
   explicit auditable rule.
3. **2026-10-11 rulings applied.** A fee-only Round is not counted in `B_max`; it is limited by a separate,
   capital-proportional `ExecutionCap` charged at allocation. In v1 the market fee is not in `R_max`; expected fee
   income enters the economic-safety computation.
4. Remaining: reconciliation of snapshot generations, old/new earn-pay and legacy horizons on every path
   (validation-only).

### R12-OPS — capabilities, retired kinds, release procedure

1. New DNS bonds (`0x10`), attestation shards (`0x11`) and precommits (`0x19`) are rejected after retirement in
   header context, populated UTXO context and template selection. Bond unbond/exit stays available.
2. Clients MUST check reported durable import status before selecting funds or signing. Under native retirement a
   client requires the import-status capability; a missing capability is accepted only for dormant legacy rules.
   Validator status reports an unknown finality authority when both the settlement read and DNS authority fail,
   never a fallback "active DNS validator" headline.
3. wRPC keeps the exact v1 encoding when the optional native fields are absent and uses response v2 when present.
4. **Release observation bounds.** Real-process drills require a separately supplied
   `rfc12-real-process-observation-bounds` v1 document (`maximumOwnedProcessRssKiB`,
   `maximumOwnedDatabaseLogicalBytes`, `maximumProbeMs`, `samplePeriodMs` ∈ [100, 5000],
   `maximumFixtureOutputBytes`) and a fixed `workflow_timeout_s`. Values: **USER DECISION REQUIRED**; none is
   invented here. A >100,000-block processor latency bound for the release: **USER DECISION REQUIRED**.
5. Before scheduling, the bond exit procedure, compatibility matrix and fingerprint MUST be published (§8 step 5).
6. Remaining: third-party wallets/SDKs/EVM tooling migration, pruned P2P, 100k release latency (validation-only).
7. **External wallet path (2026-10-15, round 3; source only, no PASS).** Items 1–3 are now available to third-party
   code as one versioned surface: `kaspa-wallet-core::misaka_preflight` (capability schema `misaka.wallet.capability.v1`,
   a single `preflight` call before signing, WASM `misakaPreflightV1`). It refuses on genesis mismatch, a missing or
   incomplete import status, an unknown finality authority, and retired kinds once retirement is active or scheduled
   within 1,000 DAA of the sink. The 1,000 DAA margin is a proposal: **USER REVIEW**. Integrator note:
   [rfc12-external-wallet-integration-2026-10-15](../design/palw/execution/rfc12-external-wallet-integration-2026-10-15.md).
   Third-party migration itself remains EXTERNAL.

### ALL-MODEL

RFC6's 2026-10-15 section holds the shared ALL-MODEL rules. For this RFC: no settlement, head or fork-choice rule
reads model availability; immutable model identity binds claims; the 2026-10-12 proposal to connect model-bond
allocation to block opportunities does not change D, W, fork choice or Final weight.

## ユーザー方針 2026-10-15 による改定

* DNS退役後のsettlementの`safe`/`settled`は、**認証済み(`SecurityCertified`)のPALW creditだけ**を意味する。native settlementはfence以後、`Final`遷移ではなく認証遷移を成熟workの証拠とする。未認証claimだけのchainはPALW settlement finalityなしと報告する(RPCは`uncertifiedFinalAnchors`と前提の文言を返す)。
* DNS-BFTの物理除去は別作業(`integ/rfc12-dns-removal`)であり、本改定はそれに依存しない。

**ユーザー方針(原文):**「この攻撃を塞ぐ方針で進める。DNS-BFTはRFCにある通り廃止したので使用できない。」— ここでの攻撃は、閉鎖モデルまたは自前Panelのモデルの未認証claimが`Final`に達し、safe frontier・safe weight・live total・settlement depth・rule E participation・settled anchorを進めて、不正な合意またはsettlement finalityを得ること。補足判断(同日):「抽選した検証者の承認を認証の柱にしている。これは承認にするということは前提にするのでそのままでいい。」

* **`Final`と`SecurityCertified`は別状態である。** `Final`は手続きの完了(利用者払いescrowの経済的settlementは既存規則どおり)。合意credit(safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来の報酬leg)は`SecurityCertified`のclaimだけが生む([RFC-0018](0018-dag-parallel-multi-model-consensus.md) §7、Gate C)。
* **認証は前提に基づく。** claim固定後の乱数でglobal poolから抽選した検証者のm-of-k肯定承認と、G14公開告発windowの異議なし経過で成立する。「抽選されたverifier stakeのうち正直な割合が閾値以上」を明示的な前提とし、無条件の数学的証明とは扱わない。暗号学的な計算証明は使わない(PRINCIPLES §8)。認証済みでもG14で反証されたclaimはvoidされ、合意creditは取り消される。
* **DNS-BFTは廃止済みであり、安全性にも収束にも使わない。**
* **genesisモデルにも認証の免除はない。70Bモデルは、独立した検証(抽選検証者による再実行)が成立するまで合意creditを持たない。**
* 本改定は追記であり、上の本文は履歴として残す。実装は休眠(fence `palw_independently_work_certified_v1`、全presetで`None`、有効化は別途のユーザー判断)。
