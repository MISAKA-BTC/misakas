# RFC-0013: Reproducible layouts and resource-bounded model onboarding tools

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* Status: Draft, 2026-10-06. Tooling remediation and release criteria; **no consensus activation or ceiling increase**.
* Source baseline: `808baa6b9adcb029e64fffe51fcd84027f82e7c0`; the working-tree fixes accompanying this RFC are not yet a released binary or a main deployment.
* Validation record: [0013-onboarding-tool-validation.json](evidence/0013-onboarding-tool-validation.json). Real-run stage status is explicit; a test result does not certify registration or source fidelity.
* Related: [RFC04](0004-palw-model-improvement.md), [RFC05](0005-palw-ml-vm.md), [RFC11](0011-permissionless-model-and-long-context-onboarding.md), [RFC02](0002-palw-tensor-ir.md), [ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md).

## 0. 日本語での結論と範囲

今回のLlama登録作業は「CPU不足でチェーンが拒否した」という一つの問題ではない。
layoutによるcourtのサイズ超過、異なるDAAの審査、packの記録不足、独立検査のメモリ方式、
校正データの形式不一致、未完了の登録・認定を分ける必要がある。
**最終の512-context候補はlive preflightで受理可能と返ったが、登録transactionはまだ送信していない。**

RFC11にあるclose-sizing・長context・資源認定の方針は維持する。
RFC05のversioned Kernel拡張、RFC04の評価・昇格方針も変更しない。
追加するのは、**同じartifact・layout・rulesetを各ツールで受け渡し、巨大artifactでも検査を省略せず、
失敗した段階から再開できる具体的なtoolchain契約**である。
これを実装しても全モデル・任意contextの無条件登録を保証しない。必要なKernelや資源・権限・証拠が
欠ける場合は、それを正確に報告する。制限の迂回、context短縮を「元モデル対応」と呼ぶこと、
`--skip-pack-verify`による成功扱いは解決策にしない。

## 1. Evidence and its limits

### 1.1 Pinned inputs

The Llama attempt used `mradermacher/Llama-3.2-3B-Instruct-uncensored-GGUF`, revision
`186911b5658f85bfe68684b83646ba2e89169a0c`, `Llama-3.2-3B-Instruct-uncensored.Q4_K_M.gguf`.
Source bytes: `2241004096`; SHA-256:
`1a01383341be9bf04a904e168089401c3f0533b3af4089d3536d8b36e497ccd6`.
Conversion produced a 28-layer Llama TIR program and an approximately 4.73-GiB integer artifact.
Quantized source size and expanded integer artifact/working-set size are different measurements.

The following live results are retained observations from the preceding onboarding run, **not new
benchmarks executed by writing this RFC**. Every refused live candidate was rejected by preflight,
not a submitted on-chain registration. Table labels describe the requested tile sizes; layout
search can try narrower candidates. Use the actual recorded layout, not its filename, for identity.

### 1.2 Attempt-by-attempt diagnosis

