# RFC-0007: PALW constraint verification — batched Freivalds/GKR Panel checks, evidence-bound receipts, and exact court on dispute

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


| Field | Value |
| --- | --- |
| Status | Revised design, 2026-10-08: **Part VI is the sole post-commit challenge protocol for the proposed new Kernel/model/claim/slice/public-check route**; Part V remains the new Panel verification direction. Neither revision is implemented or activated by this document. Parts I–IV preserve their historical design/prototype results, not live status. |
| Author(s) | MISAKA core (drafted with Claude; lane M4) |
| Created | 2026-10-01 |
| Normative dependencies | **RFC-0002** (TIR semantics and court), **RFC-0011 §§13/15** (verification-plan admission, probabilistic acceptance), **ADR-0171** (selected direction); existing Panel/segment rules, ADR-0065/0098/0111 and spec 18 for compatible legacy paths. New receipts and coverage rules require the separate Part V fence. |
| Affects | spec/palw 07/08/18: scheme/profile/evidence-bound receipts, assignment, coverage and lifecycle; 09: bounded localization into compatible exact court, separately versioned extensions if needed; 14/16: verifier operation and new fence. Existing Parts I–IV are legacy baselines; Part V changes require coordinated SDK, node, fold, DA, RPC and replay work (§V.9). |
| Original prototype branch | `rfc7/algebraic` (off `rfc4/int` @ `ce04e5c22`); the 2026-10-06 design revision is audited against `main` commit `7bff920dc`. No claim that the new scheme runs on a deployed node. |
| Related | RFC-0006 (layer-sharded panels, lane M3), lane M2's runtime residency for IR classes (`TirRowSourceV1`, ADR-0112 for IR classes), lane P's root-cause report of the testnet-12 panel backlog (`rcore/int-10-p1:docs/design/palw/t12-panel-backlog-1001.md`), ADR-0029 (carriage), ADR-0038 (receipts are claims), ADR-0062 (the data-availability court), ADR-0069 (weight needs adjudicability), ADR-0072 (the ticket is the execution), ADR-0080 (a receipt is 4,772 bytes), ADR-0097/0099/0100 (shards, the stratified panel), ADR-0103 (held context), ADR-0112 (residency), ADR-0124 (supplementary receipts), ADR-0133 (verification is its own clock), ADR-0147 (the admission jury), ADR-0152 (collateral, ejection), ADR-0160 (capacity) |

> **Revision precedence:** Part VI owns new post-commit source eligibility, ordering, seed derivation, sampling, interactive-round timing and recovery; other RFCs reference it instead of defining competing seeds. Part V owns checker/receipt coverage. Part II's private sketches and Parts III/IV's older assignment/audit sources are historical or legacy protocols, not substitutions for the new public challenge source. Existing consensus is governed by [spec 18](../spec/palw/18-verification-certificates.md), not retroactively changed by this RFC.

> **Kernel-only boundary (2026-10-06):** [ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)
> and [Kernel design §§K.0–K.8](../design/palw/versioned-kernels.md) govern Part V. Checkers are approved versioned kernel
> relations; registration, claims, receipts and transcripts bind descriptor/plan identity. GKR's
> generality does not authorize arbitrary uploaded programs, guests or universal CPU circuits.
> Missing relations require a coordinated kernel upgrade and matching bounded court, not a VM
> fallback. Part V's probabilistic Final policy is unchanged.

## 概要(日本語)

**通常のPanel検証は、割り当てられたsegmentを丸ごと再実行する方式から、commitされた計算のconstraintを安く検査する方式へ進める。** この方針をPart Vの中心要件とする。

- 同じweightを使う複数token/cellの行列積をまとめ、batched Freivaldsで検査する。GKR/sum-checkは、対応するconstraint群と境界の整合性をまとめて検査する候補とする。
- routing/TopK、量子化・丸め、非線形演算、入力・出力、memory/history、segment境界も検査範囲に含める。安いexact checkまたはreview済みのrange/lookup等を使い、未検査の関係を残さない。
- receiptには方式・profile・challenge・検査範囲・証拠を署名でbindする。**区間の検査成功を、claim全体の成功へ無条件に格上げしない。** 部分receiptは被覆条件を満たした範囲にしか数えない。
- 異常時にだけ、食い違いを局所化して互換性のある既存courtへ渡す。check失敗だけでslashせず、nodeはterminal stepをexactに判定する。
- 正の証拠、必要な被覆/quorum、DA、challenge window完了が揃えば、RFC11の誤受理確率を許容してFinalにする。nodeは通常時にモデルを実行せず、署名・割当・commitment・期限・被覆を検証する。
- Kimi K3級への拡張を目指すが、速度向上は未実測。最初はTIR MatMulの再計算とbatched Freivaldsを、CPU時間・証拠生成・帯域・メモリ・cold/warmコスト込みで比較する。

### 旧草案の概要（2026-10-01の設計・計測記録）

以下は元のPart I–IVの説明を保存したもの。新方式に対する規範は上記とPart Vであり、過去の速度推計、quorum、capped-mode、休眠状態の記述を新方式の実績として扱わない。

- **この RFC が扱うもの。** testnet-12 では検証の供給が claim の throughput を縛っている(lane P の 10-01 報告)。容量計画は bond あたり claim ×10 → ×100 → ×1000。
  この RFC は検証を安くする二つの梃子を定める。**(1) 集約を安くする**(Part I)と **(2) 検査そのものを安くする**(Part II)。そのうえで安全性の
  パラメータ(Part III)と、全 seat を使う監査網と新クラスの段階的立ち上げ(Part IV)を定める。
- **前提: 分割は仕事を減らさない。** 1-of-N の下で、ある 1 点の嘘が逃げる確率は f^m(m = その点を独立に検査する seat の数)。区間に分けても
  「誰がどこを見るか」が変わるだけで、区間あたり m を保てば総仕事は ≈ m × (1 job)。効く梃子は二つで、検査を安くすることと集約を安くすること。
- **Part I — seat の vertex と tally による licence。** 今の licence は claim ごとに ML-DSA-87 署名つき receipt を 3 枚(≈ 14.4 KB)運ぶ。
  t12 の 5.3 claim/DAA では 76 KB/DAA で、×100 なら 7.6 MB/DAA、×1000 なら 76 MB/DAA(block mass 0.5 MB の 150 ブロック分)。
  - seat は round ごとに **vertex を 1 枚** 署名する。中身はその round の判定すべてを leaf とした Merkle root と、その leaf 列。
  - vertex は 1 度だけ運ばれ、fold が leaf を数える(**licence by tally**)。panel seat の Valid が quorum に達した時点で ReceiptLicensed になる。
  - 同じ round に食い違う vertex を 2 枚署名すれば equivocation として slash する。Bullshark の順序づけは不要で、GHOSTDAG が順序を決める。
  - DA certificate は vertex の `Held` leaf として載せる。
  - 運搬量は claim あたり 14.4 KB から 66–330 B(参照の形と leaf 数による)に減り、×1000 でも数ブロック/DAA に収まる。
  - collector による licence 組み立て(V04 の重複運搬)は消える。
- **Part II — seat ローカルの代数検査(試作と実測あり)。** TIR の trace は MatMul の accumulator を commit しない(i64/i128、PALW-TIR-5)。
  - producer は seat に「witness」を配る。中身は検査対象の MatMul 出力すべて。
  - seat は weight 積を、自分だけの秘密ベクトル r による **sketch** s = W·r と Freivalds で照合する。検査コストは |out| + rows·K で、
    sketch は weight の 0.3–1.2 %。routed expert は expert ごとの sketch を使い、expert は seat 自身が再計算した TopK で選ぶ。
  - act×act は fresh r で照合する。P·V は per-row history sketch を使い、Q·Kᵀ は再計算する(score を配ると割に合わない)。
  - それ以外の node はすべて、court と同じ reference evaluator で厳密に再計算する。導いた commit 行を claim の root と照合する。
  - modulus は refined range proof の span で決める(P61 / P61·P64 / +P63)。1 段足りないと「誤差 = P61」が通ることをテストで示した。
  - r は決して公開しない。**sketch も r と同じだけ秘密**(W が公開なので、N ≤ K なら s から r が解ける)。
  - 検査の失敗は verdict ではない(witness が悪いだけかもしれない)。seat は Valid を出さず、厳密な court(不変)へ進む。
    失敗ノードの weight 行は、既定の block sketch(fetch 上限 F = 2 MiB。例: head は B = 256 で sketch +3.1 MB、fetch 1.8 MB)で絞った
    artifact opening で取るか、保持者の seat に回す。
  - 実測の結論: **効くのは「RAM に載らない class の decode を ≥ 100 Mbps で」**。
    - NVMe から weight を読む seat に対し、1 Gbps で 5–10 倍速い。
    - RAM 常駐の seat に対しては 1–2 Gbps 以上でないと得にならない。
    - prefill は 1.5–5.5 Gbps 未満では常に再計算が勝つ。
    - t12 の floor/8k class には効かない(梃子は Part I と scheduling)。
    - Part II の価値は実現可能性。qwen4_exp 相当を、常駐 2.3 GB(全体は 132–183 GB)の seat が検証できる。
- **Part III — 安全性の枠組み。** 1-of-N と厳密 court を保つ。「最大の互換クラスタが正しい」は採らない(ユーザー決定)。
  - 重みは bond と Sybil 耐性のある seat 資格、commit 後の無作為割当、permissionless challenge、無作為監査から来る。
  - 全区間の検査(代数か全再計算)が安い class では、m = panel 全員(5)。逃げる確率は f^5。
  - 標本検査(k = 4 区間)では 1 点の嘘の捕捉は ~6 %。抑止に要る slash は利得の約 17 倍になる。
- **Part IV.1 — 全 bonded seat からの無作為 leaf 監査。** 監査は one-move court の leaf check を off-chain で行うもの。
  - 監査人は class の ready seat に限らず、全 bonded seat から anchor beacon で commit 後に引く。
  - 必要なのは opening だけ(試作では tile あたり平均 1.1 KB)。
  - 全面的な捏造と部分的な手抜きは 1 − (1−q)^m で捕まる(試作で実測一致)。1 点の嘘は ≈ m/N でほぼ捕まらない。
    だからこれは sensor であって floor ではない。
  - 沈黙は Valid と見なさない。Verifier's dilemma には監査トラップ(D-F2 型の故障を植える)で対処する。
  - 小さな class(ready seat が 5–7)で最も起きやすい class 内談合を、全網から引くことで薄める。
- **Part IV.2 — 段階的立ち上げ(capped mode)。** 全範囲の保持者が揃う前でも、登録と「witness を配る」DA certificate の後、mesh 監査の下で
  上限つきで block を出せる。
  - 上限は「検出されない不正の期待利得 ≤ 検出確率 × slash 可能額」から導き、grinding の害も上限で抑える。
  - 報酬は保持者が揃うまで未確定(vest しない)。保持者は vesting 期間内に過去の capped claim を再検証し、嘘なら未確定報酬を没収・slash する。
  - weight 0 の lane が fork choice を hash にただで渡す危険に注意し、既存の pwu/receipt-ramp を使う。
  - 既存のライフサイクル(Candidate → jury → Prefetching → Probation → ActiveLimited → Active、Λ = 170 DAA は不変)に接続する。
- **有効化。** fence はすべて休眠で導入する(Some-only hash と never() 潰しを 4 箇所とも、独立した高さ)。各 fence に、出荷する binary で
  跨ぐ drill を付ける。

## Summary

The selected design changes what a Panel receipt means: a seat validates the constraints of its assigned computation with batched Freivalds, compatible GKR/sum-check and bounded exact or lookup checks, rather than replaying the entire segment. Part V specifies the new profile, scope, committed evidence, challenge schedule, tally and escalation obligations. A claim passes probabilistically only when its whole statement is covered under the declared error budget; a few individually sound sampled segments are not enough.

Part I's batched signatures remain a transport building block, and Part II's private-sketch code remains an implementation baseline. Their original carriage/performance estimates do not price the new evidence or prove its speedup. Parts III/IV supply the threat analysis and diagnostic audits. Raw audits remain distinct from licensing coverage. The exact court resolves localized disputes; it does not eliminate undetected fast-path error. Implementation starts with a measured MatMul checker and then integrates typed receipts and complete coverage before reward-bearing acceptance.

## Motivation

### 1. Verification supply binds

Lane P measured testnet-12 on 2026-10-01:

- Issuance is 5.3 claims/DAA.
- Licensing needs 3 of 5 seats' `Valid` receipts.
- Healthy receipt supply is ≈ 600 receipts per hour against a demand of 380–640, so there is no margin.
- When two hosts' replays slowed, licensing collapsed from 5.5 to 1.2–2.5 licences/DAA. The node fixes F1–F4 and the static `L_ver`
  term of int-11 respond to that.

The capacity plan (ADR-0160) multiplies claims per bond ×10, ×100, ×1000. Every one of those claims needs verification and a licence.
Two costs scale with them: the seat's work per claim and the chain's carriage per licence.

### 2. Splitting is not saving

Under the 1-of-N model, a lie at one point escapes when every seat that checks that point is the adversary's or does not look. With `m`
independent checkers of the point and an adversary holding a fraction `f` of the eligible weight, the escape probability is about `f^m`.
Cutting a job into intervals and giving each seat a few changes *who* checks which interval. At a given `m` per interval, it does not
reduce the work: total work ≈ `m × (one job)`.

