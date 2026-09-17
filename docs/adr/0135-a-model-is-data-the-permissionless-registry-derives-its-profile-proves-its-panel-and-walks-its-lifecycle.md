# ADR-0135 — A model is data: the permissionless registry derives its profile, proves its panel, and walks its lifecycle

* Status: **PROPOSED 2026-09-17; the rule set built in shadow the same day** (`palw_model_registry_v1.rs`). No
  consensus rule, parameter or fingerprint moves; this is the content of **Protocol Upgrade A** (§6).
* Operator's direction, in the operator's words: "誰でもモデルを chain へ登録でき、検証時間・必要 Panel 数・claim 上限・
  報酬係数までプロトコルが自動計算し、条件を満たしたら自動 activate"; "モデルをコードではなくデータとして追加する";
  "実測値を consensus parameter にしない"; "Panel 側も permissionless — 自己申告だけではダメ"; "モデル登録から
  activation まで全部 state machine"; "share も最終的には手動設定をなくしたい"; "既存 Canonical ML VM で表現可能 →
  permissionless、新しい opcode が必要 → protocol upgrade".
* Supersedes, in ADR-0133: Decision 2's *measured* p99 as an input to the profile (measurements become telemetry),
  §9a's "profile freeze by the operator" and "tiny activation → ramp by the operator" (the lifecycle does both).
  Keeps everything else of ADR-0133 (three clocks, Little's law, class-local fail-closed, the artifact options).
* Builds on: [0133](0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md),
  [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md),
  [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) (economic compute from the
  graph), [0049](0049-palw-adjudication-contract.md) (the canonical IR the VM boundary is drawn on), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  and [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) (the on-chain class registration and carriage this extends),
  [0056](0056-palw-permissionless-class-admission-and-share-economy.md) (class admission is permissionless already; this ADR removes the share it kept), [0071](0071-the-attempt-lanes-price-and-the-tickets-bound.md) (who may judge a class).

## 0. The sentence this ADR is

**A model is registered as data — a graph root, an artifact root and its bytes, a canonical job, a quantization,
a runtime version, and a bond — and from that data every node derives the same verification window, prefetch,
inflight cap, required ready seats, registration bond and claim cadence; seats count only when they prove the
artifact, their participation, their collateral and a recent verification; the class walks REGISTERED →
PREFETCHING → PROBATION → ACTIVE_LIMITED → ACTIVE on chain-visible facts and falls to HELD alone; and nothing —
not a window, not a rate, not a share — is ever stated by a registrant, measured by an operator or committed to
a source tree.**

## 1. What today's path still needs a human for, and what stays a human's

Today (ADR-0133 §9a): shadow measurement → the operator freezes a profile → a fence in `main` → a share set by
hand → a release. Each step is a fork or a decision per model, and the p99 the operator would freeze is the
measuring host's (an M4 200 s, an H100 50 s, a slow disk 500 s). This ADR removes every one of them **for a model
the canonical ML VM already expresses** (matmul, attention, the mixture router and experts, the gated-delta
recurrence, norms, rotations, the LM head — ADR-0049's IR at the class's `runtime_version`). What stays a
protocol upgrade: a model that needs an instruction the VM lacks. That is not a model registration but a VM
instruction-set change, and `palw_manifest_verdict_v1` says so (`UnsupportedOp`) rather than letting a node
guess at an op's meaning.

## 2. Decisions

**Decision 1 — the manifest.** `PalwModelManifestV1 { graph_ir_root, artifact_root, artifact_bytes,
canonical_prefill_tokens, canonical_decode_tokens, quantization_format, runtime_version }` with a registration
bond. It is today's on-chain class registration (ADR-0067's carriage: the profile IR and the canonical job)
plus the artifact's bytes — and nothing a human decides: no window, no reward, no share. A manifest is judged
before it is a class: `Valid`, `UnsupportedOp` (the VM boundary), `EmptyArtifact`, `EmptyJob`, `ZeroWork`.