| Attempt | Evidence / result | Actual reason and disposition |
| --- | --- | --- |
| Header-only GGUF architecture/preflight path | `ARCH_REFUSED`: header-only view holds no tensor data | The diagnostic requested data the header view cannot provide. Full GGUF conversion subsequently succeeded. This is not evidence that Llama semantics are unsupported. Route shape-only queries through metadata, or explicitly request the weight-backed path; do not suggest a new Kernel from this message alone. |
| Fidelity, 32 positions, reference evaluator | Interrupted after a long CPU run; artifact/statistics had been written | Local evaluation cost and an interrupted operation, not chain rejection. Typed execution with one reference cross-check subsequently completed. Preserve outputs and distinguish interrupted from failed/verified. |
| Fidelity `--stats-out` reused by pack | Legacy decimal-float warning | Fidelity wrote a plain serde float map while runtime packs require `misaka.palw.calib-stats.v1` bit-exact statistics. Re-serializing rounded decimals cannot recover original bits; regenerate exact statistics or retain the original bit-exact inputs. |
| ctx512, ordinary tile64, history64, logits4096 | Live DAA5756: root claim `100196 > 100000` bytes | Dissection root-carrier limit, not inference FLOPs. Narrowing only the logits tile does not necessarily shrink the history/root claim. |
| ctx512, ordinary64, history64, logits2048 | Live DAA5756: same root claim `100196 > 100000` | Wrong dimension was adjusted for this first blocker. |
| ctx512, ordinary32, history32, logits4096 | Live DAA5756: terminal close `16447959 > 3200000` bytes | Root-claim obstacle was passed, but the large logit head opening hit the next carrier limit. Smaller history alone is insufficient. |
| ctx511, default requested tiles | Offline sizing refusal; no retained live result | Reducing context by one did not remove the observed offline sizing-budget refusal. No claim about its live eligibility is justified. |
| ctx512, ordinary64, history128, logits4096 | Live DAA5758: root claim `149348 > 100000` | Enlarging the history tile increased the root-carrier requirement. |
| ctx512, ordinary128, history32, logits4096 | Live DAA5759: terminal close `16445703 > 3200000` | Enlarging ordinary commit tiles did not fix the oversized logit opening. |
| ctx512, ordinary64, history32, logits512 | Live DAA5760: `ADMISSION_OK` | A legal combination exists for this specific artifact/profile. This is not a registered class or proof of full advertised context support. |
| ctx128, ordinary64, history32, logits512 | Live DAA5761: `ADMISSION_OK` | Separate smaller-context profile; do not count this as success for the requested larger envelope. |
| Fresh bit-exact pack, ctx512, default 4×(32+4) conformance | Interrupted during expensive conformance | The convenience command serialized conversion and checks; interruption does not mean a false model. Resume/check stage outputs rather than converting again. |
| Fresh pack with two vectors (prefill≤4, decode2; 8 actual positions) | Build completed; independent implementation omitted above 512MiB | `ref2` required a whole expanded parameter map. Build completion is not full verification; verifier marks conformance SKIPPED when requested ref2 did not run. |
| Fresh exact-statistics artifact, ctx512, ordinary64/history32/logits512 | Live DAA5767: `ADMISSION_OK`; class `b4a2ba78…67c97b` | This is a different calibrated artifact/class from the earlier pass. Its pack lacked a declared record tying the base artifact to this wrapper. The next obstacle is tool provenance/conformance, not the earlier live court refusal. |
| First invocation of the new `pack bind-class --network testnet-12` command | Printed usage before binding | The outer CLI removed `--network` before dispatching the pack subcommand. Dispatch now preserves pack arguments; the CLI regression test and the actual large-pack binding pass. This was a tool argument-routing failure, not a class or chain rejection. |
| Registration, certification, line/market and mining | Not run to completion; no registration signed/broadcast in this attempt | Active bond/signature, stateful acceptance, certification, availability and sufficient qualified seats are separate untested conditions. No Panel was started. No chain refusal or mining success may be invented for these stages. |

### 1.3 The offline sizing result is not one unit over

[`palw_tir_admission_v1.rs`](../../consensus/core/src/palw_tir_admission_v1.rs),
`palw_tir_carried_closes_admit_form_v1`, translates `PALW_TIR_CLOSE_SIZING_OVER_CAP_V1` into
`value = work_cap.saturating_add(1)`. Thus `67108865 / 67108864` is a **censored lower-bound
indicator**, not a measured exact workload. It cannot justify increasing the cap by one or claim
that unlimited CPU would make the same protocol comparison pass.

There is also a specific ruleset mismatch:
[`TirOfflineGateV1::of`](../../misaka-palw-sdk/src/tir_layout.rs) selects the first TIR activation,
DAA2000 on this checkout. [`config/params.rs`](../../consensus/core/src/config/params.rs) arms
`palw_tir_fence2` at DAA3600. This selects the later range/demand rules used by admission; a live
DAA5767 query is therefore **not the same experiment** as default offline declaration at DAA2000.
This source fact explains why comparing those labels alone is invalid. Full parity still requires
the exact deployed build, network identity, schedule, class and block-state snapshot; no assertion
is made that every local/live difference is solely this height choice.

### 1.4 What the old pack could not reproduce

