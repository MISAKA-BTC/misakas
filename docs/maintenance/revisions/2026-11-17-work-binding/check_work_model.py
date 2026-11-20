#!/usr/bin/env python3
"""SWC1 integration contract model; standard library only, NOT a native implementation.

Run from any directory. The sole output file is adjacent model-results.json.
SHA-256, LP/NR encodings and Merkle openings are real computations. The bounded
payload codec, u64 toy dot-product VM, authenticated control cuts, HMAC actor
authentication, beacon knowledge/release, material availability and canonical
batch order are explicitly MODEL FIXTURES. In particular this program does not
implement BLS/DKG, native signatures/storage/LLM/DA/fork-choice or prove a cost
lower bound. An honest and an optimized zero-model execution deliberately pass
with identical commitments but different counted scalar work.
"""

from __future__ import annotations

import copy
import hashlib
import hmac
import itertools
import json
import struct
import sys
import traceback
from dataclasses import dataclass, replace
from pathlib import Path
from typing import Any, Callable


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
SOURCE_PATHS = (
    "docs/rfc/0056-global-claim-admission-difficulty.md",
    "docs/rfc/0057-court-nonce-and-panel-challenges.md",
    "docs/rfc/0058-staged-work-commitments-and-consensus-eligibility.md",
)
NETWORK = b"SWC1-contract-fixture-network"
POLICY = hashlib.sha256(b"fixture-policy-v1").digest()
MASK64 = (1 << 64) - 1
MAX256 = (1 << 256) - 1
D_MAX = (1 << 120) - 1
ZERO = bytes(32)


class Refused(Exception):
    def __init__(self, reason: str):
        self.reason = reason
        super().__init__(reason)


def require(condition: bool, reason: str) -> None:
    if not condition:
        raise Refused(reason)


def u(value: int, width: int) -> bytes:
    require(type(value) is int and 0 <= value < 1 << (8 * width), "integer-range")
    return value.to_bytes(width, "little")


def lp(value: bytes) -> bytes:
    require(type(value) is bytes and len(value) <= 16384, "lp-size")
    return u(len(value), 4) + value


def nr(tag: str, value: bytes, version: int = 1) -> bytes:
    return lp(tag.encode()) + u(version, 2) + lp(value)


def h(tag: str, *parts: bytes) -> bytes:
    return hashlib.sha256(lp(tag.encode()) + b"".join(parts)).digest()


def encode_d(value: int) -> bytes:
    require(type(value) is int and 1 <= value <= D_MAX, "difficulty-range")
    return u(value, 16)


def decode_d(encoded: bytes) -> int:
    require(len(encoded) == 16, "difficulty-width")
    value = int.from_bytes(encoded, "little")
    encode_d(value)
    return value


def threshold(value: int) -> int:
    encode_d(value)
    return MAX256 // value


def ticket_admits(ticket: int, difficulty: int) -> bool:
    require(type(ticket) is int and 0 <= ticket <= MAX256, "ticket-range")
    return ticket <= threshold(difficulty)


STAGE_CODES = {"JOB": 1, "TICKET": 2, "PANEL": 3}


def frozen_set_root(stage: str, items: tuple[bytes, ...]) -> bytes:
    require(stage in STAGE_CODES, "unknown-stage")
    require(len(items) <= 64, "frozen-set-capacity")
    code = u(STAGE_CODES[stage], 2)
    if not items:
        return h("MISAKA/PALW/SWC1/SET-EMPTY", code)
    width = 1 << (len(items) - 1).bit_length()
    nodes = [h("MISAKA/PALW/SWC1/SET-LEAF", code, u(i, 8), lp(item)) for i, item in enumerate(items)]
    nodes += [h("MISAKA/PALW/SWC1/SET-PAD", code, u(i, 8)) for i in range(len(items), width)]
    while len(nodes) > 1:
        nodes = [h("MISAKA/PALW/SWC1/SET-NODE", nodes[i], nodes[i + 1]) for i in range(0, len(nodes), 2)]
    return nodes[0]


def payload(value: Any, depth: int = 0) -> bytes:
    """A bounded, typed toy payload codec inside NR; not the native wire codec."""
    require(depth <= 8, "codec-depth")
    if type(value) is bytes:
        return b"b" + lp(value)
    if type(value) is str:
        return b"s" + lp(value.encode())
    if type(value) is int:
        return b"u" + u(value, 8)
    if type(value) is tuple:
        require(len(value) <= 64, "codec-count")
        return b"t" + u(len(value), 4) + b"".join(lp(payload(x, depth + 1)) for x in value)
    raise Refused("codec-type")


def jsonable(value: Any) -> Any:
    if isinstance(value, bytes):
        return value.hex()
    if isinstance(value, (list, tuple)):
        return [jsonable(x) for x in value]
    if isinstance(value, dict):
        return {str(k): jsonable(v) for k, v in value.items()}
    return value


def fingerprint(value: Any) -> bytes:
    # Checkpoint/fixture comparison only. Consensus identities use LP/NR above.
    return hashlib.sha256(json.dumps(jsonable(value), sort_keys=True, separators=(",", ":")).encode()).digest()


def expect_refusal(reason: str, action: Callable[[], Any]) -> None:
    try:
        action()
    except Refused as error:
        assert error.reason == reason, (reason, error.reason)
    else:
        raise AssertionError(f"expected refusal: {reason}")


@dataclass(frozen=True)
class Seal:
    actor: str
    salt: bytes
    mac: bytes


class AuthenticationFixture:
    """HMAC is used to detect tampering in the MODEL, not as a native signature."""

    @staticmethod
    def seal(actor: str, subject: bytes, salt: bytes = b"first") -> Seal:
        key = h("FIXTURE/ACTOR/SECRET", lp(actor.encode()))
        return Seal(actor, salt, hmac.new(key, subject + lp(salt), hashlib.sha256).digest())

    @classmethod
    def verify(cls, actor: str, subject: bytes, seal: Seal) -> bool:
        return seal.actor == actor and hmac.compare_digest(cls.seal(actor, subject, seal.salt).mac, seal.mac)


AUTH = AuthenticationFixture()


@dataclass(frozen=True)
class Opening:
    index: int
    count: int
    blob: bytes
    siblings: tuple[bytes, ...]


class Merkle:
    def __init__(self, context: bytes, blobs: tuple[bytes, ...]):
        require(0 < len(blobs) <= 32, "tree-size")
        self.context, self.blobs = context, blobs
        width = 1 << (len(blobs) - 1).bit_length()
        leaves = [h("FIXTURE/TREE/LEAF", context, u(i, 4), lp(x)) for i, x in enumerate(blobs)]
        leaves += [h("FIXTURE/TREE/PAD", context, u(i, 4), u(len(blobs), 4)) for i in range(len(blobs), width)]
        self.levels = [leaves]
        while len(self.levels[-1]) > 1:
            row = self.levels[-1]
            self.levels.append([h("FIXTURE/TREE/NODE", row[i], row[i + 1]) for i in range(0, len(row), 2)])
        self.root = h("FIXTURE/TREE/ROOT", context, u(len(blobs), 4), self.levels[-1][0])

    def open(self, index: int) -> Opening:
        require(0 <= index < len(self.blobs), "opening-index")
        cursor, siblings = index, []
        for row in self.levels[:-1]:
            siblings.append(row[cursor ^ 1])
            cursor //= 2
        return Opening(index, len(self.blobs), self.blobs[index], tuple(siblings))

    @staticmethod
    def verify(root: bytes, context: bytes, opening: Opening, count: int) -> bool:
        if not (0 <= opening.index < count <= 32 and opening.count == count):
            return False
        if len(opening.siblings) != (count - 1).bit_length():
            return False
        digest = h("FIXTURE/TREE/LEAF", context, u(opening.index, 4), lp(opening.blob))
        cursor = opening.index
        for sibling in opening.siblings:
            if len(sibling) != 32:
                return False
            digest = h("FIXTURE/TREE/NODE", sibling, digest) if cursor & 1 else h("FIXTURE/TREE/NODE", digest, sibling)
            cursor //= 2
        return h("FIXTURE/TREE/ROOT", context, u(count, 4), digest) == root


ARITHMETIC = b"toy-u64-mod-2^64;four-input-words;row-order;state=state+dot(row,input)"
GENERATION = b"fixed-row-count;no-early-stop;trace-includes-init-and-every-row-output"


@dataclass(frozen=True)
class ModelRef:
    content_root: bytes
    rows: int

    def program(self) -> bytes:
        return payload(("toy-dot-accumulate", self.rows, 4))


class ToyModel:
    def __init__(self, zero: bool = False):
        self.weights = tuple(tuple(0 if zero else (i + j + 1) for j in range(4)) for i in range(8))
        self.tree = Merkle(b"toy-model-rows", tuple(b"".join(u(x, 8) for x in row) for row in self.weights))
        self.ref = ModelRef(self.tree.root, len(self.weights))


