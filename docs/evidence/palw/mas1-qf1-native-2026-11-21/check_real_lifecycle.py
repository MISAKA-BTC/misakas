"""Check the real-model Original disclosures, later PANEL draw and public Stop witness."""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path

from check_original import envelope
from check_panel import verify_draw
from check_prediction import qf1, shared_rows
from check_stop import verify_case


def check(log, artifact):
    pids, (attempt, prompt_json) = shared_rows(log, r"QF1 input pid=(\d+) attempt=([0-9a-f]{64}) tokens=(\[[0-9, ]+\])")
    peers, (cls, descriptor, positions, answer_json, stop, z_hex) = shared_rows(
        log, r"QF1 fixture pid=(\d+) class=(\w+) descriptor=(\w+) positions=(\d+) tokens=(\[[0-9, ]+\]) stop=(\w+) z=(\w+)")
    if peers != pids or cls != artifact["class_id"]:
        raise ValueError("peer or real model class mismatch")
    prompt, answer = json.loads(prompt_json), json.loads(answer_json)
    schedule = qf1.schedule(prompt, answer, 3)
    if (schedule["stop_reason"], schedule["processed_positions"]) != (stop.upper(), int(positions)):
        raise ValueError("decode position or Stop")
    ordered = sorted(map(int, pids))
    def observations(name):
        rows = [json.loads(row) for row in re.findall(rf"QF1 {name} (\{{[^\n]+\}})", log)]
        if sorted(r.pop("pid") for r in rows) != ordered:
            raise ValueError(f"peer set for {name}")
        return rows
    bodies = observations("original")
    if bodies[1:] != [bodies[0]] * 2:
        raise ValueError("peers signed different Originals")
    body = bodies[0]
    receipt_peers, (receipt, original) = shared_rows(log, r"QF1 receipt pid=(\d+) block=(\w+) original=(\w+)")
    close_peers, (closed_receipt, close, blue, marker) = shared_rows(
        log, r"QF1 panel close pid=(\d+) receipt=(\w+) close=(\w+) blue=(\d+) marker=(\w+) release=pending")
    if (receipt_peers != pids or close_peers != pids or closed_receipt != receipt
            or receipt == close or body["original"] != original):
        raise ValueError("Original receipt or PANEL close")
    s, z = bytes.fromhex(attempt), bytes.fromhex(z_hex)
    roots = list(map(bytes.fromhex, body["roots"]))
    l, t, one, d = body["counts"]
    def token_root(kind, tokens):
        return qf1.root(s, kind, [struct.pack("<I", x) for x in tokens])
    if ([l,t,one] != [len(prompt), len(answer), 1]
            or roots[:3] != [token_root(1, prompt), token_root(2, answer), qf1.root(s, 3, [z])]
            or qf1.ticket(s, body["counts"], roots).hex() != body["ticket"]
            or int(body["target"], 16) != 1 << 256 or not body["ticket_meets_target"]):
        raise ValueError("private target, ticket or input/answer/readout roots")
    openings = sorted(observations("disclosure"), key=lambda r: r["index"])
    if [r["index"] for r in openings] != [0, 64, d-1]:
        raise ValueError("unexpected opened positions")
    for row in openings:
        if not qf1.verify_opening(s, 4, d, row["index"], bytes.fromhex(row["record"]),
                                  list(map(bytes.fromhex, row["siblings"])), roots[3]):
            raise ValueError("TRACE disclosure")
    init = bytes.fromhex(openings[0]["record"])
    if init != envelope(0, init[5:69] + bytes.fromhex(descriptor) + s + roots[0] + struct.pack("<IH",l,3)):
        raise ValueError("Init does not bind the signed instance and input")
    h = (len(qf1.DOM_Z) + t * (5 + 2*37*4096) + 5 + 4095) // 4096
    tiles = d - h - t - 3
    end = envelope(5, b"".join(roots[:3]) + struct.pack("<IIIQIIQ", l,t,int(positions),tiles,t,h,d) + z)
    if end != bytes.fromhex(openings[-1]["record"]):
        raise ValueError("End does not bind counts, roots and readout")
    binary = re.search(r"^test_binary_sha256=([0-9a-f]{64})$", log, re.MULTILINE)
    if binary is None:
        raise ValueError("missing binary identity")
    panel = verify_draw(log, dict(**body, pids=ordered, receipt=receipt, panel_close=close, panel_close_blue_score=int(blue),
                                  panel_marker=marker, readout_chunks=h, binary_sha256=binary[1]))
    return dict(class_id=cls,descriptor=descriptor,attempt=attempt,input_ids=prompt,output_ids=answer,stop=stop,
                readout_z=z_hex,panel=panel,court=verify_case(log,panel))