[`DeclaredClass`](../../misaka-palw-sdk/src/runtime_pack/manifest.rs) recorded context,
checkpoint and `h_tile`, but not `commit_tiles`, `state_tiles` or the effective logits scheme.
[`declared_check`](../../misaka-palw-sdk/src/runtime_pack/verify.rs) ran a new default-tile search,
with automatic logits selection, rather than replaying the exact layout. A legal explicit
logits512 layout cannot be inferred from that incomplete record. This is a code-proven gap;
not every individual variant above was run through `pack verify`.

A base artifact and a declared wrapper have different file digests (and possibly program graph
roots due to the logits scheme), even when their inventory/tokenizer are identical. A pack must
pin both, not waive digest checking. The CLI [`pack_gate.rs`](../../misaka-cli/src/pack_gate.rs)
requires **artifact and conformance PASS** before constructing a registration; conformance SKIPPED
is a pre-submission CLI blocker. Consensus does not currently require this runtime-pack policy.
Other optional SKIPPED checks are reported separately, never called full source fidelity.

## 2. Coverage by existing RFCs — no duplicate protocol redesign

| Problem | Existing coverage | RFC13 addition |
| --- | --- | --- |
| Court tile/MAC, root and terminal close bytes, sizing work | RFC11 §§1,4A,12–13: bounded sizing, legal layouts, compositional Kernel plans | Operational evidence and exact layout handoff; keep protocol caps unchanged. |
| 9B recurrent calibration/context metadata and long calibration | RFC11 §§1–4,11–13 | Shared exact-statistics format and stage identity, not a new calibration safety waiver. |
| 2M context cap, IR held rejection, inline canonical prompt cap and DA reach | RFC11 §§2,4B,12–13 | None to consensus. The Llama run did not test 2M; enough CPU does not remove these format gates. |
| Missing operators/formats, model extension | RFC05 §§K.1–K.7 and RFC11 §16; frontend for existing semantics, reviewed Kernel update otherwise | Header-only diagnostics cannot misclassify a supported weight-backed conversion as missing semantics. |
| Candidate/adapters, evaluation and promotion | RFC04, including unchanged class identity/composite bindings | Same exact pack contract for standalone classes; composite binding must name its parent before support is claimed. |
| Small probabilistic normal checks; exact dispute court | RFC11 §15, RFC07 Part V, ADR0171/0172 | Unchanged. Offline executor conformance is not normal per-claim full replay. |
| Pack exact layout, historical versus live gate, independent large-artifact checks | RFC02 runtime-pack requirements and RFC11 general parity/split-stage obligations, but no complete concrete contract for these failures | §§3–7 define it; these tool-only blockers cannot be solved merely by activating a probabilistic Kernel. |
| Bond/exposure, inclusion, reorg, qualified seats, mineability | RFC11 §§3,11–14 | Exact completion labels and evidence; no assertion that an unexecuted stage passed. |

RFC13 is a companion toolchain specification, not a replacement for RFC04/05/11. No Universal VM,
BVM/GVM, TEE trust or BFT operator authority is introduced. Versioned Kernel extensions remain
coordinated upgrades, not automatically soft forks.

## 3. Exact layout is data, not a search result to guess later

New packs MUST pin the actual layout version, context, checkpoint, history tile, **every** commit
tile and state tile, effective logits scheme, layout digest, class ID and declared file digest.
The class still uses the existing canonical Borsh/hash domains; JSON is a tool-side record only.
Search options may be recorded for provenance but cannot replace these effective values.

Verification recomputes layout digest and class ID from the pinned program/tokenizer/inventory
and recorded layout. When supplied the declared file it additionally checks the file's actual
class/layout binding. It MUST NOT run a fresh optimizer, reset tiles to defaults, substitute a
current fence, rewrite a multi-GiB artifact just to inspect its layout, or reinterpret an old class.
Identity verification and resource admission are separately labelled results.

Old manifests without the full record remain readable, but exact-layout reconstruction from a
base artifact is SKIPPED with remediation, not silently passed. Attaching an existing declared
artifact creates a **new pack/digest**, preserving the old pack and artifact. Reject unrelated
inventory, tokenizer or computation graphs, malformed layouts and bad sidecars before publishing
the new pack. Composite artifacts need their parent/section-root bindings; unsupported composite
attachment is explicitly refused, not hashed as a standalone class.

