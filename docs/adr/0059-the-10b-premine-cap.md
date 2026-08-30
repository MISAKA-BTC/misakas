# ADR-0059: The 10B premine cap — genesis mints one number, everything else is a carve

- Status: Accepted
- Date: 2026-08-30
- Depends on: ADR-0042 (the RC atomic bundle and its genesis registry), audit C-08 (a bond's
  collateral is money the genesis UTXO set really holds)
- Supersedes: the per-network premine split of 2026-08-26 (mainnet 13B / re-minted test nets
  10B) and the 2026-06-17 vault layout (40 × 0.1B custody UTXOs + one main wallet)

## The operator's decision, verbatim

Recorded 2026-08-30, and this ADR is its normative form:

1. **Every network's genesis premine is exactly 10,000,000,000 MSK.** Mainnet 10B, every
   testnet 10B. The constant is `config::premine::MISAKA_PREMINE_CAP_SOMPI`, and it is one
   number for every network by design — the 2026-08-26 split (13B mainnet / 10B testnets)
   existed only because mainnet's identity was not supposed to move; the operator has now
   moved it, and the split is gone.
2. **The vault block is deleted.** The 40 mainnet-custody vault UTXOs (4B MSK, indices 0–39)
   are removed from every network's genesis. A genesis is one main-wallet UTXO, plus — on the
   network that runs a `ConsensusV2` registry — the carve-outs below.
3. **Community allocations are carved out of the main wallet, never minted beside it.** Adding
   an entrant to `TESTNET11_COMMUNITY_ALLOCATIONS` reduces the main wallet by the same amount;
   the genesis total does not move. The same rule binds every other genesis extra: bond
   collateral and fee floats are carves too.
4. **The genesis bonds bond the main wallet.** Each genesis bond's collateral output (0.1B,
   at the bond's own outpoint index `0..cards`) is owned by the main wallet's key — the
   operator stakes the main wallet's money. There is no separate custody block to stake from.

## Why the defect this closes was invisible

The 2026-08-26 "10B" decision reduced the test networks' main wallet to 6B — and testnet-11
still minted 13.547B. The float-carve function inserted the main wallet a second time at
`9B − floats`, and `UtxoCollection` is a `HashMap`, so the second insert silently replaced the
first. Worse, the tests PINNED the wrong total ("the RC's premine is the 13B split plus the
community allocation"), so the suite was green while the decision was not implemented. Two
lessons are structural now:

* **One construction path.** `testnet11_genesis_utxos` builds the whole set in one pass; the
  main wallet is written exactly once, as `cap − Σcarves`.
* **The cap is enforced arithmetically, not by review.** The builder computes the main wallet
  with `checked_sub` against the cap — a carve list that outgrows 10B fails the build loudly —
  and `every_network_genesis_mints_exactly_the_10b_cap` asserts the sum on every network, so
  a future "grow the table" change cannot pass without the main wallet paying for it.

## What deliberately does not move

* **Outpoint indices.** Collateral at `0..cards`, the main wallet at index 40
  (`MAIN_PREMINE_INDEX`), floats at `41..41+cards`, the community set on its own sentinel
  txid. A bond's identity IS its collateral outpoint (`PalwBondKeyV2(premine_outpoint(i))`,
  inside `palw_ruleset_id`), and fleet units name float outpoints in their configs — indices
  are names, not positions, and the gap at 6..39 is deliberate.
* **The collateral value.** 0.1B per seat at this ADR's cut — *superseded the same day by
  ADR-0061*, which re-sizes the outputs to 10,000 MSK per seat (the declared collateral stays
  the derived figure, so `palw_ruleset_id` still does not move).
* **The emission schedule.** 15B over 20 years, untouched. Final supply follows the cap:
  10B + 15B = **25B** (`MAX_SOMPI` 28B → 25B, the same follow-the-premine move as the
  30B → 28B re-derivation of 2026-06-17).
* **The community table.** All 11 entrants, 547M MSK, same addresses, same order — now paid
  for by the main wallet (t11 main = 10B − 0.6B collateral − 600 floats − 547M community
  = 8,852,999,400 MSK).

## Safety of "the main wallet bonds"

Collateral at the operator's spending address is safe on both layers: consensus refuses any
transaction spending a locked bond outpoint (`palw_bond_collateral_is_locked_v2`, enforced on
the block path via `palw_v2_locked_bonds`), and the wallet's input selector excludes locked
collateral via the `palw_locked_bond_outpoints_v2` RPC — the same pair that closed the
fee-outpoint variant of this trap (audit3 H12).

## Consequences

* **Genesis identity moves on every network.** For testnet-11 this is Relaunch 3 (coinbase
  marker `11, 3`): a full re-mint — stop the whole fleet FIRST, wipe every datadir, redeploy
  (a peer wiped in-place while others run will be re-fed the old chain via IBD). Un-wiped
  peers are refused at the handshake. Mainnet has not launched, so its move is free.
* **This is a deliberate exception to the genesis-fixation policy** (2026-08-27: consensus
  changes ship by activation, never re-genesis). A supply-shape change is the one thing only
  a genesis can express, and the operator ordered it explicitly. The policy stands for
  everything else.
* The 40 custody addresses are out of consensus entirely. If mainnet wants custody splitting
  at launch, that is an operational wallet decision (spend the main wallet into N outputs),
  not a genesis structure.

## The rule for the next person who edits the genesis

Add community entrants by APPENDING to `TESTNET11_COMMUNITY_ALLOCATIONS` (the outpoint index
is the table position). Do not touch the cap. The main wallet pays; the build fails if it
cannot. If someone asks for a bigger genesis, the answer is this ADR and the operator, in
that order.
