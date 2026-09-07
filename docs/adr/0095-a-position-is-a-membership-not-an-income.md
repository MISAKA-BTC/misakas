# ADR-0095 — A position is a membership, not an income

* Status: PROPOSED 2026-09-07
* Amends: [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) Decision 1's
  "grants nothing but the right to sell it back" — it now also grants what its LINE declares;
  [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) Decision 2 (a preview
  gets a stated audience) and Decision 12 (one more row to read).
* Builds on: 0087 (no transfer, the curve), 0088 (lines, versions, previews, evaluations, roles),
  0090 (the seeded pair), 0091 (the reward buys the pair — income was already refused).
* Supersedes nothing.

## 0. The sentence this ADR is

A position buys no income and no vote; it buys **what the line's developer promises its holders** —
a new version before everyone else, the private beta, the front of the inference queue, the
experimental mode — and the chain's job is to make the holding provable, the promise readable, and
the broken promise visible, while the serving itself stays where serving belongs.

## 1. What the operator asked, in the operator's words

> Model Position を「投機対象」から「AIサービスの会員権・アクセス権」に近づけられます。単なる
> 「保有していると報酬が貰える」ではなく、**Position を持っていることでモデル利用上の明確な優位性
> が得られる**ようにする。
>
> Early Access（新 Version を先行利用）／Private Beta（一般公開前のモデルを holder に）／
> Priority Inference（高負荷モデルの優先キュー）／新機能の先行提供（長 Context、Thinking mode、
> Tool use、新しい量子化版）。Tier は保有量で分けるが、**保有量に比例して利益を受け取るのではなく、
> あくまでサービス上の権利にする**。
>
> MISAKA が全部の特典を決める必要はない。**Developer が、この Model Line の Position Holder には
> 何を提供するかを設定できる**ようにする。
>
> 重要なのは、**MISAKA プロトコルが価格を支えないこと。**
>
> Model Position を「モデルの利益を受け取る権利」にはしない。代わりに「特定 Model Line のプレミアム
> 利用権」として設計する。

## 2. Why this is the right direction, and why it was already half-decided

[ADR-0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) removed the last thing that
looked like income: five percent of a model's mining reward buys from its pair and the chain
retires what it buys, so **no holder is ever handed anything**. What was left was a position that
grants nothing at all — "the right to sell it back", in ADR-0087's words. A thing that grants
nothing and can only be sold is the definition of a purely speculative object, and the operator is
right that this is the wrong shape for a network whose point is models people use.

Three properties this market already has make a membership the natural next thing, and they are
not properties a token normally has:

* **A position cannot be transferred** (ADR-0087 Decision 5). A membership that cannot be
  transferred cannot be scalped or lent; the only way in is to pay the curve, which RAISES the
  price for the next entrant, and the only way out is to sell back, which lowers it. The
  membership's price is the curve's price, and both move with how many people want in.
* **The supply is fixed and whole** (ADR-0090): five hundred thousand memberships a line, forever,
  no dilution, and a holder's stake is a count anyone can read.
* **Usage already buys the pair** (ADR-0091). A model that is actually mined buys its own
  memberships. So service demand and production both push the same number, and neither is the
  protocol propping the price up — the operator's constraint that **MISAKA must not support the
  price** is kept: nothing here promises a price, and nothing here pays a holder.

## 3. The boundary this ADR must not blur

**The chain can prove a holding. It cannot serve an inference.** Everything below is built on that
line, and stating it precisely is most of the design:

| the chain does | the chain does NOT |
|---|---|
| hold, per line and per holder, a count of positions at a height | run a gateway, a queue, or a download |
| carry the developer's DECLARATION of what holders get | verify the declaration was honoured |
| publish every version's root, preview or current, so what was served can be checked | keep a preview secret — a root is public the moment it is published |
| record an evaluation from anyone, holders included | judge it |

So a "membership" here is a **credential the holder can prove and a promise the line has published**.
The enforcement is the developer's own serving stack; the accountability is that a promise on chain
is a promise a holder can quote, and the punishment for breaking it is holders selling — the same
curve, in the other direction. This ADR does not pretend otherwise, and no clause below asks the
chain to do something it cannot.

