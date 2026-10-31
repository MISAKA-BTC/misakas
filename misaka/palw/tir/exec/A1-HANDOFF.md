# A1 — Node exact computation checking

Initial base: `a5800d2dd`. Branch: `codex/positive-court-a1`.
Shared contract: A0 `a85152edc`, imported as `2bc5d6578` (v2/r1).
TIR annex: A0 `d615f62b6`, imported as `4ba338500` (same v2/r1 DTOs/domains).
Owned implementation: `ef8e4e87d`. Separate A0 registration delta: `b3a61454c`.
Packet-auth annex: A0 `5901eff3f`, imported as `8b65d68f5`.
Signed ingress: `729d513ec`; this is the final tested source candidate.
Apply the implementation and registration after the TIR annex, then the shared
packet-auth annex and signed-ingress delta.
Workspace: `/Users/wata/.codex/worktrees/positive-court-a1/MISAKA-testnet`.

## Owned implementation

- `src/node/positive_check.rs`: Node-derived `CoveragePlan`, conservative
  `CostEnvelope`, bounded `ExactChecker`, read-only progress and terminal
  computation comparison.
- `tests/positive_check.rs`: normal, missing/reordered/duplicate parts,
  authenticated arithmetic forgery, model/input substitution, output mismatch,
  logits bridging, sparse-program rejection, budget/heap/hostile-wire cases.
- `src/node/positive_contract.rs`: concrete A0 `NodeChecker` implementation,
  role authentication, Node-derived descriptor, bounded G14 payload codec,
  complete wire accounting and private checked-terminal type.
- `tests/positive_contract.rs`: separately constructed small arithmetic fixture,
  actual Miner/Panel signatures and A0 framed parts through the shared port.
- `tests/positive_check_hardening.rs`: evaluator exhaustion without Panel blame,
  invalid/inherited contexts and borrowed validation of oversized candidates.

`CoveragePlan::derive` takes the rooted registered class, artifact root, immutable
job, prompt form and local finite checking limits. It derives the existing TIR
canonical leaf schedule; the Panel cannot supply a smaller relation list.
`ExactChecker::begin_greedy` binds the independently authenticated Panel record
to that plan and the Miner's separately authenticated output. `submit_encoded`
preflights Borsh allocations, authenticates openings, evaluates every cone and
checks every logits lane and greedy selection. `finish` compares outputs only
after the final leaf and row. No model parameter provider is passed to the Node.

## Initial supported scope

Existing TIR v1, tiled i32 logits, fixed-length greedy decode, flat or Merkle
prompt. Every node must be committed. Sparse legacy commitments are explicitly
unsupported: a cone that only demands lanes needed by downstream outputs does
not establish complete coverage of a partially consumed intermediate tensor.
This check never modifies the registered program or changes old wire meanings.
The fixture adds commitments before registration, using ordinary TIR semantics.

The schedule includes committed computations, Fixed checkpoints and Hist tiles.
The existing court authenticates parameters/input/dependencies and re-executes
the demanded arithmetic. Full-node commitments ensure that every tensor lane
also becomes a checked target. A Panel candidate with authenticated but false
intermediate values is rejected independently of the Miner's result.

## Boundaries and integration requests

The branch supplies the minimal module-registration/dependency integration
delta separately for A0: `node/mod.rs`, executor `Cargo.toml`, and `Cargo.lock`.
It changes no shared schema, wire ID, signature domain, activation, Miner
generation, Panel construction, native Court state, accounting or persistence.
There are no success stubs. Incomplete work returns
`Incomplete`; a bad part permanently rejects the local candidate. `Matched` is
only a computation comparison, with no credit/payment authority. `OutputMismatch`
is likewise a checked fact for A4, not a slash operation.

