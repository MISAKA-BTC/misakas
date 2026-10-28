#!/usr/bin/env python3
"""reorgcheck.py E WINNER SFX SPLIT_DAA — did every claim that ENDED (Final, voided) or was REBOUND on
the winning side DURING the split end the same way after the join?

Inputs (written by `drill.sh reorg`): E/claims{SFX}-A.json and -B.json (each side's view just before
the join) and E/claims{SFX}-post.json (node 0 after all seven nodes share one sink). SPLIT_DAA is n0's
virtual DAA when the partition began (also in E/fork.txt). The winner is the side whose selected chain
survived. A claim counts as having ended during the split only when its terminal phase was reached (or
its rebind happened) at or after SPLIT_DAA — a claim that was already Final before the split proves
nothing about it. Each such claim must carry, after the join, the winner's phase, void reason, quanta,
rebind height and execution credit: a second copy of a Final's mint, a forfeiture applied twice or a
rebind counted on both branches all show up here as a difference. Exit 1 when anything differs, or when
nothing ended during the split (the split then straddled nothing and proved nothing about it).
"""
import json
import sys

E, w, sfx = sys.argv[1], sys.argv[2], sys.argv[3]
split = int(sys.argv[4]) if len(sys.argv) > 4 and sys.argv[4].strip().isdigit() else None
win = {r["claimId"]: r for r in json.load(open(f"{E}/claims{sfx}-{w}.json"))["claims"]}
lose = {r["claimId"]: r for r in json.load(open(f"{E}/claims{sfx}-{'B' if w == 'A' else 'A'}.json"))["claims"]}
post = {r["claimId"]: r for r in json.load(open(f"{E}/claims{sfx}-post.json"))["claims"]}


def ended_in_split(r):
    if split is None:  # no split height recorded: the old, weaker reading (every terminal claim)
        return r.get("phase") in ("final", "voided") or bool(r.get("reboundDaa"))
    terminal = r.get("phase") in ("final", "voided") and int(r.get("phaseDaa") or 0) >= split
    rebound = r.get("reboundDaa") is not None and int(r.get("reboundDaa") or 0) >= split
    return terminal or rebound


ended = [c for c, r in win.items() if ended_in_split(r)]
bad = []
for c in ended:
    p, r = post.get(c), win[c]
    if p is None:
        bad.append((c[:16], "missing after the join"))
        continue
    if r.get("phase") in ("final", "voided") and p.get("phase") != r.get("phase"):
        bad.append((c[:16], "phase", r.get("phase"), p.get("phase")))
    for k in ("quanta", "voidReason", "reboundDaa", "escrowSompi") + (("execCredit",) if r.get("phase") == "final" else ()):
        if r.get("phase") in ("final", "voided") and p.get(k) != r.get(k):
            bad.append((c[:16], k, r.get(k), p.get(k)))
    if r.get("reboundDaa") is not None and p.get("reboundDaa") != r.get("reboundDaa"):
        bad.append((c[:16], "reboundDaa", r.get("reboundDaa"), p.get("reboundDaa")))
# claims the LOSING side ended during the split that the winner had not: reverted by the join. Shown, not
# judged — after the join they may legitimately end again on the winning branch.
reverted = [c for c, r in lose.items() if ended_in_split(r) and not ended_in_split(win.get(c, {}))]
print(f"split at DAA {split}")
print("ended/rebound on the winning side during the split:", [c[:16] for c in ended])
print("ended only on the losing side during the split (reverted by the join):", [c[:16] for c in reverted])
print("changed after the join:", sorted(set(bad)) or "none")
sys.exit(1 if bad or not ended else 0)
