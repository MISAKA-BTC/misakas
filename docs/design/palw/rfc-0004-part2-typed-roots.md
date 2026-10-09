# RFC-0004 Part II — computation specifications with typed roots (lane R4X, 2026-10-08)

Branch `rfc4/part2-typed-roots` (base `a15a8e212`). Spec of record: RFC-0004 Part II §§II.1–II.4; direction note
`beyond-weight-files.md`. Everything here is **dormant**: it sits on the kernel route (tag 110), which is behind
`palw_probabilistic_constraints_v1` (refused at every real height), and the typed kinds additionally need the typed-roots kernel
descriptor to be Active in the route's schedule (§9: the fence). Neither live t12 id moves (params `5ee7fd8ee019968c…`, schedule
`1678e07359f6727e…`): no `Params` value of any preset changes, and an unarmed ledger's roots are byte-identical.

> **日本語要約:** クラスは「計算仕様 + 型付き root の列」を束縛する。`Weights` だけなら今日のクラスとバイト単位で同じ(同じ class id・行・
> ledger root)。`Memory` は「規則プログラムの Fixed state の書き込み」を一歩(step)ごとの post-state とし、次の step と次の job へ
> param overlay として渡す。各 step は既存 K2 claim と同じ形なので、既存の court がそのまま一歩に局所化して裁く。pre-state は claim
> の DA 義務(stage 0x40 の demand、出さなければ default)。chain は memory line の head を Final で進め、Final 後の有罪で巻き戻す。
> **head は構造的に公開**: claim は post-state を開いて運び(包含時に trace の commitment と照合)、line は head の tensor を持つ
> claim を記録する(`head_source`、`None` = 登録済み `M0`)。次の job は誰でも chain だけから作れる(最後に進めた producer の私物にならず、
> 誰も持たない tensor で止まらない)。
> `Retrieval` は公開 snapshot の Merkle root・決定的 index・整数スコアと FR-09 の tie 規則(κ = (s+2^SB)·2^b + (2^b−1−id))で、
> wrong item / missed better item の court と snapshot slice の DA(stage 0x80+s)を持つ。`Composite` は登録済みクラスの列で、
> tool stage は検証済み kernel(Retrieval か TIR)だけ。辺は on-chain 値なら包含時に厳密再計算、committed 値(logits→query)なら
> RFC-0003 型の edge court。全種類 OPV 専用(K2S v4 と同じ理由)。

## 0. What is new and what is reused

| | New | Reused unchanged |
| --- | --- | --- |
| wire | one kernel-route object `Spec` (inner kind 19) carrying a versioned `SpecObjectV1`; `ProsecutionV1::Spec`; `ClaimBodyV1::Spec` | tags 110/111 carriers, signatures, chunking, `SealClaim`, `FileProof`, `FileDemand`, `Respond`, `RequestExit`, `Withdraw` |
| state | ledger tables 22 spec classes, 23 spec jobs, 24 memory lines (in the root only when non-empty) | `claims`, `demands`, `served`, `job_claims`, `seals`, OPV rows, bonds; lifecycle; settlement |
| courts | memory step dispatch (overlayed record), retrieval wrong-item / missed-better-item, composite stage dispatch, the logits→query edge court | `FreshVerifierV1::try_proof` (every kernel relation), `verify_decode_fault_v1`, the whole demand/serve/default path |
| kernel | the typed-roots extension descriptor `K2-TR-v1` (kernel line 3, no TIR families of its own) | K2-TIR-v1/v2 for every TIR program a typed class runs |

The point of the shape: **a typed root never introduces a second way to judge arithmetic.** Every TIR value a typed class commits is
judged by the K2 court it would have as a plain class; the typed kinds add only (a) where a param's commitment comes from (memory),
(b) small exact integer relations over openings against a public snapshot root (retrieval), and (c) the equality of an edge (composite).

## 1. The wire form

```text
SpecObjectV1 (inside KernelRouteObjectV1::Spec, inner kind 19)
  RegisterClass { spec: ComputationSpecV1 }                         0
  PostJob       { job: SpecJobV1 }                                   1
  CommitClaim   { claim: SpecClaimV1, body: SpecClaimPartsV1 }       2

ComputationSpecV1 { version: u16 = 1, mode: VerificationModeV1, roots: Vec<TypedRootV1> }
TypedRootV1 (each kind versioned by its own discriminant; a new version is a new discriminant)
  WeightsV1   { descriptor, program_bytes, plan, param_commitments }      0   (= today's RegisterClass payload)
  MemoryV1    { line, slots: Vec<MemorySlotV1>, initial_root, max_steps } 1
  RetrievalV1 { snapshot: SnapshotV1, index: IndexV1, rule: RetrievalRuleV1, extension }   2
  CompositeV1 { extension, stages: Vec<CompositeStageV1> }               3
```

