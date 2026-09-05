# ADR-0087 — a position is bought from the curve and sold back to it

**Status:** PROPOSED 2026-09-05, design only (no implementation yet). Requested by the operator
on 2026-09-05: "Model Positions" — a per-model, fixed-supply position whose price is set by
market participants' beliefs about the model's future (its usage, its PALW work, its evaluation,
its migration to new versions, the scarcity of its capacity), traded against MSK on an AMM, with
1 % of every trade to the model's registrant and 5 % of every trade burned, and with **no
transfer between holders**, so that a position is never something one person hands another.
The operator's word is *position* (ポジション), not *share*: it is bought from the protocol's
curve and sold back to it, and that is the whole of what it is.

## 1. What exists, and what a market can therefore see

* **A model is a class.** `PalwClassRowV2 { class_id, status, share_permille, budget_blocks,
  canonical_leaves, is_base_class }` (palw_state_v2.rs); a post-genesis class carries
  `registrant_bond: Option<PalwBondKeyV2>`, `None` exactly for the classes the assembly
  registered at genesis (ADR-0056). A bond's `payout_payload: Hash64` is where consensus pays
  its holder. The registrant is therefore an on-chain identity with a pay address — the
  "model's adder" this ADR pays.
* **Consensus already pays and burns.** A claim's `escrowed_reward` becomes
  `PalwPayoutV2 { payload, amount }` in `pending_payouts`, honoured by the coinbase; bonds lock
  a collateral outpoint by rule (`palw_bond_collateral_is_locked_v2`) and a slashed bond has a
  burn obligation (`palw_bond_burn_obligation_v2`). "Value held by the protocol and paid out by
  the fold" is a shape the chain has, on the UTXO model it has, without covenants.
* **Objects are how the chain is told things.** `PalwConsensusObjectV2` variants ride carrier
  transactions (`misaka-cli palw submit-object`, chunk groups under the 81,920-byte carrier —
  ADR-0080), are applied in the state fold (`apply_object`), and every new rule is armed by an
  `Option<ForkActivation>` on the params (twelve exist; `palw_rc_arm_phase1` sets the shipped
  ones). The state is at `PALW_STATE_V2_VERSION = 20`.
* **The premine is 10 B and the rule is carve-not-mint** (ADR-0059): nothing here may mint.
  Everything a position pays out must have been paid in, and a burn is the only way supply moves.
* **Prior art in this repository:** the Token Program of 2026-08-10/11 (an SPL-style,
  consensus-store account ledger with transfer and burn, on the retired VLT lineage) — an
  account ledger inside consensus is a shape that was built once and reviewed. The EVM lane
  (ADR-0020) is an optional, non-default feature and is not the substrate here.
* **What the chain knows about a model, per class:** claims accepted on each lane, receipts
  licensed, the work priced on the free-prompt lane (`work_leaves`, ADR-0083), the budget granted
  and used per epoch (`budget_blocks`, `share_permille`), certification (ADR-0075), the registrant,
  the class's status. **What it does not know:** any human preference score, any "inference per
  day" other than the count of claims — those are the explorer's derivations (misakascan) and
  never a consensus fact. A market prices from the first list and from whatever its participants
  believe; this ADR gives it the first list and nothing else.

## 2. The requirement

A per-class position with a fixed supply, bought from a protocol-owned curve in MSK and sold
back to it, never moved between holders; every trade burns 5 % of its MSK leg and pays 1 % to
the class's registrant; the whole is supply-neutral except for the burn; every balance and every
price is a function of the chain alone. A position grants nothing but the right to sell it back:
no weight, no vote, no seat, no fee discount, no bond — so its price is exactly the market's
belief about the model and nothing the protocol adds.

## 3. Decisions

**Decision 1 — a position is a balance in the state fold, not a coin and not a UTXO.**
Per class: `PalwModelMarketV1 { class_id, opened_daa, msk_reserve, position_units, sold_units,
burned_sompi, registrant_paid_sompi, closed_to_buys }`. Per holder: `PalwModelPositionV1 {
class_id, holder: Hash64 (the holder's payout payload — the same identity a bond pays),
units: u64 }`. One position is `10^6` units; every class opens with
`PALW_MODEL_POSITION_SUPPLY_V1 = 100_000` positions, a network constant so that no model is
issued more room than another — the operator's example, and the number this ADR carries into
the tests. The state root covers markets and positions (state v21).

