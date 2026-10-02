# MISAKA PALW側PoW 詳細設計書

**文書ID:** MISAKA-PALW-POW-0001  
**版:** v0.1  
**作成日:** 2026-08-12  
**状態:** Draft / Shadow-First / Mainnet未承認  
**対象:** `palw_execution_algo_id = 2`、full-logits-bound PALW、VPS canonical worker、PALW Certificate、ALGO-3 block binding  
**関連文書:**

- [`PALW full-logits trace scheme v2`](palw-full-logits-trace-v2-design.md)
- [`MISAKA PALW VPS Canonical Worker 経路設計書 v0.1`](misaka-palw-vps-canonical-worker-design-v0.1-ja.md)
- [`MISAKA PALW セキュアOTA設計書 v0.1`](misaka-palw-ota-secure-update-design-v0.1-ja.md)
- PALW v8 Single-Block Dual-Resource Consensus
- MISAKA Whitepaper V4 Compute Set Registry

> [!IMPORTANT]
> 本書中の`palw_execution_algo_id = 2`はPALW内部namespaceであり、header-levelの
> `pow_algo_id = 2`ではない。historical Argon2id IDを再利用してはならない。安全モデルと
> activation gateはfull-logits基礎設計書を上位規範とし、現時点の実装・運用許可はdevnet、
> shadow mode、consensus-visible zero-creditまでである。本書のP1/P2、reward、mandatory gateは
> 将来候補であり、別ADRと全gate通過なしに有効化してはならない。

---

## 0. 設計判断

PALW側PoWは、**別のPALWブロック、別のtransaction history、別のfork-choice weightを作らない**。

正規構造は次とする。

```text
ALGO-3 Hash-PoW
  ├─ block ordering
  ├─ GHOSTDAG fork choice
  ├─ ordinary double-spend resistance
  └─ permanent hash safety floor

PALW Work Proof
  ├─ proposal-bound canonical LLM execution
  ├─ fixed compute quantum
  ├─ A/B/C independent replay commitment
  ├─ bonded Certificate
  ├─ post-certificate audit
  └─ delayed PALW reward / future mandatory gate
```

### 0.1 Namespace

```text
pow_header_algo_id       = ALGO-3の既存ID
palw_execution_algo_id   = 2
palw_trace_scheme_id     = Hash64("misaka-palw/full-logits-trace/v2")
palw_proof_class_id      = Hash64("misaka-palw/audited-compute-certificate/v1")
```

`palw_execution_algo_id = 2`をheader-level PoW IDとして再利用してはならない。

### 0.2 PoWの二つの意味を分離する

PALWでは「PoW」という語が二種類の処理を指し得る。

1. **leader-election PoW**  
   nonceを探索し、fork choiceの累積workを作る。これはALGO-3だけが担当する。

2. **resource-consumption proof**  
   固定されたLLM計算を実行したというslashable claimを作る。これはPALW Certificateが担当する。

PALW trace rootをhash target以下にするnonce探索は採用しない。PALW resultはjobごとに決定的な一つの値であり、通常のhash lotteryと同じ難易度調整を適用すると、seed・prompt・proposalのgrinding空間を作るためである。

### 0.3 初期運用profile

現時点で安全に許可する順序は次のとおり。

| Profile | Block validity | PALW reward | Fork-choice PALW weight | 状態 |
|---|---|---:|---:|---|
| P0 Shadow | Hash-PoWのみ | 0 | 0 | 実装・検証用 |
| P1 Reward-Attached | Hash-PoW、PALW添付は任意 | delayed / capped | 0 | 条件付きtestnet |
| P2 Required Dual-Resource | Hash-PoW AND PALW Certificate | delayed | 0 | 別hard fork、未承認 |
| P3 Mixed Compute Weight | Hash + PALW workをfork choiceへ加算 | positive | positive | 採用しない |

**P3は廃止する。** 別PALW lane、別DAA、別blue workは、cross-lane root差、PALW-only branch、ticket重複、zero-work blue score、二つのvalidity ruleを再導入する。

P2へ進むとtotal replica outageがchain livenessを止めるため、P0/P1と同じactivationとして扱わない。P2は明示的な新RulesetIDとhard-fork fenceを必要とする。

---

## 1. 目的

本設計の目的は次のとおり。

1. 出力文字列ではなくfull logits sequenceへ計算claimをbindする。
2. prompt、seed、token budget、runtimeをworkerが選べないようにする。
3. PALW work量をbackend telemetryではなくcanonical scheduleから再計算する。
4. 同じjobを複数bonded authorityが独立再実行できるようにする。
5. Certificateを一つのproposal、parent set、transaction root、post-state root、coinbase templateへbindする。
6. PALWが壊れてもALGO-3 fork choiceを壊さない。
7. 不正発見後にledgerを巻き戻さず、PALW rewardとbondだけを処理する。
8. current logits-only runtimeと、将来のcanonical Compute VM/TraceVMを段階分離する。
9. 10 BPS相当のBlockDAG上でもpipelineで処理可能か測定できる仕様にする。
10. positive valueへ進む前のStopShip条件を機械的に定義する。

---

## 2. 非目標

本設計は次を主張しない。

- LLM出力が事実として正しいこと
- LLM出力が社会的に有用であること
- A/B/Cが物理的に別の人・別の会社であること
- trace root単体が計算実行を暗号学的に証明すること
- bondが非経済的攻撃者を完全に止めること
- PALWがHash majority attackを単独で防ぐこと
- floating-point logits profileを永続的mainnet標準にすること
- current workerだけでpermissionless interval fraud proofが完成していること

PALWは **Proof of Audited LLM Work** であり、現段階では「計算を完全に事前排除するproof」ではなく、「複数のslashable claim、将来random audit、objective evidenceを組み合わせたwork certification」である。

---

## 3. 基本validity predicate

### 3.1 P0 / P1

```text
ValidBlock(B) =
  ValidParents(B)
  AND ValidTransactions(B)
  AND ValidAlgo3PoW(B)
  AND ValidOptionalPalwAttachmentOrNone(B)
```

PALW添付が存在する場合は完全検証する。無い場合もbase blockは有効である。

P1ではPALW添付が正しい場合だけPALW service rewardを作る。fork choiceは変えない。

### 3.2 P2 Required Dual-Resource

```text
ValidBlock(B) =
  ValidParents(B)
  AND ValidTransactions(B)
  AND ValidAlgo3PoW(B)
  AND ValidPalwCertificate(B)
  AND ExactProposalBinding(B)
```

