#!/usr/bin/env python3
"""RFC-0057 CPN1 abstract contract model; NOT native/E2E/performance evidence.

This model uses actual SHA-256, typed framing, bounded selection, a small
integer-arithmetic Merkle tree, and semantic one-use/obligation state. Native
signatures, selection, original funding, cost qualification, finality/clock,
and the RFC-0055 all-actor court planner are explicit external fixtures.
The toy operation is not an LLM and Merkle authentication is not a computation
correctness proof. A valid local witness never proves independent full replay.
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import sys
import unittest
from dataclasses import dataclass, field, replace
from datetime import datetime, timezone
from pathlib import Path


H = lambda data: hashlib.sha256(data).digest()
U256_SPACE = 1 << 256
U64_MAX = (1 << 64) - 1


class Rejected(Exception):
    def __init__(self, code: str):
        super().__init__(code)
        self.code = code


def need(condition: bool, code: str) -> None:
    if not condition:
        raise Rejected(code)


def uint(value: int, bits: int) -> int:
    need(type(value) is int and 0 <= value < 1 << bits, "IntegerRange")
    return value


def le(value: int, width: int) -> bytes:
    return uint(value, width * 8).to_bytes(width, "little")


def lp(data: bytes, maximum: int = 4096) -> bytes:
    need(type(data) is bytes and len(data) <= maximum, "LPBound")
    return le(len(data), 4) + data


def nr(tag: str, data: bytes, version: int = 1) -> bytes:
    need(type(tag) is str and len(tag.encode("utf-8")) <= 64, "NativeTag")
    need(version == 1, "NativeCodecVersion")
    return lp(tag.encode("utf-8"), 64) + le(version, 2) + lp(data)


def d32(data: bytes) -> bytes:
    need(type(data) is bytes and len(data) == 32, "DigestWidth")
    return data


@dataclass(frozen=True)
class Obligation:
    network: bytes
    claim: bytes
    origin: bytes
    actor: bytes
    duty: bytes
    round: int
    action_kind: int
    step: int

    def encode(self) -> bytes:
        return (lp(self.network) + nr("claim", self.claim) + nr("origin", self.origin)
                + nr("actor", self.actor) + nr("duty", self.duty)
                + le(self.round, 8) + le(self.action_kind, 2) + le(self.step, 8))

    @property
    def identity(self) -> bytes:
        return H(lp(b"MISAKA/PALW/CPN1/OBLIGATION") + self.encode())


def positive_nonce(obligation: bytes, context: bytes, anchor: bytes, seed: bytes) -> bytes:
    need(type(seed) is bytes and len(seed) == 64, "NativeSeedWidth")
    return H(lp(b"MISAKA/PALW/CPN1/POSITIVE-NONCE") + d32(obligation)
             + d32(context) + nr("beacon-anchor", anchor) + nr("beacon-seed", seed))


def action_nonce(obligation: bytes, context: bytes) -> bytes:
    return H(lp(b"MISAKA/PALW/CPN1/ACTION-NONCE") + d32(obligation) + d32(context))


@dataclass(frozen=True)
class Selection:
    targets: tuple[int, ...]
    hash_calls: int
    peak_map_entries: int


def select_targets(nonce: bytes, n: int, k: int, retries: int, hasher=H) -> Selection:
    """Exact bounded sparse partial Fisher-Yates specified by CPN1."""
    d32(nonce)
    need(type(n) is int and 1 <= n <= U64_MAX, "TargetCount")
    need(type(k) is int and 1 <= k <= n, "SampleCount")
    need(type(retries) is int and 1 <= retries <= (1 << 32) - 1, "RetryCount")
    sparse = {}
    chosen = []
    calls = peak = 0
    for i in range(k):
        width = n - i
        limit = U256_SPACE - U256_SPACE % width
        for attempt in range(retries):
            value = int.from_bytes(d32(hasher(lp(b"MISAKA/PALW/CPN1/DRAW")
                       + nonce + le(i, 8) + le(attempt, 4))), "big")
            calls += 1
            if value < limit:
                j = i + value % width
                chosen.append(sparse.get(j, j))
                replacement = sparse.get(i, i)
                if j != i:
                    sparse[j] = replacement
                sparse.pop(i, None)
                peak = max(peak, len(sparse))
                break
        else:
            raise Rejected("SelectionUnavailable")
    need(len(set(chosen)) == k, "InternalDuplicateSelection")
    return Selection(tuple(sorted(chosen)), calls, peak)


@dataclass(frozen=True)
class Relation:
    """Synthetic canonical signed-i64 operation out = a + b*c."""
    a: int
    b: int
    c: int
    out: int

    def encode(self) -> bytes:
        values = (self.a, self.b, self.c, self.out)
        need(all(type(v) is int and -(1 << 63) <= v < (1 << 63) for v in values), "ToyIntegerRange")
        return b"".join(v.to_bytes(8, "little", signed=True) for v in values)

    def holds(self) -> bool:
        self.encode()
        product = self.b * self.c
        total = self.a + product
        need(-(1 << 63) <= product < (1 << 63) and -(1 << 63) <= total < (1 << 63), "ToyArithmeticOverflow")
        return self.out == total


def leaf_hash(index: int, relation: Relation) -> bytes:
    return H(lp(b"TEST-ONLY/CPN1/MERKLE/LEAF") + le(index, 8) + relation.encode())


def branch_hash(left: bytes, right: bytes) -> bytes:
    return H(lp(b"TEST-ONLY/CPN1/MERKLE/BRANCH") + d32(left) + d32(right))


@dataclass(frozen=True)
class Witness:
    index: int
    relation: Relation
    siblings: tuple[bytes, ...]

    def encode(self) -> bytes:
        need(len(self.siblings) <= 64, "MerkleDepthBound")
        return le(self.index, 8) + self.relation.encode() + le(len(self.siblings), 4) + b"".join(d32(s) for s in self.siblings)


class ToyTrace:
    def __init__(self, rows: tuple[Relation, ...]):
        need(bool(rows) and len(rows) & (len(rows) - 1) == 0, "ToyPowerOfTwoTrace")
        self.rows = rows
        levels = [tuple(leaf_hash(i, row) for i, row in enumerate(rows))]
        while len(levels[-1]) > 1:
            prev = levels[-1]
            levels.append(tuple(branch_hash(prev[i], prev[i + 1]) for i in range(0, len(prev), 2)))
        self.levels = tuple(levels)
        self.root = levels[-1][0]

    def __eq__(self, other):
        return isinstance(other, ToyTrace) and (self.rows, self.levels, self.root) == (other.rows, other.levels, other.root)

    def witness(self, index: int) -> Witness:
        need(0 <= index < len(self.rows), "TargetRange")
        at, siblings = index, []
        for level in self.levels[:-1]:
            siblings.append(level[at ^ 1])
            at //= 2
        return Witness(index, self.rows[index], tuple(siblings))


def verify_witness(witness: Witness, root: bytes, n: int) -> bool:
    d32(root)
    need(0 <= witness.index < n, "TargetRange")
    need(n > 0 and n & (n - 1) == 0, "ToyPowerOfTwoTrace")
    need(len(witness.siblings) == n.bit_length() - 1, "MerkleDepth")
    witness.encode()
    value, at = leaf_hash(witness.index, witness.relation), witness.index
    for sibling in witness.siblings:
        value = branch_hash(value, sibling) if at % 2 == 0 else branch_hash(sibling, value)
        at //= 2
    need(value == root, "OpeningMismatch")
    return witness.relation.holds()


@dataclass(frozen=True)
class AuthToken:
    """Trusted native-signature verifier fixture, not a signature algorithm."""
    signing_key: bytes
    subject: bytes
    valid: bool = True


@dataclass(frozen=True)
class Body:
    statement: str
    witnesses: tuple[Witness, ...]

    def encode(self) -> bytes:
        need(self.statement in ("FullCanonicalReplayExact", "LocalHolds", "ComputationRefuted", "Notice", "Response"), "StatementScope")
        indices = tuple(w.index for w in self.witnesses)
        need(indices == tuple(sorted(set(indices))), "TargetOrderOrDuplicate")
        need(len(self.witnesses) <= 128, "ToyWitnessBound")
        return lp(self.statement.encode(), 64) + le(len(self.witnesses), 4) + b"".join(lp(w.encode(), 8192) for w in self.witnesses)



CTX_HEAD = ("claim", "origin", "miner-commit", "execution-root", "program", "model",
            "input", "output", "profile", "ruleset", "draw", "assignment", "actor",
            "role", "duty")
CTX_TAIL = ("plan", "fee-liability", "deadline")
STATEMENTS = {"FullCanonicalReplayExact": 1, "LocalHolds": 2,
              "ComputationRefuted": 3, "Notice": 4, "Response": 5}
MAX_BODY = 65_536  # Synthetic qualified fixture bounds, not protocol defaults.
PART_SIZE = 128


@dataclass(frozen=True)
class Context:
    network: bytes
    fields: tuple[tuple[str, bytes], ...]
    round: int
    action_kind: int
    step: int

    def encode(self) -> bytes:
        need(tuple(tag for tag, _ in self.fields) == CTX_HEAD + CTX_TAIL, "ContextFieldOrder")
        head, tail = self.fields[:len(CTX_HEAD)], self.fields[len(CTX_HEAD):]
        data = (le(1, 2) + lp(self.network)
                + b"".join(nr(tag, value) for tag, value in head)
                + le(self.round, 8) + le(self.action_kind, 2) + le(self.step, 8)
                + b"".join(nr(tag, value) for tag, value in tail))
        need(len(data) <= 65_536, "ContextBytes")
        return data

    @property
    def identity(self) -> bytes:
        return H(lp(b"MISAKA/PALW/CPN1/CONTEXT") + self.encode())

    def field(self, tag: str) -> bytes:
        return dict(self.fields)[tag]

    def changed(self, tag: str, value: bytes) -> Context:
        return replace(self, fields=tuple((t, value if t == tag else v) for t, v in self.fields))


def decode_context(data: bytes) -> Context:
    """The CPN1 outer codec is exact; native inner codecs are fixtures."""
    need(type(data) is bytes and len(data) <= 65_536, "ContextBytes")
    pos = 0

    def take(size):
        nonlocal pos
        need(0 <= size <= len(data) - pos, "TruncatedContext")
        value = data[pos:pos + size]
        pos += size
        return value

    def integer(size):
        return int.from_bytes(take(size), "little")

    def variable(maximum):
        length = integer(4)
        need(length <= maximum, "LPBound")
        return take(length)

    def native(expected):
        need(variable(64) == expected.encode(), "ContextFieldOrder")
        need(integer(2) == 1, "NativeCodecVersion")
        return expected, variable(4096)

    need(integer(2) == 1, "ContextVersion")
    network = variable(4096)
    head = tuple(native(tag) for tag in CTX_HEAD)
    round_, action, step = integer(8), integer(2), integer(8)
    tail = tuple(native(tag) for tag in CTX_TAIL)
    need(pos == len(data), "TrailingContext")
    return Context(network, head + tail, round_, action, step)


def target_digest(indices: tuple[int, ...]) -> bytes:
    need(indices == tuple(sorted(set(indices))), "TargetOrderOrDuplicate")
    # The inner native "target" is a synthetic u64 index codec.
    return H(lp(b"MISAKA/PALW/CPN1/TARGETS") + le(len(indices), 4)
             + b"".join(nr("target", le(index, 8)) for index in indices))


def body_digest(data: bytes) -> bytes:
    return H(lp(b"MISAKA/PALW/CPN1/BODY") + lp(data, MAX_BODY))


def chunk_leaf(index: int, data: bytes) -> bytes:
    return H(lp(b"MISAKA/PALW/CPN1/CHUNK") + le(index, 4) + lp(data, MAX_BODY))


def chunk_parent(left: bytes, right: bytes) -> bytes:
    return H(lp(b"MISAKA/PALW/CPN1/CHUNK-NODE") + d32(left) + d32(right))


class ChunkTree:
    def __init__(self, data: bytes, width: int = PART_SIZE):
        need(type(data) is bytes and len(data) <= MAX_BODY, "ChunkBodyBound")
        need(type(width) is int and 0 < width <= MAX_BODY, "ChunkWidth")
        self.width, self.length = width, len(data)
        self.chunks = tuple(data[i:i + width] for i in range(0, len(data), width)) or (b"",)
        n = len(self.chunks)
        leaves = [chunk_leaf(i, item) for i, item in enumerate(self.chunks)]
        padded = 1 << (n - 1).bit_length()
        leaves.extend(H(lp(b"MISAKA/PALW/CPN1/CHUNK-PAD") + le(i, 4)) for i in range(n, padded))
        levels = [tuple(leaves)]
        while len(levels[-1]) > 1:
            previous = levels[-1]
            levels.append(tuple(chunk_parent(previous[i], previous[i + 1])
                                for i in range(0, len(previous), 2)))
        self.levels, self.root = tuple(levels), levels[-1][0]

    def opening(self, index: int) -> tuple[bytes, ...]:
        need(0 <= index < len(self.chunks), "ChunkIndex")
        at, proof = index, []
        for level in self.levels[:-1]:
            proof.append(level[at ^ 1])
            at //= 2
        return tuple(proof)


def verify_chunk(root: bytes, length: int, index: int, data: bytes,
                 proof: tuple[bytes, ...], width: int = PART_SIZE) -> None:
    need(type(length) is int and 0 <= length <= MAX_BODY, "ChunkBodyBound")
    need(type(width) is int and 0 < width <= MAX_BODY, "ChunkWidth")
    n = max(1, (length + width - 1) // width)
    need(type(index) is int and 0 <= index < n, "ChunkIndex")
    need(len(proof) == (n - 1).bit_length(), "ChunkDepth")
    need(len(data) == min(width, length - index * width), "ChunkLength")
    value, at = chunk_leaf(index, data), index
    for sibling in proof:
        value = chunk_parent(value, sibling) if at % 2 == 0 else chunk_parent(sibling, value)
        at //= 2
    need(value == d32(root), "ChunkOpening")


@dataclass(frozen=True)
class Envelope:
    obligation: Obligation
    context_hash: bytes
    nonce: bytes
    statement_kind: int
    body_length: int
    body_hash: bytes
    chunk_root: bytes
    targets_hash: bytes
    body: Body
    auth: AuthToken
    signature_bytes: bytes = b"native-signature-fixture"
    protocol: str = "CPN1"

    @property
    def subject(self) -> bytes:
        return (lp(b"MISAKA/PALW/CPN1/SUBMISSION") + le(1, 2)
                + self.obligation.identity + d32(self.context_hash) + d32(self.nonce)
                + le(self.statement_kind, 2) + le(self.body_length, 4)
                + d32(self.body_hash) + d32(self.chunk_root) + d32(self.targets_hash))

    @property
    def identity(self) -> bytes:
        return H(self.subject)

    def check_body(self) -> None:
        data = self.body.encode()
        need(len(data) <= MAX_BODY and self.body_length == len(data), "BodyLength")
        need(self.statement_kind == STATEMENTS[self.body.statement], "StatementBinding")
        need(self.body_hash == body_digest(data), "BodyDigest")
        need(self.chunk_root == ChunkTree(data).root, "ChunkRoot")
        need(self.targets_hash == target_digest(tuple(w.index for w in self.body.witnesses)), "TargetDigest")


def envelope(obligation: Obligation, context: Context, nonce: bytes,
             body: Body, signing_key: bytes) -> Envelope:
    data = body.encode()
    result = Envelope(obligation, context.identity, nonce, STATEMENTS[body.statement],
                      len(data), body_digest(data),
                      ChunkTree(data).root,
                      target_digest(tuple(w.index for w in body.witnesses)),
                      body, AuthToken(signing_key, b""))
    return replace(result, auth=AuthToken(signing_key, result.subject))


@dataclass(frozen=True)
class Duty:
    obligation: Obligation
    context: Context
    key: bytes
    positive: bool
    targets: tuple[int, ...]
    deadline: int
    selected: bool = True
    funds_qualified: bool = True
    finite_liability: bool = True
    liability_limit: int = 7   # Synthetic native contract fixture.
    liability_until: int = 30


@dataclass(frozen=True)
class Accepted:
    submission: bytes
    principal: bytes
    statement: str
    root: bytes
    obligation: bytes


@dataclass
class Assembly:
    """Canonical CPN1 chunk commitment; body/native inner wire remains a fixture."""
    submission: bytes
    body_length: int
    body_hash: bytes
    chunk_root: bytes
    deadline: int
    reserved_bytes: int
    parts: dict[int, bytes] = field(default_factory=dict)

    @classmethod
    def begin(cls, env: Envelope, now: int, deadline: int, capacity: int,
              native_header_authenticated: bool = True) -> Assembly:
        need(native_header_authenticated, "UnauthenticatedHeader")
        need(now < deadline, "StageDeadline")
        need(0 < env.body_length <= MAX_BODY, "StageSize")
        data = env.body.encode()
        need(env.body_length == len(data) and env.body_hash == body_digest(data), "StageHeaderBinding")
        need(len(data) <= capacity, "StageCapacity")
        return cls(env.identity, len(data), env.body_hash,
                   env.chunk_root, deadline, len(data))

    def add(self, index: int, data: bytes, proof: tuple[bytes, ...], now: int) -> None:
        need(now < self.deadline, "StageDeadline")
        verify_chunk(self.chunk_root, self.body_length, index, data, proof)
        if index in self.parts:
            need(self.parts[index] == data, "ChunkConflict")
        else:
            self.parts[index] = data

    def complete(self) -> bytes:
        n = max(1, (self.body_length + PART_SIZE - 1) // PART_SIZE)
        need(len(self.parts) == n, "IncompleteBody")
        data = b"".join(self.parts[i] for i in range(n))
        need(len(data) == self.body_length and body_digest(data) == self.body_hash, "BodyDigest")
        return data


@dataclass
class IngressBudget:
    remaining: int
    maximum_bytes: int = MAX_BODY

    def check(self, size: int) -> None:
        need(type(size) is int and 0 <= size <= self.maximum_bytes, "IngressSize")
        need(self.remaining > 0, "IngressExhausted")
        self.remaining -= 1


class CourtModel:
    """Finite fixture of native duty / once / coverage / liability integration."""

    def __init__(self, trace: ToyTrace | None = None, actors: tuple[bytes, ...] = (b"A", b"B"),
                 k: int = 3, retries: int = 4, profile: bytes = b"CPN1-profile",
                 global_capacity: int = 8192):
        if trace is None:
            trace = ToyTrace(tuple(Relation(i, 3, 5, i + 15) for i in range(16)))
        self.trace = trace
        self.actors = tuple(dict.fromkeys(actors))  # Native principal normalisation.
        need(bool(self.actors), "NoSelectedPrincipal")
        need(1 <= k <= len(trace.rows) and len(self.actors) <= 32, "QualifiedProfileBound")
        need(1 <= retries <= (1 << 32) - 1, "RetryCount")
        reservation = len(self.actors) * (k * retries + 32) + 64
        need(reservation <= global_capacity, "OriginalCapacity")
        self.original_reservation = reservation
        self.global_capacity = global_capacity
        self.k, self.retries = k, retries
        self.profile = profile
        self.network = b"test-network"
        self.claim = b"claim-native-id:" + b"c" * 48
        self.origin = b"accepted-origin:" + b"o" * 48
        self.assignment = b"native-draw-assignment"
        self.keys = {actor: b"key:" + actor for actor in self.actors}
        self.duties: dict[bytes, Duty] = {}
        self.accepted: dict[bytes, Accepted] = {}
        self.tombstones: set[bytes] = set()
        self.approvals: set[bytes] = set()
        self.local_coverage: set[int] = set()
        self.common_targets = frozenset((0, len(trace.rows) - 1))
        self.common_coverage: set[int] = set()
        self.materials: dict[bytes, Witness] = {}
        self.pending: set[bytes] = set()
        self.receipts: set[bytes] = set()
        self.entitlements: dict[tuple[bytes, str], int] = {}
        self.paid: set[tuple[bytes, str]] = set()
        self.offences: dict[tuple[bytes, str], int] = {}
        self.collected: set[tuple[bytes, str]] = set()
        self.collateral = {actor: 7 for actor in self.actors}
        self.positive_debit = 0
        self.negative_remaining = {actor: 8 for actor in self.actors}
        self.refuted = self.final = self.aborted = False
        self.source_rights_live = True
        self.restored = True
        self.now = 0
        self.commit_at, self.seed_cut = 2, 8
        self.notice_until, self.hard_until = 10, 20
        self.seed = self.anchor = None
        self.selection_unavailable: set[bytes] = set()
        self.generation = 0
        self.new_claim_difficulty = 1
        self.active_new_profile = profile
        self.ingress = IngressBudget(10_000)
        for actor in self.actors:
            self._register_duty(actor, b"positive-duty", 1, True)

    def _context(self, obligation: Obligation, third_party: bool = False) -> Context:
        values = {
            "claim": self.claim, "origin": self.origin, "miner-commit": b"fixed-miner-commit",
            "execution-root": self.trace.root, "program": b"toy-program",
            "model": b"native-model-id:" + b"m" * 64, "input": b"input-root",
            "output": b"output-root", "profile": self.profile, "ruleset": b"exact-i64",
            "draw": b"\x00" if third_party else b"native-selected-draw",  # Native absence fixture.
            "assignment": self.assignment, "actor": obligation.actor,
            "role": b"native-respondent" if obligation.action_kind == 4 else (b"third-party" if third_party else b"selected-panel"),
            "duty": obligation.duty,
            "plan": b"synthetic-plan-v1" + le(len(self.trace.rows), 8) + le(self.k, 8)
                    + le(self.retries, 4) + target_digest(tuple(sorted(self.common_targets))),
            "fee-liability": b"funded:fee=1;cap=7;until=30;original-recipient",
            "deadline": b"native-clock:positive<20;liability<=30",
        }
        return Context(self.network, tuple((t, values[t]) for t in CTX_HEAD + CTX_TAIL),
                       obligation.round, obligation.action_kind, obligation.step)

    def _register_duty(self, actor: bytes, duty_name: bytes, action: int,
                       positive: bool, third_party: bool = False,
                       round_: int = 0, step: int = 0) -> Duty:
        ob = Obligation(self.network, self.claim, self.origin, actor,
                        duty_name + b":" + actor, round_, action, step)
        context = self._context(ob, third_party)
        if ob.identity in self.duties:
            existing = self.duties[ob.identity]
            need(existing.context == context, "NativeDutyMutation")
            return existing
        if not positive:
            need(sum(not d.positive and d.obligation.actor == actor for d in self.duties.values()) < 8,
                 "NativeDutyCount")
        key = self.keys.get(actor, b"third-party-key:" + actor)
        record = Duty(ob, context, key, positive, (), self.hard_until,
                      selected=not third_party)
        self.duties[ob.identity] = record
        return record

    def positive_duty(self, actor: bytes) -> Duty:
        return next(d for d in self.duties.values()
                    if d.positive and d.obligation.actor == actor)

    def provide_seed(self, seed: bytes = H(b"seed-low") + H(b"seed-high"), anchor: bytes = b"native-anchor",
                     at: int = 4, hasher=H) -> None:
        need(self.seed is None, "SeedAlreadyFixed")
        need(self.commit_at < at < self.seed_cut, "SeedTime")
        need(type(seed) is bytes and len(seed) == 64, "NativeSeedWidth")
        self.seed, self.anchor = seed, anchor
        for actor in self.actors:
            duty = self.positive_duty(actor)
            nonce = positive_nonce(duty.obligation.identity, duty.context.identity, anchor, seed)
            try:
                targets = select_targets(nonce, len(self.trace.rows), self.k, self.retries, hasher).targets
            except Rejected as error:
                if error.code != "SelectionUnavailable":
                    raise
                self.selection_unavailable.add(duty.obligation.identity)
                continue
            self.duties[duty.obligation.identity] = replace(duty, targets=targets)

    def nonce(self, duty: Duty) -> bytes:
        if duty.positive:
            need(self.seed is not None and self.anchor is not None, "SeedUnavailable")
            need(duty.obligation.identity not in self.selection_unavailable, "SelectionUnavailable")
            return positive_nonce(duty.obligation.identity, duty.context.identity, self.anchor, self.seed)
        return action_nonce(duty.obligation.identity, duty.context.identity)

    def make_positive(self, actor: bytes, statement: str = "FullCanonicalReplayExact",
                      targets: tuple[int, ...] | None = None) -> Envelope:
        duty = self.positive_duty(actor)
        indices = duty.targets if targets is None else targets
        return envelope(duty.obligation, duty.context, self.nonce(duty),
                        Body(statement, tuple(self.trace.witness(i) for i in indices)), duty.key)

    def make_action(self, actor: bytes, statement: str, index: int | None = None,
                    duty_name: bytes = b"negative-duty", third_party: bool = False,
                    step: int = 0) -> Envelope:
        # Authority to create this finite native duty is a trusted planner fixture,
        # never a field that an untrusted network packet can mint.
        need(actor in self.actors or third_party, "NativeActionEligibility")
        need(type(step) is int and 0 <= step < 4 and len(duty_name) <= 64, "NativeStepBound")
        if third_party and actor not in self.negative_remaining:
            need(len(self.negative_remaining) < len(self.actors) + 2, "ThirdPartyBudget")
            self.negative_remaining[actor] = 8
        action = 2 if statement == "ComputationRefuted" else (4 if statement == "Response" else 3)
        duty = self._register_duty(actor, duty_name, action, False, third_party, step=step)
        witnesses = () if index is None else (self.trace.witness(index),)
        return envelope(duty.obligation, duty.context, self.nonce(duty),
                        Body(statement, witnesses), duty.key)

    def _validate(self, env: Envelope) -> Duty:
        self.ingress.check(env.body_length)
        need(self.restored, "RestorationUnavailable")
        need(env.protocol == "CPN1", "RouteVersion")
        oid = env.obligation.identity
        need(oid in self.duties, "UnknownNativeDuty")
        duty = self.duties[oid]
        need(env.obligation == duty.obligation, "NativeDutyMismatch")
        need(env.context_hash == duty.context.identity, "ContextMismatch")
        need(env.nonce == self.nonce(duty), "NonceMismatch")
        need(env.auth.valid and env.auth.signing_key == duty.key
             and env.auth.subject == env.subject, "NativeAuthentication")
        need(self.now < duty.deadline, "NativeDeadline")
        need(duty.funds_qualified, "NativeFunds")
        env.check_body()
        return duty

    def submit(self, env: Envelope, worker_generation: int | None = None) -> str:
        if worker_generation is not None:
            need(worker_generation == self.generation, "StaleWorker")
        duty = self._validate(env)
        oid = duty.obligation.identity
        need(oid not in self.tombstones, "ConsumedTombstone")
        if oid in self.accepted:
            need(self.accepted[oid].submission == env.identity, "SlotConflict")
            return "Idempotent"
        need(not self.aborted, "ClaimClosed")
        actor = duty.obligation.actor
        positive = env.body.statement in ("FullCanonicalReplayExact", "LocalHolds")
        if positive:
            need(duty.positive, "ActionScope")
            need(tuple(w.index for w in env.body.witnesses) == duty.targets, "ExpectedTargets")
            need(not self.refuted, "ClaimRefuted")
            if env.body.statement == "FullCanonicalReplayExact":
                need(duty.selected and duty.finite_liability
                     and duty.liability_until >= 30
                     and self.collateral.get(actor, 0) >= duty.liability_limit, "FullApprovalQualification")
            for witness in env.body.witnesses:
                need(verify_witness(witness, self.trace.root, len(self.trace.rows)), "LocalRelationRefuted")
        else:
            need(not duty.positive, "ActionScope")
            need(self.negative_remaining.get(actor, 0) > 0, "ProtectedActionBudget")
            if env.body.statement == "ComputationRefuted":
                need(len(env.body.witnesses) == 1, "RefutationShape")
                need(not verify_witness(env.body.witnesses[0], self.trace.root, len(self.trace.rows)),
                     "NoComputationRefutation")
            elif env.body.statement == "Notice":
                need(not env.body.witnesses, "NoticeShape")
            elif env.body.statement == "Response":
                need(len(env.body.witnesses) == 1, "ResponseShape")
                verify_witness(env.body.witnesses[0], self.trace.root, len(self.trace.rows))
            else:
                raise Rejected("UnsupportedActionFixture")
        # Everything before this point is pure with respect to canonical state.
        record = Accepted(env.identity, actor, env.body.statement, self.trace.root, oid)
        self.accepted[oid] = record
        self.receipts.add(oid)
        for witness in env.body.witnesses:
            self.materials[H(witness.encode())] = witness
        if positive:
            self.positive_debit += len(env.body.witnesses) + 1
            self.local_coverage.update(w.index for w in env.body.witnesses)
            self.common_coverage.update(w.index for w in env.body.witnesses
                                        if w.index in self.common_targets)
            if env.body.statement == "FullCanonicalReplayExact":
                self.approvals.add(actor)
        else:
            self.negative_remaining[actor] -= 1
            if env.body.statement == "Notice":
                self.pending.add(oid)
            elif env.body.statement == "ComputationRefuted":
                self.refuted, self.final, self.source_rights_live = True, False, False
                for approval in self.accepted.values():
                    if approval.statement == "FullCanonicalReplayExact" and approval.root == self.trace.root:
                        full_duty = self.duties[approval.obligation]
                        if full_duty.finite_liability:
                            key = (approval.principal, "FalseFullApproval")
                            self.offences.setdefault(key, full_duty.liability_limit)
        return "Accepted"

    def supply_common(self, indices: tuple[int, ...] | None = None) -> None:
        # Existing 0017 common-coverage adapter with authenticated native scope.
        for index in (tuple(sorted(self.common_targets)) if indices is None else indices):
            need(index in self.common_targets, "CommonTargetScope")
            witness = self.trace.witness(index)
            need(verify_witness(witness, self.trace.root, len(self.trace.rows)), "LocalRelationRefuted")
            self.common_coverage.add(index)
            self.materials[H(witness.encode())] = witness

    def finalize(self) -> bool:
        need(self.restored, "RestorationUnavailable")
        countable = self.approvals & {record.principal for oid, record in self.accepted.items()
                    if record.statement == "FullCanonicalReplayExact" and oid in self.receipts}
        okay = (bool(countable) and self.common_targets <= self.common_coverage
                and self.notice_until <= self.now < self.hard_until
                and not self.pending and not self.refuted and not self.aborted
                and self.source_rights_live)
        if okay:
            self.final = True
        return okay

    def canonical_batch(self, actions: tuple[Envelope | str, ...]) -> None:
        # Qualified notices/refutations precede Final in this test adapter.
        checkpoint = self.snapshot()
        try:
            packets = [a for a in actions if isinstance(a, Envelope)]
            packets.sort(key=lambda a: 0 if a.body.statement in ("Notice", "ComputationRefuted") else 1)
            for packet in packets:
                self.submit(packet)
            for action in actions:
                if action == "Final":
                    self.finalize()
        except Rejected:
            self.restore(checkpoint, reorg=False)
            raise

    def native_entitlement(self, actor: bytes, kind: str, amount: int) -> None:
        need(kind in ("success", "performed-service") and amount > 0, "Entitlement")
        # This is an external native funding event, not a consequence of a nonce.
        self.entitlements[(actor, kind)] = amount

    def settle(self, actor: bytes, kind: str) -> int:
        key = (actor, kind)
        need(key in self.entitlements, "NoNativeEntitlement")
        need(actor in {a.principal for a in self.accepted.values()}, "NoPerformedDuty")
        if kind == "success":
            need(self.final and not self.refuted, "SuccessNotFinal")
        if key in self.paid:
            return 0
        self.paid.add(key)
        return self.entitlements[key]

    def collect_liability(self, actor: bytes) -> int:
        key = (actor, "FalseFullApproval")
        need(key in self.offences, "NoObjectiveOffence")
        if key in self.collected:
            return 0
        amount = self.offences[key]
        need(self.collateral.get(actor, 0) >= amount, "LiabilityBacking")
        self.collateral[actor] -= amount
        self.collected.add(key)
        return amount

    def release_collateral(self, actor: bytes) -> int:
        need(not self.pending and self.now > 30, "LiabilityRetention")
        need(all(key in self.collected for key in self.offences if key[0] == actor), "OutstandingDue")
        amount = self.collateral[actor]
        self.collateral[actor] = 0
        return amount

    def prune_evidence(self) -> None:
        self.materials.clear()
        self.tombstones.update(self.accepted)

    def cancel(self) -> None:
        self.aborted = True

    def snapshot(self) -> dict:
        return copy.deepcopy({key: value for key, value in self.__dict__.items()
                              if key not in ("ingress", "generation")})

    def restore(self, snapshot: dict, reorg: bool = True) -> None:
        ingress, generation = self.ingress, self.generation
        self.__dict__.clear()
        self.__dict__.update(copy.deepcopy(snapshot))
        self.ingress = ingress
        self.generation = generation + (1 if reorg else 0)

    def restart(self) -> CourtModel:
        restored = object.__new__(CourtModel)
        restored.__dict__ = copy.deepcopy(self.__dict__)
        return restored


def ready_model(**kwargs) -> CourtModel:
    model = CourtModel(**kwargs)
    model.provide_seed()
    return model


def counterexample_model(k: int = 2) -> tuple[CourtModel, int]:
    """Find a finite toy fixture whose corrupt point is outside both samples.

    This is a counterexample demonstration, not a stochastic security estimate
    or an allowed sender search in the production protocol.
    """
    for bad in range(1, 15):
        rows = tuple(Relation(i, 3, 5, i + 15 + (1 if i == bad else 0)) for i in range(16))
        model = ready_model(trace=ToyTrace(rows), k=k)
        if all(bad not in model.positive_duty(actor).targets for actor in model.actors):
            return model, bad
    raise AssertionError("No finite counterexample fixture found")


class ContractTests(unittest.TestCase):
    def rejects(self, code, operation, *args, **kwargs):
        with self.assertRaises(Rejected) as caught:
            operation(*args, **kwargs)
        self.assertEqual(caught.exception.code, code)

    def test_01_typed_context_roundtrip_preserves_native_width_and_absence(self):
        """Native ID/seed widths and canonical route absence are preserved."""
        m = CourtModel()
        context = m.positive_duty(b"A").context
        self.assertEqual(decode_context(context.encode()), context)
        self.assertGreater(len(context.field("model")), 32)
        action = m.make_action(b"third", "Notice", third_party=True)
        self.assertEqual(m.duties[action.obligation.identity].context.field("draw"), b"\x00")
        self.assertNotEqual(lp(b"ab") + lp(b"c"), lp(b"a") + lp(b"bc"))
        self.rejects("LPBound", lp, b"x" * 4097)
        self.rejects("IntegerRange", le, -1, 8)

    def test_02_context_codec_rejects_noncanonical_truncation_trailing_and_version(self):
        """Bounds and expected field order precede decoding native content."""
        context = CourtModel().positive_duty(b"A").context
        raw = context.encode()
        self.rejects("TrailingContext", decode_context, raw + b"x")
        self.rejects("TruncatedContext", decode_context, raw[:-1])
        self.rejects("ContextVersion", decode_context, b"\x02\x00" + raw[2:])
        swapped = replace(context, fields=(context.fields[1], context.fields[0]) + context.fields[2:])
        self.rejects("ContextFieldOrder", swapped.encode)
        self.rejects("LPBound", decode_context, b"\x01\x00" + le(5000, 4))

    def test_03_obligation_changes_only_with_native_duty_coordinates(self):
        """Each native scope dimension changes O; changing result/seed does not."""
        ob = CourtModel().positive_duty(b"A").obligation
        for name, value in (
            ("network", b"other"), ("claim", b"other"), ("origin", b"other"),
            ("actor", b"other"), ("duty", b"other"), ("round", 1),
            ("action_kind", 2), ("step", 1),
        ):
            with self.subTest(field=name):
                self.assertNotEqual(ob.identity, replace(ob, **{name: value}).identity)
        self.rejects("IntegerRange", replace(ob, round=1 << 64).encode)

    def test_04_all_pinned_context_dimensions_bind_without_minting_new_obligation(self):
        """Root/profile/role/fee/plan changes cannot buy a new native duty."""
        m = ready_model()
        duty = m.positive_duty(b"A")
        for tag, value in duty.context.fields:
            with self.subTest(field=tag):
                changed = duty.context.changed(tag, value + b"changed")
                self.assertNotEqual(changed.identity, duty.context.identity)
                e = envelope(duty.obligation, changed, m.nonce(duty),
                             Body("FullCanonicalReplayExact", tuple(m.trace.witness(i) for i in duty.targets)), duty.key)
                self.rejects("ContextMismatch", m.submit, e)
        self.assertFalse(m.accepted)

    def test_05_native_seed_tail_is_bound_and_truncation_rejected(self):
        """The 64-byte native beacon tail affects nonce; 32-byte truncation fails."""
        duty = CourtModel().positive_duty(b"A")
        common = b"x" * 32
        first = positive_nonce(duty.obligation.identity, duty.context.identity, b"anchor", common + b"a" * 32)
        second = positive_nonce(duty.obligation.identity, duty.context.identity, b"anchor", common + b"b" * 32)
        self.assertNotEqual(first, second)
        self.rejects("NativeSeedWidth", positive_nonce, duty.obligation.identity, duty.context.identity, b"anchor", common)

    def test_06_plan_spec_is_preselection_and_not_a_circular_target_commit(self):
        """Context PlanSpec is fixed before seed and target derivation."""
        m = CourtModel()
        before = {a: m.positive_duty(a).context for a in m.actors}
        m.provide_seed()
        self.assertEqual(before, {a: m.positive_duty(a).context for a in m.actors})
        self.assertTrue(all(m.positive_duty(a).targets for a in m.actors))

    def test_07_subject_excludes_signature_bytes_but_native_auth_remains_required(self):
        """Re-signing changes neither submission identity nor obligations."""
        m = ready_model()
        e = m.make_positive(b"A")
        resigned = replace(e, signature_bytes=b"another-valid-native-signature")
        self.assertEqual(e.identity, resigned.identity)
        self.assertEqual(m.submit(e), "Accepted")
        before = m.snapshot()
        self.assertEqual(m.submit(resigned), "Idempotent")
        self.assertEqual(m.snapshot(), before)
        self.rejects("NativeAuthentication", m.submit, replace(e, auth=replace(e.auth, valid=False)))

    def test_08_wrong_panel_or_key_cannot_relabel_a_completed_envelope(self):
        """Copying A's whole Court to B fails before B's duty is consumed."""
        m = ready_model()
        e = m.make_positive(b"A")
        b = m.positive_duty(b"B")
        before = m.snapshot()
        self.rejects("ContextMismatch", m.submit, replace(e, obligation=b.obligation))
        self.rejects("NativeAuthentication", m.submit, replace(e, auth=replace(e.auth, signing_key=b.key)))
        self.assertEqual(m.snapshot(), before)

    def test_09_client_nonce_round_step_anchor_or_seed_cannot_reroll(self):
        """The packet cannot change the native slot or fixed challenge."""
        m = ready_model()
        e = m.make_positive(b"A")
        for field in ("round", "step"):
            self.rejects("UnknownNativeDuty", m.submit,
                         replace(e, obligation=replace(e.obligation, **{field: 1})))
        self.rejects("NonceMismatch", m.submit, replace(e, nonce=H(b"client chosen")))
        self.rejects("SeedAlreadyFixed", m.provide_seed, b"x" * 64, b"different")
        self.assertFalse(m.accepted)

    def test_10_seed_order_and_ready_cut_are_strict_without_guilt_or_clock_reset(self):
        """Precommit or late seed is unavailable, not computational guilt."""
        for at in (0, 2, 8, 9):
            m = CourtModel()
            before = m.snapshot()
            self.rejects("SeedTime", m.provide_seed, at=at)
            self.assertEqual(m.snapshot(), before)
            self.assertFalse(m.offences)
            self.assertEqual(m.hard_until, 20)

    def test_11_negative_nonce_does_not_wait_for_seed_or_new_admission_difficulty(self):
        """An existing protected native notice uses ActionNonce without CAD1."""
        m = CourtModel()
        m.new_claim_difficulty = (1 << 120) - 1
        e = m.make_action(b"A", "Notice")
        self.assertEqual(e.nonce, action_nonce(e.obligation.identity, e.context_hash))
        self.assertEqual(m.submit(e), "Accepted")
        self.assertEqual(len(m.pending), 1)
        self.assertIsNone(m.seed)

    def test_12_sparse_selector_is_deterministic_unique_sorted_and_bounded(self):
        """Actual SHA-256 selection uses at most kR hashes and O(k) entries."""
        nonce = H(b"selector")
        one = select_targets(nonce, 101, 17, 5)
        two = select_targets(nonce, 101, 17, 5)
        self.assertEqual(one, two)
        self.assertEqual(len(one.targets), 17)
        self.assertEqual(tuple(sorted(set(one.targets))), one.targets)
        self.assertTrue(all(0 <= x < 101 for x in one.targets))
        self.assertLessEqual(one.hash_calls, 17 * 5)
        self.assertLessEqual(one.peak_map_entries, 17)

    def test_13_large_target_space_does_not_require_full_enumeration(self):
        """A u64-sized abstract target space uses only the small sample map."""
        result = select_targets(H(b"huge-index"), U64_MAX, 5, 3)
        self.assertEqual(len(result.targets), 5)
        self.assertLessEqual(result.hash_calls, 15)
        self.assertLessEqual(result.peak_map_entries, 5)

    def test_14_forced_bounded_rejection_exhausts_and_has_no_nonce_fallback(self):
        """Finite hash rejection reaches SelectionUnavailable at the bound."""
        calls = []
        def forced(data):
            calls.append(data)
            return b"\xff" * 32
        self.rejects("SelectionUnavailable", select_targets, H(b"nonce"), 3, 2, 4, forced)
        self.assertEqual(len(calls), 4)
        self.assertEqual(len(set(calls)), 4)
        calls.clear()
        self.rejects("SelectionUnavailable", select_targets, H(b"nonce"), 3, 2, 4, forced)
        self.assertEqual(len(calls), 4)  # Same deterministic failure, not a new seed.
        m = CourtModel(k=2)
        m.provide_seed(hasher=forced)
        used = len(calls)
        self.assertEqual(len(m.selection_unavailable), len(m.actors))
        for _ in range(3):
            self.rejects("SelectionUnavailable", m.make_positive, b"A")
        self.assertEqual(len(calls), used)  # Persisted outcome: no renewed hash budget.
        self.assertFalse(m.offences)

    def test_15_selector_edge_cases_and_invalid_parameters(self):
        """N=1 and k=N work, invalid widths/counts are rejected."""
        self.assertEqual(select_targets(H(b"x"), 1, 1, 1).targets, (0,))
        self.assertEqual(select_targets(H(b"x"), 16, 16, 4).targets, tuple(range(16)))
        for n, k, r, error in ((0, 1, 1, "TargetCount"), (1 << 64, 1, 1, "TargetCount"),
                                (4, 0, 1, "SampleCount"), (4, 5, 1, "SampleCount"),
                                (4, 1, 0, "RetryCount"), (4, 1, 1 << 32, "RetryCount")):
            self.rejects(error, select_targets, H(b"x"), n, k, r)

    def test_16_countable_record_survives_consumption_and_cannot_exist_without_once(self):
        """Consumed once is required for the accepted approval, not its negation."""
        m = ready_model()
        e = m.make_positive(b"A")
        m.submit(e)
        m.supply_common()
        m.now = 10
        self.assertTrue(m.finalize())
        m.final = False
        m.receipts.remove(e.obligation.identity)
        self.assertFalse(m.finalize())

    def test_17_same_bond_positions_or_key_rotation_do_not_add_votes(self):
        """Native actor normalization and re-signing cannot multiply principals."""
        m = ready_model(actors=(b"A", b"A", b"B"))
        self.assertEqual(m.actors, (b"A", b"B"))
        e = m.make_positive(b"A")
        m.submit(e)
        duty = m.positive_duty(b"A")
        new_key = b"rotated-key-A"
        m.duties[duty.obligation.identity] = replace(duty, key=new_key)
        retry = replace(e, auth=AuthToken(new_key, e.subject))
        self.assertEqual(m.submit(retry), "Idempotent")
        self.assertEqual(m.approvals, {b"A"})

    def test_18_one_full_approval_does_not_wait_for_nonresponders_or_shorten_window(self):
        """One eligible completed actor can satisfy the existing minimum after all other conditions."""
        m = ready_model(actors=(b"A", b"B", b"C"))
        m.submit(m.make_positive(b"A"))
        m.supply_common()
        self.assertFalse(m.finalize())
        self.assertEqual(m.notice_until, 10)
        m.now = 10
        self.assertTrue(m.finalize())
        self.assertFalse(m.pending)
        self.assertEqual(m.approvals, {b"A"})

    def test_19_individual_samples_do_not_replace_common_coverage(self):
        """Personal sample completion cannot fill absent mandatory common targets."""
        m = ready_model(k=1)
        m.submit(m.make_positive(b"A"))
        m.now = 10
        self.assertFalse(m.common_targets <= m.common_coverage)
        self.assertFalse(m.finalize())
        m.supply_common()
        self.assertTrue(m.finalize())

    def test_20_local_scope_is_not_promoted_to_full_replay(self):
        """LocalHolds contributes no full-replay approval."""
        m = ready_model(k=16)
        m.submit(m.make_positive(b"A", "LocalHolds"))
        self.assertEqual(len(m.local_coverage), 16)
        self.assertFalse(m.approvals)
        m.now = 10
        self.assertFalse(m.finalize())

    def test_21_same_slot_different_signed_body_is_conflict_not_automatic_slash(self):
        """Byte/body disagreement alone does not prove equivocation or copying."""
        m = ready_model()
        m.submit(m.make_positive(b"A"))
        conflicting = m.make_positive(b"A", "LocalHolds")
        before = m.snapshot()
        self.rejects("SlotConflict", m.submit, conflicting)
        self.assertEqual(m.snapshot(), before)
        self.assertFalse(m.offences)

    def test_22_invalid_signature_or_opening_does_not_poison_the_honest_slot(self):
        """Invalid traffic cannot consume another actor's duty, funds or Q."""
        m = ready_model()
        good = m.make_positive(b"A")
        bad_auth = replace(good, auth=replace(good.auth, valid=False))
        before = m.snapshot()
        self.rejects("NativeAuthentication", m.submit, bad_auth)
        witness = good.body.witnesses[0]
        corrupt = replace(witness, siblings=(H(b"wrong"),) + witness.siblings[1:])
        bad_body = replace(good.body, witnesses=(corrupt,) + good.body.witnesses[1:])
        duty = m.positive_duty(b"A")
        bad = envelope(duty.obligation, duty.context, m.nonce(duty), bad_body, duty.key)
        self.rejects("OpeningMismatch", m.submit, bad)
        self.assertEqual(m.snapshot(), before)
        self.assertEqual(m.submit(good), "Accepted")

    def test_23_flat_body_digest_target_digest_length_and_chunk_root_are_all_bound(self):
        """A valid header is not a substitute for authenticated body contents."""
        m = ready_model()
        e = m.make_positive(b"A")
        for field, value, code in (
            ("body_length", e.body_length + 1, "BodyLength"),
            ("body_hash", H(b"wrong"), "BodyDigest"),
            ("targets_hash", H(b"wrong"), "TargetDigest"),
            ("chunk_root", H(b"wrong"), "ChunkRoot"),
        ):
            altered = replace(e, **{field: value})
            altered = replace(altered, auth=AuthToken(e.auth.signing_key, altered.subject))
            self.rejects(code, m.submit, altered)
        self.assertFalse(m.accepted)

    def test_24_expected_targets_reject_missing_extra_duplicate_and_wrong_order(self):
        """Challenge positions cannot be substituted or counted twice."""
        m = ready_model()
        duty = m.positive_duty(b"A")
        self.rejects("ExpectedTargets", m.submit, m.make_positive(b"A", targets=duty.targets[:-1]))
        self.rejects("ExpectedTargets", m.submit, m.make_positive(b"A", targets=tuple(range(16))))
        bad = Body("FullCanonicalReplayExact", (m.trace.witness(1), m.trace.witness(1)))
        self.rejects("TargetOrderOrDuplicate", bad.encode)
        bad = Body("FullCanonicalReplayExact", (m.trace.witness(2), m.trace.witness(1)))
        self.rejects("TargetOrderOrDuplicate", bad.encode)

    def test_25_own_context_can_use_shared_public_openings_without_proving_labor(self):
        """Re-authenticated correct public witnesses may pass; physical replay is not inferred."""
        m = ready_model(k=16)
        a, b = m.make_positive(b"A"), m.make_positive(b"B")
        self.assertEqual(a.body, b.body)
        self.assertNotEqual(a.nonce, b.nonce)
        self.assertNotEqual(a.identity, b.identity)
        self.assertEqual(m.submit(a), "Accepted")
        self.assertEqual(m.submit(b), "Accepted")
        self.assertEqual(len(m.local_coverage), 16)
        self.assertEqual(len(m.materials), 16)
        self.assertEqual(m.approvals, {b"A", b"B"})
        self.assertFalse(m.offences)
        self.assertFalse(hasattr(m, "physical_replay_verified"))

    def test_26_nonce_salted_trace_is_not_the_original_miner_root(self):
        """Outer nonce never changes the canonical integer trace leaf."""
        m = ready_model()
        e = m.make_positive(b"A")
        w = e.body.witnesses[0]
        salted = replace(w, relation=replace(w.relation, out=w.relation.out + 1))
        body = replace(e.body, witnesses=(salted,) + e.body.witnesses[1:])
        duty = m.positive_duty(b"A")
        bad = envelope(duty.obligation, duty.context, m.nonce(duty), body, duty.key)
        self.rejects("OpeningMismatch", m.submit, bad)
        self.assertFalse(m.accepted)

    def test_27_chunk_tree_empty_single_padded_and_exact_merkle_openings(self):
        """The specified chunk domains, index directions and padding authenticate bytes."""
        for data in (b"", b"x", b"x" * PART_SIZE, b"a" * (PART_SIZE * 2 + 1)):
            tree = ChunkTree(data)
            for i, item in enumerate(tree.chunks):
                verify_chunk(tree.root, len(data), i, item, tree.opening(i))
        self.assertEqual(ChunkTree(b"").root, chunk_leaf(0, b""))
        tree = ChunkTree(b"a" * (PART_SIZE * 2 + 1))
        self.assertEqual(len(tree.levels[0]), 4)
        self.rejects("ChunkDepth", verify_chunk, tree.root, tree.length, 0, tree.chunks[0], ())

    def test_28_partial_bundle_missing_or_mixed_chunks_never_counts_as_complete(self):
        """Partial transport is authenticated staging, not an approval or receipt."""
        m = ready_model()
        a, b = m.make_positive(b"A"), m.make_positive(b"B")
        stage = Assembly.begin(a, 0, 8, MAX_BODY)
        ta, tb = ChunkTree(a.body.encode()), ChunkTree(b.body.encode())
        before = m.snapshot()
        stage.add(0, ta.chunks[0], ta.opening(0), 1)
        self.rejects("IncompleteBody", stage.complete)
        self.rejects("ChunkOpening", stage.add, 0, tb.chunks[0], tb.opening(0), 1)
        self.assertEqual(m.snapshot(), before)

    def test_29_chunk_retries_and_restart_are_idempotent_and_completion_checks_flat_digest(self):
        """Canonical fragments share one body and one native submission."""
        m = ready_model()
        e = m.make_positive(b"A")
        tree = ChunkTree(e.body.encode())
        stage = Assembly.begin(e, 0, 8, MAX_BODY)
        for i, item in enumerate(tree.chunks):
            stage.add(i, item, tree.opening(i), 1)
            stage.add(i, item, tree.opening(i), 1)
        restored = copy.deepcopy(stage)
        self.assertEqual(restored.complete(), e.body.encode())
        self.assertEqual(len(stage.parts), len(tree.chunks))
        broken = copy.deepcopy(stage)
        broken.body_hash = H(b"wrong flat digest")
        self.rejects("BodyDigest", broken.complete)
        m.submit(e)
        self.assertEqual(m.submit(e), "Idempotent")

    def test_30_staging_qualification_capacity_and_deadline_are_finite(self):
        """Staging headers cannot mint votes, restart clocks or allocate unbounded parts."""
        m = ready_model()
        e = m.make_positive(b"A")
        self.rejects("UnauthenticatedHeader", Assembly.begin, e, 0, 8, MAX_BODY, False)
        self.rejects("StageCapacity", Assembly.begin, e, 0, 8, 1)
        stage = Assembly.begin(e, 0, 8, MAX_BODY)
        tree = ChunkTree(e.body.encode())
        self.rejects("StageDeadline", stage.add, 0, tree.chunks[0], tree.opening(0), 8)
        self.rejects("ChunkIndex", stage.add, len(tree.chunks), b"", (), 1)
        self.assertEqual(m.notice_until, 10)
        self.assertFalse(m.approvals)

    def test_31_sample_external_refutation_after_positive_needs_no_new_seed_or_ticket(self):
        """A selected actor may correct a full affirmation with an off-sample negative."""
        m, bad = counterexample_model()
        m.submit(m.make_positive(b"A"))
        m.new_claim_difficulty = (1 << 120) - 1
        e = m.make_action(b"A", "ComputationRefuted", bad, duty_name=b"correction")
        self.assertNotEqual(e.obligation.identity, m.positive_duty(b"A").obligation.identity)
        self.assertEqual(m.submit(e), "Accepted")
        self.assertTrue(m.refuted)
        self.assertIn((b"A", "FalseFullApproval"), m.offences)

    def test_32_qualified_third_party_refutation_and_negative_budget_survive_positive_work(self):
        """Public qualification and the protected negative reserve are separate from positive nonces."""
        m, bad = counterexample_model()
        before = dict(m.negative_remaining)
        for actor in m.actors:
            m.submit(m.make_positive(actor))
        self.assertEqual(m.negative_remaining, before)
        e = m.make_action(b"third", "ComputationRefuted", bad, third_party=True)
        self.assertEqual(m.submit(e), "Accepted")
        self.assertTrue(m.refuted)

    def test_33_positive_retries_do_not_clear_pending_cases_or_infer_computation_guilt(self):
        """An unresolved qualified notice blocks success even with valid positives."""
        m = ready_model()
        e = m.make_positive(b"A")
        m.submit(e)
        notice = m.make_action(b"B", "Notice")
        m.submit(notice)
        m.submit(e)
        m.supply_common()
        m.now = 10
        self.assertFalse(m.finalize())
        self.assertFalse(m.refuted)
        self.assertFalse(m.offences)
        self.assertEqual(m.pending, {notice.obligation.identity})

    def test_34_invalid_counterevidence_does_not_close_q_or_consume_negative_duty(self):
        """A bad response or non-refuting witness only rejects that packet."""
        m = ready_model()
        notice = m.make_action(b"A", "Notice")
        m.submit(notice)
        false_refute = m.make_action(b"A", "ComputationRefuted", 4)
        before = m.snapshot()
        self.rejects("NoComputationRefutation", m.submit, false_refute)
        self.assertEqual(m.snapshot(), before)

    def test_35_refutation_wins_over_all_positive_records_and_same_batch_final(self):
        """A valid local counterexample cannot be overturned by positive count."""
        m, bad = counterexample_model()
        m.supply_common()
        for actor in m.actors:
            m.submit(m.make_positive(actor))
        m.now = 10
        counter = m.make_action(b"A", "ComputationRefuted", bad)
        m.canonical_batch(("Final", counter))
        self.assertTrue(m.refuted)
        self.assertFalse(m.final)
        self.assertFalse(m.source_rights_live)
        self.assertEqual(m.submit(m.make_positive(b"A")), "Idempotent")
        self.assertFalse(m.finalize())

    def test_36_local_holds_does_not_inherit_full_claim_false_affirmation_liability(self):
        """A local true relation is not a claim-wide affirmation."""
        m, bad = counterexample_model()
        m.submit(m.make_positive(b"A", "LocalHolds"))
        m.submit(m.make_positive(b"B"))
        m.submit(m.make_action(b"A", "ComputationRefuted", bad))
        self.assertNotIn((b"A", "FalseFullApproval"), m.offences)
        self.assertIn((b"B", "FalseFullApproval"), m.offences)

    def test_37_objective_liability_and_repeat_refutation_settle_once(self):
        """Native finite liability needs an accepted full subject and exact root refutation."""
        m, bad = counterexample_model()
        m.submit(m.make_positive(b"A"))
        counter = m.make_action(b"B", "ComputationRefuted", bad)
        m.submit(counter)
        self.assertEqual(m.collect_liability(b"A"), 7)
        self.assertEqual(m.collect_liability(b"A"), 0)
        self.assertEqual(m.submit(counter), "Idempotent")
        self.assertEqual(m.collateral[b"A"], 0)
        self.assertEqual(len(m.offences), 1)

    def test_38_liability_backing_cannot_be_released_while_window_or_case_remains(self):
        """Collateral stays through the finite native liability period and unresolved due."""
        m = ready_model()
        m.submit(m.make_positive(b"A"))
        self.rejects("LiabilityRetention", m.release_collateral, b"A")
        notice = m.make_action(b"B", "Notice")
        m.submit(notice)
        m.now = 31
        self.rejects("LiabilityRetention", m.release_collateral, b"A")
        m.pending.clear()  # Trusted objective native case closure fixture.
        self.assertEqual(m.release_collateral(b"A"), 7)

    def test_39_nonce_completion_mints_no_entitlement_and_native_reward_once_is_separate(self):
        """A new submission identifier cannot create or redirect an economic right."""
        m = ready_model()
        e = m.make_positive(b"A")
        m.submit(e)
        self.rejects("NoNativeEntitlement", m.settle, b"A", "performed-service")
        m.native_entitlement(b"A", "performed-service", 3)
        self.assertEqual(m.settle(b"A", "performed-service"), 3)
        self.assertEqual(m.submit(replace(e, signature_bytes=b"relay-resigned")), "Idempotent")
        self.assertEqual(m.settle(b"A", "performed-service"), 0)
        self.rejects("NoNativeEntitlement", m.settle, b"B", "performed-service")

    def test_40_success_payment_requires_final_and_keeps_full_replay_scope(self):
        """Existing success entitlement cannot be redeemed from nonce/local completion alone."""
        m = ready_model()
        m.submit(m.make_positive(b"A"))
        m.native_entitlement(b"A", "success", 11)
        self.rejects("SuccessNotFinal", m.settle, b"A", "success")
        m.supply_common()
        m.now = 10
        self.assertTrue(m.finalize())
        self.assertEqual(m.settle(b"A", "success"), 11)
        self.assertEqual(m.settle(b"A", "success"), 0)

    def test_41_cancel_prune_and_missing_restore_do_not_revive_consumed_obligation(self):
        """Raw evidence removal is separate from canonical once/tombstone state."""
        m = ready_model()
        e = m.make_positive(b"A")
        m.submit(e)
        m.prune_evidence()
        self.assertFalse(m.materials)
        self.rejects("ConsumedTombstone", m.submit, e)
        m.cancel()
        self.rejects("ConsumedTombstone", m.submit, e)
        restored = m.restart()
        restored.restored = False
        self.rejects("RestorationUnavailable", restored.submit, e)
        self.assertEqual(restored.approvals, {b"A"})

    def test_42_restart_preserves_context_once_pending_and_economic_state(self):
        """A fresh local process preserves native state rather than inferring it from cache."""
        m = ready_model()
        e = m.make_positive(b"A")
        m.submit(e)
        m.submit(m.make_action(b"B", "Notice"))
        m.native_entitlement(b"A", "performed-service", 3)
        m.settle(b"A", "performed-service")
        restarted = m.restart()
        self.assertEqual(restarted.snapshot(), m.snapshot())
        self.assertEqual(restarted.submit(e), "Idempotent")
        self.assertEqual(restarted.settle(b"A", "performed-service"), 0)

    def test_43_reorg_undo_is_atomic_and_old_worker_generation_is_stale(self):
        """Canonical rollback undoes once/funds/coverage together without a permanent local spent file."""
        m = CourtModel()
        checkpoint = m.snapshot()
        m.provide_seed()
        e = m.make_positive(b"A")
        generation = m.generation
        m.submit(e, generation)
        m.native_entitlement(b"A", "performed-service", 3)
        m.settle(b"A", "performed-service")
        m.restore(checkpoint)
        self.assertEqual(m.snapshot(), checkpoint)
        self.rejects("StaleWorker", m.submit, e, generation)
        m.provide_seed(H(b"fork-1") + H(b"fork-2"), b"new-native-anchor")
        new = m.make_positive(b"A")
        self.assertNotEqual(e.identity, new.identity)
        self.assertEqual(m.submit(new), "Accepted")

    def test_44_batch_failure_rolls_back_all_canonical_effects_but_not_real_ingress_cost(self):
        """Atomic batch semantics do not leave partial approval/receipt effects."""
        m = ready_model()
        first, second = m.make_positive(b"A"), m.make_positive(b"B")
        second = replace(second, auth=replace(second.auth, valid=False))
        before = m.snapshot()
        ingress_before = m.ingress.remaining
        self.rejects("NativeAuthentication", m.canonical_batch, (first, second))
        self.assertEqual(m.snapshot(), before)
        self.assertLess(m.ingress.remaining, ingress_before)

    def test_45_old_claim_profile_remains_pinned_and_new_route_cannot_downgrade(self):
        """Activation/config changes do not reinterpret an accepted claim's scope."""
        m = ready_model(profile=b"CPN1-original")
        e = m.make_positive(b"A")
        original_hash = e.context_hash
        m.active_new_profile = b"CPN2-future"
        self.assertEqual(m.make_positive(b"A").context_hash, original_hash)
        self.rejects("RouteVersion", m.submit, replace(e, protocol="legacy"))
        self.assertEqual(m.submit(e), "Accepted")
        # Legacy-claim verification itself remains an external native adapter;
        # this new-route model never certifies legacy signatures as CPN1.

    def test_46_missing_funding_or_finite_liability_prevents_countable_full_approval(self):
        """Hash agreement alone does not establish native funds or responsibility."""
        for attribute, value, code in (
            ("funds_qualified", False, "NativeFunds"),
            ("finite_liability", False, "FullApprovalQualification"),
            ("liability_until", 19, "FullApprovalQualification"),
        ):
            m = ready_model()
            duty = m.positive_duty(b"A")
            m.duties[duty.obligation.identity] = replace(duty, **{attribute: value})
            self.rejects(code, m.submit, m.make_positive(b"A"))
            self.assertFalse(m.approvals or m.accepted)

    def test_47_global_capacity_and_ingress_do_not_expand_with_model_aliases(self):
        """Opaque model names grant neither new duty nor private capacity."""
        self.rejects("OriginalCapacity", CourtModel, actors=(b"A", b"B"), global_capacity=1)
        m = ready_model()
        duty = m.positive_duty(b"A")
        body = m.make_positive(b"A").body
        before = m.snapshot()
        m.ingress = IngressBudget(3)
        for i in range(3):
            fake_context = duty.context.changed("model", ("model-alias-" + str(i)).encode())
            fake = envelope(duty.obligation, fake_context, m.nonce(duty), body, duty.key)
            self.rejects("ContextMismatch", m.submit, fake)
        self.assertEqual(m.snapshot(), before)
        self.rejects("IngressExhausted", m.submit, m.make_positive(b"A"))
        self.rejects("IngressSize", IngressBudget(3).check, MAX_BODY + 1)

    def test_48_local_sampling_can_miss_corruption_so_no_independent_labor_or_unconditional_safety_claim(self):
        """The model explicitly exhibits the nonce-only guarantee limit."""
        m, bad = counterexample_model()
        for actor in m.actors:
            m.submit(m.make_positive(actor))  # Dishonest full affirmation is not detected by its own samples.
        m.supply_common()
        m.now = 10
        self.assertTrue(m.finalize())  # No active honest full checker in this counterexample.
        self.assertFalse(m.trace.rows[bad].holds())
        self.assertFalse(m.offences)
        # Introducing one *actual valid* counterexample changes the objective result.
        m.submit(m.make_action(b"A", "ComputationRefuted", bad))
        self.assertTrue(m.refuted)
        self.assertFalse(m.final)

    def test_49_response_phase_step_uses_the_fixed_case_budget_and_no_new_seed(self):
        """A finite native response has its own action nonce without clearing Q or minting another trial."""
        m = CourtModel()
        notice = m.make_action(b"A", "Notice")
        m.submit(notice)
        response = m.make_action(b"B", "Response", 3, duty_name=b"native-case-response", step=1)
        self.assertIsNone(m.seed)
        self.assertEqual(m.submit(response), "Accepted")
        before = m.snapshot()
        self.assertEqual(m.submit(response), "Idempotent")
        self.assertEqual(m.snapshot(), before)
        forged = replace(response, obligation=replace(response.obligation, step=2))
        self.rejects("UnknownNativeDuty", m.submit, forged)
        self.rejects("NativeStepBound", m.make_action, b"B", "Response", 3, step=4)
        self.assertEqual(m.pending, {notice.obligation.identity})
        self.assertFalse(m.approvals or m.offences)
        self.assertEqual(m.negative_remaining[b"B"], 7)


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
                             "contract": test.shortDescription(),
                             "detail": self._exc_info_to_string(error, test)})

    def addError(self, test, error):
        super().addError(test, error)
        self.records.append({"name": test.id().split(".")[-1], "result": "ERROR",
                             "contract": test.shortDescription(),
                             "detail": self._exc_info_to_string(error, test)})


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(__file__).with_name("model-results.json"))
    parser.add_argument("--stdout-only", action="store_true")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[4]
    rfc = root / "docs/rfc/0057-court-nonce-and-panel-challenges.md"
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(ContractTests)
    result = unittest.TextTestRunner(verbosity=2, resultclass=RecordingResult).run(suite)
    report = {
        "schema": "misaka-cpn1-abstract-model-results-v1",
        "generated_at_utc": datetime.now(timezone.utc).isoformat(),
        "scope": "ABSTRACT CONTRACT MODEL ONLY; NOT native, E2E, hardware, beacon-security, independent-labor, or activation evidence",
        "source_commit": "2d8212b634880b0967fbf24b00b41936c2322c14",
        "rfc_path": str(rfc.relative_to(root)),
        "rfc_sha256": hashlib.sha256(rfc.read_bytes()).hexdigest(),
        "script_path": str(Path(__file__).resolve().relative_to(root)),
        "script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "tests_run": result.testsRun,
        "passed": sum(record["result"] == "PASS" for record in result.records),
        "failures": len(result.failures), "errors": len(result.errors), "skipped": len(result.skipped),
        "success": result.wasSuccessful(), "tests": result.records,
        "assumptions": [
            "All limits, clocks, funds, amounts, fees and capacities are synthetic fixture values, not protocol defaults or measurements.",
            "Native identity/inner codec/absence/authentication/selection/actor eligibility/actual capital provenance are trusted adapters.",
            "The 64-byte native-seed fixture preserves width and bytes; actual beacon unpredictability, bias resistance and anchoring are not established.",
            "The toy integer relation and trace Merkle codec are not native LLM kernels; only outer CPN1 framing, SHA-256, selector and chunk commitment are direct RFC constructions.",
            "The model keeps qualified native duties, cost/funding and finite liability fixtures; full RFC-0055 actor/service/deadline planner and actual coin/journal primitives are external.",
            "Shared-material bytes and local target coverage are separate from full-replay attestations and native payment/once records.",
            "The model-alias test checks pinned original identity and shared ingress capacity, not real registry throughput or storage performance.",
        ],
        "not_verified": [
            "Native route coverage, production codecs/signature domains, RPC/UI/Studio, DB/snapshot/IBD and native atomic persistence.",
            "Actual full LLM replay, truthful independent Panel work, physical labor, non-collusion or an active-honest-party guarantee.",
            "Native beacon/assignment security, real canonical clock/service/censorship and carrier inclusion.",
            "Minimum-node CPU/RAM/I/O, model registry scale, actual hostile network throughput, calibrated cost reservations or sustained 1 BPS.",
            "Native economic sanctions, amounts, distribution and activation: liability and entitlement events here are explicit synthetic adapters.",
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


