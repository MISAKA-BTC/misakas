# G14 / Public Prosecution — integration matrix

Owner: Lead/Integrator. Baseline: `pre` @ `082636b64` (+ `bd120c08b`, `ca6759d18` docs). Branch: `claude/g14-public-prosecution-integration-9bee39`.

**Property (G14).** The producer and every fixed Panel seat collude. One ordinary bonded verifier outside the Panel — with no
producer-private state, Panel-private state, `ServedView`, seat capture or privileged endpoint — independently checks, localizes
and obtains authenticated witnesses from canonical public authenticated material, and reaches an objective conviction the
consensus accepts deterministically, or, for withheld material, a correctly classified DA/default outcome, for every
computation / job / input / output / state / DA violation an Active `VerificationPlanV1` covers.

Statuses: **PASS** = implemented and exercised by a named test at that level; **GAP** = repository-implementable and not done;
**EXTERNAL_GATE** = needs something outside the repository (review, hardware, drill, soak, activation). Unknown is GAP, never PASS.

Two levels are kept apart:

* **Reference** — `misaka-palw-kernel` (`KernelLedgerV1`, an in-process deterministic fold; outsiders replay from genesis).
* **Real node** — the same rules as `PalwConsensusObjectV2` objects on subnetwork `0x4b`, folded into `PalwChainStateV2`
  (state root in the header, per-block deltas, pruning snapshot and IBD carriage), reached through RPC/mempool/template.

As of this revision **the kernel route is not wired into the node at all** (`palw_probabilistic_constraints_v1` is read by
nothing; `KernelLedgerV1` has no canonical codec, a `Debug`-string state root, bare-digest bonds and unsigned filings). Every
real-node cell is therefore GAP until lane D lands the carrier/fold, and the earlier audit's "repository-level complete" is
superseded by this matrix.

## 1. Violation families

