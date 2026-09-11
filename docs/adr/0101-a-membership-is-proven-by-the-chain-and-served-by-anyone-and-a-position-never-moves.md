# ADR-0101 — A membership is proven by the chain and served by anyone, and a Position never moves between holders

* Status: PROPOSED 2026-09-10. **Decisions 2, 3 and 5 IMPLEMENTED the same day, consensus-inert**
  (§9): which grant needs whom, the signed service descriptor and the check a client runs
  against chain facts, and three pins that a Position moves only between a holder and the
  curve. Decisions 4 and 7 are stated, not built. Nothing moves on any preset: no rule verifies a
  descriptor, no fold reads one, no fence exists.
* Builds on: [0095](0095-a-position-is-a-membership-not-an-income.md) (a Position is a
  membership: the line declares, the chain computes the tier, a gateway reads it; §8's "a bond
  behind the promise" rejected; the reference check of §7 step 6),
  [0087](0087-a-position-is-bought-from-the-curve-and-sold-back-to-it.md) (bought from the curve,
  sold back to it, no transfer), [0088](0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md)
  (a line's owner, developer and maintainer; anyone founds a competing line),
  [0089](0089-the-fold-is-the-truth-and-the-evm-is-its-window-and-its-hand.md) (the EVM is the
  fold's window and hand, never a second market), [0091](0091-the-reward-buys-the-pair-and-no-holder-is-paid.md)
  (use reaches the curve; no holder is paid), [0067](0067-classes-are-chain-data-kernels-are-the-build.md)
  Decision 6 (the chain carries kilobytes and no URL), [0100](0100-a-model-is-data-and-the-court-the-measure-and-the-licence-are-built-for-a-shard.md)
  (the boundary "permissionless" means, of which this is the membership half).
* Amends: nothing. Supersedes nothing. Records, as a decision, what the operator settled on
  2026-09-10: **a Position has no payment or settlement role, and therefore no transfer.**

## 0. The sentence this ADR is

**The line controls the PRODUCT, providers control the SERVING, and the chain proves the
MEMBERSHIP.** Anyone may serve a line's holders; a client checks a provider's signed statement
against what the chain says about the line — the grants it declares, the roots it names, the keys
its own bonds hold — and needs nobody's word; and a Position, bought from the curve and sold back
to it, never moves from one holder to another and never pays for anything, here or anywhere a
later decision adds.

## 1. What was read

### 1.1 The permissionless map, and its one gap

A review of `main` and testnet-11's fences (2026-09-10) mapped every path into the PALW economy.
Adopted, with the review's verdicts: a node syncs and verifies without a bond, production needs
an Active bond and nothing else (ADR-0061); a class registers with no allowlist, vote or identity
(ADR-0056); anyone certifies, and the court grades (ADR-0075); the chain draws the attempt
(ADR-0074); a bonded party challenges (ADR-0027) and accuses (ADR-0062, armed on testnet-11 at
DAA 1,900); anyone founds a line, posts a proposal or an evaluation, and a line's versions are
its developer's and its roles its owner's — ownership of a line is not permission over the chain
(ADR-0088); anyone seeds, anyone buys, a holder sells (ADR-0087, ADR-0094); a claim's Final
reward buys the line's pair with nobody's hand on it (ADR-0091); the tier is the chain's and the
early-access window is the fold's (ADR-0095).

Two corrections. **The reference gateway check exists** — `misaka-palw-gateway/src/membership.rs`
(`773f6e7c`) on `feat/adr-0095-membership-serving`, ADR-0095 §7 step 6 "done 2026-09-10" — and
is not on `main` (`a60f78e7` is not an ancestor of `main`'s tip), which is why a review of `main`
saw it missing. And **it is already provider-neutral**: its configuration is a line id and a
network domain, it reads the chain's tier, and it asks the line's owner nothing — any operator
can run it for any line today. What is missing is the rest of "anyone serves": a statement of
WHICH grant a stranger can serve, a provider's statement a client can CHECK, and a way to FIND
providers that does not put a URL on the chain.

### 1.2 A Position's movement, read off the fold

| path | what it does to a holding | where |
|---|---|---|
| buy | credits the buyer, from the curve | `model_buy_v1`, the ONE of two writers |
| sell | debits the seller, to the curve | `model_sell_v1`, the other |
| the reward's buyback (ADR-0091) | retires units into the market row's `retired_units`; credits no holder | `model_buyback_at_final` |
| a line's owner transfer (ADR-0088) | moves the LINE's ownership; writes no holding | `ModelLineOwnerTransferred` |
| the EVM facade (ADR-0089) | `transfer`, `transferFrom`, `approve`, `allowance` answer `NonTransferable()` | `kaspa-evm/src/model_market.rs` |
| the CLI | `palw model-buy`, `model-sell`, their EVM twins, `model-seed`; no transfer command | `misaka-cli` |

No object names two holders. That is the settled rule, and §5 pins it three ways.

## 2. The requirement

> **R-serve — anyone may serve a line's holders, and a client can check it.** Which grant a
> provider may serve is a function of the grant, not of anyone's approval; a provider's offer is
> a signed statement checked against the chain; the chain holds no provider and no URL; and no
> path of the serving side moves, escrows or is paid in a Position.

## 3. Decisions

**Decision 1 — the product, the serving and the membership are three parties' business.** The
line declares what each tier grants (ADR-0095 §4); the chain computes a holder's tier at the tip
and enforces what it can see (the early-access window); a provider serves. A membership whose only
server is the line's own machine dies with that machine; one any provider can serve does not.

**Decision 2 — which grant needs whom** (`palw_benefit_server_v1`, one spelling for every client
and provider):

| grant | needs | why |
|---|---|---|
| `HOLDER_VOICE` | the chain | the fold writes the mark (ADR-0095 §4.9) |
| `PRIORITY_INFERENCE`, `INFERENCE_QUOTA`, `EXPERIMENTAL`, `PRIVATE_BETA`, `EARLY_VERSION` | any provider holding bytes under a root the chain names for the line | the class's artifact or a version's; a client checks the root, and nobody's permission is involved |
| `DEVELOPER_ACCESS`, `SUPPORT` | the line's origin — a key of its owner, developer or maintainer bond | a relation with the line's own people, which a stranger's server cannot provide |

A bit this build does not know needs nobody it can name, and a descriptor offering it is refused.

**Decision 3 — the signed service descriptor, and the check a client runs.**
`PalwServiceDescriptorV1 { version, line_id, grants, roots, endpoints, valid_from_daa,
expires_daa, provider_pubkey, signature }`, its id keyed over the network domain and every field
but the signature, signed under `misaka-palw/service-descriptor/mldsa87/v1` — a context in no
committed set, on purpose: no consensus rule verifies it. `palw_service_descriptor_check_v1`
takes what the chain says about the line (`PalwLineServiceFactsV1`: the grants declared IN EFFECT
now — a lapsed promise offers nothing to serve — the class's and the versions' roots, the line's
bond keys, the tip's DAA) and refuses by name: another line, a window that does not hold, no
grant, an unknown bit, a grant the line does not declare, the chain's own grant, a served-by-root
grant with no root, a root the chain does not name for the line, an origin grant under a
stranger's key, a signature that does not verify. The verdict says `Origin` or `Open`.

**Decision 4 — discovery is not chain state.** A descriptor is transport-agnostic JSON: a
provider serves its own at a well-known path, anyone mirrors it, a directory collects them —
none of it the chain's (ADR-0067 Decision 6; ADR-0095 §8, "it is a URL, it rots"). A client
trusts no directory: every descriptor it shows is checked against the chain by Decision 3.

**Decision 5 — a Position is a membership and never money** (settled by the operator,
2026-09-10). A Position is bought from the curve and sold back to it; it does not move from one
holder to another; it is never a means of payment or settlement — on the carrier lane, in the
EVM facade, and in any serving layer this ADR admits. A provider READS a holding through the
proof the holder signs (ADR-0095 §4.8's challenge) and receives nothing from it; a descriptor has
no price in Positions, no escrow, no holder to credit; no benefit may be priced, paid or
collateralised in Positions. The three pins of §5 fail on a third writer of a holding, on an
object that moves a Position between holders, and on a facade that answers `transfer`. A proposal
to the contrary is a new ADR that supersedes this decision by name, never an edit to a pin.

**Decision 6 — serving stays unadjudicated.** The chain cannot see a download, a queue position
or an answered ticket, so it does not judge them: a provider that does not serve loses its
clients, never a bond (ADR-0095 §8's "a bond behind the promise" stays rejected), and a line that
wants a grant no longer offered stops declaring it — which makes every descriptor offering it
fail Decision 3's check at once. There is no list of providers for a line to curate or revoke.

**Decision 7 — order of work.** 1. `feat/adr-0095-membership-serving` merges (the reference
check). 2. The gateway serves its own signed descriptor at `/.well-known/misaka-service.json`
beside the challenge it already issues. 3. The site renders, per line, the descriptors it has
checked, with the verdict. 4. A directory, of any transport. 5. ADR-0095 §7 step 7 (the holder
mark) at its own activation.

## 4. What this costs

Nothing on chain. One module of pure functions and types in `consensus-core`; three pins.

## 5. Invariants the tests hold

```
1  Every known grant names who serves it; an unknown bit names nobody.
2  A stranger serves by root, the origin serves its own, and every refusal of Decision 3 is by
   name.
3  A real ML-DSA-87 signature verifies; a field changed after signing, or another network, does
   not; a stranger's key cannot claim an origin grant, signed or not.
4  A descriptor survives its JSON transport with its id.
5  A holding has exactly two writers — the buy and the sell — and no object moves a Position
   between holders (the one "Transferred" object moves a line's ownership).
6  The EVM facade answers transfer, transferFrom, approve and allowance with NonTransferable(),
   whatever the arguments.
```

1–2 are `palw_service_descriptor_v1`'s; 3 `misaka-palw-sdk/tests/service_descriptor_signed.rs`;
4–5 `consensus/core/tests/palw_adr0101_membership.rs`; 6 `kaspa-evm`'s
`a_position_has_no_transfer_door_on_the_evm`.

## 6. Order of work

Decisions 2, 3 and 5's pins — **done** (§9); the rest is Decision 7's list.

## 7. Supersession

| what | this ADR |
|---|---|
| ADR-0095 §7 step 6 (a reference gateway check) | kept; it is the membership half a provider runs, and it is provider-neutral already |
| ADR-0095 §8 (no chain URL; no bond behind the promise) | kept; Decisions 4 and 6 |
| ADR-0087 (no transfer) / ADR-0089 (the facade refuses `transfer`) | kept; named as Decision 5 and pinned |
| ADR-0100 Decision 6 (the permissionless boundary) | this is its membership half |

## 8. What is deliberately not decided

* **The directory's transport** (Decision 4).
* **The units of `INFERENCE_QUOTA`** — ADR-0095 §8: a serving decision is not a consensus one.
* **Whether an origin may delegate its grants to a named provider.** A line that wants that
  declares the provider's key as a maintainer (ADR-0088 roles) — the mechanism exists; a
  dedicated delegation object would be a new ADR.

## 9. Number hygiene and implementation record

0101 is the next free number after ADR-0100. Claimed on `feat/adr-0099-sharded-seat` on
2026-09-10. **The next free number is 0102.**

* **2026-09-10** — written and implemented the same day:
  * `consensus/core/src/palw_service_descriptor_v1.rs` — Decisions 2 and 3, with their tests;
    registered in `lib.rs`, its domains in `palw_derived_v1`'s uniqueness sweep.
  * `consensus/core/tests/palw_adr0101_membership.rs` — Invariants 4 and 5.
  * `misaka-palw-sdk/tests/service_descriptor_signed.rs` — Invariant 3, with real keys.
  * `kaspa-evm/src/model_market.rs` — Invariant 6.
