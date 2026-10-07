# RFC-0010: Permissionless PALW Panel binding and claim completion

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* Status: Draft, 2026-10-04 — design and implementation plan only. No activation height, wire version, randomness primitive, or consensus fingerprint is assigned.
* Scope: testnet-12 PALW claims, their Panel binding, verification and reward completion. An RFC merge does not change the live rules.
* Related: [RFC-0009](0009-palw-remote-miner.md), [RFC-0007](0007-palw-verification-certificates-and-algebraic-checks.md), [RFC-0008](0008-palw-claim-backed-consensus-blocks.md), [ADR-0141](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md), and the [testnet-12 Panel-seed incident and lane A](../t12-panel-seed-2026-09-25.md).

## 0. Decision proposed

**A claim's Panel MUST be selected by public consensus rules without naming a genesis bond, an operator fleet, or another privileged identity.** The claim, the eligible Panel population, and the randomness commitment must be fixed in that order. A binding block only records the result; its producer, hash, signature, timestamp, execution commitment, lane, and arrival order MUST NOT change the Panel seed or the eligible population. Any valid selected-chain block that reaches the prescribed binding point may complete that transition, including a heartbeat if the new rule admits it.

The shortest safe implementation is therefore **permissionless binding with no separate anchor-producer election**. Producer bonds remain the public qualification for claim production; Panel-seat bonds and their collateral remain the public qualification for verification. The binding block needs no third bond kind or extra privileged role. If a later design elects a particular bonded binder, it must additionally satisfy §4; election is not a prerequisite of this RFC.

For one sealed claim on one canonical history, the target invariant is `Panel(claim, binding_block_A) = Panel(claim, binding_block_B)` for every otherwise valid choice of binding block. The claimant's and beacon contributor's ability to choose among **different sealed claims or different beacon outputs** needs the separate controls in §3.

This is a conditional proposal. The current chain has no demonstrated randomness source that meets §3.3 merely because a hash function is applied to public fields. The new rule MUST stay dormant until a concrete beacon, its implementation and its adversarial bias bound pass the activation gates in §9. In particular, removing the eight-bond predicate while continuing to seed the Panel from a cheaply replaceable attempt is forbidden.

### 0.1 New verification profiles — 2026-10-06

[ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) supplies the
active-kernel/plan boundary; a new model VM is not part of open Panel admission. For new large-model
claims, [RFC07 Part V](0007-palw-verification-certificates-and-algebraic-checks.md) / [RFC11 §15](0011-permissionless-model-and-long-context-onboarding.md)
select small encoded/algebraic checks, positive scope-bound receipts and exact bounded court on
dispute. Capabilities/readiness must identify the approved suite and its real verification/DA/court
resources; the legacy full-replay working set is not automatically the new checker's requirement.
Neither reduced resources nor extra bonds waive complete constraint coverage or Final conditions.

Panel-selection randomness, generation randomness and algebraic proof challenges are distinct
domains. This RFC's fixed Panel seed does not expose future GKR round challenges prematurely or
permit reusable public Freivalds vectors. Apply RFC11's post-commit/round/transcript policy separately.
The paper's “validator” in PoSP means a computation checker here, not a DNS finality validator or a
new privileged orchestrator set. Keep RFC12's PALW-native authority and this RFC's public bond rules.

## 1. Current rule and trust boundary

On testnet-12, lane F1 derives a Panel seed from `H(anchor attempt execution_commitment_v3 || claim_id)`. Lane A then lets only an attempt from one of the eight genesis operator bonds anchor a claim's Panel. A chain block that merges such an attempt may carry the bind, but the operator attempt supplies the seed. The [incident note](../t12-panel-seed-2026-09-25.md) explains the price: an attacker could obtain fresh valid attempt wins with cheap junk work while P0-10 remained open, preview the Panel, and publish a favourable candidate. Lane A prevents an untrusted participant from controlling that source by trusting the operator position instead.