Hash-PoWとPALW Certificateは代替ではない。どちらか一方だけのblockは無効である。

### 3.3 Fork choice

全profileで次を維持する。

```text
ForkChoiceWork(B) = ExistingAlgo3CumulativeWork(B)
```

PALWの`canonical_compute_units`、Certificate数、bond量、audit結果をGHOSTDAG blue workへ加算しない。

理由:

- PALW false certificateがfork-choice workを購入するのを防ぐ
- compute profile変更をfork-choice theoremから分離する
- historical workをruntime実装へ依存させない
- PALW outage時の安全性とlivenessを評価しやすくする

---

## 4. Actorと責務

| Actor | 記号 | 責務 |
|---|---|---|
| Hash miner / proposer | H | proposal固定、control object、Certificate取得後のnonce search |
| Primary worker | A | canonical job実行、commit/open、signature |
| Replica worker | B | future-selected independent replay |
| Replica worker | C | future-selected independent replay |
| Post-audit worker | D | block後のfull replay / checkpoint challenge |
| Full node | N | proposal、selection、Certificate、PoW、state transition検証 |
| Reporter | R | objective fraud evidence提出 |
| Governance | G | Compute Set、ParameterSet、profile activation |

### 4.1 Role separation

A/B/Cは最低限次を満たす。

- distinct provider credential
- distinct bond capacity unit
- distinct assignment authorization
- same activated Compute Set
- same ShapeID
- same canonical job
- same execution seed
- same decode rule

`operator_pool_id`が同一のproviderは同じCertificate内で組み合わせない。operator identityは完全なSybil proofではないため、worst-case security analysisはA/B/C collusionを前提とする。

### 4.2 Worker鍵

```text
palw-worker: validator keyなし
palw-agent:  validator keyなし
kaspad:      receipt/certificate preimage構築
remote signer/HSM: ML-DSA-87 signatureのみ
```

worker compromiseからvalidator、bond owner、governance keyを分離する。

---

## 5. Proposal pipeline

### 5.1 なぜproposalを先に固定するか

PALW resultを見た後にtransaction、parent、coinbaseを変更できると、同じcompute claimを別blockへ移植できる。

したがってhash minerは、LLM execution前に次を固定する。

```rust
pub struct PalwProposalCoreV1 {
    pub version: u16,
    pub network_id: Vec<u8>,
    pub ruleset_id: Hash64,
    pub proposer_id: Hash64,
    pub proposal_sequence: u64,

    pub parents_root: Hash64,
    pub transaction_root: Hash64,
    pub post_state_root: Hash64,
    pub coinbase_template_root: Hash64,

    pub timestamp_slot: u64,
    pub algo3_target_id: Hash64,

    pub compute_set_id: Hash64,
    pub shape_id: Hash64,
    pub input_root: Hash64,
    pub primary_authority: Hash64,

    pub proposal_expiry_daa: u64,
    pub body_da_root: Hash64,
}
```

### 5.2 Proposal ID

```text
ProposalID = H_k(
  "misaka/palw/block-proposal/v1",
  canonical_borsh(ProposalCore)
)
```

signature bytes、relay順序、arrival time、local mempool orderは`ProposalID`へ入れない。

### 5.3 ProposalSequence

`proposal_sequence`はproposerごとのchain-assigned monotonic sequenceとする。

- carrier acceptanceで消費
- abortしても再利用不可
- B/C selectionの入力
- tx root変更によるselection grindingを抑制
- duplicate proposalをnullifierで拒否

### 5.4 Branch-local

proposal、assignment、reservation、commit/open、Certificate、audit、reward maturityはcandidate-parent stateの純関数とする。

carrierがreorgで外れた場合、そのbranchの未確定reservation、deadline、Certificate eligibility、immature rewardを逆順で戻す。

---

## 6. PALW jobの定義

### 6.1 Job ID

```text
JobID = H_k(
  "misaka/palw/block-job/v2",
  ProposalID
  || ComputeSetID
  || ShapeID
  || InputRoot
  || ExecutionSeed
)
```

### 6.2 Execution seed

```text
ExecutionSeed = H_k(
  "misaka/palw/execution-seed/v2",
  finalized_beacon
  || ProposalID
  || round_id
  || pool_snapshot_root
)
```

seedはproposal commit後に判明するfuture randomnessから導出する。

禁止入力:

- worker乱数
- proposerが自由選択したnonce
- output root
- signature bytes
- local arrival order

### 6.3 Seed challenge挿入

cross-job prefix cacheとprecomputationを抑えるため、seed-derived challenge tokenをcanonical inputの先頭近くへ置く。

推奨input layout:

```text
[BOS]
[network-domain tokens]
[execution challenge tokens]
[ProposalID-derived tokens]
[external/service payload tokens]
[deterministic padding tokens]
[EOS-control token if profile requires]
```

challengeを末尾だけへ置くと、固定prefixのKV cacheを再利用しやすいため禁止する。

### 6.4 Token IDsを正本にする

実行identityはraw UTF-8 textではなく、proposal adjunctに含まれる`prompt_token_ids`とする。

```rust
pub struct PalwCanonicalInputV2 {
    pub tokenizer_id: Hash64,
    pub prompt_token_ids: Vec<u32>,
    pub prompt_tokens_exact: u32,
    pub decode_tokens_exact: u32,
    pub max_context_tokens: u32,
    pub stop_on_eog: bool,
}
```

初期PALW PoW profileでは:

```text
stop_on_eog = false
```

EOGを生成しても`decode_tokens_exact`まで計算を継続する。早期EOG seedを探してworkを削減する攻撃を防ぐ。

### 6.5 Job class

#### ConsensusWork

- protocol-owned challenge
- fixed ShapeID
- fixed token counts
- block-side PoW claimに利用可能
- semantic usefulnessを評価しない

#### ServiceAttached

- requester payloadを含められる
- protocol challengeを必ず混入
- ShapeID上限内でpadding
- service fee/bonus対象
- P0/P1ではfork-choiceやbase block validityへ影響しない

任意external promptをそのままconsensus workへ昇格してはならない。

---

## 7. Compute SetとShape

### 7.1 Compute Set identity

次の一つでも変更したら新しい`ComputeSetID`を作る。

- model/GGUF weights
- tokenizer/vocab
- prompt template
- preprocessing
- arithmetic/quantization
- sampling/tie rule
- trace scheme
- canonical operation schedule
- LUT
- state serialization
- output commitment rule

