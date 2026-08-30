# ADR-0066 — The heartbeat lane out of `header.bits`, and the inactivity leak out of node memory

Status: **Proposed** (2026-08-30). Supersedes the *implementation* of ADR-0060 Decisions 1–2 and
Decision 4; the doctrine those decisions state is unaffected. Both features ship OFF today
(`PALW_HEARTBEAT_LANE_ENABLED = false`, `inactivity_leak_daa = u64::MAX` on every preset), withdrawn
by the 2026-08-30 audit. This is the redesign that withdrawal asked for.

**Neither part is a re-mint. Both must be activations, and both must be fenced at TOP LEVEL.**

## Why the first implementation failed, sorted by cause

The audit recorded four findings. Sorting them by *mechanism* rather than by symptom is what makes
the redesign tractable, because only two of them are about `bits`:

| # | finding | cause |
|---|---|---|
| F1 | the lane can price the bonded lane off its own chain, permanently | **`bits`** |
| F2 | ε = 1 is not small against a V2 block worth 2 | the shared **blue-work** scale |
| F3a | sibling heartbeats share one POV, so nothing bounds their width | independent |
| F3b | the retarget can never rise above its floor | **`bits`** |
| F4 | the evidence walk terminates on a node-local fact | independent |

F1 is the fatal one and it is purely a consequence of the price living in the field the global
difficulty window averages. A V2 network runs at `MAX_DIFFICULTY_TARGET` because the class lottery,
not the hash target, is its throttle — so a window of heartbeat rows demands work 33,554,432 and no
bonded block can re-enter it. The fixed point is a heartbeat-only chain recoverable only by re-mint:
the self-feeding refusal ADR-0060 was written to abolish, reintroduced by its own remedy.

## Decision 1 — the lane gets its own algorithm id, and its price never touches `bits`

Four carriers were considered. Three are unusable:

* **A new header field** works and moves no genesis hash (the `palw_state_root` precedent shows a
  double-gated field leaves every existing preimage byte-identical), but it changes `Header`, the
  p2p conversion and the RPC model, and old binaries compute a different identity for the new
  headers. Cost without a matching benefit.
* **`header.palw_commitment`** is **disqualifying**: it is excluded from every PoW digest, so a
  cadence datum carried there is post-PoW and one solved header mints unbounded sibling identities.
  A cadence datum must be pre-PoW.
* **The coinbase payload** is body-level, and bodies arrive after headers. A price that cannot gate
  header relay makes header spam free.

**So: a new `POW_ALGO_ID_HEARTBEAT_V1`, whose target is a network constant rather than
`header.bits`.** Heartbeat headers carry the global expected bits like every other lane, so they
enter the difficulty window as ordinary rows and F1 and F3b both disappear — not as a tuning, but
because the quantity that fed back on itself is gone.

The current file argues against a new id ("it would need a new finalizer arm for identical bytes").
That arm is ~8 lines, and it buys three things: every rule stops being triple-gated on
`(id == 3) && ConsensusV2 && CONST`; a hash-network algo-3 solution can never be replayed as a
heartbeat, because the Layer-0 digest binds `pow_algo_id`; and the id becomes the fence's own
observable.

## Decision 2 — the slot rule is one block deep, and the evidence walk is deleted

F4 is not a tuning problem. `heartbeat_evidence`'s walk terminates on `Err(get_header) => break`,
which is a **node-local** fact: an archival node never hits it and a pruned node hits it at its own
pruning point. Two honest nodes then compute different verdicts for the same header — a partition
along the `--archival` flag.

A retarget is what needs ancestor evidence. With Decision 1 there is no retarget, so the slot rule
becomes a function of the selected parent alone: *is this heartbeat far enough after the parent's
timestamp*. `consensus/src/processes/heartbeat_evidence.rs` is deleted, not bounded.

F3a — sibling width — is **not** fixed by this and must be stated: the slot rule bounds the chain,
not the DAG. What bounds width is the price, which is now a fixed target rather than a floor the
retarget cannot leave.

## Decision 3 — ε stops competing with a V2 block's work

F2 is independent of `bits` and survives Decisions 1–2 untouched: ε = 1 against a bonded block worth
2 is parity at `ghostdag_k = 1`. The fix is that a V2 attempt block's blue work must not be
`calc_work(bits)` — the attempt lane's throttle is the class lottery, so its work should reflect the
inference it carries, not the hash target it did not need.