**Decision 2 — the curve is constant-product over the reserve plus a virtual reserve, and the
curve is the only counterparty.** A market opens with the whole supply in the curve and no MSK:
`(msk_reserve + V) × position_units = K`, `K` fixed at opening as `V × supply`, where
`PALW_MODEL_MARKET_VIRTUAL_SOMPI_V1 = V` is a network constant that sets the first position's
price (`V / supply`) and the curve's steepness. No liquidity provider, no pool token, no pair
other than MSK↔class, no market a user can create: the market of a class is opened by the fold
when the class is registered (post-genesis) or when this rule activates (genesis classes).
The price at any moment is `(msk_reserve + V) / position_units`; there is no other price.

**Decision 3 — two moves, and only two.** `PalwModelBuyV1 { class_id, holder, msk_in,
min_units_out }` and `PalwModelSellV1 { class_id, holder, units_in, min_msk_out }`, both
`PalwConsensusObjectV2` variants in a carrier transaction. A buy's carrier pays `msk_in` to the
class market's sink — a consensus-recognised, provably unspendable output the fold credits to
`msk_reserve`; the fold computes `units_out` from the curve over the NET leg and credits the
holder, refusing the object when `units_out < min_units_out`. A sell is signed by the holder's
key; the fold debits `units_in`, computes the gross MSK leg from the curve, and writes a
`PalwPayoutV2 { payload: holder, amount: net }` the coinbase honours, refusing the object when
`net < min_msk_out`. The reserve never sits in a spendable output: it is an accounting entry
funded by sinks and drained by coinbase payouts, exactly as escrowed rewards are today.

**Decision 4 — the fee is on the MSK leg of every move, split three ways, and the split is
the operator's.** Of a gross MSK leg `m`: `burn = 5 % of m`, never paid to anyone and subtracted
from supply; `registrant = 1 % of m`, a `PalwPayoutV2` to the class's `registrant_bond`'s
`payout_payload`, or burned as well when the class has no registrant (a genesis class); the
remaining `94 %` is the net leg — on a buy it enters the reserve, on a sell it is paid out.
A round trip therefore costs 12 % plus the curve's own slippage; the record below carries the
arithmetic so nobody discovers it from a wallet.

**Decision 5 — no transfer exists.** There is no object that moves units from one holder to
another, and none that lends, locks, wraps, delegates or pledges them: a position is not a bond,
not collateral, not a fee, not a seat, not weight. The only way a position changes hands is
through the curve, and then it is not the same position but a new balance bought at the
curve's price. This is the design's answer to the operator's constraint that a position must
not be something exchanged between persons; whether the design meets a legal definition of an
exchange business is a question for counsel, and this ADR records the intent, not the verdict.

**Decision 6 — the market is a consensus rule, armed by activation, never by regenesis.**
`palw_model_market: Option<ForkActivation>` on the params; below the activation the objects are
refused and no market exists; the fingerprint moves only where the flag is set. Consensus
changes go by activation on this chain (the standing rule); this one is no exception.

**Decision 7 — a new version is a new class with a new market; nothing migrates by itself.**
Positions in class A are positions in class A. When a class leaves `Active` (retired,
superseded, frozen) its market `closed_to_buys` becomes true; sells continue at the curve's
price until the reserve is drained; nothing is redistributed, minted or moved to the successor.
A registrant who wants holders to follow a version sells the story, not a migration.

**Decision 8 — what a participant reads.** RPC `getPalwModelMarket(class_id)` (reserve, units,
price, supply sold, burned, paid, status), `getPalwModelPositions(holder)`; per-class chain
counters as they already exist in the class facts; the explorer's Model Market page derives
everything else. CLI: `misaka palw model buy|sell|show`.

## 4. What this costs, stated before it is measured

