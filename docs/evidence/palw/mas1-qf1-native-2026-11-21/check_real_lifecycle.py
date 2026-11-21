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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log",type=Path)
    parser.add_argument("class_check",type=Path)
    args=parser.parse_args()
    print(json.dumps(check(args.log.read_text(),json.loads(args.class_check.read_bytes())),indent=2))
