# RFC-0005: PALW Turing-complete ML VM (PALW-GVM) and Verified Model Improvement — general control, memory and gas over PALW-TIR precompiles, adjudicated by an interactive fraud proof; and a market that makes registered models stronger by verified distillation and RL

| Field | Value |
| --- | --- |
| Status | Draft, 2026-09-29 — design only. **Part I:** the asynchronous EVM rung (§3) when contracts ask for model calls; the fraud-proven VM (§4–§7) only after PALW-TIR is mainnet-safe and the workflow measurement (RFC-0004 §1.2, M4) shows the demand. **Part II:** its first phase (P0, mathematics, no VM) once `palw_tir_v1` and rung A run on a testnet |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-28 |
| Affects | spec/palw 03 (registry: the program kind `Gvm`), new chapter 04e (the general VM), 05 (canonical work: credited per executed call), 07/08/09 (claims, verification, court: the VM trace, the bisection game, one-step proofs), 11 (free-prompt lane: workflow jobs), 15 (model lines: a governor and a lineage head, Part II), 16 (fences); spec/evm (a job precompile and a settlement system op) · all networks (dormant until armed) · `consensus/core`, a new crate `misaka-palw-gvm` (emulator and one-step prover), `kaspa-evm` (the asynchronous rung), `misaka-palw-sdk`, a guest toolchain, and EVM-lane contracts for the improvement market (Part II) |
| Branch | `rfc/0004-0005-vm` (text only) |
| Related | RFC-0002 (PALW-TIR, 04b, Phase F), RFC-0003 (`TirProgramV2`, pipelines, R, canonical outputs), **RFC-0004** (the bounded layer this one subsumes; versioned program kinds), ADR-0020 (the selected-parent EVM lane), ADR-0089 (the fold is the truth, the EVM is its window and its hand), ADR-0139 (the lanes' gas is one budget a round), ADR-0023 (the three-lane proposal), ADR-0144 P1–P7 (the constitution), ADR-0082 and ADR-0103 (dissection, held regime), ADR-0072 (one inference, one ticket), ADR-0088 and spec 15 (model lines, versions and the market), ADR-0044 F5/F15 (beacons) |

## 概要(日本語)

**Part I — Turing 完全な ML VM(PALW-GVM)**

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

**Part II — 検証つきモデル改良(Verified Model Improvement)**

- **目的。** 外部から登録されたモデル(例:Qwen v12)を、実利用が生む hard case と収益を使って、蒸留と RL で強くする。
  流れは、実利用 → hard case と収益 → Distillation Pool → 教師(open モデル・ライセンス済み API・人間+AI・tool・solver・
  チェーン自身のモデル)が Teaching Artifact を提出 → 品質検査を経て dataset → 候補 A/B/C を学習 → PALW 評価で勝者 v13 →
  繰り返し。**agent mining であって agent rental ではない**:MISAKA は agent も GPU も借りず、届いた成果物と測定された改良に
  だけ払う。
- **検証するもの・しないもの。** チェーンが検証するのは、成果物の結果(EXEC = GVM で test・checker を走らせ fraud proof で
  確定、EXACT = bonded な出題者が commit した隠し答えとの一致、CRITIC = EXEC で示した反例)、候補の出力(候補は RFC-0002 の
  IR class で、出力はその PALW claim)、点数・勝者規則・支払い。**学習は検証しない**(float・非決定・巨大):実際にどのデータを
  使ったか、出所とライセンス、backdoor の有無は検証できない。検証していない前提に立つ報酬には、すべてその前提を明記する
  (信頼ラベル V・B・J・T)。JUDGED(登録済み judge class)と HUMAN(bonded)は重みを低くし、コード(EXEC)と数学(EXACT、
  のちに PROOF)から始める。
- **評価は隠されていて、しかも未来のもの。** 候補の凍結後の beacon を鍵に RFC-0003 の R で、凍結後に届いた hard case から
  項目を引く(時間的 hold-out)。加えて bonded な非公開セット(問題は凍結後に、答えと隠しテストは出力の確定後に開示)。
  採点は項目ごとの対比較。勝者は現 head を δ 以上上回り、下側信頼限界が 0 を超え(整数の符号検定、候補数で Bonferroni
  補正)、回帰スイートと安全スイートで ε を超える後退がないこと。満たす候補がなければ現 head が残る。head は registry の
  lineage head field で、rollback の道を持つ。
- **報酬は段階的。** S1:hard case ごとに最初に EXEC/EXACT で検証された成果物への bounty、critic 報酬、不正・spam の bond 没収。
  S2:勝者の epoch 報酬を trainer とデータで分配する。manifest に書けるのは採用済みの成果物だけで、貢献者ごとに上限を設け、
  「実際に使ったかは検証できない」と明記する。S3:ablation コンテストで限界寄与を測る(A +4.1 %、B +0.2 %、C −1.0 % の類、
  ≥ 2 の独立 trainer で再現)。研究段階で、守りは再現と bond だけ。trainer の報酬は後続 epoch にかけて vest し、backdoor や
  ライセンス違反が後で証明されれば没収する(学習は取り消せない)。原資は line の手数料収入(owner の market leg と rung A の
  依頼手数料)の一定割合と sponsor の預託。PALW の worker 報酬には手を付けない(報酬経路は憲法事項)。教師の評判
  Alpha・Beta・Gamma は採用率と下流の改善から計算し、bond と上限にだけ効かせる。
- **ライセンス。** frontier API の規約は一般に、出力を競合モデルの学習に使うことを禁じている。Claude・GPT・Gemini 型の agent が
  学習データの教師になれるのは、権利者の `TeacherLicence` がある `LICENSED_DISTILL` のときだけ(将来の teacher fee 市場)。
  tool や agent の操作者として使い、最終成果物を別の方法で検証する場合も、ライセンスが許すときに限る(保守的に)。base model の
  ライセンスが派生物を許さなければ、その line は改良できない。attestation は bond つきで争え、偽りなら以後の manifest から除外する。
- **プライバシー。** 実利用から hard case を作るには、job の明示的な data-use opt-in(既定 off)が要る。opt-in は job の pin を
  名指す別オブジェクトで、FP job の形式は変えない。
- **consensus は最小。** 市場は EVM レーンのコントラクト(rung A)で、flag day なしで更新できる。consensus が足すのは、確定した
  PALW/GVM の結果を EVM から読む道、lineage head field、GVM の checker(Part I)だけ。オブジェクトと tag は提案に留め、番号は
  取らない。
- **RL。** 検証可能な報酬による RL(RLVR)がそのまま合う:検証器が公開の報酬関数で、RL 自体は off-chain。head の rollout を
  PALW job で作り、`SELF_PLAY` の `RewardSignal` として出せる。チェーンが裁くのは結果で、更新ではない。
- **脅威。** poisoning と backdoor(評価は trigger を見られない。canary・安全スイート・vesting・rollback で軽減するが閉じない)、
  汚染、テスト攻略、sybil とコピー、ライセンス洗浄、trainer と貢献者の結託、出題者の漏洩、judge 攻略、費用 DoS。
- **比較。** Bittensor(validator の主観的な重み)、計算の貸し出し(ブリーフの "FLOP")、Virtuals(agent のトークン化)。
  MISAKA の違いは、検証を先に置いた報酬と、チェーン自身の検証可能な実行で測る改良。

**工数と推奨**

- **工数。** Part I:(A)非同期 EVM 段は agent 実装 1〜2 週、mainnet-safe 6〜9 か月(mainnet の EVM レーンは今 inert で、
  その有効化が別に要る)。(B)は agent 実装 1〜2 か月、**mainnet-safe 24〜30 か月**(形式手法・監査 3〜4 本・6 か月以上の
  soak・bounty は縮まない。Optimism と Arbitrum の permissionless fault proof も設計から数年かかった)。Part II:合計
  **47〜72 engineer-weeks(約 11〜17 EM)**。P0(数学・EXACT、VM 不要)は 22〜32 ew、agent 実装 1〜2 週、mainnet-safe は
  開始から 6〜9 か月で、rung A の mainnet-safe 以降。P1(コード・EXEC)は rung B 待ち、P3(限界寄与)は研究。
- **推奨。** Part I:(A)はコントラクト側の需要が出たら作る。(B)は PALW-TIR が mainnet-safe になり、M4(ワークフロー需要)の
  測定が需要を示してから。RFC-0004 と両方は作らない。(B)を作るなら新規 BVM 登録は閉じ、BVM → GVM の正準変換を用意する。
  Part II:P0 は今仕様を固め、`palw_tir_v1` と rung A が testnet にそろったら作る。P1 は rung B の後、P3 は研究として testnet
  だけで行う。

## Summary

This RFC has two parts. **Part I** makes MISAKA a platform for **general AI computation** —
multi-model workflows, agents, routers, tool planners and pipelines — without giving up the property
that makes PALW work: every claimed result is adjudicable by recomputing one small piece of it.

Part I separates two questions the brief fused.

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

**Part II — Verified Model Improvement** makes registered models stronger after registration. A
registered model — say Qwen v12 — gets real usage, which produces hard cases and fee income for its
line's Distillation Pool. Teachers of six declared classes submit Teaching Artifacts. An artifact is
admitted only if its outcome verifies: EXEC in the GVM, EXACT against a bonded setter's committed key,
or CRITIC by an executed counterexample. Trainers train candidates off chain. A candidate replaces the
head only if it beats it on hidden items drawn from the future, by a paired integer sign test, and
every promotion can be rolled back.

MISAKA pays for artifacts and measured gains, never for agents or compute: "agent mining, not agent
rental". It verifies outcomes and never training, and every reward states what it rests on. The market
is EVM-lane contracts. Consensus adds a read path, a lineage head and Part I's checkers. Part II costs
47–72 engineer-weeks, and its first phase, mathematics with EXACT verification, needs no VM.

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

**Goals of Part I.**

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

**Non-goals of Part I.** Floating point. Parallel or nondeterministic execution. Network or filesystem access
from inside the VM. zk proofs in v1 (§2 keeps the door open). Running ML inside the L1 EVM. Replacing
the EVM lane. A remote GPU marketplace as a reward path (ADR-0144 P1). Synchronous TIR precompiles in
the L1 EVM (§2).

**Goals of Part II.**

- G9. Verification first: every reward states what it rests on (V, B, J or T, §II.2), and nothing
  unverifiable is presented as verified.
- G10. A head changes only by a statistical win on hidden items drawn from the future; otherwise the
  incumbent stays. Every promotion can be rolled back.
- G11. Agent mining, not agent rental: pay for verified artifacts and measured improvement, never for
  compute, time or accounts.
- G12. Consensus-minimal: the market is contracts; consensus adds a read path and the lineage head,
  and Part I supplies the checkers.
- G13. Licences and privacy by default: base models that permit derivatives, licensed teachers for
  proprietary outputs, and usage data only by opt-in.
- G14. Objective domains first (code, mathematics); judged and human signals at low weight.

**Non-goals of Part II.** Verifying training, or any "proof of training". Subjective validator
weights. Renting agents, GPUs or accounts. Paying from PALW worker rewards. Content policy in
consensus (PALW-PR-1). Legal advice.

---

# Part I — The Turing-complete ML VM (PALW-GVM)

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

Part II uses both: its market lives in rung A's contracts, and its checkers — tests, graders and
tool-trace replays — run in rung B.

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

---

# Part II — Verified Model Improvement

Part I gives the chain general, verifiable control over model calls. Part II uses it to make
registered models stronger after registration, by distillation and reinforcement learning, paying
only for what can be checked.

## II.1 The loop, and what "stronger" means

```
line (head v12) ── real usage (opt-in, §II.9) ──► hard cases ────────────────┐
     ▲                    └──► fee income (§II.6.5) ──► Distillation Pool ◄───┴── sponsor deposits
     │                                                     │ bounties
     │     teachers: OPEN_DISTILL · LICENSED_DISTILL · SELF_PLAY · HUMAN · TOOL_VERIFIED · PUBLIC_DATA
     │                                                     │ Teaching Artifacts (commit–reveal, bonded)
     │                                                     ▼
     │                               verification: EXEC · EXACT · CRITIC ──► admitted dataset
     │                                                     │ trainers, off chain
     │                                                     ▼
     │                               candidates A, B, C (RFC-0002 IR classes, frozen)
     │                                                     │ PALW evaluation on hidden, future items
     └──── head := v13 ◄── winner rule ◄── paired scores ◄─┘
```

- **Stronger means measured.** A model is stronger when it beats the incumbent head on items it
  cannot have seen, scored by verifiers the chain runs. Declared benchmarks and self-reported gains
  count for nothing. Spec 15 records evaluations as declarations (`ModelEvaluationPosted`, PALW-MK-3);
  Part II adds verified evaluation for the lines that opt in.
- **Agent mining, not agent rental.** MISAKA never rents an agent, a GPU or an API account. It pays for
  delivered artifacts that pass verification, and for improvement measured on its own execution. How
  a teacher produced an artifact — a frontier API under licence, an open model, a person with tools, a
  solver — is the teacher's business: stated in the artifact, and bonded where it cannot be checked.
- **Objective domains first**: code (EXEC — tests and checkers) and mathematics (EXACT — committed
  answers; PROOF later). Open-ended writing and taste get only JUDGED and HUMAN signals, at low weight,
  until something better exists.
- **Where each part runs.** The market is EVM-lane contracts (Part I, rung A). Checkers run in
  PALW-GVM (rung B). Candidates are RFC-0002 IR classes, and their outputs are PALW claims. Consensus
  adds only what §II.10 lists.

## II.2 What the chain verifies, and what it does not

Every reward in Part II carries a **trust label** for what it rests on.

| Label | Meaning |
| --- | --- |
| **V**, verified | recomputed by the court or re-executed by every node: a PALW claim, a GVM run, a contract's arithmetic over final results |
| **B**, bonded | a statement someone staked on, challengeable by a stated procedure; a false one forfeits the bond |
| **J**, judged | a registered judge class's output through PALW: verified as *the judge's* answer, deterministic, and gameable |
| **T**, trusted | not checkable; stated as an assumption wherever a reward rests on it |

| Object | How it is established | Label |
| --- | --- | --- |
| an artifact's outcome, EXEC | its tests or checker run in PALW-GVM (Part I), fraud-proven | V |
| an artifact's outcome, EXACT | equality with a hidden answer committed by a bonded problem setter | V for the match; B for the key |
| a critique, CRITIC | its counterexample, run by EXEC | V |
| a formal proof, PROOF (later) | a proof checker as a GVM guest | V |
| a JUDGED score | a registered judge class, run as a PALW claim | J, low weight |
| a HUMAN score | a bonded person, challengeable | B, low weight |
| a candidate's outputs | PALW claims on the candidate's IR class | V |
| scores, the winner rule, payouts | contract arithmetic over finalized results | V |
| **training** — which data a trainer used, and how | nothing: float, nondeterministic, huge, off chain | **T** |
| an artifact's provenance and licence | the contributor's bonded attestation | B |
| a winner's freedom from backdoors | nothing: evaluation cannot see a trigger it does not contain | **T**, mitigated (§II.11) |

The chain judges **outcomes**. It never judges the training that produced them, and no interface may
present a T or B quantity as verified.

## II.3 The objects

### II.3.1 Lines, heads, the pool and epochs

- **The line is spec 15's line** (ADR-0088): `(class, owner, name)`, with its roles and its market.
  Part II changes nothing in it. A line **opts in** by naming a governor contract, in an object its
  owner signs (`ModelLineGovernorSet`, a proposal). Only that contract's decisions move the line's head.
- **The head** — `LineageHeadV1 { line, head_class, epoch, previous }` — is the line's current best
  class: a registry field set only by the governor's decision (§II.5.5) and restored by the rollback
  path (§II.5.6). It names a **class**, because a candidate with new weights is a new IR class:
  Phase F's class id commits to the artifact root. How that meets spec 15's rule that new weights are
  a new *version* of a line, never a new class (PALW-MK-1/2), is open question 13.
- **The Distillation Pool** of a line holds its funds (§II.6.5), its hard cases and its admitted
  dataset. The funds sit in the governor contract. Nothing is minted.
- **An epoch** runs `open → freeze → draw → evaluate → score → decide → vest`, with lengths in DAA set
  by the governor.

### II.3.2 Hard cases

```
HardCaseV1 { case_id, domain, prompt (token ids under the line's tokenizer),
             verifier: Exec { tests_commitment } | Exact { key_commitment },
             source: UsageOptIn { job_pin } | Setter { bond } | Artifact { artifact_id },
             head_evidence: { claim, verifier_outcome } }
```

- **Sources**: real usage, only with its user's opt-in (§II.9); bonded problem setters; and
  `SyntheticProblem` and `HardCaseVariant` artifacts.
- **Hardness is verified** (V). `head_evidence` names a finalized PALW claim of the head on the prompt,
  read through the read path (§II.10), and its failing verifier outcome. A case the head passes is not
  hard and pays no bounty.
- **Keys and hidden tests** are committed by hash before they can matter, and revealed when scored.

### II.3.3 Teaching Artifacts

```
TeachingArtifactV1 {
  kind:                  Answer | Critique | PreferencePair | TestCase | Counterexample | ToolTrace
                         | VerifiedCode | FormalProof | SyntheticProblem | HardCaseVariant | RewardSignal,
  task_id:               the hard case, or the artifact, it answers,
  teacher_type:          OPEN_DISTILL | LICENSED_DISTILL | SELF_PLAY | HUMAN | TOOL_VERIFIED | PUBLIC_DATA,
  teacher_id:            a bond, a TeacherLicence, or a class id (SELF_PLAY),
  license_class:         the licence under which the content may train derivatives,
  provenance_commitment: H(the contributor's provenance record: models and versions, prompts, tools, dates),
  output_hash:           H(the content); the content lives off chain, content-addressed,
  verification_type:     EXEC | EXACT | CRITIC | PROOF | JUDGED | HUMAN,
  bond, commit: H(artifact ‖ salt), reveal,
}
```

| Kind | Verified by | Label | Paid in |
| --- | --- | --- | --- |
| Answer | EXEC (code) or EXACT (math); otherwise JUDGED | V (J) | S1 bounty; S2 |
| Critique | CRITIC when it carries a counterexample; otherwise JUDGED | V (J) | S1 critic reward |
| PreferencePair | two verified outcomes, one passing and one failing; otherwise JUDGED or HUMAN | V (J, B) | S2 |
| TestCase | EXEC: the reference solution passes it and a known wrong one fails it | V | S1; S2 |
| Counterexample | CRITIC | V | S1 critic reward |
| ToolTrace | EXEC replay in PALW-GVM over its oracle transcript: the computation is verified, the transcript's truth is not (Part I §4.2) | V over T | S2 |
| VerifiedCode | EXEC | V | S1; S2 |
| FormalProof | PROOF (later) | V | S1; S2 |
| SyntheticProblem | the setter's committed key (EXACT) or tests (EXEC) | B for the key | S1, to the setter |
| HardCaseVariant | the parent's verifier, and the head verifiably failing it | V | S1, to the setter |
| RewardSignal | a SELF_PLAY rollout: the head's PALW claim and its verifier outcome | V | S2 |

### II.3.4 Teacher classes

| Class | What it is | Admitted when | Provenance label |
| --- | --- | --- | --- |
| OPEN_DISTILL | outputs of open-weight models whose licence permits training derivatives | `license_class` names such a licence; bonded attestation | B |
| LICENSED_DISTILL | outputs of a proprietary model | a `TeacherLicence` from the rights holder covers the use (§II.8) | V that the licence exists; B that the use stayed inside it |
| SELF_PLAY | the head, or other registered classes, run as PALW jobs | the claims themselves | V |
| HUMAN | human-written work, possibly tool-assisted | bonded attestation of authorship and rights | B |
| TOOL_VERIFIED | deterministic tools and solvers (compilers, SMT solvers, computer algebra) | the outcome verified by EXEC or EXACT | V for the outcome |
| PUBLIC_DATA | public datasets under permissive licences | bonded licence attestation | B |

## II.4 Admission into the dataset

1. **Commit.** `commit = H(artifact ‖ salt)`, posted with the contributor's bond. The commit's block
   orders submissions.
2. **Reveal** after `r` blocks. The content goes to content-addressed storage, and `output_hash` must
   match. **Dedupe** by `output_hash`, and by the verifier's normal form where one exists (a canonical
   patch, a normalised answer): only the earliest commit of the same content counts. Copying from the
   mempool therefore earns nothing.
