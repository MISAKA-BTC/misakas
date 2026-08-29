# PALW — mainnet re-audit of the 2026-08-28 remediation (2026-08-29)

**What this is.** A re-audit of the 34 fixes recorded in
`docs/palw-mainnet-audit-2026-08-28.md`, read against one frame: **mainnet launches with this
build.** Branch `claude/mainnet-audit-fixes-9g6oh9`, six commits `1dc1e96..0493a1f` on top of
`main` (`8e982b7`), 25 files, ~2,900 insertions.

Three questions per fix, in order: (Q1) does the fix close the hole, (Q2) did it open a new one,
(Q3) does it have a mainnet consequence it does not account for. Refutation was the default; two
findings below were **reproduced by execution**, not by reading.

**Method note, stated because it changes what this audit is worth.** The intent was the same
twelve-lane fan-out the 2026-08-28 audit used. In this environment the workflow subagents cannot
call tools at all — the permission handler strips the required parameter from every `Bash`, `Read`,
`Grep` and `Glob` call before it executes (confirmed with a dedicated probe: three for three, not
transient). Twelve lanes each burned ~45k tokens and produced nothing usable. **This pass is
therefore single-threaded and is a sampling method, not a sweep.** M1 was read closely; the M2
adjudication and transport surfaces were read at the two points the original audit called
deciding, and not exhaustively.

**Tree state.** Build clean. `kaspa-consensus-core` 1342 passed / 9 ignored, `misaka-palw-base0`
163 passed / 2 ignored, `kaspa-pq-validator-core` 36 passed. Both fingerprint pins match the code.

---

## Verdict

**M1 — mainnet as it would ship today: still NO-GO.** Not for the reasons of 2026-08-28 — M1-1's
economic hole is genuinely closed, and the destructive half of M1-2 is genuinely closed. The new
deciding facts are that **two of the fixes compose into a silent chain split** (R-1), and that the
one they compose through is the fix meant to make scheduled upgrades safe (R-2). A refusal that
used to be loud is now a warning line.

**M2 — unchanged: NO-GO.** M2-4's mechanism does look repaired: the challenger now bisects its own
execution, so the ladder can diverge. But the original audit's acceptance condition — a live
round trip that convicts a real fault *and* acquits a real innocent — is still unmet, and nothing
in the tree tests it. Separately, M2-7's (correct) removal of the silence charge leaves the
accountability layer with no participation incentive of any kind (R-7).

---

## Findings

| # | sev | scenario | one line | file:line |
|---|---|---|---|---|
| R-1 | critical | M1 | the identity split re-admits the exact fork M1-1 creates: pre- and post-fence mainnet builds now peer and diverge silently | `protocol/flows/src/flow_context.rs:1398` |
| R-2 | high | M1 | the fence-normalisation list is incomplete and unguarded — the VLT/TKN fences this repo documents as the next fork still partition at deploy | `consensus/core/src/config/params.rs:798` |
| R-3 | high | M1 | the pruning witness is still selectable by one cheap block: blue-work ties are normal and the hash tiebreak is grindable | `consensus/src/pipeline/virtual_processor/processor.rs:10706` |
| R-4 | high | M1 | the wallet's bond exclusion is unconditional, so a legitimately released bond cannot be reclaimed with the shipped tooling | `misaka-cli/src/wallet.rs:109` |
| R-7 | high | M2 | with the silence charge gone and `Unavailable` still free, no form of seat non-participation costs anything; the producer pays for it | `consensus/core/src/palw_state_v2.rs:3058` |
| R-5 | low | M1 | the wallet's "surfaced, not swallowed" invariant has a fail-open hole: an unparseable bond outpoint is silently not excluded | `misaka-cli/src/wallet.rs:146` |
| R-6 | low | both | M1-4's safety argument is false (signing is hedged, not deterministic); the conclusion happens to hold, the stated invariant does not | `kaspad/src/validator_service.rs:1503` |

