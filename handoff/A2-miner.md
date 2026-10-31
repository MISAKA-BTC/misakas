# A2 — Miner lightweight path

Workspace: `/Users/wata/.codex/worktrees/positive-court-a2-miner/MISAKA-testnet`
Branch: `codex/positive-court-a2-miner`
Initial base: `a5800d2dd5d334e89f7a1ddb6e956aedcbf4adf5`
Common base: A0 commit `a85152edc`, `misaka-palw-positive-contract`, version 2 / revision 1.

## Implemented responsibility

`misaka/palw/remote/src/positive_miner_v2.rs` is exported from the existing Miner entry point as `misaka_palw_remote::miner::positive_v2`, behind the default-off Cargo feature `positive-miner`. This is a local process build option; it is not a native activation fence.

- `infer_tokens_v2`: ordinary fixed-length greedy `TirExecutor` inference using `NoSink`; retains generated IDs, normal inference state, and no Positive trace/capture/evidence tree. Both i32 and i64 logits use the first maximal lane. Input and position bounds precede execution.
- `sign_result_v2`: signs the complete A0 `MinerResultCommit` with A0's existing r1 context. Checks admitted scope/class, finite `MinerServiceContract`, signer identity, returned signature, and deadline. No new wire tag or signature domain.
- `MinerResponderV2<D>` implements A0 `MinerSource`. `respond` receives only an authenticated canonical duty, whose exact `SourceRequest` cannot be independently substituted. `D: CanonicalDataDutyV2` is the explicit A4 integration port; there is no permissive native duty implementation.
- Existing G14 artifact inventory openings, tiled prompt openings, and whole flat prompts are authenticated before cloning an outbound response. No alternate 4-KiB source tree is introduced. `StoredProviderV2` reads already stored data/openings and receives no signing key.
- All Panels share one case session and canonical unit union. Already Node-accepted direct/provider units require no provider reads or re-preparation. Overlap does not recharge unique data. An explicit retry of a still-pending unit spends preparation bytes, provider reads, and read/hash work again.
- Local request/metadata/provider-read/body-byte limits and the Miner's signed aggregate caps bound preparation. Oversized fields are refused before serialization/hash walks. Failed reads and bad providers consume the finite read/work budget. An unsuccessful batch preserves consumed counters; it cannot silently reset them.

A prepared response is neither Node-accepted delivery nor inference checking, payment, credit, a Miner default, or a Court verdict. Provider failure and Panel abandonment cannot be converted into Miner computation fraud by this module.

## Integration required from other owners

- **A0:** review/merge the small existing-Miner export and remote dependencies. The shared crate and workspace registration are consumed unchanged from A0. Do not alias the earlier `palw_positive_court_v2` prototype domains/4-KiB service ledger to r1.
- **A4:** implement `CanonicalDataDutyV2` over current rooted admission/request state: exact result/case/contract/source catalog, request authentication and Panel scope, canonical unit geometry, resource/funding consent, exclusive cutoff and service opportunity, current branch, and per-unit acceptance. Feed each returned existing G14 unit through actual native intake. It must measure all outer framing/carrier/retry bytes, update the single all-Panel union, and account for actual fees. A peer-supplied DTO or private ACK cannot construct this capability. No native duty adapter is supplied here.
- **A3:** bind the admitted TIR job/registered plan and generated output to the r1 result, collect source units via the A4 duty, and build its own witness. A2 never generates or endorses that witness. The inference and signing APIs are separate until that admitted-job bridge exists.
- **A6:** retain/replay the preparation meter and unit union with the canonical case journal before continuing after restart/reorg. `new` starts a process session; it is not a recovery constructor and must not reset a live case's budgets.
- **A7:** run the actual integrated candidate (warm/cold, withheld ACK, false witness, Panel abandonment, cross-process providers and restarts). The in-memory A4 fixture in A2's tests is not production admission, a native atomic transaction, or an independent A7 result.

## Validation scope

Tests use real ML-DSA signatures, existing G14 Merkle/opening authentication and a small real `TirExecutor` program. The canonical A4 duty in those tests is explicitly an in-memory fixture. No RPC/native carrier or complete Miner→Panel→Node court is claimed.

Raw logs and final command/result record: `handoff/A2-evidence/`.
Real LLM cold/warm CPU/GPU/upload/RSS/retention overhead: **UNKNOWN** (not measured).
Public-network activation: **unchanged / disabled**. No consensus reward, Gate C, court status, fee settlement or network preset is modified.

## Completed checks

- Feature `positive-miner`: **190 passed, 0 failed, 1 existing ignored**, including **14 A2 tests**.
- Default-off build: **176 passed, 0 failed, 1 existing ignored**.
- Existing ignored test: `l2::tests::l2fc_cost_per_view`.
- Rustfmt on both new source files and source-only `git diff a85152edc --check -- Cargo.lock misaka/palw/remote`: **PASS**. Raw test logs retain their terminal blank lines verbatim; a whole-diff whitespace check reports those two log-only EOF lines.
- Exact reproducible commands, tested source digests and raw logs: `A2-evidence/validation.json`, `A2-evidence/with-positive-miner.log`, `A2-evidence/default-off.log`.

The integration commit should cherry-pick only A2's contribution on top of A0 `a85152edc`. No changes to the shared A0 contract are required by this contribution. The native/durability integrations listed above remain owned by A4/A6 and are unverified here.

Production success stubs: **none**. The only simulated duty implementation is under `#[cfg(test)]`; production admission is unavailable until A4 implements the explicit port.
