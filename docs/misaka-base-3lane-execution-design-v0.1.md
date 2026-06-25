# MISAKA Base + 3 Execution Lanes 統合設計書 v0.1

**副題:** PQ settlement、PQ-EVM、現行ETH互換EVM、proof-verified Parallel ECC-EVMの統合

**版:** v0.1 Draft  
**日付:** 2026-06-25  
**対象ソース:** `misakas-main (17).zip` / commit `3d5680081c4afda08f71d728e478d2e0577e76b2`  
**対象:** MISAKA L1の次期execution architecture、hard-fork、node role、state、RPC、移行、試験  
**関連:** `docs/adr/0020-selected-parent-evm-lane.md`、`docs/misaka-evm-design-v0.4.md`、`docs/misaka-evm-optimization-design-v0.1.md`、`docs/misaka-prea-design-v1.1.md`、ADR-0010/0019/0021

> 本書の **MUST / MUST NOT / SHOULD / MAY** は規範語として使用する。数値に「候補」「初期値」と明記したものは、testnet benchmark後に凍結する。

---

## 0. 結論

次の4層を採用候補とする。

```text
Base
  PQ consensus / DAG ordering / native PQ-UTXO / settlement / DA commitments
      │
      └─ Lane 1 — Primary Security Lane（親execution block）
           Solidity-compatible PQ-EVM / ML-DSA-87 native authorization
              ├─ Lane 2 — Compatibility Lane
              │    現在のETH-compatible EVMをstate/historyごと継続
              │
              └─ Lane 3 — Performance Lane
                   SVM/Sui-inspired Parallel ECC-EVM
                   高性能executor/proverのみfull execution
                   通常node/validatorはproofとcommitmentだけ検証
```

この構成は採用に値する。ただし、成立条件は次の5点である。

1. **DAGの線形化はBaseだけが行う。** Lane 2/3が別fork-choiceや別sequencer orderを持ってはならない。
2. **Lane 1の「親ブロック」はexecution anchorの意味とする。** Baseのconsensus parentは従来どおり`selected_parent`であり、Lane 1 stateがBase consensusを逆参照して循環してはならない。
3. **Lane 1とLane 2は同一のEVM engine、state backend、receipt/log schema、block environment、RPC実装を共有する。** 差分はauthorization profile、chain/lane domain、precompile policy、bridge policyに限定する。
4. **Lane 1+2の初期aggregate resource上限は現在の単一EVM上限を大きく超えさせない。** 2レーン化をそのままCPU、RAM、I/O、bandwidthの2倍化にしてはならない。
5. **Lane 3のstate rootは高性能nodeの署名だけで採用しない。** 通常nodeが検証できるvalidity proof、または同等の客観的fraud-proof機構と完全なData Availabilityがproduction assetの前提である。

### 0.1 最終的な役割

| 層 | セキュリティ／目的 | 認証 | 通常full node | 状態 |
|---|---|---|---|---|
| Base | PQ consensus、ordering、settlement | ML-DSA-87 native | 必須 | UTXO・lane registry・escrow |
| Lane 1 | 長期保護、PQ dApp、PQ資産 | ML-DSA-87のみ | 実行必須 | 新規PQ-EVM state |
| Lane 2 | 既存Ethereum UX・dApp互換 | secp256k1 EIP-2718 | 実行必須 | 現在のEVM stateを継続 |
| Lane 3 | 高TPS、ゲーム、NFT、分割可能DeFi | 初期Ed25519 | **実行不要**、proof検証のみ | 新規Parallel EVM/object state |

### 0.2 本書が採用しないもの

- LaneごとにDAGを独自linearizeする方式
- Lane 1/2/3が同一mutable EVM stateを同時更新する方式
- 同期cross-lane `CALL`
- Lane 3 committeeの多数署名だけをstate validityとみなす方式
- 各レーンへ現在の30M gas・128KiB payloadを無条件に丸ごと複製する方式
- 現行のblockごとのfull EVM state snapshot cloneを3レーンへ拡張する方式

---

## 1. 用語と境界

### 1.1 Base Anchor Block

Baseは従来のPoW/DAG/GHOSTDAG、DAA、UTXO、PQ署名、DNS/FSL finality、native supply settlementを担う。Base canonical sequenceは`selected_parent` chainと既存mergeset orderから得る。

```text
B(n-1) → B(n) → B(n+1)
```

Baseのみがcanonical time/orderを決める。execution laneはこれに従属する。

### 1.2 Lane 1の「親ブロック」

本書でLane 1を親ブロックと呼ぶ場合、意味は **Primary Execution Parent** である。

```text
Base anchor B(n)
    └─ PrimaryExecutionBlock L1(n)
         ├─ L2(n) references hash(L1(n))
         └─ L3 batch(n) references hash(L1(n))
```

Lane 1はLane 2/3のexecution epoch、timestamp、message epoch、ruleset anchorを与える。ただしLane 2/3はLane 1のaccount/storage stateを継承しない。各laneは独立state rootを持つ。

### 1.3 Core LaneとOptional Lane

- **Core lanes:** Lane 1とLane 2。通常full nodeおよびvalidatorが実行する。
- **Optional execution lane:** Lane 3。通常nodeはfull stateを持たず、proof、DA status、verified rootのみ検証・保持する。
- **Core node:** Base + Lane 1 + Lane 2を完全検証するnode。
- **Lane 3 executor:** Lane 3 batchを実行する高性能node。
- **Lane 3 prover:** Lane 3 state transition proofを生成するnode。executorと同一でも別でもよい。

### 1.4 セキュリティ表示

Laneごとの表示を曖昧にしてはならない。

```text
Lane 1: PQ-AUTHENTICATED / PQ-SETTLED
Lane 2: CLASSICAL-ECC / ETH-COMPATIBLE
Lane 3: CLASSICAL-ECC / PROOF-VERIFIED / HIGH-PERFORMANCE
```

Lane 1はEVM bytecodeとSolidity ABIを維持するが、Ethereum EOA transaction互換ではない。「Ethereum互換」と表示してはならない。

---

## 2. 現行実装への接地

### 2.1 現行EVMの要点

対象snapshotでは、現行EVMはADR-0020/v0.4に基づき次を実装している。

- `selected_parent`をEVM parentとする
- mergeset delayed acceptance
- `Header::evm_payload_hash`と`Header::evm_commitment_root`
- `EvmExecutionPayload`、`EvmExecutionHeader`
- revm Shanghai executor
- EIP-2718/EIP-1559、secp256k1 sender recovery
- UTXO↔EVM deposit/withdraw
- EVM mempool、RPC、index、snapshot store
- `MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK = 128 KiB`
- `EVM_GAS_LIMIT = 30,000,000`
- F003 ML-DSA-87 verify precompileのコードは存在するが、全networkでactivation fenceが実質inert

主要コード面は次である。

```text
consensus/core/src/evm/mod.rs
consensus/src/processes/evm/mod.rs
kaspa-evm/src/{executor,tx,snapshot,state,mldsa_verify}.rs
mining/src/evm_mempool.rs
rpc/eth/
database/src/registry.rs  (EVM store prefix 201–213)
```

### 2.2 この設計で先に解消する既知問題

multi-lane化前に次を修正しなければならない。

- blockごとの全EVM state deep clone／full snapshot永続化
- state rootの全state再計算
- RPC read/simulationのfull snapshot read・seed
- mergeset候補をgas予算外でも全decode・ECDSA recoveryする経路
- per-tx/per-block auth resource capの明示enforcement不足
- pruning、incremental state、state syncの未完成

これらを残したままLane 1/2を増やすと、現在のI/O問題を複製するだけになる。

### 2.3 移行上の基本判断

- 現行EVMは**Lane 2へ継続**する。state copyやbridge migrationではなく、activation heightで論理的にlane IDを付け替える。
- Lane 1は**新規PQ genesis**から開始する。
- Lane 3は**新規state**から開始する。
- 現行`evm_payload_hash`／`evm_commitment_root`を、future laneごとのfield追加へ拡張せず、versioned multi-lane rootへ一般化する。

