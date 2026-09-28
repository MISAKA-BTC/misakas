# RFC-0004: PALW Model Improvement Protocol — self-improvement and distillation over PALW-TIR

| Field | Value |
| --- | --- |
| Status | Draft, 2026-09-29 — design. Phase A of the improvement roadmap. By the user's order of 2026-09-29, it is implemented in full before any work on RFC-0005 (the VM) starts |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-29 |
| Normative dependencies | **RFC-0002** (PALW-TIR and its Phase F integration) and **RFC-0003** (R, `TirProgramV2`, pipelines, canonical outputs; its text profile carries RFC-0001 §A's frozen decode rule). Nothing else: **no VM, and no EVM contract** |
| Affects | spec/palw 03 (registry: candidates, composite artifacts), 05 (canonical work of evaluation jobs), 07/08/09 (evaluation claims and their court), 10 (fees, bounties, vesting), 11 (the data-use opt-in), 15 (lines: the improvement policy and protocol promotion), 16 (fences), and a new chapter `spec/palw/17-model-improvement.md` · all networks (dormant until armed) · `consensus/core` (the fold: epochs, candidates, evaluation jobs, promotion, the pool), `misaka-palw-tir` (the scoring library), `misaka-palw-tir-lower` (adapter lowering), `misaka-palw-sdk`, the node |
| Branch | `rfc/0004-0005-vm` (text only) |
| Related | RFC-0005 (optional Layer 2: bounded improvement workflows as Phase B, research programs and EXEC checkers as Phase C), ADR-0088 and spec 15 (lines, versions, proposals, the market), ADR-0160 (claim capacity), ADR-0144 P1–P7, ADR-0145 (canonical work), ADR-0069 (weight), ADR-0044 F5/F15 (beacons), ADR-0072 (one inference, one ticket) |

## 概要(日本語)

- **目的。** 外部から登録されたモデル(例:Qwen v12)を、実利用が生む hard case と収益を使って蒸留と RL で強くし、勝った候補を
  次の head(v13)にする。これを **PALW-TIR(RFC-0002/0003)だけで、VM なしで** 回す。ユーザーの順序では、この RFC を全部
  実装してから RFC-0005(VM)に進む。
- **層。** Layer 0 = PALW-TIR(候補を評価する場所)。Layer 1 = この Improvement Protocol(epoch・dataset・候補の registry・
  報酬・promotion)。Layer 2 = 任意の VM による自動化(RFC-0005)。Phase A はこの RFC で IR だけ。Phase B と C は RFC-0005。
- **分担。** 学習は consensus の外、評価は PALW-TIR、promotion はプロトコル。**作り方は自由で、成果物だけを厳密に検証する**
  (Bitcoin はブロックが正しいかを検証し、ASIC の作り方は問わない)。規範:**候補モデルが VM で作られたことを要求しては
  ならない(MUST NOT)**。プロトコルが検証するのは 4 つだけ:モデル成果物(IR class の admission)、provenance policy
  (宣言の形と参照の有効性。真偽ではない)、評価結果、promotion の資格。
- **既存の土台。** line は spec 15(ADR-0088)の line。改良に opt-in した line の head は改良行の `head`(IR class id)と
  その履歴で、spec 15 の version 行と `current` にはプロトコルは書かない(Phase F の class id は重みを含むので新しい重みは
  新しい class。version に class id を書く当初の推奨形は spec 15 の稼働中の読み手 3 つを壊すと分かった — §3)。opt-in した
  line では developer の `ModelVersionPromoted` を拒否し、head はプロトコルの promotion と rollback でだけ動く。利用量は
  head class の Final claim を改良行で数える。
- **consensus-native な epoch 状態機械。** 利用量が閾値を超える → OPEN_IMPROVEMENT_EPOCH → hard case の dataset_root を
  固定 → 候補の提出窓 → CLOSE_SUBMISSION → 凍結後に届いた hard case と setter セットから R で評価項目を引く → 各候補を
  評価 → 合格した最良の候補を promotion し、lineage head を更新。状態・オブジェクト・遷移を規定する(tag は提案だけで、
  番号は取らない)。EVM コントラクトの bounty 市場は任意の追加で、何もそれに依存しない。
- **評価は RFC-0003 の pipeline と TIR の採点 stage。** 親と各候補を同じ評価入力で TIR 上で走らせる。採点は
  (1) R で seed した生成の RFC-0003 正準出力の完全一致、(2) 参照の teacher-forced 対数尤度(forward 1 回で安く正確)、
  (3) 登録済みの judge / reward-model class(評価時に登録集合から R で引く。重みは低く、完全一致の anchor で校正)、
  (4) 親と候補の対比較。**コードをテストで評価することは Phase A ではできない**(RFC-0005 が要る)ので、コードは完全一致
  出力か尤度の課題で扱う。評価は隠されていて、しかも未来のもの:凍結後の時間的 hold-out と、答えを採点後に開示する bonded
  setter セット。
- **promotion。** 項目ごとの対比較、δ の差、下側信頼限界 > 0(ピン留めした二項表による整数の符号検定、候補数で Bonferroni)、
  回帰スイートと安全スイートで ε を超える後退なし。満たす候補がなければ現 head のまま。rollback の道がある。
- **候補の成果物。** 完全な重み、または親 + adapter(IR グラフの中の unmerged LoRA 経路。lowering は `tir/lower` a4bd7141f)。
  (1) 候補の最初の P 個の param は親と byte 一致で、artifact_root は親の root と adapter 部の root を合成するので、親の tensor は
  二度と commit しない。(2) class id は Phase F の式のまま合成 root にかかり、親 class・候補 program の root・adapter root・P を
  束ねる。(3) 親の scale を再利用する:adapter が親の約 2 倍の余裕を超えて活性を動かせば clip し、それは候補の評価点に表れる
  候補自身の危険。再較正は byte 一致の再利用を壊すので、完全な重みの候補として出す。(4) 1 射影あたり約 20 node 増えるので、
  468 node の MoE・gated-delta block では `all-linear` が 512 node の上限に入らない。そういう line は狭い adapter か完全な重み。
  DoRA・学習した bias・`modules_to_save`・`layers_to_transform`・融合射影・`fan_in_fan_out`・非正方 rsLoRA は v1 の外。
  prefetch は親の約 3 %(1.5B で約 53 MB)。
- **学習を IR で書くこと。** 理論上は可能(forward・backward・optimizer を compiler で 25 primitive に落とす。autodiff op は
  要らない)。v1 は学習を consensus に入れない(研究手法を凍結してしまうから)。将来の任意 profile として記録し、primitive で
  足りるか(optimizer state、勾配の累積、reduction、R による sampling、batching)を未決事項に残す。
- **報酬は段階的で、すべてに信頼ラベル(V・B・J・T)。** S1 = 完全一致で検証できる課題だけの bounty。S2 = 勝者の manifest が
  宣言した登録済み dataset への gain share(「実際に使ったかは検証できない」と明記)。S3 = ablation(研究)。trainer の報酬は
  vest し、後で証明が出れば没収。原資は line の手数料収入の一定割合 φ、候補の手数料、sponsor の預託で、PALW の worker 報酬には
  触れない。
- **ライセンス・プライバシー・RL。** closed API の出力は `TeacherLicence` 付きの `LICENSED_DISTILL` だけ。base model は派生物を
  許すライセンスだけ。実利用からの hard case は明示の opt-in(既定 off)。RL は IR で表せる報酬による RLVR で、rollout は
  PALW job。
- **容量と費用。** 評価 claim は ADR-0160 の容量を消費する。候補は登録料と評価料を払い、1 epoch の候補数に上限がある。seat の
  prefetch 負荷も数える。例:1.5B、n = 400、候補 4、1 項目 512 位置で約 10^6 位置 — 1 ノード 17 tok/s なら約 17 時間分で、
  多数の executor に分かれ、検証の複製は別。
- **脅威と比較。** poisoning と backdoor(評価は trigger を見られない。軽減はするが閉じない)、汚染、答え鍵の攻略、judge と
  尤度の攻略、sybil とコピー、ライセンス洗浄、結託、出題者の漏洩、beacon の grinding、費用 DoS、容量の圧迫。Bittensor・
  計算の貸し出し(ブリーフの "FLOP")・Virtuals との違いは、検証を先に置いた報酬と、チェーン自身の実行で測る改良。
- **工数(Phase A、VM なし、rung A も待たない)。** 既存コードに対する実装項目 A1〜A15(line の改良 policy と promotion、
  epoch 状態機械、hard case・setter・opt-in、合成 artifact、評価 job 種別、採点 pipeline、promotion 演算、報酬と pool、node 側、
  adapter の lowering、第 2 実装、drill、監査、soak)で合計 **60〜84 engineer-weeks(約 14〜20 EM)**。agent による実装は
  約 3〜5 週、**mainnet-safe は開始から約 8〜11 か月**。前提は PALW-TIR(Phase F)と RFC-0003 の pipeline(step 3〜5)だけ。

## Summary

A registered model should not stay as registered. This RFC defines the **Model Improvement Protocol**:
a consensus-native loop in which a line's real usage produces hard cases and revenue, anyone trains
candidates off chain by any method, the network evaluates the parent and every candidate on hidden
items drawn from the future, and the protocol promotes the best candidate that passes a conservative
statistical rule. Everything runs on PALW-TIR (RFC-0002) and RFC-0003's pipelines. There is no VM and
no EVM dependency.

- **Layers.** Layer 0 is PALW-TIR, where candidates are evaluated. Layer 1 is this protocol: epochs,
  datasets, the candidate registry, rewards and promotion. Layer 2 is optional VM automation
  (RFC-0005).
- **The division of work.** Training is outside consensus; evaluation is PALW-TIR; promotion is the
  protocol. The generation method is free and only the artifact is strictly verified — as Bitcoin checks
  that a block is valid, not how the ASIC that mined it was built. A candidate MUST NOT be required to
  come from the VM.
- **What is verified.** Exactly four things: the model artifact (IR class admission), the provenance
  policy (form and references, not truth), the evaluation result (court-adjudicated claims), and
  promotion eligibility (fold arithmetic). Training, provenance truth and freedom from backdoors are not
  verified, and every reward says which of its inputs are.
- **Evaluation** runs the parent and every candidate through RFC-0003 pipelines on the same inputs:
  exact match on canonical outputs of R-seeded generations, teacher-forced reference log-likelihood,
  and registered judge classes drawn by R at evaluation time (low weight, anchored). Promotion needs a
  paired win by `δ` with a lower confidence bound above zero and no regression beyond `ε`; otherwise the
  head stays, and every promotion can be rolled back.
- **Candidates are cheap to carry**: full weights, or the parent plus an adapter — an unmerged LoRA
  path in the IR graph — whose composite artifact lets seats reuse the parent's bytes.
- **Implementation.** The RFC ends with a plan against the existing code (spec 15's lines, the fold,
  Phase F's IR classes and court, ADR-0160's capacity) of 60–84 engineer-weeks, about 3–5 weeks of
  agent implementation and 8–11 months to mainnet-safe use.

## Motivation

1. **A registered model is frozen.** A class is its program, its weights and its tokenizer. Spec 15
   (ADR-0088) lets a line publish new versions and record proposals, evaluations and datasets, but all
   of it is declared: a developer promotes a version by signing it, and "evaluations are declarations
   from anyone" (PALW-MK-3). Nothing connects a line's usage to a better head, and nothing checks that
   a new head is better.
2. **The user's goal** is that models registered from outside grow stronger through RL and
   distillation. Real usage produces hard cases and revenue; teachers and trainers turn them into
   candidates; the network finds the one that is really better and makes it the head; and the loop
   repeats.
3. **Everything evaluation needs is already TIR.** Generation, likelihood and judge models are IR
   classes. Scoring is a small TIR program. Promotion is integer arithmetic in the fold. So the loop
   needs no VM (Phase A), and RFC-0005's VM becomes optional automation on top (Phases B and C).
4. **The Bitcoin analogy.** Bitcoin checks that a block is valid. It does not care how the ASIC that
   found it was designed. This protocol checks a candidate — its artifact, its declarations, its
   measured result — and does not care how it was trained. That keeps research methods free, and keeps
   consensus out of everything it cannot check.

## Goals and non-goals

**Goals.**

- G1. Three layers: TIR evaluates (Layer 0), this protocol decides (Layer 1), and any automation is
  optional (Layer 2, RFC-0005).
- G2. The generation method is free and only the artifact is verified. No candidate is required to have
  been produced by the VM, or by any particular method.
- G3. The protocol verifies four things and nothing else: the artifact, the provenance policy, the
  evaluation result and promotion eligibility.
- G4. Epochs are consensus-native: states, objects and transitions in the fold, advanced at DAA
  boundaries.
- G5. Evaluation is hidden and in the future; promotion is conservative and can be rolled back.
- G6. Candidates are cheap to carry: adapters reuse the parent's artifact.
- G7. Rewards are staged, and each names what it rests on (V, B, J or T).
- G8. Licences and privacy by default.
- G9. Evaluation is accounted for in the network's claim capacity (ADR-0160).
- G10. Implementable against the current code with no VM and no EVM contract.

**Non-goals.** Training inside consensus in v1 (§12). Verifying which data a trainer used. Code
evaluation by running tests (it needs RFC-0005's VM). Subjective validator weights. Renting agents,
GPUs or accounts. Paying from PALW worker rewards. Legal advice.

## 1. Layers and phases

```
Layer 2 (optional, RFC-0005)   automation: Phase B, bounded improvement workflows (PALW-BVM)
                                           Phase C, permissionless research programs and EXEC checkers (PALW-GVM)
───────────────────────────────────────────────────────────────────────────────────────────────────────
Layer 1 (this RFC, Phase A)    the Improvement Protocol: the line's policy · epochs · hard cases · datasets ·
                               the candidate registry · evaluation jobs · rewards · promotion · the lineage head
───────────────────────────────────────────────────────────────────────────────────────────────────────
Layer 0 (RFC-0002, RFC-0003)   PALW-TIR: candidates are IR classes; generation, likelihood, judges and scoring
                               stages are TIR executions, adjudicated by the TIR court
```

- **Phase A** is this RFC: IR only, complete on its own.
- **Phase B** (RFC-0005 Part I) lets a bounded VM program run improvement loops — for example
  `FOR generation IN 0..8 { generate a dataset; train; evaluate; keep the best }` — whose results
  still enter this protocol as ordinary candidates.
- **Phase C** (RFC-0005 Part II) adds permissionless research programs, and the EXEC checkers that
  code-by-tests evaluation needs.
- Neither B nor C changes a rule of this RFC. They add producers of candidates and datasets, and (C)
  more scoring kinds.

## 2. The division of work

| Activity | Where it runs | What consensus knows about it |
| --- | --- | --- |
| data collection, teaching, training | anywhere, by any method | nothing, beyond declarations |
| evaluation | PALW-TIR: evaluation jobs as claims, adjudicated by the court | every score, as a committed output |
| promotion, rewards, rollback | the fold | everything: it computes them |

**The generation method is free; only the artifact is strictly verified.** Normatively (PALW-MIP-1),
**a model candidate MUST NOT be required to have been produced by the VM**, or by any particular method,
tool or party.

### 2.1 The four things the protocol verifies

| # | What | How | Label |
| --- | --- | --- | --- |
| 1 | **the model artifact** | the candidate is an admitted IR class (Phase F admission v10) of the parent's family: same tokenizer, same output interface; its artifact is complete and available (§6) | V |
| 2 | **the provenance policy** | the candidate's declarations satisfy the line's policy: every dataset it names is registered, every licence class and teacher class is allowed, every `TeacherLicence` it cites exists and covers the use | V for the form and references; B for their truth |
| 3 | **the evaluation result** | evaluation jobs' claims, final under the court | V |
| 4 | **promotion eligibility** | the fold's paired rule over committed scores (§7.5) | V |

### 2.2 Trust labels

Every reward and every quantity an interface shows carries one of four labels.

| Label | Meaning |
| --- | --- |
| **V**, verified | recomputed by the court or computed by the fold: a PALW claim, a scoring stage, the promotion rule |
| **B**, bonded | a statement someone staked on, challengeable by a stated procedure; a false one forfeits the bond |
| **J**, judged | a registered judge class's output, verified as *the judge's* answer: deterministic, and gameable |
| **T**, trusted | not checkable at all, and stated as an assumption wherever a reward rests on it |

| Object | Label |
| --- | --- |
| a candidate's outputs and scores on evaluation items | V |
| a hard case's hardness for an exact-match or likelihood task | V (given the key: B) |
| an answer key or reference from a setter | B |
| a judge's score | J |
| **training** — which data a trainer used, and how | **T** |
| an artifact's provenance and licence | B |
| a winner's freedom from backdoors | **T**, mitigated (§15) |

No interface may present a B, J or T quantity as verified.

## 3. Lines, heads and the improvement policy

The protocol builds on spec 15's lines (ADR-0088) rather than beside them.

- **The line.** `PalwModelLineV1` (`consensus/core/src/palw_model_lines_v1.rs`): `(class, owner, name)`,
  with roles, a `current` version, previews, a contributor's permille of the owner's leg, and per-version
  usage (`PalwVersionUsageV1`: attempt claims, free-prompt claims, work leaves).
- **The head of a governed line is a class id in the line's improvement row** (`improvement_lines`),
  with a bounded head history, and it names an IR class. A legacy version is a root in force of one class
  (PALW-MK-2); Phase F's IR class id, however, commits to the artifact root, so new weights are a new
  class.
- **Amended (2026-09-29, implementation).** This RFC first recommended writing the promoted class id
  into spec 15's `PalwModelVersionV1.root` and making it the line's `current` version, so that no new
  pointer was needed (open question 1). Implementation found that form breaks three live readers of spec
  15's rows:
  1. `class_roots_in_force` reads every in-force version root of a class's lines, and attempt admission
     accepts any of them — so a class id would become an "artifact root" an attempt of the parent class
     could name;
  2. a promotion supersedes the parent's version, whose root then leaves force after the grace, so the
     parent class's attempts are refused and its cadence share goes dormant, while the promoted class —
     a separate IR class with its own share — inherits nothing;
  3. free-prompt usage is attributed through the current version's root, so it would land on the wrong
     version.
  The protocol therefore writes neither spec 15's version rows nor `current`: the governed line's head
  lives in its improvement row, and the market, the owner's leg and the parent class's share and roots
  are untouched. The trigger counts the head class's claims at `Final` in the same row (§4), not spec
  15's per-version usage, which attributes by artifact root.
- **Opting in.** The owner signs `ModelLineImprovementPolicySet { line, policy }` (a proposal, like
  every object here). From then on the line is **governed**: its head moves only by this protocol's
  promotion or rollback, and the developer's `ModelVersionPromoted` is refused on it.
  The policy changes only between epochs. Opting out takes effect after the current epoch and a delay.
- **The policy** (`PalwImprovementPolicyV1`):

  | Field | Meaning |
  | --- | --- |
  | `usage_threshold` | usage of the head since the last epoch (claims or work leaves) that opens an epoch |
  | `grid`, `w_collect`, `w_submit`, `w_holdout`, `w_eval` | the epoch's boundaries, in DAA (§4) |
  | `eval_spec` | the scoring pipelines and their weights, the regression and safety suites (roots), the judge set, `n`, `n_min`, `δ`, `ε`, `ε_s`, `α` (§7) |
  | `k_max` | the most candidates an epoch evaluates |
  | `fees` | the candidate registration fee, the evaluation fee per job, the hard-case fee |
  | `phi_permille`, `bounty_share` | the pool's funding and the share of it that S1 may spend (§8) |
  | `provenance` | the teacher classes and licence classes allowed, and whether full-weight candidates are allowed (§6) |
  | `rollback_epochs`, `vest_epochs` | the owner's rollback window `R`, and the vesting length `v` |

## 4. The epoch state machine

One epoch at a time per governed line. Every transition happens at a DAA boundary that the policy and
the chain's own facts determine, so every node advances every line identically, with no object needed
for most transitions.

```
            usage ≥ threshold at a grid boundary
  Idle ─────────────────────────────────────────────► Open ── t_fix ──► Submission ── t_close ──► HoldOut
   ▲                                                   (hard cases,      (candidates,                (post-freeze
   │                                                    datasets)         fees, bonds)               cases only)
   │                                                                                                   │ t_draw (+ beacon)
   │                                                                                                   ▼
   └── Vesting ◄── Promoted(class) / NoChange ◄── t_score ── Scoring ◄── t_eval ── Evaluating ◄── Drawn
```

| State | Enters when | What happens | Leaves when |
| --- | --- | --- | --- |
| `Idle` | the line opts in; an epoch ends | the head's usage since the last epoch is counted in the line's improvement row (the head class's claims at `Final`, §3) | a grid boundary finds usage ≥ `usage_threshold` → **OPEN_IMPROVEMENT_EPOCH** |
| `Open` | `t_open` | hard cases, registered datasets and teaching artifacts are admitted into the epoch's training material | `t_fix = t_open + w_collect` |
| `Submission` | `t_fix`: **`dataset_root` is fixed** — the Merkle root over the material admitted in `Open`, published for trainers | candidates are submitted, with fees and bonds, up to `k_max` in acceptance order | `t_close = t_fix + w_submit` → **CLOSE_SUBMISSION**: the candidate set freezes; nothing is withdrawn after it |
| `HoldOut` | `t_close` | only hard cases admitted from now on are eligible as evaluation items; setter sets must already be committed | `t_draw = t_close + w_holdout` |
| `Drawn` | the first block `d` DAA past `t_draw` | the epoch seed is that block's beacon (ADR-0044 F5/F15); R draws `n` items from the eligible pool, one judge per judged item from the judge set, and a generation seed per item; setter prompts are disclosed | at once |
| `Evaluating` | `Drawn` | evaluation jobs — one per item and subject (the parent and each candidate), plus pairwise judge jobs — are claimable (§7.2) | `t_eval = t_draw + w_eval`, after which no evaluation claim is accepted |
| `Scoring` | every evaluation claim is final, or the court window after `t_eval` has passed | setter keys are revealed; the fold scores and applies the promotion rule (§7.5) | at once |
| `Promoted(class)` / `NoChange` | `t_score` | the head moves, or not; pool payouts are scheduled; unexecuted evaluation fees are refunded | → `Vesting` |
| `Vesting` | promotion | rewards vest over `vest_epochs`; the owner may roll back within `rollback_epochs`; a proof may roll back and forfeit (§7.6) | the next epoch opens (vesting continues in parallel) |

An epoch with fewer than `n_min` eligible items at `t_draw`, or with no candidate, ends in `NoChange`.

### 4.1 Objects

Every name here is a proposal. No tag or object number is claimed; the fence and the ADR assign them.

| Object | Signed by | Effect |
| --- | --- | --- |
| `ModelLineImprovementPolicySet` | the owner | opts a line in, or changes its policy between epochs |
| `HardCaseSubmitted` | a bonded submitter | a hard case (§5.1) enters the current state's pool |
| `DataUseOptIn` | a job's committer | makes that job's prompt usable as a hard case (§10) |
| `SetterSetCommitted` / `SetterSetRevealed` / `SetterKeysRevealed` | a bonded steward | a private evaluation set: committed before `t_close`, prompts disclosed at `Drawn`, keys after outputs are final |
| `DatasetRegistered` | a bonded contributor | a dataset with its content root, licence classes, teacher classes and provenance commitment (§5.3) |
| `TeachingArtifactCommitted` / `TeachingArtifactRevealed` | a bonded teacher | an artifact for an S1 bounty (§8.1) |
| `TeacherLicenceRegistered` | a rights holder | a licence for `LICENSED_DISTILL` (§9) |
| `CandidateSubmitted` | the submitter | a candidate (§6), its declarations, its fees and bond |
| evaluation claims | executors | the existing claim objects, with the evaluation job family (§7.2) |
| `LineageHeadRolledBack` | the owner, or anyone with proof | restores the previous version (§7.6) |

The fold keeps one row per governed line (`improvement_lines`) and one per epoch (`improvement_epochs`),
entering `state_root` only once written, as every late table does (ADR-0087 M7).

### 4.2 Optional contracts

EVM-lane contracts MAY add bounty markets or sponsorship schemes that read the finalized facts of this
protocol — through a read path of the ADR-0089 D2 kind, which is itself optional. **Nothing in this
protocol depends on them**, and none of them can move a head.

## 5. Hard cases, datasets and teaching artifacts

### 5.1 Hard cases

```
HardCaseV1 { case_id, domain, prompt (token ids under the line's tokenizer),
             reference: ExactKey { commitment } | Continuation { commitment } | None,
             source: UsageOptIn { job_pin } | Setter { bond } | Artifact { artifact_id },
             head_evidence: Option<{ claim }> }
```

- **Sources**: real usage, with its user's opt-in (§10); bonded problem setters; and `SyntheticProblem`
  and `HardCaseVariant` artifacts.
- **References.** An exact-match task carries a committed key (the answer span); a likelihood task a
  committed reference continuation. A usage case may attach a reference under a bond; without one it is
  judged only.
- **Hardness.** For exact-match and likelihood tasks it is verified (V, given the key): `head_evidence`
  names a final evaluation-kind claim of the head that fails the key, or scores below the policy's
  likelihood floor. A judge-only case's hardness is J. A case the head passes is not hard and pays no
  setter reward.
- **Which pool a case joins** depends only on when it was admitted: before `t_fix`, the epoch's training
  material; in `HoldOut`, the evaluation pool; otherwise the next epoch's.

### 5.2 Verification types in Phase A

| Type | What decides it | Label |
| --- | --- | --- |
| **EXACT** | the canonical output's answer span equals the committed key (§7.3) | V (the key: B) |
| **LIKELIHOOD** | the teacher-forced log-likelihood of the committed reference (§7.3) | V (the reference: B) |
| **JUDGED** | a registered judge class | J |
| **HUMAN** | a bonded person, challengeable | B |

EXEC (running tests or checkers), CRITIC (an executed counterexample) and PROOF (a proof checker) need
a VM, and belong to RFC-0005 Phase C.

### 5.3 Teaching artifacts and registered datasets

```
TeachingArtifactV1 {
  kind:                  Answer | PreferencePair | SyntheticProblem | HardCaseVariant | RewardSignal | Critique,
  task_id, teacher_type, teacher_id, license_class, provenance_commitment, output_hash,
  verification_type:     EXACT | LIKELIHOOD | JUDGED | HUMAN,
  bond, commit: H(artifact ‖ salt), reveal,
}
DatasetRegisteredV1 { dataset_id, content_root, items, license_classes, teacher_classes,
                      provenance_commitment, bond }
```

| Kind | Verified by in Phase A | Label | Paid in |
| --- | --- | --- | --- |
| Answer — a solution trace ending in an answer span | EXACT against the case's key, compared by the fold after the key's reveal | V | S1 bounty; S2 |
| PreferencePair | two outcomes of which one matches the key and one does not; otherwise JUDGED or HUMAN | V (J, B) | S2 |
| SyntheticProblem | the setter's committed key | B | S1, to the setter, if the head fails it |
| HardCaseVariant | the parent's key, and the head verifiably failing the variant | V | S1, to the setter |
| RewardSignal | a SELF_PLAY rollout: the head's final claim and its EXACT or LIKELIHOOD score (§11) | V | S2 |
| Critique | JUDGED only (an executed counterexample needs Phase C) | J | S2 |

Kinds that need execution — `TestCase`, `Counterexample`, `VerifiedCode`, `ToolTrace`, `FormalProof` —
arrive with RFC-0005 Phase C.

**Teacher classes** are declared in every artifact and dataset:

| Class | What it is | Admitted when | Provenance label |
| --- | --- | --- | --- |
| OPEN_DISTILL | outputs of open-weight models whose licence permits training derivatives | the declared licence class is allowed by the policy; bonded attestation | B |
| LICENSED_DISTILL | outputs of a proprietary model | a registered `TeacherLicence` from the rights holder covers the use (§9) | V that the licence exists; B that the use stayed in it |
| SELF_PLAY | the head, or other registered classes, run as PALW jobs | the claims themselves | V |
| HUMAN | human-written work, possibly tool-assisted | bonded attestation of authorship and rights | B |
| TOOL_VERIFIED | outputs of deterministic tools and solvers | the outcome verified by EXACT | V for the outcome |
| PUBLIC_DATA | public datasets under permissive licences | bonded licence attestation | B |

**Admission** is commit–reveal: the commit's block orders submissions; the reveal must match
`output_hash`; content lives off chain, content-addressed; duplicates by `output_hash` (and by the
normal form of an answer span) count once, for the earliest commit, which defeats copying from the
mempool. Spam — malformed, unverifiable under its declared type, duplicate, or inadmissible by licence —
forfeits its bond. An honest failure costs only its fee.

## 6. Candidates and their artifacts

### 6.1 What a candidate is

A candidate is an **admitted IR class** (Phase F admission v10) of the parent's family: the same
tokenizer, the same output interface (a `Logits` program whose logits scheme equals the parent's), and a
program the family rule accepts. It is registered as any IR class is (`ClassRegisteredTirV1`, the 1 MSK
burn, the per-block cap), and then entered in the epoch by `CandidateSubmitted` with its fees, bond and
declarations. Its artifact is one of two kinds.

- **Full weights.** A single artifact root, with scales of its own. Allowed only if the policy allows it,
  and charged for the seats' prefetch (§6.7, §13).
- **Parent + adapter.** The candidate's program is the parent's program with an **unmerged LoRA path** at
  each adapted projection, and its artifact is **composite**: the parent's inventory, reused byte for
  byte, plus a small adapter section (§6.3–§6.6).

### 6.2 The adapter in the IR graph

The lowering is `misaka-palw-tir-lower`'s (`crate::lora`, `lower::lower_lora`; `tir/lower` a4bd7141f,
hf-coverage §12). It reads a PEFT adapter (`adapter_config.json`, `adapter_model.safetensors`) directly.
Each targeted projection keeps its parent weight and adds an unmerged path:

```
y = W·x (+ b) + (num/den) · B·(A·x)        A: [r, d_in], B: [d_out, r]
num/den = lora_alpha / r   (rsLoRA: lora_alpha / √r, at a square rank) — an exact rational
```

In integers, with every node a v1 primitive:

1. `A·x` is one exact `i32 × i16` product over the parent projection's own input codes. `A` is stored as
   `A' = A·diag(s_x / s_x0)` at per-row `i32` codes, so a split input needs no extra columns. The result
   is narrowed to `i16` at the adapter's own calibrated site `{site}.lora_a`.
2. `B·a` uses per-row `i16` codes, exact in `i64`.
3. `num/den` is applied as an integer `Mul` by `num` and a rounded `Div` by `den`.
4. One narrowing takes the result into the projection's **output scale**. Then comes an `Add` to the
   parent's narrowed output, and a `Clamp`.

To the lowering the node stays a `Linear`, so no pattern changes. Each adapted projection adds about 20
nodes. Merged and unmerged adapters do not round alike, so the candidate's semantics is the unmerged
program's. On four PEFT fixtures (ranks 4–64; q/k/v/o and MLPs; `all-linear`; rsLoRA):

- the unmerged float path matches `transformers`' merged weights to 3·10^−7;
- the integer candidate reaches top-1 1.000 and KL 3–5·10^−5 against the merged logits;
- the reference evaluator, the second implementation and the typed backend agree (hf-coverage §12).

Fidelity is measured, never required (RFC-0002 criterion 5).

### 6.3 Composite artifacts and the class id

The lowering orders params so that **the candidate's first `P` params are the parent program's,
byte-identical in declaration and tensor** (`lower::adapter_params_last`). Params `P..` are the adapter
section.

```
PalwTirArtifactRefV1 = Single { root }
                     | Composite { parent_class: Hash64, parent_root: Hash64, adapter_root: Hash64, p: u32 }
artifact_root(Composite) = H64(key "misaka-palw/improve/composite-artifact/v1",
                               parent_class ‖ parent_root ‖ adapter_root ‖ le32(p))
tir_class_id_v1 = H64(key "misaka-palw/tir/class-id/v1",
                      graph_ir_root ‖ H64(layout) ‖ artifact_root ‖ tokenizer_id)      // Phase F's formula, unchanged
```

- **The parent is never re-committed.** The composite root composes the parent's artifact root with the
  adapter section's root, which is an inventory over params `P..` only. Params `0..P` are served by the
  parent's inventory leaves, unchanged.
- **The class id binds four things** through Phase F's formula over the composite:
  - the parent class (`parent_class`, and through it the parent's program, layout and tokenizer);
  - the candidate's own program root (`graph_ir_root`: the parent's nodes plus the adapter paths);
  - the adapter root;
  - `P`.
- **Admission checks** that:
  - `parent_class` is the line's head, or another registered class of the line;
  - `parent_root` is its artifact root;
  - params `0..P` of the candidate's program carry exactly the parent program's declarations, in order;
  - no param at or after `P` names a parent tensor.
- **Court openings** of param `j` are made under `parent_root` if `j < P`, and under `adapter_root`
  otherwise. This is an appended variant of Phase F's artifact opening.

### 6.4 The calibration rule

- **The parent's scales are reused.** Every narrowing param in `0..P` is the parent's, byte for byte.
  That is what makes reuse possible.
- **The adapter's own narrowing params** — the `{site}.lora_a` sites, and each path's output-scale
  narrowing — are data in the adapter section, committed by `adapter_root`. The submitter calibrates them
  however it likes. No calibration set is pinned, because admission's range analysis already takes
  params at their dtype's full range.
- **The rule for clipping.** An adapter that moves activations past the parent's calibrated headroom
  (about 2×) **clips** at the parent's narrowing sites. That clip is the candidate's risk. It shows in the
  candidate's evaluation score, and the protocol does nothing else about it.
- **Recalibration makes a full-weight candidate.** Changing any parent-side narrowing param breaks
  byte-identical reuse. A candidate that needs recalibrated scales is therefore submitted as full weights
  (a `Single` artifact), if and only if the policy allows full-weight candidates, and it pays their
  prefetch (§6.7).

### 6.5 The node budget, and what an adapter may target

- An adapted projection adds about 20 nodes, and a block holds at most 512 (04b NF-12).
- The largest blocks of the corpus, MoE and gated-delta hybrids, already have 468 nodes. `all-linear`
  does not fit there; about two adapted projections do.
- **The rule.** An adapter may target any set of projections that keeps every block within 512 nodes.
  Admission refuses anything else by name (`NormalForm`), and `palw-class check-architecture` reports
  per block how many projections fit.
- Lines whose blocks are near the limit take full-weight candidates for broad changes, or narrow
  adapters (for example `q` and `v` only), until a leaner adapter form exists (open question 15).

### 6.6 Adapter forms out of v1

The lowering refuses these PEFT forms by name, so v1 tools cannot produce them as composite candidates:

- DoRA;
- trained biases (`bias ≠ none`, `lora_bias`);
- `modules_to_save`;
- `layers_to_transform` and `layers_pattern`;
- targets on fused projections (`qkv_proj`, `gate_up_proj`, `c_attn`);
- `fan_in_fan_out`;
- rsLoRA at a non-square rank;
- module-specific rank or alpha patterns.

Consensus does not parse adapter configurations. It checks the composite rule (§6.3) and admission. A
model trained with one of these forms therefore enters v1 only merged, as a full-weight candidate. Any
of them may later become a specified adapter form.

### 6.7 Prefetch and work

- **Prefetch.** A seat holding the parent fetches only the adapter section. The lowering stores `A'` at
  `i32` and `B` at `i16`, per row: `r · (4 · d_in + 2 · d_out)` bytes per adapted projection. At `r = 16`
  on the seven projections of a Qwen2.5-1.5B-shaped decoder (28 layers), that is about 53 MB, roughly 3 %
  of the parent's 1.6 GB IR artifact. A full-weight candidate costs the full 1.6 GB per seat.
- **Work.** The adapter adds `r · (d_in + d_out)` MACs per adapted projection and position: about 2 % of a
  1,536-wide model at `r = 16` (hf-coverage §12).

## 7. Evaluation

### 7.1 Items: hidden, and in the future

- **A temporal hold-out.** Items come from hard cases admitted in `HoldOut`, after the candidate set froze,
  so no candidate can have trained on them.
- **The draw.** At `Drawn`, R (RFC-0003, domain `CLASS_UNIFORM_V1`), keyed by the epoch seed — the
  beacon at the first block `d` DAA past `t_draw` — draws `n` items from the eligible pool.
- **Bonded setter sets.** Stewards commit private items (prompt and key commitments) before `t_close`.
  Prompts are disclosed at `Drawn`, when every candidate is already fixed. The references of
  teacher-forced items are disclosed at `Drawn` too, because a teacher-forced job reads its reference as
  prefill. Keys of generated items are disclosed only after every subject's outputs are final. A steward
  who does not reveal forfeits its bond, and its items drop out for every subject alike.
- **Caps.** No submitter or steward supplies more than a fraction `κ` of an epoch's items (§15).

### 7.2 The evaluation job

```
PalwEvalJobV1 { line, epoch, item, subject: Parent | Candidate(class_id), mode, pipeline }
  mode     = Generate { seed, max_new, stop_ids }           // RFC-0001 §A's rule, R domain 0 under `seed`
           | TeacherForced { reference }                     // prompt ‖ reference as prefill, logits over the reference
  seed     = H64(key "misaka-palw/improve/eval-seed/v1", epoch_seed ‖ item)    // the same for every subject
  pipeline = the policy's scoring pipeline for the item's domain
```

- **Everything is derived.** The chain derives each job's context from the epoch, the item and the
  subject, as Phase F derives an IR attempt's context (`palw_tir_attempt_v1`): the yardstick layout of
  the subject's class, `job_id = H(epoch ‖ item ‖ subject)`, and the item's prompt commitment.
  Executors choose nothing.
- **The pipeline** is an RFC-0003 pipeline: a subject stage (the subject class, in `Generate` or
  `TeacherForced` mode) followed by TIR scoring stages. It commits one step tree (PALW-GEN-3) and a
  `score_root` over the committed score tensor (RFC-0003's output digest).
- **Three additions to RFC-0003's pipeline format**, and nothing else (landed as A7, `rfc4/eval`):
  - a `Decode` stage kind — the text profile inside a pipeline, with FP V4's commitments and Phase F's
    `TirDecodeToken*` arms;
  - a `FinalizedOutput { claim, stage }` binding (token source 5) — an input bound to a final claim's
    committed output in the same epoch, opened under its output root, so a pairwise stage can read the
    parent's and the candidate's generations without running them again;
  - a `Key` token source (token source 4) — an exact-match key read as an input of the scoring stage,
    bound to the item's committed key.
- **Adjudication** is Phase F's arms and RFC-0003's pipeline rules, unchanged: `TirCone`, H dissection,
  `TirLogits`, `TirDecodeToken*`, `TirOutputDigestMismatch`.
- **Claiming.** Evaluation jobs are claimed openly: the first valid claim per job in the accepting
  chain's order is the one, it is paid the job's evaluation fee at `Final`, and it counts against the
  executor's claim capacity (§13).
- **Missing evaluations count for the incumbent** (amended 2026-09-29). An item missing a candidate's
  evaluation is a loss for that candidate; an item missing the parent's evaluation is a win for the
  parent against every candidate. Missing data can block a promotion and can never make one.
- **Not the reward path.** Evaluation claims earn their fee and no quantum, ticket or eligibility.

### 7.3 Scoring stages

Each is a TIR program in the scoring library (`misaka-palw-tir`, with golden vectors), run as a stage:

| Kind | What it computes | Cost | Label | Weight in promotion |
| --- | --- | --- | --- | --- |
| **ExactMatch** | the answer span between the item's delimiter ids in the generated canonical output (RFC-0003: generated token ids) equals the key's ids: `Compare`, masks from `Iota`, `Gather`, `ReduceSum` | one R-seeded generation | V (key: B) | primary |
| **RefLogLik** | `Σ` over the reference of the log-probability of each reference token under the subject: the library's wide softmax, `IntLn`, `Gather`, an exact `ReduceSum` (Q24) | one teacher-forced forward pass: prefill only, cheap, exact | V (reference: B) | primary for likelihood tasks |
| **Judge** | a registered judge (reward-model) class scores the subject's output; the judge is drawn by R at `Drawn` from the policy's judge set, and anchored (below) | one judge pass | J | low: a guard, never sufficient |
| **Pairwise** | a judge compares the parent's and the candidate's generations (read by `FinalizedOutput`), in an order drawn by R | one judge pass per pair | J | low: a guard |

- **Anchors.** A judge's jobs include anchor items whose exact answers are known. A judge whose anchor
  accuracy falls below the policy's floor is excluded for the epoch, and its scores are void.
- **Same seed, same item, every subject.** Pairing is exact: the parent and every candidate see the same
  prompt and the same randomness.

### 7.4 Code in Phase A

Code evaluated by running tests needs a VM, so it is **not available in Phase A**. It arrives with
RFC-0005 Phase C (EXEC). Until then code tasks use:

- **exact output** — the value a program prints, the result of an expression, a canonical single-line
  fix; or
- **likelihood** — the teacher-forced log-likelihood of a known-good reference solution or patch.

Both are weaker than tests. The policy says so, and the label (V given a B key) says what they rest on.

### 7.5 Promotion: a paired rule in integers

Each item gives, for candidate `C` and the parent `H`, a win, a loss or a tie. ExactMatch items compare
pass and fail; RefLogLik items compare the two log-likelihoods, by sign. Let `b` be `C`'s wins and `c`
its losses over `n` counted items. `C` is **eligible** iff all of the following hold:

1. **Enough evidence**: `n ≥ n_min`.
2. **It beats the head by δ**: `b − c ≥ δ · n`.
3. **Its lower confidence bound is above zero**: an exact one-sided sign test on the `b + c` decisive
   items rejects "no better" at level `α / K`, with `K` the number of candidates (a Bonferroni split).
   That is `b ≥ k(b + c, α / K)`, where `k` comes from a **pinned integer table** of binomial critical
   values. No float is involved.
4. **No regression beyond ε** on the policy's regression suite, by the same paired counts.
5. **No regression beyond ε_s** on the policy's safety suite.
6. **The judge guards hold**: the Pairwise and Judge outcomes show no significant loss (the same sign test,
   in the other direction). A judge can block a promotion, but can never make one.

The winner is the eligible candidate with the largest `b − c`, ties going to the earliest submission. It
becomes the line's current version, `LineageHeadSet`, applied by the fold at `t_score`. **If none is
eligible, the head stays.**

### 7.6 Rollback

- **By the owner**, within `rollback_epochs` of a promotion, without proof — the owner answers for the
  product.
- **By anyone, with proof**: a final claim of the promoted class failing a committed canary or safety
  item it was bound to pass, or a regression shown on items drawn in a later epoch, or a licence violation
  upheld on a bonded challenge.
- **Its effect**: `LineageHeadRolledBack` makes the previous version current again (the version history
  of spec 15 keeps every row), forfeits the winner's unvested rewards and bond (§8.4), and bars its
  submitter's candidates for the policy's period.

## 8. Rewards, staged

Every reward names what it rests on (§2.2). Every payout comes from the line's pool (§8.5). None of them
creates a quantum, a ticket or eligibility (ADR-0144 P1).

### 8.1 S1 — bounties, only for EXACT-verifiable tasks

- **A bounty per exact-match hard case** (V): paid to the first `Answer` whose answer span matches the
  key, in commit order, once the key is revealed.
- **Setter rewards** (V for hardness, B for the key): for a `SyntheticProblem` or `HardCaseVariant`
  that the head verifiably fails.
- **Forfeits** (V): spam and duplicates forfeit their bonds to the pool.
- **Nothing else pays in S1.** Likelihood, judged and human outcomes do not earn bounties.

### 8.2 S2 — the gain share, through declared, registered datasets

- At a promotion, a policy share of the winner's epoch reward goes to its submitter (the trainer), and
  the rest to the data.
- The data share goes to the **registered datasets** that the winner's `CandidateSubmitted` declared, in
  proportion to their declared weights, with a cap per dataset and per contributor.
- **The trust assumption is stated** in the policy, in the object and in every interface: the chain
  cannot see what a trainer used (T). A trainer can name datasets it never used — the caps bound that —
  or omit ones it did, which costs only the omitted.
- ADR-0088 D7–D8 already pays an adopted proposal's contributor a permille of the owner's leg while its
  version is current. S2 is the protocol's form of the same idea for governed lines.

### 8.3 S3 — marginal attribution ("Proof of Useful Data"), as research

- **Ablation contests.** For a dataset slice `X`, independent trainers train with and without `X`; each arm
  is replicated by **at least two independent trainers**; the arms are evaluated as candidates are (§7).
  The marginal gain of `X` is the paired difference on the same hidden, future items — for example slice
  A +4.1 points, B +0.2, C −1.0.
- **Payout** follows the gain's lower confidence bound where it is above zero: A is paid, B is inside the
  noise, C is not paid.
- **Teacher reputation** — Alpha, Beta, Gamma — comes from observable records: adoption (the registered
  datasets that winners declared) and downstream gain (S3). It sets bond sizes and caps. It never
  multiplies a payout.
- **This is research.** Replication and bonds are the only guards against a trainer who shapes the arms,
  and each contest costs several training runs per slice. S3 stays on testnets within this RFC.

### 8.4 Vesting and forfeit

A winner's S2 reward and its candidate bond **vest over `vest_epochs`**. They are forfeited if a backdoor,
a canary failure or a licence violation is proven before they vest (§7.6). A model cannot be un-trained;
money not yet paid out is the only lever left.

### 8.5 The pool

- **Funding:**
  - a parameter share `φ` of the line's fee income — the owner's market leg (spec 15 PALW-MK-7), split
    the way `split_owner_leg_v1` already splits it with an adopted contributor;
  - candidate registration fees;
  - sponsor deposits, bound to the line's pool as a model-sink output is bound to a market object
    (PALW-MK-11).
- **Not PALW worker rewards.** They are the reward path (ADR-0144 P4–P7), and the market's buyback slice
  (PALW-MK-8) already takes its share of them.
- **Nothing is minted.** Evaluation fees are escrowed per job and paid to the executor, or refunded if the
  job is never executed.
- **Payouts** leave the fold as outputs, by the path market settlements already use.

## 9. Licence and legal

- **Closed APIs.** API terms commonly forbid using outputs to train competing models. Outputs of
  proprietary models — Claude-, GPT- and Gemini-type agents — are admissible as training data **only
  under `LICENSED_DISTILL`**, backed by a registered `TeacherLicence` from the rights holder:

  ```
  TeacherLicenceV1 { rights_holder_key, model_family, scope: { domains, uses }, per_use_fee, expiry, revocation }
  ```

  This is also the long-term market: model companies register their teacher and earn a teacher fee per
  admitted use.
- **Tools and agent operators.** Such a model may help produce an artifact that is verified another way
  (an answer that matches a key), and that artifact may enter training data, **only if its licence allows
  it**. The default is no.
- **The base model.** A line may be governed only if its base model's licence permits derivatives. The
  owner declares it in the policy, under a bond. Research-only and no-derivatives licences cannot enter.
- **Attestations** of licence and provenance are bonded and challengeable. A proven false attestation
  forfeits the bond and excludes the contributor's datasets from future declarations.
- **Limits.** The chain cannot adjudicate law. A bond prices dishonesty; it does not make an unlicensed use
  lawful. This RFC is not legal advice.

## 10. Privacy

- A hard case taken from real usage needs the job's **explicit data-use opt-in**, which is **off by
  default**.
- The opt-in is a separate object, `DataUseOptIn { job_pin }`, signed by the job's committer. The FP job
  format does not change (RFC-0001 §A is frozen).
- The chain checks that the case's prompt matches the job's committed `prompt_token_ids_hash`. Opting in
  publishes the prompt ids and the head's output with the case; redacting first is the user's
  responsibility.
- `PanelDa` jobs stay private unless their user opts in. Nobody can opt in another person's job.

## 11. Reinforcement learning

- **RL with verifiable rewards fits.** ExactMatch and RefLogLik are public reward functions expressible
  in TIR. Trainers run RL against them off chain; judged rewards are available too, at J.
- **Rollouts are PALW jobs.** The head's rollouts on hard cases run as PALW jobs, and their scores as
  scoring stages. Submitted as `SELF_PLAY` `RewardSignal` artifacts, they are verified reward data (V) that
  any trainer can buy or use without trusting whoever generated them.
- **The chain judges the outcome, not the update.** Only the next candidate's measured result is paid.

## 12. Training as IR: possible, and not in v1

- **In theory, training is expressible.** A compiler can lower a forward pass, its backward pass and an
  optimizer step to the 25 primitives: every derivative is itself a composition of `MatMul`, elementwise
  operations, tables and reductions. No autodiff primitive is needed, because differentiation happens in
  the compiler, not in the IR.
- **v1 does not put training into consensus.** A consensus training method would freeze today's research
  methods into protocol, and it would make every candidate pay for a replayable training run that nobody
  needs in order to check the candidate.
- **A future optional profile.** A line MAY, in a later version, accept candidates that carry a
  replayable IR training run. Whether the primitives suffice is open question 9: optimizer state (moments
  as `Fixed` states, or weights as states), gradient accumulation over a batch (exact sums, but
  over-sized reductions), sampling and shuffling (R), batching (tensor axes), and the fidelity of integer
  training itself.

## 13. Capacity and cost

- **Evaluation claims are claims.** Each reserves capacity by ADR-0160's stage-1 rule (`⌈w_c / ρ⌉`)
  like any other claim, and each seat that verifies it pays its execution. An epoch's evaluation budget —
  `n × (k_max + 1)` subject jobs plus judge jobs, times their positions — is capped per epoch by the
  policy and by a network ceiling expressed as a share of the span's claim capacity, so evaluation cannot
  crowd out attempts.
- **Candidates pay.** A registration fee (plus the IR class's own 1 MSK burn), and evaluation fees for
  their own `n` subject jobs and pairwise jobs.
- **Candidates per epoch** are capped by `k_max` and by the network's ceiling.
- **Prefetch.** Every seat that may verify an evaluation claim must hold the subject's artifact. Adapters
  cost about 3 % of the parent (§6.7), so four adapter candidates add about 210 MB to a 1.5B line; a
  full-weight candidate adds 1.6 GB per seat, and is priced for it.
- **A worked size.** A Qwen2.5-1.5B-shaped line, `n = 400`, four candidates plus the parent, 256 prompt
  and 256 generated tokens per item: 400 × 5 × 512 ≈ 1.0 × 10^6 positions. At the 17 tokens per second
  that Phase F's typed backend measured for a 1.5B decoder (2026-09-28), that is about 17 hours of one
  node, spread across many executors, before replicas. Teacher-forced items cost only a prefill. The
  pairwise jobs add one judge pass per item and candidate.

## 14. Layer 2: what RFC-0005 may add, and what it may not change

- **Phase B** (RFC-0005 Part I, the bounded VM) may run improvement loops as programs — generate data
  with PALW jobs, filter it with this RFC's scoring stages, train (off chain, or later through the
  training profile of §12), evaluate locally, keep the best — and submit the result as an ordinary
  candidate.
- **Phase C** (RFC-0005 Part II, the Turing-complete VM) adds permissionless research programs and the
  EXEC, CRITIC and PROOF verification types: code by tests, executed counterexamples, proof checkers, and
  the artifact kinds that need them.
- **Neither may change this RFC's rules.** A candidate produced by a VM program is evaluated and promoted
  exactly as any other, and a candidate produced without one is never disadvantaged (PALW-MIP-1).

## 15. Threats

| Threat | Mechanism | Mitigation | What remains |
| --- | --- | --- | --- |
| Poisoning and backdoors | a trainer or contributor plants a trigger in a winner | evaluation cannot see a trigger it does not contain; canary and safety items, vesting and forfeit, rollback | **not closed**: a well-hidden trigger passes evaluation, and only a later proof and rollback reach it |
| Contamination | evaluation items leak into training | the temporal hold-out, setter prompts disclosed only after the freeze, the R draw | a leak through a setter (below) |
| Answer-key gaming | a setter's keys favour one candidate's quirks | per-setter caps `κ`, independent stewards, per-setter anomaly checks, bonded key challenges | within one cap |
| Likelihood gaming | a candidate memorises reference-like text | references disclosed only after the freeze; the hold-out; likelihood paired with exact-match tasks | on-distribution memorisation is also learning |
| Judge gaming | outputs tuned to a known judge | the judge drawn by R at evaluation time, anchors, a guard-only weight | inherent to judges |
| Sybils and mempool copying | another's artifact or case resubmitted | commit–reveal, dedupe by hash and normal form, bonds | near-duplicates |
| Licence laundering | unlicensed teacher output declared OPEN or HUMAN | bonded attestations, challenges, exclusion, `TeacherLicence` | laundering nobody detects |
| Trainer–contributor collusion | a declaration pays friends | registered datasets only, per-dataset and per-contributor caps, S2 labelled T | whatever fits under the caps |
| Setter leakage | a setter hands future items to a trainer | many setters, caps `κ`, private sets from independent stewards | within one cap |
| Beacon grinding | a producer grinds the draw | a beacon that postdates the hold-out, a large `n`, the private sets | small |
| Cost DoS | spam candidates, cases or artifacts | fees, bonds, `k_max`, the per-epoch evaluation budget | — |
| Capacity crowd-out | evaluation starves attempts | the evaluation budget capped as a share of claim capacity | — |
| Evaluation starvation | nobody claims a candidate's jobs | fees make claiming pay; an unclaimed parent job voids the item for everyone | a candidate nobody will run fails |
| Head capture | a bad policy or promotion | the owner's policy, the owner's rollback window, proof-based rollback | trust in the owner's choice of policy |

## 16. How this differs from nearby designs

| Design, as publicly described | What is paid for | Who decides the reward | This protocol's difference |
| --- | --- | --- | --- |
| Bittensor | a subnet miner's output, as validators weight it | validators' subjective weights, aggregated by consensus | promotion and S1 rest on outcomes the chain computes; judges are guards only |
| Compute-rental designs (the brief's "FLOP") | compute or time delivered | the buyer | nothing is paid for compute: only verified artifacts and measured gains ("agent mining, not agent rental") |
| Virtuals | tokenised agents valued by market demand | the market | a head changes only by a statistical win on hidden, future items; the line's market (spec 15) is kept apart from the evidence |

The claim is narrow: verification first, and wherever the protocol cannot verify (training, provenance,
backdoors) it says so and prices the risk with bonds, vesting and rollback.

## 17. Implementation plan and effort

Against the current code (the Phase F line, `tir/phase-f-f7`, and the live integration line), in
engineer-weeks for two to four experienced people. **Phase A depends on nothing but RFC-0002's Phase F
and RFC-0003's steps 3–5** (`TirProgramV2`, pipelines, their court). It does not wait on the EVM rung or
on any VM.

| # | Work item | Where | Depends on | Engineer-weeks | With agents |
| --- | --- | --- | --- | --- | --- |
| A1 | Spec chapter 17; the vector plan (scoring programs, the sign-test table, epoch transitions) | `docs/spec/palw/17-model-improvement.md` | — | 3–4 | 1–2 days |
| A2 | The line's improvement policy; the head as a class id in the improvement row (§3, amended); protocol promotion; head history and rollback; the developer's promotion refused on a governed line | `palw_state_v2.rs` (`improvement_lines`), spec 15's `ModelVersionPromoted` arm | Phase F | 4–5 | 2–3 days |
| A3 | The epoch state machine: `improvement_lines` and `improvement_epochs`, DAA-driven transitions, the usage trigger (the head class's claims at `Final`), `dataset_root`, the freeze, the draw (R and the beacon) | `palw_state_v2.rs` (fold, `state_root`), a new `palw_improve_epoch_v1.rs` | A2 | 6–8 | 3–4 days |
| A4 | Hard cases, setter sets, registered datasets, teaching artifacts, the data-use opt-in, `TeacherLicence` | appended `PalwConsensusObjectV2` variants; `palw_prompt_ids_v1.rs` for the prompt-hash check | A3 | 4–6 | 2–3 days |
| A5 | Candidates and composite artifacts: `CandidateSubmitted`, the family rule, `PalwTirArtifactRefV1`, sub-root openings | `palw_tir_class_v1.rs`, `palw_tir_artifact_v1.rs`, `palw_tir_admission_v1.rs`, `palw_artifact.rs` | Phase F | 4–6 | 2–3 days |
| A6 | The evaluation job family: derived contexts (as `palw_tir_attempt_v1`), job identity, open claiming, fees, capacity reservation, panels | `palw_tir_attempt_v1.rs`, `palw_job_identity.rs`, `palw_job_state.rs`, `palw_capacity_formulas_v1.rs` | A3; RFC-0003 steps 3–5 | 6–8 | 3–4 days |
| A7 | The scoring library (ExactMatch, RefLogLik, Judge, Pairwise) with golden vectors; the `Decode` stage and the `FinalizedOutput` binding; court coverage | `misaka-palw-tir` library, the RFC-0003 pipeline code, `palw_tir_court_v1.rs` | RFC-0003 steps 3–5 | 5–7 | 3–4 days |
| A8 | Promotion: the pinned binomial table, paired counts, `δ`, `ε`, the suites, the judge guards, the winner, `NoChange` | a new `palw_improve_promotion_v1.rs` | A3, A6 | 2–3 | 1–2 days |
| A9 | Rewards and the pool: `φ` (the `split_owner_leg_v1` precedent), fees, deposits, S1, S2 caps, vesting, forfeit, payouts | `palw_model_market_v1.rs`, `palw_model_lines_v1.rs` | A3, A8 | 5–7 | 2–4 days |
| A10 | Node side: the epoch watcher, the evaluation executor (generation, teacher-forced, scoring), adapter prefetch, `palw-class improve` for cases, datasets and candidates | `misaka-palw-sdk`, `misaka-palw-tir-exec`, `kaspad/src/palw_panel.rs` | A5–A7 | 5–7 | 3–4 days |
| A11 | Adapter lowering: **landed** on `tir/lower` (a4bd7141f, hf-coverage §12). What remains is emitting the composite layout (`P`, the adapter-section inventory) and per-block node-budget reports in `check-architecture` | `misaka-palw-tir-lower`, `misaka-palw-sdk` | Phase F | 1–2 | about 1 day |
| A12 | An independent second implementation of the scoring library, the promotion rule and the transitions; differential tests; a model-checked state machine | a ref2-style lane | A3, A7, A8 | 4–6 | 2–3 days |
| A13 | Drills: D-M1 a whole epoch on a salted t12 chain with a small real model and an adapter that wins; D-M2 one that must not; D-M3 a court battery on evaluation claims; D-M4 rollback by the owner and by proof; D-M5 the fence crossing on the shipping binary; D-M6 copying, setter leakage, grinding, fee DoS | scripts, the drill harness | all | 5–7 | 3–5 days |
| A14 | Two external audits (the state machine and the court; the economics) and their fixes | — | A1–A13 | 3–4 | — |
| A15 | Testnet soak (at least 3 epochs), bounty, activation | — | A14 | 3–4 | — |
| | **Total** | | | **60–84** (≈ 14–20 engineer-months) | implementation ≈ 3–5 weeks |

- **Implementation with AI agents**: about **3–5 calendar weeks** for A1–A13, to a whole epoch passing
  D-M1…D-M6 on a salted testnet-12 chain, with two or three agents and a lead who reviews and
  integrates.
- **Mainnet-safe**: about **8–11 months** from the start. That covers the second implementation's triage
  and the differential tests (1–2 months), two audits with fixes (3–4 months, partly in parallel), at
  least 3 epochs armed on a testnet (2–3 months at the epoch lengths a testnet uses), a bounty window
  overlapping them, and the activation. It cannot come before PALW-TIR and RFC-0003's pipelines are
  themselves mainnet-safe. The calibration is RFC-0005's *Effort* section: implementation compresses by
  one to two orders of magnitude, while second implementations, drills, audits and soak do not.
- **Human-only**, the same work is about 9–12 months with three people.

## Proposed Spec text (new chapter `spec/palw/17-model-improvement.md`)

Applies past `palw_improvement_v1`.

- **PALW-MIP-1 (the method is free).** A model candidate MUST NOT be required to have been produced by the
  VM, or by any particular method, tool or party.
- **PALW-MIP-2 (four checks).** The protocol MUST verify a candidate's IR class admission and its family
  relation to the parent, its declarations against the line's provenance policy (form and references),
  its evaluation results, and its promotion eligibility. It MUST NOT condition anything else on how a
  candidate was produced.
- **PALW-MIP-3 (governed lines).** A line is governed only after its owner signs an improvement policy.
  A policy MUST change only between epochs, and opting out MUST take effect only after the current
  epoch and a delay.
- **PALW-MIP-4 (the head).** A governed line's head MUST be an IR class id, kept with its history in the
  line's improvement row, and it MUST move only by this chapter's promotion or rollback. The protocol
  MUST NOT write spec 15's version rows or `current`.
- **PALW-MIP-5 (epochs).** A governed line MUST run at most one epoch at a time, advanced by the
  transitions of §4 at the DAA boundaries its policy fixes.
- **PALW-MIP-6 (opening).** An epoch MUST open at the first grid boundary at which the head's usage since
  the last epoch reaches the policy's threshold.
- **PALW-MIP-7 (the dataset root).** At `t_fix` the epoch's `dataset_root` MUST be fixed over the material
  admitted while `Open`.
- **PALW-MIP-8 (candidates).** Candidates MUST be submitted in `[t_fix, t_close)`, at most `k_max`, each an
  admitted IR class of the parent's family, with its fees and bond. No candidate MAY be withdrawn after
  `t_close`.
- **PALW-MIP-9 (the hold-out).** Evaluation items MUST be drawn by R under the epoch seed from the cases
  admitted in `HoldOut` and from committed setter sets. Setter prompts and teacher-forced references MUST
  be disclosed only after `t_close`, and the keys of generated items only after every subject's outputs
  are final. An item missing a subject's evaluation MUST count for the parent.
- **PALW-MIP-10 (evaluation jobs).** Every item MUST be evaluated for the parent and every candidate by
  evaluation jobs whose contexts the chain derives, with one generation seed per item shared by every
  subject.
- **PALW-MIP-11 (scores).** A score MUST be the committed output of the policy's scoring pipeline, and it
  is adjudicable as any committed output.
- **PALW-MIP-12 (judges).** A judge MUST be drawn by R at `Drawn` from the policy's registered judge set.
  A judge failing its anchors MUST be excluded for the epoch. A judge's score MAY block a promotion and
  MUST NOT make one.
- **PALW-MIP-13 (promotion).** The fold MUST promote the eligible candidate with the largest `b − c` under
  §7.5's rule, with the pinned table, and otherwise leave the head unchanged.
- **PALW-MIP-14 (rollback).** The owner MAY roll a promotion back within `rollback_epochs`. Anyone MAY
  roll it back with a proof of §7.6's kinds.
- **PALW-MIP-15 (composite artifacts).** A composite candidate's first `P` params MUST be the parent
  program's params, in order, with the parent's declarations, served under the parent's artifact root.
  Its artifact root MUST bind the parent class, the parent's artifact root, the adapter section's root
  and `P`, as §6.3 states. A candidate that changes any parent-side param, its scales included, MUST be a
  full-weight candidate (§6.4). Every block MUST stay within the 512-node limit (§6.5).
- **PALW-MIP-16 (rewards).** Payouts MUST come only from the line's pool. S1 MUST pay only EXACT-verified
  artifacts. S2 MUST pay only registered datasets that the winner declared, within their caps. Trainer
  rewards MUST vest over `vest_epochs` and MUST be forfeited on a proof of §7.6's kinds.
- **PALW-MIP-17 (not a reward path).** Nothing in this chapter MAY create a quantum, a ticket or
  eligibility, or draw on PALW worker rewards.
- **PALW-MIP-18 (licences).** A declaration of `LICENSED_DISTILL` MUST cite a registered, unexpired
  `TeacherLicence` that covers the use. A governed line's base model licence MUST permit derivatives, as
  its owner declares under a bond.
- **PALW-MIP-19 (privacy).** A hard case MAY use a job's prompt only under that job's `DataUseOptIn`, and
  its prompt MUST match the job's committed prompt hash.
- **PALW-MIP-20 (capacity).** Evaluation claims MUST reserve capacity as claims do (ADR-0160), and an
  epoch's evaluation budget MUST NOT exceed the policy's cap or the network's ceiling.

## Activation plan

One fence, with Phase F D1's shape:

```
palw_improvement_v1: Option<PalwImprovementFenceV1 { activation, scoring_set_id, sign_table_id,
                                                     court_version, ceilings }>
```

It is Some-only in both fingerprints, collapses to `never()` as a whole option, is visited
activation-only by `for_each_fence`, and sits at an unused height. `scoring_set_id` hashes the scoring
library, and `sign_table_id` the pinned binomial table. `ceilings` bounds `k_max`, `n`, the positions
per epoch and the evaluation budget's share of claim capacity. `validate_palw_v2` refuses to arm it
without `palw_tir_v1`, RFC-0003's `palw_gen_v1` and `palw_kary_court` at or below it, or on a ruleset
without the A-2 tolerance.

| Step | Work | Exit gate |
| --- | --- | --- |
| 1 | A1–A12 (§17) | vectors pass on two implementations; the state machine is model-checked |
| 2 | Drills D-M1…D-M6 on a salted t12 chain, on the shipping binary | all pass |
| 3 | testnet-12 (or its successor), at an unused height after RFC-0003's fence; a first governed line, adapters only | at least 3 epochs with no unresolved defect; audits closed |
| 4 | Full-weight candidates and S2 | a further epoch |
| 5 | Mainnet | after the audits, the soak and the bounty; S3 stays on testnets |

## Alternatives

| Alternative | Why not |
| --- | --- |
| **The market in EVM contracts, with consensus reduced to a read path and a head field** (an earlier draft) | The user chose consensus-native epochs. Contracts remain an optional add-on (§4.2) |
| **Developer promotion only** (spec 15 today) | Declarations, not verification. Kept for lines that do not opt in |
| **Declared evaluations** (`ModelEvaluationPosted`) | Declarations again. Kept as records |
| **Training inside consensus** | It would freeze research methods and charge every candidate for a replay nobody needs (§12). A later optional profile at most |
| **Proof of training** | Float, nondeterministic and huge; nothing practical proves a training run today |
| **Judges as the primary reward** | Gameable, so judges only guard |
| **Paying for compute or agent time** | It pays for effort, not results ("agent mining, not agent rental") |
| **Validators' subjective weights** | Opinions can be bought; scores computed by the chain cannot |
| **Requiring candidates to come from a VM program** | Forbidden by PALW-MIP-1: it would make research methods a consensus matter |

## Security, principles and compatibility

- **Threats** are in §15. The one that remains open is a well-hidden backdoor, which evaluation cannot see.
- **PALW-PR-1 and P2.** No validity rule of a claim, a reward or a prompt reads what an output means here.
  Scores are outputs of registered scoring programs that the line's owner chose, and they decide only
  that line's head. Whether the user reads PALW-PR-1 as covering this too is open question 2; if so,
  promotion would have to move to the optional contracts.
- **P1, P3 and P4–P7.** Evaluation jobs and every payout are outside the reward path. Registration stays
  permissionless: any bond may submit a candidate. A line's improvement never changes another line's
  economics.
- **Compatibility.** Lines that do not opt in are untouched. The FP job format does not change. Every
  object is appended, dropped by name below the fence and skipped by older builds under A-2.

## Open questions

1. **IR versions** — *resolved 2026-09-29*: the head is a class id in the governed line's improvement row,
   and spec 15's version rows are not written (§3). The first recommendation — a version naming a class
   id — broke three live readers of spec 15 (the roots in force that attempt admission accepts, the parent
   class's roots and share after a supersede, and free-prompt usage attribution).
2. **PALW-PR-1's scope**: is protocol promotion by registered scoring programs acceptable (recommended),
   or must promotion move to the optional contracts?
3. **The usage trigger**: claims or work leaves, and the threshold.
4. **Parameters per domain**: `δ`, `ε`, `ε_s`, `α`, `n`, `n_min`, `k_max`, `κ`, the windows, `φ`,
   `vest_epochs`, `rollback_epochs`.
5. **Continuous scores**: the sign of the paired difference (recommended), or a fixed-point paired LCB.
6. **Claiming evaluation jobs**: open claiming (recommended), or assignment by R as a seat duty.
7. **The evaluation budget**: its share of ADR-0160 capacity.
8. **The `Decode` stage, the `FinalizedOutput` binding and the `Key` source** — *resolved*: added to
   RFC-0003's pipeline format (A7, `rfc4/eval`).
9. **Training as IR**: do the 25 primitives suffice for a future training profile (optimizer state,
   gradient accumulation, reductions, sampling by R, batching)?
10. **Challenging a setter's key without a VM**: bonded human or judged challenges only, until Phase C.
11. **Artifact content**: who must keep teaching artifacts and datasets available for audits and
    challenges, and whether that is bonded.
12. **The safety suite**: who writes it for a line, and whether it may veto a winner that passes
    everything else.
13. **Full-weight candidates**: allowed from the first epoch, or adapters only at first (recommended).
14. **Cross-line teachers**: may another line's head teach a governed line as `SELF_PLAY`, and under
    which licences?
15. **A leaner adapter form** for blocks near the 512-node limit (MoE and gated-delta hybrids at 468):
    fewer nodes per adapted projection, or adapters shared across projections.

## Decision

<Open.> The drafter's recommendation:

- Accept Phase A as the first deliverable of the improvement roadmap, as the user ordered.
- Implement A1–A13 as soon as RFC-0003's steps 3–5 land. Arm on a testnet with adapter-only candidates
  and S1 first; add full-weight candidates and S2 after an epoch; keep S3 on testnets.
- Start RFC-0005 only after Phase A is fully implemented.
