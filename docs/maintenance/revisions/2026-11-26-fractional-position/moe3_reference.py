#!/usr/bin/env python3
"""MOE3 design reference only: fixed decimal quantities and atomic accounting.

This module does not activate a network, assign wire identifiers, verify work,
or replace the private MOE2 native implementation. Amounts inside this model
are integers; external decimal inputs and JSON atomic quantities are strings.
"""
from __future__ import annotations

from dataclasses import dataclass, replace
import re

PROFILE = "MOE3"
POSITION_DECIMALS = 8
SCALE = 10 ** POSITION_DECIMALS
ALLOCATION_COEFFICIENT = 1
U128_MAX = (1 << 128) - 1
U256_MAX = (1 << 256) - 1
DEFAULT_SUPPLY_WHOLE = 500_000
_DECIMAL = re.compile(r"(?:0|[1-9][0-9]*)(?:\.[0-9]{1,8})?", re.ASCII)
_ATOMS = re.compile(r"(?:0|[1-9][0-9]*)", re.ASCII)


class Refusal(ValueError):
    def __init__(self, code: str):
        self.code = code
        super().__init__(code)


def require(ok: bool, code: str) -> None:
    if not ok:
        raise Refusal(code)


def u128(value: int) -> int:
    require(type(value) is int and value >= 0, "InvalidUnsignedInteger")
    require(value <= U128_MAX, "Overflow128")
    return value


def add128(a: int, b: int) -> int:
    return u128(u128(a) + u128(b))


def mul256(a: int, b: int) -> int:
    require(type(a) is int and type(b) is int and a >= 0 and b >= 0, "InvalidUnsignedInteger")
    require(a <= U256_MAX and b <= U256_MAX, "Overflow256")
    result = a * b
    require(result <= U256_MAX, "Overflow256")
    return result


def ceildiv256(numerator: int, denominator: int) -> int:
    require(type(numerator) is int and 0 <= numerator <= U256_MAX, "InvalidNumerator")
    require(type(denominator) is int and 0 < denominator <= U256_MAX, "InvalidDenominator")
    quotient, remainder = divmod(numerator, denominator)
    # Do not evaluate numerator + denominator - 1 in a bounded integer type.
    return quotient + int(remainder != 0)


def parse_position(text: str) -> int:
    require(type(text) is str and _DECIMAL.fullmatch(text) is not None, "InvalidPositionDecimal")
    whole, _, fraction = text.partition(".")
    # The length bound avoids parsing arbitrarily long attacker-supplied integers.
    require(len(whole) <= 39, "Overflow128")
    return u128(mul256(int(whole), SCALE) + int(fraction.ljust(POSITION_DECIMALS, "0") or "0"))


def format_position(atoms: int) -> str:
    whole, fraction = divmod(u128(atoms), SCALE)
    return str(whole) if fraction == 0 else f"{whole}.{fraction:08d}".rstrip("0")


def parse_atoms_json(text: str) -> int:
    require(type(text) is str and _ATOMS.fullmatch(text) is not None, "InvalidAtomicString")
    require(len(text) <= 39, "Overflow128")
    return u128(int(text))


def atoms_json(value: int) -> str:
    return str(u128(value))


def identity(value: str) -> str:
    require(type(value) is str and len(value) > 0, "InvalidIdentity")
    return value


@dataclass(frozen=True)
class Market:
    model_id: str
    virtual_misaka_atoms: int
    real_misaka_atoms: int
    seed_locked_misaka_atoms: int
    inventory_position_atoms: int
    holder_position_atoms: int
    retired_position_atoms: int
    issued_position_atoms: int
    creator: str | None = None
    profile: str = PROFILE
    migrated_from: str | None = None

    def validate(self) -> None:
        identity(self.model_id)
        if self.creator is not None:
            identity(self.creator)
        require(self.profile == PROFILE, "UnknownProfile")
        for name in (
            "virtual_misaka_atoms", "real_misaka_atoms", "seed_locked_misaka_atoms",
            "inventory_position_atoms", "holder_position_atoms", "retired_position_atoms",
            "issued_position_atoms",
        ):
            u128(getattr(self, name))
        require(self.real_misaka_atoms >= self.seed_locked_misaka_atoms, "SeedFloor")
        require(self.inventory_position_atoms > 0, "EmptyInventory")
        require(self.x > 0, "EmptyCurve")
        total = add128(add128(self.inventory_position_atoms, self.holder_position_atoms), self.retired_position_atoms)
        require(total == self.issued_position_atoms, "SupplyMismatch")

    @property
    def x(self) -> int:
        return add128(self.virtual_misaka_atoms, self.real_misaka_atoms)

    def to_json(self) -> dict:
        return {
            "model_id": self.model_id, "profile": self.profile, "creator": self.creator,
            "migrated_from": self.migrated_from,
            **{name: atoms_json(getattr(self, name)) for name in (
                "virtual_misaka_atoms", "real_misaka_atoms", "seed_locked_misaka_atoms",
                "inventory_position_atoms", "holder_position_atoms", "retired_position_atoms",
                "issued_position_atoms",
            )},
        }


