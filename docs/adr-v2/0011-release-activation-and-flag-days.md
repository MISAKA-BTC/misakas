# ADR-v2-0011 — 公開testnet・release・有効化・flag day

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 5本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0013](../rfc-v2/0013-permissionless-bootstrap-and-release.md)

## 1. この文書の役割

実装、試験、networkでの有効化、公開releaseを区別し、過去のflag dayを保存する。

## 2. 統合した判断の読み方

### 2.1

公開testnet戦略とmainnet候補のlineage・identity判断を記録する。過去testnetの継続判断を、別networkのgenesisや現行mainnet設定へ転用しない。[旧ADR-0035](../adr/0035-palw-public-testnet-strategy.md) [旧ADR-0036](../adr/0036-palw-mainnet-activation-model.md)

### 2.2

BPS拡張前のhardeningと、対象fenceをまとまったreleaseで有効化する判断を、個別の未実装／休眠項目を残して読む。[旧ADR-0130](../adr/0130-bps1-is-hardened-before-it-is-widened.md)

### 2.3

DAA 750・1,300のflag dayは、そのnetwork・release・fence集合についての履歴として固定する。文書統合によって新しい高さや追加fenceを認可しない。[旧ADR-0154](../adr/0154-testnet-12-flag-day-daa-750.md) [旧ADR-0155](../adr/0155-testnet-12-flag-day-daa-1300.md)

## 3. 維持する境界・後続改定

ADR-0036の旧lineageやhash-floor部分は後続変更との照合が必要。過去のAccepted and armedは現在の全機能・mainnetの合格ではない。

## 4. 隣接する判断との分担

fingerprintは[ADR-v2-0001](0001-network-identity-and-fingerprint.md)、設計凍結・変更承認は[ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0035-palw-public-testnet-strategy.md](../adr/0035-palw-public-testnet-strategy.md) | L1–L199 | L14–L19 |
| [0036-palw-mainnet-activation-model.md](../adr/0036-palw-mainnet-activation-model.md) | L1–L184 | L15–L16 |
| [0130-bps1-is-hardened-before-it-is-widened.md](../adr/0130-bps1-is-hardened-before-it-is-widened.md) | L1–L280 | L17–L30 |
| [0154-testnet-12-flag-day-daa-750.md](../adr/0154-testnet-12-flag-day-daa-750.md) | L1–L71 | L5–L13 |
| [0155-testnet-12-flag-day-daa-1300.md](../adr/0155-testnet-12-flag-day-daa-1300.md) | L1–L50 | L5–L10 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0035-palw-public-testnet-strategy"></a>

<details>
<summary>0035-palw-public-testnet-strategy.md — 原文 L1–L199</summary>

[原文を開く](../adr/0035-palw-public-testnet-strategy.md) · SHA-256: `bb32fb2fc67cf40773f9a0e1fa50e84518b65fc168b0746a71b17d5b15199c8e`

<!-- ADR_SOURCE_BEGIN:0035-palw-public-testnet-strategy.md -->
~~~~~~~~markdown
# ADR-0035: The public PALW testnet is testnet-11, continued — and it pins its determinism class at the door

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

- Status: Accepted (implemented 2026-08-17; operator items listed in §6)
- Track A gate 5. Evidence base: gates 1–4 —
  `docs/palw-algo4-forgery-audit-2026-08-16.md` (gate 1),
  `docs/palw-algo4-crosshost-determinism-2026-08-16.md` (gate 2),
  `docs/palw-algo4-difficulty-economics-2026-08-16.md` (gate 3),
  `docs/history/palw-testnet11-soak-2026-08-16.md` (gate 4, running).

> **Superseded in part (index reconciliation, 2026-09-02).** Decision 1 — "the chain the fleet is
> soaking IS the public chain; no fresh re-genesis at announce" — held for Relaunch 1 only.
> [ADR-0042](0042-palw-mainnet-candidate-ruleset.md) (§"The two-network split") made testnet-11
> the PALW-RC and ruled that a public RC that changes any consensus rule is re-genesised as
> RC(n+1), never continued; testnet-11 has since been re-minted as Relaunch 2 through 5e
> (`docs/history/testnet11-relaunch2-genesis.md`, `docs/history/testnet11-remint-2026-08-29.md`,
> `docs/history/testnet11-regenesis-2026-08-30.md`, `docs/history/testnet11-relaunch5-runbook.md`,
> `docs/history/testnet11-relaunch5e-runbook.md`). The algo-4 `LegacyTn11` lane this ADR launched is not
> running anywhere: `--netsuffix=11` resolves to `palw_rc_shipped_params` (`ConsensusV2`), and the
> old `TESTNET11_PARAMS` is kept only so a node asked for it by name gets a coherent answer.
> Decision 2 (class admission pinned in code, checkable by machine) stands and is the ancestor of
> ADR-0042 Decision 11's fingerprint and ADR-0067's registration index. Map: [`README.md`](README.md).

## 1. Context

Two competing deployment plans have coexisted since the PALW work began:

- **Plan A (superseded by this ADR): t10 PALW re-genesis** —
  `docs/history/testnet10-palw-rollout-runbook.md`, the "-palw" re-genesis of the live
  testnet-10. Written before the Ollama (algo 5) forgery was found and before t10
  grew its present roles.
- **Plan B: a separate PALW net.** `TESTNET11_PARAMS` (staging preset, commit
  f9daebf) exists and is live on the fleet as the gate-4 soak: 120 s blocks, PALW-4
  (worker/full-logits flavor) from genesis, hash lane off, genesis bits `0x200ccccc`.

Facts that decide between them:

1. t10 today is the **hash-PoW + PoS-finality + EVM** experiment: bonded validators
   (A/B/C, 20k BILI each), the misakastake.com/MTP infrastructure, a public entry
   (A's 26211 socket bridge) — and its own unresolved wedge (DNS finality anchor
   stale). Re-genesising it for PALW destroys a working experiment to fix nothing.