* **Trading:** 6 % of each leg leaves the trade (5 % burned, 1 % to the registrant), so a round
  trip is 12 % before slippage. Worked with `V = 1,000 MSK`, supply `100,000`
  (`K = 10^8 MSK·positions`), from an empty market:

  | move | gross MSK | burn (5 %) | registrant (1 %) | net (94 %) | positions out / in | price after (MSK) |
  |---|---|---|---|---|---|---|
  | buy | 1,000 | 50 | 10 | 940 → reserve | 48,453 out | 0.0376 |
  | buy | 1,000 | 50 | 10 | 940 → reserve | 16,824 out | 0.0576 |
  | sell all 65,277 | 1,880 from reserve | 94 | 18.8 | 1,767.2 paid | 65,277 in | 0.0100 |

  (Corrected with the implementation, §7: the design draft's rows read 12,846 out, 3,014 from the
  reserve and 2,833 paid, which the curve cannot produce — a reserve of 1,880 pays at most 1,880.)
  Two buys of 1,000 MSK and one sell of everything bought return 1,767.2 MSK of the 2,000 paid in:
  when every unit comes back the curve's `x` returns to `V`, so the gross leg is exactly the
  reserve — there is no last-out loss beyond the fees, and no first-in gain beyond the curve. A
  partial sell is paid on the curve's slope at that point, which is the only place slippage lives.
* **State:** one market row per class, one position row per (class, holder); ≈ 150 bytes each.
* **Fold:** O(1) per move; a carrier per move (≈ 300 bytes plus the sink output).
* **Consensus:** an activation; a state version; two objects; one sink script; one payout kind.

## 5. Invariants the tests must hold

* **M1 (supply).** For every class, `position_units + Σ holders' units = supply`, always.
* **M2 (value).** For every class, `Σ msk_in = msk_reserve + Σ payouts + burned + registrant_paid`,
  always; nothing is minted.
* **M3 (no transfer).** The object set has no variant whose effect is a change of two holders'
  units in one class; a property test over every object kind.
* **M4 (the curve).** The price is `(msk_reserve + V) / position_units`; buying raises it,
  selling lowers it; a buy and an immediate sell of what it bought returns `0.94² ×` the gross
  less slippage, and the fee arithmetic is fixed in a golden table.
* **M5 (protection).** `min_units_out` and `min_msk_out` refuse, never partially fill.
* **M6 (lifecycle).** A class that leaves Active refuses buys and honours sells until the
  reserve is empty; a genesis class's registrant fee is burned.
* **M7 (determinism).** The fold over a recorded sequence of moves reaches one state root on
  every node; the fingerprint is unchanged where the flag is `None`.
* **M8 (the address).** A holder is its payout payload; a sell is signed by the key that
  payload names; no other key can sell it.

## 6. Order of work

1. State: markets, positions, v21, the two objects, the sink script, the payout kind, M1–M8.
2. Params: the flag; the fingerprint pin test for `None`.
3. RPC, CLI, the explorer page.
4. A devnet drill: register a class, buy, sell, retire, drain; then testnet-11 by activation.

## 7. Implementation record (2026-09-05, `palw-adr0084-served-answer`)

**Landed — §6 items 1–3 (state, params, RPC, CLI); the explorer page and the devnet drill are
not.**