ADR-0098 measured the other side of this. With `k = 4` sampled intervals of `N = 299`, five seats catch a one-token lie 6.51 % of the time.
Raising that to 90 % costs each seat 37 % of the job. Sampling buys a weak catch cheaply, and a strong catch costs a replay.

### 3. Two levers

- **Make aggregation cheaper** — Part I. Today each licence carries three 4,627-byte ML-DSA-87 signatures. At ×100 and ×1000 those
  signatures alone are more than the chain carries.
- **Make the check cheaper than the replay** — Part II, where the arithmetic allows it. Freivalds' algorithm checks a product `Y = X·W`
  in `O(|Y| + rows·K)` instead of `O(|Y|·K)`. A seat's sketch `S = W·v` needs `W` once per epoch; after that, the seat needs only `Y`,
  the served accumulators.

Both levers leave the court where it is. The court recomputes one leaf exactly from committed inputs and artifact openings, and it
convicts.

## Goals and non-goals

**Goals.**

- G1. Amortize signatures through verification vertices and keep carriage within a measured budget. Part I's legacy tens-to-hundreds-of-bytes estimate must be recalculated for Part V's new evidence bindings and availability costs.
- G2. One signature per seat per round, not per claim, at the gossip door and in the fold. (Part I)
- G3. Assigned segments are verified by their constraints, with batched matrix checks and compatible aggregate proofs; full segment replay is an optional baseline, not the normal large-model duty. Price actual weight/input access and authenticated preprocessing. (Part V)
- G4. Derive the whole-claim conditional error bound from coverage, fields, repetition, aggregation, binding and adaptive-query assumptions, following RFC11's proposed `2^-128` check target. A per-product `1/p` is not the claim's bound. (Part V)
- G5. Bind an approved verification suite, profile, scope and evidence to new signed receipts. Kernel selection is seat-local; coverage and security obligations are protocol-defined. The terminal court remains exact. (Part V)
- G6. Preserve permissionless exact disputes and explicitly account for compromised or inactive Panel members. Full constraint coverage can be algebraic; quorum alone does not prove arithmetic truth. (Parts III/V)
- G7. Every ordinary public bond, including one outside the Panel, can obtain authenticated public openings, localize a detected fault and reach objective conviction without producer-private state. Silence is never a verdict. Completion requires RFC14's end-to-end gates. (Part IV.1; ADR-0173)
- G8. Large classes become feasible with qualified constraint verifiers and bounded DA/court resources; Part IV.2's separate capped experiment never replaces the new route's soundness and coverage gates. (Part V)

**Non-goals.** A probabilistic court: the court stays exact. Weight from cluster agreement ("the largest compatible cluster is
correct"): refused by the user's decision. Privacy/ZK is not required; public interactive proofs and IOPs are permitted. A BFT ordering layer: GHOSTDAG orders. This document does not change shipped terminal semantics or ceilings. Part II remains optional for legacy claims; a Part V receipt must meet its explicitly assigned suite and scope.

## Part I — Batched verification certificates

### I.1 What a seat signs today, and what it costs

A V2 receipt (`PalwSeatReceiptV2`, `palw_panel_v2.rs`) is the claim id (64 bytes), a verdict (`Valid`,
`Unavailable { chunk_index, requested_daa }`, `Incapable`, `Sampled`), the seat bond (68 bytes), the signing DAA and an ML-DSA-87
signature of 4,627 bytes. On the wire it is 4,772 bytes (ADR-0080). `ReceiptLicensed { claim, receipts }` carries at least a quorum, 3
of them: ≈ 14.4 KB per licensed claim. Collectors assemble licences from a per-(claim, bond) receipt pool (`palw_receipt_pool.rs`) and
carry them, one carrier in flight per panel. Before V04 (`palw_licence_order.rs`), every collector carried the same claim and ~7 in 10
carriers were duplicates the fold dropped.

| claims/DAA | ×1 (5.3) | ×10 (53) | ×100 (530) | ×1000 (5,300) |
| --- | --- | --- | --- | --- |
| licence bytes/DAA | 76 KB | 763 KB | 7.6 MB | 76 MB |
| in 500,000-mass blocks | 0.15 | 1.5 | 15 | 153 |
| ML-DSA-87 verifications/DAA (fold) | 16 | 159 | 1,590 | 15,900 |
| receipt signatures at the gossip doors/DAA (5 per claim) | 27 | 265 | 2,650 | 26,500 |

Lane P observed ≈ 6 blocks per DAA on testnet-12. At ×100 the licences alone need more than twice that, and at ×1000 twenty-five
times.

### I.2 The vertex

A **verification vertex** is one seat's signed statement of everything it decided in one round:

```text
VerificationVertexV1 {
  version:     u16 = 1,
  seat_bond:   PalwBondKeyV2,         // the signer; its registered ML-DSA-87 key verifies
  round:       u64,                    // ⌊signed_daa / round_daa⌋, round_daa a ruleset parameter
  signed_daa:  u64,
  leaves:      Vec<VertexLeafV1>,      // strictly ascending by (kind, claim_ref); ≤ 1,024 leaves, ≤ 80,000 bytes
  leaves_root: Hash64,                 // the binary Merkle root over leaf_hash(leaf) in order (odd nodes promoted)
  signature:   Vec<u8>,                // ML-DSA-87 over vertex_message_v1, context below
}
```

- **One vertex per `(seat_bond, round)`.** A seat with nothing to say signs none. A round is `round_daa` DAA; 1 DAA keeps licence
  latency where it is today. The leaf cap keeps a vertex inside one carrier.
- **The leaves are carried, not only their root.** The fold must read every leaf to tally, so a root-only vertex would need a Merkle
  proof per counted leaf in a separate licence. With `Hash64` paths over a 1,024-leaf vertex that proof is ≈ 750 bytes, or ≈ 2.3 KB
  per claim at quorum: 6× better than today, against 44–218× for carried leaves (§I.9). The root is still signed, so a single leaf
  is provable to a third party: a court, a slash, an audit trap.
- **No parent edges.** Narwhal's vertices reference their predecessors so that availability and order can be certified. Here the
  chain carries every vertex, which is availability, and GHOSTDAG orders the carriers, which is order. An edge would add bytes and
  certify nothing new.

### I.3 Leaves

```text
VertexLeafV1 =
  | Verdict  { claim: ClaimRefV1, verdict: PalwReceiptVerdictV2 }             // tag 0 — today's receipt, unsigned
  | Held     { claim: ClaimRefV1, object: u8, first: u32, last: u32, digest: Hash64 }  // tag 1 — a DA attestation (§I.7)
  | Audited  { claim: ClaimRefV1, leaf: u64, result: u8 }                     // tag 2 — a mesh audit (Part IV.1)
ClaimRefV1 = Full(Hash64) | Compact { bound_daa: u32, id_prefix: [u8; 16] }  // 64 or 20 bytes
```

- **A `Verdict` leaf is today's receipt minus its signature.** Its verdict variants and their meanings are unchanged (`Valid`,
  `Unavailable` naming a chunk and a request DAA, `Incapable`, `Sampled`).
- **`Compact` references** name a claim by the DAA its panel bound at and a 16-byte prefix of its id. A prefix is unique among the
  claims bound at one DAA unless someone ground a 2^64 birthday collision. An ambiguous or unknown reference is ignored by the fold, so a
  seat can only lose by using one wrongly.
- **Leaf size.** A `Verdict` leaf is 66 bytes with a `Full` reference and 22 with a `Compact` one. A `Held` leaf is 138 (94), an
  `Audited` leaf 74 (30).

### I.4 Signing

```text
vertex_message_v1 = keyed BLAKE2b-512("misaka-palw/verification-vertex-message/v1",
                       network_domain ‖ borsh(seat_bond) ‖ le64(round) ‖ le64(signed_daa) ‖ le32(|leaves|) ‖ leaves_root)
context           = "misaka-palw/verification-vertex/mldsa87/v1"
leaf_hash(leaf)   = keyed BLAKE2b-512("misaka-palw/verification-vertex-leaf/v1", borsh(leaf))
```

Both domains are new and are added to the domain-uniqueness tests every PALW family runs (`palw_receipt.rs`'s
`domains_are_unique_across_all_palw_families`). Over these, a signature can be neither a receipt's nor another court move's. The message
binds the leaf count and the round, so a relayer can neither truncate a vertex nor move it to another round.

### I.5 Carriage and the tally (licence by reference)

- **Carriage.** A vertex is a consensus object (`VerificationVertex { vertex }`, the next free object tag) carried like every lifecycle
  object, any node carrying any seat's vertex. The acceptance layer checks:
  - the shape: version, sorted leaves, caps, root recomputed;
  - the seat bond's registration and key, then one signature;
  - that `(seat_bond, round)` has not been accepted before (§I.6).
- **The tally.** The fold reads every `Verdict` leaf of an accepted vertex. A leaf counts toward claim `c` only if all of these hold:
  - `c` is `PanelBound`;
  - the vertex's seat holds a seat on `c`'s panel;
  - `signed_daa` is inside `c`'s receipt window;
  - it is that seat's first counted verdict for `c`.

  This is exactly the set of conditions `validate_receipt_quorum_v2` checks per receipt today. When `c`'s counted `Valid` leaves reach
  the panel quorum, the fold applies the `ReceiptLicensed` transition itself: **the licence is the tally**, and no licence object is
  carried.
  - `Unavailable` leaves reaching quorum keep their present meaning under ADR-0065 Decision 4: they abstain and the claim redraws.
  - Supplementary `Valid` leaves past the licence credit seats as ADR-0124's supplementary receipts do.
- **What a verdict is, once counted.** It stands, as a carried receipt does today. A later vertex of the same seat that says otherwise
  is ignored for the tally: observation after the fact moves no obligation. A seat that has found a lie files no `Valid` (ADR-0098
  Decision 2) and opens the court through the challenger's half, as now.
- **Why the fold, not a collector.** The fold already knows the panels, the windows and the counts, so no node needs to assemble
  anything and no two nodes can carry the same licence. The duplicate-carrier waste of V04 and its ordering policy have nothing left to
  order.

### I.6 Equivocation

Two validly signed vertices of one `(seat_bond, round)` with different `leaves_root` are an **equivocation**.

- The fold accepts the first carried one, in GHOSTDAG's accepted order, and refuses the second.
- Anyone may carry both headers, ≈ 9.8 KB without leaves, as `VertexEquivocation { a, b }`. The fold then:
  - slashes the bond `equivocation_penalty_permille` of its stake (proposed 100 ‰);
  - forfeits every lock the seat holds on any claim either vertex names;
  - ejects the bond (ADR-0152 ejection).

  Equivocation needs no court: two signatures over one round are the whole proof.
- **What equivocation would buy.** A seat could show one view to some peers and another to the chain, or license a claim in one
  sibling block and refuse it in another. Both are refused by the rule above, and both are now priced.

### I.7 DA certificates

A `Held { claim, object, first, last, digest }` leaf attests: "I hold chunks `first..=last` of `object` of claim `claim`, whose digest
is `digest`, and I will serve them until the claim's challenge window closes." `object` names one of:

- the capture (the committed rows, `TirCaptureV1`);
- the witness (Part II);
- a trace-manifest chunk.

A **DA certificate** is `q` distinct seats' equal `Held` leaves (`q` a ruleset parameter, proposed 3). What it is used for:

- **Openings outlive their producer.** ADR-0111's slow path — a bounded on-chain availability request — may name an attester as well as
  the producer. An attester that fails such a request is slashed its `Held` exposure: it attested availability it did not provide.
- **Capped onboarding** (Part IV.2) requires one: the producer must have shown that it serves witnesses.
- **Audits** (Part IV.1) may fetch openings from attesters.

`Held` leaves are voluntary and are paid from the claim's DA fee share. Nothing in the license path depends on them.

### I.8 The pool and the assembler

What changes in the node:

- **Gossip** relays vertices, not receipts. The door verifies one signature per vertex under its budget: a seat's hundreds of verdicts a
  round cost one verification, where today they cost hundreds.
- **The pool** keeps vertices by `(bond, round)`.
- **Block templates** include pending vertices, like any lifecycle object.
- **What disappears.** The per-(claim, bond) receipt pool and its eviction defences, the licence assembler and the licence ordering —
  there is nothing to assemble.
- **Order of migration.** Below the fence, nothing changes. Past it, receipts are still accepted for one receipt window, so claims bound
  before the fence finish on the old path.

### I.9 Capacity

Per licensed claim:

- **today:** 3 receipts ≈ 14,385 bytes;
- **vertex, `Full` refs:** 3–5 leaves × 66 = 198–330 bytes;
- **vertex, `Compact` refs:** 66–110 bytes.

Fixed overhead per round: one header and signature (≈ 4.9 KB) per seat that has something to say.

| claims/DAA | ×1 | ×10 | ×100 | ×1000 |
| --- | --- | --- | --- | --- |
| today (licences) | 76 KB | 763 KB | 7.6 MB | 76 MB |
| vertices, `Full`, 5 leaves a claim | 1.7 KB | 17 KB | 175 KB | 1.75 MB |
| vertices, `Compact`, 5 leaves a claim | 0.6 KB | 6 KB | 58 KB | 583 KB |
| + headers: 200 active seats, `round_daa` = 1 | +0.98 MB | +0.98 MB | +0.98 MB | +0.98 MB |
| + headers: 200 active seats, `round_daa` = 4 | +0.25 MB | +0.25 MB | +0.25 MB | +0.25 MB |
| signatures verified by the fold, per DAA | ≤ 200 | ≤ 200 | ≤ 200 | ≤ 200 |