---

## 3. 全体アーキテクチャ

### 3.1 Canonical ordering

各Base block `B`について、各laneのaccepted inputは同じmergeset orderから抽出する。

```text
AcceptedLaneTxs_i(B) = concat(
  for X in sorted_mergeset(B):
    X.execution_payloads[lane_id=i].transactions in payload order
)
```

`B`自身のuser payloadは、現行v0.4と同様にselected childでacceptされる。off-by-one規則を全laneで統一する。

### 3.2 State transition

```text
S1(B) = ExecutePQEVM(
  S1(selected_parent(B)),
  Lane1SystemOps(B),
  AcceptedLaneTxs_1(B),
  Env(B)
)

S2(B) = ExecuteEthEVM(
  S2(selected_parent(B)),
  Lane2SystemOps(B),
  AcceptedLaneTxs_2(B),
  Env(B)
)

Input3(B) = CanonicalBatch(
  AcceptedLaneTxs_3(B),
  anchor=B,
  primary_parent=Hash(L1(B))
)

S3(B..B+k) = VerifyProofAndAdvance(previous_verified_root, Input3 range)
```

### 3.3 State isolation

```text
State1 != State2 != State3
Nonce1 != Nonce2 != Nonce3
Storage1 != Storage2 != Storage3
Base escrow is the only native supply bridge
```

account identityは必ず `(lane_id, address_or_account_id)` で解釈する。同じ20-byte aliasが複数laneに存在しても別accountである。

### 3.4 Execution dependency graph

```text
Base B(n)
  │
  ├─ execute Lane 1 synchronously → L1(n)
  │       │
  │       ├─ Lane 2 references L1(n), executes own state synchronously
  │       └─ Lane 3 input references L1(n), result is delayed/proven
  │
  └─ commit roots
```

Lane 1はLane 2/3の同anchor resultを読んではならない。cross-lane inputは最低1 anchor遅延する。これにより循環依存を排除する。

---

## 4. 共通multi-lane protocol

### 4.1 Lane registry

Base consensusにversioned registryを持つ。

```rust
pub struct LaneDescriptor {
    pub lane_id: u16,
    pub ruleset_version: u16,
    pub status: LaneStatus,
    pub execution_kind: ExecutionKind,
    pub auth_profile: AuthProfile,
    pub chain_id: u64,
    pub synchronous: bool,
    pub mandatory_for_core_node: bool,
    pub activation_daa_score: u64,
}
```

初期registry:

```text
1 = PQ_EVM_PRIMARY
2 = ETH_EVM_COMPAT
3 = PARALLEL_ECC_EVM_PROVEN
```

lane IDは再利用してはならない。停止laneもtombstoneとして残す。

### 4.2 Base header commitment

headerにlaneごとのfieldを追加し続けず、v3+で次の2 rootへ一般化する。

```rust
pub struct Header {
    // existing Base fields ...
    pub execution_payloads_root: Hash64,
    pub execution_results_root: Hash64,
}
```

```text
execution_payloads_root = MerkleRoot64(
  sorted_by_lane_id(LanePayloadCommitment)
)

execution_results_root = MerkleRoot64(
  sorted_by_lane_id(LaneResultCommitment)
)
```

現行の2 fieldと同じく「input data」と「execution result」を分離する。将来laneが増えてもheader増分は一定である。

### 4.3 Lane payload leaf

```rust
pub struct LanePayloadCommitmentV1 {
    pub lane_id: u16,
    pub ruleset_version: u16,
    pub payload_hash: Hash64,
    pub payload_bytes: u32,
    pub tx_count: u32,
    pub system_op_count: u16,
}
```

active laneはempty payloadでもcanonical empty leafを持つ。leaf omissionで意味を変えてはならない。

### 4.4 Common execution result header

Lane 1/2で同じstructureを使い、Lane 3も共通部分を維持する。

```rust
pub struct LaneExecutionHeaderV1 {
    pub lane_id: u16,
    pub ruleset_version: u16,
    pub status: LaneExecutionStatus,

    pub base_anchor: Hash64,
    pub primary_execution_parent: Hash64, // Lane1はzero、Lane2/3はHash(L1@anchor)
    pub previous_lane_state_root: EvmH256,
    pub new_lane_state_root: EvmH256,

    pub transactions_root: EvmH256,
    pub receipts_root: EvmH256,
    pub outbox_root: Hash64,
    pub inbox_consumed_root: Hash64,

    pub gas_used: u64,
    pub state_read_units: u64,
    pub state_write_units: u64,
    pub auth_units: u64,
    pub data_bytes: u32,

    pub lane_number: u64,
    pub timestamp_sec: u64,
    pub base_fee_per_gas: EvmU256,
    pub extra_data_root: Hash64,
}
```

Lane 3のproof、object root、access rootは`extra_data_root`配下のversioned extensionに入れる。共通headerを頻繁に変更しない。

### 4.5 Lane execution hash

```text
LaneExecutionHash = keyed_blake2b_512(
  key = "MISAKA_LANE_EXECUTION_V1",
  borsh(LaneExecutionHeaderV1)
)
```

Lane 2/3の`primary_execution_parent`は、同anchorのLane 1 execution hashを指す。これはclock/anchor bindingであり、state inheritanceではない。

### 4.6 Common block environment

Lane 1/2は以下を完全共有する。

```text
block_number      = Base canonical execution number
block_timestamp   = Base/Lane1 non-decreasing timestamp
prevrandao        = selected_parent ancestryからdomain-separated導出
block_hash lookup = Base canonical ancestry
coinbase          = laneごとのfee recipient
spec_id           = same EVM fork schedule
```

Laneごとに独自timestampやfork numberを作らない。ruleset versionだけをlane registryで管理する。

### 4.7 Replay protection

署名対象には最低限を含める。

```text
network_id
chain_id
lane_id
ruleset_version
transaction_type
nonce
expiry / anchor window
```

Lane 2は既存chain IDを維持する。Lane 1とLane 3は新規chain IDを割り当てる。chain ID値はmainnet前にregistryで凍結する。

---

## 5. Lane 1 — Primary Security Lane / Solidity-compatible PQ-EVM

### 5.1 目的

Lane 1は次を保証する。

- native transaction authorizationがML-DSA-87
- native deposit、withdraw、official account、official bridgeが古典ECCへ依存しない
- EVM bytecode、Solidity ABI、contract storage、event/log、`eth_call`系read APIを維持
- Ethereum EOA raw transactionは受理しない
- Lane 2/3のexecution anchorとなる

### 5.2 互換性の正確な表現

```text
対応:
  Solidity compiler output
  EVM opcodes（security profileで明示禁止したprecompileを除く）
  ABI / events / CREATE / CREATE2
  eth_call / estimateGas / getLogs / state queries

非対応:
  Ethereum legacy/type-1/type-2 EOA署名transaction
  MetaMask標準EOA signerの無変更利用
  ecrecover前提のofficial account/bridge
  EIP-2612等、ECDSA固定の署名flow
```

したがって名称は`PQ-EVM`または`Solidity-compatible PQ-EVM`とし、`Ethereum-compatible`とは呼ばない。

### 5.3 PQ transaction envelope

```rust
pub struct PqEvmTransactionV1 {
    pub tx_type: u8,
    pub network_id: u64,
    pub chain_id: u64,
    pub lane_id: u16,              // MUST be 1
    pub ruleset_version: u16,

    pub sender_alias: [u8; 20],
    pub account_id: Hash64,
    pub key_id: u32,
    pub key_version: u32,
    pub nonce: u64,

    pub gas_limit: u64,
    pub max_fee_per_gas: EvmU256,
    pub max_priority_fee_per_gas: EvmU256,
    pub value: EvmU256,

    pub calls: Vec<PqEvmCall>,
    pub access_list: Vec<EvmAccessListItem>,
    pub valid_after_anchor: u64,
    pub valid_until_anchor: u64,

    pub mldsa87_signature: Vec<u8>,
}
```