2. The soak chain already demonstrates the public-testnet shape end to end: real
   inference at real difficulty, three miners on two CPU vendors, cross-host replay
   acceptance, and a launch window that behaved exactly as gate 3 predicted — the
   pre-scaled genesis bits landed **within ~10 % of converged difficulty** (first
   adjustment moved bits from `0x200ccccc` only to the `0x200c–0x200d` range): no
   trivial-bits emission burst, no stall.
3. Gate 2 measured the determinism-class boundary exactly: across 61 seeds the
   x86-64 CPU class agrees byte-for-byte on 305/305 tag fields, while Metal agrees
   on decode length 61/61, output text 47/61 and **gemm trace 0/61**. One network
   must be one class, and membership must be checkable by machine.

## 2. Decision 1 — the public PALW testnet is testnet-11, the *current chain*

The chain the fleet is soaking IS the public chain. No fresh re-genesis at
announce.

- The multi-day real-inference history is the strongest launch artifact we have:
  every header in it is a replayed pinned inference, publicly re-verifiable by any
  in-class node from genesis (from-genesis IBD is the gate-4 join test).
- Nothing spendable predates the public: soak miners run `--allow-burn`, so every
  pre-announce coinbase is provably unspendable. There is no insider accumulation
  window to explain away.
- The genesis marker already names the chain honestly (`misaka-testnet` / TN11 /
  Relaunch 1). A "clean" re-genesis would only re-run the launch window we already
  measured, and discard the measurement.
- **t10 is untouched** and keeps its own lane (its wedge is its own workstream).
  `docs/history/testnet10-palw-rollout-runbook.md` is superseded as a deployment plan;
  its operational content (fingerprint methodology, fleet mechanics) remains valid
  reference.

## 3. Decision 2 — class admission is pinned in code, not in a runbook

`algo_id = 5` (Ollama) had a boot-time calibration (`POW_L1_PALW_OLLAMA_CALIBRATION_V1`);
`algo_id = 4` had none — a drifted worker runtime would start happily and silently
fork itself off the network, the exact failure mode the Ollama lesson catalogued.
Closed by this ADR:

- `POW_L1_PALW_PROBE_SEED_V1` — the audited "uniform/u0" probe seed
  (`BLAKE2b-256("palw-audit-2026-08-16/uniform-0/0")`).
- `POW_L1_PALW_WORKER_CALIBRATION_TN11_V1` — the 200-byte tag that seed MUST
  produce, measured byte-identical on all four fleet hosts (Broadwell + 3× EPYC,
  two vendors, four kernels; gate-2 canonical digest `311d7eab…`).
- `palw_worker_calibration_v1(network_id)` — the per-network pin table.
  **testnet-11 pins the x86-64 CPU class; devnet deliberately pins nothing**
  (a dev net is where new classes are born); nets where algo 4 is inert never
  reach the check. Every future public PALW net must add its row before its
  activation flips.
- Enforcement is double: **eagerly** in the kaspad startup rail (one probe
  inference before any peer is dialed, with an actionable message) and **lazily,
  memoized, in `palw_l1_tag`** — the path every consumer (node validation, miner,
  pruning-proof replay) must pass through, so no harness can skip it. The fixture
  tag family is refused outright on class-pinned nets (defense in depth behind the
  existing devnet-only rail).
- These constants are **admission-side, not consensus fields**: nothing entered
  `Params`, the fingerprints did not move (662/662 consensus-core tests, pins
  unchanged) — same design position as `dns_seeders` and the Ollama pins.

Functional verification, both directions:

- an Apple-Silicon/Metal worker attempting testnet-11 is **refused at startup**
  (exit 1) — and the refusal message itself renders the class split: expected vs
  got differ **only in the gemm_trace_root bytes** (output commitment, schedule,
  token counts identical) — gates 1–2's central finding, now visible to any
  operator who tries;
- devnet boots and mines unchanged (no pin, no probe, no new messages).

## 4. Decision 3 — the participation model is honest about who can join

At launch, validating/mining testnet-11 requires being in the pinned class:

- the published **worker binary** (CPU profile
  `release/cpu-only/single-variant/no-native/no-lto/no-blas/no-openmp/threads-4/gpu-off/static/v1`,
  sha `2bd857f8…`) and **GGUF** (`Qwen3.5-2B-Q4_K_M.gguf`, sha `aaf42c8b…`,
  1_280_835_840 B);
- x86-64 hosts. The class is *measured* on Broadwell + EPYC across four kernels;
  the single-variant/no-native build is designed to keep one code path on any
  x86-64, and the calibration gate turns "designed" into "checked at your door".
- **Apple Silicon / arm cannot join at launch** (measured out of class, 0/61 gemm
  agreement). This is stated, not hidden. An arm-class port of the worker (or a
  multi-class committee design) is future work and would arrive as its own pinned
  class + ADR addendum, not as a silent widening.
- The audit harness (`scripts/verify/misaka-palw-forgery-audit.py`) doubles as a
  self-service class check: run it, diff the jsonl against the published fleet
  values — the same methodology that produced the pin.

## 5. Economics at launch (facts an announcement must carry)

- Full schedule: **4445.62 BILI/block** (rate-preserving 120 s table,
  `YEAR1_PER_BLOCK_TWO_MINUTE`). Until validators bond, the chain mints the
  ADR-0018 §F worker BASE share only — **62 %, 2756.28 BILI/block to the miner**;
  the validator 30 % is don't-minted and the §D inclusion 8 % follows its own
  pool path (gate-3 measured this to the sompi on 118 consecutive coinbases).
- The fixed-difficulty launch window is exactly `min_difficulty_window_size = 150`
  blocks and, with the pre-scaled genesis bits, runs at roughly target cadence
  (soak-measured ~148 s), so there is no burst-emission window to disclaim.

## 6. Operator items (deliberately NOT decided in code)

