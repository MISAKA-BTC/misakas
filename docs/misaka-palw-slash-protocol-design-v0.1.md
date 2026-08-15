# MISAKA PALW-Slash Protocol Design Specification

**文書名:** MISAKA PALW-Slash Protocol Design Specification  
**略称:** PALW-S  
**版:** Draft v0.1  
**対象:** MISAKA BlockDAG / PALW miner / PALW verifier / DNS・finality連携  
**状態:** 実装・計測・監査前の設計案  

> [!IMPORTANT]
> **本書は入力仕様であり、判定層は ADR-0027 で置き換えられている。** 後から加わった2前提
> —— **BFT非依存**（honest-majority committeeを真理の源にしない）と
> **challenge乱数非依存**（hash由来challenge位置の予測不能性にslash判定を依存させない）——
> により、本書の quorum 判定（§12.3）、appeal jury（§16.3）、
> `ComputationFaultCertificateV1` の quorum bitmap（§15.2）、
> future-anchor challenge を安全性の根拠とする構成（§8）、および
> sampling由来の `P_detect = 1-(1-f)^q`（§9）は、**そのまま実装してはならない**。
> 置換規範は [`ADR-0027`](adr/0027-palw-slash-unilateral-fraud-proofs.md) の §4 対応表。
> bond模型（§4）、offense taxonomy（§17）、DA規則（§10）、circuit breaker（§19）、
> 段階導入（§21）、timing（§23）、禁止事項（§27）、試験計画（§28）は
> ADR-0027 の修正を適用したうえで**そのまま採用する**。

---

## 0. 文書の位置づけ

本書は、MISAKAのPALWに、miner・verifier・challengerを対象としたbondおよびslashingを組み込むための実装指向設計書である。

元設計の中核である次の方針は維持する。

1. 推論runtimeとconsensus-criticalなverification layerを分離する。
2. minerが先にproof materialへcommitし、その後の予測不能な乱数でchallenge位置を決定する。
3. proof materialをfinal logitsだけに限定せず、intermediate activation、token state、selected GEMM traceまで含める。
4. PALW verificationをBlockDAGのtransaction pathから分離し、非同期で検証・精算する。
5. PALW失敗を直ちにchain全体のinvalidityへ接続せず、段階的にreward、reputation、DNS/finality weightへ反映する。

本方式は、LLM計算の完全な数学的証明ではない。予測不能なspot checking、複数verifier、bond、slashing、異議申立てを組み合わせ、**指定計算を省略する期待利益を負にする確率的・経済的検証方式**である。

本書中の `MUST`、`MUST NOT`、`SHOULD`、`MAY` は、それぞれ必須、禁止、推奨、任意を表す。

---

## 1. 設計目標

### 1.1 Security Goals

PALW-Sは以下を満たすことを目標とする。

- minerがfinal outputだけを予測、取得、再利用してproofを生成する攻撃を困難にする。
- minerが一部token、一部layer、一部GEMMだけを計算し、残りを捏造する期待利益を負にする。
- commitment後までchallenge位置とverifier assignmentを予測不能にする。
- minerによるproof data withholdingをslash可能にする。
- verifierが再計算せず他者の判定をコピーする行為を困難にする。
- verifierによる虚偽PASS、虚偽FAIL、equivocation、応答放棄をslash可能にする。
- challengerによる根拠のない大量challengeを有料化し、griefingを抑止する。
- hardware、driver、runtime差による正直な不一致を、直ちに不可逆slashへ変換しない。
- PALW検証遅延や紛争が10 BPS BlockDAGの進行を停止させない。
- slashing実装の不具合が大量誤slashへ波及する前に停止できる。

### 1.2 Non-Goals

本版では以下を保証しない。

- SNARK/STARKと同等の完全な計算健全性。
- 任意runtime・任意hardware間の無条件bit-exact一致。
- Sybil identityの完全排除。
- off-chainで行われた贈賄・共謀の完全検出。
- 外部APIや第三者minerから取得した正しい内部状態の出所証明。

これらは、bondとランダム検証によって経済的に抑止する対象であり、暗号学的に完全排除されるわけではない。

---

## 2. システム構成

```text
                         MISAKA PALW-S
                               │
          ┌────────────────────┼────────────────────┐
          │                    │                    │
        vLLM               llama.cpp          TensorRT / ROCm
          │                    │                    │
          └────────────────────┼────────────────────┘
                               ↓
                    Runtime Adapter API
                               ↓
                    Canonicalizer Registry
                               ↓
          ┌────────────────────┼────────────────────┐
          │                    │                    │
  Canonical logits      hidden states       GEMM / op trace
          │                    │                    │
          └────────────────────┼────────────────────┘
                               ↓
                    Composite Merkle Roots
                               ↓
                    PALW Commit + Work Bond
                               ↓
                  future finalized DAG anchor
                               ↓
            Challenge + Random Verifier Assignment
                               ↓
        Miner Opening / Verifier Commit-Reveal / Appeal
                               ↓
                PASS / FAIL / DISPUTED / EXPIRED
                               ↓
        Reward release / slash / reputation / DNS weight
```

### 2.1 必須モジュール

実装は最低限、次のモジュールへ分割する。

| Module | Responsibility |
|---|---|
| `palw-profile-registry` | model、weights、tokenizer、runtime、quantization、canonicalizationの固定 |
| `palw-bond` | base bond、work bond、verifier bond、challenge bond、unbonding管理 |
| `palw-commit` | PALW commitment受理、署名、重複・期限検査 |
| `palw-randomness` | future DAG anchorおよびepoch randomnessからchallenge seed生成 |
| `palw-challenge` | token、layer、operator、GEMM tile、prefix replay位置の抽選 |
| `palw-da` | proof data manifest、content addressing、availability監視 |
| `palw-verifier` | verifier assignment、commit-reveal、quorum形成 |
| `palw-dispute` | fraud proof、appeal、再検証jury、証拠保全 |
| `palw-slash` | offense分類、penalty計算、配分、strike管理 |
| `palw-settlement` | reward解放、PALW credit、reputation、DNS/finality連携 |
| `palw-circuit-breaker` | profile単位の異常検知、slash凍結、quarantine |

Runtime adapterの更新が、`palw-bond`、`palw-slash`、`palw-settlement`等のconsensus codeを変更してはならない。

---

## 3. 参加者と責務

### 3.1 PALW Miner / Prover

minerは指定されたprofileとinputに対してLLM計算を実行し、proof materialをcommitする。

minerは以下を行う。

- base bondを登録する。
- proofごとのwork bondをlockする。
- canonical proof materialを生成する。
- commitment前にproof dataをDA層へ配置する。
- challenge後、期限内にMerkle opening、requested tensor slice、trace、checkpointを提出する。
- dispute中は関連データを保持する。

### 3.2 PALW Verifier

verifierは割り当てられたchallengeを独立に再計算する。

verifierは以下を行う。