| where | what |
|---|---|
| `consensus/core/src/palw_model_market_v1.rs` | The constants (`10^6` units a position, `100,000` positions a class, `V = 1,000 MSK`, 50 ‰ burn, 10 ‰ registrant), `PalwModelMarketV1`, the fee split (exact, remainder on the net leg), `palw_model_buy_quote_v1` / `palw_model_sell_quote_v1` over `(x + V) × u = K` with the rounding that keeps the product at or above `K` and the sell's gross capped by the reserve, the sink script `OP_RETURN "MSKMDL01" <class id>` and its reader, the holder as `BLAKE2b-512(pubkey)` — the same identity a bond pays — and the sell's signed message under its own ML-DSA-87 context. |
| `palw_state_v2.rs` | `model_markets` and `model_positions` on the state; **in the state root only when non-empty** — the root is committed in headers, so a chain on which the rule is dormant commits the roots a build without the fields computes; the carriage (persisted and served over IBD) encodes the two collections as a tagged tail only once a move was folded, so the legacy layout is byte-identical until then (`PalwStateCarriageV2Legacy` pins it); two delta entries with replay and revert; `ModelBuy { class_id, holder, msk_in, min_units_out, sink_index }` and `ModelSell { class_id, holder, units_in, min_msk_out, pubkey, signature }` appended to the object enum; the two fold arms (a buy opens the market lazily, requires `Active`, refuses under the floor; a sell debits, pays the net leg and the registrant's through `pending_payouts` keyed by the move; a class with no registrant burns that leg); eight error kinds. |
| `palw_lifecycle_objects_v2.rs` | A buy rides only when its carrier's output `sink_index` holds exactly `msk_in` under the class's sink script; a sell rides only signed. |
| `config/params.rs` | `palw_model_market: Option<ForkActivation>` with the `palw_da_court` contract; `palw_model_market_fence` / `palw_model_market_active_at`. |
| `virtual_processor/processor.rs` | Both objects refused by name below the fence at the block's DAA; a sell's signature verified at acceptance against the payload its key derives to (M8). |
| `mining/.../check_transaction_standard.rs` | The sink is the one unspendable output that is not dust — recognised by its exact script. |
| RPC | `getPalwModelMarket(classId)` (an unregistered class is `found: false`; an unopened market reads as the whole supply in the curve) and `getPalwModelPositions(holder)`, through core, service, gRPC (proto ids 1144–1147) and wRPC. |
| CLI | `misaka palw model-show [--quote-msk]`, `model-positions`, `model-buy --class --msk --min-positions --key --yes`, `model-sell --class --positions --min-msk --key --yes`; every move prints the quote the chain's own arithmetic gives against the tip and sends nothing without `--yes`. |

**Tests.** `palw_model_market_v1` (6): the corrected §4 table, the product never below `K` at
every size tried, a round trip returns at most `0.94²` of the gross, a closed market refuses buys
and honours sells, the sink names its class and nothing else does, the sell message binds its
fields. `palw_state_v2::tests::model_market` (5): M1 and M2 at every stop of a two-buyer run; M5's
floors refuse whole (`ModelBuyBelowFloor`, `ModelSellBelowFloor`, `ModelSellExceedsPosition`) and
fill at the floor exactly; M3 as "one move changes one holder's row"; M6 as a not-yet-Active
class refusing buys, a registrant paid 1 % to its payload, a registrant-less class burning it;
M7 as the deltas replaying and reverting to the same root and the carriage staying legacy until
the first move. The params test pins M7's fingerprint half. M4's monotonicity is asserted in both
suites; M8's signature check is at acceptance and is covered by reading, not by a test — a
processor-level fixture for a signed lifecycle object does not exist yet.

**What the implementation taught.** (1) The design draft's worked table was wrong twice (above);
the curve's arithmetic is now the test's golden and the table follows it. (2) "State v21" cannot
be a root-version bump: the state root is in every header, so the version and the collections
enter the root only where a move has been folded, and the carriage grows a tail rather than a
field. (3) "The market opens when the class is registered" became "on its first buy": the same
observable market at a fraction of the fold, and no edge at activation for genesis classes.
(4) The mempool's dust rule treats every unspendable output as dust; the sink needs the one
exception, matched on its exact script.

**Not landed.** The explorer's Model Market page (another repository); the devnet drill (§6 item
4) — buy, sell, retire, drain on a chain with the fence armed; arming on testnet-11.

## 8. What is deliberately not decided

* The virtual reserve `V` and the supply constant: numbers the operator sets when the flag is
  armed; the tests carry the operator's example.
* Whether a registrant may seed its market with MSK at opening (a curve with a non-zero
  starting reserve); not needed for the curve to work.
* Any on-chain human-preference or usage metric beyond what the fold already counts.
* Any incentive to migrate holders to a successor class.
* Whether burned MSK is reported against the premine cap in the explorer (the cap binds
  genesis; burns only lower supply).

## 9. Number hygiene

This is ADR-0087. The README's next free number was 0087 after 0086's row; it becomes 0088 with
this row.