`calls`をnativeに持たせ、1つのML-DSA署名で複数callを認可できるようにする。署名サイズを小さくするのではなく、署名1件あたりの有用処理を増やす。

### 5.4 Signature digest

```text
message_hash64 = keyed_blake2b_512(
  key = "MISAKA_PQ_EVM_TX_V1",
  canonical_encode(all fields except signature)
)

ML-DSA context = "MISAKA/PQ-EVM/TX/V1"
```

次を必ずbindする。

- fee fields
- 全callのtarget/value/calldata
- lane/chain/network/ruleset
- nonceとexpiry
- account ID、key ID、key version
- optional fee recipientまたはsponsor policy

署名済operationの横取りを防ぐため、relayer reimbursementやfee recipientを後付け可能にしてはならない。

### 5.5 PQ Auth Registry

public keyは毎transactionへ添付せず、初回登録する。

```rust
pub struct PqAccountRecord {
    pub account_id: Hash64,
    pub alias20: [u8; 20],
    pub scheme_id: u16,        // initial: ML-DSA-87
    pub public_key_hash: Hash64,
    pub public_key: Vec<u8>,
    pub key_id: u32,
    pub key_version: u32,
    pub status: PqKeyStatus,
    pub recovery_policy_hash: Hash64,
}
```

registryはconsensus-native stateを正とし、F004 predeployはread/write API viewとする。upgradeable Solidity contractだけをroot of trustにしない。

登録規則:

```text
account_id = H64("MISAKA_PQ_ACCOUNT_ID_V1" || scheme_id || public_key)
alias20    = AllocateAlias(H64("MISAKA_PQ_ALIAS_V1" || account_id || collision_nonce))
```

alias collision時は`collision_nonce`を増やし、既存mappingを上書きしない。未登録aliasへのnative depositはstrict modeで拒否する。

### 5.6 20-byte addressの扱い

Solidity `address`互換のため20-byte aliasを維持するが、権限identityはHash64とする。

```text
EVM alias20 → Registry → exact account_id64 → exact public key/version
```

署名から20-byte addressだけを再計算して権限を決めてはならない。contract addressもregistryへfull identityを記録し、既存aliasの再登録を禁止する。

### 5.7 Native auth flow

```text
1. canonical decode
2. lane/network/ruleset/expiry check
3. registry lookup
4. public key hash/key version check
5. ML-DSA-87 verify（EVM execution前）
6. nonce/fee affordability check
7. NormalizedEvmTxへ変換
8. common revm engineで実行
9. receiptにpq_auth metadata hashを記録
```

ML-DSA verificationをSolidity contractの任意code pathに委ねない。F003はcontract-level verification用であり、native sender authとは別である。

### 5.8 Common normalized transaction

Lane 1とLane 2はauth後に同じ内部型へ変換する。

```rust
pub struct NormalizedEvmTx {
    pub lane_id: u16,
    pub tx_hash: EvmH256,
    pub caller: EvmAddress,
    pub nonce: u64,
    pub gas_limit: u64,
    pub max_fee_per_gas: EvmU256,
    pub max_priority_fee_per_gas: Option<EvmU256>,
    pub calls: Vec<NormalizedCall>,
    pub access_list: Vec<EvmAccessListItem>,
    pub auth_cost: AuthCost,
    pub auth_receipt_hash: Hash64,
}
```

この境界以降はLane 1/2 executorを共通化する。

### 5.9 Batch semantics

初期仕様はatomic batchを推奨する。

```text
全call成功 → 全commit
いずれかrevert → 全call revert
fee/nonceはouter transaction単位
```

将来non-atomic batchを追加する場合は別transaction typeとする。1つのtypeへflagだけ追加して曖昧にしない。

### 5.10 PQ security profile

Lane 1のsystem pathは古典公開鍵暗号に依存してはならない。

- EOA authorization: ML-DSA-87のみ
- session key: 初期releaseでは禁止
- official ERC-1271 path: ML-DSA／approved PQ schemeのみ
- F002 withdrawal destination: PQ UTXO addressのみ
- F003: activation必須
- F004: activation必須
- `ecrecover`、BN254 pairing、KZG等の古典curve precompileはLane 1 profileで**disabledを既定**とする

EVM bytecode内で第三者dAppが独自古典暗号を実装することまでは完全に防げない。従ってproduct claimは「native auth・official asset pathがPQ」であり、「全contract計算がCategory 5」という意味ではない。

### 5.11 Crypto agility

ML-DSA-87だけを永久固定しない。

```text
scheme_id 0x0001 = ML-DSA-87
scheme_id 0x0002 = reserved hash-based backup
scheme_id 0x0003+ = future
```

key rotation、dual-sign migration期間、compromised scheme freeze、withdraw-only recoveryを設計に含める。

### 5.12 PQ resource budget

ML-DSA-87署名は4,627 bytes、公開鍵は2,592 bytesであるため、data/verify capをgasと別に明示する。

初期testnet候補:

```text
MAX_L1_NATIVE_MLDSA_VERIFIES_PER_CORE_BLOCK = 16
MAX_L1_NATIVE_AUTH_BYTES_PER_CORE_BLOCK      = 80 KiB
MAX_CALLS_PER_PQ_TX                          = 32
MAX_F003_VERIFIES_PER_TX                     = 8
MAX_F003_VERIFIES_PER_CORE_BLOCK             = 32
```

これらは文書値だけでなくconsensus counterでenforceする。ML-DSA verifyはshared core compute budgetから`PQ_AUTH_COMPUTE_UNITS`を消費する。

---

## 6. Lane 2 — Compatibility Lane / Current ETH-compatible EVM

### 6.1 目的

- 現在のEVM state、contract、nonce、storage、historyを継続
- EIP-2718 legacy/type-1/type-2 transactionを維持
- secp256k1 sender recoveryを維持
- `eth_*` JSON-RPC、MetaMask、既存toolingの変更を最小化
- Lane 1のPQ仕様変更を既存dAppへ強制しない

### 6.2 State移行

activation anchor `H`で次を行う。

```text
Lane2.genesis_parent_state_root = LegacyEvm.state_root(H-1)
Lane2.evm_number                = LegacyEvm.evm_number(H-1) + 1
Lane2.chain_id                  = existing EVM_CHAIN_ID
Lane2.tx/history indexes       = existing rowsをlane_id=2として参照
```

stateを別contractへbridgeしない。address、nonce、storageはそのまま維持する。

### 6.3 RPC compatibility

既存のdefault Ethereum endpointはLane 2へ向ける。

```text
http://node:port/            → Lane 2 eth_*
http://node:port/lane/1      → Lane 1 eth_* read + pq_* submit
http://node:port/lane/2      → Lane 2 explicit
http://lane3-rpc:port/       → Lane 3 executor/RPC
```

既存wallet設定を壊さないため、`eth_chainId`の既定値はLane 2で変えない。

### 6.4 Consensus behavior

Lane 2は現行v0.4 mergeset delayed acceptance、skip semantics、basefee、deposit/withdrawを原則継承する。変更はmulti-lane envelopeとstate namespace化に限定する。

### 6.5 量子耐性表示

Lane 2はclassical ECC laneである。Lane 1からLane 2への資産移動はsecurity downgradeであり、walletとbridge APIが明示しなければならない。

---

## 7. Lane 1 + Lane 2の共通化とVPS負担抑制

### 7.1 一つのexecutor、二つのauth adapter

```text
                 ┌─ ML-DSA Auth Adapter ─ Lane 1 raw tx
Common Executor ─┤
                 └─ EIP-2718 Auth Adapter ─ Lane 2 raw tx
```

共通化するもの:

- revm version、SpecId、opcode table
- block environment
- gas schedule、receipt/log generation
- state database interface
- code hash/code cache
- execution tracing
- RPC read/simulation engine
- pruning、snapshot、state sync
- metrics、observability

