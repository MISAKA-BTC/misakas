# ADR-0029: PALW chain carriage — the objects ride the rails the fork already built

Status: **Proposed (draft for review).** Activates nothing. This ADR decides how ADR-0028's
objects — job commitments, attestations, opening calls and answers, refutations — become
on-chain facts, in two stages whose first requires **zero node changes** and realizes the
Stage-0 telemetry (`P_check`, no-show, inclusion latency) as measurements of real chain events.
Date: 2026-08-16
Relates to: ADR-0028 (the scheduling fabric this carries; its Consequences named this gap),
ADR-0027 (evidence objects and DA rules), ADR-0026 §12 (the gate artifacts Stage 0 fills),
ADR-0009/0016/0017 (the DNS overlay carriage discipline this reuses),
`consensus/src/processes/transaction_validator/tx_validation_in_isolation.rs`
(`check_transaction_subnetwork` — the admission gate), `consensus/core/src/dns_finality.rs`
(`dns_tx_kind`, the stateless validators), `consensus/src/pipeline/virtual_processor/processor.rs`
(the capability store's accept/revert/backfill walk — the Stage-1 template),
`consensus/core/src/palw_legs.rs` / `palw_slash.rs` / `palw_v2.rs` (the wire bodies, all frozen).

## Facts this design stands on (verified in code, 2026-08-16)

1. **The fork has an established carriage discipline, live on t10 today**: one dedicated
   20-byte `SUBNETWORK_ID_*` per payload kind, a Borsh body in `tx.payload`, **stateless**
   validation at admission (`dns_tx_kind` → per-kind validator → `InvalidDnsOverlayPayload`),
   **stateful** checks in the virtual processor's credit walk, and a store with
   accept/revert-on-reorg plus a horizon-bounded history backfill
   (`compute_capabilities_with_ids_from_accepted_txs` + `verified_capability`). Eleven kinds
   already ride this way, including the v1 compute lifecycle.
2. **An unknown subnetwork id is rejected at admission** (`SubnetworksDisabled`). New ids are
   a coordinated release — the token band's comment states the pattern explicitly. Therefore
   new-id carriage cannot reach today's deployed fleet.
3. **A native-subnetwork transaction may carry a payload today** — asserted by the isolation
   test (`tx.payload = vec![0]` → `Ok`), with **no payload inspection at admission and no
   payload rule in mempool standardness**. Mass is the only bound.
4. **The budgets**: `mass_per_tx_byte = 1`, `max_block_mass = 500_000`, standard-tx mass cap
   `480_000` (`MAXIMUM_STANDARD_TRANSACTION_MASS`, this fork's value). ML-DSA-87 signature
   = 4 627 B.
5. **The observation surface exists**: virtual-chain-changed RPC with
   `include_accepted_transaction_ids` — added and removed chain blocks with their accepted
   transactions. A watcher can maintain a selected-chain view of carried objects without any
   node change.
6. **One evidence object does not fit**: a bare-v2 logits-event refutation carries a full
   vocab row — `248 320 × 4 B ≈ 0.99 MiB` (the number `palw_slash` itself documents) —
   exceeding both the standard cap and the block mass. §6 refuses to hand-wave this.

## Premises

P1–P3 and the ADR-0028 corollary are inherited. Two carriage-specific rules join them:

* **A payload a deployed node rejects at admission is not carriage, it is a fork.** Never
  retrofit new versions or shapes into an already-deployed subnetwork's stateless validator —
  a deployed validator that rejects the new version keeps the tx out of blocks entirely. New
  shapes get new ids, shipped together with their validators (the version trap, named).
* **Offense-grade facts require offense-grade carriage.** An objective offense (no-show,
  `W_answer` silence) may only be grounded in objects every consensus-running node indexed
  identically — i.e. Stage-1 subnetwork carriage. Stage-0 native carriage produces
  *measurements*, never offense evidence against third parties.

## Decision

### 1. Two stages, one body format

| | Stage 0 (now, zero release) | Stage 1 (coordinated release) |
| --- | --- | --- |
| Vehicle | native subnetwork, payload = `"MPALW2" ‖ kind u8 ‖ borsh body` | one new `SUBNETWORK_ID_PALW_*` per kind |
| Admission | none (native payloads are opaque to consensus) | stateless per-kind validator, `InvalidDnsOverlayPayload` on failure |
| Indexing | external watcher over the RPC acceptance stream | in-node store: accept/revert/backfill, the capability-store walk verbatim |
| Grounds | telemetry only (§12 artifacts) | duties, deadlines, objective offenses; later the §1 credit gate |
| Who can act on it | the fleet drill | every consensus-running node |

**The Borsh bodies are identical in both stages.** Migration is a change of address, not of
format: the Stage-0 magic envelope is dropped and the body moves onto its subnetwork id.
Bodies never embed their carriage (no subnetwork id, no magic inside the signed material), so
a Stage-0 object and its Stage-1 twin hash and verify identically.

### 2. The five kinds and their bodies

Every body reuses a frozen wire object; carriage adds only identity and binding — never a
second copy of anything the inner object already commits to (the dual-source rule).

```
kind 0x01  PalwCommitmentCarriageV1 {
             version u16,
             envelope: PalwJobEnvelopeV2,          // full input: replays are self-contained,
                                                   // so ADR-0028 §3's input-DA objection is
                                                   // unreachable for carried jobs
             committed_form: u8,                   // 0 = bare v2 root, 1 = execution composite
             committed_root: Hash64,
             binding: Option<PalwLegsBindingV1>,   // required iff composite: the transparent
                                                   // preimage refuters open against
             validator_id: Hash64, bond_outpoint: TransactionOutpoint,
             signature: Vec<u8>,                   // ML-DSA-87 over a carriage-domain digest
           }
kind 0x02  PalwAttestationCarriageV1 {
             version u16,
             commitment_root: Hash64,              // what §1's credit gate matches against
             attestation: PalwExecutionAttestationV1,  // palw_slash, unchanged — its message
                                                   // golden 9fb7e41e… stays the signed digest
             attester_id: Hash64, bond_outpoint: TransactionOutpoint,
           }
kind 0x03  PalwOpeningCallCarriageV1   { version u16, call: PalwLegsOpeningCallV1 }
kind 0x04  PalwOpeningAnswerCarriageV1 { version u16, call_tx_id: TransactionId,
                                         answer: PalwLegsOpeningAnswerV1 }
kind 0x05  PalwRefutationCarriageV1    { version u16, evidence: enum {
                                           Legs(PalwLegsRefutationV1),
                                           Summary(PalwTraceSummaryRefutationV1),
                                           Event(chunked — see §6),
                                         } }
```

Rules carried over from the fork's own precedents:

* **Evidence carriers declare no outputs** (kinds 0x05, and 0x03 if a fee-bond ever pays out
  at `(tx_id, 0)`) — the slashing/challenge/precommit-evidence rule, adopted so the Stage-2
  reporter-reward slot is never a retrofit.
* **Dedup**: store rows are keyed by carrying `tx_id` (revert-friendly, the capability-store
  key); logical identity is first-accepted-wins — `committed_root` for commitments,
  `(commitment_root, attester_id)` for attestations, `call_tx_id` for answers. `commit_daa`,
  `attest_daa`, call and answer times are the acceptance DAA of the first accepted carrier on
  the selected chain — the same clock `PalwShadowLedgerV1` already consumes.
* **Reorg**: Stage 1 inherits the accept/revert walk; Stage 0's watcher mirrors it in
  userspace from the RPC stream's removed/added chain blocks. ADR-0028 §2's re-anchor rule
  applies unchanged.

### 3. Mass budget — every kind sized against the real constants

At `mass_per_tx_byte = 1` against the 480 000 standard cap (block cap 500 000):

| object | size (est.) | fits? |
| --- | --- | --- |
| commitment, bare form | ≈ 7.8 KB (envelope ≈ 2.9 K + identity + 4 627 B sig) | ✓ |
| commitment, composite form | ≈ 9.1 KB (+ binding ≈ 1.3 K) | ✓ |
| attestation | ≈ 5.0 KB | ✓ |
| opening call (≤ 16 coordinates) | ≈ 3.3 KB | ✓ |
| opening answer, 16 activation openings | ≈ 152 KB (16 × ≈ 9.4 K + binding) | ✓ |
| legs refutation (one activation leaf) | ≈ 15 KB | ✓ |
| summary/schedule refutation | ≈ 2–3 KB | ✓ |
| **bare-v2 logits-event refutation** | **≈ 0.99 MB** | **✗ — twice the block mass** |

Consequently the **carriage cap for openings is 16 per call** (wire cap stays 64;
`PALW_LEGS_MAX_REQUESTED_OPENINGS` is unchanged — a request wanting more splits into several
calls, each with its own `W_answer`). The cap buys margin, not bare feasibility: 16 × ≈ 9.4 KB
≈ 152 KB is 3.2× under the standard cap, while the wire-cap 64 (~600 KB) exceeds it outright
and even 48 (~451 KB) would ride with no room for anything else in the transaction or the
block it shares.

### 4. Time semantics

An object's protocol time is **the acceptance DAA score of its first accepted carrier on the
selected chain** — observable at Stage 0 through the RPC acceptance stream, stored at Stage 1
exactly as `accepted_daa_score` already is for capabilities. Mempool time, broadcast time and
block timestamps are not protocol facts. `W_answer(call)` runs from the call carrier's
acceptance to the answer carrier's acceptance; inclusion latency — the ADR-0028 assumption-2
telemetry — is `acceptance_daa − first_broadcast` as measured by the submitting watcher (the
broadcast half is watcher-local by nature and is labeled as such in artifacts).

### 5. Stage 0 realization — the drill that fills the §12 artifacts

No node changes. Three pieces, all external:

```
submitter  (fleet hosts)   every N minutes: run a v2 job (bench path), wrap the result as
                           kind 0x01, submit via RPC as a native-subnetwork tx with the magic
                           payload; panels derived per select_replay_panel_v1 over the four
                           bonded validators; assigned hosts replay and submit kind 0x02
watcher    (host B)        subscribes virtual-chain-changed with accepted tx ids; maintains
                           the selected-chain object view (apply added / roll back removed);
                           classifies duties against job_schedule_v1; feeds
                           PalwShadowLedgerV1::observe_job
reporter   (host B)        publishes the ledger report per class — jobs, creditable, on-time
                           matches, no-shows, mismatches, attest/refutation inclusion
                           latency, replay p99 — THE §12 artifacts, now from chain events
```

Drill discipline: induced negatives are part of the schedule (a host stays silent on an
assigned duty once per session; one job is answered past `W_answer`) so the no-show and
late columns are exercised, not vacuously zero. A deliberately mismatched attestation may be
drilled ONLY against a job whose **envelope carries a dedicated drill `network_id`** — the
attestation itself carries no network id by design, but everything signed about a job is
bound through its `job_context_hash`, so the drill namespace scopes the contradiction. A
signed mismatch on the production namespace is `ClassContradictionCertificateV1` material
and must never be manufactured.

Fees: plain transaction fees from the fleet wallet. Fee-bonded calls (ADR-0028 §5's DoS
pricing) need escrow and are **deferred to Stage 1**; at Stage 0 the caller and answerer are
both ours, so the audit-call fee question does not arise.

### 6. The object that does not fit, and what that forces

A bare-v2 logits-event refutation cannot ride any single transaction on these parameters
(0.99 MB vs 0.5 MB block mass). The design refuses both easy outs — raising our own devnet's
mass caps (a dispute layer that only works on tuned parameters is self-deception) and
DA-by-reference for adjudication bytes (every node must recompute the check from the
transaction alone; fetching evidence at validation time is not consensus-safe). What remains:

1. **Legs-carrying classes are unaffected.** Their computational divergence localizes through
   activation rows (8 KB — fits with 30× margin), which is now an *additional, measured*
   argument for the composite commitment form beyond ADR-0026 §2's: **the legs are what make
   refutations carriageable.**
2. **Bare-v2 classes get chunked evidence carriage at Stage 1**: a 3-chunk reassembly
   envelope (`evidence_id ‖ chunk i/n ‖ bytes`, ≈ 331 KB per chunk, each standard-mass-legal;
   the refutation adjudicates only when all chunks of `evidence_id` are accepted; `W_round`
   applies to the last chunk). This is deliberately not designed further here — it is gated
   work, and the gate is stated as a rule:

> **No Stage 2 (slash-bearing) operation for a class whose registered commitment form is
> bare-v2 until chunked logits-evidence carriage has landed and been drilled.** Composite-form
> classes are not blocked by this.

### 7. What this ADR deliberately does not decide

* **Fee-bond escrow** for opening calls and the challenger bounty plumbing (ADR-0028 §4/§5) —
  needs the bond-UTXO covenant discipline; Stage-1 design.
* **The credit gate's consensus wiring** (Stage 2): this ADR carries the facts the gate will
  read; it does not implement the gate.
* **Subnetwork id values** — assigned at the Stage-1 coordinated release alongside the token
  band's, not reserved here.
* **DA-layer serving of checkpoint state bytes** (interval spot-checks, ADR-0028 §5) — rides
  the existing v0.1 §10 DA manifest rules when that layer is wired; nothing here blocks it.

## Assumptions that remain (stated so they can be attacked)

1. **Native-payload admission stays open.** Stage 0 rides an existing behavior; if a future
   release tightens native-payload rules, Stage 0 carriage moves to Stage 1 early. The
   watcher would detect this as submission rejections, not as silent data loss.
2. **The RPC acceptance stream is faithful** — same-node trust only: each watcher trusts the
   node it queries, which is the trust a validator already places in its own node.
3. **Standardness relay**: payload-bearing native txs relay through today's mempool (no
   payload rule exists to stop them); if relay policy changes, direct-to-miner submission on
   our own fleet still lands drill objects — degraded, and visible in inclusion latency.
4. **Watcher determinism**: two watchers over the same chain produce identical ledgers —
   guaranteed by construction (acceptance DAA + first-accepted-wins + the pure classification
   functions of `palw_schedule`), and drilled by running watchers on two hosts and diffing
   reports.

## Consequences

* **New Land module next** (consensus-inert, like everything before it): the magic envelope +
  the five carriage bodies + caps + encode/decode + golden tests in `consensus/core`
  (`palw_carriage.rs`), reusing the frozen inner objects untouched. Then the watcher/submitter
  binaries (out-of-node), then the drill runbook.
* **ADR-0028's Stage-0 telemetry becomes realizable**: `P_check`, no-show and inclusion
  latency stop being columns the ledger cannot fill. The §12 checklist items this feeds:
  `[ ] P_check measured in shadow`, `[ ] no-show/inclusion telemetry published`.
* **The composite commitment form gains a second justification** (§6): legs make refutations
  fit the chain. Registration guidance: new classes should register the composite form.
* **The version trap is now a stated rule** — the next person who wants to "just add a
  version" to a deployed validator has a sentence to collide with.