`positive_check` is exported under `node` by the imported A0 registration.
`positive_contract` is exported under `positive-contract = [node, dep:...]`,
matching A3's feature name. This compilation feature is not native activation.
Tests import these actual library modules. Plan identity uses A0's
`coverage_plan_id` and `tir::exact_derivation_id`.
The shared DTOs and ML-DSA signature domains remain those of A0 r1.

`PreparedCheck::authenticate` verifies the Miner signature, rooted A0 admission
identity, and native TIR registration/job fields before deriving the plan.
The private adapter maps `job` to native job_id, `execution` to context_hash,
`program` to graph_ir_root, `input` to prompt_token_ids_hash, and execution_rules
to native cu_ruleset_id. Generation seed/temperature/token lengths are checked.
A4 must resolve the selector as `tir::greedy_selector_id`; A1 also checks it.
`authenticate_witness` verifies the separate Panel signature, borrows/validates
the exact binding and generated length before hashing/cloning, and binds the
signed execution_record and candidate_output to the actual data. It also checks
the exact common `tir::provenance_id(result)` rather than trusting Panel metadata.

The inner payload uses A0's `tir::ExactPart` and `decode_frame` unchanged: magic
`PC2R1G14`, u32 cone/trace lengths, then existing G14 cone/trace bytes (u32::MAX
is the absent trace). No local codec, derivation hash or signature domain remains.
Each A0 frame carries exactly one relation. Its plan/result/witness/case,
cursor, full byte limit and cumulative wire bytes are checked. The reservation
includes 284 outer header bytes plus 16 inner header bytes for every canonical
relation (300 total). `raw_part_budget` handles the u32 payload cap; admission
still requires the full `canonical_count * outer_part_cap` total reservation.
Context/progress identity also binds limits and prompt form, which PlanId alone
does not cover. Foreign frame/context identities cannot poison valid progress.

Untrusted packet ingress uses `TirNodeChecker::check_signed_part`, which verifies
the common whole-frame Panel author signature before ANY progress mutation.
`check_authenticated_part` accepts A0's private-field AuthenticatedEvidenceFrame
and checks author, context and cursor before the raw port. The raw NodeChecker
method remains a trusted A4/court port, never an unauthenticated network handler.
Bad signature/foreign author/altered bytes leave the honest candidate usable.
Author-signed malformed material charges its signature and frame, then rejects
the candidate; it cannot constitute a Miner refutation.

`signed_transport_quote` separately adds checked `N * 4627` signature bytes and
checks per-packet and aggregate overflow. It does not reinterpret existing
unsigned frame caps. Progress exposes charged unsigned frame bytes, signature
bytes and their checked sum. A4 must reserve native wrapper/transaction bytes,
signature-verification CPU, refused ingress and aggregate retries separately.
Relay/takeover preserves the original author signature; generating new signed
parts without that author's key is outside this annex's authorization.

Malformed admitted payloads expose WitnessRejected; unavailable adjudication or
evaluation limits expose EndedUnverified rather than claiming Panel dishonesty.
CheckedTerminal has private fields and no deserializer, and represents only a
local comparison. It grants no native credit or payment authority.

A0 retains ownership of integrating the registration delta. A3 must adopt this
full-node-commit derivation and codec and supply every logits bridge. A4 must
authorize carriers before attributing bad payloads to a Panel, reserve resources,
enforce deadlines and aggregate attempts, and own role outcomes. The older A3/A4
prototype IDs/domains are not aliases for A0 r1 and their successful tests are
not evidence of integration with this adapter. No A2--A6 implementation is copied
or treated as a completed network path in this A1 branch.

A6 must replay authenticated commitments and parts after restart. The checker
and its plan have no deserializer or restored-success constructor; the public
progress snapshot cannot skip computation. Aggregate cross-attempt budgets and
their journal remain A4/A6 responsibilities.

## Cost interpretation

