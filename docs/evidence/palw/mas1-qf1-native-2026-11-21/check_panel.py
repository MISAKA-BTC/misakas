"""Recompute the private PANEL's fixed-weight draw from native observations."""
import hashlib
import json
import re
import sys
from fractions import Fraction
from pathlib import Path
from check_original import verify


def race(anchor, original, actor, weight):
    message = anchor + bytes.fromhex(original) + bytes.fromhex(actor)
    digest = hashlib.blake2b(message, key=b"misaka-palw/panel-v2/stake-ticket/v1").digest()
    value = int.from_bytes(digest[:8], "little") + 1
    n = value.bit_length() - 1
    y = value << (63 - n) if n <= 63 else value >> (n - 63)
    fraction = 0
    for i in range(64):
        y = (y * y) >> 63
        if y >= 1 << 64:
            y >>= 1
            fraction |= 1 << (63 - i)
    return Fraction((64 << 64) - ((n << 64) | fraction), weight), actor


def check_panel(log):
    return verify_draw(log, verify(log, 1 << 256))


def verify_draw(log, result, *, expect_replies=True, missing_material=False):
    hex64, hex32 = r"([0-9a-f]{128})", r"([0-9a-f]{64})"
    events = re.findall(r"TB1 stable PANEL pid=(\d+) slot=(\d+) close=" + hex64 + " stable=" + hex64
                        + r" stable_daa=(\d+) release_daa=(\d+) batch=" + hex64 + " certificate=" + hex64 + " seed=" + hex32, log)
    if (len(events) != 3 or {int(row[0]) for row in events} != set(result["pids"])
            or {int(row[1]) for row in events} != {0, 1, 2} or len({row[2:] for row in events}) != 1):
        raise ValueError("different PANEL release observations")
    close, stable, stable_daa, release_daa, batch, certificate, seed = events[0][2:]
    if close != result["panel_close"] or not 26 <= int(release_daa) < 50 or int(stable_daa) > int(release_daa):
        raise ValueError("wrong close or PANEL release window")
    if len({close, stable, batch, certificate, result["receipt"]}) != 5:
        raise ValueError("duplicate native block")
    exec_events = re.findall(r"TB1 publication pid (\d+) batch " + hex64 + " certificate " + hex64 + " seed " + hex32, log)
    if (len(exec_events) != 3 or {int(row[0]) for row in exec_events} != set(result["pids"])
            or len({row[1:] for row in exec_events}) != 1 or exec_events[0][3] == seed):
        raise ValueError("unexpected EXEC seed")
    reply_pattern = hex64 if expect_replies else r"(none)"
    duties = re.findall(r"TB1 replay duty pid=(\d+) actor=" + hex64
                        + r" replayed=(true|false) selected=\[([^\]]+)\] original=" + hex64 + " replies=" + reply_pattern, log)
    if missing_material:
        if expect_replies or duties:
            raise ValueError("unexpected complete or duplicate replay duty")
        material = re.findall(r"QF1 missing material pid=(\d+) actor=" + hex64 + " original=" + hex64
                              + " certificate=" + hex64 + r" selected=\[([^\]]+)\] missing=(true|false) signed=(true|false)"
                              + r" signatures=(\d+) pending_daa=(\d+) deadline=(\d+) tip=" + hex64, log)
        if (len(material) != 3 or len({(r[2:5], r[7:]) for r in material}) != 1 or material[0][3] != certificate
                or material[0][7] != "1" or int(material[0][8]) + 1 != int(material[0][9])
                or material[0][10] in {certificate, batch, stable, close, result["receipt"]}):
            raise ValueError("inconsistent incomplete reply or deadline observations")
        duties = [(r[0], r[1], r[6], r[4], r[2], None) for r in material]
    if (len(duties) != 3 or {int(row[0]) for row in duties} != set(result["pids"])
            or len({row[3:] for row in duties}) != 1 or duties[0][4] != result["original"]):
        raise ValueError("different PANEL duty receipts")
    if not expect_replies and re.search(r"TB1 replay duty [^\n]*replies=[0-9a-f]{128}", log):
        raise ValueError("unexpected positive replay receipt")
    actors = sorted(row[1] for row in duties)
    if len(set(actors)) != 3:
        raise ValueError("actor key repeated")
    anchor = hashlib.sha3_512(b"MISAKA/private/TB1/PANEL/draw-anchor/v1" + bytes.fromhex(result["panel_marker"])
                              + bytes.fromhex(seed)).digest()
    ranked = sorted(race(anchor, result["original"], actor, weight) for actor, weight in zip(actors, (1, 2, 4)))
    selected = [actor for _, actor in ranked[:2]]
    expected_workers = selected[1:] if missing_material else selected
    if re.findall(r"[0-9a-f]{128}", duties[0][3]) != selected or any((r[2] == "true") != (r[1] in expected_workers) for r in duties):
        raise ValueError("draw order or actual workers disagree")
    if missing_material:
        if any((r[5] == "true") != (r[1] == selected[0]) for r in material):
            raise ValueError("unavailable signer outside expected seat")
        result.update(unavailable_actor=selected[0], signed_actor=selected[1], signatures=1,
                      pending_daa=int(material[0][8]), deadline=int(material[0][9]), deadline_block=material[0][10])
    result.update(panel_release="certified", stable=stable, stable_daa=int(stable_daa), release_daa=int(release_daa),
                  panel_certificate=certificate, panel_batch=batch, panel_seed=seed, exec_seed=exec_events[0][3],
                  actors_by_weight=actors, selected=selected, accepted_replies=duties[0][5] if expect_replies else None)
    return result


if __name__ == "__main__":
    print(json.dumps(check_panel(Path(sys.argv[1]).read_text()), indent=2))