New optional fields require upgraded pack readers: old `deny_unknown_fields` readers may reject
them. This is a tool-manifest interoperability change, not a consensus fingerprint/class rehash;
do not claim that every old binary can read a newly enriched pack.

## 4. Target-ruleset admission and useful diagnostics

Provide distinct modes: historical `--at-daa N`, explicit pinned schedule offline, and live block
snapshot. Default historical output must prominently state its height and cannot conclude that a
model is impossible on today's chain. A future target is conditional, never currently accepted.
Do not fix this by silently choosing the largest scheduled future DAA.

Local and remote checks MUST report chain genesis/identity, binary/tool revision, schedule/fingerprint,
snapshot block/DAA, active fences, exact class/program/layout/artifact/tokenizer identities and scope
(static, stateful, certification). Run the common consensus gate under those same inputs. Refuse
unavailable/mismatching snapshots or report them as UNKNOWN; a cached eligibility result expires
when relevant state changes. For proof-equivalent sizing optimizations, preserve acceptance and all
cap outcomes through differential tests; actual acceptance changes require their own fenced release.

Each refusal needs stage, gate/rule, measured value and unit, cap, whether exact or censored,
commit point/read category when available, selected sizing twin, attempted effective layout,
and a concrete remediation. Search preserves the first and subsequent failures, including reasons
that prevented exploration; it cannot report a remembered widest refusal as an exact measurement of
every narrower candidate. It must bound its own attempts/time/allocations, not run unbounded searches.

## 5. Large-artifact independent conformance

Replace the whole-model `ref2` parameter map with a lazy byte source. Ref2 must independently decode
canonical program/tensor bytes and retain its own primitive arithmetic/state/commit calculation.
Sharing authenticated container I/O does not authorize copying the first evaluator's outputs or
using typed-backend results as the independent oracle. Compare logits and **all recorded commit
points at every tested position**, including prefill/history/decode and recurrent state.

There MUST be no unconditional 512MiB artifact cutoff or PASS-on-skip for a required implementation.
Report which implementations actually ran and why a required one failed/interrupted. Ref2 failures
and corrupt/missing tensors propagate as failures, not reference-only fallback. Streamed and loaded
checks reproduce the same vectors/digests; negative tests must show that forged commits fail.

Lazy whole-tensor decoding is a first tool-only step, **not a universal memory solution**:
the largest expanded tensor, simultaneous operands, activations, commit snapshots, history and
mapping residency still consume memory. The report's peak-parameter metric is not process peak RSS.
For models with one tensor larger than worker capacity, implement authenticated row/range readers
and independently specified tiled primitives preserving exact accumulation, rounding, overflow and
index order. A tensor can exceed the model's aggregate quantized source size after `i128` expansion.
Do not increase a cutoff and claim bounded memory or promise that ordinary hardware handles any model.

Workers may be provisioned remotely; their reports are content-addressed, bound to semantic/tool
versions and independently reproducible. A worker signature or cache is not correctness authority.
Finite conformance vectors exercise implementations; they are not a proof of whole-model fidelity,
not a new probabilistic Final rule, and not a reason to reduce normal-path constraint coverage.

## 6. Exact calibration interchange and resumable stages

Fidelity and pack conversion MUST emit/read the same bit-exact calibration schema. Keep legacy
decimal input explicitly labelled for diagnostics, but never promote it to exact historical input.
Bind corpus/token IDs/length, calibration context/rule, source/frontend/quant policy, math mode and
tool versions as required by RFC11; synthetic short tokens are not full-context semantic evaluation.
The fresh Llama pack's statistics differ from the earlier fidelity run. Earlier accuracy metrics
cannot be copied onto a recalibrated artifact; a missing upstream-reference comparison stays missing.

Expose independently restartable stages:

`resolve → calibrate → convert → declare → target preflight → bind pack → conformance → certify → submit → observe`.