3. **Verify** by `verification_type`. EXEC runs are GVM jobs requested through rung A. EXACT is contract
   arithmetic after the setter's reveal. CRITIC is an EXEC run of the counterexample.
4. **Critique window.** A verified artifact stays open to `Critique` and `Counterexample` for `w` blocks
   before it pays or is admitted.
5. **Admit or reject.** Admitted artifacts form the epoch's dataset, a Merkle list the governor
   publishes. Spam — malformed, unverifiable under its declared type, duplicate, or inadmissible by
   licence — forfeits its bond to the pool. An honest failure costs its verification fee and nothing
   more.

### II.4.1 A worked example: hard case #583927, "fix this Rust bug"

- **The case.** A user who opted in asked v12 to fix a failing crate. v12's patch fails the case's
  hidden tests (EXEC, V), so the case enters the pool with a bounty.
- **Teacher A** (LICENSED_DISTILL, under a `TeacherLicence`) submits an `Answer`, a patch. EXEC runs the
  hidden tests: pass. When the critique window closes with no valid counterexample, A is paid the
  bounty (S1, V), and the patch is admitted.
- **Teacher B** (OPEN_DISTILL) submits a patch that fails the tests. B earns nothing, keeps its bond
  (an honest failure is not spam) and pays its verification fee.