One consequence worth stating early, because it is the most common misreading: **early access is not
secrecy.** A published version's root is on chain for everyone. What a holder gets first is the
**artifact** — the weights, one to thirty-four gigabytes of them, which have never been on chain and
are the developer's to distribute — and a place in whatever queue the developer runs. The root is
what lets a holder check that the bytes they were given are the bytes the line published.

## 4. Decisions

**Decision 1 — a line declares what its positions grant, and the declaration is on chain.** A new
line-level object `ModelLineBenefitsDeclared { line_id, tiers, note, signature }`, signed by the
line's **owner** (the cold role, ADR-0088 Decision 6 — not the developer, because this is what the
line promises rather than what it ships). `tiers` is an ordered list of at most eight
`{ min_units: u64, grants: u32 (a bitset), lead_daa: u64, note: [u8; ≤64] }`, strictly increasing in
`min_units`. It replaces the line's previous declaration wholly; an empty list withdraws it.
Priced like every other registry object (rent, ADR-0088 Decision 11).

**Decision 2 — the grants are a small closed set, and the set is the chain's, not a line's.** A
bitset the whole network reads one way, so a wallet can render any line's card without knowing the
line:

| bit | grant | what it means |
|---|---|---|
| 0 | `EARLY_VERSION` | the artifact of a new version, `lead_daa` before the line makes it current |
| 1 | `PRIVATE_BETA` | versions published as previews are served to holders and to nobody else |
| 2 | `PRIORITY_INFERENCE` | the line's gateways serve holders' jobs ahead of others' |
| 3 | `EXPERIMENTAL` | modes the line runs but has not made default: longer context, a thinking mode, tools, a new quantisation |
| 4 | `DEVELOPER_ACCESS` | the line's own channel — proposals, research previews, the room where the next version is argued about |

A line may declare any subset. What a line may NOT declare is anything that pays: there is no bit
for a share, a discount denominated in MSK, a rebate, or a claim on the reserve, and adding one is
a change to this ADR rather than a value a `tiers` row can carry. **A grant is a service or it is
not a grant.**

**Decision 3 — the tier a holder is in is a pure function of chain state, and both namespaces
count.** `palw_model_benefit_tier_v1(state, line, holder, daa) -> Option<TierIndex>`: the highest
tier whose `min_units` the holder's balance meets. A holder's balance is the sum of what they hold
on the carrier lane and on the EVM lane (ADR-0089 keeps these in separate namespaces; a person is
not two people). The function is in `kaspa-consensus-core` so a gateway, a wallet and the explorer
compute the same answer, and it takes the DAA it is asked about so "were they a holder when they
asked?" has one meaning.

**Decision 4 — the lead is measured in DAA, and the version row already carries the clock.** For
`EARLY_VERSION`, a developer publishes the version as a **preview** (ADR-0088 Decision 2), serves
its artifact to the tier, and promotes it to current no earlier than `published_daa + lead_daa`.
The chain does not enforce the wait — it cannot make a developer wait — but it RECORDS both
heights, so "was the lead honoured?" is arithmetic on two numbers anyone can read, and a line that
promised 24 hours and gave 20 minutes is caught by subtraction rather than by argument.

**Decision 5 — a holder proves the holding without a transaction.** A gateway needs "this caller
holds ≥ N of line L", and a chain query answers it, but the caller must show the gateway they own
the address. `palw_model_benefit_challenge_v1(line, holder, nonce, daa)` is a message the holder
signs with the key their position is held under (the ML-DSA-87 payout key on the carrier lane, the
secp256k1 account on the EVM lane), and the gateway verifies the signature and then reads the
balance at `daa`. Nothing is submitted, nothing is spent, and a holder's proof is not a transaction
the network has to carry.

**Decision 6 — a broken promise is recorded where promises are already judged.** ADR-0088's
`ModelEvaluationPosted` takes an evaluation from anyone about a version. A holder who was promised
early access and did not get it posts one, and it stands beside the line's usage and its versions
in the explorer. There is no slashing: nothing is staked against a benefit, and inventing a stake
would put the chain in the position of judging whether a download happened, which it cannot see.
The market is the enforcement, and it is a real one — a membership nobody wants is sold back, and
the sell is the price falling.

**Decision 7 — what a participant reads.** `getPalwModelLine` gains `benefits` (the tiers, the
grants, the notes); a new `getPalwModelBenefitTier(line, holder)` answers the question a gateway
asks. `misaka palw line-benefits --line --tier …` declares them; `misaka palw benefits --line
[--key-file]` reads them and says which tier the key is in. The site shows the card on the line and
on the trade page — what this position gets you, and what you would need to hold for the next tier
— because the reason to buy should be legible before the buy, not after.