**This is the expensive decision.** It moves `header.blue_work` on every V2 block, so it needs its
own soak, its own golden vectors and a re-derivation of the pruning proof's level argument. It is
separable from Decisions 1–2 and should be staged after them.

## Decision 4 — the inactivity leak needs committed per-validator state

The leak must know how long EACH validator has been inactive. Today that lives in node memory
(`last_attestation_daa_by_validator`), which cannot be right: a value that decides block validity
and is not committed is a value two nodes can disagree about with no way to notice.

It becomes a per-validator table inside the DNS overlay's committed state, rooted into
`overlay_commitment_root` and riding `PruningPointOverlaySnapshot` so a pruned IBD imports it under
the existing trustless gate. Below the fence, and with an empty table, the root is byte-identical —
**no genesis hash moves on any preset**. `component_digests` gains a third digest, or the triage
line loses the ability to localise a divergence to the new half.

Two constraints that are easy to get wrong:

* **Entries for validators with no surviving bond must be dropped at capture**, or the table grows
  without bound and a from-genesis node disagrees with an importing one.
* **The branch-comparison prohibition stays structural.** `dns_reorg_outcome` must keep passing an
  empty leak view: a committed table does not fix the quorum-intersection problem, because a
  candidate branch commits its *own* table.

## The trap in the constant, and why both fences must be top level

`inactivity_leak_daa` lives inside `DnsParams`, which `consensus_params_id` hashes as one raw borsh
blob while `for_each_fence` deliberately does not visit it. Both halves are individually correct.
Together they mean **changing that constant from `u64::MAX` to a live value moves
`consensus_identity_id` and disconnects the first upgrading operator from every un-upgraded peer
immediately** — the deploy-day partition the identity split exists to prevent.

So the leak cannot be armed by editing that constant. It needs a top-level
`Option<PalwInactivityLeakV1 { activation, t_leak_daa }>` — ADR-0065 D1's exact shape, for its exact
reason: `for_each_fence` visits only the activation, the collapse takes the whole `Option`, and the
duration reaches `consensus_params_id` raw. `DnsParams.inactivity_leak_daa` is then retired at
`u64::MAX` permanently rather than reused.

The same applies to the heartbeat lane. Its current shape — a `const bool` that changes block
validity and moves no fingerprint — is **code-only masquerading as a rule**, which is a silent fork.
It becomes a top-level `Option<PalwHeartbeatParamsV1>`; the new algo id, the work constant and the
slot rule may not go into the V2 bundle, because the bundle's fences live under
`palw_ruleset_id_v2`, and a V2 network's doctrine is to re-mint rather than schedule.

## What this costs, honestly

**Contained (days):** deleting `heartbeat_evidence` and reshaping the slot rule (~120 lines out, ~30
in, two call sites); forcing heartbeat blocks to carry no chain position; relocating the leak fence
out of `DnsParams`; retiring `PALW_HEARTBEAT_LANE_ENABLED` for a params predicate (7 call sites).

**Multi-week:** the new algo id (finalizer arm, `check_algo_id_known` and its pruning-proof
implications, `accepts_algo_id`, `required_algo_id_for_mode`, the miner, the template adapter, and
the fingerprint fixtures for all five presets); Decision 3's blue-work change; and Decision 4's
fourth snapshot component with its apply/revert pair, reorg-symmetry tests, pruning-point capture
filter and pruned-IBD import tests.

Nothing here should be landed as a switch flip. ADR-0060 §12 already listed "what a correct
heartbeat lane needs" and "correct activation needs PERSISTED per-validator last-attestation state";
landing either without recording which of the four findings it closes, and how, would repeat exactly
the failure that produced them.

## Consequences

* Both features stay OFF until their fences exist and are armed. No preset fingerprint moves while
  they are `None`, so this ADR costs nothing to adopt and everything to arm carelessly.
* F3a (sibling width) is **not** closed by Decisions 1–2 and is recorded as open.
* The heartbeat lane remains the only known answer to trustless recovery from a total producer
  stop — ADR-0064's correction establishes that its own remedy does not close that, so this ADR is
  on the critical path for a property the chain currently does not have.
