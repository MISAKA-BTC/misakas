# RFC-0007: PALW Verification Certificates and Algebraic Checks — batched seat vertices with licence by tally (Part I), seat-local Freivalds verification of PALW-TIR executions (Part II), security parameters and escalation (Part III), the global audit mesh and staged onboarding (Part IV)

| Field | Value |
| --- | --- |
| Status | Draft, 2026-10-01 — design. **Parts I and III implemented under the dormant fence `palw_verification_vertex_v1` (branch `rfc7/vertex`, 2026-10-03; spec `docs/spec/palw/18-verification-certificates.md`).** Part II prototyped and measured (crate `misaka-palw-tir-sketch`). Part IV.1's audit prototyped off-chain. Part IV.2 is text only |
| Author(s) | MISAKA core (drafted with Claude; lane M4) |
| Created | 2026-10-01 |
| Normative dependencies | **RFC-0002** (PALW-TIR v1 and its Phase F integration: commit points, the step tree, cones, the demand evaluator, the court) for Parts II and IV; the V2 panel (`palw_panel_v2`), ADR-0065 Decision 4, ADR-0098 and ADR-0111 for Parts I, III and IV |
| Affects | spec/palw 07 (claims, panels, receipts, licensing), 08 (verification), 09 (court: availability requests only — the exact court is unchanged), 10 (rewards and slashing: equivocation, trap settlement, capped-mode vesting), 14 (node: the checker, the audit worker, the vertex pool), 16 (fences), and a new chapter `spec/palw/18-verification-certificates.md` · all networks (dormant until armed) · `consensus/core` (Part I objects and fold, Part IV objects and lifecycle) · node software (Part II checker, Part IV.1 auditor) |
| Branch | `rfc7/algebraic` (off `rfc4/int` @ `ce04e5c22`): the crate, its tests, the measurements, this text · `rfc7/vertex` (off `rfc4/int-release` @ `785d601b4`, with `rfc7/algebraic` merged): Parts I and III |
| Related | RFC-0006 (layer-sharded panels, lane M3), lane M2's runtime residency for IR classes (`TirRowSourceV1`, ADR-0112 for IR classes), lane P's root-cause report of the testnet-12 panel backlog (`rcore/int-10-p1:docs/design/palw/t12-panel-backlog-1001.md`), ADR-0029 (carriage), ADR-0038 (receipts are claims), ADR-0062 (the data-availability court), ADR-0069 (weight needs adjudicability), ADR-0072 (the ticket is the execution), ADR-0080 (a receipt is 4,772 bytes), ADR-0097/0099/0100 (shards, the stratified panel), ADR-0103 (held context), ADR-0112 (residency), ADR-0124 (supplementary receipts), ADR-0133 (verification is its own clock), ADR-0147 (the admission jury), ADR-0152 (collateral, ejection), ADR-0160 (capacity) |

## 概要(日本語)

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

PALW's binding constraint on testnet-12 is verification supply: seats replay claims and file signed receipts, and the receipts a licence
needs are the scarce resource (lane P, 2026-10-01). This RFC adds two levers and the rules around them.

1. **Part I — cheaper aggregation.** A seat signs one **vertex** per round: the Merkle root of every verdict it reached that round,
   together with the verdicts themselves as leaves. Vertices ride once. The fold **tallies** their leaves and licenses a claim when its
   panel's `Valid` leaves reach quorum. A seat that signs two different vertices for one round has equivocated and is slashed. GHOSTDAG
   orders vertices; no Bullshark-style ordering layer is needed. Carriage per licensed claim falls from ≈ 14.4 KB to 66–330 bytes, and
   capacity ×1000 fits a few blocks per DAA.
2. **Part II — cheaper checks** (prototyped). A PALW-TIR execution never commits its `MatMul` accumulators. The producer serves them as
   a **witness**. A seat checks each weight product against its secret **sketch** `S = W·v` by Freivalds' algorithm, checks each
   activation × activation product with a fresh vector, recomputes every other node exactly with the court's own reference evaluator, and
   compares the committed rows it derives with the claim's. Each node is checked over the fewest primes its refined proven interval needs.
   The exact court is unchanged. Measured on real configurations, this pays for **decode tokens of classes the seat does not hold in RAM,
   over links of 100 Mbps or more**. It does not pay for prefill on ordinary links, and for resident classes only past 1–2 Gbps. Its value
   is feasibility: a seat with 2–6 GB checks classes of 80–674 GB.
3. **Part III — security parameters and escalation.** 1-of-N with the exact court; never a cluster majority. Weight comes from bonded,
   Sybil-resistant seat eligibility, post-commit random assignment, permissionless challenge and random audits. Part III states `m` per
   interval, the audits, and the bond arithmetic.
4. **Part IV — the global audit mesh and staged onboarding.**
   - Any bonded seat, drawn post-commit, audits random committed leaves of any class from openings alone. The audit is a sensor for
     wholesale fabrication, defended against the verifier's dilemma by planted traps.
   - A newly registered class may produce under the mesh, **capped** and with **unvested** rewards, before full-coverage holders are
     seated. The holders re-verify its capped claims inside the vesting window.

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

