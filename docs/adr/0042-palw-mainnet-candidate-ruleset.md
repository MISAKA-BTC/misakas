# ADR-0042: The PALW mainnet-candidate ruleset — one atomic activation, one fork choice, one fingerprint

Status: **Proposed.** Activates nothing, moves no fence, changes no shipped preset. It is the
engineering spec the `palw-v2` branch implements before any public PALW-RC or mainnet activation.

Date: 2026-08-20
Branch: `palw-v2` (cut from `palw-only-v4` @ `26469061`, which carries the external NO-GO audit).
Audit baseline: `9cfcbf99` / `docs/palw-critical-audit-2026-08-19-ja.md` (10 P0s + 2 blockers).

Relates to / builds on:

- **ADR-0035** (public PALW testnet = testnet-11, continued; class pin at the door) — TN11 stays
  what it is. This ADR does **not** re-genesis it.
- **ADR-0036** (mainnet activation model; new network identity required; hash floor SUPERSEDED;
  120 s cadence; params are soak outputs) — this ADR is the "fill-in the ruleset" spec ADR-0036
  §"What this ADR does not decide" promises, minus the measured parameters.
- **ADR-0038** (PALW is the consensus work; invariants W1–W8; Decisions A–H) — this ADR keeps every
  invariant and turns the audit's 12 activation conditions into a single ruleset.
- **ADR-0039** (`PALW-BASE-0` instead of a hash floor; two-weight fork choice; ticket-not-hash;
  per-class DAA; bonded permissionless) — carried in full; §3d determinism obligations are load-bearing.
- **ADR-0040** (`PALW-BASE-0` integer arithmetic), **ADR-0041** (pruning-proof verification).

Supersedes: **nothing's decisions.** It replaces one *mechanism* — the five independent activation
fences — with one atomic bundle, and it names the state, admission, court and fork-choice shapes
those fences would have switched on piecemeal.

---

## Context — why an RC, and not five `Some`s

The shipped presets set `palw_credit`, `palw_fork_choice`, `palw_schedule`, `palw_ramp` and
`palw_block_commitment` to `None`, and that is the whole of today's dormancy. The field docs say so
in as many words: dormancy is "a fact about the presets, not a structural one"
(`config/params.rs`), and `palw_fork_choice`'s own doc records that setting it "partitions the
network without altering fork choice." Five booleans, individually flippable, is a machine that
*invites* being half-activated. The external NO-GO audit (2026-08-19) found ten P0s that become live
"if you flip the fences on the current code" — block forgery from one PoW solution, bond
impersonation, a fork choice that never engages, node-to-node divergence on one DAG, an
unadjudicable court, and unbounded immature work per bond.

Three of those P0s (P0-3, P0-6, P0-7) were written the same day they were audited, and a fourth
(P0-2) was being fixed in the worktree while the audit was recorded. That discovery rate is the
argument against incremental fence-flipping: **the risk is not any single fence, it is that a human
flips them in the wrong order, or one at a time, and ships a chain that forks.** The fix is
structural, not procedural.

So the RC is not "TN11 with fences on." It is a separate network, brought up from a new genesis, on
a ruleset that is byte-identical to the one mainnet will ship — the only differences permitted are
network identity, genesis/allocation, address prefix, ports/seeds, and faucet. Everything that
decides consensus is the same, and that sameness is *checkable by machine* (Decision 11).

### The two-network split this ADR assumes

| Network | Role | Same ruleset as mainnet? |
|---|---|---|
| **testnet-11** | Legacy algo-4 PoW soak (ADR-0035); runtime determinism + operator drills | No — `PalwConsensusMode::LegacyTn11` |
| **PALW Devnet** | Short windows, forced fraud, fast state-machine/court iteration | No |
| **PALW Chaosnet** | Multi-host, partitions, fault injection; private | Nearly, but not public |
| **PALW-RC(n)** | Public mainnet candidate | **Yes** |
| **Mainnet** | Ships the last RC's canonical ruleset bytes | **Yes** |

A public RC that changes any consensus rule is **re-genesised as RC(n+1)**, never continued. A
testnet forked repeatedly is not "the same as mainnet," whatever its age.

---