| # | Family | Active Plan relation | Public material | Verifier entry | Localization | Exact terminal | Consensus outcome | Resource bound | Targeted test (reference) | Reference | Real node | External |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 1 | Arithmetic (MatMul, exact families) | `FreivaldsM127` / `FreivaldsCrtV2` / `ExactRecompute` | committed node values (DA or served), public artifact, plan | `OutsiderV1::check` (own salt) | one instance / one MatMul scalar | `MatMulScalar` (3 openings) / `InstanceRecompute` | `Convicted`, reservation slashed, accuser share | `max_opening_bytes`, `max_court_work` (gate) | `k2_e2e`, `k2_ledger` A | PASS | GAP | soundness review (EXTERNAL_GATE) |
| 2 | Quantization / rounding / carry / range | exact families + `RangeRuleV1` no-wrap proof | same | same | one instance | `InstanceRecompute` under reference semantics | `Convicted` | gate | `k2_e2e` (every exact family), `k2_wide` (i128, mod 2^127−1 aliasing) | PASS | GAP | alias/CRT review |
| 3 | Routing / TopK / expert | TopK / MoE combine exact relations | committed scores + indices | same | one instance | `InstanceRecompute` | `Convicted` | gate | `k2_adversarial` swapped expert | PASS | GAP | — |
| 4 | Memory / history | `StateContinuity`; derived `Hist` windows | committed appended rows; windows **derived, never served** | same | one window position | `Misderived` court from rows alone | `Convicted` | linear public bytes | `k2_ledger` history window, `k2_adversarial` permuted window | PASS | GAP | long-context bytes measurement |
| 5 | Checkpoint / state transition | segment entry/exit roots in evidence | evidence object | inclusion check (`claim_structure_v1`) | n/a (refused) | refused at inclusion | claim never committed | evidence bytes | `k2_e2e` fabricated boundary, `k2_ledger` C | PASS | GAP | — |
| 6 | Job binding | claim → posted job id | job object | inclusion | n/a | `binding_fault_v1` | refused at inclusion | — | `k2_ledger` C, `k2_ledger_pipeline` | PASS | GAP | — |
| 7 | Input / prompt binding | `job_input_root`, pipeline `job_root` | job facts | inclusion | n/a | `WrongInput` | refused | — | `k2_ledger` C (borrowed trace) | PASS | GAP | — |
| 8 | Output / token / logits | decode rule over committed logits | delivered ids + one logits tensor | `OutsiderV1::check` decode loop | one delivered index | `verify_decode_fault_v1` | `Convicted` | `max_response_bytes` | `k2_ledger` C (last + mid-stream) | PASS | GAP | — |
| 9 | Pipeline stage | per-stage plan | stage commitments | `FreshPipelineVerifierV1` | stage, position, node | stage court | `Convicted` | per-stage sums | `k2_pipeline`, `k2_ledger_pipeline` | PASS | GAP | — |
| 10 | Pipeline edge | `EdgeRecompute` | upstream outputs + stage input | same | one edge | `verify_edge_fault_v1` | `Convicted` | edge filing bytes | `k2_ledger_pipeline` conditioning edge | PASS | GAP | — |
| 11 | VLM / media derived input | `R = dist(R(seed,…))`, image edge | job images, public seed | same | one draw / edge | edge / `R` recompute | `Convicted`; other seed refused | — | `k2_ledger_pipeline` jitter, other seed, image edge | PASS | GAP | — |
| 12 | Self-consistent garbage trace | every relation vs registered artifact | public weights | `check_salted` | first product | court | `Convicted` | — | `k2_ledger` B | PASS | GAP | — |
| 13 | Borrowed valid trace (another job) | job/input roots | job | inclusion | n/a | `WrongInput` | refused; lender untouched | — | `k2_ledger` C | PASS | GAP | — |
| 14 | Malformed Merkle / opening | response classification | response bytes | `classify_position_response_v1` | n/a | `malformed`/`wrong_bytes`/`wrong_root`/`fake_opening`/`partial`/`oversized` | response rejected; dismissed filing pays fee | `max_response_bytes`, `max_filing_bytes` | `k2_ledger` B, G | PASS | GAP | — |
| 15 | Held / fused terminal | profile material must be public | — | gate | — | `PrivateMaterial` gap | class never registers / never rewards | — | `k2_public` fused gap | PASS (fail-closed) | GAP | 8k held real-hardware (RFC-0014 P1/P2) |
| 16 | Public DA withholding | demand per stage position | on-chain `Respond` | `FileDemand` (one round, all positions) | position | served → court; silence → default | `ProducerDefault` (availability, not fraud), claim `Unavailable` | `max_concurrent_sessions` | `k2_ledger` D, G | PASS | GAP (no public DA fetch RPC either) | DA provider/transport drill |
| 17 | Court pre-emption | direct proofs | — | `FileProof` | — | adjudicated in-block whatever is open | open demands moot, bonds refunded | — | `k2_ledger` E | PASS | GAP | — |
| 18 | Challenge / Final race | window + court deadline | — | `FileDemand` / `FileProof` | — | — | Final ≤ window end + deadline; proof at window end blocks Final; post-Final liability | `liability_daa > court_deadline_daa + proof_grace_daa` | `k2_ledger` F, `k2_ledger_route` | PASS (R1 fixed `9e0cce365`) | GAP | inclusion/censorship drill |
| 19 | Reorg / restart / IBD / duplicate proof | pure fold | block sequence | replay | — | `Duplicate` | convicted once; replay root equality | — | `k2_ledger` H, every `outsider()` | PASS (R2 canonical root `c6a7b5a48`, golden vector) | GAP | — |
| 20 | Collateral / exit / double reservation | free collateral reservation | bonds | — | — | — | no double use; exit delay; withdraw with nothing reserved | — | `k2_ledger` H | PASS reference (R3: consumer-synced bonds + settlement instructions `c6a7b5a48`) | GAP (map onto V2 exposure ledger) | economics review |

EXEC work slices (directive Agent 4): **GAP** at every level — no RFC-0008 v2 code on HEAD; the v0 algo-11 branch
`rfc8/claim-backed-blocks` must not be reused. Ordered after the real-node carrier/fold (priority 6 of 8).

## 2. Reference-level gaps found in this revision