### 7.2 Shape descriptor

```rust
pub struct PalwPowShapeV1 {
    pub shape_id: Hash64,
    pub compute_set_id: Hash64,

    pub prompt_tokens_exact: u32,
    pub decode_tokens_exact: u32,
    pub context_tokens: u32,
    pub batch_size: u16,
    pub bundle_count: u16,

    pub evidence_level: EvidenceLevel,
    pub expected_trace_events: u32,
    pub max_job_bytes: u64,
    pub max_evidence_bytes: u64,

    pub canonical_cwu_per_job: u128,
    pub required_cwu_total: u128,
}
```

### 7.3 初期logits-bound calibration shape例

これはtestnet候補でありmainnet定数ではない。

```text
Shape name:       Q2B-CPU-P96-D16-B1-v1
P_exact:          96 prompt tokens
D_exact:          16 decode tokens
bundle_count:     1
batch_size:       1
trace events:     16
trace payload:    full n_vocab f32 digest per canonical decode call
stop_on_eog:      false
cross-job cache:  false
```

P=96は現測定の68〜81 token promptをpadding可能な初期bucket例であり、benchmark後に変更する場合は新ShapeIDを作る。

---

## 8. PALW difficulty

### 8.1 別PALW hash targetを持たない

PALW workはdeterministic job completionのbinary判定とする。

```text
ValidPALWWork =
  executed exact ComputeSet
  AND exact Shape
  AND exact token counts
  AND exact trace scheme
  AND required_cwu_total satisfied
  AND valid Certificate
```

`full_logits_trace_root < T_palw`のような条件を採用しない。

### 8.2 Difficulty表現

PALW difficultyは次で表す。

```text
PALW_Difficulty = (
  ComputeSetID,
  ShapeID,
  bundle_count,
  required_cwu_total,
  A/B/C replication factor,
  audit policy
)
```

### 8.3 Canonical Work Units

workerが報告したFLOP、wall time、CPU utilizationは信用しない。

```text
raw_work = Σ canonical_semantic_operation_cost(op_i)
CWU(job) = ceil(raw_work / CWU_QUANTUM)
CWU_required = bundle_count × CWU(job)
```

CWUはmodel manifestとShapeからfull nodeが再導出できる。

backend fusion、thread count、kernel分割、実測秒数でCWUが変わってはならない。

### 8.4 Dense transformer schedule例

model固有manifestは最低限次を持つ。

```text
n_layers
n_embd
n_heads
n_kv_heads
head_dim
n_ff
n_vocab
quantization profile
attention schedule
MLP schedule
lm_head schedule
```

概念上のsemantic work:

```text
prefill_work(P) =
  embedding(P)
  + Σ_layers [QKV(P) + attention(P,P) + out_proj(P) + MLP(P)]
  + lm_head(1 or profile-defined rows)

decode_work(P,D) =
  Σ_{i=0}^{D-1} [
    embedding(1)
    + Σ_layers [QKV(1) + attention(1,P+i) + out_proj(1) + MLP(1)]
    + lm_head(1)
  ]
```

実際の係数はCompute Set manifestへ固定し、一般式をnodeが推測してはならない。

### 8.5 Difficulty変更

PALW difficultyをblockごとにretargetしない。

変更手順:

1. multi-machine measurement
2. fastest known adversarial implementation評価
3. new ShapeまたはParameterSet作成
4. Shadow activation at future DAA
5. certified supply、latency、failure率測定
6. Active policyへ昇格

### 8.6 Block rate

block rateはALGO-3 targetが制御する。

PALW側は次のcapacity conditionを満たす必要がある。

```text
R_certified = N_pipe × p_complete / L_cert
```

P2 Required profileのactivation条件:

```text
R_certified >= R_block_target × safety_factor
```

初期推奨:

```text
safety_factor: 3〜5
p95 pool utilization: <= 25%
```

PALW difficultyを上げてcertified supplyが不足した場合、block targetを自動変更して辻褄を合わせてはならない。

---

## 9. Replica poolとfuture selection

### 9.1 Pool snapshot

round `r`のprovider registrationをcutoff前に閉じ、canonical snapshot rootを作る。

```rust
pub struct PalwReplicaPoolEntryV1 {
    pub authority_id: Hash64,
    pub operator_pool_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
    pub free_capacity_units: u64,
    pub supported_compute_sets_root: Hash64,
    pub supported_shapes_root: Hash64,
    pub activation_daa: u64,
    pub standing_state: StandingState,
}
```

### 9.2 Round seed

```text
RoundSeed = H_k(
  "misaka/palw/replica-round/v1",
  round_id
  || pool_snapshot_root
  || later_selected_chain_window
)
```

pool登録時点ではseedが未知でなければならない。

### 9.3 Weighted selection

selectionはcredentialごとにaggregateしたbond capacityを用い、without replacementで行う。

```text
B = WeightedSelect(RoundSeed, ProposerID, ProposalSequence, role=0)
C = WeightedSelect(RoundSeed, ProposerID, ProposalSequence, role=1)
```

禁止:

- bond outpoint単位のunweighted lottery
-同一credentialのsplitでticket数増加
- tx rootをselection inputにする
- signer signatureをselection inputにする
- local first-seen order

### 9.4 Capacity unit

```text
capacity_units = floor(free_bond / B_unit)
```

sublinear bond weightはbond分割でaggregate weightが増えるため避ける。

### 9.5 Assignment reservation

```rust
pub struct PalwAssignmentV1 {
    pub assignment_id: Hash64,
    pub proposal_id: Hash64,
    pub role: PalwRole,
    pub authority_id: Hash64,
    pub bond_outpoint: TransactionOutpoint,
    pub reserved_amount: u64,
    pub deadline_commit_daa: u64,
    pub deadline_open_daa: u64,
    pub evidence_end_daa: u64,
    pub replacement_rank: u8,
}
```

assignment acceptance時にfree capacityをatomicに減らす。同じbondを同時に過剰利用できない。

### 9.6 Replacement

B/Cの各roleは最大1回だけreplacement可能とする。

```text
Assigned -> ReplacedOnce -> Completed
                        \-> Failed
```

unlimited rerollは禁止する。攻撃者にdeadline延長とhonest replica選別の機会を与えるためである。

---

## 10. Canonical execution

### 10.1 実行経路

