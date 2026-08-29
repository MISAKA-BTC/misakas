# PALW ConsensusV2 — mainnet audit (2026-08-28)

**Method.** Twelve independent lanes (pull transport, merged work / ADR-0058, class registration,
slashing economics, crypto domains, validator spend, mainnet params, panel duty, court/dispute,
engine determinism, IBD/pruning, fail-open), every finding then put through a two-lens refutation
panel — *you misread the code* and *the state is unreachable* — with refutation as the default.
60 findings survived the panel; this editorial pass merged them to **34**, dropped 4, and
re-verified every critical and high one against the tree. Two findings were **reproduced by
execution**, not by reading (§M2-9's projected node table and §M2-9's leaf counts, via a probe
crate path-depending on `kaspa-consensus-core`). Branch `palw-merged-work` at `8e982b7e`
(== `origin/main`).

**Scope.** The primary target was everything landed after the 2026-08-22 readiness audit, and above
all the twelve commits of 2026-08-28: the protocol 103→104 material PULL (`f9683553`), the class
registration ledger rewrite (`41441c96`, `73b537bb`, `cd1bcb3b`, `51a81089`), the qwen3moe class
family (`aee23f9b`, `fd683397`, `e510e61f`), and the validator coinbase-maturity fix (`8e982b7e`),
plus ADR-0058 merged work (`3b70e11b`) and the gossip-cap raise (`7a59f037`). None of the eleven
blockers closed on 2026-08-22 was found re-opened; two of them (the gossip map's unbounded growth,
and the merged-blue payment predicate) have **new, different holes on the same seam**, reported
below as M2-1 and M2-3.

---

## Verdict

**M1 — mainnet stays hash-only (`palw_consensus_mode: Disabled`), which is today's reality: NO-GO.**
The deciding fact is that mainnet is **not** PALW-free the way the 2026-08-22 audit's `[C]` note
assumed. `MAINNET_PARAMS` sets `dns_params: Some(PRODUCTION_DNS_PARAMS)`
(`consensus/core/src/config/params.rs:2025`) and that preset sets `dns_activation_daa_score: 0`
(`params.rs:1514`) — so the DNS finality overlay, its 20M-KAS bonds, its attestation rewards and its
slashing are live from block 1 — while the same preset sets
`bond_spend_gate_mergeset_activation_daa_score: u64::MAX` (`params.rs:1645`), which leaves the only
bond spend gate scanning a chain block's own body. A bonded validator can therefore withdraw its
collateral through a merge-blue transaction while its `StakeBondRecord` stays `Active`, counting
toward the quorum denominator and nominally slashable against money that is gone. That is a live
economic defect on day one of a value-bearing mainnet, and it does not need PALW.

**M2 — mainnet enables ConsensusV2 later: NO-GO, and further from GO than on 2026-08-22.** The
deciding fact is that the adjudication layer, which is the entire reason bonded collateral means
anything here, **cannot locate a fault**: `kaspad/src/palw_panel.rs:1463` builds one
`PalwClaimRootsV1` from the *accused claim's* committed roots and both the responder and the
challenger bisect the single capture selected by it (`:1481`, `:1506`, `:1531`). Two readers of the
same bytes have no divergence, so `agree` is true at every rung, `apply_verdict` takes `self.lo = mid`
every round (`consensus/core/src/palw_bisect.rs:501`), and a 2^22-wide ladder
(`processor.rs:4416-4420` forces exactly that width) converges on leaf 4,194,303 — an index the
floor's ~7,900-leaf job cannot open. No arithmetic evidence is ever read, in either direction. On
top of that, one registered class's on-chain graph is provably not the graph its engine runs
(M2-9, reproduced by execution), and three separate remote inputs of a few hundred bytes each burn
honest collateral (M2-1, M2-2, M2-7).

---

## Findings