lane別にするもの:

- raw transaction decoder/authenticator
- chain ID/lane ID
- precompile profile
- account registry
- fee market state
- bridge/security policy
- mutable state namespace

### 7.2 Shared code store

contract bytecodeは`code_hash`でcontent-addressed共有する。

```text
shared_code_store[keccak(code)] = immutable bytecode
lane_state[(lane_id, address)]  = account metadata + code_hash
```

同一bytecodeをLane 1/2で二重保存しない。mutable account/storageは共有しない。

### 7.3 Persistent state backend

現行full snapshot cloneを廃止し、次へ移行する。

```text
FlatState(lane_id, account/storage key) → latest value
VersionedDiff(base_anchor, lane_id)     → changed keys only
IncrementalMPT(lane_id)                 → changed pathのみ更新
PeriodicSnapshotManifest                → finalized checkpoint
```

必須条件:

- empty acceptanceはO(1) fast path
- blockごとの全account deep clone禁止
- full state root再構築禁止
- RPC queryで全snapshot read禁止
- code blobはdeduplicated
- finalized diffをpruning depth後にGC
- snapshot importはstreamingかつsize-bounded

### 7.4 Aggregate resource envelope

Lane 1とLane 2へ現在の上限をそのまま各々付与すると、通常VPS要件がほぼ2倍になる。初期activationでは**shared aggregate cap**を使う。

現行上限を基準にしたtestnet候補:

```text
CORE_EVM_TOTAL_PAYLOAD_BYTES_PER_DAG_BLOCK = 128 KiB
CORE_EVM_TOTAL_ACCEPTED_GAS_PER_ANCHOR      = 30,000,000

Lane 1 guaranteed payload floor = 48 KiB
Lane 2 guaranteed payload floor = 48 KiB
Shared borrowable payload       = 32 KiB

Lane 1 guaranteed gas floor     = 12,000,000
Lane 2 guaranteed gas floor     = 12,000,000
Shared borrowable gas           =  6,000,000
```

未使用quotaは同anchor内で他core laneが借りられる。両lane混雑時はguaranteed floorを守る。

### 7.5 Compute unit accounting

EVM gasだけでなく、auth、state、dataを別counterで制限する。

```text
CoreBudget = {
  evm_gas,
  auth_compute_units,
  payload_bytes,
  state_read_units,
  state_write_units,
  receipt_log_bytes
}
```

ML-DSA verifyを「無料のpreprocessing」にしない。`PQ_AUTH_COMPUTE_UNITS`を実測で較正し、core block budgetへ算入する。

### 7.6 Weighted fair scheduler

```text
1. system ops reservation
2. Lane 1 guaranteed floor
3. Lane 2 guaranteed floor
4. shared poolをeffective fee / resource unitで配分
5. per-lane starvation guard
```

Base minerがlane順序を任意変更できないよう、tie-breakはcanonical tx hash/orderで決定する。

### 7.7 One process / one DB / one RPC stack

通常operatorは別daemonを2つ起動しない。

```text
kaspad
  ├─ Base consensus
  ├─ lane-runtime[1]
  ├─ lane-runtime[2]
  ├─ shared-state-service
  ├─ shared-eth-rpc-server
  └─ shared-index-service (optional)
```

DB keyは`(store_prefix, lane_id, key)`とする。store prefixをlaneごとに大量複製しない。

### 7.8 Optional index separation

full validationに不要な以下は既定OFFとする。

- historical logs index
- trace index
- archive state
- contract source metadata
- Lane 3 full state/index

validatorにexplorer/archive責務を負わせない。

### 7.9 Performance acceptance target

実装合格基準は、同じaggregate gas/data workloadで次を目標とする。

```text
Core node p95 CPU time   <= current single-lane implementationの1.35倍
Core node peak RAM       <= current single-lane implementationの1.35倍
Empty second lane cost   <= 3% overhead
Code storage duplication <= 5%
Lane 3 disabled時のbinary/runtime overheadは測定可能な最小値
```

これは目標値であり、benchmarkで未達ならactivationしない。

---

## 8. Lane 3 — Performance Lane / SVM・Sui型Parallel ECC-EVM

### 8.1 目的

- independent state/object workloadを複数coreで実行
- high-throughput game、NFT mint、payments、partitioned DeFi
- normal node/validatorにLane 3 full state、RPC、executionを要求しない
- Base orderingとsettlementへanchorし、別chain/sequencer化を避ける

### 8.2 非目的

- MetaMask完全互換
- 任意Ethereum raw transactionの無変更受理
- 単一hot pool／単一shared objectの物理的逐次限界の解消
- Lane 3 executor committeeへの信頼だけでnative資産をunlockすること

### 8.3 初期signature scheme

初期schemeはEd25519を推奨する。

```text
public key: 32 bytes
signature: 64 bytes
pre-execution batch verification
```

Lane 2がsecp256k1 compatibilityを担うため、Lane 3は専用wallet/SDKを許容する。transactionには`signature_scheme`を持たせるが、初期mainnetはEd25519のみをactiveにする。

### 8.4 Parallel transaction format

```rust
pub struct ParallelEvmTransactionV1 {
    pub tx_type: u8,
    pub network_id: u64,
    pub chain_id: u64,
    pub lane_id: u16,              // MUST be 3
    pub ruleset_version: u16,

    pub sender_alias: [u8; 20],
    pub sender_pubkey_or_key_id: Vec<u8>,
    pub nonce_or_object_sequence: u64,
    pub fee_fields: ParallelFeeFields,

    pub calls: Vec<ParallelCall>,
    pub access_manifest: AccessManifest,
    pub object_inputs: Vec<ObjectRef>,
    pub valid_until_anchor: u64,

    pub signature_scheme: u16,
    pub ecc_signature: Vec<u8>,
}
```

### 8.5 Access manifest

EIP-2930 access listはlist外accessが可能なため、parallel correctnessの根拠には使えない。Lane 3ではmanifestを**enforced**する。

```rust
pub enum AccessKey {
    Account([u8; 20]),
    Storage([u8; 20], [u8; 32]),
    Object([u8; 32]),
    SystemResource(u16),
}

pub enum AccessMode { Read, Write }
```

暗黙accessも規則化する。

- sender nonce/balance: write
- fee payer balance: write
- value recipient balance: write
- called code account: read
- CREATE/CREATE2 destination: write
- precompile/system resource: declared class

未申告accessは`UndeclaredAccess`としてtransaction failureにする。Base blockをinvalidにしない。

### 8.6 Object model

```rust
pub struct EvmObjectRef {
    pub object_id: [u8; 32],
    pub version: u64,
    pub digest: EvmH256,
    pub kind: ObjectKind,
}

pub enum ObjectKind {
    Owned,
    Shared,
    Immutable,
}
```

- **Owned:** owner署名とexact versionが必要。異なるobjectは並列。
- **Shared:** Base canonical orderで順序付け。同じobjectへのwriteは逐次。
- **Immutable:** read-onlyで無制限並列。

NFT、game item、position、coin shardはOwned objectへ適する。DEX pool、vault、orderbook shardはShared objectとなる。

### 8.7 Deterministic scheduler

```text
1. canonical tx orderをBaseから受け取る
2. signatureをbatch verify
3. manifest/object versionを事前検証
4. conflict graphを構築
5. deterministic waveへ割当
6. wave内をparallel execute
7. undeclared access／version mismatchをfail receipt化
8. effectsをcanonical orderでcommit
9. state/object rootをincremental update
```

conflict条件:

```text
Read(A)  vs Read(A)  = no conflict
Write(A) vs Read(A)  = conflict
Read(A)  vs Write(A) = conflict
Write(A) vs Write(A) = conflict
```

schedulerのthread countや実行順はlocal最適化でよいが、final effectはcanonical semanticsと一致しなければならない。

### 8.8 Lane 3はstrict parallel classのみ

Lane 2がcompatibilityを担うため、Lane 3では`LEGACY_SERIAL` classをmainnet初期仕様に入れない。