- verifier base bondを登録する。
- assignmentごとのverifier bondをlockする。
- 他verifierのreveal前に自分の判定をcommitする。
- reveal時にcanonical result hash、実行manifest、nonceを公開する。
- 共通challengeと個別challengeの双方を処理する。
- appealに選出された場合はreference profileで再検証する。

### 3.3 Watcher / Challenger

任意のwatcherは、公開データからobjective faultまたはcomputation faultを検出し、challenge bondを添えてfraud proofを提出できる。

### 3.4 Protocol Registry

registryは、slash可能な計算条件を一意に固定する。

registryに存在しないmodel/runtime/canonicalization profileはPALW reward対象にならない。未知profileを受理してから「たぶん同じ計算です」で押し切る設計は採用しない。

### 3.5 Settlement Layer

settlement layerは、PALW reward、work bond、verifier reward、reputation、DNS/finality weightを遅延精算する。

BlockDAG自体の進行は、通常のPALW disputeによって停止しない。

---

## 4. Bondモデル

PALW-Sでは、長期信用とproof単位のriskを分離するため、4種類のbondを使用する。

### 4.1 Base Bond

`BaseBond` はoperator identityに紐づく長期collateralである。

用途:

- 継続的な参加資格
- repeat offenseへの追加slash
- verifier selection weightの一要素
- unbonding中の過去proofに対する遡及責任

BaseBondは同一資金を無制限のproofへ重複担保してはならない。

```text
available_exposure = base_bond - locked_long_term_exposure
Σ active_exposure_i <= available_exposure × max_leverage
```

初期実装では `max_leverage <= 1.0` を推奨する。信用の二重計上は、金融でもchainでも名前を変えただけで同じ事故を起こす。

### 4.2 Work Bond

`WorkBond` はPALW proofごとにlockされる。

WorkBondは少なくとも、想定される最大不正利得に比例しなければならない。

```math
G_{max} = R_{job} + C_{honest} - C_{cheat}
```

ここで、

- `R_job`: 不正proofが通過した場合のreward
- `C_honest`: 正直な計算、DA、応答コスト
- `C_cheat`: 不正方式で実際に支払うコスト

有効penaltyを、

```math
S_{eff} = R_{forfeit} + B_{work} + E[B_{base\_slash}] + V_{reputation}
```

とする。安全係数 `λ > 1` を用いて、

```math
P_{detect} \cdot S_{eff} \ge \lambda \cdot G_{max}
```

を満たすようにする。

初期値は `λ = 2.0` 以上を推奨する。ただし `P_detect` を希望的観測で置いてはならず、attack simulationと実測から保守的な下限を使う。

### 4.3 Verifier Bond

各assignmentに対し、verifierは `VerifierBond` をlockする。

VerifierBondは、最低でも次を上回る必要がある。

- 再計算を省略して浮くverification cost
- minerから受け得る合理的なbribe
- false verdictで得られる外部利益

```math
P_{audit} \cdot S_{verifier} \ge \lambda_v \cdot G_{verifier}
```

### 4.4 Challenge Bond

watcherがfraud proofまたはappealを提出する際にlockする。

- challenge成功: 原則返却し、bountyを付与する。
- challenge失敗: 全部または一部をslashする。
- protocol/profile障害が原因: 返却し、profileをquarantineする。

Challenge Bondはspam防止用であり、正当な異議申立てを不可能にするほど高額にしてはならない。

### 4.5 Unbonding Delay

すべてのBaseBondは、最大dispute期間より長いunbonding delayを持つ。

```text
unbonding_delay >= max_active_proof_lifetime + max_dispute_window + safety_margin
```

初期提案は7日以上である。高価値proofやDNS/finality weightへ影響するproofでは、14〜30日へ延長できるprofileを用意する。

---

## 5. Canonical Profile

slashは、何を正しい計算とするかが一意でなければ成立しない。

### 5.1 `PALWProfileV1`

```rust
struct PALWProfileV1 {
    profile_id: Hash32,
    algo_id: u32,
    model_id: Hash32,
    weights_hash: Hash32,
    tokenizer_hash: Hash32,
    tokenizer_config_hash: Hash32,
    runtime_profile_hash: Hash32,
    quantization_profile_hash: Hash32,
    canonicalization_version: u32,
    generation_config_hash: Hash32,
    max_context_tokens: u32,
    max_output_tokens: u32,
    checkpoint_interval: u32,
    trace_schema_version: u32,
    activation_projection_version: u32,
    allowed_reference_runtimes_root: Hash32,
    activation_epoch: u64,
    deactivation_epoch: Option<u64>,
}
```

### 5.2 CanonicalTensorV1

raw float tensorを直接hashしてはならない。

概念例:

```text
runtime tensor
    ↓
fixed tensor ordering
    ↓
profile-defined scaling
    ↓
round-to-nearest-even
    ↓
signed integer quantization
    ↓
fixed-width little-endian encoding
    ↓
domain-separated hash
```

量子化例:

```math
q(x) = clamp(roundToEven(x / s), -2^{b-1}, 2^{b-1}-1)
```

`scale s`、bit幅 `b`、clamp規則、NaN/Inf処理、tensor orderingはprofileに固定する。

`abs(a-b) < ε` のような曖昧なruntime比較を、直接slash判定へ使用してはならない。

### 5.3 Profile Conformance

profileはactivation前に、対象hardware/runtime matrixでconformance testを通過しなければならない。

最低対象例:

- NVIDIA datacenter GPU
- NVIDIA consumer GPU
- AMD ROCm
- CPU reference runtime
- 採用予定のvLLM / llama.cpp / TensorRT-LLM adapter

未解決のcanonical mismatchが存在するprofileでは、computation mismatchによるBaseBond slashを有効化してはならない。

---

## 6. Proof Material

### 6.1 Composite Commitment

PALW proofは少なくとも以下をcommitする。

- final output hash
- token-level canonical logits commitment
- selected hidden-state commitment
- activation projection commitment
- operator/GEMM trace commitment
- checkpoint state commitment
- proof data availability root

```text
ProofRoot
  ├── TokenRoot
  │     ├── token 0 composite leaf
  │     ├── token 1 composite leaf
  │     └── ...
  ├── ActivationRoot
  ├── TraceRoot
  ├── CheckpointRoot
  └── DARoot
```

### 6.2 Token Composite Leaf

```rust
struct TokenCompositeLeafV1 {
    token_index: u32,
    token_id: u32,
    canonical_logits_hash: Hash32,
    hidden_projection_hashes_root: Hash32,
    activation_hashes_root: Hash32,
    trace_segment_root: Hash32,
    checkpoint_parent_hash: Hash32,
    prev_token_leaf_hash: Hash32,
}
```

`prev_token_leaf_hash` を含め、token間のchainを形成する。これにより、独立した正しいleafを寄せ集める攻撃を難しくする。

### 6.3 Execution Path Commitment

単一GEMM tileのhashだけでは、依存関係から切り離された偽traceを作られる余地がある。

そのためchallengeは、必要に応じて次の連続関係を要求する。