Accepted combinations (v1), anything else refused by name (`KIND_COMBINATION_UNSUPPORTED`):

| roots | class | class id |
| --- | --- | --- |
| `[WeightsV1]` | today's single-program class | **`single_class_id_v1(descriptor, program, plan, pc, mode)`** — the legacy id |
| `[WeightsV1, MemoryV1]` | memory class (the Weights root is the update-rule program, its base weights and `M0`) | `H(spec-class/v1; extension digest, borsh(spec))` |
| `[RetrievalV1]` | retrieval class | same |
| `[CompositeV1]` | composite class | same |

`extension` in Memory/Retrieval/Composite is the digest of the typed-roots descriptor `K2-TR-v1`; a class names it, so a later
`K2-TR-v2` is a new class id (a descriptor's meaning is append-only, ADR-0172). A kind the binary does not know does not decode
(`Malformed`); a known kind whose extension is not Active is `KERNEL_NOT_ACTIVE [typed-roots]`; a kind the Active extension does not
list is `KERNEL_EXTENSION_REQUIRED [<kind>]`; a Memory slot over a `Hist` state is `KERNEL_EXTENSION_REQUIRED [memory-hist-carry]`.

**Where it lives.** On the kernel route, as a class registration: that is where a computation is registered, executed and judged.
The V2 class registration (onboarding tags 104–109) is unchanged: `KernelBound` (106) binds a V2 class to a kernel class *by id*, and a
typed-root class id is a kernel class id like any other; the onboarding facts a typed class will need (artifact binding of `M0`, a
snapshot availability binding) are listed in §10.

### 1.1 `Weights` alone reproduces today byte for byte

`RegisterClass{spec: [WeightsV1{d, p, plan, pc}], mode}` is applied by **the same function** as tag 1 (`PanelLicensed`) / tag 13
(OPV): same refusals, same `ClassRowV1`, same `classes` row bytes, same OPV row, same `ClassRegistered` event. So: same class id, same
program/artifact/plan roots, same ledger root (the spec tag adds no row). Proven by `typed_roots::weights_only_*` (kernel crate) and
`r4x_weights_only_spec_is_byte_for_byte_the_legacy_registration` (real node). Tags 1/13 are untouched.

## 2. `Memory`

### 2.1 What is bound

* **The update rule** is the Weights root's TIR program (K2-TIR-v1/v2, Active). A **memory slot** pairs a param instance `(j, l)` (the
  pre-state *read* into a step) with a `Fixed` state instance `(s, l')` (the post-state *written* by its `StateWrite`). Checked at
  registration: the param is declared, the state is `Fixed` and has exactly one `StateWrite` at `(s, l')`, dtype and shape of the param
  equal the `StateWrite`'s declared output; slots strictly ascending, at most 16; `1 ≤ max_steps ≤ 64`.
* **The initial memory** `M0` is the Weights root's commitments of the slot params; `initial_root = memory_root_v1(slots, those
  commitments)` is stated and checked. `M0` is part of the attested artifact (its root is the Weights commitments' root).
* `line` (a free 64-byte label, bound in the id): the same rule and `M0` may run several independent memory lines as several classes.
* Retention: the pre-state of every claim is that claim's DA obligation for the claim's whole liability horizon (§2.4) — the ledger's
  `liability_daa`, not a class parameter, so no class can declare a shorter one.

A program expresses "start from the memory" with ordinary primitives: `m_in = Select(Compare(Input(POS), 0, Eq), Param(M), State(m))`,
`m' = rule(m_in, x)`, `StateWrite(m, m')`. **Test-time parameter updates are the same thing**: the slot param is a weight slice that
the step reads as a matrix (`MatMul(W_in, x)`) and the rule is an integer optimiser step (`W' = Clamp(W_in − (g ≫ s))`, `g` an outer
product by `MatMul`). Nothing in consensus trains; a step is one forward evaluation that includes the update.

### 2.2 Steps, overlays and the per-step roots

