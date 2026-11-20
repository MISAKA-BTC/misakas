#!/usr/bin/env python3
"""RFC-0055: independent ABSTRACT CONTRACT MODEL; synthetic fixtures only.

NOT the MISAKA implementation, E2E, a hardware benchmark, a capacity
qualification, or a formal proof. No test establishes real Node throughput.
Court/claim counts are consequences of example resource supplies and costs;
there is NO normative one-original/one-normal/three-negative rule here.

A common bounded checker is qualified, never individual model names. Cost
coefficients, timing, supply, and all test profiles below are illustrative.
Authentication, complete obligation graphs, ClockBridge, actual mixed-load
qualification, and the production wire codec remain external proof duties.
Deep copies provide abstract transactions; they are not a scalable design.

Run with Python 3.10+ and no third-party dependencies/network. The script
always writes model-results.json, including after failed test assertions.
"""
from __future__ import annotations

import copy
import hashlib
import itertools
import json
import sys
import unittest
from dataclasses import dataclass, field, replace
from datetime import datetime, timezone
from pathlib import Path

CYCLE = 120_000
PERIOD = 1_000
FIXTURE_LIVE = 4  # An example CapacityProfile value, never a protocol constant.
U256_MAX = (1 << 256) - 1
D_MAX = (1 << 120) - 1
FLOW_NAMES = ("work", "wire_bytes", "io_operations", "signatures", "control",
              "hash_bytes", "parse_expand_bytes", "io_read_bytes", "io_write_bytes", "carrier_space")
STOCK_NAMES = ("ram", "stored_bytes", "pending_metadata")


class Rejected(Exception):
    def __init__(self, code: str):
        self.code = code
        super().__init__(code)


def require(condition: bool, code: str) -> None:
    if not condition:
        raise Rejected(code)


def checked(value: int) -> int:
    require(isinstance(value, int) and 0 <= value <= U256_MAX, "IntegerRange")
    return value


def ceil_div(value: int, divisor: int) -> int:
    checked(value)
    require(divisor > 0, "ZeroSupply")
    return value // divisor + bool(value % divisor)


def ratio1024(demand: int, supply: int) -> int:
    checked(demand)
    checked(supply)
    if supply == 0:
        require(demand == 0, "UnavailableSupply")
        return 0
    return ceil_div(checked(1_024 * demand), supply)


def vector(values, dimensions: int) -> tuple[int, ...]:
    values = tuple(checked(value) for value in values)
    require(len(values) == dimensions, "VectorShape")
    return values


def add(a: tuple[int, ...], b: tuple[int, ...]) -> tuple[int, ...]:
    require(len(a) == len(b), "VectorShape")
    return tuple(checked(x + y) for x, y in zip(a, b))


def fits(demand: tuple[int, ...], supply: tuple[int, ...]) -> bool:
    require(len(demand) == len(supply), "VectorShape")
    return all(0 <= x <= y for x, y in zip(demand, supply))


