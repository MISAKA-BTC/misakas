# ADR-v2-0027 — Free-prompt claim・lock・native責任期間

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 3本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0007](../rfc-v2/0007-claims-exec-and-answer-accounting.md)

## 1. この文書の役割

推論がclaimになり、lock・Final・認証・責任終了に進む経路を整理する。

## 2. 統合した判断の読み方

### 2.1

free-prompt receiptとminingへの接続を、推論成功だけで報酬を得たことにしないlifecycleとして読む。[旧ADR-0044](../adr/0044-palw-free-prompt-receipts.md)

### 2.2

lockはそれ自身のclaim・責任を持ち、finalityのlabelを全chain停止と同一視しない。[旧ADR-0109](../adr/0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md)

### 2.3

native責任期間、Panel-licensed Finalの記録専用event、認証報酬の単一legを区別する。報酬legを追加するときに既存のreporter share／feeと重複させない。[旧ADR-0184](../adr/0184-native-liability-horizon-panel-record-and-single-certified-leg.md)

## 3. 維持する境界・後続改定

FinalとSecurityCertifiedは別状態であり、後続Gate Cの意味を上書きしない。旧claimの期限・署名済み責任を文書統合で延長しない。

## 4. 隣接する判断との分担

仕事量は[ADR-v2-0026](0026-canonical-work-and-answer-accounting.md)、prefix依存は[ADR-v2-0048](0048-claim-dependencies-and-prefix-origin.md)、金融契約は[ADR-v2-0037](0037-fee-escrow-and-role-consent.md)。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0044-palw-free-prompt-receipts.md](../adr/0044-palw-free-prompt-receipts.md) | L1–L552 | L17–L19 |
| [0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md](../adr/0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md) | L1–L289 | L17–L33 |
| [0184-native-liability-horizon-panel-record-and-single-certified-leg.md](../adr/0184-native-liability-horizon-panel-record-and-single-certified-leg.md) | L1–L27 | L3–L8 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0044-palw-free-prompt-receipts"></a>

<details>
<summary>0044-palw-free-prompt-receipts.md — 原文 L1–L552</summary>

[原文を開く](../adr/0044-palw-free-prompt-receipts.md) · SHA-256: `32a738a65f90d2a2e734da0406758add1e6144449da80fdf6150b6382b22bbc6`

<!-- ADR_SOURCE_BEGIN:0044-palw-free-prompt-receipts.md -->
~~~~~~~~markdown
# ADR-0044: Free-prompt PALW — the user's own inference becomes the consensus work, certified before it mines

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **New verification-challenge scope — 2026-10-08:** [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol) is the sole proposed post-commit policy for Kernel/model conformance and claim/slice/public checks. This ADR's existing receipt-ticket/Panel beacon rules retain their historical/versioned meaning; they do not qualify a cheap BASE-0 floor or an unadmitted attempt as entropy for the new protocol. Source validity must be independent of the consuming challenge, with reviewed paid-work/unpredictability/bias bounds. No free-header hash, committee/DNS/BFT signature or timeout entropy fallback is allowed. No live rule is changed by this note.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: **Proposed.** Activates nothing, moves no shipped preset, changes no live network. It is the
engineering spec the `palw-freeprompt-v3` branch implements on top of the `palw-v2` RC substrate
(ADR-0042, PR-00…PR-10).

Date: 2026-08-20
Branch: `palw-freeprompt-v3` (cut from `palw-v2` @ `a460cdd7`).

Relates to / builds on:

- **ADR-0042** (the atomic RC ruleset; `PalwConsensusMode::ConsensusV2`) — this ADR **extends the
  V2 bundle** with a second work source. It does not add a fence: the free-prompt ruleset is a
  required part of the bundle, validated at boot, hashed into `palw_ruleset_id_v2`. A bundle
  without it is a different ruleset, checkable by fingerprint, exactly as Decision 11 demands.
- **ADR-0038 / ADR-0039** (PALW is the consensus work; ticket-not-hash; per-class DAA; bonded
  permissionless) — carried. This ADR keeps the attempt lottery *and* adds the receipt lane.
- **ADR-0037 Decisions 2–9** (state machine, panels, court seating) — the claim lattice this ADR
  certifies receipts through is the one `palw_state_v2.rs` already implements.
- The **submitted free-prompt draft** (2026-08-20, "Audit-before-Mine"): user-selected prompts,
  commit-then-future-randomness audit, one-shot certified receipts, PWU from measured work, no
  retroactive weight. This ADR adopts that structure and **repairs two of its mechanisms** (§Context)
  under the standing instruction to prefer the safer construction where one exists.

Supersedes: nothing's decisions. It **amends** ADR-0042 Decision 3's "the only algorithm a V2
network demands or accepts" from one id to two (attempt + receipt), and it **fills** ADR-0039's
open PWU-derivation item for the receipt lane only.

---

> **Amended by later ADRs (index reconciliation, 2026-09-02).** Decision 4's sentence "the
> receipt lane is weightless" is amended by [ADR-0073](0073-real-demand-work-bears-the-weight.md)
> (after its Decisions 1 and 3 the lane bears weight), and Decision 4's beacon draw is adopted by
> the attempt lane too ([ADR-0074](0074-the-attempt-is-a-claim-drawn-by-the-chain.md)). Decision 6
> is extended by [ADR-0055](0055-palw-position-is-earned-not-declared.md) (chain position is
> earned; the question is set by the block). Decision 7's CU pricing (`fp_cu_v3`, `QUANTUM_CU`,
> `PWU_PER_QUANTUM`) is **withdrawn** by ADR-0074 Decision 5 — the unit is step leaves and a
> quantum is an eighth of the class's own canonical job. The certified free-prompt set this ADR
> keeps in the build is chain state since [ADR-0075](0075-certification-is-a-consensus-object.md)
> (`fp_certified_families` / `fp_certified_classes`, read as genesis ∪ chain). The Status line's
> "activates nothing" is the 2026-08-20 state: the lane is a required part of the `ConsensusV2`
> bundle testnet-11 and devnet ship. Map: [`README.md`](README.md).

## Context — what was asked, and the two flaws in the straightforward answer

The product ask: a person uses their own LLM for their own work — code review, drafting,
summarizing — through a normal chat interface, and **that same single inference** is what mines
MISAKA blocks. The chain must not assign the prompt; the user must not run a second
mining-only inference; a receipt must be usable once; and block weight must never be revised
after acceptance.

The submitted draft gets the *shape* right: run the user's inference first, commit its trace,
select an audit panel from randomness that becomes known only after the commitment, certify,
then let the certified receipt win a block ticket drawn from later randomness. Two of its
mechanisms, however, are unsound on this codebase, and both failures are structural, not
parametric:

### Flaw 1 — "future block hash" randomness is free to grind once blocks need no inference

Today's attempt lottery (algo 4, and V2's algo 6) has one property everything else leans on:
**re-rolling any chain hash costs one inference.** `challenge = H(net ‖ pre_pow ‖ timestamp ‖
nonce ‖ class ‖ bond)` is consumed *by* the model run, so a producer who wants a different block
hash — to bias a panel draw, a ticket seed, anything derived from "the first chain block after
X" — pays a full inference per sample. `PalwAnchorFactV2`'s anchor rule is sound *because* of
this.

