# ADR-0144 — PALW pays for the inference you were going to run anyway

**Status:** CONSTITUTION, 2026-09-19. This ADR fixes what PALW is FOR. It changes no code and arms
no fence. Its purpose is to make the next four redesigns answerable: a proposal is in scope if it
serves the sentence below, and out of scope if it does not.

> **A person uses a local LLM on their own machine, with prompts they chose for their own reasons,
> and the same inference that answered them is the inference the chain rewards.**

Remote inference markets, GPU rental and third-party inference serving are **non-goals**. A
localhost OpenAI-compatible endpoint, so existing tools can reach the user's own model, is in scope.
An internet-facing gateway that sells somebody else's GPU is not.

## 1. Why this has to be written down now

The mechanism for it already exists and already says so. `palw_freeprompt_v3.rs` opens with

> ADR-0044 (FP-01): the free-prompt job, its execution commitment, and the quantized one-shot receipt
> spend — **the data layer of "the user's own inference becomes the consensus work."**
> `job — what the USER chose to run: their tokens, under a registered class, from a bond`

and the weight is real: a `FreePrompt` claim reaching `Final` retires
`pwu/quanta × spent.len()` of safe weight, beside an `Attempt` claim's `claim.pwu`
(`palw_state_v2.rs:10223-10228`). This ADR adds no new mechanism. It promotes an intent that is
already in the tree to the principle the rest of the protocol is judged against.

**It is written now because the measurement says the economy is not there.** On testnet-11 between
2026-09-15 and 2026-09-19 the node log carries **nine** `FreePromptCommitted` lifecycle objects, the
last on 09-18 01:54, and **zero** `palw-fp` subsystem lines, against 1,839 `palw-panel` lines, 10,206
`palw-interval` lines and a recent block mix that is 100 % algo 6. Essentially all reward today comes
from protocol-generated anchored jobs. The useful path is built, proven and carries none of the
economy.

That is not an argument that the design is wrong. It is an argument that **whatever is cheapest to
farm is what the economy will do**, and right now the cheapest thing to farm is a job nobody asked
for. A protocol that meant to buy useful inference and is buying synthetic inference has become an
LLM-flavoured proof of work, and it will stay one until the useful lane is the profitable one.

## 2. The five principles

**P1 — Local first.** Rewardable inference is inference the user ran on their own machine for their
own use. Remote GPU marketplaces are a non-goal, and no part of the reward path may assume one.

**P2 — Free prompt.** The protocol does not choose the question. Code, prose, translation, RAG,
agents, images — whatever the person actually wanted. The protocol never judges whether a prompt was
*worth* asking.

**P3 — Same inference, one purpose.** The answer the user reads and the work the chain rewards are
the same execution. Not a second run, not a proof job beside it.

**P4 — Representation-neutral accounting.** Reward may change only when *verified work* changes.
It must NOT change with `tile_len`, serialization, how a commitment is split, a registrant-declared
multiplier, the shape of a profile, map iteration order, or any self-reported cost.

**P5 — Efficiency is rewarded; accounting tricks are not.** The same canonical work on a faster GPU,
a better runtime, a better quantisation or a better architecture must leave the miner *more* profit
— that is the technical progress the network wants. Making the same work merely *look* larger must
leave them with nothing.

And the rule that makes P2 survivable:

**P6 — Local inference is unlimited; reward eligibility is scarce and protocol-assigned.** A person
may run their model ten thousand times a day. Only the inferences a future beacon makes eligible
earn. Spam economics are controlled by the scarcity of eligibility, never by the chain forming an
opinion about a prompt.

**P7 — Model admission is permissionless; economic weight is earned through verified use.**

Any user may register a new compatible local model **without modifying `main`**, without adding the
model to a hard-coded consensus list, and without a model-specific protocol upgrade or fork
activation. Model identity, architecture and execution semantics are carried as protocol-defined
canonical data, so a model nobody has heard of enters through the same generic path as one that
shipped with the network.

Registration alone MUST NOT grant unrestricted reward, consensus weight or full eligibility. A newly
registered model starts at bounded or zero eligibility and earns capacity only from
protocol-verifiable evidence: independent validation, correct execution, sustained real local use,
completed claims, continued compliance with the accounting rules.

**The distinction that makes this safe, and the one most likely to be got wrong:**

```
the price of one unit of canonical work   →  NEVER changes with usage
how much reward-eligible work a model
may contribute                            →  grows with verified use
```

A model does not become worth more per inference because it is popular. It becomes able to put MORE
of its inference in front of the protocol. Pricing popularity would make the economy a contest to
appear used; budgeting it makes the economy a contest to actually be used.

Specifically:

* adding a model MUST NOT require changing the `main` source tree merely to name or recognise it;
* registrants MUST NOT choose their own reward multiplier, work value or admission strength;
* usage growth MUST be derived from verified protocol events, never from self-reported popularity
  or self-declared compute;
* repeated legitimate use MAY increase the reward-eligible work budget available to that model;
* increasing usage MUST NOT change the accounting value of one unit of canonical work;
* a newly registered model MUST NOT alter the reward rate, difficulty, accounting coefficients or
  economic position of any unrelated existing model;