## Decision 1 — One atomic activation bundle, not five fences

The five `Option` fences are replaced, on the V2 lineage, by a single mode enum. A network is in
exactly one PALW mode, and `ConsensusV2` carries *all* of the ruleset or none of it.

```rust
pub enum PalwConsensusMode {
    Disabled,                              // hash networks; PALW machinery inert
    LegacyTn11(LegacyPalwParams),          // ADR-0035 algo-4 soak, unchanged
    ConsensusV2(PalwConsensusParamsV2),    // the RC / mainnet ruleset
}

pub struct PalwConsensusParamsV2 {
    pub protocol_version: u16,
    pub algorithm_id: u8,                  // POW_ALGO_ID_PALW_COMMITTED_V2 (Decision 3)
    pub cadence: PalwCadenceParams,        // frozen 120 s (ADR-0038 H)
    pub class_catalog_root: Hash,          // registered classes incl. PALW-BASE-0
    pub court_catalog_root: Hash,          // adjudicable primitive set
    pub bond: PalwBondParams,
    pub exposure: PalwExposureParams,      // Decision 6
    pub panel: PalwPanelParams,            // Decision 7
    pub windows: PalwWindowParams,         // bind / receipt / challenge / prosecution
    pub fork_choice: PalwForkChoiceParams, // Decision 9
    pub class_daa: PalwClassDaaParams,
    pub rewards: PalwRewardParams,         // Decision 10
}
```

`ConsensusV2` is validated **once, at construction**, and a node refuses to start unless every
invariant below holds. There is no path that switches on "fork choice" without "commitment," or
"schedule" without "credit," because there are no longer separate switches.

Startup invariants (all must hold, or the node does not boot):

```
BASE-0 exists in the class catalog and holds non-zero target share
BASE-0 court coverage == 100% (every reachable primitive is adjudicable)
window_court  >=  worst-case court duration ((ceil(log2(max_step_leaves)) + terminal) × turn deadline)   [amended A1]
bond withdrawal delay  >  bind + receipt + challenge + prosecution + reorg margin   (Decision 6)
the class share table SATISFIES the exposure/starvation inequality (ADR-0039 D5e: no StarvedClass)
a live exposure ceiling is set                                                       (Decision 6)
reward maturity  >  the full liability period                                        (Decision 10)
the safe/live fork-choice comparator is present and total                            (Decision 9)
protocol_version, algorithm_id, and both catalog roots are the ones the fingerprint commits to
```

This is the plan's central move: **PALW V2 can be enabled only as one bundle whose invariants a
node checks before it dials a peer.** ADR-0039 D5e already refuses a starved share table at startup;
this generalizes that gate to the whole ruleset.

---

## Decision 2 — The block state machine, and why a fresh tip is weighable (closes P0-3)

Every PALW block moves through one lattice. Weight is a function of the state, and the state only
advances.

```
Provisional ──future anchor──▶ PanelBound ──quorum receipt──▶ ReceiptLicensed
                                                                   │        │
                                                       no dispute  │        │ fraud / withholding / timeout
                                                                   ▼        ▼
                                                                 Final    Voided ──▶ Slash
```

| State | live weight | safe weight |
|---|---|---|
| Provisional | bounded immature pwu (β·pwu, β ≤ 1000‰) | 0 |
| PanelBound | bounded immature pwu | 0 |
| ReceiptLicensed | bounded immature pwu | 0 |
| Final | full pwu | full pwu |
| Voided | 0 | 0 |

The audit's P0-3 is a structural cycle: the old weigher required a *future* panel anchor
(`accepted_daa + delta_bind`) to weigh a block, but a fresh tip has no block after it, so its anchor
is always `None`, so every live tip is `UnresolvedBlock`, so PALW weight is never consulted and fork
choice silently falls back to blue work. **The fix is that a fresh commitment is weighable
immediately as `Provisional`, deterministically, with no panel.** The panel is derived later, from
the anchor once it exists, and it gates only the `PanelBound → ReceiptLicensed → Final` transitions.