def check_pending_freeze(log, artifact):
    peers, (registered, model, inventory, state, daa, blue) = shared_rows(
        log, r"QF1 registered pid=(\d+) block=(\w+) class=(\w+) artifact=(\w+) state=(\w+) daa=(\d+) blue=(\d+)")
    linked, (block, prepared, parent_state) = shared_rows(
        log, r"QF1 registered EXEC pid=(\d+) registration=(\w+) preparation=(\w+) parent_state=([0-9a-f]{128})")
    qualified, (preparation, exec_root, panel_root, consumer) = shared_rows(
        log, r"TB1 qualification pid=(\d+) preparation=(\w+) exec=(\w+) panel=(\w+) consumer=(\w+)")
    observations = re.findall(
        r"TB1 pending registered freeze pid=(\d+) slot=(\d+) registration=(\w+) preparation=(\w+) close=(\w+) "
        r"generation=(\w+) daa=(\d+) from=(\d+) deadline=(\d+) publication=WouldBlock signing=WouldBlock", log)
    if (peers != linked or peers != qualified or len(observations) != 3 or {x[0] for x in observations} != peers
            or {x[1] for x in observations} != {"0", "1", "2"} or len({x[2:] for x in observations}) != 1):
        raise ValueError("different peer, holder or selected-chain observations")
    close, generation = observations[0][4:6]
    observed, release, deadline = map(int, observations[0][6:])
    if not release <= observed < deadline:
        raise ValueError("observation outside the original signing window")
    if (registered != block or prepared != preparation or observations[0][2:4] != (registered, prepared)
            or state != parent_state or state == "0" * 128 or len({registered, preparation, close, generation}) != 4
            or model != artifact["class_id"] or inventory != artifact["artifact_root"] or exec_root == panel_root):
        raise ValueError("registration, pinned header state, qualification or freeze mismatch")
    binary = re.search(r"^test_binary_sha256=([0-9a-f]{64})$", log, re.MULTILINE)
    if binary is None or "TB1 publication pid" in log:
        raise ValueError("missing build identity or unexpected release")
    return dict(binary_sha256=binary[1], pids=sorted(map(int, peers)), class_id=model, artifact_root=inventory,
                registration=registered, registration_daa=int(daa), registration_blue=int(blue), state_root=state,
                preparation=preparation, exec_qual_root=exec_root, panel_qual_root=panel_root, consumer_root=consumer,
                close=close, generation=generation, observed_daa=observed, release_daa=release, deadline=deadline,
                publication="WouldBlock", signing="WouldBlock")