The privilege is consensus-visible in [`palw_operator_anchor_v1.rs`](../../consensus/core/src/palw_operator_anchor_v1.rs) and the post-launch testnet-12 fence in [`params.rs`](../../consensus/core/src/config/params.rs). [`VirtualStateProcessor::palw_chain_block_as_anchor_v1`](../../consensus/src/pipeline/virtual_processor/processor.rs), its anchor walk, SW-8 step 4c, the draw gate, and the one-state pre-check all consume that answer. If no operator attempt reaches a claim before its bind deadline, `BindTimeout` voids it. New public bonds do not become lane-A operators by maturing.

Running a node, registering a bond, producing an ordinary attempt, providing a Panel seat, and anchoring a Panel are distinct permissions. This RFC removes the last identity restriction. It does not claim that an empty network, a class with too few capable seats, or missing evidence can reach `Final`.

## 2. Required properties and attack model

1. **Open admission.** A new participant who satisfies the same on-chain bond, maturity, collateral, class and capability rules as any other participant can produce a claim and serve a Panel. No genesis membership or operator-maintained allowlist participates in a post-fence bind.
2. **One Panel opportunity per accepted claim.** In one canonical history, a claim has one committed identity and one fixed Panel seed. Re-signing, changing header nonce/timestamp, choosing a different binding block, delaying a bind, or replacing a failed binder cannot produce another seed. Reorgs may change canonical history; replaying a claim on a new history must obey explicit replay and finality rules.
3. **No cheap claim search.** A claimant cannot choose many claim identities after learning their Panel. Merely writing `ticket = H(bond_id, claim_id, slot, R)` does not establish this if `claim_id`, `slot`, or `R` can be chosen or discarded cheaply.
4. **No cheap source search.** Whoever supplies the last randomness input cannot cheaply select among outputs by withholding or replacing it. A work-cost argument must price all valid alternatives, including junk attempts, different bonds, withheld contributions, private forks and delayed publication.
5. **Collateral-split neutrality.** If a selection mechanism assigns weight to bonds, dividing one amount among bonds must not increase an owner's aggregate first-choice probability merely by increasing the number of bond identifiers. Fallback and repeated selection need their own analysis; first-choice proportionality alone is insufficient. The protocol cannot infer distinct human ownership from distinct keys.
6. **Finite progress.** An offline actor cannot hold a claim in an unbounded waiting state. A chain that advances with adequate publicly eligible seats, evidence and an available beacon binds and resolves claims within stated windows. A total absence of those resources ends in a named, non-fraud outcome.
7. **Same answer on every node.** Candidate-chain validation, reorg, IBD and pruning-proof sync derive identical claim identity, snapshot, entropy, Panel, exposure locks and timeout. The transition is independent of a node's local RPC, fleet membership or material cache.

The adversary can control many bonds and Panel seats, split collateral, submit many syntactically valid claims, grind unadmitted headers while P0-10 is open, withhold blocks or evidence, displace blocks through DAG competition, and choose when to go offline. The security evaluation must state the maximum adversarial bonded weight and the assumed honest online capacity. No permissionless protocol can guarantee completion when all qualifying producers, seats or data providers are unavailable.

## 3. Proposed state machine

### 3.1 Seal a claim before its Panel entropy is known

An accepted claim enters `PendingSeal` with its existing executor bond, class, job and evidence commitments, payout, exposure, acceptance point and claim ID. A new `ClaimSealV1` identifies the **complete, immutable** claim content and its chain-specific acceptance. It MUST cover every claimant-controlled field that could change a draw; randomized signature bytes and equivalent encodings must not create new draw identities for the same claim. Duplicate work/claim identities follow an explicit once-per-history rule. The seal is committed by the chain before the entropy used by this claim can be learned.

The seal becomes `Sealed` only at a consensus-defined checkpoint or depth that is stable under the network's stated finality/reorg policy. The checkpoint must not depend on this claim's own Panel verdict, the eight genesis operators or a DNS validator set that may be inactive; the network's objective L1 fallback must be specified. A miner-selected `anchor_slot` or claimed checkpoint is not authoritative. A reorg before sealing rolls back the claim; a reorg after sealing must satisfy the existing finality rules. Binding deadlines are measured from the new seed-readiness point, not blindly from the old `accepted_daa + window_bind`, because waiting for a safe seal may outlast the current window. Collateral and fee exposure across this wait must be bounded.