**Decision 8 — no fence of its own.** Under `palw_model_lines`, armed at DAA 1,900 on testnet-11
and `None` on every other preset. The declaration is inert where the registry is.

## 5. Security — the four principles, checked

* **Nothing is minted, nothing is paid.** No decision here moves a sompi. The grants are services;
  the closed set in Decision 2 is what keeps a future "just a small rebate" from arriving as a
  `tiers` field.
* **The credential cannot be forged.** A tier is a function of the state and a signature over a
  nonce; a gateway that checks both is checking the chain, not a claim.
* **The credential cannot be lent.** Positions do not transfer (ADR-0087 Decision 5), so a
  membership cannot be rented out — a holder can only proxy for someone else, which costs them
  their own place in the queue and is a thing no rule can or should prevent.
* **A declaration is a statement, not a lie the chain endorses.** The chain says "the line
  declared this", never "the line did this". Decision 4's two heights and Decision 6's evaluations
  are how a reader tells the difference.

Attacks considered:

| | threat | why it is not one |
|---|---|---|
| A1 | a developer declares rich benefits to pump the price, then serves nobody | the declaration is dated and the promotion heights are on chain; holders sell, and the price they sell into is the one the declaration raised |
| A2 | a whale buys the top tier and resells access | they cannot transfer the position; proxying costs them their own queue slot and gains them a customer who could have bought in at the curve |
| A3 | a holder sells the moment they have the early artifact | they got one version early and gave up the next one; the artifact they hold is the root the chain published, so nothing about it is secret to steal |
| A4 | a gateway is asked to trust a stale balance | the challenge names a DAA and the gateway reads the balance at it; a holder who sold cannot answer for a later height |
| A5 | tiers are used to sell weight, votes or fee shares | the grant set is closed, and none of those is in it |

## 6. Invariants the tests must hold

* **N1 (no payment).** No path added by this ADR writes a payout, a position or a reserve. The
  declaration's only effect on state is its own row.
* **N2 (the tier is the balance).** `benefit_tier` equals the highest tier whose `min_units` the
  holder's carrier + EVM balance meets, at the DAA asked, and is `None` below the first tier.
* **N3 (both lanes, one person).** A holder with 60 units on the carrier lane and 60 on the EVM
  lane is in the 100-unit tier.
* **N4 (the owner declares).** A declaration signed by the developer or the maintainer is refused;
  only the owner's signature stands (ADR-0088 Decision 6's cold/hot split).
* **N5 (the set is closed).** A declaration carrying an unknown grant bit is refused at the fold,
  not stored and ignored — an unknown bit is a promise no reader can render.
* **N6 (ordering).** Tiers strictly increase in `min_units`, at most eight, or the object is
  refused.
* **N7 (the lead is readable).** For a version published as a preview and later promoted, the
  explorer computes `promoted_daa − published_daa` and compares it to the tier's `lead_daa`.

## 7. Order of work

1. This text; the README row; the banner on ADR-0087.
2. The row, the grant set, `palw_model_benefit_tier_v1`, the challenge message, and their goldens.
3. The object, its signature, the fold arm and the refusals (N4–N6).
4. RPC (`benefits` on the line, `getPalwModelBenefitTier`), CLI (`line-benefits`, `benefits`).
5. The site: the benefits card on the line and the trade page, and the next tier's distance.
6. A reference gateway check — the smallest honest one: verify the signature, read the tier, and
   choose a queue — so the serving side has something to copy rather than invent.

## 8. What is deliberately not decided

* **Quotas denominated in requests.** "1,000 requests/day for holders" is the first grant that
  starts to look like an income, and the operator said so themselves; it is left out until the
  simple grants have been run.
* Whether a line may declare benefits before its class is `Active`. Probably yes, but nothing
  turns on it until a gateway exists to honour them.
* Whether the chain should carry a gateway endpoint for a line. It is a URL, it rots, and the
  registry is not a directory.
* Any protocol-level enforcement of a promise. The chain cannot see a download; pretending it can
  is the one mistake this design must not make.

## 9. Number hygiene

0095 was the README's next free number on 2026-09-07 (0094 is this branch's). The next is 0096.
