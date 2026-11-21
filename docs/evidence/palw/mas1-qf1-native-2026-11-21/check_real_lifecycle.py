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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log",type=Path)
    parser.add_argument("class_check",type=Path)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--model-audit", action="store_true", help="recompute the registered non-paying model audit freeze")
    mode.add_argument("--pending-freeze", action="store_true", help="verify held class registration and refusal before native finality")
    args=parser.parse_args()
    verify = check_model_audit if args.model_audit else check_pending_freeze if args.pending_freeze else check
    print(json.dumps(verify(args.log.read_text(), json.loads(args.class_check.read_bytes())), indent=2))
