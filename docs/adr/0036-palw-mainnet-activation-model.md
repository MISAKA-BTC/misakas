# ADR-0036: PALW mainnet activation — lineage reconciliation and the model that governs

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Proposed (governance decision), with Decision 4's hash floor SUPERSEDED.**
Activates nothing, changes no code, moves no fence.

> **Decision 4 の恒久 hash floor は撤回済み。** 混合 difficulty の consensus 負担と、有用計算を hash mining で迂回する誘因があるため。[ADR-0039](0039-palw-only-block-production.md) が置き換える。

This ADR settles a *documentation* conflict the 2026-08-16/17 mainnet-readiness audit surfaced:
two Accepted, non-ancestral ADRs each describe "mainnet PALW", with different mechanisms, a
colliding "PALW / algo-4" name, and overlapping ADR-number spaces. It decides which lineage
governs and what carries forward. It deliberately does **not** decide mainnet parameters — those
come from the Stage-0→3 soak (see "What this ADR does not decide").

> **Numbered 0036, not 0035.** This was first drafted as ADR-0035 and renumbered the same day: a
> parallel session landed `0035-palw-public-testnet-strategy.md` (testnet-11 as the public PALW
> *testnet*) at commit `e4848d2` while this was being written. That collision — two same-day
> ADR-0035s on one lineage — is a live instance of exactly the number-hygiene problem this ADR
> settles for the mainnet/backup fork. The two are otherwise disjoint: ADR-0035 decides the public
> *testnet*, this ADR decides the *mainnet* activation model.

Date: 2026-08-17
Supersedes: **ADR-0041 as to mechanism** (and, by extension, the `palw_spam` / `palw_algo4_accept`
/ `palw_compute_work_scale` activation vocabulary of the `main-backup-8107bfb-20260807` snapshot,
ADRs 0039–0048). It **adopts** ADR-0041's two surviving conclusions (below).
Relates to: ADR-0026/0027/0028 (the four-stage credit ladder this lineage implements),
ADR-0033 (`palw_credit` gate), ADR-0034 (routing),
ADR-0035 (`0035-palw-public-testnet-strategy.md` — the public PALW *testnet* decision; distinct
from this *mainnet* one, and the reason this ADR is 0036),
`docs/palw-mainnet-readiness-audit-2026-08-16-ja.md` (the audit and its 9 blockers),
`docs/palw-class-activation-gate-status.md` (the §12 gate ledger, corrected the same day).

> **Landed later than written (2026-08-17).** This ADR was drafted in a worktree and never
> committed, so the lineage that then produced ADR-0037 and ADR-0038 branched without it — while
> ADR-0038's header states "Everything else in ADR-0036 … stands" and rests normative weight on
> decisions whose text did not exist in any branch. That dangling reference was itself a finding
> of the 2026-08-17 re-audit (blocker 12). It is committed here unchanged in substance, with one
> section added below reconciling it against the pivot that overtook it.

## Relationship to ADR-0037 and ADR-0038 (added 2026-08-17)

[ADR-0038](0038-palw-is-the-consensus-work.md) は PALW を block production に昇格し、[ADR-0039](0039-palw-only-block-production.md) は hash floor を撤回した。本 ADR の lineage・network identity・activation の分離は維持する。旧 hash floor を正当化する比較は現行の要件ではない。

## Context — two lineages, one name

The audit (H12) found two Accepted ADRs describing mainnet PALW that do not reference each other
and sit on branches with no ancestry relation:

| | **Live lineage (governs)** | **Historical snapshot** |
| --- | --- | --- |
| branch | `misakas` → `origin/main` + `palw-llm-pow-*` (canonical tip `palw-llm-pow-unified`) | `main-backup-8107bfb-20260807` |
| mainnet ADR | **ADR-0028** — mainnet is a separate ADR after Stage 3 | **ADR-0041** — mainnet ships PALW active from a new v4 genesis |
| ADR numbers | 0021, 0024–0034 | 0039–0048 |
| mechanism | `palw_credit` fence + `PalwCreditParamsV1` staged credit gate | `palw_spam` / `palw_algo4_accept` / `palw_compute_work_scale` + qwen-8.0 `mint.rs` 12-gate |
| mainnet params today | `palw_credit: None`, `pow_palw_activation: never()` | (its own params, not on the live tree) |

Established by measurement, not assertion:

* **The two lineages are non-ancestral.** `git merge-base --is-ancestor main-backup-8107bfb-20260807 <origin/main | unified | bps01>` → **NOT-ANCESTOR** for all three. Their merge-base is `2dd863c` (2026-07-16); the snapshot diverged and was not carried forward.
* **ADR-0041's mechanism does not exist on the live tree.** `palw_algo4_accept` / `palw_spam` appear nowhere in the live `consensus/core/src/config/params.rs`; the live tree uses `palw_credit` (the ADR-0028/0033 gate). Porting ADR-0041's mechanism would mean *replacing* the lineage's credit design, not merging.
* **ADR-0041 is narrower than its commit message.** It decides only the *land* shape of mainnet — a new v4 genesis, genesis-active lane (`palw_activation_daa_score = 0`), non-inert `palw_spam` — and explicitly keeps `palw_algo4_accept = false`, `palw_compute_work_scale = 0`, and mint `eligible=false / weight=0` behind 12 external gates. Its "genesis-active" is a *land* decision, not a *credit* decision, so it is **not** in contradiction with ADR-0028's "credit after Stage 3" once the two layers are separated.

The genuine conflicts are therefore: (a) a name/number collision between two codebases, (b) two
different mechanisms for the same idea, and (c) two parallel, mutually-unaware decisions about
the mainnet identity. Left unresolved, no one can write a coherent release plan, because
"the mainnet ADR" is ambiguous.

## Decision

1. **The live `palw_credit` lineage governs.** The four-stage credit ladder of
   ADR-0026/0027/0028 and the `PalwCreditParamsV1` gate of ADR-0033 are the mainnet PALW design.
   The `main-backup-8107bfb-20260807` snapshot and its ADRs 0039–0048 are **historical**: a
   parallel design that was not taken forward. They reserve no numbers on the live lineage
   (the live line is free to use 0035, 0036, … and is not bound by the snapshot's 0039–0048).

2. **ADR-0041 is superseded as to mechanism, and two of its conclusions are adopted.**
   * **Adopted — mainnet PALW requires a new network identity.** ADR-0041 reaches this from the
     Header-v4 anti-spam fence (public/value requires `genesis.version == 4` + genesis-active
     PALW, which a fence retrofit on the existing identity cannot satisfy) and from the measured
     one-time cost of retrofitting pruning depth onto a running chain. The audit reaches the
     **same conclusion independently** (H13): the current `MAINNET_PARAMS` identity cannot carry
     PALW, because at 10 BPS `finality_depth = 432_000` and **both** shipped window presets
     (`W_challenge` 8_640 and 720) fail `PalwScheduleParamsV1::validate`'s
     `finality_depth < W_challenge` rule, and a 100 ms block interval is physically incompatible
     with a 37–91 s replay. Two independent design threads converging is strong signal; this is
     adopted as a hard constraint on the future mainnet-parameter ADR.
   * **Adopted — the land → accept → mint separation.** ADR-0041's three stages map onto this
     lineage's ladder: *land* = Stage 0 (a genesis-active lane may exist with credit OFF);
     *accept* = the objective-slash stages; *mint* = Stage 2+ credit, gated by §12 and a separate
     activation decision. Genesis-active *presence of the lane* does not imply genesis-active
     *credit*.
   * **Not adopted — the mechanism.** `palw_spam` / `palw_algo4_accept` / `palw_compute_work_scale`
     / qwen-8.0 `mint.rs` are not this lineage's mechanism and are not ported. Any specific
     0039–0048 item the project still wants (e.g. the v4 anti-spam accumulator shape) is ported
     deliberately, item by item, as a new live-lineage ADR — never by adopting the snapshot
     wholesale.

3. **Mainnet activation is gated behind the full ladder, the §12 gate, and the audit's blockers.**
   ADR-0028's "separate ADR, separate activation, after Stage 3" stands. That future ADR may not
   be signed while any §12 gate item it depends on is unmet **or** while any of the 9
   mainnet-readiness blockers is open. The blockers are not on the current ladder and must be
   added to it — the ledger's through-line ("every unmet item is a fleet measurement, not a
   design gap") was false and is retracted.

4. **恒久 hash floor — 不採用。** 混合 difficulty の consensus 負担と、有用計算を hash mining で迂回する誘因があるため。[ADR-0039](0039-palw-only-block-production.md) に置き換えた。worker 不在の startup 拒否と transient failure の bounded retry は別の有効な hardening として維持する。

5. **Namespace.** Because the snapshot is non-ancestral and historical, the live lineage owns the
   name "PALW / algo-4"; no rename is required on the live tree. This ADR is the record that the
   snapshot's use of the name and the 0039–0048 numbers is superseded, so future readers do not
   mistake `git show main-backup-…:docs/adr/0041-*.md` for a live decision.

## What this ADR does not decide

* **Mainnet parameters.** The new network identity's name, genesis, suffix/ports/seeds, the
  window preset (a 10-BPS-or-slower set that passes `validate`), the credited-job ceiling, `base(C)`
  as a fraction of subsidy, `q`, bonds, and `ρ_v` — all come from the Stage-0→3 soak and are the
  subject of a *later* ADR (the parameterized mainnet-activation ADR ADR-0028 promises). That ADR
  cannot be honestly drafted before the soak, because its parameters are the soak's outputs.
* **Whether to port any specific 0039–0048 item.** Adopted here are only ADR-0041's two
  conclusions in §Decision.2. Anything else from the snapshot is a separate, deliberate port.
* **The 9 blockers themselves.** They are fixed in code, not decided here; this ADR only binds the
  release decision to their closure.

## Consequences

* **ADR-0028's mainnet clause is now self-consistent.** It carries a pointer to this resolution
  (edited the same day), so its "after Stage 3" and ADR-0041's "genesis-active" no longer read as
  a contradiction.
* **The §12 gate ledger gains a "Wired?" column and corrected rows** (`palw-class-activation-gate-status.md`,
  same-day revision), so no future promotion mistakes a landed struct for a live consensus path —
  the specific failure mode that let the through-line be false.
* **Future ADR numbering on the live lineage is unblocked:** this is ADR-0036; 0037+ are free; the
  snapshot's 0039–0048 do not reserve anything here. (0035 is the public-testnet-strategy ADR that
  triggered this renumber — the concrete cost of the un-settled number space this ADR closes.)