- **Teacher C** (HUMAN) submits a `Critique` of v12's original patch with a `Counterexample`: an input on
  which that patch panics. EXEC confirms it (CRITIC, V). C earns the critic reward, and the
  counterexample joins the case's tests.
- **The caveat.** In P1 the tests run on a binary. That the binary was built from the patch's source by
  the pinned toolchain is B, not V, until compiling inside the GVM is affordable (§II.13).

## II.5 Candidates and evaluation: hidden, and in the future

### II.5.1 Candidates

- A candidate is an **RFC-0002 IR class** of the line's family, registered before the epoch's
  **freeze** with a candidate bond and an evaluation fee. Registration commits its weights (its
  artifact root), so nothing about it can change after the freeze.
- Its trainer submits a **manifest**: the admitted artifacts it says it used. The manifest is T (§II.2),
  and it matters only for S2.

### II.5.2 Items

- **A temporal hold-out.** Items are drawn from hard cases that **arrived after the freeze**, so no
  candidate can have trained on them.
- **The draw.** The governor draws `n` items with RFC-0003's R (domain `CLASS_UNIFORM_V1`), keyed by
  the epoch seed: the chain's beacon at the first block `d` DAA past the freeze, under the discipline
  eligibility beacons follow (ADR-0044 F5/F15).