Every scheduled leaf reserves one maximum encoded part and one full per-cone
evaluation allowance. All multiplication/addition is checked; overflow or an
insufficient aggregate cap rejects admission. Failed parsed/evaluated parts are
charged and terminate the candidate. The decoded-part bound includes expanded
vectors and both payload objects. It is not total Node RSS: the rooted program,
active interpreter allocations, authentication/scan CPU and external admission
storage require separate reservation/measurement. Real LLM measurements and
profile enablement remain UNKNOWN, pending A7/integration measurements.
The review identified interpreter memo/stack/context storage, range/inventory
analysis, authenticated-source maps and byte clones, logits i128 expansions,
and retained context/plan copies as additional reservations. Demand element/term
limits alone do not give a complete total-RAM quote; native activation must not
treat this DTO bound as one.

## Validation

Run from this workspace:

```sh
CARGO_TARGET_DIR=/Users/wata/.cache/misaka-positive-a1-port-target \
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 \
cargo test --locked --config 'profile.dev.package.kaspa-consensus-core.opt-level=0' \
  -p misaka-palw-tir-exec --features positive-contract --test positive_check \
  --test positive_check_hardening --test positive_contract --test court
```

Initial commit `40cbcb559`: all 10 A1 tests passed (Fixed checkpoints and Hist).
All 7 existing TIR Court regression tests passed: 22 cases, 4,056 honest leaves,
1,108 forged values, 150 logits accusations and 192 decode pins. This is TIR
Court regression coverage, not the entire G14/network test suite.
The completion candidate adds ragged 4,101-vocabulary coverage, cross-tile/row
substitution, resource-unverified handling, actual shared-port signatures,
signed false metadata, cap/context substitution and malformed frame payloads.
Final same-candidate logs are in `validation/`.
Final source `729d513ec`: 13 core + 2 hardening + 13 shared-port/packet + 7
existing Court = 35 tests pass. The same workspace's shared contract adds 17
passing tests (52 total, zero failures). Machine-readable counts, log hashes,
source hashes and check scope are in `validation/a1-completion-validation.json`.
These are A1 implementation and independently authored tests;
they do not replace A7 independent acceptance/performance testing or same-commit
Miner/Panel/Court/process integration. Native activation remains unchanged/off.

## Validation scope and remaining checks

The executor package and the three A1 test targets pass Clippy with
`--no-deps -- -D warnings`. The dependency-inclusive command remains blocked by
two unchanged consensus-core lints: `clone_on_copy` in
`palw_model_panel_one_v1.rs:92` and `nonminimal_bool` in
`palw_state_v2/chain_state.rs:3728`. Both commands' raw logs are preserved; no
lint suppression or unrelated consensus edit was introduced.

The docs checker passes in no-new-regressions mode with 43 known missing
references and zero new occurrences. This is not a clean strict-link audit.

Raw evidence in `validation/` distinguishes the superseded draft-codec run,
the unsigned A0 TIR-annex run, and the final authenticated-packet candidate.
The source-sha256 manifests bind implementation, registration and common
contract files to their named tested candidates. Final evidence/documentation
commits do not change those Rust inputs.

The A1 shared-port API is complete for the stated private initial profile.
No success placeholders remain. Native A4 admission/process integration, A5
credit/accounting, A6 recovery/journal and A7 full workload measurements remain
separate owners' integration work; they are not reported as passed here.

## A3 interoperability reference

A3 separately integrated `ef8e4e87d` and `b3a61454c` into source candidate
`b699358de`, with evidence commit `84d95e372`. Its manifest reports 75 selected
tests, including actual PanelBuilder/NodeChecker, cold G14-to-Panel replay,
author-signed frame relay, false result, bad Panel material and source guards.
This report independently checked all six listed raw-log SHA-256 hashes and
the total count; see `validation/a3-integration-reference.json`.

That candidate does not include A1's later convenience-wrapper commit
`729d513ec`. Its successful tests are a distinct source candidate's integration
evidence, not additional tests of the final A1 branch or native deployment.
No A3 implementation or test data was copied into this A1 branch.
