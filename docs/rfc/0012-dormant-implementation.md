# RFC-0012 dormant implementation

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: implemented behind an **unassigned** `Params::palw_dns_retirement` fence.
All shipped network presets use `None`. No network, activation height, D/W threshold,
operator/class concentration cap, or legacy evidence horizon is assigned by this implementation.
There is no CLI override for this policy. Test fixtures do not constitute deployment settings.
Spec closures of 2026-10-15 are in the [RFC](0012-palw-only-consensus-and-native-evm-settlement.md#spec-closure-2026-10-15)
and summarized [below](#spec-closures--2026-10-15).
The [RFC's release gates](0012-palw-only-consensus-and-native-evm-settlement.md#8-implementation-sequence-and-release-gates)
remain mandatory before activation.

## Coordinated transition

`PalwDnsRetirementV1` commits its activation, policy version and all policy values,
legacy evidence horizon, subsidy/fee schedule, retired transaction kinds, and native
snapshot version into both the consensus-parameter and schedule identities. `None`
contributes no new bytes; existing preset fingerprint vectors remain the reference.

Fork choice resolves retirement at the incumbent sink. After retirement, the DNS
BFT veto and stale-stake preferred-tip selection abstain; the existing full PALW
state comparator, frontier/maturity rules, provenance checks, strict wins, shallow
tie rules and missing-data refusal remain in force. DNS confirmation updates stop.
Peer-discovery DNS and header ordering as a download hint are unaffected.

Transaction admission resolves the fence at the accepting DAA score. New DNS bonds
(`0x10`), attestation shards (`0x11`) and precommits (`0x19`) are rejected in header
context, populated UTXO context and template selection. Historical equivocation is
allowed only through the configured finite accepting-DAA horizon, with both signed
targets strictly before retirement; normal evidence signature checks still apply.
Bond unbond/exit validation, historical snapshots, codecs, PALW producer/seat bonds,
panel/court/slashing, payout types and shared signing-key loaders remain available.

## Native EVM evidence and heads

The existing native lock/claim/refund/withdrawal executor remains authoritative:
claim before timeout, refund at/after timeout, same-block claim/refund exclusion,
scaling, tips, single credit/debit and parent-payload/child-result ordering remain
unchanged. Retirement releases the optional DNS Pause admission gate. Acceptance
and settlement confidence remain separate facts.

The new native evidence row uses the canonical L1 sink hash as its generation.
It records policy/ruleset ids, PALW frontier, optional latest/safe/finalized heads,
eligible depth, decimal unique work and a stop reason. It is written in the same
virtual-state RocksDB batch as the legacy three-hash EVM-head row and UTXO/index
changes; the old singleton's Borsh layout remains unchanged.

* `latest` is the newest persisted canonical EVM execution result. Readers verify its committed execution root and selected-chain
  membership. It is a fact about execution and is reported even when a certificate cannot be built (and in `FinalizedConflict`).
* `safe` is the deepest **contiguous executed prefix** covered by the PALW frontier,
  closed relevant claim/court/DA lifecycle and the configured D/W/concentration
  requirements. Work before the effect's accepting DAA/blue position does not
  count as subsequent evidence, including equal-DAA but earlier-blue work.
* `finalized` additionally requires the validated pruning point to be an executed
  ancestor of that certified safe prefix. Pruning alone supplies no certificate.

Evidence consists of REAL (non-floor) attempt claims read at their `Final` transition and matured free-prompt slices read at the spend
that consumed them, **both taken from the PALW delta of the chain block that carried them** — not from the claims the sink still holds, which
are retired at `terminal + claim_retirement` before their trace retention lapses (see the [policy proposal](../design/palw/rfc-0012-policy-proposal.md)
§0). A `Final → Voided` conviction anywhere on the chain retracts what the `Final` counted. A fact is mature at
`max(trace_retention, Final + claim_retirement)` (and, for a slice, no earlier than the spend plus the committed execution-quantum maturity
window); an unspent FP grant, heartbeat, floor or execution carrier alone supplies no W. A slice is identified by
`(canonical work id, quantum index)`; carrier copies cannot supply duplicate credit. FP pricing must be in the compute regime. D counts
distinct qualifying grant/attempt anchors, so multiple slices from one grant do not manufacture depth. W concentration is measured by
registered operator and class. The per-block extraction is cached in memory by block hash (rebuilt from the retained deltas after a restart)
and certified by an O(N + F) sweep. These definitions still need economic and adversarial justification before production parameters are
assigned; the proposal carries the measurements.

PALW delta provenance follows the child header's commitment to its parent's state;
the sink is checked against its reconstructed candidate state. Missing/pruned
contributions are never guessed. Imported checkpoints clear native evidence and
safe/finalized pointers until reconstruction supplies proof. Readers reject mismatched
generations, policies, rulesets, roots and unordered head prefixes. A branch abandoning
a previously finalized head reports `FinalizedConflict` and logs a resync requirement;
the conflict remains sticky until a validated resync/import clears it. Incompatible or
unreadable persisted evidence is preserved for recovery and makes readers fail closed,
rather than being replaced with an empty certificate. Neither case reinstates DNS authority.

Publication history is stored separately at EVM-head prefix 209 plus
`/native/finalized-publication/v1`, as `Option<NativeSettlementSnapshotV1>`. It keeps the deepest published
finalized head even if the current snapshot withholds that label. Ordinary rollback across the fence
preserves both this row and the native incident snapshot, hidden by the existing reader gates, so a
sticky alarm survives re-entry. It is an alarm anchor, never a certificate or a selection veto. A validated
execution-state import clears it in the same batch and invalidates the same-generation readiness memo;
rejected imports preserve it. Old positive evidence bootstraps history, while missing history beside old
negative evidence requires recovery rather than an assumption of no past publication. The legacy
three-hash singleton and native RPC wire types are unchanged.

EVM import checks that its target is the established pruning point before root validation and pins it
through commit after a second check. Identical existing header/snapshot/checkpoint rows are reused and
missing rows completed; contradictory or unreadable rows refuse before flat-state writes or incident
reset. The pruning-history context import also reuses identical samples and rejects empty/genesis/order/
shape/sample errors before writing history. This strengthens the existing proof/catch-up caller contract;
it supplies no new proof authority or branch-selection rule. See RFC §4.3 and the import-recovery record.

Current checkpoint rows are excluded from the evidence cache and evict any prior ordinary row of the
same block. Returning that block above a later boundary requires loading its delta. Readiness memo
reuse also checks the pruning point carried by its evaluation. RPC attaches that advisory explanation
only when its version, policy and shared head/stop statements agree with the returned committed snapshot;
a context change at the same sink must not pair an old certificate with contradictory new advice.

An absent ETH block tag returns `null`. State/call queries for unavailable executed
heads return an error, rather than reading tip state or inventing an empty account.
The evidence API also verifies endpoint roots and provenance. The legacy DAA-only
`settled` field requires a DAA strictly below the native safe head, because DAA alone
cannot identify ordering within an equal-DAA group. Exact head hashes are authoritative. Its guarded session
keeps retirement status, evidence and the legacy PALW status at one generation.

## Money and retained liabilities

*(2026-10-15: the split and fee shares in this section are the RFC §6 **proposal** encoded in the unarmed fence's
policy values for mechanics tests. They are not adopted; production values are USER DECISION REQUIRED.)*

New eligible non-floor attempts escrow 92% and useful-work inclusion earns up to 8%,
with zero new DNS subsidy. Normal and native-settlement transaction fees go to their
carrier (100% worker, zero DNS/service). Allocation uses the lower earning/paying
DAA for claim escrow. Old claims retain their original recorded entitlement; a later
payer does not retroactively increase it. Recovery floor retains its prior bounded
carve. The coinbase withholds the full new worker base; unallocated remainder is
never minted, preventing the retired 20% from becoming an immediate miner windfall.
Inclusion eligibility follows the existing entitlement/dedup checks and excludes
heartbeat, execution Round and floor useful-work credit. Retained quality obligations
continue through the old deferred payout path; no new DNS reward keys are generated.

## Readers and services

In-process and standalone DNS validator workers stop signing/funding after retirement.
Status reports the retired role; historical bond exits remain accessible. CLI,
TypeScript, gRPC and wRPC expose optional native evidence/retirement fields. wRPC
keeps the exact v1 encoding when those fields are absent and uses response v2 when
present. Old JSON without these fields remains readable. The wallet clears an
explicitly unavailable DNS shortcut and retains its long coinbase-maturity fallback.
Explorer overlay/finality pages become native settlement views after retirement;
bridge notices report native evidence availability instead of DNS funding/quorum waits.

## Activation work still required

This is dormant engineering support, **not a completed production migration or a PALW common-prefix security proof**. Status after lane X12
([record](../design/palw/rfc-0012-implementation-record.md), [policy proposal](../design/palw/rfc-0012-policy-proposal.md)):

* **Policy.** D, W, concentration caps, evidence horizon and trusted-sync assumptions are *proposed with measurements*, not assigned.
  2026-10-11 user ruling (drill only): D = 3, W = 6,331,093,440 pwu, operator cap 400 ‰, class cap 1000 ‰ (re-evaluate 600 ‰ at ≥ 3
  REAL classes with mature work), maturity `max(accepted + 5,400, later retirement deadline)`, legacy horizon 10,080 DAA. Production values
  remain USER DECISION REQUIRED (RFC [R12-SETTLE](0012-palw-only-consensus-and-native-evm-settlement.md#spec-closure-2026-10-15)). Open:
  the maturity-rule decision (≥ 7.5 days to the first `safe`), a live census of operators / classes / `pwu` per window, release builds.
* **Private-network matrix.** Run in-process (`rfc12_zero_dns_matrix`: healthy zero-DNS deposits, withdrawals and market orders across the
  fence; opposed DNS state; sibling / deep forks and partitions; fee / subsidy / retained-liability reconciliation; restart; old-version node;
  finalized conflict; lost history; historical exit; hostile block). Open: a real multi-node drill with real `Final` claims through the shipped
  binary, a pruned join with EVM state, and market fills (release gate 4).
* **Evidence loss and cost.** Done: evidence survives claim retirement (read from deltas), a missing delta stops certification, the walk is cached
  and the sweep is O(N + F). Open: a >100,000-block processor run and release-build timings.
* **Clients and operations.** Done for the wallet, CLI, wRPC / gRPC and the first-party explorer; the `FinalizedConflict` alarm and resync are
  implemented, the pager rule and runbook are OPS. Open: third-party wallets / SDKs / EVM tooling (EXTERNAL).

Focused regression tests cover committed policy values, unset presets, duplicate
work, concentration/maturity/order/lifecycle stops, evidence windows, integer subsidy
conservation, opposed DNS state at the retirement fence, transaction/template refusal,
old entitlement preservation, native generation mismatch and atomic row persistence.
Legacy bridge/execution/reorg tests remain part of validation; they do not by themselves
satisfy the private-network activation matrix above.

## Spec closures — 2026-10-15

This records how the [RFC's 2026-10-15 section](0012-palw-only-consensus-and-native-evm-settlement.md#spec-closure-2026-10-15)
maps onto this dormant implementation. It changes no code, preset, fingerprint or height and assigns no PASS.

| Requirement | Specified in the RFC | Implementation observation (not verified here) |
| --- | --- | --- |
| R12-AUTHORITY | Rule E at or below retirement; per-call-site predicate (incumbent sink / accepting DAA / lower earn-pay DAA); trusted-sync statement and removal inventory as release artifacts | EVM-execution and snapshot-boundary predicates not yet specified; no multi-process zero-DNS drill |
| R12-SETTLE | Shared open DA + computation court inventory; delta-read unique work; EXEC consumer horizon; maturity ≥ every applicable court/DA/liability endpoint; named D/W/caps/M/horizon | Values above are drill-provisional only; census and D1 derivation/attack evidence absent |
| R12-BRIDGE | Claim/refund exclusivity at the executing block's accepting DAA; market effects in the same generation | Real market fills and two-ledger root equality after reorg not observed |
| R12-HEADS | Synchronous WAL barriers; publication history with the UTXO generation; one-batch declaration/tip/marker install; no tag publication from an incomplete copy | Power-loss and every-store atomicity not established; recent barrier/batch sources are uncompiled checkpoints per the completion record |
| R12-MONEY | 92/8/0 and 100% carrier fees not adopted; invariants (no extra issuance, escrow ≤ withheld base, old entitlements kept); 2026-10-11 ExecutionCap / market-fee rulings | Split and fee schedule are USER DECISION REQUIRED |
| R12-OPS | Retired kinds 0x10/0x11/0x19; import-status capability; wRPC v1/v2; observation-bounds document and 100k latency bound | Bound values are USER DECISION REQUIRED; third-party tooling migration open |

## R12-OPS closure: external wallet path — 2026-10-15 (round 3)

This section is source only. Nothing was compiled or run, and no PASS is assigned.
Presets, fences and heights are unchanged.

| R12-OPS item | Surface for third parties | Observation (not verified here) |
| --- | --- | --- |
| 1 Retired kinds `0x10/0x11/0x19` | `evaluate_preflight_v1` refuses them once retirement is active (`dnsRetiredAt`) or scheduled within 1,000 DAA of the sink; it uses the consensus predicate `creates_dns_participation` | The margin value is a proposal (USER REVIEW); unbond/exit are not refused |
| 2 Import status / finality authority | `MisakaNodeCapabilitiesV1` mirrors the CLI's `node_import_verdict` and the validator reader's `native`/`dns`/`unknown` authority; `importStatusMissing`, `importIncomplete` and `finalityAuthorityUnknown` refuse before signing | Not run against a node; unit sources prepared in `wallet/core/src/misaka_preflight.rs` |
| 3 wRPC v1/v2 | The capability is built from serde JSON of the existing responses; absent optional fields read as unavailable, never as a pass | WASM bindings (`misakaCapabilitiesFromReadsV1`, `misakaPreflightV1`) not built for wasm32 |
| 6 Third-party migration | Integrator note `docs/design/palw/rfc12-external-wallet-integration-2026-10-15.md`; one `preflight(rpc, expectations, txs)` call | No external wallet, SDK or EVM tool migrated or tested: still EXTERNAL |


## Port of `codex/rfc12-remove-dns-bft` — 2026-10-15

Source only; nothing was compiled or run in this port and no PASS is assigned. Presets, fences, heights, the consensus
params id, the schedule id and the int-12 A-2 vectors are unchanged.

**Approach.** The Codex snapshot (`344a1b882`, base `e35409e2a`, 10-08, unbuilt) deleted the DNS BFT gate, most of
`dns_finality`, the in-process and standalone validator workers, their flags and RPC, and re-pinned every preset
(t12 fingerprint `5ee7fd8e…` → `59a6d1b0…`). Its own note says that build "cannot revalidate the old BFT network history".
That is approach (a), a new network. This RFC specifies approach (b) instead: §5 introduces the `palw_dns_retirement_v1`
fence, keeps "sufficient versioned data/readers to replay and verify old history", says "delete live voter/aggregator
scheduling, not consensus history indiscriminately", and the 2026-10-15 spec closures resolve the fence per call site
(incumbent sink / accepting DAA / lower earn-pay DAA) and require rule E at or below it. testnet-12 runs `dns_bft_gate`
from DAA 6,001 and its history carries DNS anchors and votes, so a node that follows t12 needs the legacy rules below the
fence. The port therefore keeps every pre-retirement path byte-identical and carries the snapshot's intent — no DNS
authority on the new rules — as fence-gated behaviour plus an enforced inventory, not as physical deletion.

**What the port adds** (beyond the dormant implementation above, which already gates fork choice, DNS confirmation updates,
EVM heads, bridge freshness/Pause, coinbase settlement, stake preference, DNS kinds `0x10/0x11/0x19`, precommit duty, the
ConsensusApi DNS reads and both validator workers):

* lane R12's unmerged fixes (`origin/rfc12/r12-wip` `e96e93234`): op 203 serves the abstaining DNS gate facts past the
  retirement; `get_attestation_quality_deficits` reports none past it; the RFC-0009 L2 client reads
  `palw_fork_choice_rule_e_v1` (it was hard-coded `None`); tests `rfc12_authority`, `rfc12_x17`, the L2 rule-E STOP;
* `rfc12_dns_reader_inventory` (R12-AUTHORITY item 4 as a test): every DNS authority read in the consensus crate is listed
  with its reason and must ask `dns_retired_at`, delegate only to such a function, or be reachable only from one; the DNS BFT
  veto has exactly one call site, skipped past the retirement at the incumbent sink. It is a textual check — the behaviour is
  `rfc12_zero_dns_matrix`'s;
* operator tooling: `misaka validator setup` refuses to stake a DNS bond once the node reports the retirement (and, where the
  build schedules one, when the status cannot be read); the `ruleset-candidate` extension refuses to arm or move `dns_bft_gate`;
* shared helper extraction (§5 "extract shared … helpers first"): `kaspa_pq_validator_core::PALW_TX_FEE_FLOOR_SOMPI` (same
  value) is the name PALW, wallet and operator callers use; `ATTESTATION_TX_FEE_FLOOR_SOMPI` stays as the validator's alias.

**Not carried over from the snapshot**, because each breaks t12 replay or its identity: deleting `dns_bft.rs`,
`dns_bft_v1.rs`, the `dns_finality` validation types, `validator_service.rs`, `kaspa-pq-validator`'s `run`/`bond`,
the `--enable-validator`/`--validator-*`/`--stake-bond`/`--evm-bridge-finality=pause` flags, `getPrecommitDuty`, the
attestation mempool policy and the processor's DNS tests; removing `dns_bft_gate` and its fingerprint bytes from `Params`;
the re-pinned README/checklist/join-guide fingerprints. Each of these stays reachable only below the retirement. Physical
removal belongs to a release whose supported networks no longer replay DNS-era history (a fresh-genesis preset, or after
t12 retires and is no longer validated from genesis); it is not scheduled here.

**Path mapping** (snapshot path → this tree):

| Snapshot (pre-reorg) | Reorganized tree | Port |
| --- | --- | --- |
| `consensus/src/pipeline/virtual_processor/dns_bft.rs` | same | kept; abstains past retirement |
| `consensus/src/pipeline/virtual_processor/processor.rs` (`dns_reorg_outcome`, `dns_stake_preferred_tip`, `dns_coinbase_settlement*`) | `…/processor/dns.rs` (10-13 split) | kept; gated |
| `…/processor.rs` (`palw_fork_choice_openings_v1_impl`) | `…/processor/palw.rs` | R12 fix ported |
| `…/processor.rs` (template, `bridge_finality_is_fresh`) | `…/processor/template.rs`, `processor.rs` | kept; gated |
| `…/virtual_processor/palw_reorg.rs` (new in snapshot) | — | not created; rule E lives in `palw_rule_e.rs` |
| `consensus/core/src/dns_bft_v1.rs`, `dns_finality.rs` | same (+ `dns_finality/`) | kept (history codecs/readers) |
| `consensus/core/src/config/params.rs` | `params.rs` + `params/{identity,validation}.rs` | unchanged |
| `kaspad/src/validator_service.rs`, `args.rs`, `daemon.rs`, `palw_drill.rs` | same | kept; worker reports `Retired` |
| `kaspa-pq-validator/`, `kaspa-pq-validator-core/`, `kaspa-pq-signer/` | `misaka/pq/validator/`, `misaka/pq/validator-core/`, `misaka/pq/signer/` | kept; fee-floor name extracted |
| `misaka-cli/src/{operator/wizard.rs, setup/, palw_*.rs, wallet.rs, operator/market.rs}` | `misaka/apps/cli/src/…` | wizard guard; fee-floor rename |
| `misaka-palw-extension/src/kinds/ruleset_candidate.rs` | `misaka/palw/extension/src/kinds/ruleset_candidate.rs` | `dns_bft_gate` refused |
| `misaka-palw-gateway/src/bin/rail.rs` | `misaka/palw/gateway/src/bin/rail.rs` | fee-floor rename |
| `misaka-palw-remote/src/l2.rs` (R12 lane) | `misaka/palw/remote/src/l2.rs` | rule E read ported |
| `scripts/check-repin-enumeration.py`, `scripts/misaka-palw-model-market-devnet-e2e.sh` | `scripts/ci/…`, `scripts/drills/…` | unchanged (no re-pin) |
| `rpc/service/src/service.rs`, `components/consensusmanager/src/session.rs`, `consensus/core/src/api/mod.rs` | same | unchanged; reads already retire |
| `wallet/core/src/utxo/{processor,reference,settings}.rs` | same | unchanged; wallet already clears the shortcut |
| `contrib/…`, `docs/{validator-runbook,testnet12-join-mining,t12-rcore-launch-checklist}.md`, `README.md` | same | unchanged (live t12 values stay) |

**Still referencing DNS, and why** (allow-listed): bond mutations, unbond authorization and proved-slash filtering
(historical exits and the finite evidence horizon); `update_dns_state`/`dns_bft_*` and `DnsState` (replay of t12 below the
fence); the legacy `safe` arm of `update_evm_canonical_heads` (below the fence only); the DNS reward split and payout keys
(pre-fence entitlements, settled once); `dns_params`/`dns_bft_gate` in `Params` and their fingerprint bytes (identity of the
live presets); the validator workers and `misaka validator *` (t12 runs DNS-BFT until the fence); DNS seeders and peer
discovery (unrelated to the finality overlay).