A memory job is `MemoryJobV1 { class, pre_root, chunks: Vec<Vec<u32>>, nonce }`: step `i` absorbs chunk `i` and delivers one token
(greedy over the logits of the chunk's last position). **Step `i` is exactly a K2 claim** of the sub-job
`KernelJobV1 { class, prompt: chunk_i, max_new_tokens: 1, Greedy, nonce: H(job, i) }` over the rule program, with one difference: its
param commitments are the **overlay**

```text
overlay_0     = base commitments with every slot param ← the line head's slot commitments      (the pre-state of the job)
overlay_{i+1} = base commitments with slot k's param   ← C(step i's StateWrite of slot k at its last position)
post-state    = the slot commitments of step S−1's last StateWrites
memory_root_v1(slots, c) = H("misaka-palw/spec/memory-root/v1"; n; for each slot: j, l, s, l', c_k)
step_roots    = [root(overlay_0 slots), root(overlay_1 slots), …, root(post)]          (S + 1 boundary roots)
```

The claim carries `step_roots`, every step's evidence object and trace commitments, and **its post-state opened** (`post_state`: one
tensor per slot); inclusion recomputes every overlay and every boundary root **from the committed traces** (never from a statement),
requires `step_roots[0] = job.pre_root = line head`, checks every carried post-state tensor against the slot param's declared dtype and
shape and against the commitment the traces derive (`check_post_state_v1`), and runs each step's binding and structure checks
(`binding_fault_v1`, the header with `artifact_root = overlay_i.root()`, `FreshVerifierV1::structure`) — so a fabricated boundary, a
borrowed trace, a step over another pre-state or a post-state that is not the committed write is refused at inclusion, objectively.

### 2.3 The court (localisation to one step)