Persist each stage's input/output hashes and status using atomic completion records. No output is
complete merely because its path exists. Resume only when all pinned inputs still match; changed
tile/scheme/DAA invalidates affected checks without recalibrating unchanged weights. Checksums, roots,
conformance vectors and admission cannot be borrowed across changed inputs. An I/O failure after
writing sidecars leaves an incomplete output, never a valid completion record. Support cancellation,
disk/memory/work budgets and progress per vector/position; do not duplicate multi-GiB artifacts per
small metadata experiment when a verified reference/layout view suffices.

## 7. Release tests and observable completion

1. Nondefault ordinary/history/logits/checkpoint layouts round-trip from base and declared artifacts;
   mutate every field, scheme, digest, tokenizer and weight binding and require failure. Old packs
   parse, report missing exact layout, and migrate without overwriting old files.
2. Loaded versus lazy ref2 produces identical bytes/errors over golden and generated programs;
   missing/corrupt sources fail. Exercise an artifact above 512MiB and then the real Llama artifact
   with requested independent implementation: no size-based SKIPPED. Report tested vectors, resource
   use and duration. Tiny fixtures alone do not establish real-size completion.
3. Match historical DAA2000 and post-fence3600/current snapshot SDK/node gate inputs; report expected
   differences, then prove parity under identical inputs. Preserve the cap+1 lower-bound label and
   tested cap−1/cap/cap+1 behavior. Do not relax court limits to satisfy a benchmark.
4. Exact calibration emit/read/materialization reproduces bytes; reject changed sidecars and forged
   context coverage. Interrupt after each stage and resume to the same roots; changed inputs force
   rechecking. Test memory, disk exhaustion and invalid output/cached-success handling.
5. Observe registration by **accepted class state**, not a preflight pass, tx hash, elapsed DAA or
   model-market entry. Record bond authorization/exposure, processor verdict, block/class/root,
   RPC listing and reorg behavior. Certify/mineable/first Final/market-open are independent outcomes.

For this operator task the Panel-start step is expressly out of scope. Missing qualified seats may
leave a safely registered class non-mineable. Do not fabricate ready status or claim that merely
creating a model position completes mining qualification. RFC11's all-HF/full-task/context coverage
denominator and evidence requirements remain unchanged.

## 8. Implementation boundary of the accompanying patch

The real `5079273984`-byte Llama artifact completed streamed conformance: **2 vectors, 8 actual
positions**, identical logits and all commits on reference, typed-backend and independent ref2.
`/usr/bin/time -l` reported 1228.90 seconds and maximum RSS 12828917760 bytes (about 11.95GiB)
on the 24GiB Mac, with zero swaps. The seeded `prefill=4` option is a maximum prompt length,
not a claim that every vector has four prompt tokens. This evidence covers only these actual
positions, not full 512-context fidelity or all models. Manifest, sidecars, source, frontend,
artifact and exact declared identity also passed. **HF reference is SKIPPED** for this newly
calibrated pack; the report is `ok=true`, `verified=false`, not full source-fidelity certification.
The real run predates the progress/log-message-only additions; its binary hash is pinned in
the evidence record. Current logging is tested separately on the small regression fixture.

The accompanying tool changes add full layout records, direct identity verification, a non-overwriting
`pack bind-class` path, lazy independent parameter decoding, per-position streamed verification
progress on stderr, and exact fidelity statistics output. Progress is not a resumable completion record.
They do not implement all of §§4–7: explicit target-snapshot/offline parity, row/range ref2,
durable per-position resume, complete calibration provenance, registration/certification and broad
coverage remain distinct acceptance work. Test/real-run results belong in the evidence report;
neither this text nor a passing build proves those unfinished outcomes.

## Mission alignment amendment — 2026-10-07

historical/live gate分離、exact layout、独立streamed conformanceは維持する。registration/PASS/Finalのrelease reportに、その実際のprofileで外部public bondが証拠を公開取得しlocalizeしてconvictできるかを別項目で報告する。preloaded producer captureや既知fault indexを渡すfixtureは独立prosecutionの成功証拠にしない。toolのメモリ削減で裁定の最大bytes/workを隠さない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。