* efficiency improvements may increase miner profit; representation tricks may not increase reward
  per canonical work;
* admission and growth rules MUST stay generic, so future architectures arrive permissionlessly
  without model-specific consensus code.

The lifecycle, and what each transition controls:

```
Candidate → independently validated → Probation → verified local use
          → ActiveLimited → sustained verified use → Active
```

Each transition changes **how much** reward-eligible work the model may contribute — never what one
unit of canonical work is worth.

### What of this already exists, and what does not

The shape is implemented. `PalwModelLifecycleStateV1` already gates admission by state —
`admission_permille()` returns **50 ‰ in Probation, 100 ‰ in ActiveLimited and 1,000 ‰ when Active**
— a class is admitted into `Probation { probes_passed: 0 }`, and promotion is driven by
`probation_claims` (10) COMPLETED CLAIMS, which are protocol-verified events rather than anybody's
word. That is P7's budget-not-price rule, already in the tree.

**Two things are not there, and P7 is written to name them rather than to imply they are handled.**

First, **the unit price is exactly what a registrant declares**, which is the reward audit's critical
finding: `claim.pwu = expected_attempts × pwu_per_inference`, and `pwu_per_inference` is the step-leaf
count of a canonical job the registrant writes. Until that is a derived quantity, P7's central
sentence is violated at the root — the lifecycle budgets HOW MUCH work a model may contribute while
the registrant still sets what a unit of it is worth.