@dataclass(frozen=True)
class Job:
    model: ModelRef
    inputs: tuple[int, ...]
    job_seed: bytes
    job_slot: int
    expiry: int

    def work_key(self) -> bytes:
        # No display alias, claimant, funding scope, signature, root, target epoch or carrier.
        return h("MISAKA/PALW/SWC1/WORK", lp(NETWORK),
                 nr("canonical-model-content", self.model.content_root),
                 nr("canonical-program", self.model.program()), nr("arithmetic-rules", ARITHMETIC),
                 nr("actual-input", payload((self.inputs, self.job_seed))),
                 nr("initial-state-and-dependencies", payload((0, "none"))),
                 nr("generation-and-stop-rules", GENERATION),
                 nr("execution-range", payload((0, self.model.rows))))


@dataclass(frozen=True)
class Registration:
    owner: str
    beneficiary: str
    capital: str
    sequence: int
    job_slot: int
    model: ModelRef
    request: tuple[int, ...] = (11, 17, 23, 29)
    target: int = 12
    difficulty: int = 1
    expiry: int = 100
    display_alias: str = "model-A"

    def trial(self) -> bytes:
        return h("FIXTURE/TRIAL", lp(self.capital.encode()), u(self.sequence, 8))

    def identity(self) -> bytes:
        return h("MISAKA/PALW/SWC1/REGISTRATION", lp(NETWORK), POLICY, self.trial(),
                 nr("owner", self.owner.encode()), nr("beneficiary", self.beneficiary.encode()),
                 nr("funding-consent", payload((self.capital, self.sequence))),
                 nr("requested-job", payload(self.request)), nr("model-content", self.model.content_root),
                 nr("program", self.model.program()), nr("execution-rules", ARITHMETIC + GENERATION),
                 nr("stage-schedule", payload((self.job_slot, self.target, encode_d(self.difficulty), self.expiry))),
                 nr("resource-plan", payload((self.model.rows, 3, 2))))

    def opportunity(self) -> bytes:
        return h("MISAKA/PALW/SWC1/OPPORTUNITY", lp(NETWORK), self.trial(), self.identity())


@dataclass(frozen=True)
class Stage:
    name: str
    slot: int
    parent: bytes
    members: tuple[bytes, ...]
    extras: bytes
    close_tick: int
    reveal_tick: int
    key_epoch: int = 1
    items: tuple[bytes, ...] | None = None

    def identity(self) -> bytes:
        require(tuple(sorted(set(self.members))) == self.members, "stage-member-order")
        items = self.members if self.items is None else self.items
        require(len(items) == len(self.members), "stage-item-count")
        members_root = frozen_set_root(self.name, items)
        descriptor = (u(1, 2) + lp(NETWORK) + POLICY + u(STAGE_CODES[self.name], 2)
                      + u(self.key_epoch, 8) + u(self.slot, 8) + nr("parent-stage", self.parent)
                      + nr("stable-control-anchor", h("FIXTURE/STABLE-CONTROL-ANCHOR", u(self.slot, 8)))
                      + members_root + u(len(self.members), 8)
                      + nr("stage-schedule", payload(("fixture-clock", self.close_tick, self.reveal_tick,
                                                      self.reveal_tick + 1, self.extras)))
                      + nr("randomness-scheme", payload(("FIXTURE-KNOWLEDGE-ORACLE", self.key_epoch,
                                                        h("FIXTURE/BEACON-PUBLIC-KEY", u(self.key_epoch, 8))))))
        return h("MISAKA/PALW/SWC1/FREEZE", descriptor)


class BeaconFixture:
    def __init__(self):
        self.closed: dict[tuple[str, int, int], Stage] = {}
        self.released: dict[bytes, bytes] = {}
        self.unavailable: set[bytes] = set()

    def freeze(self, stage: Stage, tick: int) -> None:
        require(tick == stage.close_tick < stage.reveal_tick, "stage-order")
        key = (stage.name, stage.key_epoch, stage.slot)
        require(key not in self.closed, "stage-already-closed")
        stage.identity()
        self.closed[key] = stage

    def read(self, stage: Stage, tick: int) -> bytes:
        key = (stage.name, stage.key_epoch, stage.slot)
        require(self.closed.get(key) == stage, "stage-not-frozen")
        require(stage.identity() not in self.unavailable, "beacon-unavailable-no-fallback")
        require(tick >= stage.reveal_tick, "seed-not-yet-known-in-fixture")
        # Public deterministic test fixture: this is NOT a real unpredictability construction.
        beacon_message = h("MISAKA/PALW/SWC1/BEACON-MESSAGE", stage.identity())
        seed = h("FIXTURE/BEACON-RELEASE", b"public-test-vector-20261117", beacon_message)
        existing = self.released.setdefault(stage.identity(), seed)
        assert existing == seed
        return seed

    def fail(self, stage: Stage) -> None:
        require(stage.identity() not in self.released, "already-released")
        self.unavailable.add(stage.identity())


def derive_job(registration: Registration, seed: bytes, job_freeze: bytes) -> Job:
    job_seed = h("MISAKA/PALW/SWC1/JOB-SEED", lp(NETWORK), registration.identity(), job_freeze,
                 nr("job-beacon-anchor", job_freeze), nr("job-seed", seed))
    # Every byte of all four u64 seed words enters the actual VM input; no low-bit projection.
    words = struct.unpack("<4Q", job_seed)
    inputs = tuple(x ^ y for x, y in zip(words, registration.request))
    return Job(registration.model, inputs, job_seed, registration.job_slot, registration.expiry)


@dataclass(frozen=True)
class Original:
    registration: bytes
    opportunity: bytes
    work: bytes
    artifact: bytes
    trace: bytes
    output: bytes
    execution: bytes
    count: int
    beneficiary: str

    def identity(self) -> bytes:
        return h("MISAKA/PALW/SWC1/ORIGINAL", lp(NETWORK), POLICY, self.opportunity, self.work,
                 nr("native-original", payload((self.registration, "toy-normalized-original"))),
                 nr("artifact-root", self.artifact), nr("trace-root", self.trace),
                 nr("output-root", self.output), nr("execution-root", self.execution),
                 nr("trace-manifest-and-counts", payload((self.trace, self.count))),
                 nr("work-accounting", payload((self.count - 1, 4))),
                 nr("beneficiary-and-liability", payload((self.beneficiary, 15, 19, 20))),
                 nr("material-supply-plan", payload(("fixture-available", 100))))


@dataclass(frozen=True)
class LocalEvidence:
    step: int
    before: Opening
    after: Opening
    model_row: Opening


class Material:
    """Only a miner/Panel owns this full toy trace/model, not the Ledger."""
    def __init__(self, registration: Registration, job: Job, model: ToyModel,
                 corrupt_final: bool = False, shortcut: bool = False):
        assert model.ref == job.model
        if shortcut:
            require(all(all(x == 0 for x in row) for row in model.weights), "shortcut-requires-zero-model")
            states, muls, adds = [0] * (model.ref.rows + 1), 0, 0
        else:
            states, muls, adds = [0], 0, 0
            for row in model.weights:
                dot = 0
                for weight, value in zip(row, job.inputs):
                    dot = (dot + weight * value) & MASK64
                    muls += 1
                    adds += 1
                states.append((states[-1] + dot) & MASK64)
                adds += 1
        if corrupt_final:
            states[-1] ^= 1
        self.states, self.model, self.job = tuple(states), model, job
        self.tree = Merkle(b"toy-state/" + job.work_key(), tuple(u(x, 8) for x in self.states))
        output = h("FIXTURE/OUTPUT", u(self.states[-1], 8))
        execution = h("FIXTURE/EXECUTION", job.work_key(), self.tree.root, output,
                      model.ref.content_root, u(len(self.states), 4), u(0, 8))
        self.original = Original(registration.identity(), registration.opportunity(), job.work_key(),
                                 model.ref.content_root, self.tree.root, output, execution,
                                 len(self.states), registration.beneficiary)
        self.metrics = {"scalar_multiplications": muls, "scalar_additions": adds,
                        "trace_leaf_hashes": len(self.states), "trace_internal_hashes": len(self.tree.levels[0]) - 1}

    def evidence(self, step: int) -> LocalEvidence:
        return LocalEvidence(step, self.tree.open(step), self.tree.open(step + 1), self.model.tree.open(step))


def check_local(job: Job, original: Original, evidence: LocalEvidence) -> bool:
    """Returns arithmetic consistency; malformed unauthenticated evidence is refused."""
    i = evidence.step
    require(0 <= i < job.model.rows, "step-range")
    require(original.work == job.work_key() and original.artifact == job.model.content_root, "local-scope")
    require(evidence.before.index == i and evidence.after.index == i + 1 and evidence.model_row.index == i, "local-index")
    require(Merkle.verify(original.trace, b"toy-state/" + original.work, evidence.before, job.model.rows + 1), "opening-invalid")
    require(Merkle.verify(original.trace, b"toy-state/" + original.work, evidence.after, job.model.rows + 1), "opening-invalid")
    require(Merkle.verify(original.artifact, b"toy-model-rows", evidence.model_row, job.model.rows), "opening-invalid")
    require(len(evidence.before.blob) == len(evidence.after.blob) == 8 and len(evidence.model_row.blob) == 32, "local-codec")
    before, after = int.from_bytes(evidence.before.blob, "little"), int.from_bytes(evidence.after.blob, "little")
    row = struct.unpack("<4Q", evidence.model_row.blob)
    expected = (before + sum(w * x for w, x in zip(row, job.inputs))) & MASK64
    composite = h("FIXTURE/EXECUTION", original.work, original.trace, original.output,
                  original.artifact, u(job.model.rows + 1, 4), u(0, 8))
    init_ok = i != 0 or before == 0
    end_ok = i != job.model.rows - 1 or h("FIXTURE/OUTPUT", u(after, 8)) == original.output
    return expected == after and init_ok and end_ok and composite == original.execution