`SpecFaultV1::MemoryStep { step, proof }` is built and judged by `FreshVerifierV1` over step `step`'s record (`claim_id = H(claim,
step)`, the rule program, the plan, step `step`'s evidence and commitments, **overlay_step** as the param commitments, the chunk as
tokens). The court reads only: the claim row (on chain), the class row, and the filing (which opens the slot params against the overlay
commitments like any param). `MemoryDecode { step, logits }` is `verify_decode_fault_v1` over the same step. **Completeness**: if any
committed value of any step is wrong, take the first step `i` that has one; its overlay is the honest pre-state (every earlier step is
right, and overlay_0 is the head), so the K2 court on step `i` convicts exactly as it would a plain claim. **Soundness**: an honest
step's record is a correct K2 claim; every court dismisses.

### 2.4 DA and the default

Demand stages of a memory claim: stage `0` = the steps' positions in one global index (step `i` covers `[off_i, off_i + |chunk_i|)`),
stage `0x40` position `0` = **the pre-state** (one "position" whose values are the slot tensors, classified against the line head's slot
commitments by `classify_position_response_v1`). Step `i ≥ 1`'s pre-state is step `i−1`'s committed `StateWrite` at its last
position — demandable at stage 0, and the claim's own DA: withholding it is the producer's availability default (`ProducerDefault`,
never a conviction). Step 0's pre-state (the line head) is **already public**: the carried post-state of the claim that advanced the
line (`SpecClaimBodyV1::pre_source`), or the registered `M0` in the attested artifact (§2.5) — so an outsider never needs to demand it.
The `0x40` obligation stays as RFC-0004 §II.2 states it (withheld → default), though anyone can answer it from the chain. The outsider
(§7) reports exactly which positions it needs.

### 2.5 Memory carried across jobs (the chain-tracked state root)

`MemoryLineV1 { head: Vec<Digest> (slot commitments), head_root, head_source: Option<claim>, advances: Vec<AdvanceV1 { claim, pre,
pre_source, post, until_daa }> }`, one per memory class (table 24). Rules:

* `PostJob` of a memory job requires `pre_root = head_root`; `CommitClaim` requires it again (a claim never commits over a stale head).
* At a claim's **Final**: if `head_root` is still its `step_roots[0]`, the head becomes its post-state, `head_source` becomes the claim
  (whose carried `post_state` is the head's tensors) and an advance is recorded until the claim's liability horizon ends; otherwise the
  claim is Final and paid but **superseded** (it computed correctly from a state that was the head when it committed; it does not move
  the line).
* A **post-Final conviction** of an advancing claim rolls the head back to that claim's pre-state — `head_source` back to that state's
  source — and drops every later advance (their claims are not convicted — they are superseded). A pre-Final conviction moved nothing.
* Advances past their horizon are pruned (a conviction can no longer reach them); the head and its source are never pruned (claims
  rows are kept).

**The head is public by construction.** `KernelLedgerV1::memory_head_tensors_v1(class, artifact)` returns the head's tensors from the
rows (`head_source`'s carried post-state) or, for `M0`, the public artifact — each checked against the head's commitments. Any bond
produces the next job from that alone; the E2E's second-job producers are other cards than the first job's and read only the node's
API. Without it the head's tensors would be held by the producer that advanced the line last: only it could continue the line (a
private monopoly over a public class), and if it went away the line would stall forever. The price is `M` bytes per memory claim on
chain (bounded at registration: §2.6).

RFC-0004 §II.3: the head is the in-job analogue of a line head; promoting a memory state through §§4–8 is a candidate over this class
(a later lane), not a new mechanism.

### 2.6 Bounds (memory)

`base` = `public_prosecution_complete_v1` of the rule program (per step); `M` = Σ slot bytes; `S` = `max_steps`.

| bound | value |
| --- | --- |
| public bytes per prosecution | `base.max_public_bytes + M` (one step's material, its pre-state, the artifact) |
| opening / filing / court work | `base` (a step fault is a kernel fault) |
| response | `max(base.max_response_bytes, M + slots · 128 + 128)` |
| rounds | 2 (one demand round, one filing) |
| verifier RAM | `base.max_verifier_ram + M` |
| retained per claim | `S · (base.max_retained_state + evidence) + 64 · (S + 1) + 64 · slots + M` (the carried post-state; the commit carrier must fit it) |
| sessions per claim | `S · plan.max_positions + 1` (every position and the pre-state demandable at once) ≤ `max_sessions_per_claim` |
| per prosecution (K2S sense) | the faulty step's positions + 1 (with a v4 rule program: 2 + 1) |

## 3. `Retrieval`

### 3.1 What is bound

```text
item           = { key: [i32; D], payload: [u32; ≤ P] }
key_digest     = H(".../retrieval-key/v1"; D, key LE)       payload_digest = H(".../retrieval-payload/v1"; len, payload LE)
leaf(id)       = H(".../retrieval-leaf/v1"; id u64, key_digest, payload_digest)
snapshot root  = H(".../retrieval-snapshot/v1"; N, D, P, B, merkle(leaves in id order))     (binary; an unpaired last node carried up)
index          = Flat (the snapshot's own id order; index commitment = H(".../retrieval-index/v1"; Flat, snapshot root, rule))
rule           = TopKCountingV1 { k, score_bits SB }   (1 ≤ k ≤ 64, 8 ≤ SB ≤ 62)
```

`B` = items per DA slice (bound in the root, so slices are a public function of the class). The snapshot root must be attested public
(the same consumer fact the artifact uses; GAP: an onboarding snapshot binding, §10).

**The rule** (RFC-0002 FR-09's counting threshold, `lower/dsa.rs`, generic-frontend §9.6, reused exactly): `s(id) = clamp(Σ q_j ·
key_j, −(2^SB − 1), 2^SB − 1)` in exact integers; `κ(id) = (s + 2^SB) · 2^b + (2^b − 1 − id)` with `b = ⌈log2 N⌉` (κ distinct: ties to
the lowest id); the result is the `min(k, N)` items of largest κ, in descending κ. No float, no approximate index, no randomness.

### 3.2 Job, claim, inclusion

`RetrievalJobV1 { class, query: [i32; D], nonce }`; the claim delivers `result: [{ id, score, key_digest, payload_digest }]`.
Inclusion: `len = min(k, N)`, `id < N`, `|score| < 2^SB`, strictly descending κ computed from the claimed `(score, id)` (so distinct).

### 3.3 Courts (per item)

* `WrongItem { index, item, path }` — the item opened at the claimed id against the snapshot root. Convicts iff the opening
  authenticates and `(key_digest, payload_digest) ≠` the claimed ones (**a wrong opening**) or `s(query, key) ≠` the claimed score
  (**a wrong score**). An opening that does not authenticate is dismissed (`NotAuthentic`).
* `MissedBetter { id, item, path }` — an authenticated item **not** in the result with `κ(id, s(query, key)) > κ(last entry)`.

**Completeness**: if the result is not the rule's output, either some entry disagrees with the snapshot at its id (`WrongItem`
convicts) or every entry is the snapshot's item with its true score — then, the claimed set being a strictly κ-ordered set of `min(k, N)`
true items that is not the top-`min(k,N)`, some excluded item has κ above the last entry (`MissedBetter` convicts). **Soundness**: the
honest result has every entry equal to the snapshot's item and no excluded item above its last κ.

### 3.4 DA

Stage `0x80 + s` (s = 0 for a plain retrieval class), position `t` = slice `[t·B, min((t+1)·B, N))`. The response is the items with
their paths; classified `malformed` / `fake_opening` / `oversized`; served → stored as a `ServedPositionV1` (key and payload as
tensors). A withheld slice is the claim producer's default — the producer needed the whole snapshot to compute the result, so it is
always able to serve it.

### 3.5 Bounds (retrieval)

`item = 8 + 4D + 4P + 128`, `path = 64 · ⌈log2 N⌉`. Opening `item + path`; filing `item + path + 64 KiB`; court work `D` MACs
`+ ⌈log2 N⌉ + k` hashes/compares; response `B · (item + path) + 128` (≤ carrier: checked); sessions per claim `⌈N/B⌉ ≤
max_sessions_per_claim` (so `N ≤ 1,024 · B`); **per prosecution 1 session** (the slice holding the better or the wrong item); retained
per claim `k · 144 + 64`. Detection (finding a missed item) scans the snapshot, `N · item` bytes — reported as the class's
`claim_material_bytes`, the outsider's choice, as K2S treats a whole-claim check.

## 4. `Composite`

```text
CompositeStageV1 { component: class id, input: StageInputV1 }
  StageInputV1::Tokens { sources: Vec<TokenSourceV1>, max_new_tokens }    (component: a registered Weights class — a model)
  StageInputV1::Query(QuerySourceV1)                                       (component: a registered Retrieval class — a tool)
  TokenSourceV1: JobPrompt | StagePayloads{stage} | StageGenerated{stage}
  QuerySourceV1: JobQuery | StageLogits{stage}
```

2 ≤ stages ≤ 8; a source names an earlier stage of the right kind; a Memory component is `KERNEL_EXTENSION_REQUIRED
[composite-memory]` (the line would need a pipeline-level advance rule). **A tool stage is only ever a verified kernel**: a stage's
component is a registered class whose own spec the chain already judges (a Retrieval class, or a TIR program class); there is no
external API, no code upload, and nothing a stage reads that is not on chain or committed.

**Per-stage localisation.** Each stage is judged by its component's own courts on a stage record: a model stage is a K2 claim of the
sub-job `KernelJobV1 { component, prompt = the concatenated sources, max_new_tokens, Greedy, H(job, s) }`; a retrieval stage is a
retrieval claim with the stage's query. `SpecFaultV1::Stage { stage, fault }` names the stage; a fault filed against an honest stage
is dismissed, so a conviction always names the stage that lied.

**Edges** (RFC-0003's edge relation, extended): an edge has no arithmetic. When both of its sides are on chain (the job's prompt and
query, a retrieval stage's payload tokens — carried in the claim and checked against the result's `payload_digest` — and a model
stage's delivered tokens) it is recomputed **at inclusion**: a model stage's evidence must commit exactly that input (`binding_fault_v1`),
so an edge fault cannot be committed at all. When the source is a committed-only value (`StageLogits`: the query is the committed logits
of a model stage's last position, carried in the claim as the stage's query) the **edge court** `SpecFaultV1::Edge { stage, logits }`
opens that tensor against the upstream stage's commitments and convicts iff it differs from the carried query.

Bounds: Σ over stages for public bytes, RAM, retained state and sessions; max over stages (and the edge filing: one logits tensor) for
opening, filing, response and court work; per prosecution: the lying stage's.

## 5. Ledger integration

* Objects: `KernelRouteObjectV1::Spec { object: SpecObjectV1 }` (inner kind 19, one ceiling = `MAX_COMMIT_CLAIM_BYTES_V1`);
  `authorize` names the producer for `CommitClaim`.
* Claims: `ClaimBodyV1::Spec(SpecClaimBodyV1)` (variant 3) in the ordinary `claims` table, so lifecycle, OPV window and reservation,
  seal-then-reveal, one-claim-per-job, demands, defaults, Final, liability and settlements are the route's own. `SealClaim` accepts a
  spec job.
* Proofs: `ProsecutionV1::Spec(Vec<u8>)` (variant 4; borsh `SpecFaultV1`), adjudicated by `adjudicate` from ledger state only,
  charged the class's declared worst court work.
* Demands: `ClaimBodyV1::position` serves stage 0 (steps / model-stage positions) and `0x40` (pre-state); stage `0x80 + s` (snapshot
  slices) is demandable by index and classified by the spec code in `respond`.
* Tables (each in the ledger root only when non-empty, so every existing root is unchanged): **22** spec classes (`class →
  ComputationSpecV1` record; derived rows rebuilt like `ClassRowV1`), **23** spec jobs, **24** memory lines.
* Hooks: the tick's Final transition advances a memory line; `convict` after Final rolls it back.
* **OPV only**: typed kinds register only under `OptimisticPublicVerification` (the OPV fence, the network admission, the class's
  carrier fit and censorship economics, exactly as tag 13). A Panel seat cannot cover a multi-step or snapshot claim with today's
  receipts, and G14 is OPV's premise (K2S makes the same choice for v4). `[WeightsV1]` keeps both modes (§1.1).

## 6. OPV eligibility per kind (OPV-BOOT's derived E1–E7)

| | Memory | Retrieval | Composite |
| --- | --- | --- | --- |
| E1 kernel Active | the rule program's K2-TIR descriptor and `K2-TR-v1` | `K2-TR-v1` | `K2-TR-v1` and every component's |
| E2 conformance of the statement | the rule program + plan + the artifact root over base weights **and `M0`** | none sampled: every relation is an exact integer relation — the complete-check case (ε = 0, no beacon) once the conformance contract has a snapshot statement (GAP) | each component's; the composite adds only exact edges |
| E3 G14-complete | the derived gate of §2.6 (every step relation has a public court; the pre-state has a demand/default) | §3.5 | every stage's, plus the edge court |
| E4 DA binding | the artifact binding of base + `M0`; each later pre-state is the claim's own DA obligation | the snapshot's attestation (GAP: onboarding snapshot binding) | each component's |
| E5 bounds/economics | §2.6 within carriers; censorship cost > gain | §3.5 | §4 |
| E6 policy bits | the rule program's (Freivalds) | `Complete` | the weakest stage's |
| E7 deny-list | class id | class id | class id **and** every component id |

## 7. The outsider

`SpecOutsiderV1` (same inputs as `OutsiderV1`: the ledger rebuilt from served rows, a public DA directory, the public artifact, a salt):
memory — the step-0 pre-state from the chain (served `0x40`, else the source claim's carried post-state, else `M0` from the artifact),
every step's record rebuilt with its overlay, missing positions reported as demands, else the first decode or kernel fault as a
`MemoryStep`/`MemoryDecode` filing; retrieval — every entry re-opened (`WrongItem`), then a full scan of
the snapshot for a better excluded item (`MissedBetter`), missing slices reported as demands; composite — the edge, then each stage.

## 8. G14, per criterion

(1) every object is filed by any bond (no seat, no operator); (2) a fresh node builds the verifier from the class row, the claim row and
served/public material only; (3) every byte is authenticated against the on-chain commitments — trace commitments, the overlay derived
from them, the line head, the registered artifact and snapshot roots; (4) a fault localises to one step / one item / one stage with the
bounds of §§2.6, 3.5, 4; (5) withheld → default at the deadline, lie → conviction, honest → dismissal; (6) direct proofs are never
pre-empted; (7) the E2E runs the real node path (§11). Forbidden things stay out by construction: no API, no unverified code, no
nondeterministic retrieval (κ is a total order), no state an outsider cannot obtain (every pre-state is a committed value or the claim's
DA obligation; every snapshot item is DA of the claim).

## 9. The fence (decided by the Lead, 2026-10-08)

`K2-TR-v1` (`kernel_id 3`, version 1; families none — it is an extension, not a TIR kernel; identity = its semantics digest) must be
**Active in the route ledger's schedule**. **`palw_typed_roots_v1: Option<ForkActivation>`** (`crate::palw_typed_roots_v1`) follows
`palw_probabilistic_constraints_v1` exactly: hashed Some-only into `consensus_params_id` and `consensus_schedule_id` with the `never()`
collapse, visited by `for_each_fence`, an arm in the `fork_id_v1` probe, `None` on every preset, refused by `validate_palw_v2` at every
height. The processor resolves it once (`palw_kernel_typed_roots_at`), drops a `Spec` object by name at the acceptance walk below it,
and hands its activation to the fold (`PalwKernelRouteExtrasV1::typed_roots`), which records it in the route header
(`PalwKernelRouteHeaderV1::typed_roots`) and schedules `K2-TR-v1` Active from it (`palw_kernel_route_template_typed_v1`). With the fence
absent the template, the schedule, `config_root` and every root are byte for byte the historical ones
(`the_unarmed_typed_roots_fence_leaves_the_schedule_config_root_and_root_unchanged`). It does not ride the K2 fence: the full-activation
release arms it at the same height as the others, as its own decision. The E2E arms it through the harness's `Config` seam.

**Prerequisites, by name** (checked before the blanket refusal, so they hold once it is lifted): `validate_palw_typed_roots_v1` requires
`palw_probabilistic_constraints_v1` (the route the typed kinds ride) and `palw_panel_free_v1` (every typed kind is OPV-only) armed at
or below its height (`the_fence_is_dormant_everywhere_refused_when_armed_and_hashed_some_only`).

## 10. GAPs (not done here)

* Onboarding facts for typed classes: an artifact binding that names `M0` (today the attested set covers it as part of the Weights
  root), a snapshot availability binding and its conformance statement (RFC-0007 Part VI) — OB lanes. **The snapshot binding must also
  attest that every leaf is a well-formed item** (key of length `D`, payload ≤ `P`): an item that is not cannot be opened by any court
  (`authentic` refuses its shape), so a producer could state anything for its id unconvicted; and an honest producer could not serve
  its slice (`malformed`).
* Memory carried through a `Hist` state; memory inside a composite; IVF/HNSW-style indexes (each a versioned kernel extension).
* Panel-licensed typed classes (a receipt scope per step / item).
* RFC-0011 §18 census: a repository that is a complete computation of a supported kind counts in `D_complete` — HFX/COV own the census;
  this lane exposes the predicate (`supported_kinds`).
* ~~GAP-5 escrow (G14-R4)~~ — **closed at the G14-R4 merge (2026-10-09)**: `post_spec_job` checks `job_escrow_affordable` before the
  charge and `apply_spec` opens the poster's escrow after `JobPosted`, exactly as `PostJob`; a typed claim's Final is paid out of it.
* ~~Memory-line liveness~~ — **closed (R4X successor, 2026-10-09)**: the claim carries its post-state opened and the line records its
  source (§2.5), so the head is always public; no producer can hold a line hostage and no line stalls on unpublished tensors.
* Withholding after Final (a post-Final default on a memory claim) forfeits the reservation but does not roll the line back: the
  computation is not proven wrong, and the head's tensors are on chain regardless.
* ~~A memory job stranded by a moved head~~ — **closed at the G14-R4 merge**: it can never be claimed (a claim never commits over a
  stale head), so no claim holds it and no seal of it outlives the seal TTL; the ordinary idle-escrow rule returns its escrow after
  `job_escrow_ttl_daa` (G14-R4's `release_idle_job_escrows`).
* Typed claims are OPV-only, so past `palw_panel_free_v1` they reveal **salted** (OPV-BOOT GAP-B1a): `CommitClaimSalted` carrying
  `SaltedCommitV1::Spec` (inner kind 20; it needs `palw_typed_roots_v1` too, at the node's gate and in the ledger's schedule).

## 11. Evidence (RFC-0004 §II.4)

| §II.4 | Test |
| --- | --- |
| 1 wire form, Weights byte for byte | kernel `typed_roots::weights_only_*`; node `r4x_weights_only_spec_is_byte_for_byte_the_legacy_registration` |
| 2 Memory E2E | node `r4x_memory_*`: register → job 1 → Final → head advanced (its tensors served by the node) → job 2 over job 1's post-state, produced by OTHER cards from the node's API alone → a lie in one step convicted by an outsider (the fault names the step) → honest claim → head carried again; withheld pre-state (step 1's, the claim's DA; and the `0x40` obligation) → default; kernel `typed_roots::memory_*` also: a carried post-state that is not the committed write refused at inclusion, the rollback restoring `M0` as the head's source |
| 3 Retrieval E2E | node `r4x_retrieval_*`: wrong item convicted, missed better item convicted, withheld slice → default |
| 4 Composite | node `r4x_composite_*`: one verified tool stage (retrieval) feeding a model stage; a lie in the tool stage convicted at stage 0, a filing against the honest stage dismissed |
| 5 Bounds | kernel `typed_roots::bounds_*` per kind (values, carrier fit, refusals past each ceiling) |

## 12. Implementation map

| Piece | Where |
| --- | --- |
| wire, kinds, class id, bounds, typed root, `K2-TR-v1` | `misaka-palw-kernel/src/spec/mod.rs` |
| memory (slots, overlays, steps, line) | `spec/memory.rs` |
| retrieval (snapshot, rule, courts, slices) | `spec/retrieval.rs` |
| composite (stages, edges, stage records) | `spec/composite.rs` |
| the ledger's typed rules (child of `ledger`) | `spec/ledger_impl.rs`; hooks in `ledger.rs`, tables in `rows.rs`, root in `state.rs` |
| the outsider | `spec/outsider.rs` |
| an honest (or lying) producer | `spec/produce.rs` |
| fixtures (`memory_v1`, `memory_ttt_v1`) | `misaka-palw-tir-sketch/src/fixture.rs` |
| fence | `consensus/core/src/palw_typed_roots_v1.rs`, `config/params.rs`, `fork_id_v1.rs`, `processor.rs`, `palw_kernel_route_{v1,fold_v1}.rs` |
| tests | `misaka-palw-kernel/tests/typed_roots.rs` (ledger), `consensus/.../tests/r4x_typed_roots_e2e.rs` (real node) |

## 13. The design premise, condition by condition (`docs/PRINCIPLES.md` §6)

A typed class earns a reward or consensus work weight only when all seven conditions of the premise hold. Each typed kind adds no way
of judging arithmetic of its own (§0), so most of what remains is the kernel route's and OPV's, shared with every K2 class. The kinds
below are what each condition means for a typed class, what this branch shows, and what is left — by kind of blocker (readiness
matrix: CODE / DESIGN / POLICY / EXTERNAL).

| §6 | Memory | Retrieval | Composite | Left (kind, owner) |
| --- | --- | --- | --- | --- |
| 1 coverage of every rewarded relation | every TIR relation of every step: the rule program's K2 plan; the memory edges (overlays, boundary roots, the carried post-state) are recomputed at inclusion from committed traces; the token by the decode court | exact: the opening (Merkle), the score (dot + clamp), the order (κ at inclusion), completeness (`MissedBetter`) | each stage's own; token edges at inclusion; the logits→query edge court | — (met by construction, given the K2 plan's coverage) |
| 2 approved probabilistic soundness, grinding resistance | the rule program's plan (Freivalds bits); outsiders check with their own salt after commit, so a producer cannot grind an outsider's draw; `S` steps multiply work, not the per-step error | ε = 0: no sampling, nothing to grind | the weakest stage's | POLICY: the challenge policy's security level (interim 2 bits, target 128). CODE: sealed-source beacon v3 (OPVB / G14-R4) for any sampled approval. EXTERNAL: soundness review of the composition |
| 3 public, authenticated material | base weights and `M0`: the attested artifact; step 0's pre-state: **on chain** (§2.5); step `i ≥ 1`'s pre-states and every committed value: the claim's DA (demand → serve → default) | the snapshot: attested public; slices: the claim's DA | components' artifacts and snapshots; stage values: DA | CODE/EXTERNAL (onboarding, RFC-0014 §16): an on-chain availability fact for artifacts and snapshots — today the route's attested list is test-only; the snapshot binding must attest leaf well-formedness (§10) |
| 4 bounded localisation and objective adjudication by one outsider | one step (§2.3); the fresh outsider rebuilds every step from the rows, the chain's head and DA | one item (§3.3), one session per prosecution | one stage or edge (§4) | — (shown: kernel `typed_roots::*`, node `r4x_*`, each with a fresh outsider over rows a second node replays) |
| 5 collectable collateral consistent with the maximum gain | a claim's gain includes the line's future: a lie that reaches Final moves the head every later job computes from; rolled back by a post-Final conviction within the liability horizon, permanent after it | the gain of a promoted / suppressed item is external to the reward | one reservation for the whole pipeline | POLICY: the OPV reservation (interim 1,000 BILI) from max gain ÷ detection probability; DESIGN: a memory class's **line value at risk** belongs in that gain (a per-class declared bound, or a liability horizon that scales with it) |
| 6 resources, cost and incentive for an honest verifier in time | a whole-claim check is `S` step checks (parallel); the window must fit them | detecting a missed item scans the snapshot: `N · item` bytes fetched once per class (amortised), `N · D` MACs per claim (`retrieval_claim_material_bytes_v1`) | Σ over stages | EXTERNAL (MEAS): `T_check` per kind on real sizes; POLICY: a window ≥ `T_beacon + T_fetch + T_check + T_localize + T_file + T_margin` per class, the accuser reward |
| 7 dispute, DA/default, Final and reorg consistency | the route's lifecycle; Final advances the line, a post-Final conviction rolls it back, a superseded claim never moves it; the line is ledger rows, folded per block (replayed by a second node) | the route's | the route's | CODE (FINX): rule E (ADR-0178) before the release; (closed at the G14-R4 merge: GAP-5 escrow for spec jobs; a stranded memory job's escrow returned by the idle-escrow rule) |

**RFC-0004 §II.4 item 6 (census).** `SUPPORTED_KINDS_V1` is the predicate a census reads: a repository that is a complete computation
of a supported kind (for example a retriever + index + generator repository as a Composite) belongs in `D_complete` and counts as
registered once registered as that kind. Classifying Hugging Face repositories into kinds is the census's (HFX / COV): CODE, not here.
