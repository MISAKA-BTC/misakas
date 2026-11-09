#!/usr/bin/env python3
"""Finite specification checks for PublicOneHonest; not a production verifier.

This model tests the new state-machine obligations, not their implementation.
An explicit *tiny* complete public transcript is the only modeled objective
inspection prerequisite.  It is not a solution for off-chain LLM availability:
the corresponding large-data profile procedure is NOT ESTABLISHED.  Unknown
procedures are rejected, and no receipt, URL, boolean, small subset, or exhausted
quota is accepted as evidence of a complete inspection opportunity.

The previous revision's 16 checks are imported and run without invoking its
main(), editing its files, or writing its result file.  Their scope is reported
separately.  The old positional Merkle checker is reused for tiny local proofs.

Run: python3 -B check_public_panel_model.py
     python3 -B check_public_panel_model.py --json --output /path/report.json
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import importlib.util
import itertools
import json
import sys
from dataclasses import dataclass, field, replace
from pathlib import Path

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
DOCS = HERE.parents[2]
LEGACY_PATH = HERE.parent / "2026-11-09-palw-execution-integrity/check_protocol_model.py"
spec = importlib.util.spec_from_file_location("palw_preserved_20261109_model", LEGACY_PATH)
assert spec is not None and spec.loader is not None
legacy = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = legacy
spec.loader.exec_module(legacy)

NEW = "PublicOneHonest-toy-domain"
OLD = "LegacyGateC-toy-domain"
SELECTED = ("bond-a", "bond-b", "bond-c")
TOY_PROCEDURE = "tiny-complete-authenticated-public-transcript"
UNESTABLISHED_PROCEDURE = "unestablished-large-offchain-availability"


def digest(value):
    return legacy.digest(value)


def require(condition, explanation):
    if not condition:
        raise AssertionError(explanation)


@dataclass(frozen=True)
class Leaf:
    execution: str
    producer: str
    model: str
    profile: str
    original: object
    prior_admitted: bool = False
    work: int = 2


@dataclass(frozen=True)
class Subject:
    version: str
    leaves: tuple[Leaf, ...]
    sealed_at: int = 0
    draw_at: int = 1

    @property
    def identity(self):
        return digest((self.version, self.sealed_at,
                       tuple((x.execution, x.producer, x.model, x.profile,
                              x.original.claim, x.work) for x in self.leaves)))

    @property
    def coverage(self):
        return tuple(x.execution for x in self.leaves)


@dataclass(frozen=True)
class Material:
    tree: str
    values: tuple

    @property
    def identity(self):
        return (self.tree, legacy.Merkle(self.tree, self.values).root, len(self.values))


@dataclass(frozen=True)
class Approval:
    issuer: str
    subject: str
    version: str
    coverage: tuple[str, ...]
    authenticated: bool = True  # signature verification is an explicit assumption


@dataclass(frozen=True)
class Slot:
    subject: str
    claimant: str
    opens: int = 1
    closes: int = 9


@dataclass(frozen=True)
class InspectionRecord:
    """Derived evidence references; never an externally supplied success bool."""
    subject: str
    claimant: str
    full_scope: tuple
    established_at: int
    inspect_until: int
    include_until: int


@dataclass
class Case:
    identity: str
    owner: str
    kind: str
    leaf: int
    relation: str
    inspect_until: int = 8
    prerequisite: InspectionRecord | None = None
    phase: str = "AwaitingMaterial"
    quota_used: int = 0


@dataclass(frozen=True)
class Event:
    kind: str
    actor: str = "bond-a"
    case: str = ""
    leaf: int = 0
    relation: str = "Add"
    material: Material | None = None
    approval: Approval | None = None
    evidence: object | None = None
    authenticated: bool = True


@dataclass
class Model:
    subject: Subject
    selected: tuple[str, ...] = SELECTED
    procedure: str = TOY_PROCEDURE
    notice_until: int = 4
    hard: int = 10
    max_cases_per_owner: int = 2
    phase: str = "VerificationPending"
    local_positive: bool = True
    dependencies: bool = True
    economics: bool = True
    service: bool = True
    opportunity_schedule: bool = True
    approvals: set[str] = field(default_factory=set)
    cases: dict[str, Case] = field(default_factory=dict)
    materials: dict[tuple, Material] = field(default_factory=dict)
    issued_execution_ids: set[str] = field(default_factory=set)
    credits: int = 0

    def __post_init__(self):
        self.slots = {x: Slot(self.subject.identity, x) for x in set(self.selected)}
        self.admitted = self.valid_subject() and self.procedure == TOY_PROCEDURE
        if not self.admitted:
            self.phase = "UnsupportedProfile"

    def valid_subject(self):
        leaves = self.subject.leaves
        # The tiny procedure has at most three leaves, not a production limit.
        return (self.subject.version in (NEW, OLD)
                and 1 <= len(leaves) <= 3
                and self.subject.sealed_at < self.subject.draw_at
                and len(set(x.execution for x in leaves)) == len(leaves)
                and len(set(x.producer for x in leaves)) == 1
                and len(set(x.model for x in leaves)) == 1
                and len(set(x.profile for x in leaves)) == 1
                and all(not x.prior_admitted for x in leaves)
                and not set(x.execution for x in leaves) & self.issued_execution_ids)

    @property
    def required_material(self):
        return frozenset(root for leaf in self.subject.leaves for root in leaf.original.roots)

    @property
    def q(self):
        return {key for key, c in self.cases.items()
                if c.phase not in ("ClosedCompleted", "ClosedRejected", "ChallengerDefault")}

    def case_id(self, actor, kind, leaf, relation, mutants=frozenset()):
        claimant = "shared" if "merge_claimants" in mutants else actor
        return digest((self.subject.identity, claimant, kind, leaf, relation))

    def open_case(self, now, actor, kind, leaf=0, relation="Add", mutants=frozenset()):
        if (not self.admitted or now > self.notice_until or actor not in self.selected
                or not 0 <= leaf < len(self.subject.leaves)
                or relation not in legacy.RELATIONS):
            return None
        identity = self.case_id(actor, kind, leaf, relation, mutants)
        if identity in self.cases:
            return identity
        if sum(c.owner == actor for c in self.cases.values()) >= self.max_cases_per_owner:
            return None
        self.cases[identity] = Case(identity, actor, kind, leaf, relation)
        return identity

    def count_approval(self, item, mutants=frozenset()):
        if item is None or not item.authenticated or item.issuer not in self.selected:
            return False
        if item.subject != self.subject.identity or item.coverage != self.subject.coverage:
            if ("accept_late_leaf_with_old_approval" not in mutants
                    or self.subject.coverage[:len(item.coverage)] != item.coverage):
                return False
        if item.version != self.subject.version and "ignore_version" not in mutants:
            return False
        self.approvals.add(item.issuer)
        return True

    def publish(self, material):
        if material is None or len(material.values) > 12:
            return False
        if any(len(json.dumps(value).encode()) > 64 for value in material.values):
            return False
        if material.identity not in self.required_material:
            return False
        # The full tiny transcript is at most 41 scalar records for three leaves.
        # This is deliberately not a scalable LLM publication proposal.
        self.materials[material.identity] = material
        return True

    def derive_prerequisite(self, case, now, mutants=frozenset()):
        if self.procedure != TOY_PROCEDURE:
            return None
        full = self.required_material.issubset(self.materials)
        if "partial_is_full" in mutants:
            full = bool(self.materials)
        if "quota_is_delivery" in mutants and case.quota_used >= 2:
            full = True
        slot = self.slots.get(case.owner)
        if not full or slot is None or slot.subject != self.subject.identity:
            return None
        if slot.claimant != case.owner or not self.service or not self.opportunity_schedule:
            return None
        if not (slot.opens <= now and now + 2 <= case.inspect_until
                and case.inspect_until + 1 <= slot.closes <= self.hard):
            return None
        return InspectionRecord(self.subject.identity, case.owner,
                                tuple(sorted(self.required_material)), now,
                                case.inspect_until, slot.closes)

    def can_final(self, now, mutants=frozenset()):
        minimum = 41 if self.subject.version == OLD else 1
        if "all_ack" in mutants:
            minimum = len(set(self.selected))
        window = now > self.notice_until or "early_one_vote" in mutants
        return (self.admitted and self.phase not in ("Refuted", "Revoked", "AbortedUnverified")
                and self.subject.draw_at <= now <= self.hard and window
                and len(self.approvals) >= minimum and self.local_positive
                and self.dependencies and self.economics and self.service
                and self.opportunity_schedule and (not self.q or "ignore_q" in mutants))

    def settle(self):
        if self.phase != "Final":
            return 0
        ids = set(self.subject.coverage)
        if ids & self.issued_execution_ids:
            return 0
        self.issued_execution_ids.update(ids)
        amount = sum(x.work for x in self.subject.leaves)
        self.credits += amount
        return amount

    def batch(self, now, events=(), mutants=frozenset()):
        if not self.admitted:
            return
        # Events denote canonical inclusion. Their textual ordering is immaterial.
        for e in events:
            if e.kind in ("material_request", "mismatch") and e.authenticated:
                self.open_case(now, e.actor, e.kind, e.leaf, e.relation, mutants)
        for e in events:
            if e.kind == "publish":
                self.publish(e.material)
            elif e.kind == "approval":
                self.count_approval(e.approval, mutants)
        for case in self.cases.values():
            if case.identity not in self.q:
                continue
            if self.materials:
                case.phase = "InspectionPending"
            if case.prerequisite is None:
                case.prerequisite = self.derive_prerequisite(case, now, mutants)
            if "delivery_closes_case" in mutants and self.required_material.issubset(self.materials):
                case.phase = "ClosedCompleted"
        for e in events:
            if e.kind != "proof" or not e.authenticated or now > self.hard:
                continue
            case = self.cases.get(e.case)
            if case is None or case.owner != e.actor or case.leaf != e.leaf:
                continue
            if case.identity not in self.q:
                continue
            result = legacy.judge(self.subject.leaves[e.leaf].original, e.evidence)
            if result == "ComputationRefuted":
                self.phase = "Revoked" if self.phase == "Final" else "Refuted"
                if "partial_batch_credit" in mutants:
                    self.credits += sum(x.work for n, x in enumerate(self.subject.leaves) if n != e.leaf)
            elif (result == "LocalPredicateHolds" and e.evidence.relation == case.relation
                  and case.kind == "mismatch"):
                # A tiny fixed relation is the canonical terminal for this toy case.
                case.phase = "ClosedRejected"
            # An invalid packet never resolves an existing legitimate case.
        for e in events:
            if e.kind == "complete" and e.authenticated:
                case = self.cases.get(e.case)
                if case is not None and case.owner == e.actor and case.prerequisite is not None:
                    case.phase = "ClosedCompleted"
        for case in self.cases.values():
            if case.identity not in self.q:
                continue
            if now > case.inspect_until:
                if case.prerequisite is not None:
                    case.phase = "ChallengerDefault"
                else:
                    case.phase = "NoSlashHold"
        if self.phase in ("Refuted", "Revoked"):
            return
        if now > self.hard and self.phase != "Final":
            self.phase = "AbortedUnverified"
        elif self.can_final(now, mutants):
            self.phase = "Final"
            self.settle()


def fixture(count=1, bad_leaf=None, version=NEW, procedure=TOY_PROCEDURE):
    leaves, trees = [], []
    for i in range(count):
        x, w, b = i + 1, 3, 8
        trace = list(legacy.canonical_trace(x, w, b))
        if i == bad_leaf:
            trace[2] += 1
            trace[3] += 1
        original, material = legacy.make_original(x, w, b, tuple(trace),
                                                  ("toy-chain", version, "exec-" + str(i)))
        model_root = dict((n, r) for n, r, _ in original.roots)["model"]
        leaves.append(Leaf("exec-" + str(i), "miner", model_root, "integer-toy", original))
        trees.append(material)
    return Model(Subject(version, tuple(leaves)), procedure=procedure), tuple(trees)


def publication(trees):
    seen = {}
    for group in trees:
        for name, tree in group.items():
            item = Material(name, tree.values)
            seen[item.identity] = Event("publish", material=item)
    return tuple(seen.values())


def approval(model, actor):
    return Approval(actor, model.subject.identity, model.subject.version, model.subject.coverage)


def publish_all(model, trees, now=2):
    model.batch(now, publication(trees))


def cold_inspect(model, actor):
    """Panel-side complete tiny replay; initially it has no model cache."""
    if not model.required_material.issubset(model.materials):
        return "Unavailable", None
    for index, leaf in enumerate(model.subject.leaves):
        values = {kind: model.materials[(kind, root, count)].values
                  for kind, root, count in leaf.original.roots}
        x = values["input"][0]
        w, b = values["model"]
        trace = values["trace"]
        truths = legacy.relation_truths(x, w, b, trace)
        if not all(truths.values()):
            relation = next(name for name, valid in truths.items() if not valid)
            trees = {name: legacy.Merkle(name, vals) for name, vals in values.items()}
            return "Refutation", (index, relation, legacy.make_evidence(leaf.original, trees, relation))
        expected, _ = legacy.make_original(x, w, b, legacy.canonical_trace(x, w, b), leaf.original.scope)
        require(expected.roots == leaf.original.roots, "all relations passed but canonical roots differ")
    return "Approval", approval(model, actor)


def check_cold_single_honest():
    outcomes = 0
    for count in range(1, 4):
        for honest in SELECTED:
            state, trees = fixture(count)
            require(not state.materials and state.admitted, "model preholding became admission condition")
            require(cold_inspect(state, honest)[0] == "Unavailable", "cold panel fabricated replay")
            publish_all(state, trees)
            result, signed = cold_inspect(state, honest)
            require(result == "Approval", "correct cold replay failed")
            state.batch(3, (Event("approval", approval=signed),))
            require(state.phase != "Final", "one vote finalized before notice window")
            state.batch(4)
            require(state.phase != "Final", "inclusive notice endpoint finalized")
            state.batch(5)
            require(state.phase == "Final" and state.credits == count * 2, "one honest approval cannot finalize")
            require(state.approvals == {honest}, "hidden all-ACK or other signatures required")
            outcomes += 1
    return {"cold_start_complete_runs": outcomes, "other_selected_signatures": 0}


def check_one_refutation_wins():
    runs = 0
    for count in range(1, 4):
        for bad in range(count):
            for honest in SELECTED:
                state, trees = fixture(count, bad)
                state.batch(1, tuple(Event("approval", approval=approval(state, x)) for x in SELECTED if x != honest))
                publish_all(state, trees)
                result, packet = cold_inspect(state, honest)
                require(result == "Refutation", "full replay missed bad leaf")
                index, relation, proof = packet
                cid = state.open_case(4, honest, "mismatch", index, relation)
                require(cid is not None, "honest case not admitted at notice endpoint")
                state.batch(5, (Event("proof", actor=honest, case=cid, leaf=index, evidence=proof),))
                require(state.phase == "Refuted" and state.credits == 0, "positive approvals defeated proof")
                runs += 1
    return {"bad_leaf_and_honest_identity_combinations": runs,
            "local_checker": "preserved authenticated positional Merkle checker"}


def check_material_and_inspection():
    state, trees = fixture()
    cid = state.open_case(2, "bond-c", "material_request")
    require(cid is not None, "request incorrectly requires arithmetic proof or preheld model")
    state.batch(2, (Event("approval", approval=approval(state, "bond-a")),))
    pieces = publication(trees)
    state.batch(5, pieces[:1])
    case = state.cases[cid]
    require(case.phase == "InspectionPending" and case.prerequisite is None and cid in state.q,
            "partial material completed full scope")
    state.batch(6, pieces[1:])
    require(case.prerequisite is not None and cid in state.q, "full delivery wrongly closes inspection")
    result, signed = cold_inspect(state, "bond-c")
    require(result == "Approval", "post-notice existing scope cannot be inspected")
    state.batch(7, (Event("complete", actor="bond-c", case=cid), Event("approval", approval=signed)))
    require(state.phase == "Final", "completion of existing case cannot permit Final")
    other, badtrees = fixture(bad_leaf=0)
    latecid = other.open_case(4, "bond-c", "material_request")
    other.batch(5, (Event("approval", approval=approval(other, "bond-a")),) + publication(badtrees))
    _, packet = cold_inspect(other, "bond-c")
    index, relation, proof = packet
    other.batch(7, (Event("proof", actor="bond-c", case=latecid, leaf=index, evidence=proof),))
    require(other.phase == "Refuted", "post-notice continuation lost refutation right")
    return {"same_case_after_notice": ["correct_completion_to_Final", "bad_trace_to_Refuted"],
            "delivery_alone_closes_case": False}


def check_inspection_prerequisite_space():
    total = established = 0
    original, trees = fixture()
    pieces = publication(trees)
    for mask, when, service, opportunity, exhausted in itertools.product(
            range(1 << len(pieces)), range(2, 8), (False, True), (False, True), (False, True)):
        state, _ = fixture()
        cid = state.open_case(2, "bond-c", "material_request")
        case = state.cases[cid]
        state.service, state.opportunity_schedule = service, opportunity
        case.quota_used = 2 if exhausted else 0
        for i, event in enumerate(pieces):
            if mask & (1 << i):
                state.publish(event.material)
        record = state.derive_prerequisite(case, when)
        expected = mask == (1 << len(pieces)) - 1 and when <= 6 and service and opportunity
        require((record is not None) == expected, "inspection prerequisite was inferred from insufficient evidence")
        if record is not None:
            require(record.claimant == "bond-c" and record.full_scope == tuple(sorted(state.required_material)),
                    "wrong scope or claimant in derived prerequisite")
            established += 1
        total += 1
    return {"enumerated_combinations": total, "established": established,
            "derivation": "all tiny authenticated material + claimant-specific slot + original remaining time",
            "large_offchain_LLM_availability": "NOT ESTABLISHED"}


def check_no_false_default():
    runs = 0
    for missing in range(3):
        for exhausted in (False, True):
            state, trees = fixture()
            cid = state.open_case(2, "bond-c", "material_request")
            state.cases[cid].quota_used = 2 if exhausted else 0
            state.batch(3, (Event("approval", approval=approval(state, "bond-a")),)
                        + tuple(e for i, e in enumerate(publication(trees)) if i != missing))
            state.batch(9)
            require(state.cases[cid].phase == "NoSlashHold" and cid in state.q,
                    "withheld material became claimant default")
            state.batch(11)
            require(state.phase == "AbortedUnverified" and state.credits == 0,
                    "unverified material received success credit")
            runs += 1
    late, trees = fixture()
    cid = late.open_case(4, "bond-c", "material_request")
    late.batch(7, publication(trees))
    require(late.cases[cid].prerequisite is None, "late material reset original inspection deadline")
    late.batch(11)
    require(late.phase == "AbortedUnverified", "late supply completed without inspection time")
    return {"withholding_and_quota_cases": runs, "late_delivery_without_remaining_time": "AbortedUnverified"}


def check_false_cases_and_withholding_votes():
    runs = 0
    for honest in SELECTED:
        for order in itertools.permutations(SELECTED):
            state, trees = fixture()
            publish_all(state, trees)
            false_cases = []
            for actor in order:
                if actor != honest:
                    material = state.open_case(4, actor, "material_request")
                    computation = state.open_case(4, actor, "mismatch", relation="Mul")
                    require(material and computation, "maximum legal false cases not admitted")
                    false_cases.append((actor, material, computation))
            _, signed = cold_inspect(state, honest)
            state.batch(5, (Event("approval", approval=signed),))
            require(state.phase != "Final", "unresolved false case ignored")
            for actor, _, computation in false_cases:
                proof = legacy.make_evidence(state.subject.leaves[0].original, trees[0], "Mul")
                state.batch(6, (Event("proof", actor=actor, case=computation, evidence=proof),))
            state.batch(9)
            require(all(state.cases[c].phase == "ChallengerDefault" for _, c, _ in false_cases),
                    "objective toy opportunity failed to terminate nonresponse")
            require(state.phase == "Final" and state.approvals == {honest},
                    "malicious abstention or bounded false cases blocked valid completion")
            runs += 1
    return {"unique_honest_and_request_order_runs": runs, "max_false_cases_per_run": 4,
            "liveness_scope": "tiny complete-public-transcript procedure and reserved canonical service only"}


def check_version_and_scope():
    old, _ = fixture(version=OLD)
    old.selected = tuple("legacy-" + str(i) for i in range(75))
    old.slots = {x: Slot(old.subject.identity, x) for x in old.selected}
    for n in range(40):
        old.count_approval(approval(old, old.selected[n]))
    old.batch(5)
    require(old.phase != "Final", "legacy threshold changed to 1")
    old.count_approval(approval(old, old.selected[40]))
    old.batch(5)
    require(old.phase == "Final", "legacy 41 threshold failed")
    new, _ = fixture()
    accepted = approval(new, "bond-a")
    invalid = [replace(accepted, version=OLD), replace(accepted, issuer="outsider"),
               replace(accepted, subject="stale"), replace(accepted, coverage=()),
               replace(accepted, authenticated=False)]
    for item in invalid:
        require(not new.count_approval(item), "stale/partial/unauthorized approval accepted")
    require(new.count_approval(accepted) and new.count_approval(accepted), "valid approval rejected")
    require(len(new.approvals) == 1, "duplicate signature creates independent approvals")
    new.batch(5)
    require(new.phase == "Final", "new profile silently retained legacy threshold")
    return {"legacy_selected_positions": 75, "legacy_required": 41, "new_required": 1,
            "invalid_scope_variants_rejected": len(invalid)}


def check_gate_enumeration():
    total = final = 0
    for bits in itertools.product((False, True), repeat=7):
        vote, pending, local, dependencies, economics, service, opportunity = bits
        for now in (1, 4, 5, 10, 11):
            state, _ = fixture()
            state.local_positive, state.dependencies = local, dependencies
            state.economics, state.service, state.opportunity_schedule = economics, service, opportunity
            if vote:
                state.count_approval(approval(state, "bond-a"))
            if pending:
                state.open_case(2, "bond-c", "material_request")
            want = vote and not pending and local and dependencies and economics and service and opportunity and 4 < now <= 10
            require(state.can_final(now) == want, "Final conjunction/temporal boundary mismatch")
            total += 1
            final += int(want)
    return {"gate_time_combinations": total, "permitted_Final_cases": final}


def check_batch_immutability_and_atomicity():
    invalid = []
    state, _ = fixture(2)
    subject = state.subject
    leaves = subject.leaves
    variants = [replace(subject, leaves=(leaves[0], leaves[0])),
                replace(subject, leaves=(leaves[0], replace(leaves[1], producer="other"))),
                replace(subject, leaves=(leaves[0], replace(leaves[1], model="other"))),
                replace(subject, leaves=(leaves[0], replace(leaves[1], profile="other"))),
                replace(subject, leaves=(replace(leaves[0], prior_admitted=True), leaves[1])),
                replace(subject, sealed_at=subject.draw_at)]
    for candidate in variants:
        require(not Model(candidate).admitted, "malformed or post-draw atomic batch admitted")
        invalid.append(candidate.identity)
    old_approval = approval(state, "bond-a")
    changed = replace(subject, leaves=(leaves[1], leaves[0]))
    require(not Model(changed).count_approval(old_approval), "sealed leaf ordering was mutable")
    other, _ = fixture(3)
    require(not other.count_approval(old_approval), "old approval covered appended leaf")
    new, trees = fixture(3, bad_leaf=1)
    publish_all(new, trees)
    _, (index, relation, proof) = cold_inspect(new, "bond-c")
    cid = new.open_case(4, "bond-c", "mismatch", index, relation)
    new.batch(5, (Event("approval", approval=approval(new, "bond-a")),
                  Event("proof", actor="bond-c", case=cid, leaf=index, evidence=proof)))
    require(new.phase == "Refuted" and new.credits == 0, "bad leaf left partial batch credit")
    return {"invalid_batch_variants": len(invalid), "reordered_or_appended_scope_rejections": 2,
            "one_bad_leaf_refutes_whole_batch": True}


def check_credit_once_and_case_identity():
    state, trees = fixture(3)
    publish_all(state, trees)
    cid_a = state.open_case(3, "bond-a", "material_request")
    cid_c = state.open_case(3, "bond-c", "material_request")
    require(cid_a != cid_c, "shared material identity merged claimant rights")
    state.batch(4)
    state.batch(5, (Event("complete", actor="bond-a", case=cid_a),
                    Event("approval", approval=approval(state, "bond-a"))))
    require(cid_c in state.q and state.phase != "Final", "one claimant closed another's case")
    state.batch(6, (Event("complete", actor="bond-c", case=cid_c),))
    require(state.phase == "Final" and state.credits == 6, "atomic batch not credited once")
    for _ in range(5):
        state.batch(7, (Event("approval", approval=approval(state, "bond-a")),))
        state.settle()
    require(state.credits == 6 and len(state.issued_execution_ids) == 3, "replay minted duplicate execution credit")
    restored = copy.deepcopy(state)
    restored.batch(8)
    require(restored.credits == 6, "state restore renewed credit")
    rewritten = replace(state.subject, sealed_at=2, draw_at=3)
    retry = Model(rewritten, issued_execution_ids=set(state.issued_execution_ids))
    require(not retry.admitted and retry.credits == 0,
            "rewrapped subject reminted globally issued execution IDs")
    return {"independent_claimant_cases": 2, "repeated_settlement_attempts": 6,
            "issued_execution_ids": 3, "credits": 6,
            "rewrapped_batch_with_already_issued_execution_ids": "rejected",
            "restoration_scope": "in-memory state-copy invariant, not actual restart/reorg/IBD implementation"}


def check_source_order_independence():
    template, trees = fixture(bad_leaf=0)
    original = template.subject.leaves[0].original
    proof = legacy.make_evidence(original, trees[0], "Add")
    outcomes = set()
    for ordering in itertools.permutations(("material_request", "mismatch", "approval", "proof", "noop")):
        state, _ = fixture(bad_leaf=0)
        cid = state.case_id("bond-c", "mismatch", 0, "Add")
        events = {"material_request": Event("material_request", actor="bond-c"),
                  "mismatch": Event("mismatch", actor="bond-c"),
                  "approval": Event("approval", approval=approval(state, "bond-a")),
                  "proof": Event("proof", actor="bond-c", case=cid, evidence=proof),
                  "noop": Event("noop")}
        state.batch(4, tuple(events[x] for x in ordering))
        state.batch(5)
        outcomes.add((state.phase, state.credits))
    require(outcomes == {("Refuted", 0)}, "event textual order allowed false Final")
    return {"permutations": 120, "distinct_outcomes": len(outcomes)}


def check_unknown_profile_and_assumption_limits():
    unknown, _ = fixture(procedure=UNESTABLISHED_PROCEDURE)
    unknown.batch(5, (Event("approval", approval=approval(unknown, "bond-a")),))
    require(unknown.phase == "UnsupportedProfile" and unknown.credits == 0,
            "undefined availability procedure admitted by a boolean")
    all_bad, _ = fixture(bad_leaf=0)
    all_bad.batch(1, (Event("approval", approval=approval(all_bad, "bond-a")),))
    all_bad.batch(5)
    require(all_bad.phase == "Final", "expected all-collusion boundary was hidden")
    censored, _ = fixture(bad_leaf=0)
    censored.batch(1, (Event("approval", approval=approval(censored, "bond-a")),))
    # The honest request/proof never reaches canonical input; Node has no oracle.
    censored.batch(5)
    require(censored.phase == "Final", "canonical censorship was incorrectly detectable")
    return {"unestablished_large_data_profile": "UnsupportedProfile",
            "expected_outside_guarantee_counterexamples": ["all verifiers collude", "all honest communication censored"],
            "actual_LLM_publication_prerequisite_mechanism": "NOT ESTABLISHED"}


def check_honest_coverage():
    witnesses = 0
    for honest in SELECTED:
        all_state = run_assignment(honest, SELECTED)
        require(all_state.phase == "Refuted", "complete assignment lost unique honest verifier")
        for size in (1, 2):
            for subset in itertools.combinations(SELECTED, size):
                if honest not in subset:
                    weakened = run_assignment(honest, subset)
                    require(weakened.phase == "Final" and weakened.credits > 0,
                            "arbitrary-subset mutant lacks false-success counterexample")
                    witnesses += 1
    require(witnesses == 9, "subset counterexample enumeration changed")
    # One honest verifier checks only leaf zero; a malicious leaf one is uncovered.
    state, trees = fixture(2, bad_leaf=1)
    first = state.subject.leaves[0]
    vals = {name: t.values for name, t in trees[0].items()}
    first_true = all(legacy.relation_truths(vals["input"][0], *vals["model"], vals["trace"]).values())
    require(first_true and cold_inspect_after_publish(state, trees)[0] == "Refutation",
            "partitioned honest coverage counterexample missing")
    weakened, same_trees = fixture(2, bad_leaf=1)
    publish_all(weakened, same_trees)
    # In the weakened responsibility, checking assigned leaf zero is mislabeled
    # as full approval. The native signature cannot reveal that scope omission.
    weakened.batch(5, (Event("approval", approval=approval(weakened, "bond-c")),))
    require(weakened.phase == "Final", "partition-only mutant lacks false-success witness")
    return {"subset_excludes_unique_honest_counterexamples": witnesses,
            "partitioned_layer_or_leaf_counterexamples": 1,
            "classification": "EXPECTED_UNSAFE_OPTIMIZATIONS_NOT_ADOPTED"}


def cold_inspect_after_publish(state, trees):
    publish_all(state, trees)
    return cold_inspect(state, "bond-c")


def run_assignment(honest, assigned):
    state, trees = fixture(bad_leaf=0)
    publish_all(state, trees)
    for actor in assigned:
        if actor == honest:
            _, (index, relation, proof) = cold_inspect(state, actor)
            cid = state.open_case(4, actor, "mismatch", index, relation)
            state.batch(4, (Event("proof", actor=actor, case=cid, leaf=index, evidence=proof),))
        else:
            state.batch(4, (Event("approval", approval=approval(state, actor)),))
    state.batch(5)
    return state


def check_mutations():
    killed = []
    for mutant in ("early_one_vote", "all_ack", "ignore_q", "ignore_version", "partial_is_full",
                   "quota_is_delivery", "delivery_closes_case", "merge_claimants", "partial_batch_credit"):
        muts = frozenset((mutant,))
        state, trees = fixture(2 if mutant == "partial_batch_credit" else 1,
                               bad_leaf=1 if mutant == "partial_batch_credit" else None)
        if mutant == "early_one_vote":
            state.batch(2, (Event("approval", approval=approval(state, "bond-a")),), muts)
            exposed = state.phase == "Final"
        elif mutant == "all_ack":
            state.batch(5, (Event("approval", approval=approval(state, "bond-c")),), muts)
            exposed = state.phase != "Final"
        elif mutant == "ignore_q":
            state.open_case(3, "bond-c", "material_request")
            state.batch(5, (Event("approval", approval=approval(state, "bond-a")),), muts)
            exposed = state.phase == "Final" and bool(state.q)
        elif mutant == "ignore_version":
            exposed = state.count_approval(replace(approval(state, "bond-a"), version=OLD), muts)
        elif mutant in ("partial_is_full", "quota_is_delivery"):
            cid = state.open_case(3, "bond-c", "material_request")
            state.cases[cid].quota_used = 2
            state.publish(publication(trees)[0].material)
            state.batch(4, mutants=muts)
            state.batch(9, (Event("approval", approval=approval(state, "bond-a")),), muts)
            exposed = state.phase == "Final" and not state.required_material.issubset(state.materials)
        elif mutant == "delivery_closes_case":
            cid = state.open_case(3, "bond-c", "material_request")
            state.batch(5, publication(trees) + (Event("approval", approval=approval(state, "bond-a")),), muts)
            exposed = state.phase == "Final" and state.cases[cid].phase == "ClosedCompleted"
        elif mutant == "merge_claimants":
            first = state.open_case(3, "bond-a", "material_request", mutants=muts)
            other = state.open_case(3, "bond-c", "material_request", mutants=muts)
            exposed = first == other
        else:
            publish_all(state, trees)
            _, (index, relation, proof) = cold_inspect(state, "bond-c")
            cid = state.open_case(4, "bond-c", "mismatch", index, relation)
            state.batch(5, (Event("proof", actor="bond-c", case=cid, leaf=index, evidence=proof),), muts)
            exposed = state.phase == "Refuted" and state.credits > 0
        require(exposed, "mutant not exposed: " + mutant)
        killed.append(mutant)
    # These two errors concern assignment/envelope acceptance outside the fold.
    coverage = check_honest_coverage()
    require(coverage["subset_excludes_unique_honest_counterexamples"] > 0, "subset mutant survived")
    killed.append("assign_only_arbitrary_subset")
    sealed, _ = fixture(1)
    extended, _ = fixture(2)
    require(sealed.subject.identity != extended.subject.identity
            and not extended.count_approval(approval(sealed, "bond-a")), "late-leaf mutant survived")
    extended.batch(5, (Event("approval", approval=approval(sealed, "bond-a")),),
                   frozenset(("accept_late_leaf_with_old_approval",)))
    require(extended.phase == "Final" and extended.credits == 4,
            "late-leaf mutant failed to expose unapproved new credit")
    killed.append("accept_late_leaf_with_old_approval")
    return {"mutants_detected": killed, "count": len(killed)}


def source_hashes():
    names = ("0016-miner-execution-commit-and-material-supply.md",
             "0017-noninteractive-positive-verification.md",
             "0018-interactive-negative-court-and-defaults.md",
             "0019-model-free-node-and-bounded-evidence.md",
             "0020-verification-gated-final-and-adversarial-models.md")
    return {"docs/rfc-v2/" + name: hashlib.sha256((DOCS / "rfc-v2" / name).read_bytes()).hexdigest()
            for name in names}


def run_preserved_checks():
    checks = legacy.Checks()
    functions = (("M01", legacy.check_trace_space), ("M02", legacy.check_authentication),
                 ("M03", legacy.check_batch_permutations), ("M04", legacy.check_deadlines),
                 ("M05", legacy.check_case_isolation), ("M06", legacy.check_failure_classification),
                 ("M07", legacy.check_once_and_revocation), ("M08", legacy.check_protected_reservations),
                 ("M09", legacy.check_final_gate_space), ("M10", legacy.check_stale_scopes),
                 ("M11", legacy.expected_limitations), ("M12", legacy.check_mutants),
                 ("M13", legacy.check_material_inspection_continuity),
                 ("M14", legacy.check_claimant_case_identity), ("M15", legacy.check_authenticated_raw_schema),
                 ("M16", legacy.check_consumer_post_batch_state))
    for name, fn in functions:
        checks.run(name, fn)
    return checks


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="print full result JSON")
    parser.add_argument("--output", type=Path, default=HERE / "public-panel-model-results.json")
    args = parser.parse_args()
    checks = legacy.Checks()
    functions = (("P01-cold-single-honest-normal-completion", check_cold_single_honest),
                 ("P02-one-refutation-overrides-all-approvals", check_one_refutation_wins),
                 ("P03-material-inspection-continuity", check_material_and_inspection),
                 ("P04-derived-inspection-prerequisite-space", check_inspection_prerequisite_space),
                 ("P05-no-default-from-missing-material-or-quota", check_no_false_default),
                 ("P06-bounded-false-cases-and-abstention", check_false_cases_and_withholding_votes),
                 ("P07-signature-version-and-scope", check_version_and_scope),
                 ("P08-Final-gate-enumeration", check_gate_enumeration),
                 ("P09-atomic-sealed-batch", check_batch_immutability_and_atomicity),
                 ("P10-case-isolation-and-once-credit", check_credit_once_and_case_identity),
                 ("P11-canonical-event-order", check_source_order_independence),
                 ("P12-unestablished-profile-and-guarantee-limits", check_unknown_profile_and_assumption_limits),
                 ("P13-honest-whole-subject-coverage", check_honest_coverage),
                 ("P14-mutation-sensitivity", check_mutations))
    for name, fn in functions:
        checks.run(name, fn)
    prior = run_preserved_checks()
    report = {"schema": "misaka.public-one-honest.finite-model.v1",
              "revision": "2026-11-09-public-panel",
              "status": "FAIL" if checks.failures or prior.failures else "PASS",
              "scope": "Finite conditional specification model; no runtime, LLM, public-availability or capacity certification.",
              "assumptions": ["correct signature/draw authentication, hash binding and native toy arithmetic",
                              "model events have canonical inclusion, not mere network transmission",
                              "exactly specified finite inspection and filing opportunities are supplied",
                              "one active honest selected participant inspects the entire subject"],
              "toy_availability_procedure": {
                  "kind": TOY_PROCEDURE,
                  "scope": "At most three tiny scalar execution leaves; all required authenticated material is explicitly in a finite public transcript.",
                  "maximum_scalar_material_records": 41,
                  "not_a_proposal_for": "Publishing a full LLM through Node or certifying off-chain delivery by a boolean/receipt.",
                  "large_LLM_procedure": "NOT ESTABLISHED; unknown procedure admission is rejected."},
              "not_established": ["profile-specific objective availability/inspection mechanism for large off-chain LLM data",
                                  "actual LLM kernel coverage, production numeric policy or trace/resource bounds",
                                  "Rust implementation, real signatures, censorship resistance, live network or activation",
                                  "physical Miner/Panel execution or actual independent work",
                                  "real restart, reorg, IBD or throughput and economic parameter correctness"],
              "source_sha256": source_hashes(),
              "preserved_model_sha256": hashlib.sha256(LEGACY_PATH.read_bytes()).hexdigest(),
              "checks": checks.results,
              "summary": {"passed": len(checks.results)-len(checks.failures), "failed": len(checks.failures)},
              "preserved_revision_checks": {"scope": "Reexecuted unchanged previous model; its old toy quorum policy is not the new PublicOneHonest policy.",
                                             "checks": prior.results,
                                             "summary": {"passed": len(prior.results)-len(prior.failures), "failed": len(prior.failures)}}}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    else:
        for check in checks.results:
            print(check["status"], check["id"], check.get("error", ""))
        print("New checks:", report["summary"])
        print("Preserved previous checks:", report["preserved_revision_checks"]["summary"])
        print("Large-data profile procedure / implementation / measurement: NOT ESTABLISHED")
        print("Report:", args.output)
    return int(report["status"] == "FAIL")


if __name__ == "__main__":
    raise SystemExit(main())