The draft removes the inference from block production ("nonce探索なし") and keeps deriving both
the audit-panel seed and the block-ticket seed from future block hashes. But a receipt-licensed
block with no work function over its header bytes is **costlessly malleable by its own
producer**: nonce and timestamp alone give ~2^64 free re-rolls, parent selection gives more. The
producer of the anchor block can therefore grind the panel that will audit *their own* pending
commitment, and the ticket seed that assigns the next block rights — at zero cost. That is not a
parameter to tune; it is the loss of the property the whole anchor construction stands on.

### Flaw 2 — any executor-chosen field the inference does not consume is a free grinding surface

In the attempt lottery, grinding is priced because the lottery inputs pass *through* the model.
A free-prompt job is the opposite: the model consumes **only the user's tokens** (that is the
point — F1, the prompt is not mutated), so every other identity field (`job_nonce`, bond choice,
padding) can be re-chosen *after* the inference at zero cost. Any randomness derived from the
receipt's own identity — `ticket = H(receipt_id)`, or a per-bond precommitted secret evaluated
against `receipt_id` — is therefore grindable: one inference, millions of candidate ids, submit
the winning one. The draft's own rule "the ticket must not derive from prompt or output" is
necessary but not sufficient; it must not derive from *anything the executor can re-choose
post-hoc*, which is every field except the chain's own later randomness.

### The repair, in one sentence

**Keep the attempt lane as a minority floor, and let its inference-hardened block hashes be the
only randomness source ("beacons") for both panel selection and receipt tickets; receipts are
certified through the existing claim lattice before any block may spend them.** Grinding a beacon
costs one inference per sample again; receipts are fixed on chain before their beacon exists; the
floor also solves bootstrap (a fresh chain runs on attempts until the first receipts certify) and
anti-stall (a broken receipt pipeline degrades to the floor instead of halting the chain), which
retires the draft's genesis-receipt ceremony entirely.

### What certification reuses

The draft's pipeline — commitment → future-randomness panel → replay attestations → quorum
certificate → challenge window → certified receipt — is **structurally the claim lattice that
already exists**:

```
Provisional ──beacon anchor──▶ PanelBound ──quorum receipt──▶ ReceiptLicensed ──window──▶ Final
                                                    │                                        │
                                                    ▼                                        ▼
                                                 Voided ◀──── court fraud / withholding    (certified;
                                                                                            spendable)
```

`palw_panel_v2.rs` (sortition, exclusion trio, two-sided quorum), `palw_court_v2.rs`
(proof-carrying arithmetic adjudication), `palw_state_v2.rs` (snapshot accounting, exposure,
deadline sweeps, delta apply/revert) and `palw_reward_v2.rs` apply verbatim. What ADR-0042 audits
*after* a block (its own attempt), this ADR audits *before* one (a standalone job). A
**certified receipt is a free-prompt claim in `Final`**, nothing more.

---

## Decision 1 — Two work sources, one atomic bundle

A `ConsensusV2` network with this ADR's bundle runs two block kinds:

| Lane | Algo id | Work | Share of cadence | Roles |
|---|---|---|---|---|
| **Attempt** | 6 (`POW_ALGO_ID_PALW_COMMITTED_V2`) | one fresh chain-challenge inference per ticket, exactly ADR-0042 | minority floor (`attempt_share_permille`, e.g. 150‰) | randomness beacons · bootstrap · anti-stall floor |
| **Receipt** | 7 (`POW_ALGO_ID_PALW_RECEIPT_V3`) | spending one quantum of a certified free-prompt receipt | the remainder (e.g. 850‰) | the user's own inference, mined |