@dataclass(frozen=True)
class Positive:
    actor: str
    original: bytes
    obligation: bytes
    context: bytes
    nonce: bytes
    target: int
    evidence: LocalEvidence
    full_scope_declared: bool
    seal: Seal

    def subject(self) -> bytes:
        e = self.evidence
        body = payload((e.step, e.before.blob, e.after.blob, e.model_row.blob,
                        e.before.siblings, e.after.siblings, e.model_row.siblings))
        return h("FIXTURE/POSITIVE-SUBJECT", self.original, self.obligation, self.context, self.nonce,
                 u(self.target, 4), bytes([self.full_scope_declared]), h("FIXTURE/BODY", body))


@dataclass(frozen=True)
class Negative:
    actor: str
    original: bytes
    obligation: bytes
    context: bytes
    nonce: bytes
    evidence: LocalEvidence
    seal: Seal

    def subject(self) -> bytes:
        e = self.evidence
        body = payload((e.step, e.before.blob, e.after.blob, e.model_row.blob,
                        e.before.siblings, e.after.siblings, e.model_row.siblings))
        return h("FIXTURE/NEGATIVE-SUBJECT", self.original, self.obligation, self.context,
                 self.nonce, h("FIXTURE/BODY", body))


class Ledger:
    def __init__(self, capacity: int = 4):
        self.capacity = capacity
        self.capital = {f"coin-{i}": "alice" for i in range(8)}
        self.registrations: dict[bytes, Registration] = {}
        self.trials: dict[bytes, bytes] = {}
        # A finite capital-scoped high-water survives job-history GC. A larger job
        # slot cannot revive an already consumed capital/sequence pair.
        self.trial_highwater: dict[str, int] = {}
        self.jobs: dict[bytes, Job] = {}
        self.work_owner: dict[bytes, bytes] = {}
        self.records: dict[bytes, dict[str, Any]] = {}
        self.work_use: dict[bytes, bytes] = {}
        self.rights: dict[bytes, dict[str, Any]] = {}
        self.once: set[bytes] = set()
        self.open_cases: set[bytes] = set()
        self.revocations: dict[bytes, int] = {}
        self.accepted: set[bytes] = set()
        self.negative_room: dict[bytes, int] = {}
        self.oracle = BeaconFixture()
        self.stages: dict[str, Stage] = {}
        self.ticket_seed: bytes | None = None
        self.panel_seed: bytes | None = None
        self.floor = -1
        self.archive_root = ZERO
        self.archived_records = 0
        self.batch = 0
        self.settled = False

    def view(self) -> dict[str, Any]:
        return {
            "capacity": self.capacity, "capital": sorted(self.capital.items()),
            "trials": sorted((k.hex(), v.hex()) for k, v in self.trials.items()),
            "trial_highwater": sorted(self.trial_highwater.items()),
            "registered": sorted((k.hex(), r.identity().hex()) for k, r in self.registrations.items()),
            "jobs": sorted((k.hex(), j.work_key().hex(), j.job_slot, j.expiry) for k, j in self.jobs.items()),
            "owners": sorted((k.hex(), v.hex()) for k, v in self.work_owner.items()),
            "work_use": sorted((k.hex(), v.hex()) for k, v in self.work_use.items()),
            "records": sorted((key.hex(), r["original"].identity().hex(), r["reg"].hex(), r["state"], r.get("ticket"),
                               r.get("admission_cut"), tuple(r.get("actors", ())),
                               tuple(sorted(r.get("positives", ()))) ) for key, r in self.records.items()),
            "rights": sorted((k.hex(), v["state"], v["source"].hex(), v["purpose"], v["quantity"],
                              v.get("consumed_batch")) for k, v in self.rights.items()),
            "once": sorted(x.hex() for x in self.once), "cases": sorted(x.hex() for x in self.open_cases),
            "revocations": sorted((k.hex(), v) for k, v in self.revocations.items()),
            "accepted": sorted(x.hex() for x in self.accepted),
            "negative_room": sorted((k.hex(), v) for k, v in self.negative_room.items()),
            "stages": sorted((k, v.identity().hex()) for k, v in self.stages.items()),
            "oracle_closed": sorted((k, v.identity().hex()) for k, v in self.oracle.closed.items()),
            "released": sorted((k.hex(), v.hex()) for k, v in self.oracle.released.items()),
            "unavailable": sorted(x.hex() for x in self.oracle.unavailable),
            "ticket_seed": self.ticket_seed, "panel_seed": self.panel_seed, "pool": getattr(self, "pool", ()),
            "floor": self.floor, "archive_root": self.archive_root.hex(), "archived": self.archived_records,
            "batch": self.batch, "settled": self.settled,
        }

    def register(self, reg: Registration, seal: Seal) -> None:
        require(reg.job_slot > self.floor, "expired-job-namespace")
        require("JOB" not in self.stages, "registration-set-closed")
        require(AUTH.verify(reg.owner, reg.identity(), seal), "registration-auth")
        require(self.capital.get(reg.capital) == reg.owner, "registration-unfunded")
        require(reg.sequence > self.trial_highwater.get(reg.capital, -1), "trial-sequence-reused")
        require(reg.trial() not in self.trials, "trial-already-used")
        require(len(self.registrations) < self.capacity, "registration-capacity")
        # Only after all prerequisites are authenticated are canonical indices mutated.
        self.registrations[reg.identity()] = reg
        self.trials[reg.trial()] = reg.identity()
        self.trial_highwater[reg.capital] = reg.sequence

    def freeze_job(self) -> Stage:
        stage = Stage("JOB", 1, ZERO, tuple(sorted(self.registrations)), b"job-rules-fixed", 1, 2)
        self.oracle.freeze(stage, 1)
        self.stages["JOB"] = stage
        return stage

    def bind_jobs(self) -> None:
        require(not self.jobs, "jobs-already-bound")
        seed = self.oracle.read(self.stages["JOB"], 2)
        # Fixed registration order, never first arrival of Original, breaks a hypothetical duplicate mapping tie.
        for key in self.stages["JOB"].members:
            job = derive_job(self.registrations[key], seed, self.stages["JOB"].identity())
            self.jobs[key] = job
            self.work_owner.setdefault(job.work_key(), key)

    def commit(self, reg: Registration, original: Original, seal: Seal) -> None:
        require(reg.job_slot > self.floor, "expired-job-namespace")
        require("TICKET" not in self.stages, "original-set-closed")
        require(reg.identity() in self.jobs, "registration-not-bound")
        require(AUTH.verify(reg.owner, original.identity(), seal), "original-auth")
        job = self.jobs[reg.identity()]
        require(original.registration == reg.identity() and original.opportunity == reg.opportunity(), "original-registration")
        require(original.work == job.work_key() and self.work_owner.get(original.work) == reg.identity(), "work-owner-mismatch")
        require(original.beneficiary == reg.beneficiary and original.artifact == job.model.content_root, "original-bound-fields")
        require(original.count == job.model.rows + 1, "original-shape")
        require(original.work not in self.work_use, "work-already-fixed")
        oid = original.identity()
        self.work_use[original.work] = oid
        self.records[oid] = {"original": original, "reg": reg.identity(), "state": "Committed", "positives": set()}

    def commit_batch(self, entries: tuple[tuple[Registration, Original, Seal], ...]) -> bytes:
        """Toy atomic batch adapter: each leaf keeps its own work/slot/Original authority."""
        require(0 < len(entries) <= self.capacity, "batch-capacity")
        batch_root = h("FIXTURE/BATCH-WRAPPER", payload(tuple(original.identity() for _, original, _ in entries)))
        trial = copy.deepcopy(self)
        for reg, original, seal in entries:
            trial.commit(reg, original, seal)
        self.__dict__ = trial.__dict__
        return batch_root

    def freeze_ticket(self) -> Stage:
        targets = tuple((rid, self.registrations[rid].target, encode_d(self.registrations[rid].difficulty)) for rid in sorted(self.registrations))
        members = tuple(sorted(self.records))
        items = tuple(self.records[oid]["original"].opportunity + self.records[oid]["original"].work + oid for oid in members)
        stage = Stage("TICKET", 12, self.stages["JOB"].identity(), members, payload(targets), 3, 4, items=items)
        self.oracle.freeze(stage, 3)
        self.stages["TICKET"] = stage
        return stage

    def draw_tickets(self) -> None:
        require(self.ticket_seed is None, "ticket-already-drawn")
        stage = self.stages["TICKET"]
        self.ticket_seed = self.oracle.read(stage, 4)
        for oid in stage.members:
            record = self.records[oid]
            original, reg = record["original"], self.registrations[record["reg"]]
            digest = h("MISAKA/PALW/SWC1/WORK-TICKET", lp(NETWORK), POLICY, u(reg.target, 8),
                       original.opportunity, original.work, oid, stage.identity(),
                       nr("ticket-beacon-anchor", stage.identity()), nr("ticket-seed", self.ticket_seed))
            ticket = int.from_bytes(digest, "big")
            eligible = ticket_admits(ticket, reg.difficulty)
            record.update(ticket=ticket, state="TicketEligible" if eligible else "Lost")

    def admit(self, maximum: int | None = None) -> tuple[bytes, ...]:
        require(self.ticket_seed is not None, "ticket-not-drawn")
        require("PANEL" not in self.stages, "cohort-closed")
        limit = self.capacity if maximum is None else min(maximum, self.capacity)
        ordered = sorted((r["ticket"], oid) for oid, r in self.records.items() if r["state"] == "TicketEligible")
        for _, oid in ordered:
            if len(self.accepted) == limit:
                self.records[oid]["state"] = "CapacityNotAdmitted"
                continue
            self.accepted.add(oid)
            self.negative_room[oid] = 2  # part of the pre-reserved toy plan, unavailable to new claims
            self.records[oid]["state"] = "VerificationPending"
            self.records[oid]["admission_cut"] = 4
        return tuple(sorted(self.accepted))

    def freeze_panel(self, pool: tuple[str, ...] = ("panel-0", "panel-1", "panel-2")) -> Stage:
        require(self.ticket_seed is not None, "ticket-not-drawn")
        require(tuple(sorted(set(pool))) == pool, "pool-order")
        members = tuple(sorted(self.accepted))
        items = tuple(oid + nr("admission-cut", u(self.records[oid]["admission_cut"], 8))
                      + nr("pool-snapshot", payload(pool)) + nr("draw-policy", b"fixture-two-distinct-actors") for oid in members)
        stage = Stage("PANEL", 13, self.stages["TICKET"].identity(), members, payload(pool), 5, 6, items=items)
        self.oracle.freeze(stage, 5)
        self.stages["PANEL"] = stage
        self.pool = pool
        return stage

    def draw_panel(self) -> None:
        stage = self.stages["PANEL"]
        self.panel_seed = self.oracle.read(stage, 6)
        for oid in stage.members:
            actors = sorted(self.pool, key=lambda a: h("FIXTURE/PANEL-DRAW", self.panel_seed, oid, lp(a.encode())))[:2]
            self.records[oid]["actors"] = tuple(actors)

    def cpn_native_scope(self, oid: bytes, actor: str, positive: bool, step: int) -> tuple[bytes, bytes]:
        """Exact CPN1 outer field order; role/assignment/fees are bounded native fixtures."""
        record = self.records[oid]
        original = record["original"]
        job = self.jobs[record["reg"]]
        action_kind = 1 if positive else 2  # toy action registry, NOT allocated native wire IDs
        duty = b"full-replay-positive" if positive else b"third-party-local-refutation"
        draw = self.stages["PANEL"].identity() if positive else b"\x00"
        assignment = h("FIXTURE/ASSIGNMENT", draw, lp(actor.encode())) if positive else b"\x00"
        obligation = h("MISAKA/PALW/CPN1/OBLIGATION", lp(NETWORK), nr("claim", oid),
                       nr("origin", original.registration), nr("actor", actor.encode()), nr("duty", duty),
                       u(0, 8), u(action_kind, 2), u(step, 8))
        context = h("MISAKA/PALW/CPN1/CONTEXT", u(1, 2), lp(NETWORK),
                    nr("claim", oid), nr("origin", original.registration), nr("miner-commit", oid),
                    nr("execution-root", original.execution), nr("program", job.model.program()),
                    nr("model", original.artifact), nr("input", payload(job.inputs)), nr("output", original.output),
                    nr("profile", b"toy-bounded-profile"), nr("ruleset", ARITHMETIC),
                    nr("draw", draw), nr("assignment", assignment), nr("actor", actor.encode()),
                    nr("role", b"panel" if positive else b"public-watcher"), nr("duty", duty),
                    u(0, 8), u(action_kind, 2), u(step, 8),
                    nr("plan", payload((job.model.rows, 1, 8))),
                    nr("fee-liability", payload(("fixture-funded-original", 2, 19))),
                    nr("deadline", payload((15, 19, 20))))
        return obligation, context

    def action_binding(self, oid: bytes, actor: str, step: int) -> tuple[bytes, bytes, bytes]:
        original = self.records[oid]["original"]
        obligation, native_context = self.cpn_native_scope(oid, actor, False, step)
        context = h("MISAKA/PALW/SWC1/CPN-ACTION-CONTEXT", u(1, 2), native_context,
                    original.opportunity, original.work, oid)
        nonce = h("MISAKA/PALW/CPN1/ACTION-NONCE", obligation, context)
        return obligation, context, nonce

    def challenge(self, oid: bytes, actor: str) -> tuple[bytes, bytes, bytes, int]:
        require(self.panel_seed is not None, "panel-seed-not-drawn")
        record = self.records[oid]
        require(actor in record.get("actors", ()), "actor-not-assigned")
        original = record["original"]
        obligation, native_context = self.cpn_native_scope(oid, actor, True, 0)
        panel_anchor = self.stages["PANEL"].identity()
        context = h("MISAKA/PALW/SWC1/CPN-POSITIVE-CONTEXT", u(1, 2), native_context,
                    original.opportunity, original.work, oid, panel_anchor,
                    nr("panel-beacon-anchor", panel_anchor), nr("panel-seed", self.panel_seed))
        nonce = h("MISAKA/PALW/CPN1/POSITIVE-NONCE", obligation, context,
                  nr("beacon-anchor", panel_anchor), nr("beacon-seed", self.panel_seed))
        n = self.jobs[record["reg"]].model.rows
        top = (1 << 256) // n * n
        for counter in range(8):
            value = int.from_bytes(h("FIXTURE/SAMPLE", nonce, u(counter, 4)), "big")
            if value < top:
                return obligation, context, nonce, value % n
        raise Refused("sample-cap-exhausted")

    def make_positive(self, oid: bytes, actor: str, material: Material, salt: bytes = b"first") -> Positive:
        obligation, context, nonce, target = self.challenge(oid, actor)
        draft = Positive(actor, oid, obligation, context, nonce, target, material.evidence(target), True, Seal(actor, b"", b""))
        return replace(draft, seal=AUTH.seal(actor, draft.subject(), salt))

    def positive(self, statement: Positive, tick: int = 7) -> None:
        oid, actor = statement.original, statement.actor
        record = self.records[oid]
        require(record["state"] in ("VerificationPending", "VerifiedPendingMaturity"), "positive-state")
        require(tick <= 15, "positive-window-closed")
        expected = self.challenge(oid, actor)
        require((statement.obligation, statement.context, statement.nonce, statement.target) == expected, "positive-context")
        require(AUTH.verify(actor, statement.subject(), statement.seal), "positive-auth")
        require(statement.full_scope_declared, "full-scope-not-declared")
        require(statement.evidence.step == statement.target, "positive-target")
        require(check_local(self.jobs[record["reg"]], record["original"], statement.evidence), "positive-local-fault")
        require(statement.obligation not in self.once, "positive-once")
        self.once.add(statement.obligation)
        record["positives"].add(actor)
        # Full replay by at least one honest actor and DA are fixture assumptions, NOT inferred from a sample.
        if oid not in self.open_cases and oid not in self.revocations:
            record["state"] = "VerifiedPendingMaturity"

    def make_negative(self, oid: bytes, evidence: LocalEvidence, actor: str) -> Negative:
        obligation, context, nonce = self.action_binding(oid, actor, evidence.step)
        draft = Negative(actor, oid, obligation, context, nonce, evidence, Seal(actor, b"", b""))
        return replace(draft, seal=AUTH.seal(actor, draft.subject()))

    def refute(self, oid: bytes, evidence: LocalEvidence, actor: str, tick: int) -> str:
        # Sender-side fixture convenience; negative() below is the authenticated receiver.
        return self.negative(self.make_negative(oid, evidence, actor), tick)

    def negative(self, statement: Negative, tick: int) -> str:
        oid, evidence, actor = statement.original, statement.evidence, statement.actor
        require(oid in self.accepted, "refutation-not-reserved")
        record = self.records[oid]
        # Action scope: no fresh admission difficulty, new beacon or positive sample restriction.
        expected = self.action_binding(oid, actor, evidence.step)
        require((statement.obligation, statement.context, statement.nonce) == expected, "negative-context")
        require(AUTH.verify(actor, statement.subject(), statement.seal), "negative-auth")
        require(statement.obligation not in self.once, "negative-once")
        require(not check_local(self.jobs[record["reg"]], record["original"], evidence), "no-local-fault")
        require(self.negative_room[oid] > 0, "negative-reservation-exhausted")
        self.negative_room[oid] -= 1
        self.once.add(statement.obligation)
        self.revocations[oid] = self.revocations.get(oid, 0) + 1
        self.open_cases.discard(oid)
        if tick <= 19:
            record["state"] = "Refuted"
            outcome = "timely-refutation-no-success"
        else:
            record["state"] = "LateEvidenceStopsNewUses"
            outcome = "post-window-new-use-stop-no-retroactive-slash"
        for right in self.rights.values():
            if right["source"] == oid and right["state"] != "Consumed":
                right["state"] = "Invalidated"
        return outcome

    def notice(self, oid: bytes, tick: int) -> None:
        require(tick <= 15, "notice-window-closed")
        require(oid in self.accepted, "notice-not-reserved")
        require(self.records[oid]["state"] in ("VerificationPending", "VerifiedPendingMaturity"), "notice-state")
        require(oid not in self.open_cases, "case-already-open")
        self.open_cases.add(oid)
        self.records[oid]["state"] = "VerificationPending"

    def resolve_no_fault(self, oid: bytes, tick: int) -> None:
        require(self.records[oid]["state"] == "VerificationPending", "case-not-live")
        require(oid in self.open_cases and tick <= 19, "case-terminal-outside-contract")
        self.open_cases.remove(oid)
        record = self.records[oid]
        record["state"] = "VerifiedPendingMaturity" if record["positives"] and oid not in self.revocations else "VerificationPending"

    def abort(self, oid: bytes) -> None:
        self.records[oid]["state"] = "AbortedUnverified"
        self.open_cases.discard(oid)
        # WorkUse/TrialUse stay consumed. Reserving another stage/seed cannot revive this work.

    def eligibility(self, oid: bytes, tick: int) -> bool:
        record = self.records[oid]
        return (record["state"] in ("VerifiedPendingMaturity", "MatureEligible")
                and bool(record["positives"])
                and tick >= 20 and tick > 19 and tick <= self.jobs[record["reg"]].expiry
                and oid not in self.open_cases and oid not in self.revocations)

    def mature(self, oid: bytes, tick: int) -> None:
        require(self.eligibility(oid, tick), "not-mature-eligible")
        self.records[oid]["state"] = "MatureEligible"

    def mint(self, oid: bytes, tick: int) -> bytes:
        self.mature(oid, tick)
        right = h("FIXTURE/RIGHT", self.records[oid]["original"].work, oid, u(0, 4))
        require(right not in self.rights, "right-already-issued")
        self.rights[right] = {"state": "Minted", "source": oid, "purpose": "toy-exec", "quantity": 1}
        return right

    def consume(self, right_id: bytes, tick: int) -> None:
        right = self.rights[right_id]
        require(self.eligibility(right["source"], tick), "current-source-ineligible")
        require(right["state"] == "Minted", "right-not-spendable")
        right["state"] = "Consumed"
        right["consumed_batch"] = self.batch

    def batch_apply(self, tick: int, actions: tuple[tuple[Any, ...], ...]) -> tuple[str, ...]:
        """Body order cannot put a consume ahead of a valid source stop in the same batch."""
        next_state = copy.deepcopy(self)
        next_state.batch += 1
        outcomes = []
        for action in actions:
            if action[0] == "refute":
                outcomes.append(next_state.refute(action[1], action[2], action[3], tick))
        for action in actions:
            if action[0] == "consume":
                try:
                    next_state.consume(action[1], tick)
                    outcomes.append("consumed")
                except Refused as error:
                    outcomes.append(error.reason)
        self.__dict__ = next_state.__dict__
        return tuple(outcomes)

    def checkpoint(self) -> tuple["Ledger", bytes]:
        state = copy.deepcopy(self)
        return state, fingerprint(state.view())

    def restore(self, checkpoint: tuple["Ledger", bytes], trusted_root: bytes) -> None:
        state, carried = checkpoint
        require(carried == trusted_root and fingerprint(state.view()) == trusted_root, "checkpoint-authentication")
        self.__dict__ = copy.deepcopy(state.__dict__)

    def prune(self, expired_job_slot_floor: int) -> None:
        require(expired_job_slot_floor >= self.floor, "floor-cannot-decrease")
        old = [oid for oid, r in self.records.items() if self.jobs[r["reg"]].job_slot <= expired_job_slot_floor]
        require(len(old) == len(self.records), "prune-requires-closed-model-cohort")
        require(all(self.records[x]["state"] in ("Refuted", "AbortedUnverified", "LateEvidenceStopsNewUses") for x in old), "prune-live-duty")
        require(not any(r["source"] in old and r["state"] == "Consumed" for r in self.rights.values()), "retain-consumed-provenance")
        self.archive_root = h("FIXTURE/EXPIRED-SUMMARY", self.archive_root, fingerprint(tuple(sorted(old))), u(expired_job_slot_floor, 8))
        self.archived_records += len(old)
        self.floor = expired_job_slot_floor
        for oid in old:
            record = self.records.pop(oid)
            regid = record["reg"]
            self.work_use.pop(record["original"].work)
            self.work_owner.pop(record["original"].work, None)
            self.accepted.discard(oid)
            self.negative_room.pop(oid, None)
            self.revocations.pop(oid, None)
            self.jobs.pop(regid)
            reg = self.registrations.pop(regid)
            self.trials.pop(reg.trial())
        self.rights = {key: right for key, right in self.rights.items() if right["source"] not in old}
        # Fixed fixture phase is complete. Old job namespaces stay closed by the authenticated floor.
        self.once.clear()
        self.stages.clear()
        self.oracle = BeaconFixture()
        self.ticket_seed = self.panel_seed = None


