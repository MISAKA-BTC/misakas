# RFC-0005: PALW Turing-complete ML VM (PALW-GVM) — general control flow, memory and gas over PALW-TIR precompiles, adjudicated by an interactive fraud proof over the execution trace

| Field | Value |
| --- | --- |
| Status | Draft, 2026-09-28 — design only. Recommended order: the asynchronous EVM rung (§3) when contracts ask for model calls; the fraud-proven VM (§4–§7) only after PALW-TIR is mainnet-safe and the workflow measurement (RFC-0004 §1.2, M4) shows the demand |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-28 |
| Affects | spec/palw 03 (registry: the program kind `Gvm`), new chapter 04e (the general VM), 05 (canonical work: credited per executed call), 07/08/09 (claims, verification, court: the VM trace, the bisection game, one-step proofs), 11 (free-prompt lane: workflow jobs), 16 (fences); spec/evm (a job precompile and a settlement system op) · all networks (dormant until armed) · `consensus/core`, a new crate `misaka-palw-gvm` (emulator and one-step prover), `kaspa-evm` (the asynchronous rung), `misaka-palw-sdk`, a guest toolchain |
| Branch | `rfc/0004-0005-vm` (text only) |
| Related | RFC-0002 (PALW-TIR, 04b, Phase F), RFC-0003 (`TirProgramV2`, pipelines, R, canonical outputs), **RFC-0004** (the bounded layer this one subsumes; versioned program kinds), ADR-0020 (the selected-parent EVM lane), ADR-0089 (the fold is the truth, the EVM is its window and its hand), ADR-0139 (the lanes' gas is one budget a round), ADR-0023 (the three-lane proposal), ADR-0144 P1–P7 (the constitution), ADR-0082 and ADR-0103 (dissection, held regime), ADR-0072 (one inference, one ticket) |

## 概要(日本語)

- **目的。** MISAKA を「未来のモデル」だけでなく「未来の計算そのもの」に強くする。複数モデルのワークフロー、agent、router、
  tool planner、pipeline を、`while`・一般の制御・メモリ・gas 計量・実行 trace の commit を持つ VM で書けるようにし、争いは
  **対話型 fraud proof**(trace を 1 命令まで二分し、その 1 命令を one-step 証明で裁く — optimistic rollup の fault proof と同じ形)で
  決める。テンソル演算は **TIR の precompile** として呼び、既存の TIR court がそのまま裁く。
- **最初の分岐は「制御をどこで動かすか」。** (A) **on-chain**:全ノードが再実行する既存の EVM レーン。fraud proof は要らないが、
  モデル呼び出しは非同期(job を出し、`Final` の後に callback)。(B) **off-chain**:executor がワークフロー全体を 1 つの job として
  実行し、trace を commit、争いは二分探索と one-step 証明。(A) は小さく早い。agent のように呼び出しが多いものは (B) が要る。
- **基盤の評価と推奨。** (B) の VM は **RISC-V RV32IM**(単一 hart、A/F/D/C なし、CSR なし、syscall は `ECALL`)。
  理由:命令は 50 個未満(RV32I の 40 と M の 8)で固定長デコード、ISA として完全に決定論的(ゼロ除算も結果が
  定義済み)、Rust/C の成熟した toolchain、公式の形式モデル(Sail)を第 2 実装・差分検証の oracle に使える、fault-proof VM と
  zkVM で広く使われる(将来、制御部分を validity proof に置き換える道が残る)、one-step prover が小さい。WASM は仕様・検証・機械状態(値スタック・制御スタック・
  テーブル)が大きく one-step 証明が重い。自前 VM は toolchain が無く全バグが自前。**既存 EVM レーンに同期の TIR precompile を
  足す案は不可能**(全ノードが推論を再実行することになる)。off-chain の EVM に one-step prover を作る道は Optimism が一度
  放棄した。EVM レーンは (A) の非同期オーケストレーションに使う(ADR-0089 の writer と settlement と同じ型)。EVM の書き味が
  欲しければ、RISC-V の guest として revm を動かせる(逆はできない)。
- **VM の設計。** 状態 = pc・レジスタ 32・メモリの Merkle root(4 GiB、keyed BLAKE2b-512)・gas・TIR 予算・handle table の
  root・出力の累積 hash・status(約 400 byte)。syscall は `HALT`・`INPUT_READ`・`ORACLE_READ`(外部 tool の応答の transcript、
  FP レーンだけ)・`OUTPUT_WRITE`・`TIR_CALL`・`TENSOR_READ`・`TENSOR_FROM_MEM`(64 KiB まで)・`RAND`・`GAS_LEFT`。
  登録済みの TIR class を class id で呼べる(他の登録者のモデルを組み合わせられる)。
- **trace の commit は混成。** VM 状態を `C_vm`(2^20)命令ごとと各 `TIR_CALL` の前後で checkpoint leaf として eager に commit し、
  その間は lazy:争いになったときだけ executor が中間状態の root を出す k-ary 二分探索。命令ごとに memory root を作るのは
  高すぎる(10^9 命令で 10^10 回規模の hash)。
- **court は 4 段。** (1) 既存の ladder で最初に食い違う leaf を探す。(2) checkpoint の区間なら VM の対話型二分探索
  (≤ ⌈log_k C_vm⌉ 手)。(3) 最後の 1 命令を one-step 証明(命令語とメモリの Merkle 証明、≤ 1 carrier)。(4) `TIR_CALL` の
  record で呼び先の root だけが違えば、呼び先の Phase F step tree に降りて既存の TIR court。RFC-0004 と同じ合成。
- **gas の健全性。** gas は状態の一部で、one-step 証明が毎命令の加算と上限を検査する。1 命令 ≥ 1 gas なので trace の長さ・
  争いの手数・必要メモリがすべて gas で上から抑えられる。refund は無い。TIR の費用は別の予算(MAC・lane・leaf)で数える。
- **報酬と憲法。** PALW の報酬(credited work)は実行された TIR 呼び出しの構造的仕事量の和だけで、VM 命令は 0(推論ではない)。
  ADR-0144 P1(local first):ユーザーが手元で走らせた agent ワークフローを claim するのは P1/P3 の範囲。(A) の「コントラクトが
  リモート executor に払う」呼び出しは報酬経路ではない fee-for-service(eligibility なし)。報酬経路にするのは憲法改正で、
  提案しない。
- **「未来の計算に強い」の正直な範囲。** 制御は何でも書ける。足りない演算も VM で遅い道(スカラー実装)が取れるが、小さい
  テンソルに限る。重い新演算は依然として TIR の prim 改訂が要る。
- **費用。** consensus 面は TIR を 1.0 として +1.2〜1.5(1.5〜2 万行と第 2 の紛争ゲーム)。攻撃面は prover ≡ emulator、
  メモリ証明、gas 表、ゲームの liveness、oracle、EVM callback。**prover が真実を定義する**ので、prover のバグはそのまま
  誤審になる。
- **工数。** (A) の非同期 EVM 段:agent 実装 1〜2 週、mainnet-safe 6〜9 か月(ただし mainnet の EVM レーンは今 inert で、
  その有効化が別に要る)。(B):agent 実装 1〜2 か月、**mainnet-safe 24〜30 か月**(形式手法・監査 3〜4 本・6 か月以上の
  soak・bounty は縮まない。Optimism と Arbitrum の permissionless fault proof も設計から数年かかった)。
- **推奨。** (A) はコントラクト側の需要が出たら作る。(B) は PALW-TIR が mainnet-safe になり、M4(ワークフロー需要)の測定が
  需要を示してから。RFC-0004 と両方は作らない。(B) を作るなら新規 BVM 登録は閉じ、BVM → GVM の正準変換を用意する。

## Summary

Make MISAKA a platform for **general AI computation** — multi-model workflows, agents, routers,
tool planners and pipelines — without giving up the property that makes PALW work: every claimed
result is adjudicable by recomputing one small piece of it.

The RFC separates two questions the brief fused.

1. **Where does control run?** Either on chain, re-executed by every node (the existing EVM lane,
   with model calls made asynchronously through PALW jobs), or off chain, run by the executor as one
   job and fraud-proven.
2. **On what substrate does off-chain control run?** A custom VM, WASM, a RISC-V-style ISA, or MISAKA's
   EVM lane with TIR precompiles.

The recommendations:

- **Rung A — the EVM lane as an asynchronous orchestrator** (§3). A `ModelJobs` precompile escrows a
  fee and queues a PALW job. The job is executed and adjudicated by PALW as any job is, and at `Final`
  a settlement system op calls the contract back with the output's digest and a small inline result.
  No new VM and no new fraud proof: the EVM is already re-executed by every node. It is the ADR-0089
  pattern, extended from the market to inference.
- **Rung B — PALW-GVM, an off-chain RV32IM VM with TIR precompiles** (§4–§7). A whole workflow is one
  claim. Its trace is committed at checkpoints and at every model call. A dispute is a four-level game:
  the existing ladder, an interactive k-ary bisection over instructions inside a checkpoint interval,
  a one-step proof of one RV32IM instruction with Merkle proofs of memory, and — at a model call — a
  descent into the unchanged PALW-TIR court.

Rewards follow executed TIR work only. VM steps are metered by gas and credit nothing. The whole
design composes with RFC-0004 through versioned program kinds, and it supersedes RFC-0004 if both
were ever needed.

It is also expensive: roughly 2.2–2.5 times TIR's consensus surface, and 24–30 months to mainnet-safe
use, most of which no amount of implementation speed shortens.

## Motivation

### 1. Future computation, not only future models

RFC-0002 made a model data. The next frontier is not another architecture but what people build
*from* models:

- **agents** — an LM that plans, calls tools, reads the results and loops until the task is done;
- **routers and cascades** — a small model decides which large one answers, or whether one is needed;
- **tool planners and program executors** — a model writes a program, the program runs, and its
  output feeds the model again;
- **search** — best-of-N with a verifier, tree search with pruning, self-consistency;
- **multi-model pipelines whose structure depends on the data** — retrieval, then reranking, then
  generation, with branches.

What they share: control that is data-dependent with no useful static bound ("until the answer
parses"); memory (assembling contexts, parsing tool output, bookkeeping); many model calls per task;
and glue written in ordinary programming languages.

### 2. What the layers below cannot do

| Layer | Control | Memory | Model calls per task |
| --- | --- | --- | --- |
| PALW-TIR (RFC-0002) | none: a static DAG and a position scan | `Fixed`/`Hist` states only | one program |
| Pipelines (RFC-0003) | stages in a fixed order | edges between stages | a fixed list |
| PALW-BVM (RFC-0004) | bounded `IF`/`FOR`/`CALL` between whole calls | a register file | bounded by static loops |
| **This RFC** | general, metered by gas | byte-addressed, up to 4 GiB | unbounded under gas |

An agent loop that runs until its output parses, manipulating strings in between, is none of the
first three.

### 3. What MISAKA already has

- **PALW**: execution off chain, adjudication on chain; integers only; the court recomputes one cone.
- **The EVM lane** (ADR-0020): every node re-executes it; it is general and gas-priced for L1
  re-execution — Shanghai rules on revm, a chain block ceiling of 390 M gas (30 M plus 120 rounds of
  3 M, ADR-0139). It is live on testnet-11 and testnet-12 from genesis and inert on mainnet.
- **A bridge between them** (ADR-0089): contracts read the PALW fold through precompiles
  (`0x…F010`–`0x…F012`), and a writer (`0x…F013`) escrows value and queues actions that the fold
  settles in the selected child (`MarketSettle`).

A general AI compute platform needs general control *over* PALW's model calls. The two sites of
§1 are the two ways to get it from what exists.

### 4. The constraint: ADR-0144

The constitution's P1 says that rewardable inference is the user's own, run on their own machine, and
that no part of the reward path may assume a remote GPU marketplace. P3 says the execution the user
reads is the one the chain rewards. So:

- a workflow the user runs on their own machine — their agent, their tools, their models — is their
  own inference, and claiming it is within P1 and P3;
- a contract paying a remote executor for a model call (rung A) is **fee-for-service**, outside the
  reward path: it earns the fee and no eligibility. Making it a reward path would be a constitutional
  change. This RFC does not propose one.

## Goals and non-goals

**Goals.**

- G1. General control — `while`, recursion, memory — metered by gas.
- G2. Every tensor computation is a TIR precompile: the fast path stays PALW-TIR, adjudicated by the
  TIR court, which never learns that a VM exists.
- G3. One claim per workflow, whose whole trace is adjudicable: by bisection to one instruction and a
  one-step proof, or by descent into the TIR court at a model call.
- G4. Determinism: no floating point, no clock, no thread, no IO except committed oracles.
- G5. Gas soundness: gas bounds the trace, the memory, the executor's work and the dispute game.
- G6. Composition with the EVM lane, asynchronously, with results delivered only after `Final`.
- G7. Versioned program kinds: RFC-0004's classes, if any exist, are untouched.
- G8. Reuse of PALW's court machinery: clocks, bonds, carriers, the ladder and the held regime.

**Non-goals.** Floating point. Parallel or nondeterministic execution. Network or filesystem access
from inside the VM. zk proofs in v1 (§2 keeps the door open). Running ML inside the L1 EVM. Replacing
the EVM lane. A remote GPU marketplace as a reward path (ADR-0144 P1). Synchronous TIR precompiles in
the L1 EVM (§2).

## 1. Two execution sites

| | Site A — on chain (the EVM lane) | Site B — off chain, fraud-proven (PALW-GVM) |
| --- | --- | --- |
| Who executes the control | every node, in the selected-parent EVM lane | the executor, as one PALW job |
| How the control is verified | re-execution by every node | bisection and one-step proofs, only when disputed |
| Model calls | asynchronous: each is a PALW job settled at `Final`, then a callback | synchronous: each is a `TIR_CALL` inside the job |
| Latency per model call | one job lifecycle (claim, panel, challenge window) | none beyond the job's own |
| Data between calls | digests and small inline results (block space) | unbounded: held off chain, opened in disputes |
| Cost of control | EVM gas for every node | the executor's and the seats' CPU; gas only meters it |
| New consensus machinery | a precompile, an action and a settlement op (ADR-0089's shape) | a VM, a one-step prover, a bisection game, a descent |
| Fits | contracts that need a few model calls: a classifier gate, a scored oracle, a mint conditioned on a model's answer | agents, planners, search: tens to thousands of calls per task |

The recommendation is to build both, in that order, each when its demand is shown. Rung A is small
and nearly free of new risk. Rung B is the real VM, and it carries the real cost.

## 2. The substrate: evaluation and recommendation

### 2.1 The criteria

For Site B the substrate must be adjudicable by a **one-step prover that lives in consensus**. Five
criteria follow from that, and two more from who will write the programs:

1. **The machine state and one step must be small.** The prover recomputes one step from a Merkleized
   state. Every register, stack, frame or table the machine has is something a proof must open.
2. **Determinism by construction**, with no implementation-defined corner to pin down.
3. **An independent reference** for the second implementation and for differential testing.
4. **Precedent** in production fault-proof systems.
5. **Fit with TIR**: tensors live outside the VM, behind handles; the VM needs only integer glue.
6. **Toolchains**: workflows are written in Rust, C or anything that compiles to the target —
   including interpreters for other languages, run as guests.
7. **A path to validity proofs** for the control part later, since ML itself will stay optimistic.

### 2.2 The options

| Option | State and one step | Determinism | Reference / formal model | Fault-proof precedent | Toolchain | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| **EVM lane + synchronous TIR precompiles** | — | — | — | — | Solidity | **impossible**: the L1 lane is re-executed by every node, so a synchronous TIR precompile would make every node run the inference |
| **Off-chain EVM + an EVM one-step prover** | large: 256-bit stack, memory expansion, call frames, the storage trie, precompiles, gas rules | good (Shanghai is pinned) | revm, other clients; formal models exist but are partial | Optimism abandoned its EVM-level fraud-proof design in favour of a MIPS VM running the node's own code | Solidity, Vyper | **no**: the largest prover of all, and a gas schedule built for L1 storage, not for glue |
| **EVM lane, asynchronous** (rung A) | none new: every node re-executes | pinned | revm | — (no fraud proof needed) | Solidity | **yes, for Site A** (§3) |
| **Custom VM** (RFC-0004 plus `while`, memory and gas) | the smallest possible, tailored to handles | by construction | none: we would write both implementations | none | none: a DSL and a compiler to build and to trust | **no**: every bug is ours, and nobody can write programs for it |
| **WASM** (core, integer subset) | medium-large: value stack, control stack, locals, globals, tables, memory | good once floats are refused and stack limits pinned | the spec interpreter; mechanised semantics exist | Arbitrum compiles WASM into a WASM-derived format designed for one-step proofs | excellent | **second choice** |
| **MIPS32** (Cannon-style) | small: registers, `pc`, memory | good | vendor manuals only | production (OP Stack) | legacy (delay slots, a shrinking ecosystem) | **no**: precedent without a future |
| **RISC-V RV32IM** | small: 32 registers, `pc`, memory; fixed 32-bit instructions | by construction (division by zero and overflow have defined results; no floats; no CSRs in the profile) | **Sail**, RISC-V International's formal golden model, as an oracle | RISC-V fault-proof VMs (Asterisc in the OP Stack ecosystem, Cartesi) and most zkVMs (RISC Zero, SP1, Jolt) | excellent (`riscv32im-unknown-none-elf` in Rust; C, Zig; interpreters as guests) | **recommended** |

### 2.3 The recommendation: the PALW-RV32IM profile for Site B, the EVM lane for Site A

- **RISC-V RV32IM wins on the criteria that decide consensus risk.** Fewer than fifty instructions
  (RV32I's forty and M's eight), fixed-width decoding, defined results for every input, a state of 32
  registers and a program counter, and an official formal model that serves as an independent
  reference. Its prover is the smallest of the realistic options, and the ecosystem's zkVMs keep open
  a later move of the *control* part to validity proofs.
- **WASM is the honest second.** Its toolchains are as good and its structured control is friendlier to
  analysis, but its machine state (value and control stacks, frames, tables) makes every one-step proof
  larger. Arbitrum's experience is that WASM must first be transformed for provability.
- **The EVM lane is the right Site A, and the wrong Site B.** It already runs contracts that everyone
  re-executes. What it cannot do is run inference, or be fraud-proven without a new EVM prover. Its role
  is orchestration by callback (§3).
- **EVM semantics inside Site B, if wanted, come as a guest.** revm compiles to RISC-V — zkVMs run it
  that way to prove Ethereum blocks — so a Solidity workflow can run inside PALW-GVM at an interpreter's
  slowdown. The converse is not possible.

## 3. Rung A: the EVM lane as an asynchronous orchestrator

The EVM already runs general control, re-executed by every node. What it lacks is a way to ask PALW
for an inference and to act on the answer. ADR-0089 built the same bridge for the model market — read
precompiles, a writer that escrows and queues, and a settlement the selected child carries — and rung A
extends it from trading to inference.

### 3.1 The flow

```
contract ──call──► 0x…F014 ModelJobs.requestJob{value: fee}(class, input, seed, deadline, callbackGas, valueAtRisk)
                     │ escrows the fee; logs JobRequested (the writer's normalised-record pattern)
                     ▼
PALW fold ── EvmJobRequestedV1 { request_id, requester, class_id, input (inline, ≤ 4 KiB), seed, … }
                     │ any bonded executor claims it; panel; court — exactly as any claim
                     ▼
Final / convicted / deadline ─► the fold decides the settlement
                     ▼
selected child's EVM payload ── system op JobSettled { request_id, Delivered { output_root, inline_result } | Refunded }
                     │ pays the executor (or refunds the requester); stores the result
                     ▼
                  callback(request_id, output_root, inline_result) on the requester, with callbackGas
```

### 3.2 The rules

- **Registration and fences.** `0x…F014` is a call-frame intercept of the writer's shape, registered
  through `register_all_misaka_precompiles` only past `palw_evm_jobs_v1`. Below the fence it is an
  empty account — the F003 idiom — so execution and state roots are byte-identical.
- **Encoding.** 64-byte ids cross the ABI as two `bytes32` words, high half first (ADR-0089's rule).
  Inputs are inline, at most 4 KiB (token ids, a small tensor, a canonical image thumbnail). A larger
  input needs a data-availability lane, which rung A does not add.
- **The job is a pure function.** The class, the inline input and the requester's seed (RFC-0003 R)
  determine the output. **No oracle transcript** (§4.2) is allowed in a requested job: the requester, not
  the executor, chooses every input, so the executor cannot steer the answer.
- **Results only after `Final`.** The callback carries an adjudicated result. A convicted claim is
  re-offered or refunded. A job still open at its deadline is refunded.
- **Value at risk.** A contract may move value on a model's answer. The claim-collateral rule (a false
  claim's largest gain must not exceed what can be recovered) therefore includes the requester's
  declared `valueAtRisk`: a claim on a requested job reserves collateral of at least that amount until
  it is `Final`. A contract that moves more than it declared accepts the residual court risk itself.
- **Fee for service, never a reward path.** The escrowed fee pays the executor. The job earns no quantum,
  ticket or eligibility (ADR-0144 P1: a remote executor serving a contract is a marketplace).
- **Bounded load.** Per-block and per-account caps on requests, as the market's actions have
  (`MAX_MARKET_ACTIONS_PER_EVM_BLOCK`), and a per-request callback gas ceiling inside ADR-0139's budget.
- **Callbacks cannot block settlement.** A reverting callback is recorded. The result stays readable
  through a read precompile (`resultOf(request_id)`), and the fee is settled all the same.

### 3.3 What rung A is, and is not

It gives contracts model calls with the full PALW guarantee, a workflow language (Solidity) with mature
tooling and audit practice, and composability with the market and DeFi. It adds a precompile, an
object, a settlement op and collateral accounting — about the size of one ADR-0089 decision. It is not a
VM for agents: every model call costs one claim lifecycle of latency, gas at every node, and block space
for results. **Mainnet caveat:** the EVM lane is inert on mainnet (`evm_activation_daa_score = u64::MAX`),
and ADR-0023's precondition — an authoritative incremental EVM state backend before the EVM's load
grows — stands. Rung A on mainnet waits for both.

## 4. Rung B: the VM — PALW-GVM v1 on the PALW-RV32IM profile

### 4.1 The machine

- **ISA.** RV32I plus M, at a ratified version named by `isa_id` (the keyed hash of a descriptor, as
  `prim_set_id` is). Nothing else: no C, A, F, D or V extension, and no Zicsr. `FENCE` executes as a
  no-op (one hart, no cache). `FENCE.I`, the CSR instructions, `EBREAK`, `WFI` and every privileged
  instruction end the job in `Faulted(Illegal)`.
- **Registers.** `x0` is hard-wired to 0; `x1`–`x31` are 32-bit; `pc`.
- **Memory.** 2^32 bytes, little-endian, zero except where the image places bytes. Instructions are
  4-byte aligned. A misaligned load or store ends the job in `Faulted(Misaligned)`, so every access
  touches exactly one memory leaf.
- **Defined arithmetic.** RV32M already defines every case: division by zero gives −1 (`DIVU`: 2^32 − 1)
  and a remainder equal to the dividend; `−2^31 / −1` gives `−2^31`, remainder 0. There is no trap and
  no implementation-defined result.
- **Start.** Memory holds the image; `pc` is its entry; `sp` is its stack top; every other register is 0;
  the handle table holds the job's input handles; gas and TIR budget are unused.
- **Ends.** `Halted(code)`, `Faulted(class)`, `OutOfGas`, `OutOfTirBudget`. Every end is a committed,
  adjudicable state; none is a panic.

### 4.2 System calls (`ECALL`, the number in `a7`)

| # | Call | Effect | Gas | One-step evidence |
| --- | --- | --- | --- | --- |
| 0 | `HALT(code)` | ends the job | 0 | — |
| 1 | `INPUT_READ(off, len, dst)` | copies job input bytes (≤ 4,096 per call) | 64 + len | input tiles opened under `input_root`; the destination range's memory proof |
| 2 | `ORACLE_READ(off, len, dst)` | copies bytes of the claim's **oracle transcript** (below) | 64 + len | tiles opened under `oracle_root`; memory proof |
| 3 | `OUTPUT_WRITE(src, len)` | appends bytes to the output stream | 64 + len | the source range's memory proof |
| 4 | `OUTPUT_TENSOR(h, kind)` | emits a handle as a canonical output (RFC-0003 §I.3) | 64 | the handle's table entry |
| 5 | `TIR_CALL(desc)` | runs a segment (§4.3); appends its output handles | 256; the segment's static cost is debited from the TIR budget | the descriptor's memory proof; the `CallRecord` |
| 6 | `TENSOR_READ(h, i)` | `a0` = lane `i` of handle `h` | 64 | the element opened under the handle's root, checked against its proven interval (PALW-TIR-33) |
| 7 | `TENSOR_FROM_MEM(src, dtype, shape)` | a new handle over at most 64 KiB of memory | 64 + len | the source range and its proof; the court re-derives the root |
| 8 | `RAND(coords, dst)` | one digest of R (RFC-0003) in the domain `GVM_UNIFORM_V1`, keyed by the job's seed | 128 | none: the court recomputes it |
| 9 | `GAS_LEFT`, `TIR_BUDGET_LEFT` | the meters | 16 | — |

- **The oracle transcript** is how agents use tools without IO. Everything an agent learns from outside
  — a web page, a tool's answer, a user's reply — is appended by the executor to a transcript, committed
  in the claim as `oracle_root`, and read by offset. The court never judges whether a transcript is
  *true*, exactly as it never judges a prompt (P2). It judges that the computation over it is. A
  transcript is allowed only in a job the user runs for themselves (the free-prompt lane). It is refused
  in a requested job (§3.2), where it would let the executor steer the answer.
- **Handles.** `HandleV1 { root, dtype, shape (rank ≤ 4), origin: Input | Call { ordinal, output } |
  Memory { instret } }`. The table is append-only, holds at most 2^16 entries, and is committed as a
  Merkle tree whose root is in the state. Tensor data never enters VM memory except through
  `TENSOR_READ`, one lane at a time, or leaves it except through `TENSOR_FROM_MEM`.

### 4.3 `TIR_CALL`: the precompile

The descriptor names a segment from the class's segment table, a mode (`Scan`, or `Decode` with
`max_new` and stop ids, as RFC-0004 §3.4), a trip count, input handles and scalars. A segment is either:

- **a carried TIR program** (`TirProgramV2` or a pipeline), admitted with the class; or
- **a registered TIR class, named by its class id.** A workflow can call models other registrants
  listed; their programs are in `tir_classes` and their weights in their artifacts.

The call runs from the segment's initial state, keys its randomness by a per-call seed
(`H64(key "misaka-palw/gvm/call-seed/v1", seed ‖ le32(ordinal))`, as RFC-0004 §3.3 does), is committed
as its own Phase F step tree under
`seg_ctx = H64(key "misaka-palw/gvm/segment-ctx/v1", gvm_ctx ‖ ordinal ‖ segment ‖ input roots ‖ scalars ‖ mode ‖ trip)`,
and appends one handle per output. Its static cost at its trip (MACs, lanes, leaves) is debited from the
TIR budget before it runs; a call the budget cannot cover ends the job in `OutOfTirBudget`. A primitive is
called as a one-node segment.

### 4.4 Memory and state commitments

- **Memory** is a binary Merkle tree over 32-byte leaves, of depth 27 (4 GiB). A node is
  `H64(key "misaka-palw/gvm/mem-node/v1", left ‖ right)`, and a fixed table gives the root of an all-zero
  subtree of every height, so an untouched region costs nothing. A one-leaf proof is 32 bytes and 27
  siblings, about 1.8 KB. A contiguous range is proved by its bytes plus at most two sibling paths.
- **The state** is small enough to open whole:

  ```
  GvmStateV1 { status, pc: u32, regs: [u32; 32], mem_root: Hash64, instret: u64,
               gas_used: u64, gas_limit: u64, tir_used: TirBudgetV1, tir_limit: TirBudgetV1,
               handle_root: Hash64, handle_count: u32, tcalls: u32,
               output_acc: Hash64, output_len: u64 }                     // about 400 bytes
  gvm_state_root_v1(s) = H64(key "misaka-palw/gvm/state/v1", borsh(s))
  ```

### 4.5 The image, the class and admission

- **The image** `GvmImageV1 { entry, stack_top, segments: [(vaddr, bytes)] }` is canonical (sorted,
  non-overlapping, leaf-aligned). Its initial memory root is `image_root`. Its bytes ride in the class's
  artifact, whose availability is already the condition for serving and verifying any class; the chain
  carries `image_root`, `image_len` and the header. The court never needs the whole image: every
  one-step proof opens the one instruction word it executes.
- **The class:**

  ```
  PalwGvmClassV1 { version, image_root, image_len, entry, stack_top,
                   segments: [SegmentRefV1],         // RegisteredClass { tir_class_id } | Carried { graph_ir_root, layout }
                   layout: { c_vm, arity },          // checkpoint interval (a power of two) and bisection arity
                   gas_limit_max, tir_limit_max, tokenizer_id }
  gvm_class_id_v1 = H64(key "misaka-palw/gvm/class-id/v1", borsh(class) ‖ artifact_root)
  ```

- **Admission** (`gvm_admit_v1`) is short, because a Turing-complete program cannot be bounded
  statically and is not asked to be: canonical decoding; header sanity (the entry and the stack inside
  the image's range, `image_len` within the cap); every carried segment through `tir_admit_v1` or pipeline
  admission; every registered segment existing; `gas_limit_max` and `tir_limit_max` within the network's
  ceilings; `c_vm` in `[2^10, 2^24]`; and the court window (§6.6). Gas bounds everything else at run time.

### 4.6 The gas schedule (v1, calibrated before the fence)

| Item | Gas |
| --- | --- |
| every instruction | 1 |
| the first touch of a 4 KiB page | 4,096 — so a job touches at most `gas_limit / 4,096` pages: 4 GiB at the v1 ceiling |
| system calls | the table of §4.2 |
| refunds | none |

v1 ceilings, proposed: `max_gas_per_job = 2^32`; `c_vm = 2^20`; at most `2^14` `TIR_CALL`s per job; the
TIR budget per job within RFC-0003's per-profile ceilings.

## 5. Execution and the trace commitment

A claim commits a two-level step tree, as RFC-0004's does, with one change: the control trace is
checkpointed rather than recorded at every step.

- **Outer leaves, in execution order:**
  - `Checkpoint { instret, state_root }` at every multiple of `c_vm` instructions, and immediately before
    and after every `TIR_CALL`;
  - `CallRecord { ordinal, segment, seg_ctx, inputs, scalars, mode, trip, callee_root, callee_leaf_count,
    outputs }` between the two checkpoints around each call;
  - `End { status, output_root, oracle_root }`, last.
- **Inner trees:** one Phase F step tree per `CallRecord`, unchanged, built under `seg_ctx`.
- **The execution root:**
  `gvm_execution_root_v1 = H64(key "misaka-palw/gvm/execution-root/v1", ctx_hash ‖ outer_root ‖ outer_leaf_count ‖ output_root ‖ oracle_root)`.
- **Between checkpoints nothing is committed.** Committing every instruction would mean a memory root
  per store: about 27 hashes each, some 10^10 hashes for a 10^9-instruction run. At checkpoints the
  executor rehashes only dirty pages, and it keeps a snapshot at each one, so that it can answer any
  bisection move by re-executing at most `c_vm` instructions.
- **Size.** At the v1 ceilings the outer tree has at most `2^32 / 2^20 + 3 · 2^14 + 1 ≈ 53,000` leaves.
  The inner trees cost exactly the executed calls' TIR commitments.
- **The invariant is Phase F's, again.** Every outer leaf is a function of the outer leaves before it,
  the job, the oracle transcript and the inner trees of earlier calls. Every inner leaf is a function of
  the inner leaves before it and of its inputs. So the first divergent leaf is always adjudicable.
- **Resume** (ADR-0133) restarts from the last checkpoint whose state and dirty pages the seat holds.

## 6. The court: four levels

```
claim ─ ladder over outer leaves ─┬─ Checkpoint ─► VM bisection (≤ ⌈log_k c_vm⌉ rounds) ─► GvmOneStep (one instruction or one system call)
        (level 1)                 │                 (level 2)                                  (level 3)
                                  ├─ CallRecord ─► GvmCall (its fields) ─┬─ a field differs → convicted
                                  │                                      └─ only the root differs → descend (level 4):
                                  │                                         ladder over the callee's leaves ─► TirCone | dissection | TirLogits | TirDecodeToken*
                                  ├─ Checkpoint after a call ─► GvmCallReturn
                                  └─ End ─► GvmEnd
```

Everything below the descent is Phase F, unchanged. Everything above it is new.

### 6.1 Level 1: the ladder over the outer leaves

The existing ladder (binary or k-ary, at most 48 rounds) narrows to the first divergent outer leaf `j`,
all of whose predecessors agree with the challenger's honest trace.

- `j` is a `Checkpoint` that follows another checkpoint → level 2, over the instructions between them.
  The initial state, a function of the image and the job, counts as checkpoint zero.
- `j` is the `Checkpoint` right after a `CallRecord` → `GvmCallReturn`: the state after a call is the state
  before it, plus the new handles, `a0` and the meters — one step.
- `j` is a `CallRecord` → `GvmCall` recomputes its deterministic fields from the checkpoint before it
  (the descriptor, read from memory under its proof; the input handles, from the table). A mismatch
  convicts. If only `callee_root` or `callee_leaf_count` differs → level 4.
- `j` is `End`, or one trace ends where the other continues → `GvmEnd`: the status, `output_root` (the
  canonical digest of the output stream and emitted tensors) and `oracle_root`.

### 6.2 Level 2: the interactive VM bisection

- **The dispute** is an instruction interval `(a, b]` whose start root `S_a` both parties hold and whose
  end root `S_b` the challenger disputes.
- **Responder's move** — `CourtGvmBisected { session, round, roots }`: the executor's state roots at the
  `k − 1` interior points of the pinned cut of `(a, b]` (the `palw_attn_dissect` cut arithmetic).
- **Challenger's move** — `CourtGvmChildChosen { session, round, child }`: a child whose start root it
  accepts and whose end root it disputes. One always exists while the interval's start is agreed and its
  end is not.
- **Rounds**: at most `⌈log_k c_vm⌉` — 7 at `k = 8` and `c_vm = 2^20`. The bottom is one instruction
  `(s, s + 1]`.
- **Clocks and signatures** are ADR-0082's: one rung window per move, and silence loses. Moves are signed
  with ML-DSA-87 under the responder's and challenger's contexts, over messages that open with their own
  domains (`misaka-palw/gvm/bisect/round/v1`, `…/choice/v1`).
- **There is no fold check**, unlike a sum's dissection: intermediate roots cannot be checked against each
  other, and they need not be. A responder that lies somewhere has a first lying root, and the challenger
  can always follow the child that contains it.

### 6.3 Level 3: the one-step proof

```
GvmOneStepV1 { binding, s,
               pre: GvmStateV1,                       // its root is the agreed S_s
               fetch: (u32 word, MemProofV1),         // the instruction at pre.pc, under pre.mem_root
               access: Option<(leaf, MemProofV1)>,    // the one leaf a load or a store touches
               syscall: Option<SyscallEvidenceV1> }   // §4.2's column
```

1. `gvm_state_root_v1(pre)` equals the agreed `S_s`, and `pre` is well formed.
2. The fetched word is proved under `pre.mem_root` at `pre.pc`.
3. Exactly one instruction or system call is executed by the profile's semantics. A store recomputes
   `mem_root` from the same path. Gas and the meters are updated and checked; a step that would pass
   `gas_limit` produces the state `OutOfGas`.
4. `gvm_state_root_v1(post)` is compared with the claimed `S_{s+1}`: different → `ExecutorGuilty`; equal →
   `ChallengerDefeated`.

- **Size.** About 400 bytes of state, two proofs of about 1.8 KB, and at most about 70 KB of evidence
  (`TENSOR_FROM_MEM`'s 64 KiB range): always one carrier (PALW-TIR-38).
- **Either party may file it.** The root `S_s` is agreed, so the challenger can supply the preimage and
  the proofs from its own honest memory.
- **One step function.** The emulator and the prover are one Rust function over a memory trait (full
  memory, or proofs). Cannon and Arbitrum wrote the emulator and the prover in different languages;
  here both are Rust in consensus, which removes that class of divergence. The independent second
  implementation and the Sail oracle guard the shared function itself (§*Effort*).

### 6.4 Level 4: the descent into the tensor court

The descent is RFC-0004 §6.4, unchanged. The callee's binding is Phase F's `PalwTirStepBindingV1`, with job
context `seg_ctx`, the segment's class and `callee_root`. `External` inputs are opened under the input
handles' roots: earlier callees, job inputs, or memory-made handles whose roots the one-step proof of
`TENSOR_FROM_MEM` fixed. `Random` inputs are recomputed from the per-call seed. The Phase F arms decide,
charged to the GVM claim's executor. The tensor court never learns that a VM exists.

### 6.5 The held regime

No bisection is played on testnet-12's held regime (ADR-0103), and the accuser names a leaf in one move.

- A `CallRecord`, a call return or an `End` is decided in that move.
- An inner leaf is decided as RFC-0004 §6.2 describes.
- A `Checkpoint` cannot be decided in one move, since the instructions between checkpoints are not
  committed. It opens a session at the VM bisection phase, and the executor's intermediate roots are its
  first move — the pattern RFC-0002 F7 set for dissected tiles.

### 6.6 The window

Admission checks the extended O-5 inequality:

```
(2 · (B_outer + R_vm + B_inner + R) + t + 1) · D + 2 · 4 · max_close_chunks  <  window_court
```

where `R_vm = ⌈log_k c_vm⌉`. At `k = 8`: `B_outer ≤ 6` (about 53,000 leaves), `R_vm = 7`, and `B_inner ≤ 11`
for a `2^32`-leaf callee, plus any dissection rounds — inside the 48-round cap.

### 6.7 Malformed statements

A `Checkpoint` preimage that is not a well-formed state — `gas_used` above `gas_limit`, a handle count
beyond the table, a running status followed by `End` — or a `CallRecord` naming no segment of the class,
convicts the executor whichever leaf was disputed (`GvmMalformedState`). This is PALW-TIR-33's rule one
level up; the root binds the preimage, so a challenger cannot manufacture one.

### 6.8 Why a lie is always convictable (informative)

The honest trace is unique: a function of the job, the transcript, the params and the image. A
dishonest claim first differs at some outer leaf with agreed predecessors.

- A checkpoint interval whose end differs contains a first differing instruction. Bisection reaches it,
  because the challenger always follows a child with an agreed start and a disputed end, and the one-step
  proof recomputes it from the agreed state.
- A call whose fields differ is recomputed directly.
- A callee whose root differs contains a first divergent inner leaf, which Phase F convicts (04b §9.5.7,
  Phase F §2.5).

An honest executor's every root and leaf is an evaluation, so every close acquits it.

### 6.9 What the court adds, as objects

- Close proofs appended to `PalwCourtVerdictProofV2`: `GvmOneStep`, `GvmCall`, `GvmCallReturn`, `GvmEnd`.
- Ladder phases: `CourtGvmBisected`, `CourtGvmChildChosen`, `CourtGvmDescended`.
- A one-move accusation, `GvmShardCourtAccused`.
- A step fault, `GvmMalformedState`.
- Two tables, `gvm_classes` and `gvm_bisections`, rooted only once written.

## 7. Gas-accounting soundness, denial of service and determinism

### 7.1 Gas soundness

1. **Termination.** Every instruction costs at least 1 gas, so `instret ≤ gas_limit`. The trace is finite,
   and its bound is known before the job starts.
2. **Checked at every step.** `gas_used` is in the state, and every one-step proof checks both the
   increment and the limit. A trace that runs past its limit has a first step whose honest post-state is
   `OutOfGas` and whose claimed one is not, and that step convicts.
3. **Memory is bounded.** First-touch gas limits a job to `gas_limit / 4,096` pages.
4. **The game is bounded.** At most `gas_limit / c_vm + 3 · tcalls + 1` outer leaves, at most
   `⌈log_k c_vm⌉` VM rounds, and one-step proofs of bounded size (every system call's evidence is capped).
5. **TIR work is bounded separately.** The TIR budget is debited at the segment's static cost before the
   call runs. Inside the call, the callee's own admission bounds hold.
6. **No refunds.** Gas only grows, which keeps 1–4 simple. EVM gas refunds are a known source of
   complexity and bugs, and this VM has no reason to copy them.
7. **What gas does not claim.** It is not the price of inference (TIR work is), not a reward unit, and not a
   bound on a JIT's wall-clock time — only on the reference emulator's work, up to a constant.

### 7.2 Denial of service

- **Registration** is linear in the carried bytes. The image stays off chain, and segments are admitted as
  TIR.
- **Acceptance** checks that the declared `gas_limit` and TIR budget are within the class's and the
  network's ceilings.
- **Verification.** A seat that re-executes a job pays its gas and its TIR work, both bounded and known
  at acceptance. The VM's share is capped per job, so VM-heavy jobs cannot starve panels (open question 4).
- **The court** has bounded rounds and bounded proofs, and one adjudication slot per block as today.
- **Oracle transcripts** are bounded per job (proposed 16 MiB), committed by root, and never on chain.

### 7.3 Determinism

- The profile leaves nothing implementation-defined. Every system call is a pure function of the state
  and of committed data. There is one hart, no clock, and no randomness but R.
- An implementation may interpret, translate or JIT, but it must equal the reference step function bit
  for bit at every checkpoint — RFC-0002's F-1 rule, applied to the VM.
- The toolchain is outside consensus. The chain judges the image, not the source: a miscompiled guest is
  its author's problem, as a mis-lowered TIR program is.

## 8. Work, rewards and the principles

- **Credited work is executed inference, and nothing else.** A GVM claim is credited `Σ` over its
  `CallRecord`s of the callee's structural work at the executed trip — RFC-0004 §7's rule. VM
  instructions credit nothing: glue is not inference (P1), and it is cheap to multiply. The calls are in
  the committed trace, so the credited work is adjudicable.
- **Lanes.** A user's own workflow, run on their machine, is claimed in the free-prompt lane, which
  already credits per job (PALW-FP-5). The attempt lane is not for GVM classes: a canonical job of a
  Turing-complete program has no static work to price.
- **Gas pays nobody** in the free-prompt lane. It is a declared limit that bounds the trace, the game and
  the seats' verification work.
- **Rung A is fee-only** (§3.2).
- **Calls into other registrants' classes are verified use of those classes.** Whether that use counts
  toward the called class's weight or share (ADR-0137) is open question 7.

## 9. Program kinds, and RFC-0004

- `Gvm(1)` joins the program kinds of RFC-0004 §2.1. No other kind's class changes.
- **If PALW-BVM exists**, `palw_gvm_v1`'s value carries a height after which new `Bvm(1)` registrations are
  refused. Existing BVM classes keep the BVM court. `bvm_to_gvm_v1` translates a BVM program into a GVM
  image with a small runtime (the register file in memory, `TCALL` as `TIR_CALL`, `READ` as
  `TENSOR_READ`), so tooling can re-register it as a new GVM class beside the old one.
- **If PALW-BVM was never built** — the recommended outcome — there is nothing to do.
- The formats line up by design: a BVM trace is a GVM trace with a checkpoint at every step and no
  memory, and a `CallRecord` and its descent are the same in both.

## 10. "Strong against future computation": the honest scope

- **Control: anything computable, within gas.** Agents, planners, search, tool use, program execution,
  routers and cascades are all programs.
- **Missing tensor arithmetic gets a slow path, not a fast one.** A guest can read lanes with
  `TENSOR_READ`, compute in RV32IM and write back with `TENSOR_FROM_MEM` (64 KiB per call). That serves
  small operations: a new activation over a 4,096-wide row costs on the order of 10^5 instructions per
  token. It does not serve heavy ones: a new attention variant over 10^4 positions and 32 heads is
  10^9 instructions or more per token. **Heavy new operations still need a TIR prim-set revision.**
- **Data-dependent control inside a model** is expressible, but per-token control through calls pays a
  trace per position. Guarded TIR (RFC-0004 *Alternatives*) remains the better tool for it.
- **Future proof systems.** The RISC-V choice leaves open proving the control part with a zkVM later,
  while the ML stays under the optimistic TIR court.

## Proposed Spec text

### New chapter `spec/palw/04e-general-vm.md` (applies past `palw_gvm_v1`)

- **PALW-GVM-1 (the machine).** A GVM class's control MUST be the execution of its image on the
  PALW-RV32IM profile named by `isa_id`. No other instruction has consensus meaning.
- **PALW-GVM-2 (defined results).** Every instruction MUST have the profile's result. An illegal instruction
  and a misaligned access MUST end the job in `Faulted(class)`.
- **PALW-GVM-3 (system calls).** Only the system calls of §4.2, under `syscall_set_id`, MAY exist.
- **PALW-GVM-4 (tensor arithmetic is TIR).** The VM MUST NOT compute on a tensor except through
  `TENSOR_READ` and `TENSOR_FROM_MEM`. Every `TIR_CALL` MUST be a Phase F execution under its `seg_ctx`.
- **PALW-GVM-5 (gas).** Every instruction MUST cost at least 1 gas under `gas_schedule_id`. There MUST be
  no refund. A step that would pass `gas_limit` MUST produce `OutOfGas`.
- **PALW-GVM-6 (the TIR budget).** A `TIR_CALL` MUST debit its segment's static cost at its trip before
  the segment runs, and MUST produce `OutOfTirBudget` if the budget cannot cover it.
- **PALW-GVM-7 (state and memory).** The state MUST be `GvmStateV1`, and memory MUST be committed by
  §4.4's tree.
- **PALW-GVM-8 (the image).** `image_root` MUST be the initial memory root. The image's bytes MUST be in
  the class's artifact.
- **PALW-GVM-9 (canonical form and identity).** Admission MUST refuse bytes that are not a canonical
  encoding. The class id MUST be `gvm_class_id_v1`.
- **PALW-GVM-10 (admission).** Admission MUST check the header, admit every carried segment as TIR,
  resolve every registered segment, apply the network's ceilings and check §6.6's inequality.
- **PALW-GVM-11 (the trace).** A claim MUST commit a `Checkpoint` at every multiple of `c_vm` instructions
  and around every `TIR_CALL`, a `CallRecord` for every call, `End` last, and one inner tree per call.
- **PALW-GVM-12 (bisection).** A disputed checkpoint interval MUST be bisected by §6.2's game: the pinned
  cut, a responder's roots, a challenger's choice, a clock per move, and silence losing.
- **PALW-GVM-13 (one step).** The bottom MUST be decided by recomputing one instruction or one system call
  from the agreed pre-state.
- **PALW-GVM-14 (calls and descent).** A `CallRecord`'s deterministic fields MUST equal what the state
  before it dictates. A dispute over its root MUST descend into its inner tree and be decided by the
  Phase F arms.
- **PALW-GVM-15 (malformed statements).** A malformed checkpoint preimage or `CallRecord` MUST convict the
  executor.
- **PALW-GVM-16 (oracle transcripts).** A transcript MUST be committed by `oracle_root` and read only by
  offset. No rule MAY judge its truth. It MUST be refused in a requested job (PALW-EVJ-3).
- **PALW-GVM-17 (randomness).** Randomness MUST be R under the per-call seed or the domain
  `GVM_UNIFORM_V1`. Nothing else MAY enter.
- **PALW-GVM-18 (credited work).** A claim's credited work MUST be `Σ` over its calls of the callee's
  structural work at the executed trip. VM steps MUST credit nothing.
- **PALW-GVM-19 (implementations).** Any implementation MUST equal the reference step function at every
  checkpoint.
- **PALW-GVM-20 (versioned kinds).** As PALW-BVM-18.

### Additions to `spec/evm` (applies past `palw_evm_jobs_v1`)

- **PALW-EVJ-1 (the door).** `0x…F014` MUST be registered only past the fence. Below it, it is an empty
  account.
- **PALW-EVJ-2 (the request).** A request MUST escrow its fee, log its normalised record, and carry an
  inline input of at most 4 KiB and a seed.
- **PALW-EVJ-3 (a pure job).** A requested job MUST be a function of its class, its input and its seed. An
  oracle transcript MUST be refused.
- **PALW-EVJ-4 (settlement).** Only the fold's decision, carried as a system op by the selected child, MAY
  settle a request: `Delivered` after `Final`, and `Refunded` after a conviction or at the deadline.
- **PALW-EVJ-5 (callbacks).** A callback MUST follow its settlement. Its failure MUST NOT revert the
  settlement, and the result MUST stay readable.
- **PALW-EVJ-6 (value at risk).** A claim on a requested job MUST reserve collateral of at least the
  request's declared `valueAtRisk` until it is `Final`.
- **PALW-EVJ-7 (not a reward path).** A requested job MUST NOT earn a quantum, a ticket or eligibility.
- **PALW-EVJ-8 (bounded load).** Requests MUST be capped per block and per account, and callback gas MUST
  fit ADR-0139's budget.

## Activation plan

Two fences, one per rung, each with Phase F D1's shape: Some-only in both fingerprints, the whole option
collapsing to `never()`, activation-only in `for_each_fence`, at an unused height.

### Rung A: `palw_evm_jobs_v1`

`Option<PalwEvmJobsFenceV1 { activation, request_caps, callback_gas_ceiling }>`, refused by
`validate_palw_v2` unless the EVM lane, ADR-0089's market fence (`palw_model_evm`) and `palw_tir_v1` are
active at or below it.

| Step | Work | Exit gate |
| --- | --- | --- |
| 1 | spec/evm additions; the precompile, the request object, the settlement op, the collateral reservation | executor ↔ `eth_call` parity (the one registration seam, `register_all_misaka_precompiles`) |
| 2 | Drills: D-EJ1 a contract requests a TIR job and is called back after `Final`; D-EJ2 a convicted claim is re-offered and refunded; D-EJ3 a deadline refund; D-EJ4 value-at-risk collateral held and released; D-EJ5 the fence crossing on the shipping binary | all pass |
| 3 | testnet-12 (or its successor) at an unused height; one audit; at least 3 months armed | no unresolved defect |
| 4 | mainnet | only once the EVM lane is active on mainnet and ADR-0023's state-backend precondition is met |

### Rung B: `palw_gvm_v1`

`Option<PalwGvmFenceV1 { activation, isa_id, syscall_set_id, gas_schedule_id, court_version, ceilings,
bvm_admission_closed_at }>`, refused unless `palw_tir_v1`, `palw_gen_v1` and `palw_kary_court` are active
at or below it and the ruleset has the A-2 tolerance.

| Step | Work | Consensus change | Exit gate |
| --- | --- | --- | --- |
| 0 | M4 (RFC-0004 §1.2): workflow demand | none | the decision to build, by the user |
| A | Spec chapter 04e | none | reviewed text |
| B | Reference emulator with checkpointing (`misaka-palw-gvm`) | none | golden vectors `consensus-vectors/gvm-v1/`: every instruction on edge operands, every system call, gas, every fault |
| C | Independent second implementation; differential against Sail | none | at least 10^9 instruction-level steps against Sail with no disagreement; the vectors |
| D | One-step prover and proof formats | none | the prover equals the emulator on every step of fuzzed programs; hostile proofs refused totally |
| E | Admission, objects, the game, the descent | dormant fence | a court battery: a planted lie at every instruction class, every system call, every call field, every end case and inside callees; delay and silence cases; honest runs acquitted |
| F | Node side and guest SDK | node release | executor equals the reference; the responder answers every move within its rung window |
| G | Drills: D-G1 the court battery on a salted t12 chain; D-G2 the forged-output red-team on a GVM class; D-G3 the fence crossing on the shipping binary; D-G4 an agent (an LM, a tool model and a transcript) end to end; D-G5 a worst-case dispute at the gas ceiling inside the window | — | all pass |
| H | The live testnet, staged: 0‰ weight; gas at most `2^28` and a small TIR budget at first; ceilings raised by fence values after each soak stage | fence | 6–9 months or more with no unresolved court defect |
| I | Mainnet | fence | audits closed, the bounty run, the formal results in |

## Effort to mainnet-safe use

### Calibration

RFC-0004's *Effort* section gives the evidence from RFC-0002 in full. Implementation compressed by one to
two orders of magnitude with agents. The bugs that mattered — fifteen specification defects, a
verdict-changing admission disagreement, a court arm that panicked on hostile input, a lie that ended
with nobody slashed, a wrong-token lie that escaped the bisection terminal — were found by independent
reimplementation, differential testing and drills, the work that does not compress. Two things make
this RFC heavier than RFC-0004:

- **The prover defines the truth.** A bug in the one-step prover or in the bisection game is not a crash.
  It is a verdict: an honest executor convicted, or a liar acquitted. That is why fault-proof systems
  spend most of their calendar on independent implementations, formal work, audits and staged rollouts.
- **Outside practice.** Optimism's permissionless fault proofs reached mainnet in 2024, about two years
  after its MIPS VM began and after an earlier EVM-level design had been abandoned, and they needed a
  corrective upgrade within months of launch. Arbitrum's permissionless BoLD reached mainnet in 2025,
  years after its first fraud-proof design. Both teams were larger than two to four people.

### Rung A — the asynchronous EVM orchestrator

| Phase | Work | Engineer-months | Calendar | With agents | Compresses? |
| --- | --- | --- | --- | --- | --- |
| Spec | spec/evm additions, the object, the settlement | 0.5 | 2–3 weeks | about 1 day | review does not |
| Implementation | precompile, request object, settlement op, collateral reservation, `eth_call` parity | 1–1.5 | 4–6 weeks | 3–5 days | yes |
| Drills and differential testing | D-EJ1…D-EJ5 | 0.5–1 | 3–4 weeks | 2–3 days | mostly |
| External audit | one engagement and fix review | 0.5 | 1.5–2 months | — | no |
| Testnet soak and bounty | at least 3 months armed | 0.5 | 3 months or more | — | no |
| **Total** | | **≈ 3–4 engineer-months** | human-only ≈ 7–10 months | implementation ≈ 1–2 weeks | |

### Rung B — PALW-GVM

| Phase | Work | Engineer-months (2–4 experienced people) | Calendar | With agents | Compresses? |
| --- | --- | --- | --- | --- | --- |
| Spec | ISA profile, system calls, memory, state, gas, the game, the image, the precompile bridge | 3–4 | 2–3 months | 2–4 days to draft | review does not |
| Reference emulator | with checkpointing and snapshots | 2–3 | 6–8 weeks | 2–4 days | yes |
| Independent second implementation | from the text; differential against Sail | 2–3 | 2–3 months | 3–5 days | the writing does; triage does not |
| Program verifier | image and class admission; gas calibration | 1–1.5 | 4–6 weeks | 2–3 days | yes |
| One-step prover | the shared step function over proofs; proof formats | 3–4 | 2–3 months | 3–5 days | yes |
| Court arms | game objects and clocks, checkpoint leaves, descent, held regime, window, bonds | 5–7 | 3–4 months | 1–2 weeks | yes |
| Node side | executor, snapshots, responder, memory cache, guest SDK, the Rust target, TIR bindings, the tool | 6–8 | 3–4 months | 2–3 weeks | yes |
| Fuzzing and differential testing | instruction level against Sail; random programs; proof round trips; adversarial game simulation | 3–4 | 4–6 months, then continuous | about a week of harness, months of CPU | the CPU does; triage does not |
| Formal methods | the step function against the ISA semantics, mechanised against Sail; soundness of memory proofs; termination and liveness of the game | 4–8 | 4–6 months | assists only | mostly not |
| External audits | three or four engagements: emulator and prover; game and court; integration and economics; guest SDK | 3–4 (fixes) | 6–9 months | — | no |
| Testnet soak with staged caps and drills | D-G1…D-G5, the staged ceilings of step H | 2–3 | 6–9 months or more | — | no |
| Bug bounty | large, before and after activation | 1 | at least 6 months before activation | — | no |
| Mainnet activation | staged caps | 1 | 2–3 months | — | no |
| **Total** | | **≈ 36–52 engineer-months** | human-only ≈ 30–40 months | implementation ≈ 40–70 agent-days | |

### The two figures

- **Implementation with AI agents.** Rung A: about 1–2 calendar weeks to passing drills. Rung B: about
  1–2 calendar months to a GVM class passing D-G1…D-G5 on a salted testnet-12 chain, with three or four
  agents and a lead who reviews and integrates.
- **Mainnet-safe.** Rung A: about 6–9 months, and only once the EVM lane is active on mainnet. Rung B:
  **about 24–30 months** from its start, whatever the implementation speed, and only after PALW-TIR is
  mainnet-safe. The critical path is: implementation (1–2 months), differential and formal work (4–6
  months), three or four audits with fixes (6–9 months, overlapping a staged soak of at least 6–9
  months), a bounty window of at least 6 months before activation, and a staged activation (2–3 months).

## Comparison: TIR only, TIR + Bounded VM, TIR + EVM-class VM

The same table appears in RFC-0004.

| | TIR only (RFC-0002/0003) | TIR + Bounded VM (RFC-0004) | TIR + EVM-class VM (this RFC) |
| --- | --- | --- | --- |
| Coverage | ≈ 91 % of HF decoder checkpoints by count today; 100 % of the 54 modelled architectures; the rest are lowerer and primitive gaps | + the `NEEDS_VM` share: **0 today**; in future, routing between whole models (C13) and very long loops | + everything computable within gas: agents, tool use, planners, search; a slow path for small missing operations |
| Workflows | static pipelines (RFC-0003) | bounded and data-dependent, between whole model calls | unbounded under gas, with memory and recursion |
| Consensus surface (index; TIR = 1.0 ≈ 14k lines of consensus-path code plus 04b's 1,800 lines) | 1.0 | ≈ 1.3 (+ 4–5k lines, chapter 04d) | ≈ 2.2–2.5 (+ 15–20k lines and a second dispute game) |
| Court complexity | ladder → cone or H dissection (2 levels) | + an outer control trace of one-step transitions over ≤ 8 KiB states, and a descent (3 levels) | + interactive bisection inside checkpoints, one-step RV32IM proofs with memory proofs, syscall and precompile descent (4 levels) |
| Attack surface | interpreter, admission, dissection | + VM interpreter, the bounds program, the ladder's agreement, the step space | + prover ≡ emulator, memory proofs, the gas schedule, game liveness, oracles, EVM callbacks |
| Effort to mainnet-safe, after TIR | TIR's own path | + 9–12 months (agents: 2–4 weeks to drill-passing code) | + 24–30 months (agents: 1–2 months); the asynchronous EVM rung alone ≈ 6–9 months |
| Recommendation | build and ship (under way) | keep the design; build it only if RFC-0004's §1 gate opens; never build both BVM and GVM | the asynchronous EVM rung when contracts ask for it; the fraud-proven VM only after TIR is mainnet-safe and M4 shows the demand |

## Alternatives

| Alternative | Why not, or when |
| --- | --- |
| **Stop at pipelines and, if its gate opens, RFC-0004** | Right if M4 shows no demand for agents and workflows. Nothing in this RFC should be built on speculation |
| **Synchronous TIR precompiles in the L1 EVM** | Impossible: every node re-executes the lane, so every node would run every inference |
| **Off-chain EVM with an EVM one-step prover** | The largest prover of all (256-bit stack, memory expansion, frames, the storage trie, precompiles), and a gas schedule built for L1 storage. Optimism abandoned this path |
| **WASM** | The second choice (§2): as good a toolchain, a larger machine state and heavier one-step proofs |
| **A custom VM** | No toolchain, no reference model, and every bug is ours |
| **MIPS (Cannon-style)** | Production precedent, but a legacy ISA without RISC-V's toolchains, formal model or zkVM future |
| **Validity proofs (zkVM) for the whole job** | Proving ML is orders of magnitude costlier than re-executing it (RFC-0002, RFC-0003). The control part alone could move to a zkVM later, and the RISC-V choice keeps that open |
| **Committing every instruction** | About 10^10 hashes for a 10^9-instruction run (§5) |
| **Fully lazy commitment, with no checkpoints** | One-move localisation under the held regime would be lost, and every dispute would bisect the whole trace |
| **A remote-compute marketplace as a reward path** | ADR-0144 P1 forbids it. Not proposed; rung A is fee-for-service |
| **Gas refunds, EVM-style** | They complicate soundness (§7.1) for no benefit here |
| **Oracle transcripts in requested jobs** | They would let the executor steer the answer the requester pays for (§3.2) |

## Security and economic analysis

- **The prover is the truth.** A defect in the step function, the memory proofs or the game convicts
  honest executors or acquits liars. Mitigations: a minimal profile; one step function shared by the
  emulator and the prover; an independent second implementation; differential testing against Sail at
  instruction level; a mechanised proof of the step function against the ISA semantics; three or four
  audits; a staged soak with value caps; a large bounty.
- **Emulator and prover divergence** would convict honest executors (unfair) or strand disputes (a
  liveness failure). Sharing the step function removes the cross-language class of divergence; the
  vectors and the Sail differential cover the rest.
- **Game attacks.** Delay is bounded by the round counts and the rung clocks, and silence loses. Griefing
  is priced by the existing accusation charges. Resource exhaustion is bounded by the proof sizes. Several
  challengers follow the existing session rules.
- **Data availability.** The image rides in the artifact, available exactly when weights are. Inputs,
  transcripts and dirty pages are the executor's to serve in a dispute, and silence loses, as today.
- **Oracle transcripts** are inputs, never judged for truth (P2). They are confined to jobs the user runs
  for themselves and refused in requested jobs.
- **Handles cannot be forged.** They come only from job inputs, `TIR_CALL` outputs and `TENSOR_FROM_MEM`,
  and the table is committed.
- **Gas mispricing** could let jobs impose verification costs out of proportion to their declared limits.
  Mitigations: calibration before the fence, per-job ceilings, no refunds, and a cap on the VM's share of a
  job's verification work (open question 4).
- **Memory denial of service** is priced by first-touch gas.
- **Determinism risks** are the profile's to exclude, and it leaves nothing implementation-defined. A JIT
  must match the reference at every checkpoint.
- **Rung A callbacks.** Reentrancy is the contract author's concern, as for any call. Settlement cannot be
  blocked by a failing callback. Results arrive only after `Final`. The value-at-risk reservation brings
  the claim-collateral rule to the value a callback can move.
- **Post-quantum posture.** PALW-GVM and every PALW object are signed with ML-DSA-87. The EVM lane's
  secp256k1 stays confined to that lane (ADR-0020).
- **Economics.** Credited work is executed TIR work only (§8). A busy loop earns nothing, and a workflow's
  inference earns what that inference would earn called directly.
- **The principles.** P1 and P3: a user's own workflow is their own inference, and rung A is outside the
  reward path. P2: no rule reads what a transcript or a branch means. P4 and P5: credit follows verified
  executed work. P7: registration stays permissionless, and calls into other registrants' classes are
  verified use of those classes.

## Compatibility and migration

- TIR, pipeline, legacy and BVM classes are unchanged. Both fences are dormant until armed. Every new
  object is appended, dropped by name below its fence and skipped by older builds under A-2.
- Rung A adds one system address, an empty account below its fence (the F003 idiom).
- The EVM lane is inert on mainnet, so rung A on mainnet waits for the lane's own activation there and
  for ADR-0023's precondition.
- If PALW-BVM exists, §9 describes its closing and its translation.

## Open questions

1. **Build order and evidence**: rung A when contracts ask for it; rung B only after PALW-TIR is
   mainnet-safe and M4 shows workflow demand (recommended).
2. **The ISA**: RV32IM (recommended), or RV64IM — native 64-bit arithmetic, but a larger state, the `W`
   instructions, and a mostly 32-bit zkVM ecosystem.
3. **The checkpoint interval and the arity**: `c_vm = 2^20` and `k = 8` (recommended), to be measured.
4. **Verification cost of the VM part**: a per-job cap on gas relative to TIR work, or a fee to seats.
5. **Oracle transcripts**: free-prompt lane only (recommended), or never.
6. **Image carriage**: in the artifact with the root on chain (recommended), or on chain across carriers.
7. **Calls into registered classes of other registrants**: allowed (recommended); whether that use counts
   toward the called class's weight or share (ADR-0137).
8. **Rewards**: executed TIR work only (recommended); VM steps credit nothing.
9. **Rung A on mainnet**: tied to the EVM lane's mainnet activation and ADR-0023's precondition.
10. **Validity proofs for the control part later**: a zkVM over the same profile, with the TIR court
    unchanged.
11. **Superseding RFC-0004**, if it was built: close BVM admissions at `palw_gvm_v1` and ship
    `bvm_to_gvm_v1`.
12. **Value at risk in rung A**: the declared-cap rule (recommended), or a network-wide cap per request.

## Decision

<Open.> The drafter's recommendation:

- **Rung A**: specify it now; build it when a contract use case asks for model calls. It is small, reuses
  ADR-0089's machinery, and adds no fraud proof.
- **Rung B**: do not start it before PALW-TIR is mainnet-safe and M4 shows the demand. When it starts,
  budget for 24–30 months to mainnet-safe use, whatever the implementation speed.
- **Never both RFC-0004 and rung B.** If the workflow case is proven, rung B subsumes the bounded layer.