Rejected alternative — a pure-receipt chain (the draft's final form): it needs a genesis
bootstrap-receipt ceremony, halts outright if the certification pipeline stalls (this network has
already lived through an 81-hour wedge; a work source with no floor is a wedge invitation), and —
Flaw 1 — has no grind-priced hashes left to derive randomness from. The floor's "waste" is the
explicit price of unbiasable randomness, and it is the same inference-work the chain's identity
is built on, not a hash fallback (ADR-0039's hash floor stays superseded).

Rejected alternative — RANDAO-style epoch seeds over certified-receipt id sets: last-actor
withholding bias is bounded but real, it adds epoch machinery, and it still needs the floor for
bootstrap and stall anyway. Beacons dominate it on every axis here.

The two lanes are **one bundle**. There is no receipt lane without the attempt lane, no fence
that enables one of them, and `palw_ruleset_id_v2` covers both (Decision 9).

## Decision 2 — The free-prompt job: user tokens in, nothing appended, total binding

```rust
pub struct PalwFreePromptJobV3 {
    pub version: u16,               // PALW_FP_V3_VERSION
    pub network_domain: Hash64,     // same value the attempt lane binds (Decision 3a of ADR-0042)
    pub class_id: PalwClassId,      // registry resolves model/runtime/tokenizer/manifest — ids are lookup keys, not passengers
    pub executor_bond: TransactionOutpoint,
    pub executor_pubkey: …,         // MUST equal the bond record's key (admission, not trust)
    pub operator_id: Hash64,
    pub anchor_block: Hash64,       // chain freshness binding: a recent chain block
    pub anchor_daa: u64,
    pub job_nonce: [u8; 32],        // uniqueness only — carries no lottery meaning (Flaw 2)
    pub tokenizer_id: Hash64,       // MUST equal the class row's tokenizer_id (cross-check, admission-refused on mismatch)
    pub prompt_token_ids_hash: Hash64,
    pub prompt_tokens: u32,
    pub decode_token_limit: u32,    // ceiling; actual decode may stop at EOG (Decision 7)
    pub max_context_tokens: u32,
    pub privacy_mode: u8,           // 1 = PublicDa; the only weight-bearing mode at v1 (Decision 8)
}
```

`fp_job_id_v3 = H(domain ‖ canonical(job))` — **every field is in the preimage.** The draft's
job-id list omitted four of its own struct fields; on this codebase "bound indirectly through
another field" is the defect class the audits keep finding, so the rule is total binding, no
exceptions. Model/runtime/manifest identities are deliberately *not* repeated in the job: the
class registration (`PalwClassRegistrationV1`) already binds them, admission checks the job
against the registered row, and carrying the same fact in two places is how the two drift apart.

**F1 — the prompt is not modified.** The model consumes `prompt_token_ids` and nothing else. The
legacy VLT executor's `new_job_input` DAA-suffix (`kaspad/src/compute.rs:428`) does not exist on
this path and must never be ported to it: freshness is bound by `(anchor_block, anchor_daa,
job_nonce)` in the job identity, outside the model's input. PALW on or off, the user's answer is
byte-identical.

## Decision 3 — The execution commitment, and certification through the existing lattice

After the inference (one inference — the same one that answered the user):

```rust
pub struct PalwFreePromptCommitmentV3 {
    pub job: PalwFreePromptJobV3,
    pub trace_root: Hash64,             // full_logits_trace_root_v2 — openable, court-grade
    pub output_root: Hash64,            // output_commitment_v2
    pub schedule_root: Hash64,          // operation_schedule_commitment
    pub decode_tokens_executed: u32,    // ≤ decode_token_limit; EOG is a real stop (Decision 7)
    pub stop_reason: …,                 // ExactBudgetReached | EndOfGeneration
    pub cu: u128,                       // recomputed by every validator from the CU rule — a claim, checked, never trusted
    pub trace_manifest_root: Hash64,    // DA obligation trio, as PR-06 defined it
    pub trace_chunk_count: u32,
    pub trace_retention_daa: u64,
}
// + envelope { commitment, signature }  — identity is fp_claim_id_v3 = H(canonical(commitment)),
//   NOT the signature bytes (ML-DSA-87 signatures are not unique; ADR-0042 Decision 3c).
```

The commitment (with the **prompt token ids carried whole** — PublicDA, Decision 8) rides an
overlay transaction. When its block is accepted, the state transition folds a
`FreePromptCommitted` object: a claim is created at `Provisional`, exposure is reserved against
the executor's bond exactly as an attempt claim reserves (one shared per-bond ceiling across both
lanes — two lanes with separate ceilings is doubled leverage), and from there the existing
machinery runs unmodified:

- **PanelBound** — panel derived by `derive_panel_v2` from the claim's **beacon** (Decision 4),
  exclusion trio and operator dedup as PR-06 built them;
- **ReceiptLicensed** — `validate_receipt_quorum_v2`, two-sided (Valid / producer-Unavailable);
  a panel seat replays the job from the on-chain prompt tokens and the class's pinned runtime —
  the same `--mode v2-job`-style replay the VLT lane already performs;
- **court** — `palw_court_v2` verbatim: proof-carrying arithmetic refutation against the
  committed trace root; `DoesNotAdjudicate` refuses the close; window_court backstops;
- **Final** — the challenge-window sweep. **Final IS the certified receipt.** `Voided` forfeits
  and slashes through the existing paths.

No new certification machinery is invented by this ADR. That is its main safety argument: the
panel, quorum, court, exposure and sweep code paths are the ones already carrying adversarial
tests.

## Decision 4 — The beacon rule: only attempt blocks carry randomness

```
beacon(slot) = the FIRST chain block B with B.daa ≥ slot whose algo id is the ATTEMPT id (6)
```

- **Panels**: a free-prompt claim's anchor slot is `accepted_daa + anchor_delay` (as V2), and its
  anchor is `beacon(slot)` — carried as `PalwBeaconFactV3 { beacon_block, beacon_daa,
  prev_attempt_daa }`, the `PalwAnchorFactV2` shape with one added obligation: `prev_attempt_daa`
  is the DAA of the last attempt-class chain block *before* the slot, so "first attempt block at
  or after the slot" is checkable, and every receipt-class block in between is structurally
  irrelevant. Under this ADR's bundle the **attempt lane's own claims anchor on beacons too** —
  one anchor rule per network, not one per lane.
- **Tickets** (Decision 5): drawn from `beacon(final_daa + receipt_maturity_daa)`.

Why this closes Flaw 1: an attempt block's hash is downstream of its commitment root, which is
downstream of `challenge = H(… ‖ timestamp ‖ nonce ‖ …)` — the finalizer consumes
`Expand(commitment_root)`, so **every alternative beacon sample costs one full inference**, and
withholding a winning attempt to re-roll additionally forfeits its block reward. Receipt blocks,
whose headers are costlessly malleable, can never be anchors. This is also why the floor share
has a lower bound (Decision 9): beacons must keep arriving.

**Nothing may derive randomness from receipt-block hashes.** That sentence is an invariant
(F15), not a guideline.

## Decision 5 — Quantized one-shot tickets

Certification prices the work (`cu`, Decision 7); the lottery consumes it in **uniform quanta**:

```
quanta(claim)          = min( ⌊cu / quantum_cu⌋ , max_quanta_per_receipt )
ticket(claim, q)       = leading 128 bits of H(FP_TICKET_DOMAIN ‖ network_domain ‖ beacon_block ‖ claim_id ‖ q_le)
eligible(claim, q)     ⟺ palw_ticket_admits_v1(ticket, receipt_target[class])
```

- One quantum = **one draw**, against the single beacon `beacon(final_daa +
  receipt_maturity_daa)` — fixed by the chain, after the claim is irrevocably `Final`. The
  executor cannot re-choose anything the draw depends on (Flaw 2 closed: claim id is fixed on
  chain before the beacon exists; grinding `job_nonce` at job time cannot aim at a beacon that
  does not exist yet, and grinding the beacon costs inferences).
- One winning quantum = **one block**, spendable once per candidate chain (the spent set lives in
  the claim's state; a fork may spend it too — that is the UTXO double-spend analogy, resolved by
  fork choice, never by a node-global cache).
- A win must be used promptly: the spending block's DAA must lie in
  `[beacon_daa, beacon_daa + receipt_use_window_daa]`. An unused win expires — a weight claim
  from the deep past is a reorg lever, not a savings account.
- Uniform quanta make the lottery math trivial (flat per-class target, the existing 128-bit
  ticket space and `palw_ticket_admits_v1` reused as-is), give a big job and ten small jobs the
  same expected block count per CU, and bound the per-receipt jackpot (`max_quanta_per_receipt`)
  — the draft's `MAX_PWU_PER_RECEIPT` split, made the primitive instead of the patch. A
  sub-quantum job (`cu < quantum_cu`) is not carried on chain at all: the state refuses a
  zero-quanta commitment, because a claim that can never act is exposure reserved for nothing
  and dead weight in every root.
- Expected value is linear in certified CU; variance favors nobody. Pooling needs no protocol:
  a gateway pointing at a shared bond *is* a pool (the bond owner is the accountable party).

Public predictability: once the beacon lands, everyone can enumerate the winning quanta of the
next window. That is an accepted v1 trade (producers are bonded operators; the window is short;
a DoS'd producer costs the chain one slot, not liveness). A private-eligibility layer
(per-bond precommitted secret Merkle trees) is sketched under *Not decided*.

## Decision 6 — The receipt block (algo 7): admission a full node runs with no model

A receipt block's post-PoW header extension carries the spend envelope (the
`palw_block_commitment.rs` carriage shape):

```rust
pub struct PalwReceiptSpendUnsignedV3 {
    pub version: u16,
    pub network_domain: Hash64,
    pub claim_id: Hash64,          // the certified commitment being spent
    pub quantum_index: u32,
    pub beacon_block: Hash64,      // the draw this spend claims
    pub producer_bond: TransactionOutpoint,   // MUST be the claim's executor bond
    pub producer_pubkey: …,
}
// spend_id_v3 = H(canonical(unsigned));  L1 tag = Expand(spend_id)  — the finalizer consumes it,
// so a receipt block's identity is total over its spend, and the signature is a witness.
```

Admission, stateless: version · canonical encoding · size bounds · `spend_id` recompute ·
signature under the carried key · L1-tag/finalizer recompute · network domain. Stateful, against
the candidate chain's `PalwChainStateV2`:

```
1. claim exists and is a free-prompt claim in Final
2. quantum_index < quanta(claim), and not in the claim's spent set
3. beacon_block == beacon(final_daa + receipt_maturity_daa) on this chain (beacon fact checked)
4. ticket(claim, q) admits under receipt_target[class] at the CANDIDATE point
5. block DAA within [beacon_daa, beacon_daa + receipt_use_window_daa]
6. producer_bond == the claim's executor bond, Active, not in withdrawal
7. the bond record's pubkey == the carried producer_pubkey
8. class Active (not frozen) at the candidate point
```

Item 4's target point, precisely: the BEACON fixes the draw (historical, grind-priced); the
TARGET is the spending block's own difficulty context, read at the candidate point like every
difficulty check on this chain — past targets are not state. A marginal ticket can flip
eligibility across a retarget boundary inside its use window, deterministically and identically
on every node, with no grinding surface. (An earlier draft of this ADR said "as of the beacon's
chain point"; the implementation is where that was discovered to demand a target history nobody
keeps, and this text now matches the code.)

Weight: a receipt block contributes `pwu_per_quantum` at **`Final` stage immediately** — safe
weight at acceptance. Everything the block's validity rests on is prior chain fact; there is
nothing left to dispute, so there is no ramp, no `Provisional` stage, and **no post-acceptance
weight revision** (the draft's F10, achieved by construction). For the same reason the receipt
block's coinbase needs **no escrow ladder**: `palw_reward_v2`'s escrow exists because an attempt
block can still be voided; a receipt block cannot. Ordinary coinbase maturity applies.

The header `nonce` remains a uniqueness field with **no work meaning and no randomness meaning**
(Decision 4). Full nodes run: hashes, signatures, one beacon-fact check, one set lookup, one
ticket comparison. No model, no exception — algo 7 is deliberately *not* in `is_palw_algo`'s
inference-priced set, and a test pins that a node with no runtime installed validates receipt
blocks completely.

## Decision 7 — Pricing: CU from the executed shape, conservative by construction

**Withdrawn by [ADR-0074](0074-the-attempt-is-a-claim-drawn-by-the-chain.md) Decision 5.**
旧 prefill/decode の CU 係数表は実装しない。恣意的な係数で class 間の価格を決めず、step leaves と class の canonical job を基準にするため。

## Decision 8 — Data availability and privacy, v1

| Mode | Prompt tokens | Weight-bearing |
|---|---|---|
| `PublicDa` (v1) | carried whole in the commitment transaction (≤ `max_prompt_tokens`, ≤ the existing 4096-token frame budget) | **yes** |
| encrypted / panel-keyed | future ADR | no — zero weight until specified |

PublicDA-on-chain does to the prompt-withholding failure mode what carrying the output token list
whole did to output disputes: it deletes the failure mode instead of adjudicating it. The panel
replays from chain data alone; there is no "producer hid the prompt" arm. The **trace** stays
off-chain under the PR-06 DA obligation trio (manifest root, chunk count, retention deadline) —
withholding a requested opening defaults the producer, as already specified.

The gateway MUST show the user, before first use, that PublicDA prompts are permanently public.
A daily-use tool that silently publishes prompts is a betrayal dressed as a feature.

## Decision 9 — Bundle extension, invariants, and what moves

`PalwConsensusParamsV2` gains one required field:

```rust
pub freeprompt: PalwFreePromptParamsV3 {
    receipt_algorithm_id: u8,          // must equal POW_ALGO_ID_PALW_RECEIPT_V3
    attempt_share_permille: u16,       // the floor; the receipt lane gets the remainder
    quantum_cu: u128,
    pwu_per_quantum: u64,
    cu_prefill_weight: u32,
    cu_decode_weight: u32,
    max_quanta_per_receipt: u32,
    max_prompt_tokens: u32,
    max_decode_tokens: u32,
    receipt_maturity_daa: u64,
    receipt_use_window_daa: u64,
    max_beacon_gap_daa: u64,           // a measured/declared bound, enforced against windows below
}
```

New startup invariants (a node that fails any does not boot, as Decision 1 of ADR-0042):

```
receipt_algorithm_id == POW_ALGO_ID_PALW_RECEIPT_V3
0 < attempt_share_permille < 1000            (a zero floor has no beacons; 1000 has no receipts)
quantum_cu, pwu_per_quantum, cu_decode_weight, max_quanta_per_receipt > 0
max_prompt_tokens ≤ PALW_V2_MAX_PROMPT_TOKENS, and prompt+decode fit the trace-event cap
receipt_use_window_daa > 0
anchor_delay + max_beacon_gap_daa < window_bind     (a late beacon must still bind inside the window)
receipt_maturity_daa ≥ reorg_margin_daa             (a draw beacon deeper than the reorg margin)
```

Per-source retarget: the state's target table becomes keyed by `(class, lane)`. Attempt targets
retarget exactly as PR-09 landed (share of realized production); receipt targets retarget with
expectation `share[class] · (1000 − attempt_share) / 1000 · span blocks`, through the same
`retarget_over_span_v1` fold. Expectation is over **realized production**, so one lane starving
never silently re-prices the other; the floor share is what guarantees the attempt lane's
expectation is nonzero.

The mode's algo demand becomes a set: `{6, 7}` under this bundle (template side chooses per lane;
header side accepts either and then applies that lane's admission). The four wired seam sites
keep their `Option<u8>` shape until the wiring PR swaps them to the set form — every shipped
preset answers `None` before and after, pinned by the existing seam test.

`palw_ruleset_id_v2` moves for any bundle (the struct grew). No shipped preset carries a bundle,
so every pinned preset fingerprint is unchanged — the existing golden-vector tests prove it.

## Decision 10 — The user pipeline: one inference, an answer and a commitment

Off-consensus, but specified here because F1 lives or dies in it:

```
user app ──POST /v1/chat/completions──▶ misaka-palw-gateway
    │  canonical template render (frozen text transform, gateway-side, template id pinned)
    │  canonical tokenize (worker mode, pinned GGUF tokenizer — the class's tokenizer_id)
    ▼
palw-worker v3-job  ──▶  answer (output ids + rendered bytes, returned to the CALLER)
                    └──▶  projection (trace/output/schedule roots, counts)  ──▶ commitment tx
```

- The worker's v2 rule "no rendered output leaves the process" is **amended for the v3 job
  response only**: the response envelope carries the answer to its caller. The *projection* still
  never carries raw text — it is the consensus-compared object and stays hashes-and-counts.
- Tokenization and template rendering are the canonical front end the tree currently lacks. The
  template is a frozen string transform applied gateway-side (its sha pinned in the class
  profile); tokenization is a new worker mode against the pinned GGUF. Both are new identities —
  the v2 class pins `"none/token-ids-input/v2"` and is not edited.
- v1 is non-streaming (one frame in, one frame out, the agent's Phase-A single slot respected;
  the gateway queues). A streaming side channel is *Not decided*.
- The gateway never re-runs the inference for mining. There is no second lane. That property is
  testable: the worker executes once per job id, and the commitment's roots are byte-derived from
  the same execution that produced the returned answer.
- Two properties that must not be conflated (the gateway smoke's first draft did, instructively):
  re-asking the same conversation yields the SAME answer — F1, the fresh `job_nonce` never
  touches the model's input — and a DIFFERENT trace root, because every trace event binds the
  job id (nonce included). The second is the anti-replay binding: one job's trace can never be
  presented as another's. "Same input, same trace" holds per job, never across jobs.
- The v1 template is plain-marker text, deliberately: the pinned tokenizer runs with
  `parse_special = false` (untrusted text must never smuggle control tokens), so a ChatML
  template would tokenize its own markers as prose. Consequence, measured: the model rarely
  emits EOG under it, answers end at the decode ceiling, and the gateway's display-layer stop
  guard trims presentation while the commitment covers every executed token. A ChatML profile
  with segment-wise special tokenization is a future class identity.

## Invariants

```
F1   The user's prompt token sequence is never modified by PALW metadata (no suffix, no wrap).
F2   Canonical token ids, not text, are the execution identity; the tokenizer is the class's.
F3   Model/runtime/tokenizer/template/shape are bound via the registered class row; the job
     carries the class id and the admission cross-checks, never a second copy of the facts.
F4   Panel randomness becomes known only after the commitment is irrevocable on chain.
F5   Ticket randomness becomes known only after certification (Final) is irrevocable on chain.
F6   No randomness derives from prompt, output, trace, receipt id alone, or any executor-chosen field.
F7   CU/PWU is recomputed by every validator from the committed shape under the bundle's rule;
     a mismatched claim is invalid, never clamped.
F8   One quantum is spendable at most once per candidate chain; spends are branch-scoped state.
F9   A claim below Final licenses no block.
F10  A receipt block's weight is safe at acceptance and never revised.
F11  Same genesis · DAG · bodies · evidence ⟹ same receipt state, same weights, same tip.
F12  A full node validates receipt blocks with no model runtime, structurally (algo 7 is not an
     inference-priced algo; the CI dependency edge test already forbids the import).
F13  Per-bond exposure is one ceiling across attempt and free-prompt claims.
F14  An expired draw (outside the use window) licenses nothing, forever.
F15  Only attempt-class (algo 6) chain blocks are randomness anchors — for panels AND tickets.
F16  The attempt floor share is bounded away from zero by a startup invariant.
```

## Threat disposition

| Attack | Closed by |
|---|---|
| Prompt grinding for favorable tickets | F5/F6 — the draw beacon postdates Final; D5 |
| `job_nonce`/identity grinding (Flaw 2) | same — no randomness derives from the id; D5 |
| Anchor/beacon grinding by block producers (Flaw 1) | D4 — beacons are inference-priced; receipt hashes carry no randomness (F15) |
| Panel selection bias toward colluders | D4 + `derive_panel_v2`'s registry-resolved exclusion trio |
| Receipt replay / double-spend | F8 branch-scoped spent set; cross-network by `network_domain`; cross-class/bond by total binding |
| Fake commitment, never served | exposure reserve at commit + panel two-sided quorum (ProducerDefaulted) + DA default |
| Fabricated trace | court: proof-carrying arithmetic refutation against the committed root |
| PWU inflation via workload shape | D7 conservative pricing invariant + quantization + per-class epoch budgets (existing) |
| Tiny-job spam | commitment tx fees + sub-quantum certification draws nothing |
| Stockpiled wins injected later | F14 use window |
| Receipt pipeline outage → chain halt | D1 attempt floor |
| Weight revision as reorg lever | F10 — nothing to revise |
| Bond leverage across lanes | F13 shared exposure ceiling |

## Implementation ladder

Substrate first, consensus-inert, exactly as PR-01…PR-10 were staged; no shipped preset moves at
any point before a dedicated RC/devnet preset PR.

| PR | Content | Done when |
|---|---|---|
| **FP-00** | this ADR | — |
| **FP-01** | `palw_freeprompt_v3.rs`: job/commitment/spend objects, total-binding ids, CU rule, quanta, tickets, beacon fact — golden vectors + per-field mutation tests | mutating any field moves the id; domains separate from every V2 tag |
| **FP-02** | algo id 7 registered, demanded by nothing; mode-level `allowed_algo_ids` (unwired) | every preset's accepted set unchanged, pinned |
| **FP-03** | state machine: free-prompt claims through the lattice, shared exposure, spent sets, per-lane targets + retarget, sweeps | delta replay/revert differentials green; equal-DAG differential green |
| **FP-04** | receipt-block admission (stateless + stateful) + spend header carriage | every admission item has a named red test |
| **FP-05** | bundle extension + startup invariants + ruleset id | invariant-refusal table extended; preset fingerprints byte-stable |
| **FP-06** | worker v3: tokenize mode, v3-job (EOG stop, answer in response), new shape/profile ids | one execution yields answer + projection; v2 golden gate untouched |
| **FP-07** | `misaka-palw-gateway`: /v1/chat/completions → template → tokenize → job → answer + commitment emission | F1 test: bytes to the model == canonical tokens of the user request, nothing else |
| **FP-08** | pipeline wiring: objects from carriage, beacon facts from stores, seam swap to the two-id set, trace DA retention + the signer/executor rail, store layer (shared with V2's own pending wiring) | devnet blocks of both kinds validate cross-node |
| **FP-09** | fleet drill + measured params + RC/devnet preset | soak outputs fill the bundle |

This branch lands FP-00 through FP-07. FP-06/07 are measured on the real pinned model
(Qwen3.5-2B): one inference produced the user's answer AND the commitment roots; the Text arm
and the TokenIds replay arm reached byte-identical roots; repeated runs were byte-identical on
every consensus-visible field; the OpenAI-compatible surface answered with the roots and the CU
in-band and the artifact in the outbox.

## What this ADR does not decide

- **Measured parameters** — shares, quantum size, CU weights, windows are soak/calibration
  outputs, like every other number in the bundle.
- **Streaming** — the side-channel token stream for the gateway (the commitment still only exists
  at completion; the stream is UX, not consensus).
- **Private eligibility** — hiding the winner until use (per-bond precommitted secret Merkle
  trees over round indices: secrets fixed at bond registration, revealed in the spending block,
  verified against the bond's committed root; grinding at registration is aimless because neither
  beacons nor claim ids exist yet). v1 accepts public draws.
- **Encrypted prompts** — `PublicDa` is the only weight-bearing mode; ML-KEM panel-keyed DA and
  ZK forms are their own ADR, zero-weight until then.
- **KV-cache / multi-turn continuation** (`ContinueFrom{parent_receipt}`) — priced continuation
  without double-counting cached prefill; needs its own trace semantics.
- **Receipt delegation/transfer** — receipts stay bound to the executing bond; pooling happens at
  the gateway/bond layer.

## Number hygiene

This is ADR-0044; 0043 (`palw-v2` state-root ordering) is the last committed on the base branch.
If a concurrent branch also claims 0044, the tie breaks by content-keep-and-renumber-the-later
writer, per ADR-0036 Decision 5.

## Mission alignment amendment — 2026-10-07

PublicDaの公開証拠原則を将来の全報酬profileに適用する。後続PanelDaが同じweightを持つという旧設計は、公開訴追可能性が満たされない限り新設計の根拠にしない。ローカル推論の有用性・privacy告知と訴追可能性を別に示す。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* 将来の報酬・mineability・consensus weightのgateには、対象profileのfresh non-seat public verifierが公開証拠からlocalizeしてobjective convictionまで完結する証拠を追加する。static cost、kernel catalog、family certificate、seat readiness、正直なFinalだけでは代替できない。未対応profileはこの新gateを閉じたままとする。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0044-palw-free-prompt-receipts.md -->

</details>

<a id="source-0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause"></a>

<details>
<summary>0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md — 原文 L1–L289</summary>

[原文を開く](../adr/0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md) · SHA-256: `ce03b28c91f721226682a137c4b57a7146ed5c1e56a5e61612b1726ac58c78d2`

<!-- ADR_SOURCE_BEGIN:0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md -->
~~~~~~~~markdown
# ADR-0109 — A lock is its own claim, and finality is a label, not a pause

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


* Status: PROPOSED 2026-09-11, at the operator's request ("今の EVM ブリッジや送金がスムーズに liveness
  に行かない根本的原因を分析して liveness を優先にする設計に修正を行なって"). **Decisions 1–5
  IMPLEMENTED the same day** (§9), consensus-inert: no block-validity rule, no `Params` field, no
  fingerprint moves. A node takes it by an ordinary rebuild and restart; a node that never takes it
  validates every block the new nodes make.
* Builds on: [0020](0020-selected-parent-evm-lane.md) (the EVM lane executes on the selected chain;
  B's own payload is executed by its selected child), [0009](0009-dns-probabilistic-finality.md)
  (DNS finality is probabilistic and two-resource; "liveness depends on both PoW miners and PoS
  validators while the overlay is active"), [0105](0105-a-heartbeat-never-turns-a-bonded-block-red.md)
  (the 09-10 heartbeat trap, during which the bridge stood paused), [0072](0072-the-ticket-is-the-execution.md)
  (a draw is the execution; a template is fixed for the whole draw), the 2026-09-10 bridge-freshness
  fix (`dns_finality_fresh_for_bridge` measured in blue score beyond the anchor's structural
  distance), and ADR-0108's rule that a node extension is policy a build carries, never a rule the
  chain enforces.
* Amends: the producer policy and the RPC policy of kaspa-pq EVM Lane v0.4 §9.2 (deposit claims)
  and §15 (template assembly). The claim's *validity* (`validate_one_deposit_claim`) is unchanged.
* Supersedes nothing.

## 0. The sentence this ADR is

**A deposit lock that the chain has accepted is claimed by every producer, unasked, in the next
template each of them builds; the chain's finality machinery labels what is final and never decides
what is included; a transaction the chain will refuse is refused at the mempool, out loud; and the
one bound that remains is the block interval, which this ADR measures and names rather than hides.**

## 1. What was measured (2026-09-10/11, testnet-11, main `40ac431b`)

**1.1 The bridge's latency is not the chain's latency.** The lane's protocol asks the *depositor* for
a second act after the first: a `submitEvmDepositClaim` RPC, which the node can only take once the
lock's transaction is ACCEPTED on the selected chain (before that the outpoint is "absent/spent in the
virtual UTXO set"). The claim is then queued in the node's memory, gossiped on a low-priority relay
lane, and re-validated into the *next* template each producer builds. Only a chain block's payload
executes (ADR-0020), so a claim that reached one producer and not another executes only if the
first one's block becomes the chain block.

The 1,000,000 MSK deposit of 2026-09-11 (`c57eec8d…`), read from the chain:

| when (UTC) | what | DAA |
|---|---|---|
| 06:26:22 | lock tx carried by block `2a703493…` (template time) | 3372 |
| 06:54:20 | accepted by chain block `ac6ee446…` | 3373 |
| 07:07:22 | chain block `b90571fe…`, EVM payload **empty** | 3376 |
| 07:22:05 | chain block, EVM payload **empty** | 3377 |
| ≈07:30 | the wallet's claim first *accepted* by the RPC (the wallet re-asked only while its popup was open; the operator reloaded a wallet that re-asks from the background) | — |
| 07:43:12 | the first template built after the claim reached a producer — chain block at DAA 3379 carries it (template time) | 3379 |
| ≈08:15 | that block arrives and is the chain block; the credit is at `latest` (read 08:16Z: 1,000,000 MSK) | — |

**Lock in a block at 06:26Z, credit at 08:15Z: 1 h 49 min**, of which the chain needed one interval
after someone asked (07:43 → 08:15) and everything before that was waiting for the ask.

The previous day's 5 MSK deposit (`8e5315b3…`) took six minutes — claim RPC 13:27Z, credited by the
chain block at DAA 3297 (template 13:33:53Z) — because a template happened to be built just after
the relay. The difference between six minutes and an hour is not the chain; it is whether someone
asked at the right moment.

**1.2 A template is one per draw, and a draw is long.** A QWEN36 draw on the fleet's hosts takes ≈17
min; the template (and the EVM payload in it) is fixed when the draw starts (ADR-0072: the ticket is
the execution). Anything that arrives mid-draw waits for the next draw. Chain blocks on 2026-09-11
04:59–07:22Z came at intervals of 22.5, 26.2, 14.6, 23.3, 28.0, 13.0 and 14.7 min — **median 22.5 min,
mean 20.3**. A transfer needs the lock in a block (1 interval), accepted (the next chain block), a
claim in a template built after that (up to 1 draw), that block (1 interval), and that block to be
the chain block. **Under the current design that is 3–4 chain blocks after the depositor's second
act; under this ADR it is 2 chain blocks after the lock, with no second act.**

**1.3 The finality gate pauses inclusion.** `bridge_finality_is_fresh` is producer policy: when the
DNS-confirmed anchor sits more than `dns_bridge_max_anchor_distance_blue_score` (14 on testnet-11)
below the sink, or DNS is not confirmed at all, the template carries an **empty EVM payload** — no
claims, no EVM transactions, no EVM coinbase — and the claim RPC answers `EVM bridge is paused`. On
2026-09-10 the heartbeat trap (ADR-0105) kept DNS unconfirmed from 11:41Z to 13:26Z: **1 h 45 min in
which every claim was refused and no EVM transaction could be included**, while the base ledger kept
producing blocks. The partition of the same day did the same for hours. On 2026-09-11 the pause did
not fire (0 `EVM lane producer paused` lines on all eight fleet logs) — because nothing was queued
during a stale window, not because the window did not exist.

The gate never made a credit *final*: a claim landing at blue N with the anchor at N−8 is exactly as
reorgable as one landing with the anchor at N−20. What the gate did was couple the EVM lane's
*liveness* to the validators' — the thing ADR-0009 lists as a property to be stated honestly ("liveness
depends on both PoW miners and PoS validators"), not one to be desired.

**1.4 A refused transaction is refused silently.** The 2026-09-11 00:57Z lock `6388c588…` spent eight
outputs that are PALW producer bonds (the depositor's wallet chose its largest coins). The mempool
admitted it, a block carried it, and the merge refused it (`SpendsNonReleasableBond`) — correct, and
invisible: no error to the submitter, no event, the wallet waited 90 minutes before concluding
anything. The mempool knows the locked set (`palw_locked_bond_outpoints_v2` serves it over RPC) and
did not consult it.

**1.5 Nothing says where a claim is.** No RPC reports whether a lock is claimed, queued here, or
relayed; the node logs nothing when it queues one. The wallet of 2026-09-11 infers the fate of a lock
from the explorer's acceptance record and the node's sink blue score. The explorer shows no bridge at
all (the operator's third request of the day).

**1.6 The bound that stays.** Block cadence is PALW: a block is a successful draw, a draw is an
inference, and the fleet's producers draw for ≈17 min each. That is a property of the network's
hardware and its ceilings (ADR-0092, 0097), and a fast lane exists — the PALW-BASE-0 floor family
draws in seconds (chain block `ac6ee446…` at DAA 3373 is one). This ADR does not change cadence; it
removes everything the bridge added on top of it, and §8 names what would change cadence.

## 2. The causes, in one table

| cause | kind | fixed by |
|---|---|---|
| the claim is a second, depositor-initiated act, taken only after acceptance | protocol *policy* (the claim op and its validity are unchanged) | D1 |
| the claim lives in one node's memory, reaches producers by low-priority gossip, is evicted after 600 absent templates, dies with a restart | node policy | D1 |
| a stale DNS anchor empties the EVM payload and refuses every claim | producer + RPC policy | D2 |
| a spend of a locked bond is admitted to the mempool and dropped at the merge | mempool policy | D3 |
| `safe` = sink; the only finality-bearing tag is the pruning point | RPC policy | D4 |
| a template is one per draw; a draw is ≈17 min; a chain block every ≈20 min | PALW cadence | not here (§8) |

## 3. Decisions

### Decision 1 — Every accepted lock is claimed by every producer, unasked

Consensus keeps a node-local **deposit-lock index**: the set of `EVM_DEPOSIT_LOCK` outputs currently
in the virtual UTXO set, with what a claim needs (amount, EVM address, tip, timeout, the lock's DAA
score). It is staged in `commit_virtual_state` from the *same* accumulated UTXO diff the virtual set
is written from — an added output whose script parses as a lock is inserted, a removed outpoint is
deleted — so it moves with every virtual change, reorgs included, atomically with the set it
mirrors. It is rebuilt once from the virtual UTXO set on a node whose database predates it (a
marker says whether it was built), and rebuilt whenever the virtual set is replaced wholesale (a
pruning-point import).

The template path unions the mining manager's queued claims (the RPC and relay lane, kept for
compatibility) with the index's, oldest lock first, and hands the union to the unchanged
`prepare_deposit_claims` under the same virtual read lock, so every claim is validated against the
generation the template is built from — no TOCTOU, no new validity rule, the per-block caps as
before. **The depositor's second act becomes optional; a restart, a full queue, an evicted claim or
a relay that never arrived no longer decide whether a deposit completes.**

### Decision 2 — Finality is a label the reader asks for, not a pause the producer imposes

`Config::evm_bridge_finality` is `Label` (default, every network) or `Pause` (the behaviour before
this ADR). Under `Label` the producer includes claims, EVM transactions and its EVM coinbase whatever
the DNS anchor's distance, and `submitEvmDepositClaim` queues a claim whatever the anchor's distance.
`--evm-bridge-devnet-unpaused` becomes a spelling of `Label` (kept so a devnet script keeps working).
Block validity never read the gate; it still does not.

What the gate was protecting is now carried by the *reader*: Decision 4's `safe` tag. A wallet that
wants a credit it can act on externally asks for `safe`; a wallet that wants to pay gas asks for
`latest`. That is the Ethereum model during an inactivity leak — the head advances, finality waits —
and it is what ADR-0009's public-claim discipline already permits saying.

### Decision 3 — A spend the chain will refuse is refused at the mempool

`validate_mempool_transaction` refuses a transaction whose input is a PALW producer bond the
registry holds locked at the virtual tip, with the same error the merge would give
(`SpendsNonReleasableBond`), so the submitter learns at `submitTransaction` what the chain would have
told nobody. Policy: a block carrying such a transaction is still valid and the merge still skips
the transaction, exactly as before.

### Decision 4 — `safe` is the DNS-confirmed anchor

`CanonicalEvmHeads.safe` follows `DnsState.last_dns_confirmed_anchor` when that anchor carries an
EVM result, and the sink otherwise (as before). `latest` stays the sink; `finalized` stays the
pruning point. The eth RPC already resolves `safe` from the heads store, so `eth_getBalance(addr,
"safe")` and `eth_getBlockByNumber("safe")` answer with the two-resource-confirmed state without a
further change.

### Decision 5 — The claim RPC and the relay lane stay, as accelerators

`submitEvmDepositClaim` still queues and gossips a claim (a producer whose index lags — a node
mid-IBD — can still be handed one). Its refusals are the lock's own (absent, not a lock, tip over
amount, past timeout); staleness is no longer one. The wallet's background relay of 2026-09-11 stays
harmless and becomes unnecessary.

## 4. What this costs

The index is one row per unclaimed lock — on testnet-11 today, one. Staging it is one script-class
check per added virtual output (`ScriptClass::from_script`, a length-and-opcode test). Reading it at
template time is one store iteration under a lock the template already holds. The rebuild is one
pass over the virtual UTXO set, once per database. Nothing here adds a byte to a block or a rule to
its validity.

## 5. Security amendments

* **SA-1 — the index never decides validity.** Every claim the index yields passes the unchanged
  `prepare_deposit_claims` against the template's own view; an index row that is wrong (a bug) makes
  a claim that is dropped, never a block that is invalid.
* **SA-2 — the index is derived, never declared.** Rows come only from the virtual UTXO diff or from
  a scan of the virtual set; no RPC writes it. A relayed claim is still re-resolved against the live
  view (the relay flow is unchanged).
* **SA-3 — `Label` moves no finality boundary.** `finalized` is still the pruning point; `safe` is
  the DNS-confirmed anchor, which is exactly the boundary the old gate measured its distance from.
  A reader who acted on `latest` before this ADR was acting on an unconfirmed head then too.
* **SA-4 — a paused-mode operator keeps the old behaviour byte for byte.** `Pause` reproduces the
  previous template and RPC decisions, so a network that wants the coupling can set it per node.
* **SA-5 — the mempool refusal is the merge's own predicate** (the locked set the registry serves
  over RPC, read at the virtual tip), so a transaction the mempool refuses is one the merge would
  have skipped; it cannot refuse a spend the chain would accept, other than by the one-block lag
  between the tip the mempool read and the block that accepts.

## 6. Invariants the tests hold

* **I-1** A lock in the imported virtual UTXO set is claimed by the next template with *no* queued
  claim (the index path), under `Label`; under `Pause` with a stale anchor the payload stays empty,
  and with a fresh anchor it carries the claim (the pre-ADR behaviour, unchanged).
* **I-2** Staging: an added lock output enters the index, an added non-lock output does not, a removed
  outpoint leaves it; the index equals the set of lock outputs in the virtual set after a rebuild.
* **I-3** A template's index claims are ordered oldest lock first and deduplicated against queued
  claims by outpoint.
* **I-4** `safe` is the DNS-confirmed anchor when it carries an EVM result, the sink otherwise.
* **I-5** A mempool transaction spending a locked PALW bond is refused with
  `SpendsNonReleasableBond(outpoint)`; the same transaction with a free input is admitted.

## 7. What is deliberately not decided

* **The same-block claim** — a lock and its claim in one chain block would cut one more interval. It
  is a validity change (the claim view excludes the block's own body, `validate_evm_deposit_claims`)
  and therefore a fence; ADR-0108 classifies it as a ruleset change. Not here.
* **A deposit-status RPC** (`getEvmDepositStatus`: present / queued here / claimed in / refund at).
  The explorer's bridge view (built alongside this ADR from chain data) and the wallet's
  explorer-plus-sink inference cover the need today; the RPC is the right home and is a follow-up.
* **Cadence.** Floor-family producers (PALW-BASE-0, seconds per draw) raise the block rate without a
  rule change; whether the fleet should run more of them is an operations decision with an economics
  side (the floor's share) that ADR-0054/0107 govern.
* **Mainnet's `evm_bridge_finality` default** is `Label` like every network; the launch preset can say
  `Pause` if the launch wants the coupling.

## 8. Number hygiene and implementation record

`docs/adr/README.md` on `main` at `40ac431b` says the next free number is 0108; 0108 is resident on
`feat/adr-0108-extension-envelope` (this operator's session, same day, unpushed) and its README says
0109. This ADR takes **0109**; **the next free number is 0110.**

Implementation, 2026-09-11, on `feat/adr-0109-bridge-liveness` — the measured results are in §9.

## 9. Measured results

Implemented 2026-09-11 on `feat/adr-0109-bridge-liveness` (from `main` `40ac431b`), nothing pushed.

**Where it lives.** `EvmDepositLockRecord` and `EvmBridgeFinalityPolicy` in `consensus/core/src/evm/mod.rs`;
`Config::evm_bridge_finality` + `evm_bridge_finality_effective()`; the store `DbEvmDepositLockStore`
(`consensus/src/model/stores/evm.rs`, DB prefixes **228** rows / **229** built-marker — 254 and 255 were
taken, which the compiler said as E0081); in `processor.rs`: `stage_evm_deposit_locks` (called from
`commit_virtual_state` next to `write_diff_batch`), `rebuild_evm_deposit_lock_index` (from `init` when the
marker is absent and from `import_pruning_point_utxo_set`), `with_indexed_deposit_claims` (in the template
path, under the virtual read lock, before `prepare_deposit_claims`), the `safe` head in
`update_evm_canonical_heads`, and the mempool refusal in `validate_mempool_transaction_impl`
(`palw_mempool_locked_bonds`, memoised per registry tip and DAA); `kaspad --evm-bridge-finality
label|pause`; the claim RPC's staleness refusal now only under `Pause`.

**Tests** (`CARGO_TARGET_DIR` private to the branch; `MISAKA_PALW_POW_FIXTURE=1`):

| suite | result |
|---|---|
| `cargo test -p kaspa-consensus --features evm` | **311 passed, 0 failed**, 7 ignored (before I-4 was added; I-4's test passes on its own) |
| `cargo test -p kaspad -p kaspa-rpc-service -p kaspa-mining` | **191 passed, 0 failed** |
| new: `evm_producer_claims_every_accepted_lock_unasked` (I-1), `evm_template_claims_oldest_lock_first_and_once` (I-3), `evm_deposit_lock_index_follows_the_virtual_utxo_diff` (I-2), `first_locked_input_names_the_locked_bond_spend` (I-5), I-4 inside `evm_active_chain_executes_persists_and_moves_heads` | pass |
| `cargo clippy -p kaspa-consensus -p kaspad -p kaspa-rpc-service -p kaspa-database --features evm --tests --no-deps -D warnings` | clean |
| `cargo clippy -p kaspa-consensus-core` | 6 errors — **the same 6 on `main` `40ac431b`** (measured side by side; none in lines this ADR touched) |
| `rustfmt --edition 2024 --check` on every touched `.rs` | clean |
| `bash scripts/ci-gates.sh --group fast` | 6/6 green |
| `python3 scripts/ci/check-repin-enumeration.py` | 7 unclassified — `main`'s baseline, unchanged |

**Mutations** (each applied alone to `processor.rs`, then restored byte for byte): no index union in the
template → I-1 and I-3 fail; `Label` ignored (the old pause) → I-1, I-3 and the pipeline test fail; `safe`
left at the sink → I-4 fails; the diff not staged into the index → I-2 fails. **4 of 4 caught.**

Two fixtures had to change, and both changes are the point of the ADR, not accommodations:
`set_fresh_dns_finality` named an anchor hash no header exists for — since the 2026-09-10 blue-score
freshness fix such an anchor reads as stale, so the one test that depended on "fresh" was passing only
because nothing else had asked; it now names genesis. The pre-existing queue-path test asserts
"a stale anchor keeps the payload empty", which is `Pause`'s behaviour; it now says `Pause` explicitly.

**Not verified here:** the fleet does not run this build yet (a restart-only rollout: the index builds
itself on first start, and old-build peers validate every block a new build makes); the index's
rebuild over a large UTXO set (testnet-11's is small; on a mainnet-sized set it is one pass at the first
start); a live claim made by the index on testnet-11 (it needs the rollout).

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md -->

</details>

<a id="source-0184-native-liability-horizon-panel-record-and-single-certified-leg"></a>

<details>
<summary>0184-native-liability-horizon-panel-record-and-single-certified-leg.md — 原文 L1–L27</summary>

[原文を開く](../adr/0184-native-liability-horizon-panel-record-and-single-certified-leg.md) · SHA-256: `88a4ef31fb9e1457fadf717d85fff996d4895ba6ea01603826e59c99f201dc22`

<!-- ADR_SOURCE_BEGIN:0184-native-liability-horizon-panel-record-and-single-certified-leg.md -->
~~~~~~~~markdown
# ADR-0184 — native claim の責任期間、Panel-licensed Final の記録専用 event、認証報酬の単一 leg

- **Decision:** Accepted — ユーザー決定 2026-10-17(ADR-0181 の例外変更経路で記録)。
- **Scope:** RFC08(native claim の責任と手数料)、RFC07(Panel-licensed Final の event)、RFC18(認証済み報酬の支払い)。
- **Implementation:** RFC07 は `integ/validation-r2`(`KernelFinalReadV1::panel_record`、`panel_record_v1`)。RFC08 は 08-A(4 family の admission sourcing)と 08-B の native job v39(`palw_native_liability_horizon_v1`、kernel route aux 144)を `integ/validation-r2` に実装。text v29・funded v11 の保持と post-Final 告発は未実装。RFC18 は変更なし。
- **Verification:** core 単体のみ(保持・解放・返金・post-Final 有罪化、aux 144 codec、fingerprint 不変)。実ノード試験は未実施。
- **Activation:** なし(全 fence dormant、t12 identity 不変)。
- **Related:** [外部監査の状況](../audit/palw/design-reviews/rfc-completion-audit-2026-10-17-status.md)、ADR-0185。

## 決定

1. **RFC08:** native claim(funded v11・text・job v39・mining v39)の責任期間 = Final の下限 + `window_court`(他 lane の責任期間と同じ式)。producer が commit 時に署名する。手数料はその期間まで保持し、満了で支払う。有罪または default のとき、未払いの手数料は requester へ返金し、担保は slash する。
2. **RFC07:** Panel-licensed Final は、認証済み(rooted な ledger 行から読む)の記録専用 event を出す。beacon source にも credit にも数えない(`event` には入れず、`panel_record` に置く)。
3. **RFC18:** 認証済み報酬の解放は producer への単一 leg のまま。検証者には既存の reporter share と検証手数料で支払う。

## 追記 2026-10-18(ユーザー決定)

- **RFC08 の horizon は導出する。** Final 下限 + `window_court` を、producer が署名した commit 入力(とその時点の ruleset)から決定論的に計算する。wire・object kind は変更しない。producer の署名は式の全入力を覆うので、horizon は署名済み入力で確定する。新しい dormant fence の下で、fence 以後に commit された claim だけが対象。

## 実装記録 2026-10-18(設計変更なし)

- **新しい dormant fence `palw_native_liability_horizon_v1`**(全 preset で `None`、`palw_settlement_once_v1` 以下を要求、state params の mirror は borsh skip で identity・fingerprint 不変)。fence 以後に commit された claim だけが対象。
- **native job v39:** Final で producer への手数料を payout queue に入れず、aux 144(`0x01 ‖ family ‖ job`)に保持する(受取人、額、requester の返金先、horizon = 署名済み deadline + `window_court`)。bond budget の予約も horizon まで開いたまま。closing tick が horizon 経過かつ未決の告発(pending demand・proof owner)がない行を解放し、Final が行うはずだった支払い・native income・予約 close を行う。
- **post-Final の告発:** court(proof owner)・公開 DA request・DA default を、保持中の Final に horizon まで受け付ける(各 clock の上限は horizon)。`NativeJobBookV39::convict_after_final` が Final の settlement を有罪に置き換え、保持中の手数料を requester へ返金し、reserve を pre-Final の有罪と同じく回収し、finder に支払う。証拠は family 既存の court・DA のみ(PRINCIPLES §8)。
- **Master / mining:** 手数料が保持中の component がある Master は close しない(mining claim は component の horizon 後にのみ成立)。
- **kernel 修正(バグ):** paid claim の Final・default が EXEC hold で延長された liability を paid lifetime の値で短縮していた(`max` に修正)。root の署名済み lifetime が paid lifetime より長いと EXEC の一貫性検査で tip が拒否されていた。
- **残り:** text v29・funded v11 の保持と post-Final 告発、mining receipt の直接の vesting(現状は Master の close 待ちで間接的に horizon 後)。
~~~~~~~~
<!-- ADR_SOURCE_END:0184-native-liability-horizon-panel-record-and-single-certified-leg.md -->

</details>