def setup(zero: bool = False, corrupt: bool = False, difficulty: int = 1,
          shortcut: bool = False, capital: str = "coin-0") -> tuple[Ledger, Registration, Material]:
    model = ToyModel(zero)
    ledger = Ledger()
    reg = Registration("alice", "alice-payee", capital, 0, 1, model.ref, difficulty=difficulty)
    ledger.register(reg, AUTH.seal(reg.owner, reg.identity()))
    ledger.freeze_job()
    ledger.bind_jobs()
    material = Material(reg, ledger.jobs[reg.identity()], model, corrupt, shortcut)
    ledger.commit(reg, material.original, AUTH.seal(reg.owner, material.original.identity()))
    return ledger, reg, material


def prepare_panel(ledger: Ledger) -> None:
    ledger.freeze_ticket()
    ledger.draw_tickets()
    ledger.admit()
    ledger.freeze_panel()
    ledger.draw_panel()


def honest_approval(ledger: Ledger, material: Material) -> Positive:
    oid = material.original.identity()
    # Full replay is done here by the fixture Panel before making its declaration.
    reg = ledger.registrations[ledger.records[oid]["reg"]]
    replay = Material(reg, ledger.jobs[reg.identity()], material.model)
    require(replay.original == material.original, "honest-panel-full-replay-mismatch")
    statement = ledger.make_positive(oid, ledger.records[oid]["actors"][0], material)
    ledger.positive(statement)
    return statement


