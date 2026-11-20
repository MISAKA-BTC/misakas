#!/usr/bin/env python3
"""Adversarial checks of the independent AN1 toy implementation.

These checks do not run the linked author's code or prove network/cryptographic
assumptions. Run from any directory with Python 3 and its standard library.
"""
from __future__ import annotations

from collections import defaultdict
from dataclasses import replace
from pathlib import Path
import copy
import hashlib
import json
import random

import an1_reference as a


HERE = Path(__file__).resolve().parent
GROUPS: dict[str, int] = defaultdict(int)
LABELS: list[dict] = []


def check(group: str, condition: bool, label: str) -> None:
    if not condition:
        raise AssertionError(f"{group}: {label}")
    GROUPS[group] += 1
    LABELS.append({"group": group, "check": label, "status": "PASS"})


def rejects(group: str, fn, label: str) -> None:
    try:
        fn()
    except (ValueError, KeyError, TypeError):
        check(group, True, label)
    else:
        check(group, False, label)


def rebind_claim(claim, material):
    return replace(claim,
                   trace_root=a.Tree("trace", claim.attempt, material.trace_records,
                                     material.trace_padding).root,
                   output_root=a.Tree("output", claim.attempt, material.output_records).root)


def hostile_tree_root(kind: str, scope: str, records: list[bytes]) -> str:
    """An adversary can hash malformed/oversized bytes without using Tree."""
    n = len(records)
    L = 1 << (n - 1).bit_length()
    payloads = records + [a.pad_record(i) for i in range(n, L)]
    level = []
    for i, raw in enumerate(payloads):
        pre = a.Tree.preimage("AN1-real-leaf" if i < n else "AN1-padding-leaf",
                             kind, scope, n, L, [i], [raw])
        level.append(hashlib.sha256(pre).digest())
    height = 1
    while len(level) > 1:
        parents = []
        for i in range(len(level) // 2):
            pre = a.Tree.preimage("AN1-internal", kind, scope, n, L,
                                 [height, i], [level[2 * i], level[2 * i + 1]])
            parents.append(hashlib.sha256(pre).digest())
        level = parents
        height += 1
    return level[0].hex()


def ledger_for(context, sigma, jobs, claim):
    ledger = a.Ledger({(context["beacon_key"], context["beacon_round"]): (sigma, 20)})
    slot = ledger.register(context, jobs, 10, context["owner"])
    ledger.seal(slot, claim, 30, context["owner"])
    panel = ledger.select_panel(slot, bytes.fromhex("44" * 32), 100,
                                ["p0", "p1", "p2", "p3", "p4", "p5"])
    return ledger, slot, panel


def main():
    c, sigma, jobs, weights = a.fixture_context()
    claim, material = a.build_claim(c, sigma, jobs, weights)
    native = a.build_native_context(c, sigma, jobs, claim)
    model, trace, output = a.material_trees(native, material)
    tokens = a.derive_input(claim.attempt, jobs)

    group = "actual_input_binding_and_canonical_encoding"
    byte_tokens = [3 + b for b in range(256)]
    check(group, len(set(byte_tokens)) == 256, "all 256 byte values have distinct token IDs")
    check(group, not set(byte_tokens) & {0, 1, 2}, "byte tokens do not alias BOS/POW/SEP")
    check(group, tokens[:2] == [0, 1] and tokens[34] == 2 and tokens[35:] == jobs,
          "exact challenge-first token layout")
    check(group, bytes(t - 3 for t in tokens[2:34]).hex() == claim.attempt,
          "all 32 attempt bytes are present in actual tokens")
    changed = {}
    for field, value in [("owner", "55" * 32), ("nonce", "66" * 32)]:
        alt = dict(c, **{field: value})
        alt_claim, alt_material = a.build_claim(alt, sigma, jobs, weights)
        alt_native = a.build_native_context(alt, sigma, jobs, alt_claim)
        changed[field] = (alt, alt_claim, alt_material, alt_native)
        check(group, alt_claim.attempt != claim.attempt, f"{field} changes attempt ID")
        check(group, a.derive_input(alt_claim.attempt, jobs) != tokens, f"{field} changes actual input")
        rejects(group, lambda alt=alt: a.build_native_context(alt, sigma, jobs, claim),
                f"old claim is not accepted under changed {field}")
    rejects(group, lambda: a.validate_context(dict(c, nonce=c["nonce"].upper() + "00")),
            "noncanonical nonce cannot produce a second spelling")
    rejects(group, lambda: a.validate_context(dict(c, comment="free salt")),
            "unregistered metadata is not part of identity")
    rejects(group, lambda: a.derive_input(claim.attempt, [259]), "out-of-vocabulary job token rejected")
    rejects(group, lambda: a.derive_input(claim.attempt, [1] * 9), "input budget overflow rejects instead of truncating")
    for kind, bad_tokens in [("truncated", tokens[:-1]), ("dropped challenge", tokens[:2] + tokens[34:]),
                             ("UNK-normalized", tokens[:2] + [3] * 32 + tokens[34:])]:
        wrong_root = a.Tree("input", claim.attempt,
                            [a.encode({"i": i, "v": t}) for i, t in enumerate(bad_tokens)]).root
        rejects(group, lambda wrong_root=wrong_root: a.build_native_context(
            c, sigma, jobs, replace(claim, input_root=wrong_root)), f"{kind} input cannot be the native input")

    group = "registration_original_and_panel_order"
    ledger = a.Ledger({(c["beacon_key"], c["beacon_round"]): (sigma, 20)})
    rejects(group, lambda: ledger.register(c, jobs, 20, c["owner"]), "registration at beacon access rejected")
    rejects(group, lambda: ledger.register(c, jobs, 21, c["owner"]), "post-beacon nonce registration rejected")
    rejects(group, lambda: ledger.register(c, jobs, 10, "77" * 32), "wrong authenticated registration owner")
    slot = ledger.register(c, jobs, 10, c["owner"])
    rejects(group, lambda: ledger.register(c, jobs, 11, c["owner"]), "same registration only once")
    rejects(group, lambda: ledger.register(c, jobs, 11, c["owner"], "different-funding-reference"),
            "different funding cannot create another slot for identical canonical C")
    rejects(group, lambda: ledger.register(dict(c, chain="foreign-chain"), jobs, 10, c["owner"]),
            "payload cannot select another native chain")
    rejects(group, lambda: ledger.select_panel(slot, bytes.fromhex("44" * 32), 100,
            ["a", "b", "c", "d", "e"]), "no Panel before original claim")
    rejects(group, lambda: ledger.seal(slot, claim, 19, c["owner"]), "no original before execution beacon")
    rejects(group, lambda: ledger.seal(slot, claim, 100, c["owner"]), "no original after Panel event")
    rejects(group, lambda: ledger.seal(slot, claim, 30, "77" * 32), "wrong authenticated original owner")
    ledger.seal(slot, claim, 30, c["owner"])
    rejects(group, lambda: ledger.seal(slot, claim, 31, c["owner"]), "original roots cannot be replaced")
    rejects(group, lambda: ledger.select_panel(slot, sigma, 100,
            ["a", "b", "c", "d", "e"]), "execution beacon is not interchangeable with Panel event")
    panel = ledger.select_panel(slot, bytes.fromhex("44" * 32), 100,
                                ["a", "b", "c", "d", "e"])
    rejects(group, lambda: ledger.select_panel(slot, bytes.fromhex("44" * 32), 101,
            list(panel)), "Panel event cannot be redrawn")
    check(group, ledger.rows[slot]["claim"].original_id == claim.original_id,
          "sealed claim remains identical after Panel selection")

    group = "full_reexecution_and_canonical_root"
    review = a.panel_review(native, material)
    check(group, review["status"] == "ApprovedFullReplay", "full honest replay approved")
    check(group, review["checked_records"] == claim.trace_count, "full trace compared, not sampled")
    check(group, len(review["output_tokens"]) == 2, "fixed two-token generation")
    again, again_material = a.build_claim(c, sigma, jobs, weights)
    check(group, again == claim, "same registered attempt has one canonical claim and Ticket")
    check(group, again_material.trace_records == material.trace_records, "deterministic full trace bytes")
    check(group, trace.L == 8192 and trace.n == 6153, "independent profile pinned shape")
    check(group, len(trace.hash_inputs) == 2 * trace.L - 1, "all padded tree nodes individually hashed")
    check(group, len(set(trace.hash_inputs)) == len(trace.hash_inputs), "distinct positions imply distinct preimages")
    for field, (_, ac, am, an) in changed.items():
        _, at, _ = a.material_trees(an, am)
        check(group, not set(trace.hash_inputs) & set(at.hash_inputs),
              f"all tree input preimages change for {field}")
        check(group, a.panel_review(an, am)["status"] == "ApprovedFullReplay",
              f"new {field} has independently correct execution")

    group = "native_local_court_for_valid_relations"
    view = native.court_view()
    check(group, set(view.__dict__) == {"model_root", "claim", "program_scope", "input_length", "output_refs"},
          "Node Court context contains no full model, program or trace arrays")
    representatives = {}
    for i, instruction in enumerate(native.program.instructions):
        representatives.setdefault(instruction["op"], i)
    samples = sorted(set(representatives.values()) | set(random.Random(20261118).sample(
        range(claim.trace_count), 12)))
    for i in samples:
        evidence = a.make_step_evidence(native, material, i)
        check(group, a.judge_court(view, evidence) == "NoRefutation", f"valid local instruction {i}")
    for i in range(2):
        check(group, a.judge_court(native, a.make_output_evidence(native, material, i)) == "NoRefutation",
              f"output {i} binds to terminal selection")
    check(group, a.judge_court(native, a.make_padding_evidence(native, material, trace.n)) == "NoRefutation",
          "canonical padding is not fraud")

    group = "authenticated_arithmetic_and_structure_refutation"
    bad_materials = []
    for op in ["INPUT", "EMBED", "MUL", "DIV", "ARGMAX_UPDATE"]:
        i = representatives[op]
        bad = copy.deepcopy(material)
        v = a.parse_value(bad.trace_records[i], i)
        bad.trace_records[i] = a.encode({"i": i, "v": v + 1})
        bc = rebind_claim(claim, bad)
        bn = a.build_native_context(c, sigma, jobs, bc)
        result = a.panel_review(bn, bad)
        check(group, result["status"] == "Refuted", f"Panel finds committed false {op}")
        check(group, a.judge_court(bn, result["court"]) == "ComputationRefuted",
              f"Node confirms committed false {op}")
        bad_materials.append((bc, bad, bn, result["court"]))
    for raw in [a.encode({"i": 0, "v": 1, "extra": 2}), a.encode({"i": 999, "v": 0}),
                a.encode({"i": 0, "v": True}), a.encode({"i": 0, "v": 1 << 64})]:
        bad = copy.deepcopy(material)
        bad.trace_records[0] = raw
        bc = rebind_claim(claim, bad)
        bn = a.build_native_context(c, sigma, jobs, bc)
        check(group, a.judge_court(bn, a.make_step_evidence(bn, bad, 0)) == "ComputationRefuted",
              f"authenticated malformed record {raw.decode()}")

    group = "canonical_numeric_boundaries_regression"
    wrong_claim, _, _, _ = bad_materials[0]
    reg = a.Ledger({(c["beacon_key"], c["beacon_round"]): (sigma, 20)})
    reg_slot = reg.register(c, jobs, 10, c["owner"])
    for field in ["trace_count", "output_count"]:
        expected = getattr(claim, field)
        for value in [float(expected), True, False, -1, 0, 1 << 64, str(expected), None]:
            altered = replace(wrong_claim, **{field: value})
            label = f"{field}={value!r} ({type(value).__name__})"
            rejects(group, lambda altered=altered: a.build_native_context(c, sigma, jobs, altered),
                    "native rejects " + label)
            rejects(group, lambda altered=altered: reg.seal(reg_slot, altered, 30, c["owner"]),
                    "original admission rejects " + label)
    for field in ["epoch", "beacon_round", "deadline", "panel_event"]:
        for value in [float(c[field]), True, -1, 1 << 64]:
            rejects(group, lambda field=field, value=value: a.validate_context(dict(c, **{field: value})),
                    f"context {field} strictly rejects {value!r}")
    for value in [True, False, 1.0, -1, (1 << 256) + 1]:
        rejects(group, lambda value=value: a.validate_context(dict(c, target=value)),
                f"Ticket target strictly rejects {value!r}")
    for value in [10.0, True, -1, 1 << 64]:
        empty = a.Ledger({(c["beacon_key"], c["beacon_round"]): (sigma, 20)})
        rejects(group, lambda empty=empty, value=value: empty.register(c, jobs, value, c["owner"]),
                f"native clock strictly rejects {value!r}")
    valid_evidence = a.make_step_evidence(native, material, 0)
    for index in [0.0, True, False, -1, "0"]:
        e = copy.deepcopy(valid_evidence)
        e["index"] = index
        check(group, a.judge_court(native, e) == "InvalidEvidence", f"Court index is strict integer: {index!r}")
    e = copy.deepcopy(valid_evidence)
    e["target"] = replace(e["target"], index=0.0)
    check(group, a.judge_court(native, e) == "InvalidEvidence", "opening index float does not alias integer")
    # Canonical integers still permit an authenticated error to be refuted.
    check(group, a.judge_court(bad_materials[0][2], bad_materials[0][3]) == "ComputationRefuted",
          "strict metadata preserves valid negative Court for erroneous trace")

    group = "invalid_evidence_and_weight_index_authentication"
    i = representatives["EMBED"]
    evidence = a.make_step_evidence(native, material, i)
    wrong = copy.deepcopy(evidence)
    opening = wrong["target"]
    wrong["target"] = replace(opening, payload=a.encode({"i": i, "v": 999}))
    check(group, a.judge_court(native, wrong) == "InvalidEvidence", "fake value with old path is not a refutation")
    wrong = copy.deepcopy(evidence)
    wrong["target"] = replace(opening, payload=b"x" * 513)
    check(group, a.judge_court(native, wrong) == "InvalidEvidence", "oversized local record rejected before parsing")
    wrong = copy.deepcopy(evidence)
    wi = evidence["weight"].index
    wrong["weight"] = model.opening((wi + 1) % 1060)
    check(group, a.judge_court(native, wrong) == "InvalidEvidence", "same model wrong weight index rejected")
    wrong = copy.deepcopy(evidence)
    foreign = a.model_tree(a.deterministic_weights(24))
    wrong["weight"] = foreign.opening(wi)
    check(group, a.judge_court(native, wrong) == "InvalidEvidence", "foreign model weight rejected")
    wrong = copy.deepcopy(evidence)
    wrong["program"] = native.program.tree.opening(i + 1)
    check(group, a.judge_court(native, wrong) == "InvalidEvidence", "different authenticated instruction rejected")
    wrong = copy.deepcopy(evidence)
    wrong["parents"] = [trace.opening(0)]
    check(group, a.judge_court(native, wrong) == "InvalidEvidence", "arbitrary trace dependency rejected")
    for field, (_, _, _, alt_native) in changed.items():
        check(group, a.judge_court(alt_native, evidence) == "InvalidEvidence", f"old Court under changed {field}")
        wrong = copy.deepcopy(evidence)
        wrong["original_id"] = alt_native.claim.original_id
        check(group, a.judge_court(alt_native, wrong) == "InvalidEvidence", f"rewritten ID with old openings for {field}")

    group = "output_and_padding_binding"
    bad = copy.deepcopy(material)
    bad.output_records[0] = a.encode({"i": 0, "v": (review["output_tokens"][0] + 1) % 259})
    bc = rebind_claim(claim, bad)
    bn = a.build_native_context(c, sigma, jobs, bc)
    result = a.panel_review(bn, bad)
    check(group, result["status"] == "Refuted", "Panel detects output/terminal mismatch")
    check(group, a.judge_court(bn, result["court"]) == "ComputationRefuted", "Node confirms output/terminal mismatch")
    bad_padding = copy.deepcopy(material)
    bad_padding.trace_padding[trace.n] = a.encode({"padding": trace.n, "value": 99})
    pc = rebind_claim(claim, bad_padding)
    pn = a.build_native_context(c, sigma, jobs, pc)
    result = a.panel_review(pn, bad_padding)
    check(group, pc.trace_root != claim.trace_root, "free padding would change Root and Ticket")
    check(group, result["status"] == "Refuted", "Panel rejects noncanonical padding")
    check(group, a.judge_court(pn, result["court"]) == "ComputationRefuted", "authenticated padding violation refuted")

    group = "actual_material_roots_not_root_attributes"
    bad = copy.deepcopy(material)
    bad.weights[0] += 1
    bad.claimed_model_root = c["model_root"]
    check(group, a.panel_review(native, bad)["status"] == "MaterialsUnauthenticated",
          "model root attribute cannot replace hashing actual weights")
    bad = copy.deepcopy(material)
    bad.trace_records[0] = a.encode({"i": 0, "v": 111})
    bad.claimed_trace_root = claim.trace_root
    check(group, a.panel_review(native, bad)["status"] == "MaterialsUnauthenticated",
          "trace root attribute cannot replace hashing actual records")
    rejects(group, lambda: a.build_claim(c, sigma, jobs, a.deterministic_weights(24)),
            "Miner generator also validates actual weight root")
    check(group, a.panel_review(native, None)["status"] == "MaterialsUnavailable", "missing data is not approval")

    group = "bounded_material_notice_before_final_regression"
    bad = copy.deepcopy(material)
    bad.trace_records[0] = b" " * (a.MAX_LEAF_BYTES + 1)
    raw_root = hostile_tree_root("trace", claim.attempt, bad.trace_records)
    oversized_claim = replace(claim, trace_root=raw_root)
    oversized_native = a.build_native_context(c, sigma, jobs, oversized_claim)
    ledger, slot, panel = ledger_for(c, sigma, jobs, oversized_claim)
    for actor in panel[:4]:
        ledger.approve(slot, actor)
    ledger.rows[slot].update(materials_ready=True, closed=True, delivered=True)
    report = a.panel_review(oversized_native, bad)
    check(group, report["status"] == "MaterialsUnauthenticated" and "court" not in report,
          "oversized committed record is not mislabeled as a bounded refutation")
    check(group, report["required_scope"] == {"kind": "trace", "index": 0},
          "honest Panel identifies the unserviceable authenticated trace position")
    batch = ledger.apply_batch(slot, [], True, 180, material_notices=[(panel[4], report)])
    check(group, len(batch["material_holds"]) == 1 and not batch["final"],
          "included qualified material notice precedes same-batch Final")
    check(group, not ledger.credits and slot not in ledger.consumed,
          "four approvals and stale closure flags do not bypass material hold")
    check(group, not ledger.finalize(slot, 301), "material hold cannot expire into success")
    rejects(group, lambda: ledger.material_notice(slot, "unselected", report, 180),
            "notice must be from the selected authenticated duty holder")
    wrong_report = copy.deepcopy(report)
    wrong_report["required_scope"]["index"] = 0.0
    rejects(group, lambda: ledger.material_notice(slot, panel[4], wrong_report, 180),
            "material notice index uses the same strict integer rule")
    for invalid_padding in [None, [], [1]]:
        malformed = copy.deepcopy(material)
        malformed.trace_padding = invalid_padding
        result = a.panel_review(native, malformed)
        check(group, result["status"] == "MaterialsUnauthenticated" and "required_scope" in result,
              f"padding transport {invalid_padding!r} returns a hold report, not an exception")
    malformed = copy.deepcopy(material)
    malformed.output_records[1] = b"x" * 513
    result = a.panel_review(native, malformed)
    check(group, result["required_scope"] == {"kind": "output", "index": 1},
          "unserviceable output cannot be resolved by an unrelated trace scope")
    malformed = copy.deepcopy(material)
    malformed.weights[7] = 1.0
    result = a.panel_review(native, malformed)
    check(group, result["required_scope"] == {"kind": "model", "index": 7},
          "malformed model weight reports its actual model index")

    group = "one_honest_panel_negative_precedence"
    bc, bm, bn, court = bad_materials[2]
    ledger, slot, panel = ledger_for(c, sigma, jobs, bc)
    for actor in panel[:4]:
        ledger.approve(slot, actor)
    row = ledger.rows[slot]
    row.update(materials_ready=True, closed=True, delivered=True)
    check(group, len(row["approved"]) == 4, "four dishonest positive approvals are possible")
    honest_review = a.panel_review(bn, bm)
    batch = ledger.apply_batch(slot, [honest_review["court"]], True, 180)
    check(group, batch["court_results"] == ["ComputationRefuted"], "one honest valid Court is accepted")
    check(group, not batch["final"] and slot not in ledger.consumed,
          "same-batch valid negative precedes Final despite four approvals")
    check(group, not ledger.credits, "refuted original earns no credit")
    # Demonstrate the boundary: a valid local Court is not a full validity proof.
    local_only = a.make_step_evidence(bn, bm, 0)
    check(group, a.judge_court(bn, local_only) == "NoRefutation",
          "one copied truthful local Court can coexist with an invalid whole trace")
    no_honest, other_slot, others = ledger_for(c, sigma, jobs, bc)
    no_honest.approve(other_slot, others[0])
    no_honest.rows[other_slot].update(materials_ready=True, closed=True, delivered=True)
    check(group, no_honest.finalize(other_slot, 180),
          "counterexample: absent honest inspection/inclusion assumptions, dishonest approval can pass")

    group = "pending_availability_and_current_final_gate"
    ledger, slot, panel = ledger_for(c, sigma, jobs, claim)
    ledger.approve(slot, panel[0])
    check(group, not ledger.finalize(slot, 180), "approval alone is insufficient")
    row = ledger.rows[slot]
    row.update(materials_ready=True, closed=True, delivered=True)
    row["pending"].add("missing-trace-scope")
    check(group, not ledger.finalize(slot, 180), "unresolved data request blocks success")
    check(group, not ledger.finalize(slot, 301), "timeout never turns pending data into success")
    row["pending"].clear()
    row["active"] = False
    check(group, not ledger.finalize(slot, 180), "current inactive profile blocks stale approval")
    row["active"] = True
    row["delivered"] = False
    check(group, not ledger.finalize(slot, 180), "execution approval does not prove delivery")
    row["delivered"] = True
    row["closed"] = False
    check(group, not ledger.finalize(slot, 180), "no success before valid inspection closure")
    rejects(group, lambda: ledger.approve(slot, "outsider"), "unselected positive actor rejected")

    group = "atomic_slot_credit_and_rollback"
    ledger, slot, panel = ledger_for(c, sigma, jobs, claim)
    ledger.approve(slot, panel[0])
    ledger.rows[slot].update(materials_ready=True, closed=True, delivered=True)
    check(group, ledger.finalize(slot, 180), "one valid modeled finalization")
    check(group, not ledger.finalize(slot, 180), "second same-slot finalization rejected")
    check(group, ledger.credits[c["owner"]] == 1 and slot in ledger.consumed,
          "credit and slot consumption occur together")
    check(group, claim.attempt in ledger.attempt_consumed, "attempt consumption is atomic with slot and credit")
    work_key = a.computational_work_key(c, sigma, jobs)
    check(group, work_key in ledger.work_consumed, "actual computational work consumption is atomic")
    fake_slot = "99" * 32
    ledger.rows[fake_slot] = copy.deepcopy(ledger.rows[slot])
    ledger.rows[fake_slot]["final"] = False
    check(group, not ledger.finalize(fake_slot, 180), "imported duplicate row cannot remap an original to another slot")
    ledger.rollback_last_final()
    check(group, ledger.credits[c["owner"]] == 0 and slot not in ledger.consumed
          and not ledger.rows[slot]["final"] and claim.attempt not in ledger.attempt_consumed,
          "native rollback undoes credit, slot and attempt consumption together")
    check(group, work_key not in ledger.work_consumed, "rollback also undoes computational-work consumption")
    ledger.attempt_consumed.add(claim.attempt)
    check(group, not ledger.finalize(slot, 180), "already consumed attempt blocks even if slot flag is absent")
    ledger.attempt_consumed.remove(claim.attempt)
    ledger.work_consumed.add(work_key)
    check(group, not ledger.finalize(slot, 180), "consumed computational work blocks another credit")
    ledger.work_consumed.remove(work_key)
    check(group, ledger.finalize(slot, 180) and ledger.credits[c["owner"]] == 1,
          "reapplied canonical final does not retain detached credit")

    group = "useful_job_identity_and_mining_attempt_separation"
    alt_context = changed["nonce"][0]
    check(group, a.base_work_key(c) == a.base_work_key(alt_context),
          "new nonce is not automatically new original useful job")
    check(group, a.computational_work_key(c, sigma, jobs) != a.computational_work_key(alt_context, sigma, jobs),
          "different actual challenge tokens are different computational jobs")
    useful = dict(c, accounting_version="AN1-TEST-USEFUL-ONCE-1")
    uc, um = a.build_claim(useful, sigma, jobs, weights)
    useful2 = dict(useful, nonce="88" * 32)
    uc2, um2 = a.build_claim(useful2, sigma, jobs, weights)
    ledger = a.Ledger({(c["beacon_key"], c["beacon_round"]): (sigma, 20)})
    slot = ledger.register(useful, jobs, 10, useful["owner"])
    slot2 = ledger.register(useful2, jobs, 10, useful2["owner"])
    ledger.seal(slot, uc, 30, useful["owner"])
    ledger.seal(slot2, uc2, 30, useful2["owner"])
    panel = ledger.select_panel(slot, bytes.fromhex("44" * 32), 100,
                                ["p0", "p1", "p2", "p3", "p4", "p5"])
    panel2 = ledger.select_panel(slot2, bytes.fromhex("44" * 32), 100,
                                 ["p0", "p1", "p2", "p3", "p4", "p5"])
    ledger.approve(slot, panel[0])
    ledger.approve(slot2, panel2[0])
    ledger.rows[slot].update(materials_ready=True, closed=True, delivered=True)
    ledger.rows[slot2].update(materials_ready=True, closed=True, delivered=True)
    check(group, ledger.finalize(slot, 180), "one useful-job credit under prebound accounting version")
    check(group, not ledger.finalize(slot2, 180), "different slot and attempt cannot duplicate useful-job credit")
    ledger.rollback_last_final()
    check(group, a.base_work_key(useful) not in ledger.useful_consumed,
          "rollback includes purpose-specific useful-job consumption")

    group = "strict_target_and_cost_guarantee_boundary"
    for difficulty in [1, 2, 3, 17, 1 << 20]:
        old_target = ((1 << 256) - 1) // difficulty
        theta = old_target + 1
        for value in sorted({0, old_target, min(old_target + 1, (1 << 256) - 1), (1 << 256) - 1}):
            check(group, (value <= old_target) == (value < theta),
                  f"CAD1 inclusive/AN1 strict target equivalence D={difficulty}, value={value}")
    check(group, ((1 << 256) - 1) + 1 == 1 << 256, "maximal strict target is not uint256 zero")
    rejects(group, lambda: a.validate_context(dict(c, target=(1 << 256) + 1)), "target beyond mathematical range rejected")
    zero_weights = [0] * 1060
    zc, zs, zj, zw = a.fixture_context(weights=zero_weights)
    zclaim, zmat = a.build_claim(zc, zs, zj, zw)
    znative = a.build_native_context(zc, zs, zj, zclaim)
    check(group, a.panel_review(znative, zmat)["status"] == "ApprovedFullReplay",
          "correct zero model is not rejected as arithmetic fraud")
    check(group, [a.parse_value(r, i) for i, r in enumerate(zmat.output_records)] == [0, 0],
          "distinct inputs need not imply distinct output text or expensive evaluation")

    vectors = {
        "notice": "Independent toy vectors, not the linked AN1 author's original vectors.",
        "profile_id": a.PROFILE_ID, "context": c, "context_encoding_hex": a.encode(c).hex(),
        "sigma_hex": sigma.hex(), "slot_id": claim.slot, "attempt_id": claim.attempt,
        "job_tokens": jobs, "input_tokens": tokens, "output_tokens": review["output_tokens"],
        "model_root": model.root, "program_root": claim.program_root,
        "input_root": claim.input_root, "trace_root": claim.trace_root, "output_root": claim.output_root,
        "original_id": claim.original_id, "ticket_hex": f"{claim.ticket:064x}",
        "model_weights": 1060, "trace_records": trace.n, "padded_trace_leaves": trace.L,
        "trace_hash_calls": len(trace.hash_inputs), "output_refs": list(native.program.output_refs),
        "weight_seed": 23, "trace_record_samples": [json.loads(material.trace_records[i])
                                                 for i in [0, 1, 2, trace.n - 1]],
    }
    results = {
        "status": "PASS", "independent_implementation": True,
        "original_linked_bundle_obtained_or_rerun": False,
        "profile_id": a.PROFILE_ID, "group_count": len(GROUPS),
        "assertions": sum(GROUPS.values()), "groups": dict(GROUPS),
        "model": {"vocabulary": 259, "dimension": 2, "weights": 1060,
                  "generated_tokens": 2, "trace_records": trace.n,
                  "padded_trace_leaves": trace.L, "trace_hash_calls": len(trace.hash_inputs)},
        "assumptions_not_implemented": ["real owner signatures", "real beacon authentication/unpredictability",
                                       "network DA", "protected Court inclusion", "actual honest Panel selection",
                                       "transactional DB", "stable consensus/finality", "LLM cost lower bound"],
        "negative_assumption_test": "A false claim can pass when all inspection/closure fixtures lie and no honest refutation arrives. This is an explicit boundary test, not an unconditional safety claim.",
        "checks": LABELS,
        "source_sha256": {name: hashlib.sha256((HERE / name).read_bytes()).hexdigest()
                          for name in ["an1_reference.py", "check_an1.py"]},
    }
    (HERE / "profile.json").write_text(json.dumps(a.PROFILE, ensure_ascii=False, indent=2) + "\n")
    (HERE / "test_vectors.json").write_text(json.dumps(vectors, ensure_ascii=False, indent=2) + "\n")
    (HERE / "results.json").write_text(json.dumps(results, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k: results[k] for k in ["status", "group_count", "assertions", "model"]}, indent=2))


if __name__ == "__main__":
    main()