- **A bonded private set.** Before the freeze, one or more bonded stewards commit to private items:
  prompt commitments and answer or hidden-test commitments. Prompts are disclosed after the freeze,
  when every candidate is already fixed. Keys and tests are disclosed only after the candidates'
  outputs are final. A steward who does not reveal forfeits its bond, and its items drop out.
- **Caps.** No setter or steward supplies more than a fraction `κ` of an epoch's items (§II.11).

### II.5.3 Running the evaluation

- The governor requests, through rung A, one PALW job per candidate and item, and one per item for the
  incumbent head. Any bonded executor serves them, paid from the evaluation fees. No trainer chooses
  which items its candidate runs, or who runs them.
- A result arrives only after `Final`, through the read path: the output's root and a **verified
  excerpt** — for example the answer span of a generated text, whose opening under the claim's
  committed output the fold checks.
- A result missing at the deadline scores as a failure for that candidate.
- Each output is scored by its item's verifier: EXACT by the contract (token-span equality under the
  line's tokenizer), EXEC by a GVM job.

### II.5.4 The winner rule, paired per item

For a candidate `C` and the incumbent head `H` on the same `n` pass/fail items, let `b` be the number
of items `C` passes and `H` fails, and `c` the reverse. `C` is **eligible** iff all of the following
hold:

1. **Enough evidence**: `n ≥ n_min`.
2. **It beats the head by δ**: `b − c ≥ δ · n`.
3. **Its lower confidence bound is above zero**: an exact one-sided sign test on the `b + c`
   discordant items rejects "no better" at level `α / K`, where `K` is the number of candidates (a
   Bonferroni split). That is `b ≥ k(b + c, α / K)`, with `k` read from a pinned integer table of
   binomial critical values: integer arithmetic, no floats.
4. **No regression beyond ε** on a fixed public regression suite, by the same paired counts.
5. **No regression beyond ε_s** on a fixed safety suite. The suite is the line's policy, enforced by
   its contract. No consensus validity rule reads content (PALW-PR-1).

The winner is the eligible candidate with the largest `b − c`, with ties going to the earliest
registration. **If no candidate is eligible, the incumbent stays.** Partial-credit scores need a
different pinned test (open question 16).

### II.5.5 The head update

The governor's decision reaches consensus the ADR-0089 way (D5–D6: an EVM action that the fold applies
one block later), as `LineageHeadSet { line, class, epoch }`. The head's history is kept. Serving
follows the head by default: a node serving the line serves its head (open question 17).

### II.5.6 Rollback

- **By the governor**, on proof found after promotion: a backdoor or trigger shown by a
  `Counterexample` or a PALW claim; a licence violation shown against the manifest; or a regression on
  items drawn later.
- **By the line's owner**, within `R` epochs of a promotion and without proof: the owner answers for
  the product.
- A rollback restores `previous`, forfeits the winner's unvested rewards and candidate bond (§II.6.4),
  and bars its trainer's candidates for a governor-set period.

## II.6 Rewards, staged

Every reward below names the label it rests on (§II.2).

### II.6.1 S1 — bounties and critics

- **A bounty per hard case** (V), paid to the first EXEC- or EXACT-verified artifact that answers it,
  in commit order, once its critique window has closed.
- **Critic rewards** (V) for a valid `Critique` or `Counterexample` against an artifact, a head output or
  a setter's key.
- **Setter rewards** (V for hardness, B for the key) for a `SyntheticProblem` or `HardCaseVariant` that
  the head verifiably fails.
- **Forfeits** (V): invalid, spam or duplicate artifacts forfeit their bonds to the pool.

### II.6.2 S2 — the gain share

- The winner's epoch reward (§II.6.5) is split between the trainer and the data, by a governor
  parameter.
- The data share goes to the artifacts named in the winner's manifest. A manifest may name **only
  admitted artifacts**, and each contributor's share is capped.
- **The trust assumption is stated in the contract and in every interface**: the chain cannot see what
  a trainer used. S2 pays what the manifest says. A trainer may name artifacts it never used, to pay
  friends, and the caps bound that. It may also omit ones it did use, and that only costs the omitted.

### II.6.3 S3 — marginal attribution ("Proof of Useful Data")