def liar_approval_with_locally_correct_sample(ledger: Ledger, material: Material) -> Positive:
    oid = material.original.identity()
    for actor in ledger.records[oid]["actors"]:
        statement = ledger.make_positive(oid, actor, material)
        if check_local(material.job, material.original, statement.evidence):
            ledger.positive(statement)
            return statement
    raise AssertionError("fixed adversarial fixture needs an uncorrupted sample; do not protocol-reroll a seed")


def positive_flow() -> dict[str, Any]:
    ledger, reg, material = setup()
    oid = material.original.identity()
    expect_refusal("not-mature-eligible", lambda: ledger.mint(oid, 2))
    prepare_panel(ledger)
    statement = honest_approval(ledger, material)
    expect_refusal("not-mature-eligible", lambda: ledger.mint(oid, 19))
    right = ledger.mint(oid, 20)
    assert not ledger.settled
    assert ledger.batch_apply(21, (("consume", right),)) == ("consumed",)
    expect_refusal("right-not-spendable", lambda: ledger.consume(right, 22))
    return {"registration": reg.identity(), "work_key": material.job.work_key(), "original": oid,
            "stage_freezes": {k: s.identity() for k, s in ledger.stages.items()},
            "ticket": hex(ledger.records[oid]["ticket"]), "panel_target": statement.target,
            "right_state": ledger.rights[right]["state"], "ledger_settled": ledger.settled}


def invalid_front_run() -> dict[str, Any]:
    model, ledger = ToyModel(), Ledger()
    reg = Registration("alice", "alice-payee", "coin-0", 0, 1, model.ref)
    before = fingerprint(ledger.view())
    expect_refusal("registration-auth", lambda: ledger.register(reg, AUTH.seal("mallory", reg.identity())))
    unpaid = replace(reg, capital="invented-coin")
    expect_refusal("registration-unfunded", lambda: ledger.register(unpaid, AUTH.seal("alice", unpaid.identity())))
    assert fingerprint(ledger.view()) == before
    ledger.register(reg, AUTH.seal("alice", reg.identity()))
    ledger.freeze_job()
    ledger.bind_jobs()
    material = Material(reg, ledger.jobs[reg.identity()], model)
    before = fingerprint(ledger.view())
    expect_refusal("original-auth", lambda: ledger.commit(reg, material.original, AUTH.seal("mallory", material.original.identity())))
    wrong = replace(material.original, work=h("attacker-self-declared-work", b"wrong"))
    expect_refusal("work-owner-mismatch", lambda: ledger.commit(reg, wrong, AUTH.seal("alice", wrong.identity())))
    assert fingerprint(ledger.view()) == before and not ledger.work_use
    ledger.commit(reg, material.original, AUTH.seal("alice", material.original.identity()))
    return {"invalid_inputs_changed_canonical_state": False, "legitimate_owner_committed": True}