```text
operator input commitment
        ↓
selected matrix tiles / activation slice
        ↓
operator output commitment
        ↓
next operator input commitment
```

この形式を `ExecutionPathChallenge` と呼ぶ。

### 6.4 Checkpoint Commitment

長いpromptや長いoutputでは、任意tokenの検証にprefix全体の再計算が必要になる。

minerは `checkpoint_interval` ごとに、canonical KV stateまたはprofileで定義されたreplay stateをcommitする。

ただしcheckpoint自体を無条件に信用してはならない。challengeには次を混在させる。

1. `LocalRangeChallenge`: checkpointから短い範囲を再計算
2. `CheckpointAncestryChallenge`: 前checkpointとの接続を検査
3. `FullPrefixReplayChallenge`: promptまたはgenesis checkpointから再計算

full replayは高コストだが、一定確率で必須とする。安い検証だけにすると、攻撃者も安い部分だけ正しく作る。人類は試験範囲が分かるとそこしか勉強しないため、minerも同じである。

---

## 7. PALW Commit

### 7.1 `PALWCommitV1`

```rust
struct PALWCommitV1 {
    chain_id: Hash32,
    proof_id: Hash32,
    job_id: Hash32,
    algo_id: u32,
    profile_id: Hash32,
    model_id: Hash32,
    weights_hash: Hash32,
    tokenizer_hash: Hash32,
    runtime_profile_hash: Hash32,
    quantization_profile_hash: Hash32,
    canonicalization_version: u32,
    input_hash: Hash32,
    prompt_length: u32,
    output_length: u32,
    output_hash: Hash32,
    token_root: Hash32,
    activation_root: Hash32,
    trace_root: Hash32,
    checkpoint_root: Hash32,
    da_root: Hash32,
    proof_manifest_hash: Hash32,
    miner_pubkey: PublicKey,
    base_bond_id: Hash32,
    work_bond_amount: u128,
    commit_daa_score: u64,
    latest_anchor_daa_score: u64,
    nonce: u64,
    signature: Signature,
}
```

### 7.2 Commit Acceptance Rules

commit受理時に、chainは次を検査する。

- profileがactiveである。
- signatureが有効である。
- input/jobが有効である。
- work bondが必要額以上lockされている。
- base bondがunbonding中でない。
- `proof_id` が重複していない。
- manifest size、leaf count、output lengthがprofile上限内である。
- DA availabilityの最低条件を満たす。
- commitがanchor deadline前である。

commit受理後、minerはcommitを無償で取り消せない。

---

## 8. Challenge Randomness

### 8.1 Seed

challenge seedは、commit後のfinalized DAG情報を用いる。

```text
R = H(
    "MISAKA/PALW/CHALLENGE/V1"
    || chain_id
    || proof_id
    || proof_manifest_hash
    || commitment_root
    || finalized_future_anchor_hash
    || anchor_daa_score
    || epoch_randomness
)
```

### 8.2 Anchor Requirements

- anchorはcommit時点で未知でなければならない。
- arbitrary tip hashではなく、protocolが定めるfinalizedまたは十分なconfidenceを持つDAG anchorを使う。
- anchor reorg可能期間中はchallengeを確定しない。
- challenge確定後にanchorが例外的に無効化された場合、slashを自動実行せずprofile/proofを凍結する。

### 8.3 Grinding Resistance

minerが複数commitを投げ、都合のよいchallengeだけ応答する攻撃を防ぐため、各commitごとにwork bondをlockする。

未応答commitは `NON_RESPONSE` としてslash対象になる。

### 8.4 Challenge Types

```rust
enum ChallengeTypeV1 {
    TokenLogits,
    HiddenProjection,
    ActivationSlice,
    GemmTile,
    ExecutionPath,
    LocalRangeReplay,
    CheckpointAncestry,
    FullPrefixReplay,
    DataAvailability,
}
```

### 8.5 Sampling

同一dimension内では原則として重複なしで抽選する。

```text
positions = SampleWithoutReplacement(PRF(R, domain), population, q)
```

verifierごとに、

- `shared_positions`: 全verifierが再計算する共通位置
- `private_positions`: verifier pubkeyをseedへ加えた個別位置

を割り当てる。

共通位置はquorum形成に使い、個別位置はコピー検証とcoverage拡大に使う。

---

## 9. Challenge数と検出確率

不正が全leafの割合 `f` に影響し、独立に `q` 箇所を検査すると近似する場合、

```math
P_{detect} = 1 - (1-f)^q
```

複数dimensionを検査する場合の近似は、

```math
P_{spot} = 1 - (1-f_t)^{q_t}(1-f_a)^{q_a}(1-f_g)^{q_g}(1-f_p)^{q_p}
```

- `f_t`: 不正token比率
- `f_a`: 不正activation比率
- `f_g`: 不正GEMM tile比率
- `f_p`: 不正execution path比率

full replay確率を `P_full` とすると、

```math
P_{total} = 1 - (1-P_{spot})(1-P_{full})
```

ただし、実際の不正箇所は独立とは限らない。bond sizingには、式の理論値ではなくred-teamで観測した最悪ケースの下限を用いる。

### 9.1 Risk Tier

初期提案:

| Tier | 典型用途 | Primary Verifiers | Shared token/range | Activation | GEMM tile | Execution path | Full prefix replay |
|---|---|---:|---:|---:|---:|---:|---:|
| L | 小額reward、shadow | 3 | 4 | 4 | 8 | 1 | 1/16 proof |
| M | 通常PALW reward | 5 | 8 | 8 | 16 | 2 | 1/8 proof |
| H | 高額reward、DNS weight | 7 | 16 | 16 | 32 | 4 | 1/2 proof |
| C | dispute/appeal | 13 | dispute依存 | dispute依存 | dispute依存 | 8以上 | 原則必須 |

この表はconsensus永久値ではなく、testnet開始用の提案値である。

### 9.2 Dynamic q

`q` は少なくとも以下から決定する。

- work reward
- work bond
- estimated cheating gain
- model size
- context/output length
- runtime risk score
- operator reputation
- profile conformance history
- recent mismatch rate

高reputationだからchallengeをゼロにすることは禁止する。長く正直だった参加者が、その信用を一度だけ換金する攻撃を防ぐためである。

---

## 10. Data Availability

commitmentだけ存在し、openingに必要なデータが取得できなければ検証不能である。

### 10.1 ProofDataManifest

```rust
struct ProofDataManifestV1 {
    proof_id: Hash32,
    schema_version: u32,
    chunk_count: u32,
    chunk_size: u32,
    erasure_coding_profile: u32,
    data_root: Hash32,
    chunk_roots_root: Hash32,
    token_leaf_count: u32,
    activation_leaf_count: u32,
    trace_leaf_count: u32,
    checkpoint_leaf_count: u32,
    retention_until_daa_score: u64,
}
```

### 10.2 Availability Rules