**Decision 2 — the work, derived.** From the graph, every node computes `PalwModelWorkV1 { verification_ccu
(ADR-0131's economic compute of the job a seat replays), economic_ccu_per_claim (the same over the class's
expected draws, ADR-0132), artifact_bytes, working_set_bytes, ops_supported }`. Dense MACs, attention over the
kv length, the recurrence, the active experts, the logits and the elementwise work are counted by the one
versioned cost table; two nodes with one graph get one number.

**Decision 3 — the profile, derived, never measured.** Against `PalwRegistryGlobalsV1` — one set of reference
constants for every class, changed only by a fence:

* `verification_window_spans = ⌈safety × verification_ccu / reference_work_per_span⌉ + receipt_allowance_spans`
* `artifact_prefetch_spans = ⌈io_safety × artifact_bytes / reference_bytes_per_span⌉`
* `required_ready_seats = max(seat_count + spare_seats, ⌈seat_count × verification_ccu / (utilization ×
  reference_work_per_span)⌉)` — the panel plus the spare, or more where one claim a span already needs more
  replay time than that many seats offer at the target utilization
* `max_inflight_claims = ⌊utilization × required_ready_seats × window × reference_work_per_span / (seat_count ×
  verification_ccu)⌋` — Little's law
* `registration_bond = bond_per_span × verification_window_spans` — the burden a class puts on panels is what
  its registrant bonds
* `admission_claims_per_span = min(budget_ccu_per_span / economic_ccu_per_claim, max_inflight / window)` —
  Decision 5

The fleet's reference (ADR-0133): 4 G MAC-eq/s (2.4 T a span), 1 GB/s (600 GB a span), ×2 and ×2, one span of
receipt allowance, five seats plus two spare, 70 %. On it the dense tier and the hybrid derive two spans (one of
verification, one of allowance), one span of prefetch, seven ready seats; a Kimi-class of 1 T MAC-eq and 300 GiB
derives two spans, two of prefetch, seven seats; a 10 T MAC-eq graph derives ten spans and more than seven seats
— all from the same code, none from a decision. **Measured p95/p99 are telemetry**: op 185 keeps printing them
so the fleet can see whether the derived window is being met; they enter no rule.

**Decision 4 — readiness is evidence.** `PalwReadinessEvidenceV1 { artifact_root_matches, all_chunks_held,
participation_ok, free_collateral_sompi, last_probe_ok_span }`; a seat is ready for a class only with the root
matched, every chunk held (a possession proof over the manifest's chunks), its node participating (synced, not
held), free collateral for `readiness_collateral_multiple` seat exposures, and a canonical probe verified within
`readiness_probe_max_age_spans`. A declaration alone counts for nothing (the failure ADR-0132 found: every
genesis bond "declared" the hybrid and one could run it). A seat that says `Incapable` is simply not ready and
is not dealt the class until its evidence says otherwise.

**Decision 5 — no share.** A class's claim cadence is `budget_ccu_per_span / economic_ccu_per_claim`, capped by
what its window can hold: a heavy model claims rarely and is paid much a claim, a light one often and little, and
the network's compute budget a span is what is constant. Under the single lottery (ADR-0132 S, ADR-0133 §9a Fence
2) each class's share of draws is its admission over every class's (`palw_class_shares_from_admission_v1`); a
new class takes its slice from all in proportion, a class with zero admission holds none. The reward stays
ADR-0132's `f(EconomicAttempted, GlobalRate)` — no model's name enters it — and a per-claim escrow cap is not
adjusted per model: cadence is what moves.

**Decision 6 — the lifecycle.**

```text
REGISTERED ──manifest valid──▶ PREFETCHING ──ready seats ≥ required, collateral ok──▶ PROBATION
PROBATION ──probation_claims probes passed, none failed──▶ ACTIVE_LIMITED (a tenth of admission)
ACTIVE_LIMITED ──stable_epochs spans at target, nothing held──▶ ACTIVE (full admission)
PROBATION / ACTIVE_LIMITED / ACTIVE ──panel not drawable (ready < seat_count) or utilization ≥ 1──▶ HELD
HELD ──ready seats ≥ required, utilization < 1──▶ PROBATION (never straight back to ACTIVE)
```

Every arrow is `palw_lifecycle_step_v1` over `PalwLifecycleObservationV1` (ready seats, probes passed and
failed, utilization, collateral, span stability) — chain-visible facts at a span boundary. `HELD` holds the
class's own new claims and nothing else: another class on its own facts stays where it is, the execution lane
schedules the classes with `Final`s, the anchor cadence is untouched (pinned).

