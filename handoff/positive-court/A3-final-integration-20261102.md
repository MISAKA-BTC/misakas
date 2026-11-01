# A3 Panel — final integrated handoff, 2026-11-02

対象commit: Root integration `4449ad442f749fce12e66f53fe971580018153b6`, whose final code is `7a0ffcc7fb318fdf025d60beeb45f4b411539eec`. A3's old `codex/a3-loaded-stdio-join-v2` at `87017a0041693fbf3429a887a26a69b2d777fbcb` is preserved. The dedicated workspace now uses `codex/a3-final-positive-court-20261102`. This handoff adds documentation and evidence only; the integrated Rust implementation is retained without an overlay.

## 変更した責務・ファイル

A3 implements the evidence-producing side: authenticated material acquisition, independent replay, bounded positive witness/record construction, signed submission, authenticated resume and original-author handoff. A1 independently decides whether the supplied coverage and arithmetic match. A4 owns court admission/state, A5 owns credit/accounting, and A6 owns persistence/reorg/recovery.

The delivered implementation includes `node::positive_panel`, `remote::positive_panel`, `remote::positive_panel_sources`, the typed wide run/capture and `positive_wide_material_v2` producer, the Source-local audited ownership facade, bounded original artifact header, `node::owned_artifact_params_v2`, the opaque compile plan, `loaded_wide_artifact_v2` and its private Panel bridge. `tests/loaded_wide_process_v2.rs` supplies the real stdin/stdout parent/child join.

The five bridge files from `973ed058c494caeb759a8049970b47b17b9e558c` and stdio test from `9d828c26503349d1f77c6a24e79a36d2dc031454` are unchanged in the final integrated source. Root's ordinary-forward/shared parameter helpers are retained. The historical source-only reports remain unchanged and their original UNRUN labels describe their freeze dates; the runtime records below supersede those labels for the specific tested sources.

## 接続したAPI

- Existing A2 responder/provider and canonical source-duty interfaces provide authenticated original artifact/input material to the Panel source cache.
- A0's common signed-evidence, bounded-record and budget contracts feed the actual A1 independent stream checker through its required relations and terminal EOF.
- The private Loaded bridge borrows its opaque compiled plan and native-width Params and calls existing `capture_one_pass_v2` exactly once. It preserves the shared work/IO/disk counters and reservation lifetimes; retries do not reset spent counters or create another production replay.
- Loaded/stdio/Panel APIs still require a **full existing `PalwTirStepBindingV1`**, opaque verified V2 binding and original registration/job/tokenizer/context pins. The bridge adds no trace-free first-Miner doorway or replacement zero roots.

## 実行した試験と結果

These rows retain their individual sources and scopes. They are not one additive final-commit test count.

| Source | Observed result | Scope |
| --- | --- | --- |
| `3a55034152880c9fc39feab54a8a679aec376f91` | Core54 / Exec71 / SDK19 selected outer cases passed | Includes Params4, compile3, Loaded6 and Panel11 including the two Loaded bridge cases. The overall manifest remains INCOMPLETE because a later Node build hit its disk floor. |
| Same `3a55` | Actual OS-main stdio join1 passed, separately | Real regular File -> Loaded -> CPU -> stdin/stdout, two Open/Step/Close cycles; not a libtest/default-stack case. |
| `50fe43e4c9ded609b1be9ab51125caa33ffffd8b` | Core54 and Native19 + trained registration/replay/DB recovery3 passed | Separate native regression evidence; nested Source child helper is not counted again. |
| `a867e78e05f8fdfb6586c21c8ddbf64f76dd159b` | 39 unique regressions passed | Original34 + A7 recovery5 at the same source/snapshot; original resource-stop INCOMPLETE record retained. |
| Same `a867` | Three actual trained-model ordinary/dense comparisons passed | SmolLM2-360M/135M retain maximum-context metadata8192; Pythia-70M retains2048. Actual jobs are16 input /1 generated token. |
| `7a0ffcc7fb318fdf025d60beeb45f4b411539eec` | Strict Clippy4 commands /8 selected targets passed | Metadata checks only, no runtime test/model count. |

The stdio raw marker records `profile=audited-source-v1 actual_file=1 loaded=1 cpu_i64_endpoints=1 cycles=2 client_steps=2 server_steps=2 params_read=26 child_guard_released=1 reaped=1 stack=os-main`. Bounded Take EOF and direct-child reap are checked; natural child exit0, kernel-pipe EOF and bounded descendant cleanup are not asserted.

The 39-case independent audit checks all4,956 source blobs, exact disjoint test names, closed raw/list/build records and Cargo-linked binary identities. The trained3 audit checks its three closed executions and source linkage without rerunning a model. Ordinary/dense ID/hash/context equality is enforced in the caller before its ordinary-only raw report is emitted; this is local same-owned-data consistency, not an independent numerical oracle or execution at every maximum-context position.

After the a867 executions, `bf3e523445a6de3b70278c5ad5e1a7609419f416` only changes the optional-layout nested predicate into a short-circuit let-chain. Final7a0 additionally changes a CPU-limit **test** constant conversion to a checked conversion. No a867 runtime result is relabelled as a 7a0 execution. Clippy frontend Source-profile observations remain metadata observations and do not substitute for the supported runtime profile.

See the Root [ordinary-forward results and exact manifests](../../docs/evidence/positive-court-a0-2026-10-31/integration/ordinary-local-forward-a867-results-20261102.md), [native registration/replay/recovery report](../../docs/evidence/positive-court-a0-2026-10-31/integration/trained-native-registration-50fe-results-20261102.md), [original selected-suite manifest](../../docs/evidence/positive-court-a0-2026-10-31/integration/native-validation-3a55-all-20261102.json), [stdio raw log](../../docs/evidence/positive-court-a0-2026-10-31/integration/native-validation-3a55-all-20261102-loaded_wide_stdio_os_main_1.log) and [final Clippy manifest](../../docs/evidence/positive-court-a0-2026-10-31/integration/plain-clippy4-7a0-20261102.json). A3's final independent audits and import inventory are preserved under `A3-r1-evidence/final-integration-20261102/`.

## 未接続・未検証の箇所 / 残っている仮実装

A3's delivered path has no placeholder success, zero-root binding bypass, fallback acceptance or refunded retry counter. Synthetic fixtures remain labelled; their data associations are not native permissions. Native Assignment/Result/role/funding authority, drawn-validator approval, court attribution and payment are other owners' boundaries. This bridge does not grant them or declare full native/public end-to-end acceptance.

Actual model loading/short-job replay and its original metadata do not constitute model review or a model-tier promotion. No model-tier rule is changed. Requested-allocation ledger figures and sampled process RSS do not establish a physical-memory bound or full-context performance result. Prior failed stack/build/profile stages and their raw records remain preserved.

## 他担当に必要な変更 / 有効化設定

No further Rust change is required to import this A3 handoff. The import contains only67 previously absent A3-owned historical documentation/evidence paths, an additive patch-whitespace attribute, this final report and its independent audit snapshots. Root's bounded-header result document, A0 status, current evidence and all other-owner files are retained.

有効化設定: **変更なし / public presets disabled**. Local replay agreement and authenticated signatures do not replace claim-level approval or establish computation credit. No cryptographic computation proof or model review mechanism is introduced.