- minerはcommit前に必要chunkをcontent-addressed storageへ配置する。
- commit時に最低数のavailability attestationを要求できる。
- challenge対象chunkを期限内に配信できない場合、objective `DATA_WITHHOLDING` offenseとなる。
- dispute window終了前にデータを削除してはならない。
- verifierは取得したchunk hashを記録する。

### 10.3 DA Griefing Protection

verifierが「取得できなかった」と虚偽申告する攻撃を防ぐため、取得failureは複数source、request receipt、time proof、backup fetcherによって確認する。

単一verifierの申告だけでDATA_WITHHOLDING slashを実行してはならない。

---

## 11. Miner Response

### 11.1 `PALWChallengeResponseV1`

```rust
struct PALWChallengeResponseV1 {
    proof_id: Hash32,
    challenge_id: Hash32,
    miner_pubkey: PublicKey,
    response_manifest_hash: Hash32,
    openings_root: Hash32,
    response_data_root: Hash32,
    submitted_daa_score: u64,
    signature: Signature,
}
```

response dataは各challengeについて以下を含む。

- challenged leaf preimage
- Merkle path
- tensor shape、index、dtype metadata
- canonical serialization bytesまたはそのcontent hash
- operator dependency metadata
- checkpoint parent proof
- profile hash

### 11.2 Immediate Objective Checks

chainまたはlightweight settlement nodeは次を即時検査できる。

- Merkle pathがcommit rootへ到達するか
- leaf indexがchallengeと一致するか
- preimage schemaが正しいか
- profile/versionが一致するか
- deadline内か
- response hash/signatureが一致するか

これらに失敗した場合、再計算なしでobjective offenseを成立させられる。

---

## 12. Verifier Assignment

### 12.1 Selection

verifier selectionは以下を組み合わせる。

```text
VRF randomness
+ capped bond weight
+ PALW verification reputation
+ availability score
+ operator diversity constraints
```

PALW contribution量だけをselection weightにしてはならない。

### 12.2 Exclusion Rules

最低限、次を禁止する。

- miner pubkeyと同一operator groupのverifier
- 同一proofで同じwithdrawal keyを持つ複数verifier
- appeal juryへのprimary verifierの重複参加
- active suspension中のverifier
- profile未対応verifier

off-chain operator同一性を完全判定できないため、これらはSybil対策の一部にすぎない。最終的な抑止はbondとランダム選択に依存する。

### 12.3 Quorum Defaults

推奨初期値:

- Tier L: `2-of-3`
- Tier M: `4-of-5`
- Tier H: `5-of-7`
- Appeal: `9-of-13`

quorumは単なるPASS/FAIL票ではなく、同一のcanonical expected hashまたは同一fault classへの一致を要求する。

---

## 13. Verifier Commit-Reveal

### 13.1 Verifier Commit

verifierは他者の結果公開前に次を提出する。

```text
verifier_commit = H(
    "MISAKA/PALW/VERIFIER_COMMIT/V1"
    || proof_id
    || challenge_id
    || verdict
    || shared_result_root
    || private_result_root
    || execution_manifest_hash
    || nonce
)
```

### 13.2 Verifier Reveal

```rust
struct VerifierRevealV1 {
    proof_id: Hash32,
    challenge_id: Hash32,
    verifier_pubkey: PublicKey,
    verdict: Verdict,
    shared_result_root: Hash32,
    private_result_root: Hash32,
    expected_leaf_hashes_root: Hash32,
    observed_leaf_hashes_root: Hash32,
    execution_manifest_hash: Hash32,
    runtime_attestation_hash: Option<Hash32>,
    nonce: [u8; 32],
    signature: Signature,
}
```

commitとrevealが一致しない場合はobjective equivocation/malformed revealとなる。

### 13.3 Verdict

```rust
enum Verdict {
    Pass,
    FailObjective,
    FailComputation,
    DataUnavailable,
    Indeterminate,
}
```

`Indeterminate` は、canonicalizationやruntimeの異常を疑う場合に使用する。Indeterminateを出しただけでslashしてはならないが、乱用にはreward減額とassignment score低下を適用できる。

---

## 14. 判定フロー

```text
COMMITTED
   ↓ future anchor
CHALLENGED
   ↓ miner response
RESPONDED
   ↓ verifier commit-reveal
   ├── quorum PASS ───────────────→ PROVISIONAL_PASS
   ├── objective fault ───────────→ PROVISIONAL_FAIL_OBJECTIVE
   ├── quorum same compute fault ─→ PROVISIONAL_FAIL_COMPUTE
   └── no quorum / mixed result ──→ DISPUTED

PROVISIONAL_* 
   ↓ dispute window
   ├── no valid appeal → FINALIZED
   └── valid appeal    → APPEAL_JURY

APPEAL_JURY
   ├── 9-of-13 PASS → PASS_FINAL
   ├── 9-of-13 same FAIL → FAIL_FINAL
   └── no quorum/profile anomaly → PROFILE_QUARANTINE + BOND_FREEZE
```

### 14.1 PASS

PASS成立条件:

- required quorumを満たす。
- shared challengeのcanonical result rootが一致する。
- private challengeに重大faultがない。
- DA failureがない。
- dispute windowを経過する。

### 14.2 FAIL_OBJECTIVE

次のようなchain上で再現可能な証拠に基づく。

- invalid Merkle opening
- response schema mismatch
- signed equivocation
- deadline超過
- commit/reveal mismatch
- work bond不足または二重使用
- challenge対象データの確定的な非提供

### 14.3 FAIL_COMPUTATION

複数verifierが同一profileで再計算し、同じcanonical expected hashへ一致し、miner openingと不一致である場合に成立候補となる。

初期段階では、primary quorumだけでBaseBondを不可逆slashしてはならない。少なくともdispute windowとappeal pathを経る。

### 14.4 No Quorum

no quorumはminerの有罪を意味しない。

- rewardとbondを一時凍結する。
- backup verifierまたはappeal juryへ移行する。
- profile-wide mismatchが疑われる場合はcircuit breakerを評価する。

---

## 15. Fraud Proof

### 15.1 Objective Fraud Proof

```rust
struct ObjectiveFraudProofV1 {
    proof_id: Hash32,
    challenge_id: Hash32,
    offense_type: ObjectiveOffense,
    signed_objects_root: Hash32,
    merkle_evidence_root: Hash32,
    deadline_evidence: Option<DeadlineEvidence>,
    challenger_pubkey: PublicKey,
    challenge_bond_id: Hash32,
    signature: Signature,
}
```

objective proofは、第三者がLLMを再計算せず検証できなければならない。

### 15.2 Computation Fault Certificate

```rust
struct ComputationFaultCertificateV1 {
    proof_id: Hash32,
    challenge_id: Hash32,
    profile_id: Hash32,
    fault_class: ComputationFaultClass,
    challenged_positions_root: Hash32,
    miner_observed_results_root: Hash32,
    verifier_expected_results_root: Hash32,
    verifier_execution_manifests_root: Hash32,
    agreeing_verifier_bitmap: BitVec,
    quorum_signatures: AggregateSignature,
    evidence_data_root: Hash32,
}
```

