# MISAKA model market — Solidity interfaces to the native primitives

The Solidity face of ADR-0089 (*the fold is the truth; the EVM is its window and its hand*),
on top of ADR-0087 (the market is a balance in the state fold) and ADR-0088 (lines, versions,
roles, usage). This directory plays the role `L1Read.sol` plays for HyperEVM: **interfaces and
address constants only**. There is nothing to deploy.

## These are interfaces to NATIVE code

Every address below is implemented by the node's executor, not by bytecode. `EXTCODESIZE` is
`0` and `EXTCODEHASH` is the empty-code hash at all of them — exactly as at `0x…F002`
(withdraw) and `0x…F003` (ML-DSA-87 verify). The truth they read is the **PALW state fold**,
never EVM storage: a read returns rows of `fold(selected_parent(B))` for the EVM block `B` the
call executes in — the one fold state that is a function of `B`'s parents alone — so two nodes
with the same store answer byte-identically and no oracle stands in between.

| address | interface | kind |
|---|---|---|
| `0x…F010` | `IMisakaModelRegistry` | read precompile — classes, lines, versions, usage, evaluations, proposals, `facadeOf`/`lineOf`, `chainDaa` |
| `0x…F011` | `IMisakaModelAMM` | read precompile — market rows, price, quotes, curve constants |
| `0x…F012` | `IMisakaModelPosition` | read precompile — balances by 64-byte holder id or EVM address |
| `0x…F013` | `IMisakaModelWriter` | the hand — `sendAction(bytes)`, a call-frame intercept; also the buy escrow account |
| `0x4d50 ‖ 18 bytes` | `IMRC20` | one native facade per **line** — ERC-20's read half, the curve in place of its transfer half |