Status 2026-10-08 (lane B, integrated `9e0cce365..0211b2cad`, 109 kernel tests green): **R1–R5 and the default design note are fixed at reference level.** Lane B also found and fixed a pre-existing bug: `RegisterClass` overwrote an existing class row, letting anyone re-register a class id under another network/ruleset and break every court reading it. API frozen (additive changes only): `KernelRouteObjectV1` v1 tags 1–11, `AuthV1`, `KernelRefusalV1`, `SettlementInstructionV1` kinds 1–12, `sync_bond` / `attest_artifact` / `apply_panel_tally`, `begin_block` / `apply_object` / `tick`. Open for the real node: artifact availability is a consumer attestation (`attest_artifact`); `CommitClaim` carries O(positions × nodes × 64 B) inline (chunked carriage/pruning needed for real classes); the per-block adjudication budget's inclusion order is the block producer's choice (bounded by fee × budget).


* **GAP-R1 (Final race after service).** A demand served in a block whose tick also closes the window lets the claim finalize
  in that same block (and the producer's reward is credited) before the demander can file the proof the served values enable.
  Post-Final liability still convicts, but an accepted, qualified prosecution must not lose to Final. Fix: a bounded proof grace
  after each service (`Final ≥ last service + proof_grace`), fixed absolute bound `window end + court deadline + proof grace`,
  `liability_daa > court_deadline_daa + proof_grace`. Owner: lane B.
* **GAP-R2 (state root).** `KernelLedgerV1::root` hashes `format!("{self:?}")`: not a canonical consensus encoding. Fix: Borsh on
  every row, per-collection roots, a versioned root. Owner: lane B.
* **GAP-R3 (bonds and authorization).** Bonds are bare digests with self-declared collateral; filings, demands and responses are
  unsigned; `PanelCovered` is a transaction anyone may submit; `credits` have no mint path. In the node these must map onto
  `PalwBondKeyV2` collateral, ML-DSA-87 signatures at acceptance, the V2 claim phase and `pending_payouts`. Owner: lane B (API
  shape) + lane D (consensus mapping), Lead reviews the mapping.
* **GAP-R4 (per-object API and budget).** `apply_block` logs refusals into state (junk input grows state) and adjudicates without
  a per-block budget. Fix: `apply_object -> Result<events, refusal>`, a separate `tick`, a per-block court budget. Owner: lane B.
* **GAP-R5 (claim beacon).** `beacon.rs` derives claim challenges from selected-chain block hashes and the ledger's beacon is
  `H(claim id, daa)`. Outsiders never depend on it (own salt), but CLAIM_VERIFICATION checks must come from
  `misaka-palw-challenge` (PALW Work Beacon). Owner: lane B on top of the Lead contract.
* **Design note (default vs. conviction economics).** A post-Final demand that defaults costs the producer only
  `default_penalty`, while conviction takes the reservation. Pre-Final this is harmless (no reward), post-Final a fraudulent
  producer keeps the reward if `claim_reward > default_penalty`. Track under lane B; resolve with lane D's real collateral.

## 2a. Lane B final (integrated through `b68676bdc`, 113 kernel tests green)

* **Family review PASS (reference):** a lie at every node of the reference classes (all 25 TIR v1 primitives, a new fixture for the
  five no fixture had) and at every edge of five reference pipelines (all 8 binding kinds) is localized to that node/edge and
  convicted by the public court (`k2_family_review`).
* **Carrier fit is part of G14 on the real node.** A class the gate calls complete can still be unprosecutable if its declared worst
  opening/filing/response/commitment exceeds what a carrier can hold (the toy fixture declares ≈33 MB per opening and ≈193 MB per
  position response). Lane D must refuse kernel-class registration unless `carrier_fit_v1(bounds, filing_cap, response_cap,
  commit_cap)` passes with the node's real caps (chunked carriage counted). No policy/root change.
* **Bond mapping constraints (lane D):** sync at block start after the previous block's settlements; synced collateral = real
  collateral − every non-kernel reservation (kernel reservations stay inside it); keep `kernel reserved ≤ synced collateral`;
  kernel `Withdraw` = the route forgets the bond (not a V2 exit); sync only bonds that sign kernel objects or carry kernel exposure.
* Still open: ledger restore from row bytes (snapshot/IBD carriage), incremental per-collection hashing, chunked `CommitClaim`
  carriage + pruning at liability end, artifact shape/dtype vs program (onboarding conformance before Active).

## 3. Real-node integration plan (lane D, Lead owns the shared parts)

1. **Lead (shared):** `PalwConsensusObjectV2` variants with declared tags ≥ 110 for kernel-route objects, may-ride entries,
   `palw_object_kind_name`, a Some-only root block and a carriage tail, delta entries with apply/revert. Fence: the existing
   `palw_probabilistic_constraints_v1`, still refused on every preset.
2. **Lane D:** acceptance arms (signatures, fee, per-block caps), fold arms calling lane B's per-object API, the per-block tick,
   bonds mapped to `PalwBondKeyV2` with `reserve`/`slash_bond`, RPC reads (claim record, served positions, demands, verdicts) and
   a public material read, then the T12Chain-based E2E: RPC → public material → fresh verifier → signed demand/proof → mempool →
   template → fold → conviction/default → slash/void → Final blocked → restart/IBD/reorg equality.
3. **Lane D adversarial:** cases A–X of the directive, each from a fresh verifier process/state.

## 4. External gates (EXTERNAL_GATE_PENDING — not stop reasons)

Independent soundness review (composition, alias bounds, CRT); beacon bias/withholding/grinding review and k/D/delay selection
(RFC-0007 §VI.8); real-hardware 9B-8k / long-context / Kimi measurements; public testnet G14 drill; shadow period; audit/soak;
activation height.

## 5. Change log

* 2026-10-08 — C4 round 1 on the kernel ledger, fixed by the Lead (116 → 117 kernel tests green):
  * F-C4-03 (HIGH) copied claim paid twice → one claim per job (`job_claims`, `fffaf74ec`); F-C4-09 job squatting by
    shape-correct junk → a claim holds its job from the Panel's coverage, not its commit (`592ab3382`).
  * GAP-R6 front-running by a mempool copyist → seal-then-reveal (`SealClaim`, tag 12; reveal needs a seal ≥ `claim_seal_delay_daa`
    old; seals expire after `seal_ttl_daa`) (`3d27e026d`).
  * F-C4-02 (MEDIUM-HIGH) post-Final Sybil demand default erased liability and paid the colluders → post-Final forfeit burned whole;
    a Final claim inside its horizon is adjudicated even with nothing reserved (`fffaf74ec`).
  * F-C4-04 slash above synced collateral → clamped, unslashed rest released explicitly (`fffaf74ec`).
  * Recorded, not kernel bugs: R-C4-10 a bond posting its own job with an identical prompt and answering with a published trace is
    paid (consumer economics: job price ≥ reward — DESIGN_GAP for lane D/economics); O-C4-09 a post-Final conviction frees the job
    and the first reward is not clawed back (collateral > reward by policy).

* 2026-10-08 — **P0 found by C4 (independent fuzz) and fixed (`098ffc749`)**: a one-bond stranger could panic the kernel-route
  ledger (release overflow-checks) with a tiny FileProof (Decode/Kernel) or Respond whose tensor/opening shape overflows the element
  count (`[u64::MAX, 2]`, or `[0, 2^40, 2^40]` via LayoutV1 m·n). Now malformed, never a panic (checked counts in misaka-palw-tir
  `Tensor::new`/`from_le_bytes`, `LayoutV1::try_of`). Live t12 TIR courts only build tensors from admitted program shapes —
  unaffected; behaviour for every non-overflowing input unchanged. Rule for lane D: no kernel/TIR call on attacker bytes outside a
  Result path inside the consensus fold.

* 2026-10-08 — lane B R1–R5 integrated; RFC-0010 circularity rule in the contract (`fcde1e3dc`); tag registry in `remaining-rfc-integration-matrix.md` §2 (kernel route: tags 110–119, deltas 160–169, tail 0xEC).

* 2026-10-08 — matrix created; shared challenge contract `misaka-palw-challenge` landed (policy, PALW Work Beacon, seed,
  samplers, transcripts, lifecycle, conformance records; dormant).
