#!/usr/bin/env python3
"""Abstract model-retirement lot ledger, not native BFT/Court authorization.

Funding ranges identify already existing assets. Unissued entitlements have no
funding range and cancellation never mints/burns cash. Role protects Panel and
Gate-C lots regardless of suspicion; a separate verified Court order may slash.
"""
from dataclasses import dataclass, replace
from moe3_reference import Refusal, add128, identity, mul256, require, u128

ROLES = frozenset({"REGISTRANT", "MINER", "PANEL", "GATE_C", "BFT"})
ASSETS = frozenset({"MISAKA", "POSITION"})


@dataclass(frozen=True)
class FundingSource:
    source_id: str
    asset: str
    total_atoms: int
    position_model_id: str | None = None


@dataclass(frozen=True)
class FundingRange:
    source_id: str
    start: int
    amount: int

    @property
    def end(self):
        return add128(self.start, self.amount)


@dataclass(frozen=True)
class Lot:
    lot_id: str
    model_id: str
    owner: str
    kind: str  # POOL, BOND or REWARD
    role: str
    asset: str
    original_atoms: int
    remaining_atoms: int
    status: str  # LIVE bond/pool; UNISSUED, ESCROWED or SETTLED reward
    funding: FundingRange | None
    virtual_pool_misaka_atoms: int = 0
    historical_lp_misaka_atoms: int = 0

    def binding(self):
        # Classification and original native asset identity are frozen. A valid
        # Court may reduce remaining collateral without changing this binding.
        return (self.lot_id, self.model_id, self.owner, self.kind, self.role, self.asset,
                self.original_atoms, self.funding, self.virtual_pool_misaka_atoms,
                self.historical_lp_misaka_atoms)


@dataclass(frozen=True)
class RetirementCase:
    case_id: str
    model_id: str
    bindings: tuple[tuple, ...]


@dataclass(frozen=True)
class RetirementReceipt:
    case_id: str
    model_id: str
    burned_misaka_atoms: int
    retired_position_atoms: int
    cancelled_unissued_misaka_atoms: int
    burns_by_purpose: tuple[tuple[str, int], ...]
    paid_to_bft_misaka_atoms: int = 0
    bounty_misaka_atoms: int = 0


@dataclass(frozen=True)
class VerifiedRetirementDecision:
    case_id: str
    model_id: str
    verified: bool
    # Trusted input after the real BFT authorization checks, which are not
    # implemented by this reference model.


@dataclass(frozen=True)
class CourtRecipient:
    role: str
    liability_identity: str
    weight: int


@dataclass(frozen=True)
class VerifiedCourtOrder:
    order_id: str
    offence_id: str
    lot_id: str
    defendant_identity: str
    max_recovery_atoms: int
    recipients: tuple[CourtRecipient, ...]
    reward_claim_deadline: int
    verified: bool
    # An explicit trusted-input abstraction. This class verifies no signature,
    # quorum, proof of a violation or appeal. The supplied recipient order and
    # weights are a reference plan, not a new mandatory native role allocation.


@dataclass(frozen=True)
class CourtRecovery:
    order: VerifiedCourtOrder
    recovered_misaka_atoms: int
    rewarded_misaka_atoms: int
    resolved_plan: tuple[CourtRecipient, ...] | None = None


@dataclass(frozen=True)
class CourtCredit:
    offence_id: str
    collection_id: str
    role: str
    liability_identity: str
    misaka_atoms: int


@dataclass(frozen=True)
class CourtRewardEscrow:
    offence_id: str
    collection_id: str
    original_misaka_atoms: int
    remaining_misaka_atoms: int
    status: str  # OPEN, PAID or EXPIRED


def court_pool_allocation(total: int, recipients: tuple[CourtRecipient, ...]) -> tuple[int, ...]:
    """Explicit ordered-weight reference plan for this collection's delta Q.

Each entry takes floor(remaining_pool * weight / remaining_weight). The final
    entry gets the remainder. No default half/half or recipient identity is invented.
    Previously paid collections are never re-apportioned.
"""
    remaining = u128(total)
    require(type(recipients) is tuple and len(recipients) > 0, "MissingCourtRecipientPlan")
    weight_total = 0
    seen = set()
    for row in recipients:
        require(isinstance(row, CourtRecipient) and row.role in {"PANEL", "GATE_C"}, "InvalidCourtRecipient")
        identity(row.liability_identity)
        require((row.role, row.liability_identity) not in seen, "DuplicateCourtRecipient")
        seen.add((row.role, row.liability_identity))
        require(u128(row.weight) > 0, "InvalidCourtWeight")
        weight_total = add128(weight_total, row.weight)
    result = []
    for row in recipients:
        amount = mul256(remaining, row.weight) // weight_total
        result.append(amount)
        remaining -= amount
        weight_total -= row.weight
    require(remaining == 0, "CourtPlanConservation")
    return tuple(result)