def register_market(model_id: str, virtual_misaka_atoms: int, creator: str | None = None,
                    supply_whole: int = DEFAULT_SUPPLY_WHOLE) -> Market:
    """One market for any model; no genesis, fork, tier or admission-route exemption."""
    supply = u128(mul256(u128(supply_whole), SCALE))
    market = Market(model_id, virtual_misaka_atoms, 0, 0, supply, 0, 0, supply, creator)
    market.validate()
    return market


@dataclass(frozen=True)
class Fees:
    burn_misaka_atoms: int
    creator_misaka_atoms: int
    net_misaka_atoms: int


def trade_fees(gross_misaka_atoms: int, creator_present: bool) -> Fees:
    gross = u128(gross_misaka_atoms)
    require(type(creator_present) is bool, "InvalidCreatorPresence")
    burn = mul256(gross, 50) // 1000
    creator = mul256(gross, 10) // 1000 if creator_present else 0
    return Fees(burn, creator, gross - burn - creator)


@dataclass(frozen=True)
class CoinbaseSplit:
    coinbase_misaka_atoms: int
    bft_misaka_atoms: int
    verification_misaka_atoms: int
    lp_misaka_atoms: int
    lp_purchase_misaka_atoms: int
    miner_purchase_misaka_atoms: int

    @property
    def purchase_misaka_atoms(self) -> int:
        return add128(self.lp_purchase_misaka_atoms, self.miner_purchase_misaka_atoms)


def split_coinbase(coinbase_misaka_atoms: int) -> CoinbaseSplit:
    """Latest user decision: nominal 50/20/20/5/5; all cash dust stays in D."""
    c = u128(coinbase_misaka_atoms)
    f = mul256(c, 50) // 1000
    p = mul256(c, 50) // 1000
    a = mul256(c, 200) // 1000
    j = mul256(c, 200) // 1000
    d = c - f - p - a - j
    return CoinbaseSplit(c, f, p, a, j, d)


def apportion(amount: int, weights: tuple[tuple[str, int], ...]) -> tuple[tuple[str, int], ...]:
    """Integer largest remainder; caller provides authenticated canonical IDs/weights."""
    amount = u128(amount)
    require(type(weights) is tuple and len(weights) > 0, "InvalidWeights")
    values = {}
    total = 0
    for model_id, weight in weights:
        identity(model_id)
        require(model_id not in values, "DuplicateModel")
        total = add128(total, weight)
        values[model_id] = u128(weight)
    require(total > 0, "ZeroWeight")
    rows = [(key, *divmod(mul256(amount, weight), total)) for key, weight in values.items()]
    residual = amount - sum(base for _, base, _ in rows)
    ranked = sorted(rows, key=lambda row: (-row[2], row[0].encode("utf-8")))
    awarded = {key: base + int(index < residual) for index, (key, base, _) in enumerate(ranked)}
    return tuple(sorted(awarded.items(), key=lambda row: row[0].encode("utf-8")))


@dataclass(frozen=True)
class ModelBudget:
    model_id: str
    lp_purchase_misaka_atoms: int
    lp_misaka_atoms: int
    miner_purchase_misaka_atoms: int
    purchase_misaka_atoms: int


