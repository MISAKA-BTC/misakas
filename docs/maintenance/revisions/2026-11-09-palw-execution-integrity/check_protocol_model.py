#!/usr/bin/env python3
"""Finite abstract checks for the 2026-11-09 PALW document revision.

This is an executable, deliberately small model of RFC-v2 16/18/19/20.
It is NOT the Rust implementation, a real LLM, a production policy, a proof
of cryptographic security, a resource measurement, or an activation test.
The local-evidence part uses actual SHA-256 positional Merkle openings;
signatures, draw authority and canonical inclusion are explicit assumptions.

Run from any directory.  Results are written beside this script by default.
    python3 check_protocol_model.py
    python3 check_protocol_model.py --json
"""

from __future__ import annotations

import argparse
import copy
import hashlib
import itertools
import json
from dataclasses import dataclass, field, replace
from pathlib import Path


def digest(value: object) -> str:
    raw = json.dumps(value, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(raw).hexdigest()


@dataclass(frozen=True)
class Opening:
    tree: str
    index: int
    value: int | str
    siblings: tuple[str, ...]


class Merkle:
    """Tiny fixed-shape positional tree, with explicit domain and padding."""

    def __init__(self, tree: str, values: tuple[int, ...]):
        self.tree = tree
        self.values = values
        width = 1 << (max(1, len(values)) - 1).bit_length()
        leaves = [digest(("leaf", tree, i, v)) for i, v in enumerate(values)]
        leaves += [digest(("pad", tree, i)) for i in range(len(values), width)]
        self.levels = [leaves]
        while len(self.levels[-1]) > 1:
            row = self.levels[-1]
            self.levels.append([digest(("branch", row[i], row[i + 1]))
                                for i in range(0, len(row), 2)])
        self.root = self.levels[-1][0]

    def open(self, index: int) -> Opening:
        at = index
        path = []
        for row in self.levels[:-1]:
            path.append(row[at ^ 1])
            at //= 2
        return Opening(self.tree, index, self.values[index], tuple(path))


def opening_valid(opening: Opening, root: str, count: int) -> bool:
    depth = (max(1, count) - 1).bit_length()
    if not 0 <= opening.index < count or len(opening.siblings) != depth:
        return False
    current = digest(("leaf", opening.tree, opening.index, opening.value))
    at = opening.index
    for sibling in opening.siblings:
        current = (digest(("branch", sibling, current)) if at & 1
                   else digest(("branch", current, sibling)))
        at //= 2
    return current == root


# The immutable native program is a tiny exact integer multiply, add and emit.
# It is an explanatory subdomain, not a choice of PALW's numeric/kernel policy.
PROGRAM = digest(("toy-native-v1", "p=w*x", "z=p+b", "emit-z", "halt"))
SCOPE = ("toy-chain", "claim-v-next", "rules-1", "draw-1", "branch-A")
RELATIONS = ("WellFormedness", "Init", "Mul", "Add", "ReadMul", "ReadAdd", "StatePreserved",
             "EndOutput", "EndLength", "EndWork", "EndHalt")
# Trace positions: initial, product, sum, declared output, mul read-from,
# add read-from, halt, output length, work units, unchanged state, step count,
# and a bounded authenticated raw schema tag.  This does not model all real codecs.
NEEDED = {
    "WellFormedness": (("trace", 11),),
    "Init": (("input", 0), ("trace", 0)),
    "Mul": (("model", 0), ("trace", 0), ("trace", 1)),
    "Add": (("model", 1), ("trace", 1), ("trace", 2)),
    "ReadMul": (("trace", 4),),
    "ReadAdd": (("trace", 5),),
    "StatePreserved": (("trace", 9),),
    "EndOutput": (("trace", 2), ("trace", 3)),
    "EndLength": (("trace", 7),),
    "EndWork": (("trace", 8),),
    "EndHalt": (("trace", 6), ("trace", 10)),
}


def canonical_trace(x: int, w: int, b: int) -> tuple[int, ...]:
    # Independent direct execution oracle; the local checker below never calls it.
    product = w * x
    result = product + b
    return (x, product, result, result, 0, 1, 1, 1, 2, 0, 2, 1)


def evaluate_relation(relation: str, v: dict[tuple[str, int], int]) -> bool:
    if relation == "WellFormedness":
        return v[("trace", 11)] == 1
    if relation == "Init":
        return v[("trace", 0)] == v[("input", 0)]
    if relation == "Mul":
        return v[("trace", 1)] == v[("model", 0)] * v[("trace", 0)]
    if relation == "Add":
        return v[("trace", 2)] == v[("trace", 1)] + v[("model", 1)]
    if relation == "ReadMul":
        return v[("trace", 4)] == 0
    if relation == "ReadAdd":
        return v[("trace", 5)] == 1
    if relation == "StatePreserved":
        return v[("trace", 9)] == 0
    if relation == "EndOutput":
        return v[("trace", 3)] == v[("trace", 2)]
    if relation == "EndLength":
        return v[("trace", 7)] == 1
    if relation == "EndWork":
        return v[("trace", 8)] == 2
    if relation == "EndHalt":
        return v[("trace", 6)] == 1 and v[("trace", 10)] == 2
    raise ValueError(relation)


@dataclass(frozen=True)
class Original:
    claim: str
    scope: tuple[str, ...]
    program: str
    roots: tuple[tuple[str, str, int], ...]


@dataclass(frozen=True)
class Evidence:
    claim: str
    scope: tuple[str, ...]
    program: str
    relation: str
    openings: tuple[Opening, ...]


def make_original(x: int, w: int, b: int, trace: tuple[int, ...],
                  scope: tuple[str, ...] = SCOPE):
    trees = {"input": Merkle("input", (x,)),
             "model": Merkle("model", (w, b)),
             "trace": Merkle("trace", trace)}
    roots = tuple((name, t.root, len(t.values)) for name, t in sorted(trees.items()))
    claim = digest((scope, PROGRAM, roots))
    return Original(claim, scope, PROGRAM, roots), trees


def make_evidence(original: Original, trees: dict[str, Merkle], relation: str):
    return Evidence(original.claim, original.scope, PROGRAM, relation,
                    tuple(trees[name].open(i) for name, i in NEEDED[relation]))


def judge(original: Original, evidence: Evidence) -> str:
    """Only native expected metadata and the bounded package, never full trace."""
    if (evidence.claim != original.claim or evidence.scope != original.scope
            or evidence.program != original.program or original.program != PROGRAM
            or evidence.relation not in NEEDED):
        return "EvidenceRejected"
    keys = tuple((o.tree, o.index) for o in evidence.openings)
    if keys != NEEDED[evidence.relation]:
        return "EvidenceRejected"
    if len(evidence.openings) > 3 or sum(len(o.siblings) for o in evidence.openings) > 9:
        return "EvidenceRejected"
    # Bounded envelope/leaf bytes are checked before hashing or arithmetic.
    if any(len(json.dumps(o.value).encode()) > 64
           or any(len(s) != 64 for s in o.siblings) for o in evidence.openings):
        return "EvidenceRejected"
    roots = {name: (root, count) for name, root, count in original.roots}
    values = {}
    for opening in evidence.openings:
        if opening.tree not in roots:
            return "EvidenceRejected"
        root, count = roots[opening.tree]
        if not opening_valid(opening, root, count):
            return "EvidenceRejected"
        values[(opening.tree, opening.index)] = opening.value
    return ("LocalPredicateHolds" if evaluate_relation(evidence.relation, values)
            else "ComputationRefuted")


def relation_truths(x: int, w: int, b: int, trace: tuple[int, ...]):
    values = {("input", 0): x, ("model", 0): w, ("model", 1): b}
    values.update({("trace", i): v for i, v in enumerate(trace)})
    return {r: evaluate_relation(r, values) for r in RELATIONS}


@dataclass(frozen=True)
class Policy:
    # Small ordered coordinates preserve RFC boundary relations, not DAA values.
    notice_until: int = 2
    hard: int = 5
    post_notice_until: int = 7
    post_collect_until: int = 8
    k: int = 2
    draw: tuple[str, ...] = ("bad-a", "bad-b", "honest")
    inspection_bound: int = 1


@dataclass
class Case:
    owner: str
    relation: str
    collect_until: int
    stage: str = "DisputePending"
    material_id: str | None = None
    material_ready_at: int | None = None


@dataclass(frozen=True)
class Event:
    kind: str
    actor: str = "honest"
    case: str = "case-1"
    relation: str = "Add"
    evidence: Evidence | None = None
    scope: tuple[str, ...] = SCOPE
    # None is a test-construction shorthand, expanded before the modeled ingress;
    # it is not a missing claim field accepted by a protocol decoder.
    claim: str | None = None
    signed: bool = True
    prerequisites: tuple[bool, ...] = (True,) * 6
    matched_trace_root: str | None = None


@dataclass
class Protocol:
    original: Original
    policy: Policy = field(default_factory=Policy)
    phase: str = "VerificationPending"
    approvals: set[str] = field(default_factory=set)
    q: dict[str, Case] = field(default_factory=dict)
    reservations: dict[str, int] = field(default_factory=dict)
    reservation_keys: set[tuple[str, str]] = field(default_factory=set)
    effects: set[str] = field(default_factory=set)
    refuted: bool = False
    delivery_default: bool = False
    service_failed: bool = False
    profile_supported: bool = True
    local_checked: bool = True
    dependencies_ready: bool = True
    review_ready: bool = True
    economic_ready: bool = True
    credit: int = 0
    credit_issued: bool = False
    reserved_permits: int = 0
    active_schedules: int = 0
    accepted_exec_records: int = 0
    successful_commits: int = 0
    computation_slashes: int = 0
    delivery_slashes: int = 0
    challenger_slashes: int = 0
    rejected_events: int = 0

    def __post_init__(self):
        if not self.reservations:
            self.reservations = dict.fromkeys(self.policy.draw, 1)

    def approval_count(self):
        # Actual draw multiplicity counts; duplicate packets do not add votes.
        return sum(actor in self.approvals for actor in self.policy.draw)

    def case_key(self, actor: str, content: str, mutants=frozenset()) -> str:
        # actor is the canonical authenticated bond, never a relayer/queue name.
        identity = () if "shared_case_identity" in mutants else (actor,)
        return digest((self.original.scope, self.original.claim, identity, content))

    def has_case(self, actor: str, content: str) -> bool:
        return self.case_key(actor, content) in self.q

    def source_rights_usable(self) -> bool:
        # This tests source qualification, separately from an unspent balance.
        return (self.phase == "Final" and not self.refuted and not self.q
                and not self.delivery_default and not self.service_failed
                and self.profile_supported and self.local_checked
                and self.dependencies_ready and self.review_ready and self.economic_ready
                and self.approval_count() >= self.policy.k)

    def consume(self, event: Event, mutants: frozenset[str]):
        if not self.authorize(event, mutants) or not self.source_rights_usable():
            return
        if event.kind == "issue_credit" and not self.credit_issued:
            self.credit_issued = True
            self.credit = 1
        elif event.kind == "reserve_permit" and self.credit and not self.reserved_permits:
            self.reserved_permits = 1
        elif event.kind == "activate_schedule" and self.credit and self.reserved_permits:
            self.active_schedules = 1
        elif event.kind == "consume_permit" and self.credit and self.reserved_permits and self.active_schedules:
            self.credit = self.reserved_permits = self.active_schedules = 0
            self.accepted_exec_records += 1

    def authorize(self, event: Event, mutants: frozenset[str]) -> bool:
        return (event.signed and event.claim == self.original.claim
                and ("ignore_scope" in mutants or event.scope == self.original.scope))

    def reserve(self, actor: str, case: str, mutants: frozenset[str]) -> bool:
        key = (actor, case)
        if key in self.reservation_keys:
            return True
        if actor not in self.reservations:
            return False
        if "pooled_reservations" in mutants:
            donor = next((a for a, left in self.reservations.items() if left), None)
            if donor is None:
                return False
            self.reservations[donor] -= 1
        else:
            if not self.reservations[actor]:
                return False
            self.reservations[actor] -= 1
        self.reservation_keys.add(key)
        return True

    def notice(self, event: Event, now: int, mutants: frozenset[str]):
        ordinary = now <= self.policy.notice_until and self.phase not in {
            "Final", "Refuted", "Revoked", "AbortedUnverified"}
        post_final = (self.phase == "Final" and now <= self.policy.post_notice_until)
        if not self.authorize(event, mutants) or not (ordinary or post_final):
            self.rejected_events += 1
            return
        key = self.case_key(event.actor, event.case, mutants)
        if key in self.q:
            case = self.q[key]
            if (case.owner, case.relation) != (event.actor, event.relation):
                self.rejected_events += 1
            return
        if not self.reserve(event.actor, event.case, mutants):
            self.rejected_events += 1
            return
        until = self.policy.post_collect_until if post_final else self.policy.hard
        material = event.kind == "material_request"
        self.q[key] = Case(event.actor, event.relation, until,
                           "MaterialPending" if material else "DisputePending",
                           digest((self.original.claim, event.case)) if material else None)

    def apply_proof(self, event: Event, now: int, mutants: frozenset[str]):
        if not self.authorize(event, mutants) or event.evidence is None:
            self.rejected_events += 1
            return
        key = self.case_key(event.actor, event.case, mutants)
        if key not in self.q:
            # A completed direct package may create its notice within a legal window.
            self.notice(replace(event, kind="notice", relation=event.evidence.relation), now, mutants)
        case = self.q.get(key)
        if (case is None or now > case.collect_until
                or case.owner != event.actor):
            self.rejected_events += 1
            return
        if "timeout_first" in mutants and now >= self.policy.hard:
            self.rejected_events += 1
            return
        verdict = judge(self.original, event.evidence)
        if "raw_schema_is_packet_error" in mutants and event.evidence.relation == "WellFormedness":
            verdict = "EvidenceRejected"
        if case.stage == "DisputePending" and case.relation != event.evidence.relation:
            self.rejected_events += 1
            return
        if case.stage in {"MaterialPending", "InspectionPending"}:
            # A real contradiction may finish the existing inquiry directly.
            # A locally true fragment does not establish completed whole-claim inspection.
            if verdict != "ComputationRefuted":
                self.rejected_events += 1
                return
            case.stage = "DisputePending"
            case.relation = event.evidence.relation
        if verdict == "ComputationRefuted":
            if "quorum_overrides_proof" in mutants and self.approval_count() >= self.policy.k:
                del self.q[key]
                return
            effect = digest((self.original.scope, self.original.claim, "computation-offence"))
            if effect not in self.effects:
                self.effects.add(effect)
                self.computation_slashes += 1
            self.refuted = True
            self.credit = 0
            self.reserved_permits = self.active_schedules = 0
            if self.phase == "Final":
                self.phase = "Revoked"
            elif self.phase != "AbortedUnverified":
                self.phase = "Refuted"
            del self.q[key]
        elif verdict == "LocalPredicateHolds":
            if "clear_all_cases" in mutants:
                self.q.clear()
            else:
                del self.q[key]
        else:
            self.rejected_events += 1
            if "packet_rejection_closes_case" in mutants:
                self.q.pop(key, None)

    def finalize(self, now: int, mutants: frozenset[str]):
        terminal = self.phase in {"Final", "Refuted", "Revoked", "AbortedUnverified"}
        after_notice = (now >= self.policy.notice_until if "early_final" in mutants
                        else now > self.policy.notice_until)
        clear = not self.q or "ignore_pending_cases" in mutants
        if (not terminal and after_notice and now <= self.policy.hard and clear
                and not self.refuted and not self.delivery_default and not self.service_failed
                and self.profile_supported and self.local_checked and self.dependencies_ready
                and self.review_ready and self.economic_ready
                and self.approval_count() >= self.policy.k):
            self.phase = "Final"
            self.credit = 1
            self.credit_issued = True
            self.successful_commits += 1

    def batch(self, now: int, events: tuple[Event, ...], mutants=frozenset()):
        """Canonical phases, not transaction-order-dependent Final writes."""
        mutants = frozenset(mutants)
        events = tuple(replace(e, claim=self.original.claim) if e.claim is None else e
                       for e in events)
        consumers = tuple(e for e in events if e.kind in {
            "issue_credit", "reserve_permit", "activate_schedule", "consume_permit"})
        if "consume_before_revocation" in mutants:
            for event in consumers:
                self.consume(event, mutants)
        if "timeout_first" in mutants and now >= self.policy.hard and self.phase != "Final":
            self.phase = "AbortedUnverified"
        for event in events:
            if event.kind == "approval":
                if self.authorize(event, mutants) and event.actor in self.policy.draw:
                    self.approvals.add(event.actor)
                else:
                    self.rejected_events += 1
        for event in events:
            if event.kind in {"notice", "material_request"}:
                self.notice(event, now, mutants)
        for event in events:
            key = self.case_key(event.actor, event.case, mutants)
            case = self.q.get(key)
            if event.kind == "material_delivered" and self.authorize(event, mutants) and case:
                if case.stage == "MaterialPending" and all(event.prerequisites):
                    case.stage = "InspectionPending"
                    case.material_ready_at = now
                    if "delivery_closes_inspection" in mutants:
                        del self.q[key]
                    elif now + self.policy.inspection_bound > case.collect_until:
                        self.service_failed = True
            elif event.kind == "inspection_mismatch" and self.authorize(event, mutants) and case:
                if case.stage == "InspectionPending" and now <= case.collect_until:
                    case.stage = "DisputePending"
                    case.relation = event.relation
            elif event.kind == "inspection_match" and self.authorize(event, mutants) and case:
                expected_trace = next(root for name, root, _ in self.original.roots if name == "trace")
                if (case.stage == "InspectionPending" and now <= case.collect_until
                        and event.matched_trace_root == expected_trace and not self.service_failed):
                    # This is a modeled claimant-signed full replay attestation,
                    # not evidence that this person physically executed the program.
                    del self.q[key]
        # Refutations take precedence over holds even if their packet order differs.
        proofs = [e for e in events if e.kind == "proof"]
        proofs.sort(key=lambda e: (judge(self.original, e.evidence) != "ComputationRefuted"
                                   if e.evidence is not None else True, e.case))
        for event in proofs:
            self.apply_proof(event, now, mutants)
        for event in events:
            key = self.case_key(event.actor, event.case, mutants)
            if event.kind == "delivery_default":
                if (self.authorize(event, mutants) and len(event.prerequisites) == 6
                        and all(event.prerequisites) and key in self.q
                        and self.q[key].stage == "MaterialPending"):
                    self.delivery_default = True
                    effect = digest((self.original.scope, self.original.claim,
                                     key, "delivery-offence"))
                    if effect not in self.effects:
                        self.effects.add(effect)
                        self.delivery_slashes += 1
                else:
                    self.rejected_events += 1
            elif event.kind == "challenger_default":
                if (self.authorize(event, mutants) and all(event.prerequisites) and key in self.q
                        and self.q[key].owner == event.actor):
                    effect = digest((self.original.scope, self.original.claim, key, "challenger-default"))
                    if effect not in self.effects:
                        self.effects.add(effect)
                        self.challenger_slashes += 1
                    del self.q[key]
                else:
                    self.rejected_events += 1
            elif event.kind == "service_failure":
                if self.authorize(event, mutants):
                    self.service_failed = True
            elif event.kind == "dependency_invalidated":
                if self.authorize(event, mutants):
                    self.dependencies_ready = False
        if now > self.policy.hard and self.phase not in {"Final", "Refuted", "Revoked"}:
            self.phase = "AbortedUnverified"
            self.credit = 0
        if any(e.kind == "final" for e in events):
            self.finalize(now, mutants)
        if "consume_before_revocation" not in mutants:
            # All consumers read post-proof, post-Q and post-dependency state.
            for event in consumers:
                self.consume(event, mutants)
        return self


class Checks:
    def __init__(self):
        self.results = []
        self.failures = []

    def run(self, name, function):
        try:
            detail = function()
            self.results.append({"id": name, "status": "PASS", "detail": detail})
        except Exception as exc:
            failure = {"id": name, "status": "FAIL", "error": f"{type(exc).__name__}: {exc}"}
            self.results.append(failure)
            self.failures.append(failure)


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def check_trace_space():
    numeric = metadata = valid = invalid = 0
    witness_types = set()
    max_records = max_siblings = 0
    params = tuple(itertools.product((-1, 0, 1), repeat=3))
    meta_domains = ((0, 1), (0, 1), (0, 1), (0, 1, 2), (0, 1, 2), (0, 1), (1, 2, 3), (0, 1, 2))
    for x, w, b in params:
        expected = canonical_trace(x, w, b)
        spaces = (("numeric", (tuple(a) + expected[4:]
                               for a in itertools.product(range(-2, 3), repeat=4))),
                  ("metadata", (expected[:4] + tuple(a)
                                for a in itertools.product(*meta_domains))))
        for kind, traces in spaces:
            for trace in traces:
                numeric += kind == "numeric"
                metadata += kind == "metadata"
                truths = relation_truths(x, w, b, trace)
                semantic = trace == expected
                require(all(truths.values()) == semantic,
                        f"relation coverage differs from direct execution: {trace}")
                original, trees = make_original(x, w, b, trace)
                if semantic:
                    valid += 1
                    require(all(judge(original, make_evidence(original, trees, r))
                                == "LocalPredicateHolds" for r in RELATIONS),
                            "valid trace convicted")
                else:
                    invalid += 1
                    relation = next(r for r in RELATIONS if not truths[r])
                    proof = make_evidence(original, trees, relation)
                    require(judge(original, proof) == "ComputationRefuted",
                            f"false trace lacks a bounded witness: {relation}")
                    witness_types.add(relation)
                    max_records = max(max_records, len(proof.openings))
                    max_siblings = max(max_siblings, sum(len(o.siblings) for o in proof.openings))
    require(witness_types == set(RELATIONS), "some relation kind was never falsified")
    return {"numeric_trace_cases": numeric, "metadata_trace_cases": metadata,
            "valid_cases": valid, "false_cases_with_local_witness": invalid,
            "relation_kinds": sorted(witness_types), "max_witness_records": max_records,
            "max_witness_sibling_hashes": max_siblings,
            "enumeration": "all numeric traces and, separately, all listed metadata combinations"}


def fixture(bad=True):
    trace = list(canonical_trace(1, 1, 1))
    if bad:
        trace[2] = trace[3] = 1  # The add is false; subsequent output is consistent.
    original, trees = make_original(1, 1, 1, tuple(trace))
    return Protocol(original), trees


def approval_events():
    return (Event("approval", actor="bad-a"), Event("approval", actor="bad-b"))


def check_authentication():
    state, trees = fixture()
    proof = make_evidence(state.original, trees, "Add")
    first = proof.openings[0]
    variants = {
        "different_claim": replace(proof, claim="another-claim"),
        "different_scope": replace(proof, scope=SCOPE[:-1] + ("branch-B",)),
        "different_program": replace(proof, program=digest("another-program")),
        "different_coordinate": replace(proof, openings=(replace(first, index=0),) + proof.openings[1:]),
        "wrong_source_with_valid_membership": replace(proof, openings=(trees["model"].open(0),) + proof.openings[1:]),
        "changed_value": replace(proof, openings=(replace(first, value=first.value + 1),) + proof.openings[1:]),
        "changed_path": replace(proof, openings=(replace(first, siblings=("0" * 64,)),) + proof.openings[1:]),
        "missing_operand": replace(proof, openings=proof.openings[:-1]),
        "duplicated_operand": replace(proof, openings=proof.openings + proof.openings[:1]),
        "unregistered_relation": replace(proof, relation="ArbitraryClamp"),
    }
    for name, candidate in variants.items():
        require(judge(state.original, candidate) == "EvidenceRejected", name)
    return {"rejected_variants": sorted(variants)}


def check_batch_permutations():
    outcomes = set()
    count = 0
    state, trees = fixture()
    proof = make_evidence(state.original, trees, "Add")
    events = approval_events() + (Event("notice"), Event("proof", evidence=proof), Event("final"))
    for order in itertools.permutations(events):
        copy_state, _ = fixture()
        copy_state.batch(copy_state.policy.notice_until, order)
        require(copy_state.phase == "Refuted" and copy_state.credit == 0, "batch ordering bypass")
        require(copy_state.computation_slashes == 1, "refutation effect count")
        outcomes.add((copy_state.phase, copy_state.credit, copy_state.computation_slashes))
        count += 1
    return {"permutations": count, "distinct_material_outcomes": len(outcomes)}


def check_deadlines():
    checked = 0
    for notice_at in range(0, 4):
        state, trees = fixture()
        state.batch(notice_at, approval_events() + (Event("notice"),))
        expected = notice_at <= state.policy.notice_until
        require(bool(state.q) == expected, f"notice endpoint {notice_at}")
        checked += 1
    for final_at in range(0, 7):
        state, _ = fixture(False)
        state.batch(final_at, approval_events() + (Event("final"),))
        expected = state.policy.notice_until < final_at <= state.policy.hard
        require((state.phase == "Final") == expected, f"Final endpoint {final_at}")
        checked += 1
    state, trees = fixture()
    state.batch(2, approval_events() + (Event("notice"),))
    state.batch(5, (Event("final"), Event("proof", evidence=make_evidence(state.original, trees, "Add"))))
    require(state.phase == "Refuted", "hard-day proof was lost to timeout")
    state, _ = fixture()
    state.batch(2, approval_events() + (Event("notice"),))
    state.batch(5, (Event("final"),))
    require(state.phase != "Final", "unresolved case passed on hard day")
    state.batch(6, (Event("final"),))
    require(state.phase == "AbortedUnverified" and state.computation_slashes == 0,
            "hard+1 must abort without inferred computation guilt")
    state.batch(7, approval_events() + (Event("final"),))
    require(state.phase == "AbortedUnverified", "terminal resurrected")
    # Two canonical units at the same DAA endpoint cannot finalize before notice.
    state, _ = fixture()
    state.batch(2, approval_events() + (Event("final"),))
    state.batch(2, (Event("notice"), Event("final")))
    require(state.phase != "Final" and state.q, "same-DAA delayed notice suppressed")
    return {"boundary_cases": checked + 5, "coordinates": {"notice": 2, "hard": 5,
            "hard_plus_one": 6}, "note": "toy ordered coordinates, not production DAA parameters"}


def check_case_isolation():
    state, trees = fixture()
    state.batch(2, approval_events() + (
        Event("notice", actor="bad-a", case="false-case", relation="Init"),
        Event("notice", case="real-case", relation="Add")))
    holds = make_evidence(state.original, trees, "Init")
    state.batch(3, (Event("proof", actor="bad-a", case="false-case", evidence=holds), Event("final")))
    real_key = state.case_key("honest", "real-case")
    require(set(state.q) == {real_key} and state.phase != "Final", "false case cleared another case")
    # Even an authenticated hold cannot close an unrelated fixed dispute.
    state.batch(3, (Event("proof", case="real-case", evidence=holds), Event("final")))
    require(set(state.q) == {real_key}, "unrelated LocalPredicateHolds closed real case")
    bad_auth = replace(make_evidence(state.original, trees, "Add"), program="wrong")
    state.batch(3, (Event("proof", case="real-case", evidence=bad_auth),))
    require(set(state.q) == {real_key} and state.computation_slashes == 0,
            "invalid evidence convicted or erased a live case")
    return {"case_scoping_scenarios": 3}


def check_failure_classification():
    scenarios = 0
    for missing in range(6):
        state, _ = fixture(False)
        state.batch(2, approval_events() + (Event("material_request", relation="material"),))
        prerequisites = tuple(i != missing for i in range(6))
        state.batch(3, (Event("delivery_default", prerequisites=prerequisites), Event("final")))
        require(not state.delivery_default and state.delivery_slashes == 0
                and state.computation_slashes == 0, "default without all native prerequisites")
        scenarios += 1
    for event in (Event("delivery_default"), Event("service_failure")):
        state, _ = fixture(False)
        state.batch(2, approval_events() + (Event("material_request", relation="material"),))
        state.batch(3, (event, Event("final")))
        require(state.phase != "Final" and state.computation_slashes == 0,
                "delivery/service failure became success or computation guilt")
        state.batch(6, (Event("final"),))
        require(state.phase == "AbortedUnverified" and state.computation_slashes == 0,
                "finite failure classification")
        scenarios += 1
    return {"scenarios": scenarios, "delivery_default_uses": "six modeled native prerequisites"}


def check_once_and_revocation():
    state, trees = fixture()
    proof = make_evidence(state.original, trees, "Add")
    state.batch(2, approval_events() + (Event("proof", evidence=proof), Event("proof", evidence=proof)))
    state.batch(2, (Event("proof", evidence=proof), Event("final")))
    state = copy.deepcopy(state)  # A preserved abstract snapshot, not an IBD implementation.
    state.batch(3, approval_events() + (Event("final"),))
    require(state.computation_slashes == 1 and state.phase == "Refuted" and state.credit == 0,
            "duplicate refutation or restart resurrected claim")
    state, trees = fixture()
    state.batch(3, approval_events() + (Event("final"), Event("final")))
    require(state.phase == "Final" and state.successful_commits == 1, "success effect duplication")
    state.batch(7, (Event("notice", case="post-final"),))
    proof = make_evidence(state.original, trees, "Add")
    state.batch(8, (Event("proof", case="post-final", evidence=proof), Event("final")))
    require(state.phase == "Revoked" and state.credit == 0 and state.computation_slashes == 1,
            "explicit liability-window proof did not revoke")
    late, late_trees = fixture()
    late.batch(3, approval_events() + (Event("final"),))
    late.batch(8, (Event("proof", case="new-too-late",
                        evidence=make_evidence(late.original, late_trees, "Add")),))
    require(late.phase == "Final" and late.computation_slashes == 0,
            "material retention invented a new notice right")
    return {"scenarios": 4, "post_final_rights": "explicit toy notice<=7 and collection<=8"}


def check_protected_reservations():
    counts = 0
    for attempts in itertools.product(range(4), repeat=2):
        state, _ = fixture()
        for actor, number in zip(("bad-a", "bad-b"), attempts):
            for i in range(number):
                state.batch(1, (Event("notice", actor=actor, case=f"{actor}-{i}"),))
        state.batch(2, (Event("notice", case="honest-case"),))
        require(state.has_case("honest", "honest-case"), f"other owners consumed honest reservation {attempts}")
        require(len(state.q) <= 3, "aliases regenerated original claim budget")
        counts += 1
    state, _ = fixture()
    state.policy = replace(state.policy, draw=("bad-a", "bad-a", "honest"))
    state.reservations = dict.fromkeys(state.policy.draw, 1)
    require(len(state.reservations) == 2, "duplicate draw position duplicated budget")
    state.batch(1, (Event("approval", actor="bad-a"), Event("approval", actor="bad-a")))
    require(state.approval_count() == 2, "draw multiplicity is not packet multiplicity")
    return {"adversarial_demand_cases": counts, "duplicate_draw_case": 1}


def check_final_gate_space():
    cases = 0
    for votes in range(3):
        for supported, local, deps, review, economic, service, default, pending in itertools.product((False, True), repeat=8):
            state, _ = fixture(False)
            state.profile_supported, state.local_checked = supported, local
            state.dependencies_ready, state.review_ready = deps, review
            state.economic_ready, state.service_failed = economic, service
            state.delivery_default = default
            if pending:
                state.q["pending"] = Case("honest", "Add", 5)
            state.approvals = set(("bad-a", "bad-b")[:votes])
            state.batch(3, (Event("final"),))
            expected = all((votes == 2, supported, local, deps, review, economic,
                            not service, not default, not pending))
            require((state.phase == "Final") == expected, "Final gate conjunction bypass")
            require(state.credit == int(expected), "credit differs from successful transition")
            cases += 1
    for count in range(2):
        state, _ = fixture(False)
        state.approvals = set(("bad-a",)[:count])
        state.batch(6, (Event("final"),))
        require(state.phase == "AbortedUnverified" and state.credit == 0, "no approval became success")
    return {"gate_combinations": cases, "insufficient_approval_timeouts": 2}


def check_stale_scopes():
    rejected = 0
    for i in range(len(SCOPE)):
        stale = list(SCOPE)
        stale[i] += "-stale"
        state, _ = fixture()
        state.batch(1, (Event("approval", actor="bad-a", scope=tuple(stale)),
                        Event("notice", scope=tuple(stale))))
        require(not state.approvals and not state.q, f"stale scope dimension {i}")
        rejected += 2
    state, _ = fixture()
    state.batch(1, (Event("approval", actor="bad-a", signed=False), Event("notice", signed=False)))
    require(not state.approvals and not state.q, "unsigned authority")
    state.batch(1, (Event("approval", actor="bad-a", claim="another-claim"),
                    Event("notice", claim="another-claim")))
    require(not state.approvals and not state.q, "another claim's signature or notice")
    return {"scope_dimension_rejections": rejected, "unsigned_rejections": 2,
            "different_claim_rejections": 2}


def check_material_inspection_continuity():
    state, trees = fixture()
    request = Event("material_request", case="needed-range", relation="material")
    state.batch(2, approval_events() + (request,))
    key = state.case_key("honest", "needed-range")
    initial_until = state.q[key].collect_until
    state.batch(3, (Event("material_delivered", case="needed-range"), Event("final")))
    require(key in state.q and state.q[key].stage == "InspectionPending" and state.phase != "Final",
            "delivery acknowledgement erased pending independent inspection")
    require(state.q[key].collect_until == initial_until, "delivery reset original case clock")
    # A locally correct fragment is insufficient to finish the full inspection.
    state.batch(3, (Event("proof", case="needed-range",
                         evidence=make_evidence(state.original, trees, "Init")), Event("final")))
    require(key in state.q and state.q[key].stage == "InspectionPending",
            "a local hold falsely completed whole-claim inspection")
    state.batch(4, (Event("inspection_mismatch", case="needed-range", relation="Add"),))
    require(key in state.q and state.q[key].stage == "DisputePending", "old case did not continue after notice window")
    require(state.q[key].collect_until == initial_until, "mismatch created fresh clock")
    state.batch(5, (Event("proof", case="needed-range",
                         evidence=make_evidence(state.original, trees, "Add")),))
    require(state.phase == "Refuted", "timely continuation failed to refute")
    direct, trees = fixture()
    direct.batch(2, approval_events() + (request,))
    direct.batch(3, (Event("material_delivered", case="needed-range"),))
    direct.batch(4, (Event("proof", case="needed-range",
                           evidence=make_evidence(direct.original, trees, "Add")),))
    require(direct.phase == "Refuted", "direct refutation could not finish old inspection case")
    late, _ = fixture()
    late.batch(2, approval_events() + (request,))
    late.batch(5, (Event("material_delivered", case="needed-range"), Event("final")))
    require(late.phase != "Final" and late.service_failed and late.q,
            "late delivery used a nonexistent remaining inspection opportunity")
    late.batch(6, (Event("final"),))
    require(late.phase == "AbortedUnverified" and late.computation_slashes == 0,
            "late material must end unverified, not infer arithmetic guilt")
    normal, normal_trees = fixture(False)
    normal.batch(2, approval_events() + (request,))
    normal.batch(3, (Event("material_delivered", case="needed-range"),))
    normal.batch(4, (Event("inspection_match", case="needed-range",
                          matched_trace_root=normal_trees["trace"].root), Event("final")))
    require(normal.phase == "Final" and not normal.q,
            "valid completed inspection has no permitted success route")
    return {"scenarios": 6, "same_case_clock": initial_until,
            "result": "delivery keeps Q; old inspection can continue after initial notice window"}


def check_claimant_case_identity():
    state, trees = fixture()
    events = (Event("material_request", actor="bad-a", case="same-range", relation="material"),
              Event("material_request", actor="honest", case="same-range", relation="material"))
    state.batch(2, approval_events() + events)
    bad_key, honest_key = state.case_key("bad-a", "same-range"), state.case_key("honest", "same-range")
    require(bad_key != honest_key and set(state.q) == {bad_key, honest_key},
            "first claimant captured another claimant's case")
    require(state.q[bad_key].material_id == state.q[honest_key].material_id,
            "shared bytes should keep a reusable material identity")
    state.batch(3, (Event("challenger_default", actor="bad-a", case="same-range"), Event("final")))
    require(set(state.q) == {honest_key} and state.phase != "Final" and state.computation_slashes == 0,
            "another claimant's default erased the honest inquiry")
    state.batch(3, (Event("material_delivered", case="same-range"),))
    state.batch(4, (Event("proof", case="same-range",
                         evidence=make_evidence(state.original, trees, "Add")),))
    require(state.phase == "Refuted", "independent honest case lost continuation right")
    return {"separate_case_ids": 2, "shared_material_ids": 1,
            "default_closes_only_defaulting_claimant": True}


def check_authenticated_raw_schema():
    checked = 0
    for raw_tag in (0, 2, "invalid-schema"):
        trace = canonical_trace(1, 1, 1)[:-1] + (raw_tag,)
        original, trees = make_original(1, 1, 1, trace)
        proof = make_evidence(original, trees, "WellFormedness")
        require(judge(original, proof) == "ComputationRefuted",
                "authenticated bounded malformed Miner record was treated only as packet rejection")
        checked += 1
    original, trees = make_original(1, 1, 1, canonical_trace(1, 1, 1)[:-1] + (0,))
    proof = make_evidence(original, trees, "WellFormedness")
    require(judge(original, replace(proof, openings=())) == "EvidenceRejected", "missing envelope")
    unopenable = replace(original, roots=tuple((name, "0" * 64 if name == "trace" else root, count)
                                               for name, root, count in original.roots))
    require(judge(unopenable, proof) == "EvidenceRejected", "unopenable commitment was convicted")
    huge_original, huge_trees = make_original(1, 1, 1, canonical_trace(1, 1, 1)[:-1] + ("x" * 65,))
    huge_proof = make_evidence(huge_original, huge_trees, "WellFormedness")
    require(judge(huge_original, huge_proof) == "EvidenceRejected", "oversized material fabricated conviction")
    return {"authenticated_schema_refutations": checked,
            "envelope_unopenable_oversized_rejections": 3,
            "scope": "bounded toy raw schema tag, not a real PALW codec implementation"}


def check_consumer_post_batch_state():
    cases = 0
    for consumer_kind in ("issue_credit", "reserve_permit", "activate_schedule", "consume_permit"):
        for order in (False, True):
            state, trees = fixture()
            state.batch(3, approval_events() + (Event("final"),))
            if consumer_kind == "issue_credit":
                # A successful source whose separately gated credit has not yet issued.
                state.credit, state.credit_issued = 0, False
            if consumer_kind in {"activate_schedule", "consume_permit"}:
                state.reserved_permits = 1
            if consumer_kind == "consume_permit":
                state.active_schedules = 1
            proof_event = Event("proof", case="late-proof",
                                evidence=make_evidence(state.original, trees, "Add"))
            consumer = Event(consumer_kind)
            events = (consumer, proof_event) if order else (proof_event, consumer)
            state.batch(4, events)
            require(state.phase == "Revoked" and state.credit == 0
                    and state.reserved_permits == 0 and state.active_schedules == 0
                    and state.accepted_exec_records == 0,
                    f"same-batch {consumer_kind} escaped revocation order")
            cases += 1
    for blocker in (Event("notice", case="pending"), Event("dependency_invalidated")):
        state, _ = fixture()
        state.batch(3, approval_events() + (Event("final"), Event("reserve_permit"), Event("activate_schedule")))
        state.batch(4, (Event("consume_permit"), blocker))
        require(not state.source_rights_usable() and state.accepted_exec_records == 0,
                "pending inquiry/dependency invalidation did not hold unused rights")
        cases += 1
    previous, trees = fixture()
    previous.batch(3, approval_events() + (Event("final"), Event("reserve_permit"),
                    Event("activate_schedule"), Event("consume_permit")))
    require(previous.accepted_exec_records == 1, "previous canonical execution fixture")
    previous.batch(4, (Event("proof", case="post-consumption",
                             evidence=make_evidence(previous.original, trees, "Add")),))
    require(previous.phase == "Revoked" and previous.accepted_exec_records == 1
            and previous.credit == 0, "revocation alone rewound previously canonical transactions")
    return {"consumer_and_order_cases": cases, "previous_batch_consumption_preserved": True,
            "modeled_consumers": ["issuance", "permit reservation", "schedule activation", "permit consumption"],
            "scope": "one abstract entitlement token; not a real transaction or schedule engine"}


def expected_limitations():
    cases = []
    for reason in ("all selected verifiers collude; no public challenger participates",
                   "the only honest verifier's entire communication is censored before canonical inclusion"):
        state, trees = fixture()
        require(judge(state.original, make_evidence(state.original, trees, "Add"))
                == "ComputationRefuted", "fixture is not actually false")
        state.batch(3, approval_events() + (Event("final"),))
        require(state.phase == "Final", "model incorrectly claims unconditional safety")
        cases.append({"assumption_failure": reason, "observed_abstract_result": "Final",
                      "classification": "EXPECTED_OUTSIDE_GUARANTEE_COUNTEREXAMPLE"})
    return cases


def check_mutants():
    killed = []
    # Each mutant has a concrete security counterexample.  Detection is asserted;
    # these are not alternative protocol implementations or desirable outcomes.
    for mutant in ("ignore_pending_cases", "early_final", "quorum_overrides_proof",
                   "timeout_first", "clear_all_cases", "pooled_reservations", "ignore_scope",
                   "packet_rejection_closes_case", "delivery_closes_inspection",
                   "shared_case_identity", "raw_schema_is_packet_error", "consume_before_revocation"):
        state, trees = fixture()
        muts = frozenset((mutant,))
        if mutant == "ignore_pending_cases":
            state.batch(2, approval_events() + (Event("notice"),), muts)
            state.batch(3, (Event("final"),), muts)
            exposed = state.phase == "Final" and bool(state.q)
        elif mutant == "early_final":
            state.batch(2, approval_events() + (Event("final"),), muts)
            exposed = state.phase == "Final"
        elif mutant == "quorum_overrides_proof":
            state.batch(2, approval_events() + (Event("notice"),), muts)
            state.batch(3, (Event("proof", evidence=make_evidence(state.original, trees, "Add")),
                            Event("final")), muts)
            exposed = state.phase == "Final" and not state.refuted
        elif mutant == "timeout_first":
            state.batch(2, (Event("notice"),), muts)
            state.batch(5, (Event("proof", evidence=make_evidence(state.original, trees, "Add")),), muts)
            exposed = state.phase == "AbortedUnverified" and not state.refuted
        elif mutant == "clear_all_cases":
            state.batch(2, approval_events() + (Event("notice", case="real"),
                        Event("notice", actor="bad-a", case="false", relation="Init")), muts)
            state.batch(3, (Event("proof", actor="bad-a", case="false",
                              evidence=make_evidence(state.original, trees, "Init")), Event("final")), muts)
            exposed = state.phase == "Final" and not state.q
        elif mutant == "pooled_reservations":
            for i in range(3):
                state.batch(1, (Event("notice", actor="bad-a", case=f"bad-{i}"),), muts)
            state.batch(2, (Event("notice", case="honest-case"),), muts)
            exposed = not state.has_case("honest", "honest-case")
        elif mutant == "ignore_scope":
            stale = SCOPE[:-1] + ("branch-old",)
            state.batch(3, tuple(replace(e, scope=stale) for e in approval_events()) + (Event("final"),), muts)
            exposed = state.phase == "Final"
        elif mutant == "packet_rejection_closes_case":
            state.batch(2, approval_events() + (Event("notice"),), muts)
            invalid = replace(make_evidence(state.original, trees, "Add"), program="wrong")
            state.batch(3, (Event("proof", evidence=invalid), Event("final")), muts)
            exposed = state.phase == "Final" and not state.refuted
        elif mutant == "delivery_closes_inspection":
            state.batch(2, approval_events() + (Event("material_request", relation="material"),), muts)
            state.batch(3, (Event("material_delivered"), Event("final")), muts)
            exposed = state.phase == "Final"
        elif mutant == "shared_case_identity":
            state.batch(2, approval_events() + (
                Event("material_request", actor="bad-a", case="shared", relation="material"),
                Event("material_request", actor="honest", case="shared", relation="material")), muts)
            state.batch(3, (Event("challenger_default", actor="bad-a", case="shared"), Event("final")), muts)
            exposed = state.phase == "Final" and not state.q
        elif mutant == "raw_schema_is_packet_error":
            original, raw_trees = make_original(1, 1, 1, canonical_trace(1, 1, 1)[:-1] + (0,))
            state = Protocol(original)
            state.batch(2, (Event("notice", relation="WellFormedness"),), muts)
            state.batch(3, (Event("proof", evidence=make_evidence(original, raw_trees, "WellFormedness")),), muts)
            exposed = not state.refuted and bool(state.q)
        else:
            state.batch(3, approval_events() + (Event("final"), Event("reserve_permit"),
                        Event("activate_schedule")))
            state.batch(4, (Event("consume_permit"), Event("proof", case="same-batch-proof",
                            evidence=make_evidence(state.original, trees, "Add"))), muts)
            exposed = state.phase == "Revoked" and state.accepted_exec_records == 1
        require(exposed, f"mutant was not exposed: {mutant}")
        killed.append(mutant)
    trace = list(canonical_trace(1, 1, 1))
    trace[3] = -1
    truths = relation_truths(1, 1, 1, tuple(trace))
    require(all(value for name, value in truths.items() if not name.startswith("End"))
            and tuple(trace) != canonical_trace(1, 1, 1), "End-omission mutant lacks counterexample")
    killed.append("omit_End_relations")
    trace = list(canonical_trace(0, 0, 0))
    trace[4] = 1  # Numeric coincidence cannot justify a wrong read-from edge.
    truths = relation_truths(0, 0, 0, tuple(trace))
    require(all(value for name, value in truths.items() if name != "ReadMul")
            and tuple(trace) != canonical_trace(0, 0, 0), "read-from mutant lacks counterexample")
    killed.append("omit_read_from_relation")
    return {"mutants_detected": killed, "count": len(killed)}


def source_hashes():
    # parents[3] is docs for this fixed revision directory layout.
    docs = Path(__file__).resolve().parents[3]
    names = ("0016-miner-execution-commit-and-material-supply.md",
             "0018-interactive-negative-court-and-defaults.md",
             "0019-model-free-node-and-bounded-evidence.md",
             "0020-verification-gated-final-and-adversarial-models.md")
    return {f"docs/rfc-v2/{name}": hashlib.sha256((docs / "rfc-v2" / name).read_bytes()).hexdigest()
            for name in names}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="print the complete JSON report")
    parser.add_argument("--output", type=Path, default=Path(__file__).with_name("model-results.json"))
    args = parser.parse_args()
    checks = Checks()
    functions = (
        ("M01-finite-trace-local-completeness", check_trace_space),
        ("M02-authentication-and-binding", check_authentication),
        ("M03-batch-order-permutations", check_batch_permutations),
        ("M04-notice-hard-and-final-boundaries", check_deadlines),
        ("M05-case-isolation", check_case_isolation),
        ("M06-failure-classification", check_failure_classification),
        ("M07-once-effects-and-revocation", check_once_and_revocation),
        ("M08-protected-reservations", check_protected_reservations),
        ("M09-final-gate-enumeration", check_final_gate_space),
        ("M10-stale-authority-scopes", check_stale_scopes),
        ("M11-guarantee-limit-counterexamples", expected_limitations),
        ("M12-mutation-sensitivity", check_mutants),
        ("M13-material-inspection-continuity", check_material_inspection_continuity),
        ("M14-claimant-case-and-shared-material-identity", check_claimant_case_identity),
        ("M15-authenticated-raw-schema-vs-envelope", check_authenticated_raw_schema),
        ("M16-consumers-read-post-batch-state", check_consumer_post_batch_state),
    )
    for name, function in functions:
        checks.run(name, function)
    report = {
        "schema_version": 1,
        "revision": "2026-11-09-palw-execution-integrity",
        "status": "PASS" if not checks.failures else "FAIL",
        "scope": "Finite abstract specification model; not runtime, real-LLM, resource or activation verification.",
        "assumptions": ["signature/draw authority is correctly authenticated",
                        "events provided to batch() are canonically included, not merely sent",
                        "fixed native semantics and hash binding are trusted assumptions",
                        "post-Final rights exist only as explicitly configured in the toy policy"],
        "not_established": ["actual Miner or Panel independent physical execution",
                            "safety without an honest participating and included challenger",
                            "arbitrary LLM local completeness or performance",
                            "production parameter selection, Rust implementation or deployment readiness",
                            "network censorship detection, full localization protocol, reorg or IBD implementation"],
        "source_sha256": source_hashes(),
        "checks": checks.results,
        "summary": {"passed": len(checks.results) - len(checks.failures),
                    "failed": len(checks.failures)},
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    else:
        for result in checks.results:
            print(f"{result['status']} {result['id']}")
        print(f"{report['status']}: {report['summary']['passed']}/{len(checks.results)} finite abstract checks")
        print(f"Report: {args.output}")
    return int(bool(checks.failures))


if __name__ == "__main__":
    raise SystemExit(main())
