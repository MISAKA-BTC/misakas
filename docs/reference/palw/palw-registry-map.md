# Where the model registry is — the PALW provenance map

> **PALW共通前提 — 2026-10-10:** [旧ADR-0176](../../rfc/0012-bonds-budgets-and-slashing.md#8-claim予約の分解と期間上限)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [旧ADR-0177](../../rfc/0013-model-work-accounting.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**Read this before proposing a provenance layer.** Every question of the form *"where is the model
registry / the artifact hash / the runtime registry / the receipt / the output root / the
verification / the metering / the capability profile?"* has an answer in this tree, and the answer
is a **field, a struct or a function**, not a plan to add one. This page is that answer, one row
per layer, with the file the row lives in and the ADR that decided it.

It exists because the alternative has a price this repository has already paid: a second spelling
of a field that already exists (`derived-sets-need-one-spelling`). 旧ADR-0079 §7 disposed of a
proposal to add ten provenance layers by showing that nine of them were already here under other
names, and unit **R-09** is the instruction to write that mapping down where a reader will find it.
旧ADR-0079 §7 is the argument; this page is the index.

Two conventions, both load-bearing:

* **`consensus/core/src/…` means the chain sees it.** Anything under `misaka-palw-gateway`,
  `misaka-palw-agent`, `misaka/palw/base0/src/bin` or `misaka-cli` is executor-side: it can change
  without a fork, and it can never be evidence.
* **A refusal is a row too.** Four proposed layers are refused by name, and each refusal has a
  reason that was paid for once. They are listed with the rest so they do not get re-proposed.

---

## 1. The provenance chain — model → runtime → input → execution → receipt → output

| layer | the field / struct / function that IS it | file | decided by |
|---|---|---|---|
| **① Model registry** | `PalwClassRowV2` / `PalwClassStateV2` — the class **is** the registry row. Its identity is `class_id == profile.shape_profile_id() == H(profile)`: the architecture, the geometry, the quantization map and the context length hash to the id, so a model that differs in any of them is a different class by construction. Admission is permissionless and on-chain (`PalwClassAdmissionCarriageV2`, whose `shape_profile_id()` must equal the registration's `class_id`). | `consensus/core/src/palw_state_v2.rs` (rows), `consensus/core/src/palw_step.rs::shape_profile_id` | 旧ADR-0056, 旧ADR-0067 D1–D2, 旧ADR-0075 |
| **② Model artifact hash** | `artifact_root` on the registration and on every attempt (`PalwAttemptUnsignedV2`, `PalwClassStateV2::artifact_root`) — *"what artifact openings prove against; an attempt whose `artifact_root` differs is"* not this class's. The worker recomputes SHA-256 **from the bytes** on every load and compares against the pin. | `consensus/core/src/palw_state_v2.rs`, `consensus/core/src/palw_attempt_v2.rs`; gate: `misaka/palw/worker/src/main.rs::pinned_model_path_v2` | 旧ADR-0067 D6, 旧ADR-0079 D9 |
| — *the registered-class arm of ②* | `resolve_chain_registered` step 4: a holding is admitted only when its **computed** digest equals the registered root — the same rule for a class this build has no compiled row for. | `misaka/palw/sdk/src/sdk.rs::resolve_chain_registered` | 旧ADR-0067 D5, 旧ADR-0079 D9 |
| **③ Runtime / engine registry** | `PalwRuntimeManifestV2` — worker self-hash, patchset root, `libm_arithmetic_digest`, golden-vector root — plus 旧ADR-0075 Decision 5's kernel-coverage binding (`PalwClassLaneCertificationV2`). There is **one** engine by decision, so this names a *build*, never a choice of engine. | `misaka/palw/worker/src/main.rs::runtime_manifest_v2`; `consensus/core/src/palw_state_v2.rs::PalwClassLaneCertificationV2` | 旧ADR-0053, 旧ADR-0067, 旧ADR-0075 |
| **④ Input canonicalization** | `prompt_token_ids_hash_v2` (consensus binds the **ids**, never the text) and `fp_job_id_v3` (the job a replayer rebuilds from chain data alone). The template is executor-side and frozen: `TEMPLATE_ID_V1` renders plain-text markers; the segment-wise form (`PalwFpPromptSegmentV1`: `Special(u32)` from the worker manifest, `Text(Vec<u8>)` encoded with specials disabled) is the 旧ADR-0077 Decision 6 arm. | `consensus/core/src/palw_v2.rs::prompt_token_ids_hash_v2`, `consensus/core/src/palw_freeprompt_v3.rs::{fp_job_id_v3, PalwFpPromptSegmentV1}`, `misaka/palw/gateway/src/main.rs::TEMPLATE_ID_V1` | 旧ADR-0044 D10, 旧ADR-0077 D6, 旧ADR-0079 D7 + SA-6 |
| **⑤ Inference receipt** | `execution_commitment_v3(attempt, execution_anchor)` — the commitment over the whole attempt; the claim and the receipt block carry it. | `consensus/core/src/palw_attempt_v2.rs::execution_commitment_v3` | 旧ADR-0072, 旧ADR-0073, 旧ADR-0074 |
| **⑥ Output root** | `output_commitment_v2(job_context_hash, generated_token_ids, rendered_output_hash)` — over exactly the emitted ids, bound to the job context that produced them. 旧ADR-0078's derived artifacts are the layer above it. | `consensus/core/src/palw_v2.rs::output_commitment_v2` | 旧ADR-0044, 旧ADR-0078 D2 |
| **⑦ Deterministic verification** | The court: checkpoint-anchored replay, the seat's `k` sampled intervals compared **exactly**, and bisection to one leaf. Openings prove against the class root; a leaf is either the accused's or it is not. | `consensus/core/src/palw_court_v2.rs`, `palw_step.rs`, `palw_step_leg.rs`, `palw_step_refute.rs`; seat side: `kaspad/src/palw_panel.rs` | 旧ADR-0069, 旧ADR-0077 D8 |
| **⑧ Resource / cost metering** | Leaves and pwu — an integer count of executed work. `PALW_STEP_MAX_LEAVES` bounds the answer, `work_leaves` counts the work, `palw_expected_attempts_v1` turns a class target into expected attempts, and the per-class epoch budget is denominated in **pwu, never in ramped weight**. Never a clock and never a watt. | `consensus/core/src/palw_step.rs::PALW_STEP_MAX_LEAVES`, `palw_pwu.rs`, `palw_class_daa.rs` | 旧ADR-0045, 旧ADR-0071, 旧ADR-0078 D10 |
| **⑨ Capability / hardware profile** | 旧ADR-0067 Decision 6's four storage tiers, resolved by the SDK per family (`resolve_*`, the mmap tier for the 33 GiB hybrid). Registration and possession are different acts and nothing may couple them: the registration carries kilobytes and no URL. | `misaka/palw/sdk/src/sdk.rs` | 旧ADR-0067 D6 |
| **⑩ Artifact store / delivery** | For **outputs**: the gateway returns the answer and the derived artifact to the user; the chain holds the derivation, and the DA obligation covers the trace, not the thing. For **models**: refused — see §3. | `misaka/palw/gateway/src/main.rs`, `misaka/palw/gateway/src/derive.rs` | 旧ADR-0077 D3–D4, 旧ADR-0078 D1 & D6 |

## 2. The host half — what 旧ADR-0079 added, and where it lives

None of it is a chain fact. That is Decision 2: the chain cannot observe whether a host ran
confined, so a confinement claim on the chain would be a vote, and a vote is what a court is not.

| layer | the thing that IS it | file | decided by |
|---|---|---|---|
| **Sandbox / capability control** | `ConfinementBackend` and `establish_confinement` — what is **actually in force**, never what was configured; `harden_worker_command` (the `env_clear()` + `PALW_WORKER_ENV_ALLOWLIST` spawn, `PATH` deliberately absent); `worker_working_dir` (an explicit `0700` scratch dir, never `$HOME` or the datadir). | `misaka/palw/security/src/lib.rs` (re-exported as `misaka_palw::host_security`) | 旧ADR-0079 D1, D5, SA-4 |
| **Resource ceiling** | `PALW_WORKER_MAX_RESIDENT_BYTES`, `arm_memory_ceiling`, `attach_to_cgroup`, `resident_bytes` — cgroup v2 `memory.max` where available, a supervisor resident watchdog otherwise. **Not `RLIMIT_AS`**: the hybrid maps 33 GiB. Exceeding it is a `JobFailed`, never a dead node. | `misaka/palw/security/src/lib.rs` (re-exported as `misaka_palw::host_security`); enforcement: `misaka/palw/agent/src/agent.rs::run_worker_job` | 旧ADR-0079 D6, SA-1 |
| **Secrets isolation** | `reachable_signing_secrets` — the gateway refuses to boot when a signing secret is reachable in its own view; it holds the executor **public** key only, and the ML-DSA-87 signature belongs to the signer sidecar. | `misaka/palw/security/src/lib.rs` (re-exported as `misaka_palw::host_security`), `misaka/palw/gateway/src/main.rs` | 旧ADR-0079 D4, D8, SA-2 |
| **Network egress control** | `listen_is_loopback` / `check_loopback_bind` — every non-loopback bind fails at startup. Public gateway opt-in is removed (旧ADR-0144). Plus the platform backend's own denial of `socket`/`connect` for the worker. | `misaka/palw/security/src/lib.rs` (re-exported as `misaka_palw::host_security`), `misaka/palw/gateway/src/main.rs` | 旧ADR-0079 D5, D10 |
| **Tool / agent permission system** | Decision 8, in the only honest form available: **the model has no tools.** A worker returns token ids; nothing in the tree executes, fetches or shells out on the strength of model output, and the signer signs one message shape — an id it re-derives itself. A policy engine is deferred until something exists to police. | (a prohibition; guarded by `misaka/palw/core/tests/host_security_tree_guard.rs`) | 旧ADR-0079 D8, S10 |
| **Prompt-injection boundary** | The lexical one, and only that one: untrusted text may not become a control token. The corpus that pins it is `misaka/palw/base0/tests/corpus/special-tokens.txt`. **The protocol does not and cannot promise that a model ignores instructions inside its own context** — obedience is not a consensus property, and Decision 8 is the structural answer instead. | `misaka/palw/base0/tests/special_token_corpus.rs` | 旧ADR-0079 D7, S7, SA-6 |
| **Security / integrity receipt** | The integrity half is ②'s full read. The posture half is `misaka node security-report` — printed from live state, **signed by nobody**, committed nowhere; it reports the backend in force, the *measured* worker environment, the live listeners, the interpreter fence read off the running node's argv, and the artifact digests. It prints paths and posture, never key material and never prompts. | `misaka/apps/cli/src/security.rs` (`misaka node security-report`) | 旧ADR-0079 D2, D13, S12, SA-7 |

## 3. The refusals, with the reason each was paid for

A refusal without a reason gets re-proposed, so each keeps its argument next to it.

| proposed | why it is refused | decided by |
|---|---|---|
| A **multi-engine runtime registry** (llama.cpp / vLLM / MLX / TensorRT rows) | One execution family was decided: pure Rust, in the tree, integer. A float path is a path two honest hosts disagree on, and a llama.cpp worker cannot produce a commitment at all — it has no step legs. The registry that exists (③) names a *build*. Other engines are non-consensus conveniences and can never sign a commitment. | 旧ADR-0053, 旧ADR-0079 §7 item 3 |
| A **tolerance-based receipt verifier** (RBO / p95 "more realistic" attestation) | A tolerant verifier cannot convict, and a check that cannot convict is a vote. This decision was taken against a measured competitor's design. The seat's sampled interval is a *licence*, not a verdict, and it compares exactly. | 旧ADR-0026, 旧ADR-0079 §7 item 7B |
| **Metering in GPU-seconds, watts or wall clock** | A clock is not adjudicable. Work is metered in leaves and pwu (⑧). Token counts are carried; seconds and watts are operator telemetry and never chain facts. | 旧ADR-0078 D10, 旧ADR-0079 §7 item 8 |
| A **job scheduler** between the user and the worker | Jobs are self-originated and the attempt is a claim the chain draws; there is no orderer, and inserting one would create a party that chooses who earns. The user chooses an executor; the chain chooses the ticket. | 旧ADR-0074, 旧ADR-0079 §7 |
| A **chain-side model distribution network** | The registration carries kilobytes and no URL, precisely so a fleet's disk does not grow with strangers' decisions. Model bytes are obtained out of band and admitted only by digest (②). | 旧ADR-0067 D6, 旧ADR-0079 §7 item 10 |
| A **`security_policy_hash` inside the priced/committed bytes** | A posture the chain cannot verify, committed into bytes that price a lottery, is both an unfalsifiable claim and a grinding surface — every free field inside the priced bytes is a free draw. The exhaustive field-classification test refuses to compile one. | 旧ADR-0072 D8, 旧ADR-0079 D2, S1 |

## 4. If you are about to add a field

1. Find the row above that already carries the meaning. If one does, extend it — do not spell it a
   second time.
2. If the field would live inside priced, committed or certified bytes, it must be **pinned by an
   equation**, not free: 旧ADR-0072 Decision 8, and the field-classification test that enumerates
   every one of them.
3. If it changes what a node accepts or rejects, or what the state root hashes, it rides a fence:
   a new top-level `Params` field `Option<ForkActivation>`, `None` on every shipped preset,
   classified in `for_each_fence` (`consensus/core/src/config/params.rs` — an exhaustive
   destructure, so the compiler forces the classification).
4. If it is about the **host** and not the chain, it belongs in `misaka/palw/security/src/lib.rs` (re-exported as `misaka_palw::host_security`)
   and in the report, and nowhere near a commitment. That is the whole of 旧ADR-0079 Decision 2.

---

**Sources.** 旧ADR-0079 §1 and §7
(`docs/rfc/0003-jobs-tokenizers-and-generation.md`) are the
argument this page indexes; `docs/adr/README.md` is the ADR index and says which decisions were
later moved; `SECURITY.md` §3 is the operator-facing statement of the host half.