```text
kaspad
  -> authenticated local IPC
  -> palw-agent
  -> pinned palw-worker
  -> pinned llama.cpp CPU runtime
  -> full logits trace
  -> canonical result projection
```

Ollama、`llama-server`、OpenAI互換API、共有中央workerへfallbackしない。

### 10.2 Runtime profile

初期portable CPU classは最低限次を固定する。

```text
worker binary hash
llama.cpp commit / library hash
compiler/linker/build flags
GGML_NATIVE=OFF
GGML_OPENMP=OFF
GGML_BLAS=OFF
GGML_ACCELERATE=OFF
ISA feature profile
thread count / affinity
FP environment
GGUF hash
tokenizer/vocab hash
prompt token IDs
exact P / D
no early EOG
no cross-job KV cache
```

### 10.3 Floating-point policy

```text
rounding mode = round-to-nearest ties-to-even
fast-math = disabled
FMA policy = profile fixed
FTZ/DAZ = profile fixed
NaN/Inf = invalid execution
signed zero = raw IEEE-754 bytes
serialization = f32 little-endian
```

### 10.4 Job context hash

> [!IMPORTANT]
> **version境界の規則（2026-08-13）。** 現Land実装の `"misaka-palw/job-context/v2"` の
> preimageは基礎設計書§6（= `consensus/core/src/palw_v2.rs`、golden vector凍結済み）で
> あり、ProposalIDを持たない。本節のProposalID束縛形はproposal pipeline導入時の**将来形**
> である。導入時は同じ`/v2` domainを再解釈せず、**新しいdomain（`/v3`等）と新しい
> `trace_scheme_id`** で切り替えること。同一domain keyの下に2つのpreimage形が並存すると、
> 基礎設計書§6で訂正したのと同型の「正直な2実装が互いをrefuteする」forkになる。

```text
JobContextHash = H_k(
  "misaka-palw/job-context/v3"（proposal pipeline導入時に採番）,
  NetworkID
  || ProposalID
  || JobID
  || AssignmentRound
  || ExecutionSeed
  || ComputeSetID
  || RuntimeImplementationID
  || ShapeID
  || TraceSchemeID
  || H(prompt_token_ids)
  || P_exact
  || D_exact
)
```

### 10.5 Logits event

```text
LogitsEvent_i = H_k(
  "misaka-palw/full-logits-event/v2",
  JobContextHash
  || phase
  || phase_step
  || n_vocab
  || logits_dtype
  || logits_count
  || canonical_logits_bytes
)
```

### 10.6 Full logits root

```text
FullLogitsTraceRoot = MerkleRoot(
  LogitsEvent_0,
  ...,
  LogitsEvent_{D-1}
)
```

rootは次もbindする。

- event count
- first/last event kind
- P/D
- n_vocab
- output token IDs hash
- stop reason

### 10.7 Output commitment

```text
OutputRoot = H_k(
  "misaka-palw/output/v2",
  JobContextHash
  || generated_token_count
  || generated_token_ids
  || rendered_output_hash
)
```

rendered textは補助であり、token IDsを正本にする。

---

## 11. Result projectionとcommit-before-open

### 11.1 Projection

```rust
pub struct PalwResultProjectionV2 {
    pub proposal_id: Hash64,
    pub job_id: Hash64,
    pub compute_set_id: Hash64,
    pub runtime_implementation_id: Hash64,
    pub shape_id: Hash64,

    pub execution_root: Hash64,
    pub checkpoint_vector_root: Hash64,
    pub output_root: Hash64,
    pub operation_schedule_commitment: Hash64,
    pub canonical_cwu: u128,

    pub prompt_tokens: u32,
    pub generated_tokens: u32,
    pub stop_reason: u16,
}
```

current logits profileでは:

```text
execution_root = FullLogitsTraceRoot
```

### 11.2 Result commit

各role `x ∈ {A,B,C}`はfresh salt `ρ_x`で先にcommitする。

```text
ResultCommit_x = H_k(
  "misaka-palw/result-commit/v2",
  ProposalID
  || JobID
  || Role_x
  || Projection_x
  || ρ_x
)
```

全commitがchain-orderedになる前に、他roleのprojection/openingを見せない。

### 11.3 Opening

openingは次を含む。

- Projection
- salt
- worker/runtime manifest reference
- evidence DA root
- signer credential
- ML-DSA-87 signature

### 11.4 Exact match

```text
Projection_A == Projection_B == Projection_C
```

比較対象:

- execution root
- output root
- checkpoint vector root
- operation schedule commitment
- canonical CWU
- token counts
- stop reason
- ComputeSet/Shape/Job binding

token-only matchは禁止する。

### 11.5 Mismatch

mismatchだけで誰かを自動slashしない。

処理:

1. proposalを`ComputeMismatch`で終了
2. PALW rewardを発行しない
3. reservationsをevidence windowまで保持
4. objective evidenceが得られたsignerだけslash
5. runtime class incidentを開始

honest minority runtimeを単純majority voteでslashしてはならない。

---

## 12. Certificate

### 12.1 Certificate body

```rust
pub struct PalwCertificateBodyV2 {
    pub version: u16,
    pub network_id: Vec<u8>,
    pub ruleset_id: Hash64,

    pub proposal_id: Hash64,
    pub job_id: Hash64,

    pub parents_root: Hash64,
    pub transaction_root: Hash64,
    pub post_state_root: Hash64,
    pub coinbase_template_root: Hash64,

    pub compute_set_id: Hash64,
    pub shape_id: Hash64,
    pub result_projection: PalwResultProjectionV2,

    pub pool_snapshot_root: Hash64,
    pub round_seed_id: Hash64,
    pub assignment_a: Hash64,
    pub assignment_b: Hash64,
    pub assignment_c: Hash64,

    pub certificate_expiry_daa: u64,
    pub evidence_da_root: Hash64,
}
```

### 12.2 Certificate ID

A/B/Cは同じbodyをrole-separated ML-DSA contextで署名する。

```text
CertificateID = H_k(
  "misaka-palw/certificate/v2",
  CertificateBody
  || SigA
  || SigB
  || SigC
)
```

signature順序はA,B,Cで固定する。

### 12.3 Certificateの意味

Certificateが示すのは次だけである。

> 三つの選出済みbonded authorityが、同じproposal-bound canonical result projectionへslashable signatureを行った。

物理的な三重実行やoperator independenceを暗号学的に証明するものではない。

### 12.4 One-shot