def envelope_alias_funding_reroll() -> dict[str, Any]:
    ledger, reg, material = setup()
    alias = replace(reg, display_alias="a-different-model-name")
    assert alias.identity() == reg.identity()
    before = fingerprint(ledger.view())
    for name, external in (("nonce", b"999"), ("relay", b"relay-B"), ("signature", b"different-signature-randomizer")):
        expect_refusal("work-already-fixed", lambda salt=external: ledger.commit(alias, material.original, AUTH.seal("alice", material.original.identity(), salt)))
    assert fingerprint(ledger.view()) == before
    # A different valid capital slot is a separately paid registration. Its seed must ACTUALLY
    # change the job; it cannot claim the old expected WorkKey by changing an outer funding label.
    second = replace(reg, capital="coin-1")
    paired = Ledger()
    for registration in (reg, second):
        paired.register(registration, AUTH.seal("alice", registration.identity()))
    paired.freeze_job()
    paired.bind_jobs()
    first_material = Material(reg, paired.jobs[reg.identity()], material.model)
    second_material = Material(second, paired.jobs[second.identity()], material.model)
    paired.commit(reg, first_material.original, AUTH.seal("alice", first_material.original.identity()))
    borrowed = replace(first_material.original, registration=second.identity(), opportunity=second.opportunity())
    expect_refusal("work-owner-mismatch", lambda: paired.commit(second, borrowed, AUTH.seal("alice", borrowed.identity())))
    assert second_material.job.inputs != first_material.job.inputs
    assert second_material.job.work_key() != first_material.job.work_key()
    reused_leaf = (reg, first_material.original, AUTH.seal("alice", first_material.original.identity()))
    fresh_leaf = (second, second_material.original, AUTH.seal("alice", second_material.original.identity()))
    paired_before = fingerprint(paired.view())
    wrapper_roots = []
    for batch in ((reused_leaf, fresh_leaf), (fresh_leaf, reused_leaf)):
        wrapper_roots.append(h("FIXTURE/BATCH-WRAPPER", payload(tuple(x[1].identity() for x in batch))))
        expect_refusal("work-already-fixed", lambda batch=batch: paired.commit_batch(batch))
        assert fingerprint(paired.view()) == paired_before
    assert wrapper_roots[0] != wrapper_roots[1]
    same_calculation = replace(material.job, job_slot=99, expiry=999)
    assert same_calculation.work_key() == material.job.work_key()
    return {"same_work_original_count": len(ledger.work_use), "aliases_and_resigning_create_draws": 0,
            "funding_only_is_not_work_identity": True, "legitimate_new_job_differs_in_actual_input": True,
            "funded_other_slot_cannot_claim_old_work": True, "batch_reordering_changes_only_wrapper": True,
            "adding_fresh_leaf_cannot_reroll_reused_leaf": True, "failed_atomic_batch_leaves_no_partial_new_claim": True}


def freeze_and_failed_opportunity() -> dict[str, Any]:
    assert len(encode_d(1)) == len(encode_d(D_MAX)) == 16
    assert decode_d(encode_d(1)) == 1 and decode_d(encode_d(D_MAX)) == D_MAX
    expect_refusal("difficulty-range", lambda: encode_d(0))
    expect_refusal("difficulty-range", lambda: encode_d(D_MAX + 1))
    expect_refusal("difficulty-width", lambda: decode_d(encode_d(1) + b"\x00"))
    expect_refusal("difficulty-range", lambda: decode_d(u(1 << 127, 16)))
    assert threshold(1) == MAX256 and ticket_admits(MAX256, 1) and ticket_admits(0, 1)
    expect_refusal("ticket-range", lambda: ticket_admits(MAX256 + 1, 1))
    edge = threshold(D_MAX)
    assert ticket_admits(edge - 1, D_MAX) and ticket_admits(edge, D_MAX)
    assert not ticket_admits(edge + 1, D_MAX)
    ledger, reg, material = setup(difficulty=D_MAX)
    stage = ledger.freeze_ticket()
    expect_refusal("seed-not-yet-known-in-fixture", lambda: ledger.oracle.read(stage, 3))
    changed = replace(material.original, output=h("false-output", b"new"))
    expect_refusal("original-set-closed", lambda: ledger.commit(reg, changed, AUTH.seal("alice", changed.identity())))
    ledger.draw_tickets()
    oid = material.original.identity()
    assert ledger.records[oid]["state"] == "Lost"  # fixed nonzero vector; no seed reroll in test
    expect_refusal("ticket-already-drawn", lambda: ledger.draw_tickets())
    expect_refusal("stage-already-closed", lambda: ledger.oracle.freeze(replace(stage, extras=b"easier-D"), 3))
    expect_refusal("original-set-closed", lambda: ledger.commit(reg, material.original, AUTH.seal("alice", material.original.identity())))
    ledger.abort(oid)
    assert material.original.work in ledger.work_use and reg.trial() in ledger.trials
    return {"losing_ticket": hex(ledger.records[oid]["ticket"]), "lost_and_aborted_work_remains_used": True,
            "root_and_target_changes_refused_after_freeze": True, "difficulty_container_bits": 128,
            "difficulty_range": [1, D_MAX], "D1_inclusive_threshold": hex(MAX256),
            "Dmax_inclusive_threshold": hex(edge), "threshold_below_equal_above_checked": True}


def counterfeit_and_local_refutation() -> dict[str, Any]:
    ledger, reg, material = setup(corrupt=True)
    prepare_panel(ledger)
    oid = material.original.identity()
    assert ledger.records[oid]["ticket"] <= MAX256  # D=1: even this false root wins
    assert not ledger.rights and not ledger.eligibility(oid, 10)
    evidence = material.evidence(material.model.ref.rows - 1)
    bad_opening = replace(evidence, after=replace(evidence.after, blob=u(0, 8)))
    before = fingerprint(ledger.view())
    expect_refusal("opening-invalid", lambda: ledger.refute(oid, bad_opening, "public-watcher", 10))
    assert fingerprint(ledger.view()) == before
    result = ledger.refute(oid, evidence, "public-watcher", 10)
    expect_refusal("not-mature-eligible", lambda: ledger.mint(oid, 25))
    return {"false_root_passed_ticket": True, "local_authenticated_mismatch": True,
            "outcome": result, "rights": len(ledger.rights), "fresh_negative_ticket_required": False}


def panel_context_copy_and_shared_material() -> dict[str, Any]:
    ledger, reg, material = setup()
    prepare_panel(ledger)
    oid = material.original.identity()
    first_actor, second_actor = ledger.records[oid]["actors"]
    first = ledger.make_positive(oid, first_actor, material)
    transplanted = replace(first, actor=second_actor)
    before = fingerprint(ledger.view())
    expect_refusal("positive-context", lambda: ledger.positive(transplanted))
    assert fingerprint(ledger.view()) == before
    honest_approval(ledger, material)
    second = ledger.make_positive(oid, second_actor, material)
    ledger.positive(second)  # both use the SAME retained miner trace, each in their own scope
    expect_refusal("positive-once", lambda: ledger.positive(ledger.make_positive(oid, first_actor, material, b"re-sign")))
    right = ledger.mint(oid, 20)
    expect_refusal("right-already-issued", lambda: ledger.mint(oid, 20))
    return {"distinct_positive_nonces": first.nonce != second.nonce,
            "sample_targets": [first.target, second.target], "shared_authenticated_trace_allowed": True,
            "obligations_consumed": len(ledger.once), "work_rights": len(ledger.rights),
            "independent_physical_replay_proven": False, "right": right}


