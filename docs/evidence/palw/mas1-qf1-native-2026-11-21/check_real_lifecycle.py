"""Check the real-model Original disclosures, later PANEL draw and public Stop witness."""
import argparse
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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log",type=Path)
    parser.add_argument("class_check",type=Path)
    parser.add_argument("--pending-freeze", action="store_true", help="verify held class registration and refusal before native finality")
    args=parser.parse_args()
    verify = check_pending_freeze if args.pending_freeze else check
    print(json.dumps(verify(args.log.read_text(), json.loads(args.class_check.read_bytes())), indent=2))