| # | sev | scenario | one line | file:line |
|---|---|---|---|---|
| M1-1 | critical | M1 | genesis-active DNS overlay + `u64::MAX` mergeset spend fence ⇒ bonded 20M KAS is withdrawable while the bond stays Active | `consensus/core/src/config/params.rs:1645` |
| M1-2 | critical | M1 | one PoW block poisons a pruning point forever; the IBD refuses **after** the utxoset is deleted | `consensus/src/pipeline/virtual_processor/processor.rs:10667` |
| M1-3 | high | M1 | `misaka wallet send`/`utxo consolidate` selects the validator's locked StakeBond output | `misaka-cli/src/wallet.rs:356` |
| M1-4 | medium | both | in-process validator burns an epoch permanently on any submit failure | `kaspad/src/validator_service.rs:1482` |
| M1-5 | medium | both | the sidecar's "transaction gone" match can never fire over wRPC | `kaspa-pq-validator/src/main.rs:1714` |
| M1-6 | medium | both | every fence value is in the consensus fingerprint and a mismatch is a hard handshake refusal ⇒ a scheduled fork partitions at deploy time | `consensus/core/src/config/params.rs:891` |
| M2-1 | critical | M2 | the pull is gated on pool EMPTINESS, and 4 tiny unauthenticated blobs spend a claim's relay budget ⇒ honest producers and honest seats slashed | `kaspad/src/palw_panel.rs:1672` |
| M2-2 | critical | M2 | unverified foreign material is persisted, pooled and re-broadcast with no bound ⇒ remote disk, RAM and a ~10^5 bandwidth amplifier | `kaspad/src/palw_panel.rs:966` |
| M2-3 | critical | M2 | ADR-0058 pays a merged block the transition refuses, anti-replay keyed on "a claim exists" | `consensus/src/processes/coinbase.rs:270` |
| M2-4 | critical | M2 | both court parties bisect the accused's own capture ⇒ the ladder can never find a fault, and the sweep then convicts the responder | `kaspad/src/palw_panel.rs:1463` |
| M2-5 | critical | M2 | 2 of 3 genesis classes have no court responder, and the opening rung convicts on silence | `misaka-palw-base0/src/qwen36_backend.rs:273` |
| M2-6 | critical | M2 | the class-registration signature covers 5 of the object's 9 fields and nothing binds it to a carrier | `consensus/core/src/palw_state_v2.rs:1437` |
| M2-7 | critical | M2 | the chain charges seats for silence it cannot observe (`&[]` at timeout; the submitter's inbox at license) | `consensus/core/src/palw_state_v2.rs:4219` |
| M2-8 | critical | M2 | a future-`activation_daa` registration is permanent, free and unbounded — no cap, no expiry, no reclaim | `consensus/core/src/palw_state_v2.rs:4409` |
| M2-9 | critical | M2 | `strip_absent_subgraphs` deletes `attn_o.weight` and the attention residual: the qwen3moe class's on-chain graph is not the graph its engine runs | `consensus/core/src/palw_qwen36_profile.rs:561` |
| M2-10 | high | M2 | the documented producer configuration cannot answer a court; conviction costs one transaction | `consensus/core/src/palw_state_v2.rs:3503` |
| M2-11 | high | M2 | opening a court is subject to no exposure ceiling, no bond status and no collateral floor | `consensus/core/src/palw_state_v2.rs:4603` |
| M2-12 | high | M2 | `initial_target` is registrant-chosen and never checked against the chain's terms | `consensus/src/pipeline/virtual_processor/processor.rs:4519` |
| M2-13 | high | M2 | the panel's fee-funding recovery scan selects the bond's own locked collateral | `kaspad/src/palw_panel.rs:353` |
| M2-14 | high | M2 | `backends()` deep-copies every dense class artifact (~1.65 GiB) per duty **and** per pooled material | `kaspad/src/palw_panel.rs:194` |
| M2-15 | high | M2 | `court_pending` re-accumulates the same responder move every tick under back-pressure | `kaspad/src/palw_panel.rs:1600` |
| M2-16 | medium | M2 | registration exposure is taken but never checked against a ceiling ⇒ the registry's spam price is unenforced | `consensus/core/src/palw_state_v2.rs:2693` |
| M2-17 | medium | M2 | the Dormant re-entry path ADR-0056 advertises is unreachable from the shipped node | `consensus/core/src/palw_state_v2.rs:1958` |
| M2-18 | medium | M2 | a class-registration signature is a bearer token forever: no chain point, no nonce, no incarnation | `consensus/core/src/palw_state_v2.rs:1443` |
| M2-19 | medium | M2 | `Unavailable` is self-asserted and free to win; seats are one-per-bond, unweighted, on a self-declared operator id | `consensus/core/src/palw_panel_v2.rs:445` |
| M2-20 | medium | M2 | the RC preset's depth budget: finality 600 against pruning 1144, and a 4,800-DAA lattice over a ~1,144-DAA header horizon | `consensus/core/src/config/params.rs:4519` |
| M2-21 | medium | M2 | a restarted seat does not read its own persisted foreign material before accusing | `kaspad/src/palw_panel.rs:1663` |
| M2-22 | medium | M2 | the producer's retention is never pruned and is re-broadcast in full to every peer every ~60 s | `kaspad/src/palw_producer.rs:276` |
| M2-23 | low | M2 | `signature_contexts_root` commits to 2 of the 8 ML-DSA contexts consensus verifies | `consensus/core/src/palw_mode_v2.rs:79` |
| M2-24 | low | M2 | the court's worst-case-duration formula counts ROUNDS where the ladder clocks RUNGS | `consensus/core/src/palw_mode_v2.rs:330` |
| M2-25 | low | M2 | a single build failure burns the submit budget for every pooled claim in one tick | `kaspad/src/palw_panel.rs:1899` |
| M2-26 | low | M2 | `Box::leak` in the qwen3moe IR projection, on a per-duty path | `consensus/core/src/palw_qwen36_profile.rs:539` |
| M2-27 | low | M2 | `safe_frontier` can now name a block that is not on the selected chain | `consensus/core/src/palw_state_v2.rs:3380` |
| M2-28 | low | M2 | a `#[cfg(test)]` debug probe detached the ADR-0042 doc block from `palw_rc_shipped_params` | `consensus/core/src/config/params.rs:4382` |

Counts: **critical 11, high 7, medium 10, low 6** — 34 kept, 4 dropped (see §*Dropped*). By
scenario: 2 critical / 1 high / 3 medium reachable on **M1 today**; the remaining 28 need M2.

---

---

## Remediation status (2026-08-29)

**All 34 findings are addressed in the tree.** What follows is what changed, and what each change
costs to deploy; the sections below are left as they were WRITTEN, as a record of what was broken.

| Finding | State | Note |
|---|---|---|
| M1-1 | fixed | mainnet's bond spend gate activates at genesis; mainnet fingerprint re-pinned (unlaunched, so legal) |
| M1-2 | fixed | one witness child chosen by BLUE WORK, disqualified blocks skipped, both sidecar imports moved before the destructive clear |
| M1-3 | fixed | the wallet marks bonded outpoints from the node's own registry and both spenders skip them |
| M1-4 | fixed | `AllowRebroadcast` rebuilds; a non-Active mode signs nothing |
| M1-5 | fixed | match the payload, not the Debug-rendered wrapper; regression test feeds the real wire shape |
| M1-6 | fixed | fingerprint split into an identity id (what the handshake refuses on) and a schedule id (reported) |
| M2-1 | fixed | pull gated on verification, pool evicts oldest, solicited answers exempt from the per-claim budget |
| M2-2 | fixed | retention only for verified duty material, bounded by count and age, unicast serve under a global budget, no lock across the disk read |
| M2-3 | fixed | the payment path derives pwu exactly as the transition does |
| M2-4 | fixed | capture chosen by role (challenger bisects its OWN execution); the rung clock does not run at `Terminal` |
| M2-5 | fixed | opening-rung silence closes challenger-side; the inverted test is restored to the guard |
| M2-6 | fixed | the signed preimage is the whole object, canonical job included |
| M2-7 | fixed | no charge for unprovable silence, at any of the three sites; receipt pool evicts oldest |
| M2-8 | fixed | activation bounded to a window; pending grants count against the share table |
| M2-9 | fixed | stripper seeds only the gate projection; rule-form regression test; the shipped hybrid id pinned |
| M2-10 | fixed | a producer with no lifecycle funding path refuses to start (the other two halves are M2-5's fix) |
| M2-11 | partial | Active status and the collateral floor are required to open a court; **the exposure ceiling still needs the admission params threaded into the transition** |
| M2-12 | fixed | `initial_target` must equal the chain's own |
| M2-13 | fixed | the bond's own outpoint excluded from the fee scan |
| M2-14 | fixed | dense artifacts are `Arc`s |
| M2-15 | fixed | the responder branch has the already-queued guard |
| M2-16 | partial | a registration must be affordable against the registrant's collateral; the ratio-based ceiling awaits the same plumbing as M2-11 |
| M2-17 | fixed | the known-roots filter sees only live registrations, so Dormant re-entry works |
| M2-18 | fixed | the network domain binds the genesis, so a signature names one incarnation |
| M2-19 | partial | one key, one bond is now enforced; **collateral-weighted seats and a cost for `Unavailable` are not implemented** |
| M2-20 | fixed | the RC preset recomputes its depths from the finality it sets, and `validate_palw_v2` refuses a preset whose lattice outlives its pruning horizon |
| M2-21 | fixed | the verdict arm reads this node's own retention before accusing |
| M2-22 | fixed | retention is pruned past the horizon and no longer pushed to every peer every minute |
| M2-23 | fixed | the ruleset commits to all eight verified contexts; the court's own list gains OPEN |
| M2-24 | fixed | the formula counts clocked MOVES; `WINDOW_COURT` raised to 3,000 to hold the ladder |
| M2-25 | fixed | a submit attempt is charged only for a failure this claim's object caused |
| M2-26 | fixed | the projection owns its rewritten input lists |
| M2-27 | fixed | the field's definition corrected (the value is load-bearing for the job anchor) |
| M2-28 | fixed | probe removed, doc block reattached |

**Deployment consequence.** These are consensus rule changes, and the testnet-11 fingerprint moves
`15bab795…` → `404f8715…`. That is the intended behaviour, not a side effect: this project's own
record is that a rule change not declared by a version bump forks a network silently. testnet-11
must be re-minted onto this build; **mainnet has not launched, so nothing there is disrupted**, and
M1-6's identity/schedule split is what makes the NEXT such change deployable without a flag day.

**Still open**, and deliberately: the exposure ceiling at court-open and registration (M2-11, M2-16)
wants the admission params inside the transition; the seat economics of M2-19 (weighting, a price
for `Unavailable`) is a design decision, not a repair. Neither is a prerequisite for M1.

## M1 — mainnet as it ships today

### M1-1 `[critical]` The overlay is genesis-active and its spend gate is never-active
`consensus/core/src/config/params.rs:1645`

**Mechanism.** `MAINNET_PARAMS { dns_params: Some(PRODUCTION_DNS_PARAMS) }` (`params.rs:2025`);
`PRODUCTION_DNS_PARAMS { dns_activation_daa_score: 0, … bond_spend_gate_mergeset_activation_daa_score: u64::MAX }`
(`params.rs:1514`, `:1645`). In `consensus/src/pipeline/virtual_processor/utxo_validation.rs:775-778`:

```rust
let mergeset_bond_gate_active =
    self.dns_params.as_ref().is_some_and(|p| header.daa_score >= p.bond_spend_gate_mergeset_activation_daa_score);
if !mergeset_bond_gate_active {
    self.check_bond_spend_gate(&txs, selected_parent_bond_view, header.daa_score)?;
}
```

`txs` is the chain block's own body. The acceptance-time replacement is built behind the same fence
(`utxo_validation.rs:295-305`, `.filter(|p| pov_daa_score >= p.bond_spend_gate_mergeset_activation_daa_score …)`),
and at `:352-359` `bond_filter` is
`(bond_gate_view.is_some() || !ctx.palw_v2_locked_bonds.is_empty() || !ctx.palw_v2_bond_burns.is_empty()).then_some(...)`
— on a hash-only mainnet all three are `None`/empty, so **no bond check runs against accepted
mergeset transactions at all**. The field's own doc states the gap verbatim
(`consensus/core/src/dns_finality.rs:1061-1066`). Spending a bond's output-0 emits no
`BondMutation`: the enum has only `Insert`/`Slash`/`Unbond` (`dns_finality.rs:5012-5035`), all
derived from DNS-subnetwork transactions, so the record survives the withdrawal untouched.

**Failure path.** A validator signs an ordinary P2PKH spend of its own bond output-0. Nothing in
`mining/src/mempool` inspects bonds, so it relays and every miner includes it. If the carrying block
becomes the next chain block it is disqualified (`NonReleasableBondSpendInBlock`) — but the block
stays in the DAG, is merged blue by a later chain block, and its transactions are then accepted
through `calculate_utxo_state` with `bond_filter == None`. The collateral is now a free UTXO while
`ActiveBondView` still lists the bond as `Active` with its declared amount, still counting toward
`min_active_stake_sompi` and the attestation quorum denominator, and still nominally slashable.

**Fix.** Do not launch with `dns_activation_daa_score: 0` and
`bond_spend_gate_mergeset_activation_daa_score: u64::MAX` in one preset. Either set the mergeset
fence to `0` (the devnet/simnet posture at `params.rs:1436`) or move `dns_activation_daa_score`
above 0 so the overlay does not precede its own spend gate. `dns_params` is not a genesis-block
input (`params.rs:1512`), so this costs no re-mint — but see M1-6 for why it cannot be rolled out
gradually either.

### M1-2 `[critical]` One block poisons a pruning point, and the refusal lands after the utxoset is gone
`consensus/src/pipeline/virtual_processor/processor.rs:10667`

**Mechanism.** `import_pruning_point_overlay_snapshot` verifies the peer's snapshot against the
pruning point's children:

```rust
let children: Vec<BlockHash> = RelationsStoreReader::get_children(&self.relations_service, pruning_point) …;
for child in children {
    let Ok(header) = self.headers_store.get_header(child) else { continue };
    if self.ghostdag_store.get_selected_parent(child).ok() != Some(pruning_point) { continue; }
    if header.overlay_commitment_root != got {
        return Err(PruningImportError::ImportedOverlayCommitmentMismatch(pruning_point, got, header.overlay_commitment_root));
    }
    verified_against = Some(child);
}
```

There is **no block-status filter**: a header the consensus disqualified, or one never UTXO-validated
at all, carries the same authority as an honest one. `overlay_commitment_root` is validated in
exactly one place in the tree — `verify_expected_utxo_state`
(`utxo_validation.rs:682-727`), reached only from `processor.rs:1246` for blocks resolved as
UTXO-valid chain candidates; a grep over `consensus/src/pipeline/header_processor/` finds no
occurrence. It *is* inside the PoW preimage (`consensus/core/src/hashing/header.rs:88`), so the
attacker pays for exactly one real block. The same shape is on the PALW twin at `processor.rs:10572-10590`.

**Failure path.** Mainnet has `dns_params: Some(...)`, so `overlay_active` is true and this import is
mandatory (`protocol/flows/src/ibd/flow.rs:2355-2360`, `if !msg.found { return Err(...) }`). Pruning
samples are deterministic, so an attacker knows which chain block becomes the next pruning point and
mines ONE valid block whose sole parent is that block, carrying a garbage
`overlay_commitment_root`. It is a side block, so `verify_expected_utxo_state` never runs on it; its
header, relations and ghostdag row are stored and served by every node. When that sample becomes the
pruning point, every node running `sync_new_utxo_set` (`ibd/flow.rs:2162-2195`) executes
`async_clear_pruning_utxo_set()` (`:2176`) **before** `sync_pruning_point_overlay_snapshot` (`:2189`),
so the verification failure leaves the node with its pruning utxoset deleted and
`is_utxo_stable = false` latched. Every retry against every peer fails identically, because the
poisoned header is a fact of the DAG, not of the peer. On the `DownloadHeadersProof` path this
happens after `staging.commit_if(...)` / `mark_active_consensus_replaced()`
(`ibd/flow.rs:1523-1594`). Recovery is a datadir wipe. This is the 2026-08-22 audit's "consequence B"
re-opened: the 2026-08-22 fix moved only the not-found *fetch* before the destructive clear, and the
2026-08-27 hardening (`cc017cae`) turned a survivable mismatch into a deterministic abort.

**Fix.** Filter the children loop by block status — only a header the local consensus accepted may
set or contradict the expected root; better, verify against the selected-chain child alone, resolved
via the selected-chain store rather than by iterating a grindable `get_children` hash set.
Independently, move *both* sidecar imports before `async_clear_pruning_utxo_set` — they need only the
pruning point's header and the peer's bytes.

### M1-3 `[high]` The wallet spends the bond; every other spend path excludes it
`misaka-cli/src/wallet.rs:356`

**Mechanism.** `let mut mature: Vec<Funding> = page_all(&nv, &from_addr).await?.into_iter().filter(|u| u.mature).collect();
mature.sort_by(|a, b| b.amount.cmp(&a.amount));` then a greedy take from the front (`:361-367`).
`from_addr` is `key.funding_address(...)` (`:345`) — the validator's own address, since this crate
wraps "the SAME signing path the validator bonds with" (`:1-4`). The StakeBond's output-0 lives at
that address, is non-coinbase, and is typically the largest UTXO there, so it sorts first.
`utxo consolidate` (`:236-248`) drains everything. A grep for `bond` in the file returns exactly one
hit — a header comment. Every other spender excludes it: `kaspad/src/validator_service.rs:811-830`
("EXCLUDE our own `bond_outpoint` from funding candidates … a validator self-wedge"), and
`kaspa-pq-validator/src/main.rs` threads an `exclude: Option<TransactionOutpoint>` through bond,
unbond and equivocate.

**Failure path.** `misaka wallet send --key-file <validator key> … --yes` puts the bond at input 0.
The mempool has no bond rule, so it relays; every miner that includes it produces a block marked
`StatusDisqualifiedFromChain` and loses it, while the transaction stays in the mempool poisoning the
next template. Combined with M1-1, the same transaction *succeeds* the moment it lands in a merged
non-chain block.

**Fix.** Give the wallet the exclusion the validator paths have: filter the node-reported bond
outpoint out of `page_all`'s output before selection, defaulting to refusing to spend any outpoint
the node reports as a known bond.

### M1-4 `[medium]` The in-process validator burns an epoch permanently on any submit failure
`kaspad/src/validator_service.rs:1482`

**Mechanism.** `guarded_build_funded` flushes the signing record **before** submission
(`:1520-1526`, `if let Err(e) = store.record_and_flush(record)`), and the next heartbeat picks its
targets from the SIGNED log: `let last_signed = … s.last_signed_epoch();` then
`async_get_validator_attestation_targets(outpoint, e + 1, ATTESTATION_CATCH_UP_LIMIT)` (`:529-532`).
When epoch E is offered again by any route the guard refuses to act:
`SignedEpochCheckOutcome::AllowRebroadcast => { info!("…rebroadcast-safe, not re-signing"); None }`
(`:1482-1484`) — it returns `None`, so nothing is rebuilt or rebroadcast. The submit failure path
only warns (`:788`). The standalone sidecar does the opposite and is the proof this is an oversight:
`kaspa-pq-validator/src/main.rs:1438` falls through `Allow | AllowRebroadcast => {}` and rebuilds.

**Failure path.** Any `submit_rpc_transaction` error at epoch E — a full mempool, an orphaned parent,
a double-spend after the exclusion set was wiped, a storage-mass refusal — and epoch E is never
attested by this validator. Sharper variant with no failure at all: in any mode other than `Active`,
`try_attest` still calls `guarded_build_funded` (which flushes) and then takes the
`… mode={} so NOT submitting` branch (`:794-798`), so a node started Passive has permanently consumed
every epoch it observed while passive. Live on mainnet because the overlay is genesis-active there.

**Fix.** Split the two cursors: keep `SignedEpochStore` as the equivocation guard only, track
"submitted and observed" separately, rebuild and resubmit on `AllowRebroadcast`, and do not flush a
record in non-`Active` modes.

### M1-5 `[medium]` `MempoolStatus::Gone` is unreachable over the only transport the validator uses
`kaspa-pq-validator/src/main.rs:1714`

**Mechanism.** The arm is
`Err(RpcError::RpcSubsystem(msg)) if msg.starts_with("Transaction") && msg.ends_with("not found")`.
The string that actually arrives is built by: server `.map_err(|e| ServerError::Text(e.to_string()))`
(`rpc/macros/src/wrpc/server.rs:57`) → borsh client `Err(Error::RpcCall(err))`
(`workflow-rpc-0.18.0/src/client/protocol/borsh.rs:59`) → whose Display is
`#[error("RPC response error {0:?}")] RpcCall(ServerError)` — **Debug**, not Display —
(`workflow-rpc-0.18.0/src/client/error.rs:67-68`) → `RpcError::RpcSubsystem(e.to_string())`
(`rpc/macros/src/wrpc/client.rs:74`). The final string is
`RPC response error Text("Transaction <id> not found")`: it starts with `RPC` and ends with `")`.
Both guard clauses fail. The structured arm above it is also dead, because `KaspaRpcClient` is built
by `build_wrpc_client_interface!` and that macro flattens every server error into `RpcSubsystem`; the
validator connects with `KaspaRpcClient::new(WrpcEncoding::Borsh, …)` (`main.rs:1194`).

**Failure path.** `mempool_status` is a two-state function for this client. Nothing is ever removed
from `inflight_spent` (the removal at `:1504-1508` is gated on `Gone`), so it grows one entry per
attested epoch forever and every attest then performs up to `INFLIGHT_SCAN_CAP = 512` serial
`get_mempool_entry` round-trips before it can build. The code's own comment at `:1709-1713` states
this consequence as the reason the arm exists. Secondary: the head-confirmation check
(`:1517-1543`) sees `Unknown` for a mined head, so `stalled_epochs` is never reset and the
`STALL_WARN_EPOCHS` alarm latches on permanently.

**Fix.** Match the payload, not the rendered wrapper — `msg.contains(&txid.to_string()) && msg.contains("not found")`
— or better, stop string-matching and have the wRPC client preserve `RpcError::TransactionNotFound`.
A unit test feeding the real `RpcCall(ServerError::Text(...)).to_string()` through `mempool_status`
would have caught it; the current tests never construct the wRPC-shaped error.

### M1-6 `[medium]` The activation-height mechanism the standing policy depends on partitions the network at deploy time
`consensus/core/src/config/params.rs:891`

**Mechanism.** `consensus_params_id` hashes the raw score of every fence — `crescendo_activation`
(`:877`), `pow_palw_activation` / `pow_palw_ollama_activation` (`:891-892`), `pq_activation_daa_score`
(`:893`), all five `evm_*_activation_daa_score` (`:894-899`), and the whole `DnsParams` struct as a
length-prefixed borsh blob (`:880-888`) — and, inside the V2 arm, the compile-time
`PALW_STATE_V2_VERSION` (`:958`, currently `10` at `palw_state_v2.rs:151`). The handshake then
refuses any peer whose value differs:

```rust
let local_params_id = self.config.params.consensus_params_id();
if peer_version.consensus_params_id != local_params_id.as_bytes().to_vec() {
    return Err(ProtocolError::WrongConsensusParams(...));
}
```
(`protocol/flows/src/flow_context.rs:1386-1393`; the same comparison rejects IBD candidates and
block-relay summaries at `ibd/flow.rs:1159` and `v7/blockrelay/flow.rs:456`).

**Failure path.** There is no state in which an old-fence node and a new-fence node are peers — not
before the activation height, not ever. Publishing a build that moves any fence to a coordinated
future height H (arming M1-1's mergeset fence, say) disconnects the first upgrading operator from
every un-upgraded peer immediately; as more upgrade, two disjoint meshes build two chains. Nothing
about H is involved: the split starts at deploy and lasts the whole rollout, and at 10 BPS with a
12-hour finality window any realistic public rollout exceeds it, so the last operators to upgrade
hold a chain the new mesh cannot reorg to. The PALW half is identical: `apply_palw_transition_v4`'s
step 4b (`palw_state_v2.rs:3286-3336`) runs with no DAA fence and no second code path, so ADR-0058
itself was a re-mint (`testnet-11` genesis moved `bb0a3ad3…` → `15bab795…`) rather than an
activation.

This is not an attack — no remote input reaches it — but it is on the audit's charter, because it is
what makes the standing "no re-genesis, ship as activation" policy unusable on a value-bearing
network, and it is the reason M1-1, M2-6 and M2-11 are hard to ship rather than hard to write.

**Fix.** Separate the two things the fingerprint conflates. Hash a rule-set identity that a
scheduled-but-not-yet-active fence does not move — e.g. hash `is_active_at(current_tip_daa)` per
fence, or a schedule id both builds agree on before H — so old and new stay peers until H and
diverge only there, where fork choice can resolve it. Keep the hard refusal for changes that alter
the validity of blocks already on the chain. For PALW, keep the version constant in the fingerprint
but select step 4b on `ctx.daa_score >= palw_merged_work_activation_daa` and domain-separate the
state root by the rule in force at that height.

---

## M2 — if ConsensusV2 is enabled

### M2-1 `[critical]` The pull is gated on emptiness, and a claim's relay budget is spendable by anyone
`kaspad/src/palw_panel.rs:1672` (also `protocol/flows/src/palw_gossip.rs:204`)

Five lanes found this from four angles; it is one defect with two halves that compose.

**Mechanism (a) — the seat never asks.** Control reaches `:1671` only because the loop at
`:1650-1667` ran `backend.verify_material(...)` over every pooled blob and none returned `Matches`.
The seat then asks the wrong question:

```rust
if materials.get(&duty.claim_id).map(|pool| pool.is_empty()).unwrap_or(true)
    && requested.get(&duty.claim_id).is_none_or(|at| current_daa >= at.saturating_add(25))
{
    requested.insert(duty.claim_id, current_daa);
    self.flow_context.request_palw_material(duty.claim_id).await;
}
```

One non-matching byte-string makes `pool.is_empty()` false and the pull — landed today for exactly
this case — never fires. The seat falls through to `:1681-1686` and signs
`PalwReceiptVerdictV2::Unavailable { chunk_index: 0, requested_daa: first_seen[&duty.claim_id] }`,
whose `requested_daa` is its own first-sighting DAA, not evidence of any request. The inbox handler
pools unconditionally: `let pool = materials.entry(claim).or_default(); if pool.len() < MATERIALS_PER_CLAIM { pool.push(bytes); }`
(`:1241-1243`, `MATERIALS_PER_CLAIM = 4` at `:69`), with no verification and no eviction.

**Mechanism (b) — the honest bytes are refused network-wide.** `admit_digest` charges a per-claim
counter before it knows who sent the bytes or whether the claim exists
(`protocol/flows/src/palw_gossip.rs:203-209`):

```rust
if let Some(claim) = material_claim {
    let count = state.materials_per_claim.entry(claim).or_insert(0);
    if *count >= PALW_MATERIALS_PER_CLAIM { return PalwGossipAdmit::Duplicate; }   // 4
    *count += 1;
}
```

`Duplicate` means not relayed and not queued to the inbox (`v8/palw_gossip_flow.rs:52-55`,
`palw_gossip.rs:239-243`). The module doc's defence — "the honest producer's re-broadcast still fits
because distinct bytes have distinct digests" (`palw_gossip.rs:19-22`) — is wrong about its own code:
the budget counts payloads, not digests. Today's serve path is subject to the same gate, so the
answer to a pull for a claim whose budget is spent is dropped everywhere too. `admit_material`
imposes only an upper size bound (`:233-243`), so a zero-byte payload is `Fresh`.

**Failure path.** The producer broadcasts its 2.3–9.7 MB material *before* submitting the block
(`kaspad/src/palw_producer.rs:561`), so the claim id is public while the multi-megabyte transfer is
still hops away; it is also `attempt_id_v2` over the header's `palw_commitment`, so every node can
compute it from the header. An attacker sends four distinct ~70-byte
`PalwTraceMaterialBroadcast` messages for that claim. They are `Fresh`, so every peer relays them and
they reach the seats in milliseconds. At each seat: `materials_per_claim[C] == 4`, so the honest
material is `Duplicate` and never queued; the pool holds four non-matching blobs, so the pull never
fires; at `bound_daa + window/2` the seat signs `Unavailable`. Three such receipts make
`ProducerDefaulted`, which runs
`void_and_slash(..., PalwVoidReasonV2::ProducerWithholding)` (`palw_state_v2.rs:4676`) against the
honest producer's bond — and `slash_dissenting_seats(&claim, &verdicts, false)` (`:4671`) charges
every honest seat that *did* hold the material and filed `Valid`. Attacker cost: ~280 bytes per claim,
no bond, no block, no transaction. This is the same "slashing machine" the 16 MiB cap raise
(`7a59f037`) landed to close, reachable through the per-claim counter instead of the byte cap.

**Fix.** Three, all needed. (1) Gate the pull on "no pooled payload VERIFIES" — the loop above it
already computes exactly that — not on `pool.is_empty()`. (2) Do not let unverified bytes occupy the
pool against verified ones: verify against the duty's roots before pooling, or keep a separate
one-slot verified entry a matching payload can always fill. (3) In the gossip center, do not charge
the per-claim budget against a payload no consumer accepted, or exempt the pull's served
re-broadcast from it (still digest-deduped) so an answer to an explicit request can always be
ingested.

### M2-2 `[critical]` Unverified foreign material: unbounded disk, unbounded RAM, and a ~10^5 amplifier
`kaspad/src/palw_panel.rs:966`, `:1996`; `protocol/flows/src/v8/palw_gossip_flow.rs:63`

Six lanes; one seam, three consequences.

**Disk.** `persist_foreign_material` (`:966-985`) writes `retention/foreign/{claim}.material` for
EVERY `PalwGossipEvent::Material`, called at `:1240` under the comment "Persisted the moment it
arrives, before any verdict." There is no check that the claim exists on chain, that this node is
seated on it, that the bytes verify, or that a bonded party sent them; the only bound is a 72-hour
mtime sweep (`:975`). `retention_dir` is `app_dir/<network>/palw-retention` (`daemon.rs:1405`) — the
same volume as the consensus RocksDB. The doc-comment's bound ("~2.3 MB a floor material, a few
hundred claims a day … single-digit GiB worst case", `:969-972`) describes honest traffic only.
Aggravating: the prune is a full `read_dir` plus a `metadata()` syscall per entry on every new-claim
write, executed synchronously in the panel's async tick, so the N-th write costs N stat calls; and
`let _ = std::fs::write` swallows a full disk, so the panel keeps believing it is retaining.

**RAM.** The pool's age bound is inert for exactly the claims that dominate it:

```rust
let stale = |claim: &Hash64| first_seen.get(claim).is_some_and(|seen| current_daa > seen.saturating_add(PANEL_POOL_RETENTION_DAA));
materials.retain(|claim, _| live.contains(claim) || (!submitted.contains_key(claim) && !stale(claim)));
```
(`:1996-1998`). `first_seen` has exactly one writer in the file —
`first_seen.entry(duty.claim_id).or_insert(...)` at `:1618`, inside `for duty in &duties`. For a
claim this node holds no duty on, `first_seen.get(claim)` is `None`, `is_some_and` is false, `stale`
is permanently false, and `submitted` never contains it either, so the predicate evaluates
`false || (true && true)` and the entry is kept for the life of the process. The comment directly
above (`:1983-1994`) claims this leak was fixed and is "Bounded by the claim's own age instead" — it
bounded duty claims and left every non-duty claim on the unbounded branch. Even with no attacker, a
seat is drawn on ~5/N of claims, so the overwhelming majority of what it pools is foreign.

**Amplification.** `palw_gossip_flow.rs:63-79` answers a ~70-byte `PalwMaterialRequest` with
`self.ctx.hub().broadcast(msg, None).await` — the full payload to EVERY peer, not the asker.
`hub.rs:172` clones per router into a channel of
`outgoing_network_channel_size() = (1 << 17) + 256 = 131,328` messages
(`connection_handler.rs:183-186`), so back-pressure is effectively absent. The only brake is
`served_recently`: a **per-claim** 10-second window (`palw_gossip.rs:154-173`), with no per-peer and
no global budget. The requester chooses K, because the store is attacker-writable (the disk half
above). Worse, the resolver closure — a blocking `std::fs::read` of up to 16 MiB
(`palw_panel.rs:1164-1170`) — is invoked **while the global `material_resolver` mutex is held**
(`palw_gossip.rs:165-168`), and the field's own doc asserts the opposite: "The closure does disk
I/O; callers hold no lock while invoking it" (`palw_gossip.rs:100`).

**Failure path.** `PalwGossipFlow` is subscribed for every peer unconditionally
(`protocol/flows/src/v8/mod.rs:143-153`) and `PALW_MATERIAL_MAX_BYTES = 16 << 20`
(`palw_gossip.rs:44`). An attacker streams materials with fresh random claim ids and 16 MiB
payloads: each fresh id has an unspent 4-slot budget, so each is `Fresh`, relayed by every
intermediate node, pooled forever and written to disk. It then cycles `PalwMaterialRequest` over the
K ids it planted; each names a different claim so the 10-second throttle never triggers. With K=64
and 8 peers, 4.5 KB of requests produce 64 × 8 × 16 MiB = 8 GiB of enqueued `KaspadMessage` clones at
once. Every outcome — OOM, a full volume killing RocksDB, or a stalled tick — makes the seat silent,
and `slash_silent_seats` (`palw_state_v2.rs:2999-3016`) then charges its collateral for the silence.
Sub-case, first-write-wins: `if path.exists() { return; }` (`:969`) makes the FIRST bytes ever seen
for a claim the permanent on-disk copy, so a small garbage payload planted early is what the node
serves for that claim thereafter.

**Fix.** Persist and pool only material for a claim the chain actually carries and that this node has
a duty or dispute on, and only after `verify_material(...) == Matches`. Stamp an arrival DAA for
every claim entering the pool (at `:1240`, not `:1618`) and default `stale` to TRUE for a claim with
no recorded arrival; cap the pool by entries and bytes. Cap `retention/foreign/` by file COUNT and
total bytes with LRU eviction, prune on a timer rather than inside the write path, and log the write
error. Answer a pull as a **unicast to the requester**, add a per-peer token bucket and a global
bytes-served budget, refuse to serve a claim the chain does not hold, clone the `Arc` out of the
mutex before calling it, and move the read to `spawn_blocking`.

### M2-3 `[critical]` ADR-0058 pays a merged block the transition refuses, and the anti-replay key never engages
`consensus/src/processes/coinbase.rs:270`

**Mechanism.** ADR-0058's new arm pays an in-window red directly to its own miner:

```rust
if palw_pay_entitled_reds_to_their_miner && !mergeset_non_daa.contains(red) {
    let value = match carve { … };
    if value > 0 { outputs.push(TransactionOutput::new(value, reward_data.script_public_key.clone())); }
    continue;
}
```
(`coinbase.rs:269-285`). The only gate is `palw_unentitled_blues` (`:257`), built by
`palw_v2_unentitled_blues` (`processor.rs:4034-4131`), which asks exactly three questions:
`check_palw_producer_entitlement_v2` (bond/key/operator/class/artifact —
`palw_admission_v2.rs:147-197`, which never reads pwu, budget or exposure), an attempt-id dedup keyed
`state.claim(&attempt_id).is_some() || !seen_here.insert(attempt_id)` (`processor.rs:4113`), and the
class lottery (`:4118-4123`). The transition's admission is strictly stronger —
`PwuClaimNotDerived` (`palw_admission_v2.rs:230-236`), `EpochBudgetExceeded` (`:275-297`),
`ExposureCeilingExceeded` (`:316-336`) — and ADR-0058 makes a merged work that fails it **SKIP**
rather than disqualify the accepting block (`palw_state_v2.rs:3296-3335`, `Err(refused) => Some(...)`
→ `merged_skips.push(...)`, block stands). So a merged block can be PAID while creating no claim, no
panel duty and no reserved exposure — and the dedup that stops double payment keys on the claim the
transition did not create, so it never engages.

Compounding it, `hash_override_nonce_time_64` excludes `palw_commitment`
(`hashing/header.rs:196`, `PalwCommitmentDigestRule::Exclude`) while the block-identity `hash()`
includes it (`:165-166`). The relay-path fix (`pre_ghostdag_validation.rs:170-176`) requires a VALID
signature — but `libcrux_ml_dsa::ml_dsa_87::sign(sk, msg, ctx, randomness)` takes caller-supplied
randomness (every call site in `tests.rs` passes it explicitly), so the bond holder can still mint
unbounded distinct valid siblings of its own solve. The file's own comment says the consequence:
"one solved block becomes unbounded distinct blocks" (`pre_ghostdag_validation.rs:145-147`).

**Failure path.** An attacker holds one Active bond on the FLOOR class, which
`check_palw_attempt_admission_v2` exempts from the epoch budget entirely
(`if attempt.class_id != state_params.base_class_id()`, `palw_admission_v2.rs:275`). It runs the
floor until one attempt wins both the Layer-0 target and its class ticket — one honest block's work
— then sets `pwu` to any value that is not `palw_pwu_v1(target, pwu_per_inference)`. The only
stateless pwu rule is `pwu >= 1` (`ZeroPwu`, `palw_attempt_v2.rs:329`), so every header gate passes.
It re-signs `attempt_id_v2` N times with N randomness values, producing N headers identical except
`palw_commitment`: one PoW, N distinct blocks, all with valid signatures. Per merging chain block:
`palw_v2_unentitled_blues` finds the bond Active, the class Active, the ticket under target,
`state.claim(attempt_id)` absent (the transition refused it and skipped) and `seen_here` fresh → NOT
unentitled → `coinbase.rs:283` pays the full §F worker share to the attacker's own script. The
transition then refuses it again, identically, forever. Roughly 30 full worker carves (merge depth
3600/120 s = 30 blocks) for one inference, paid to a key carrying no claim, no panel duty and no
reserved exposure — and `apply_attempt`'s `epoch_counters` bump only counts claims that WERE
admitted, so ADR-0054's share walk and the per-class retarget measure a census the paid blocks are
not in. Before `3b70e11b` the same value went to the honest merging miner via `red_reward`
(`coinbase.rs:289-305`), so this is specifically what ADR-0058 changed. Its own end-to-end test
demonstrates the benign half ("counter fills to its budget of 200 exactly, 3 excess skipped") and
never checks that those 3 skipped blocks were still paid.

**Fix.** Do not let two predicates decide payment and accountability. Either derive the coinbase's
pay set from `claim.accepted_block` after the fold (which needs the coinbase/transition ordering
inverted — deferred by ADR-0058 §5), or make `palw_v2_unentitled_blues` ask the FULL
`check_palw_attempt_admission_v2` against the same live fold state the transition uses, and back the
anti-replay with a durable seen-attempt-id set that survives `retire_claim`
(`palw_state_v2.rs:3105-3136`) instead of claim presence. The second is smaller and closes the
sibling replay independently of the malleability.

### M2-4 `[critical]` The court bisects one capture with two readers, so it can never find a fault — and then convicts the responder
`kaspad/src/palw_panel.rs:1463`; `consensus/core/src/palw_state_v2.rs:3500`

**Mechanism.** The court-duty loop resolves ONE capture and hands it to both roles:

```rust
let roots = PalwClaimRootsV1 { execution_root: duty.execution_root, trace_root: duty.trace_root, anchor };
…
let Some(capture) = materials.get(&duty.claim_id)
    .and_then(|pool| pool.iter().find(|b| backend.verify_material(b, roots) == PalwMaterialVerdictV1::Matches))
```
(`:1463`, `:1481`). `duty.execution_root` / `duty.trace_root` are the ACCUSED CLAIM's committed roots
(`consensus/core/src/palw_producer_v2.rs:588-589`). Both arms then read that same `capture`: the
responder at `:1506` (`backend.bisect_prefix_state(capture, midpoint)`) and the CHALLENGER at
`:1531` (`let Some(ours) = backend.bisect_prefix_state(capture, disclosed.0)`, then
`agree: ours == disclosed.1`). `base0_bisect_prefix_state_v1` (`misaka-palw-base0/src/legs.rs:305`)
is a pure function of `(job_context, leaves, index)`, so identical bytes at an identical index give
an identical hash by construction. `verify_material` does not re-execute — it checks the anchor and
`base0_material_matches_claim_v1` (`backend.rs:81-101`), which a lying capture satisfies by
re-deriving its own binding (exactly what `execute_with_injected_fault`, `backend.rs:103`, produces).
The challenger's own honest re-execution at `:1332` is used only for the root comparison at `:1333`
and then dropped; the pool's only two writers (`:1241`, `:1476`) are both keyed to the claim's roots,
so the honest material never enters. The panel's own comment states the bug as the intent: "both
parties act from the SAME capture, through the same functions, so the bisection converges on a real
divergence rather than on whoever stayed awake" (`:1387-1390`).

Consequence: `agree` is always true, `apply_verdict` takes `self.lo = mid` every round
(`palw_bisect.rs:501`), and over the ruleset-forced space of `PALW_STEP_MAX_LEAVES = 1 << 22`
(`palw_step.rs:62`, forced at `processor.rs:4416-4420`) the interval converges to lo = 4,194,303.
The floor's real job has ~7,900 leaves, so `canonical_step_coordinates` returns `None`
(`palw_step.rs:958`) and `refutation_for_index` errors — no close can be assembled by either party.

**Then the sweep charges the wrong side.** `court_next_deadline_v2` deliberately excludes `Terminal`
from the rung clock: `PalwBisectTurnV1::Terminal | PalwBisectTurnV1::Abandoned => session.deadline_daa`
(`palw_state_v2.rs:3464`), documented at `:3456-3463` as "`Terminal` has no move the responder can
make … The backstop still ends it, on the challenger's side, which is what an unproven accusation
deserves." But that function only decides WHEN a session is visited. WHO is charged is decided by
`sweep_court_deadlines`, which re-derives the rung condition from two raw numbers with no reference
to the turn:

```rust
let rung_fired = session.ladder.last_deadline_daa() < ctx.daa_score
    && session.ladder.last_deadline_daa() < session.deadline_daa;
```
(`:3500-3501`). At `Terminal`, `last_deadline_daa` is the last completed rung's deadline — well below
both — so `rung_fired` is TRUE, `declare_no_show` returns
`PalwBisectTurnV1::Terminal => PalwBisectPartyV1::Responder` (`palw_bisect.rs:529`), and `:3512` runs
`void_and_slash(..., PalwVoidReasonV2::CourtFraud)`. On the RC ruleset the condition is always
satisfied: rungs are 60 DAA apart and the backstop is 2,400 DAA from the opening. The panel code says
the terminal move is "EITHER party" (`palw_panel.rs:1548-1552`), so the two comments state opposite
rules and the sweep's arithmetic silently picks the one that charges the defendant.

**Failure path.** Case A, fraud escapes: producer P commits a claim with one tampered tile and a
re-derived binding; a seat's `verify_material` accepts it, so it licenses normally. Honest challenger
C re-executes, sees different roots, opens a court, and then holds only P's material. Every rung, P
discloses the prefix of its own capture and C computes the same value and posts `agree`. After 22
rounds the interval is `[4194303, 4194304)` and neither party can close. Case B, honest producer
convicted: attacker A opens against an honest claim and posts `agree` at every rung — which is also
what honest challenger software computes — reaches the same terminal index, and at
`opened + 2,400 DAA` the sweep charges P `CourtFraud` and returns A's stake in full
(`write_court(session_id, None)` → `palw_state_v2.rs:2805-2812`). Attacker cost: 23 transaction fees,
zero collateral at risk.

The two-material property IS tested — `misaka-palw-base0/src/backend.rs:349-355` asserts
`honest_before == lying_before` and `honest_after != lying_after` across TWO materials — but the
panel wiring feeds one material to both sides, so the tested property is not the shipped one. Same
shape as the 2026-08-22 item-7 defect.

**Fix.** The challenger must bisect its OWN execution: retain `mine_run.material` at
`palw_panel.rs:1332` keyed to the claim, and select the capture per ROLE — the responder answers from
the claim-matching capture, the challenger from its own re-execution. Gate the sweep's rung branch on
the turn `court_next_deadline_v2` already publishes rather than re-deriving it from two numbers.
Acceptance test: drive both panel halves against a claim with one injected fault and assert the
ladder terminates on the tampered leaf index and the close reads `ExecutorGuilty`; a one-way green is
what hid this.

### M2-5 `[critical]` Two of the three genesis classes have no court responder, and the opening rung convicts on silence
`misaka-palw-base0/src/qwen36_backend.rs:273`

**Mechanism.** Only `Base0Backend` implements the court half of `PalwExecutionBackendV1`.
`impl … for Qwen36Backend` (`qwen36_backend.rs:273`) and `impl … for Qwen25A16Backend`
(`qwen25_a16_backend.rs:197`) define only `model_id`, `job_for_anchor`, `execute` and
`verify_material`, so they take the trait default
`fn bisect_prefix_state(&self, _material: &[u8], _index: u64) -> Option<Hash64> { None }`
(`consensus/core/src/palw_backend.rs:128`) — and their own test asserts it
(`qwen36_backend.rs:446`). The responder arm dead-ends on exactly that
(`kaspad/src/palw_panel.rs:1506`, "the backend cannot state its prefix at the midpoint").

Meanwhile the opening rung was re-clocked onto the tight rung window on the strength of a responder
only the floor has: `let first_deadline_daa = ctx.daa_score.checked_add(builder.params.turn_deadline_daa())?.min(deadline_daa);`
(`palw_state_v2.rs:4568-4572`), whose surrounding comment concedes the risk — "a bundle whose window
is tighter than its own software convicts honest producers" (`:4562`). Nothing in
`validate_court_opened_v2` (`palw_court_v2.rs:167-213`) or in the transition asks whether the claim's
class has an adjudicable backend. These are not fringe classes:
`PALW_RC_GENESIS_QWEN36_SHARE_PERMILLE = 200` and `PALW_RC_GENESIS_QWEN25_A16_SHARE_PERMILLE = 200`
(`params.rs:2451`, `:2456`) give them 40 % of genesis cadence between them.

**Failure path.** Attacker holds any registered bond (min collateral 400,000 sompi,
`palw_fp_devnet_v3.rs:174`). It sends one `CourtOpened` naming a licensed Qwen claim, space
`StepLeaves`, size `court.max_step_leaf_count()`, signed under
`PALW_COURT_V2_MLDSA87_OPEN_CONTEXT`. Every check passes; no class check exists. The transition arms
the ladder at `daa + turn_deadline_daa` = `daa + 60` (`COURT_TURN_DEADLINE = 60`,
`palw_fp_devnet_v3.rs:122`). The victim's panel resolves a Qwen backend, gets a matching capture,
calls `bisect_prefix_state` → `None`, and stays silent — there is no other code path in this tree.
60 DAA later `sweep_court_deadlines` computes `rung_fired = true`, `declare_no_show` names the
Responder, and `:3512` burns the producer's `claim.reserved`. `write_court(None)` returns the
attacker's reservation in full. Repeat against every licensed claim of every model-tier producer.

This re-opens the exact defect the 2026-08-22 audit closed as item 4: the mitigation
(`a_responder_is_not_convicted_for_a_move_no_software_can_make`) was removed globally on the strength
of a responder that exists for one family out of three, and replaced by its inverse
(`a_silent_responder_now_loses_the_opening_rung`, `palw_state_v2.rs:7073`).

**Fix.** Make adjudicability a chain-visible property of the class record and gate `CourtOpened` on
it; or restore the session-budget backstop for the opening rung of any class this tree cannot answer
for; or implement `bisect_prefix_state` / `refutation_for_index` / `operand_openings_for` for both
Qwen families. Restore a guard test in the shape of the deleted one, parameterised over every
registered backend rather than over `Base0Backend` alone.

### M2-6 `[critical]` The class-registration signature covers 5 of the object's 9 fields, and nothing binds it to a carrier
`consensus/core/src/palw_state_v2.rs:1437`

**Mechanism.** `palw_class_registration_message_v2` hashes exactly five things:

```rust
state.update(network_domain.as_byte_slice());
state.update(class_id.as_byte_slice());
state.update(&share_permille.to_le_bytes());
state.update(&activation_daa.to_le_bytes());
state.update(&borsh::to_vec(registrant_bond)…);
```
(`:1443-1448`). The object it authenticates carries NINE fields (`:1489-1520`): `class_id`,
`artifact_root`, `slash_value_per_pwu`, `pwu_rule`, `initial_target`, `share_permille`,
`activation_daa`, and the boxed carriage `{ profile, canonical, registrant_bond, signature }`. The
acceptance layer verifies only the five (`processor.rs:4546-4563`) and then hands the object to
`verify_class_admission_v2`, which binds `profile` (`shape_profile_id() == class_id`,
`palw_class_admission_v2.rs:562-565`) but **copies `artifact_root` through verbatim** (`:658`) and
derives `pwu_per_inference` from the UNSIGNED `canonical` job:
`let counted = step_leaf_count(profile, canonical)` … `if *pwu_per_inference != counted` (`:643-653`),
accepting any counted value up to the profile's worst case. The transition writes both straight into
consensus state (`palw_state_v2.rs:4424`, `:4426`). Finally, `palw_lifecycle_object_may_ride_v2`
accepts `ClassRegistered { admission: Some(_), .. } => Ok(())` with no carrier binding at all
(`palw_lifecycle_objects_v2.rs:149`), and `palw_bond_registration_binds_its_carrier_v2` returns
`Ok(())` for every non-`BondRegistered` object (`:229`) — so ANY funded 0x4b transaction from ANY
party may carry it. The node-side signer's comment claims the opposite: "Signing anything assembled
beside the object would sign a class that is not the one being registered"
(`kaspad/src/palw_panel.rs:671-672`).

**Failure path.** An honest operator broadcasts its registration. An attacker reads the 0x4b carrier
from the mempool (or off chain), lifts the five signed values plus the 3,309-byte signature, and
builds its own 0x4b transaction carrying a `ClassRegistered` with the same class_id / share /
activation_daa / registrant_bond / signature and the same `profile` (it must be, since
`shape_profile_id() == class_id`), but with (a) `artifact_root` = a root over weights the ATTACKER
holds and (b) `canonical` = a much longer job with `DerivedV1 { pwu_per_inference }` set to that
job's counted leaves. Both survive: the signature covers neither, and `verify_class_admission_v2`
re-counts against the attacker's own canonical. It fee-bumps to land first. Outcome: the class is
pinned to the attacker's `artifact_root`, so `palw_admission_v2.rs:192`
(`if class.artifact_root != attempt.artifact_root`) refuses every attempt from the operator that
actually holds the model; the honest registration is then refused `DuplicateClass`
(`palw_state_v2.rs:4351-4355`) permanently; the victim's bond is written as `registrant_bond`
(`:4432`) and charged the registry price by `move_registration_exposure(..., true)` (`:4443`); and
because pwu is per-CLASS, not per-job (`palw_admission_v2.rs:230-236`), inflating the canonical job
multiplies the fork-choice weight and escrow of every block ever mined in that class. Post-genesis
class registration is the only way to add a model class without a re-genesis, so every such
registration is hijackable for one transaction fee.

**Fix.** Sign the object, not five of its fields: hash the borsh encoding of the whole
`ClassRegistered` with `signature` zeroed, or at minimum add `artifact_root`,
`slash_value_per_pwu`, `initial_target`, `pwu_rule` and a digest of the carriage's `canonical` to the
preimage. This changes `PALW_V2_SIGNATURE_CONTEXTS`' sibling material and therefore the ruleset id —
see M1-6.

### M2-7 `[critical]` The chain charges seats for silence it cannot observe
`consensus/core/src/palw_state_v2.rs:4219` and `:4515`

**Mechanism (a) — the timeout.** `sweep_deadlines`' `PanelBound` arm is literally

```rust
builder.slash_silent_seats(&claim_id, &claim, &[])?;
```
(`:4219`), so `slash_silent_seats` (`:2998-3016`) charges ALL five seats
`min(claim.reserved, min_collateral_sompi)` regardless of what they filed. Its doc claims "A seat
with something to say is never here", and the arm's own comment says "filing is always available to
it, and a filed answer is never a no-show" (`:4211-4218`) — neither is true, because the chain never
sees a filed receipt unless some node carries a concluding object.

**Mechanism (b) — the license.** `ReceiptLicensed` and `ProducerDefaulted` both call
`slash_silent_seats(claim_id, &claim, &verdicts)` (`:4515`, `:4672`) where `verdicts` is only what
the carried object happened to contain. That object is not the set of seats that answered; it is the
set of receipts ONE node had in RAM at one 2-second tick:
`let pool = receipts.get(&claim).cloned().unwrap_or_default(); let Some(object) = session.palw_v2_receipt_quorum_assemble(claim, pool) else { continue };`
(`kaspad/src/palw_panel.rs:1895-1898`). There is no wait-for-stragglers and no re-open of a landed
object, and consensus cannot verify what receipts existed off-chain, so no acceptance rule can repair
it.

**Failure paths.** (1) Honest steady state: five seats file `Valid` on time; the collector's pool
holds three when its tick fires; the transition charges the other two. That is 2 of 5 seat-duties
slashed on every licensed claim of a perfectly honest network. (2) Adversarial submitter: any seat
holding a funded `--palw-fee-outpoint` is the submitter (`palw_panel.rs:1714`) and chooses which
receipts it carries — it can permanently exclude two chosen competitors at no cost to itself.
(3) Free remote griefing of the timeout arm: the collector's receipt pool is unauthenticated and
capped — `if let Ok(receipt) = borsh::from_slice::<PalwSeatReceiptV2>(&bytes) { … if pool.len() < RECEIPTS_PER_CLAIM && !pool.contains(&receipt) { pool.push(receipt) } }`
(`:1246-1252`, `RECEIPTS_PER_CLAIM = 16` at `:67`) — so 16 distinct well-formed junk receipts
(an empty `signature: Vec<u8>` is valid borsh) naming a live claim fill the pool before any real
receipt arrives; every honest receipt is dropped at the door, no quorum assembles, and all five
honest seats are charged at `ReceiptTimeout`. (4) `PalwReceiptVerdictV2::Incapable` "counts toward
neither side" (`palw_panel_v2.rs:435-443`) and its doc says "It is never charged" — but 3 of 5
`Incapable` pleas make both quorums unreachable, the window closes with no object, and the `&[]` arm
charges every seat including the ones that pleaded. The promise is false in exactly the situation the
verdict exists for.

**Fix.** A seat may only be charged for silence the chain can prove. Drop the no-show charge from the
`PanelBound` timeout arm entirely (no on-chain evidence distinguishes a silent seat from an
unsubmitted quorum), or let receipts ride the chain independently of a concluding object. Make the
concluding object valid only at `receipts.len() == panel.seats.len()` or after a separate collection
deadline, and have the panel delay submission accordingly. Authenticate the collector's receipt pool
(verify the ML-DSA-87 signature and panel membership before `pool.push`) so junk cannot displace real
receipts. Exempt `Incapable`-eligible claims from the timeout charge.

### M2-8 `[critical]` A weightless registration is permanent, free and unbounded
`consensus/core/src/palw_state_v2.rs:4409`

**Mechanism.** The transition validates the share grant for every registration but only WRITES it
when the class activates now:

```rust
let table = granted_share_table_v2(builder.params, &builder.state.class_shares, *class_id, *share_permille)?;
let weightless = *activation_daa > ctx.daa_score;
if !weightless { for (id, share) in table { … builder.write_share(id, Some(share)); } }
```
(`:4408-4420`). Because a weightless registration writes no permille, `class_shares` is unchanged, so
the NEXT weightless registration is checked against the identical table and passes identically — the
check never accumulates and therefore never refuses. The record is written unconditionally
(`:4421-4438`) with `status: Registered { activation_daa, pending_share_permille }`, plus a
`class_target` and a `receipt_target` (`:4449-4452`). Nothing ever removes it: `activate_due_classes`
only touches records whose `activation_daa <= ctx.daa_score` (`:3731-3736`);
`apply_class_reclamation` walks only `class_shares.keys()` filtered to `Active` (`:3954-3960`), and a
`Registered` class holds no share; even reclamation keeps the row (`:3933-3934`). There is no
max-class parameter in the tree and no path that calls `write_class(id, None)`. The acceptance layer
does not bound `activation_daa` either (`processor.rs:4495-4567` reads it only to rebuild the
signature message), and the ride list is a bare `ClassRegistered { admission: Some(_), .. } => Ok(())`.
Distinct class ids are free: the id is the Borsh digest of the whole profile
(`palw_step.rs:794-797`), and `n_threads` is a `u32` that `validate_shape` only checks non-zero
(`palw_step.rs:568-570`) while `n_batch`/`n_ubatch`/`n_seq`/`repack_on`/`llamafile_on`/`fused_gdn_on`/
`use_ref_off`/`kv_cache_f16`/`reference_ruleset_id`/`state_chunk_map_id` are not checked at all and
are read by nothing downstream.

**Failure path.** One Active bond at the 400,000-sompi minimum. Take the shipped BASE-0 floor profile,
set `n_threads = k` for k = 1..N, and for each build a `ClassRegistered` with
`share_permille = min_grantable_share_permille`, the base class's `slash_value_per_pwu`, any non-zero
`initial_target`, a junk `artifact_root` and `activation_daa = u64::MAX`. Every one passes acceptance
and the transition. Result: N permanent rows in `classes` + N in `class_targets` + N in
`receipt_targets` that will never activate, be reclaimed or be removed — at one ordinary transaction
fee each, and nothing more, because the registry reservation is never checked against anything
(M2-16). The extractor imposes no per-block count (`palw_lifecycle_objects_v2.rs:264-297`), so the
rate is bounded only by block mass. Every full node then re-hashes the whole classes map into the
state root on every block (`palw_state_v2.rs:2108-2109`, `:2123`) and `activate_due_classes` scans it
linearly every block; the tree's own measurement of the same shape (`:328-332`) puts 1M rows at
467 ms and 538 MB per block. The only remedy for a poisoned registry is a re-genesis, which the
standing policy forbids.

**Fix.** Bound `activation_daa` at acceptance to a small window ahead of the point's DAA score, give
`Registered` a hard expiry the transition sweeps (drop the class and release its reservation if it
has not activated in its window), cap the live registry with a `PalwStateParamsV2` field so the cap
is inside the ruleset id, and count PENDING registrations against the share table so N weightless
grants accumulate.

### M2-9 `[critical]` The qwen3moe class's on-chain graph is not the graph its engine runs
`consensus/core/src/palw_qwen36_profile.rs:561`

**Reproduced by execution.** A probe crate path-depending on `kaspa-consensus-core` printed
`qwen36_profile_v1(QWEN3_CODER_30B_A3B)`: **33 attention nodes**, and the tail is

```
  15 MatMulQuant blk.{layer}.attn_values.a16
  16 MulElem     blk.{layer}.attn_align.a16
  17 MulElem     blk.{layer}.attn_residual.a16
```

There is **no `attn_o.weight` node** and **no residual `AddElem`**: the declared graph computes the
attention values and discards them, and the residual stream passes the layer input straight through.
The same call for `QWEN36_35B_A3B` returns 47 nodes with `attn_o.weight` at index 19 and the
`AddElem` at 21 — the shipped hybrid is untouched. Class id of the truncated graph:
`e4fbba1fad100fea45317ffd708747d0e017f90465514dd5a37a2196c68077ae216b03905212c307402c76279e0c483a35c94c4ea891931c70bf20c1d4c62e3a`,
which is the id already registered on the live chain.

**Mechanism.** `project` seeds the stripper with three prefixes when a member has no attention gate:

```rust
if g.attn_output_gate == 0 {
    seeds.push("attn_gate.weight");
    seeds.push("attn_gated.a16");
}
```
(`:557-562`). The second seed is the mistake. `QWEN36_ATTN_IR` node 18 is
`n(K::MulElem, KDESC_Q36_GATE_APPLY, "blk.{layer}.attn_gated.a16", QDim, &[Step(16), Step(17)])`
(`:375`) — the gated multiply itself, which the function's own doc says rule 3 (the FOLD) handles:
"a two-input fold left with one input this way … is an identity: it is dropped too, and references to
IT forward to its surviving input" (`:461-463`). But the fold loop skips anything already dropped
(`if dropped[i] || !matches!(node.op, AddElem | MulElem) { continue; }`, `:495`) and the seed pass has
already set `dropped[18] = true`, so `forward[18]` stays `None`. The closure pass then cascades off
it (`:481`): node 19 is
`n(K::MatMulQuant, KDESC_Q36_MATMUL_GROUPED, "blk.{layer}.attn_o.weight", Hidden, &[Step(18)])`
(`:376`) whose ONLY input is node 18, so the attention output projection is dropped; node 21
(`AddElem [Step(20), Step(19)]`, `:380`) is then left with one input and folded away to node 20, the
`attn_align` rescale of the layer input.

The ENGINE does the opposite. `misaka-palw-base0/src/qwen36.rs:1028` is
`let out = self.project(&n("attn_o.weight"), &gated, d, false)?;` — unconditional, outside every
`if gate_raw.is_some()` — and the converter writes `attn_o.weight` and its a16 params for both
families. Nothing catches the divergence: `validate_shape` passes (references still point backwards,
widths non-zero), `verify_catalog_coverage_v1` passes (a smaller kernel set is still a subset), the
new `qwen3moe_geometry_probe` test (`:680-722`) only asserts admission succeeds for n_ctx 4..=10, and
`the_qwen36_table_separates_its_members` (`classes.rs:573`) compares only ids and shapes. No test
anywhere compares the engine's op sequence to the projected node table.

**Failure path.** A node with a converted `.palwq36` runs
`--palw-register-class huihui-ai/Huihui-Qwen3-Coder-30B-A3B-Instruct-abliterated`;
`kaspad/src/palw_panel.rs:625-637` walks `qwen36_canonical_classes_v1()`, `c.profile()` projects the
33-node table, and the registration carries THAT profile. Three consequences. (1) The class id the
chain stores is the id of the truncated graph, and correcting the projection renames the class — the
registered id becomes unreachable, the "n_ctx 17 is BURNED" situation (`classes.rs:184-189`) but for a
class that is producing. (2) `pwu_per_inference` is `step_leaf_count` over a graph missing a
[2048 × 4096] matmul in each of 48 layers; measured `worst_case_step_leaf_count_v1` is **3,267,272**
for the truncated graph against **2,685,440** for the hybrid, and the declared count is derived from
the same wrong table — so exposure, `derive_court_cost_v1`, ladder depth and every cross-class share
and per-class DAA comparison price this class on a false unit. (3) The adjudication premise is void
for this class: the moment a step space is wired for this family the court enumerates leaves over a
graph with no `attn_o` node and a pass-through residual, so fraud in the attention output projection
of all 48 layers is structurally unrefutable. Today that is latent only because
`Qwen36Backend::bisect_prefix_state` returns `None` — which is M2-5.

**Fix.** Remove `"attn_gated.a16"` from the seed list: the gate's `MulElem` is an identity once its
sigmoid input is gone, and rule 3 forwards it to `Step(16)` correctly, which restores
`attn_o.weight` with `refs=[15]` and the residual add. Then add the test that would have caught it:
assert the projected `attn_nodes` / `gdn_nodes` weight names, in order, are exactly the tensor names
`Qwen36Engine::forward_token_probed` touches for the same shape, for BOTH members. This changes the
qwen3moe class id, so it must land before that class carries value anywhere it cannot be re-minted.

*Adjacent, not separately reported:* `attn_output_gate` is DERIVED by the engine
(`qwen36.rs:117-119`, `!self.is_full_attention_only()`) and STORED by the profile
(`palw_qwen36_profile.rs:85`), and `Qwen36CanonicalClassV1::shape_matches` (`classes.rs:243-283`)
compares 13 dimensions but has no field for the gate. The two shipped geometries happen to agree, and
no remote input, artifact or flag can make them disagree — only a future internally-inconsistent
geometry commit — so this is a trap to close alongside the fix above, not a finding.

### M2-10 `[high]` The documented producer configuration cannot answer a court
`consensus/core/src/palw_state_v2.rs:3503`

**Mechanism.** The opening rung is the RESPONDER's move (`palw_bisect.rs:269`, `:526`) and is clocked
at `turn_deadline_daa` = 60 (`palw_state_v2.rs:4568-4572`). When it lapses,
`sweep_court_deadlines` runs `void_and_slash(..., PalwVoidReasonV2::CourtFraud)` (`:3503-3516`) for
the producer's full `claim.reserved`, and the winning challenger loses nothing —
`write_court(session_id, None)` hands the stake back (`:2805-2812`); only challenger-side exits reach
`slash_seat(challenger_bond, ...)` (`:3426`). The producer's only defence, `CourtDisclosed`, is built
at `kaspad/src/palw_panel.rs:1498-1520` but **every submission path in that service is inside
`if self.config.fee_outpoint.is_some() {`** (`:1714`) — and `docs/testnet11-join-mining.md:180-203`
gives the producer command with no `--palw-fee-outpoint` and says "that IS a choice: a panel seat
without one runs receipts-only … It will answer and file, but it will not carry anything to the
chain." A producer following the documentation builds the disclosure into `court_pending` and never
sends it.

**Failure path.** Attacker registers one bond at `min_collateral_sompi` (400,000 sompi), which under
`MAX_EXPOSURE_RATIO_PERMILLE = 500` buys two concurrent courts. It signs `CourtOpened` against any
claim in `ReceiptLicensed` inside its 1,200-DAA challenge window. 60 DAA later the sweep convicts the
silent producer of `CourtFraud`; the attacker's stake is returned in the same block. The capital is
never at risk, so the only cost is transaction fees.

Related documentation defect: `docs/testnet11-join-mining.md:249` tells operators "mine only while
you can leave the node up for the day". The real exposure after the last block is
`window_bind + window_receipt + window_challenge + window_court` = 600 + 600 + 1,200 + 2,400 =
**2,400 DAA past the last licensing**, i.e. up to 80 hours at the frozen 120,000 ms cadence
(`palw_fp_devnet_v3.rs:46-49`). §6b does name the lattice ("several thousand DAA"), so this is an
understated practical rule rather than a false one — but given the paragraph above, the advice is
moot: the documented configuration is convictable on demand whether the node is up or not.

**Fix.** Three, all needed. (1) A node started with `--palw-produce` on a ConsensusV2 network must
refuse to start without a funding path for lifecycle carriers, the same way `daemon.rs` refuses a
non-EVM build on an EVM-active network. (2) The opening rung must not be clockable to a fraud verdict
against a party that has posted nothing — restore the session budget for it, or require the
challenger to post substance first (see also M2-5). (3) A challenger that wins by responder silence
should forfeit something, or opening courts remains a free option. Separately, print
`bind + receipt + challenge + court` DAA and its wall-clock at producer startup.

### M2-11 `[high]` Opening a court has no exposure ceiling, no bond status and no collateral floor
`consensus/core/src/palw_state_v2.rs:4603`

**Mechanism.** The `CourtOpened` transition adds to the challenger's exposure without comparing it to
anything:

```rust
let already = builder.state.reserved_exposure.get(challenger_bond).copied().unwrap_or(0);
let next = already.checked_add(claim.reserved).ok_or(PalwStateV2Error::Overflow("challenger exposure"))?;
builder.write_exposure(*challenger_bond, Some(next));
```
(`:4603-4605`). The comment three lines above claims the opposite: "which is also what stops one bond
opening unboundedly many courts at once: the ceiling it already lives under does the counting"
(`:4600-4601`). There is no such ceiling here. `max_exposure_ratio_permille` is enforced in exactly
one place — item 8 of ATTEMPT admission (`palw_admission_v2.rs:316-336`), reached only from
`validate_attempt_admission_v2`. `validate_court_opened_v2` (`palw_court_v2.rs:167-213`) checks only
that `state.bond(challenger_bond)` is `Some`: it never reads `bond.collateral` and never reads
`bond.status`, so a `Retiring` bond ("may take no new claims") may still prosecute. And losing is free
once the bond is empty: `slash_bond` computes `let debit = u64::try_from(amount.min(record.collateral as u128))…; if debit == 0 { return Ok(()); }`
(`:2930-2933`), and a zero-collateral bond is never removed from `state.bonds`.

**Failure path.** One minimum bond opens courts against N distinct licensed claims in N transactions;
each adds to `reserved_exposure` with no refusal. The only consequence is that the attacker can no
longer submit its own attempts, which it does not want to. After at most one loss the collateral is
zero and it prosecutes with literally zero downside for as long as it can pay fees. Even with M2-4,
M2-5 and M2-10 fixed, this leaves a free unbounded freeze: an open court disarms the claim's Final
deadline (`:4617`), so N free courts freeze N honest claims for a full `window_court` each.

**Fix.** Apply the attempt path's ceiling at `CourtOpened` and mirror it in
`validate_court_opened_v2`: refuse when
`reserved_exposure + registration_exposure + claim.reserved > collateral × max_exposure_ratio_permille / 1000`,
and require `status == Active` and `collateral >= min_collateral_sompi` to open at all.

### M2-12 `[high]` `initial_target` is registrant-chosen and never checked
`consensus/src/pipeline/virtual_processor/processor.rs:4519`

**Mechanism.** `PalwRegistrationTermsV2` exists so "every field here is one the admission gate checks
against something the chain already holds" (`palw_state_v2.rs:1411-1414`) and carries
`initial_target: base_target.target` (`processor.rs:3905`). The acceptance arm enforces exactly ONE
of those terms: `let floor = state_params.min_grantable_share_permille(); if *share_permille != floor { … }`
(`:4523-4528`). `slash_value_per_pwu` is enforced in the transition against the base class's, with an
explicit rationale — "A registrant naming 1 where the floor names 5 gets five times the finality
weight for the same money at risk" (`palw_state_v2.rs:4380-4391`). `initial_target` gets neither: the
transition only rejects zero (`:4357-4359`) and then writes it verbatim into both the attempt and
receipt target slots (`:4449-4452`); `verify_class_admission_v2` never reads it. Yet that number IS
the class's difficulty (`palw_admission_v2.rs:304-308`) and the other factor of pwu
(`palw_pwu.rs:118-146`), and the retarget cannot claw it back — a post-genesis entrant sits at the
grant floor, its epoch budget is defined to make its expectation exactly one block, so observed ==
expected and `apply_class_retargets` moves nothing.

**Failure path.** Register at `initial_target = u128::MAX`. `palw_ticket_admits_v1` then admits every
ticket, so the class never has to win its own lottery, unlike every incumbent, and produces on the
network Layer-0 target alone. Its pwu is `1 × pwu_per_inference`, the smallest any class on the chain
can reserve, so it gets an order of magnitude more concurrent claims per sompi of collateral than an
honest class under the same item-8 ceiling — which is P0-10's Sybil bound. Because the class can be
the BASE-0 floor's graph with one cosmetic field changed, the attacker can actually execute it, so the
claims license rather than default. The opposite mistake is an equally reachable honest-operator
footgun: `initial_target = 1` makes `pwu` saturate, every attempt fails the exposure ceiling, and the
class can never produce with no way to change the number afterwards.

**Fix.** Check `initial_target == terms.initial_target` in the `ClassRegistered` acceptance arm,
exactly as `share_permille` is checked one line above, and bind it into
`palw_class_registration_message_v2` (M2-6) so a relayer cannot swap it under the signature.

### M2-13 `[high]` The panel's fee-funding recovery scan selects the bond's own locked collateral
`kaspad/src/palw_panel.rs:353`

**Mechanism.** `resolve_fee_funding`'s recovery scan filters on
`if entry.script_public_key != script || entry.is_coinbase { continue; }` (`:353`), where `script`
comes from `fee_script`: `let payload = session.palw_bond_payout_payload_v2(PalwBondKeyV2(bond))?; Some(p2pkh_mldsa87_spk(&payload.as_bytes()))`
(`:408-409`). But the bond's collateral output-0 is required to be at exactly that script:
`if output.script_public_key != crate::dns_finality::p2pkh_mldsa87_spk(&owner) { return Err("a bond's collateral output must pay to the payload the registration names as its payee"); }`
(`consensus/core/src/palw_lifecycle_objects_v2.rs:222-226`). It is non-coinbase, unspent, and not in
the node's own mempool, so it passes `is_free` and the scan returns it. There is no bond exclusion
anywhere in `resolve_fee_funding` (`:296-392`) — unlike `validator_service.rs:811-830`, which
excludes exactly this outpoint for exactly this reason.

**Failure path.** A panel whose remembered outpoints are both dead (a dropped carrier, an evicted
change, a spent genesis float — all documented at `:322-332` as the normal case this scan exists for)
falls into the scan and picks its own 20,000-MSK collateral. On a ConsensusV2 network
`ctx.palw_v2_locked_bonds` is non-empty, so the acceptance-time `bond_filter`
(`utxo_validation.rs:352-359`) is built and every carrier spending it is SKIPPED — silently, at
acceptance. The panel therefore builds carrier after carrier that never confirms, burning
`SUBMIT_ATTEMPTS` per claim (M2-25) while reporting success at submit time. On a network where the
filter is inert, the collateral simply leaves.

**Fix.** Exclude `self.bond` from the scan (and from the two remembered candidates), the same
one-line exclusion `find_funding_candidates` already applies. Name "the only output under this script
is the bond's own collateral" as a fourth distinct reason in the failure log at `:378-388`.

### M2-14 `[high]` `backends()` deep-copies every dense class artifact, per duty and per pooled material
`kaspad/src/palw_panel.rs:194`

**Mechanism.** `fn backends(&self) -> PalwBackendRegistry { PalwBackendRegistry::new(self.config.court, self.class_artifacts.clone(), self.qwen36_artifacts.clone(), …) }`
(`:191-198`). `class_artifacts` is `Vec<Base0ArtifactV1>` held BY VALUE (`:171`), and
`Base0ArtifactV1` derives `Clone` over owned weight buffers — `pub embed: Vec<i8>`,
`pub unembed: Vec<i8>`, `pub layers: Vec<Base0LayerWeightsV1>` with `wq/wk/wv/wo/w_gate/w_up/w_down:
Vec<i8>` (`misaka-palw-base0/src/artifact.rs:216-261`). No `Arc` on that path, so `.clone()` is a full
deep copy. For the registered A16 class (QWEN25_1_5B: 28 layers, hidden 1536, ffn 8960, vocab 151,936
— `consensus/core/src/palw_qwen25_profile.rs:79-90`) that is
233,373,696 + 233,373,696 + 28 × 46,792,704 = **1,776,943,104 bytes ≈ 1.65 GiB per call**.
`PalwBackendRegistry::resolve` then clones it AGAIN on the A16 arm:
`Qwen25A16Backend::new(std::sync::Arc::new(artifact.clone()), …)` (`kaspad/src/palw_backends.rs:74`).
The qwen36 tier is correctly `Arc`-shared; the dense tier is not. Call sites: `:1299` (per disputable
claim), `:1414` (per court duty), `:1626` (per seat duty), and `:1647` **inside** the per-material
loop.

**Failure path.** One seat duty whose pool holds four materials executes five registry constructions
per tick, each a 1.65 GiB allocate-and-memcpy plus a second 1.65 GiB clone inside each successful
`resolve` — ~16.5 GiB of churn for ONE duty every 2 seconds, with peak RSS at least 3× the artifact
while a clone and its `Arc` copy are both live. An attacker amplifies it 5× for free by gossiping four
payloads per claim (M2-1's primitive). Being OOM-killed is itself a slashing event (M2-7), and where
the node survives, the copying starves the loop past `receipt_deadline` for the same charge.

**Fix.** Store the dense artifacts as `Vec<Arc<Base0ArtifactV1>>` and have `PalwBackendRegistry` hold
`Arc`s, matching the qwen36 tier; `Qwen25A16Backend::new` already takes an `Arc`, so only the clone
feeding it has to go. Hoist the `backends()` call out of the per-material loop at `:1647` — the class
is a property of the duty, not of the payload.

### M2-15 `[high]` `court_pending` re-accumulates the same responder move every tick
`kaspad/src/palw_panel.rs:1600`

**Mechanism.** The court-duty loop ends with an unconditional
`court_pending.push((duty.session_id, duty.round, duty.i_am_responder, object));` (`:1600`). The only
skip guard at the top of the loop consults `court_moved` (`:1828-1832`), which is written ONLY on a
successful mempool submission (`:1861`). Under back-pressure the submitter deliberately keeps unsent
moves: `let Some(...) = funding.clone().filter(|_| inflight < MAX_INFLIGHT_CARRIERS) else { unsent.push(...); continue; }`
(`:1833-1838`), then `court_pending = unsent;` (`:1877`). The next tick pushes the SAME
`(session, round, side)` again, because nothing checks what is already pending. The challenger branch
has exactly this guard — `if court_pending.iter().any(|(sid, _, _, _)| *sid == session_id) { continue; }`
(`:1367`) — and it was not carried to the responder branch.

**Failure path.** `MAX_INFLIGHT_CARRIERS = 8` (`:85`) and the cap clears only when the panel's own
carrier chain confirms, which at a 120 s cadence is minutes. The file records 357 live court sessions
on the drill (`:79-83`). Each duplicate entry is a fully assembled `CourtClosed` carrying a refutation
plus `operand_openings_for` (`:1558-1590`), at the frozen 80 KiB close ceiling — 357 × 80 KiB per
tick, 30 ticks a minute, on top of re-running `refutation_for_index` and `operand_openings_for` for
each every 2 s. When funding frees up, the submitter walks the whole vector and spends the 8 in-flight
slots and 8 real fees re-sending moves the chain already has, while distinct moves behind them stay
unsent — the exact silent loss `MAX_INFLIGHT_CARRIERS` was added to prevent. Because a rung is
clocked, the crowded-out moves are the ones whose lapse convicts the responder.

**Fix.** Apply the `:1367` guard to the responder branch: skip a duty whose
`(session_id, round, i_am_responder)` is already in `court_pending`, before the expensive assembly.

### M2-16 `[medium]` The registry's price is taken but never checked
`consensus/core/src/palw_state_v2.rs:2693`

`move_registration_exposure` only accumulates (`:2693-2706`): there is no comparison against the
bond's collateral anywhere on the registration path — the acceptance arm reads the bond only for
`Active` and the signature, and the transition calls
`move_registration_exposure(carriage.registrant_bond, true)?` (`:4442-4444`) with no ceiling. The ONLY
application of `max_exposure_ratio_permille` is item 8 of ATTEMPT admission
(`palw_admission_v2.rs:316-336`). So the reservation is a debit collected only from a bond that tries
to produce a block; a registrant that never mines is charged nothing. The parameter's own doc states
the price as a fact — "a flooder wanting a hundred dead classes needs twenty minimum bonds' worth of
collateral, idle" (`palw_fp_devnet_v3.rs:181-192`) — and that is false as implemented: one minimum
bond registers 100,000 classes, because the ceiling is never consulted and the attacker mines under a
different bond. This is the economic backstop M2-8 was supposed to have.

**Fix.** Apply the ceiling at registration — compute
`reserved_exposure + registration_exposure + registration_exposure_sompi` against
`collateral × max_exposure_ratio_permille / 1000` and refuse the object when it does not fit.

### M2-17 `[medium]` ADR-0056's Dormant re-entry is unreachable from the shipped node
`consensus/core/src/palw_state_v2.rs:1958`

The transition deliberately allows one id to be registered twice —
`Some(PalwClassStatusV2::Dormant { .. }) => {}` (`:4351-4355`), documented as "the way back is the way
in" — and reclamation keeps the record precisely so this is possible (`:3933-3934`, `:4005-4008`). But
the terms the builder filters on are derived from ALL rows regardless of status:
`pub fn class_artifact_roots(&self) -> Vec<Hash64> { self.classes.values().map(|record| record.artifact_root).collect() }`
(`:1958-1960`), wired at `processor.rs:3907`. A reclaimed class is still a row, so its artifact root is
still in `registered_artifact_roots`, so `build_class_registration` skips that artifact
(`palw_panel.rs:604`, `:622`) and reports "no `--palw-class-artifact` matches a class this build
knows". After a multi-epoch outage an operator with the same weights on disk can never bring the class
back: attempts are refused `ClassDormant` (`palw_admission_v2.rs:186-188`), and the only route is a
profile change, which is a different class and another permanent registry row.

**Fix.** Filter `class_artifact_roots()` to live registrations —
`matches!(record.status, Active | Registered { .. })` — the same predicate
`verify_registry_consistency` already uses for the exposure ledger (`:2273`).

### M2-18 `[medium]` A class-registration signature is a bearer token forever
`consensus/core/src/palw_state_v2.rs:1443`

The preimage contains network_domain, class_id, share_permille, activation_daa and the registrant
bond — and nothing else (`:1443-1448`). `network_domain` is a keyed BLAKE2b over the NetworkId
**string alone** (`palw_attempt_v2.rs:141-146`): no genesis hash, no pruning point, no ruleset id.
`activation_daa` is hard-coded 0 by the only builder in the tree (`kaspad/src/palw_panel.rs:681`). So
the signed bytes are height-independent and incarnation-independent, while ADR-0056 Decision 5
reopens the id for re-registration and calls for "a fresh signature". Two consequences. (a) When a
class goes Dormant, any stranger can re-carry the original object — optionally with `artifact_root`
and `canonical` swapped per M2-6, since those are unsigned — and
`move_registration_exposure(carriage.registrant_bond, true)` (`:4443`) charges the registry price to
the original registrant's bond again, for a class it did not choose to bring back. Retiring the bond
does not revoke the signature. (b) Because the domain has no genesis binding, every registration
signature published on one incarnation of a network is valid on the next; this repo has re-minted
testnet-11 repeatedly.

**Fix.** Add the genesis hash (or the ruleset id) to `palw_network_domain_v2`'s preimage, and add a
registration nonce or the class's incarnation index to the message. Both are ruleset-id changes
(M1-6).

### M2-19 `[medium]` `Unavailable` is self-asserted and free to win; the panel is unweighted with an unproven operator id
`consensus/core/src/palw_panel_v2.rs:445` and `consensus/core/src/palw_state_v2.rs:4283`

`validate_receipt_quorum_v2`'s `Unavailable` arm checks only that the accusation is well-formed
against fields the accuser chose — `chunk_index < claim.trace_chunk_count`,
`requested_daa >= bound_daa`, `requested_daa <= receipt.signed_daa`,
`requested_daa <= claim.trace_retention_daa` (`:445-477`) — and nothing binds `requested_daa` to a
request that happened: the panel fills it from its own `first_seen` map
(`palw_panel.rs:1684`, set when the DUTY is noticed at `:1618`, not when anything was asked). A
successful accuser is charged nothing anywhere.

The panel that decides it is not stake-weighted. `BondRegistered` checks outpoint uniqueness,
`collateral >= min_collateral_sompi`, non-empty `operator_pubkey` and non-zero payout payload
(`:4283-4302`) — there is NO uniqueness check on `pubkey`, and `operator_pubkey` is never proved:
`operator_id` is a keyed hash of whatever bytes the registration carried (`:188-193`), and the
acceptance layer verifies only that the registration is signed by `pubkey`
(`processor.rs:4699-4706`). The panel dedups on that field
(`palw_panel_v2.rs:222-224`) and its ticket is one per bond with collateral not an input
(`:208-212`), while `slash_seat` caps the downside at `min_collateral_sompi` (`:2924-2925`). So seat
share is a function of bond COUNT, not stake — and `palw_v2_min_genesis_bonds_v1() = PALW_V2_PANEL_SEATS + 1 = 6`
(`palw_fp_devnet_v3.rs:341-347`) means 5 of 6 bonds sit on every claim of a minimum-registry chain,
where any 3 are a permanent quorum. (The lanes' cost model for splitting was disputed and is not
reproduced here; the mechanism above is what stands.)

**Fix.** Make an `Unavailable` cost the accuser its own reservation until the claim resolves,
forfeited if the material is later served or the claim licenses — an accusation should cost at least
what a court opening stakes. Make the seat ticket collateral-weighted so splitting confers no
advantage and `slash_seat`'s cap can be raised to the seat's own collateral. Enforce the stated
one-key-one-bond rule and require a second signature under `operator_pubkey`.

### M2-20 `[medium]` The RC preset's depth budget: finality 600 over pruning 1,144, and a 4,800-DAA lattice over a ~1,144-DAA header horizon
`consensus/core/src/config/params.rs:4519`

**Mechanism.** `palw_rc_params` builds on `palw_rc_base_params()`, which inherits `TESTNET_PARAMS`'
`BlockrateParams::new_two_minute_bps()` (`params.rs:2132`). That constructor derives, at 120 s/block
with `ghostdag_k = 1`: finality_depth = 43,200/120 = 360, merge_depth = 3,600/120 = 30,
lower_bound = 360 + 60 + 4·180·1 + 2 + 2 = 1,144, duration_term = 108,000/120 = 900, so
pruning_depth = 1,144 (`params.rs:240-265`, `constants.rs:70-81`). `palw_rc_params` then writes
`params.blockrate.finality_depth = bundle.state.window_challenge() / 2;` (`:4519`) = 1,200/2 = **600**
and leaves pruning_depth untouched. The correct lower bound is now 600 + 60 + 720 + 4 = **1,384**, and
`anticone_finalization_depth()` computes 600 + 30 + 720 + 2 + 2 = 1,354 and then silently clamps to
`min(pruning_depth, ...)` = 1,144 — under a comment that says the clamp exists because "for some tests
we use a smaller (unsafe) pruning depth" (`params.rs:1129-1132`). `validate_palw_v2()` does not
compare the two.

**Second consequence, same root.** The pruning processor deletes headers:
`if !keep_headers.contains(&current) { self.headers_store.delete_batch(&mut batch, current).unwrap(); }`
(`pruning_processor/processor.rs:736-738`), where `keep_headers = self.past_pruning_points()`
(`:461`). Every judgement in the lattice is anchored on the claim's own block header —
`job_anchor_for_claim` does `let header = session.palw_claim_block_header_v2(accepted_block)?;` then
`pre_pow_hash_64(&header)` (`palw_panel.rs:955-957`) — while
`min_trace_retention_daa = window_bind + window_receipt + window_challenge + window_court` = 600 + 600
+ 1,200 + 2,400 = **4,800** (`palw_producer_v2.rs:193-197`). The lattice is 4.2× deeper than the
evidence horizon, and past it three things happen silently: the receipt path takes
`else { break 'verdict None }` (`palw_panel.rs:1650-1658`) so the seat files nothing and is charged as
a no-show; the dispute path takes `else { continue; }` (`:1321-1329`) so no court can be opened at all;
and the court responder path substitutes a zero anchor via `.unwrap_or_default()` (`:1455-1463`).

**Fix.** Recompute the whole blockrate from the intended finality depth rather than mutating one
field, and add `anticone_finalization_depth() <= pruning_depth` and
`window_bind + window_receipt + window_challenge + window_court <= pruning_depth` as assertions in
`validate_palw_v2()` so a preset that violates either cannot be constructed. Alternatively make the
anchor's inputs survive pruning by carrying the claim's block pre-PoW hash in `PalwClaimStateV2`.
Do not leave `.unwrap_or_default()` at `palw_panel.rs:1463` in either case — a zero anchor is a wrong
answer that looks valid. `blockrate.pruning_depth` is in `consensus_params_id` (`params.rs:874`), so
this must be settled before launch (M1-6).

### M2-21 `[medium]` A restarted seat does not read its own persisted foreign material before accusing
`kaspad/src/palw_panel.rs:1663`

Today's work persists every heard material to disk (`:1240` → `:966-983`) and registers a resolver so
PEERS can read it (`:1163-1170`). The seat's own verdict loop reads none of it: it iterates only the
in-memory pool — `for bytes in materials.get(&duty.claim_id).map(|v| v.as_slice()).unwrap_or(&[])`
(`:1663`) — which is a `HashMap` created fresh at every `worker()` start (`:1172`). The one place that
falls back to disk is the COURT arm (`:1467-1479`), and `retained_capture` (`:986-988`) reads only
this node's OWN captures, never `foreign/`. So after a restart (an OOM sweep, an upgrade, a reboot —
the file records three OOM cycles in a row on 2026-08-28 at `:1264-1270`) a seat that HOLDS the
verifying bytes on its own disk signs `Unavailable` against the claim. The pull transport's stated
promise — "One surviving copy anywhere in the fleet now serves the whole network" (`:1236-1239`) — is
not kept for the copy the seat itself holds.

**Fix.** In the verdict loop, when the in-memory pool holds nothing that verifies, read
`retention/foreign/{claim}.material` (and this node's own captures) and run `verify_material` on it —
exactly what the court arm already does. One `or_else` removes the class.

### M2-22 `[medium]` The producer's retention is never pruned and is re-broadcast in full every ~60 s
`kaspad/src/palw_producer.rs:276`

`retain_execution` (`:255-269`) writes one `{claim}.material` per produced block and nothing ever
deletes it — a grep for `remove_file` / `remove_dir` in `kaspad/src/` finds deletions only in
`compute.rs` and the foreign-material prune. `rebroadcast_retained` (`:276-300`) runs every 300 ticks
of a 200 ms loop, i.e. every **60 s** (`:325-331`, `ticks.is_multiple_of(300)`), does a full `read_dir`
and for every file younger than 48 h calls `broadcast_palw_material`, which is
`self.hub().broadcast(msg, None)` (`flow_context.rs:521-531`) — the complete payload to EVERY peer on
every cycle. Peers dedup on `admit_material` so they do not relay it, but the sender has already put
the bytes on every wire. A producer holding 30 claims of the last 48 h re-sends 68 MB (floor) to
291 MB (QWEN25-A16) to each of ~8 peers every minute, and its retention directory grows monotonically
on the consensus volume.

**Fix.** Prune retention when a claim reaches a terminal phase or passes `min_trace_retention_daa`,
and replace the unconditional push with the announce/pull that `f9683553` already provides.

### M2-23 `[low]` `signature_contexts_root` covers 2 of the 8 contexts consensus verifies
`consensus/core/src/palw_mode_v2.rs:79`

`pub const PALW_V2_SIGNATURE_CONTEXTS: &[&[u8]] = &[PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_RECEIPT_V2_MLDSA87_CONTEXT];`
(`:78-80`), and the startup gate recomputes only that root (`:560`) under a doc that promises
"Editing a context string here without re-minting the ruleset id is a startup failure rather than a
silent cross-family replay" (`:76-77`). The acceptance layer verifies six MORE contexts that are not
in the list: class registration (`processor.rs:4557`), bond retirement (`:4661`), bond registration
(`:4703`), and the three court contexts (`palw_court_v2.rs:65`, `:70`, `:80`).
`PALW_COURT_V2_ALL_DOMAINS` does not list the OPEN context either, so the family's own collision test
(`palw_court_v2.rs:1590`) does not cover it. Editing any of the six passes the startup gate on both
old and new builds; a lifecycle object that then fails verification on one side is DROPPED while the
block stands (`processor.rs:4280-4285`), so two halves of a network can hold different class
registries with no block ever rejected and nothing in either log saying so.

**Fix.** Put every context the consensus acceptance layer verifies into `PALW_V2_SIGNATURE_CONTEXTS`,
add the OPEN context to `PALW_COURT_V2_ALL_DOMAINS`, and add one tree-wide test asserting the set of
contexts passed to any verify site under `consensus/src` equals the committed list. Adding entries
changes the root (M1-6).

### M2-24 `[low]` The worst-case-court formula counts rounds where the ladder clocks rungs
`consensus/core/src/palw_mode_v2.rs:330`

`worst_case_duration_daa` charges one turn window per ROUND:
`let rounds = u64::from(self.bisection_rounds()).checked_add(u64::from(self.terminal_rounds))?; rounds.checked_mul(self.turn_deadline_daa)`
(`:329-332`), where `bisection_rounds()` is `ceil(log2(max_step_leaf_count))` (`:321-324`). But one
round is TWO on-chain rungs and each independently resets the clock (`palw_bisect.rs:440-455`,
`:490-510`, both ending `self.last_deadline_daa = deadline`). On the shipped RC ruleset
(`WINDOW_COURT = 2_400`, `COURT_TURN_DEADLINE = 60`, `COURT_TERMINAL_ROUNDS = 2`, space 2^22 so
bisection_rounds = 22) the declared worst case is (22 + 2) × 60 = **1,440**, and the startup gate
(`:571-575`) passes it against 2,400. The true worst case is 22 disclosure rungs + 22 verdict rungs +
the terminal window = 45 × 60 = **2,700**, which exceeds the backstop. A responder answering at
`accepted_daa + 59` every rung — legal, and nothing penalises a late-but-in-time move — pushes
`last_deadline_daa` past `session.deadline_daa`, at which point the sweep's `rung_fired` is false and
control falls to `rearm_after_challenger_side_close` (`palw_state_v2.rs:3535` → `:3426`), slashing the
honest challenger.

The panel split on this one (one lens confirmed the mechanism and the path; the other held that the
exploit does not follow). The arithmetic is not in dispute, and `window_court` is inside
`palw_ruleset_id_v2`, so it is a genesis-time number worth correcting regardless.

**Fix.** `worst_case_duration_daa` should be `(2 * bisection_rounds + terminal_rounds) * turn_deadline_daa`,
and `WINDOW_COURT` re-derived from it (> 2,700 for the shipped shape, with margin). Add a test that
walks a full 2^22 ladder with both parties moving at deadline − 1.

### M2-25 `[low]` One build failure burns the submit budget for every pooled claim in a tick
`kaspad/src/palw_panel.rs:1899`

The submitter increments before it builds (`:1894`, `:1898`, `:1899`), and a mempool refusal sets
`funding = None` (`:1928`) so the loop breaks out — but a BUILD failure does not (`:1932` warns and
continues), and the dominant build failure is claim-independent:
`if funding.amount <= needed { return Err(format!("funding UTXO holds {} sompi; this carrier needs {fee} fee + {locked} locked — fund the address again", …)) }`
(`:755-760`). `SUBMIT_ATTEMPTS = 3` (`:96`) and the counter clears only on success (`:1922`) or when
the claim leaves `receipts` (`:2004`), i.e. up to `PANEL_POOL_RETENTION_DAA = 4,000`. Three
consecutive low-balance ticks — six seconds — leaves every pooled claim at the cap, and refunding does
not undo it. The log line says "fund the address again", which reads as recoverable and is not, for
the claims already counted out. (The panel split on the size of the harm; the accounting is as
described.) This compounds M2-13, whose failure mode is exactly a funding source that never confirms.

**Fix.** Count an attempt only against failures attributable to THIS claim's object; on a panel-wide
funding shortfall, `break` without incrementing and log once. Clear `submit_attempts` for a claim
when `resolve_fee_funding` returns a different source.

### M2-26 `[low]` `Box::leak` in the qwen3moe IR projection, on a per-duty path
`consensus/core/src/palw_qwen36_profile.rs:539`

`out.push(Ir { inputs: Box::leak(inputs.into_boxed_slice()), ..*node });` — a permanent allocation per
surviving node, needed because `Ir::inputs` is `&'static [I]`. It would be defensible once per
process, but class ids are re-derived on every lookup: `Qwen36CanonicalClassV1::class_id()`
(`classes.rs:234-236`) calls `self.profile()` → `qwen36_profile_v1`, and
`kaspad/src/palw_backends.rs:86-88` does `qwen36_canonical_classes_v1().into_iter().find(|c| c.class_id() == Some(class_id))`
inside `resolve`, which runs per block attempt and repeatedly inside the panel's 2-second sweep
(`palw_panel.rs:1299`, `:1626`, `:1647`). The hybrid entry (empty seeds, identity branch) is free; the
qwen3moe entry leaks, as does every fall-through for an unknown class id. A lane measured 595 bytes
per `qwen36_profile_v1(QWEN3_CODER_30B_A3B)` against 1 byte for the hybrid.

**Fix.** Build the canonical tables — class ids included — once into a `LazyLock` and have
`PalwBackendRegistry` hold a reference, so `resolve` is a lookup rather than a re-projection. Longer
term give `Ir::inputs` a `Cow<'static, [I]>` so the projection needs no leak.

### M2-27 `[low]` `safe_frontier` can name a block that is not on the selected chain
`consensus/core/src/palw_state_v2.rs:3380`

Step 5 sets `builder.state.safe_frontier = claim.accepted_block;`, and since ADR-0058
`accepted_block` is `origin.carrying_block` (`:4876`), which for a merged work is the merged block —
at `ghostdag_k = 1` typically a RED. The field's definition three lines above still reads "the
deepest block ON THIS CHAIN whose PALW work is `Final`" (`:3338-3340`). Every consumer today reads
only the blue-score half (`palw_fork_authority_v2.rs:70-76`, `palw_fork_choice.rs:73-74`,
`ibd/flow.rs:1901`, `processor.rs:3722`/`:3741`), and `safe_frontier_blue_score` is still the
ACCEPTING chain block's blue score, so nothing is broken today. It is a consensus-committed field
whose documented invariant is now false, waiting for the first consumer that resolves the hash.

**Fix.** Record the accepting chain block in a separate field and keep `safe_frontier` chain-scoped,
or amend the definition and the ADR.

### M2-28 `[low]` A debug probe detached the ADR-0042 doc block from `palw_rc_shipped_params`
`consensus/core/src/config/params.rs:4382`

Commit `8e982b7e` (whose message describes only the validator maturity fix) inserted
`#[cfg(test)] mod genesis_probe { #[test] fn t11_genesis_hash_probe() { … eprintln!("t11 genesis: {}", p.genesis.hash); } }`
(`:4382-4390`) between the seven-line doc block at `:4375-4381` and `pub fn palw_rc_shipped_params()`
at `:4391`. Doc comments bind to the next item, so the ADR-0042 Decision 11 rationale now documents a
test module that asserts nothing, and the function deciding what ruleset a binary ships is
undocumented. No runtime effect.

**Fix.** Move the module below the function, or delete it; restore the doc block.

*Same species, uncommitted:* the audited worktree also carries an unstaged
`#[cfg(test)] mod register_class_flag_probe` with an `eprintln!` appended to `kaspad/src/args.rs`
(13 added lines, not in `8e982b7e`). It is not part of any commit under audit, but it is the second
scratch probe found in a shipped source file in one day. Worth a `git status` gate in CI: a
`#[cfg(test)] mod *_probe` that only prints is a debugging session that outlived its session.

---

## Dropped

Four findings that survived the refutation panel did not survive this pass:

* **Redundant ML-DSA verify + state clone per mergeset member** (`processor.rs:4885`,
  `palw_state_v2.rs:2857`). The mechanism is real — the signature is verified once at
  `pre_ghostdag_validation.rs:170-176` and again per merged work, and `checkpoint()` is
  `(self.state.clone(), self.entries.len())`. But 180 ML-DSA-87 verifications at a 120 s cadence is
  tens of milliseconds, and the claim's "permanently recorded multiplier on IBD" rests on M2-3's
  malleability, which is reported there. A cost, not a defect.
* **`misaka-cli/src/setup/mod.rs:1508` asks the maturity floor.** Real (`coinbase_maturity()` rather
  than `coinbase_spend_maturity()`), but unreachable as a failure on mainnet — the settlement value is
  0 there, so the two numbers coincide — and the CLI's actual spend path already uses
  `is_spendable_settled` with both numbers (`wallet.rs:108-115`).
* **`DnsParams` operational knobs inside the fingerprint** (`params.rs:880`). Folded into M1-6, whose
  mechanism it shares; its own "an operator changes a knob" path does not exist, because nothing in
  the tree mutates `config.params.dns_params` at runtime, and at least two of the four fields it named
  as non-consensus are disputed.
* **`attn_output_gate` derived-vs-stored** (`qwen36.rs:117`). Real seam, but no remote input, artifact
  file or operator flag can produce the divergence — only a future internally-inconsistent geometry
  commit. Recorded as a note under M2-9 instead.

---

## What this audit did NOT cover

* **The EVM lane.** `evm_*_activation_daa_score` is `u64::MAX` on mainnet, and no lane exercised the
  EVM execution, bridge or deposit-lock paths beyond noting the fences.
* **The free-prompt (ADR-0044) lane** — `palw_freeprompt_v3.rs`, `palw_fp_objects_v3.rs`,
  `PalwReceiptSpendEnvelopeV3` — was read only where it intersects the shared admission and slashing
  code. Its own commitment/spend contexts are named in M2-23 but not audited.
* **Arithmetic correctness of the BASE-0 kernels and the SoftFloat second implementation.** The 2026-08-26
  work (`palw-base0-artifacts-and-ref2-findings`) is assumed; nothing here re-derives a kernel.
* **The Qwen3.6 / A16 numerical fidelity** beyond the graph-vs-engine correspondence of M2-9. Whether
  the engine reproduces the reference to the required precision was not re-measured.
* **Live-fleet behaviour.** Nothing was run against testnet-11; every claim above is from source, plus
  two probe executions of `qwen36_profile_v1`. Where a lane cited a measured RSS or a drill count, that
  number is quoted from the tree's own comments, not independently reproduced.
* **The DNS/PoS overlay's own consensus** (attestation eligibility, `StakeScore`, the finality fold)
  beyond the bond-spend seam of M1-1 and M1-3.
* **Wallet/CLI paths other than `send`, `utxo consolidate` and the setup readout.**
* **Performance under the fixes.** Several fixes here (verify-before-pool, unicast serve, full
  admission in `unentitled_blues`) move work onto hot paths and need their own measurement.
* **Exhaustiveness.** Twelve lanes and a refutation panel is a sampling method, not a proof. The two
  findings reproduced by execution (M2-9) were both found by reading first — which is a reminder that
  the tree's tests do not currently compare the two halves of a correspondence, and that is where the
  next class of defect will be.

---

## Shortest path to GO

### M1 (mainnet stays hash-only)

1. **Decide the DNS overlay's launch posture before anything else** (M1-1). Either set
   `bond_spend_gate_mergeset_activation_daa_score: 0` in `PRODUCTION_DNS_PARAMS`, or move
   `dns_activation_daa_score` above 0. The two fences must be armed in one preset. This is a
   parameter edit, but see item 3.
2. **Close the pruning-point poisoning** (M1-2): status-filter the children loop in
   `import_pruning_point_overlay_snapshot` and in its PALW twin, and move BOTH sidecar imports before
   `async_clear_pruning_utxo_set`. Acceptance: a two-node test where a disqualified child header
   carrying a garbage `overlay_commitment_root` exists at the pruning point, and the joiner completes
   IBD with its utxoset intact.
3. **Fix the upgrade path** (M1-6) *before* shipping item 1, or item 1 is a flag day with a guaranteed
   partition over its whole rollout. Hash a rule-set identity a scheduled fence does not move.
4. **Wallet and validator spend hygiene** (M1-3, M1-4, M1-5): exclude the bond outpoint in
   `misaka-cli`, split the signed-epoch cursor from the submitted cursor, and fix the wRPC error
   match. All three are small and independently shippable.
5. Re-run this audit's M1 lanes against the result, then launch.

### M2 (mainnet enables ConsensusV2)

**Phase 0 — the transport, days, parallelisable.** M2-1 (pull on verification, not emptiness;
verified-slot pooling; budget not spendable by unverified bytes), M2-2 (persist and pool only what
the chain carries and the node verifies; count and byte caps; unicast serve with per-peer and global
budgets; drop the mutex before the read), M2-14 (`Arc` the dense artifacts), M2-15 (the missing
`court_pending` guard), M2-13 (exclude the bond from the funding scan), M2-21 (read the local
retention store before accusing), M2-25, M2-26. None of these changes consensus rules.

**Phase 1 — issuance and the registry, weeks.** M2-3 (one predicate for payment and accountability;
a durable seen-attempt-id set), M2-6 (sign the whole object; bind it to its carrier), M2-8 + M2-16
(bound `activation_daa`, expire `Registered`, cap the registry, charge the reservation against the
ceiling), M2-12 (`initial_target` against the terms), M2-17, M2-18. Most of these move
`palw_ruleset_id_v2`, so they should land as ONE ruleset change — which is only possible after M1-6.

**Phase 2 — the adjudication layer, the actual blocker.** This is where the 2026-08-22 audit's Phase 2
was, and it is not closed. M2-4 (the challenger must bisect its own execution; the sweep must consume
the turn classification), M2-5 (either a chain-visible adjudicability gate or real court responders
for both Qwen families), M2-10 (a producer that cannot carry must not start; the opening rung must not
convict a party that has posted nothing), M2-11 (the ceiling, status and floor at `CourtOpened`),
M2-7 (a seat may only be charged for provable silence), M2-19, M2-24. **Acceptance condition, and it
is the one this project keeps proving it needs:** a live fleet round trip in which a real producer
injects one wrong tile, a third-party challenger actually convicts it, AND an honest producer actually
proves its innocence — with the ladder terminating on the tampered leaf index in the first case. A
one-way green is not evidence; that is precisely what hid M2-4.

**Phase 3 — the class layer.** M2-9 (fix the projection, add the graph-vs-engine correspondence test,
re-mint the qwen3moe class id **before** it carries value), M2-20 (recompute the RC blockrate and add
the two depth assertions to `validate_palw_v2()`), M2-23 (all eight contexts in the committed list).

**Then** re-mint with the new ruleset id, run a public testnet across **at least one pruning cycle**
(the only way M2-20's header horizon and M2-2's retention bounds are proven on real hardware), and
re-audit. Until all four phases pass, enabling ConsensusV2 on mainnet is NO-GO.