- **Ablation contests.** For a dataset slice `X` (one teacher's, one teacher class's, one kind's),
  trainers train with and without `X`. Each arm is replicated by **at least two independent trainers**,
  and the arms are evaluated as candidates are (§II.5). The marginal gain of `X` is the paired difference
  on the same hidden, future items: for example slice A +4.1 points, B +0.2, C −1.0.
- **The payout** is proportional to the gain's lower confidence bound where that bound is above zero. A
  is paid. B is inside the noise and is not. C is not paid, and its teacher's reputation falls.
- **Teacher reputation** — Alpha, Beta, Gamma — comes from observable records: the adoption rate (the
  share of a teacher's admitted artifacts that winning manifests name) and the downstream gain (S3). It
  sets bond sizes and caps. It never multiplies a payout.
- **This is research.** Replication and bonds are the only guards against a trainer who shapes the
  arms, and this RFC does not claim more for it. Each contest costs several training runs per slice.

### II.6.4 Vesting and forfeit

A winning trainer's reward and candidate bond **vest over the next `v` epochs**. They are forfeited if a
backdoor or a licence violation is proven before they vest (§II.5.6). A model cannot be un-trained, so
the only lever left is money that has not yet been paid out.

### II.6.5 Where the money comes from

- **A parameter share `φ` of the line's fee income** — the owner's market leg (spec 15 PALW-MK-7) and
  the rung-A request fees of its classes — set when the line opts in.
- **Sponsor deposits**: anyone may fund a line's pool, a domain within it, or a single case.
- **Not PALW worker rewards.** They are the reward path (ADR-0144 P4–P7), and the market's buyback
  slice (PALW-MK-8) already takes its share of them. Diverting more would be a constitutional change,
  and this RFC does not propose one.
- Nothing is minted.

## II.7 Reinforcement learning

- **RL with verifiable rewards fits Part II as it stands.** The verifiers are **public reward
  functions** (EXEC and EXACT), and trainers run RL against them off chain.
- **Rollouts can be produced on chain.** The head's rollouts on hard cases can run as PALW jobs and be
  submitted as `SELF_PLAY` `RewardSignal` artifacts. The rollout (a claim) and its outcome (a verifier
  run) are both V, so a trainer can buy verified reward data without trusting whoever generated it.
- **The chain judges the outcome, not the update.** Whether a trainer's policy gradient was computed
  correctly is invisible and irrelevant. Only the next candidate's measured result is paid.

## II.8 Licence and legal

- **Frontier APIs.** API terms commonly forbid using outputs to train competing models. Claude-, GPT-
  and Gemini-type agents are therefore admissible as teachers of training data **only under
  `LICENSED_DISTILL`**, backed by a `TeacherLicence` signed by the rights holder:

  ```
  TeacherLicenceV1 { rights_holder_key, model_family, scope: { domains, uses }, per_use_fee, expiry, revocation }
  ```

  This is also the long-term market: model companies register their teacher and earn a teacher fee per
  admitted use.
- **As tools and agent operators.** Such a model may help produce an artifact whose final form is
  verified another way (a patch that passes tests), and that artifact may enter training data, **only
  if its licence allows that**. The default is conservative: a contributor who used a frontier API
  without a licence covering training use must not submit the result.
- **The base model.** A line can be improved only if its base model's licence permits derivatives.
  Research-only and no-derivatives licences cannot enter.
- **Attestations** of licence class and provenance are bonded and challengeable. A proven false
  attestation forfeits the bond and excludes the contributor from future manifests.
- **Limits.** The chain cannot adjudicate law. A bond prices dishonesty; it does not make an unlicensed
  use lawful. Operators, gateways and contributors remain responsible under the law that applies to
  them. This RFC is not legal advice.

## II.9 Privacy

- A hard case taken from real usage needs the job's **explicit data-use opt-in**, which is **off by
  default**.
- The opt-in is a separate object, signed by the job's committer, that names the job's pin
  (`DataUseOptInV1`, a proposal). No FP job format changes (RFC-0001 §A is frozen).
- Opting in publishes the prompt ids and the head's output with the case. The chain checks that the
  case's prompt matches the job's committed `prompt_token_ids_hash`. Redacting before opting in is the
  user's responsibility.
- `PanelDa` jobs stay private unless their user opts in. Nobody can opt in another person's job.

## II.10 A consensus-minimal architecture

**The market is EVM-lane contracts** (Part I, rung A). They hold the pool, the hard cases, artifact
commit–reveal and dedupe, bounties, the evaluation's orchestration, the winner rule, payouts, vesting,
reputation and licences. They can be upgraded without a flag day, and each line's governor names the
contract version it trusts. Part II scores the quality of answers by design, so it must stay contract
logic: no consensus validity rule may read content (PALW-PR-1). Consensus only records what the
governor decided.

**Consensus adds only three things:**

1. **A read path** for finalized PALW and GVM results into the EVM lane: a read precompile serving the
   fold's rows of final claims (class, job pin, status, output root) and verified excerpts of their
   outputs, at the EVM block's selected parent (ADR-0089 D2's discipline).
2. **The lineage head**: the governor opt-in, the head field with its history, and rollback, all set by
   the governor's actions through the writer path (ADR-0089 D5–D6).
3. **GVM checkers**: Part I, rung B, unchanged.

**Proposed objects and addresses, none claimed**: `ModelLineGovernorSet`, `LineageHeadSet`,
`LineageHeadRolledBack` and `DataUseOptInV1`; a read precompile (`FinalizedResults`); a writer action
kind (`HeadDecision`). Tags and addresses are assigned only when an ADR accepts them.

## II.11 Threats

| Threat | Mechanism | Mitigation | What remains |
| --- | --- | --- | --- |
| Poisoning and backdoors | a trainer or contributor plants a trigger in a winner | evaluation cannot see a trigger it does not contain; canary items, the safety suite, vesting and forfeit, rollback | **not closed**: a well-hidden trigger passes evaluation, and only later proof and rollback reach it |
| Contamination | evaluation items leak into training | the temporal hold-out, the private set disclosed after the freeze, R-drawn items | a leak through a setter (below) |
| Test gaming | an artifact or a candidate fits the visible tests | hidden tests; critic rewards that turn counterexamples into tests | weak tests stay weak until someone criticises them |
| Sybils and mempool copying | another's artifact resubmitted | commit–reveal, content-hash dedupe on normal forms, bonds | near-duplicates that differ syntactically |
| Licence laundering | unlicensed teacher output declared OPEN or HUMAN | bonded attestations, challenges, exclusion, `TeacherLicence` | laundering nobody detects |
| Trainer–contributor collusion | a manifest pays friends | admitted-only manifests, per-contributor caps, S2 stated as T | whatever fits under the caps |
| Leakage by problem setters | a setter hands future items to a trainer | many setters, per-setter caps `κ`, private sets from independent stewards, per-setter anomaly checks | a leak within one cap |
| Judge gaming | outputs tuned to a judge class | low JUDGED weight; objective domains first | inherent to judges |
| Beacon grinding | a producer grinds the item draw | a beacon that postdates the freeze, a large `n`, the private set | small |
| Cost DoS | spam candidates or artifacts | candidate registration bond and evaluation fee; artifact bonds; request caps | — |
| Governor capture | a bad contract version moves a head | the owner names the governor; the owner's rollback | trust in the owner's choice |

## II.12 How this differs from nearby designs

| Design, as publicly described | What is paid for | Who decides the reward | Part II's difference |
| --- | --- | --- | --- |
| Bittensor | a subnet miner's output, as validators weight it | validators' subjective weights, aggregated by consensus | rewards rest on verified outcomes (EXEC, EXACT, CRITIC) and on improvement measured by the chain's own verifiable execution, not on validators' opinions |
| Compute-rental designs (the brief's "FLOP") | compute or time delivered | the buyer | Part II pays for no compute: only for artifacts that pass verification and for measured gain ("agent mining, not agent rental") |
| Virtuals | tokenised agents, valued by market demand | the market | a head changes only by a statistical win on hidden, future items; the line's market (spec 15) is kept apart from the evidence |

The claim is deliberately narrow: Part II is **verification-first**. Where it cannot verify — training,
provenance, backdoors — it says so, and prices the risk with bonds, vesting and rollback.

## II.13 Phasing and effort

Engineer-weeks for two to four experienced people, as in Part I's tables.

| Phase | Scope | Depends on | Engineer-weeks | With agents | Mainnet-safe requires |
| --- | --- | --- | --- | --- | --- |
| **P0** — math, no VM | EXACT hard cases, setters and bounties; critics of keys; candidates; EXACT evaluation; the winner rule; the head field and the read path. Breakdown: spec 3–4, consensus pieces 4–6, contracts 8–12, tooling 4–6, drills 3–4 | `palw_tir_v1`; rung A | 22–32 | 1–2 weeks | two contract audits (security, economics); at least 3 full epochs on a testnet with adversarial drills (copying, setter leakage, grinding, rollback); a bounty; the EVM lane active on mainnet |
| **P1** — code, EXEC | checker runtimes as GVM guests (a WASM interpreter, a scripting-language interpreter, a test harness); `TestCase`, `Counterexample`, `VerifiedCode`, `ToolTrace`; the pinned-toolchain path for compiled languages | rung B | 10–16 | 1–2 weeks once rung B exists | rung B mainnet-safe (Part I: 24–30 months); an audit of the checkers; at least 2 epochs with EXEC cases |
| **P2** — the gain share | manifests, caps, vesting, forfeit, the `φ` routing | P0 | 5–8 | about 1 week | an economic audit; at least 2 epochs paying out on a testnet |
| **P3** — marginal attribution | ablation contests, replication bonds, reputation | P2 | 6–10, plus research | 1–2 weeks | testnet only within this RFC's horizon: at least 6 months of experiments before any mainnet decision |
| **P4** — licensed teachers | `TeacherLicence`, rights-holder keys, per-use fees | P0 | 4–6 | a few days | rights holders' participation and legal review; no date can be estimated |
| **Total** | | | **47–72** (≈ 11–17 engineer-months); P0–P2: 37–56 | | |

- **P0 first.** It needs no VM, and it exercises every hard part that is not the VM: hidden and future
  evaluation, the winner rule, the head, rollback, bonds and payouts.
- **Mainnet-safe figures.** P0: about 6–9 months from its start, and not before rung A is mainnet-safe
  (it can share rung A's soak). P1: nothing before rung B; then about 2–3 months for its checkers' audit
  and soak. P2: 3–6 months after P0. P3: research, no date. P4: the calendar of outside parties.
- **What does not compress** is what never does (Part I, *Effort*): audits, epochs of soak with real
  contests, the bounty window, and the legal review of the licence gates.

---

# Rules, activation, effort and decisions (both parts)

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

### Additions for Part II (proposals; the consensus pieces only)

The market's rules are contract logic, specified with the contracts rather than in consensus. Consensus
needs only the following.

- **PALW-IMP-1 (the governor).** A line MAY name one governor contract, by an object its owner signs.
  Only that contract's actions MAY set or roll back the line's head. A change of governor MUST take
  effect only after a delay.
- **PALW-IMP-2 (the head).** A governed line's head MUST be a registered IR class of the line's family.
  `LineageHeadSet` MUST record the class, the epoch and the previous head, and the history MUST be kept.
- **PALW-IMP-3 (rollback).** `LineageHeadRolledBack` MUST restore the previous head. The governor MAY
  apply it at any time; the line's owner MAY apply it within `R` epochs of a promotion.
- **PALW-IMP-4 (the read path).** The EVM lane MUST be able to read, at the EVM block's selected parent,
  the fold's rows of final claims (class, job pin, status, output root) and excerpts of their outputs
  whose openings the fold has checked. A claim that is not final MUST NOT read as final.
- **PALW-IMP-5 (the data-use opt-in).** A hard case MAY reference a job's prompt only if the job's
  committer has signed a `DataUseOptInV1` for that job, and the case's prompt MUST match the job's
  committed prompt hash.
- **PALW-IMP-6 (not a reward path).** Improvement-pool payouts MUST NOT create a quantum, a ticket or
  eligibility (ADR-0144 P1), and MUST NOT draw on PALW worker rewards.

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

### Part II

The market is contracts, so most of Part II activates without a flag day. Its consensus pieces sit
behind one fence, `palw_lineage_head_v1` (the governor, the head, rollback and the data-use opt-in),
which `validate_palw_v2` refuses unless `palw_tir_v1` is active and the EVM lane runs. The read path
joins `palw_evm_jobs_v1` (rung A), whose settlement needs it anyway.

| Step | Work | Exit gate |
| --- | --- | --- |
| P0-a | The market's interfaces, the EXACT encodings, the pinned sign-test table and the trust labels | reviewed text |
| P0-b | `palw_lineage_head_v1` and the read path. Drills: a head set by a governor; a rollback by the owner and one by the governor; an opt-in checked against a job's prompt hash; the fence crossing on the shipping binary | all pass |
| P0-c | Contracts and tools; a testnet line running at least 3 epochs of mathematics contests, with adversarial drills (copying, setter leakage, grinding, a forced rollback) | no unresolved defect; audits closed |
| P1 | EXEC checkers on rung B | rung B's gates, and at least 2 epochs of code contests |
| P2–P4 | the gain share; ablation contests (testnet only); licensed teachers | §II.13 |

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

### Part II — Verified Model Improvement

§II.13 has the table: **47–72 engineer-weeks (≈ 11–17 engineer-months)** in total, of which P0–P2 are
37–56.

- **Implementation with AI agents.** P0: about 1–2 weeks. P1: about 1–2 weeks once rung B exists. P2:
  about a week. P3: 1–2 weeks, plus research.
- **Mainnet-safe.** P0: about 6–9 months from its start, and not before rung A is mainnet-safe. P1: not
  before rung B (24–30 months), then 2–3 months more. P2: 3–6 months after P0. P3: research only. P4:
  on outside parties' calendar.

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

Part II adds no execution model. Its cost is contracts, the consensus pieces of §II.10, and the
checkers it runs on rung B.

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
| **Part II: paying for compute or agent time** | Pays for effort, not results — the opposite of Part II's rule ("agent mining, not agent rental") |
| **Part II: validators' subjective weights** | Rewards would follow opinions, which can be bought; Part II's follow verified outcomes |
| **Part II: rewarding declared training data** | Unverifiable (T). S2 does it only under caps, with the assumption stated |
| **Part II: proof of training** | Float, nondeterministic and huge; nothing practical proves a training run today |
| **Part II: LLM judges as the primary reward** | Gameable, so JUDGED stays at low weight |
| **Part II: the improvement market inside consensus** | Every change would need a flag day, and consensus would read answer quality, against PALW-PR-1 |

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
- **Part II.** Its threats — poisoning and backdoors above all, which evaluation cannot close — are in
  §II.11.

## Compatibility and migration

- TIR, pipeline, legacy and BVM classes are unchanged. Every fence of this RFC is dormant until armed.
  Every new object is appended, dropped by name below its fence and skipped by older builds under A-2.
- **Part II** changes no existing object. Spec 15's lines keep their rules, and a line that does not opt
  in is untouched. The FP job format does not change: the data-use opt-in is a separate object.
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

13. **Part II — IR lines and spec 15 versions**: the head names a class (recommended, matching Phase F's
    identity), or a line's IR versions become roots in force of one class.
14. **The governor**: a contract the owner names (recommended), or one protocol-standard contract.
15. **Parameters per domain**: `φ`, `δ`, `ε`, `ε_s`, `α`, `n_min`, `κ`, `v`, `w`, `R` and the epoch lengths.
16. **Non-binary scores**: which pinned test replaces the sign test.
17. **Serving follows the head** by default (recommended), or the owner decides.
18. **Compiled languages in P1**: compiling inside the GVM (far higher gas ceilings), or the
    pinned-toolchain attestation (B).
19. **JUDGED signals**: whether they ever pay, and at what weight.
20. **`TeacherLicence`**: its format and how it is verified, agreed with rights holders (P4).
21. **Artifact content**: it lives in content-addressed storage off chain. Who must keep it available
    for audits and challenges, and is that bonded?
22. **The safety suite**: who writes it for a line, and whether it may veto a winner that passes
    everything else.

## Decision

<Open.> The drafter's recommendation:

- **Rung A**: specify it now; build it when a contract use case asks for model calls. It is small, reuses
  ADR-0089's machinery, and adds no fraud proof.
- **Rung B**: do not start it before PALW-TIR is mainnet-safe and M4 shows the demand. When it starts,
  budget for 24–30 months to mainnet-safe use, whatever the implementation speed.
- **Never both RFC-0004 and rung B.** If the workflow case is proven, rung B subsumes the bounded layer.
- **Part II**: specify P0 now, and build it once `palw_tir_v1` and rung A run on a testnet; it needs
  no VM. Build P1 after rung B and P2 after P0; run P3 as research on a testnet only. Every reward keeps
  its trust label.