```text
proposal_nullifier    = H(NetworkID || ProposalID)
certificate_nullifier = H(NetworkID || CertificateID)
job_nullifier         = H(NetworkID || JobID)
```

branch-local selected-parent pastで使用済みなら再利用不可。

---

## 13. ALGO-3 block binding

### 13.1 PoWはCertificate後

P2では、Certificate完成後にhash minerがALGO-3 nonce searchを行う。

```text
PoWHeader = (
  ProposalCore,
  CertificateID,
  Nonce,
  ExtraNonce,
  existing ALGO-3 target fields
)

Y = ALGO3_HASH(canonical(PoWHeader))
ValidAlgo3PoW = Y <= T3
```

`CertificateID`がPoW preimageへ入るため、nonceを別Certificateへ移植できない。

P1 optional attachmentでは、PALW attachmentをblock adjunctへbindし、PALW reward claimをblock hashへbindする。base PoW validityはattachmentなしでも成立する。

### 13.2 Exact proposal binding

```text
Cert.ProposalID             == ProposalID(Block)
Cert.ParentsRoot            == Block.ParentsRoot
Cert.TransactionRoot        == Block.TransactionRoot
Cert.PostStateRoot          == Block.PostStateRoot
Cert.CoinbaseTemplateRoot   == Block.CoinbaseTemplateRoot
```

一つでも違えば別proposalであり、Certificateは無効。

### 13.3 Full-node validation order

DoSを避けるため安い検査から行う。

1. wire length、version、network、RulesetID
2. header parents、timestamp、mass、target、ALGO-3 digest
3. proposal carrier ancestry、sequence、expiry
4. transaction root、UTXO/EVM transition、post-state root
5. coinbase template equality
6. pool snapshot、future seed、A/B/C selection、distinctness
7. bonds、reservations、deadlines
8. result commitments/openings
9. exact projection match
10. Certificate signatures
11. exact proposal binding
12. nullifiers
13. branch-local state and reward maturity record

P0/P1ではPALW attachmentが無い場合、PALW手順をskipする。存在するのにinvalidならblockまたはPALW reward claimをprofile規則に従って拒否する。

---

## 14. Data availability

### 14.1 必須DA

最低限次を取得可能にする。

- ProposalCore
- transaction/body bytes
- canonical prompt token IDs
- model/runtime/shape manifest reference
- ResultCommit/opening
- result projection
- per-event root listまたはreconstructible trace object
- checkpoint vector commitment
- Certificate
- audit objects
- fraud evidence objects

### 14.2 URLをconsensus inputにしない

URL、DNS、vendor API、central object serviceはblock validity inputにしない。

on-chain objectまたはcontent-addressed P2P DAから、hash/length/proofで検証可能にする。

### 14.3 Retention

```text
Retention >= AuditWindow
           + EvidenceWindow
           + StabilityWindow
           + ReorgMargin
```

DA欠落時:

- P0/P1: PALW reward不成立、base blockはprofileに従う
- P2: Certificate/adjunct availability不足ならblock rejectまたはproposal失効

### 14.4 Reserved control band

proposal、assignment、commit/open、audit、evidenceにはper-block/per-epoch reserved byte/cycle budgetを持つ。

通常transaction fee marketがevidence inclusion capacityを完全に奪えないようにする。

ただしHash majority censorshipをreserved bandだけで解決したと主張しない。

---

## 15. Post-certificate audit

### 15.1 Audit seed

block接続後のlater selected-chain windowから導出する。

```text
AuditSeed = H_k(
  "misaka-palw/post-audit/v2",
  BlockHash
  || CertificateID
  || later_window
)
```

proposal/certification時点でaudit targetを予測できない。

### 15.2 Audit probability

初期testnet推奨:

```text
minimum p_audit = 10%
```

次は20〜100%へ引き上げられる。

- new Compute Set
- new runtime implementation
- recent mismatch
- recent no-show
- high-value external output
- suspicious operator concentration

ruleはParameterSetへ固定し、auditorが恣意的に変更しない。

### 15.3 Auditor D

DはA/B/Cを除外してfuture-selectする。

Dも:

- bond
- assignment reservation
- commit-before-open
- deadline
- no-show slash

を持つ。

### 15.4 Current logits-only phase

current full-logits profileでは、Dはjob全体をfull replayする。

- canonical job再実行
- execution/output/schedule/CWU比較
- mismatch時にevidence bundle作成

**full replay resultの不一致だけでcorrectness slashを実行しない。** objective false signed claimをnodeが再実行できるevidence形式が必要である。

### 15.5 Future TraceVM phase

canonical Compute VM/checkpoint witnessが完成したら、first divergent intervalを提示する。

```text
FraudEvidence = (
  BlockHash,
  CertificateID,
  Signer,
  IntervalIndex,
  PreStateRoot,
  ClaimedPostStateRoot,
  ExecutionWitness,
  MerklePaths
)
```

reference verifierがintervalを再実行し:

```text
DerivedPostStateRoot != ClaimedPostStateRoot
```

ならobjective fraudとする。

TraceVM完成前に「permissionless fraud proof済み」と表現しない。

---

## 16. Slashing

### 16.1 Closed fault list

| Fault | Objective evidence | 初期処理 |
|---|---|---|
| NoShow | accepted assignment + available inclusion + deadline miss | reservationの一部 |
| CommitRefusal | signed commit後にopeningなし | reservation大部分 |
| MalformedSignedObject | signer signature付きcanonical violation | reservation |
| Equivocation |同一scopeのconflicting valid signatures | base bond全体候補 |
| DoubleReservation | free capacity超過のstate proof | base bond全体候補 |
| FalseInterval | signed checkpoint claimをTraceVMが反証 | reservation + base bond |
| FalseCertificate | canonical executionと異なるsigned Certificate | false signerのbase bond |
| RepeatedFraud | stability horizon内の再犯 | remaining bond + standing reset |

第三者が作ったinvalid signatureやmalformed bytesを被告providerのfaultにしない。

### 16.2 Evidence ID

```text
EvidenceID = H_k(
  "misaka-palw/evidence/v2",
  NetworkID
  || BlockHash
  || AssignmentID
  || FaultClass
  || CanonicalDetail
)
```

### 16.3 State transition

```text
Absent -> Frozen -> Settled
```

valid evidence acceptance時:

1. bond/reservationをbranch stateから再解決
2. duplicate/lower-severity laundering拒否
3. lienをfreeze
4. authorityをsuspend、free capacityを0
5. immature PALW rewardをBurnPending
6. stability後にburn/reporter/compensationを決定
7. standing/ageをreset

追加のoperator裁量やvalidator voteをsettlement条件にしない。

### 16.4 Burn-first

```text
Slash = Burn + Reporter + Compensation
Burn / Slash >= 80%
Reporter + Compensation <= 20%
```

FalseCertificate、equivocation、repeated fraudはburn 90%以上を推奨する。

reporter rewardを大きな固定割合にせず、replay/proof/inclusion costとbounded bountyで制限する。self-reportで資金を回収する攻撃を抑える。

### 16.5 Exit delay

```text
ExitDelay >= AuditWindow
           + EvidenceWindow
           + StabilityWindow
           + Margin
```

assignment、pending evidence、immature rewardが一つでもあればwithdraw不可。

---

## 17. Reward

### 17.1 OrderingとPALW rewardを分離する

block接続時:

- ordinary transaction stateはALGO-3規則で確定
- hash miner rewardは通常maturity
- PALW signer rewardは追加maturity

```text
PALWRewardMaturity >= AuditWindow + EvidenceWindow + StabilityWindow
```

### 17.2 Fraud後

false Certificateが後から証明されても次をrollbackしない。

- block ordering
- transactions
- UTXO/EVM state
- ALGO-3 cumulative work

変更するのは次だけ。

- immature PALW reward burn
- false signer bond slash
- standing/capacity
- reporter/audit reimbursement

### 17.3 Reward structure

具体比率はMonetaryPolicyIDへ固定する。

概念上:

```text
R_total = R_hash + R_primary + R_replica_B + R_replica_C + R_audit_reserve
```

- `R_hash`: base ordering/security payment
- `R_primary`: A execution service
- `R_replica_*`: independent replay service
- `R_audit_reserve`: selected auditとevidence inclusion

PALW reward総額は、false outputから得られる最大protocol gainをboundできる上限を持つ。

### 17.4 External output

外部applicationがLLM outputを即時利用し、block rewardを超える価値を得る場合、その価値をattack gainへ含めなければならない。

高価値outputは:

- audit maturityまでprovisional
- requester独自verification
-追加bond/escrow

のいずれかを要求する。

---

## 18. 経済条件

worst-caseではA/B/C collusionを前提とする。

```text
p_detect_min × (irrecoverable_slash + lost_future_fees)
  > max_protocol_gain
    + saved_compute_cost
    + bribe_budget
    + external_value
    + uncertainty_margin
```

### 18.1 Bond floor

mainnet bondを固定名目額だけで決めない。

```text
B_min >= (
  G_max
  + C_saved
  + B_bribe
  + ExternalValueCap
  + Margin
) / (p_detect_min × burn_fraction)
```

Testnetで1,000,000 MISAKAを使う場合も、mainnet価値を意味するものではない。

### 18.2 Assignment reserve

一つのbase bondが同時に無制限Certificateへ署名できないよう、assignmentごとにreserveする。

初期testnet候補:

```text
base bond:          1,000,000 MISAKA
assignment reserve: 100,000 MISAKA
```

数値はcalibration前提であり、hard-coded marketing numberにしない。

---

## 19. Liveness

### 19.1 Parallel proposal pipeline

LLM replayがblock intervalより長くても、複数proposalを同時処理する。

```text
R_certified = N_pipe × p_complete / L_cert
```

必要pipeline数:

```text
N_pipe >= ceil(
  R_target × L_cert × safety_factor / p_complete
)
```

### 19.2 Inventory

hash minerがnonce searchできるcertified proposal inventoryを保持する。

```text
InventoryTarget >= R_block_target × InventoryHorizon
```

inventoryが少ないとき、同じproposalへ無制限replica rerollせず、新しいparallel proposalを供給する。

### 19.3 Deadline

```text
ReplayDeadline(shape) =
  ceil(alpha × T_exec_p99(shape) / control_tick)
  + network_margin
```

`alpha > 1`。自己申告latencyではなく公開measurementからParameterSetへ固定する。

### 19.4 Global outage

#### P0/P1

PALW replica capacityが不足した場合:

- new PALW assignments停止
- PALW reward停止
- base ALGO-3 block liveness継続

#### P2

- new proposal commit停止
-既存certified inventoryのhash miningは継続
- inventory枯渇後chain停止
- validator vote、DNS、operator flagでhash-only fallbackしない

P2からhash-onlyへ戻すには、事前定義された別Rulesetまたはhard forkが必要。

---

## 20. Attack analysis

### 20.1 Output entropy collapse

出力textのentropyが低くてもfull logits sequenceがinput-sensitiveなら、text dictionary attackは直接成立しない。

ただしhash化は元入力のentropyを増やさないため、10,000以上のseedでroot distribution、collision、p_maxを測定する。

### 20.2 Fake root

workerは任意rootを申告できる。root自体はproofではない。

防御:

- independent replay
- commit-before-open
- post-audit
- bond/slash
- maturity
- objective fraud evidence

### 20.3 Seed grinding

防御:

- proposal commit後のfuture seed
- ProposalSequence消費
- fixed P/D
- no early EOG
- challengeをprefix近くへ挿入
- tx rootをreplica selection inputにしない

### 20.4 Cached prefix

challengeを先頭近くへ置き、jobごとに新contextを作る。

「cache禁止」は悪意workerへ強制できないため、最速合法cacheを含むminimum adversarial costでrewardをcalibrateする。

### 20.5 Shared backend

A/B/Cが同一中央API、同一model process、同一cache、同一operator control planeを共有する場合、独立replicaとして数えない。

### 20.6 A/B/C collusion

exact matchだけでは防げない。

防御:

- future selection
- operator/failure-domain diversity
- post-audit D
- permissionless evidence
-高bond
- burn-first
- limited reward

residual riskとして明示する。

### 20.7 Selective abort

不利なresultやauditorを見てabortする攻撃に対し:

- commit-before-open
- proposal sequence fee
- reservation slash
- one replacement only
- standing penalty

### 20.8 Transaction transplant

CertificateがProposalID、parents、tx root、post-state root、coinbase rootへbindし、CertificateIDがPoW preimageへ入るため拒否する。

### 20.9 Replay/double use

proposal、job、Certificate、reward、evidenceに独立nullifierを持つ。local first-seen cacheではなくbranch-indexed stateで管理する。

### 20.10 Audit censorship

