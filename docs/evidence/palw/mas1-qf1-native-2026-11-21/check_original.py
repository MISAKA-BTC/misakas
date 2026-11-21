"""Rebuild every record of the stateful test TIR from the three-peer log."""
import json
import re
import struct
import sys
from pathlib import Path
from check_prediction import check, qf1, shared_rows


def envelope(kind, payload):
    return struct.pack("<BI", kind, len(payload)) + payload


def tiles(pos, stage, layer, node, dtype, values):
    code = {1: "h", 2: "i", 5: "I"}[dtype]
    for start in range(0, len(values), 64):
        tile = values[start:start + 64]
        header = struct.pack("<IBHIIHB", pos, stage, layer, node, start, len(tile), dtype)
        yield envelope(1, header + struct.pack(f"<{len(tile)}{code}", *tile))


def verify(log):
    result = check(log)
    original = [json.loads(row) for row in re.findall(r"QF1 original (\{[^\n]+\})", log)]
    disclosures = [json.loads(row) for row in re.findall(r"QF1 disclosure (\{[^\n]+\})", log)]
    for rows in (original, disclosures):
        if sorted(row["pid"] for row in rows) != result["pids"]:
            raise ValueError("different peer set")
    body = [{k: v for k, v in row.items() if k != "pid"} for row in original]
    if body[1:] != body[:1] * 2:
        raise ValueError("disagreement on Original")
    receipt_peers, (receipt, received_id) = shared_rows(
        log, r"QF1 receipt pid=(\d+) block=([0-9a-f]+) original=([0-9a-f]+)"
    )
    close_peers, (closed_receipt, close, blue, marker) = shared_rows(
        log, r"QF1 panel close pid=(\d+) receipt=([0-9a-f]+) close=([0-9a-f]+) blue=(\d+) marker=([0-9a-f]+) release=pending"
    )
    if (receipt_peers != close_peers or sorted(map(int, receipt_peers)) != result["pids"]
            or received_id != body[0]["original"] or receipt != closed_receipt or close == receipt):
        raise ValueError("native receipt and later PANEL close differ across peers")
    init = bytes.fromhex(next(x["record"] for x in disclosures if x["index"] == 0))
    # The C digest is the observed native input; all other Init fields and
    # every subsequent record are independent of the observed trace bytes.
    context = init[5:69]
    prompt, answer = result["input_tokens"], result["answer_tokens"]
    s = bytes.fromhex(result["attempt"])
    def token_root(kind, tokens):
        return qf1.root(s, kind, [struct.pack("<I", token) for token in tokens])
    rx, ry = token_root(1, prompt), token_root(2, answer)
    records = [envelope(0, context + bytes.fromhex(result["instance"]) + s + rx + struct.pack("<IH", len(prompt), 3))]
    state, tir_count, steps = 0, 0, []
    for pos, token in enumerate(prompt + answer[:-1]):
        state = min(32767, max(-32767, state + (-10000 if token == 7 else 1 + token % 7)))
        base = [min(32767, max(-32767, state + i % 7 - 3)) for i in range(4096)]
        first = len(records)
        records.extend(tiles(pos, 0, 65535, 4, 1, base))
        rows = []
        for layer in range(36):
            row = [min(32767, x + layer + 1) for x in base]
            records.extend(tiles(pos, 1, layer, 1, 1, row))
            rows.append(row)
        if pos + 1 < len(prompt):
            tir_count += len(records) - first
            continue
        norm = [min(32767, x + 41) for x in base]
        steps.append(rows + [norm])
        records.extend(tiles(pos, 2, 65535, 1, 1, norm))
        logits = [0] * 151936
        peak = max(norm)
        logits[7] = logits[11] = peak
        logits[151643] = logits[151645] = -peak
        records.extend(tiles(pos, 2, 65535, 3, 2, logits))
        score = max(logits)
        winner = logits.index(score)
        records.extend(tiles(pos, 2, 65535, 4, 5, [winner]))
        tir_count += len(records) - first
        j = pos + 1 - len(prompt)
        if winner != answer[j]:
            raise ValueError("unexpected greedy output")
        records.append(envelope(2, struct.pack("<IIIiI", j, pos, winner, score, 4)))
    t, n = len(answer), len(prompt) + len(answer) - 1
    records.append(envelope(3, struct.pack("<IIIB", t, n, answer[-1], 0 if answer[-1] in qf1.EOS else 1)))
    chunks = qf1.chunk_records(b"".join(qf1.readout_parts(steps)))
    for c in chunks:
        records.append(envelope(4, struct.pack("<IQHB", c.index, c.consumed, c.length, c.final_flag) + c.chain_after))
    z = qf1.readout(steps)
    rz = qf1.root(s, 3, [z])
    d = len(records) + 1
    records.append(envelope(5, rx + ry + rz + struct.pack("<IIIQIIQ", len(prompt), t, n, tir_count, t, len(chunks), d) + z))
    roots = [rx, ry, rz, qf1.root(s, 4, records)]
    counts = [len(prompt), t, 1, d]
    if ([r.hex() for r in roots], counts, qf1.ticket(s, counts, roots).hex()) != (body[0]["roots"], body[0]["counts"], body[0]["ticket"]):
        raise ValueError("roots, lengths or ticket differ from the reference")
    theta = int.from_bytes(bytes.fromhex(body[0]["target"]), "big")
    if theta != 1 << 255 or qf1.is_winner(bytes.fromhex(body[0]["ticket"]), theta) != body[0]["ticket_meets_target"]:
        raise ValueError("ticket comparison differs from the prespecified fixture threshold")
    indices = sorted(row["index"] for row in disclosures)
    if indices != [0, 64, d - 1]:
        raise ValueError("missing Init, final pre tile or End disclosure")
    for row in disclosures:
        i = row["index"]
        if bytes.fromhex(row["record"]) != records[i] or not qf1.verify_opening(
            s, 4, d, i, records[i], [bytes.fromhex(p) for p in row["siblings"]], roots[3]
        ):
            raise ValueError("disclosure disagrees with the fixture computation")
    binary = re.search(r"^test_binary_sha256=([0-9a-f]{64})$", log, re.MULTILINE)
    return dict(**body[0], tir_tiles=tir_count, readout_chunks=len(chunks), opened_indices=indices,
                context_digest=context.hex(), pids=result["pids"], receipt=receipt, panel_close=close,
                panel_close_blue_score=int(blue), panel_marker=marker, panel_release="pending",
                **({"binary_sha256": binary.group(1)} if binary else {}))


if __name__ == "__main__":
    print(json.dumps(verify(Path(sys.argv[1]).read_text()), indent=2))