Plus one assumption risk, A-1, which is not a code defect but decides whether R-1 is a
deployment nuisance or a catastrophe.

---

### R-1 `[critical]` `[M1]` The identity split re-admits the exact fork M1-1 creates

**Reproduced by execution.**

`consensus_identity_id()` (`params.rs:798`) normalises **every** activation fence to a sentinel
before hashing, unconditionally. `flow_context.rs:1398-1409` then refuses a peer only when the
*identity* differs; a peer whose `consensus_params_id` differs but whose identity agrees is kept,
with a `warn!`.

M1-1 moved `PRODUCTION_DNS_PARAMS.bond_spend_gate_mergeset_activation_daa_score` from `u64::MAX`
to `0` (`params.rs:1759`). Both values normalise to the same sentinel. So:

```
genesis equal      : true
params_id differ   : true
IDENTITY EQUAL     : true   <-- the handshake KEEPS the peer
schedule differ    : true
```

(probe over `MAINNET_PARAMS` with the fence set back to `u64::MAX`, run against this tree.)

**Failure scenario.** Mainnet is running. Operator A upgrades to this build; operator B has not.
Same genesis, same network name, identity ids equal, so they connect and stay connected. A
enforces the mergeset bond-spend gate from block 1; B does not. A validator withdraws its
collateral through a merge-blue transaction: B accepts the block, A rejects it. Two chains, no
handshake error, one `warn!` line on each side that says the peers merely "run the same consensus
rules on a DIFFERENT activation schedule" — which is precisely what is *not* happening.

Before this changeset the same pair was refused at the handshake with `WrongConsensusParams`. The
fix that was supposed to make scheduled upgrades deployable turned a loud refusal into a silent
fork, and the first change to use it is a fence that is *already active*, not scheduled.

**Why the normalisation is wrong as written.** "Scheduled" and "active" are not the same fact. Two
builds that differ about a fence in the future agree about every block that exists today — that is
M1-6's insight and it is correct. Two builds that differ about a fence that is **already in
force** disagree about blocks right now. Normalising both cases to one sentinel erases the
distinction.