chainはLLM arithmetic自体を再実行せず、次を検証する。

- 選出verifierの署名
- quorum閾値
- challenge/profile/proofとの対応
- commit-reveal整合性
- evidence data availability

この方式はbonded verifier quorumへの信頼を含む。したがってappeal、watcher、profile quarantine、verifier slashingが必要である。

---

## 16. Appeal

### 16.1 Appeal Eligibility

以下がappealできる。

- miner
- primary verdictと異なる結果を得たverifier
- 十分なchallenge bondをlockしたwatcher
- profile monitor

### 16.2 Appeal Requirements

appealは最低限、次を含む。

- disputed challengeの特定
- primary verdictに対する具体的な反証
- reference runtime execution manifest
- expected canonical result root
- appeal bond

単なる「結果に納得できない」はappealではない。chainは感情相談窓口ではないためである。

### 16.3 Appeal Jury

- primary verifierと重複しない。
- minerとoperator groupが重複しない。
- 原則13 verifier、9一致で確定する。
- high-risk caseでは複数reference runtimeを要求する。
- full prefix replayまたはより深いexecution pathを必須とする。

### 16.4 Appeal Outcome

| Outcome | Miner | Primary verifiers | Appellant |
|---|---|---|---|
| Primary upheld | original penalty | reward | appeal bond一部slash |
| Primary overturned | bond解放、reward回復 | false verdict penalty | bond返却+bounty |
| Profile anomaly | bond凍結、no slash | no slash | bond返却 |
| No quorum | quarantine継続 | reward保留 | bond返却または一部返却 |

---

## 17. Offense Taxonomy

### 17.1 Miner Objective Offenses

| Code | Offense | Evidence | Default Severity |
|---|---|---|---|
| `M-O1` | Invalid Merkle opening | root/path mismatch | Critical |
| `M-O2` | Signed equivocation | same proof_idで異なるsigned commit/response | Critical |
| `M-O3` | Response non-delivery | deadline proof | Major |
| `M-O4` | Data withholding | multi-source availability proof | Major/Critical |
| `M-O5` | Malformed proof after acceptance | schema/profile mismatch | Major |
| `M-O6` | Bond double-use | on-chain exposure proof | Critical |
| `M-O7` | Challenge cancellation/grinding | bonded commitの選択的放棄 | Major |

### 17.2 Miner Computation Offenses

| Code | Offense | Evidence | Default Severity |
|---|---|---|---|
| `M-C1` | Wrong canonical logits | final appeal quorum | Critical |
| `M-C2` | Wrong activation/hidden state | final appeal quorum | Critical |
| `M-C3` | Invalid GEMM/operator trace | final appeal quorum | Critical |
| `M-C4` | Broken checkpoint ancestry | final appeal quorum | Critical |
| `M-C5` | Partial execution / fabricated path | correlated multi-position fault | Critical |

### 17.3 Verifier Offenses

| Code | Offense | Evidence | Default Severity |
|---|---|---|---|
| `V-O1` | Commit without reveal | deadline proof | Minor/Major |
| `V-O2` | Commit-reveal mismatch | hash mismatch | Critical |
| `V-O3` | Equivocation | conflicting signatures | Critical |
| `V-C1` | False PASS | appeal overturn + fault evidence | Critical |
| `V-C2` | False FAIL | appeal overturn + valid miner proof | Critical |
| `V-C3` | Recompute omission / copied result | private challenge failure、audit | Major/Critical |
| `V-C4` | Repeated Indeterminate abuse | statistical policy evidence | Minor/Major |

### 17.4 Challenger Offenses

| Code | Offense | Evidence | Default Severity |
|---|---|---|---|
| `C-O1` | Invalid objective proof spam | deterministic rejection | Major |
| `C-C1` | Baseless computation challenge | appeal jury rejection | Minor/Major |
| `C-O2` | Evidence withholding | missing committed evidence | Major |
| `C-O3` | Equivocating appeal data | conflicting signatures | Critical |

---

## 18. Slash Policy

### 18.1 原則

1. reward forfeitureとbond slashを分ける。
2. 最初の単発faultでは、原則WorkBondを先にslashする。
3. BaseBond slashは、critical objective offense、appealで確定したcomputation offense、repeat offenseに限定する。
4. profile anomaly時はslashせずfreezeする。
5. penaltyはoffense発生時にlockされていたbondを上限とし、事後的な無限債務を作らない。
6. protocol bugによるmass slashを防ぐためcircuit breakerを設ける。

### 18.2 推奨初期Penalty

以下はtestnetから初期mainnet移行時の提案値であり、実測により調整する。

#### Miner

| Offense | Reward | WorkBond | BaseBond | Other |
|---|---:|---:|---:|---|
| `M-O1` invalid opening | 100%没収 | 100% | strikeに応じ0〜25% | suspension |
| `M-O2` equivocation | 100%没収 | 100% | 25%〜100% | 長期suspension |
| `M-O3` non-response | 100%没収 | 50% | repeat時2〜10% | availability低下 |
| `M-O4` data withholding | 100%没収 | 75〜100% | repeat時10〜25% | DA suspension |
| `M-O5` malformed | 100%没収 | 25〜50% | repeat時2〜10% | profile ban |
| `M-O6` bond double-use | 100%没収 | 100% | 25〜100% | identity suspension |
| `M-C1`〜`M-C5` compute fraud | 100%没収 | 100% | 1回目5%、2回目25%、3回目100% | reputation reset |

#### Verifier

| Offense | Verifier Reward | Assignment Bond | BaseBond | Other |
|---|---:|---:|---:|---|
| `V-O1` no reveal | 100%没収 | 10〜25% | repeat時2% | selection score低下 |
| `V-O2` mismatch | 100%没収 | 100% | 10〜25% | suspension |
| `V-O3` equivocation | 100%没収 | 100% | 25〜100% | long suspension |
| `V-C1` false PASS | 100%没収 | 100% | 10〜100% | reputation reset |
| `V-C2` false FAIL | 100%没収 | 100% | 10〜100% | reputation reset |
| `V-C3` copied/no compute | 100%没収 | 50〜100% | repeat時25% | private challenge増加 |

#### Challenger

| Offense | Challenge Bond | Other |
|---|---:|---|
| invalid objective proof | 100% | rate limit |
| rejected computation appeal | 25〜100% | reputation低下 |
| evidence withholding | 100% | suspension |
| profile anomalyで不成立 | 0% | 全額返却 |

### 18.3 Repeat-Offense Escalation

strikeはprofile単位とoperator全体の双方で管理する。

例:

```text
base_slash_rate(k) = min(100%, 5% × 5^(k-1))
```

- 1回目: 5%
- 2回目: 25%
- 3回目: 100%

ただしobjective equivocationやbond double-useは初回からより重く扱う。

strikeは一定期間で減衰できるが、完全消去ではなく長期履歴をreputationへ残す。