This seal blocks **post-entropy** claim-ID search. It does not make many distinct pre-entropy claims free: each additional claim must face the existing execution, admission, exposure and fee rules, with a measured cap on simultaneous outstanding claims per economic weight. If P0-10 still permits a near-free claim, that is an activation blocker (§9).

### 3.2 Freeze the eligible population and draw policy

For sealed claim `C`, consensus computes an immutable `PanelSnapshotV1` from the claim's designated pre-entropy chain checkpoint. It records a commitment to eligible seat bonds, amounts, maturity, capabilities, model readiness, exclusions (including the executor), stake weights, the draw policy and its rule version. Registration, top-up, split, withdrawal, readiness and capability changes **after** this checkpoint cannot change this claim's initial population or weights. A new bond can qualify for later claims once it matures; a genesis bond has no special status.

The snapshot is **before** the claim's entropy is fixed. If a class lacks enough capable seats, the claim stays in its existing no-panel/held path or reaches a named `NoCapablePanel` outcome; an operator cannot supply an ad hoc replacement list. A class-registration jury and readiness transition must likewise read public bonds and objective proofs. The current `Candidate` → `Prefetching` → `Probation` prerequisites remain capacity conditions, not a named-operator approval path. The implementation audit must check that a fresh cohort can supply the floor population and required model seats without genesis keys.

Draw-time collateral headroom also needs a fixed answer. At a deterministic `AssignmentPointV1` after entropy readiness, consensus processes due claims in canonical `(beacon epoch, sealed acceptance order, claim occurrence index)` order, derives their seat order from the frozen snapshot, and reserves live exposure atomically. The tie-break must come from consensus acceptance order, not a hash that the claimant can grind for queue priority. The binder cannot choose the base state or delay assignment until a desired seat becomes free. Processing limits and backlog order must be consensus parameters; a cap cannot let a block producer choose which due claims to skip. A claim whose frozen candidates lack reservable collateral follows a named, deterministic no-panel path.

### 3.3 Supply entropy that the binder cannot choose

For a sealed claim, `R` must be unpredictable under the stated adversary model until **after** the seal and Panel snapshot are fixed. Its specification must name: the producer set, exact input message, output/proof bytes, verification cost, contribution deadline, missing-contribution rule, chain/fork domain, and an adversary's maximum ability to bias or suppress it. The source may use permissionless mature bonds and certified execution tickets, provided every contributing opportunity has one canonical work identity and consensus can reject a second valid variant. It must not require a named operator or an active DNS validator set. A digest that merely hashes an arbitrary attempt or a producer-chosen signature is not a certified ticket.

The implementation must resolve P0-10's cheap unadmitted-attempt path if it uses PALW attempts as entropy. Header-stage pinning alone is documented as insufficient: execution roots can remain unchecked until later Panel work. A certified-execution source must be verifiable before its randomness is consumed, without asking the Panel being selected to certify its own source. Its bootstrap, computation bound, fraud/court latency and header-validation cost must be specified. A different beacon is acceptable only after equivalent unpredictability, uniqueness, withholding and liveness analysis.

`R` is the same for every binding carrier of a claim. The proposed domain-separated derivation is:

```text
panel_seed_v3 = H(
    "misaka-palw/panel-v3/seed" || network_id || ruleset_id ||
    claim_seal_id || anchor_slot || panel_snapshot_root ||
    beacon_epoch || beacon_output
)
```

These names describe the required binding; the exact encoding and hash function belong to a versioned wire specification. `anchor_slot` is derived by consensus at seal time. The beacon must not be selected *because* this seed yields a desired Panel. A last contributor's ability to suppress an output is part of the security budget, even when the output is unique if published. Hashing, a single VRF, unpenalized commit–reveal, a later raw block hash, or a VDF claim does not by itself establish the required property; the [incident analysis](../t12-panel-seed-2026-09-25.md) already identifies their relevant last-mover or claim-grinding failures.

Until a source meets these requirements, the new fence stays `None` and lane A continues under its existing rule. `BeaconUnavailable` after a bounded deadline ends a claim without a fraud slash; silently substituting a cheap seed is forbidden.

### 3.4 Bind on an ordinary chain transition