def stage_knowledge_and_unavailable() -> dict[str, Any]:
    ledger, reg, material = setup()
    known_job = ledger.oracle.read(ledger.stages["JOB"], 2)
    claimed_future_ticket = h("bad-domain-separated-ticket", known_job)
    claimed_future_panel = h("bad-domain-separated-panel", known_job)
    assert claimed_future_ticket != claimed_future_panel
    assert h("bad-domain-separated-ticket", known_job) == claimed_future_ticket
    # Different hash domains give different values, not hidden values once their input is known.
    stage = ledger.freeze_ticket()
    expect_refusal("seed-not-yet-known-in-fixture", lambda: ledger.oracle.read(stage, 3))
    ledger.oracle.fail(stage)
    expect_refusal("beacon-unavailable-no-fallback", lambda: ledger.draw_tickets())
    expect_refusal("stage-not-frozen", lambda: ledger.oracle.read(replace(stage, slot=stage.slot + 1), 4))
    ledger.abort(material.original.identity())
    other, _, other_material = setup()
    prepare_panel(other)
    pstage = other.stages["PANEL"]
    expect_refusal("stage-not-frozen", lambda: other.oracle.read(replace(pstage, extras=payload(("favoured-panel",))), 6))
    # An already admitted third-party action works BEFORE PanelFreeze exists and keeps the
    # exact same context after a later Panel beacon becomes unavailable.
    relief, _, false_material = setup(corrupt=True)
    relief.freeze_ticket()
    relief.draw_tickets()
    relief.admit()
    oid = false_material.original.identity()
    proof = false_material.evidence(false_material.model.ref.rows - 1)
    before_context = relief.action_binding(oid, "public-watcher", proof.step)
    negative = relief.make_negative(oid, proof, "public-watcher")
    frozen_panel = relief.freeze_panel()
    relief.oracle.fail(frozen_panel)
    expect_refusal("beacon-unavailable-no-fallback", lambda: relief.draw_panel())
    assert relief.panel_seed is None and "actors" not in relief.records[oid]
    assert relief.action_binding(oid, "public-watcher", proof.step) == before_context
    altered = replace(negative, nonce=h("attacker-future-seed-nonce", b"injected"))
    expect_refusal("negative-context", lambda: relief.negative(altered, 7))
    assert relief.negative(negative, 7) == "timely-refutation-no-success"
    expect_refusal("negative-once", lambda: relief.negative(negative, 7))
    native_seed = bytes(range(64))
    assert len(nr("native-seed", native_seed)) == 4 + len("native-seed") + 2 + 4 + 64
    assert h("fixture-full-native-seed", nr("native-seed", native_seed)) != h("fixture-full-native-seed", nr("native-seed", native_seed[:32]))
    assert frozen_set_root("JOB", ()) != frozen_set_root("TICKET", ())
    assert frozen_set_root("JOB", (b"a", b"b")) != frozen_set_root("JOB", (b"b", b"a"))
    expect_refusal("unknown-stage", lambda: frozen_set_root("UNKNOWN", ()))
    return {"known_seed_domains_differ": True, "adversary_computes_both_immediately": True,
            "domain_separation_establishes_stage_unknownness": False,
            "unavailable_is_not_zero_demand": True, "fallback_new_seed_refused": True,
            "pool_substitution_refused": True, "beacon_security_proved": False,
            "action_context_fixed_before_panel_freeze": True, "negative_succeeded_while_panel_beacon_unavailable": True,
            "action_has_no_panel_seed_dependency": True, "native_64_byte_seed_not_truncated": True,
            "stage_set_ordinal_empty_and_domain_rules_checked": True}


def maturity_and_case_boundaries() -> dict[str, Any]:
    # Independent review regression: resolving a no-fault case is not a positive
    # full-scope approval, and must never manufacture one.
    no_approval, _, empty_material = setup()
    prepare_panel(no_approval)
    empty_oid = empty_material.original.identity()
    no_approval.notice(empty_oid, 15)
    no_approval.resolve_no_fault(empty_oid, 19)
    assert no_approval.records[empty_oid]["state"] == "VerificationPending"
    assert not no_approval.records[empty_oid]["positives"]
    expect_refusal("not-mature-eligible", lambda: no_approval.mint(empty_oid, 20))
    # Even an erroneously cached phase label cannot bypass the consumer predicate.
    no_approval.records[empty_oid]["state"] = "VerifiedPendingMaturity"
    expect_refusal("not-mature-eligible", lambda: no_approval.mint(empty_oid, 20))
    no_approval.records[empty_oid]["state"] = "VerificationPending"
    late_statement = no_approval.make_positive(empty_oid, no_approval.records[empty_oid]["actors"][0], empty_material)
    expect_refusal("positive-window-closed", lambda: no_approval.positive(late_statement, 20))
    aborted, _, aborted_material = setup()
    prepare_panel(aborted)
    aborted_oid = aborted_material.original.identity()
    honest_approval(aborted, aborted_material)
    aborted.abort(aborted_oid)
    aborted_before = fingerprint(aborted.view())
    expect_refusal("notice-state", lambda: aborted.notice(aborted_oid, 15))
    expect_refusal("case-not-live", lambda: aborted.resolve_no_fault(aborted_oid, 19))
    expect_refusal("not-mature-eligible", lambda: aborted.mint(aborted_oid, 20))
    assert fingerprint(aborted.view()) == aborted_before
    ledger, _, material = setup()
    prepare_panel(ledger)
    honest_approval(ledger, material)
    oid = material.original.identity()
    ledger.notice(oid, 15)  # inclusive final notice tick
    expect_refusal("notice-window-closed", lambda: ledger.notice(oid, 16))
    held_branch = copy.deepcopy(ledger)
    expect_refusal("not-mature-eligible", lambda: held_branch.mature(oid, 25))  # time alone does not close a case
    ledger.resolve_no_fault(oid, 19)
    expect_refusal("not-mature-eligible", lambda: ledger.mature(oid, 19))
    ledger.mature(oid, 20)
    assert not ledger.settled
    return {"inclusive_notice_tick": 15, "last_terminal_tick": 19, "first_mature_tick": 20,
            "unresolved_case_blocks_after_clock_passed": True, "maturity_is_settlement": False,
            "zero_positive_case_resolution_cannot_mint": True, "eligibility_rechecks_positive_not_only_phase": True,
            "aborted_state_cannot_be_revived_by_notice_or_resolution": True, "late_positive_refused": True}


def same_batch_stop_and_prior_consumption() -> dict[str, Any]:
    ledger, _, material = setup(corrupt=True)
    prepare_panel(ledger)
    oid = material.original.identity()
    # This fixture explicitly violates the one-honest-full-replay assumption. A sampled
    # approval can be wrong; a later objective proof must stop NEW uses without pretending
    # to prove that previously settled history is rolled back by this model.
    statement = liar_approval_with_locally_correct_sample(ledger, material)
    assert statement.target != material.model.ref.rows - 1
    right = ledger.mint(oid, 20)
    proof = material.evidence(material.model.ref.rows - 1)
    original = ledger.checkpoint()
    bodies = (("consume", right), ("refute", oid, proof, "public-watcher"))
    endings = []
    for permutation in itertools.permutations(bodies):
        trial = copy.deepcopy(ledger)
        outcomes = trial.batch_apply(21, permutation)
        assert "current-source-ineligible" in outcomes
        assert trial.rights[right]["state"] == "Invalidated"
        endings.append(fingerprint(trial.view()))
    assert len(set(endings)) == 1
    assert ledger.batch_apply(21, (("consume", right),)) == ("consumed",)
    consumed_batch = ledger.rights[right]["consumed_batch"]
    result = ledger.batch_apply(22, (("refute", oid, proof, "public-watcher"),))
    assert ledger.rights[right]["state"] == "Consumed"
    assert ledger.rights[right]["consumed_batch"] == consumed_batch
    expect_refusal("not-mature-eligible", lambda: ledger.mint(oid, 23))
    ledger.restore(original, original[1])  # ordinary canonical undo restores the COMPLETE checkpoint
    assert ledger.rights[right]["state"] == "Minted" and oid not in ledger.revocations and ledger.batch == 0
    return {"same_batch_all_body_orders_equal": True, "sample_outside_fraud_step": statement.target,
            "prior_batch_consumed_provenance_preserved": True, "late_evidence_outcome": result,
            "retroactive_slash_or_settlement_rollback_claimed": False, "canonical_checkpoint_undo_in_model": True}


def finite_nullifier_restart() -> dict[str, Any]:
    ledger, reg, material = setup()
    ledger.abort(material.original.identity())
    before_prune = ledger.checkpoint()
    # Independent review regression: the record's registration resolver is part of
    # authenticated state, including the job/expiry it selects.
    swapped_registration = copy.deepcopy(before_prune)
    swapped_registration[0].records[material.original.identity()]["reg"] = h("different-registration", b"substitution")
    expect_refusal("checkpoint-authentication", lambda: Ledger().restore(swapped_registration, before_prune[1]))
    ledger.prune(1)
    assert not ledger.work_use and not ledger.trials and ledger.floor == 1
    after_prune = ledger.checkpoint()
    restarted = Ledger()
    restarted.restore(after_prune, after_prune[1])
    expect_refusal("expired-job-namespace", lambda: restarted.register(reg, AUTH.seal("alice", reg.identity())))
    expect_refusal("expired-job-namespace", lambda: restarted.commit(reg, material.original, AUTH.seal("alice", material.original.identity())))
    same_capital_new_epoch = replace(reg, job_slot=reg.job_slot + 1)
    expect_refusal("trial-sequence-reused", lambda: restarted.register(same_capital_new_epoch, AUTH.seal("alice", same_capital_new_epoch.identity())))
    expect_refusal("checkpoint-authentication", lambda: restarted.restore(before_prune, after_prune[1]))
    corrupt = copy.deepcopy(after_prune)
    corrupt[0].floor = -1
    expect_refusal("checkpoint-authentication", lambda: restarted.restore(corrupt, after_prune[1]))
    return {"live_work_nullifiers_after_gc": len(restarted.work_use), "live_trial_nullifiers_after_gc": len(restarted.trials),
            "capital_sequence_highwater_after_gc": dict(restarted.trial_highwater),
            "authenticated_expired_floor": restarted.floor, "summary_root": restarted.archive_root,
            "archived_terminal_records": restarted.archived_records, "old_work_resurrected": False,
            "new_epoch_does_not_reset_capital_sequence": True, "record_registration_reference_tamper_refused": True,
            "trusted_checkpoint_authentication_is_fixture": True, "native_ibd_or_accumulator_verified": False}


