# LG14-A — the legacy V2 Panel route under G14: one filer for everyone, a reservation that holds Final, and direct proofs that no session pre-empts

Lane LG14-A, branch `g14/legacy-filer` (from integration `b594620dd`, with lane INTF's `int/rs49-fence` merged so the conviction
economics below its fence are int-12's). User decision GAP-80 (2026-10-10): the legacy V2 Panel route, ARMED on testnet-12, must
also meet G14. This lane owns RFC-0014 §6–§7 (and §8 as far as the conviction path needs it) on that route. Lane LG14-B
(`g14/legacy-held-da`, `docs/design/palw/legacy-route-g14-held-da.md`) owns §4–§5: the public descent of the step and checkpoint
trees, the three canonical 8k held/fused DA gaps, the leaf recompute and the committed kernel witness.

Status words are the user's (2026-10-09): **implemented** (the code exists), **verified** (it compiled and its unit, real-node
E2E and attack tests passed on a recorded run of this branch), **armable** (also economic parameters, external review and
activation conditions). Nothing here is armable.

Allocations (Lead): fence `palw_legacy_public_filer_v1`; object tags 154–156; deltas 200–204; carriage tail `0xE2`; RPC ops
204–206. Used: tags 154–155, delta 200, tail `0xE2`, RPC 204–206. Tag 156 and deltas 201–204 stay reserved (§9).

## 0. The bar, and why the legacy route fails it today

The producer and EVERY Panel seat collude: every seat signs `Valid` / receipts. One public bonded verifier outside the Panel,
registered after genesis, holding its own copy of the registered model (ADR-0177), using only public authenticated material,
must reach one of:

- an objective conviction for computation fraud or a job / input / output / state violation covered by an active plan;
- the correct DA default for withheld claim material;
- a dismissal of an honest claim, or of its own wrong challenge.

G14C's matrix (`g14-completion-matrix.md` §3.9, branch `g14/completion`) records why F9–F16 fail. The cells this lane owns:

| Cell | Today | Reference |
|---|---|---|
| C2/C3 | the replay filer and the held filer are seat-only; the operator DA filer files row 0 only; no non-seat filer builds the descent | `kaspad/src/palw_filer_replay.rs`, `palw_operator_da.rs` |
| C6 | `ExecutorRefuted` (kind 4), `PanelFalseValidV2` (kind 3) and `TirIdentityMismatch` (kind 7) are refused with `ClaimUnderSession` while ANY court is open — another bond's session pre-empts a direct proof | `palw_offence_attribution_v1.rs` (three `open_courts_of > 0` checks) |
| C8 | past `palw_rcore_plus` only a SEAT's DA session pauses a V2 claim (DA-5, V3S-08). A bystander's accusation is outrun by the bind / receipt / challenge clocks; the claim voids uncharged or reaches Final while the pursuit is still descending | `palw_rcore_deadline_v1`; `t46…::t18c_ii_body` doc |
| C6 (budget) | DA-8's non-seat budget — three open, sixteen over the claim's life — is SHARED by every non-seat, so a producer's Sybils exhaust it | `da_admission_v1` |
| C1, C7, C8 | no chain-path test, no bond registered after genesis, no restart / reorg / IBD with a non-seat pursuit | — |

What already works on testnet-12 and is reused unchanged:
- one-move court accusations (`ShardCourtAccused`, `CheckpointAccused`) land whatever court is open past `palw_offence_attribution`
  (armed at genesis), and a void by any route closes every court session neutrally (ADR-0152 §4-ter C3);
- a disclosed held `StepLeaf` is adjudicated by the one-move verdict: a guilty answer convicts the producer
  (`convict_by_court_verdict_v1`), an honest one refutes the session;
- an unanswered unit defaults (DA-7): `ProducerWithholding`, S1 live / S3 after Final;
- `ExecutorRefuted` is objective, fee-paid and needs no bond; its reporter reward is R-3's commit–reveal (any Active bond);
- a conviction after `Final` reverses it (S3), within the claim's liability horizon.

## 1. The fence `palw_legacy_public_filer_v1`