This is consistent with ADR-0039 §3e/§Consequences: a block's classification, once it reaches
`Final`/`Voided`, is permanent and unique; below that the stage may move freely, and
`advancing_a_block's_stage_never_lowers_either_weight` (β ≤ 1000‰ makes the `live→safe` jump
non-decreasing). ADR-0042 adds only the near end of the ladder — that a panel-less block is
`Provisional`, not `Unresolved`.

---

## Decision 3 — `PalwAttemptEnvelopeV2`, a new algo id, and identity by `attempt_id` (closes P0-1)

### 3a. The commitment binds to the PoW, or the PoW is free

P0-1: the live PoW path (`calculate_pow_layer0` over `pre_pow_hash, timestamp, nonce, network_id`)
does not consume the commitment root, while `PalwBlockCommitmentV1::l1_tag_bytes` — which *would*
bind it — is never called on the live path. So one PoW solution mints unlimited distinct block
identities by swapping the commitment. V2 makes the binding mandatory and total:

```
challenge        = H(network_domain ‖ pre_pow_hash ‖ timestamp ‖ nonce ‖ class_id ‖ bond_outpoint)
commitment_root  = H(challenge ‖ class_id ‖ bond_outpoint ‖ trace_root ‖ output_root ‖ pwu)
PoW L1 tag       = Expand(commitment_root)              // the finalizer CONSUMES this, not (pph,ts,nonce)
```