**Decision 7 — the boundary.** Registration is permissionless for a graph the canonical VM expresses at the
manifest's `runtime_version`; a graph that needs a new op is refused as `UnsupportedOp` and its op is a protocol
upgrade. The registry therefore never has to understand an operation it was not built to verify.

## 3. What Kimi's registration looks like

A third party submits the manifest and the bond. Every node derives `economic_ccu`, `verification_ccu`,
`artifact 287 GiB`, `prefetch 2`, `verification 2`, `max inflight`, `required ready 7` (or more), the bond. Seven
operators fetch the artifact, run the probe, and their evidence makes them ready. Ten probe claims finalize
with none failing: limited activation at a tenth of the derived admission. Three stable spans: `ACTIVE`. Two
operators leave: `HELD`; the dense tier and the lane continue; the operators return: `PROBATION` again. Nobody
updates a repository.

## 4. What this changes in ADR-0132 and ADR-0133

* ADR-0133 Decision 2 (`warm_p99`/`cold_p99` as inputs): **superseded** — the shadow module keeps them as this
  node's telemetry, the registry's profile ignores them.
* ADR-0133 §9a Fence 1: **becomes Protocol Upgrade A**, the generic framework — manifest, derived profile,
  readiness evidence, lifecycle, class-local capacity gate — armed once; no "Kimi activation" step exists after it.
* ADR-0132 Decision 6 (a new model earns only after a shadow period): **becomes** the lifecycle's `PROBATION` and
  `ACTIVE_LIMITED`, automatic.
* ADR-0124 Decision 6's unit and ADR-0131's rate: **the rate is global** (ADR-0132 C); no class sets a unit.

## 5. Security amendments

* **SA-1 — the registrant cannot shrink its window**: the profile reads the graph, not the manifest's opinion
  of it; under-reporting is impossible because nothing is reported.
* **SA-2 — the registrant cannot flood**: the registration bond grows with the window it imposes, admission is
  budgeted, and a class that cannot draw a panel is `HELD` at no cost to any other.
* **SA-3 — a seat cannot fake readiness cheaply**: the possession proof is over the artifact root the manifest
  names, the probe is a canonical verification the chain can check, and the collateral is real.
* **SA-4 — a class cannot fork the network by existing**: a manifest the VM cannot express is refused at
  registration; nothing about a registered class changes a rule any node runs.
* **SA-5 — globals are one fence, not one per model**: the reference constants are consensus parameters a
  fence moves for every class at once; a measured p99 never moves one.

## 6. The road

```text
now      ADR-0132 / 0133 / 0135 in shadow: the CLI prints each class's derived profile beside its telemetry
   A     Protocol Upgrade A — Permissionless Model Registry V1: manifest, derived profile, readiness evidence,
         lifecycle, class-local capacity gate (ADR-0132 F1/F2 folded in); the dense tier and the hybrid walk it live
   B     the single lottery, if adopted (ADR-0132 S): class shares from admission
   C     EconomicAttempted + a global rate + budgeted admission (ADR-0132 Fence 3), no per-model cap
after    Kimi, Llama, a new mixture, a 100B model: a manifest and a bond, no fork
```

## 7. What is built (shadow)

`consensus/core/src/palw_model_registry_v1.rs`: the manifest and its verdict, the derived work, the globals, the
six derivations, the readiness evidence and its predicate, the lifecycle and its step, the share-free admission
and the single lottery's shares. Tests: the profile is derived from work alone (deterministic; the dense tier,
the hybrid, a Kimi-class and a 10 T graph); the manifest verdict and the VM boundary; readiness is evidence, not
a declaration (root, chunks, participation, collateral, a fresh probe); the lifecycle walks on facts and a held
class stops only itself (with the real lane snapshot); shares follow admission and no one sets them. Not built:
the on-chain manifest transaction and bond, the possession proof, the probe claim, the fold-side gate and the
fence — Protocol Upgrade A's own work.

## 8. Number hygiene

0135 was free when written; the next free number is 0136.