```text
manifestなし             → admission reject
manifest外access          → deterministic failed receipt
unknown object version    → failed receipt / retry
unsupported dynamic call  → admission rejectまたはfailed receipt
```

これによりhigh-performance laneへ互換性負債を持ち込まない。

### 8.9 Local object congestion control

同じshared objectへ集中するtransactionはparallel化できない。objectごとにcapacityとsurchargeを持たせる。

```rust
pub struct SharedObjectBudget {
    pub object_id: [u8; 32],
    pub execution_units_per_anchor: u64,
    pub congestion_base_fee: u128,
}
```

hot objectだけのfeeを上げ、Lane 3全体のbase feeを過剰に上げない。dAppにはpool/orderbook/vaultのpartitionを促す。

### 8.10 Execution/proof pipeline

```text
Base/Lane1 anchor orders Lane 3 input
        ↓
permissionless Lane 3 executor executes
        ↓
prover generates state-transition proof
        ↓
Base core nodes verify proof
        ↓
verified Lane 3 state/outbox root advances
```

normal node/validatorが行うのは次だけである。

- input commitmentのcanonicality確認
- data availability status確認
- proof verifier実行
- verified root/outboxの保存
- cross-lane messageのfinality判定

Lane 3 state DB、object DB、mempool execution、trace/indexは不要である。

### 8.11 Proof statement

```text
VerifyLane3Proof(
  ruleset_version,
  previous_verified_state_root,
  previous_object_root,
  ordered_input_roots[batch_start..batch_end],
  new_state_root,
  new_object_root,
  receipts_root,
  outbox_root,
  gas_and_resource_totals,
  verifier_key_hash
) == true
```

proofはsignature validation、nonce/object version、access enforcement、EVM execution、fee accounting、root更新を含む。

### 8.12 Proof system policy

- productionで高性能nodeのN-of-M署名だけをproof代替にしてはならない。
- proof systemは`proof_system_id`、`circuit_version`、`verifier_key_hash`でversion化する。
- verifier upgradeはBase hard forkまたは明示governance activationとする。
- pairing-based proofだけを使う場合、Lane 3はclassical securityであり、Base native asset escrowへ上限を設ける。
- PQ settlement claimを維持するなら、hash-based proofまたはdual-proof migrationを長期要件とする。

### 8.13 Delayed commitment

Lane 3はBase 10 BPSを止めない。

```text
B(n):   Lane3 input rootを確定
B(n+k): input range n..mのproofを受理
        Lane3 verified rootを更新
```

RPC status:

```text
input_accepted
executed_unverified
proof_submitted
verified
base_finalized
```

cross-lane withdraw/messageは`verified + Base finalized`からのみ許可する。

### 8.14 Liveness

proofが期限内に来なくてもBase、Lane 1、Lane 2は進み続ける。Lane 3だけがstalledになる。

- 誰でも同じinputからproof生成可能
- first valid proofに報酬
- executor/prover outageでstate rootを推測採用しない
- timeoutはslashingより報酬喪失を基本とする
- objective double-commit／invalid DA certificateのみslash対象候補

### 8.15 Data Availability

executionを分離しても、data bandwidthは自動的に消えない。段階導入する。

**Phase DA-1（初期、安全優先）**

- Lane 3 batch dataをBase bodyへ圧縮掲載
- strict bytes cap
- normal nodeは短期保持後prune可能
- high TPS上限はBase bandwidthに従う

**Phase DA-2（拡張）**

- erasure coding
- hash/Merkle-based data commitment
- samplingまたはavailability certificate
- full dataはLane 3 DA/archive nodeが保持
- Base normal nodeの保持量をboundedにする

DA-2は独立ADRと監査を必須とする。DAが成立しないbatchへproof/withdraw finalityを与えない。

---

## 9. Node roleと参加条件

### 9.1 Role matrix

| Role | Base | Lane 1 | Lane 2 | Lane 3 execution | Lane 3 proof verify | Index/RPC |
|---|---:|---:|---:|---:|---:|---:|
| Core full node | Full | Full | Full | No | Yes | Minimal |
| Validator | Full | Full | Full | No | Yes | Minimal |
| Miner | Full | Full | Full | No | Yes | Template only |
| Lane 3 executor | Fullまたはtrusted feed | Optional local | Optional local | **Full** | Yes | Optional |
| Lane 3 prover | Input/state access | No | No | witness/replay | Generate | No |
| Lane 3 RPC/archive | Full Base view | Optional | Optional | Full | Yes | **Full** |
| Light client | Header/proofs | Proof | Proof | No | Succinct | No |

「通常validatorはLane 3不参加」とは、Lane 3 user txのfull execution、state保存、RPC/indexを行わないという意味である。BaseへLane 3 rootを安全に取り込むためのproof verificationは必須とする。

### 9.2 Single binary policy

現在のADR-0010と同様、node roleは原則1 binary + opt-in subsystemで提供する。

```text
kaspad
  --core-lanes=1,2                 # default on active network
  --lane3-proof-verifier=on        # default on
  [--enable-lane3-executor]
  [--enable-lane3-prover]
  [--enable-lane3-rpc]
  [--lane3-data-dir <path>]
```

Lane 3 heavy dependenciesはoptional cargo feature／別sidecar processへ分離し、normal binaryへGPU/prover dependencyを持ち込まない。

### 9.3 Build profile

```text
core-node:
  Base + common EVM + ML-DSA auth + secp Lane2 + Lane3 proof verifier

lane3-executor:
  core types + parallel scheduler + object DB + Ed25519 batch verify

lane3-prover:
  proof backend + witness generator

archive-all:
  all lanes + full indexes
```

Lane 2がcore mandatoryであるため、network全体を「binaryも完全secp-free」とは表現できなくなる。PQ guaranteeはBase/Lane 1のsecurity domainについて行う。

---

## 10. Cross-lane settlementとmessage

### 10.1 原則

- 同期cross-lane `CALL`禁止
- sourceでeffectを確定し、outbox rootへcommit
- Baseがsource rootのstatusを確認
- destinationが後続anchorでexactly-once consume
- message IDを全lane共通domainで生成

### 10.2 Message format

```rust
pub struct CrossLaneMessageV1 {
    pub source_lane: u16,
    pub destination_lane: u16,
    pub source_anchor: Hash64,
    pub source_tx_hash: EvmH256,
    pub message_index: u32,
    pub sender: LaneAccountId,
    pub target: LaneAccountId,
    pub asset_id: Hash64,
    pub amount_or_object_id: [u8; 32],
    pub payload_hash: Hash64,
    pub timeout_anchor: u64,
}
```

```text
message_id = H64("MISAKA_XLANE_MSG_V1" || canonical_message)
```

inboxはmessage IDを一度だけconsumeする。

### 10.3 Native token

native supplyのroot of truthはBaseとする。

```text
Base UTXO → lane deposit:
  Base escrow lock
  destination lane credit after verified claim

lane → Base UTXO:
  source burn/debit
  source outbox verified/finalized
  Base synthetic UTXO materialization
```

各lane balanceは同時に同じcoinを表現しない。

### 10.4 Lane 1からclassical laneへの移動

```text
Lane 1 → Lane 2: ML-DSA protectionからsecp256k1へdowngrade
Lane 1 → Lane 3: ML-DSA protectionからEd25519へdowngrade
```

walletは明示確認を必須とする。自動routingでsecurity downgradeしてはならない。

### 10.5 Lane 3 asset gate

Lane 3 native bridgeは次を満たすまでdisabledとする。

- proof verifier production audit完了
- DA production gate完了
- replay/exactly-once property test完了
- escrow cap、withdraw rate limit、emergency freeze実装
- invalid proof／verifier bug時の停止手順確認

---

## 11. Consensus validityとfailure semantics

### 11.1 Base block validity

