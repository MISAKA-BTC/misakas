# ADR-0058: Merged work is counted — the mergeset carries claims, not just the chain

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


- Status: Accepted
- Date: 2026-08-27
- Depends on: ADR-0038 (PALW is consensus work), ADR-0039 (per-class DAA), ADR-0045/0056
  (share economy), ADR-0054 (share follows production)
- Supersedes: nothing; amends the transition's step 4 and the meaning of "produced" everywhere
  a counter reads it

## The defect, measured

On testnet-11 (`bb0a3ad3…`, 2026-08-27), the Qwen3.6 class produced 15 blocks by real
inference. Zero of them are on the selected chain; 743 floor blocks are (542 of 546 chain
slots). The margin each Qwen block lost by — 2M to 14M blue work, i.e. 2 to 10 floor blocks —
is its own inference latency: at 5–19 minutes per block against the floor's ~1 per minute, a
Qwen tip is always stale by the time it exists. The probability of losing 12 of 12 by chance
is ~4×10⁻⁷. This is structure, not luck.

The structure: a block's PALW attempt is applied to chain state only when the block joins the
virtual selected chain, and ordinary tip selection on a V2 network is deliberately blue-work
ordered (the pre-validation heap cannot evaluate the PALW comparator — see the wedge history in
`palw_tip_weights_v1` — so the one comparator runs at the deep-reorg gate, which a tip that
*extends* the sink never reaches). Both halves are individually correct. Together they mean a
slow-cadence class **cannot create claims at all**:

* no claim → `epoch_counters` never move → the per-class retarget's `observed` is 0 forever
  (`observed == 0 → continue`), so difficulty never eases;
* no counter → ADR-0054's growth walk sees "produced nothing" forever, so the share step the
  class was promised for filling its budget can never fire;
* no claim → no weight, so the work secures nothing and pays nothing.

Difficulty and share were designed to follow *measured production*, and the measurement was
wired to a race a slow class structurally loses. Every entrant class heavier than the floor
starves on arrival, at any share, at any difficulty.

## The decision

**A chain block applies the PALW work of its whole mergeset — blues and reds alike — not just
its own.**

Reds are not an edge case; on this network they are the point. The frozen 120 s cadence fixes
`ghostdag_k = 1`, so any block whose anticone holds two or more blocks is a red *by
construction* — which is every block of every class slower than the floor. All twelve real
Qwen3.6 blocks measured above are reds. A blues-only rule would have measured nothing.

1. **Step 4 of the transition** takes, after the block's own work, the work of every merged
   block of its mergeset (selected parent excluded — it applied its own work when it was the
   chain tip; non-DAA blocks excluded, matching the coinbase's pay set), in consensus mergeset
   order, reds included.
2. **Acceptance-context admission.** Each merged work passes the full stateful admission
   (`check_palw_attempt_admission_v2`) against the accepting block's live fold state — the same
   state, re-checked sequentially, so two merged blues cannot both take the last budget slot or
   the last sompi of exposure headroom. The stateless half (shape, challenge binding, executor
   signature) is checked once in the processor, against the carrying header.
3. **Refusal skips; it never disqualifies.** The accepting block did not author its anticone. A
   merged work the state refuses (budget exhausted, bond retired meanwhile, duplicate, exposure
   ceiling) is skipped deterministically — every node, same state, same order, same verdict —
   and the block stands. The block's OWN work still disqualifies on refusal, unchanged.
4. **The claim records the carrying block.** `accepted_block` is the merged blue itself — the
   panel derives the job anchor from the carrying header's pre-PoW hash, and the producer bound
   its material to that block. `accepted_daa`/`accepted_blue_score` are the accepting chain
   block's — deadlines and the safe frontier are chain-order facts.
5. **A merged claim escrows nothing (`escrowed_reward = 0`), this revision — and the coinbase
   pays an entitled in-window RED to its own miner script.** Blues were already paid to their
   own miners; entitled reds' worker shares were lumped into the *merging* miner's red reward,
   which under this ADR would have put the slash exposure on one key and the pay on another.
   The red's share now goes to the red's own script, through the same carve arithmetic, gated
   to `ConsensusV2` networks so every other network's coinbase stays byte-identical.
   Making merged claims escrow instead would require the coinbase to know the transition's
   outcome before the transition runs (they validate in that order). Today an entitled merged
   block is paid with *no claim, no verification and no slash exposure*; under this decision it
   is paid the same but its claim now exists — panel-verified, court-triable, bond-slashable
   (`reserved = pwu × slash_value_per_pwu ≫ carve`). Strictly tighter than the status quo.
   Symmetric escrow (withhold every applied merged block's carve, release at `Final`) is left
   as a follow-up that restructures coinbase validation ordering.
6. **Zero-escrow claims enqueue no payout.** A `Final` with `escrowed_reward = 0` writes no
   payout row, so no zero-value coinbase output can exist.

## What this closes

* **The retarget loop closes.** `apply_attempt` bumps `epoch_counters`; merged production now
  counts, `observed > 0`, the per-class DAA eases toward the class's budget share.
* **ADR-0054 closes.** The growth walk reads the same counters; a class that fills its budget
  from the anticone steps its share up exactly as promised.
* **ADR-0038's premise is restored.** PALW weight was "the network's whole fork choice", but
  only chain blocks minted weight, so a class that lost parent selection contributed no
  security. Merged claims mature into `safe_weight` on every chain that merges them; a private
  fork that excludes them now weighs less than the public chain that counts them.
* **Pay-without-verification narrows.** The entitled merged block's instant payment existed
  with no claim behind it; now the claim, panel duty and slash path exist for every applied
  work — and for reds, the payment finally lands on the key that carries the slash.

## Consensus impact — read before deploying

* `PALW_STATE_V2_VERSION` 9 → 10. Every state root moves from genesis.
* The params fingerprint now hashes `PALW_STATE_V2_VERSION` explicitly. Every previous bump
  moved the handshake only because it happened to ride a bundle-shape change; a semantic-only
  bump like this one would have left old and new nodes peering and then silently disqualifying
  each other's chains. The version is block-validity-relevant, so it is hashed.
* **testnet-11 must re-mint.** Old-fingerprint nodes refuse new ones at handshake (that is the
  point). Artifacts, class ids, pins and bonds' keys are untouched; genesis and the network
  fingerprint change.

## Rejected alternatives

* **Weigh the candidate tip's own attempt in fork choice.** Inverts the maturity principle
  (fork choice would trust unverified claimed pwu), and mirrors the bug instead of fixing it:
  if a Qwen tip always out-weighs floor tips, floor blocks stop being chain blocks and *their*
  production stops being counted.
* **Throttle the floor operationally.** The floor is permissionless; a fairness property that
  depends on volunteers slowing down is not a property.
* **Count merged blocks into the counters without applying claims.** Difficulty would ease and
  share would grow for work nobody verified and nobody can slash — production without
  accountability, worse than the defect.

## Mission alignment amendment — 2026-10-07

* 将来のlicense/Final、early weight、slice/claimの報酬解放は、証拠保持・proof期間・clock・collectible collateralと整合させる。多数派の署名で有効なfraud proofを無効にしない。DA default、算術conviction、false Validのscope別責任は区別し、verifier不在やローカルtimeoutをproducer fraudにしない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。