def coupled_model_budgets(split: CoinbaseSplit, weights: tuple[tuple[str, int], ...]) -> tuple[ModelBudget, ...]:
    require(split == split_coinbase(split.coinbase_misaka_atoms), "InvalidBudgetSplit")
    j = dict(apportion(split.lp_purchase_misaka_atoms, weights))
    excess = split.miner_purchase_misaka_atoms - 2 * split.lp_purchase_misaka_atoms
    require(0 <= excess <= U128_MAX, "InvalidBudgetSplit")
    e = dict(apportion(excess, weights))
    result = []
    for key in j:
        d = add128(u128(2 * j[key]), e[key])
        result.append(ModelBudget(key, j[key], j[key], d, add128(j[key], d)))
    return tuple(result)


@dataclass(frozen=True)
class VerificationAllocation:
    role: str
    payee: str
    misaka_atoms: int


def validate_verification_plan(total: int, allocations: tuple[VerificationAllocation, ...] | None) -> None:
    """None reserves the whole pool without inventing Panel/Gate-C recipients."""
    u128(total)
    if allocations is None:
        return
    require(type(allocations) is tuple, "InvalidVerificationPlan")
    seen = set()
    allocated = 0
    for item in allocations:
        require(isinstance(item, VerificationAllocation), "InvalidVerificationPlan")
        require(item.role in {"PANEL", "GATE_C"}, "InvalidVerificationRole")
        identity(item.payee)
        key = (item.role, item.payee)
        require(key not in seen, "DuplicateVerificationPayee")
        seen.add(key)
        allocated = add128(allocated, item.misaka_atoms)
    require(allocated == total, "VerificationPlanTotal")


@dataclass(frozen=True)
class Limits:
    max_coinbase_misaka_atoms: int = U128_MAX
    max_purchase_misaka_atoms: int = U128_MAX
    max_lp_misaka_atoms: int = U128_MAX
    max_gross_position_atoms: int = U128_MAX
    max_lp_position_atoms: int = U128_MAX
    min_miner_position_atoms: int = 0
    expected_gross_position_atoms: int | None = None

    def validate(self) -> None:
        for name in self.__dataclass_fields__:
            value = getattr(self, name)
            if name != "expected_gross_position_atoms" or value is not None:
                u128(value)


@dataclass(frozen=True)
class Quote:
    coinbase_misaka_atoms: int
    bft_misaka_atoms: int
    verification_misaka_atoms: int
    lp_purchase_misaka_atoms: int
    miner_purchase_misaka_atoms: int
    purchase_misaka_atoms: int
    lp_misaka_atoms: int
    gross_position_atoms: int
    lp_position_atoms: int
    miner_position_atoms: int
    buy_x: int
    buy_inventory_position_atoms: int
    after: Market

    def to_json(self) -> dict:
        return {**{name: atoms_json(getattr(self, name)) for name in self.__dataclass_fields__ if name != "after"},
                "after": self.after.to_json()}


def quote_reward(market: Market, coinbase_misaka_atoms: int, limits: Limits = Limits(),
                 profile: str = PROFILE) -> Quote:
    require(profile == PROFILE, "UnknownProfile")
    market.validate()
    split = split_coinbase(coinbase_misaka_atoms)
    coinbase = split.coinbase_misaka_atoms
    limits.validate()
    purchase = split.purchase_misaka_atoms
    lp_cash = split.lp_misaka_atoms
    require(coinbase <= limits.max_coinbase_misaka_atoms
            and purchase <= limits.max_purchase_misaka_atoms
            and lp_cash <= limits.max_lp_misaka_atoms, "OutsideQuote")
    x1 = add128(market.x, purchase)
    inventory = market.inventory_position_atoms
    u1 = u128(ceildiv256(mul256(market.x, inventory), x1))
    q = inventory - u1
    lp_units = mul256(q, split.lp_purchase_misaka_atoms) // purchase if purchase else 0
    miner_units = q - lp_units
    if coinbase > 0:
        require(all((purchase, lp_cash, split.lp_purchase_misaka_atoms,
                     split.miner_purchase_misaka_atoms, q, lp_units, miner_units)), "LiquidityDust")
    require(miner_units >= limits.min_miner_position_atoms
            and q <= limits.max_gross_position_atoms
            and lp_units <= limits.max_lp_position_atoms, "OutsideQuote")
    if limits.expected_gross_position_atoms is not None:
        require(q == limits.expected_gross_position_atoms, "GrossQuoteMismatch")
    after = replace(market,
                    real_misaka_atoms=add128(market.real_misaka_atoms, add128(purchase, lp_cash)),
                    inventory_position_atoms=add128(u1, lp_units),
                    holder_position_atoms=add128(market.holder_position_atoms, miner_units))
    after.validate()  # Includes checked X2=V+r2, fixed supply and the old seed floor.
    mul256(after.x, after.inventory_position_atoms)
    return Quote(coinbase, split.bft_misaka_atoms, split.verification_misaka_atoms,
                 split.lp_purchase_misaka_atoms, split.miner_purchase_misaka_atoms,
                 purchase, lp_cash, q, lp_units, miner_units, x1, u1, after)