How to read it:

- At ×1000 with compact references and 4-DAA rounds, licensing costs ≈ 0.85 MB/DAA. That is less than two blocks' mass, against 153
  today, and 200 signature checks against 15,900.
- At ×1 the headers dominate. Seats then choose longer rounds, because licence latency costs nothing when the queue is short. The ruleset
  sets `round_daa`, and a node may skip rounds.

## Part II — Algebraic verification (seat-local)

**Baseline scope:** this section preserves the private-sketch design and historical prototype results. Part V's new default uses committed evidence, profile-bound duties and a reviewed challenge protocol; its normal path need not recompute every non-MatMul node as this baseline does. Part II alone is not evidence that the new public/GKR route is implemented.

The prototype is the crate `misaka-palw-tir-sketch`. Its soundness tests are `tests/soundness.rs` (16) and its unit tests 8 (Part IV.1's
audit tests are `tests/audit.rs`, 3). Its
measurements are `docs/design/palw/tir/algebraic-verification-measurements.md`. The crate depends on the IR and the typed backend and on
nothing of consensus.

### II.1 What the trace commits, and what a seat lacks

A TIR execution commits its **commit points** as step leaves (spec 04b §10.1). The commit points are:

- carry-outs and the logits;
- every `TopK`;
- every `HistAppend` input row;
- the points a lowerer adds until every cone fits the court;
- `Fixed` checkpoints every `C` positions, and history tiles every `h_tile` positions.

The `MatMul` accumulators are never committed. They are `i64`/`i128` (PALW-TIR-5), and a capture (`TirCaptureV1`) carries only leaf
preimages. Accumulators are exactly what a seat without the weights cannot recompute.

### II.2 The witness

The producer serves, per position, **the output of every `MatMul` the seat checks algebraically**, plus the committed rows a dense
capture already serves. What is served is policy (§II.6), and the canonical serving set is:

- every weight product whose contraction is at least 64;
- every `P·V` from a history of 1,024 positions.

A seat may ignore any of it.

The witness carries no commitment of its own and needs none for the seat. The seat accepts only if every served value passes its check
and the committed rows it derives from them equal the claim's. A wrong served value either fails its check or produces a derived row
that differs (§II.3). The witness does not reach the court (§II.8).

### II.3 The check, and why it is sound

Positions in order; in each, occurrences in schedule order; in each, nodes in index order:

- **A served node** is first checked against its declaration: dtype, shape at the running `H`, and **every element inside its refined
  proven interval**. An element outside is a malformed witness, as an out-of-interval committed operand is a malformed commitment
  (PALW-TIR-33). Then the node is checked algebraically:
  - **Weight product** `out = MatMul(a, b)` with `W` the static operand and `X` the other. For every **A-row** `α` — an index over the
    axes along which `X` varies, plus `X`'s free matrix axis — compare

    ```text
    LHS(α) = Σ_f v[f] · out[α ⊕ f]        RHS(α) = Σ_t X[α, t] · S[α, t]        S[w, t] = Σ_f v[f] · W[w ⊕ f, t]
    ```

    The sum runs over the axes free for the weight: `W`'s own free matrix axis, and every batch axis along which `X` is broadcast and `W`
    is not gathered. `S` is the epoch's sketch.

    A **routed** weight is gathered along batch axes, which are never compressed, so each A-row names one expert. The sketch used is
    that expert's own, and the expert is the one the seat's own `TopK` selected. A producer that serves the product of a different expert
    therefore fails the check.
  - **Activation × activation product** (`Q·Kᵀ`, `P·V`, the MoE combine). The same layout with `W = b`. The vector is fresh: keyed by
    job, position, occurrence and node. The sketch is built from `b`'s verified value.

    *Per-row history sketches* (modelled, not built in the prototype): when `b` is a view of a history window, a seat keeps one sketch
    per appended row, `S_V[h] = Σ_d σ[d]·V_h[d]`. For `Q·Kᵀ` it keeps the running `Σ_h ρ[h]·K_h`, with `ρ` indexed by absolute position
    and fixed per job and layer. A position then pays one row's sketch instead of the window's. This is sound for the reason a weight's
    sketch is: the vector is secret, and every row is verified before it is sketched.
- **Every other node the check needs** is recomputed exactly by the reference evaluator, one node at a time with its operands supplied
  (`Interpreter::eval_cone`, spec 04b §9.2). It is never read from the producer, and it never reads a weight that a sketch stands for.
  `tests/soundness.rs` runs the checker on a param source that does not contain the weight matrices.
- **After each position**, the committed rows the seat derived are compared in slot order with the served ones. After the job, their
  root is compared with the claim's. The prototype's root stands in for the step-tree root and hashes the same 4-byte lanes.

**Soundness.** Induction over the evaluation order:

- If every value a node reads is honest, an exactly recomputed node is honest.
- A served node that passes its check is honest except with probability `1/p`. `LHS − RHS = Σ_f v[f]·E[α ⊕ f]` for the error
  `E = out − X·W`. If a row of `E` is nonzero modulo `p`, a uniform `v` makes the sum vanish with probability exactly `1/p`, and every
  A-row is compared.
- §II.5 guarantees that a nonzero integer error is nonzero modulo at least one of the node's moduli.

So if every check passes, every derived committed row is honest except with probability at most `1/p` per dishonest node. A claim whose
rows are not honest then fails the final comparison.

**Tested** (`tests/soundness.rs`, all green):

- an honest witness is accepted by a seat holding no weight matrix;
- the typed backend and a reference walk serve byte-identical witnesses;
- each of these lies, with every value downstream computed honestly from it, is refused:
  - an off-by-one accumulator, found at its own node;
  - a changed narrowed output;
  - a swapped routed expert;
  - a changed attention output (served, or recomputed and caught at the next product);
  - a changed key row;
  - a non-`argmax` token;
- out-of-interval, missing and extra witness values are refused;
- over 240 random lies at random nodes, a seat accepts only a witness whose claim is the honest one, and refuses every lie in a served
  product;
- a tamperer who knows `v` builds an error orthogonal to it and passes, and the same error fails another seat's `v`. **Soundness is the
  secrecy of `v` and nothing else;**
- at a toy prime `p = 101`, random errors pass 1/p of the time (expected 198 of 20,000; the test bounds the count to [140, 260]). At
  `P61`, none passes.

### II.4 The seat secret, the sketches and their refresh

```text
epoch key  = BLAKE2b-256(key = seat secret, "misaka-palw/tir/sketch/epoch/v1" ‖ class id ‖ le64(epoch))
site seed  = BLAKE2b-256(key = epoch key,   "misaka-palw/tir/sketch/site/v1"  ‖ le16(occurrence) ‖ le16(node) ‖ modulus tag)
fresh seed = BLAKE2b-256(key = epoch key,   "misaka-palw/tir/sketch/fresh/v1" ‖ job ‖ le32(pos) ‖ le16(occurrence) ‖ le16(node) ‖ modulus tag)
vector     = ChaCha20(seed), each word masked to p's bit length and rejected until < p (exactly uniform)
```

- **The secret is never serialised.** It has no encoding, `Debug` prints no byte, and it is wiped on drop. A production node keeps it in
  its key store, beside its signing key, and may draw it per epoch.
- **A sketch is as secret as the secret.** The sketch `S = W·v` of a public `W` is `K` linear equations in the entries of `v`, one per
  index of the free axis. When the free extent is at most `K` (a down projection) they determine `v`. A sketch store therefore has no encoding either, and an epoch's store is dropped with
  its epoch.
- **What a producer learns.** A seat's only output about a claim is its verdict. A refused lie tells the producer only that its error
  was not orthogonal to `v` — a set of measure `1 − 1/p` — and the attempt is prosecuted in the exact court. Probing `v` costs one
  conviction per probe.
- **The build.** At each epoch, the store is built in one pass over the artifact:
  - each param instance's `[min, max]`, which refines the plan's intervals to the weights at hand;
  - the moduli, per site;
  - one sketch per weight site per expert.

  This prototype reads params through the reference `ParamSource`. A node reads them through its residency's row source (lane M2's
  `TirRowSourceV1`), one row at a time. A sketch is a sum over the rows of `W` (`S += v[r]·W[r, :]`), so it streams in row order and never
  holds more than a row. Measured at 1.09 G weights/s per core: an 80 GB class of `i8` weights takes ≈ 75 core-seconds per modulus.
- **Refresh.** Rebuild per epoch, or sooner after any conviction obtained through this seat's check (a conviction shows that someone
  was probing).

### II.5 Moduli from the range proofs

The served value and the true value both lie in the node's refined interval `[lo, hi]`, so an error is an integer `e` with
`0 < |e| ≤ hi − lo`. A node takes the fewest rungs of the ladder whose product exceeds `hi − lo`:

| span `hi − lo` | moduli |
| --- | --- |
| `< 2^61 − 1` | `P61 = 2^61 − 1` (Mersenne: shifts and adds) |
| `< P61 · P64 ≈ 2^125` | `P61`, `P64 = 2^64 − 59` |
| otherwise | `P61`, `P64`, `P63 = 2^63 − 25` (product ≈ 2^188) |

- **Why a missing rung is a hole.** One rung too few is not merely weaker: an error of exactly `P61` passes the `P61` check with
  certainty. `tests/soundness.rs` raises an `i64` accumulator by `P61` and computes everything downstream from it. A seat forced onto
  `P61` alone accepts the false claim, and a seat using the ladder refuses it at `P64`.
- **What the measured classes need.** Every weight site of the four classes measured needs only `P61`: their `i64` accumulators are
  proven below 2^61. The wider rungs are for `i32`-wide activations, wide fixed-point products and `i128` nodes.
- **The kernel.** It is branchless. Signed `i128` lanes are sized by the operand's proven bit width: an `i16` activation sums 2^30
  terms (the cap) before one reduction. It runs at 1.30 G terms/s per core.

### II.6 Policies: what is served

A seat's policy is its own and is not consensus. The measured default:

- **Weight products with contraction ≥ 64 are served.** Shorter ones are recomputed from a held weight: the A16 lowering's 16-column
  outlier products, and scalar gates. Their output would cost 8 bytes per element against `K ≤ 16` MACs.
- **`P·V` is served** from a history of 1,024, with per-row history sketches. It is `heads × d` values whatever `H` is, and it removes
  half the attention work.
- **`Q·Kᵀ` is recomputed.** Serving its scores costs 8 bytes per score against `d` = 128 MACs saved, which never pays below
  ≈ 10 Gbps (measurements §5).

The producer serves the canonical set of §II.2. A seat that wants less reads less.

### II.7 The trace-serving obligation and its DA

The producer's duty is to **serve the witness** of a claim to its panel's seats during the receipt window: chunked per interval, on
request, through the interval lane's off-chain path.

- **Enforcement is the licence.** A seat that cannot obtain a witness either recomputes, if it holds the class, or files
  `Unavailable { chunk_index, requested_daa }`. Under ADR-0065 Decision 4, `Unavailable` abstains: a producer that withholds gets no
  licence, the claim redraws once and then voids at `ReceiptTimeout`, and nobody is slashed for silence.
- **Naming witness chunks.** Today `Unavailable` names a chunk of the committed trace manifest. To name witness chunks, the manifest
  gains a witness root (fence `palw_witness_manifest_v1`, §Activation): the producer commits the witness's chunk digests at claim time.
  Without the fence, Part II still works; seats simply cannot name which witness chunk was withheld.
- **Availability past the window** belongs to the capture, not the witness. The court never needs the witness (§II.8). `Held` leaves
  (§I.7) attest the capture's availability where a network wants it beyond the producer.

### II.8 Failure, escalation, and the exact court

**A failed check proves the witness wrong, not the claim.** A producer can serve a bad witness for an honest claim, and gains nothing
by it. So a failure is never a verdict: the seat files no `Valid` (ADR-0098 Decision 2) and escalates. Escalation finds **the first
committed row the seat disagrees with** — the named leaf of ADR-0111, which the unchanged one-move court then tries exactly. That needs
the honest values from the failing node to the next commit point, which needs that node's weight rows. Two paths, either or both:

- **(a) Fetch the rows.** The seat fetches the failing node's weight rows as inventory openings: leaves of ≤ 32 KiB with closed-form
  indices, opened against `artifact_root`, from the producer or from any holder. A request that is not answered goes to ADR-0111's
  bounded on-chain availability request.
  - The worst-case fetch is the largest served weight: the vocabulary head, 311–927 MB for the classes measured.
  - **Block sketches are the default.** For each weight site, a seat keeps `B = ⌈|W| / F⌉` row-block sketches, `F` being a fetch cap
    (proposed 2 MiB). A block sketch sums only its block's rows, and the routine check uses the whole-site sketch, the sum of its blocks.
    The blocks are read only after a failure, to name the failing block's `|W| / B ≤ F` bytes of rows.
    - At `B = 256`, the 151,936-row head of Qwen2.5-1.5B costs 3.1 MB of extra sketch and a 1.8 MB fetch instead of 467 MB.
    - The overhead is `K · 8 / F` of a site's weight bytes: 0.6 % at `K = 1,536`, 3.4 % at `K = 8,960`.
    - A routed weight's sketch is already per expert, so a failing row names one expert slice (1–3 MB).
  - The **transfer bound** per escalation is therefore ≤ `F` per failed node, plus its paths. The seat then recomputes up to the next
    commit point, a cone the court's ceilings already bound (≤ 16 Mi terminal MACs a tile).
