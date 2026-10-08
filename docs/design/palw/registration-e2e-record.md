# Registration E2E record — a model registration on the REAL consensus path (G14 lane D, phase 1)

Branch `g14/d-node-e2e` (base `febc07f24`). Everything below is executed by tests in
`consensus/src/pipeline/virtual_processor/tests/g14_registration_e2e.rs` on the real testnet-12 harness
(`t12_with_harness_cards`: the launched ruleset, the eight genesis cards on harness keys, signed `0x4b` carriers funded from the
genesis float, blocks mined through the node's own template, `palw_tir_v1` armed). Status is **PASS only where a test exercises the
real path**; anything the current code cannot express is **GAP** with the exact missing object or state.

```text
SDK preflight -> TIR admission -> registration object -> signature/bond/fee -> mempool -> block template/inclusion ->
apply_class_registration_v1 -> state persistence (tip, per-block delta) -> ConsensusApi / RPC lookup -> status -> restart/IBD/reorg
```

## 0. How to reproduce

```text
export CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
cargo test --offline -p kaspa-consensus --lib g14_            # 18 tests, ~260 s (two fence-schedule sweeps are ~80% of it)
cargo test --offline -p kaspa-consensus-core --lib palw_model_registration        # the reader fix's unit tests
# regenerate the real-checkpoint fixtures (headers only, no weights; ignored by default):
G14_CKPT_ROOT=/Users/wata/Downloads/MISAKA-wt-b/hf-ckpt [G14_ONLY=<name>] [G14_CONTEXTS=1024,512,...] \
  cargo test --offline -p misaka-palw-sdk --test g14_registration_fixture -- --ignored --nocapture
```

## 1. What "parity" means (the contract the tests hold)

`palw_tir_registration_preflight_at_v1` (the gate: IR fence, then admission v10 at a height) is what the SDK preflight, `palw-class`,
`getPalwModelPreflight` and the registration builders all call. The chain's verdict is the processor's acceptance walk
(`palw_v2_accepted_objects`: drop-by-name below the fence, the bond's signature, the chain's target, admission v10 under the rules the
PROCESSOR resolves, the share rule, then the fold's own refusals). The gate's own doc lists what it does not hold ("the registrant
bond's signature and collateral, the chain's target, and the share the certification decides"). So the tests assert:

1. gate REFUSES => chain refuses at its admission arm and **its sentence carries the gate's own error text** (same reason class);
2. chain ACCEPTS => gate admits;
3. gate admits, chain drops => only for a **stateful** reason the gate cannot see, named in the case.

Verdict columns below: `G` = gate (`palw_tir_registration_preflight_at_v1`, code), `C` = chain (`palw_v2_validate_objects`, the fold,
and the acceptance walk). "keeps" = folded by the walk.

## 2. Parity table (genesis state, signed objects, registrant = a harness card)

Test: `g14_adversarial_registrations_gate_and_chain_agree` (every row; one `[g14]` line each with `--nocapture`),
`g14_parity_corpus_classes_real_inventory_roots`, `g14_fence_height_is_the_same_for_gate_and_chain`, `g14_duplicate_registrations`.

| Case | Path exercised | G | C | Status |
|---|---|---|---|---|
| dense-gqa-2layer, moe-top2-shared, mamba2, gdn-k2-v4-grouped, sliding-global (real inventory roots) | gate, arm, fold, walk | admits | keeps | PASS |
| fixed-state-saturation, hist-window (range analysis cannot prove) | same | `TIR_PROGRAM_REFUSED` | refuses, gate's text | PASS |
| program: trailing byte | admission step 1 | `TIR_PROGRAM_REFUSED` | refuses, same text | PASS |
| program: **unknown operation tag** (unsupported op masquerading as a known one; found as the first byte whose replacement is an unknown Borsh tag) | decode_canonical | `TIR_PROGRAM_REFUSED` | refuses, same text | PASS |
| program: another primitive set's id | decode / prim-set | `TIR_PROGRAM_REFUSED` | refuses, same text | PASS |
| program: unknown logits scheme; token bound below the vocabulary; held history bound (2^21) | admission step 2 | `TIR_CLASS_REFUSED` | refuses, same text | PASS |
| tokenizer swapped, id stale | admission step 7 | `TIR_CLASS_ID_IS_NOT_DERIVED` | refuses, same text | PASS |
| tokenizer swapped, id re-derived | a different class | admits | keeps (a different class id: **inert**) | PASS |
| artifact root swapped, id stale (artifact substitution) | step 7 | `TIR_CLASS_ID_IS_NOT_DERIVED` | refuses, same text | PASS |
| artifact root swapped, id re-derived | a different class | admits | keeps (inert; **nothing binds a root to real weights**, see GAP-8) | PASS (inert) |
| layout of another program (mamba2's tiles on dense); a commit tile dropped | step 3/4 | `TIR_CLASS_REFUSED` | refuses, same text | PASS |
| layout `max_context` changed, id stale (stale layout) | step 7 | `TIR_CLASS_ID_IS_NOT_DERIVED` | refuses, same text | PASS |
| canonical job one decode token longer (plan substitution of the job) | step 6 | `CLASS_NOT_ATTRIBUTABLE` | refuses, same text | PASS |
| pwu rule `MaxPerAttempt` on an IR class; `pwu_per_inference` +1 | step 6 | `CLASS_IS_NOT_DERIVED`, `PWU_PER_INFERENCE_MISMATCH` | refuses, same text | PASS |
| share_permille 1 for a class no certified family covers (class family claim) | step 8 | `NOT_END_TO_END_CERTIFIED` | refuses, same text | PASS |
| target not the chain's | `starts_at_the_chains_target` | admits | refuses ("difficulty is not a registrant's to choose") | PASS (stateful, named) |
| slash value not the network's; zero slash value | fold (`apply_class_registration_v1`) | admits | drops in the fold | PASS (stateful, named) |
| activation 4,001 DAA ahead (lookahead 4,000) — **target DAA mismatch** | fold | admits | drops in the fold | PASS (stateful, named) |
| signature by another card's key; **replayed from testnet-11**; **replayed on the same network name with another genesis**; unsigned | `palw_v2_class_registration_is_signed` | admits | refuses ("not signed by the bond it names") | PASS (stateful, named) |
| registrant bond the chain does not have | same | admits | refuses | PASS (stateful, named) |
| the same object twice in one block | walk (the IR registration cap) | — | keeps the first only | PASS |
| the same object / the same class by another registrant, in a later block | fold (`DuplicateClass`) | admits | drops in the fold | PASS (stateful, named) |
| same weights (artifact root) under another tokenizer = another class id (`g14_same_weights_under_another_class_id_...`) | gate, fold | admits | **keeps** | PASS as observed; see GAP-9 (only the SDK filters it) |
| `palw_tir_v1` fence at 5: DAA 0,1,4 | gate = `TirNeedsItsFence`; walk drops by name | refuses | refuses ("not in force") | PASS |
| fence at 5: DAA 5, 6, 1,000, 100,000 | same object | admits | keeps | PASS |

### 2.1 Mutation differential (`g14_program_mutation_differential`)

1,050 single-byte mutations of the dense and MoE corpus programs (the first 96 bytes with two XOR values, then a stride through the body),
each carried as a registration whose id, canonical job and signature are consistent with the mutated class: **96 admitted by both, 943
refused by both as `TIR_PROGRAM_REFUSED`, 11 refused by both as `TIR_CLASS_REFUSED`, 0 disagreements** (and every refusal carries the
gate's own text).

### 2.2 The whole shipped fence schedule (`g14_gate_and_chain_agree_across_the_shipped_fence_schedule`)

`palw_t12_shipped_params()` with the harness cards (all ten schedule heights 750, 1000, 1300, 1700, 2000, 3600, 5300, 5395, 5490, 5585;
`palw_tir_v1` at 2000). 30 classes (the corpus at declared contexts 64, 512, 4,096 and 32,000, plus the two real-checkpoint classes at 1,024) judged at h-1,
h, h+1 of every height (33 heights): below 2000 all 30 refused by both ("not in force"); from 2000 on 22 admitted and 8 refused (the two
programs the range analysis cannot prove, at four contexts), **identically for gate and chain at every height, the same classes at every
height** (no later fence moves a corpus verdict). **0 disagreements.**

## 3. Pipeline, persistence, restart, IBD, reorg

| Case | Path exercised | Result | Test | Status |
|---|---|---|---|---|
| Signed IR registration carried on `0x4b`, funded from a genesis float | `validate_mempool_transaction` (Ok) -> node's own heartbeat template carries it -> next chain block folds it | class row `Registered{activation, 0‰}`, `registrant_bond`, the gate's exact `tir_classes` record (program travels, `check_program_v1` ok), at the chain's target, 1 MSK burned, exposure reserved | `g14_registration_mined_end_to_end_on_the_real_node_path` | PASS |
| ConsensusApi reads | `palw_v2_class_table`, `palw_tir_class_record_v1`, `palw_v2_registration_terms` (lists the id and root), `palw_model_registry_v1` | status `Registered`, no share row, registry lifecycle `Candidate` (admits no claim) | same | PASS |
| Registered is not eligible | `palw_producer_facts_v2(new class).ready_to_produce` | `Err("the model registry admits no new claim of this class now")` | same | PASS (producer-side read; no attempt block was built for the class) |
| Activation clock | heartbeats to `activation_daa` | `Active`, share 0 | same | PASS |
| Carrier-status readers (`getPalwModelRegistrationStatus <txid>`) | `palw_registration_row_written_by_v1`, `palw_registration_carrier_object_v1` | **were BUG** (never saw `ClassRegisteredTirV1`); fixed in `b468d5ccf` | same | PASS after fix |
| Second node replays A's blocks | `validate_and_insert_block` | same sink, same state root, same class row, same record, same per-block delta roots | `g14_registration_replay_on_a_second_node_and_across_a_reorg` | PASS |
| Reorg onto a heavier branch that never saw the carrier | sink switch | class row, record, burn, exposure all reverted; roots equal B's fresh replay; the virtual UTXO view still merges A's blue blocks (carrier change stands) | same | PASS |
| ... then the next chain block on B's side | mergeset acceptance | folds the merged carrier: the registration is re-folded at that block's DAA, same gate record, burn taken once | same | PASS |
| A out-works B again | reorg back | identical class row (now Active), roots, delta rows, collateral | same | PASS |
| Pruned join inside the registration's life | `capture_pruning_point_palw_state` -> `import_pruning_point_palw_state` | carriage carries the class + `tir_classes` program; importer folds the activation flip to A's roots | `g14_registration_survives_a_pruned_import` | PASS |
| **Real restart** (new `Consensus` over the same database, `process_genesis` off) | `TestConsensus::with_db` reopened | sink, PALW tip, class row, record, class table, delta rows, UTXO all off disk; the restarted node folds the activation flip; a replaying node agrees with every root across the restart | `g14_registration_survives_a_node_restart_over_the_same_database` | PASS |
| Registration carried BELOW the fence | mempool Ok (the pool does not run the gate), mined, dropped by name | no class, no burn, fee spent; the dropped-carrier derivation re-reads the object and the gate gives `TirNeedsItsFence`; the SAME object re-carried past the fence registers | `g14_registration_mined_below_the_fence_is_dropped_by_name_then_accepted_past_it` | PASS |
| Registrations the gate/chain refuse, mined (unknown op, stale id, share claim, replayed network, wrong target) | mempool Ok -> template -> dropped | block stands, class absent, no burn/exposure, fee spent; re-diagnosis names the gate's code for the three gate reasons and can say nothing for the two stateful ones | `g14_refused_registrations_cost_a_fee_and_write_nothing` | PASS (see finding F2/F3) |
| Two registrations in one template | walk cap `PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1` | first in block order folded, second dropped by name, both fees spent | `g14_two_registrations_in_one_block_one_is_folded` | PASS |

## 4. Classes lowered from REAL local checkpoints

Generator: `misaka-palw-sdk/tests/g14_registration_fixture.rs` (ignored; shape-only lowering of the SDK preflight — `config.json` and the
safetensors header, **no weights loaded**, so the artifact root is a SYNTHETIC commitment: a keyed hash of config and tokenizer bytes;
registration never reads weights). The layout is the SDK's layout search under THIS harness's ruleset (launch + `palw_tir_v1` armed),
judged by the SDK's `tir_class_admission_offline_v1`; the fixtures are `tests/fixtures/g14/*.json`, and the consensus test re-derives the
class id and compares. `g14_real_checkpoint_classes_parity_and_mined` runs, per fixture: the gate and the chain on the class, six
adversarial edits of it (trailing byte, stale tokenizer id, stale root id, another program's layout, share claim, unsigned), and the whole
mined pipeline of section 3.

| Checkpoint (local, headers only) | Ruleset the layout was searched and judged under | Context | Program | Gate | Chain (acceptance walk) | Whole pipeline (section 3) | Status |
|---|---|---|---|---|---|---|---|
| `HuggingFaceTB/SmolLM2-1.7B-Instruct` (llama, GQA, tied head) | harness: launch + `palw_tir_v1` from genesis | 1,024 (the widest the gate admits under the launch ladder) | 13,770 B | admits | keeps | mined, folded, persisted, read, activation flip | PASS |
| `state-spaces/mamba-370m-hf` (SSM) | harness | 1,024 | 11,603 B | admits | keeps | same | PASS |
| the four below | harness (launch ladder) | not generated: the layout search at the launch ladder did not finish in a debug build within the budget | — | — | — | — | not run (not needed: they register under the release rules below) |
| `SmolLM2-1.7B-Instruct`, `mamba-370m-hf`, `ibm-granite/granite-3.1-1b-a400m-instruct` (MoE), `Qwen/Qwen3.5-0.8B` (GDN hybrid) | `palw_t12_shipped_params()` at DAA 5,585 (what `palw-class preflight` judges) | **32,783** (the widest the gate admits there) | 13,770 / 11,603 / 14,443 / 32,284 B | admits | keeps | **mined under the whole release armed** (`t12_release_compressed`: launch ruleset + every release flag day in its order, compressed to DAA 20..40; the corpus sweep shows no verdict moves between the real heights): carrier -> mempool -> template -> fold -> tip -> reads -> activation flip; plus a second node replaying that chain to the same roots | PASS |
| the same four, verdict by height under the real schedule (33 heights incl. h-1/h/h+1 of all ten) | shipped | 32,783 | — | SmolLM2: refused until DAA 2000, then admitted; mamba, granite, qwen: refused until DAA 3600 (`palw_tir_fence2`'s ladder), then admitted | **the same at every height** (0 disagreements) | — | PASS |

The artifact root of every row is **synthetic** (a keyed hash of `config.json` and `tokenizer.json`): the lowering is the preflight's
shape-only one, no weight was read, and registration never reads weights. A converted artifact's inventory root is what a real
registration would declare; the corpus classes of section 2 are the ones with real inventory roots.

## 5. Findings

**F1 (BUG, fixed `b468d5ccf`).** `palw_registration_carrier_object_v1` / `_class_v1` (hence `palw_registration_row_written_by_v1`,
used by `getPalwModelRegistrationStatus <carrier txid>`) matched only `ClassRegistered`. An IR registration (tag 61) rides the same
carrier and writes the same class row, but its status could never be found by carrier (`REGISTRATION_NOT_INCLUDED` for a folded
registration), and a mined-and-dropped IR carrier fell to the "could not re-read it" verdict. Node-local read path only; legacy behaviour
byte-identical.

**F2 (design observation, no change made).** The mempool admits an IR registration the chain will drop: `validate_palw_lifecycle_tx`
checks the shape only (an IR registration RIDES at every height, A-2), and no node policy asks the gate. A refused registration is
therefore relayed, mined and dropped, and its sender pays the carrier fee (tests: `g14_refused_registrations_cost_a_fee_and_write_nothing`,
`g14_registration_mined_below_the_fence_...`). Proposal (node-local policy, like `palw_mempool_market_refusal`): run
`palw_tir_registration_preflight_at_v1` plus the arm's O(1) checks (signature, target) on a `ClassRegisteredTirV1` carrier at admission.

**F3 (observation).** `getPalwModelRegistrationStatus`'s dropped-carrier diagnosis re-asks only the gate at the accepting DAA. It names the
gate's reasons exactly; for a drop by a chain-state reason (signature, network replay, target, slash value, activation lookahead,
duplicate) the gate admits and the diagnosis falls to "unknown — see the node log". Same cause as F2; fixing it means running the arm's
stateful checks in the diagnosis.

**F4 (observation, safe direction).** The RPC/SDK judge at the virtual (tip) DAA, the chain at the including block's DAA. At the IR fence
height the gate may refuse (`FAMILY_FENCE_CLOSED`) a registration that would land after the fence; the sweep (2.2) found NO case in the
shipped schedule where the gate admits and a later height refuses (no fence tightens a corpus verdict).

**F5 (observation).** `PalwRegistrationTermsV2::registered_artifact_roots` is a client-side filter; consensus keys artifact ownership by
`(class_id, root)` so the same weights under a second class id (another tokenizer/layout) pass gate and chain.

**F6 (observation).** A model alias is a free claim: `ModelLineFounded.name` (the only on-chain alias; the registration carries none)
is accepted from ANY active bond on any Active class, bound to no property of the weights
(`g14_a_model_alias_is_a_free_claim_on_an_active_class`).

**No preflight/consensus disagreement of the unsafe kind** (gate admits a class the chain drops for a reason the gate should have seen,
or the reverse) was found in 1,050 mutations, 28 corpus classes x 33 heights, ~27 adversarial rows and the real-checkpoint classes.

## 6. GAP — cannot be expressed on the current code (Lead-owned objects/states needed)

`misaka-palw-challenge` and `misaka-palw-kernel` are not dependencies of `kaspa-consensus`, `kaspa-consensus-core`, `kaspa-rpc-service`
or the SDK's consensus path (kernel is an SDK dependency only); nothing in consensus reads a policy id, a conformance statement, a
beacon or a verification plan. No wire format is invented here.

| # | Case | What is missing (object / state) | What the test asserts once present |
|---|---|---|---|
| GAP-1 | Challenge-policy substitution | a `challenge_policy_id` (`PostCommitChallengePolicyV1::id()`) bound at class registration or activation (a field of the class record or an object), and a fold rule refusing activation / claim acceptance under another id | registration under policy A, then a claim or activation under policy B is refused with the same verdict on gate and chain; restart/IBD/reorg equality of the binding |
| GAP-2 | Conformance commitment after an artifact change | `ConformanceCommitmentV1::statement_root` stored per `(class, artifact_root)` (object + state) and a rule that a new root (`ModelVersionPublished` / `roots_in_force`, Lane MU) invalidates it | a conformance statement for root R does not carry over to R'; publishing R' without a new statement blocks activation |
| GAP-3 | Beacon contribution reorder | beacon contribution objects and a `beacon_state` folded from FUTURE PALW work (`verify_work_beacon_v1` wired), with an order rule | the same contributions in another order give the same beacon (or are refused), identically on gate and chain |
| GAP-4 | Heartbeat as beacon | the same beacon consumer, with the exclusion of heartbeats and BASE-0 from entropy | a heartbeat contribution is refused / inert |
| GAP-5 | Candidate self-beacon | the beacon consumer plus the candidate's own claims excluded from its own draw | a candidate's own attempt contributes nothing to its challenge draw |
| GAP-6 | G14-incomplete activation | `KernelRouteObjectV1` carriage (acceptance arm with ML-DSA signatures and fees), state `class.kernel_binding`, and `PUBLIC_PROSECUTION_COMPLETE(plan, profile)` in the lifecycle step into `Prefetching`/`Probation`/`Active` (today `Candidate -> ... -> Active` consults seats/readiness/panel only) | a class with an incomplete plan stays Candidate on the real node; RPC says why |
| GAP-7 | Plan substitution (`VerificationPlanV1::root()`) / kernel binding (`ModelKernelBindingV1::class_binding_id()`) | the plan root and binding are not part of `ClassRegisteredTirV1` nor of the class id (which binds class + artifact root only) | a registration naming plan P cannot be activated under plan Q |
| GAP-8 | Tokenizer / artifact root bound to real bytes | `tokenizer_id` and `artifact_root` are declarations bound only by the class id; nothing proves them against the weights or the tokenizer | covered by GAP-2 once a conformance statement exists |
| GAP-9 | Same weights under a second class id | a rule (or the Lead's decision that none is wanted) beyond the client-side `registered_artifact_roots` filter | refused / accepted by decision, same on gate and chain |
| GAP-10 | Model alias bound to anything | `ModelLineFounded.name` has no relation to the weights; the Lead's MN lane (`ModelDistributionDeclared`, name required on chain) | a name claim not matching the declared distribution is refused |
| GAP-11 | RPC over the wire (not run, not a defect) | the registration/preflight RPCs (`getPalwModelPreflight`, `submitPalwModelRegistration`, `getPalwModelRegistrationStatus`) were exercised through their constituent functions and the ConsensusApi reads, not through a running RPC service; there is no RPC read at all for challenge/beacon/kernel-route state | a daemon test of the three calls on an IR-armed node |