Hash majorityがevidenceをwindow全体でcensorすればslashingは失敗する。reserved bandは軽減策であり、ALGO-3 inclusion-liveness assumptionを置き換えない。

### 20.11 Compute VM bug

verifier bugがhonest signerの自動没収へつながり得る。

- correctness slashは段階的activation
- positive/negative golden vectors
-二実装agreement
- external audit
- emergency halt

を必須にする。

---

## 21. Reorg、IBD、pruning

### 21.1 Reorg

proposal carrier、assignment、reservation、Certificate、audit、evidence、reward maturityをbranch-local storeへ置く。

losing branchのcontent blobは残っても、selected-parent viewでgoverning stateにならない。

### 21.2 Fork binding

Certificateはexact parent rootとcarrier ancestryへbindする。

同じtransaction rootでもparent setが異なれば別ProposalID。

### 21.3 Pruning snapshot

pruning bundleは最低限次をcommitする。

- active Compute Set/ParameterSet roots
- provider pool snapshot state
- live assignments/reservations
- active proposals/certificates
- job/certificate nullifier root
- audit/evidence state
- immature PALW reward state
- DA retention commitments
- RulesetID / WireContractID / GoldenVectorRoot

pruned nodeもarchival nodeと同じPALW predicateを再導出する。

---

## 22. Object budgetとDoS

各object、block、round、epochへ独立capを置く。

```text
Σ Bytes(objects) <= ByteBudget
Σ VerifyCycles(objects) <= CycleBudget
Σ EvidenceCost(objects) <= EvidenceBudget
Σ Signatures(objects) <= SignatureBudget
```

producer申告costを信用せず、bodyからnodeが再計算する。

必須cap:

- proposal count
- active proposal count per proposer
- assignments per authority
- Certificate bytes
- signature count
- checkpoint entries
- trace/event count
- DA chunk count
- evidence count
- replacement count
- recursive proof depth = 0または固定

---

## 23. 初期testnet ParameterSet例

以下は研究用初期値でありmainnet承認ではない。

| Parameter | 候補値 | 備考 |
|---|---:|---|
| Header PoW | ALGO-3のみ | fork choice維持 |
| PALW execution algo | 2 | runtime namespace |
| P/D | 96 / 16 | calibration shape例 |
| Replication | A+B+C | 3 executions |
| Base bond | 1,000,000 MISAKA | test value |
| Assignment reserve | 100,000 MISAKA | exposure cap |
| Post-audit | 10%以上 | risk-based増加可 |
| Replacement | roleごと最大1回 | state bound |
| Burn fraction | 80%以上 | severe fraud 90%以上 |
| Pool p95 utilization | 25%以下 | spare capacity |
| Certified supply | block demandの3〜5倍 | P2 gate |
| PALW reward maturity | audit+evidence+stable | clawback回避 |
| Fork-choice PALW work | 0 | 全profile共通 |
| P0/P1 hash fallback | base block継続 | PALW reward停止 |
| P2 fallback | なし | hard forkのみ |

---

## 24. Rollout

| Stage | 動作 | PALW reward | Block mandatory | Fork-choice work |
|---|---|---:|---:|---:|
| S0 Local | two VPS manual vectors | 0 | no | 0 |
| S1 Closed Replay | five nodes、UDS | 0 | no | 0 |
| S2 Consensus Visible | roots/objects on chain | 0 | no | 0 |
| S3 Public No-Value | permissionless operator | 0 | no | 0 |
| S4 Reward Attached | delayed capped reward | limited | no | 0 |
| S5 Required Testnet | dual-resource hard fork | limited | yes | 0 |
| Mainnet Candidate | separate ceremony | governed | TBD | 0 |

### 24.1 Three activation levers

```text
Land   = code/types/testsを入れる、consensus behavior不変
Accept = objects/certificatesをnetwork ruleで受理
Value  = rewardまたはmandatory validityを有効化
```

この順序を逆転しない。

---

## 25. Pre-publication gates

### G1 Wire and identity

- ProposalID、JobID、CertificateID、PoW preimageの二実装vector一致
- unknown version/kind fail closed
- canonical Borsh re-encode equality

### G2 Runtime determinism

- 3以上のCPU/hardware classでsame-job mismatch 0
- full 64-byte root比較
- restart、cold/warm、concurrency
- input-sensitive root

### G3 Real execution binding

- model runnerからreceipt issuerまでauthenticated IPC
- live binary/model hash
- Ollama/JSON fake result pathなし
- partial trace成功扱いなし

### G4 Independent actors

- 3以上operator
- A/B別所有者
- Cまたはauditor別operator
- shared central backendなし

### G5 DA

- public content-addressed retrieval
- withholding challenge
- retention
- pruning後replay

### G6 Proposal capacity

- certified supply 3〜5x demand
- p95 utilization <=25%
- no-show/one replacement
- stale proposal rate公開

### G7 Reorg/IBD

- 50/50 partition/reconnect
- carrier reorg
- Certificate reorg
- clean IBD
- late join
- pruning parity

### G8 Fraud path

- false Certificateを意図的に接続
- objective evidenceでA/B/Cをslash
- PALW rewardだけburn
- transaction stateをrollbackしない

TraceVM未完成ならG8は未達であり、correctness slash/mandatory P2へ進まない。

### G9 Economic

- A/B/C coalition
- Hash + coalition
- audit censorship
- non-economic attacker
- external output value
- bond/reward sensitivity

### G10 Supply chain

- source commit
- toolchain
- binary hash
- model hash
- WireContractID
- ComputeSetID
- ParameterSetID
- GoldenVectorRoot
- signed OTA release

---

## 26. 実装module

### consensus/core

```text
palw_pow.rs
  ProposalCore
  JobDescriptor
  Assignment
  ResultCommit/Opening
  Certificate
  nullifiers
  parameter set
  fault/evidence types
```

### consensus/processes

```text
proposal validation
pool snapshot/selection
reservation accounting
certificate validation
block attachment validation
reward maturity
slashing settlement
reorg/pruning state
```

### runtime-palw

```text
canonical job parser
worker adapter
full logits trace
schedule/CWU
result projection
local verifier
```

### palw-agent

```text
UDS
queue/deadline
worker lifecycle
runtime quarantine
metrics
```

### stores

```text
proposal store
assignment/reservation store
certificate store
nullifier store
audit/evidence store
immature reward store
pruning frontier
```

### RPC