- **(b) Hand the interval to a holder.** The seat hands the interval to a seat that holds the class: a full-coverage holder, or the
  layer shard of RFC-0006 that holds the failing layer. The holder replays the interval and names the leaf.

Either way the court is unchanged. It sees the named leaf, its cone's committed inputs and the artifact openings, never a sketch or a
witness.

### II.9 Where it pays (measured)

From the measurements document. Per-core rates on this Mac:

- typed-backend GEMV: 14.9 GMAC/s;
- `P61` check: 1.30 G terms/s;
- sketch build: 1.09 G weights/s;
- streaming read: 48 GB/s.

The modelled seat has 8 cores, and its NVMe reads 3 GB/s. Classes were lowered from their `config.json` without weights; decode is
priced per token at H = 1,024.

| class | held + sketches (of params) | served / token, raw (packed) | recompute, RAM / NVMe | check | break-even link, RAM / NVMe (raw) |
| --- | --- | --- | --- | --- | --- |
| Qwen2.5-1.5B | 525 MB + 10.5 MB (1.84 GB) | 6.7 MB (3.9) | 43 ms / 611 ms | 5.9 ms | 1.44 / 0.09 Gbps |
| Qwen3-30B-A3B | 871 MB + 249 MB (31.1 GB) | 16.6 MB (9.3) | 79 ms / 1.04 s | 17.5 ms | 2.16 / 0.13 Gbps |
| Qwen3-Next-80B-A3B | 1.38 GB + 924 MB (80.7 GB) | 20.0 MB (10.5) | 159 ms / 1.28 s | 86 ms | 2.18 / 0.13 Gbps |
| DeepSeek-V3 | 3.64 GB + 2.05 GB (674 GB) | 138 MB (84) | 861 ms / 12.4 s | 138 ms | 1.52 / 0.09 Gbps |
| qwen4_exp-like (analytic) | ≈ 1.4 GB + 0.93 GB (132–183 GB) | ≈ 26.5 MB (14) | ≈ 173 ms / 1.45 s | ≈ 90 ms | ≈ 2.6 / 0.16 Gbps |

The qwen4_exp row uses the user's shape: 48 layers × 512 experts, top-10, hidden 2,048, inter 512, a 51 G n-gram table, hyper-connections.
That is Qwen3-Next's MoE geometry plus analytic terms.

The verdicts:

- **Decode against NVMe:** algebra wins 5–10× at 1 Gbps and is about even at 100 Mbps (0.7–1.1× raw, 1.2–1.9× packed).
- **Decode against RAM:** algebra wins only above 1–2 Gbps.
- **Prefill** of 1,024 positions breaks even at 1.5–5.5 Gbps (packed–raw) even against NVMe, because a batched recompute reads each
  weight once.
- **At 5 Mbps** algebra never wins.
- **Long context does not rescue it.** `Q·Kᵀ` stays a recompute, and algebra halves attention at best.