`eth_call` against any of them works from any Shanghai toolchain at chain id `0x4D534B` with
no MISAKA-specific RPC. Receipts of the settling block carry the facade events under the
standard log format (the receipts *root* is MISAKA's; the log fields are standard).

Unknown key ⇒ the **zero row**, not a revert — the zero is itself a fact. Malformed input
reverts and consumes the frame's gas. Below the `palw_model_evm` fence (ADR-0089 Decision 9)
every one of these addresses is an **empty account**: a call succeeds with empty return data.
A reader that wants to know the window is open checks that `chainDaa()` returns 32 bytes.

## The files

| file | what it is |
|---|---|
| `MisakaModelAddresses.sol` | a library of constants: the four system addresses, `FACADE_PREFIX = 0x4d50`, the chain id, `NATIVE_SCALE_WEI = 1e10`, the writer's encoding version / action ids / per-block cap |
| `IMisakaModelRegistry.sol` | the registry window: `classCount/classAt/classRow/certified/rootsInForceCount/rootInForceAt`, `lineCount/lineAt/linesOfCount/lineOfClassAt/line`, `version/usage`, `evaluationCount/evaluation`, `proposalCount/proposal`, `facadeOf/lineOf`, `chainDaa` |
| `IMisakaModelAMM.sol` | the curve window: `market` (ADR-0091 appended `buybackSompi` and `retiredUnits` at the end, so every earlier word keeps its offset), `price`, `quoteBuy`, `quoteSell`, `constants` |
| `IMisakaModelPosition.sol` | the position window: `balanceOf` (64-byte holder), `balanceOfAddress`, `totalSupply`, `sold`, `holderIdOf` |
| `IMRC20.sol` | the per-line facade: `name/symbol/decimals/totalSupply/balanceOf`, `lineId/circulating/price/quoteBuy/quoteSell`, `buy/sell/seed`, `supportsInterface`; events `Bought/Sold/Seeded/Refused`; errors `NonTransferable/NotAnAccount/ClosedToBuys/BadValue/SeedTooSmall` |
| `IMisakaModelWriter.sol` | the hand: `sendAction(bytes) payable`, event `ActionQueued`, error `NotAnAccount`; the data layout and the settlement timing |

Vocabulary (ADR-0088 §3): a **class** is a registered model family with a share and a budget;
a **line** is a model — a class, an owner, a name — and the **founding line of a class has the
class id as its line id**; a **version** is one developer-signed publication on a line (dense,
monotone, 1-based); a **payload** is a bond's payout payload, the 64-byte identity a bond pays
to and the holder id of its positions. The market and the positions are keyed by line.

## The encoding rule (binding)

Every 64-byte id — line id, class id, holder id, payout payload, root, hash, proposal id,
evaluator id, seed — is **two `bytes32` words, high half first**, named `<x>A` / `<x>B`
(`lineA` = bytes 0..32 of the id, `lineB` = bytes 32..64). There are no dynamic types in these
ABIs except `string` for `name()` / `symbol()`, `bytes` for `sendAction(bytes)` and the
`ActionQueued` event's `data`, and `bytes4` for `supportsInterface`.

Units: MSK amounts are in **sompi** (1 MSK = 1e8 sompi), the fold's unit, everywhere — in the
views, the quotes and the settlement events. The one **wei** quantity is `msg.value` on a buy
(`sendAction` action 1 or `IMRC20.buy`), which must be a **nonzero multiple of 1e10**
(`EVM_NATIVE_SCALE`, the F002 rule) so the fold's sompi leg is exact. Position amounts are in
**units (ADR-0090)**: `decimals() == 0` — a position is a whole number, one unit, no fraction; the
whole supply of a line is 500,000 positions, fixed at the seed. **The seed (ADR-0090)**: a line's
market exists only once someone locks at least 100,000 MSK into it (`seed()` on the facade, or
`sendAction` id 3, or the carrier object `ModelSeed`); the whole seed is the reserve, the first price
is `seed / 500,000`, the seeder receives no position, and no object ever pays the seed out — the
curve's product never falls, so with every position back in the curve the reserve is the seed or
more.

The pair also grows without a trade: at the `Final` of a claim a block earned with the model,
five percent of that block's escrowed worker reward buys from the line's curve and stays in it,
the miner is named the other ninety-five, and the positions the curve gives up are **retired** —
the chain's, for good, counted by `retiredUnits` and by no `balanceOf`. Nothing is distributed to
holders; the price is what mining moves (ADR-0091).

Facade addresses are `0x4d50 ‖ blake2b_512("misaka-evm/model-position-facade/v1" ‖
line_id)[..18]`. BLAKE2b is **not available in Solidity** on this lane, so a contract cannot
derive one: read it from `IMisakaModelRegistry.facadeOf(lineA, lineB)` and check an address
that arrives from outside with `lineOf(address)`. The prefix alone proves nothing — a prefixed
address that names no line is an empty account. The same holds for an EVM account's holder id
(`evm_holder_v1`, a keyed BLAKE2b-512): read it from `IMisakaModelPosition.holderIdOf`.

## The writer's data layout

```
data = [version u8 = 1] ‖ [action id u24, big-endian] ‖ [abi]

id 1  buy   abi = abi.encode(bytes32 lineA, bytes32 lineB, uint256 minUnitsOut)
            msg.value = the gross MSK leg in wei, a nonzero multiple of 1e10
id 2  sell  abi = abi.encode(bytes32 lineA, bytes32 lineB, uint256 unitsIn, uint256 minMskOutSompi)
            msg.value = 0
3–255       reserved
```

`IMRC20.buy` / `IMRC20.sell` are the same two actions with the line filled in by the address —
one write path, two doors. No action founds a line, publishes a version, sets a role, registers
a class or certifies a family: those are ML-DSA-87 bond signatures on the UTXO side, and an
EVM account has no bond.

The call reverts `NotAnAccount()` unless `msg.sender == tx.origin` and `msg.sender` has no
code; it also reverts, at the call, for malformed data, an unknown line, a bad `msg.value`, a
sell of zero, a buy on a line closed to buys, and the 129th action in one EVM block (at most
128). A reverted call leaves no log and therefore no action. The block's action list is built
by scanning the committed logs of the accepted txs for `ActionQueued` from `0x…F013`, in log
order — a forged log from any other address is not the writer's.

## Settlement timing

| step | where | what happens |
|---|---|---|
| **emit** | `EVM(B)` — the EVM block of chain block `B` | the writer validates the call, escrows a buy's `msg.value` at `0x…F013`, emits `ActionQueued`. Nothing else: the call returns no result. |
| **apply** | `fold(B)` | after every carrier-borne `ModelBuy` / `ModelSell` of `B`, the EVM actions in queue order; each is quoted on the row *as it then stands*, its `min` checked, and **filled** (the row and the holder's units move) or **refused** with a reason — never partially filled. |
| **settle** | `EVM(C)`, `C` = the selected child of `B`, as system ops before `C`'s user txs | a filled buy's escrow is **burned** into the line's sink output (`OP_RETURN ‖ MSKMDL01 ‖ line`); a refused buy's escrow is **refunded** to the account; a filled sell **credits** the account `net × 1e10` wei. The line's facade emits `Bought` / `Sold` / `Refused` here, in a system receipt at index 0 of `C`'s EVM block. |
| **visible** | `EVM(C)` and after | the precompiles at `C` read `fold(B)`, which already holds the moved row. |

So a position bought at `B` is readable, and settled, **one chain block later**. The facade
events are emitted at `C`, never at the call: the calling tx's receipt carries only
`ActionQueued`. Carriers trade first by rule, so the EVM door is never the faster one to the
same curve; a quote from `IMisakaModelAMM` is against the selected parent's row — pass it as
`min` and let the fold refuse rather than fill worse. A refusal costs the caller the call's gas
and nothing else, because the value never reached the fold: a refused buy is a refund inside
the EVM at `C`.

## Why a contract cannot hold a position

ADR-0087 Decision 5 is a fold fact: **no object moves units between holders** — a position is
bought from the curve and sold back to it, and grants nothing but the right to sell it back.
On the EVM that fact would not survive two constructions that rebuild a transfer from the
outside: the **wrapper** (a contract buys positions and issues its own transferable shares)
and the **delegate** (a contract sells on a holder's behalf). Both die on one rule (ADR-0089
Decision 4): **a holder is the signing account and only the signing account.** `sendAction`,
`IMRC20.buy` and `IMRC20.sell` refuse any call where `msg.sender != tx.origin` or `msg.sender`
has code (`NotAnAccount()`), so no contract ever holds or moves a position, and a position's
only counterparty is the curve.

The cost is stated: no contract wallet, no multisig, no account-abstraction account can hold a
position on this lane. EIP-7702 is not in Shanghai; if a later fork admits it, the code-size
clause is what keeps the rule, and that fork must re-decide it explicitly. Consequently these
interfaces serve two kinds of caller: an **externally owned account** sending a tx to the
writer or a facade, and **any contract or off-chain reader** using the views over `eth_call`.
A contract that *calls* `buy` gets `NotAnAccount()`, not a position.

The facade is ERC-20's read half only. `transfer`, `transferFrom`, `approve` and `allowance`
revert `NonTransferable()`; there is no `Transfer` and no `Approval` event;
`supportsInterface(0x36372b07)` is `false` on purpose, and `supportsInterface(type(IMRC20).
interfaceId)` is `true`. And an EVM account's position never crosses to the PQ side: an EVM
holder's id is its own namespace (`holderIdOf`), a position bought from the EVM is sold from
the EVM, and ADR-0023's `CLASSICAL-ECC` label is what a wallet shows for it.

## Compiling

No toolchain is required to use these files — they are `pragma solidity ^0.8.20` interfaces
and a constants library with no imports; each file stands alone. Any `solc` ≥ 0.8.20
(`evm_version = "shanghai"`, as in `../nft/foundry.toml` and `../pq-account/foundry.toml`)
compiles them; `forge build` on a project that lists this directory as a source path is
enough. The ABIs are the ones the native code implements; do not extend them here — a selector
the executor does not serve returns empty data, which the zero-row rule would make look like a
fact. `IMRC20` and `IMisakaModelWriter` each declare the one `NotAnAccount()` selector they
share; a mock that implements both must implement the functions and declare the error once
rather than inherit both interfaces (solc rejects the duplicate declaration).