- payload root mismatch: Base block invalid/disqualified
- lane ID/ruleset/size canonical encoding mismatch: Base block invalid/disqualified
- Lane 1/2 execution commitment mismatch: current EVMと同じくchain candidate disqualification
- Lane 3 user transactionの実行failure: Base blockをinvalidにしない
- Lane 3 invalid proof: proof submissionだけreject

### 11.2 Mandatory core lanes

activeなLane 1/2は各Base anchorにcanonical resultを1つ持つ。empty laneでもempty result leafを持つ。

### 11.3 Lane 3 status leaf

Base result rootのLane 3 leafはcurrent inputの未証明rootをstate rootとして扱わない。

```rust
pub struct ProvenLaneStatusV1 {
    pub latest_input_batch: u64,
    pub latest_executed_claim: u64,
    pub latest_verified_batch: u64,
    pub latest_verified_state_root: EvmH256,
    pub latest_verified_outbox_root: Hash64,
    pub proof_system_id: u16,
}
```

### 11.4 Reorg

EvmResult／LaneResultはblock hashに対して不変とする。virtual changeではcanonical pointerを切り替え、同じblockを再実行して別resultを作らない。

Lane 3 proofはbase anchor rangeへbindし、canonical chainから外れたanchorを含むproofはcanonical rootをadvanceしない。

### 11.5 Lane 1 liveness protection

Lane 1は親execution blockであるため、resource abuseがBase livenessへ波及しやすい。次を必須とする。

- PQ bytes cap
- native verify count cap
- F003 verify cap
- gas/state read/write/log cap
- deterministic skip/failure semantics
- empty fast path
- preverification cache
- lane-specific mempool quota

---

## 12. Storage、pruning、sync

### 12.1 DB keying

```text
LaneKey = lane_id || base_anchor || object_key
```

既存EVM storeを一般化する。

```text
ExecutionHeaderStore
ExecutionPayloadStore
StateRootStore
StateDiffStore
ReceiptStore
TxLookupStore
LogsStore
CanonicalHeadsStore
CrossLaneOutboxStore
ProofStore
ObjectStore (Lane 3 executor only)
```

### 12.2 Snapshot

Lane 1/2 snapshotは同じformatを使う。

```rust
pub struct LaneSnapshotManifestV1 {
    pub lane_id: u16,
    pub ruleset_version: u16,
    pub base_anchor: Hash64,
    pub state_root: EvmH256,
    pub chunk_root: Hash64,
    pub chunk_count: u32,
    pub uncompressed_bytes: u64,
    pub code_store_root: Hash64,
}
```

chunk size、total size、decompression ratioをboundedにする。streaming importし、全snapshotをRAMへ展開しない。

### 12.3 Pruning profile

```text
Core pruned node:
  Base required history
  Lane 1/2 recent diffs + finalized snapshots
  Lane 3 verified roots/proofs only

Archive node:
  all historical states/receipts/logs

Lane 3 archive:
  full input data/object state/witness
```

### 12.4 State sync trust

snapshotを受け取っても、manifest rootとBase committed state rootを検証する。RPC providerの署名だけを信頼しない。

---

## 13. Mempool、P2P、template

### 13.1 Separate queues

```text
mempool_lane1_pq
mempool_lane2_eth
mempool_lane3_parallel
system_ops_queue
cross_lane_inbox_queue
```

queueは別だが、core lanesのtemplate allocatorはshared aggregate budgetを使う。

### 13.2 P2P topics

- Base block/body
- Lane 1 PQ tx relay
- Lane 2 ETH tx relay
- Lane 3 input tx relay
- Lane 3 proof relay
- Lane 3 DA shard relay（将来）

Lane 3 heavy trafficは別queue、別bandwidth class、別peer scoringにする。Base/attestation/UTXO trafficをstarveさせない。

### 13.3 Template order

```text
1. Base system/UTXO/attestation reservation
2. Lane 1/2 guaranteed allocations
3. shared core allocation
4. Lane 3 data allocation（独立cap）
5. rootsを計算
6. PoW finalize
```

Lane 3 execution/proofをblock template critical pathへ入れない。

---

## 14. RPC、wallet、explorer

### 14.1 Common lane-aware provider

```rust
trait LaneProvider {
    fn lane_id(&self) -> u16;
    fn head(&self, tag: LaneBlockTag) -> LaneHead;
    fn balance(&self, account: EvmAddress, tag: LaneBlockTag) -> U256;
    fn call(&self, request: CallRequest, tag: LaneBlockTag) -> CallResult;
    fn receipt(&self, tx: EvmH256) -> Option<LaneReceipt>;
}
```

RPC implementationはprovider差し替えでLane 1/2を提供する。method codeを複製しない。

### 14.2 Method policy

Lane 1:

```text
eth_call / eth_estimateGas / eth_getBalance / eth_getCode / eth_getLogs
pq_sendRawTransaction
pq_registerKey
pq_rotateKey
pq_getAccount
misaka_getLaneStatus
```

Lane 2:

```text
既存eth_*を維持
eth_sendRawTransaction
misaka_getLaneStatus
```

Lane 3:

```text
parallel_sendTransaction
parallel_simulateAccess
parallel_getObject
parallel_getBatchStatus
eth_call subset（executor RPC）
```

### 14.3 Block tags

```text
latest_input
latest_executed
latest_verified
safe
finalized
```

Lane 1/2では`latest_executed`と`latest_verified`が通常一致する。Lane 3では一致しない。

### 14.4 Explorer schema

全tableへ`lane_id`を追加し、Lane 1/2で同じschemaを使う。Lane 3 object/effect tableはextensionとする。

### 14.5 Wallet UX

walletは常に次を表示する。

- 現在のlane
- auth scheme
- security class
- bridge後のsecurity class
- proof/finality status
- Lane 3の`executed`と`verified`の差

---

## 15. Fee marketと報酬

### 15.1 Lane 1/2

base fee stateはlaneごとに独立させる。shared aggregate capを使っても、Lane 1混雑がLane 2 base feeへ直接反映されないようにする。

```text
total_fee = EVM execution fee
          + data byte fee
          + auth compute fee
          + state access/write fee（将来）
```

Lane 1のML-DSA auth feeはEVM gasと別明細にするが、block admissionではshared compute budgetを消費する。

### 15.2 Lane 3

```text
Lane3 fee = Base data/DA fee
          + execution fee
          + proof fee
          + shared-object congestion surcharge
```

- Base data fee: miner/DA provider
- execution/proof fee: first valid proof submitterまたはmarket contract
- invalid proof submission: fee forfeiture
- timeoutだけで大規模slashしない

### 15.3 Fee isolation

一つのlaneのspamで他laneの最低枠を奪えない。unused capacityだけをborrow可能にする。

---

## 16. セキュリティ不変条件

```text
I-01: Baseだけがcanonical DAG orderを決める。
I-02: Lane 2/3は同anchorのLane 1 execution hashへbindする。
I-03: Lane 1/2/3 mutable stateは完全分離する。
I-04: 同一raw txはlane/network/rulesetを跨いでreplayできない。
I-05: Lane 1 native sender authはactive PQ schemeだけを受理する。
I-06: Lane 1 official deposit先はregistered PQ identityに限定する。
I-07: Lane 1 official withdrawはPQ UTXO destinationだけを受理する。
I-08: Lane 1/2のexecutor coreは共通だがauth/precompile policyは明示分離する。
I-09: Lane 1+2 aggregate resource capはconsensusでenforceする。
I-10: empty laneはO(1)で、全state clone/root再計算を起こさない。
I-11: cross-lane messageはsource verified/finalized rootからのみconsumeする。
I-12: cross-lane messageはexactly-onceである。
I-13: native supplyはBase escrow + all lane balancesの保存則を満たす。
I-14: Lane 3 executor/proverはordering powerを持たない。
I-15: Lane 3 state advanceはvalid proofなしに起こらない。
I-16: Lane 3 pending/executed-unverified stateからBase/Lane 1/2へwithdrawできない。
I-17: Lane 3 proofはcanonical Base input rangeへbindする。
I-18: Lane 3 DA不成立batchはverifiedにならない。
I-19: normal validatorはLane 3 full stateを持たなくてもBase safetyを検証できる。
I-20: Lane 3停止はBase/Lane 1/Lane 2を停止させない。
I-21: Lane 1からLane 2/3への移動はwalletでsecurity downgrade表示する。
I-22: headerにはlaneごとのfieldを追加せず、versioned rootで拡張する。
I-23: ruleset/crypto/proof verifier upgradeはversioned activationを経る。
I-24: state snapshot importはsize-bounded、streaming、root-verifiedである。
```