After `AssignmentPointV1`, the first selected-chain block at or after the prescribed bind point commits the already-derived `PanelBoundV3` transition **automatically in the state fold**. Its lane and producer do not supply a draw input. A valid heartbeat can bind when the chain's existing heartbeat rule advances the clock; a valid bonded attempt can bind under the same rule. The block cannot pick a subset of due claims or choose their order. If the protocol keeps a separate explicit Panel carrier, any peer may relay it and its payload must be completely determined and validated by consensus.

The state records `claim_seal_id`, `panel_snapshot_root`, `beacon_id`, `panel_seed_v3`, `assignment_point`, `binding_block`, the exact seat list and exposure reservations. `binding_block` is an inclusion witness; it is never a source of Panel randomness. Alternative valid blocks at the bind point compute the same Panel and reservations for the same canonical history. The pre-fence `PalwAnchorFactV2` and its historical block/execution seed remain valid for old claims.

### 3.5 Unavailability and a finite fallback

If an assigned seat does not answer, any permitted redraw chooses the **next deterministic, non-reused seat** from the fixed snapshot using `H(panel_seed_v3 || retry_index || role)`. It never obtains new randomness from the timeout block, a candidate anchor, or a late bond. Retry count and windows are bounded; exhaustion is `NoCapablePanel` or the existing named non-fraud void outcome. A seat's silence may influence which precommitted alternate is reached, so the abuse bound must include adversarial seat weight and economic cost; the protocol must not claim perfect withholding resistance from a deterministic list alone.

No answer, a node's local fetch failure, or an unpublished candidate is not proof of fraud. A cryptographically proven conflicting signature, duplicate spend, invalid certified ticket or other objectively verifiable violation may be slashable under a separately specified evidence window. A participant that simply misses an opportunity loses that opportunity. The existing verifier-unavailability and producer-fraud cases must remain distinct ([ADR-0166](../adr/0166-verifier-unavailability-is-not-producer-fraud.md)).

## 4. Bond weight and canonical tickets

No new `anchor bond` is introduced. The existing bond object may carry producer and Panel capabilities at separately specified collateral thresholds. If the chosen beacon needs contributors, their admission is based on mature, publicly registered bonds and one canonical contribution per `(network, bond, beacon epoch)` or narrower declared opportunity. The input must bind to a pre-entropy committed work identity, not a selectable signature, nonce, claim ID or block hash. A second contribution for the same opportunity is invalid or duplicate; mere nonpublication cannot be proved as equivocation.

Panel seats also have a Sybil limit: distinct bond keys are not evidence of distinct people. A single owner can split enough collateral into multiple seat-eligible bonds and may occupy several Panel positions. The security proof must calculate quorum-capture probability and minimum attack collateral under such splitting, including the class jury, outsider seat, redraw and court. Stake-weighted first-seat selection alone does not establish a multi-seat independence guarantee. State the honest-bonded-weight assumption and check that the required collateral and slashable exposure exceed the value a captured Panel can extract.

An elected-binder variant would add work without improving Panel fairness after §3.3: it needs a bond snapshot, stake-weighted candidate order, offline fallback and a split-invariance proof. If pursued, it MUST freeze the eligible set before the beacon, use one canonical score per bond and opportunity, make all fallback ranks public before the first timeout, and keep **the same Panel seed** across every rank. Where both bond sizes meet that role's threshold, the first weighted winner's aggregate probability for `130,000 BILI × 1` and `13,000 BILI × 10` should match when total effective collateral matches. The full fallback distribution and an actor controlling many bonds require separate adversarial analysis; different keys are not evidence of independent owners. This RFC's recommended binder rule avoids that extra lottery.

## 5. Completing a claim without the existing fleet

The Panel-anchor change alone is insufficient for an end-to-end permissionless claim. The release may claim that property only when all of the following paths are live for a new participant using publicly obtainable funds and artifacts:

| Stage | Required open path | Implementation dependency |
| --- | --- | --- |
| Register and produce | Public bond registration, maturity and collateral rules; no genesis or fleet key. New model onboarding needs enough objectively eligible independent seats. | Existing registry plus §3.2 audit. |
| Submit | Miner signs a claim/attempt under its own bond and can send it through any validating node or relay. | [RFC-0009 §3](0009-palw-remote-miner.md). |
| Bind | Public beacon, frozen Panel snapshot and any valid chain binder; no eight-bond lane A after the fence. | This RFC §§3–4. |
| Verify | Public seats and permitted non-seat auditors can fetch the same committed evidence and participate in Panel/court without the producer's continuously running node. Accusation and court entry use public conditions, not an operator fleet. | [RFC-0009 §4](0009-palw-remote-miner.md) and [RFC-0007 Part IV](0007-palw-verification-certificates-and-algebraic-checks.md), with availability proofs and liability split. |
| Settle and redeem | `Final` and ordinary claim payout follow bond ownership; a winning free-prompt quantum can be carried by another eligible builder and still pay the miner. | [RFC-0009 §5](0009-palw-remote-miner.md). |
| Observe | A miner can verify the relevant chain, bond, claim, Panel and payout facts without trusting one fleet RPC. | [RFC-0009 §6](0009-palw-remote-miner.md). |

DNS-finality validators are a separate role and must not become a hidden prerequisite for PALW Panel selection or claim payout. A network with no willing independent Panel seats, beacon contributors or data providers cannot promise automatic `Final`; it must expose the blocking condition and bounded, non-fraud outcome. Testnet faucet funding from a premine is a current **operational** access barrier, even when the consensus entry rule itself is public. Launch claims about practical permissionlessness require a working funding route and measured participation concentration.

## 6. Concrete implementation changes

| Area | Required change |
| --- | --- |
| `consensus/core/src/config/params.rs` | Add a versioned, initially dormant `palw_permissionless_panel_v1` fence and all timing, snapshot, queue and beacon parameters. Validate dependency ordering; hash the rule and values into the relevant params/schedule identities and normalize dormant values consistently. Revise `palw_anchor_at_ceiling`'s lane-A prerequisite for the new version without changing its historical meaning. Never arm at an already scheduled height that would hide a fork-ID change. |
| `consensus/core/src/palw_operator_anchor_v1.rs` | Keep the eight-genesis predicate solely for pre-fence historical validation. No post-fence code path may call `operator_of_v1` to decide binding or entropy eligibility. Do not rewrite old blocks or old pending claims under the new rule. |
| `consensus/core/src/palw_panel_v2.rs` | Add versioned snapshot, seed and deterministic alternate-seat derivation. Preserve v1/v2 domain separation. Pin exactly which snapshot fields affect stake weights, maturity, capability, exposure and class readiness. |
| `consensus/core/src/palw_state_v2.rs` | Add sealed/entropy-ready/assigned/bound states or an equivalent versioned transition, canonical pending-claim queue, bounded assignment, exposure reservations, `BeaconUnavailable` and no-panel outcomes. Change SW-8 step 4c and timeout handling only for claims admitted under the new fence. The old `PanelBound` representation must remain decodable. |
| `consensus/src/pipeline/virtual_processor/processor.rs` | Version `palw_v2_anchor_fact_of_candidate`, `palw_chain_block_as_anchor_v1`, `palw_v2_anchor_fact_with_seed_v1`, `palw_sw8_anchor_for`, the one-state pre-check, draw gate and fold to read one post-fence `PanelBindingFactV3`. Derive it from the seal, snapshot and beacon, then use the first eligible chain block solely as binding witness. Ensure mergeset displacement, reorg and pruning replay read the same inputs. |
| `consensus/src/pipeline/header_processor/pre_ghostdag_validation.rs` and PALW admission | If PALW work supplies the beacon, enforce a canonical, certified, bounded ticket before it can affect randomness. The existing header-pins fence alone does not certify an execution. Fix or bypass P0-10 under an auditable new rule before allowing such a source. |
| Node, Panel, RPC and CLI | Expose seal, snapshot, beacon, assignment, binder, retry and failure reasons. Remove operator-fleet assumptions from new claim instructions. Integrate RFC-0009's independent evidence and public redemption paths before advertising node-less end-to-end completion. |

The implementation specification must name exact wire objects, signature domains, bytes in every root, validation stage, state-root changes, size limits, bounded per-block work and network-specific activation. File paths above are the current seam map, not a license to change existing types in place without versioning.