**Repair.** Normalise a fence only when it is in the future for both sides; compare it exactly
otherwise. The handshake has what it needs — take the local virtual DAA score (or, for a stable
pre-sync answer, the pruning point's), and for each fence hash the *predicate* `fence <=
current_daa` rather than the raw score. Two peers then agree on identity iff they agree about
which rules are in force now, and a genesis-active fence (`0`) can never be normalised away.

---

### R-2 `[high]` `[M1]` The normalisation list is incomplete, and nothing keeps it complete

**Reproduced by execution.**

`consensus_params_id()` hashes `dns_params` as **one borsh blob** (`params.rs:~880`), so every
nested field is in the fingerprint. `consensus_identity_id()` normalises six named `DnsParams`
fences. It does not touch:

* `dns.vlt.vlt_shadow_activation_daa_score` (`vlt.rs:1658`)
* `dns.vlt.vlt_activation_daa_score` (`vlt.rs:1677`)
* `dns.tkn.tkn_shadow_activation_daa_score`, `tkn_activation_daa_score`, `emission_activation_epoch` (`token.rs:625-633`)
* `full_reward_split_daa_score`, `mandatory_attestation_inclusion_daa_score`

```
VLT fence normalised: false
TKN fence normalised: false
```

The VLT pair is not a hypothetical. `params.rs:1799-1816` documents the next mainnet fork as
exactly `vlt_shadow_activation_daa_score: <H>` then `vlt_activation_daa_score: <H + span>`, and
`TESTNET_VLT_SHADOW_FORK_DAA_SCORE` is described as "the ONE constant a release cut has to
choose". **Scheduling it still partitions the network at deploy time** — the defect M1-6 exists to
remove, still present for the upgrade M1-6 was written for.

**Second half, structural.** `consensus_params_id()` destructures `Params` exhaustively on purpose,
with a comment telling the next author that a new field must be classified. `consensus_identity_id()`
is `self.clone()` plus assignments — a new fence added later lands in the identity silently and
re-opens the partition. That is the same class of defect as M1-6 itself.

**Repair.** Drive both from one exhaustive description: a single `for_each_fence(&mut self)` that
destructures, so adding a fence fails to compile until it is classified. Then R-1's
active-vs-future rule applies to all of them at once.

---

### R-3 `[high]` `[M1]` The pruning witness is still selectable by one cheap block

M1-2 replaced "check every child, any disagreement is fatal" with "check the heaviest child"
(`pruning_point_witness_child`, `processor.rs:10664-10710`). The destructive half of the fix is
real and holds: both sidecar imports now run before `async_clear_pruning_utxo_set`
(`ibd/flow.rs:2172-2190`), so a verification failure no longer leaves a node with its utxoset
deleted. That part is closed.

The selection is not. `best` is chosen by the tuple `(blue_work, hash)`:

```rust
if best.is_none_or(|(seen_work, seen_hash)| (work, child) > (seen_work, seen_hash)) {
```

`blue_work` of a block is `selected_parent.blue_work + Σ work(mergeset_blues)`
(`ghostdag/protocol.rs:182`). An attacker block whose selected parent is the pruning point can
merge exactly the same public blocks the honest child merges, so its blue work **equals** the
honest maximum; it can merge more and exceed it. On a tie the **larger hash wins**, and a miner
who is producing a block anyway can grind for a high hash at negligible extra cost.

The `StatusDisqualifiedFromChain` filter does not help here: this runs during IBD, headers-first,
where the pruning point's children are `HeaderOnly`. Nothing has disqualified them yet.

**Failure scenario.** An attacker mines one valid block whose sole parent is the block about to
become the pruning point, carrying a garbage `overlay_commitment_root`, grinding until its hash
exceeds the honest children's. Every joining node selects that block as its witness, the honest
peer's correct snapshot mismatches, and IBD aborts — against every peer, deterministically. The
node is no longer damaged (that is M1-2's other half), but it cannot join until the pruning point
advances, and the attacker repeats for one block's work at each new pruning point.

**Repair.** The witness must not be a single block an attacker can outrank. Take the majority
commitment across qualifying children (honest children are unanimous by construction), or defer
the check until the selected-chain successor of the pruning point is resolvable and use it. If the
heaviest-child heuristic is kept, the tiebreak must not be a grindable hash.

---

### R-4 `[high]` `[M1]` The wallet's bond exclusion strands a legitimately released bond

`bonded_outpoints()` (`wallet.rs:109`) asks `getStakeBonds` with `status_in: None` and
`pov_daa_score: None`, and every returned outpoint is marked `bonded`. `send` and `utxo
consolidate` then filter `!u.bonded` unconditionally.

Consensus is narrower than that. `PalwSpendLocks::locks` (`utxo_validation.rs:238-250`) treats a
bond as locked **unless** it is `Unbonding` and past its release DAA:

```rust
let releasable = effective_bond_status(bond, self.daa_score) == BondStatus::Unbonding
    && bond_release_daa_score(bond).is_some_and(|release| self.daa_score >= release);
!releasable
```

`BondStatus` has no terminal "withdrawn" state (`dns_finality.rs:344-358`), so a released bond
keeps its record and keeps being returned by the RPC. The sidecar's `unbond` command files the
unbond *request* only — it explicitly refuses to touch output-0 (`kaspa-pq-validator/src/main.rs:977`)
— and no other shipped command spends it.

**Failure scenario.** A mainnet validator unbonds, waits out `unbonding_period_blocks`, and tries
to move its collateral. `misaka wallet send` reports insufficient funds; `utxo consolidate` reports
nothing to consolidate. The 20M KAS is spendable by consensus and unreachable by the tooling. The
operator's remaining options are a hand-built transaction or a patched binary.

**Repair.** Exclude only bonds that are *not* releasable at the node's current DAA. The RPC already
returns the status and accepts `pov_daa_score`; mirror the `locks` predicate rather than the set of
all bond outpoints.

---

### R-5 `[low]` `[M1]` The wallet's fail-closed invariant has a fail-open hole

`bonded_outpoints` is documented as failing closed — "a wallet that cannot ask which of its outputs
are locked must not guess" — and the RPC error path honours that. The parse does not
(`wallet.rs:133-145`, parse at `:146`):

```rust
for b in resp.bonds {
    if let Some(op) = parse_outpoint_str(&b.bond_outpoint) { out.insert(op); }
}
```

An entry that does not parse is silently skipped, so that bond is *not* excluded and becomes
selectable — the exact outcome the function exists to prevent, reached by a malformed or
unexpected value rather than by an error. The format matches today
(`rpc/service/src/service.rs:1286` renders `"{txid}:{index}"`), so this is latent rather than live;
it should still be an error, not a `continue`.

---

### R-6 `[low]` `[both]` M1-4's stated safety argument is false

`validator_service.rs:1503` justifies rebuilding on `AllowRebroadcast` with:

> signing is deterministic here (the ML-DSA call takes a fixed `[0u8; 32]`), so the rebuild produces
> the identical signature and the identical transaction.

`sign_with_context` (`kaspa-pq-validator-core/src/lib.rs:230-232`) does the opposite:

```rust
let mut randomness = [0u8; 32];
rand::thread_rng().fill_bytes(&mut randomness);
```

Signing is hedged, so the rebuild yields a **different** signature; the funding UTXO may also be
re-selected, so the transaction is not identical either.

**The conclusion still holds**, for a reason the comment does not give: equivocation is decided on
`(target_hash, target_daa_score)` alone, and `dns_finality.rs:2127-2129` and `:2440-2444` state
explicitly that the signature fingerprint is *not* part of the predicate, precisely because ML-DSA
is hedged. So the rebuild is safe. One loose end: on a rebuild the record is not rewritten
(`if !already_recorded`), so the durable `signature_fingerprint` names a signature that was never
broadcast, and its documented purpose is to recognise an in-flight rebroadcast across restarts.

Worth correcting because the next author will read the comment as the invariant.

---

### R-7 `[high]` `[M2]` Nothing costs a seat anything any more, and the producer pays

M2-7's repair is right on its own terms: the chain was charging seats for silence it could not
observe, and both call sites were unable to supply the evidence (`palw_state_v2.rs:3058-3078`).
Removing the charge removes a slash that fired on honest nodes.

What it leaves is a layer with no participation incentive in either direction:

* a seat that files a receipt and is **refuted** pays (`bond(2).slashed > 0` in the rewritten test);
* a seat that files **nothing** pays nothing (this fix);
* a seat that files **`Unavailable`** pays nothing and needs no execution — M2-19, still open, is
  that `Unavailable` is self-asserted and free;
* no seat *reward* for answering appears anywhere in the transition.

The dominant strategy is therefore to never file a substantive receipt. When enough seats do that
the claim voids on `ReceiptTimeout` (`palw_state_v2.rs:4335`) and the entire loss falls on the
honest producer, whose work is discarded.

This is not an argument for restoring the old charge — that charge was unprovable, which is why it
had to go. It is that the remediation table records M2-7 as `fixed` and only M2-19 as `partial`,
which understates it: **after this changeset the accountability layer has no working incentive at
all**, and closing it needs the piece M2-19 defers (a price for `Unavailable`, or a reward for
answering, or both). That is a design decision and it is now on the critical path for M2, not
beside it.

---

## A-1 — the assumption everything in M1-1 rests on

Two of this changeset's decisions are legal only if **mainnet has not launched**: re-pinning
`MAINNET_PARAMS`'s fingerprint, and arming the bond spend gate at genesis rather than at a
scheduled height.

The only statement of that fact I can find anywhere is the previous audit's own sentence
(`palw-mainnet-audit-2026-08-28.md:144`). The tree does not corroborate it, and two nearby signals
point the other way: `genesis.rs:120-124` records that this genesis differs from "the prior
Argon2id-era mainnet genesis", and the 9B custody ceremony is described as complete
(`genesis.rs:139-143`).

I am not asserting mainnet is live — I could not establish it either way from source. I am saying
**this is not a fact the codebase carries, and both fixes depend on it.** If mainnet *is* live,
R-1 stops being a mixed-fleet nuisance and becomes a silent fork of a value-bearing chain, and
M1-1 has to ship as a scheduled fence instead — which R-2 says is not yet possible. Confirm it out
of band before this branch is cut, and record the answer in the tree.

---

## What holds

Stated because a re-audit that only lists defects misrepresents the work.

* **M1-1's economic hole is closed**, and closed narrowly: `TESTNET_DNS_PARAMS` overrides the same
  field with `TESTNET_VLT_SHADOW_FORK_DAA_SCORE` (`params.rs:2051`), which is already `0`, so the
  edit moves mainnet only. The four `dns_params` assignment sites confirm no other network reaches
  `PRODUCTION_DNS_PARAMS`.
* **M1-2's destructive half is closed.** A verification failure can no longer leave a joiner with a
  deleted utxoset and `is_utxo_stable` latched false. This was the part that required a datadir wipe.
* **M1-5 is closed and correctly tested.** The predicate now matches the payload, and the test
  reproduces the real borsh-wRPC rendering rather than asserting against the shape the code wanted.
* **M2-4's mechanism is genuinely different.** The challenger bisects its own execution and re-runs
  when it has lost it; two readers of one capture can no longer agree at every rung by construction.
  Unproven, not unrepaired.
* **M2-6, M2-12, M2-13, M2-14** read as real closures at their sites.

---

## Shortest path from here

1. **R-1 first, and before anything is cut.** It is the only finding that converts a loud failure
   into a silent one, and it is triggered by this changeset's own flagship fix. Hash the
   active/inactive predicate, not the raw score.
2. **R-2 with it** — one exhaustive fence enumeration driving both ids, so R-1's rule covers the
   VLT and TKN fences and nothing new escapes.
3. **A-1 out of band.** Confirm mainnet's launch state and write the answer down. It decides
   whether M1-1 ships as genesis-active at all.
4. **R-4** before any validator is asked to bond on mainnet — the exit path must exist before the
   entry path is used.
5. **R-3** before public IBD matters.
6. **R-5, R-6** are small and independent.
7. **R-7 is an M2 decision**, not an M2 repair, and should be made before the adjudication layer is
   re-costed — the M2-4 round trip the previous audit demanded will otherwise be run against a
   panel nobody has a reason to staff.

---

## What this re-audit did NOT cover

The single-threaded constraint above is the main one. Concretely, not read or not read closely:

* the M2 transport surface (`palw_gossip.rs`, `palw_producer.rs`, the panel's retention bounds and
  unicast serve budget) beyond the M2-4/M2-7 sites — M2-1, M2-2, M2-15, M2-21, M2-22, M2-25 are
  **unverified** here;
* the class-registration preimage's ambiguity properties (whether the new nine-field concatenation
  is injective), and whether `palw_network_domain_v2_for` reached *every* signer and verifier —
  a single missed site is a network-wide signature mismatch, and I checked the processor's sites
  but not the producer's;
* the qwen3moe stripper across flavors other than the fixture's (M2-9's fix verified by test, not
  by construction);
* the depth arithmetic's downstream effects — raising `pruning_depth` changes storage, IBD size and
  the pruning proof, and none of that was measured;
* everything the 2026-08-28 audit already listed as out of scope (EVM lane, free-prompt lane,
  kernel arithmetic, live-fleet behaviour).

The two findings reproduced by execution here were both found by reading first — the same lesson
the previous pass recorded, and the reason the correspondence tests it asked for still matter.