This is ADR-0039 Decision 4 made non-optional on the live path: one new ticket costs one new
inference (W2), and a commitment cannot be replayed onto another attempt, header, class or executor.
A consensus test fixes that **mutating any one bit of the commitment fails the PoW** (the exact
property `l1_tag_bytes`'s own unit tests assert, now enforced where blocks are validated).

### 3b. The unsigned attempt and its envelope

```rust
pub struct PalwAttemptUnsignedV2 {
    pub version: u16,
    pub network_domain: Hash,
    pub challenge: Hash,
    pub class_id: PalwClassId,
    pub executor_bond: TransactionOutpoint,
    pub executor_pubkey: MlDsa87PublicKey,   // MUST equal the bond record's key (Decision 6)
    pub operator_id: Hash,                    // registered at bond time; panel dedup (Decision 7)
    pub artifact_root: Hash,                  // MUST equal the class's registered root
    pub trace_root: Hash,
    pub output_root: Hash,
    pub pwu: u64,
}
pub struct PalwAttemptEnvelopeV2 {
    pub attempt: PalwAttemptUnsignedV2,
    pub signature: MlDsa87Signature,
}
```

### 3c. Block identity is `attempt_id`, not the raw signature — **deferred with cause (Amendments §A2)**

`block_id` incorporates `attempt_id = H(canonical(PalwAttemptUnsignedV2))`, **not** the signature
bytes. ML-DSA-87 signatures are not guaranteed unique, so folding raw signature bytes into the
identity would re-open malleability under the disguise of a fix: a second valid signature over the
same message would yield a second block id. The signature is verified as a *witness* at admission;
identity is `canonical header ‖ attempt_id`.

> **Amended at implementation (2026-08-20):** as written, this clause hands a third party a
> zero-cost censorship primitive and MUST NOT land naively — flip one signature bit and the block
> keeps its id but fails admission, so the first-seen mutated copy poisons the honest block's id
> in every known-invalid cache. The tree keeps raw-carrier-bytes identity (a mutated copy is a
> DIFFERENT id that dies alone; only the key holder can mint valid-signature siblings, and those
> deduplicate at the claim, `claim_id = attempt_id`). 3c lands only together with a
> mutated-witness path that rejects without caching id-invalidity. See §A2 for the full analysis.

### 3d. A new algo id, so no old node re-interprets a V2 block

V2 uses `POW_ALGO_ID_PALW_COMMITTED_V2`, distinct from `POW_ALGO_ID_PALW_LLM` (4) and
`POW_ALGO_ID_PALW_OLLAMA` (5). A pre-V2 node cannot mistake a V2 block for legacy algo-4. Because the
RC is a new genesis, no compatibility shim is owed.

---

## Decision 4 — The full node runs no model (closes W1 / the runtime half of P0-8)

A full node validates and adjudicates **without any LLM.** It checks:

```
canonical encoding · signature · attempt_id · ticket · bond state · class state
· PWU · exposure · panel · receipt · court proof · safe/live transitions
```

It never runs Qwen / Llama / Ollama / vLLM, never loads a model file, never re-executes an inference.
This is not a convention — it is a **compile-time boundary**:

- the `misakad` / `consensus` crates carry **no dependency** on any model-runtime crate;
- inference lives only in sidecars: `palw-worker` (execute, produce trace), `palw-signer`
  (bond key + anti-equivocation journal), `palw-watchtower` (re-execute, detect fraud, prosecute);
- **CI forbids** a `consensus`→runtime import edge (a test that greps the dependency graph, not a
  code comment);
- the "no runtime → `panic!`" paths are deleted. A full node without a model is the normal case, not
  a fault. (ADR-0036's `PalwWorkerFailed` retry belongs to the *producer* sidecar, not the validator.)

---

## Decision 5 — Candidate-scoped PALW state, and an authenticated commitment for pruning (closes P0-4, half of P0-5)

P0-4: today the weigher reads the node's current-sink mutable stores (`bond_view`, carriage,
capability), so two nodes with the same DAG but different applied history weigh the same candidate
differently — a permanent partition, and a direct violation of W3 (equal DAGs ⇒ equal weights).

V2 derives **every** weight input from the candidate chain point, never from the node's sink:

```rust
pub struct PalwChainStateV2 {
    pub state_root: Hash,
    pub bonds: BondStateRoot,
    pub reserved_exposure: ExposureStateRoot,   // Decision 6
    pub classes: ClassStateRoot,
    pub class_targets: ClassTargetRoot,         // per-class DAA
    pub capabilities: CapabilityStateRoot,
    pub claims: ClaimStateRoot,
    pub panels: PanelStateRoot,
    pub court_sessions: CourtStateRoot,
    pub safe_weight: U256,
    pub live_weight: U256,
    pub safe_frontier: Hash,
    pub epoch_counters: EpochCounterRoot,
}

fn apply_palw_transition(
    parent_state: &PalwChainStateV2,
    accepted_objects: &[PalwConsensusObject],
    current_attempt: &PalwAttemptEnvelopeV2,
) -> Result<PalwChainStateV2>;
```

The transition is built from the **same candidate-chain context and deterministic acceptance
ordering as UTXO validation** — never a separate "PALW-ish timeline." The invariant (ADR-0039 §3d,
made a gate here):

```
same genesis · same DAG · same block bodies · same evidence
    ⟹ identical palw_state_root, safe/live weight, and selected tip
```

independent of: block/evidence arrival order, prior sink, DB insertion order, restart count,
archival-vs-pruned, IBD start point, thread count, or ISA (x86-64/arm64). "Reading absent data as
nothing" is forbidden — a missing fact is an error, never a permissive zero.

For pruning (ADR-0041), the PALW state root, class state, bond exposure, unresolved court sessions,
safe frontier and unresolved claims are carried in the pruning proof. When a new header commits a
state root, the ADR that lands it **must spell out a hash ordering with no challenge↔commitment
cycle** — this is the one place the plan flags as "don't implement by vibes."

---

## Decision 6 — Admission split, and per-bond exposure (closes P0-2, P0-10)

Admission is two phases. **Stateless** (no chain state): version + canonical encoding, size bounds,
`attempt_id` recompute, signature verification **with the public key inside the commitment**, ticket
recompute, target check, domain/network separator. **Stateful** (candidate-chain snapshot):

```
1. bond outpoint is Active at the candidate chain point
2. the bond record's pubkey  ==  the commitment's executor_pubkey     ← closes P0-2
3. operator_id matches the bond registration
4. class is Active (not frozen)
5. artifact_root == the class's registered root
6. PWU is consistent with the class rules
7. within the class epoch budget (ADR-0039 D5, as a predicate on the PRODUCING block's own
   selected-chain class production — never the broken mergeset formulation of D5c)
8. within the bond's exposure ceiling                                  ← closes P0-10
9. the bond is not in retirement / withdrawal wait
10. no equivocation by the same bond
```

P0-2: today admission checks the signature's *length* and that the named bond is *Active*, but never
that the bond's holder *authorized* anything — so W8 ("no bond, no block") degrades to "write any
Active bond's name." Verifying the ML-DSA-87 signature under the bond record's key, before the
ticket check, restores it. (A fix along these exact lines is already in flight on `palw-only-v4`;
this ADR is where it becomes part of the atomic ruleset rather than a lone patch.)

P0-10: `Provisional`/`ReceiptLicensed` carry positive live weight, but nothing bounds how many
immature claims one bond backs, so a fake-root grinder can stack many claims before the first slash
lands — collateral is levered many times over. V2 reserves exposure per bond as a consensus fact:

```
reserved_exposure(bond)  =  Σ immature_claim.pwu × slash_value_per_pwu[class]
reserved_exposure(bond)  ≤  slashable_collateral(bond) × max_exposure_ratio
```

reserved at commitment admission (or at live-weight eligibility), released only on `Final`,
`Voided`, or `timeout`. Relay in-flight limits (ADR-0039 D6) stay as *supplementary* DoS control,
never the economic bound. And withdrawal is delayed past the whole liability period (the startup
invariant in Decision 1) so a bond cannot commit fraud and leave before it is provable.

---

## Decision 7 — Panel, data availability, and no-show (closes the wiring half of P0-7, DA half of P0-8)

The panel module (`palw_job_panel.rs`) already excludes the executor and dedups by bond outpoint;
P0-7 is the **caller** in the virtual processor passing `executor_bond_outpoint.transaction_id` as
the exclusion id, while candidates carry `validator_pubkey_hash` — different namespaces, so the
executor is never actually excluded, and `operator_root` is always `None`. V2 wires the caller to the
bond-registry-resolved identity:

- exclude executor's **bond**, executor's **validator key hash**, and executor's **operator_id**;
- `operator_id` is a **required** bond-registration field, DERIVED from an operator key
  (`palw_operator_id_v2(key)`), and every identity must carry the bundle's `min_collateral_sompi`
  — so splitting X collateral across bonds manufactures at most `⌊X / min_collateral⌋` panel
  identities. **Sybil is bounded, not prevented** (Amendments §A3); the original wording ("does
  not manufacture extra panel seats") overclaimed;
- an assigned seat that no-shows past its deadline is slashed / loses collateral, derived
  chain-scoped so reorg and replay reach the same verdict.

Trace data availability: the full trace is not on-chain, but the commitment binds
`trace_manifest ‖ chunk_merkle_root ‖ chunk_count ‖ retention_deadline`. A producer that fails to
serve a requested opening/chunk by the deadline **defaults**: claim void, bond slash — so silence
can never pin a block at `Provisional` forever. The four states — data obligation, receivability,
`unavailable` receipt, producer withholding — are kept distinct so a panel member is never punished
for data the *producer* hid.

---

## Decision 8 — The BASE-0 court is complete and proof-carrying (closes P0-8, P0-9)

At RC, the **only** class that carries weight is `PALW-BASE-0` (ADR-0039 D1a). An accelerated class
that is registered but has `coverage < 100%` or no independent verifier gets **share 0**.

P0-8: today the court fetches raw model rows from a `PalwWeightOracleV1`, but the full-node
production oracle `PalwNoWeightsV1` returns `None` for every row, so every arithmetic dispute is
`Unadjudicable`; swapping in a real oracle only where a node happens to hold the artifact makes
adjudication depend on local files — a partition. V2 makes the **evidence proof-carrying**: a fraud
proof includes the disputed step, opcode, pre/post-state roots, input operands, weight row,
quantization parameters, RoPE table slice, and Merkle proofs to the class artifact root and the
trace root. The full node re-runs one primitive on the CPU from the proof alone. `Unadjudicable` is
**removed as a reachable outcome** for BASE-0's op set.

P0-9: bisection is completed on both soundness and liveness —

1. session id includes the attempt id and both bonds;
2. the midpoint state is proven to lie inside the trace commitment (no responder-chosen interval);
3. each turn has a DAA-boundary deadline;
4. challenger timeout ⇒ default loss; responder timeout ⇒ default loss;
5. interval length 1 ⇒ terminal one-step adjudication (a terminal *opening* exists);
6. data-withholding ⇒ default;
7. round count is derived from trace length, **not fixed at 10**:

```
rounds  =  ceil(log2(max_step_leaf_count)) + terminal/opening rounds
```

and it is an **activation condition** that, for every registered class, the worst-case terminal
verdict fits inside the COURT window (Decision 1's amended `window_court ≥ worst-case court
duration` — see Amendments §A1: an open court suspends finality, so the court races its own
window, never the challenge window). The committed ladder-gap test on this branch
(`palw_schedule.rs`, commit `d1891333`) already measures that a 10-round ladder cannot reach the
pinned model; V2 sizes the ladder to the measured trace instead, and `a7be964e` closes the loop
against the catalog: `PalwCourtParamsV2::max_step_leaf_count` must cover
`PalwClassCatalogV2::max_step_leaf_count()`, so understating the ladder to shrink `window_court`
contradicts the catalog the same bundle commits to.

---

## Decision 9 — One fork-choice authority (closes the other half of P0-5)

A single pure comparator orders PALW candidates, and **every** chain-selection site calls it:

```rust
fn compare_palw_candidates(a: &PalwChainStateV2, b: &PalwChainStateV2) -> Ordering;
// 1. safe frontier   2. safe weight   3. live weight among same-frontier descendants   4. tie-break
// live_total = safe_weight + bounded immature weight  (so maturing never lowers the total)
```

Sites that MUST use it: virtual canonical tip, IBD-complete tip, pruning point, finality/deep-reorg
gate, restart recovery, sync-peer chain comparison.

P0-5: the header processor passes `palw = None` for both candidate and previous tip
(`header_processor/processor.rs:436-449` / ADR-0039 §3d names `:416` as the site), so the
header-selected-tip store stays blue-work-ordered even after the virtual sink starts using PALW
weight — two canonical-chain views inside one node. V2 forbids a *second* authority. A header-only
processor that lacks bodies and PALW state MAY use blue work as a **download-ordering hint**, but
that store is renamed `header_download_hint` (not `header_selected_tip`) and is never consulted for
chain authority, pruning, or finality.

---

## Decision 10 — Reward is not spendable before `Final` (closes reward-before-void)

A `Provisional` block can still become `Voided`, so its PALW reward must not be spendable
immediately. V2 escrows it:

```
block accepted → reward escrow → claim reaches Final → spendable
```

Escrow is preferred over a fixed long coinbase maturity because each claim reaches `Final` at a
different point; a single maturity constant is either too short (spends voided reward) or needlessly
long for the common case. PALW reward is a **carve of the fixed subsidy** (ADR-0038 Decision G /
ADR-0035 §5's 62 % worker share shape), never an addition to it — the schedule is never exceeded
(I6/I15).

---

## Decision 11 — The ruleset fingerprint, committed to genesis (RC == mainnet, by hash)

RC-equals-mainnet is not a promise a human keeps; it is a hash a node checks.

```
palw_ruleset_id = H(
    protocol_version ‖ canonical_consensus_params ‖ class_catalog_root ‖ court_catalog_root
    ‖ trace_format_version ‖ signature_contexts ‖ fork_choice_version
)
```

`palw_ruleset_id` is committed to genesis. **The last RC and mainnet share the same
`palw_ruleset_id`.** Network identity lives in the challenge's `network_domain` (Decision 3a), so a
testnet block still cannot be replayed on mainnet even with an identical ruleset id. The P2P
handshake exchanges `network_id`, `palw_ruleset_id`, `class_catalog_root`, `court_catalog_root` and
drops a mismatched peer early. The mainnet binary is built from the **same git tag** as the final RC,
changing only network-scoped constants, and it reads the RC's canonical ruleset bytes rather than a
human re-typing parameters.

---

## The release gate — the audit's 12 conditions, as one checklist

No PALW-RC genesis and no mainnet activation until **all** hold. Each maps to the external audit's
"Activation の最低条件" and to a decision above.

| # | Condition | Decision | Closes |
|---|---|---|---|
| 1 | Ticket binding: commitment mutation ⇒ PoW failure | 3a | P0-1 |
| 2 | Signer authentication: commitment signature verified under the bond key | 6 | P0-2 |
| 3 | Fresh-tip semantics: a panel-less block is `Provisional`, deterministically | 2 | P0-3 |
| 4 | Candidate-chain purity: all facts from the candidate point, no sink reads | 5 | P0-4 |
| 5 | Single fork-choice authority across virtual/header/IBD/pruning/finality | 9 | P0-5 |
| 6 | Typed signature APIs (receipt/commitment/attestation/court contexts split) | 3, 6 | P0-6 |
| 7 | Proof-carrying court: operands + artifact proofs in the evidence | 8 | P0-8 |
| 8 | Complete bisection: midpoint verify, terminal opening, defaults, bond binding, depth | 8 | P0-9 |
| 9 | Collateral accounting: per-bond immature exposure ceiling | 6 | P0-10 |
| 10 | Panel enforcement: correct exclusion id, operator dedup, no-show penalty | 7 | P0-7 |
| 11 | Class lifecycle: Active/frozen/coverage, per-class retarget, redistribution, epoch budget | 1, 5 | W blockers |
| 12 | Determinism suite: prior sink / restart / IBD start / pruning point / store order invariant | 5 | P0-4 |

Additional (from the audit's §追加): **W1 runtime separation** (Decision 4) and **reward escrow**
(Decision 10) are release-blocking too.

---

## Implementation order — PR-00 … PR-10

Intermediate PRs are **never** enabled on a public preset. The public profile is added only in the
last PR.

| PR | Content | Done when |
|---|---|---|
| **PR-00** | this ADR, threat model, P0-reproducing red-test spec | the attack tests are red on current impl |
| **PR-01** | `PalwAttemptEnvelopeV2`, new algo id, canonical hash transcript | every field mutation invalidates the ticket |
| **PR-02** | full-node runtime-free admission | consensus build carries no model dependency (CI edge test) |
| **PR-03** | `PalwChainStateV2`, delta, state root | equal-DAG differential test passes |
| **PR-04** | bond-signature / class / PWU / epoch / exposure admission | foreign bond & over-exposure rejected |
| **PR-05** | worker / signer sidecars, anti-equivocation journal | fail-closed before a double-sign |
| **PR-06** | Provisional / panel / receipt / DA / no-show | a fresh tip is always weighable |
| **PR-07** | BASE-0 court, terminal bisection, default rules | intentional fraud is adjudicable |
| **PR-08** | safe/live fork choice, IBD, pruning unified | all subsystems pick the same tip |
| **PR-09** | per-class DAA, lifecycle, reward escrow | class stop/resume is deterministic |
| **PR-10** | atomic params, ruleset id, new genesis, public profile | PALW-RC boots from genesis |

**The next file to touch is not the five `None`s in `params.rs`.** It is PR-01. The public profile
is PR-10, and only after PR-00…PR-09 close.

---

## Amendments (2026-08-20, implementation-time)

Three clauses changed when the implementation tested them. Each is amended in place above and
recorded here with the reasoning, so the spec a reader audits is the spec the tree implements.

### A1 — The court races `window_court`, not the challenge window (Decision 1, Decision 8)

Original invariant: `challenge window > worst-case court duration`. That presumes finality RACES
an open court — that a claim could reach `Final` while its dispute is still being bisected, so the
challenge window had to be long enough to contain the whole prosecution. The implemented state
machine is stronger: **an open court session suspends `ReceiptLicensed → Final` outright**, so a
challenged claim cannot finalize early no matter how the windows compare, and what actually needs
bounding is the court against its own deadline. The binding inequality is therefore
`window_court ≥ (ceil(log2(max_step_leaf_count)) + terminal_rounds) × turn_deadline_daa`,
enforced at bundle construction (`PalwCourtParamsV2`, Decision 8's formula, overflow = refusal)
and cross-checked against the class catalog's real worst case (`verify_against_catalog`,
`a7be964e`). Implementing the original inequality was attempted and withdrawn: it added no safety
(the suspension already provides it) and taxed every honest, unchallenged claim with the
worst-case court duration for nothing.

### A2 — Decision 3c must not land without a mutated-witness path

With identity = `attempt_id` (signature excluded), any third party can flip one bit of a carried
signature and relay the result: same block id, invalid witness. The first-seen mutated copy fails
admission and the id lands in known-invalid caches — after which the HONEST block, arriving
second with the same id, is refused unseen. One flipped bit censors one block, network-wide, at
zero cost. (Bitcoin met this exact shape in its mutated-block/witness-malleation handling: reject
the copy without caching invalidity for the id.)

The tree therefore keeps **raw-carrier-bytes identity** for the `palw_commitment` field: a
mutated copy is a *different* id that dies alone at admission; valid-signature siblings of one
attempt are mintable only by the bond holder (ML-DSA-87 signing is hedged), share one PoW ticket,
and deduplicate at the claim (`claim_id = attempt_id`), so self-malleation buys DAG spam bounded
by the holder's own ban exposure, never a second paid claim. The PoW side of 3c is fully in force
— the signature is outside `attempt_id`, outside `commitment_root_v2`, and moves no digest
(`palw_v2_commitment_mutation_invalidates_pow` pins this). 3c's identity half lands only together
with a pipeline path that rejects witness-mutated carriers WITHOUT marking the block id invalid.

### A3 — Decision 7's Sybil guarantee, restated as the bound it is

Original wording: requiring `operator_id` at bond registration means "splitting collateral across
bonds does not manufacture extra panel seats." It does — up to a bound. What the implementation
provides (`4596a644`): `operator_id` is derived from an operator KEY, and every distinct identity
must independently carry `min_collateral_sompi`. Splitting X collateral therefore yields at most
`⌊X / min_collateral⌋` panel identities. That is a real economic floor per seat, and it is the
honest claim: **Sybil-bounded, not Sybil-proof.** Panel quorum math and slash sizing must assume
an adversary holds every identity their collateral can fund.

### A4 — the free panel re-roll is priced on BOTH lanes, and only one of them was

Decision 7's panel is drawn from an anchor the claim cannot choose, so there is nothing to shop
for at mining time. What remained was the *re-roll*: a producer that dislikes its drawn panel lets
the bind window lapse and commits again. On the ATTEMPT lane that is already expensive, and
measurably so (`abandoning_a_panel_costs_a_block_its_reward_and_its_epoch_budget`): the claim is a
BLOCK, so a re-roll is another solved PoW, the abandoned block's reward is forfeit, and the
class's epoch budget was spent at acceptance and is never refunded.

**All three of those costs rest on the same fact, and the free-prompt lane does not have it.** An
ADR-0044 commitment rides a transaction, not a block. Measured on the merged tree: after a
`BindTimeout` the reservation was released in full, no counter moved, no bond was debited, and the
next commitment was accepted in the very next block — a redraw priced at one transaction fee,
indefinitely repeatable.

The fix keeps an abandoned free-prompt claim's collateral RESERVED for
`PalwStateParamsV2::fp_abandon_hold_daa` after the void. Nothing is confiscated — declining to
bind is not an offence — the reservation is delayed, and every sompi returns when the span
elapses. What that buys is a denominator: N concurrent redraws need N × the reservation, so the
redraw rate is bounded by collateral. That is deliberately the same currency §A3's Sybil bound
speaks, so the two compose — an adversary's identity count and its redraw rate are both bounded by
the same X, and neither can be traded for the other.

`fp_abandon_hold_daa = 0` is the pre-FP configuration and leaves the attempt lane exactly as it
was, which is what every attempt-only fixture runs at.

---

## What this ADR does not decide

- **Measured mainnet parameters** — window sizes, share table, ρ_r, k, bonds, `base(C)` — are soak
  outputs (ADR-0036), filled into `PalwConsensusParamsV2` after Chaosnet + RC, in a later ADR.
- **The PWU derivation** (ADR-0039's open item) — still its own record; V2 requires it before any
  class carries weight.
- **Accelerated-class catalogs and second implementations** — BASE-0 is the only weight-bearing
  class at RC.
- **Operator items** — seeds, ports, public entry, faucet — are ADR-0035 §6 shaped and per-network.

## Number hygiene

This is ADR-0042; 0041 is the last committed. ADR-0036 records a same-day 0035/0036 collision from
two parallel sessions; if a concurrent branch also claims 0042, the tie is broken by keeping this
file's content and renumbering the later writer, per ADR-0036 Decision 5.