1. **Discovery**: TN11 seeder records (e.g. `n11-seed*.misakascan.com`) and which
   hosts answer them; whether public nodes default to the soak port (37711) or the
   suffix default. Ports are node-local config, not consensus.
2. **Public entry**: whether A's socket-bridge pattern (t10's 26211 precedent) is
   replicated for TN11, and on which host — C cannot take inbound (ufw
   default-deny), ibm currently centers the star.
3. **Validator bonding** on TN11 (lifts emission from 62 % toward full; reuses the
   t10 bond tooling) — its own runbook when scheduled.
4. **B (95.111.236.186)**: disk 100 % full; `/root/palw` (15 G) is the cleanup
   candidate. B joins the fleet only after that operator decision.
5. **Launch criteria** before announcing (gate 6's entry checklist):
   ≥ 48 h clean soak; a fresh node's from-genesis IBD join over the public path;
   the calibration-gated binary deployed fleet-wide (next natural rebuild — the
   running soak binaries ARE the measured class; the gate exists for new joiners);
   node-operator doc published with the shas and the audit-harness check.

### Status of the gate-6 checklist, measured 2026-08-18

| item | state |
|---|---|
| ≥ 48 h clean soak | 44.7 h, 0 panics — met on schedule, nothing to do |
| a fresh node's from-genesis IBD join | **IBD half PASSED: 7 h 45 m** for a ~1,300-block chain. The *participation* half failed — see below |
| calibration-gated binary deployed fleet-wide | **NOT met.** The fleet kaspad is from 2026-08-16 13:05 and predates both the remote-panic fix and the switch-counter fix |
| node-operator doc | **published:** `docs/history/testnet11-node-operator.md` |
| discovery (item 1) | **NOT met.** `n11-seed*.misakascan.com` do not resolve |

**The join measured a cost this ADR did not carry, and an announcement must.** 7 h 45 m for 1.5 days
of history, sustained at ~163 headers/hour against a chain producing ~40/hour. The cost grows with
chain age until the pruning proof caps it at roughly 35–50 h (ADR-0041's per-header numbers). Budget
**1.5–2 days for a first sync on a mature chain**.

**And the join found the blocker.** The node completed its IBD and was then permanently
`quarantined`: it reached the chain-switch cap of 5 without ever having switched, because every
refused candidate advanced the counter that refused it (`switched chains 384 times`), and
`--clear-quarantine` could not recover it — the count it preserved was what re-quarantined the node
seconds later. Fixed in `fix(ibd): a refused chain switch no longer feeds the counter that refused
it`; it fires by construction on a PALW network, where a node that verifies slower than its chain
grows is permanently behind and so sees every peer as verified-better forever.

So the remaining launch work is **operational, not consensus**: rebuild and redeploy the fleet
binary with both fixes, then settle discovery. Nothing on this list now needs a code decision.

## 7. Consequences

- Track A's remaining work is now exactly gate 6: the operator items above plus
  the coordinated announce. No consensus changes remain on the critical path.
- Track B (verified-compute credits, VLT overlay activation on TN11) stays fully
  decoupled, as the original two-track decision required.
- The class-pin table is the template for any future public PALW network,
  including a mainnet decision — which this ADR explicitly does NOT make.
~~~~~~~~
<!-- ADR_SOURCE_END:0035-palw-public-testnet-strategy.md -->

</details>

<a id="source-0036-palw-mainnet-activation-model"></a>

<details>
<summary>0036-palw-mainnet-activation-model.md — 原文 L1–L184</summary>

[原文を開く](../adr/0036-palw-mainnet-activation-model.md) · SHA-256: `9e368fad154179316127402a16361311b6c98cfe424161eade14a9766e718071`

<!-- ADR_SOURCE_BEGIN:0036-palw-mainnet-activation-model.md -->
~~~~~~~~markdown
# ADR-0036: PALW mainnet activation — lineage reconciliation and the model that governs

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

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
~~~~~~~~
<!-- ADR_SOURCE_END:0036-palw-mainnet-activation-model.md -->

</details>

<a id="source-0130-bps1-is-hardened-before-it-is-widened"></a>

<details>
<summary>0130-bps1-is-hardened-before-it-is-widened.md — 原文 L1–L280</summary>

[原文を開く](../adr/0130-bps1-is-hardened-before-it-is-widened.md) · SHA-256: `0604398b281c1018eb5d0d12571187a4cc494417f104249ba84e06ba74fae586`

<!-- ADR_SOURCE_BEGIN:0130-bps1-is-hardened-before-it-is-widened.md -->
~~~~~~~~markdown
# ADR-0130 — BPS 1 is hardened before it is widened

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: **ACCEPTED 2026-09-17; Decisions 2–6 IMPLEMENTED the same day, Decision 1 built dormant, Decisions 7–8 not built** — on `feat/palw-exec-lane-and-validator-retirement` (§7).
  Decisions 2–5 ride testnet-11's DAA 6,001 flag day with ADR-0124/0125/0126/0128. Decision 1 is built
  dormant and computed in shadow (Decision 8); the operator decided (2026-09-17) that λ is off at 6,001 and
  gets a height only once the shadow shows the capacity and a shorter liability lifetime exists (§3).
* **ADR-0144 alignment (2026-09-21).** §5 deferred widenings stay deferred under 0144 item 0. See the end section.
* Operator's direction, in the operator's words: "BPSは当分1で固定するなら、今は高速化ではなく『BPS1でも壊れ
  ない経済性・選出・settlement』を固める段階"; "一番大きい変更を1つだけ選ぶなら、まず
  `panel_exposure = max(3×claim.reserved, λ×seat_reward)`"; "その次が BPS1でのcross-round operator連続禁止、
  その次が claim由来seedから『snapshot後のfuture DAG anchor由来seed』への変更"; "代替operatorがいない →
  round miss"; "安全性のためのルールを、参加者不足だからこっそり解除するのは危険".
* Builds on: [0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)
  (the panel's pay and a seat's exposure), [0125](0125-the-execution-lane-is-a-second-lane-inside-the-cadence-and-it-widens-one-permit-at-a-time.md)
  (the lane and its schedule), [0129](0129-a-double-spend-needs-the-anchors-not-the-blocks.md) (settlement
  counts anchors), [0072](0072-the-ticket-is-the-execution.md) (a block's draw is its header position).

## 0. The sentence this ADR is

**While the execution lane runs at one block a second, a panel seat risks more than it can earn on the
claim it judges, the draw gives an operator one entry however its collateral is split, no operator holds
two consecutive rounds, and a span's producers are chosen by a PALW anchor mined after the set they are
chosen from was fixed — with a missed round, never a relaxed rule, where nobody qualifies.**

## 1. What exists, and where it fails at width 1

* **A seat earns one to five orders of magnitude more than it risks, and a producer more still.** Past DAA
  6,001 a testnet-11 claim escrows 3,200.85 BILI before ADR-0124's work price; the pool is a fifth, 128.03 BILI
  a seat at full price. A claim reserves one inference's worth at the network's slash value, 5 sompi a pwu
  (read from the live node's registration terms), and a seat three times that. A model class is priced by
  its pwu against the heaviest weight-bearing model class (the unit), so its reward-to-exposure ratio does
  not depend on the class, only on the unit; the floor is not priced:

  | role | model class (unit = 9,000,776 pwu: `Qwen3.8-27B/graph-v3`, share 1 ‰) | floor (7,708 pwu, unpriced) |
  |---|---|---|
  | panel seat: `128.03 BILI × price ÷ (3 × pwu × 5 sompi)` | ~95× | ~110,700× (128.03 / 0.0012 BILI) |
  | producer: `2,560.68 BILI × price ÷ (pwu × 5 sompi)` | ~5,700× | ~6,640,000× |

  The unit is the heaviest *weight-bearing* model class, and a class bears weight once its share is above
  zero: testnet-11's `Qwen3.8-27B/graph-v3` (registered at DAA 1,165, 1 ‰) is therefore the unit, pricing
  `Qwen/Qwen2.5-1.5B/graph-v5@512` (6,630,544 pwu) at 73.7 % and `Qwen3.6-35B-A3B/graph-v3` (2,685,360) at
  29.8 %. (Were the unit 6,630,544 the seat ratio would be ~129×; at 2,685,360, ~318×.) The panel side is what
  the operator asked to fix first; the producer side is the same shape and larger (Decision 8 reports both).
* **A bond split is a ticket bought.** Past ADR-0124 every eligible bond draws one ticket; one seat per
  operator per panel stops a quorum, not the odds: ten bonds are ten chances to be among the five.
* **"One permit an operator a round" is vacuous at width 1.** A round has one permit, so one operator can
  hold rounds N, N+1, N+2… The domain parity rule stops a *domain* holding consecutive rounds; an operator
  with bonds in both parities is not stopped.
* **The schedule's seed is the finals' claim ids and execution roots.** A producer that can choose among
  executions can choose among seeds before the set is fixed.
* **A span publishes an hour of producers.** At 30 DAA a span, every round's permit holder is computable an
  hour ahead.

## 2. Decisions

**Decision 1 — a seat reserves at least λ times what it can earn (dormant).**
`Params::palw_panel_exposure_floor: Option<PalwPanelExposureFloorV1 { activation, reward_multiple_permille }>`.
Past it a seat reserves `max(3 × claim.reserved, λ × max_seat_reward)`, where `max_seat_reward` is the
per-seat share of the pool the claim's escrow holds (work pricing only lowers it) — resolved at the claim's
anchor with the rest of the draw policy, used by the eligibility headroom and by the reservation alike, and
stored on the seat's duty so release and the dissent slash move exactly what was reserved. The panel floor
(ten producer floors: 130,000 BILI on a mainnet card since `a3c5db22`, 2026-09-24; 100,000 before) stays a
separate participation floor. Mainnet's λ is **5** (the user's 2026-09-25 decision, from the 5–10 this
paragraph recorded as under consideration): testnet-12 arms it at `reward_multiple_permille = 5_000` from
genesis; the mainnet card does not state it yet, and testnet-11 and devnet keep `None`. §3 is why
testnet-11 does not arm it at 6,001.

**Decision 2 — one entry per operator.** Past `palw_panel_economy` the draw groups the eligible bonds by
operator; an operator's candidate is its eligible bond with the lowest bond ticket, and the operator draws
one ticket `H(anchor ‖ claim ‖ operator)`. Splitting collateral across bonds buys nothing; an entry needs an
operator key whose bond can reserve the seat's exposure, so more entries need more capital (with Decision 1,
λ times the seat reward each). The fingerprint names the rule (`palw_panel_draw/operator_ticket_v1`).

**Decision 3 — an operator never holds two consecutive rounds.** The schedule gives each operator one parity —
that of the domain carrying most of its credit — and drops its bonds from domains of the other parity. Rounds
alternate parity, so no operator holds adjacent rounds of a span at any width, and a round whose parity has no
eligible operator has no permit: a miss, counted, not a relaxed rule.

**Decision 4 — participants first, then a future anchor.** At the first chain block of span `n`:
the pending snapshot (quotas, parities, bonds — everything but the seed), taken at the start of span `n − 1`
from the finals of span `n − 2`, becomes span `n`'s schedule, seeded with
`H(A ‖ n ‖ safe frontier)` where `A` is the last chain block carrying an admitted attempt during span `n − 1`
— mined after the snapshot, and re-rolled only by a new winning draw (ADR-0072); then the finals of span
`n − 1` become the pending snapshot for span `n + 1`. No anchor, or a stale snapshot, is no schedule: the lane
idles that span. Credits are spendable two spans after they are earned, and a span's producers are known only
once its first block exists. No beacon. The fingerprint names the rule set (`palw_execution_lane/scheduler_v2`).

**Decision 5 — testnet-11's span is 5 DAA** (about ten minutes at 120 s).

**Decision 6 — the width stays 1.** The stage table keeps its ceiling of ten and every widening its own
fenced height; none is scheduled.

**Decision 8 — λ in shadow, and the economics in one table (node-local, no consensus).** Until λ has a
height the node computes, for the live claims, what λ = 2 would require: each seat's required exposure,
whether each operator would be eligible, whether each panel would fail to draw, each operator's capacity and
utilization; it records each claim's lifecycle (bound, licensed, final or voided, exposure released); and it
reports reward per reserved sompi for the producer and the seat by class. λ gets a height only when the shadow
shows p95 operator utilization under 70 %, eligible operators beyond the seats after excluding the executor,
would-fail draws under 0.1–1 %, receipt latency p99 well inside the new deadline, an acceptable round-miss rate,
and several complete claim lifecycles observed. Once armed λ is a fixed consensus parameter: it is never lowered
because operators are scarce — a panel that cannot draw leaves its claim short of `Final`, and scarcity is
answered with operators and collateral.

**Decision 7 — a receipt's verdict may resolve, not reverse (recorded, not built).** `Unavailable → Valid` and
`Unavailable → Invalid` within the receipt window stay honest (a restarted seat can sign both); a seat that
signs `Valid` and `Invalid` for one claim contradicts itself and should be slashable. Built after BPS 1 has run.

## 3. testnet-11's capital, measured (2026-09-17, DAA 5,774)

Read from the explorer node (`getPalwClaims`, role `seat`, each genesis bond):

| | |
|---|---|
| genesis bonds | 8, each 10,000.00 MSK |
| claims a bond is seated on, not yet terminal | 296 on bond 0; 500+ on each of the other seven (the node's row cap) |
| of those, `panel_bound` | 177–480 a bond (the receipt window is 600 DAA) |
| exposure a bond reserves for them today | 88.5–115.3 MSK |
| reservation ceiling | 500 ‰ of collateral = 5,000 MSK a bond |

With λ = 2 a full-price seat reserves at least 256.07 MSK (a Qwen3.6 seat priced at 40.5 % about 104 MSK, a
floor seat 256.07 MSK), so ~500 concurrent seats need on the order of 100,000–128,000 MSK reserved a bond —
some 200,000–256,000 MSK of collateral at the 500 ‰ ceiling, twenty to twenty-five times what a genesis bond
holds. At 256 MSK a seat a 10,000 MSK bond holds ~19 seats. Armed on today's
bonds, the draw would find too few operators with headroom, `PanelBound` could not be built, and claims would
stop reaching `Final` — and the lane, whose schedule is made of finals, would stop with them. Arming it on
testnet-11 therefore needs one of: collateral scaled to the concurrency (new bonds of ~300,000 MSK, or more
operators), a shorter claim life (the receipt window drives the concurrency), or both; it is not armed at
6,001 (the operator's decision).

**Why the concurrency is 500: the liability lifetime, measured** (1,311 distinct claims read from four bonds,
newest first, at DAA 5,774):

| segment | p50 | p90 | p99 |
|---|---|---|---|
| accepted → bound | 641 | 642 | 646 — 739 of 1,292 were redrawn: the first panel did not license in 600 DAA |
| bound → licensed (the 91 licensed) | 75 | 252 | 562 |
| age of the 824 still `panel_bound` | 257 | 471 | 590 |
| accepted → `Final` (48) | 2,016 | 2,121 | 2,200 — the challenge window is 1,200 DAA before 6,000 |
| accepted → voided (320, all `receipt_timeout`) | 1,242 | 1,244 | 1,245 — two receipt windows |

By class: the 2,685,360-pwu class (Qwen3.6, 488 ‰) had 500 claims sampled and none licensed; the
6,630,544-pwu class (491 ‰) licensed at a median 82 DAA and ended 48 `Final` against 320 voided; the floor
licensed at a median 6 DAA. **The window is not the root cause — licensing is.** A 30-DAA receipt window
today would void nearly every model-class claim (licence p50 75–82 DAA, and never for Qwen3.6). The order is
therefore: find why model-class panels do not reach quorum in time; then shorten the receipt window and the
challenge window to what licensing actually takes (6,000 already cuts the challenge window to 120 DAA); then
size bonds to the concurrency that remains (the operator's first trial: ~30 DAA with 20,000 MSK panel bonds
→ ~39 seats of capacity against ~25 in use); then arm λ. The lane is exposed to the same cause: its schedule
is made of finals, so past 6,001 spans in which only the floor and the 6,630,544-pwu class finalize schedule
only their domains, and spans with no final idle.

## 4. Security amendments and residuals

* **SA-1 — a seat's downside is priced on the claim, not the network.** λ multiplies the claim's own seat
  share, so a class paid less reserves less, and the reservation is released exactly as reserved.
* **SA-2 — the operator key is still free.** Decision 2 prices an entry in collateral only through the
  reservation; below Decision 1's floor an extra key with a small bond is an extra entry. Decision 1 is what
  makes an entry cost capital proportional to what it can earn.
* **SA-3 — span boundaries.** Two schedules meet at a boundary; an operator may hold the last round of one
  span and the first of the next.
* **SA-4 — the anchor's producer chooses among its own winning draws.** Biasing a seed costs one winning draw
  a try, and the anchor must be the last attempt block of its span.
* **SA-5 — misses are the signal.** A span with no attempt anchor, a parity with no eligible operator, and a
  panel with too few eligible operators all fail visibly; none falls back to a weaker rule.

## 5. Deferred, by name

The panel share derived from verification cost (10–30 %); a DA/availability reward; a 33 % domain cap or a
two-round domain cooldown; widths 2–10; the base-DAG carrier; a rolling schedule that names producers only
seconds ahead (before mainnet); Decision 7's slash.

## 6. What comes next (M3, M4)

* **Settlement under attack**, as property tests and simulations: one operator with 80 % of the compute, one
  domain with 90 %, half the producers offline, two and three of five seats malicious, a permit signed twice,
  one output spent on two branches, a claim withheld before an anchor, a bond split a hundred ways, a
  producer DDoSed out of its rounds — each asserting that no number of execution blocks moves settlement depth;
  and four the operator added: anchor grinding (withholding a last attempt block to re-roll a span's seed),
  operator-parity gaming (moving credit between domains to choose a parity), λ-shadow exhaustion (replaying a
  claim stream to the DAA each operator's free collateral runs out under λ = 2), and one top operator offline
  under the consecutive-round rule (round misses and settlement progress).
* **What BPS 1 must show over weeks**, readable over RPC: rounds total/produced/missed, permit conflicts and
  double signs, operator and domain shares (p50/p95/max), consecutive-round rejections, settlement latency and
  reorgs, panel response/correct/slash rates, panel and producer reward per reserved sompi, BILI per canonical
  compute by class.

## 7. Implementation record (2026-09-17)

Merged into `feat/palw-exec-lane-and-validator-retirement` as M1 (`fc4325d9`, the seat exposure floor and the
operator lottery) and M2 (`da837d62`, the scheduler). testnet-11's fingerprint is the union pin
`ab4e7b9c7e20d14cbadc0874312b8c6dff89ca66ed7e9be3af2f5523dc58b5f2`; M1 alone measured `236cdb56…`, M2 alone
`a4df92df…`, and the first 7,001 build `4787b92a…` — none of the three was deployed. The identity and the fence
schedule (…, 6000, 6001, 6201, 6900, 2125000) did not move between them, so the fork-id gate cannot tell these builds apart: the fleet
carries one pin or it forks silently at 6,001.

* **Decision 1 — built, dormant.** `Params::palw_panel_exposure_floor: Option<PalwPanelExposureFloorV1
  { activation, reward_multiple_permille }>`, hashed `Some`-only, `None` on every preset and card, refused where
  it cannot floor (`adr0130_the_seat_exposure_floor_is_dormant_everywhere_and_refused_where_it_cannot_floor`,
  `adr0130_the_exposure_floor_is_hashed_some_only_and_scheduled_like_a_fence`). Past it a seat reserves
  `max(3 × claim.reserved, λ × max_seat_reward)`, the draw's headroom asks for that amount, and the duty row
  stores what binding reserved, so the release and the dissent slash move that amount across a fence crossing
  (`adr0130_what_a_release_returns_is_what_binding_reserved_across_the_fence`,
  `adr0130_the_dissent_slash_takes_the_stored_exposure`,
  `adr0130_no_floor_is_the_shipped_reservation_and_the_shadow_ledger_prices_another`). The seat exposure, the
  eligibility, the lottery and the stored/hypothetical seat ledgers are pure public functions for a shadow read.
* **Decision 2 — at 6,001.** Past `palw_panel_economy` the draw groups the eligible bonds by operator, an
  operator's candidate is its bond with the lowest bond ticket, and each operator draws one ticket under its own
  domain; the rule is named in the fingerprint as `palw_panel_draw/operator_ticket_v1` beside the fence's height
  (`palw_panel_v2.rs`, `palw_panel_economy_v1.rs`, `palw_state_v2.rs`, `palw_state_v2_sync.rs`, the virtual
  processor). Where it does not reach (`51330459`): the stratified shard draw takes no economy, so neither
  ADR-0124's floor and headroom nor this lottery and exposure floor ride it; the operator counter the maturity
  warning prints is the lottery's unit; the seat economy carries three numbers, not two.
* **Decisions 3, 4, 5 — at 6,001.** `palw_execution_lane_v1.rs` scheduler v2: the snapshot/seed split, one
  parity an operator (`adr0130_an_operator_spread_over_both_parities_misses_one_paritys_rounds`,
  `adr0130_no_operator_holds_permits_in_two_consecutive_rounds_of_a_span`), the pending-snapshot and
  seed-anchor rows in `palw_state_v2.rs` (`adr0130_a_snapshot_is_seeded_only_by_an_anchor_recorded_after_it_was_taken`,
  `adr0130_a_gap_drops_stale_finals_and_snapshots`,
  `adr0130_the_attempt_carrying_block_that_opens_a_span_anchors_the_next_target`,
  `adr0130_the_seed_moves_with_the_anchor_span_and_frontier_and_nothing_the_finals_choose`), and the pipeline
  (`adr0130_a_span_is_seeded_at_its_first_block_from_the_anchor_of_the_span_before`,
  `adr0130_a_round_block_whose_permit_the_previous_round_held_is_not_granted`,
  `adr0125_a_finalized_attempt_schedules_the_span_two_on_and_a_permit_is_accepted_once`). The rule set
  `palw_execution_lane/scheduler_v2` rides the fingerprint and the schedule id; testnet-11's
  `schedule_span_daa` is 5 (was 30). Readers: `getPalwRoundLane` names the span its finals are for
  (`finals_span + 2`), `misaka mining status` prints "N finals toward span s+2" or "no finals gathered yet",
  the round producer's doc names the two-span delay, and the devnet drill waits for two span boundaries with a
  default span of 10 DAA (`LANE=0,2,10`). **The drill has not been re-run on the merged branch.**
* **Decision 6 —** unchanged: width 1, no widening scheduled. **Decision 7 —** recorded, not built.
  **Decision 8 —** not built: no node computes the λ = 2 table yet; ADR-0131's op 185 carries the per-class
  census (claims by state, escrow, compute), not the seat-exposure shadow.
* **Residuals the implementation named.** An operator is the registry's operator id, so the parity rule and
  the lottery inherit ADR-0125's Sybil residual (SA-2: until Decision 1 has a height, a second operator id is a
  second entry and a second parity). SA-3 stands: two schedules meet at a span boundary. The seed anchor is the
  execution key of the latest attempt-carrying chain block of the span before, chosen among that producer's own
  winning draws (SA-4). The fork-id gate does not see the rule change (above).
* **Verification of the union branch (M1 + M2 + ADR-0131's shadow, `1b0bc958`), 2026-09-17:** consensus-core
  2,133 / consensus (evm) 305 / kaspad 96 / mining 92 / misaka-cli 175 / pq-validator-core 12 + pq-validator 42 /
  rpc-core 146 / rpc-service 1 / palw-extension 2 / integration `rpc_tests::sanity_test` 1 — all green; clippy
  `-D warnings` over the twelve crates clean; `shipped_presets_have_pinned_fingerprints` and
  `the_moved_heights_refuse_the_deployed_7000_release_from_6000_and_keep_it_below` green on the union pin.

## 8. Number hygiene

0130 was free when written; the next free number is 0131.

## ADR-0144 alignment (2026-09-21)

[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) §6 item 0: do not widen
the current economy until accounting and admission land.

§5's deferred list stays deferred, and is no longer "after BPS 1 is hardened":

* widths 2–10, a rolling schedule, a derived panel share, a DA/availability reward, tighter domain
  caps, Decision 7's slash — each extends reward, admission or throughput. They wait on ADR-0144
  items 2–3 (and on 0146 before any new economic scalar). Decision 8's λ shadow may keep measuring;
  it does not arm a new pay table.
* Decisions 2–6 as already implemented (width 1, the scheduler, the snapshot/seed split) stand.
  Width stays 1.

## Mission alignment amendment — 2026-10-07

* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。
* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0130-bps1-is-hardened-before-it-is-widened.md -->

</details>

<a id="source-0154-testnet-12-flag-day-daa-750"></a>

<details>
<summary>0154-testnet-12-flag-day-daa-750.md — 原文 L1–L71</summary>

[原文を開く](../adr/0154-testnet-12-flag-day-daa-750.md) · SHA-256: `37d0069886268b0341236f203c07d4a0c58b6f50680299090117393f48a13c89`

<!-- ADR_SOURCE_BEGIN:0154-testnet-12-flag-day-daa-750.md -->
~~~~~~~~markdown
# ADR-0154 — testnet-12's first post-launch flag day: thirteen fences at DAA 750

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

* Status: **Accepted and armed.** The fences activated on public testnet-12 at DAA 750 on 2026-09-27
  (about 07:29 JST), with release `c3dbaee3c` and params fingerprint `dbbc9104…`.
* Date: 2026-09-26 (decided), 2026-09-27 (armed)
* Decided by: the operator. On 2026-09-26 the decision was to put every CRITICAL and HIGH post-launch
  fix into one fence, with V02 taken as option (a). At 17:05 that day the height moved from DAA 500 to
  DAA 750.
* Amends: [ADR-0152](0152-account-stake-staged-reserve-and-vested-rewards.md) (SW-8's seed and anchor;
  L-3's lock life; A-6's lock budget; B's maturity). It was written after the fact, to close the gap
  of fences with no ADR ([INVENTORY.md](INVENTORY.md), gaps).

## Context

After the launch of 2026-09-25, the pre-freeze audit and the live chain found consensus-level
CRITICAL and HIGH issues:

- the panel seed could be re-rolled for free by re-signing the anchor;
- an unbonded heartbeat miner could absorb public attempts and double-spend within the merge depth;
- a pruning-proof IBD could be taken over;
- round-lane blocks could be applied child-before-parent;
- seat locks filled the work budget, so claims voided with their escrow burned;
- an operator at its exposure ceiling could not anchor, which voided claims in a loop;
- a readiness dip voided claims and reset probation.

The operator chose one flag day for all of them rather than a series of fences, at a height below
bond maturity (DAA 1,000). Two fences at one height would be invisible to the fork id.

## Decision

`PALW_T12_POST_LAUNCH_FENCES_V1`, armed at `PALW_T12_POST_LAUNCH_FENCE_DAA` = 750. The list is shipped
and never grows again. Each entry is hashed Some-only with the `never()` collapse.

| # | Fence | Decision | Spec |
| --: | --- | --- | --- |
| D1 | `palw_registry_resilience` | A no-capable-panel claim retries until its bind window closes. Probation resets only on failed probes from two distinct bonds (V03, V05) | 07 §7.4, 03 §3.3 |
| D2 | `palw_panel_seed_execution` | The panel seed is the anchor attempt's execution commitment (CRITICAL) | 08 §8.1 |
| D3 | `palw_heartbeat_transparent_same_chain` | Heartbeat transparency stops at the merging block's own chain (CRITICAL) | 13 §13.4 |
| D4 | `palw_reorg_strict_economic_win` | A deep reorg needs a strict economic win. A tie within two DAA ticks is decided by GHOSTDAG's order | 13 §13.2 |
| D5 | `palw_bond_maturity_early` | The 1,000-DAA maturity applies to non-genesis bonds from 750 | 10 §10.1 |
| D6 | `palw_model_sink_bound` | A model sink output is valid only when bound to an object | 15 §15.4 |
| D7 | `palw_operator_anchor` | Only the genesis operator bonds' attempts anchor panels (the stopgap for the panel-seed CRITICAL) | 07 §7.3 |
| D8 | `palw_final_lock_full_collateral` | A resolved lock is carried by the whole collateral, with a four-floor accuser reserve (V02 a) | 10 §10.4 |
| D9 | `palw_final_lock_life` | A resolved `Valid` lock lives F + 1,000, and stops being slashable when it stops being committed | 10 §10.3 |
| D10 | `palw_anchor_at_ceiling` | An operator's own attempt at its exposure ceiling still anchors | 07 §7.3 |
| D11 | `palw_slashing_evidence_utxo_genuine` | A DNS slash's UTXO side effect needs genuine evidence (MSK-26A) | 09 §9.4 |
| D12 | `palw_pruning_proof_strict_economic_win` | A pruning-proof or IBD staging commit needs a strict economic win (hf-pptake2) | 13 §13.2 |
| D13 | `palw_lane_accept_parents_first` | Tied round lanes are applied parents first, and a round block carries no EVM payload | 12 §12.3 |

## Consequences

- From DAA 750, nodes older than `c3dbaee3c` (`0e8ec984e`, `8a0810992`) are refused at the handshake.
  A node that crossed 750 on an old build must re-sync. The fence schedule went from `1000` to
  `750, 1000`, with schedule id `7c652212…`.
- Seat throughput recovered (D8, D9). Locks dated before 750 kept licence + 3,000, which led to the
  second flag day ([ADR-0155](0155-testnet-12-flag-day-daa-1300.md)).
- Panel seeding trusts the operators while D7 is armed. External producers' claims bind at operator
  attempts.
- A fresh node syncing from genesis is exempt from D12, so first sync stays with the operators' public
  nodes.

## Links

- Spec: [16 network parameters and fences](../spec/palw/16-network-parameters-and-fences.md) §16.3, and the chapters above
- Design: [design/palw/lineage.md](../design/palw/lineage.md) (flag days), [collateral.md](../design/palw/collateral.md) §6
- Records: [launch note §00](../history/networks/testnet-12/t12-launch-2026-09-25.md), [t12-panel-seed-2026-09-25.md](../history/networks/testnet-12/t12-panel-seed-2026-09-25.md),
  `contrib/t12-deploy-kit/DAA750-ROLLOUT.md`
- Code: `consensus/core/src/config/params.rs` (`PALW_T12_POST_LAUNCH_FENCES_V1`, `PALW_T12_POST_LAUNCH_FENCE_DAA`,
  `palw_t12_arm_post_launch_fences_v1`)
~~~~~~~~
<!-- ADR_SOURCE_END:0154-testnet-12-flag-day-daa-750.md -->

</details>

<a id="source-0155-testnet-12-flag-day-daa-1300"></a>

<details>
<summary>0155-testnet-12-flag-day-daa-1300.md — 原文 L1–L50</summary>

[原文を開く](../adr/0155-testnet-12-flag-day-daa-1300.md) · SHA-256: `dffa6877038381438d87b139ad25c0c1a62caf3432c83b7ff95c575de7635ed2`

<!-- ADR_SOURCE_BEGIN:0155-testnet-12-flag-day-daa-1300.md -->
~~~~~~~~markdown
# ADR-0155 — testnet-12's second post-launch flag day: two fences at DAA 1,300

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

* Status: **Accepted, armed at DAA 1,300.** Release `587cab2b0`, params fingerprint `24e1aec3…`. The
  public fleet was upgraded one node at a time on 2026-09-27 between 19:10 and 19:37 JST.
* Date: 2026-09-27
* Decided by: the operator (2026-09-27, with the live chain at DAA ~853)
* Amends: [ADR-0152](0152-account-stake-staged-reserve-and-vested-rewards.md) (SW-10's refusal and SW-8's
  anchor-block void; L-3) and [ADR-0154](0154-testnet-12-flag-day-daa-750.md) D9.

## Context

Between DAA ~624 and 749 the seats ran out of room. SW-10's eligible-stake floor (875 ‰) refused the
panel draw, and claims voided at their anchor slot: 169–172 claims, and about 541,000–551,000 MSK of
escrowed worker reward burned, with no collateral taken. The DAA-750 fences restored the room. But
locks dated before 750 still expired at licence + 3,000. At DAA 816 each genesis seat held 959–1,583 of
them, so the room would close again at about DAA 1,350–1,575.

## Decision

`PALW_T12_POST_LAUNCH_FENCES_V2`, armed at `PALW_T12_POST_LAUNCH_FENCE_V2_DAA` = 1,300. No other fence
uses that height. The next flag day's fixes are added to this list.

- **D1 — `palw_floor_refusal_retry`.** A claim whose stake draw is refused for eligibility at every
  seed on the anchor block's pre-object base re-anchors at its next slot instead of voiding. The
  qualifying refusals are `InsufficientEligibleStake`, `InsufficientEligibleBonds` and `NoOutsider`.
  The existing deadlines still apply. → spec 07 §7.4 (PALW-LC-13).
- **D2 — `palw_final_lock_life_retro`.** At the crossing block, every seat lock of an honest `Final`
  claim is re-dated to `min(expiry, max(F + 1,000, H))`. After it, a `Final` dates its seat locks at
  exactly F + 1,000. The producer's `E` recovery (F + 3,000), the court's liability records and locks
  of claims with DA history are untouched. Its prerequisite is `palw_final_lock_life` (DAA 750).
  → spec 10 §10.3 (PALW-CO-18).

## Consequences

- From DAA 1,300, nodes on `c3dbaee3c` or older are refused at the handshake. The fence schedule
  becomes `750, 1000, 1300`, with schedule id `d263d7f2…`.
- The drill crossed both fences at DAA 190 on a salted chain. All 865 locks that were Final before the
  fence matched `min(old expiry, max(F + 1,000, 190))`, and there were no voids after the fence.
- The next flag day (DAA 1,500) gets its own ADR.

## Links

- Spec: [16 network parameters and fences](../spec/palw/16-network-parameters-and-fences.md) §16.4 ·
  [07](../spec/palw/07-claim-lifecycle.md) §7.4 · [10](../spec/palw/10-collateral-and-economics.md) §10.3
- Design: [design/palw/lineage.md](../design/palw/lineage.md) (flag days), [collateral.md](../design/palw/collateral.md) §6
- Records: [launch note §000](../history/networks/testnet-12/t12-launch-2026-09-25.md)
- Code: `consensus/core/src/config/params.rs` (`PALW_T12_POST_LAUNCH_FENCES_V2`,
  `PALW_T12_POST_LAUNCH_FENCE_V2_DAA`, `palw_t12_arm_post_launch_fences_v2`)
~~~~~~~~
<!-- ADR_SOURCE_END:0155-testnet-12-flag-day-daa-1300.md -->

</details>