def check_exec_quote(log, artifact, *, slots=(7, 9)):
    frozen = check_pending_freeze(log, artifact)
    peers, (model, prep, close, anchor, deadline, reward, min_out, consent) = shared_rows(
        log, r"MOE2 closed quote pid=(\d+) class=(\w+) preparation=(\w+) close=(\w+) quote_anchor=(\w+) "
             r"deadline=(\d+) max_reward=(\d+) min_out=(\d+) consent=(\w+) no_debit=true")
    quoted_peers, (root, buy, lp, burned, miner, panel) = shared_rows(
        log, r"MOE2 anchor preview pid=(\d+) parent_root=(\w+) buy=(\d+) lp=(\d+) burn=(\d+) miner=(\d+) "
             r"panel=(\d+) original_bond_and_market_unchanged=true")
    curve_peers, curve = shared_rows(
        log, r"MOE2 curve pid=(\d+) virtual=(\d+) reserve=(\d+) inventory=(\d+) purchased=(\d+) locked=(\d+)")
    if (peers != quoted_peers or peers != curve_peers or sorted(map(int, peers)) != frozen["pids"]
            or (model, prep, close, anchor, root) != tuple(frozen[k] for k in
                ("class_id", "preparation", "close", "registration", "state_root"))
            or int(deadline) < frozen["observed_daa"] or int(min_out) != 2):
        raise ValueError("quote is not bound to the same peers, market anchor or EXEC close")
    allocations = []
    listed = re.findall(r"MOE2 allocation row pid=(\d+) rounds=\w+ slot=(\d+) ", log)
    if sorted((pid, int(slot)) for pid, slot in listed) != sorted((pid, slot) for pid in peers for slot in slots):
        raise ValueError("unexpected EXEC allocation set")
    for slot in slots:
        observed, (rounds, key, reference) = shared_rows(
            log, rf"MOE2 allocation row pid=(\d+) rounds=(\w+) slot={slot} key=(\w+) reference=(\w+)")
        if observed != peers:
            raise ValueError("missing quote allocation")
        allocations.append((rounds, slot, bytes.fromhex(key), bytes.fromhex(reference)))
    rounds = allocations[0][0]
    if len({key for _, _, key, _ in allocations}) != len(slots):
        raise ValueError("duplicate EXEC registration key")
    for identifier, slot, key, ref in allocations:
        if identifier != rounds:
            raise ValueError("different EXEC rounds")
        if slot != 7:
            source = hashlib.sha3_512(b"MISAKA/private/FOW1/MOE2/quote-consent/v1" + key + slot.to_bytes(8, "big")).digest()
            if source != ref or (slot == 9 and source.hex() != consent):
                raise ValueError("quote consent differs from signed registration and slot")
    h = hashlib.sha3_512(b"MISAKA/private/FOW1/exec-input-set/v2" + bytes.fromhex(rounds)
                         + bytes.fromhex(prep) + len(slots).to_bytes(4, "big"))
    for _, slot, key, ref in allocations:
        h.update(slot.to_bytes(8, "big") + key + ref)
    if h.hexdigest() != frozen["consumer_root"]:
        raise ValueError("quote allocation differs from the closed consumer root")
    reward = int(reward)
    purchase, liquidity = 3 * reward // 10, reward // 5
    workers = reward - purchase - liquidity
    virtual, reserve, inventory, purchased, locked = map(int, curve)
    after_buy = virtual + reserve + purchase
    quantity = inventory * purchase // after_buy
    lp_quantity = liquidity * (inventory - quantity) // after_buy
    if (purchased, locked) != (quantity, lp_quantity) or purchased < int(min_out):
        raise ValueError("MOE2 quantities differ from the quote anchor's curve")
    if (int(buy), int(lp), int(burned), int(miner), int(panel)) != (purchase, liquidity, purchased - locked, workers - workers // 5, workers // 5):
        raise ValueError("quote preview distribution mismatch")
    return dict(**frozen, quote_consent=consent, quote_deadline=int(deadline), quote_max_reward=reward,
                purchase=purchase, permanent_lp=liquidity, purchased_units=purchased, locked_units=locked,
                retired_units=int(burned), miner=int(miner), panel=int(panel),
                native_funding_debit=0)


def check_exec_budget(log, artifact, *, slots=(7, 9)):
    quote = check_exec_quote(log, artifact, slots=slots)
    peers, (close, parent_root, consent, collateral, rate, liability, reward, epoch, panel_release, bootstrap) = shared_rows(
        log, r"MOE2 budget preview pid=(\d+) close=(\w+) parent_root=([0-9a-f]{128}) slot=9 consent=(\w+) "
             r"collateral=(\d+) export_permille=(\d+) liability=(\d+) reward=(\d+) epoch=(\d+) panel_release=(\d+) bootstrap=(true|false) no_debit=true")
    collateral, rate, liability, reward, epoch = map(int, (collateral, rate, liability, reward, epoch))
    if (sorted(map(int, peers)) != quote["pids"] or close != quote["close"] or parent_root == quote["state_root"]
            or int(panel_release) <= quote["quote_deadline"] or consent != quote["quote_consent"] or reward != quote["quote_max_reward"] or not 0 < rate <= 510
            or liability != (1000 * reward + rate - 1) // rate or liability > collateral):
        raise ValueError("close, signed offer, export ceiling, or minimum collateral mismatch")
    return dict(**quote, close_parent_state_root=parent_root, collateral_sompi=collateral, export_cap_permille=rate,
                minimum_liability_sompi=liability, model_allocation_epoch=epoch, allocation_bootstrap=(bootstrap == "true"),
                earliest_panel_release_daa=int(panel_release))


def check_live_offers(log, artifact):
    result = check_exec_budget(log, artifact, slots=(7, 9, 10))
    peers, (consent, parent, tip, root, now, release, deadline, assignment, assigned_daa, capital,
            buy, lp, burned, miner, panel) = shared_rows(
        log, r"MOE2 live offer pid=(\d+) slot=10 consent=(\w+) execution_parent=(\w+) tip=(\w+) root=(\w+) now=(\d+) "
             r"panel_release=(\d+) deadline=(\d+) assignment=(\w+) assignment_daa=(\d+) assigned=(\d+) buy=(\d+) lp=(\d+) "
             r"burned=(\d+) miner=(\d+) panel=(\d+) short_expired=true native_reward_debit=0")
    rows, (_, key, ref) = shared_rows(log, r"MOE2 allocation row pid=(\d+) rounds=(\w+) slot=10 key=(\w+) reference=(\w+)")
    if (rows != peers or sorted(map(int, peers)) != result["pids"] or consent != ref or consent == result["quote_consent"]
            or len({result["preparation"], result["close"], assignment, parent, tip}) != 5
            or root in (result["state_root"], result["close_parent_state_root"])
            or not int(assigned_daa) < result["release_daa"] <= result["quote_deadline"] < int(release) <= int(now) <= int(deadline)
            or int(release) != result["earliest_panel_release_daa"] or int(capital) != 2 * result["minimum_liability_sompi"]
            or int(capital) > result["collateral_sompi"]
            or list(map(int, (buy, lp, burned, miner, panel))) != [result[k] for k in ("purchase", "permanent_lp", "retired_units", "miner", "panel")]):
        raise ValueError("later offer is not bound to the funded EXEC slot and original terms")
    seasoned_peers, (source, previous, final, final_root, count, daa, epoch, start, assigned, mean_epochs, average, accrued, available, grant, reserved) = shared_rows(
        log, r"MOE2 seasoned pid=(\d+) consent=(\w+) parent=(\w+) tip=(\w+) root=(\w+) blocks=(\d+) daa=(\d+) epoch=(\d+) "
             r"start=(\d+) assigned=(\d+) avg_epochs=(\d+) average=(\d+) accrued=(\d+) available=(\d+) projected_grant=(\d+) reserved=(\d+) native_reward_debit=0")
    if (seasoned_peers != peers or source != consent or assigned != capital or int(count) < 1
            or len({tip, previous, final}) != 3 or final_root == root or int(epoch) <= result["model_allocation_epoch"]
            or not int(now) < int(start) <= int(daa) <= int(deadline) or int(available) > int(accrued)
            or int(mean_epochs) <= 0 or int(average) != int(assigned) // int(mean_epochs)
            or int(available) < int(grant) or int(grant) != result["quote_max_reward"] or int(reserved) != 0):
        raise ValueError("seasoned assignment or non-bootstrap grant mismatch")
    return dict(**result, long_offer=dict(key=key, consent=consent, deadline=int(deadline), observed_daa=int(now),
                parent=parent, tip=tip, state_root=root, assignment=assignment, assignment_daa=int(assigned_daa),
                assigned_sompi=int(capital)), seasoned=dict(parent=previous, tip=final, state_root=final_root,
                blocks=int(count), daa=int(daa), epoch=int(epoch), start_daa=int(start), average_epochs=int(mean_epochs),
                model_average_capital_sompi=int(average), accrued_sompi=int(accrued),
                available_sompi=int(available), projected_grant_sompi=int(grant), actual_reserved_sompi=int(reserved)))


def check_model_audit(log, artifact):
    registered = check_pending_freeze(log, artifact)
    plans, plan = shared_rows(
        log, r"MAS1 audit plan pid=(\d+) audit=(\w+) panel=(\w+) generation=(\d+) suite=(\w+) reservation=(\w+) "
             r"candidates=(\d+) tests=(\d+) case_deadline=(\d+)")
    observed = re.findall(
        r"MAS1 registered audit pid=(\d+) slot=(\d+) class=(\w+) descriptor=(\w+) preparation=(\w+) close=(\w+) "
        r"freeze=(\w+) audit_qual=(\w+) evaluation_panel_qual=(\w+) daa=(\d+) from=(\d+) deadline=(\d+) "
        r"publication=WouldBlock signing=WouldBlock", log)
    if (sorted(map(int, plans)) != registered["pids"] or {r[0] for r in observed} != plans
            or sorted(int(r[1]) for r in observed) != [0, 1, 2] or len({r[2:] for r in observed}) != 1):
        raise ValueError("different observers, slots or model audit cuts")
    cls, descriptor, prep, close, marker, audit_q, panel_q, daa, release, deadline = observed[0][2:]
    audit, panel, generation, suite, reservation, candidates, tests, case_deadline = plan
    if (cls != registered["class_id"] or len({registered["preparation"], registered["close"], prep, close}) != 4
            or len({audit_q, panel_q, registered["exec_qual_root"], registered["panel_qual_root"]}) != 4 or audit == panel
            or not int(release) <= int(daa) < int(deadline) <= int(case_deadline) or int(candidates) != 1 or int(tests) < 1):
        raise ValueError("wrong registered class, budget, round or disclosure window")
    raw = bytes.fromhex
    def digest(tag, *inputs):
        return hashlib.sha3_512(tag + b"".join(inputs)).digest()
    rounds = digest(b"MISAKA/private/MAS1/qualified-evaluation-rounds/v1", raw(audit), raw(audit_q), raw(panel), raw(panel_q))
    consumer = digest(b"MISAKA/private/MAS1/non-rewarding-candidates/v1", raw(prep), rounds, raw(suite), raw(reservation),
                      struct.pack(">IIQI", int(candidates), int(tests), int(case_deadline), 1),
                      raw(cls), raw(registered["artifact_root"]), raw(descriptor), struct.pack(">Q", int(generation)))
    freeze = digest(b"MISAKA/private/TB1/mining-freeze/v1", raw(audit), raw(audit_q), raw(prep), consumer, raw(suite))
    if freeze.hex() != marker:
        raise ValueError("native close differs from the frozen plan and independently qualified rounds")
    expired, end = shared_rows(
        log, r"MAS1 audit expiry pid=(\d+) tip=(\w+) daa=(\d+) blocks=(\d+) close_daa=(\d+) prosecution_daa=(\d+) "
             r"earliest_attempt_cert_daa=(\d+) finalized=none")
    tip, end_daa, blocks, close_daa, window, earliest = end
    if (expired != plans or int(blocks) < 1 or int(end_daa) < max(registered["deadline"], int(deadline))
            or int(close_daa) + int(window) != int(earliest) or int(earliest) < int(deadline)):
        raise ValueError("expiry or original certification window mismatch")
    expiry = dict(tip=tip, daa=int(end_daa), selected_blocks=int(blocks), close_daa=int(close_daa),
                  prosecution_window_daa=int(window), earliest_governed_attempt_certification_daa=int(earliest))
    return dict(expiry=expiry, registered_exec=registered, audit_round=audit, evaluation_panel_round=panel, audit_qualification=audit_q,
                panel_qualification=panel_q, descriptor=descriptor, model_generation=int(generation), suite=suite,
                reservation=reservation, candidate_budget=int(candidates), tests_per_candidate=int(tests), preparation=prep,
                close=close, candidate_root=consumer.hex(), freeze=marker, observed_daa=int(daa), release_daa=int(release),
                deadline=int(deadline), case_deadline=int(case_deadline), publication="WouldBlock", signing="WouldBlock")


def check_certified_history(log, artifact):
    verdicts = re.findall(r"MAS1 certified audit frontier pid=(\d+) registration=(\w+) preparation=(\w+) close=(\w+) "
                          r"class=(\w+) floor=(\w+) frontier=(\w+) tip=(\w+) daa=(\d+) deadline=(\d+) readiness=(\{[^\n]+\})", log)
    replays = re.findall(r"MAS1 audit replay pid=(\d+) source=(archive|producer) state=(\w+) freeze=(\w+) "
                         r"accepted=(\w+) ready_signatures=(\d+) blocks=(\d+)", log)
    pids = {v[0] for v in verdicts}
    if (len(verdicts) != 3 or len(pids) != 3 or len({v[1:] for v in verdicts}) != 1 or {v[0] for v in replays} != pids
            or sorted(v[1] for v in replays) != ["archive", "archive", "producer"] or len({v[2:] for v in replays}) != 1):
        raise ValueError("different selected audit histories or missing independent reader")
    registered, preparation, close, cls, floor, frontier, tip, daa, deadline, raw = verdicts[0][1:]
    state, freeze, accepted, signature_count, blocks = replays[0][2:]
    ready = json.loads(raw)
    cert = re.findall(r"TB1 prior floor certified claim=(\w+) accepted=(\w+) frontier=(\w+) positions=(\d+) quorum=(\d+)", log)
    if (cert != [(floor, accepted, frontier, "75", "41")] or accepted != frontier or cls != artifact["class_id"]
            or len({registered, preparation, close, frontier, tip}) != 5 or signature_count != "18"
            or tip != ready["generation"] or int(daa) != ready["sinkDaa"] or int(blocks) != ready["sinkBlue"]
            or int(daa) >= int(deadline) or ready["safe"] is not None or ready["finalized"]["finalized"] is not None):
        raise ValueError("floor certificate, model, native generation or audit deadline")
    waits = {w["kind"]: (int(w["have"]), int(w["need"])) for w in ready["blocking"]["waits"]}
    if (waits != {"insufficientDepth": (0, 1), "insufficientWork": (0, 1)}
            or ready["blocking"]["evidence"]["pendingFacts"] or ready["skipped"]["baseClass"] != 1):
        raise ValueError("native maturity tally")
    binary = re.search(r"^test_binary_sha256=([0-9a-f]{64})$", log, re.MULTILINE)
    if binary is None:
        raise ValueError("missing executable identity")
    return dict(binary_sha256=binary[1], processes={v[0]: v[1] for v in sorted(replays)}, blocks=int(blocks),
                registration=registered, preparation=preparation, close=close, freeze=freeze, class_id=cls, floor_claim=floor,
                accepted=accepted, frontier=frontier, tip=tip, committed_state_root=state, verified_ready_signatures=int(signature_count),
                certified_daa=int(daa), deadline=int(deadline), blocking=ready["blocking"], finalized=ready["finalized"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log",type=Path)
    parser.add_argument("class_check",type=Path)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--certified-history", action="store_true", help="verify the native floor certificate and history replay on three peers")
    mode.add_argument("--live-offers", action="store_true", help="verify the same signed long offer before and after native capital seasoning")
    mode.add_argument("--exec-budget", action="store_true", help="check the signed offer and native monetary capacity at its close")
    mode.add_argument("--exec-quote", action="store_true", help="recompute the signed quote reference, EXEC close and preview")
    mode.add_argument("--model-audit", action="store_true", help="recompute the registered non-paying model audit freeze")
    mode.add_argument("--pending-freeze", action="store_true", help="verify held class registration and refusal before native finality")
    args=parser.parse_args()
    verify = check
    if args.certified_history:
        verify = check_certified_history
    elif args.live_offers:
        verify = check_live_offers
    elif args.exec_budget:
        verify = check_exec_budget
    elif args.exec_quote:
        verify = check_exec_quote
    elif args.model_audit:
        verify = check_model_audit
    elif args.pending_freeze:
        verify = check_pending_freeze
    print(json.dumps(verify(args.log.read_text(), json.loads(args.class_check.read_bytes())), indent=2))