* **The mainnet-parameter ADR has its constraints pre-recorded:** new identity (Decision 2),
  post-soak parameters (does-not-decide), hash-floor resolution (Decision 4), all 9 blockers closed
  and on the ladder (Decision 3). It is a fill-in-the-measured-values exercise on top of this
  governance frame, not a fresh design.
* **Landed with this ADR (2026-08-17), the two "live today" items of the audit's critical path:**
  * **libm is now part of the class identity (B8).** `PalwRuntimeManifestV2` gained
    `libm_identity` (diagnostic) and `libm_arithmetic_digest` (load-bearing) — a behavioural
    fingerprint of the resolved `expf`/`logf` over the frozen `PALW_LIBM_PROBE_V1` vector,
    measured through the same dynamic symbols llama.cpp resolves. Manifest version → **v3**: a v2
    manifest could not distinguish two libms, so its class claim was under-specified and must not
    compare equal to a v3 one. Behavioural rather than a build id on purpose — a build id moves on
    rebuilds that do not change arithmetic, while this moves iff the arithmetic moves. It does not
    replace ADR-0031's disassembly audit, which remains what licenses `libm_transcribed`.
    *Verified:* the consensus fingerprint does **not** move (the manifest is not a `Params` field);
    `kaspa-consensus-core` 663/663.
  * **The GGUF pin is no longer bypassable (B15).** The v1 model gate consulted a
    `.palw-gguf-sha.json` in the process CWD keyed on `path|size|mtime` and returned the cached
    digest on a match — and its one caller was `--mode verify`, *the mode block validation
    invokes*, putting the bypass on the consensus PoW path. v1 was folded into the always-recompute
    v2 gate (VPS design §4.4) rather than patched, so one policy remains and no second
    implementation can drift. Cost: one 1.2 GB hash per job process, amortized by the persistent
    agent. Operators should delete any stale `.palw-gguf-sha.json`; it is now inert.
  * **Transient worker faults no longer panic a node.** `run_worker_with_retry` gives
    `PalwWorkerFailed` bounded attempts with linear backoff before the caller's panic; a missing
    worker (`PalwUnavailable`) still fails immediately, since retry cannot fix configuration.
* **What is NOT closed by this ADR:** the hash floor itself is designed and implemented with the
  mainnet identity, not here; TN11/devnet remain single-algo, and a persistent runtime failure
  still halts them by design. That is now a recorded trade, and it is on the ladder rather than
  hidden in a doc-comment.
* **If the project instead wants the snapshot's design to govern,** that is a reversible choice —
  but it means adopting the `palw_spam` mechanism onto the live tree and superseding ADR-0026/0027/
  0028/0033, which is a far larger change than porting ADR-0041's two conclusions. This ADR records
  the smaller, evidence-backed default; reversing it is a deliberate act with its own ADR.

## Mission alignment amendment — 2026-10-07

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