@dataclass(frozen=True)
class Receipt:
    settlement_id: str
    model_id: str
    miner: str
    coinbase_misaka_atoms: int
    bft_misaka_atoms: int
    verification_misaka_atoms: int
    lp_purchase_misaka_atoms: int
    miner_purchase_misaka_atoms: int
    purchase_misaka_atoms: int
    lp_misaka_atoms: int
    gross_position_atoms: int
    lp_position_atoms: int
    miner_position_atoms: int
    verification_allocations: tuple[VerificationAllocation, ...] | None
    # History only. No token, owner, withdrawal, transfer or fee-claim balance.


@dataclass(frozen=True)
class BudgetAllocation:
    settlement_id: str
    miner: str
    split: CoinbaseSplit
    verification_allocations: tuple[VerificationAllocation, ...] | None


@dataclass(frozen=True)
class Snapshot:
    market: Market
    miner_balances: tuple[tuple[str, int], ...] = ()
    receipts: tuple[Receipt, ...] = ()
    consumed: frozenset[str] = frozenset()
    coinbase_debited_misaka_atoms: int = 0
    verification_reserved_misaka_atoms: int = 0
    bft_reserved_misaka_atoms: int = 0
    market_reserved_misaka_atoms: int = 0
    allocations: tuple[BudgetAllocation, ...] = ()
    has_permanent_lp: bool = False


@dataclass(frozen=True)
class Undo:
    before: Snapshot
    after: Snapshot