---

## 17. 脅威モデルと主なリスク

### 17.1 Lane 1

- ML-DSA署名byte flood
- verify CPU exhaustion
- public-key registry collision／rotation bug
- 20-byte alias誤認
- F003 resource abuse
- native batch内の権限/fee binding不備
- crypto monoculture

対策はexplicit counters、registry identity、batch digest binding、scheme agility、strict bridgeである。

### 17.2 Lane 2

- CRQCによるsecp256k1 EOA資産奪取
- Ethereum互換性維持による古典precompile依存
- legacy dAppのreentrancy／bridge risk

Lane 2は互換性laneとしてリスクを明示し、PQ claim対象外とする。

### 17.3 Lane 3

- executor/prover集中
- proof system bug／verifier key compromise
- DA withholding
- access manifest omission
- shared-object hotspot
- optimistic `executed`表示の誤認
- high-performance RPCの中央集権

correctnessをproof、orderingをBase、availabilityをDA protocolへ分離し、各trust boundaryをUIとdocsで表示する。

### 17.4 Shared infrastructure

- shared code cache poisoning
- lane ID keying omissionによるstate混線
- resource allocator bugによるstarvation
- RPC default lane取り違え
- migration heightのstate root mismatch

すべてのcache/index/store keyへlane IDを含め、cross-lane property testsを必須とする。

---

## 18. Activationと移行計画

### Phase 0 — prerequisite remediation

- full snapshot clone廃止
- incremental state root
- state pruning/snapshot sync
- mergeset prefilter
- RPC allocation caps
- explicit auth resource counters

### Phase 1 — generalized lane types、Lane 2 only

- `execution_payloads_root`／`execution_results_root`
- lane registry
- current EVMをlane ID 2としてshadow execution
- old/new commitment differential test
- Base header v3 version gate

このphaseではLane 1/3はcanonical empty/inactive leafとする。

### Phase 2 — Lane 2 migration

hard-fork anchor `H`で現行EVMをLane 2へ正式再分類する。

- chain ID維持
- state root/history/index continuity
- default `eth_*` endpoint維持
- old binaryは明示停止

### Phase 3 — Lane 1 PQ-EVM activation

- new PQ genesis state
- F003/F004 activation
- PqEvmTransactionV1
- strict registered deposit
- PQ withdraw
- batch transaction
- wallet/SDK

Lane 1とLane 2のaggregate capを有効化する。

### Phase 4 — Lane 3 shadow mode

- input ordering
- Ed25519 auth
- access/object scheduler
- executor/prover network
- no canonical native asset
- normal nodeはproof verifyをshadow比較

### Phase 5 — Lane 3 test assets

- proof-required verified root
- Base-inline DA
- test token only
- limited cross-lane messages

### Phase 6 — Lane 3 capped production

- audited proof/DA
- native escrow cap
- withdraw rate limit
- emergency freeze
- multiple independent executors/provers

### Phase 7 — Lane 3 DA scaling

- erasure-coded DA
- sampling/certificate ADR
- cap increaseはmeasured propagationとavailabilityに基づく

---

## 19. コード変更map

### 19.1 Consensus core

```text
consensus/core/src/execution_lanes/
  mod.rs
  registry.rs
  payload.rs
  header.rs
  commitment.rs
  cross_lane.rs
  pq_tx.rs
  parallel_tx.rs
```

`consensus/core/src/evm/mod.rs`のEVM共通型は段階的に上記へ移し、compat re-export期間を設ける。

### 19.2 Header/block/wire

```text
consensus/core/src/header.rs
consensus/core/src/hashing/header.rs
consensus/core/src/block.rs
protocol/p2p/src/convert/block.rs
rpc/core/src/model/{header,block}.rs
rpc/grpc/core/src/convert/
```

- header v3 gating
- multi-lane payload canonical encoding
- old testnet serialization compatibilityはactivation前後で明示分離

### 19.3 Execution orchestrator

```text
consensus/src/processes/execution_lanes/mod.rs
consensus/src/processes/execution_lanes/core_lanes.rs
consensus/src/processes/execution_lanes/lane3_proof.rs
```

現行`consensus/src/processes/evm/mod.rs`のmergeset collection、pipeline、persistをlane-generic化する。

### 19.4 EVM runtime

```text
kaspa-evm/src/common/
  env.rs
  executor.rs
  normalized_tx.rs
  receipts.rs
  precompile_profile.rs

kaspa-evm/src/auth/
  pq.rs
  ethereum.rs

kaspa-evm/src/state/
  backend.rs
  diff.rs
  incremental_trie.rs
  snapshot.rs
```

Lane 1/2を別executor copyにしない。

### 19.5 Lane 3 runtime

heavy dependency分離のため新規crateを推奨する。

```text
kaspa-parallel-evm/
  tx.rs
  access.rs
  objects.rs
  scheduler.rs
  executor.rs
  effects.rs
  witness.rs

kaspa-lane3-proof/
  statement.rs
  verifier.rs
  prover_adapter.rs
```

normal core nodeは`statement`と`verifier`だけをlinkする。

### 19.6 Database

```text
database/src/stores/execution_lanes/
```

existing EVM prefixesをlane-awareにmigrationする。`lane_id`をkey prefixへ必ず含める。DB version bumpとclean migration/resync policyを定義する。

### 19.7 Mining/mempool

```text
mining/src/lane_mempool.rs
mining/src/lane_allocator.rs
mining/src/lane_template.rs
```

Lane 1/2 common admission interface、Lane 3 data-only admissionを分離する。

### 19.8 RPC/CLI/wallet

```text
rpc/eth/                 # lane-aware provider共通化
rpc/core/src/model/lane.rs
misaka-cli/src/lane/
wallet/core/src/pq_evm/
```

既存RPC defaultはLane 2へ固定し、breaking changeを避ける。

---

## 20. 試験計画

### 20.1 Commitment／ordering

```text
T-01 all active lanes have canonical leaves
T-02 lane order is sorted and duplicate lane ID is rejected
T-03 mergeset payload is exactly-once per lane
T-04 own payload is accepted by selected child
T-05 Lane2/3 primary_execution_parent equals L1@same anchor
T-06 no lane has independent fork-choice
T-07 reorg changes pointers only
```

### 20.2 Lane 1 PQ

```text
T-10 ML-DSA KAT / exact lengths / malformed input
T-11 wrong lane/network/ruleset/nonce/expiry rejected
T-12 registry alias collision cannot overwrite
T-13 key rotation/version replay rejected
T-14 batch digest binds every call/value/fee
T-15 direct secp transaction rejected
T-16 unregistered deposit rejected
T-17 PQ withdraw only
T-18 verify/bytes caps are consensus-enforced
T-19 F003 limits and gas/resource accounting
```

### 20.3 Lane 2 compatibility

```text
T-20 pre/post migration state root continuity
T-21 existing contract address/nonce/storage unchanged
T-22 raw EIP-2718 tx hash unchanged
T-23 chain ID unchanged
T-24 MetaMask/Foundry/Hardhat smoke suite
T-25 existing receipt/log RPC compatibility
```

### 20.4 Core resource

```text
T-30 Lane1 empty overhead
T-31 Lane2 empty overhead
T-32 both busy aggregate gas cap
T-33 guaranteed floor/starvation
T-34 ML-DSA verify + EVM gas combined worst case
T-35 incremental root equals reference full rebuild
T-36 no full snapshot clone allocation
T-37 pruning/state sync under multi-lane
T-38 RPC queries do not seed full state
```