def cached_zero_model_counterexample() -> dict[str, Any]:
    ledger, reg, normal = setup(zero=True)
    optimized = Material(reg, normal.job, normal.model, shortcut=True)
    assert optimized.states == normal.states and optimized.original == normal.original
    assert optimized.metrics["scalar_multiplications"] == 0 < normal.metrics["scalar_multiplications"]
    for i in range(normal.model.ref.rows):
        assert check_local(normal.job, normal.original, optimized.evidence(i))
    cached = normal.original  # copying a correct commitment costs no new VM invocation in this toy accounting
    assert cached.identity() == normal.original.identity()
    prepare_panel(ledger)
    honest_approval(ledger, optimized)
    oid = normal.original.identity()
    ledger.mint(oid, 20)
    expect_refusal("no-local-fault", lambda: ledger.refute(oid, optimized.evidence(0), "watcher", 21))
    # A separate legitimately funded registration has a fresh, fully used input seed but the
    # zero model is still cheaply evaluable. We do not call this an arithmetic violation.
    alternate = replace(reg, capital="coin-1")
    newjob = derive_job(alternate, ledger.oracle.read(ledger.stages["JOB"], 2), ledger.stages["JOB"].identity())
    newnormal = Material(alternate, newjob, normal.model)
    newfast = Material(alternate, newjob, normal.model, shortcut=True)
    assert newjob.inputs != normal.job.inputs and newjob.work_key() != normal.job.work_key()
    assert newnormal.original == newfast.original
    return {"normal": normal.metrics, "optimized": optimized.metrics,
            "identical_valid_trace_and_root": True, "fresh_seed_does_not_force_large_scalar_work_on_zero_model": True,
            "correct_cached_or_optimized_results_slashed": False, "cost_lower_bound_proved": False,
            "metric_limit": "toy scalar-operation counts only; root hashing, memory, model checks and physical time still cost work"}


def global_capacity_keeps_reserved_refutation() -> dict[str, Any]:
    ledger, model = Ledger(capacity=3), ToyModel()
    regs = [Registration("alice", "alice-payee", f"coin-{i}", 0, 1, model.ref, display_alias=f"alias-{i}") for i in range(3)]
    for reg in regs:
        ledger.register(reg, AUTH.seal("alice", reg.identity()))
    ledger.freeze_job()
    ledger.bind_jobs()
    materials = {}
    for reg in regs:
        material = Material(reg, ledger.jobs[reg.identity()], model, corrupt_final=True)
        materials[material.original.identity()] = material
        ledger.commit(reg, material.original, AUTH.seal("alice", material.original.identity()))
    ledger.freeze_ticket()
    ledger.draw_tickets()
    accepted = ledger.admit(maximum=2)
    assert len(accepted) == 2 and sum(r["state"] == "CapacityNotAdmitted" for r in ledger.records.values()) == 1
    before = len(ledger.accepted)
    oid = accepted[0]
    outcome = ledger.refute(oid, materials[oid].evidence(model.ref.rows - 1), "independent-watcher", 10)
    assert len(ledger.accepted) == before and ledger.negative_room[oid] == 1
    return {"ticket_eligible": 3, "global_accepted": 2, "capacity_not_admitted": 1,
            "reserved_negative_succeeded_under_full_new_claim_budget": True, "outcome": outcome,
            "one_bps_or_large_model_performance_measured": False}


CASES = (
    ("C01", "owner_to_actual_integer_execution_to_mature_one_time_right", positive_flow),
    ("C02", "unauthenticated_or_unfunded_front_run_cannot_consume_owner", invalid_front_run),
    ("C03", "alias_signature_transport_and_funding_do_not_reroll_same_work", envelope_alias_funding_reroll),
    ("C04", "freeze_root_target_losing_and_aborted_opportunity_are_not_reset", freeze_and_failed_opportunity),
    ("C05", "false_root_may_win_but_authenticated_local_refutation_blocks_success", counterfeit_and_local_refutation),
    ("C06", "panel_context_copy_rejected_shared_openings_allowed_once", panel_context_copy_and_shared_material),
    ("C07", "known_seed_domain_separation_is_not_future_unknownness_and_no_fallback", stage_knowledge_and_unavailable),
    ("C08", "inclusive_notice_unresolved_case_strict_release_and_not_settled", maturity_and_case_boundaries),
    ("C09", "same_batch_stop_priority_prior_batch_provenance_and_canonical_undo", same_batch_stop_and_prior_consumption),
    ("C10", "finite_nullifier_gc_restart_and_expired_namespace_nonresurrection", finite_nullifier_restart),
    ("C11", "negative_demonstration_correct_cache_and_zero_model_shortcut_pass", cached_zero_model_counterexample),
    ("C12", "global_admission_limit_preserves_existing_negative_reservation", global_capacity_keeps_reserved_refutation),
)


def read_source_hashes() -> dict[str, str]:
    return {path: hashlib.sha256((ROOT / path).read_bytes()).hexdigest() for path in SOURCE_PATHS}


def main() -> int:
    initial_hashes = read_source_hashes()
    results = []
    for cid, name, function in CASES:
        try:
            details = function()
            results.append({"id": cid, "name": name, "passed": True, "details": jsonable(details)})
        except Exception as error:
            results.append({"id": cid, "name": name, "passed": False, "error": str(error),
                            "traceback": traceback.format_exc(limit=5)})
    final_hashes = read_source_hashes()
    unchanged = initial_hashes == final_hashes
    passed = unchanged and all(item["passed"] for item in results)
    result = {
        "schema_version": 1,
        "scope": "abstract-model-only",
        "model": "SWC1 integer-trace integration contract v1",
        "passed": passed,
        "source_hashes": final_hashes,
        "sources_unchanged_during_run": unchanged,
        "model_script_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "normative_outer_codec_parts": [
            "SHA-256, LP=u32le length, NR=tag/version/native bytes",
            "D=u128 little-endian container with exact range 1..2^120-1; ticket uint256 big-endian",
            "SWC1 WorkTicket, JobSeed, StageDescriptor, FreezeId, BeaconMessage domains and field order",
            "SWC1 SET leaf/ordinal/padding/parent/empty-root domains",
            "CPN1 outer context/obligation and SWC1 positive/action context distinction",
        ],
        "case_count": len(results),
        "cases_passed": sum(item["passed"] for item in results),
        "fixtures": [
            "bounded typed NR payload codec is toy-specific, not a native wire implementation",
            "HMAC actor authentication is not ML-DSA/native signature verification",
            "deterministic public beacon vectors and release API model knowledge order, not cryptographic unpredictability",
            "stable control cuts, roster identity, funding ownership and trusted snapshot roots are fixture facts",
            "material is locally available to the fixture Panel; no adversarial DA protocol is implemented",
            "u64 modular dot-product model is not an LLM and the operation counters are not measured physical costs",
            "checkpoint deepcopy plus authenticated digest is not native DB, IBD, crash recovery or a history accumulator",
            "canonical batch ordering and checkpoint undo do not prove DAG common-prefix, convergence or finality",
            "toy ticks, two selected actors, scalar stock limits and two negative slots are fixture sizes, not adopted timing/Panel/court constants",
            "stage ordinals are the fixture's bounded canonical ID order; native order and vector meter are not implemented",
            "GC models a complete expired bounded cohort without consumed rights; native partial-history accumulator and settled-provenance pruning are not implemented",
            "the CAD1 feedback controller is not reimplemented here; only its D range, codec and ticket gate are integrated",
        ],
        "not_verified": ["BLS", "DKG", "native signatures", "native DB", "native LLM", "network DA", "DAG theorem",
                         "PoW cost lower bound", "non-amortization", "Studio native route", "one BPS performance", "10000-model load"],
        "negative_demonstrations": {
            "known_seed_domain_separation_gives_stage_unpredictability": False,
            "correct_optimized_trace_implies_full_physical_work": False,
            "mature_work_implies_settled_ledger": False,
            "valid_local_samples_prove_full_panel_replay": False,
            "cost_lower_bound_proved": False,
        },
        "results": results,
    }
    if not unchanged:
        result["initial_source_hashes"] = initial_hashes
        result["failure"] = "RFC source changed during execution; rerun after edits finish"
    output = HERE / "model-results.json"
    output.write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": passed, "scope": result["scope"], "cases_passed": result["cases_passed"],
                      "case_count": len(results), "source_hashes": final_hashes,
                      "failed": [r["id"] for r in results if not r["passed"]]}, ensure_ascii=False))
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
