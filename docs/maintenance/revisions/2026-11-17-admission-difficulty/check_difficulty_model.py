#!/usr/bin/env python3
"""RFC-0056 abstract admission contract model; NOT native/E2E/performance evidence.

This executable specification connects paid canonical preseals, one-use funded
trial slots, freeze/seed/readiness, SHA-256 tickets, a persistent ranked cursor,
qualified observations, and the delayed integer difficulty schedule.  It uses
synthetic finite capacities and an intentionally small RFC-0055 reservation
adapter.  Native authentication, native inner-ID codecs, cost derivation, full
actor/deadline planning, beacon security, actual carrier scheduling, Panel work,
node memory/I/O, and economic Final/credit issuance are external assumptions.

No model-name qualification table, cryptographic computation proof, hardware
claim, mainnet activation, 50,000-model performance claim, or closed-loop
stability claim is provided.  Deep copies and Python containers are test-only.
Run from any directory; only model-results.json is written by default.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import struct
import sys
import unittest
from dataclasses import dataclass, field, replace
from datetime import datetime, timezone
from pathlib import Path


EPOCH_MS = 120_000
U256_MAX = (1 << 256) - 1
D_MAX = (1 << 120) - 1
FLOW_NAMES = (
    "deterministic_work", "signature_checks", "hash_bytes",
    "parse_and_expand_bytes", "wire_bytes", "io_read_bytes", "io_write_bytes",
    "io_operations", "control_operations", "carrier_payload_space",
)
N_FLOW = len(FLOW_NAMES)
FIELDS = (
    "work", "producer", "bond", "model", "program", "weights", "input",
    "conditions", "output", "trace", "shape", "funding", "consent", "deadline",
    "route-semantics",
)
CONNECTED_NEW_ROUTES = frozenset((
    "Attempt", "FreePrompt", "non-TIR", "TFP1", "TFO1", "TFO2", "evaluation", "Batch",
))
H = lambda value: hashlib.sha256(value).digest()


class Rejected(Exception):
    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


def require(condition: bool, code: str) -> None:
    if not condition:
        raise Rejected(code)


def uint(value: int, bits: int = 256) -> int:
    require(type(value) is int and 0 <= value < 1 << bits, "IntegerRange")
    return value


def le(value: int, width: int) -> bytes:
    return uint(value, 8 * width).to_bytes(width, "little")


def lp(value: bytes, limit: int = 4096) -> bytes:
    require(type(value) is bytes and len(value) <= limit, "LPBound")
    return le(len(value), 4) + value


def nr(tag: str, native: bytes, version: int = 1) -> bytes:
    require(tag in FIELDS + ("capital-outpoint",), "NativeTag")
    require(version == 1, "NativeVersion")
    # Version 1 is a SYNTHETIC policy fixture, not an allocated production codec.
    return lp(tag.encode("ascii"), 64) + le(version, 2) + lp(native)


def digest32(value: bytes) -> bytes:
    require(type(value) is bytes and len(value) == 32, "DigestWidth")
    return value


def slot_key(network: bytes, generation: int, capital: bytes, sequence: int) -> bytes:
    return H(lp(b"MISAKA/PALW/CAD1/SLOT") + lp(network, 64) + le(generation, 8)
             + nr("capital-outpoint", capital) + le(sequence, 8))


def fields_encoding(network: bytes, policy: bytes, target: int, slot: bytes,
                    native: tuple[tuple[str, bytes], ...]) -> bytes:
    require(tuple(tag for tag, _ in native) == FIELDS, "FieldOrderOrCount")
    encoded = (le(1, 2) + lp(network, 64) + digest32(policy) + le(target, 8)
               + digest32(slot) + b"".join(nr(tag, data) for tag, data in native))
    require(len(encoded) <= 65_536, "PresealTotalBound")
    return encoded


def decode_fields(encoded: bytes) -> tuple[bytes, bytes, int, bytes, tuple[tuple[str, bytes], ...]]:
    """Strict bounded outer CAD1 codec; inner native bytes remain opaque fixtures."""
    require(type(encoded) is bytes and len(encoded) <= 65_536, "PresealTotalBound")
    at = 0

    def take(size: int) -> bytes:
        nonlocal at
        require(0 <= size <= len(encoded) - at, "TruncatedField")
        result = encoded[at:at + size]
        at += size
        return result

    def number(size: int) -> int:
        return int.from_bytes(take(size), "little")

    def variable(limit: int) -> bytes:
        size = number(4)
        require(size <= limit, "LPBound")  # Before slicing/allocation of content.
        return take(size)

    require(number(2) == 1, "PresealVersion")
    network, policy, target, slot = variable(64), take(32), number(8), take(32)
    values = []
    for expected in FIELDS:
        require(variable(64) == expected.encode("ascii"), "FieldOrderOrCount")
        require(number(2) == 1, "NativeVersion")
        values.append((expected, variable(4096)))
    require(at == len(encoded), "TrailingField")
    return network, policy, target, slot, tuple(values)


def preseal_id(encoded: bytes) -> bytes:
    decode_fields(encoded)
    return H(lp(b"MISAKA/PALW/CAD1/PRESEAL") + encoded)


def ticket_value(network: bytes, policy: bytes, target: int,
                 identity: bytes, seed: bytes) -> int:
    subject = (lp(b"MISAKA/PALW/CAD1/TICKET") + lp(network, 64) + digest32(policy)
               + le(target, 8) + digest32(identity) + digest32(seed))
    return int.from_bytes(H(subject), "big")


def threshold(difficulty: int) -> int:
    require(type(difficulty) is int and 1 <= difficulty <= D_MAX, "DifficultyRange")
    return U256_MAX // difficulty


def eligible(ticket: int, difficulty: int) -> bool:
    return uint(ticket) <= threshold(difficulty)


def vector(values: tuple[int, ...]) -> tuple[int, ...]:
    require(len(values) == N_FLOW, "VectorDimension")
    return tuple(uint(value) for value in values)


def vv(number: int) -> tuple[int, ...]:
    return (uint(number),) * N_FLOW


def add(left: tuple[int, ...], right: tuple[int, ...]) -> tuple[int, ...]:
    return tuple(uint(a + b) for a, b in zip(vector(left), vector(right)))


def subtract(left: tuple[int, ...], right: tuple[int, ...]) -> tuple[int, ...]:
    return tuple(uint(a - b) for a, b in zip(vector(left), vector(right)))


def ratio(numerator: int, denominator: int) -> int:
    uint(numerator)
    uint(denominator)
    if denominator == 0:
        require(numerator == 0, "UnavailableSupply")
        return 0
    require(numerator <= denominator, "CapacityInconsistency")
    return uint(uint(1024 * numerator) + denominator - 1) // denominator


@dataclass(frozen=True)
class Profile:
    # Synthetic units; no measured or recommended hardware values.
    total_flow: tuple[int, ...] = vv(120)
    fixed_flow: tuple[int, ...] = vv(20)
    total_stock: int = 120
    fixed_stock: int = 20
    outer_permits_per_epoch: int = 120  # Separate from carrier_payload_space.
    preseal_control_capacity: int = 10_000
    preseal_stock_capacity: int = 1_000
    control_per_preseal: int = 10
    visit_cost: int = 2
    batch_work_capacity: int = 10_000
    quote_work_capacity_per_preseal: int = 4
    ready_guard_ms: int = 30_000  # Synthetic fixture, not a qualified production guard.
    max_wire: int = 65_536
    max_expanded: int = 131_072
    horizon: int = 10
    qualified: bool = True

    @property
    def supply(self) -> tuple[int, ...]:
        return subtract(self.total_flow, self.fixed_flow)

    @property
    def stock_supply(self) -> int:
        return uint(self.total_stock - self.fixed_stock)

    def validate(self) -> None:
        self.supply
        self.stock_supply
        require(self.control_per_preseal > 0 and self.visit_cost > 0, "ZeroControlCost")
        require(self.horizon > 0, "EmptyHorizon")
        require(0 < self.ready_guard_ms < EPOCH_MS, "InvalidReadyGuard")


@dataclass(frozen=True)
class Shape:
    # Authenticated common-checker outputs are assumed; NOT user cost declarations
    # in a production implementation and NOT a full RFC-0055 actor graph planner.
    flow: tuple[int, ...] = vv(10)
    stock: int = 1
    duration_ms: int = 1
    future_flow: tuple[int, ...] = vv(0)
    kernel: str = "common-bounded-v1"

    def encoded(self) -> bytes:
        return (lp(self.kernel.encode("ascii"), 64)
                + b"".join(le(n, 8) for n in vector(self.flow))
                + le(self.stock, 8) + le(self.duration_ms, 8)
                + b"".join(le(n, 8) for n in vector(self.future_flow)))


@dataclass(frozen=True)
class Submission:
    capital: bytes
    sequence: int
    target: int = 2
    model: bytes = b"unseen-native-model-id"
    work: bytes = b"existing-native-work-id"
    input_data: bytes = b"same-prompt"
    payload: bytes = b"result"
    shape: Shape = field(default_factory=Shape)
    original_deadline: int = 3 * EPOCH_MS
    signature_expiry: int = 3 * EPOCH_MS
    fee_limit: int = 100
    lock_limit: int = 10_000
    authenticated: bool = True
    route_qualified: bool = True
    enabled: bool = True
    computationally_correct: bool = True  # Deliberately NEVER used by the gate.
    output_commit: bytes = field(init=False)
    trace_commit: bytes = field(init=False)
    declared_payload_bytes: int = field(init=False)

    def __post_init__(self):
        # PRODUCER fixture preparation, outside AdmissionModel. The node gate
        # reads only these compact commits/declared bounds before eligibility.
        # Keeping payload on this test object is not a production storage design.
        object.__setattr__(self, "output_commit", H(self.payload))
        object.__setattr__(self, "trace_commit", H(b"fixture-trace/" + self.payload))
        object.__setattr__(self, "declared_payload_bytes", len(self.payload))

    def native_fields(self) -> tuple[tuple[str, bytes], ...]:
        body = {
            "work": self.work,
            "producer": b"fixture-producer-native-id",
            "bond": b"fixture-existing-bond-reference",
            "model": self.model,
            "program": b"fixture-common-program-native-id",
            "weights": b"fixture-native-weights-id",
            "input": self.input_data,
            "conditions": b"deterministic-integer-fixture",
            "output": self.output_commit,
            "trace": self.trace_commit,
            "shape": self.shape.encoded(),
            "funding": nr("capital-outpoint", self.capital) + le(self.sequence, 8)
                       + le(self.fee_limit, 16) + le(self.lock_limit, 16),
            "consent": b"fixture-explicit-paid-scope-v1",
            "deadline": le(self.original_deadline, 8) + le(self.signature_expiry, 8),
            "route-semantics": b"fixture-equivalent-original-semantics-v1",
        }
        return tuple((tag, body[tag]) for tag in FIELDS)


@dataclass
class Capital:
    available: int
    authorized_through: int = 10_000
    highwater: int = 0
    locked_by: bytes | None = None


@dataclass
class Record:
    submission: Submission
    slot: bytes
    identity: bytes
    lock: int
    ticket: int | None = None
    disposition: str | None = None
    ready: bool = False
    settled: bool = False
    refunded: int = 0
    quote_work_spent: int = 0
    quote_bases: set[int] = field(default_factory=set)


@dataclass
class Target:
    epoch: int
    difficulty: int
    records: list[bytes] = field(default_factory=list)
    closed_root: bytes | None = None
    anchor: int | None = None
    seed: bytes | None = None
    seed_ms: int | None = None
    ready_root: bytes | None = None
    ranked: list[bytes] = field(default_factory=list)
    cursor: int = 0
    control_reserved: int = 0
    visits: int = 0
    state: str = "Collecting"
    coverage: bool = True
    unavailable: bool = False


@dataclass
class Reservation:
    identity: bytes
    start: int
    flow: dict[int, tuple[int, ...]]
    stock: int
    retain_until: int


@dataclass(frozen=True)
class Observation:
    epoch: int
    cut_ms: int
    qualified_demand: bool
    known_congestion: bool
    complete: bool
    unavailable: bool
    future: int
    spent: int
    stock: int
    pending_count: int
    disposition_counts: tuple[tuple[str, int], ...]

    @property
    def utilization(self) -> int:
        return max(self.future, self.spent, self.stock)


def difficulty_update(previous: int, low_run: int, observation: Observation) -> tuple[int, int]:
    threshold(previous)
    require(type(low_run) is int and 0 <= low_run < 3, "LowRunState")
    high = observation.qualified_demand and (observation.utilization >= 768
                                            or observation.known_congestion)
    low = (observation.complete and observation.utilization <= 256
           and not observation.known_congestion and not observation.unavailable)
    if observation.unavailable:
        return previous, 0
    if high:
        return min(D_MAX, uint(uint(9 * previous) + 7) // 8), 0
    if low:
        low_run += 1
        if low_run == 3:
            return max(1, uint(8 * previous) // 9), 0
        return previous, low_run
    return previous, 0


@dataclass
class LocalIngress:
    capacity: int = 4
    active: int = 0
    received: int = 0
    decode_calls: int = 0

    def receive(self, profile: Profile, wire: int, expanded: int, authenticated: bool) -> None:
        uint(wire)
        uint(expanded)
        require(wire <= profile.max_wire and expanded <= profile.max_expanded, "HeaderBound")
        require(self.active < self.capacity, "IngressLeaseUnavailable")
        self.active += 1
        try:
            self.received += 1
            # Authentication rejection does not touch any consensus pressure state.
            require(authenticated, "InvalidAuthentication")
            self.decode_calls += 1
        finally:
            self.active -= 1


class AdmissionModel:
    """Finite synthetic state machine, not a replacement for native consensus."""

    def __init__(self, difficulty: int = 1, profile: Profile | None = None):
        threshold(difficulty)
        self.profile = profile or Profile()
        self.profile.validate()
        self.network = b"fixture-net"
        self.policy = H(b"synthetic-CAD1-policy-with-frozen-common-codecs")
        self.generation = 1
        self.epoch = 0
        self.last_boundary = 0
        self.low_run = 0
        self.difficulty = {2: difficulty}
        self.targets = {2: Target(2, difficulty)}
        self.records: dict[bytes, Record] = {}
        self.slot_uses: dict[bytes, bytes] = {}
        # Original economic identity deliberately excludes CAD1 policy generation.
        self.economic_uses: set[tuple[bytes, bytes, int]] = set()
        self.capital_origins: dict[bytes, bytes] = {}
        self.capital: dict[bytes, Capital] = {}
        self.reservations: dict[bytes, Reservation] = {}
        self.original_escrow: dict[bytes, int] = {}
        self.claims: set[bytes] = set()
        self.protected: dict[bytes, set[str]] = {}
        self.observations: dict[int, Observation] = {}
        self.flow_spent: dict[int, tuple[int, ...]] = {}
        self.stock_peak: dict[int, int] = {}
        self.canonical_unavailable = False
        self.revoked_slots: set[bytes] = set()
        self.invalid_funding_slots: set[bytes] = set()
        self.planner_fault_slots: set[bytes] = set()
        self.ingress = LocalIngress()
        self.local_ready = True
        self.fees = 0
        self.initial_money = 0
        self.heavy_calls = 0
        self.decode_heavy_calls = 0
        self.batch_receipts: dict[str, tuple[int, tuple[bytes, ...], tuple[str, ...]]] = {}
        self.connected_routes = set(CONNECTED_NEW_ROUTES)

    def add_capital(self, identity: bytes, amount: int = 10_000) -> None:
        require(identity not in self.capital, "DuplicateCapital")
        uint(amount)
        self.capital[identity] = Capital(amount)
        self.capital_origins[identity] = identity
        self.initial_money += amount

    def canonical_snapshot(self) -> dict:
        return copy.deepcopy({key: value for key, value in vars(self).items()
                              if key not in ("local_ready", "ingress")})

    def restore(self, snapshot: dict) -> None:
        expected = set(vars(self)) - {"local_ready", "ingress"}
        if set(snapshot) != expected:
            self.local_ready = False
            raise Rejected("IncompleteSnapshot")
        for key, value in copy.deepcopy(snapshot).items():
            setattr(self, key, value)
        self.local_ready = True

    def money_total(self) -> int:
        return (sum(c.available for c in self.capital.values())
                + sum(r.lock for r in self.records.values())
                + sum(self.original_escrow.values()) + self.fees)

    def _slot_and_id(self, submission: Submission) -> tuple[bytes, bytes]:
        slot = slot_key(self.network, self.generation, submission.capital, submission.sequence)
        encoded = fields_encoding(self.network, self.policy, submission.target,
                                  slot, submission.native_fields())
        return slot, preseal_id(encoded)

    def submit(self, submission: Submission, route: str = "Attempt",
               signature: bytes = b"signature-a") -> bytes:
        require(self.local_ready, "LocalNotReady")
        require(route in self.connected_routes, "UnconnectedNewRoute")
        require(submission.enabled, "DisabledFeature")
        require(submission.authenticated and bool(signature), "InvalidAuthentication")
        require(submission.route_qualified, "InvalidQualification")
        slot, identity = self._slot_and_id(submission)
        if slot in self.slot_uses:
            require(self.slot_uses[slot] == identity, "TrialAlreadyBound")
            require(identity in self.records, "PrunedTrial")
            return identity
        require(submission.target == self.epoch + 2, "CollectionWindow")
        target = self.targets[submission.target]
        require(target.state == "Collecting", "FrozenCollection")
        require(self.profile.qualified and not self.canonical_unavailable, "UnavailableProfile")
        require(submission.capital in self.capital, "UnknownCapital")
        capital = self.capital[submission.capital]
        economic_key = (self.network, self.capital_origins[submission.capital], submission.sequence)
        require(economic_key not in self.economic_uses, "EconomicOpportunityAlreadyUsed")
        require(capital.highwater < submission.sequence <= capital.authorized_through,
                "ConsumedOrUnauthorizedSequence")
        require(capital.locked_by is None, "CapitalAlreadyLocked")
        require(submission.shape.kernel == "common-bounded-v1", "UnsupportedShape")
        require(any(submission.shape.flow), "ZeroClaimCost")
        require(submission.declared_payload_bytes <= self.profile.max_expanded, "HeaderBound")
        # Only simple bounded shape/amount handling before the unknown-seed gate.
        entry_fee = 2
        quote_count = self.profile.quote_work_capacity_per_preseal // self.profile.visit_cost
        claim_lock = uint(sum(vector(submission.shape.flow)) + submission.shape.stock + 1 + quote_count)
        require(submission.fee_limit >= entry_fee + 1 + quote_count and submission.lock_limit >= claim_lock,
                "FundingScope")
        require(capital.available >= entry_fee + claim_lock, "InsufficientFunds")
        control = uint(target.control_reserved + self.profile.control_per_preseal)
        require(control <= self.profile.preseal_control_capacity, "PresealControlCapacity")
        require(len(self.records) < self.profile.preseal_stock_capacity, "PresealStockCapacity")
        # Commit begins only after all checks, with no partial slot/fee/capital state.
        capital.available -= entry_fee + claim_lock
        capital.highwater = submission.sequence
        capital.locked_by = identity
        self.fees += entry_fee
        self.slot_uses[slot] = identity
        self.economic_uses.add(economic_key)
        self.records[identity] = Record(submission, slot, identity, claim_lock)
        target.records.append(identity)
        target.control_reserved = control
        return identity

    def provide_seed(self, epoch: int, anchor: int, beacon: bytes,
                     now_ms: int | None = None) -> None:
        require(self.local_ready, "LocalNotReady")
        target = self.targets[epoch]
        require(self.epoch == epoch - 1 and target.state == "Frozen", "SeedWindow")
        require(target.seed is None and anchor == epoch - 1, "SeedAnchorOrReuse")
        now = (epoch - 1) * EPOCH_MS + 1 if now_ms is None else uint(now_ms, 64)
        require(now > (epoch - 1) * EPOCH_MS, "SeedNotAfterFreeze")
        if now >= epoch * EPOCH_MS - self.profile.ready_guard_ms:
            target.unavailable = True
            raise Rejected("SeedCutMissed")
        require(not target.unavailable, "SeedCutMissed")
        digest32(beacon)
        # A deterministic TEST derivation: actual beacon/anchor contract is external.
        target.seed = H(b"TEST/seed/" + lp(self.network) + self.policy
                        + target.closed_root + le(epoch, 8) + le(anchor, 8) + beacon)
        target.anchor = anchor
        target.seed_ms = now
        for identity in target.records:  # Finite and fully control-reserved at collection.
            self.records[identity].ticket = ticket_value(self.network, self.policy, epoch,
                                                        identity, target.seed)

    def stage(self, identity: bytes, payload: bytes, now_ms: int | None = None) -> None:
        record = self.records[identity]
        target = self.targets[record.submission.target]
        now = (target.seed_ms or self.epoch * EPOCH_MS) + 1 if now_ms is None else uint(now_ms, 64)
        require(self.epoch == target.epoch - 1 and target.state == "Frozen"
                and (target.epoch - 1) * EPOCH_MS <= now < target.epoch * EPOCH_MS,
                "ReadinessWindow")
        require(target.seed is not None and eligible(record.ticket, target.difficulty), "Ineligible")
        require(now >= target.seed_ms and not target.unavailable, "ReadinessWindow")
        require(len(payload) <= self.profile.max_expanded, "HeaderBound")
        require(H(payload) == record.submission.output_commit, "CommitMismatch")
        require(now < record.submission.signature_expiry, "SignatureExpired")
        record.ready = True

    def _flow(self, epoch: int) -> tuple[int, ...]:
        total = vv(0)
        for reservation in self.reservations.values():
            total = add(total, reservation.flow.get(epoch, vv(0)))
        return total

    def _stock(self, epoch: int) -> int:
        return uint(sum(r.stock for r in self.reservations.values()
                        if r.start <= epoch < r.retain_until))

    def _future_ratio(self, first_epoch: int) -> int:
        values = [0]
        for epoch in range(first_epoch, first_epoch + self.profile.horizon):
            values.extend(ratio(n, c) for n, c in zip(self._flow(epoch), self.profile.supply))
            values.append(ratio(self._stock(epoch), self.profile.stock_supply))
        return max(values)

    def _fits(self, record: Record, empty: bool) -> bool:
        shape, epoch = record.submission.shape, record.submission.target
        for point, cost in ((epoch, shape.flow), (epoch + 1, shape.future_flow)):
            used = vv(0) if empty else add(self._flow(point), self.flow_spent.get(point, vv(0)))
            if any(n > cap for n, cap in zip(add(used, cost), self.profile.supply)):
                return False
        for point in (epoch, epoch + 1):
            used_stock = 0 if empty else self._stock(point)
            if used_stock + shape.stock > self.profile.stock_supply:
                return False
        return True

    def _visit(self, identity: bytes) -> str:
        record = self.records[identity]
        submission = record.submission
        target = self.targets[submission.target]
        if record.disposition is not None:
            return record.disposition
        require(record.ticket is not None, "MissingTicket")
        if not eligible(record.ticket, target.difficulty):
            return "Ineligible"
        if not record.ready:
            return "MissingMaterial"
        if record.slot in self.revoked_slots or not submission.route_qualified:
            return "InvalidQualification"
        if record.slot in self.invalid_funding_slots:
            return "InsufficientFunds"
        if submission.shape.kernel != "common-bounded-v1":
            return "UnsupportedShape"
        self.heavy_calls += 1
        self.decode_heavy_calls += 1
        require(record.slot not in self.planner_fault_slots, "PlannerIncomplete")
        if (self.epoch * EPOCH_MS + submission.shape.duration_ms > submission.original_deadline
                or self.epoch * EPOCH_MS >= submission.signature_expiry):
            return "IntrinsicDeadlineFailure"
        if not self._fits(record, empty=True):
            return "IntrinsicShapeFailure"
        if not self._fits(record, empty=False):
            return "NoCapacity"
        shape = submission.shape
        self.reservations[identity] = Reservation(
            identity, self.epoch, {self.epoch: shape.flow, self.epoch + 1: shape.future_flow},
            shape.stock, self.epoch + 2)
        self.stock_peak[self.epoch] = max(self.stock_peak.get(self.epoch, 0), self._stock(self.epoch))
        self.original_escrow[identity] = record.lock
        record.lock = 0
        self.claims.add(identity)
        self.protected[identity] = {"honest-actor"}
        return "Accepted"

    def advance(self, batch: str, identities: tuple[bytes, ...],
                generation: int | None = None) -> tuple[str, ...]:
        require(self.local_ready, "LocalNotReady")
        require(generation is None or generation == self.generation, "StaleGeneration")
        if batch in self.batch_receipts:
            epoch, expected, results = self.batch_receipts[batch]
            require(epoch == self.epoch and identities == expected, "BatchConflict")
            return results
        require(self.epoch >= 2, "BootstrapNoAdmission")
        target = self.targets[self.epoch]
        require(target.state == "Active" and not target.unavailable
                and not self.canonical_unavailable and self.profile.qualified, "NewAdmissionUnavailable")
        require(identities and identities == tuple(target.ranked[target.cursor:target.cursor + len(identities)]),
                "NotNextPrefix")
        require(len(identities) * self.profile.visit_cost <= self.profile.batch_work_capacity,
                "BatchWorkBudget")
        # Whole-batch atomicity includes funds, book, outcomes, cursor, and cost debits.
        trial = copy.deepcopy(self)
        next_target = trial.targets[trial.epoch]
        results = []
        for identity in identities:
            result = trial._visit(identity)
            record = trial.records[identity]
            if record.disposition is None:
                require(record.lock > 0 or identity in trial.original_escrow, "MissingFeeReserve")
                if record.lock > 0:
                    record.lock -= 1
                else:
                    trial.original_escrow[identity] -= 1
                trial.fees += 1
            record.disposition = result
            results.append(result)
            next_target.cursor += 1
            next_target.visits += 1
        receipt = tuple(results)
        trial.batch_receipts[batch] = (trial.epoch, identities, receipt)
        self.__dict__.update(trial.__dict__)
        return receipt

    def process_all(self, batch: str = "all") -> tuple[str, ...]:
        target = self.targets[self.epoch]
        remaining = tuple(target.ranked[target.cursor:])
        return self.advance(batch, remaining) if remaining else ()

    def cancel(self, identity: bytes) -> None:
        record = self.records[identity]
        require(record.disposition != "Accepted", "AcceptedResponsibilitiesRemain")
        if record.disposition is None:
            record.disposition = "Canceled"

    def settle(self, identity: bytes) -> int:
        record = self.records[identity]
        if record.settled:
            return 0
        require(record.disposition is not None and record.disposition != "Accepted", "NotSettlementReady")
        require(self.epoch >= record.submission.target + 1, "SettlementNotReleased")
        refund = record.lock
        self.capital[record.submission.capital].available += refund
        self.capital[record.submission.capital].locked_by = None
        record.lock = 0
        record.settled = True
        record.refunded = refund
        return refund

    def prune(self, identity: bytes) -> None:
        record = self.records[identity]
        require(record.settled and self.epoch >= record.submission.target + 3, "RetentionStillLive")
        # Highwater and authenticated slot/nullifier summary survive record removal.
        del self.records[identity]

    def consume_claim_work(self, identity: bytes) -> None:
        reservation = self.reservations[identity]
        cost = reservation.flow.pop(self.epoch, vv(0))
        self.flow_spent[self.epoch] = add(self.flow_spent.get(self.epoch, vv(0)), cost)

    def terminal_release(self, identity: bytes, native_terminal: bool = False) -> None:
        require(native_terminal, "UnprovenTerminal")
        # This authorization is a trusted fixture, not native court/Final evidence.
        del self.reservations[identity]

    def remedy(self, identity: bytes, actor: str, enlarges_original: bool = False) -> bool:
        require(identity in self.claims and actor in self.protected.get(identity, set()), "UnknownProtectedOriginal")
        require(not enlarges_original, "NewResponsibility")
        return True  # Original reservation assumed; no new D/seed/credit test.

    def quote(self, identity: bytes | None, detailed: bool, basis: int = 0) -> bool:
        uint(basis)
        if not detailed:
            return True  # A bounded header-only fixture; no heavy calls.
        require(identity is not None and identity in self.records, "DetailedQuoteNeedsPreseal")
        record = self.records[identity]
        target = self.targets[record.submission.target]
        require(record.ticket is not None and eligible(record.ticket, target.difficulty), "Ineligible")
        require(record.lock > 0, "DetailedQuoteNeedsFunds")
        # Charge/cache semantics: one (identity,basis) evaluation; changed basis costs again.
        if basis not in record.quote_bases:
            require(record.quote_work_spent + self.profile.visit_cost
                    <= self.profile.quote_work_capacity_per_preseal, "DetailedQuoteWorkBudget")
            require(record.lock >= 1, "DetailedQuoteNeedsFunds")
            record.lock -= 1
            self.fees += 1
            self.heavy_calls += 1
            record.quote_work_spent += self.profile.visit_cost
            record.quote_bases.add(basis)
        return True

    def _close(self, epoch: int, complete_service: bool) -> Observation:
        target = self.targets.get(epoch)
        demand = congestion = False
        unavailable = self.canonical_unavailable or not self.profile.qualified or epoch < 2
        counts: dict[str, int] = {}
        pending = 0
        complete = complete_service and epoch >= 2
        if target is not None:
            unavailable = unavailable or target.unavailable
            complete = complete and target.coverage and target.seed is not None
            for identity in target.records:
                record = self.records[identity]
                disposition = record.disposition
                if disposition is None:
                    disposition = "SeedUnavailable" if target.unavailable else "ExpiredUnvisited"
                    record.disposition = disposition
                    pending += 1
                counts[disposition] = counts.get(disposition, 0) + 1
                demand = demand or disposition in ("Accepted", "NoCapacity")
                congestion = congestion or disposition == "NoCapacity"
            complete = complete and pending == 0 and target.cursor == len(target.ranked)
            target.state = "Closed"
        try:
            future = self._future_ratio(epoch + 1)
            spent = max((ratio(n, c) for n, c in zip(self.flow_spent.get(epoch, vv(0)),
                                                    self.profile.supply)), default=0)
            stock = ratio(self.stock_peak.get(epoch, 0), self.profile.stock_supply)
        except Rejected as error:
            require(error.code in ("UnavailableSupply", "CapacityInconsistency"), error.code)
            unavailable = True
            future = spent = stock = 0  # Explicit stop, never a clipped success.
        return Observation(epoch, (epoch + 1) * EPOCH_MS, demand, congestion, complete,
                           unavailable, future, spent, stock, pending, tuple(sorted(counts.items())))

    def boundary(self, next_epoch: int, complete_service: bool = True) -> Observation:
        require(self.local_ready, "LocalNotReady")
        uint(next_epoch, 64)
        require(next_epoch == self.epoch + 1 and self.last_boundary == self.epoch, "BoundaryOrder")
        uint(next_epoch + 2, 64)
        require(next_epoch + 1 in self.difficulty, "MissingPublishedDifficulty")
        require(next_epoch + 2 not in self.difficulty, "FrozenTarget")
        observation = self._close(self.epoch, complete_service)
        self.observations[self.epoch] = observation
        next_d, run = difficulty_update(self.difficulty[next_epoch + 1], self.low_run, observation)
        self.difficulty[next_epoch + 2] = next_d
        self.low_run = run
        self.targets[next_epoch + 2] = Target(next_epoch + 2, next_d)
        to_freeze = self.targets[next_epoch + 1]
        require(to_freeze.state == "Collecting", "AlreadyFrozen")
        to_freeze.closed_root = H(b"TEST/closed/" + b"".join(sorted(to_freeze.records)))
        to_freeze.state = "Frozen"
        self.epoch = next_epoch
        self.last_boundary = next_epoch
        self.stock_peak[next_epoch] = self._stock(next_epoch)
        if next_epoch >= 2:
            target = self.targets[next_epoch]
            target.ready_root = H(b"TEST/ready/" + b"".join(sorted(
                identity for identity in target.records if self.records[identity].ready)))
            target.unavailable = target.unavailable or target.seed is None
            target.ranked = sorted(target.records, key=lambda identity: (
                self.records[identity].ticket if self.records[identity].ticket is not None else U256_MAX,
                identity))
            target.state = "Active"
        return observation


def fixture_submission(number: int, **changes) -> Submission:
    return replace(Submission(capital=b"native-capital-" + le(number, 4), sequence=1), **changes)


def prepare(model: AdmissionModel, submissions: tuple[Submission, ...],
            stage_ready: bool = True, seed: bytes = H(b"fixed-future-beacon")) -> tuple[bytes, ...]:
    identities = []
    for submission in submissions:
        if submission.capital not in model.capital:
            model.add_capital(submission.capital)
        identities.append(model.submit(submission))
    model.boundary(1)
    model.provide_seed(2, 1, seed)
    if stage_ready:
        for identity in identities:
            record = model.records[identity]
            if eligible(record.ticket, model.targets[2].difficulty):
                model.stage(identity, record.submission.payload)
    model.boundary(2)
    return tuple(identities)


class ContractTests(unittest.TestCase):
    def rejects(self, code: str, function, *args, **kwargs):
        with self.assertRaises(Rejected) as failure:
            function(*args, **kwargs)
        self.assertEqual(failure.exception.code, code)

    def test_01_codec_roundtrip_retains_native_width_and_rejects_ambiguous_fields(self):
        """Outer typed LP keeps 64-byte legacy IDs intact and rejects alternate fields."""
        model = AdmissionModel()
        submission = fixture_submission(1, model=b"m" * 64)
        slot, identity = model._slot_and_id(submission)
        encoded = fields_encoding(model.network, model.policy, 2, slot, submission.native_fields())
        decoded = decode_fields(encoded)
        self.assertEqual(decoded[-1][3], ("model", b"m" * 64))
        self.assertEqual(fields_encoding(*decoded), encoded)
        self.assertEqual(identity, preseal_id(encoded))
        self.rejects("TrailingField", decode_fields, encoded + b"\0")
        self.rejects("TruncatedField", decode_fields, encoded[:-1])
        reversed_fields = tuple(reversed(submission.native_fields()))
        self.rejects("FieldOrderOrCount", fields_encoding, model.network, model.policy, 2, slot, reversed_fields)
        self.rejects("NativeVersion", nr, "model", b"m", 2)
        malformed = le(1, 2) + le(65, 4)
        self.rejects("LPBound", decode_fields, malformed)

    def test_02_ticket_known_bytes_and_domain_network_policy_epoch_binding(self):
        """Exact ticket bytes/endian and all contextual bindings are checked independently."""
        network, policy, identity, seed = b"n", bytes(range(32)), b"p" * 32, b"s" * 32
        # Construct the fixture without calling the CAD1 lp/le/ticket encoders.
        domain = b"MISAKA/PALW/CAD1/TICKET"
        message = (struct.pack("<I", len(domain)) + domain + b"\x01\0\0\0n"
                   + policy + struct.pack("<Q", 2) + identity + seed)
        expected_digest = hashlib.sha256(message).digest()
        value = ticket_value(network, policy, 2, identity, seed)
        self.assertEqual(value, int.from_bytes(expected_digest, "big"))
        self.assertNotEqual(value, int.from_bytes(expected_digest, "little"))
        variants = ((b"other", policy, 2, identity, seed), (network, H(policy), 2, identity, seed),
                    (network, policy, 3, identity, seed), (network, policy, 2, H(identity), seed),
                    (network, policy, 2, identity, H(seed)))
        self.assertTrue(all(ticket_value(*entry) != value for entry in variants))
        self.rejects("DigestWidth", ticket_value, network, policy[:-1], 2, identity, seed)

    def test_03_threshold_inclusive_boundaries_and_integer_domain(self):
        """Ticket equal to T passes; T+1 fails, including D limits and type rejection."""
        for difficulty in (1, 2, 8, 9, D_MAX):
            cut = threshold(difficulty)
            self.assertTrue(eligible(cut, difficulty))
            self.assertTrue(eligible(0, difficulty))
            if cut < U256_MAX:
                self.assertFalse(eligible(cut + 1, difficulty))
        self.assertTrue(eligible(U256_MAX, 1))
        for invalid in (0, -1, D_MAX + 1, True):
            self.rejects("DifficultyRange", threshold, invalid)
        self.rejects("IntegerRange", eligible, U256_MAX + 1, 1)
        self.rejects("IntegerRange", le, 1 << 64, 8)

    def test_04_complete_paid_pipeline_reaches_observed_difficulty_without_minting_rights(self):
        """A real hash ticket drives acceptance, measured synthetic pressure, and D_5."""
        model = AdmissionModel()
        (identity,) = prepare(model, (fixture_submission(1, shape=Shape(flow=vv(80), stock=1)),))
        self.assertEqual(model.process_all(), ("Accepted",))
        model.consume_claim_work(identity)
        model.terminal_release(identity, native_terminal=True)
        observed = model.boundary(3)
        self.assertTrue(observed.qualified_demand)
        self.assertEqual((observed.future, observed.spent), (0, 820))
        self.assertEqual(model.difficulty[5], 2)
        self.assertEqual((model.difficulty[2], model.difficulty[3], model.difficulty[4]), (1, 1, 1))
        self.assertEqual(model.money_total(), model.initial_money)
        self.assertNotIn("credit", vars(model))

    def test_05_trial_slot_blocks_result_route_model_target_and_nonce_rerolls(self):
        """One funded slot binds the first result; aliases cannot buy another free seed."""
        model = AdmissionModel()
        submission = fixture_submission(1)
        model.add_capital(submission.capital)
        identity = model.submit(submission)
        before = model.canonical_snapshot()
        self.assertEqual(model.submit(submission, "FreePrompt", b"another-signature"), identity)
        self.assertEqual(model.canonical_snapshot(), before)
        for variant in (replace(submission, payload=b"false-alternative"),
                        replace(submission, model=b"alias"), replace(submission, target=3),
                        replace(submission, input_data=b"different")):
            self.rejects("TrialAlreadyBound", model.submit, variant)
        self.assertEqual(len(model.records), 1)

    def test_06_distinct_funded_slots_allow_legitimate_same_input_without_worldwide_dedup(self):
        """Two paid opportunities for the same input remain legitimate independent trials."""
        model = AdmissionModel()
        ids = prepare(model, (fixture_submission(1), fixture_submission(2)))
        self.assertNotEqual(model.records[ids[0]].slot, model.records[ids[1]].slot)
        self.assertEqual(model.process_all(), ("Accepted", "Accepted"))
        self.assertEqual(len(model.claims), 2)
        self.assertEqual(model.money_total(), model.initial_money)

    def test_07_actual_capital_lock_and_preseal_fee_are_required_before_ticket(self):
        """Insufficient funds and capital reuse fail without partial fees or slot state."""
        model = AdmissionModel()
        submission = fixture_submission(1)
        model.add_capital(submission.capital, 1)
        before = model.canonical_snapshot()
        self.rejects("InsufficientFunds", model.submit, submission)
        self.assertEqual(model.canonical_snapshot(), before)
        model.capital[submission.capital].available += 1_000
        model.initial_money += 1_000
        model.submit(submission)
        self.rejects("CapitalAlreadyLocked", model.submit, replace(submission, sequence=2))
        self.assertEqual(model.fees, 2)
        self.assertEqual(model.money_total(), model.initial_money)

    def test_08_global_preseal_budget_does_not_expand_for_models_or_principals(self):
        """Nonzero per-preseal worst-case control reserves bound all identities together."""
        model = AdmissionModel(profile=replace(Profile(), preseal_control_capacity=20))
        for number in (1, 2):
            submission = fixture_submission(number, model=f"model-{number}".encode())
            model.add_capital(submission.capital)
            model.submit(submission)
        third = fixture_submission(3, model=b"another-key-and-model")
        model.add_capital(third.capital)
        before = model.canonical_snapshot()
        self.rejects("PresealControlCapacity", model.submit, third)
        self.assertEqual(model.canonical_snapshot(), before)
        self.rejects("ZeroControlCost", AdmissionModel, 1, replace(Profile(), control_per_preseal=0))

    def test_09_freeze_seed_and_readiness_use_canonical_half_open_windows(self):
        """Seed-after additions, changed anchors, second seeds and late material fail."""
        model = AdmissionModel()
        submission = fixture_submission(1)
        model.add_capital(submission.capital)
        identity = model.submit(submission)
        self.rejects("SeedWindow", model.provide_seed, 2, 1, H(b"beacon"))
        model.boundary(1)
        root = model.targets[2].closed_root
        late = fixture_submission(2)
        model.add_capital(late.capital)
        self.rejects("CollectionWindow", model.submit, late)
        self.rejects("SeedAnchorOrReuse", model.provide_seed, 2, 2, H(b"wrong"))
        model.provide_seed(2, 1, H(b"beacon"))
        self.rejects("SeedAnchorOrReuse", model.provide_seed, 2, 1, H(b"second"))
        model.stage(identity, submission.payload, 2 * EPOCH_MS - 1)
        self.rejects("ReadinessWindow", model.stage, identity, submission.payload, 2 * EPOCH_MS)
        model.cancel(identity)
        self.assertEqual(model.targets[2].closed_root, root)
        model.boundary(2)
        self.rejects("ReadinessWindow", model.stage, identity, submission.payload)

    def test_10_seed_failure_stops_new_responsibility_and_no_free_next_seed(self):
        """A missing designated seed disables its target without changing a protected claim."""
        model = AdmissionModel()
        submission = fixture_submission(1)
        model.add_capital(submission.capital)
        identity = model.submit(submission)
        old = H(b"old-protected-original")
        model.claims.add(old)
        model.protected[old] = {"honest-actor"}
        model.boundary(1)
        model.boundary(2)
        self.rejects("NewAdmissionUnavailable", model.process_all)
        self.assertTrue(model.remedy(old, "honest-actor"))
        observed = model.boundary(3)
        self.assertTrue(observed.unavailable)
        self.assertEqual(model.difficulty[5], model.difficulty[4])
        self.assertEqual(model.low_run, 0)
        self.rejects("TrialAlreadyBound", model.submit, replace(submission, target=5))
        self.assertGreater(model.settle(identity), 0)
        self.assertEqual(model.settle(identity), 0)

    def test_11_ineligible_never_decodes_heavy_material_or_calls_planner(self):
        """An actual D_max ticket is rejected before heavy payload handling."""
        model = AdmissionModel(D_MAX)
        (identity,) = prepare(model, (fixture_submission(1),))
        self.assertFalse(eligible(model.records[identity].ticket, D_MAX))
        self.assertEqual(model.process_all(), ("Ineligible",))
        self.assertEqual((model.heavy_calls, model.decode_heavy_calls), (0, 0))
        observed = model.boundary(3)
        self.assertTrue(observed.complete)
        self.assertFalse(observed.qualified_demand)
        self.assertEqual(model.difficulty[5], D_MAX)
        self.assertEqual(model.low_run, 1)  # Bootstrap epochs did not count as idle.

    def test_12_missing_or_mismatched_material_is_not_qualified_congestion(self):
        """Missing canonical readiness is terminal without forging NoCapacity."""
        model = AdmissionModel()
        submission = fixture_submission(1)
        model.add_capital(submission.capital)
        identity = model.submit(submission)
        model.boundary(1)
        model.provide_seed(2, 1, H(b"beacon"))
        self.rejects("CommitMismatch", model.stage, identity, b"other-payload")
        model.boundary(2)
        self.assertEqual(model.process_all(), ("MissingMaterial",))
        observed = model.boundary(3)
        self.assertFalse(observed.qualified_demand or observed.known_congestion)
        self.assertEqual(model.heavy_calls, 0)

    def test_13_paid_false_commit_can_qualify_but_does_not_prove_computation(self):
        """Residual paid manipulation is explicit: the gate cannot establish output truth."""
        model = AdmissionModel()
        (identity,) = prepare(model, (fixture_submission(1, computationally_correct=False,
                                                       payload=b"funded-false-result"),))
        self.assertEqual(model.process_all(), ("Accepted",))
        self.assertIn(identity, model.claims)
        self.assertGreater(model.fees, 0)
        observed = model.boundary(3)
        self.assertTrue(observed.qualified_demand)

    def test_14_rank_is_independent_of_ingress_order_and_legal_batch_partition(self):
        """Same closed set/canonical clock/service cut/external state gives partition-invariant results."""
        submissions = tuple(fixture_submission(n) for n in (1, 2, 3))
        left, right = AdmissionModel(), AdmissionModel()
        prepare(left, submissions)
        prepare(right, tuple(reversed(submissions)))
        self.assertEqual(left.targets[2].closed_root, right.targets[2].closed_root)
        self.assertEqual(left.targets[2].ranked, right.targets[2].ranked)
        expected = left.process_all()
        order = right.targets[2].ranked
        first = right.advance("first", tuple(order[:1]))
        rest = right.advance("rest", tuple(order[1:]))
        self.assertEqual(expected, first + rest)
        for name in ("claims", "reservations", "original_escrow", "fees", "slot_uses"):
            self.assertEqual(getattr(left, name), getattr(right, name))
        self.assertEqual(left.boundary(3), right.boundary(3))

    def test_15_prefix_skips_reorder_and_duplicate_batch_cannot_spend_twice(self):
        """A native advance must visit the exact next prefix; replay returns one receipt."""
        model = AdmissionModel()
        prepare(model, (fixture_submission(1), fixture_submission(2)))
        order = tuple(model.targets[2].ranked)
        self.rejects("NotNextPrefix", model.advance, "skip", order[1:])
        first = model.advance("one", order[:1])
        before = model.canonical_snapshot()
        self.assertEqual(model.advance("one", order[:1]), first)
        self.assertEqual(model.canonical_snapshot(), before)
        self.rejects("BatchConflict", model.advance, "one", order[1:])
        self.rejects("NotNextPrefix", model.advance, "again", order[:1])

    def test_16_whole_batch_rolls_back_planner_exception_without_forged_congestion(self):
        """Failure after an earlier candidate discards its funds/book/cursor/cost changes."""
        model = AdmissionModel()
        prepare(model, (fixture_submission(1), fixture_submission(2)))
        order = model.targets[2].ranked
        model.planner_fault_slots.add(model.records[order[1]].slot)
        before = model.canonical_snapshot()
        self.rejects("PlannerIncomplete", model.process_all)
        self.assertEqual(model.canonical_snapshot(), before)
        observed = model.boundary(3)
        self.assertFalse(observed.complete or observed.known_congestion or observed.qualified_demand)
        self.assertEqual(model.low_run, 0)

    def test_17_dynamic_fit_uses_vectors_and_can_admit_multiple_small_candidates(self):
        """No fixed count or winner: two small candidates fit while a large one fails intrinsically."""
        model = AdmissionModel()
        submissions = (fixture_submission(1, shape=Shape(flow=vv(110))),
                       fixture_submission(2, shape=Shape(flow=vv(20))),
                       fixture_submission(3, shape=Shape(flow=vv(20))))
        ids = prepare(model, submissions)
        model.process_all()
        self.assertEqual(model.records[ids[0]].disposition, "IntrinsicShapeFailure")
        self.assertEqual([model.records[x].disposition for x in ids[1:]], ["Accepted", "Accepted"])
        self.assertEqual(len(model.claims), 2)
        self.assertEqual(model.money_total(), model.initial_money)

    def test_18_valid_all_refused_candidates_still_create_known_congestion(self):
        """A funded candidate fitting the empty residual book produces high on real conflict."""
        model = AdmissionModel()
        prepare(model, (fixture_submission(1, shape=Shape(flow=vv(20))),))
        existing = H(b"prior-responsibility")
        model.reservations[existing] = Reservation(existing, 2, {2: vv(90)}, 0, 4)
        self.assertEqual(model.process_all(), ("NoCapacity",))
        observed = model.boundary(3)
        self.assertTrue(observed.qualified_demand and observed.known_congestion)
        self.assertEqual(len(model.claims), 0)
        self.assertEqual(model.difficulty[5], 2)

    def test_19_invalid_qualification_funding_and_deadline_do_not_manufacture_high(self):
        """High existing reservations do not turn disqualified candidates into valid demand."""
        for kind in ("qualification", "funding", "deadline"):
            with self.subTest(kind=kind):
                model = AdmissionModel()
                submission = fixture_submission(1, original_deadline=2 * EPOCH_MS if kind == "deadline" else 3 * EPOCH_MS)
                (identity,) = prepare(model, (submission,))
                record = model.records[identity]
                if kind == "qualification":
                    model.revoked_slots.add(record.slot)
                if kind == "funding":
                    model.invalid_funding_slots.add(record.slot)
                previous = H(b"existing")
                model.reservations[previous] = Reservation(previous, 2, {3: vv(90)}, 0, 4)
                model.process_all()
                observed = model.boundary(3)
                self.assertEqual(observed.future, 922)
                self.assertFalse(observed.qualified_demand or observed.known_congestion)
                self.assertEqual(model.difficulty[5], 1)

    def test_20_raw_invalid_ingress_never_changes_canonical_pressure_or_difficulty(self):
        """RPC noise stays in finite local ingress; only authenticated canonical observations count."""
        model = AdmissionModel()
        before = model.canonical_snapshot()
        for _ in range(100):
            self.rejects("InvalidAuthentication", model.ingress.receive, model.profile, 100, 100, False)
        self.assertEqual(model.canonical_snapshot(), before)
        self.assertEqual(model.ingress.active, 0)
        self.rejects("HeaderBound", model.ingress.receive, model.profile, 10**9, 100, True)
        model.ingress.active = model.ingress.capacity
        calls = model.ingress.decode_calls
        self.rejects("IngressLeaseUnavailable", model.ingress.receive, model.profile, 100, 100, True)
        self.assertEqual(model.ingress.decode_calls, calls)

    def test_21_spent_work_and_peak_live_stock_survive_completion_before_boundary(self):
        """Closed-epoch charges and stock peaks prevent a false low after actual release."""
        model = AdmissionModel()
        (identity,) = prepare(model, (fixture_submission(1, shape=Shape(flow=vv(10), stock=80)),))
        model.process_all()
        model.consume_claim_work(identity)
        model.terminal_release(identity, native_terminal=True)
        observed = model.boundary(3)
        self.assertEqual((observed.future, observed.spent, observed.stock), (0, 103, 820))
        self.assertEqual(observed.utilization, 820)  # max, not sum 923.
        self.assertEqual(model.difficulty[5], 2)

    def test_22_released_unexecuted_future_peak_is_not_permanent_pressure(self):
        """Only present obligations, actual cost/live peak, and recorded congestion survive."""
        model = AdmissionModel()
        (identity,) = prepare(model, (fixture_submission(1, shape=Shape(flow=vv(80), stock=0,
                                                                       future_flow=vv(90))),))
        model.process_all()
        self.assertEqual(model._future_ratio(2), 922)
        model.terminal_release(identity, native_terminal=True)
        observed = model.boundary(3)
        self.assertEqual(observed.utilization, 0)
        self.assertFalse(observed.known_congestion)
        self.assertEqual(model.difficulty[5], 1)

    def test_23_far_future_obligations_and_fixed_system_subtraction_are_observed(self):
        """All bounded future intervals use C_A, preserving fixed system reservations."""
        model = AdmissionModel()
        prior = H(b"far-future")
        model.reservations[prior] = Reservation(prior, 9, {9: vv(80)}, 0, 10)
        self.assertEqual(model.profile.supply, vv(100))
        self.assertEqual(model._future_ratio(1), 820)
        self.assertLess((1024 * 80 + 119) // 120, 768)  # Wrong full C hides pressure.
        self.rejects("IntegerRange", AdmissionModel, 1, replace(Profile(), fixed_flow=vv(121)))

    def test_24_outer_one_bps_is_separate_from_copacked_payload_capacity(self):
        """120 outer permits do not subtract 120 from the residual inner payload dimension."""
        model = AdmissionModel()
        self.assertEqual(model.profile.outer_permits_per_epoch, 120)
        self.assertEqual(model.profile.supply[-1], 100)
        prepare(model, (fixture_submission(1, shape=Shape(flow=vv(50))),))
        self.assertEqual(model.process_all(), ("Accepted",))
        empty = AdmissionModel()
        observed = empty.boundary(1)
        self.assertEqual(observed.utilization, 0)
        self.assertFalse(observed.qualified_demand)

    def test_25_zero_supply_and_book_overflow_stop_instead_of_clipping_or_easing(self):
        """Positive unavailable supply or an overfull book is a stop signal, never wrapped low."""
        self.assertEqual(ratio(0, 0), 0)
        self.rejects("UnavailableSupply", ratio, 1, 0)
        self.rejects("CapacityInconsistency", ratio, 101, 100)
        self.rejects("IntegerRange", ratio, (1 << 256) - 1, (1 << 256) - 1)
        model = AdmissionModel(8, replace(Profile(), total_flow=vv(20)))
        previous = H(b"inconsistent-obligation")
        model.reservations[previous] = Reservation(previous, 1, {1: vv(1)}, 0, 2)
        observed = model.boundary(1)
        self.assertTrue(observed.unavailable)
        self.assertEqual((model.difficulty[3], model.low_run), (8, 0))

    def test_26_incomplete_tail_prevents_low_but_preserves_valid_known_high(self):
        """Incomplete coverage may increase D on sound evidence; it can never idle-decay."""
        low = AdmissionModel(8)
        prepare(low, (fixture_submission(1), fixture_submission(2)))
        observed = low.boundary(3)
        self.assertFalse(observed.complete)
        self.assertEqual(low.low_run, 0)
        self.assertEqual(low.difficulty[5], 8)
        high = AdmissionModel()
        prepare(high, (fixture_submission(1, shape=Shape(flow=vv(80), stock=80)), fixture_submission(2)))
        order = high.targets[2].ranked
        high.advance("first", tuple(order[:1]))
        # Ensure a sound congestion observation even if the first ranked job is small.
        existing = H(b"existing-for-tail-test")
        high.reservations[existing] = Reservation(existing, 2, {3: vv(80)}, 0, 4)
        observed = high.boundary(3)
        self.assertFalse(observed.complete)
        self.assertTrue(observed.qualified_demand)
        self.assertGreaterEqual(observed.utilization, 768)
        self.assertEqual(high.difficulty[5], 2)

    def test_27_complete_empty_and_zero_eligible_epochs_allow_intended_idle_decay(self):
        """Three complete low observations ease D; an explicitly missing service resets it."""
        model = AdmissionModel(9)
        model.boundary(1)
        model.provide_seed(2, 1, H(b"empty-target-seed"))
        model.boundary(2)
        self.assertEqual((model.difficulty[4], model.low_run), (9, 0))
        model.provide_seed(3, 2, H(b"empty-target3-seed"))
        observed = model.boundary(3)
        self.assertTrue(observed.complete)
        self.assertEqual((model.difficulty[5], model.low_run), (9, 1))
        model.provide_seed(4, 3, H(b"empty-target4-seed"))
        model.boundary(4)
        self.assertEqual((model.difficulty[6], model.low_run), (9, 2))
        model.boundary(5)
        self.assertEqual((model.difficulty[7], model.low_run), (8, 0))
        interrupted = AdmissionModel(9)
        interrupted.boundary(1)
        interrupted.boundary(2, complete_service=False)
        self.assertEqual((interrupted.difficulty[4], interrupted.low_run), (9, 0))

    def test_28_exact_controller_thresholds_rounding_and_unavailable_precedence(self):
        """Independent boundary vectors cover 256/768, gains, clamps and priority."""
        def observation(u=0, demand=False, congestion=False, complete=True, unavailable=False):
            return Observation(2, 3 * EPOCH_MS, demand, congestion, complete, unavailable, u, 0, 0, 0, ())
        self.assertEqual(difficulty_update(8, 0, observation(768, True)), (9, 0))
        self.assertEqual(difficulty_update(9, 0, observation(768, True)), (11, 0))
        self.assertEqual(difficulty_update(1, 0, observation(768, True)), (2, 0))
        self.assertEqual(difficulty_update(D_MAX, 0, observation(768, True)), (D_MAX, 0))
        self.assertEqual(difficulty_update(9, 2, observation(256)), (8, 0))
        self.assertEqual(difficulty_update(9, 2, observation(257)), (9, 0))
        self.assertEqual(difficulty_update(9, 2, observation(767, True)), (9, 0))
        self.assertEqual(difficulty_update(9, 2, observation(0, True, True, False)), (11, 0))
        self.assertEqual(difficulty_update(9, 2, observation(1024, True, True, False, True)), (9, 0))
        self.assertEqual(difficulty_update(1, 2, observation()), (1, 0))
        self.assertEqual(difficulty_update(9, 2, observation(0, False, False, False)), (9, 0))

    def test_29_bootstrap_schedule_and_boundary_order_do_not_rewrite_frozen_difficulty(self):
        """D_2 precedes collection; later boundaries publish only b+2 and reject gaps/replay."""
        model = AdmissionModel(8)
        self.assertEqual(model.difficulty, {2: 8})
        self.assertEqual(model.low_run, 0)
        self.rejects("BootstrapNoAdmission", model.advance, "boot", (H(b"x"),))
        self.rejects("BoundaryOrder", model.boundary, 2)
        model.boundary(1)
        self.assertTrue(model.observations[0].unavailable)
        self.assertFalse(model.observations[0].complete)
        self.assertEqual((model.difficulty[3], model.low_run), (8, 0))
        frozen = dict(model.difficulty)
        self.rejects("BoundaryOrder", model.boundary, 1)
        model.boundary(2)
        self.assertTrue(model.observations[1].unavailable)
        self.assertFalse(model.observations[1].complete)
        self.assertEqual((model.difficulty[4], model.low_run), (8, 0))
        self.assertTrue(all(model.difficulty[key] == value for key, value in frozen.items()))
        del model.difficulty[4]
        self.rejects("MissingPublishedDifficulty", model.boundary, 3)

    def test_30_all_routes_use_the_gate_and_future_or_disabled_routes_default_deny(self):
        """Every connected new responsibility requires the same funded preseal path."""
        for index, route in enumerate(sorted(CONNECTED_NEW_ROUTES), 1):
            with self.subTest(route=route):
                model = AdmissionModel()
                submission = fixture_submission(index)
                model.add_capital(submission.capital, 1)
                self.rejects("InsufficientFunds", model.submit, submission, route)
        model = AdmissionModel()
        submission = fixture_submission(1)
        model.add_capital(submission.capital)
        for route in ("TFO3", "future-route", "legacy-downgrade", "pretend-rescue"):
            self.rejects("UnconnectedNewRoute", model.submit, submission, route)
        self.rejects("DisabledFeature", model.submit, replace(submission, enabled=False), "evaluation")

    def test_31_existing_remedies_are_exempt_but_original_expansion_is_not(self):
        """New D, unavailable seed/profile and entry noise cannot revoke prior actor rights."""
        model = AdmissionModel()
        (identity,) = prepare(model, (fixture_submission(1),))
        model.process_all()
        model.canonical_unavailable = True
        model.profile = replace(model.profile, qualified=False)
        model.difficulty[5] = D_MAX
        self.assertTrue(model.remedy(identity, "honest-actor"))
        self.rejects("UnknownProtectedOriginal", model.remedy, H(b"unknown"), "honest-actor")
        self.rejects("UnknownProtectedOriginal", model.remedy, identity, "not-an-actor")
        self.rejects("NewResponsibility", model.remedy, identity, "honest-actor", True)
        self.rejects("AcceptedResponsibilitiesRemain", model.cancel, identity)

    def test_32_failed_and_expired_trials_refund_once_without_free_reroll(self):
        """Finite settlement conserves funds, preserves consumed slots, and does not slash."""
        model = AdmissionModel(D_MAX)
        (identity,) = prepare(model, (fixture_submission(1),))
        model.process_all()
        self.rejects("SettlementNotReleased", model.settle, identity)
        model.boundary(3)
        fees = model.fees
        refund = model.settle(identity)
        self.assertGreater(refund, 0)
        self.assertEqual(model.settle(identity), 0)
        self.assertEqual(model.fees, fees)
        self.assertEqual(model.money_total(), model.initial_money)
        submission = model.records[identity].submission
        self.rejects("TrialAlreadyBound", model.submit, replace(submission, target=5))
        next_trial = replace(submission, sequence=2, target=5,
                             original_deadline=6 * EPOCH_MS, signature_expiry=6 * EPOCH_MS)
        self.assertNotEqual(model.submit(next_trial), identity)

    def test_33_pruning_does_not_resurrect_a_used_trial(self):
        """Compact highwater/nullifier state survives finite record retention."""
        model = AdmissionModel(D_MAX)
        (identity,) = prepare(model, (fixture_submission(1),))
        model.process_all()
        submission = model.records[identity].submission
        model.boundary(3)
        model.settle(identity)
        self.rejects("RetentionStillLive", model.prune, identity)
        model.boundary(4)
        model.boundary(5)
        model.prune(identity)
        self.assertEqual(model.capital[submission.capital].highwater, 1)
        self.rejects("PrunedTrial", model.submit, submission)
        self.rejects("TrialAlreadyBound", model.submit, replace(submission, target=7))

    def test_34_cancel_preserves_closed_set_paid_cost_and_finalized_observation(self):
        """Cancellation does not erase a closed root, spent money or a previous congestion fact."""
        model = AdmissionModel()
        (identity,) = prepare(model, (fixture_submission(1),))
        root = model.targets[2].closed_root
        model.cancel(identity)
        model.process_all()
        observed = model.boundary(3)
        fees = model.fees
        model.cancel(identity)
        self.assertEqual(model.targets[2].closed_root, root)
        self.assertEqual(model.observations[2], observed)
        self.assertEqual(model.fees, fees)
        self.assertEqual(dict(observed.disposition_counts), {"Canceled": 1})

    def test_35_canonical_snapshot_restores_schedule_cursor_funds_slots_and_book(self):
        """Reorg restores one checkpoint; stale worker generations cannot mutate the new branch."""
        model = AdmissionModel()
        prepare(model, (fixture_submission(1), fixture_submission(2)))
        before = model.canonical_snapshot()
        model.process_all()
        model.boundary(3)
        model.restore(before)
        self.assertEqual(model.canonical_snapshot(), before)
        old_generation = model.generation
        model.generation += 1
        order = tuple(model.targets[2].ranked)
        self.rejects("StaleGeneration", model.advance, "stale", order, old_generation)
        model.advance("new", order, model.generation)
        self.assertEqual(model.money_total(), model.initial_money)

    def test_36_missing_snapshot_is_local_hold_not_consensus_unavailability_vote(self):
        """An incomplete local host abstains; it cannot manufacture controller observations."""
        model = AdmissionModel(9)
        snapshot = model.canonical_snapshot()
        bad = copy.deepcopy(snapshot)
        del bad["slot_uses"]
        self.rejects("IncompleteSnapshot", model.restore, bad)
        self.assertFalse(model.local_ready)
        self.assertFalse(model.canonical_unavailable)
        self.rejects("LocalNotReady", model.boundary, 1)
        self.assertEqual(model.observations, {})
        model.restore(snapshot)
        model.boundary(1)
        self.assertTrue(model.observations[0].unavailable)  # Bootstrap, not a local-host vote.
        self.assertFalse(model.canonical_unavailable)

    def test_37_unknown_ten_thousand_and_fifty_thousand_model_ids_do_not_create_budget(self):
        """Identifier invariance only: NOT a benchmark or a real registry/load test."""
        model = AdmissionModel()
        original = fixture_submission(1)
        slot = slot_key(model.network, model.generation, original.capital, original.sequence)
        for count in (10_000, 50_000):
            for number in range(count):
                variant = replace(original, model=f"unseen-{number}".encode())
                # Identity is the only changing datum; no per-model budget/table is made.
                self.assertEqual(slot_key(model.network, model.generation, variant.capital, variant.sequence), slot)
            self.assertEqual((model.profile.supply, model.difficulty), (vv(100), {2: 1}))
        self.assertNotIn("models", vars(model))

    def test_38_detailed_quotes_cannot_bypass_eligibility_or_run_free_on_changed_basis(self):
        """Cheap quote is header-only; detailed quote needs an eligible paid trial and cache key."""
        model = AdmissionModel()
        for _ in range(100):
            self.assertTrue(model.quote(None, detailed=False))
        self.assertEqual(model.heavy_calls, 0)
        self.rejects("DetailedQuoteNeedsPreseal", model.quote, None, True)
        submission = fixture_submission(1)
        model.add_capital(submission.capital)
        identity = model.submit(submission)
        self.rejects("Ineligible", model.quote, identity, True)
        model.boundary(1)
        model.provide_seed(2, 1, H(b"quote-seed"))
        model.quote(identity, True, basis=1)
        fee = model.fees
        model.quote(identity, True, basis=1)
        self.assertEqual(model.fees, fee)
        model.quote(identity, True, basis=2)
        self.assertEqual(model.fees, fee + 1)
        self.rejects("DetailedQuoteWorkBudget", model.quote, identity, True, basis=3)
        self.assertEqual(model.money_total(), model.initial_money)

    def test_39_batch_visit_cost_caps_apply_even_to_ineligible_or_rejected_records(self):
        """Cheap failures cannot create zero-cost unbounded cursor visits."""
        profile = replace(Profile(), batch_work_capacity=2)
        model = AdmissionModel(D_MAX, profile)
        prepare(model, (fixture_submission(1), fixture_submission(2)))
        order = tuple(model.targets[2].ranked)
        before = model.canonical_snapshot()
        self.rejects("BatchWorkBudget", model.advance, "too-many", order)
        self.assertEqual(model.canonical_snapshot(), before)
        self.assertEqual(model.advance("one", order[:1]), ("Ineligible",))
        self.assertEqual(model.advance("two", order[1:]), ("Ineligible",))
        self.assertEqual(model.targets[2].visits, 2)

    def test_40_difficulty_never_overrides_hard_resource_capacity(self):
        """All tickets can pass at D=1 without admitting an over-capacity original."""
        model = AdmissionModel()
        prepare(model, tuple(fixture_submission(number, shape=Shape(flow=vv(60))) for number in (1, 2, 3)))
        results = model.process_all()
        self.assertEqual(results.count("Accepted"), 1)
        self.assertEqual(results.count("NoCapacity"), 2)
        self.assertTrue(all(value <= cap for value, cap in zip(model._flow(2), model.profile.supply)))
        self.assertEqual(model.money_total(), model.initial_money)

    def test_41_seed_guard_cut_is_strict_and_late_seed_is_not_missing_material_fault(self):
        """The designated seed must finalize after freeze and before cut, with guard remaining."""
        model = AdmissionModel()
        submission = fixture_submission(1)
        model.add_capital(submission.capital)
        identity = model.submit(submission)
        model.boundary(1)
        self.rejects("SeedNotAfterFreeze", model.provide_seed, 2, 1, H(b"beacon"), EPOCH_MS)
        cut = 2 * EPOCH_MS - model.profile.ready_guard_ms
        before = model.canonical_snapshot()
        model.provide_seed(2, 1, H(b"beacon"), cut - 1)
        model.stage(identity, submission.payload, cut)
        model.boundary(2)
        self.assertEqual(model.process_all(), ("Accepted",))
        model.restore(before)
        self.rejects("SeedCutMissed", model.provide_seed, 2, 1, H(b"beacon"), cut)
        model.boundary(2)
        self.rejects("NewAdmissionUnavailable", model.process_all)
        observed = model.boundary(3)
        self.assertTrue(observed.unavailable)
        self.assertEqual(dict(observed.disposition_counts), {"SeedUnavailable": 1})
        self.assertEqual(model.records[identity].disposition, "SeedUnavailable")
        self.assertEqual(model.low_run, 0)
        for guard in (0, EPOCH_MS):
            self.rejects("InvalidReadyGuard", AdmissionModel, 1, replace(Profile(), ready_guard_ms=guard))

    def test_42_policy_generation_and_capital_rollover_cannot_resurrect_original_opportunity(self):
        """TrialUse keeps the underlying native opportunity across policy hashes and outpoint aliases."""
        model = AdmissionModel()
        submission = fixture_submission(1)
        model.add_capital(submission.capital)
        identity = model.submit(submission)
        original_slot = model.records[identity].slot
        model.generation += 1
        model.policy = H(b"new-policy")
        new_slot, _ = model._slot_and_id(submission)
        self.assertNotEqual(original_slot, new_slot)
        self.rejects("EconomicOpportunityAlreadyUsed", model.submit, submission)
        rollover = b"authenticated-rollover-outpoint"
        model.add_capital(rollover)
        model.capital_origins[rollover] = submission.capital  # Authenticated native lineage fixture.
        self.rejects("EconomicOpportunityAlreadyUsed", model.submit, replace(submission, capital=rollover))
        unauthorized = fixture_submission(3, sequence=10_001)
        model.add_capital(unauthorized.capital)
        self.rejects("ConsumedOrUnauthorizedSequence", model.submit, unauthorized)

    def test_43_genuine_reorg_undoes_preseal_fees_and_slot_in_one_checkpoint(self):
        """Canonical undo may remove a preseal; ordinary cancel/prune cannot imitate that undo."""
        model = AdmissionModel()
        submission = fixture_submission(1)
        model.add_capital(submission.capital)
        before = model.canonical_snapshot()
        identity = model.submit(submission)
        model.cancel(identity)
        self.rejects("TrialAlreadyBound", model.submit, replace(submission, payload=b"branch-result"))
        model.boundary(1)
        model.restore(before)
        self.assertEqual((model.fees, model.slot_uses, model.economic_uses), (0, {}, set()))
        replacement = model.submit(replace(submission, payload=b"branch-result"))
        self.assertNotEqual(identity, replacement)
        self.assertEqual(len(model.records), 1)
        self.assertEqual(model.money_total(), model.initial_money)

    def test_44_executed_flow_cannot_be_reused_within_the_same_epoch(self):
        """Finishing and releasing a claim does not refund already-debited interval capacity."""
        model = AdmissionModel()
        prepare(model, tuple(fixture_submission(number, shape=Shape(flow=vv(60))) for number in (1, 2)))
        first, second = model.targets[2].ranked
        self.assertEqual(model.advance("first", (first,)), ("Accepted",))
        model.consume_claim_work(first)
        model.terminal_release(first, native_terminal=True)
        self.assertEqual(model._flow(2), vv(0))
        self.assertEqual(model.advance("second", (second,)), ("NoCapacity",))
        observed = model.boundary(3)
        self.assertEqual(observed.spent, 615)
        self.assertTrue(observed.known_congestion)
        self.assertEqual(model.difficulty[5], 2)

    def test_45_every_connected_route_reaches_the_same_actual_ticket_gate(self):
        """Plain, funded, evaluation and batch originals all fail the same real D_max ticket."""
        for route in sorted(CONNECTED_NEW_ROUTES | {"TFO3"}):
            with self.subTest(route=route):
                model = AdmissionModel(D_MAX)
                if route == "TFO3":
                    # Synthetic connection declaration, not evidence TFO3 exists in native main.
                    model.connected_routes.add("TFO3")
                submission = fixture_submission(1)
                model.add_capital(submission.capital)
                identity = model.submit(submission, route)
                model.boundary(1)
                model.provide_seed(2, 1, H(b"common-route-seed"))
                model.boundary(2)
                self.assertFalse(eligible(model.records[identity].ticket, D_MAX))
                self.assertEqual(model.process_all(), ("Ineligible",))
                self.assertEqual((model.heavy_calls, len(model.claims)), (0, 0))

    def test_46_unsupported_or_zero_capacity_candidate_does_not_vote_global_unavailable(self):
        """Candidate-local impossibility is rejected without forging an active-policy service fault."""
        flow = list(vv(120))
        flow[0] = 20  # Residual capacity zero in this dimension, no live positive obligation.
        model = AdmissionModel(profile=replace(Profile(), total_flow=tuple(flow)))
        submission = fixture_submission(1)
        model.add_capital(submission.capital)
        bad = replace(submission, shape=Shape(kernel="unsupported-user-request"))
        before = model.canonical_snapshot()
        self.rejects("UnsupportedShape", model.submit, bad)
        self.assertEqual(model.canonical_snapshot(), before)
        prepare(model, (submission,))
        self.assertEqual(model.process_all(), ("IntrinsicShapeFailure",))
        observed = model.boundary(3)
        self.assertFalse(observed.unavailable or observed.qualified_demand or observed.known_congestion)
        self.assertTrue(observed.complete)
        self.assertEqual(observed.utilization, 0)


class RecordingResult(unittest.TextTestResult):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.records = []

    def addSuccess(self, test):
        super().addSuccess(test)
        self.records.append({"name": test.id().split(".")[-1], "result": "PASS",
                             "contract": test.shortDescription()})

    def addFailure(self, test, error):
        super().addFailure(test, error)
        self.records.append({"name": test.id().split(".")[-1], "result": "FAIL",
                             "contract": test.shortDescription(), "detail": self._exc_info_to_string(error, test)})

    def addError(self, test, error):
        super().addError(test, error)
        self.records.append({"name": test.id().split(".")[-1], "result": "ERROR",
                             "contract": test.shortDescription(), "detail": self._exc_info_to_string(error, test)})


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(__file__).with_name("model-results.json"))
    parser.add_argument("--stdout-only", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[4]
    rfc = root / "docs/rfc/0056-global-claim-admission-difficulty.md"
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(ContractTests)
    result = unittest.TextTestRunner(verbosity=2, resultclass=RecordingResult).run(suite)
    report = {
        "schema": "misaka-cad1-abstract-model-results-v1",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "scope": "ABSTRACT CONTRACT MODEL ONLY; NOT native, E2E, hardware qualification, stability, or activation evidence",
        "source_commit": "2d8212b634880b0967fbf24b00b41936c2322c14",
        "rfc_path": str(rfc.relative_to(root)),
        "rfc_sha256": hashlib.sha256(rfc.read_bytes()).hexdigest(),
        "script_path": str(Path(__file__).resolve().relative_to(root)),
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "tests_run": result.testsRun,
        "passed": sum(record["result"] == "PASS" for record in result.records),
        "failures": len(result.failures), "errors": len(result.errors), "skipped": len(result.skipped),
        "success": result.wasSuccessful(),
        "tests": result.records,
        "assumptions": [
            "All costs, capacities, fees, timing fixtures and native reference bytes are synthetic, not measured values.",
            "Native inner codecs/signature verification/capital provenance/common cost derivation are trusted fixture inputs.",
            "Seed/closed/ready-root derivations prefixed TEST are deterministic test adapters, not allocated wire formats or beacon security proofs.",
            "The reservation adapter aggregates bounded current/future vectors and stock; full RFC-0055 actor graph, ClockBridge and court planner are external.",
            "Qualified demand is derived from candidate outcomes; output truth is deliberately not an admission input.",
            "Local ingress and restoration readiness are excluded from canonical difficulty observations.",
            "10,000/50,000 model-ID loops check identifier invariance only; no real registry/model/load benchmark was run.",
        ],
        "not_verified": [
            "Native route integration, actual native inner codecs, signatures, DB/index/reorg implementation and production allocator.",
            "Beacon unpredictability/bias resistance, real carrier service/inclusion/fairness and off-chain censorship.",
            "Actual LLM/Panel/provider cold acquisition, full rerun, public court, Final, P_m and finite credit/right issuance.",
            "Hardware performance, CPU/RAM/I/O/retention limits, hostile network throughput, 50,000-model registry/IBD scale.",
            "Closed-loop stability, utilization/latency target achievement, actual sustained 1 BPS, deployment or activation.",
        ],
    }
    encoded = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.stdout_only:
        print(encoded)
    else:
        args.output.write_text(encoded, encoding="utf-8")
        print(f"Saved {args.output}: {report['passed']}/{report['tests_run']} PASS")
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    sys.exit(main())