@dataclass(frozen=True)
class State:
    sources: tuple[FundingSource, ...] = ()
    lots: tuple[Lot, ...] = ()
    cases: tuple[RetirementCase, ...] = ()
    receipts: tuple[RetirementReceipt, ...] = ()
    retired_models: frozenset[str] = frozenset()
    executed_cases: frozenset[str] = frozenset()
    burned_misaka_atoms: int = 0
    retired_position_atoms: tuple[tuple[str, int], ...] = ()
    cancelled_unissued_misaka_atoms: int = 0
    court_debited_misaka_atoms: int = 0
    court_burned_misaka_atoms: int = 0
    court_reward_forfeited_misaka_atoms: int = 0
    court_collection_ids: frozenset[str] = frozenset()
    court_recoveries: tuple[CourtRecovery, ...] = ()
    court_credits: tuple[CourtCredit, ...] = ()
    court_reward_escrows: tuple[CourtRewardEscrow, ...] = ()


@dataclass(frozen=True)
class Undo:
    before: State
    after: State


class RetirementLedger:
    def __init__(self):
        self._state = State()

    @property
    def state(self):
        return self._state

    def lot(self, lot_id):
        result = next((row for row in self._state.lots if row.lot_id == lot_id), None)
        require(result is not None, "UnknownLot")
        return result

    def add_source(self, source: FundingSource):
        identity(source.source_id)
        require(source.asset in ASSETS, "UnknownAsset")
        require(u128(source.total_atoms) > 0, "EmptyFunding")
        if source.asset == "POSITION":
            identity(source.position_model_id)
        else:
            require(source.position_model_id is None, "WrongPositionModel")
        require(not any(row.source_id == source.source_id for row in self._state.sources), "DuplicateFundingSource")
        self._state = replace(self._state, sources=self._state.sources + (source,))

    def add_lot(self, lot: Lot):
        for value in (lot.lot_id, lot.model_id, lot.owner):
            identity(value)
        require(lot.role in ROLES and lot.kind in {"POOL", "BOND", "REWARD"}, "UnknownPurpose")
        require(lot.asset in ASSETS, "UnknownAsset")
        require(u128(lot.original_atoms) > 0 and lot.remaining_atoms == lot.original_atoms, "InvalidLotAmount")
        u128(lot.remaining_atoms)
        u128(lot.virtual_pool_misaka_atoms)
        u128(lot.historical_lp_misaka_atoms)
        require(not any(row.lot_id == lot.lot_id for row in self._state.lots), "DuplicateLot")
        require(lot.model_id not in self._state.retired_models, "ModelRetired")
        require(not any(case.model_id == lot.model_id for case in self._state.cases), "FrozenClassification")
        if lot.kind in {"POOL", "BOND"}:
            require(lot.status == "LIVE" and lot.asset == "MISAKA", "InvalidLotStatus")
        else:
            require(lot.status in {"UNISSUED", "ESCROWED", "SETTLED"}, "InvalidLotStatus")
        if lot.kind == "POOL":
            require(not any(row.kind == "POOL" and row.model_id == lot.model_id for row in self._state.lots), "DuplicatePool")
        else:
            require(lot.virtual_pool_misaka_atoms == 0 and lot.historical_lp_misaka_atoms == 0, "UnexpectedPoolMetadata")
        if lot.status == "UNISSUED":
            require(lot.funding is None and lot.asset == "MISAKA", "UnissuedIsNotNativeFunds")
        else:
            require(isinstance(lot.funding, FundingRange), "MissingFunding")
            funding = lot.funding
            source = next((row for row in self._state.sources if row.source_id == funding.source_id), None)
            require(source is not None, "UnknownFundingSource")
            require(source.asset == lot.asset, "WrongFundingAsset")
            require(source.asset != "POSITION" or source.position_model_id == lot.model_id, "WrongPositionModel")
            require(u128(funding.amount) == lot.original_atoms and funding.end <= source.total_atoms, "FundingRange")
            for old in self._state.lots:
                if old.funding is not None and old.funding.source_id == funding.source_id:
                    require(not (funding.start < old.funding.end and old.funding.start < funding.end), "OverlappingFunding")
        self._state = replace(self._state, lots=self._state.lots + (lot,))

    def reclassify_bond(self, lot_id: str, role: str):
        lot = self.lot(lot_id)
        require(lot.kind == "BOND" and role in ROLES, "UnknownPurpose")
        require(not any(case.model_id == lot.model_id for case in self._state.cases), "FrozenClassification")
        require(lot.model_id not in self._state.retired_models, "ModelRetired")
        self._state = replace(self._state, lots=tuple(replace(row, role=role) if row.lot_id == lot_id else row
                                                     for row in self._state.lots))

    def freeze_case(self, case_id: str, model_id: str) -> Undo:
        identity(case_id)
        identity(model_id)
        before = self._state
        require(model_id not in before.retired_models, "ModelRetired")
        require(not any(case.case_id == case_id for case in before.cases), "DuplicateCase")
        require(not any(case.model_id == model_id for case in before.cases), "ModelAlreadyFrozen")
        bindings = tuple(sorted((lot.binding() for lot in before.lots if lot.model_id == model_id), key=lambda row: row[0]))
        require(len(bindings) > 0, "UnknownModel")
        case = RetirementCase(case_id, model_id, bindings)
        self._state = replace(before, cases=before.cases + (case,))
        return Undo(before, self._state)

    def retire(self, case_id: str, decision: VerifiedRetirementDecision,
               fail_after: str | None = None) -> tuple[RetirementReceipt, Undo]:
        require(fail_after in (None, "lots", "receipt"), "UnknownFailurePoint")
        before = self._state
        require(case_id not in before.executed_cases, "AlreadyRetired")
        case = next((row for row in before.cases if row.case_id == case_id), None)
        require(case is not None, "MissingFrozenCase")
        require(isinstance(decision, VerifiedRetirementDecision) and decision.verified is True
                and (decision.case_id, decision.model_id) == (case.case_id, case.model_id), "UnverifiedRetirementDecision")
        require(case.model_id not in before.retired_models, "ModelRetired")
        current = tuple(sorted((lot.binding() for lot in before.lots if lot.model_id == case.model_id), key=lambda row: row[0]))
        require(current == case.bindings, "FrozenClassificationChanged")
        burned = retired = cancelled = 0
        by_purpose = {}
        lots = []
        for lot in before.lots:
            updated = lot
            if lot.model_id == case.model_id and lot.remaining_atoms:
                burn = (lot.kind == "POOL" or (lot.kind == "BOND" and lot.role in {"REGISTRANT", "MINER"})
                        or (lot.kind == "REWARD" and lot.role == "MINER" and lot.status == "ESCROWED"
                            and lot.asset == "MISAKA"))
                if burn:
                    burned = add128(burned, lot.remaining_atoms)
                    purpose = "POOL" if lot.kind == "POOL" else f"{lot.kind}:{lot.role}"
                    by_purpose[purpose] = add128(by_purpose.get(purpose, 0), lot.remaining_atoms)
                    updated = replace(lot, remaining_atoms=0, status="BURNED")
                elif lot.kind == "REWARD" and lot.role == "MINER" and lot.status == "ESCROWED" and lot.asset == "POSITION":
                    retired = add128(retired, lot.remaining_atoms)
                    updated = replace(lot, remaining_atoms=0, status="RETIRED")
                elif lot.kind == "REWARD" and lot.role == "MINER" and lot.status == "UNISSUED":
                    cancelled = add128(cancelled, lot.remaining_atoms)
                    updated = replace(lot, remaining_atoms=0, status="CANCELLED")
            lots.append(updated)
        require(fail_after != "lots", "InjectedFailure")
        receipt = RetirementReceipt(case.case_id, case.model_id, burned, retired, cancelled, tuple(sorted(by_purpose.items())))
        positions = dict(before.retired_position_atoms)
        positions[case.model_id] = add128(positions.get(case.model_id, 0), retired)
        after = replace(before, lots=tuple(lots), receipts=before.receipts + (receipt,),
                        retired_models=before.retired_models | {case.model_id},
                        executed_cases=before.executed_cases | {case_id},
                        burned_misaka_atoms=add128(before.burned_misaka_atoms, burned),
                        retired_position_atoms=tuple(sorted(positions.items())),
                        cancelled_unissued_misaka_atoms=add128(before.cancelled_unissued_misaka_atoms, cancelled))
        require(fail_after != "receipt", "InjectedFailure")
        self._state = after
        return receipt, Undo(before, after)

    def court_collect(self, order: VerifiedCourtOrder, collection_id: str, collected_atoms: int, now: int = 0) -> Undo:
        require(isinstance(order, VerifiedCourtOrder) and order.verified is True, "UnverifiedCourtOrder")
        identity(order.order_id)
        identity(order.offence_id)
        identity(order.defendant_identity)
        identity(collection_id)
        u128(now)
        u128(order.reward_claim_deadline)
        before = self._state
        require(collection_id not in before.court_collection_ids, "CourtCollectionAlreadyUsed")
        lot = self.lot(order.lot_id)
        require(lot.kind == "BOND" and lot.asset == "MISAKA", "NotCourtCollateral")
        require(lot.owner == order.defendant_identity, "CourtDefendantMismatch")
        require(u128(collected_atoms) <= lot.remaining_atoms, "CourtAmount")
        require(u128(order.max_recovery_atoms) > 0, "CourtAmount")
        previous = next((row for row in before.court_recoveries if row.order.offence_id == order.offence_id), None)
        if previous is None:
            previous = CourtRecovery(order, 0, 0)
        else:
            require(previous.order == order, "CourtScopeChanged")
        recovered = add128(previous.recovered_misaka_atoms, collected_atoms)
        require(recovered <= order.max_recovery_atoms, "CourtRecoveryCap")
        rewarded = mul256(recovered, 490) // 1000
        delta_reward = rewarded - previous.rewarded_misaka_atoms
        require(0 <= delta_reward <= collected_atoms, "CourtRewardConservation")
        delta_burn = collected_atoms - delta_reward
        # Recipient problems cannot excuse a valid debit. Delta Q is reserved
        # separately and can be claimed only after the eligible plan is resolved.
        late = now > order.reward_claim_deadline
        escrows = () if delta_reward == 0 else (CourtRewardEscrow(
            order.offence_id, collection_id, delta_reward, 0 if late else delta_reward,
            "EXPIRED" if late else "OPEN"),)
        forfeited = delta_reward if late else 0
        recovery = CourtRecovery(order, recovered, rewarded, previous.resolved_plan)
        recoveries = [row for row in before.court_recoveries if row.order.offence_id != order.offence_id] + [recovery]
        updated = replace(lot, remaining_atoms=lot.remaining_atoms - collected_atoms)
        after = replace(before, lots=tuple(updated if row.lot_id == lot.lot_id else row for row in before.lots),
                        court_debited_misaka_atoms=add128(before.court_debited_misaka_atoms, collected_atoms),
                        court_burned_misaka_atoms=add128(before.court_burned_misaka_atoms, add128(delta_burn, forfeited)),
                        court_reward_forfeited_misaka_atoms=add128(before.court_reward_forfeited_misaka_atoms, forfeited),
                        court_collection_ids=before.court_collection_ids | {collection_id},
                        court_recoveries=tuple(sorted(recoveries, key=lambda row: row.order.offence_id)),
                        court_reward_escrows=before.court_reward_escrows + escrows)
        self._state = after
        return Undo(before, after)

    def bind_court_plan(self, offence_id: str, recipients: tuple[CourtRecipient, ...], verified: bool, now: int = 0) -> Undo:
        require(verified is True, "UnverifiedCourtRecipientPlan")
        before = self._state
        recovery = next((row for row in before.court_recoveries if row.order.offence_id == offence_id), None)
        require(recovery is not None, "UnknownOffence")
        require(u128(now) <= recovery.order.reward_claim_deadline, "CourtRewardExpired")
        court_pool_allocation(0, recipients)
        require(all(row.liability_identity != recovery.order.defendant_identity for row in recipients), "DefendantCannotReceive")
        require(recovery.resolved_plan is None or recovery.resolved_plan == recipients, "CourtRecipientPlanChanged")
        updated = replace(recovery, resolved_plan=recipients)
        self._state = replace(before, court_recoveries=tuple(updated if row.order.offence_id == offence_id else row
                                                            for row in before.court_recoveries))
        return Undo(before, self._state)

    def claim_court_reward(self, offence_id: str, collection_id: str, now: int = 0) -> Undo:
        before = self._state
        recovery = next((row for row in before.court_recoveries if row.order.offence_id == offence_id), None)
        require(recovery is not None, "UnknownOffence")
        require(u128(now) <= recovery.order.reward_claim_deadline, "CourtRewardExpired")
        require(recovery.resolved_plan is not None, "CourtRecipientPlanUnresolved")
        escrow = next((row for row in before.court_reward_escrows
                       if (row.offence_id, row.collection_id) == (offence_id, collection_id)), None)
        require(escrow is not None, "NoCourtReward")
        require(escrow.status == "OPEN", "CourtRewardAlreadyFinalized")
        amounts = court_pool_allocation(escrow.remaining_misaka_atoms, recovery.resolved_plan)
        credits = tuple(CourtCredit(offence_id, collection_id, row.role, row.liability_identity, amount)
                        for row, amount in zip(recovery.resolved_plan, amounts) if amount)
        require(sum(row.misaka_atoms for row in credits) == escrow.remaining_misaka_atoms, "CourtRewardConservation")
        updated = replace(escrow, remaining_misaka_atoms=0, status="PAID")
        self._state = replace(before, court_credits=before.court_credits + credits,
                              court_reward_escrows=tuple(updated if row == escrow else row for row in before.court_reward_escrows))
        return Undo(before, self._state)

    def expire_court_rewards(self, offence_id: str, now: int) -> Undo:
        before = self._state
        recovery = next((row for row in before.court_recoveries if row.order.offence_id == offence_id), None)
        require(recovery is not None, "UnknownOffence")
        require(u128(now) > recovery.order.reward_claim_deadline, "CourtRewardNotExpired")
        expired = 0
        escrows = []
        for row in before.court_reward_escrows:
            if row.offence_id == offence_id and row.status == "OPEN":
                expired = add128(expired, row.remaining_misaka_atoms)
                row = replace(row, remaining_misaka_atoms=0, status="EXPIRED")
            escrows.append(row)
        self._state = replace(before, court_reward_escrows=tuple(escrows),
                              court_burned_misaka_atoms=add128(before.court_burned_misaka_atoms, expired),
                              court_reward_forfeited_misaka_atoms=add128(before.court_reward_forfeited_misaka_atoms, expired))
        return Undo(before, self._state)

    def undo(self, undo: Undo):
        require(self._state == undo.after, "OutOfOrderUndo")
        self._state = undo.before

    def conservation(self) -> dict[str, int]:
        totals = {"MISAKA": 0, "POSITION": 0}
        available = {"MISAKA": 0, "POSITION": 0}
        for source in self._state.sources:
            totals[source.asset] += source.total_atoms
            funded = [lot for lot in self._state.lots if lot.funding is not None and lot.funding.source_id == source.source_id]
            unused = source.total_atoms - sum(lot.original_atoms for lot in funded)
            available[source.asset] += unused + sum(lot.remaining_atoms for lot in funded)
        court_paid = sum(row.misaka_atoms for row in self._state.court_credits)
        court_escrow = sum(row.remaining_misaka_atoms for row in self._state.court_reward_escrows)
        require(self._state.court_debited_misaka_atoms == self._state.court_burned_misaka_atoms + court_paid + court_escrow,
                "CourtRewardConservation")
        available["MISAKA"] += court_paid + court_escrow
        require(totals["MISAKA"] == available["MISAKA"] + self._state.burned_misaka_atoms
                + self._state.court_burned_misaka_atoms, "MisakaConservation")
        require(totals["POSITION"] == available["POSITION"] + sum(dict(self._state.retired_position_atoms).values()),
                "PositionConservation")
        return {"initial_misaka_atoms": totals["MISAKA"], "available_misaka_atoms": available["MISAKA"],
                "initial_position_atoms": totals["POSITION"], "available_position_atoms": available["POSITION"]}