Second, and the correction that matters for any decision about the registry fence:
**`palw_model_registry` does not gate registration.** Permissionless `ClassRegistered` is already
open and has been since ADR-0049 Decision H ("the refusal that stood here was never a policy, it was
the absence of a check"). What the fence adds is the LIFECYCLE — so arming it is the brake described
above, and disarming it does not close permissionless registration, it removes the only throttle a
new class currently meets. A proposal to disarm the registry in the name of safety is therefore
backwards on its own terms, and this paragraph exists because that proposal was made, by the author
of this ADR, before the code was read carefully enough.

## 3. What the protocol verifies, and what it deliberately does not

| verifies | does not |
|---|---|
| the class was a registered one | whether the question was worth asking |
| the inference actually ran, as committed | whether the answer was good |
| how much canonical work it was | who the user is |
| that the same work is not paid twice | what the prompt said |
| that this inference held an eligibility ticket | |

## 4. Execute now, settle later

Mining must not make the product worse. A person asks a question and wants the answer immediately;
they cannot wait for a lottery. So the order is:

```
t0  user types a prompt
    -> commitment over prompt, model, params is fixed
    -> local inference starts
    -> the answer is shown          <- the product, finished here
t1  a future beacon resolves
    -> was this inference eligible?
       yes -> receipt / proof -> claim -> reward
       no  -> it was just a local inference, and nothing else happens
```

**The beacon must resolve AFTER execution began.** This is not a UX convenience, it is the
anti-grinding rule, and the tree already has it: "The quantum ticket consumes a BEACON — an
attempt-class chain block, whose hash costs one inference per re-roll — that does not exist yet when
every field of the claim is fixed on chain" (`palw_freeprompt_v3.rs`). Reversed — ticket first, then
choose what to run — a miner draws a winning ticket and spends it on the cheapest prompt they can
compose, which is the failure this whole ADR exists to prevent.

## 5. What this ADR does NOT decide

**The unit of canonical work.** It is not decided here because the audit of 2026-09-19 showed both
available answers are wrong, and saying otherwise would bless a number that cannot carry the weight:

* **STEP leaves** — what is paid today — is a count of committed Merkle tiles, and `tile_len` is a
  registrant-chosen field bounded only by `[4, 65_536]` (`palw_step.rs:649`). Two independent probes
  put the admissible spread in pay-per-MAC-executed at **427x** on testnet-11's own dense graph, and
  a legal profile built from catalogued kernels reaches **1.00 MAC-eq per leaf** against the live
  classes' 3,957–22,077. That is P4 violated at the root.
* **MAC-equivalents** — the proposed replacement — carry no memory-traffic term, so they price a
  dense GEMM and a KV read alike. **The first draft of this ADR cited a 13.7x fleet measurement here
  and that citation was wrong**: the numbers came from `#[cfg(test)] mod tests` fixtures in
  `palw_verification_profile_v1.rs`, and the production path overrides a measured p99 with a
  reference-rate estimate anyway. The red-team refuted it, and the honest statement is weaker and
  still disqualifying — MAC-equivalents have not been shown to track real cost, and the controlled
  experiment that would settle it (both live classes' draw jobs replayed warm on one host, artifact
  resident and then not) has not been run.

The reason neither works is the same: **one scalar cannot price dense GEMM, routed experts,
attention, KV traffic and quantised kernels at once**, because the hardware does not. The direction
this ADR fixes, without fixing the arithmetic, is that canonical work is a **vector** derived from
the registered graph and the execution facts —

```
CanonicalInferenceWork {
    dense_compute, routed_expert_compute,
    attention_prefill, attention_decode,
    kv_read, kv_write, ...
}
```

— converted to reward units by **protocol coefficients that no miner may set**, calibrated across
hardware, runtimes and quantisations. How those coefficients are set, versioned and updated is
consensus-critical and is the hardest open question here; it gets its own ADR and its own adversarial
review before anything arms.

**KV reuse is part of that question, not separate from it.** A normal long chat re-sends a context
the executor already holds. Paying prefill for 30k cached tokens and 2k new ones as though 32k were
computed is P4 violated through a different door, and the free-prompt credit rule does exactly that
today: pay scales ~62x with prompt length while a producer holding the prefix's KV cache spends ~3 %
more. A miner's own word for "that was a cache hit" can never be the input. The commitment must make
*which KV state was consumed, which token range was newly evaluated, and which state was produced*
checkable by a seat that re-runs it.

## 6. Order of work

**0. Do not widen the current economy.** No new fence that extends reward or admission until 2 and 3
land. This is the only item that binds today.

1. **This ADR.**
2. **Reward accounting redesign** — P4 and P5 made true. Nothing a registrant writes may multiply
   reward.
3. **Class admission redesign** — registration and reward eligibility become separate states.
4. **Local FreePrompt as the standard path** — MISAKA Studio end to end, including cache.
5. **Shrink synthetic Attempt** — first make it the less attractive fallback, then replace its role
   as the beacon source with consensus randomness that costs no inference, so the network stops
   running LLM work nobody wanted purely for security.

Writing the constitution before the accounting is deliberate: the ADR says what the economy is FOR,
and step 0 says the economy does not grow until the accounting can serve it.

## 7. The release this sits beside

The build rolled to testnet-11 on 2026-09-19 (params fingerprint `c3a5e91d…`) schedules
`palw_model_registry` at **DAA 7,101** (`params.rs:20454`). That height opens permissionless class
registration — the precondition of every critical finding the audit confirmed. Existing classes are
NOT affected (`check_class_admits_claim` refuses a rowless class only when `work_target_active`,
which is dormant on every preset), so the release is safe for what is running. Whether 7,101 should
open registration before item 2 lands is an operator decision, and this ADR's position is that it
should not: item 0 is the reason this section exists.

## 8. How we will know it worked

Not a green test suite. This:

> A person uses MISAKA Studio normally for a day — code, documents, RAG, long chats — never once
> changes a prompt to suit mining, and some of that real inference earns. And nobody can change the
> reward-to-work ratio of the same inference by how they wrote a profile or registered a model.

The first half is a product measurement. The second half is P4, and it is the one an adversary will
test for us.

## 9. Alignment pass (2026-09-21)

Earlier unimplemented ADRs that contradicted this constitution were amended in place (bodies not
rewritten; dated banners and end sections only):

| ADR | what contradicted this file | what the amendment does |
|---|---|---|
| [0073](0073-real-demand-work-bears-the-weight.md) D2 / Phase ④ | self-prompt "exactly as good as" a canonical job; receipt share/position as an operator activation | withdraw the self-dealing reading; Phase ④ waits on §6 items 2–3 |
| [0077](0077-a-prompt-a-person-would-type-is-a-claim-the-court-can-try.md) | public commercial gateway as PALW goal | keep R0 / Phase A / local Decision 3; public serving out of reward |
| [0078](0078-what-was-made-from-it-is-committed-the-thing-never-rides.md) D7 | a derivation transformer earns PALW leaf weight | door closed unless the transformer **is** the user's local inference |
| [0079](0079-a-pure-function-needs-no-permissions-the-sandbox-is-for-the-host.md) | Done-when = public LLM entrance | sandbox stays for the loopback host; public entrance is not a PALW product |
| [0101](0101-a-membership-is-proven-by-the-chain-and-served-by-anyone-and-a-position-never-moves.md) D4/D6/D7 | anyone-serves inference marketplace | keep membership pins; withdraw unimplemented serving/discovery |
| [0130](0130-bps1-is-hardened-before-it-is-widened.md) D7–D8 / §5 | widen BPS, derived panel share, DA reward | stay deferred under §6 item 0 |
| [0131](0131-a-claim-is-paid-for-the-compute-it-cost-in-economic-compute-not-leaves.md) D3–D6 | arm a scalar CCU after shadow | shadow may measure; do not arm until [0146](0146-a-coefficient-is-justified-by-the-arbitrage-it-permits.md) |
| [0069](0069-e2e-adjudicability-is-the-price-of-weight.md) leftover pwu | close a hole by adding synthetic weight | close by shrinking Attempt (§6 item 5) |
| [0075](0075-certification-is-a-consensus-object.md) gateway remainder | (aligned, restated) | remaining work is local `/v1` reading genesis ∪ chain |

Orthogonal and not amended: 0023 (EVM lanes), 0010–0018 (validator ops; 0126 already retired the
PALW coupling), 0063 D1/D4 (BIP39 / `miner`), 0140 (heartbeat as emergency generator), 0141
(decides nothing), 0104 (never written). Implementation order: [`README.md`](README.md).
