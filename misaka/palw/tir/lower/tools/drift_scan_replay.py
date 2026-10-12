# Usage (offline, one heavy run at a time): python tools/drift_scan_replay.py <checkpoint> <drift.json> <stats.json> <layers, e.g. 0,16,32,47> [positions]  (freeze-v1 §5.2)
# Mamba-1's selective scan over 4,096 positions: float64 vs the lowering's roundings (x, B, C on
# their A16 code grids, Δ and the decay in Q24), and with finer grids for the scan's inputs.
import json, sys, numpy as np, torch
from transformers import AutoModelForCausalLM
ck, drift, stats_f = sys.argv[1], sys.argv[2], sys.argv[3]
LAYERS = [int(x) for x in sys.argv[4].split(",")]
T = int(sys.argv[5]) if len(sys.argv) > 5 else 4096
seq = json.load(open(drift))["sequences"][0][:T]
stats = json.load(open(stats_f))
torch.set_grad_enabled(False)
m = AutoModelForCausalLM.from_pretrained(ck, dtype=torch.float32)
m.eval()
cap = {}
mod_dt = {i: l.mixer.dt_proj for i, l in enumerate(m.backbone.layers)}
N = m.config.state_size
R = m.config.time_step_rank
for i, lyr in enumerate(m.backbone.layers):
    if i not in LAYERS:
        continue
    mx = lyr.mixer
    def xh(mod, a, out, i=i):
        cap.setdefault(i, {})["x"] = a[0][0].double().numpy().copy()
        o = out[0].double().numpy()
        cap[i]["B"] = o[:, R:R + N].copy(); cap[i]["C"] = o[:, R + N:].copy()
        ts = out[0][:, :R].double()
        w, b = mod_dt[i].weight.double(), mod_dt[i].bias.double()
        cap[i]["dt"] = torch.nn.functional.softplus(ts @ w.T + b).numpy().copy()
    mx.x_proj.register_forward_hook(xh)
    cap.setdefault(i, {})["A"] = (-torch.exp(mx.A_log.float())).double().numpy().copy()
    cap[i]["D"] = mx.D.double().numpy().copy()
m.backbone(torch.tensor([seq]), use_cache=False)
del m
print("captured", sorted(cap), flush=True)

def grid(key):
    return max(stats[key]["absmax"] * 2.0, 1e-30) / 32767

def run(c, mode, gx, gb, gc):
    x, B, C, dt, A, D = c["x"], c["B"], c["C"], c["dt"], c["A"], c["D"]
    h = np.zeros(A.shape)
    y = np.zeros(x.shape)
    fx, fb, fc = mode.get("fx", 0), mode.get("fb", 0), mode.get("fc", 0)
    for t in range(x.shape[0]):
        xt, bt, ct, d = x[t], B[t], C[t], dt[t]
        if mode["int"]:
            xt = np.round(xt / (gx / 2**fx)) * (gx / 2**fx)
            bt = np.round(bt / (gb / 2**fb)) * (gb / 2**fb)
            ct = np.round(ct / (gc / 2**fc)) * (gc / 2**fc)
            d = np.round(d * 2**24) / 2**24
            dec = np.round(np.exp(d[:, None] * A) * 2**24) / 2**24
        else:
            dec = np.exp(d[:, None] * A)
        h = dec * h + (d * xt)[:, None] * bt[None, :]
        y[t] = h @ ct + D * xt
    return y

def win(a, r, lo, hi):
    return np.linalg.norm(a[lo:hi] - r[lo:hi]) / np.linalg.norm(r[lo:hi])

E, Lw = (64, 192), (T - 128, T)
for L in sorted(cap):
    c = cap[L]
    gx, gb, gc = grid(f"L{L}.mamba.conv"), grid(f"L{L}.mamba.B"), grid(f"L{L}.mamba.C")
    ref = run(c, {"int": False}, gx, gb, gc)
    print(f"L{L}: grids x {gx:.3g} B {gb:.3g} C {gc:.3g}; |y| rms {np.sqrt((ref**2).mean()):.3g}", flush=True)
    for name, mode in [("lowering today (x, B, C on A16 grids)", {"int": True}),
                       ("x 8 bits finer", {"int": True, "fx": 8}),
                       ("x and B 8 bits finer", {"int": True, "fx": 8, "fb": 8}),
                       ("x, B and C 8 bits finer", {"int": True, "fx": 8, "fb": 8, "fc": 8})]:
        o = run(c, mode, gx, gb, gc)
        e, l = win(o, ref, *E), win(o, ref, *Lw)
        print(f"   {name:40s} rel err {E[0]}-{E[1]}: {e:.3e}   {Lw[0]}-{Lw[1]}: {l:.3e}   (x{l/e:.2f})", flush=True)