```text
getPalwProposal
getPalwCertificate
getPalwAssignment
getPalwAuditStatus
getPalwRuntimeCapability
getPalwParameterSet
submitPalwEvidence
```

RPC responseはconsensus stateの観測であり、block validation inputとしてremote RPCを参照しない。

---

## 27. 擬似コード

### 27.1 Proposal作成

```text
function build_proposal(parent_state, txs, compute_set, shape):
    require compute_set is Shadow or Active per profile
    require shape belongs to compute_set

    seq = parent_state.next_proposal_sequence(proposer)
    roots = execute_transaction_transition_without_commit(txs)

    core = ProposalCore(
        parent roots,
        tx root,
        post-state root,
        coinbase template root,
        compute set,
        shape,
        seq,
        expiry
    )

    proposal_id = hash(core)
    publish BlockProposalCommit(core, signature, DA root)
```

### 27.2 Assignment

```text
function assign_replicas(proposal, snapshot, future_seed):
    B = weighted_select_without_replacement(snapshot, seed, proposal.proposer, proposal.seq, 0)
    C = weighted_select_without_replacement(snapshot - {A,B}, seed, proposal.proposer, proposal.seq, 1)

    require A,B,C distinct credentials
    require operator policy
    reserve bond capacity atomically
    return assignments
```

### 27.3 Execute

```text
function execute_job(job):
    verify runtime profile and live hashes
    verify exact token IDs, P, D
    create fresh context
    disable early EOG and cross-job cache

    for canonical decode step i in 0..D:
        logits = model_forward()
        reject non-finite logits
        event[i] = hash(job context, i, full logits bytes)
        token[i] = deterministic_argmax(logits)

    projection = {
        execution_root = merkle(event),
        output_root = hash(token IDs),
        schedule_root,
        checkpoint_root,
        CWU,
        counts,
        stop_reason
    }
    return projection
```

### 27.4 Certificate

```text
function certify(A_open, B_open, C_open):
    verify all commit/open pairs
    verify assignments, bonds, deadlines
    require projection_A == projection_B == projection_C
    body = bind proposal roots + projection + assignments + expiry
    require three role-separated signatures
    return Certificate(body, sigA, sigB, sigC)
```

### 27.5 Block verify

```text
function verify_block(block, parent_state):
    verify ALGO-3 PoW
    verify ordinary block transition

    if profile == P0/P1 and no PALW attachment:
        accept base block

    verify proposal carrier and ProposalID
    verify CertificateID binding
    verify future selection and reservations
    verify exact proposal roots
    verify signatures and nullifiers
    record audit/maturity state

    if profile == P2:
        require Certificate exists

    accept block
```

---

## 28. 注意事項

1. **full logits rootはproofではない。** 任意rootを申告できるため、replay/audit/bondが必要。
2. **A/B/C exact matchは独立性を証明しない。** 同じoperatorの三つのprocessを三者と数えない。
3. **P2はlivenessをPALW poolへ依存させる。** total outage時の停止を受け入れられないならP1に留める。
4. **PALWをfork-choice workへ加算しない。** current correctness pathで加算するとfalse certificateがhistory selectionへ影響する。
5. **mismatchだけでslashしない。** objective signed false claimが必要。
6. **early EOGをwork削減に使わせない。** exact Dまで実行。
7. **external promptを自由なwork difficultyにしない。** fixed Shape、challenge、paddingが必要。
8. **dynamic PALW DAAを作らない。** Shape/ParameterSetをfuture DAAで変更する。
9. **runtime telemetryをwork量として信用しない。** CWUはscheduleから再導出。
10. **fraud後にtransaction historyを巻き戻さない。** ALGO-3 orderingとPALW reward accountabilityを分離。
11. **audit censorshipはHash inclusion-livenessへ依存する。** reserved bandだけで無敵にはならない。
12. **current logits-only pathでTraceVM完成を主張しない。** correctness slashingは段階的に有効化。
13. **旧Ollama経路へfallbackしない。** compute capabilityを停止する。
14. **OTA導入とCompute Set/Ruleset activationを分離する。** binaryが届いたことはgovernance approvalではない。
15. **1M bondはmainnet価値を意味しない。** reward、検出率、外部価値から再計算する。

---

## 29. 現時点の判定

| 項目 | 判定 |
|---|---|
| full-logits PALW worker開発 | GO |
| P0 Shadow object/state実装 | GO |
| closed multi-VPS replay | GO |
| public no-value PALW testnet | 条件付き |
| P1 delayed capped reward | audit/DA/operator gate後 |
| correctness automatic slash | TraceVM/objective evidence完成までNO-GO |
| P2 every-block mandatory PALW | capacity/fraud/reorg gateまでNO-GO |
| PALW fork-choice work加算 | NO-GO / 設計上不採用 |
| mainnet positive value | 別承認 |

現コード監査では、real model runnerからreceipt issuerまでのauthenticated binding、independent Provider A/B、production auditor、DA、global nullifier、settlement、correctness evidenceがStopShip項目として残っている。したがって、今すぐ安全に実装できるのはP0 Shadowとclosed replayまでである。

---

## 30. 参考資料

- Bitcoin: A Peer-to-Peer Electronic Cash System
- Hashcash: A Denial of Service Counter-Measure
- PALW v8: Single-Block Dual-Resource Consensus
- MISAKA Whitepaper V4 Compute Set Registry
- MISAKA PALW VPS Canonical Worker Design v0.1
- MISAKA PALW Release Readiness Audit
- TrueBit: A Scalable Verification Solution for Blockchains
- Proof of Replication / Proof of Spacetime literature

---

## 31. 最終結論

PALW側PoWの正しい位置付けは、**Hash-PoWを置換する別chain workではなく、ALGO-3 proposalへbindされた固定量のaudited compute certificate**である。

実装の中心は次の七点となる。

```text
1. proposal commit before compute
2. future challenge and future replica selection
3. fixed Compute Set / Shape / CWU
4. A/B/C commit-before-open exact replay
5. CertificateID in block/PoW binding
6. post-certificate audit + delayed reward
7. objective evidence only slashing
```

初期mainlineではPALWのfork-choice weightを0に保ち、P0 ShadowからP1 Reward-Attachedへ進める。全blockにPALW Certificateを必須とするP2は、total replica outageでchainが止まること、A/B/C collusion、audit censorship、TraceVM correctness pathを含む全gateを通過した後の別Rulesetとしてのみ検討する。