The pattern every dormant fence follows (DA16's `palw_provider_court_v1` is the template):
- `None` on every preset and in no flag-day list;
- hashed Some-only into the params id and the schedule id;
- `Some(never())` collapsed to `None`;
- a `palw_fences_v1()` entry and a fork-id probe arm;
- `validate_palw_legacy_public_filer_v1` refuses arming on a ruleset that is not `ConsensusV2`, without `palw_rcore_plus` and
  `palw_offence_attribution` in force at or below it, and — until the full-activation release names its height — any arming at all.

The processor resolves it once (`palw_legacy_public_filer_at`) and hands the fold its height in
`PalwTransitionExtrasV1::legacy_public_filer_from_daa`. Everything past it is **state-driven** where it can be: a reservation can
only be written by the tag-154 arm past the fence, so the code that reads reservations (the deadline, the DA budget, the exposure
ledger) needs no fence of its own — below the fence the table is empty and every reader returns what it returned before.

## 2. Direct proofs over open sessions (RFC-0014 §7.4)

Past the fence, the three adjudicators no longer refuse a proof because another bond's court is open:

- `palw_check_executor_refuted_at_v1`, `palw_check_panel_false_valid_at_v2` and `palw_check_tir_identity_mismatch_at_v1` take a
  `PalwSessionRuleV1` (`RefusedUnderSession` below the fence — the old behaviour, byte for byte — or `DirectProofFirst`). The old
  entry points keep their signatures and delegate with `RefusedUnderSession`, so every existing caller (kaspad's filers, the
  tests) is unchanged.
- The processor's gate and the fold's consumer call the `_at` forms with the rule the fence gives at the block — one reading at
  both sites, as for every adjudicator.
- The conviction itself is the existing funnel. A live claim is voided (`CourtFraud`), and the void closes every open court
  session NEUTRALLY (challenger reservation released, nobody slashed, ADR-0152 §4-ter C3) and every DA session with exposure
  returned and `refuted_held` refunded (DA-6), and — new — every reservation on the claim (§3.6). That is one deterministic
  transition: conviction, session ends, exposure settlement and deadline release in the same block.
- Nothing said INSIDE a court can close another session; only a completed objective proof does.

## 3. The reservation `PalwDisputeReservationV1` (RFC-0014 §7.2–§7.3)

### 3.1 Objects

```text
154 DisputeReservedV1   { reservation: PalwDisputeReservationV1 { version 1, claim, execution_root, trace_root, reserver },
                          signature }
155 DisputeReleasedV1   { claim, reserver, signature }
```

Each is signed by the reserver's registered ML-DSA-87 key over
`palw_legacy_dispute_message_v1(network domain, tag, reserver, borsh(payload))` under the context
`misaka-palw/legacy-dispute/object/v1` (a new context, outside every live network's committed set; the Some-only fence covers it,
as DA16's does). The fee is the carrier's, as for every lifecycle object.

### 3.2 Admission of a reservation (tag 154), past the fence

1. The claim exists and has a pursuit to hold: `Provisional`, `PanelBound`, `ReceiptLicensed`, or `Final` with its vesting row
   unmatured (the post-`Final` DA stage, `FinalRow`). `Voided` and retired claims are refused.
2. The roots named are the claim's (`execution_root`, `trace_root`).
3. The reserver is an `Active` bond at or above the registry floor and is not the claim's executor — the same standing a DA
   accusation needs (`da_admission_v1`). It need not be a seat, a genesis card or an operator; it may be any of them.
4. **One reservation per bond per claim over the claim's life.** A bond that released, lapsed or was charged never reserves the
   same claim again (this is also the replay rule: a replayed tag 154 or 155 finds the bond closed).
5. `now ≤ hard_deadline` (§3.3).
6. Owner budgets: at most 64 retained dispute records per bond, including closed replay guards, and 64 live reservations per bond. Other bonds cannot spend this budget. The former claim-wide 64-live / 256-lifetime caps are removed; see the coalition-admission follow-up below.
7. **The deposit** fits the reserver's free half: `D = min(⌈r · S_P(stage)⌉, min_collateral)` — DA-6's exposure for one session at
   the claim's stage, with INTF's `r` (1,000 bps below `palw_reporter_share_v2`, 4,900 past it). It is checked through the one
   accuser gate (`palw_rcore_gate_room_of_v1`, `Accuser`) and joins the A-6 ledger (`palw_accuser_exposure_v1`).

Effects: the reservation row `{ reserved_daa, deposit, sessions_opened: 0 }`; the claim's record is created at its first reservation
with `hard_deadline`; the `Valid` locks of its signers and its vesting row are re-dated to `hard_deadline + window_challenge` exactly
as a DA session re-dates them (`da_rekey_v1`), so a conviction landing inside the hold finds them live.

### 3.3 The hard deadline is the claim's, not the reserver's

```text
hard_deadline(claim) = claim.trace_retention_daa − W_disclose
```

That is the last DAA at which the claim's retention obligation still admits a DA session (`da_admission_v1` refuses a session whose
deadline passes `trace_retention_daa`). Consequences:

- **No front-running of the clock.** It is fixed by the claim's own acceptance (`trace_retention_daa = accepted + bind + receipt +
  challenge + court`, pinned at admission), never by who reserves first, so a Sybil's early reservation cannot shorten an honest
  verifier's time and a late one cannot extend the hold. Another bond's reservation, a repeat, a re-send or a seat change never
  restarts it.
- **Bounded.** The hold never outlives the producer's retention obligation, and nothing is held for which no demand can still be
  made.
- **The §7.2 envelope** (`start + B_cold_fetch + B_check + B_localize + B_disclose + B_court + B_carrier + B_reorg_slack ≤
  hard_deadline`) is then a statement about the retention obligation: on testnet-12 the hold is ≈ `bind + receipt + court` after
  acceptance (≈ 3,200+ DAA). Whether that covers a class's worst case is MEAS's measurement (GAP-07), and a class whose measured
  envelope exceeds it is not DisputeAdmissible for this route. This lane does not assert it does.

### 3.4 What a live reservation holds (the Final condition, §7.3)

While at least one reservation on the claim is live, `palw_rcore_deadline_v1` gives the claim NO deadline. So nothing on the
deadline index can end it:

- no `Final` (and no retirement of a `Final` claim);
- no non-fraud end: no `BindTimeout`, `ReceiptTimeout`, redraw, `PanelUnavailable` or `NotReplayBacked` expiry;
- the claim is a pending accusation for `palw_accusation_pending_v1` (the RFC-0010 / part-C guards read it).

Every Final writer, the deadline sweep, the vesting and work-rights readers see the same predicate, because they all read DL-1's one
deadline function (`assert_deadline_consistency` checks the index against it at every load). The panel still binds, seats still
license, and objects still land: the hold is the clock's, not the claim's. This is RFC-0014 §7.3's AND:

```text
Final(c) ⇐ … AND no live reservation on c AND no open seat-like DA session on c AND no open court on c
```

**Lapse.** At `hard_deadline` the sweep (`sweep_legacy_disputes`, right after the DA sweep, in the fold and in the pre-object
base) lapses every live reservation of the claim: each deposit moves to `dismissed_held` (§3.6) and DL-1 re-derives the claim's
deadline from its own, uncredited anchors. A claim whose natural deadline passed during the hold is then swept in the next block:
an honest licensed claim reaches `Final` at once; a claim no panel licensed takes the timeout it would have taken. No time is
credited back — the hold is the challenger's, not a seat's (DA-5's credit is for seats that could not get the material), so a
reservation never lengthens an honest claim's own windows; it only defers their end.

### 3.5 Reserved DA sessions: the reserver's own budget

A DA demand by a bond that holds a live reservation on the claim (any demand object that opens an R-core+ session —
`DefaultAccused`, `DefaultAccusedHeld`, `DefaultAccusedTirStep`, `DefaultAccusedPipelineStep`, and LG14-B's tag 157, all of which go
through `open_da_session_rcore_v1`) is a **reserved session**:

- it is admitted on the RESERVATION's budget — at most 34 sessions per reservation (`PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1`:
  the localizer's worst case is `⌈log₂ ⌈n / 1,024⌉⌉ + 3` demands, 33 at `n = 2⁴⁰` leaves), one open at a time (DA-1) — not on DA-8's
  shared non-seat budget (three open, sixteen ever), which it neither reads nor spends;
- it is seat-like for DA-5 (`accuser_is_seat = true`): it pauses the claim while open and its pause is credited back at its close,
  exactly as a seat's — so a session opened just before `hard_deadline` still runs to its own deadline;
- everything else is DA-1…DA-8 unchanged: the stage, the retention bound, DA-3's draws, DA-6's exposure, the answer rules, a
  default's charge and its named reward to the session's accuser.

A seat that also reserves keeps its seat budget. A bond whose reservation ended opens ordinary non-seat sessions again.

### 3.6 Outcomes and the deposit

| Outcome | Reservation | Deposit |
|---|---|---|
| conviction (any route: kind 3/4/7, a guilty disclosed leaf, a court, LG14-B's tag 159) or a DA default | every live one closes | refunded; every `dismissed_held` deposit on the claim refunded too |
| the claim voids for a reason nobody was convicted under | every live one closes | refunded; `dismissed_held` stays |
| the reserver releases (tag 155) | closed | moved to `dismissed_held` |
| `hard_deadline` passes | lapsed | moved to `dismissed_held` |
| the claim retires | — | every `dismissed_held` deposit not refunded is burned (`slash_seat`, capped at the floor), earning nobody anything |

This is DA-6's rule for a refuted session applied to the reservation: a challenge that ends without an objective outcome costs its
deposit unless somebody later proves the claim false, in which case it was right and is made whole. "Dismissal of an honest claim's
challenge" is the release or the lapse followed by the claim's `Final` and retirement.

The hooks are single: the release of every reservation runs inside `da_release_all_v1` (which every void, conviction, reversal and
DA default already calls), and the burn inside `retire_claim` beside `da_retire_v1`.

## 4. The common filer `palw_fraud_filer` (RFC-0014 §6)

### 4.1 One engine, every role

`kaspa_consensus_core::palw_fraud_filer_v1` holds the engine's pure half; `kaspad/src/palw_fraud_filer.rs` runs it on a node.

```text
Discover → ResolvePlan → Check → Mismatch → ReservePursuit → Localize → RequestCommittedWitness → JudgeExact
         → SubmitExecutorRefuted / PlayBoundCourt → Convicted / Dismissed / DaDefault / Expired
```

- A Panel seat, a genesis operator, an ordinary bond holder and an external watchdog call the SAME builders. What a role may change
  is discovery (which claims), priority and the local budget — never what counts as evidence, which terminal a case may reach, or
  what the public reads return. The engine never filters by producer or signer: a genesis operator's claim is a claim.
- `Mismatch` is local; `ReservePursuit` onward is chain state. A local checker failure or a resource refusal is never filed as
  fraud.
- Adapters: a public read (the tip state and accepted lifecycle objects through `ConsensusApi` / the RPC ops of §6), a family
  checker (the honest replay of the claim's job on the verifier's own model copy, authenticated against the registered root), a
  localizer, and a carrier builder (sign, fee, submit).
- Localizers: the legacy bisection (`PalwLegacyBisectV1`): the binding read (`DefaultAccused` of event `(0, 0)`, whose answer carries
  the step binding the fold authenticated against `execution_root`), width-1 `StepRange` demands halving the interval that holds the
  first leaf differing from the verifier's own run until it fits one 1,024-leaf range, that range, then the `StepLeaf` terminal —
  `⌈log₂ ⌈n / 1,024⌉⌉ + 3` reserved sessions at most; LG14-B's tree descent replaces the halving where it lands, and a fused located
  leaf is its held dissection's (`HeldRoute`).
- Terminals: a guilty disclosed leaf convicts in the fold itself; otherwise the engine builds `StepArithmetic` → `ExecutorRefuted`
  from the disclosed openings and its own artifact rows (P2-8's builder), with R-3's commit–reveal for the reporter reward.

### 4.2 The seat and operator modules

`palw_filer_replay.rs`'s mismatch, named leaf and conviction door, and `palw_operator_da.rs`'s `Refuted` verdict, feed the same
engine: a self-consistent garbage trace that answered row 0 is no longer "handled" by the row-0 accusation — the engine reserves and
descends. The operator module's producer/signer filter is a discovery preference, never a permission.

### 4.3 Node, RPC and restart (§6.3)

- The node keeps, per (claim, plan), the case phase, the local material digest (the honest root it computed), the carriers it sent
  and the deadlines. All of it is a cache: the progress that matters — the reservation, the sessions, the answered units, the
  convictions — is chain state.
- On restart the engine re-derives every case from the canonical chain: a live reservation of its bond, its open sessions, the
  answered units, the claim's phase. It never reuses an opening or receipt from a block a reorg removed (it reads the units the
  canonical state marks answered, from the accepted blocks of the canonical chain only). A proof re-sent after a restart is a no-op
  in the fold (one offence per claim / per seat and claim), so the producer is never charged twice.
- No flag: identity arms it, as lane B's duty — a node that carries (`--palw-fee-outpoint`) runs the engine for its bond past the
  fence, and is a clean no-op below it. The node module is `kaspad/src/palw_fraud_filer.rs` (a child of `palw_panel`): one replay in
  flight in the seat's spare replay slot, at most two mismatched captures held, one queued item a case on the court lane (rounds
  `u32::MAX − 10` reserve, `u32::MAX − 11` demand), each item sent at most twice. Nothing is persisted: the candidates and the case
  facts come from the tip (`palw_fraud_filer_candidates_v1`, `palw_legacy_dispute_v1`), the answers from a walk of the accepted
  blocks back to the oldest pursued claim's acceptance, the honest run from a fresh replay. A seat's claim is left to the seat's own
  duties (P2-6, P2-8b/8d), which are not yet rewired onto the engine.

## 5. Pre-emption and resources (RFC-0014 §7.4)

| Attack | Why it fails |
|---|---|
| the producer's Sybil (or a colluding seat) opens a court first | direct proofs land over it (§2) and the conviction closes it |
| a Sybil reserves first and does nothing | the hold is shared and its end is the claim's (§3.3); the honest verifier's reservation and budget are its own |
| Sybils exhaust DA-8's shared non-seat budget | a reserved session never reads it (§3.5) |
| Sybils fill the former 64 live reservations of a claim | there is no shared admission count: a new owner with headroom may reserve and use its own sessions |
| Sybils fill the former 256-reserver lifetime | closed guards consume their owners' budgets, never a shared lifetime budget; the newcomer still has its own headroom |
| misdirection ("the lie is at leaf j") | nothing on chain is a claim about where the lie is; the shared progress is only the units the fold authenticated, and every verifier compares them with its own replay |
| serial reservations to hold an honest claim | the hard deadline is absolute; each reservation is a deposit at risk |
| spam against an honest producer | each reserved session is charged; each owner has at most 64 retained records and 34 sessions per record. The per-claim response envelope now scales with its distinct owners; largest-population processing and inclusion remain to be measured, not proved by deposits alone |

**Residuals, named.**
- **Reward front-running by the producer.** A guilty disclosed leaf rewards the accuser whose session demanded the unit FIRST. A
  colluding producer knows its own lie, so its Sybil can demand the guilty unit first and take the named reward (or the DA
  default's). This is ADR-0032's self-reporting: the producer recovers `r` of what it is slashed, its net loss ≥ (1 − r) of the
  collected slash. The deterrence analysis must use the net figure (ADR-0176 D6). Not closed here.
- **Inclusion.** All of the above assumes the honest verifier's carriers are included within the network's inclusion bound
  (RFC-0014 §2.1). Censorship by every block producer is outside the bar.
- **Post-`Final` reservations** hold only the vesting row and the locks; DA-5's pause does not apply to a terminal claim.

## 6. RPC (ops 204–206)

- **204 `getPalwLegacyDispute { claim_id }`** — the public read of RFC-0014 §6.3 for one claim: class, phase, roots, retention,
  `hard_deadline`, the deadline DL-1 gives, the live reservations and `dismissed_held`, the open DA sessions with their units, the
  answered units, the open courts, and whether its kind-4 / court / DA-default offences are recorded.
- **205 `getPalwLegacyDisputes { reserver?, limit }`** — the claims with a live reservation (by reserver when given).
- **206 `getPalwFraudFilerStatus { bond }`** — one bond's standing as a filer, read off the tip (so any node answers it for any
  bond): the fence, its live reservations (deposit, sessions opened, hard deadline), the deposits held on it, its exposure.

204 and 206 answer a versioned camelCase JSON document (`PalwLegacyDisputeObservationV1`, `PalwFraudFilerStatusV1`), as op 220 does;
205 answers claim ids. Proto payloads 1222–1227 (the op-to-payload rule `1214 + 2 (op − 200)`).

## 7. Economics (RFC-0014 §8), as far as the conviction path needs them

- The deposit is DA-6's session exposure at the claim's stage, with INTF's share (§3.2). Nothing new is paid out.
- A conviction uses the existing funnel (S2/S3, U3, the forfeits, the reporter reward R-1 at INTF's `r`). The reservation changes
  WHEN a claim may end, never what a conviction charges.
- **ADR-0176 (lane BUDGET).** This lane adds no reward and no consensus weight. Its one interaction with the bond budget is a
  DEFERRED `Final`: the claim's B/R/F reservation, taken at claim acceptance, must be re-checked at the deferred `Final` exactly as
  at an undeferred one, and `reuse_not_before = d + W` is unchanged (a hold neither releases nor extends the budget). The hook
  BUDGET's engine owns is `finalize_claim`'s re-check; this lane builds no part of that engine.

## 8. Wire, state, A-2 and byte identity

- **Objects**: tags 154 and 155 (§3.1). Tag 156 reserved.
- **State**: `legacy_disputes: BTreeMap<claim, PalwDisputeClaimV1 { opened_daa, hard_deadline_daa, live: BTreeMap<bond,
  PalwDisputeReservationRowV1>, closed: BTreeSet<bond>, dismissed_held: Vec<(bond, u128)> }>`, written through one journaled writer
  (delta 200 `LegacyDispute`, deltas 201–204 reserved), carried in tail `0xE2` and rooted in a Some-only block
  (`legacy_disputes/v1`) — both only when non-empty, which nothing below the fence can make it. Two derived indexes are rebuilt at
  load and checked against the map: `(hard_deadline, claim)` for the sweep and `(reserver, claim)` for the exposure ledger.
- **A-2.** Below the fence an object of tags 154–155 is the live int-12 build's undecodable payload:
  - isolation: its may-ride arm is unconditionally `Ok` (no stateless refusal at any height), so a block carrying one is valid
    here exactly where int-12 tolerates its bytes;
  - the acceptance walk drops it by name, first, charged nothing (no slot, rent, budget or refund);
  - the fold refuses it as the second lock.
  Rows for lane A2U's central table (`palw_lifecycle_kind_owner_v1`, `PALW_LIFECYCLE_NEW_KINDS_V1`):
  `(154, "DisputeReservedV1", LegacyPublicFilerV1)`, `(155, "DisputeReleasedV1", LegacyPublicFilerV1)`, and the fence variant
  `PalwLifecycleKindFenceV1::LegacyPublicFilerV1` → `"palw_legacy_public_filer_v1"`. No appended variant rides inside an int-12
  kind, so no `CarriedAppended` row; a `StateEncoding` row for tail `0xE2` (written only past the fence).
- **Byte identity.** The unarmed fold of every int-12 object is unchanged: the adjudicators' old entry points, `da_admission_v1`
  and `open_da_session_rcore_v1` read an empty table, and `da_release_all_v1` / `retire_claim` / the sweep touch nothing. A test
  folds the same scripted chain with the fence unset, `Some(never())` and armed-but-unused and compares roots and deltas;
  `scripts/t12-repin.sh --shipping --drift-only` must show no drift.

## 9. Tests

1. **V-unit** (consensus-core): the fence pattern; the messages and signatures; admission refusals; the hard deadline; the hold in
   DL-1; the lapse; release; every outcome row of §3.6 with exposure conservation (the deposit is reserved, then refunded or
   burned, never both); the reserved-session budget and the non-seat budget left untouched; direct proof over an open court with
   the court closed neutrally; the carriage / root / delta round trip; byte identity unarmed.
2. **V-node** (`consensus/src/pipeline/virtual_processor/tests/lg14a_legacy_filer_e2e.rs`), the fence test-armed WITHOUT its
   validation (as `g14_kernel_route_e2e` arms its own): testnet-12's harness; every seat signs `Valid`; ONE bond registered after
   genesis through a real `0x4b` carrier, which is never a seat of the claim; every object through the mempool, the node's own
   template, the chain block's fold, the persisted tip and `ConsensusApi` reads.
   - a self-consistent lie (an injected step fault) convicted BEFORE `Final` by the reserved bisection;
   - a lie convicted AFTER `Final` (reserved at the `FinalRow` stage);
   - a withheld unit defaulted (LG14-B's held units ride the same reservation when they land);
   - an honest claim: the challenge dismissed, the claim `Final`, no producer charge, the deposit held;
   - pre-emption: a Sybil reservation and a colluding seat's court cannot stop the conviction, and the honest reservation's
     sessions are admitted with DA-8's shared budget exhausted;
   - a second node fed the blocks (IBD) reaches the same roots; a reorg away and back, and a restart, give the same state.
3. **The unarmed twin**: the same script with the fence off — the reservation is dropped by name and the bystander is outrun, as
   G14C's matrix records.

## 10. G14 cells and what remains

Closed by this lane when §9.2 passes (V-node, legacy family, non-held floor class): C1 (post-genesis bond), C6 (direct proof over
sessions, own budget), C8 (the hold; restart, IBD, reorg), C7's chain path except the served-RPC leg, C5 for a non-fused step lie
before and after Final and for the dismissal.

Remaining, stated:
- the held / fused / canonical 8k rows, the tree descent and CKW: LG14-B (their sessions take this lane's reservation unchanged);
- C2 "full" (a node started after the claim that prosecutes from ITS OWN reads): the engine runs on any node's reads, the E2E's
  second node replays; a separately launched node's RPC leg is G14C's harness;
- the served RPC leg of C7 (ops 204–206 are tested through the wire models and the conversion only);
- MEAS's measured envelope against `hard_deadline` (GAP-07); the deposit factor, the caps and the burn-vs-compensate choice are
  POLICY values;
- the reward front-running residual (§5);
- BUDGET's re-check at the deferred `Final` (§7).

## 11. LG14-B interfaces (after the integration merge)

- **Final hold.** LG14-B's tag-157 demand opens its session through `open_da_session_rcore_v1`, so a reserver's descent sessions are
  reserved sessions: seat-like for DA-5, on the reservation's own budget, and the reservation holds `Final` and every timeout for the
  whole descent.
- **The fused terminal after Final.** `ShardCourtAccused` (the held dissection's one move) is admitted on a `Final` claim when the
  accuser's own reservation on it is live; below the fence the reservation table is empty and the refusal is unchanged.
- **The common engine.** `palw_fraud_filer_next_descent_v1` runs LG14-B's `palw_legacy_descent_next_v2` behind the same prelude
  (reserve, one session at a time, the binding read); `PalwLegacyProbeV1::HeldNode` builds tag 157 and names
  `PalwDaUnitV1::LegacyHeldV2`. The node module still runs the bisection only: building the verifier's own tree
  (`PalwLegacyOwnTreeV2`) and reading tag-158 frontiers in the node is not done.
- **A2U.** `PalwLifecycleKindFenceV1::LegacyPublicFilerV1 = 5` owns tags 154–155 (allocation 154–156), with a `StateEncoding` row for
  delta 200 / tail `0xE2`.
- **RPC.** Ops 204–206 are built by `kaspa_rpc_core::convert::palw_legacy` (the service's handlers call them and nothing else); the
  real-node suite calls the same builders on a node started after the claim and carries the answers through the JSON wire form.

## 12. Handover status (2026-10-10, moved to codex)

Branch `g14/legacy-filer`, merged with integration head `68bde1ea6`. Status words: implemented / verified / armable. Nothing is
armable: `validate_palw_legacy_public_filer_v1` refuses every height.

**Verified** (binaries run on this head): core lib `palw_legacy_public_filer*` unit tests (bisection bound, engine roles, descent
mapping, facts read, demand builder, fence dormant / Some-only hashing); `lg14a_legacy_dispute_fold` 7/7 (unarmed byte identity,
hold of Final / bind / receipt, lapse + burn, reserved budget, default refunds, admission refusals, direct proof over an open court);
`rcore_m3_da_court`, `rcore_m5_reorg` green; kaspad `palw_fraud_filer` book tests; rpc-core round trips for ops 204–206.
Before the integration merge, `scripts/t12-repin.sh --shipping --drift-only` showed only drift identical to OPVB's log on the base
(schedule ids of every preset; t12 params/identity unchanged) — not re-run after the merge.

**Implemented, not verified**: the real-node E2E (`lg14a_legacy_filer_e2e`, 8 tests) — compiles, FAILS on this head (see next
steps); the node filer's tick in `palw_panel` (dormant; never run in a loop); the fused dissection on a reserved `Final` claim
(`ShardCourtAccused`); `palw_fraud_filer_next_descent_v1` + `PalwLegacyProbeV1::HeldNode` (unit-tested mapping only); RPC handlers
through `kaspa_rpc_core::convert::palw_legacy`; candidates / status reads.

**Next steps, in order**
1. E2E pursuits stall ("no verdict in 400 steps, 5 demands"; restart test 2). Before the merge the failure was the nonce bucket
   (fixed: `0x47_0000 + nonce`). Diagnose with `--nocapture`: likely the Range demand's answer never reads (the producer answers via
   `producer_answers`, which builds Held answers; check `answer_on_chain` scans far enough — 512 blocks — and that DA16b/LG14-B's
   merged admission still routes reserved sessions). The dismissal test: Final not reached 3 beats after release (DL-1 rearm on
   release; check `legacy_dispute_hold_ended_v1` re-arms past `window_challenge`). Reorg test: Z does not return to A (A needs more
   work than B: raise `target`).
2. `t12_a2u_new_kinds_uniform`: the harness arm for `palw_legacy_public_filer_v1` was added after the last build — re-run.
3. Integration-owned failures seen here (not LG14-A): A2U pin of tag 109 and `dns_finality::TakeoverToken (gone)` unclassified.
4. Move the E2E onto G14C's canonical harness (`g14_kernel_route_e2e/canonical.rs`: `ibd_node`, `Rpc`) so the verifier itself reads
   through a fresh node; today a fresh IBD node only re-serves ops 204/206 identically after the conviction.
5. Node filer: run the LG14-B descent (build `PalwLegacyOwnTreeV2` from the run, read tag-158 frontiers); rewire the seat modules
   (P2-8b/8d, lane B) onto the engine.
6. Re-run repin on the final tree.

**Traps**: A2U fence index — LG14-A is `LegacyPublicFilerV1 = 6` (BUDGET took 5). Court-queue rounds `u32::MAX − 10/− 11` and proto
payloads 1222–1227 are this lane's choice, unallocated by the Lead. Every merge so far damaged brace pairs at conflict tails
(carriage encoder/decoder, `palw_legacy_held_signature_ok`): check `git diff integration -- <file> | grep '^-'` after merging.


## 13. Codex audit of the merged filer (2026-10-10)

`e9eee4911` integrates `g14/legacy-filer` at `51d026bc9` into the reviewed kernel branch. The pre-fix node batch reproduced
**1 PASS / 7 FAIL**; §12's implementation claims are not end-to-end verification claims.

The legacy midpoint algorithm was unsound for sparse faults. Comparing a single leaf at the middle does not authenticate the
prefix it skipped: an adversary may change one earlier tile and leave the midpoint and all later tiles intact. The old unit test
used only a monotone predicate (`leaf < lie`), so it never exercised this case. `PalwLegacyBisectV1` now scans consecutive bounded
ranges (the historical API name is retained) and ignores disconnected range observations. The worst-case count is
`ceil(leaves / 1024) + 2`, including the binding read and terminal. A ladder that exceeds the 34-session reservation cap is
explicitly Unjudged by this fallback; the code no longer promises a logarithmic bound for an unauthenticated prefix. The tests
cover isolated faults, matching suffixes, and multiple disjoint faults. LG14-B's authenticated subtree descent remains the
required route for larger ladders; this repair does not establish that the production filer has connected it.

The material reader also authenticates bytes before caching or learning them. A block may carry a `MaterialDisclosedV2` that the
fold rejected, followed by a genuine answer to the same unit. A tip's `answered(unit)` flag does not authenticate every historical
carrier with that name. The binding must recompute to the claim's execution root; a range must have the requested index/count
and its Merkle path must reach that binding's step root. Invalid responses cannot poison the first-entry cache, advance the
localizer, or trigger own-model replay. A later valid response is still usable.

Harness corrections are distinct from protocol changes: the dismissal test waits for the actual rearmed Final deadline;
reservation-hold coverage deliberately waits past the unheld Final floor before pursuing, rather than expecting the localizer to
be slow; and a reorg restore test uses the post-Final reservation stage, where both branches retain equal Final work and a shallow
GHOSTDAG win is admissible. A pre-Final held branch cannot displace an unheld Final branch merely by mining more heartbeats under
the production strict-economic-win rule. These corrections do not relax that rule or arm a fence.

Remaining review items include the production LG14-B tree/CKW/fused-terminal connection, a fresh verifier prosecuting through its
own served RPC, complete measured deadline/inclusion bounds, and reservation saturation. `64` simultaneous and `256` lifetime
reservers are hard claim-wide caps: the single-Sybil E2E does not prove that an outsider can obtain a reservation after a coalition
fills them. Direct-proof priority alone does not supply missing localization material. No G14-wide or activation PASS follows
from fixing the small-range fallback.

Verification of the repair: core filer **10 PASS**, reservation fold **7 PASS**, real-node batch **7 PASS / 1 FAIL**, followed by
**1 PASS** for the corrected reorg case. The other seven node scenarios cover pre-Final and post-Final conviction, DA default,
honest dismissal, a second reservation, restart, and the unarmed twin. A2U is **3 PASS** and shipping repin reports no drift. The
reorg correction stays inside the production two-DAA shallow tie window; it does not relax fork choice. These are separate runs,
not a single all-green eight-case batch. Disk-full build/run interruptions are recorded separately from code failures. Logs and
the test boundaries are in [the audit implementation record](../../audit/g14-review-2026-10-10/implementation.md). The default-feature
`kaspad --lib palw_fraud_filer` suite also passes **5/5** after the task's regenerable build cache was cleaned.

### History reader follow-up

The production filer now reads resumable pages of accepted history. DAA distance is not a chain-block count. Only reaching the
requested floor or chain root completes a walk; missing headers, acceptance, bodies or selected parents leave the watermark
unchanged. A backfill retains its original tip, so arrivals during it are read afterwards. A new pursuit backfills its own old
answers even when newer than an earlier case, and a reorg outside the anchor restarts from the oldest pursued claim on the new
branch. Only authenticated units used by the contiguous localizer enter the deduplicated cache: one binding and at most 32 ranges
per pursued claim. Filer **7 V-unit PASS** and accepted-history reader **4 V-unit PASS** cover pagination, repeated DAA, later tips,
reorgs, read failures/restoration and the cache unit bound. This does not establish full C2 on a freshly started real node.

The original pagination repair alone did not retrieve permanently pruned answers: ordinary DA admission rejects an already answered unit. The numbered reacquisition below adds a fresh public disclosure obligation for supported units. Full-node retention/startup and the remaining unit families still require validation. The [audit record](../../audit/g14-review-2026-10-10/implementation.md) contains the logs.


## 回答済みunitの再取得（Codex、2026-10-10）

公開carrierがpruneされた時に、`answered`だけを理由に追及を止めないため、LG14-Aへ割当済みの未使用tag 156を`DisputeReacquiredV1`へ使う。既存の休眠`palw_legacy_public_filer_v1` fence内であり、heightの有効化は行わない。

要求はclaim、reserver、予約の受理DAA、次のsession番号、署名付き期限、unitと必要なbindingを持つ。session番号は予約rowの`sessions_opened + 1`に一致し、要求期限はclaim自身のhard deadline以下でなければならない。既存DAのActive/floor、担保・exposure、1 bond/claimの1 open session、予約の34-session予算とretention期限を維持する。再取得は既に回答済みの1 unitだけを要求し、追加のランダムdrawを行わない。

historical `answered`は消さない。従来の番号なしaccusationは再取得の前後も拒否される。open sessionがある間は`DaSessionAlreadyOpen`が先に返ることもあり、session終了後は`DaUnitAlreadyAnswered`となる。新要求もsession番号を受理時に消費するため、回答後に古い署名を再送して別sessionを開けない。未回答の明示的再取得はdispute recordのreserver→unit mapで保持し、通常のdispute writer／delta 200／休眠tail 0xE2へjournalする。回答admission、producer／covering signer duties、session close、defaultは同じfreshness predicateを読む。

producer又は依然責任を負うsignerの再公開は既存tag 55／158を使い、同じclaim root、binding、openingを検証する。新しい署名を必要とせず、第三者が元の正しい署名付き回答を再掲してもよい。そのbytesが再び公開されたこと自体が要求を満たす。異なるunitの回答、又はFlatがcoverしないtileは満たさない。正しい再公開は同unitを待つ全sessionを閉じ、非応答はretention内の通常DA defaultへ進む。算術のCourtFraudとは区別する。

公開viewとop 204のJSONには`flatAnsweredRows`を追加し、Flatによるhistorical回答も個別unitを列挙せず伝える。Flatのcoverageはin-runのtile 0だけであり、隣のtileや範囲外のrowへ広げない。

nodeの共通filerは、必要unitの公開bytesがない場合、30 DAAの履歴再試行後に明示的要求を署名してqueueする。signatureのretryと次の番号の要求は区別する。履歴readerは古いsuffixで失敗しても、それ以前に読めた独立に認証済みの回答を保持する。floorの読了watermarkは進めず、次tickで新しいtipから再開するため、古いcarrierが永久に欠けていても新しい再公開を読める。

現段階の対応unitはEvent、従来Held（prompt tile／state chunk／step range／非fused leaf）、LG14-B frontier／CKW。TIR／pipelineの自動再取得、pruneされたFP job commitmentそのものの再取得、input未取得のbootstrap、全枠飽和、fresh nodeの実service／包含、最大profileと累積scopeの完成は別途必要である。再取得回数も既存予算を消費するため、任意回数のrestartや無期限の取得を保証しない。検証結果は監査記録へ追記する。

## Coalition admission follows the reserver's own budget (Codex, 2026-10-10)

The former 64-live / 256-lifetime claim caps let a funded coalition exclude a fresh public verifier. Burning the coalition's deposits is an economic deterrent, not prosecution reachability. The never-armed public-filer fence now admits reservations without either shared count test. Active status, collateral floor, exact roots, once per bond/claim, deposit headroom and the claim's absolute hard deadline still apply. Reserved sessions continue to use their owner's 34-session budget, bypassing DA-8's shared 3-open / 16-lifetime non-seat budget.

State is attributed to owners instead: each bond may retain at most 64 claim records, including live rows, released/lapsed deposits and closed replay guards with zero exposure. Releasing a reservation does not free its retained-record budget. The derived `(bond, claim)` index includes every live/closed row; the writer, delta/load rebuild, admission and carriage consistency share that index. A neutral outcome can retain a refunded closed guard while another owner's held deposit keeps the record, and that guard still counts. Objective settlement or claim retirement deletes records as before and restores the owner's budget. Duplicate held-deposit entries and pending reacquisitions without a known owner/session are refused on load.

For `B` distinct bond identities attributed in retained dispute records, there are at most `64 * B` owner/claim rows, at most one pending reacquisition per row and one held deposit per closed row. This replaces a fixed per-claim envelope with a bound scaling with registered owners; it does not provide a fixed global memory bound or a deadline/inclusion guarantee under unbounded arrivals. The block fold still clones a claim's record, so peak memory/time and carrier scheduling at the largest permitted population need measured acceptance evidence. No scarce slot is handed to another bond or replaced by a preferred filer. A verifier needs its own unspent owner budget and collateral headroom.

Op 206 appends `retainedRecords` and `retainedCap` to its observation. No rooted field, object encoding, fence activation or shipping pin changes. Attack fixtures cross 64 concurrent and 256 lifetime reservers, then open the newcomer’s own session despite an exhausted shared DA-8 counter. A small PanelDA FP fixture also fills these thresholds before public input acquisition, own-model replay and objective conviction/default. These are V-fold tests, not full fresh-node service or a proof of all-family throughput. Final results and logs are in the audit implementation record.

## Canonical FP input faults before replay (Codex, 2026-10-11)

A self-consistent bad canonical FP count/root previously stopped the outsider at input admission. Attempt-only `PromptNotAnchored` could not judge it. Contradiction 13 now appends `PalwPromptProofV1::FpCanonicalWhole { job }` at proof tag 2, behind the same never-armed public-filer fence as direct-proof precedence. Existing proof tags 0/1, object tags, state layout and activation remain unchanged. Both kind-3 and kind-4 adjudicators reject this proof below that fence, even with no open court. Legacy V1 readers still refuse contradiction 13.

The binding must verify against this claim's execution root and class. Its context pin must reproduce the claim's recorded FP job pin. The public job must hash to that context's job id, derive its seed and match class, executor, tokenizer, prompt count/hash and context ceiling. Only then does the checker derive the family's canonical count and anchor (`fp_canonical_anchor_v1`), including the raw job's anchor and nonce. The current extension admits ordinary V3/V4 canonical jobs, in PublicDA or PanelDA mode; prefix/evaluation/tensor jobs need their own rules.

A wrong count proves the fault without hashing the whole prompt. With the correct count, a wrong root is checked by a whole-root recompute. The processor and fold use the same fence-aware heavy-accounting helper: unauthenticated jobs and below-fence proofs consume no heavy slot; count faults consume no recompute slot; root checks use the existing 262,144-id budget and per-claim memo. Carrier rent conservatively includes the declared prompt count. The node reserves temporary ids/tree/proof buffers and hashes off the service loop. It builds whole proofs only within the existing budget; this is not permission to run arbitrary larger profiles.

Rent also reads the public-filer fence. The legacy pricing wrappers treat FP proof tag 2 as the old build's undecodable form and price it at zero. The processor's acceptance walk and UTXO coinbase burn share the new fence-aware pricing function at the block DAA; below the fence no new prompt rent is burned. This is necessary even for a proof the adjudicator refuses, because carrier rent affects the coinbase independently of conviction.

Once-per-claim accounting now has a synchronous thread-local block memo. The real chain processor holds one guard through acceptance, nested rehearsals and the final fold. Standalone acceptance/fold calls establish their own cold scope. Budgeted authenticated roots never evict; retained weight is at most 262,144 ids (at least one per entry). Overflow from an unbudgeted caller computes the pure answer without evicting existing roots. Other blocks, threads and unwinding restore or isolate scopes. The old eight-entry process FIFO remains a best-effort speedup outside block scopes and is not used to bound block work.

A five-carrier gate/acceptance/fold fixture pays the actual rent, has combined transient mass 357,140, and reaches kind-3/kind-4 conviction. Even with another thread evicting the process cache between acceptance and fold, one joint scope computes exactly one root (262,143 ids, 11 cache hits). This is V-fold evidence using synthetic commitments and fixture class admission. Individually weighed carriers reuse a fixture funding outpoint, so this is not proof of five valid mined spends or full-service inclusion. The initial many-claim attack fixture exceeded the mass cap and was rejected. Final tests and raw logs are in the audit implementation record.

The node's funded lifecycle signer now pays fence-aware protocol rent plus the actual compute-mass relay fee: rent is burned and carriage remains for the miner. Its wrapper reads current virtual DAA; rent uses the same offence, batch and public-filer rules as acceptance/coinbase, and filters inactive owning kinds. The priority queue and common carrier request an object-specific funding minimum (rent + the largest standard relay fee + positive change, at least the old float floor). With no carrier in flight they retry the existing mature/unlocked/unspent UTXO scan rather than repeatedly offering a relay-sized residue. Failed scans preserve a smaller float for cheap moves and are not repeated for the same or a higher minimum within that priority pass. Stuck-carrier replacement keeps its own input. Actual price/locked outputs are checked before signing. New V-unit tests authenticate real ML-DSA spends and measure fee/change for kind-3/kind-4, old Whole/FP and batch-fence pricing; they do not authenticate a model execution or prove actual mempool/mined inclusion. Full fresh-node service remains a C2/C7/C8 acceptance condition.

The node retains an authenticated ordinary canonical job in a Pending bootstrap case under a small input ticket, before deriving the backend ceiling context or requesting full replay memory. Full-replay memory pressure therefore does not prevent binding acquisition and an input proof. Canonical Pending cases remain eligible to retry honest replay when memory becomes available; they need no served input ids. After binding acquisition the node attempts the direct kind-4 reporter proof and waits without demanding producer prompt ids. Nonresponse uses the existing DA default, not an arithmetic conviction. A correct input twin yields no input proof. The test's synthetic step/checkpoint roots isolate this input rule; that twin does not establish honesty of the entire execution. Validation and remaining G14 conditions are recorded in the audit implementation log.
