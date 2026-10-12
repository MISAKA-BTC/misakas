#!/usr/bin/env python3
"""Teacher-forced integer logits (`palw-tir-generate --logits-out`) against HF's (`audit_e2e.py`):
top-1 agreement and mean KL(HF ‖ integer). Prints one JSON line.

    python tools/audit_compare.py DIR     (reads DIR/hf-logits.f32, DIR/int.f32, DIR/hf.json)
"""
import json
import sys

import numpy as np

d = sys.argv[1]
v = json.load(open(f"{d}/hf.json"))["vocab"]
hf = np.fromfile(f"{d}/hf-logits.f32", dtype="<f4").astype(np.float64).reshape(-1, v)
it = np.fromfile(f"{d}/int.f32", dtype="<f4").astype(np.float64).reshape(-1, v)
assert hf.shape == it.shape, (hf.shape, it.shape)


def logsm(x):
    m = x.max(-1, keepdims=True)
    return x - m - np.log(np.exp(x - m).sum(-1, keepdims=True))


lp, lq = logsm(hf), logsm(it)
kl = (np.exp(lp) * (lp - lq)).sum(-1)
agree = hf.argmax(-1) == it.argmax(-1)
top1 = float(agree.mean())
# The integer program's noise at a row: the RMS difference of its logits from HF's.
noise = np.sqrt(((hf - it) ** 2).mean(-1))
srt = np.sort(hf, -1)
margin = srt[:, -1] - srt[:, -2]
tf_tie = float((margin[~agree] / noise[~agree]).max()) if (~agree).any() else 0.0
# Greedy departures, judged by HF's margin there against the median noise.
gen = json.load(open(f"{d}/gen.json"))
recs = json.load(open(f"{d}/hf.json"))["records"]
med = float(np.median(noise))
greedy_tie = 0.0
for r, g in zip(recs, gen["records"]):
    k = g.get("identical_prefix", len(r["generated"]))
    if k < len(r["generated"]):
        greedy_tie = max(greedy_tie, r["margins"][k] / med)
print(json.dumps({"positions": int(hf.shape[0]), "top1": top1, "kl_mean": float(kl.mean()), "kl_max": float(kl.max()),
                  "tf_tie": tf_tie, "greedy_tie": greedy_tie, "noise_median": med, "margin_median": float(np.median(margin))}))