def identical_count_bound(supply: tuple[int, ...], cost: tuple[int, ...]) -> int:
    """Explanation only: applicable to repeated IDENTICAL cost vectors."""
    supply = vector(supply, len(cost))
    cost = vector(cost, len(supply))
    require(any(cost), "ZeroCost")
    return min(s // q for s, q in zip(supply, cost) if q > 0)


def work_identity(fields: dict) -> str:
    # Test-only JSON encoding, NOT the normative wire codec.
    semantic = {key: fields[key] for key in
                ("program", "weights", "input", "output", "trace", "conditions")}
    encoded = json.dumps(semantic, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(b"TEST/work/" + encoded).hexdigest()


def toy_work(number: int) -> dict:
    return dict(program="p", weights="w", input=number, output=number,
                trace=f"trace-{number}", conditions={"integer": True})


@dataclass(frozen=True)
class Header:
    operation: str = "bounded_dot"
    encoded_bytes: int = 8
    decoded_bytes: int = 16
    records: int = 1
    depth: int = 1
    terms: int = 4
    integer_bits: int = 32
    signatures: int = 1
    hash_bytes: int = 8
    io: int = 1
    parts: int = 1
    workspace: int = 8


@dataclass
class CommonChecker:
    """One checker/version/cost policy for all model IDs; no per-model table."""
    qualified: bool = True
    version: str = "synthetic-common-checker-v1"
    header_caps: dict = field(default_factory=lambda: dict(
        encoded_bytes=1_024, decoded_bytes=4_096, records=64, depth=8,
        terms=1_024, integer_bits=256, signatures=8, hash_bytes=4_096,
        io=1_024, parts=16, workspace=4_096))
    supported: tuple[str, ...] = ("bounded_dot", "bounded_reduce", "state_link", "terminal")

    def estimate(self, header: Header) -> tuple[tuple[int, ...], tuple[int, ...]]:
        require(self.qualified, "CommonProfileUnqualified")
        require(header.operation in self.supported, "UnsupportedKernel")
        # Fixed header inspection precedes expansion, graph fetch, and allocation.
        for name, cap in self.header_caps.items():
            require(checked(getattr(header, name)) <= cap, "ShapeBound")
        require(header.integer_bits > 0 and header.parts > 0, "ShapeBound")
        # Synthetic integer coefficients, NOT fitted CPU/WCET claims.
        work = checked(1 + 5 * header.signatures + header.hash_bytes
                       + header.decoded_bytes + 2 * header.terms + header.records)
        flow = (work, header.encoded_bytes, header.io, header.signatures, 1,
                header.hash_bytes, header.decoded_bytes, header.io, header.io, header.encoded_bytes)
        # Include input, expanded data, scratch and one explicit scratch copy.
        ram = checked(header.encoded_bytes + header.decoded_bytes + 2 * header.workspace)
        stock = (ram, header.encoded_bytes, 1)
        return flow, stock


@dataclass
class HierarchyMeter:
    """Atomic packet/case/original/block/global charging of the SAME work."""
    caps: dict[str, tuple[int, ...]]
    used: dict[str, tuple[int, ...]] = field(default_factory=dict)

    def charge(self, scopes: tuple[str, ...], cost: tuple[int, ...]) -> None:
        candidate = dict(self.used)
        for scope in scopes:
            require(scope in self.caps, "UnknownBudget")
            total = add(candidate.get(scope, (0,) * len(cost)), cost)
            require(fits(total, self.caps[scope]), "AggregateBudget")
            candidate[scope] = total
        self.used = candidate


@dataclass(frozen=True)
class ServiceWindow:
    start_ms: int
    end_ms: int
    supply: tuple[int, ...]
    kinds: tuple[str, ...] = ("normal", "negative", "control", "terminal")


@dataclass(frozen=True)
class StockHold:
    owner: str
    start_ms: int
    until_ms: int
    cost: tuple[int, ...]


@dataclass(frozen=True)
class Task:
    task_id: str
    original: str
    actor: str
    kind: str = "negative"
    release_ms: int = 0
    deadline_ms: int = CYCLE
    duration_ms: int = 1
    cost: tuple[int, ...] = (1,) * len(FLOW_NAMES)
    predecessors: tuple[str, ...] = ()

    def priority(self) -> tuple:
        return self.deadline_ms, self.release_ms, self.original, self.actor, self.task_id


@dataclass(frozen=True)
class Booking:
    task: Task
    window_index: int
    start_ms: int
    finish_ms: int
    generation: int


@dataclass
class ResourceBook:
    """Finite calendar; actual heterogeneous costs, no fixed court count.

    Jobs execute serially inside each service window in this conservative toy
    planner. Multiple jobs can share a window if time AND every flow fit.
    Production can use a different qualified bounded planner, but cannot infer
    deadline feasibility from a 120-second aggregate alone.
    """
    windows: tuple[ServiceWindow, ...]
    stock_caps: tuple[int, ...] = (10_000, 10_000, 10_000)
    bookings: dict[str, Booking] = field(default_factory=dict)
    holds: list[StockHold] = field(default_factory=list)
    generation: int = 1
    graph_cap: int = 512
    search_cap: int = 100_000

    def validate(self) -> None:
        previous_end = -1
        for window in self.windows:
            require(0 <= window.start_ms < window.end_ms, "InvalidWindow")
            require(window.start_ms >= previous_end, "OverlappingServiceWindows")
            vector(window.supply, len(FLOW_NAMES))
            previous_end = window.end_ms
        vector(self.stock_caps, len(STOCK_NAMES))

    def flow_used(self, index: int, bookings=None) -> tuple[int, ...]:
        total = (0,) * len(FLOW_NAMES)
        for booking in (self.bookings if bookings is None else bookings).values():
            if booking.window_index == index:
                total = add(total, booking.task.cost)
        return total

    def validate_stock(self, holds: list[StockHold]) -> None:
        endpoints = set()
        for hold in holds:
            require(0 <= hold.start_ms < hold.until_ms, "InvalidRetention")
            vector(hold.cost, len(STOCK_NAMES))
            endpoints.add(hold.start_ms)
            endpoints.add(hold.until_ms)
        for time in sorted(endpoints):
            used = (0,) * len(STOCK_NAMES)
            for hold in holds:
                if hold.start_ms <= time < hold.until_ms:
                    used = add(used, hold.cost)
            require(fits(used, self.stock_caps), "GlobalStockCap")

    @staticmethod
    def gap_start(window: ServiceWindow, occupied: list[Booking], task: Task,
                  pred_finish: int) -> int | None:
        cursor = max(window.start_ms, task.release_ms, pred_finish)
        for booking in sorted(occupied, key=lambda b: (b.start_ms, b.task.task_id)):
            if cursor + task.duration_ms <= booking.start_ms:
                break
            if cursor < booking.finish_ms:
                cursor = booking.finish_ms
        finish = checked(cursor + task.duration_ms)
        if cursor < window.end_ms and finish <= min(window.end_ms, task.deadline_ms):
            return cursor
        return None

    def plan(self, tasks: tuple[Task, ...], new_holds: tuple[StockHold, ...] = ()):
        self.validate()
        require(len(tasks) <= self.graph_cap, "PlannerGraphCap")
        pending = {task.task_id: task for task in tasks}
        require(len(pending) == len(tasks), "DuplicateTask")
        require(not (pending.keys() & self.bookings.keys()), "DuplicateTask")
        for task in tasks:
            vector(task.cost, len(FLOW_NAMES))
            require(task.cost[0] > 0, "ZeroCost")
            require(task.duration_ms > 0, "InvalidDuration")
            require(all(p in pending or p in self.bookings for p in task.predecessors),
                    "MissingPredecessor")
        candidate = dict(self.bookings)
        holds = list(self.holds) + list(new_holds)
        self.validate_stock(holds)
        searched = 0
        while pending:
            ready = [t for t in pending.values() if all(p in candidate for p in t.predecessors)]
            require(bool(ready), "CyclicGraph")
            task = min(ready, key=Task.priority)
            pred_finish = max((candidate[p].finish_ms for p in task.predecessors), default=0)
            placed = None
            for index, window in enumerate(self.windows):
                searched += 1
                require(searched <= self.search_cap, "PlannerSearchCap")
                if task.kind not in window.kinds:
                    continue
                if not fits(add(self.flow_used(index, candidate), task.cost), window.supply):
                    continue
                occupied = [b for b in candidate.values() if b.window_index == index]
                start = self.gap_start(window, occupied, task, pred_finish)
                if start is not None:
                    placed = Booking(task, index, start, start + task.duration_ms, self.generation)
                    break
            require(placed is not None, "NoProtectedCapacity")
            candidate[task.task_id] = placed
            del pending[task.task_id]
        return candidate, holds

    def reserve(self, tasks: tuple[Task, ...], holds: tuple[StockHold, ...] = ()) -> None:
        self.bookings, self.holds = self.plan(tasks, holds)

    def utilization(self, block_ms: int = CYCLE) -> int:
        """Every 120-second profile interval, all flow dimensions, peak stock."""
        self.validate()
        require(bool(self.windows), "IncompleteHorizon")
        start, end = self.windows[0].start_ms, self.windows[-1].end_ms
        require(start % block_ms == 0 and end % block_ms == 0, "IncompleteHorizon")
        values = []
        for block in range(start, end, block_ms):
            supply, demand = (0,) * len(FLOW_NAMES), (0,) * len(FLOW_NAMES)
            for index, window in enumerate(self.windows):
                if block <= window.start_ms and window.end_ms <= block + block_ms:
                    supply = add(supply, window.supply)
                    demand = add(demand, self.flow_used(index))
                else:
                    require(window.end_ms <= block or window.start_ms >= block + block_ms,
                            "CrossBlockSupply")
            values.extend(ratio1024(d, s) for d, s in zip(demand, supply))
        for time in {h.start_ms for h in self.holds}:
            used = (0,) * len(STOCK_NAMES)
            for hold in self.holds:
                if hold.start_ms <= time < hold.until_ms:
                    used = add(used, hold.cost)
            values.extend(ratio1024(d, s) for d, s in zip(used, self.stock_caps))
        return max(values, default=0)


def uniform_book(cycles: int = 10, supply: tuple[int, ...] = (100,) * len(FLOW_NAMES),
                 stock: tuple[int, ...] = (1_000, 1_000, 1_000)) -> ResourceBook:
    return ResourceBook(tuple(ServiceWindow(c * CYCLE, (c + 1) * CYCLE, supply)
                              for c in range(cycles)), stock_caps=stock)


@dataclass(frozen=True)
class Candidate:
    original: str
    model: str
    work: str
    tasks: tuple[Task, ...]
    hold: StockHold
    route: str = "TFO3"
    basis: int = 0
    funds: int = 10
    ticket_eligible: bool = True
    checker_version: str = "synthetic-common-checker-v1"


def candidate_for(name: str, model: str = "unseen-model", cycle: int = 0,
                  basis: int = 0, cost: int = 10) -> Candidate:
    begin = cycle * CYCLE
    tasks = (Task(f"{name}/normal", name, "producer", kind="normal",
                  release_ms=begin, deadline_ms=begin + CYCLE, cost=(cost,) * len(FLOW_NAMES)),
             Task(f"{name}/negative", name, "honest", release_ms=begin,
                  deadline_ms=begin + CYCLE, cost=(cost,) * len(FLOW_NAMES)))
    return Candidate(name, model, f"work/{name}", tasks,
                     StockHold(name, begin, begin + 2 * CYCLE, (1, 1, 1)), basis=basis)


@dataclass
class Protocol:
    book: ResourceBook = field(default_factory=uniform_book)
    checker: CommonChecker = field(default_factory=CommonChecker)
    free_funds: int = 100_000
    escrow: dict[str, int] = field(default_factory=dict)
    claims: dict[str, Candidate] = field(default_factory=dict)
    accepted_work: set[str] = field(default_factory=set)
    basis: int = 0
    local_ready: bool = True  # Local readiness is not a consensus difficulty input.
    unavailable: bool = False
    initial_difficulty: int = 8
    difficulty: int = 8
    low_run: int = 0
    target_d: dict[int, int] = field(default_factory=dict)
    winner_or_opportunity_state: dict = field(default_factory=dict)
    consumed_carriers: dict = field(default_factory=dict)

    def admit(self, candidate: Candidate, now: int = 0, canonical_epoch: int = 2) -> None:
        # Ordinary fixtures start after bootstrap; epoch is an authenticated
        # canonical input, not a submitted timestamp or a host stopwatch.
        checked(canonical_epoch)
        require(canonical_epoch >= 2, "BootstrapNoAdmission")
        require(self.local_ready, "LocalNotReady")
        require(candidate.route in ("Attempt", "FreePrompt", "TFO3", "Batch", "Native"),
                "UnknownNewRoute")
        require(self.checker.qualified and candidate.checker_version == self.checker.version
                and not self.unavailable, "CommonProfileUnqualified")
        require(candidate.ticket_eligible, "NotEligible")
        require(candidate.basis == self.basis, "StaleBasis")
        require(candidate.work not in self.accepted_work, "DuplicateWork")
        require(candidate.funds >= 0 and self.free_funds >= candidate.funds, "InsufficientFunds")
        require(all(t.original == candidate.original for t in candidate.tasks), "WrongOriginal")
        trial = copy.deepcopy(self)
        trial.free_funds -= candidate.funds
        # Stage funding first so the tests exercise rollback after a later planner refusal.
        trial.escrow[candidate.original] = candidate.funds
        tasks = tuple(replace(t, release_ms=max(now, t.release_ms)) for t in candidate.tasks)
        trial.book.reserve(tasks, (candidate.hold,))
        trial.claims[candidate.original] = candidate
        trial.accepted_work.add(candidate.work)
        trial.basis += 1
        self.__dict__.update(trial.__dict__)

    def snapshot(self) -> dict:
        return copy.deepcopy({key: value for key, value in self.__dict__.items() if key != "local_ready"})

    def restore(self, snapshot: dict) -> None:
        expected = set(self.__dict__) - {"local_ready"}
        if set(snapshot) != expected:
            self.local_ready = False
            raise Rejected("IncompleteRestore")
        self.__dict__.update(copy.deepcopy(snapshot))
        self.local_ready = True


def ranked_fit(protocol: Protocol, candidates: tuple[tuple[int, str, Candidate], ...],
               candidate_cap: int, visit_cap: int, now: int = 0,
               canonical_epoch: int = 2) -> tuple[list[str], list[tuple[str, str]]]:
    """One bounded pass over an already frozen set, with authenticated ticket inputs.

    The complete preseal/seed/wire protocol is not implemented here. This checks
    the deterministic fit behavior and refuses an oversized set before sorting.
    A failed expensive candidate does not reserve resources or stop smaller
    later candidates that fit. Each attempt uses the then-current prefix.
    """
    require(len(candidates) <= candidate_cap, "CandidateBudget")
    require(len(candidates) <= visit_cap, "VisitBudget")
    require(len({identity for _, identity, _ in candidates}) == len(candidates), "DuplicatePreseal")
    for ticket, _, _ in candidates:
        checked(ticket)
    accepted, refused = [], []
    for _, _, candidate in sorted(candidates, key=lambda entry: (entry[0], entry[1])):
        try:
            protocol.admit(replace(candidate, basis=protocol.basis), now, canonical_epoch)
            accepted.append(candidate.original)
        except Rejected as error:
            refused.append((candidate.original, error.code))
    return accepted, refused


@dataclass
class Lease:
    amount: int
    retain_until: int
    worker_done: bool = False
    io_done: bool = False
    canceled: bool = False


@dataclass
class LocalAllocator:
    capacity: int
    leases: dict[str, Lease] = field(default_factory=dict)
    decode_calls: int = 0
    ingress_charges: int = 0

    @property
    def used(self) -> int:
        return sum(lease.amount for lease in self.leases.values())

    def acquire(self, token: str, amount: int, retain_until: int) -> None:
        require(token not in self.leases, "DuplicateLease")
        require(checked(self.used + checked(amount)) <= self.capacity, "LocalBackpressure")
        self.leases[token] = Lease(amount, retain_until)

    def receive(self, checker: CommonChecker, header: Header, token: str,
                authenticated: bool = True) -> bool:
        self.ingress_charges += 1
        _, stock = checker.estimate(header)
        # Process-wide maximum decode credit is acquired before authentication/decode.
        self.acquire(token, stock[0], retain_until=200)
        if not authenticated:
            unused = self.leases[token]
            unused.worker_done, unused.io_done, unused.retain_until = True, True, 0
            self.release(token, 0)
            return False
        self.decode_calls += 1
        return True

    def cancel_or_reorg(self, token: str) -> None:
        # Canonical reservation undo does NOT prove a physical worker has stopped.
        self.leases[token].canceled = True

    def release(self, token: str, now: int) -> bool:
        if token not in self.leases:
            return False
        lease = self.leases[token]
        if not (lease.worker_done and lease.io_done and now >= lease.retain_until):
            return False
        del self.leases[token]
        return True


@dataclass
class ProtectedCourt:
    book: ResourceBook
    consumed: set[str] = field(default_factory=set)
    ingress_charges: int = 0
    notices: set[tuple[str, str]] = field(default_factory=set)
    evidence_rejected: set[str] = field(default_factory=set)
    terminal: set[str] = field(default_factory=set)

    def submit(self, task_id: str, actor: str, now: int, *, authenticated: bool = True,
               shape_valid: bool = True, arithmetic_valid: bool = True,
               generation: int = 1) -> bool:
        booking = self.book.bookings.get(task_id)
        if not authenticated or not shape_valid or booking is None or actor != booking.task.actor:
            self.ingress_charges += 1
            return False
        require(generation == booking.generation, "StaleGeneration")
        if task_id in self.consumed:
            return False
        require(booking.start_ms <= now < booking.finish_ms, "WrongWindow")
        require(all(p in self.consumed for p in booking.task.predecessors), "PredecessorIncomplete")
        require(booking.task.original not in self.terminal, "OriginalTerminal")
        self.consumed.add(task_id)
        if not arithmetic_valid:
            # Failed evidence spends the authorized execution, but does not
            # establish a native false-challenge terminal or remove any Q.
            self.evidence_rejected.add(task_id)
        return True

    def notice(self, original: str, actor: str, now: int) -> None:
        checked(now)
        require(any(b.task.original == original and b.task.actor == actor
                    for b in self.book.bookings.values()), "NoProtectedActor")
        self.notices.add((original, actor))

    def release(self, original: str, reason: str) -> None:
        require(reason == "terminal" and original in self.terminal, "ReservationStillLive")
        self.book.bookings = {key: b for key, b in self.book.bookings.items()
                              if b.task.original != original}
        # Holds have independent retention; terminal is not an unconditional release.


def difficulty_step(difficulty: int, high: bool, low: bool, low_run: int,
                    unavailable: bool = False) -> tuple[int, int]:
    """Integer update primitive; resource-pressure predicates are separate."""
    require(1 <= difficulty <= D_MAX, "DifficultyRange")
    require(0 <= low_run <= 2, "ControllerState")
    if unavailable:
        return difficulty, 0
    if high:
        return min(D_MAX, ceil_div(checked(9 * difficulty), 8)), 0
    if low:
        low_run += 1
        if low_run == 3:
            return max(1, checked(8 * difficulty) // 9), 0
        return difficulty, low_run
    return difficulty, 0


def pressure_predicates(utilization: int, demand: bool, congested: bool,
                        unavailable: bool = False) -> tuple[bool, bool]:
    checked(utilization)
    high = demand and (utilization >= 768 or congested)
    low = utilization <= 256 and not congested and not unavailable
    return high, low


def apply_difficulty_boundary(protocol: Protocol, boundary: int, high: bool, low: bool) -> None:
    # An incompletely restored host abstains from computing a transition; it
    # does not publish its local recovery state as canonical unavailability.
    require(protocol.local_ready, "LocalNotReady")
    require(boundary >= 0, "WrongUpdateBoundary")
    target = boundary + 2
    require(target not in protocol.target_d, "FrozenTarget")
    if boundary == 0:
        require(1 <= protocol.initial_difficulty <= D_MAX, "DifficultyRange")
        # There is no invented epoch -1 pressure and no inherited low counter.
        d, run = protocol.initial_difficulty, 0
    else:
        require(boundary + 1 in protocol.target_d, "MissingPublishedDifficulty")
        previous = protocol.target_d[boundary + 1]
        d, run = difficulty_step(previous, high, low, protocol.low_run, protocol.unavailable)
    protocol.target_d[target] = d
    protocol.difficulty, protocol.low_run = d, run


@dataclass
class CarrierBook:
    network: str = "test-network"
    policy: int = 1
    live_slots: int = FIXTURE_LIVE
    spent: dict[tuple[str, int, int], str] = field(default_factory=dict)

    def consume(self, slot: int, now: int, carrier_id: str, kind: str,
                inner_claim_accepted: bool = True) -> bool:
        checked(slot)
        checked(now)
        require(self.live_slots > 0, "InvalidCarrierProfile")
        token = (self.network, self.policy, slot)
        if token in self.spent:
            require(self.spent[token] == carrier_id, "PermitConflict")
            return False
        release = checked(slot * PERIOD)
        require(release <= now < checked(release + self.live_slots * PERIOD), "PermitExpiredOrFuture")
        self.spent[token] = carrier_id
        return True


@dataclass
class Outcome:
    # Projection of one unit authorized by the EXISTING economic contract.
    # It imposes no one-claim/one-block or one-right-per-epoch rule.
    original: str
    normal: bool = False
    independent: bool = False
    opportunity_complete: bool = False
    funds_dependencies_ok: bool = True
    q: bool = False
    refuted: bool = False
    final: bool = False
    expiry: int = 10 * CYCLE
    right: str = "PendingMint"
    minted_once: bool = False
    economic_issue_allowed: bool = False
    credit: int = 0
    spent_credit: int = 0

    def eligible(self, now: int) -> bool:
        return (self.normal and self.independent and self.opportunity_complete
                and self.funds_dependencies_ok and not self.q and not self.refuted and now < self.expiry)


def apply_final_batch(claim: Outcome, events: tuple[str, ...], now: int = 0,
                      issue_reserved: bool = True, use_reserved: bool = True) -> None:
    # Inputs assume native refutation qualification and pre-reserved issue/use budgets.
    if "notice" in events:
        claim.q = True
    if "refute" in events:
        claim.refuted, claim.q = True, True
        if claim.right != "Consumed":
            claim.right = "Revoked"
        claim.credit = 0
    if "normal" in events:
        claim.normal = True
    if "final" in events and claim.eligible(now):
        claim.final = True
    if (claim.final and claim.eligible(now) and claim.economic_issue_allowed
            and issue_reserved and not claim.minted_once):
        claim.right, claim.minted_once = "Available", True
    if "use_right" in events and claim.final and claim.eligible(now) and use_reserved:
        if claim.right == "Available":
            claim.right = "Consumed"
    if "spend_credit" in events and claim.final and claim.eligible(now):
        claim.spent_credit += claim.credit
        claim.credit = 0


@dataclass
class ReleaseLedger:
    original_funds: int = 100
    spent: int = 20
    locked: int = 80
    refund: int = 0
    authorized_burn: int = 0
    terminal: str | None = None
    material_until: int = 200
    funds_until: int = 250
    journal_until: int = 300
    material_released: bool = False
    funds_released: bool = False
    journal_released: bool = False

    def release(self, now: int, service_complete: bool, settled: bool) -> None:
        if self.terminal is None:
            return
        if now >= self.material_until and service_complete:
            self.material_released = True
        if now >= self.funds_until and service_complete and settled and not self.funds_released:
            self.refund += self.locked
            self.locked = 0
            self.funds_released = True
        if now >= self.journal_until and settled:
            self.journal_released = True

    def conserved(self) -> bool:
        return self.original_funds == self.spent + self.locked + self.refund + self.authorized_burn


class ContractTests(unittest.TestCase):
    """30 semantic tests; all quantities in fixtures are nonnormative."""
    def rejects(self, code: str, call, *args, **kwargs):
        with self.assertRaises(Rejected) as raised:
            call(*args, **kwargs)
        self.assertEqual(raised.exception.code, code)

    def test_01_small_evidence_more_jobs_heavy_evidence_fewer_jobs(self):
        """Same supply accepts 10 small or 2 heavy operations; neither count is policy."""
        for cost, expected in ((10, 10), (40, 2)):
            book = uniform_book(cycles=1)
            jobs = tuple(Task(f"t{n}", "o", f"a{n}", cost=(cost,) * len(FLOW_NAMES)) for n in range(expected))
            book.reserve(jobs)
            self.assertEqual(len(book.bookings), expected)
            self.rejects("NoProtectedCapacity", book.reserve,
                         (Task("extra", "o", "a", cost=(cost,) * len(FLOW_NAMES)),))
            self.assertEqual(len(book.bookings), expected)

    def test_02_heterogeneous_vectors_require_actual_sums(self):
        """CPU-light bytes-heavy and bytes-light CPU-heavy cannot use count-only admission."""
        book = uniform_book(cycles=1, supply=(10,) * len(FLOW_NAMES))
        a = Task("a", "o", "a", cost=(6, 2) + (1,) * (len(FLOW_NAMES) - 2))
        b = Task("b", "o", "b", cost=(2, 6) + (1,) * (len(FLOW_NAMES) - 2))
        book.reserve((a, b))
        self.assertEqual(book.flow_used(0), (8, 8) + (2,) * (len(FLOW_NAMES) - 2))
        self.rejects("NoProtectedCapacity", book.reserve,
                     (Task("c", "o", "c", cost=(1, 3) + (1,) * (len(FLOW_NAMES) - 2)),))
        self.rejects("NoProtectedCapacity", book.reserve,
                     (Task("d", "o", "d", cost=(3, 1) + (1,) * (len(FLOW_NAMES) - 2)),))
        self.assertEqual(len(book.bookings), 2)

    def test_03_identical_count_formula_is_only_an_explanatory_special_case(self):
        """min floor(S/q) works for identical q, not arbitrary case counts."""
        self.assertEqual(identical_count_bound((100, 90, 80), (10, 30, 20)), 3)
        self.assertEqual(identical_count_bound((100, 90, 80), (40, 10, 20)), 2)
        self.assertEqual(identical_count_bound((0, 20), (0, 5)), 4)
        self.rejects("ZeroCost", identical_count_bound, (10, 10), (0, 0))

    def test_04_unseen_ten_thousand_and_fifty_thousand_models_do_not_expand_supply(self):
        """Registry/model names are not capacity multipliers or qualification entries."""
        checker = CommonChecker()
        baseline = checker.estimate(Header())
        for count in (10_000, 50_000):
            # Each ID is unseen; the checker API deliberately has no model whitelist.
            identities = (f"unseen-model-{n}" for n in range(count))
            seen = 0
            for _ in identities:
                self.assertEqual(checker.estimate(Header()), baseline)
                seen += 1
            self.assertEqual(seen, count)
            protocol = Protocol(book=uniform_book(cycles=1))
            for n in range(5):
                protocol.admit(candidate_for(f"c{n}", model=f"unseen-model-{n}", basis=n))
            before = protocol.snapshot()
            self.rejects("NoProtectedCapacity", protocol.admit,
                         candidate_for("overflow", model=f"unseen-model-{count}", basis=5))
            self.assertEqual(protocol.snapshot(), before)
            self.assertEqual(len(protocol.claims), 5)

    def test_05_only_common_checker_qualification_and_kernel_support_are_required(self):
        """An unsupported kernel/version is refused; an unseen model name is accepted."""
        protocol = Protocol()
        protocol.admit(candidate_for("new", model="invented-during-test"))
        self.assertIn("new", protocol.claims)
        self.assertNotIn("model_approvals", protocol.__dict__)
        self.rejects("UnsupportedKernel", protocol.checker.estimate, Header(operation="arbitrary_python"))
        protocol.checker.qualified = False
        self.rejects("CommonProfileUnqualified", protocol.admit,
                     candidate_for("next", basis=1))
        self.rejects("CommonProfileUnqualified", CommonChecker(qualified=False).estimate, Header())

    def test_06_shape_and_expansion_bounds_precede_allocation_and_decode(self):
        """Oversized compressed expansion/depth/terms never reach decode or allocation."""
        allocator, checker = LocalAllocator(10_000), CommonChecker()
        for field_name in ("encoded_bytes", "decoded_bytes", "records", "depth", "terms",
                           "integer_bits", "signatures", "hash_bytes", "io", "parts", "workspace"):
            malformed = replace(Header(), **{field_name: checker.header_caps[field_name] + 1})
            self.rejects("ShapeBound", allocator.receive, checker, malformed, field_name)
        self.assertEqual(allocator.decode_calls, 0)
        self.assertEqual(allocator.used, 0)
        self.assertEqual(allocator.ingress_charges, 11)

    def test_07_local_allocation_token_is_acquired_before_decode(self):
        """Header-valid packets still backpressure if process-wide RAM is unavailable."""
        checker = CommonChecker()
        ram = checker.estimate(Header())[1][0]
        allocator = LocalAllocator(ram)
        self.assertTrue(allocator.receive(checker, Header(), "first"))
        self.rejects("LocalBackpressure", allocator.receive, checker, Header(), "second")
        self.assertEqual(allocator.decode_calls, 1)
        self.assertEqual(allocator.used, ram)
        self.rejects("LocalBackpressure", allocator.receive, checker, Header(), "forged",
                     authenticated=False)
        self.assertEqual(set(allocator.leases), {"first"})
        fresh = LocalAllocator(ram)
        self.assertFalse(fresh.receive(checker, Header(), "forged", authenticated=False))
        self.assertEqual((fresh.used, fresh.decode_calls), (0, 0))

    def test_08_each_flow_and_stock_dimension_has_an_independent_limit(self):
        """Work cannot compensate for bytes/I/O/signatures/control/RAM/storage/metadata."""
        for index, name in enumerate(FLOW_NAMES):
            with self.subTest(flow=name):
                cost = [1] * len(FLOW_NAMES)
                cost[index] = 101
                book = uniform_book(cycles=1)
                self.rejects("NoProtectedCapacity", book.reserve,
                             (Task("x", "o", "a", cost=tuple(cost)),))
                self.assertEqual(book.bookings, {})
        for index, name in enumerate(STOCK_NAMES):
            with self.subTest(stock=name):
                book = uniform_book(stock=(10, 10, 10))
                cost = [1] * len(STOCK_NAMES)
                cost[index] = 11
                self.rejects("GlobalStockCap", book.reserve, (),
                             (StockHold("o", 0, 100, tuple(cost)),))
                self.assertEqual(book.holds, [])

    def test_09_packet_case_original_block_global_budget_charge_is_atomic(self):
        """Splitting into packets/cases does not reset the original/global work budget."""
        scopes = ("packet-a", "case", "original", "block", "global")
        meter = HierarchyMeter({key: (10,) * len(FLOW_NAMES) for key in scopes})
        meter.charge(scopes, (6, 2) + (1,) * (len(FLOW_NAMES) - 2))
        before = copy.deepcopy(meter.used)
        meter.caps["packet-b"] = (10,) * len(FLOW_NAMES)
        self.rejects("AggregateBudget", meter.charge,
                     ("packet-b", "case", "original", "block", "global"), (5, 1) + (1,) * (len(FLOW_NAMES) - 2))
        self.assertEqual(meter.used, before)
        self.assertNotIn("packet-b", meter.used)

    def test_10_multi_step_case_reserves_all_steps_and_dependency_order(self):
        """One outer case/object does not compress three heavy checks into one cost."""
        tasks = tuple(Task(f"step{n}", "o", "actor", cost=(10,) * len(FLOW_NAMES),
                           predecessors=(() if n == 0 else (f"step{n-1}",))) for n in range(3))
        book = uniform_book()
        book.reserve(tasks)
        self.assertEqual(book.flow_used(0), (30,) * len(FLOW_NAMES))
        for n in range(1, 3):
            self.assertGreaterEqual(book.bookings[f"step{n}"].start_ms,
                                    book.bookings[f"step{n-1}"].finish_ms)
        too_small = uniform_book(cycles=1, supply=(20,) * len(FLOW_NAMES))
        self.rejects("NoProtectedCapacity", too_small.reserve, tasks)
        self.assertEqual(too_small.bookings, {})

    def test_11_aggregate_120_second_fit_does_not_prove_deadline_feasibility(self):
        """Two jobs fit the period budget but cannot both execute in its final millisecond."""
        tasks = tuple(Task(f"t{n}", "o", f"a{n}", release_ms=CYCLE - 1,
                           deadline_ms=CYCLE, duration_ms=1, cost=(1,) * len(FLOW_NAMES)) for n in range(2))
        book = uniform_book(cycles=1)
        self.rejects("NoProtectedCapacity", book.reserve, tasks)
        self.assertEqual(book.bookings, {})
        book.reserve((tasks[0],))
        self.assertEqual(book.bookings["t0"].finish_ms, CYCLE)
        self.rejects("NoProtectedCapacity", uniform_book(cycles=1).reserve,
                     (replace(tasks[0], release_ms=CYCLE),))

    def test_12_seventy_five_at_three_is_a_nonnormative_capacity_example(self):
        """Illustrative profile: 3 legal jobs/cycle needs 25 cycles; policy fixes neither 75 nor 3."""
        windows = tuple(ServiceWindow(c * CYCLE, (c + 1) * CYCLE, (3,) * len(FLOW_NAMES)) for c in range(25))
        tasks = tuple(Task(f"t{n:03d}", "o", f"actor{n:03d}", deadline_ms=25 * CYCLE,
                           duration_ms=40_000, cost=(1,) * len(FLOW_NAMES)) for n in range(75))
        book = ResourceBook(windows)
        book.reserve(tasks)
        self.assertEqual(len(book.bookings), 75)
        self.assertEqual(max(b.finish_ms for b in book.bookings.values()), 25 * CYCLE)
        self.rejects("NoProtectedCapacity", ResourceBook(windows[:24]).reserve, tasks)
        # A different safe supply/timing fixture supports five, with no code/policy change.
        five = ResourceBook((ServiceWindow(0, CYCLE, (5,) * len(FLOW_NAMES)),))
        five.reserve(tuple(replace(t, duration_ms=24_000, deadline_ms=CYCLE) for t in tasks[:5]))
        self.assertEqual(len(five.bookings), 5)

    def test_13_graph_bounds_cycles_and_ready_order_are_deterministic(self):
        """Planner workspace/search is finite; graph defects leave no partial reservation."""
        book = uniform_book()
        self.rejects("MissingPredecessor", book.reserve,
                     (Task("x", "o", "a", predecessors=("missing",)),))
        cyclic = (Task("x", "o", "a", predecessors=("y",)),
                  Task("y", "o", "b", predecessors=("x",)))
        self.rejects("CyclicGraph", book.reserve, cyclic)
        self.assertEqual(book.bookings, {})
        book.graph_cap = 1
        self.rejects("PlannerGraphCap", book.reserve, cyclic)
        book.graph_cap, book.search_cap = 512, 0
        self.rejects("PlannerSearchCap", book.reserve, (Task("x", "o", "a"),))
        tasks = (Task("b", "o", "b"), Task("a", "o", "a"),
                 Task("c", "o", "c", predecessors=("a",)))
        plans = [uniform_book().plan(tuple(order))[0] for order in itertools.permutations(tasks)]
        self.assertTrue(all(plan == plans[0] for plan in plans))

    def test_14_admission_rolls_back_claim_money_all_resources_and_quote(self):
        """A failed future continuation cannot leave a funded half-installed original."""
        protocol = Protocol(book=uniform_book(cycles=1, supply=(25,) * len(FLOW_NAMES)))
        old = Task("old", "old", "old-actor", cost=(10,) * len(FLOW_NAMES))
        protocol.book.reserve((old,))
        before = protocol.snapshot()
        self.rejects("NoProtectedCapacity", protocol.admit, candidate_for("new"))
        self.assertEqual(protocol.snapshot(), before)
        self.assertEqual(protocol.book.bookings["old"].task.actor, "old-actor")
        self.rejects("StaleBasis", protocol.admit, candidate_for("stale", basis=1))
        unfunded = Protocol(free_funds=0)
        self.rejects("InsufficientFunds", unfunded.admit, candidate_for("no-funds"))
        self.assertEqual(unfunded.claims, {})

    def test_15_three_invalid_packets_cannot_spend_honest_actor_reservation(self):
        """Ingress-invalid cannot steal capacity; executed rejected evidence does not clear other duties."""
        book = uniform_book()
        book.reserve((Task("honest/task", "o", "honest", duration_ms=100),))
        court = ProtectedCourt(book)
        before = dict(book.bookings)
        for _ in range(3):
            self.assertFalse(court.submit("honest/task", "attacker", 0, authenticated=False))
        self.assertEqual(court.ingress_charges, 3)
        self.assertEqual(court.consumed, set())
        self.assertEqual(book.bookings, before)
        court.notice("o", "honest", 0)
        notice_before = set(court.notices)
        self.assertTrue(court.submit("honest/task", "honest", 0, arithmetic_valid=False))
        self.assertEqual(court.evidence_rejected, {"honest/task"})
        self.assertEqual(court.notices, notice_before)
        self.assertEqual(court.terminal, set())
        self.assertFalse(court.submit("honest/task", "honest", 1))

    def test_16_old_actor_rights_are_not_cut_when_new_claims_are_backpressured(self):
        """No new difficulty/credit and no first-quarter-only notice gate for old obligations."""
        protocol = Protocol(book=uniform_book(cycles=1, supply=(20,) * len(FLOW_NAMES)))
        protocol.admit(candidate_for("old"))
        protocol.unavailable = True
        self.rejects("CommonProfileUnqualified", protocol.admit, candidate_for("new", basis=1))
        court = ProtectedCourt(protocol.book)
        for time in (0, 30_000, 90_000, CYCLE - 1):
            court.notice("old", "honest", time)
        self.assertIn(("old", "honest"), court.notices)
        booking = protocol.book.bookings["old/negative"]
        self.assertFalse(court.submit("old/negative", "producer", booking.start_ms))
        self.rejects("StaleGeneration", court.submit, "old/negative", "honest",
                     booking.start_ms, generation=0)
        self.assertTrue(court.submit("old/negative", "honest", booking.start_ms))

    def test_17_stock_is_reserved_across_retention_not_only_at_execution(self):
        """Non-overlap at execution does not allow overlapping retained material to exceed stock."""
        book = uniform_book(stock=(10, 10, 10))
        book.reserve((), (StockHold("a", 0, 200, (6, 6, 1)),))
        before = copy.deepcopy(book.holds)
        self.rejects("GlobalStockCap", book.reserve, (), (StockHold("b", 100, 300, (5, 5, 1)),))
        self.assertEqual(book.holds, before)
        book.reserve((), (StockHold("b", 200, 300, (5, 5, 1)),))
        self.assertEqual(len(book.holds), 2)
        self.rejects("InvalidRetention", book.reserve, (), (StockHold("bad", 300, 300, (1, 1, 1)),))

    def test_18_local_token_survives_cancel_reorg_worker_and_io_lifetimes(self):
        """Canonical undo is not physical memory reclamation; tokens release exactly once."""
        allocator = LocalAllocator(100)
        allocator.acquire("worker", 100, retain_until=200)
        allocator.cancel_or_reorg("worker")
        self.assertFalse(allocator.release("worker", 300))
        self.rejects("LocalBackpressure", allocator.acquire, "new", 1, 400)
        allocator.leases["worker"].worker_done = True
        self.assertFalse(allocator.release("worker", 300))
        allocator.leases["worker"].io_done = True
        self.assertFalse(allocator.release("worker", 199))
        self.assertTrue(allocator.release("worker", 200))
        self.assertFalse(allocator.release("worker", 999))
        self.assertEqual(allocator.used, 0)
        allocator.acquire("new", 100, 400)
        self.assertEqual(allocator.used, 100)

    def test_19_empty_mempool_cannot_release_future_actor_capacity(self):
        """Only an actual terminal/legal expiry/replan may remove a protected continuation."""
        book = uniform_book(cycles=20)
        task = Task("future", "o", "a", release_ms=10 * CYCLE, deadline_ms=11 * CYCLE)
        hold = StockHold("o", 0, 12 * CYCLE, (1, 1, 1))
        book.reserve((task,), (hold,))
        court = ProtectedCourt(book)
        self.rejects("ReservationStillLive", court.release, "o", "empty-mempool")
        self.rejects("ReservationStillLive", court.release, "o", "terminal")
        self.assertIn("future", book.bookings)
        court.terminal.add("o")
        court.release("o", "terminal")
        self.assertEqual(book.bookings, {})
        self.assertEqual(book.holds, [hold])

    def test_20_resource_utilization_includes_every_future_120_second_interval(self):
        """Per-120s denominators are profile supplies; old ten-cycle averaging cannot hide a burst."""
        # Illustrative supply 10/epoch: one unit is ceil(1024/10)=103.
        normal = uniform_book(supply=(10,) * len(FLOW_NAMES))
        normal.reserve((Task("one", "o", "a"),))
        self.assertEqual(normal.utilization(), 103)
        # Illustrative supply 30/epoch: one unit is ceil(1024/30)=35.
        other = uniform_book(supply=(30,) * len(FLOW_NAMES))
        other.reserve((Task("one", "o", "a"),))
        self.assertEqual(other.utilization(), 35)
        future = uniform_book(cycles=30, supply=(10,) * len(FLOW_NAMES))
        future.reserve(tuple(Task(f"t{n}", "o", "a", release_ms=20 * CYCLE,
                                  deadline_ms=21 * CYCLE) for n in range(8)))
        self.assertEqual(future.utilization(), 820)
        self.assertEqual(future.utilization(block_ms=10 * CYCLE), 82)
        peak_stock = uniform_book(stock=(10, 10, 10))
        peak_stock.reserve((), (StockHold("future-stock", 8 * CYCLE, 9 * CYCLE, (8, 1, 1)),))
        self.assertEqual(peak_stock.utilization(), 820)
        self.assertEqual(ratio1024(0, 0), 0)
        self.rejects("UnavailableSupply", ratio1024, 1, 0)
        self.assertEqual(uniform_book(cycles=25).utilization(), 0)
        partial = ResourceBook((ServiceWindow(0, CYCLE - 1, (10,) * len(FLOW_NAMES)),))
        self.rejects("IncompleteHorizon", partial.utilization)

    def test_21_difficulty_integer_high_low_unavailable_and_bounds(self):
        """The controller never relaxes hard resources; exact integer updates are reproducible."""
        self.assertEqual(difficulty_step(8, True, False, 0), (9, 0))
        self.assertEqual(difficulty_step(9, True, False, 0), (11, 0))
        self.assertEqual(difficulty_step(D_MAX, True, False, 0), (D_MAX, 0))
        d, run = 10, 0
        for expected in (1, 2):
            d, run = difficulty_step(d, False, True, run)
            self.assertEqual((d, run), (10, expected))
        self.assertEqual(difficulty_step(d, False, True, run), (8, 0))
        self.assertEqual(difficulty_step(1, False, True, 2), (1, 0))
        self.assertEqual(difficulty_step(10, True, False, 2, True), (10, 0))
        self.assertEqual(difficulty_step(10, False, False, 2), (10, 0))
        self.rejects("DifficultyRange", difficulty_step, 0, False, True, 0)
        self.rejects("IntegerRange", checked, 1 << 256)
        protocol = Protocol(book=uniform_book(cycles=1, supply=(0,) * len(FLOW_NAMES)), difficulty=1)
        self.rejects("NoProtectedCapacity", protocol.admit, candidate_for("easy-ticket"))
        self.assertEqual(pressure_predicates(768, True, False), (True, False))
        self.assertEqual(pressure_predicates(767, True, False), (False, False))
        self.assertEqual(pressure_predicates(1_024, False, False), (False, False))
        self.assertEqual(pressure_predicates(0, False, False), (False, True))
        self.assertEqual(pressure_predicates(0, True, True), (True, False))
        self.assertEqual(pressure_predicates(256, True, False), (False, True))
        self.assertEqual(pressure_predicates(257, True, False), (False, False))
        self.assertEqual(pressure_predicates(0, False, False, True), (False, False))

    def test_22_difficulty_application_does_not_rewrite_frozen_target(self):
        """Bootstrap has no epoch -1 or epoch 0/1 admission; later D uses published D_(b+1)."""
        protocol = Protocol(low_run=2)
        apply_difficulty_boundary(protocol, 0, True, False)
        self.assertNotIn(0, protocol.target_d)
        self.assertNotIn(1, protocol.target_d)
        self.assertEqual(protocol.target_d, {2: 8})
        self.assertEqual((protocol.difficulty, protocol.low_run), (8, 0))
        candidate = candidate_for("boot")
        before = protocol.snapshot()
        for epoch in (0, 1):
            for route in ("Attempt", "FreePrompt", "TFO3", "Batch", "Native"):
                self.rejects("BootstrapNoAdmission", protocol.admit,
                             replace(candidate, route=route), epoch * CYCLE, canonical_epoch=epoch)
        self.assertEqual(protocol.snapshot(), before)
        old_book = uniform_book()
        old_book.reserve((Task("old/remedy", "old", "honest", duration_ms=100),))
        self.assertTrue(ProtectedCourt(old_book).submit("old/remedy", "honest", 0))
        apply_difficulty_boundary(protocol, 1, True, False)
        self.assertEqual(protocol.target_d, {2: 8, 3: 9})
        apply_difficulty_boundary(protocol, 2, True, False)
        self.assertEqual(protocol.target_d, {2: 8, 3: 9, 4: 11})
        protocol.admit(candidate_for("first-new", cycle=2), now=2 * CYCLE, canonical_epoch=2)
        self.assertEqual(set(protocol.claims), {"first-new"})
        self.rejects("FrozenTarget", apply_difficulty_boundary, protocol, 1, False, True)
        self.rejects("WrongUpdateBoundary", apply_difficulty_boundary, protocol, -1, False, True)
        self.rejects("MissingPublishedDifficulty", apply_difficulty_boundary, Protocol(), 1, True, False)
        self.rejects("DifficultyRange", apply_difficulty_boundary,
                     Protocol(initial_difficulty=0), 0, False, False)

    def test_23_shared_eligibility_and_semantic_identity_no_alias_or_route_reset(self):
        """Common difficulty authorization is separate from actual resource acceptance."""
        protocol = Protocol()
        self.rejects("NotEligible", protocol.admit,
                     replace(candidate_for("bad"), ticket_eligible=False))
        original = toy_work(1)
        renamed = dict(original, alias="another", signature="changed", nonce=42, carrier="different")
        self.assertEqual(work_identity(original), work_identity(renamed))
        first = candidate_for("a")
        protocol.admit(first)
        self.rejects("DuplicateWork", protocol.admit,
                     replace(candidate_for("b", basis=1), work=first.work, route="Batch"))
        self.rejects("UnknownNewRoute", protocol.admit,
                     replace(candidate_for("c", basis=1), route="LegacyDowngrade"))

    def test_24_all_carrier_kinds_share_permit_and_inner_rejection_never_refunds(self):
        """Outer carrier 1 BPS is distinct from dynamic inner claim/court counts."""
        book = CarrierBook()
        self.assertTrue(book.consume(0, 0, "one", "EXEC", inner_claim_accepted=False))
        for kind in ("REAL", "BASE", "Heartbeat", "court-only"):
            self.rejects("PermitConflict", book.consume, 0, 1, f"other-{kind}", kind)
        self.assertFalse(book.consume(0, 99_000, "one", "EXEC"))
        self.assertEqual(len(book.spent), 1)

    def test_25_carrier_profile_live_width_expiry_and_no_catchup(self):
        """Different live permits permit DAG concurrency; L is a profile value, not a court-count constant."""
        book = CarrierBook()
        for slot in range(4):
            self.assertTrue(book.consume(slot, 3_999, f"parallel-{slot}", "BASE"))
        empty = CarrierBook()
        self.rejects("PermitExpiredOrFuture", empty.consume, 0, 4_000, "expired", "BASE")
        self.rejects("PermitExpiredOrFuture", empty.consume, 4, 3_999, "future", "BASE")
        self.rejects("PermitExpiredOrFuture", empty.consume, 5, 100_000, "old", "court-only")
        for slot in range(97, 101):
            empty.consume(slot, 100_000, f"restart-{slot}", "court-only")
        self.assertEqual(len(empty.spent), 4)
        for width in (1, 2, 7):
            profiled = CarrierBook(live_slots=width)
            for slot in range(101 - width, 101):
                profiled.consume(slot, 100_000, f"live-{slot}", "BASE")
            self.assertEqual(len(profiled.spent), width)
            self.rejects("PermitExpiredOrFuture", profiled.consume, 100 - width,
                         100_000, "expired", "BASE")

    def test_26_carrier_burst_ceil_bound_for_integer_ms_half_open_intervals(self):
        """Five consumes fit in 999 ms; the conservative bound needs ceil, not floor."""
        book = CarrierBook()
        for slot in range(4):
            book.consume(slot, 3_002, f"c{slot}", "BASE")
        book.consume(4, 4_000, "c4", "BASE")
        duration = 4_001 - 3_002
        self.assertGreater(len(book.spent), book.live_slots + duration // PERIOD)
        self.assertLessEqual(len(book.spent), book.live_slots + ceil_div(duration, PERIOD))
        for start in range(0, 10_001, 137):
            for width in (1, 2, 999, 1_000, 1_001, 3_999, 4_000, 10_001):
                end = start + width
                possible = sum(slot * PERIOD < end and slot * PERIOD + book.live_slots * PERIOD > start
                               for slot in range((end - 1) // PERIOD + 1))
                self.assertLessEqual(possible, book.live_slots + ceil_div(width, PERIOD))

    def test_27_same_batch_refute_precedes_final_right_and_credit(self):
        """All 120 event orders forbid invalid success; prior lawful consumption is not reminted."""
        for events in itertools.permutations(("normal", "final", "use_right", "spend_credit", "refute")):
            claim = Outcome("o", independent=True, opportunity_complete=True, credit=7,
                            economic_issue_allowed=True)
            apply_final_batch(claim, events)
            self.assertTrue(claim.refuted)
            self.assertFalse(claim.final or claim.minted_once)
            self.assertEqual((claim.credit, claim.spent_credit, claim.right), (0, 0, "Revoked"))
        local = Outcome("local")
        apply_final_batch(local, ("normal", "final"))
        self.assertFalse(local.final)
        no_new_right = Outcome("not-authorized-by-old-economics", normal=True,
                               independent=True, opportunity_complete=True)
        apply_final_batch(no_new_right, ("final",))
        self.assertTrue(no_new_right.final)
        self.assertFalse(no_new_right.minted_once)
        good = Outcome("good", normal=True, independent=True, opportunity_complete=True, credit=3,
                       economic_issue_allowed=True)
        apply_final_batch(good, ("final", "use_right", "spend_credit"))
        self.assertEqual((good.right, good.spent_credit), ("Consumed", 3))
        apply_final_batch(good, ("refute",))
        self.assertEqual((good.right, good.spent_credit), ("Consumed", 3))
        self.assertTrue(good.minted_once)
        self.assertEqual(good.credit, 0)

    def test_28_terminal_release_is_finite_independent_once_and_conserved(self):
        """Unverified termination is not guilt and does not erase remaining storage/settlement."""
        ledger = ReleaseLedger(terminal="AbortedUnverified")
        ledger.release(100, False, False)
        self.assertEqual(ledger.locked, 80)
        self.assertFalse(ledger.material_released or ledger.journal_released)
        ledger.release(200, True, False)
        self.assertTrue(ledger.material_released)
        self.assertFalse(ledger.funds_released or ledger.journal_released)
        ledger.release(250, True, True)
        self.assertEqual((ledger.locked, ledger.refund), (0, 80))
        self.assertFalse(ledger.journal_released)
        ledger.release(300, True, True)
        before = copy.deepcopy(ledger)
        ledger.release(10_000, True, True)
        self.assertEqual(ledger, before)
        self.assertTrue(ledger.conserved())
        self.assertTrue(ledger.journal_released)
        self.assertEqual(ledger.authorized_burn, 0)

    def test_29_reorg_snapshot_restores_all_canonical_resources_and_no_empty_book_fallback(self):
        """State restore covers money, D, opportunity, identity, permit and full reservation together."""
        protocol = Protocol()
        before = protocol.snapshot()
        protocol.admit(candidate_for("o"))
        protocol.winner_or_opportunity_state[0] = "frozen"
        protocol.consumed_carriers[0] = "carrier"
        apply_difficulty_boundary(protocol, 0, True, False)
        apply_difficulty_boundary(protocol, 1, True, False)
        self.assertNotEqual(protocol.snapshot(), before)
        protocol.restore(before)
        self.assertEqual(protocol.snapshot(), before)
        protocol.book.reserve((Task("old", "old", "a"),))
        book_before = copy.deepcopy(protocol.book)
        incomplete = protocol.snapshot()
        del incomplete["book"]
        self.rejects("IncompleteRestore", protocol.restore, incomplete)
        self.assertFalse(protocol.local_ready)
        self.assertFalse(protocol.unavailable)
        self.assertEqual(protocol.book, book_before)
        self.rejects("LocalNotReady", apply_difficulty_boundary, protocol, 0, False, True)
        self.assertEqual(protocol.target_d, {})

    def test_30_parallel_prefixes_and_multiple_admissions_cannot_double_reserve(self):
        """Finite ranked fit allows multiple claims; stale siblings cannot spend residual capacity twice."""
        protocol = Protocol(book=uniform_book(cycles=1, supply=(40,) * len(FLOW_NAMES)))
        a, b = candidate_for("a"), candidate_for("b")
        protocol.admit(a)
        self.rejects("StaleBasis", protocol.admit, b)
        protocol.admit(replace(b, basis=1))
        self.assertEqual(len(protocol.claims), 2)
        self.assertEqual(protocol.book.flow_used(0), (40,) * len(FLOW_NAMES))
        before = protocol.snapshot()
        self.rejects("NoProtectedCapacity", protocol.admit, candidate_for("c", basis=2))
        self.assertEqual(protocol.snapshot(), before)
        frozen = ((2, "preseal-b", candidate_for("b", cost=10)),
                  (0, "preseal-heavy", candidate_for("heavy", cost=25)),
                  (1, "preseal-a", candidate_for("a", cost=10)))
        outcomes = []
        for order in itertools.permutations(frozen):
            state = Protocol(book=uniform_book(cycles=1, supply=(40,) * len(FLOW_NAMES)))
            outcomes.append(ranked_fit(state, tuple(order), 3, 3))
            self.assertEqual(state.book.flow_used(0), (40,) * len(FLOW_NAMES))
        self.assertTrue(all(outcome == outcomes[0] for outcome in outcomes))
        self.assertEqual(outcomes[0], (["a", "b"], [("heavy", "NoProtectedCapacity")]))
        empty = Protocol()
        self.rejects("CandidateBudget", ranked_fit, empty, frozen, 2, 3)
        self.rejects("VisitBudget", ranked_fit, empty, frozen, 3, 2)
        self.assertEqual(empty.claims, {})


class JsonResults(unittest.TextTestResult):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.records = []

    def record(self, test, outcome, detail=None):
        row = dict(id=test.id().split(".")[-1], description=test.shortDescription(), outcome=outcome)
        if detail:
            row["detail"] = detail
        self.records.append(row)

    def addSuccess(self, test):
        super().addSuccess(test)
        self.record(test, "passed")

    def addFailure(self, test, err):
        super().addFailure(test, err)
        self.record(test, "failed", self._exc_info_to_string(err, test))

    def addError(self, test, err):
        super().addError(test, err)
        self.record(test, "error", self._exc_info_to_string(err, test))

    def addSkip(self, test, reason):
        super().addSkip(test, reason)
        self.record(test, "skipped", reason)

    def addSubTest(self, test, subtest, err):
        super().addSubTest(test, subtest, err)
        if err is not None:
            self.record(test, "failed_subtest", self._exc_info_to_string(err, test))


def main() -> int:
    here = Path(__file__).resolve()
    root = here.parents[4]
    rfc = root / "docs/rfc/0055-global-admission-court-calendar-and-one-bps.md"
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(ContractTests)
    result = unittest.TextTestRunner(verbosity=2, resultclass=JsonResults).run(suite)
    report = {
        "artifact": "RFC-0055 dynamic-resource abstract contract model",
        "scope": "ABSTRACT MODEL ONLY — not implementation, E2E, hardware qualification, or formal proof",
        "policy_revision": "User clarification: no normative fixed claim/court counts; common checker plus resource vectors",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "base_source_sha": "c3da7c6eaf5bf87f8e032d5b122c603d720a8398",
        "rfc_path": str(rfc.relative_to(root)),
        "rfc_sha256": hashlib.sha256(rfc.read_bytes()).hexdigest(),
        "script_sha256": hashlib.sha256(here.read_bytes()).hexdigest(),
        "counts": {"run": result.testsRun, "passed": sum(row["outcome"] == "passed" for row in result.records),
                   "failed": len(result.failures), "errors": len(result.errors), "skipped": len(result.skipped)},
        "all_passed": result.wasSuccessful(),
        "tests": result.records,
        "assumptions": [
            "All supply, cost, timing, 75/3, 1/10 and 1/30 quantities are NONNORMATIVE SYNTHETIC FIXTURES; no measured hardware capacity is asserted.",
            "A single common checker/version is qualified; there is no per-model approval table or per-model performance test.",
            "Complete obligation graphs, trusted native cost derivation, authentication, canonical order and actual profile qualification are external inputs.",
            "ClockBridge has already derived conservative integer-ms releases and inclusive completion deadlines; canonical epoch inputs are authenticated and ordinary fixtures start after bootstrap.",
            "This conservative serial toy planner checks window-level flow sums, concrete deadlines and retention stock; no optimality claim is made.",
            "Difficulty tests check resource-pressure predicates, bootstrap D2/low_run, empty epoch 0/1 admission, published D_(b+1), and target b+2 timing; full canonical derivation of demand/congestion is not implemented.",
            "Carrier tests check permit supply/consumption only, not body staging, latency, schedule authentication, or wall-clock synchronization.",
            "Finality projects one unit authorized by the existing economic contract, with admissible court results and reserved issue/use capacity; no one-claim/one-right rule or full P_m/credit economics is added.",
            "Identity uses test-only JSON encoding; it is not a wire/cryptography equivalence test.",
            "Authenticated snapshot correctness is assumed. Deep copies are abstract transactions, not a proposed scalable implementation."
        ],
        "not_demonstrated": [
            "Production native/Rust/Studio/RPC wiring, implementation equivalence or actual Node RSS/CPU/I/O behavior.",
            "Real LLM integer execution, full independent Panel verification, complete local falsifiability, or one-honest material availability.",
            "Panel cold download/full rerun/provider capacity and its separate resource reservations; native EvidenceRejected/MaterialPending/LocalPredicateHolds classification and complete Q lifecycle.",
            "Minimum hardware, actual mixed-load WCET/service guarantees, sustainable real-time 1 BPS, or 10k/50k registry performance.",
            "All IBD/reorg/restart/GC/WAL cases, actual asynchronous allocator behavior, physical transport, or cryptographic seed security.",
            "Successful CapacityProfile activation, formal safety proof, fairness, or full liveness."
        ]
    }
    output = here.with_name("model-results.json")
    output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"model_only": True, "result": str(output), **report["counts"],
                      "all_passed": report["all_passed"]}, ensure_ascii=False))
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
