# A1 — Node exact computation checking

Base: `a5800d2dd`. Branch: `codex/positive-court-a1`.
Workspace: `/Users/wata/.codex/worktrees/positive-court-a1/MISAKA-testnet`.

## Owned implementation

- `src/node/positive_check.rs`: Node-derived `CoveragePlan`, conservative
  `CostEnvelope`, bounded `ExactChecker`, read-only progress and terminal
  computation comparison.
- `tests/positive_check.rs`: normal, missing/reordered/duplicate parts,
  authenticated arithmetic forgery, model/input substitution, output mismatch,
  logits bridging, sparse-program rejection, budget/heap/hostile-wire cases.

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

Shared files, module registration, wire IDs, signature domains, activation,
Miner generation, Panel witness construction, Court state, accounting and
persistence are unchanged. There are no success stubs. Incomplete work returns
`Incomplete`; a bad part permanently rejects the local candidate. `Matched` is
only a computation comparison, with no credit/payment authority. `OutputMismatch`
is likewise a checked fact for A4, not a slash operation.

A0 owns registering `positive_check` under `node/mod.rs`. Until that integration
change, the test includes the exact implementation source by path. The common
`misaka-palw-positive-contract` is being authored separately in A0's worktree;
this branch does not copy an uncommitted contract or invent plan-ID/signature
domains. A0 must fix the plan digest representation and evidence payload codec
before an adapter for `ports::NodeChecker`/`EvidenceFrame` is connected. A3 must
use the same full-node-commit profile and emit a logits bridge for each logits
tile. A4 must provide authenticated role commitments and rooted scope, reserve
resources, enforce deadlines/aggregate attempts and own final role outcomes.

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

## Validation

Run from this workspace:

```sh
CARGO_TARGET_DIR=/Users/wata/.cache/misaka-positive-a1-target \
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 \
cargo test -p misaka-palw-tir-exec --features node --test positive_check --test court
```

2026-10-31: all 10 A1 tests pass (including Fixed checkpoints and Hist tiles).
All 7 existing TIR Court regression tests pass: 22 cases, 4,056 honest leaves,
1,108 forged values, 150 logits accusations and 192 decode pins. This is TIR
Court regression coverage, not the entire G14/network test suite.
`rustfmt --check` and `git diff --check` pass. These are A1 implementation tests;
they do not replace A7 independent acceptance/performance testing or same-commit
Miner/Panel/Court/process integration. Native activation remains unchanged/off.