### 18.4 Slash Distribution

self-challenge bounty farmingを防ぐため、slashed amountの大半をchallengerへ渡してはならない。

推奨初期配分:

- 50%: protocol insurance / security reserve
- 20%: burn
- 20%: honest verifierおよびappeal jury
- 10%: successful challenger bounty

reward forfeitureは別枠で、verification cost補填とPALW reserveへ配分する。

bountyがslash額より十分小さいため、minerが別identityで自分をchallengeしても純利益にならない。

---

## 19. Circuit Breaker

### 19.1 必要性

canonicalizer、driver、runtime、model profileに共通bugがあると、多数の正直minerが同時にFAILする可能性がある。

その状況で自動slashを続ける設計は、攻撃者より先にprotocol自身が参加者を破壊する。

### 19.2 Trigger

次のいずれかでprofile単位のslashを自動凍結する。

- 異なるoperator間でmismatch率が閾値を超えた。
- 複数reference runtime間で同一inputのcanonical resultが分岐した。
- appeal juryが規定回数連続でno quorumとなった。
- 同一software/driver versionへfailureが集中した。
- challenge anchorまたはDA層にchain-wide anomalyが検出された。

初期提案:

```text
profile_mismatch_rate > 0.5% over 1,000 challenges
OR
3 consecutive appeal no-quorum events
OR
2 reference runtimes disagree on any slash-critical leaf
```

### 19.3 Effect

circuit breaker発動時:

- 新規PALW commitを停止またはreward-onlyへ降格する。
- pending bondを解放せずfreezeする。
- 新規slashを実行しない。
- BlockDAG進行とhash PoW floorは継続する。
- DNS/finality weightへの新規PALW反映を停止する。
- profile versionをquarantineする。

修正版profileは新しい`profile_id`として登録し、旧profileを上書きしてはならない。

---

## 20. PALW Credit・Reward・DNS連携

### 20.1 Delayed Settlement

PALW rewardはcommit時に確定させずescrowへ置く。

```text
commit
  ↓
challenge
  ↓
primary verification
  ↓
provisional credit
  ↓ dispute window
final settlement
```

### 20.2 Provisional Credit

Stage 2以降、primary PASS後に限定的なprovisional PALW creditを付与できる。

制約:

- finalization前のcreditにはcapを設ける。
- disputeでFAILした場合はrevertする。
- provisional creditだけでchain validityを決めない。
- 同一operatorが未精算proofを大量に積み上げる場合、exposure capを適用する。

### 20.3 DNS / Finality Weight

PALW結果をDNS/finalityへ反映する場合:

- PASS_FINALのみをfull weight対象とする。
- PROVISIONAL_PASSは最大でも小さいcap付きweightとする。
- disputed、quarantined、expired proofはweight 0とする。
- later slash時は将来weightとreputationを削減する。
- 過去finalized DAGを通常の単発PALW slashで巻き戻さない。

PALWをいきなりchain validityの絶対条件にしない。

---

## 21. Rollout Stages

### Stage 0: Shadow

- bondは記録するがslashしない。
- verifier disagreement、hardware差、検証コスト、DA failureを計測する。
- reward、DNS weight、fork-choiceへ影響させない。

Exit Criteria例:

- 100万以上のchallenge leafで未説明canonical mismatchが0件
- target runtime matrixで再現性が確認済み
- false positive/false negative attack simulation完了
- p99 verification costと時間がprofile budget内

### Stage 1: Reward-Only + Objective Slash

- invalid Merkle opening、equivocation、non-response、bond double-useのみslash可能。
- computation mismatchはreward保留・profile quarantine対象だが、BaseBondをslashしない。
- PALW rewardはPASS_FINAL後に支払う。

### Stage 2: Bounded Computation Slash

- appealで確定したcomputation faultにWorkBond slashを適用する。
- BaseBond slashは最大5〜10%へcapする。
- DNS weightはcap付きで反映する。
- circuit breakerを必須化する。

### Stage 3: Full Economic Security

- 実測 `P_detect` に基づくWorkBond sizingを有効化する。
- repeat offenseでBaseBond全額slashを可能にする。
- PALW creditをDNS/finality securityへより強く反映する。
- それでもhash PoW floorとfallback pathを残す。

---

## 22. State Machine

```rust
enum PALWProofState {
    Registered,
    BondLocked,
    Committed,
    AnchorPending,
    Challenged,
    ResponsePending,
    Responded,
    VerifierCommitPending,
    VerifierRevealPending,
    ProvisionalPass,
    ProvisionalFailObjective,
    ProvisionalFailComputation,
    Disputed,
    AppealPending,
    PassFinal,
    FailFinal,
    Expired,
    Quarantined,
    Settled,
}
```

### 22.1 Valid Transitions

```text
Registered -> BondLocked -> Committed -> AnchorPending -> Challenged
Challenged -> ResponsePending -> Responded
Responded -> VerifierCommitPending -> VerifierRevealPending
VerifierRevealPending -> ProvisionalPass | ProvisionalFail* | Disputed
Provisional* -> PassFinal | FailFinal | AppealPending
AppealPending -> PassFinal | FailFinal | Quarantined
PassFinal -> Settled
FailFinal -> Settled
Expired -> Settled
Quarantined -> PassFinal | FailFinal | administrative profile retirement
```

invalid state transitionは受理しない。

---

## 23. 推奨Timing Parameters

時刻はwall clockではなく、DAGのDAA scoreまたはprotocol-defined epochで管理する。

名目10 BPS時の参考値:

| Parameter | DAA score差 | 約時間 | Note |
|---|---:|---:|---|
| future anchor delay | 100 | 10秒 | finalized confidenceは別途必要 |
| miner response window | 3,000 | 5分 | model tierで延長可 |
| verifier commit window | 6,000 | 10分 | full replay tierは延長 |
| verifier reveal window | 1,200 | 2分 | commit後 |
| primary dispute window | 18,000 | 30分 | watcher参加 |
| appeal window | 36,000 | 60分 | high-riskは延長 |
| proof DA retention | 864,000以上 | 24時間以上 | disputeより長くする |
| base bond unbonding | protocol epoch | 7日以上 | 高価値は14〜30日 |

DAG liveness異常時にdeadlineをどう扱うかは明示する。推奨は、DAA scoreが進まなければdeadlineも進めず、wall-clockのみでslashしないことである。

---

## 24. Slash Execution Algorithm

```text
function finalize_offense(offense):
    assert offense.state == FINAL
    assert dispute_window_closed(offense)
    assert not circuit_breaker_active(offense.profile_id)

    if offense.class == OBJECTIVE:
        assert verify_objective_evidence(offense.evidence)
    else:
        assert verify_fault_certificate_quorum(offense.certificate)
        assert appeal_complete_or_expired(offense)

    penalty = calculate_penalty(
        offense_type,
        work_bond,
        base_bond,
        strike_count,
        profile_stage,
        risk_tier
    )

    penalty = min(penalty, locked_exposure)

    forfeit_pending_reward(offense.proof_id)
    slash_work_bond(penalty.work)
    slash_base_bond(penalty.base)
    update_reputation(offense.operator, offense)
    distribute_slash(penalty.total)
    emit SlashFinalized(...)
```