## 7. Migration and compatibility

The new rule applies to claims whose **acceptance/seal** is at or after its fence. A claim already accepted under lane A keeps the eight-operator rule until it reaches `Final` or void; an anchor that happens after the fence does not silently change its seed or responsibility. The release must document the maximum old-claim drain time. Only after that drain can the fleet be removed as a liveness requirement for *all* outstanding claims.

Versioned signatures and hash domains prevent replay across networks and old/new claim forms. Upgraded and unupgraded nodes must split at the stated fork boundary with an explicit fingerprint/schedule change. Mainnet remains unlaunched and receives no implied activation from a testnet RFC. RFC-0008's proposed chain-eligible work-slice block needs its own compatibility review before it can act as a binder or beacon contributor.

## 8. Failure cases the design must settle

1. The claimant creates many IDs from one cheap computation, varies a slot/parent, or withholds all but a favourable claim.
2. One bonded actor creates many cheap valid attempts, or one entropy contributor withholds its result after previewing two possible outcomes.
3. The same collateral is split among bonds; first-choice chance, fallback ranks and repeated assignments are compared at equal total collateral.
4. A proposed binder is silent, a heartbeat wins the selected chain, or an honest bonded attempt is displaced into a mergeset. The Panel and exposure reservations stay the same.
5. A Panel seat becomes unavailable after the frozen snapshot; alternates come from one fixed list and the claim leaves pending state within a finite bound.
6. Claims compete for the same collateral; every node applies the same queue order and cannot double-reserve a seat.
7. A reorg crosses the seal, entropy or bind point, including IBD and pruning recovery. Historical lane-A claims keep their original rule.
8. A model is registered by outsiders while the old fleet refuses to carry its artifact or seat it. The protocol either supplies an open path with enough new seats or reports the exact capacity condition that blocks admission.
9. Evidence providers, Panel seats or receipt builders disappear after the miner turns off its PC. Missing data, fraud and unredeemed reward follow different objective rules.

## 9. Activation gates

1. **Security argument:** a concrete, implemented entropy source with measured/analysed bias under withholding, stake splitting, private forks, claim-ID search and P0-10. The paper rule `H(fixed inputs)` is not sufficient. If no source meets this gate, retain lane A and do not claim permissionless Panel anchoring.
2. **Consensus replay:** two independent nodes reproduce the same sealed claims, snapshots, beacon, Panels, locks and outcomes across the §8 cases and a real chain replay. This includes existing legacy claims spanning the fence.
3. **Availability and economics:** bounded seal-to-bind delay, backlog, seat capacity, beacon failure and court windows; no unbounded lock or silent payout loss. Publish the distribution of operator-controlled bonds and effective Panel weight, not just the number of keys.
4. **End-to-end drill:** a new, non-genesis bonded participant registers, produces and submits a claim; non-genesis seats verify it; a non-operator block binds it; independent providers serve evidence; the claim reaches `Final` and its free-prompt receipt is redeemed by another builder, with payout to the miner. Repeat after removing the original eight operator nodes from the drill.
5. **Release:** audited versioned wire/state changes, explicit fence and fingerprint, migration runbook, rollback policy before activation, and public status metrics. Enabling this RFC and RFC-0009 may be staged, but the public claim of end-to-end permissionlessness waits for all required stages.

## 10. Decision boundary

This RFC proposes replacing **operator identity as the Panel seed's trust anchor** with a fixed claim, frozen public Panel population, independently established entropy and binder-independent state transition. It deliberately leaves the new entropy primitive and numeric economics unset until §9. The eight-genesis rule is removed only at a fence where the replacement's safety and liveness have been demonstrated. This separates a permissionless consensus qualification from the measured availability of the people and machines that actually run it.

## Mission alignment amendment — 2026-10-07

public bindingとgenesis-anchor特権の廃止は維持するが、binding成功だけでpermissionless prosecutionの完成としない。非Panelのpublic bondによる証拠取得、accusation、terminal proof、licensed claimのconvictionも完成条件に加える。現在のoperator-anchorは移行対象であり安全性の永続前提ではない。Panel=0はRFC14完成前に有効化せず、現在のbinding方式を恒久必須ともしない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。