### 20.5 Lane 3 parallelism

```text
T-40 disjoint manifests execute in parallel
T-41 read/read no conflict
T-42 write/read and write/write conflict
T-43 undeclared SLOAD/SSTORE/CALL fails deterministically
T-44 object version replay rejected
T-45 owned/shared/immutable semantics
T-46 parallel effect equals canonical reference semantics
T-47 thread-count-independent root
T-48 hot object congestion budget
T-49 adversarial manifest size/depth caps
```

### 20.6 Proof／DA

```text
T-50 proof binds previous root and exact input range
T-51 proof for orphaned anchor rejected
T-52 forged/modified public inputs rejected
T-53 missing proof stalls only Lane3
T-54 invalid proof does not invalidate Base block
T-55 DA-unavailable batch cannot verify/finalize
T-56 verifier version activation/rollback
T-57 multiple independent prover interoperability
```

### 20.7 Cross-lane

```text
T-60 exactly-once consume
T-61 no consume before source finality
T-62 Lane3 unverified withdraw rejected
T-63 supply invariant across all lanes
T-64 timeout/refund race
T-65 security downgrade UI/API flag
T-66 reorg across source/destination anchors
```

### 20.8 Benchmark matrix

- Lane 1 batched PQ transfers
- Lane 1 PQ contract calls
- Lane 2 native/erc20/swap compatibility
- Lane 1+2 mixed load under aggregate cap
- Lane 3 independent transfers
- Lane 3 NFT mint across independent objects
- Lane 3 many DEX pools
- Lane 3 single hot pool
- proof latency、proof size、verify latency
- Base propagation with Lane 3 DA payload
- pruned core node long-run disk growth

---

## 21. 運用サイジングとrelease gate

### 21.1 Protocol requirementではなくcapacity target

CPU core数やRAM容量をconsensus membership条件にしない。hardware偽装を検出できずpermissioned化するためである。protocolが判定するのはdeadline内のvalid proof、DA、objective equivocationである。

暫定capacity planning target:

| Profile | CPU | RAM | Storage | Network | 備考 |
|---|---:|---:|---:|---:|---|
| Core pruned node / validator | 8 vCPU級 | 32 GB級 | 1–2 TB NVMe | 1 Gbps級 | Base + Lane1 + Lane2、Lane3 proofのみ |
| Core RPC | 16 vCPU級 | 64 GB級 | 2–4 TB NVMe | 1–2 Gbps | logs/index範囲次第 |
| Lane 3 executor | 32 core級 | 128 GB級 | 4 TB+ NVMe | 2 Gbps+ | object/state full execution |
| Lane 3 prover | workload依存 | 128–512 GB級 | witness領域 | 2–10 Gbps | GPU/acceleratorはproof system依存 |
| Full archive/explorer | 32–64 core級 | 256 GB+ | 8 TB+ | 5 Gbps+ | 全lane index/history |

数値は保証ではなく、benchmark後にrunbookへ移す。

### 21.2 Mainnet release gate

**P0 — multi-lane前**

- persistent state backend
- incremental root
- pruning/state sync
- resource counters
- audit Critical/High remediation

**P1 — Lane 1/2**

- migration differential test
- PQ registry/auth audit
- aggregate cap stress
- existing dApp regression
- cross-lane supply proof

**P2 — Lane 3 testnet**

- deterministic parallel executor
- independent implementation/reference executor
- proof statement freeze
- DA measurement

**P3 — Lane 3 production asset**

- proof verifier audit
- DA audit
- 2以上のindependent executor/prover operators
- emergency stop/withdraw cap
- 90日以上のsoak
- Base node burdenがtarget内

---

## 22. Open decisions

```text
O-01 Lane1/Lane3 chain IDの最終値
O-02 Lane1でdisabledにするclassical precompileの確定一覧
O-03 PqEvmTransaction type byteとcanonical encoding
O-04 atomic batchのMAX_CALLSとfailure semantics
O-05 PQ_AUTH_COMPUTE_UNITSの実測較正
O-06 Lane1/2 guaranteed floorとshared capの最終値
O-07 Lane3 Ed25519以外のscheme追加時期
O-08 exact access manifestのSDK生成／simulation retry規則
O-09 Lane3 object storeをEVM storageとどうmappingするか
O-10 proof system、verifier key lifecycle、PQ migration
O-11 Lane3 DA-2のsampling/certificate方式
O-12 Lane3 proof fee market
O-13 Lane3 native escrow capとrate limit
O-14 Base header v3 activationをre-genesisに同梱するか
O-15 existing EVM history/indexを物理migrationするかlogical aliasにするか
```

---

## 23. 最終設計判断

1. BaseはPQ consensus、DAG ordering、native settlementを維持する。
2. Lane 1は同Base anchor内のPrimary Security Execution Blockとし、Lane 2/3のparent execution referenceになる。
3. Lane 1はML-DSA-87 native authorizationのPQ-EVMとし、Ethereum EOA transactionを受理しない。
4. Lane 2へ現在のETH-compatible EVMをstate/history/chain IDごと継続する。
5. Lane 1/2は同じEVM engine、state backend、RPC code、receipt/log schemaを共有する。
6. Lane 1/2の初期aggregate bytes/gas/state/auth budgetを共有し、通常VPSへの負担を現在の単一EVMから大きく増やさない。
7. Lane 3はEd25519、enforced access manifest、object version、deterministic conflict schedulingを採用する。
8. Lane 3のfull executionは高性能executor/proverへ分離し、通常node/validatorはproofとDA statusのみ検証する。
9. Lane 3 state rootはvalid proofなしにBaseへ採用しない。
10. cross-laneは非同期outbox/inboxとし、Lane 3からのexitはverified + finalizedだけを許可する。
11. header拡張はlaneごとのfieldではなく2つのMerkle rootへ一般化する。
12. state clone/root/I/O問題を直す前にmulti-lane activationしてはならない。

この条件下で、次の三段構えはMISAKAの設計として整合する。

```text
Lane 1 = Security / PQ adoption
Lane 2 = Compatibility / ecosystem adoption
Lane 3 = Performance / professional execution
```

---

## 参考資料

### MISAKA source snapshot

- `docs/adr/0020-selected-parent-evm-lane.md`
- `docs/misaka-evm-design-v0.4.md`
- `docs/misaka-evm-optimization-design-v0.1.md`
- `docs/misaka-prea-design-v1.1.md`
- `docs/adr/0010-validator-node-architecture.md`
- `consensus/core/src/evm/mod.rs`
- `consensus/src/processes/evm/mod.rs`
- `kaspa-evm/src/executor.rs`
- `kaspa-evm/src/tx.rs`
- `kaspa-evm/src/snapshot.rs`
- `kaspa-evm/src/mldsa_verify.rs`

### 外部primary sources

- NIST FIPS 204, Module-Lattice-Based Digital Signature Standard: https://csrc.nist.gov/pubs/fips/204/final
- EIP-2718, Typed Transaction Envelope: https://eips.ethereum.org/EIPS/eip-2718
- EIP-2930, Optional Access Lists: https://eips.ethereum.org/EIPS/eip-2930
- EIP-1559, Fee Market Change: https://eips.ethereum.org/EIPS/eip-1559
- EIP-7928, Block-Level Access Lists: https://eips.ethereum.org/EIPS/eip-7928
- Solana Transaction Structure: https://solana.com/docs/core/transactions/transaction-structure
- Solana Transaction Pipeline: https://solana.com/docs/core/transactions/transaction-pipeline
- Sui Consensus: https://docs.sui.io/develop/sui-architecture/consensus
- Sui Architecture / Object Model: https://docs.sui.io/develop/sui-architecture/
- Sui Object-Based Local Fee Markets: https://docs.sui.io/develop/transaction-payment/local-fee-markets
- Block-STM paper: https://arxiv.org/abs/2203.06871