### 24.1 Idempotency

同じoffense/evidenceによる二重slashを禁止する。

```text
slash_id = H(proof_id || challenge_id || offense_type || evidence_root)
```

`slash_id` は一度しかfinalizeできない。

### 24.2 Correlated Offense

同一proofに複数faultがある場合、無制限にpenaltyを加算しない。

- reward forfeitureは1回のみ。
- WorkBond slashは最大100%。
- BaseBond slashは最も重いoffenseを基準にし、明示的なequivocation等のみ加算する。

---

## 25. Events

```rust
PALWProfileActivated(profile_id)
PALWBaseBondRegistered(operator, amount)
PALWWorkBondLocked(proof_id, amount)
PALWCommitAccepted(proof_id, commitment_root)
PALWChallengeCreated(proof_id, challenge_id, seed_hash)
PALWVerifierAssigned(proof_id, verifier)
PALWResponseSubmitted(proof_id, response_root)
PALWVerifierCommitted(proof_id, verifier, commit_hash)
PALWVerifierRevealed(proof_id, verifier, verdict)
PALWProvisionalVerdict(proof_id, verdict)
PALWAppealOpened(proof_id, appeal_id)
PALWProfileQuarantined(profile_id, reason)
PALWSlashFrozen(proof_id, reason)
PALWSlashFinalized(slash_id, operator, amount, offense)
PALWRewardReleased(proof_id, amount)
PALWDNSCreditUpdated(operator, delta)
```

すべてのslash-critical eventは、監査可能なcontent hashと署名参照を持つ。

---

## 26. 攻撃分析

### 26.1 Final Output Shortcut

**攻撃:** final tokenやlogitsだけを外部から取得する。  
**対策:** hidden state、activation、GEMM trace、execution path、checkpoint ancestryを同時commitし、post-commit challengeする。

### 26.2 Partial Computation

**攻撃:** 一部token/layerのみ正しく計算する。  
**対策:** multi-dimensional sampling、private challenge、full prefix replay、dynamic q、十分なWorkBond。

### 26.3 Cache / Precomputation

**攻撃:** inputを予測して事前計算する。  
**対策:** inputへfuture chain challenge、nonce、job-specific randomnessを含める。再利用可能なproof materialをdomain-separated hashで無効化する。

### 26.4 Commitment Grinding

**攻撃:** 多数commitから有利なchallengeだけ選ぶ。  
**対策:** commitごとのbond lock、未応答slash、operator exposure cap。

### 26.5 Verifier Copying

**攻撃:** 他verifierの結果をコピーする。  
**対策:** commit-reveal、verifier-specific private positions、execution manifest、random audit。

### 26.6 Miner-Verifier Collusion

**攻撃:** minerと選出verifierが共謀する。  
**対策:**ランダムassignment、operator exclusion、bond、watcher appeal、appeal jury、private challenge、selection concentration cap。

### 26.7 False FAIL Griefing

**攻撃:** verifierが正しいminerをFAIL扱いする。  
**対策:** primary quorum、appeal、false verdict slash、profile anomaly freeze。

### 26.8 False PASS

**攻撃:** verifierが不正proofをPASSする。  
**対策:** shared/private challenge、secondary random audit、watcher fraud proof、appeal overturn時の重いslash。

### 26.9 DA Withholding

**攻撃:** commit後にtrace/opening dataを隠す。  
**対策:** pre-commit DA、availability attestation、retention、multi-source fetch evidence、objective slash。

### 26.10 Hardware Divergence

**攻撃ではない事故:** 正直なruntimeが異なるcanonical resultを出す。  
**対策:** profile conformance、reference runtimes、Indeterminate verdict、circuit breaker、Stage 1ではcompute mismatchをBaseBond slashしない。

### 26.11 Mass Software Bug

**事故:** canonicalizerやadapter bugで多数FAIL。  
**対策:** profile-level anomaly detector、slash freeze、immutable profile versioning、rollbackではなくnew profile activation。

### 26.12 Self-Challenge Bounty Farming

**攻撃:** minerが別identityで自分の不正をchallengeしbountyを回収する。  
**対策:** bountyをslash額の小割合に制限し、reward没収・burn・insurance配分を大きくする。

### 26.13 Unbond Before Discovery

**攻撃:** 不正後すぐunbondする。  
**対策:** dispute期間を上回るunbonding delay、active exposureの解放禁止。

### 26.14 Challenge Censorship

**攻撃:** block producerがappeal transactionを検閲する。  
**対策:** dispute windowを十分長くし、複数submission route、inclusion listまたはcensorship proofを将来導入する。現時点で完全解決とはしない。

---

## 27. 実装上の禁止事項

- runtime内部へslash条件を直接埋め込まない。
- raw FP16/FP32 tensorをそのままconsensus hashにしない。
- 1 verifierの判定だけでcompute fraud slashを確定しない。
- 1 token spot checkだけを固定使用しない。
- PALW量だけでverifier selectionを決めない。
- challenge前に検査位置をminerへ漏らさない。
- response dataが取得不能なcommitをreward対象にしない。
- primary verdict直後にBaseBondを不可逆slashしない。
- profile bugが疑われる状態で自動slashを継続しない。
- PALW failureを初期段階からBlockDAG validityへ直結しない。
- 同一証拠で複数回slashしない。
- governance操作で過去profileのhash規則を上書きしない。

---

## 28. テスト計画

### 28.1 Canonicalization Matrix

各profileについて次を最低100万challenge単位で比較する。

- H100 / A100 / RTX系
- CUDA driver差
- vLLM version差
- llama.cpp backend差
- TensorRT-LLM
- ROCm
- CPU reference
- quantization profile差

測定項目:

- bit-exact canonical match率
- mismatch tensor/layer分布
- runtime upgrade前後差
- NaN/Inf/subnormal処理
- round-to-nearest-even実装差

### 28.2 Adversarial Miner Tests

- final outputのみ正しい
- token 1%だけ不正
- layer 1つだけ不正
- GEMM tile 0.1%だけ不正
- checkpoint捏造
- traceだけ別executionから流用
- DA chunk一部欠損
- commit grinding
- challenge後の選択的応答

各攻撃について、実測 `P_detect` と経済損益を算出する。

### 28.3 Adversarial Verifier Tests

- 結果コピー
- commit後のreveal放棄
- false PASS
- false FAIL
- primary quorum collusion
- appeal jury一部collusion
- private challenge無視
- execution manifest捏造

### 28.4 Slashing Safety Tests

- 同一slashのreplay
- reorg中のslash
- circuit breaker発動境界
- profile deactivation中のpending proof
- unbonding race
- integer overflow
- aggregate signature bitmap spoofing
- duplicate verifier assignment
- offense重複による過剰slash

