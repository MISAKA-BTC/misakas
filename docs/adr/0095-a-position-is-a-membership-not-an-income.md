# ADR-0095 — A position is a membership, not an income

* Status: PROPOSED 2026-09-07; **IMPLEMENTED 2026-09-07 → 2026-09-10** behind
  `Params::palw_model_benefits` — scheduled on testnet-11 at DAA 2,400
  (`PALW_RC_MODEL_BENEFITS_FENCE_DAA`), `None` on every other preset and on every card — **except
  §4.9's holder mark**, which is a fold write and waits for a decision (§10). §7 lists the steps;
  §10 records what landed and what the implementation found.
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decision 1's
  "grants nothing but the right to sell it back" — it now also grants what its LINE declares;
  [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) Decision 2 (a version's
  entry and promotion are constrained by the line's declaration) and Decision 12 (rows to read).
* Builds on: 0087 (no transfer, the curve), 0088 (lines, versions, previews, evaluations,
  proposals, roles), 0090 (the seeded pair), 0091 (the reward buys the pair — income was already
  refused).

## 0. The sentence this ADR is

A position buys no income and no vote; it buys **what the line's developer owes its holders** — the
new version first, the private beta, the front of the queue, the experimental modes, a voice in what
ships next — and the chain's job is to make the holding provable, the promise readable, **the
exclusivity window a rule the fold enforces**, and the abandoned promise lapse on its own.

## 1. What the operator asked, in the operator's words

> Model Position を「投機対象」から「AI サービスの会員権・アクセス権」に近づけられます。単なる
> 「保有していると報酬が貰える」ではなく、**Position を持っていることでモデル利用上の明確な優位性
> が得られる**ようにする。Early Access／Private Beta／Priority Inference／新機能の先行提供。Tier は
> 保有量で分けるが、**保有量に比例して利益を受け取るのではなく、あくまでサービス上の権利にする**。
> **Developer が、この Model Line の Position Holder には何を提供するかを設定できる**ようにする。
> 重要なのは、**MISAKA プロトコルが価格を支えないこと。**

And, on the first draft of this ADR:

> **より Holder が モデル提供者からの恩恵を受けられる設計に**強化修正して。

The first draft declared everything and enforced nothing: every benefit sat behind "the developer
promised, and the market punishes a liar". That is too little. This revision keeps the boundary
honest — the chain still cannot serve an inference — but takes every part of the promise that IS
chain-visible and makes it a rule instead of a hope. Sections 4.4, 4.6 and 4.7 are that change.

## 2. Why a membership, and why this market was already shaped for one

[ADR-0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) removed the last thing that
looked like income: five percent of a model's mining reward buys from its pair and the chain retires
what it buys, so **no holder is ever handed anything**. What was left was a position that grants
nothing at all. A thing that grants nothing and can only be sold is the definition of a speculative
object, and that is the wrong shape for a network whose point is models people use.

Three properties this market already has make a membership the natural next thing, and they are not
properties a token normally has:

* **A position cannot be transferred** (ADR-0087 Decision 5). A membership that cannot be
  transferred cannot be scalped or lent; the only way in is to pay the curve, which RAISES the price
  for the next entrant, and the only way out is to sell back, which lowers it.
* **The supply is fixed and whole** (ADR-0090): five hundred thousand memberships a line, forever,
  and a holder's stake is a count anyone can read.
* **Usage already buys the pair** (ADR-0091), so a model that is actually mined buys its own
  memberships. Service demand and production push the same number, and **nothing here promises a
  price or pays a holder** — the operator's constraint that the protocol must not support the price
  is kept by construction.

## 3. The boundary, and what is on each side of it after this revision

**The chain cannot serve an inference, cannot keep a published root secret, and cannot see a
download.** Any design that pretends otherwise is lying to holders. What it CAN do is more than the
first draft admitted:

| the chain ENFORCES (a rule, refused at the fold) | the chain RECORDS (a fact, read by anyone) | the chain CANNOT (and does not claim) |
|---|---|---|
| a version may not be promoted before the declared lead has passed (§4.4) | who holds how many, at any height | serve the artifact, or run a queue |
| while an early-access lead is in effect, a version must ENTER as a preview (§4.4) | what the line declared, and when | verify a download happened |
| a weakening of the declaration takes effect only after notice (§4.7) | how long a holder has held without selling | make a developer answer an email |
| a declaration that outlives its cadence or expiry stops granting anything (§4.6) | every evaluation and proposal, and whether a holder wrote it | judge whether a model got better |

So the exclusivity window — the thing a holder is actually buying when a line declares Early Access
— **is not a promise in this design. It is arithmetic the fold performs.** Everything a developer
does off chain is still theirs to do or fail to do, and §4.6 makes failing to ship visible without
anyone having to file a complaint.

The most common misreading, again: **early access is not secrecy.** A published root is public. What
a holder gets first is the **artifact** — one to thirty-four gigabytes of weights that have never
been on chain — and a place in whatever queue the developer runs. The root is what lets a holder
check that the bytes they were handed are the bytes the line published.

## 4. Decisions

### 4.1 A line declares what its positions grant, and the declaration is on chain

`ModelLineBenefitsDeclared { line_id, tiers, cadence_daa, expires_daa, signature }`, signed by the
line's **owner** — the cold role of ADR-0088 Decision 6, not the developer, because this is what the
line promises rather than what it ships. `tiers` is an ordered list of at most eight
`{ min_units, grants (a bitset), lead_daa, min_hold_daa, note ≤64 bytes }`, strictly increasing in
`min_units`. A declaration replaces the previous one wholly, subject to §4.7's notice; an empty list
withdraws it, also subject to notice. Priced like every registry object (rent, ADR-0088 Decision 11).

### 4.2 The grants are a small closed set, and the set belongs to the chain

A bitset the whole network reads one way, so a wallet renders any line's card without knowing the
line:

| bit | grant | what the holder gets |
|---|---|---|
| 0 | `EARLY_VERSION` | the artifact of a new version, `lead_daa` before the line may make it current — **and §4.4 makes that window a rule** |
| 1 | `PRIVATE_BETA` | versions published as previews are served to holders and to nobody else |
| 2 | `PRIORITY_INFERENCE` | the line's gateways serve holders' jobs ahead of others' |
| 3 | `EXPERIMENTAL` | modes the line runs but has not made default: longer context, a thinking mode, tools, a new quantisation |
| 4 | `DEVELOPER_ACCESS` | the line's own room — proposals, research previews, where the next version is argued about |
| 5 | `INFERENCE_QUOTA` | served capacity: a request allowance the gateway honours, stated in the tier's note |
| 6 | `HOLDER_VOICE` | evaluations and proposals from holders carry the holder mark and the tier they held at (§4.9) |
| 7 | `SUPPORT` | the line answers holders' reports first |

What a line may NOT declare is anything that pays. There is no bit for a share, a rebate, a discount
in MSK, or a claim on the reserve, and an unknown bit is **refused at the fold**, not stored and
ignored (§6 N5): a promise no reader can render is not a promise. **A grant is a service or it is
not a grant.**

### 4.3 The tier is a pure function of chain state, and both lanes count

`palw_model_benefit_tier_v1(state, line, holder, daa) -> Option<TierIndex>`: the highest tier whose
`min_units` the holder meets **and** whose `min_hold_daa` their tenure satisfies (§4.5). A holder's
balance is the sum of the carrier lane and the EVM lane — ADR-0089 keeps these in separate
namespaces, and a person is not two people. The function lives in `kaspa-consensus-core` so a
gateway, a wallet and the explorer compute the same answer, and it takes the height it is asked
about, so "were they a holder when they asked?" has exactly one meaning.

### 4.4 The lead is a consensus rule, not a promise — **this is the revision's core**

Let `L` be the largest `lead_daa` over the tiers that declare `EARLY_VERSION` in the declaration in
effect (§4.6, §4.7). While `L > 0`:

* **`ModelVersionPromoted` is refused** before `published_daa + L`. The fold returns
  `ModelBenefitLeadNotElapsed { line, version, promotable_at }`. A developer who wants to promote
  sooner must first lower their own declaration, and §4.7 makes that take notice.
* **A version may not be published straight to current.** `ModelVersionPublished` with a
  non-preview status is refused while `L > 0`; the version must ENTER as a preview and be promoted
  later. Without this clause the whole mechanism is bypassable in one field, and the first draft was.

The two clauses together are what a holder is buying: **the window exists whether or not the
developer feels like honouring it.** What the chain still cannot do is put the artifact in the
holder's hands; that stays the developer's act, and §4.6 is what happens when they stop performing
it.

### 4.5 Tenure: a membership rewards not selling

Each `(line, holder)` carries `holding_since_daa`. It is set when a holder's balance rises from
zero, **preserved when they buy more**, and **reset to the current height whenever their balance
falls** — selling any part of a holding restarts the clock. A tier may require
`min_hold_daa`, and the tier function refuses a holder who has not held that long.

This is the anti-flip lever, and it is the honest description of what it measures: not "how long you
have held this many", but **how long since you last sold**. A holder who buys more just before a
release keeps their old clock; that is a deliberate choice, because punishing a holder for
increasing their stake would be perverse.

### 4.6 A promise that stops being kept lapses by itself

A declaration names `cadence_daa` (how often the line undertakes to publish a version) and
`expires_daa`. `palw_model_benefits_in_effect_v1(row, line, daa)` returns nothing — no tiers, no
grants, **and therefore no lead for §4.4 to enforce** — when either

* `daa ≥ expires_daa`, or
* `cadence_daa > 0` and `daa > last_version_published_daa + cadence_daa`.

Nobody submits anything: a line that stops shipping stops granting, on the block that crosses the
line, and the explorer shows `LAPSED` with the height it lapsed at. This is the answer to "what
makes the developer keep delivering" that the chain can actually give — it cannot verify a download,
but it can see that nothing has shipped for three months, and it can stop the line from advertising
a membership it is no longer servicing. A lapsed declaration is revived by declaring again, which
starts §4.7's notice afresh for anything it strengthens.

### 4.7 Taking a benefit away takes notice; giving one takes effect at once

A new declaration is compared with the one in effect. It is **strengthening** if every tier's grants
are a superset, no `min_units` rose, no `min_hold_daa` rose, and `expires_daa` did not fall;
otherwise it is a **weakening**. Strengthening applies at the block it lands in. A weakening — a
removed grant, a raised threshold, a shortened expiry, a withdrawal — is stored as
`pending { tiers, effective_daa = daa + PALW_MODEL_BENEFIT_NOTICE_DAA }` and the OLD declaration
stays in effect until then.

Without this, everything above is theatre: a developer would declare rich benefits, take a holder's
money at the curve, and withdraw the declaration in the next block. The notice is the holder's
protection, and it is the reason a benefit card can be read as something more durable than the
current block.

### 4.8 A holder proves the holding without spending anything

A gateway needs "this caller holds ≥ N of line L, and has held since H".
`palw_model_benefit_challenge_v1(line, holder, nonce, daa)` is a message the holder signs with the
key their position is held under — the ML-DSA-87 payout key on the carrier lane, the secp256k1
account on the EVM lane. The gateway verifies the signature and reads the tier at `daa`. Nothing is
submitted, nothing is spent, and a holder proving membership does not cost the network a byte.

### 4.9 Holders get a voice, and a path to be paid for work rather than for holding

With `HOLDER_VOICE`, an evaluation (ADR-0088 Decision 5) or a proposal (Decision 7) records
`by_holder_tier: Option<u8>` — the tier its author held at the height it landed. The explorer ranks
holder evaluations first, and a developer looking for what to fix next reads the people who paid to
be there.

This is also the one place a holder can legitimately receive MSK from the line, and it is worth
being precise about why it is not income: ADR-0088 Decision 8 pays the **contributor share** of the
owner's leg to the author of an **adopted proposal** while that version is current. That is payment
for work that was adopted, available to holders and non-holders alike; it is not a return on
holding, it does not scale with units, and no rule here routes anything to a holder because they
hold. The membership buys standing and attention. It never buys a share.

### 4.10 What a participant reads

`getPalwModelLine` gains `benefits` (the tiers in effect, any pending weakening with its height,
the cadence, the expiry, and whether it has lapsed); `getPalwModelBenefitTier(line, holder)` answers
the gateway's question. `misaka palw line-benefits --line --tier …` declares them; `misaka palw
benefits --line [--key-file]` reads them and says which tier a key is in and what the next tier
costs at today's price. The site shows the card on the line page and the trade page — what this
position gets you, what the next tier needs, when the promise expires — because the reason to buy
belongs before the buy.

### 4.11 A fence of its own — the first draft was wrong about this

The draft said "no fence of its own; under `palw_model_lines`". **That is wrong on any chain where
the registry is already live**, which testnet-11 is: this ADR writes two collections that enter the
state root — the declarations (§4.1) and the tenure clocks (§4.5) — so switching it on with the
registry's fence would move the root under a running network the moment the code shipped, and old
and new nodes would disagree about the next block. A consensus change gets an activation.

So: `Params::palw_model_benefits`, `Some` only where `palw_model_lines` is also armed (a membership
over a registry that does not exist is meaningless), scheduled on testnet-11 at
`PALW_RC_MODEL_BENEFITS_FENCE_DAA` and `None` on every other preset and on every mainnet card.
Below the fence a declaration is refused, no tenure clock is written, and §4.4's two refusals never
fire — so a chain that has not armed it keeps exactly the state root it had. The carriage gains a
tail of its own (`0x95`) for the same reason, and both new collections enter the root only when
non-empty.

**A card does not arm it, deliberately.** A membership is a promise a LINE makes, and arming the
mechanism from genesis on a network whose lines do not exist yet would be a default rather than a
decision. The test that compares a card's fences with testnet-11's names this exemption, so it stays
a decision someone took.

## 5. Security — the four principles, checked

* **Nothing is minted, nothing is paid.** No decision here moves a sompi. The closed grant set of
  §4.2 is what stops "just a small rebate" from arriving later as a `tiers` field.
* **The credential cannot be forged.** A tier is a function of state plus a signature over a nonce.
* **The credential cannot be lent.** Positions do not transfer, so a membership cannot be rented;
  a holder may proxy for a friend, which costs them their own place in the queue, and no rule can or
  should prevent that.
* **A declaration is a statement, and the chain never endorses more than it can check.** §4.4 is
  enforced, §4.6 lapses on arithmetic, and everything else is labelled as declared.

| | threat | why it is not one |
|---|---|---|
| A1 | declare rich benefits, sell into the demand, withdraw next block | §4.7: a withdrawal takes notice, and the old declaration governs until then |
| A2 | declare a 30-day lead and promote the next day anyway | §4.4: the fold refuses the promotion |
| A3 | bypass the window by publishing straight to current | §4.4's second clause: while a lead is in effect a version must enter as a preview |
| A4 | declare a lead, then never ship again, keeping the card on the site | §4.6: the declaration lapses at the cadence and grants nothing |
| A5 | buy in the block before a release, take the tier, sell after | §4.5: `min_hold_daa` is the line's own answer, and the tenure clock resets on any sale |
| A6 | a whale buys the top tier and resells access | positions do not transfer; proxying costs them their own slot |
| A7 | tiers used to sell weight, votes or fee shares | the grant set is closed and contains none of them |
| A8 | a gateway is handed a stale balance | the challenge names a height and the gateway reads the balance at it |
| A9 | lower the lead to promote today, using §4.7 as the escape | lowering a lead is a weakening, so it takes notice too |

## 6. Invariants the tests must hold

* **N1 (no payment).** No path added here writes a payout, a position, or a reserve.
* **N2 (the tier is the balance and the clock).** `benefit_tier` is the highest tier whose
  `min_units` and `min_hold_daa` the holder meets at the height asked, and `None` below the first.
* **N3 (both lanes, one person).** 60 units on the carrier lane and 60 on the EVM lane is the
  100-unit tier.
* **N4 (the owner declares).** A declaration signed by the developer or the maintainer is refused.
* **N5 (the set is closed).** An unknown grant bit is refused at the fold.
* **N6 (ordering).** Tiers strictly increase in `min_units`, at most eight, or the object is refused.
* **N7 (the lead is a rule).** Promotion before `published_daa + L` is refused; at exactly
  `published_daa + L` it is accepted — the boundary is pinned on both sides.
* **N8 (no straight-to-current).** While `L > 0`, a non-preview publish is refused.
* **N9 (notice).** A weakening does not change what `in_effect` returns until `effective_daa`; a
  strengthening changes it in its own block.
* **N10 (lapse).** Past the cadence or the expiry, `in_effect` is empty AND §4.4 stops refusing —
  a lapsed promise constrains nobody.
* **N11 (tenure resets on a sale, not on a buy).** Buying more preserves `holding_since_daa`;
  selling one unit resets it to the current height.
* **N12 (the lapse is not a state write).** `in_effect` is a read-time function of the row and the
  line's last publication, so two nodes at the same height agree without anyone submitting anything.

## 7. Order of work

1. This text; the README row; the banner on ADR-0087. **(done)**
2. `palw_model_benefits_v1`: the grant set, the tier row, `tier_for_units`, `in_effect`, the
   strengthening comparison, the challenge message, and their goldens. **(done — 9 tests)**
3. State: the row, the tenure map, the fold arm, the refusals (N4–N6, N9), and §4.4's two
   refusals in the version paths (N7, N8). **(done — 6 tests, and the fence of §4.11)**
4. RPC (`benefits` on the line), CLI (`line-benefits`, and the card on `line-show`). **(done)**
   §4.10's other two readers — `getPalwModelBenefitTier` and `misaka palw benefits` — were not in
   this step's "done" and landed on 2026-09-10 (§10). **(done)**
5. The site: the benefits card on the line and trade pages, the next tier's distance, the lapse.
   **(done 2026-09-10 — the card and the lapse had landed with the store rename; where the connected
   account stands, and what the next tier needs, landed here, §10)**
6. A reference gateway check — verify the signature, read the tier, choose a queue — so the serving
   side has something to copy rather than invent. **(done 2026-09-10, §10)**
7. §4.9's holder mark — `by_holder_tier` on an evaluation or a proposal, the tier its author held at
   the height it landed. **Not done, and not an implementation detail:** it is a fold write into rows
   that enter the state root, so it needs its own activation (the lesson ADR-0094's amendment
   records), and on testnet-11 a height after 2,400 or a second flag day. §10 says what it needs.

## 8. What is deliberately not decided

* **A bond behind the promise.** Considered and rejected for now: the only thing a bond could
  secure that the chain can adjudicate is shipping cadence, and §4.6 gets that without escrowing
  anything or inventing a payout path. Anything richer needs the chain to see a download.
* Whether a line may declare benefits before its class is `Active`. Probably yes; nothing turns on
  it until a gateway exists.
* Whether the chain should carry a gateway endpoint for a line. It is a URL, it rots, and the
  registry is not a directory.
* The units of `INFERENCE_QUOTA`. The tier's note states it and the gateway honours it; putting a
  number in consensus would freeze a serving decision that has nothing to do with consensus.

## 9. Number hygiene

0095 was the README's next free number on 2026-09-07. The next is 0096.

## 10. Implementation record

**2026-09-07** (`14c453d1`, on `origin/main`): §7 steps 1–4 as recorded there — the pure module,
the state (declaration row, tenure map, fold arm, §4.4's two refusals, §4.7's notice), the fence of
§4.11, `benefits` on `getPalwModelLine`, `misaka palw line-benefits` and the card on `line-show`.

**2026-09-10** (`feat/adr-0095-membership-serving`):

* **The tier read** (`b86d6808`). `getPalwModelBenefitTier(line, holders)` — op 177 through
  rpc-core, the service, gRPC (messages 1156/1157), wRPC and the integration test — answers from
  `PalwChainStateV2::model_benefit_tier_across` at the tip through the new
  `ConsensusApi::palw_model_benefit_tier_v1`: the ids summed each once, the most recent clock, the
  tier, the next rung, and the declaration in effect in the shape the line read carries (both reads
  now resolve §4.6 through one helper). The list is bounded by `PALW_MODEL_BENEFIT_MAX_PROOF_IDS`
  (16) before it is parsed. `misaka palw benefits --line [--key-file] [--holder …]` prints where a
  person stands and what the next rung needs — in positions, in MSK at the tip's price (the chain's
  own buy quote, bisected to the least buy that reaches it), and in tenure — and with `--nonce
  --daa` signs the §4.8 proof a gateway checks. Two constants give §4.8 one spelling:
  `PALW_MODEL_BENEFIT_CHALLENGE_MLDSA87_CONTEXT` (its own context, because the key that proves a
  membership is the key that signs a sell) and `palw_model_benefit_challenge_evm_digest_v1` (EIP-191
  `personal_sign` over the challenge's 64 bytes).
* **The reference gateway check** (`6b00692a`, `misaka-palw-gateway/src/membership.rs`), behind
  `--membership-line <line>` (which needs `--rpc`); without the flag nothing about the gateway
  changes. `GET /v1/membership/challenge` issues a single-use nonce bound to the tip's DAA (a bounded
  book, 300 s); the chat body carries `misaka_membership {nonce, daa, carrier[], evm[]}`; the nonce
  is consumed before any signature is checked, so a proof answers one attempt; the carrier lane's
  holder id is derived from the ML-DSA-87 key and its signature must cover the challenge built for
  that id; the EVM lane recovers `personal_sign` with alloy on k256; the tier is
  `getPalwModelBenefitTier` over exactly the proved ids. `PRIORITY_INFERENCE` buys the priority
  queue — two reserved in-flight places and the one job slot ahead of any waiting stranger — and
  every other grant is reported in `misaka.membership`, the operator's to honour. A proof that does
  not verify is a 403 naming why, never a job quietly served in the public queue. §4.8 says the
  gateway "reads the tier at `daa`": the fold keeps only the tip, so the gateway reads the tip,
  which is at or after the height the challenge named.
* **The site** (`6745584a`): with a wallet connected, the card on the store page and the line page
  says where that account stands — the chain's tier for its EVM holder id, the time since it last
  left, its rung marked, and what the next rung needs (memberships priced as the least join the
  curve fills, and tenure). A node from before the read is reported as not serving it, never as
  "not a member". Self-test 68/68.

Tests: consensus-core 10/10 in `palw_model_benefits_v1` plus
`the_tenure_clock_restarts_on_a_sale_and_a_person_is_counted_once` (N3 and N11 had no state-level
test until now); misaka-cli 3; the gateway 44 (nine for the membership check, among them a
personal_sign signature recovered to its account and to no other, the EVM digest checked against
alloy's `eip191_hash_message`, and a waiting member served before a waiting stranger). The
integration crate's `GetPalwModelBenefitTier` case (absent line, a repeated id echoed once, a
malformed id and an over-long list refused) type-checks; it was not run here, because it starts a
daemon.

**What the implementation found**, recorded so none of it becomes a surprise:

1. **`consensus_params_id` destructures `palw_model_benefits` and never hashes it** — the
   `unused variable` warning at `consensus/core/src/config/params.rs` since `14c453d1`. The
   handshake is unaffected by design (`consensus_identity_id` normalises a scheduled fence away,
   and the fork id and the schedule id both carry the 2,400 height through `for_each_fence`), but
   the fingerprint a node PRINTS at startup is the same with and without this fence: a seat on a
   build from before `14c453d1` and a seat on one after it both print testnet-11's pinned
   `060e3597…`, so the fingerprint an operator confirms during a rollout cannot tell them apart.
   The fix is one `h.write` beside `palw_model_lines`'s — and it moves testnet-11's fingerprint pin.
   **Not changed here: that is the operator's call**, and it is the same flag-day arithmetic §4.11's
   fence already carries.
2. **`model_position_across` summed a repeated id twice** — a caller naming one id three times held
   three times its units. Fixed: each id is counted once (the read is off the fold; no state
   moves). The RPC also sorts and deduplicates, and echoes the set it computed over.
3. **A position held from before the fence has no tenure clock.** The clock is written only on a
   balance change past the fence, and `model_position_tenure` reads a missing clock as zero — so a
   holder who joined between a line's opening (testnet-11: DAA 1,947) and 2,400 and never trades
   again never meets a `min_hold_daa` tier, however long they hold. The honest lower bound for such
   a holder is "since the fence": nothing past the fence changed their balance, or a clock would
   exist. Reading it that way is a read-side rule (the tier is not a fold input today) but it is
   still §4.5's meaning, so it is recorded here and not decided.
4. **§4.9's holder mark is not implemented** (§7 step 7). It is the one part of this ADR the fold
   would have to WRITE: the author's tier at the landing height, into evaluation and proposal rows
   that enter the state root. The rows cannot be extended in place on a live chain (ADR-0094's
   amendment), so it needs either a derivable encoding or a collection of its own that enters the
   root only when non-empty, and an activation of its own.
5. **The gateway was never secp-free.** `misaka-palw-derive` links `kaspa-evm`, and with it alloy and
   k256, so the EVM lane's recovery adds no curve to the binary; it names a coupling that existed.
   A first draft of this step put the EVM lane behind an opt-in feature on the premise that it
   would; `cargo tree` said otherwise once it was asked properly (a check piped through
   `2>/dev/null` had reported the absence of what it could not see).
6. **ADR-0096's branch rewrites the gateway's request surface** so that a field its table does not
   name is refused by name. `misaka_membership` must join that table when the two branches meet, or
   every membership proof becomes a 400.