**What this means.** For testnet-12's binding classes — Qwen2.5-class dense, the 8k class prefill-heavy — Part II does not cut a seat's
time. Their levers are Part I and scheduling (lane P's F2). Part II's value is **feasibility**: a seat holding 2–6 GB checks a class of
80–674 GB at decode speed over a 1 Gbps link. That is how a qwen4_exp-sized class can have enough seats at all.

### II.10 Bandwidth pricing

- **Who pays what.** Serving the witness costs the producer upload, and the producer wants the licence. Fetching it costs the seat
  download, and the seat chooses its path by the table above.
- **The derived number.** A class's **witness bytes per position** are a deterministic function of its program and the canonical
  serving set. `cost::tir_position_cost_v1` computes them from the program alone. The class profile should carry them next to the
  canonical work vector (ADR-0145), for two uses:
  - the verification-window derivation (ADR-0133 §11.3), so that a class whose witness cannot reach its seats within the receipt window
    is held, not voided;
  - seat pay, in proportion to the seat's chosen cost (an open question).

  None of this is consensus until a fence makes the profile carry it.

## Part III — Security parameters and escalation

### III.1 The model

- **1-of-N with the exact court.** One honest seat that finds a lie files an accusation, and the one-move court recomputes one leaf
  exactly from committed inputs and artifact openings. A cluster majority never decides correctness (the user's decision). Weight comes
  from bonded, Sybil-resistant seat eligibility, post-commit random assignment (the anchor beacon), permissionless challenge and random
  audits.
- **The adversary** holds a fraction `f` of the eligible seat weight. Panels are drawn by weight after the claim commits.

### III.2 `m` per interval

This historical illustration assumes independently drawn checkers, detection when an honest checker covers the fault, and timely challenge access. `f^m` is not a theorem for arbitrary quorum/correlated operators, nor does it include algebraic error. Part V uses an explicit composition and actual coverage/tally rules. “Coverage” means all required constraints are represented, not all arithmetic is re-executed.

`m` is the number of independent checkers of one interval:

| how seats check | `m` | escape of a one-point lie | at `f` = 0.1 / 0.2 / 0.33 |
| --- | --- | --- | --- |
| full replay or algebraic, all 5 seats, duty to `Final` | 5 | `f^5` | 0.001 % / 0.03 % / 0.4 % |
| full, licensed at quorum, the last 2 seats stop | 3 | `f^3` | 0.1 % / 0.8 % / 3.6 % |
| `k = 4` of `N = 299` intervals sampled (ADR-0098) | 5 × 4/299 | `(f + (1 − f)(1 − k/N))^5` | 94.1 % / 94.8 % / 95.6 % |

Two rules follow:

- **Duty runs to `Final`, not to the licence.** Seats keep checking after quorum, and a later finding opens the court inside the
  121-DAA licence-to-`Final` window. That keeps `m = 5` for every class a seat can check fully.
- **Full constraint coverage for Part V.** Seats discharge assigned segments with the new suite; aggregation may cover the whole claim. No full-replay seat is inherently required by the new route. Raw interval sampling remains a sensor and cannot fill a missing segment or proof obligation. Legacy assignment/replay rules stay effective below the new fence.

### III.3 Audits and challenges

- **The global audit mesh** (Part IV.1) adds `1 − (1 − q)^m` detection of wholesale fabrication across the whole bonded seat set.
- **Permissionless challenge** stays: any bonded node may re-run a licensed claim and open the court. ADR-0098's watchdog is the full-
  coverage case.
- **Audit traps** (Part IV.1) keep auditors honest.

### III.4 Bond economics

A conviction slashes the claim's reservation. A rational producer lies when `gain > P(detect) × slash`.

- With **full coverage**, `P(detect) ≈ 1 − f^5`, so a slash of about the gain deters.
- With **sampling**, `P(detect) ≈ 6 %` for the forger's best lie, so the slash must be ≈ 17× the gain. Collateral then has to be priced
  for the class's worst lie, not its average one. The ADR-0152 design bar (maximum profit from absconding ≤ recoverable value) applies at
  that multiple.
- **Equivocation** (Part I) and **trap misattestation** (Part IV.1) are slashed from the seat's bond. They need no court, and their
  evidence is two signatures, or a signature and a reveal.

## Part IV — The global audit mesh and staged onboarding

### IV.1 Global random leaf audits — a sensor, not the security floor

**The audit.**

- **What it is.** An audit is the one-move court's leaf check run off-chain, with exactly the court's arithmetic: the demand evaluator of
  spec 04b §9.4.
- **Assignment.** Auditors are drawn from **all bonded seats**, not only the class's ready seats. The draw is post-commit, by the
  panel-anchor beacon, weighted by eligibility and bond. Each draw names a seat, a claim and a committed leaf.
- **What the auditor opens.** The output tile; the cone's committed inputs, opened against the step root (other commit points of the
  occurrence, carry-ins, earlier history rows, checkpoints); and the artifact rows the cone reads, opened against `artifact_root` as
  ≤ 32 KiB inventory leaves with closed-form indices. The auditor holds no model.
- **Who serves.** The producer serves the openings. This is ADR-0111's leaf demand at scale: the off-chain fast path first; on failure a
  bounded on-chain availability request; a leaf still unopened at the deadline voids the claim. Openings may also come from `Held`
  attesters (§I.7).

**Prototype** (`misaka-palw-tir-sketch::audit`, `tests/audit.rs`). A `DemandSource` answers every question of the court's evaluator from
openings and counts what it opened. On the dense + MoE fixture claim (104 tiles of 8 lanes):

- **No false alarms.** An honest claim passes all 104 audits. An audit opens 1.1 KB of values on average and 2.4 KB at most, about 30
  openings (Merkle paths).
- **A fabricator is caught at the predicted rate.** The fabricator rewrote a fraction `q` of its tiles and computed on honestly from
  them:

  | `q` | caught at `m` = 1 / 4 / 16 | predicted `1 − (1 − q)^m` |
  | --- | --- | --- |
  | 9.6 % | 9.8 / 32.5 / 76.5 % | 9.6 / 33.3 / 80.2 % |
  | 30.8 % | 29.0 / 79.0 / 99.8 % | 30.8 / 77.0 / 99.7 % |
- **One lied tile is nearly invisible.** A single lied tile among 104 is found by 8 audits 6.45 % of the time (`m/N` ≈ 7.4 %).

**On real classes.** An audit opens the tile's cone:

- the output tile, plus up to `tile_len` rows of each weight the cone reads (`tile_len` × `K` bytes — 98 KB for a 64-row tile of a
  1,536-wide projection, 573 KB of the 8,960-wide down projection), plus the cone's committed inputs;
- the court's ceilings bound it: ≤ 16 Mi terminal MACs a tile, ≤ 8 operands.

So a typical audit opens 0.1–1 MB, and the ceilings cap any audit at ≈ 16 MB of `i8` weights. At ten typical audits per seat per DAA
that is ≤ 10 MB/DAA: 0.5 Mbps at 150 s a DAA. **The mesh is the work of every seat, the low-bandwidth ones included.**

**What it catches, and what it does not.**

- **Wholesale fabrication, and partial skipping at a fraction `q`:** caught at `1 − (1 − q)^m`.
- **Not a single-point lie.** Detection ≈ `m/N`, with `N` the job's committed tiles. For a 512-position Qwen2.5-1.5B job, `N` is
  ≈ 10.6 M at 64 lanes a tile, or ≈ 170 k at 4,096 lanes. **64 audits catch one lied tile with probability 0.0006–0.04 %.**
- **Why single-point lies matter: grinding.** A single late-layer lie is exactly cheap lottery grinding. The ticket is a function of the
  execution (ADR-0072), so a producer who may lie undetected in the last layer re-draws its ticket at the price of the last layer: up to
  ≈ `L`× cheaper per draw.
- **So full coverage stays with holders**: panels, RFC-0006 shards, Part II sketching seats. The mesh is a sensor and never the floor.

**Attestations are positive and batched.** An auditor reports through a Part I vertex as an
`Audited { claim, leaf, result }` leaf: `result` is 0 for a match and 1 for a mismatch, and a mismatch is also handed to the court as a
named leaf. **Silence is never `Valid`.** An assigned audit with no attestation counts as not done; it earns nothing and, like any
silence, is never charged.

**The verifier's dilemma, and audit traps.** An auditor who attests "match" without auditing saves work and is wrong only when the tile
is false — rarely. Audit traps make that bet lose.

- **Who plants.** A bonded **trap setter** — any bond, drawn by the beacon at a rate `trap_rate` of audit assignments (proposed 1 %) —
  produces a claim of the class with **a planted fault** of one of D-F2's commit-point kinds:
  - a carry-out, a `TopK`, a `HistAppend` row;
  - a state checkpoint, a history tile;
  - the logits, a dissected attention output.

  Before the claim is carried, the setter carries `TrapCommitted { H(claim id ‖ fault leaf ‖ salt) }`.
- **Indistinguishable.** A trap claim is an ordinary claim of the class: the setter runs the producer's code and corrupts one committed
  tile, exactly as D-F2's drill does. The audit draw that hits it is the same beacon draw as every other, and the commitment reveals
  nothing until opened.
- **Revealed and settled.** After the audit window the setter carries `TrapRevealed { claim, fault leaf, salt }`; the fold checks the
  commitment, and the fault is checkable by the court's own leaf check.
  - An auditor that attested **match** on the trapped leaf is slashed `trap_penalty` (proposed: the claim's reservation).
  - One that attested **mismatch** earns the trap bounty.
  - The trap claim itself is voided **without** slashing the setter: it was declared before it was carried.
  - A setter that never reveals forfeits its trap deposit.
- **Cost.** One inference, a deposit and a bounty per trap, from a trap pool funded by a share of audit pay. At 1 % of audits, an auditor
  who skips work is caught in ≈ 1 % of its skips and pays a reservation each time. Skipping pays only if a skipped audit is worth more than
  `0.01 ×` a reservation, which it is not by construction.

**Per-seat bandwidth budget.** A seat declares an audit budget in bytes/DAA as a readiness field, not consensus. The draw skips seats
whose budget is exhausted, and a seat's audit pay is proportional to the openings it fetched and attested.

**Niche classes.** A class with a ready pool of 5–7 seats is where per-class collusion is cheapest: three colluding operators are a
quorum. The mesh draws its auditors from the whole bonded population, so detecting such a class's fabrication needs only one honest
auditor anywhere in the network. It does not catch single-point lies (above). That is why niche classes are the first users of IV.2's
cap.

### IV.2 Staged onboarding (capped mode)

**Legacy experiment boundary:** this section records the original mesh-only capped proposal; current implemented behavior is specified in spec 18. It is not the normal Part V path and its timing/cap estimates are not a security proof for probabilistic receipts. Part V requires §V.6's full claim coverage before licensing and §V.7's Final conditions; it does not inherit mesh-only credit, arbitrary random-subset vesting or these activation estimates.

**What it is.** A class may produce blocks under mesh audits **before full-coverage holders are seated**. Three things must hold:

- it is registered;
- it has passed the admission jury (ADR-0147);
- a **DA certificate** (§I.7) shows that its producer serves witnesses and openings: `q` mesh seats attest `Held` on the class's probe
  claims' captures.

Its production is **capped**, and its rewards are **provisional**.

**The lifecycle.** A new state `Capped { since_daa }` is appended last in `PalwModelLifecycleV1`'s Borsh order: inserting it earlier
would renumber every row, the 2026-09-10 failure. Its place in the walk is between `Prefetching` and `Probation`:

```text
Candidate ──jury──▶ Prefetching ──(DA certificate, ready < seat_count)──▶ Capped ──(full coverage seated, ready ≥ required)──▶ Probation ──▶ ActiveLimited ──▶ Active
                         └──────────────(ready ≥ required, as today)─────────────────────────────▶ Probation
```

`Probation`'s 10 probes and its finality Λ (170 DAA on testnet-12) are unchanged: a capped class still passes Probation once holders sit.
`Capped` admits at `capped_admission_permille` and **never** counts toward `panel_drawable`. Its claims are checked by the mesh, never
licensed by a class-local panel.

**Deriving the cap.** Let a capped class hold a fraction `w_c` of fork-choice weight and pwu, and `ρ_c` of rewards per DAA.

- **Unrecoverable gain.** An undetected lie gains at most its claim's reward plus its fork-choice influence. Rewards are unvested
  (below), so the reward part is recoverable whenever holders later detect the lie. What cannot be recovered is the fork-choice influence
  already spent.
- **The bound.** Require `E[unrecoverable gain per DAA] ≤ P(detect) × slashable bond per DAA`. Set `P(detect)` to the single-point rate,
  `m/N` — the mesh does not help there. Two caps follow:
  - **Weight cap.** Over all capped classes together, `w_c ≤ w_cap`, with `w_cap` (proposed 1 %) chosen so that the weight an adversary
    can mint with undetected lies cannot outrun the honest share: a fork-choice attack needs `> 1/2`, and `w_cap` keeps it far below.
  - **Grinding bound.** A single late-layer lie re-draws the ticket at ≈ `1/L` of an inference, so a capped class's tickets may be ground
    up to `L`× cheaper. The class's share of block production is capped at `w_cap`, so grinding gains at most `w_cap × L` times the
    honest rate within its own lane. The lane's per-class target retarget (ADR-0076) absorbs it within one retarget span.
- **The zero-weight hazard.** A lane whose blocks carry *no* PALW weight hands fork choice to the block hash for free: its blocks are
  ordered by the cheap lottery. A capped class therefore carries the ordinary pwu at the receipt ramp's first stage. Here the ramp
  matures on mesh `Audited` leaves instead of panel receipts, under ADR-0069's rule that weight needs adjudicability — every claim's
  leaves are court-checkable. The capped class gets **ρ_r's first stage and no later stage** until holders are seated.

**Provisional rewards.**

- Capped-mode rewards accrue unvested, in the vesting machinery the improvement protocol already uses (RFC-0004 §8.4).
- When holders are seated (`Capped → Probation`), they **re-verify a random subset — or all — of the capped claims inside the vesting
  window**. Each capped claim is a job the holders can replay; Part II sketching seats do it at decode speed.
- A re-verification that convicts slashes the producer and forfeits its unvested rewards.
- **Already-accepted blocks are not undone.** Fork choice moved, and the cap bounds by how much. Rewards are undone because they never
  vested. Fork-choice influence is not undone, which is why it is the capped quantity.

**Transition to full mode** happens when full coverage is seated. Full coverage means `required_ready_seats` holders of the class, each
of them one of: a full replica, a set of RFC-0006 layer shards covering every layer, or a Part II sketching seat with the class's
witness bytes inside its link budget.

**Time to first block.** At 150 s per DAA; prefetch at 1 Gbps.

| class | today: first block (first probe claim) | today: first non-probe block | staged: first capped block |
| --- | --- | --- | --- |
| Qwen2.5-1.5B (1.84 GB) | jury ≤ 100 DAA + prefetch 15 s + readiness ≤ 40 DAA ≈ 140 DAA (5.8 h) | + Probation Λ = 170 DAA ≈ 310 DAA (13 h) | jury ≤ 100 + DA certificate ≈ 10 DAA ≈ 110 DAA (4.6 h) |
| qwen4_exp-like (132–183 GB) | needs ≥ `seat_count` (5) ready seats, each holding 132–183 GB or a residency share with ≈ 1.45 s/token replay; prefetch 18–24 min a seat at 1 Gbps. **Possibly never on testnet-12** | — | ≈ 110 DAA (4.6 h). Holders follow: sketching seats need one 132–183 GB stream (18–24 min at 1 Gbps) and 2.3 GB resident |

For the t12 classes staging gains hours. For a qwen4_exp-sized class it is the difference between producing and not producing.

## Part V — Constraint verification is the normal Panel path (2026-10-06)

### V.1 Scope and the existing implementation boundary

For a claim using the new route, **the Panel MUST be able to discharge its assigned segment by verifying its constraints without replaying that segment end to end**. Batched Freivalds is the first implementation target; GKR/sum-check is an approved direction for compatible aggregate relations, subject to a concrete reviewed suite. Exact small operations may remain local. A full replay is useful for conformance, shadow comparison and small classes, but must not remain a hidden mandatory duty for the largest-model path.

Source audit at `7bff920dc`:

| Existing surface | Actual role and required extension |
| --- | --- |
| [`palw_verification_v2.rs`](../../consensus/core/src/palw_verification_v2.rs) | S1 partitions the job and assigns a full-replay seat plus partial seats, with coverage tally. Part V must version both the full-seat duty and coverage rule; changing a Panel command-line flag is insufficient. |
| [`PalwSeatReceiptV3`](../../consensus/core/src/palw_panel_v2.rs) | This name already means the V2 receipt plus a signed segment mask. Do not redefine it as the new scheme. `Sampled` is explicitly audit-only and does not count toward quorum/coverage. |
| [`palw_verification_profile_v1.rs`](../../consensus/core/src/palw_verification_profile_v1.rs) | A shadow timing/capacity profile, not a proof-suite registry or a soundness certificate. Keep resource scheduling distinct from the new cryptographic profile. |
| [`misaka-palw-tir-sketch`](../../misaka-palw-tir-sketch/src/lib.rs), [`Panel sketch integration`](../../kaspad/src/palw_panel/sketch.rs) | Private-sketch checker and integration provide the starting point. Their existence does not establish public transcript binding, GKR, or the new receipt/tally. |
| [Spec 18](../spec/palw/18-verification-certificates.md) | Existing vertex, witness, audit and capped-mode wire rules. Reuse compatible machinery, reserve new tags/versions, and keep old claim interpretation unchanged. |

The attachment's name “Verification V3” is a conceptual label only; proposed names below are deliberately distinct from the already existing `PalwSeatReceiptV3`. Exact serialized fields/tags require a spec update and tests before activation. This RFC revision changes no Rust code or network parameters.

### V.2 Profile, statement and constraint coverage

Propose a `PalwConstraintVerificationProfileV1` embedded by digest in RFC11's `VerificationPlanV1`. It binds:

* frozen program/artifact/tokenizer/input-output schema, full context, arithmetic and memory semantics, and constraint-compiler version;
* approved `verification_scheme_id` and version, field/modulus/range policy, repetition and aggregation rules, transcript/challenge construction;
* allowed scopes (`WholeClaim`, canonical `SegmentSet`, `AuditOnly`), the assignment and coverage policy, exact/nonlinear/lookup checker families and all boundary obligations;
* deterministic limits on evidence decoding, tensor/query/proof sizes, verifier work, localization, terminal court, DA retention and concurrent duties;
* the derivation of conditional soundness, including maximum relations/rounds/attempts, plus measured cold/warm timing and bandwidth inputs for the separate capacity profile.

This is a permissionless composition of reviewed primitive/verifier templates, not a hand-maintained allowlist of model brands. Qwen, Kimi/MoE and recurrent models may compose different templates, but cannot choose weaker security thresholds or omit a constraint. Unknown templates remain unsupported until versioned review/activation. Nodes recompute permitted parameters and coverage metadata; a producer-declared `soundness_bits` never establishes a bound.

Every execution-affecting relation has a checker: products, model-weight binding, quantization/rounding/carry, activation/normalization, TopK and expert selection, dynamic reads/writes, history and segment entry/exit, initial input and final output. TIR semantics stay exact. A correct `Y=XW` with fabricated `X`, substituted `W`, wrong expert or disconnected predecessor must fail the composed statement. Range/lookup and state-continuity checks cannot themselves be raw spot checks unless the suite includes their selection loss or a sound encoding/aggregation. Artifact-possession samples and Merkle membership prove neither computation nor complete state consistency.

### V.3 Batch repeated work, not semantic context

Collect rows from multiple tokens/cells using the same weight root, layer/expert, dtype, shape and arithmetic policy into `X[b,k]`, `W[k,n]`, `Y[b,n]`. The canonical batch directory binds every row to `(claim, segment, global token, layer, operator, expert, row index)` before random coefficients are known. Sort keys, lengths and padding are specified; missing/duplicated/reordered rows cannot escape coverage accounting or create new PWU.

Two candidate checks, whose parameters belong to the suite:

```text
right projection:       X (W r) = Y r          r ∈ F_p^n
token-row aggregation: (aᵀ X) W = aᵀ Y         a ∈ F_p^b
```

For a fixed incorrect field product, either complete vector equality has miss probability at most `1/p` with a fresh uniform vector. Repeating independently gives at most `p^-t`. Do not reduce both sides to a single scalar and reuse that bound without a separate analysis. Do not multiply repetition counts from correlated rounds, seats or moduli; enough moduli to prevent integer aliasing do not automatically provide independent repetitions. Rounding and narrowing are separate constrained relations. The relevant source for these algebraic checks is [Slalom](https://arxiv.org/html/1806.03287v2); its trusted-hardware setup is not a MISAKA assumption.

This batches verification after rows exist; it does not parallelize autoregressive generation or erase sequential dependencies. Expert-routed rows are grouped per actual expert and the routing relation is verified. Require a bounded batching wait so a low-traffic class can still finish inside its window; when `b=1`, report the actual GEMV cost. No fixed 256-token minimum may stall all smaller jobs. Initial rollout batches **within one claim**; cross-claim batching needs a separate binding, deadline and exposure design.

Direct checks still read large operands: `O(bk+kn+bn)` work per repetition. Fresh public vectors require fresh projections or authenticated computation of them. A claimed `Wr` returned by a worker is not verified by its signature or a weight Merkle root alone. Charge a verified streaming pass, a reviewed evaluation-opening protocol or another sound check. Keep Part II's secret reusable sketches private and account for their distinct leakage, refresh and adaptive-probing assumptions. Do not obtain an apparent speedup by treating unverified projections as inputs.

### V.4 GKR and encoded-query extension

Where circuit structure permits, a GKR/sum-check suite can verify a larger segment or a whole-claim constraint graph with less verifier work. Bind the wiring, layer relations, input/weight evaluations, state/memory interfaces and output statement. Sum-check rounds contribute error according to degree, field and round count; proof generation, circuit depth and authenticated input access remain costs. [GKR](https://www.microsoft.com/en-us/research/publication/delegating-computation-interactive-proofs-for-muggles/) and [SafetyNets](https://arxiv.org/abs/1706.10268) establish relevant component protocols, not an off-the-shelf verifier for arbitrary TIR/MoE.

An optional FRI/IOP suite needs an explicit constraint-to-encoding reduction and boundary checks; Reed–Solomon proximity alone does not establish computation. Plain erasure-coded DA and execution soundness have separate acceptance predicates. Select commitments compatible with the network's post-quantum policy. Public non-ZK proofs are allowed; witness generation can be expensive and is included in feasibility measurements.

A partial-segment GKR proof remains partial. A whole-claim aggregate proof covers all of its bound relation set, even if its verifier reads few encoded queries. This distinction must appear in the receipt scope and the coverage report.

### V.5 Commitments and challenge ordering

Before deriving check positions or coefficients, bind the claim, class/profile/suite, initial/final and segment boundary states, trace/constraint/batch directory, complete witness commitments and artifact/input roots. Evidence openings must refer to those exact roots. Private-sketch baseline witnesses without a commitment cannot be adopted as the new public-challenge protocol.

Resolve `PostCommitChallengePolicyV1` and derive `sample_seed` exclusively under [Part VI](#post-commit-challenge-protocol), from its future qualifying PALW work and the bound statement. Node validation recomputes its source selection, lock, seed and queries; neither seats nor registrants choose seeds or counts. Panel assignment and generation randomness have separate domains and security contracts and cannot replace this source.

Every interactive prover message precedes **its own** challenge, using the mode pinned by Part VI. A single seed exposing all future sum-check challenges is prohibited. All nodes replay the fixed transcript/source facts, never freshly draw local consensus randomness. Part VI bounds identity changes, aborts, retries, reorgs and Fiat–Shamir queries; a new branch or carrier grants no free fresh draw.

### V.6 Typed receipts and a coverage-aware tally

Propose `PalwConstraintReceiptV1` (conceptual schema; tag allocation pending):

```text
claim_id, class_id, verification_profile_hash, verification_scheme_id, scheme_version
assignment_root, scope_kind, scope_root, covered_relation_count
challenge_policy_id, commitment_root, challenge_anchor, beacon_evidence_root
sample_seed, sample_count, sampled_cells_root
field_policy_id, freivalds_rounds, soundness_policy_id, derived_soundness_bits
evidence_manifest_root, algebraic_check_root, transition_check_root, transcript_root
verdict, seat_bond, signed_daa, signature
```

`sample_count` measures the suite's queries, not an arbitrary number of tokens. `scope_root` commits to the full statement attested; `sampled_cells_root` commits to the actual query set, which may be much smaller in an aggregate proof. The plan fixes which fields are meaningful for each suite, with canonical encodings for unused fields. Round/field/security values must match derived policy; metadata is not a substitute for the proof or the honest-seat assumption.

Sign all fields with a fresh network-separated ML-DSA domain; Part I-style batching may carry the same signed meaning in a new versioned leaf. The network/version/ruleset must be bound by the message/domain. Do not append fields to existing receipt encodings, repurpose `Sampled` or tally a V1 bare `Valid` as a new scheme's pass. Bind the evidence locator to content hashes; retain material through challenges and ongoing disputes. Root-only carriage is acceptable only with separately specified availability duties, not as evidence that the checker ran.

The fold checks registration and scheme eligibility, signatures, assigned scope, anchor/seed, required counts/policy, caps and deadlines. It deduplicates by claim/seat/duty under the frozen assignment; duplicate or overlapping receipts from one seat never increase coverage. It applies the plan's **per-segment/relation** quorum/independence requirements and boundary coverage as well as any overall Panel quorum. Three receipts for the same small segment cannot license the remainder. A `WholeClaim` receipt counts as full coverage only when that suite actually attests the complete statement. A failed receipt followed by a same-duty pass cannot erase a dispute; tally and appeal follow explicit first-counted/equivocation rules.

The first implementation SHOULD preserve existing segment coverage strength while replacing replay with cheaper constraint verification: the assigned full-scope seat checks the whole statement algebraically, and partial seats check their assigned scopes. This removes its full-**replay** duty without removing its full-**scope** obligation. A later partition-only/GKR assignment can remove that role only after proving equivalent or stronger coverage/independence under a separately pinned policy. Cross-segment state equality must have designated checkers or aggregate coverage; a merely committed but unchecked boundary is insufficient.

Nodes perform these deterministic structural and lifecycle checks; Panels run the heavy verification suite off chain. Publicly replayable evidence permits audits but its presence does not mean every node validated its arithmetic. Thus a malicious accepting Panel remains a security event. A node-verified succinct-proof mode would require separately metered consensus proof verification and a new acceptance rule; it is not implicitly provided by this receipt schema.

### V.7 Error accounting, licence, Final and dispute

For `N` raw segments and `s` distinct uniform samples, a single bad segment is missed with probability `1-s/N`. If selected segments have conditional error `ε_local`, the single-fault miss bound is `1-(s/N)(1-ε_local)`, under the stated selection/check assumptions. Freivalds does not remove this selection term. Therefore an `AuditOnly` receipt cannot become full-security coverage, regardless of its small `ε_local`.

For the new qualifying route, derive the **whole-claim** `ε_check` from every product/aggregate proof, field representation, nonlinear/memory constraints, boundary binding, openings and any selection loss. Use RFC11's proposed conditional target `ε_check ≤ 2^-128`; the attachment's illustrative `2^-80` is not adopted as a silent reduction. Neither target is presently demonstrated. A conservative composition sums error bounds unless a stronger theorem applies. The general `ε ≤ Σ_i ε_i` union bound needs no independence, whereas multiplying repeated-check errors does. Shared public vectors across seats do not create independent repetitions.

Publish separate compromised-Panel, biased-beacon, unavailable-data/censorship and grinding assumptions. Across `Q` adaptive attempts the conditional check contribution can grow to `Q·ε_check`; add the appropriately modeled bad-assumption probability instead of advertising network-wide 128-bit security. An `f^5` estimate is only valid under the actual independent drawing, duty and timely-challenge assumptions, not merely because the Panel has five seats. Economic audits/PoSP incentives complement this analysis; they do not replace it.

```text
committed evidence → bound challenge → constraint checks → positive scoped receipts
                                                        → coverage/quorum → licensed
                                                        → challenge window + DA → Final
                    mismatch → localized dispute → exact terminal verdict
                    missing evidence / no quorum → existing compatible DA/timeout path
```

No receipt silence, absent proof round, unchecked relation or missed DA duty counts as positive evidence. Licence is not immediate Final. Open accepted disputes block finalization under the versioned lifecycle. Bind pre-Final weight/escrow exposure to the existing compatible accounting and RFC11/RFC8 requirements; extra checks, batches and receipts mint no extra inference work. Retain evidence until the last relevant dispute/retention deadline, including across reorg/pruning.

A failing checker files no passing receipt. It names the bound failing relation/evidence; a correct claim with a bad served witness is not automatically producer fraud. Localize through authenticated batch partitions, matrix tiles, state steps or approved circuit relations to an exact **TIR/kernel terminal** supported by court, never a VM step. Bound **total** localization work, bytes, moves and concurrency, not just the last operation. Finding a false GKR statement does not itself locate a leaf: each suite needs a tested localization protocol. Unknown/incompatible terminal semantics require a versioned kernel/court extension. No fallback may require complete Kimi-class inference on a seat or node.

Anyone with the required bond/material can challenge; exact court conviction, dismissal and DA/timeout outcomes keep their distinct evidence rules. Missing material is handled as availability; slashing requires the corresponding proven violation. Court catches filed disputes, leaving residual undetected error after Final. This proposal adds no automatic post-final rollback.

### V.8 Delivery plan and acceptance measurements

| Stage | Work | Passing evidence |
| --- | --- | --- |
| P0 — one TIR MatMul | Compare independent exact execution, Part II private sketch and fresh committed batched Freivalds. Include batch 1, uneven sizes and multiple-token batches, cold/warm weights and field/range cases. | Same deterministic outputs; single-scalar/alias/substitution faults covered; toy-field experiments agree with the error model. Report CPU/GPU time, preprocessing, bytes, RSS/VRAM and amortization. No claimed 10–100× gain without measurements. |
| P1 — complete segment | Cover nonlinear, routing, quantization, memory and boundary relations; add authenticated batch/evidence directory and bounded failure localization. | A valid segment passes without routine whole-segment replay. Forged predecessor/input/expert and disconnected-but-locally-correct matrices fail; worst dispute fits budgets. |
| P2 — receipts and claim coverage | New profile, challenge schedule, receipt/vertex version, scope-aware tally, DA retention, licence/Final and state/RPC reporting. | Partial/duplicate/foreign-scheme receipts cannot license a whole claim; no hidden full replay; old claims retain old meaning; cold IBD, reorg and pruning reproduce decisions. |
| P3 — GKR/encoded aggregation where needed | Implement a specific circuit/opening/transcript suite with reviewed composition and deterministic integer semantics. | Single sparse fault and boundary faults remain covered; adaptive-message/grinding tests and proof review; real measured prover/verifier costs and bounded exact localization. Freivalds/exact composition may serve compatible classes earlier if it already meets the same whole-claim gate. |
| P4 — large models and activation | 9B-8k, validated 2M and source-pinned Kimi K3 cases; shadow comparisons followed by separate rollout decision. | Full task/context, qualified seats, measured cost/DA/court budgets, reviewed error and Panel assumptions, claim through Final. Missing implementation or benchmarks remain explicit blockers. |

Measure prefill and autoregressive decode separately, including MoE batch fragmentation and recurrent state. Report producer execution plus witness/proof generation, seat p50/p95/p99, verification service capacity, network transfer, cold preprocessing and worst-case attack-driven disputes. Compare equivalent security/coverage and the same hardware, not a GPU miner against an unrelated CPU baseline. The new mode must fit a predeclared resource/window budget and demonstrate its claimed advantage on its target workload. Extra witness/proof work can cancel arithmetic savings. Stop promotion for a workload if it does not meet its budget; adjust the scheme or retain its compatible replay path without claiming acceleration.

### V.9 Activation and required spec changes

Use RFC11/ADR-0171's proposed `palw_probabilistic_constraints_v1` fence for the new route, with **no height selected here**. It is distinct from existing RFC7 vertex/witness/audit/capped fences. Profile and scheme versions enter class/claim identity, signature domains, evidence/receipt handling, replayable state and fingerprints as applicable. Pin the claim's rules at binding so a mid-window fence cannot reinterpret its duties. Mixed versions need explicit compatibility; a new receipt never silently upgrades an old sampled audit.

Update spec 07/08/18 and the relevant class/claim, Panel assignment, receipt/vertex pool, fold, DA, RPC and SDK paths together. Terminal court changes only when the chosen suite cannot localize to an existing supported step. Reserve wire tags centrally, append rather than renumber enum variants, charge bytes and node work before allocation, and cap pending transcript/dispute state. Recalculate Part I's carriage table with the new fields, roots, evidence availability and signatures; its legacy 66–330-byte estimate is not the new receipt cost.

Normative requirements for the new route:

* **PALW-CV-1:** an eligible immutable profile defines complete statement coverage, allowed suites and derived security/resource limits.
* **PALW-CV-2:** evidence and each interactive message are bound before their corresponding challenges; seeds and queries are reproducible and policy-valid.
* **PALW-CV-3:** all constraints in a receipt's declared scope pass the suite, and the signed receipt binds that exact scope, profile and evidence. `AuditOnly` never satisfies missing licence coverage.
* **PALW-CV-4:** licence requires scope-aware coverage and the specified quorum/independence; Final additionally requires the challenge/DA conditions and no unresolved accepted dispute.
* **PALW-CV-5:** failure escalates within bounded total resources to compatible exact court or the proper DA/timeout outcome; a failed probabilistic test alone is not a slashing verdict.
* **PALW-CV-6:** old receipt/claim semantics, unique work accounting, replay determinism and the explicit error/Panel assumptions are preserved by the migration. Unimplemented schemes stay ineligible.

These are proposed requirements. Implementations must attach a test/evidence artifact to each before activation. The execution blueprint follows [RFC11 §15](0011-permissionless-model-and-long-context-onboarding.md#15-sampling-first--court-on-dispute-2026-10-06-decision) and [ADR-0171](../adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md); neither the sketches in Part II nor this text proves the new protocol complete.

<a id="post-commit-challenge-protocol"></a>

## Part VI — Post-commit challenge protocol: the single source of truth (2026-10-08)

### VI.1 Authority, scope and security boundary

This part is the sole proposed protocol for challenge randomness shared by Kernel differential conformance,
model onboarding, per-claim verification, EXEC work slices and public verification/prosecution. RFC02/05/11/13/14/15
bind or use it; they must not invent alternative beacon/seed formulas. Its names are proposed schemas, not current
Rust types, wire ids or activated rules. Existing claims, ticket draws and Panel assignments retain their old rules.

The beacon selects unpredictable checks of an already fixed statement. It does not approve a model, define an
unknown operator, prove constraint completeness, attest availability or authorize a Kernel upgrade. Require, in order:
semantic completeness, constraint coverage, exact-court completeness, public-material completeness, resource bounds,
then this challenge protocol and probabilistic checking. G14 remains conjunctive with all those obligations.

```text
VerificationPlanV1             = what is checked
PostCommitChallengePolicyV1     = how/when challenges are derived
PalwDisputePlanV1 (RFC14)       = public material and exact failure prosecution
```

These are bound together by the new class/profile. `PalwDisputePlanV1` is the public-prosecution plan; do not introduce
a second independent `PublicProsecutionPlan` codec. No BFT committee, validator ballot, threshold signature, DNS-final
or external attestation issues/locks a beacon. Nodes derive it from their PALW canonical history.

### VI.2 Immutable policy and committed subject

```text
PostCommitChallengePolicyV1 {
    version, randomness_source_policy_id,
    anchor_delay_slots, work_count_k, source_eligibility_policy_id,
    anchor_settlement_policy_id, settlement_depth_D,
    binding_schema_id, hash_suite_id, seed_hash_domain,
    sampling_algorithm_id, field_sampling_algorithm_id, field_policy_id,
    repetition_policy_id, repetition_count, soundness_policy_id, security_bits,
    grinding_budget_policy_id, retry_limit, abort_policy_id, reorg_policy_id,
    interactive_mode, transcript_transform_id,
    resource_schedule_id, retention_policy_id
}
```

`challenge_policy_id` is the digest of the complete canonical descriptor. Checker suite, challenge policy and
soundness policy form one approved tuple; reject unknown/inconsistent ids or registrant-selected weaker repetitions.
The exact canonical codec/hash suite, integer widths, tagged optional fields and resource ceilings must be pinned in
the release; missing values are an activation blocker, not node-local defaults. No k, D, delay or retry value is
selected by this document. Changing a policy creates a new identity/version, never changes an in-flight subject.

`subject_kind` is one of `KERNEL_CONFORMANCE`, `MODEL_CONFORMANCE`, `CLAIM_VERIFICATION`, `WORK_SLICE`,
`PUBLIC_PROSECUTION`. Before challenge collection, bind its immutable subject id and roots: Kernel descriptor,
VerificationPlan, program/artifact/tokenizer/input schema, layout, input, initial/final/boundary state, all constraints,
evidence/oracle/batch directory, conformance scope, implementation set and the commitment object. A scope-specific
binding grammar uses explicit typed absence where a root is inapplicable, never an ambiguous zero wildcard.

Artifact/layout/plan/implementation/scope changes require a new pre-beacon commitment and a newly eligible future
window. Preserve the failed/aborted attempt in the retry ledger and resource accounting. Changing a candidate id,
job nonce, signature or carrier does not reset the allowed grinding budget.

### VI.3 PALW Work Beacon source eligibility and non-circularity

The selected source direction is `PalwWorkBeaconV1`: aggregate k distinct future qualifying useful-work commitments.
An eligible source must be an independently valid useful-work claim of a pre-existing Active Kernel/model profile
whose complete verification plan, G14 public prosecution and required public DA/retention are established. Freeze
eligible source profiles from the subject's commitment state, check their applicable status/obligations at contribution,
and exclude the model/Kernel candidate under test and all work depending on its proposed semantics.

G14 is a release-completion criterion, not a registrant-provided `complete=true` or committee signature. The active
versioned eligibility rule must derive qualifying capability/class/claim/DA facts from authenticated state; runtime
validation never asks a maintainer to approve a source or fetches an off-chain test report as consensus authority.
The release review proves that the source profile rules implement the G14 and independent-validity requirements.

Sources must have their canonical work commitment accepted/sealed after the subject's commitment and the policy's
future start slot S, then reach claim Final and the policy's PALW-native settlement condition. A claim computed or
committed earlier that merely reaches Final after S is not a fresh source. Future inclusion alone does not prove
physical execution time or entropy; the source profile must demonstrate unpredictable execution inputs/results
under the stated adversary and that an alternative valid entropy sample requires additional paid useful computation.

Eligibility requires unique canonical work identity and independence of validity from the challenge consuming it.
The source/challenge dependency graph must be acyclic: the target claim cannot seed its own verification, two pending
claims cannot mutually certify each other, and a candidate cannot generate its own conformance randomness. Existing
independently verified source profiles/bootstrap evidence need a separate eligibility review; absence of such a
profile leaves this route pending, without a trusted genesis-operator exception or self-certification loop.

Exclude heartbeat, BASE-0 fallback, receipt-only/signature-only material, EXEC_TX/round carriers, provisional or merely
header-valid attempts, unverified slices and bare Panel receipts. A long claim may contribute at most once through
its eligible, independently Final root useful-work identity; its N EXEC carriers never become N samples. EXEC
weight/DAA stays zero. Finishing a slice does not make it an entropy source.

Do not use block hash, timestamp, nonce, signature, freely rewrappable claim id or settlement-anchor hash as entropy.
`source_ref`/settlement position are provenance and deterministic-order facts, not fresh randomness. Canonical
work identity and execution commitment must be stable under header/carrier reattachment; if a profile cannot prove
that changing entropy requires new useful computation, it is ineligible. Correctness/Final/G14 alone is not an entropy
theorem. Cheap unadmitted attempts and precomputed public deterministic outputs remain explicit source threats.

### VI.4 Canonical collection, accumulator and lock

Derive S from the accepted commitment position and the policy's delay under the existing chain clock. Do not change
REAL/heartbeat/BASE-0 cadence to obtain randomness. On the canonical PALW selected history, enumerate eligible Final
events in `(settlement_position, occurrence_index, canonical_work_id)` order and use the first k distinct work identities.
Positions are chain-derived logical positions, not local arrival times, RPC ordering or producer-selected lists.
Duplicate/reincluded/reattached work cannot enlarge the set. Pin treatment of simultaneous events and dependency order.

Under the policy's canonical hash/encoding suite, define only here:

```text
entropy_item_i = H("MISAKA/PALW/WORK-BEACON/ITEM/V1",
                   source_profile_id, canonical_work_id_i, execution_commitment_i)
acc_0 = H("MISAKA/PALW/WORK-BEACON/V1",
          chain_genesis, ruleset_id, challenge_policy_id, commitment_root, challenge_epoch)
acc_i = H("MISAKA/PALW/WORK-BEACON/MIX/V1", acc_(i-1), i, entropy_item_i)
beacon_output = acc_k
```

The public evidence records each source reference, eligibility/Final/settlement proof, immutable work identity,
order, accumulator and policy. `anchor_settlement_policy_id` uses reviewed PALW-native settlement evidence/depth D,
never DNS/BFT finality. A raw count of heartbeat/EXEC blocks cannot manufacture useful-work settlement confidence.
`BeaconLocked` is a branch-relative protocol state after that predicate, not absolute finality or a new reorg veto.

Mixing several work items does not automatically yield unbiased randomness. One honest unpredictable contribution
helps only under the reviewed commitment/selection/withholding assumptions; an adaptive last contributor, producer
collusion, reordering, censorship, reorg and selective publication can bias a simple accumulator. Bound that bias,
grinding cost and adversarial retry exposure explicitly. Hashing more known work is not evidence of new entropy.
Claim Final is a lifecycle result with its applicable residual-check/Panel/DA assumptions, not certainty that a
source is correct. Compose source false-acceptance and correlated failure/bias into the environment/network error
model alongside downstream adaptive trials; a consumer's conditional checker epsilon does not cover those events.
VDF is not part of v1; a later VDF cannot repair an invalid or biased entropy source by itself.

### VI.5 One seed derivation and domain-separated sampling

After the statement commitment and BeaconLocked predicate, define only here:

```text
challenge_seed = H("MISAKA/PALW/CHALLENGE/V1",
    chain_genesis, ruleset_id, challenge_policy_id,
    subject_kind, subject_id, kernel_id, verification_plan_root,
    program_root, artifact_root, tokenizer_or_schema_root, layout_root,
    input_root, state_root, constraint_root, commitment_root,
    challenge_anchor, beacon_output)
```

`challenge_anchor` is the normalized policy-bound window/epoch and stable ordered work-identity descriptor,
not a producer-chosen block hash. Header hashes/signatures and mutable provenance wrappers used to prove inclusion
or settlement remain outside seed-bearing bytes: rewrapping the same work or advancing a heartbeat cannot reroll
the challenge. Any seed-bearing anchor field must be fixed before the source randomness or change only through
new qualifying work/a recorded canonical reorg under the reviewed bias model.
The binding schema covers all additional subject-specific roots inside `commitment_root`. Resolve and validate
every root/anchor from authenticated public history; cached seeds or exporter metadata are not authoritative.

Derived streams have distinct domains for `query`, `segment`, `tensor-range`, `vector`, `freivalds`, `aggregation`
and `proof-round`, with subject/scope/relation, repetition and coordinate indices. Panel draw/ticket/generation R
have separate domains and contracts. Replaying a claim's original checks uses its original subject kind and seed;
`PUBLIC_PROSECUTION` is only for explicitly committed supplemental checks, not an alternate favorable reroll or a
prerequisite for filing an already authenticated exact fraud proof.

The policy fixes unbiased field and index sampling, no-replacement/query weighting and rejection behavior. Pin a
reviewed rejection sampler/expansion with golden vectors; modulo reduction without a proven uniform mapping is
not sufficient. Enforce bounded work and named exhaustion failure rather than biased fallback coefficients.
Uniform field vectors, repetitions and whole-statement composition must match the suite's reviewed soundness.
Substreams are computational derivations under their stated assumptions; N watchers of the same seed are not
N independent repetitions. Raw sampling of a few rows does not prove unchecked semantic/constraint families.

### VI.6 Freivalds versus interactive GKR/sum-check

Freivalds fixes the full matrix statement/oracles before the source window, then derives the specified indexed
repetition vectors from this seed. Authenticate all sampled/evaluated material and include integer/field equivalence,
constraint/boundary coverage and any query selection loss. A sampled conformance PASS has an explicit scoped error
model; it is not full semantic admission, a test of every tensor or a universal whole-model correctness theorem.

For interactive GKR/sum-check, the active policy pins exactly one reviewed mode:

- **Staged beacon:** commit prover message j and the transcript prefix on the canonical history before collecting
  a new qualifying future source window for challenge j. Its roots/index are subject-specific bindings of this
  same protocol. Message j+1 may depend on challenge j, but must be fixed before challenge j+1. Price window latency,
  source scarcity, retention and total rounds. Reusing the already exposed initial beacon for every round is prohibited.
- **Transcript-bound Fiat–Shamir:** the approved transform absorbs the immutable statement, policy/source binding,
  round index and every prior message/challenge, including message j before deriving challenge j. Recompute it from
  canonical bytes, using the pinned transform and distinct round domain. Require its own random-oracle/QROM,
  commitment-binding, adaptive grinding/query and post-quantum review; a public beacon does not prove that transform sound.

Choose/freeze the mode before execution; it is not a per-prover fallback. Publishing a single seed and all future
interactive challenges before later messages is never acceptable. Transcript mismatch, missing round or source
exhaustion is unresolved/failed verification, not a passing receipt. No client RFC specifies its own transform.

### VI.7 Reorg, abort, scarcity and deterministic recovery

`Committed → BeaconCollecting → BeaconCandidate → BeaconLocked → Checked` is a semantic lifecycle, not a current enum.
Before lock, recompute the source list/accumulator under canonical reorg. After lock, a reorg removing commitment,
source Final/settlement or lock evidence rolls back all dependent challenge/conformance/activation/settlement state
with that branch. Do not preserve a stale seed or silently rewrite an already accepted result. Once changes are
canonical again, retry according to the same policy and bounded ledger. Lock neither forbids valid reorg nor adds
DNS/BFT finality. Final/post-Final liability follows the applicable lifecycle; this protocol adds no unbounded rollback.

Each changed commitment, unpublished attempt, abort, reattachment, alternate source window, transcript query and
permitted retry contributes to the stated grinding/exposure bound. Record enforceable on-chain limits and separately
analyze unobservable off-chain trials; a fee or retry ledger alone cannot bound all adversarial precomputation.
An honest timeout/reorg is not equivocation evidence. Slashing needs the actual proof/obligation rule.

With too little qualifying work, return `BEACON_UNAVAILABLE` / pending. Models can remain `RegisteredDormant`;
claims obey their explicit pending/deadline/DA rules without a false pass. Never fall back to heartbeat/BASE-0 hash,
EXEC hash, committee signature, local RNG or caller-chosen seed after a timeout. Chain, transactions, heartbeat and
BASE-0 continue under existing liveness; reserve their validation/relay resources even during collecting/backlog.
Cold/restarted/IBD/pruned nodes reconstruct the same policy, commitment, source facts, seed, queries and transcript.
Pruning cannot discard the public provenance/material needed before its last retention/liability deadline.

### VI.8 Activation and evidence gates

All gates remain open; this text performs no new beacon, conformance run or soundness proof. A separate coordinated
version/fingerprint release must supply:

1. Complete policy/codec/hash/domain/sampling/transform vectors; per-field mutations and unknown/version mismatch refusal.
2. Independent source-cost, unpredictability, bootstrap/non-circularity, bias/last-contributor/withholding/reorg and
   adaptive-grinding analysis. k/D/delay selection and measured completion latency; no inference-cost-only entropy claim.
3. Candidate/kernel/source substitution, precommit/prefinal/cheap-header exclusion, duplicated root/slice/carrier,
   abort/retry and changed artifact/plan/layout tests; bad sources cannot activate a candidate or settle work.
4. Fixed-statement Freivalds and per-message GKR transcript/order tests, soundness composition, failed-check-to-bounded
   exact court, DA/default distinction and cold outsider replay without producer-private state.
5. RFC11's three onboarding stages, RFC13's resumable commitment/evidence records, RFC14/G14 and RFC15's separate gates;
   no semantic admission or Panel=0 inferred from conformance alone.
6. Restart/IBD/pruning/reorg/activation-boundary equality and useful-work scarcity/flood tests showing unchanged main
   heartbeat/BASE-0/clock/EXEC_TX liveness. Existing ticket/Panel seed rules and old identities stay frozen for old subjects.

## Relations

- **RFC-0006 (layer-sharded panels, lane M3).** Shards are the bandwidth-efficient way to use the global seat set for full coverage. A
  shard seat holds the weights of its layers and recomputes them, and the boundary rows are committed and opened; nothing is
  transferred per position beyond those rows. Part II and RFC-0006 are complementary:
  - **Shards** suit prefill-heavy jobs and classes whose seats can hold a layer range. No witness is needed.
  - **Sketching seats** suit decode of classes no seat can hold. No weights are held, but the witness crosses the link.
  - **Together.** A shard seat may itself check its layers algebraically, keeping its layers' sketches (1 % of them) and taking its
    layers' witness. RFC-0006's stratified panel then counts a sketching seat for a shard exactly as a holder.
- **Lane M2's residency (ADR-0112 for IR classes).** The sketch builder reads weights through the residency's row source
  (`TirRowSourceV1`), in row order, holding a row at a time. Under a residency, the RAM-versus-NVMe comparison of §II.9 is the seat's
  actual choice. A seat whose budget is a fifth of the artifact is in the NVMe column for most of a large class.
- **RFC-0002.** The witness is defined by the program alone. The moduli come from the admission range analysis, refined. The exact half
  of the check is the reference evaluator, and the audit is the demand evaluator. Nothing in this RFC changes a primitive, a commit point,
  a cone or the court.
- **RFC-0002's staged registration and mining enablement.** IV.2's `Capped` state is the middle stage between "registered and
  inspectable" (`Candidate`) and "panel-verified" (`Probation` onward). It is enabled by the same seat-readiness facts, with the mesh
  where today the class-local panel is required.

## Proposed Spec text (new chapter `spec/palw/18-verification-certificates.md`)

**Historical Parts I–IV text:** the chapter now exists; its current wire/activation rules govern deployed behavior. Rules below describe the original proposal and are retained for context, not an override of that chapter. Part V adds PALW-CV-1…6 under its own future fence. In this older path, rules marked *(node)* are seat-local software; that does not exempt Part V's profile, receipt or coverage obligations from consensus.

- **PALW-VC-1 (one vertex per seat-round).** Past `palw_verification_vertex_v1`, the fold MUST accept at most one
  `VerificationVertexV1` per `(seat_bond, round)`, the first in accepted order, and MUST refuse a vertex whose leaves are not strictly
  ascending, whose count or size exceeds the caps, or whose `leaves_root` does not recompute.
- **PALW-VC-2 (the signature).** A vertex MUST verify under its seat bond's registered ML-DSA-87 key, over `vertex_message_v1`, with
  context `misaka-palw/verification-vertex/mldsa87/v1`.
- **PALW-VC-3 (the tally).** A `Verdict` leaf MUST count toward its claim only under the conditions `validate_receipt_quorum_v2` applies
  to a receipt. A claim whose counted `Valid` leaves reach the panel quorum MUST transition as `ReceiptLicensed` would, in the block that
  completes the quorum.
- **PALW-VC-4 (a verdict stands).** A seat's first counted verdict for a claim MUST NOT be replaced by a later leaf of the same seat.
- **PALW-VC-5 (equivocation).** Two vertices of one `(seat_bond, round)` with different roots, both validly signed, MUST be accepted as
  `VertexEquivocation` evidence. The fold MUST slash, forfeit locks and eject as §I.6 states.
- **PALW-VC-6 (DA attestations).** A `Held` leaf MUST bind its signer to serve the named chunks until the claim's challenge window
  closes. An ADR-0111 availability request MAY name an attester, and an attester that fails it MUST be slashed its `Held` exposure.
- **PALW-AV-1 (the witness)** *(node)*. A producer SHOULD serve, on request during the receipt window, the output of every `MatMul` of the
  canonical serving set at every position of a claim, with the claim's committed rows.
- **PALW-AV-2 (secrecy)** *(node)*. A seat MUST NOT disclose its sketch secret, its keys or its sketches.
- **PALW-AV-3 (moduli)** *(node)*. A seat checking a node algebraically MUST use moduli whose product exceeds the span of the node's
  refined proven interval, and MUST refuse a served value outside that interval.
- **PALW-AV-4 (failure is not a verdict)** *(node)*. A seat whose check fails MUST NOT file `Valid` for the claim. It escalates by §II.8.
- **PALW-AM-1 (audit assignment).** Past `palw_audit_mesh_v1`, audit assignments MUST be drawn from the anchor beacon over all bonded
  seats, by eligibility weight, after the claim commits.
- **PALW-AM-2 (positive attestation).** Only an `Audited` leaf counts as an audit. Absence MUST NOT count as a match.
- **PALW-AM-3 (traps).** A `TrapRevealed` that opens a prior `TrapCommitted` MUST void the trap claim without slashing its setter, MUST
  slash every auditor whose `Audited` leaf attested a match at the planted leaf, and MUST pay those who attested a mismatch.
- **PALW-AM-4 (capped mode).** Past `palw_capped_onboarding_v1`, a class in `Capped` MUST admit at most `capped_admission_permille`. All
  capped classes together MUST hold at most `w_cap` of fork-choice weight. Their claims' rewards MUST NOT vest before the class leaves
  `Capped` and the vesting window's re-verification closes.

## Activation plan

**Legacy activation record:** the four-fence plan below was drafted before integration. Current spec 18 records the t12 DAA-5,300 activation; the old table is not a claim that those fences remain dormant today. New Part V uses §V.9's separate proposed fence, with no height. Preserve the existing **Some-only** fingerprint discipline for a new field: a preset leaving it `None` must retain its prior fingerprint. Integration includes all four places:

1. the `Params` field;
2. `for_each_fence`;
3. the Some-only fingerprint write;
4. the `never()` → `None` collapse in `normalize_values_a_scheduled_fence_drags_with_it`.

Each goes at an **independent height**: a fence at an already-scheduled height is invisible to the fork id.

| fence | what it arms | depends on | drill (on the shipping binary, crossing the fence) |
| --- | --- | --- | --- |
| `palw_verification_vertex_v1` | vertex objects, tally licensing, equivocation evidence, `Held` leaves; receipts accepted for one receipt window past it | — | a salted t12 drill chain. Licences by receipts below the fence and by tally above it; a claim bound across the fence licenses on the old path; an equivocating seat slashed; carriage per licence measured at the drill's issuance ×10 |
| `palw_witness_manifest_v1` | the witness root in the trace manifest, so that `Unavailable` names witness chunks | — | a producer withholding one witness chunk: seats file `Unavailable` naming it, and the claim voids at `ReceiptTimeout` with nobody slashed |
| `palw_audit_mesh_v1` | the audit draw, `Audited` leaves, `TrapCommitted` / `TrapRevealed`, audit pay | `palw_verification_vertex_v1` | a fabricating producer at `q` = 0.1 caught within the predicted number of audits; a lazy auditor slashed by a planted trap; the trap setter not slashed |
| `palw_capped_onboarding_v1` | the `Capped` lifecycle state, the caps, provisional rewards, the re-verification window | `palw_audit_mesh_v1` | a new class registered past the fence produces capped blocks under mesh audits; holders seated; one capped claim forged and convicted in re-verification with its unvested reward forfeited; the weight cap never exceeded |

Part II's original local checker needs no new receipt-policy fence: it is a node release. The original rollout called for a node flag, default off, with the soundness suite and a mirror
check — the checker run beside a full replay on every claim a seat already replays, with any disagreement logged and alarmed — before any
seat relies on it. That rollout does not activate Part V: its new receipts, duties and tally require the separate fence and shadow gates in §V.8–9.

## Alternatives

| alternative | why not |
| --- | --- |
| Aggregate ML-DSA-87 signatures | They do not aggregate (ADR-0098 §3). A vertex amortises one signature over a round instead |
| Root-only vertices plus per-claim licences carrying Merkle proofs | ≈ 2.3 KB a claim with `Hash64` paths: 6× better than today, and a licence object, a collector and an ordering still exist. Carrying the leaves is 44–218× better and removes the collector |
| A Narwhal/Bullshark DAG of vertices with its own ordering | GHOSTDAG already orders, and the chain carries every vertex, which is availability. Parent edges would certify nothing new |
| Weight from cluster agreement ("the largest compatible cluster is correct") | Refused by the user's decision. A vote over executions is a vote on who holds the most seats; 1-of-N with the exact court is the model |
| Serve `Q·Kᵀ`'s scores | 8 bytes a score against `d` = 128 MACs saved; never pays below ≈ 10 Gbps (measurements §5) |
| A failed algebraic check as an immediate conviction | A served witness may be wrong even for a correct claim; localize and use exact court. Public vectors are unsafe if known before the claimed result is fixed; Part V defines post-commit public checks separately from Part II's secret reusable sketches |
| Freivalds/GKR replacing normal segment replay | **Selected in Part V**, with complete constraints, reviewed error composition and evidence-bound receipts; exact court remains the dispute endpoint |
| zk proofs of the execution | Orders of magnitude above recompute (RFC-0002 Alternatives) |
| Sample more intervals instead | 90 % catch of a one-point lie costs each seat 37 % of the job (ADR-0098), and grinding makes the uncaught 10 % valuable (ADR-0072) |
| Mesh audits as the security floor | `m/N` against single-point lies: 64 audits catch one lied tile of a 512-position job with probability ≤ 0.04 % |

## Security and economic analysis (summary)

- **Part I.**
  - Equivocation is slashed by two signatures alone.
  - A verdict once counted stands.
  - Compact references can be ambiguous only after a 2^64 birthday search, and an ambiguous leaf is ignored.
  - The tally applies exactly the per-receipt conditions of today.
  - The fold's signature work falls from 3 per claim to 1 per seat-round, which also removes the gossip door's receipt-flood surface (the
    2026-09-24 pool flush).
- **Part II.**
  - Sound with probability `1 − 1/p` per dishonest node, provided `v` is secret. Sketches are as secret as `v`.
  - A forced single modulus is a demonstrated hole, closed by the ladder.
  - A failed check convicts no one, so the seat escalates to the exact court.
  - A producer gains nothing by serving a bad witness: it loses its licence.
  - Seats are heterogeneous by design, and the check is local.
- **Part III.** The historical independent-checker example gives `f^5` for full coverage and ≈ 95 % escape for raw interval sampling at `f = 0.2`. Part V requires actual coverage/tally and assumption analysis; these numbers are not its whole-claim soundness bound.
- **Part IV.**
  - The mesh catches fabrication at `1 − (1 − q)^m` and single-point lies at `m/N`. It is a sensor.
  - Traps price lazy auditing.
  - Capped mode bounds the unrecoverable part of a lie — fork-choice influence — by `w_cap`, and makes the recoverable part, rewards,
    revocable.

## Historical open questions (Parts I–IV)

These were the original draft's questions. Spec 18 §§18.0/18.20 records subsequent implementation decisions; do not reopen settled wire/policy choices merely because this list is retained. Part V's remaining decisions follow this list.

1. **`round_daa` and the leaf caps.** 1 DAA (today's licence latency) or longer (less header overhead at low load)? Should a seat be
   allowed to sign mid-round when its leaves fill a vertex?
2. **`Compact` references in v1**, or `Full` first and `Compact` behind a second fence?
3. **The equivocation penalty.** 100 ‰ of the bond plus locks plus ejection, or the ADR-0152 per-act table?
4. **Part I migration.** Receipts accepted for one receipt window past the fence, or until every claim bound before the fence is
   licensed or void?
5. **Witness bytes in the class profile** (§II.10). Should they enter the verification-window derivation, and seat pay?
6. **Seat pay for algebraic checks.** Should a receipt filed by a sketching seat earn what a replay earns? It costs the network the same
   licence, but costs the seat less compute and more bandwidth.
7. **Audit-mesh parameters.** `trap_rate` (1 %?), `trap_penalty` (a reservation?), audits per claim, and audit pay.
8. **`w_cap` for capped mode** (1 % of fork-choice weight?) and `capped_admission_permille`. Re-verify all capped claims, or a random
   subset with a stated detection rate?
9. **Per-row history sketches** (§II.3) are modelled, not built. Build them before any long-context class relies on Part II?
10. **The canonical serving set** (§II.2). Keep it as node policy, or pin it in the class profile so that every producer serves the same
    bytes?

## Decision

**Selected 2026-10-06:** adopt Part V's constraint-based Panel verification as the new large-model direction. Begin with batched Freivalds on a real TIR MatMul, extend to complete segment constraints and scope-bound receipts, then aggregate compatible relations with GKR/IOP where measurements justify it. Preserve exact bounded court on dispute and RFC11's whole-claim security target.

Before implementation is eligible for activation, settle the exact suite/transcript and opening construction, canonical receipt encoding/tag allocation, assignment/coverage policy, derived parameter limits, total localization bound, evidence retention and measured capacity. §V.8 is the delivery order and §V.9 the migration contract. No deployment, benchmark success, Kimi compatibility or change to existing claim semantics is established by this decision.

## Mission alignment amendment — 2026-10-07

G7およびPart IV/Vの外部監査主体を、Panelに選ばれたseatから普通のpublic bond全体へ拡張する。private sketches/preprocessingは通常検査の最適化に限り、それを持たない外部verifierにも独立した公開localization/conviction経路を用意する。scope-bound receiptsの集約やquorumは算術真実の根拠ではない。Freivalds/GKRの失敗をそのままslashせず、認証されたbounded terminal proofへ落とす。raw cell samplingと未検出確率は別途扱う。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](0014-panel-independent-fraud-prosecution.md)、[RFC15](0015-panel-free-permissionless-verification.md)を参照する。

## Bond予算・総影響保存の改定 — 2026-10-10

[ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)と[RFC15 §8](0015-panel-free-permissionless-verification.md)を適用する。

確率的receiptとexact escalationに共通bond/DAA予算を追加する。claim容量×mでもblock/reward/Final weightの総配分は同じであり、受理予約とFinalで検査する。
全constraint coverage・error composition・challenge bindingを維持する。carrier/claim数の細分化だけで独立乱数sourceや追加work creditを作らず、source/Final/settlement readerを同じversioned会計へ接続する。

本節は将来の規範・受入条件を改定する。過去の実装/測定、旧claim会計とactivation履歴は保持し、文書改定だけで新規則を有効化しない。