### 28.5 Chaos Tests

- DA node 30%停止
- verifier 40%offline
- GPU runtime crash
- network partition
- delayed blocks / DAA停止
- profile-wide canonical mismatch
- appeal transaction混雑

---

## 29. Mainnet Activation Gates

computation faultによるBaseBond slashを有効化する前に、最低限次を満たす。

1. reference canonicalizerが独立実装2系統以上で一致する。
2. 対象hardware/runtime matrixで未説明mismatchが0件である。
3. 100万以上のchallengeに対するfalse slashが0件である。
4. red-teamで主要shortcut、partial execution、DA withholdingを評価済みである。
5. WorkBond式に使う `P_detect` の保守的下限が実測されている。
6. appeal juryの独立性と選出集中度が監視できる。
7. circuit breaker、bond freeze、profile quarantineをchaos test済みである。
8. slash moduleとstate transitionが独立監査済みである。
9. rewardだけでなくverification costを含めた持続可能性が確認されている。
10. hash PoW floorおよびPALW停止時fallbackが動作する。

---

## 30. 初期推奨設定

実装開始時の安全側設定:

```text
Rollout stage                  = Stage 0 → Stage 1
Primary verifier set          = 5 (Tier M), 7 (Tier H)
Appeal jury                    = 13
Primary quorum                = 4/5 or 5/7
Appeal quorum                 = 9/13
Compute mismatch BaseBond     = 0% at Stage 1
Objective WorkBond slash      = enabled
Non-response WorkBond slash   = 50%
Invalid opening WorkBond      = 100%
Equivocation BaseBond slash   = 25% minimum
Full prefix replay            = 1/8 normal, 1/2 high-risk
Challenge bounty              = 10% of slash maximum
Unbonding delay               = 7 days minimum
Profile anomaly               = freeze, not slash
PALW chain validity coupling  = disabled
DNS provisional weight cap    = low or disabled until Stage 2
```

---

## 31. 残る研究課題

### 31.1 Soundness Bound

multi-dimensional samplingで、特定shortcutがproof materialの何%へ影響するかを形式化する必要がある。`f` を攻撃者が任意に極小化できる場合、spot checkingだけではbondを現実的な額にできない。

### 31.2 Checkpoint Trust

KV/checkpointを使うとverificationは安くなるが、checkpoint自体の正当性をどう効率的に検査するかが残る。full prefix replay頻度とcheckpoint ancestry samplingの最適化が必要である。

### 31.3 Cross-Hardware Canonicalization

integer canonicalizationを定義しても、量子化境界付近の値がhardware差で別integerへ落ちる可能性がある。profile activation前の大規模実測が必須である。

### 31.4 Verifier Collusion Bound

Sybil resistance、operator independence、bond concentration、verifier selectionの安全仮定を定量化する必要がある。

### 31.5 Useful Work Definition

PALW securityを外部user query需要へ依存させると、需要の偏りや自家発注でsecurityが変動する。consensus challenge由来のinput、canary、固定work laneを混在させる設計が必要である。

### 31.6 Formal Economics

`G_max`、`P_detect`、reputation価値、bribe上限を含むgame-theoretic modelをsimulationし、risk tierごとのbondとqを最適化する必要がある。

---

## 32. 最終提案

MISAKA PALWへslashを組み込む場合、中心となる設計は次である。

```text
Runtime-independent PALW proof engine
    + canonical integer commitments
    + logits / activation / GEMM / checkpoint roots
    + post-commit future-DAG challenge
    + shared and verifier-private spot checks
    + verifier commit-reveal
    + miner / verifier / challenger bonds
    + objective slash and appealed computation slash separation
    + delayed settlement
    + profile-level circuit breaker
    + staged DNS/finality integration
```

最も重要な安全原則は、次の3点である。

1. **計算不一致と客観的不正を同じslash経路に入れない。**
2. **slash前にappealとprofile anomaly判定を置く。**
3. **PALWが壊れてもBlockDAG全体が停止しないfallbackを残す。**

PALW-Sは「不正を絶対に不可能にする」方式ではない。commit後の予測不能な検査と、逃げ切れないbond exposureによって、合理的な攻撃者にとって不正計算を割に合わなくする方式である。

---

## Appendix A. Minimal Wire Objects

```rust
struct BondLockV1 {
    bond_id: Hash32,
    owner_pubkey: PublicKey,
    bond_type: BondType,
    amount: u128,
    exposure_id: Hash32,
    lock_daa_score: u64,
    unlock_after_daa_score: u64,
    signature: Signature,
}

enum BondType {
    BaseMiner,
    WorkMiner,
    BaseVerifier,
    AssignmentVerifier,
    Challenger,
    Appeal,
}

struct PALWChallengeV1 {
    proof_id: Hash32,
    challenge_id: Hash32,
    anchor_hash: Hash32,
    randomness_hash: Hash32,
    risk_tier: u8,
    shared_positions_root: Hash32,
    verifier_assignment_root: Hash32,
    response_deadline_daa_score: u64,
}

struct SlashDecisionV1 {
    slash_id: Hash32,
    proof_id: Hash32,
    offender_pubkey: PublicKey,
    offense_code: u16,
    evidence_root: Hash32,
    work_bond_slash: u128,
    base_bond_slash: u128,
    reward_forfeit: u128,
    finalization_daa_score: u64,
    decision_certificate: AggregateSignature,
}
```

---

## Appendix B. Decision Matrix

| Evidence class | On-chain arithmetic replay | Primary quorum | Appeal required | BaseBond slash |
|---|---:|---:|---:|---:|
| Invalid signature/equivocation | Yes | No | Optional | Yes |
| Invalid Merkle opening | Yes | No | Short dispute | Yes, severity依存 |
| Deadline/non-response | Yes | No | Short dispute | Repeat時 |
| DA withholding | Partial | Multi-source | Yes for critical | Repeat時 |
| Canonical compute mismatch | No | Yes | Yes | Stage 2以降 |
| Runtime/profile disagreement | No | No quorum | Profile review | No, freeze |
| Verifier false verdict | No | Appeal result | Appeal itself | Yes |

---

## Appendix C. Source-Derived Assumptions vs New Design Choices

### Source-derived core

- runtimeとverification layerの分離
- Merkle commitment後のfuture randomness challenge
- logits単独ではなくactivation、GEMM traceまで深くcommitする方針
- asynchronous verification
- PALWを即時chain validityへ入れない段階導入
- verifier selectionをPALW contributionだけへ依存させない方針
- `P_detect × Slash > Cheating Gain` という経済条件

### This document's proposed additions

- 4種類のbond
- objective offenseとcomputation offenseの分離
- verifier commit-revealおよびprivate challenge
- appeal juryとquorum値
- penalty table
- slash distribution
- profile-level circuit breaker
- concrete state machine、wire objects、timing、rollout gates

これらの追加部分は、実装前にsimulation、testnet計測、独立監査を必要とする。