- G1. Licence carriage per claim is a small constant (tens to hundreds of bytes), and capacity ×1000 fits the block mass. (Part I)
- G2. One signature per seat per round, not per claim, at the gossip door and in the fold. (Part I)
- G3. A seat checks a PALW-TIR execution without recomputing its weight products, and without holding the weight matrices. (Part II)
- G4. The check is sound by construction: per node, an error escapes with probability at most `1/p`, with moduli chosen from the
  admission range proofs. (Part II)
- G5. The check is seat-local. No consensus rule decides which seat checks how, and the court never sees a sketch. (Part II)
- G6. Security stays 1-of-N with the exact court. Full coverage of an interval comes from holders — panels, RFC-0006 shards, or Part II
  sketching seats — and never from a vote. (Part III)
- G7. Every bonded seat can audit every class from openings alone, and silence is never a verdict. (Part IV.1)
- G8. A class can produce before full-coverage holders are seated, with its damage capped and its rewards revocable. (Part IV.2)

**Non-goals.** A probabilistic court: the court stays exact. Weight from cluster agreement ("the largest compatible cluster is
correct"): refused by the user's decision. zk proofs. A BFT ordering layer: GHOSTDAG orders. Changing the exact court's terminal, its
cones or its ceilings. Making Part II mandatory for any seat.

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

`m` is the number of independent checkers of one interval:

| how seats check | `m` | escape of a one-point lie | at `f` = 0.1 / 0.2 / 0.33 |
| --- | --- | --- | --- |
| full replay or algebraic, all 5 seats, duty to `Final` | 5 | `f^5` | 0.001 % / 0.03 % / 0.4 % |
| full, licensed at quorum, the last 2 seats stop | 3 | `f^3` | 0.1 % / 0.8 % / 3.6 % |
| `k = 4` of `N = 299` intervals sampled (ADR-0098) | 5 × 4/299 | `(f + (1 − f)(1 − k/N))^5` | 94.1 % / 94.8 % / 95.6 % |

Two rules follow:

- **Duty runs to `Final`, not to the licence.** Seats keep checking after quorum, and a later finding opens the court inside the
  121-DAA licence-to-`Final` window. That keeps `m = 5` for every class a seat can check fully.
- **Full coverage where it is affordable.** For classes where full replay or Part II is affordable, seats check every interval. Where
  neither is, sampled intervals are a sensor, and weight-bearing work needs full-coverage holders: RFC-0006 shards, or Part II sketching
  seats.

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

Applies past the fences named. Rules marked *(node)* are node software and bind no consensus object.

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

Every fence is dormant on every shipped preset. The fingerprint is **Some-only**: a preset that leaves a fence `None` fingerprints
byte-identically to a build without the field. Each fence is written in all four places:

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

Part II's checker needs no fence: it is a node release. It ships behind a node flag, default off, with the soundness suite and a mirror
check — the checker run beside a full replay on every claim a seat already replays, with any disagreement logged and alarmed — before any
seat relies on it.

## Alternatives

| alternative | why not |
| --- | --- |
| Aggregate ML-DSA-87 signatures | They do not aggregate (ADR-0098 §3). A vertex amortises one signature over a round instead |
| Root-only vertices plus per-claim licences carrying Merkle proofs | ≈ 2.3 KB a claim with `Hash64` paths: 6× better than today, and a licence object, a collector and an ordering still exist. Carrying the leaves is 44–218× better and removes the collector |
| A Narwhal/Bullshark DAG of vertices with its own ordering | GHOSTDAG already orders, and the chain carries every vertex, which is availability. Parent edges would certify nothing new |
| Weight from cluster agreement ("the largest compatible cluster is correct") | Refused by the user's decision. A vote over executions is a vote on who holds the most seats; 1-of-N with the exact court is the model |
| Serve `Q·Kᵀ`'s scores | 8 bytes a score against `d` = 128 MACs saved; never pays below ≈ 10 Gbps (measurements §5) |
| Algebraic verification as the court | The court is exact and needs no secret. A public sketch would be forgeable (§II.4), so a probabilistic check cannot convict |
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
- **Part III.** Full coverage gives `f^5`, and sampling gives ≈ 95 % escape for the best one-point lie at `f = 0.2`. Weight-bearing work
  needs full coverage.
- **Part IV.**
  - The mesh catches fabrication at `1 − (1 − q)^m` and single-point lies at `m/N`. It is a sensor.
  - Traps price lazy auditing.
  - Capped mode bounds the unrecoverable part of a lie — fork-choice influence — by `w_cap`, and makes the recoverable part, rewards,
    revocable.

## Open questions

1. ~~**`round_daa` and the leaf caps.**~~ **Settled (the lead, 2026-10-03):** `round_daa` = 1 DAA; one vertex per seat per round; a
   per-vertex leaf cap (1,024 leaves, 80,000 bytes); no mid-round signing in v1. Implemented as `PALW_VERTEX_ROUND_DAA_V1`,
   `PALW_VERTEX_MAX_LEAVES_V1`, `PALW_VERTEX_MAX_LEAF_BYTES_V1`; a seat whose round holds more leaves than a vertex carries the
   excess into its next round.
2. ~~**`Compact` references in v1**, or `Full` first and `Compact` behind a second fence?~~ **Settled:** `Compact` ships in v1 under the
   same fence (it is what brings per-claim signature data from 14.4 KB to 66–330 B); `Full` is accepted too. An unknown or ambiguous
   compact reference is ignored by the fold.
3. ~~**The equivocation penalty.**~~ **Settled:** a new act in the ADR-0152 per-act slash table: 100 ‰ of the bond
   (`PALW_VERTEX_EQUIVOCATION_PENALTY_PERMILLE_V1`) plus every lock the seat holds on a claim either vertex names, plus ejection (a forced
   retirement: the bond backs its existing claims to resolution and takes no new work).
4. ~~**Part I migration.**~~ **Settled:** receipts are accepted past the fence until every claim bound before the fence is licensed or
   void (no stranding). The path rule is one predicate (`palw_vertex_claim_licenses_by_tally_v1`): a claim whose panel bound at or after
   the fence licenses by tally and refuses a receipt licence by name; a claim bound before it licenses on the receipt path, and a leaf
   naming it counts for nothing. The two paths never count one seat twice.
5. **Witness bytes in the class profile** (§II.10). Should they enter the verification-window derivation, and seat pay? *(Parts II/IV.)*
6. **Seat pay for algebraic checks.** Should a receipt filed by a sketching seat earn what a replay earns? It costs the network the same
   licence, but costs the seat less compute and more bandwidth. *(Parts II/IV.)*
7. **Audit-mesh parameters.** `trap_rate` (1 %?), `trap_penalty` (a reservation?), audits per claim, and audit pay. *(Part IV.)*
8. **`w_cap` for capped mode** (1 % of fork-choice weight?) and `capped_admission_permille`. Re-verify all capped claims, or a random
   subset with a stated detection rate? *(Part IV.)*
9. **Per-row history sketches** (§II.3) are modelled, not built. Build them before any long-context class relies on Part II? *(Part II.)*
10. **The canonical serving set** (§II.2). Keep it as node policy, or pin it in the class profile so that every producer serves the same
    bytes? *(Part II.)*

## Decision

**Part I and Part III: implemented under the dormant fence `palw_verification_vertex_v1`** (branch `rfc7/vertex`; the lead's decisions of
2026-10-03 on questions 1–4 above). Parts II and IV (questions 5–10) are the algebraic lane's.

What the implementation fixed where this text left room (spec `docs/spec/palw/18-verification-certificates.md` is normative):

- **The leaf carries no segment mask.** A `Valid` attests exactly the seat's assigned mask (`validate_receipt_coverage_v2` requires it of
  a receipt), so the tally derives it. A `Verdict` leaf is 67 bytes with a `Full` reference and 23 with a `Compact` one (the RFC's 66 and 22
  plus the verdict's tag byte; `Unavailable` adds 12).
- **The tally rides the licensing arm.** Counted leaves are expanded into the `Vec<PalwSeatReceiptV3>` a `ReceiptLicensedV2` of the same
  seats would carry and fed to the same fold arm, so the licence, its locks, its door record and its recount are the receipt path's.
  On R-core+ the coverage door needs every segment attested twice, which is the full seat and the four partial seats: the tally licenses
  when the fifth `Valid` lands, as the receipt path does.
- **The state is three tables** (`vertex_rounds`, `vertex_tallies`, `vertex_held`), one Some-only root block `vertex/v1`, one carriage tail
  `0xE4`, one generic delta entry `VertexRow` (delta 101). Rows of a round are kept for the evidence window (1,200 DAA) and swept 256 a
  block; a tally lives while its claim is `PanelBound`; `Held` rows while it is live.
- **Equivocation evidence carries two headers** (≈ 4.9 KB each) and, when the filer has them, the leaves of either side, so the fold can
  forfeit the seat's locks on the claims they name. One conviction per `(seat, round)`.
- **The `Held` exposure.** Every attester of a claim whose data a data-availability default concludes was not served loses 5 ‰ of its
  collateral (`PALW_VERTEX_HELD_EXPOSURE_PERMILLE_V1`), charged beside the producer's own charge. A certificate is `q` = 3 equal leaves.
- **Prerequisites** (named by `validate_palw_v2`): `palw_verification_v2`, `palw_rcore_plus`, `palw_unavailable_abstains`,
  `palw_panel_economy`, `palw_objective_offence`, each in force at or below the fence.
- **Tags and numbers**: objects 91 (`VerificationVertexV1`) and 92 (`VertexEquivocationV1`), delta entry 101, carriage tail `0xE4`.