class ReferenceBook:
    """Single-model allocation + atomic market journal, not a native adapter.

Allocation reserves earned P/F independently. A subsequent market refusal never
revokes them. The convenience settle method only publishes its successful result.
"""
    def __init__(self, market: Market):
        market.validate()
        self._state = Snapshot(market)

    @property
    def state(self) -> Snapshot:
        return self._state

    def allocate_budget(self, settlement_id: str, miner: str, coinbase: int,
                        verification_allocations: tuple[VerificationAllocation, ...] | None = None) -> Undo:
        identity(settlement_id)
        identity(miner)
        before = self._state
        require(settlement_id not in before.consumed, "AlreadySettled")
        require(not any(item.settlement_id == settlement_id for item in before.allocations), "AlreadyAllocated")
        split = split_coinbase(coinbase)
        validate_verification_plan(split.verification_misaka_atoms, verification_allocations)
        if coinbase == 0:
            return Undo(before, before)
        allocation = BudgetAllocation(settlement_id, miner, split, verification_allocations)
        after = replace(before,
                        coinbase_debited_misaka_atoms=add128(before.coinbase_debited_misaka_atoms, coinbase),
                        verification_reserved_misaka_atoms=add128(before.verification_reserved_misaka_atoms,
                                                                 split.verification_misaka_atoms),
                        bft_reserved_misaka_atoms=add128(before.bft_reserved_misaka_atoms, split.bft_misaka_atoms),
                        market_reserved_misaka_atoms=add128(before.market_reserved_misaka_atoms,
                                                           add128(split.purchase_misaka_atoms, split.lp_misaka_atoms)),
                        allocations=before.allocations + (allocation,))
        self._state = after
        return Undo(before, after)

    def settle_market(self, settlement_id: str, limits: Limits = Limits(),
                      profile: str = PROFILE, fail_after: str | None = None) -> tuple[Quote, Undo]:
        require(fail_after in (None, "quote", "miner", "cash", "receipt"), "UnknownFailurePoint")
        before = self._state
        require(settlement_id not in before.consumed, "AlreadySettled")
        allocation = next((item for item in before.allocations if item.settlement_id == settlement_id), None)
        require(allocation is not None, "MissingAllocation")
        quote = quote_reward(before.market, allocation.split.coinbase_misaka_atoms, limits, profile)
        require(fail_after != "quote", "InjectedFailure")
        miner = allocation.miner
        balances = dict(before.miner_balances)
        balances[miner] = add128(balances.get(miner, 0), quote.miner_position_atoms)
        require(fail_after != "miner", "InjectedFailure")
        market_cash = add128(quote.purchase_misaka_atoms, quote.lp_misaka_atoms)
        require(before.market_reserved_misaka_atoms >= market_cash, "MissingMarketFunds")
        require(fail_after != "cash", "InjectedFailure")
        receipt = Receipt(settlement_id, before.market.model_id, miner, quote.coinbase_misaka_atoms,
                          quote.bft_misaka_atoms, quote.verification_misaka_atoms,
                          quote.lp_purchase_misaka_atoms, quote.miner_purchase_misaka_atoms,
                          quote.purchase_misaka_atoms, quote.lp_misaka_atoms,
                          quote.gross_position_atoms, quote.lp_position_atoms, quote.miner_position_atoms,
                          allocation.verification_allocations)
        after = replace(before, market=quote.after, miner_balances=tuple(sorted(balances.items())),
                        receipts=before.receipts + (receipt,), consumed=before.consumed | {settlement_id},
                        market_reserved_misaka_atoms=before.market_reserved_misaka_atoms - market_cash,
                        has_permanent_lp=True)
        require(fail_after != "receipt", "InjectedFailure")
        self._state = after  # The only mutation: every projected leg commits together.
        return quote, Undo(before, after)

    def settle(self, settlement_id: str, miner: str, coinbase: int, limits: Limits = Limits(),
               profile: str = PROFILE, fail_after: str | None = None,
               verification_allocations: tuple[VerificationAllocation, ...] | None = None) -> tuple[Quote, Undo]:
        """Convenience successful-path projection; use allocate_budget for earned P/F."""
        before = self._state
        projected = ReferenceBook(before.market)
        projected._state = before
        projected.allocate_budget(settlement_id, miner, coinbase, verification_allocations)
        if coinbase == 0:
            quote = quote_reward(before.market, coinbase, limits, profile)
            return quote, Undo(before, before)
        quote, _ = projected.settle_market(settlement_id, limits, profile, fail_after)
        self._state = projected.state
        return quote, Undo(before, self._state)

    def undo(self, undo: Undo) -> None:
        require(self._state == undo.after, "OutOfOrderUndo")
        self._state = undo.before


@dataclass(frozen=True)
class LegacyWholeMarket:
    model_id: str
    virtual_misaka_atoms: int
    real_misaka_atoms: int
    seed_locked_misaka_atoms: int
    inventory_whole: int
    holder_whole: int
    retired_whole: int
    issued_whole: int
    creator: str | None = None
    source_profile: str = "MOE2"
    pending_old_claims: int = 0
    live_old_allowances: int = 0


class MigrationBook:
    """Abstract one-time scale migration; old claims/allowances need an explicit drain."""
    def __init__(self):
        self._markets: dict[str, Market] = {}

    @property
    def markets(self) -> dict[str, Market]:
        return dict(self._markets)

    def migrate(self, legacy: LegacyWholeMarket | Market) -> Market:
        require(not isinstance(legacy, Market), "AlreadyMigrated")
        require(isinstance(legacy, LegacyWholeMarket), "InvalidLegacyMarket")
        identity(legacy.model_id)
        require(legacy.model_id not in self._markets, "AlreadyMigrated")
        require(legacy.source_profile in {"LEGACY_WHOLE", "MOE1", "MOE2"}, "UnknownProfile")
        require(u128(legacy.pending_old_claims) == 0, "PendingOldClaims")
        require(u128(legacy.live_old_allowances) == 0, "LiveOldAllowances")
        values = [u128(mul256(u128(getattr(legacy, name)), SCALE))
                  for name in ("inventory_whole", "holder_whole", "retired_whole", "issued_whole")]
        market = Market(legacy.model_id, legacy.virtual_misaka_atoms, legacy.real_misaka_atoms,
                        legacy.seed_locked_misaka_atoms, *values, legacy.creator, PROFILE, legacy.source_profile)
        market.validate()
        self._markets[market.model_id] = market
        return market
