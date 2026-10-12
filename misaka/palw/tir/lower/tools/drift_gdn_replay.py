# Usage (offline, one heavy run at a time): python tools/drift_gdn_replay.py <checkpoint> <drift.json> <stats.json> <layers, e.g. 0,8,16,22> [positions]  (freeze-v1 §5.2)
# Does the gated-delta step's integer arithmetic accumulate error along a 4,096-token sequence?
# Capture each chosen GDN layer's (q, k, v, g, beta) from transformers' float run, then replay the
# recurrence in float64 exactly and with the lowering's roundings (tir_library_v1 gdn_step_q36:
# q/k unit codes at 2^-15, v's A16 code grid, the read w and the delta's RSR on v's grid, beta and
# the decay in Q24, u narrowed to the delta's 24-bit grid), and with finer grids for w/RSR and v.
import json, sys, numpy as np, torch
from transformers import AutoModelForCausalLM
import transformers.models.qwen3_5.modeling_qwen3_5 as mq
ck, drift, stats_f = sys.argv[1], sys.argv[2], sys.argv[3]
LAYERS = [int(x) for x in sys.argv[4].split(",")]
T = int(sys.argv[5]) if len(sys.argv) > 5 else 4096
seq = json.load(open(drift))["sequences"][0][:T]
stats = json.load(open(stats_f))
torch.set_grad_enabled(False)
m = AutoModelForCausalLM.from_pretrained(ck, torch_dtype=torch.float32)
m.eval()
cur = {"layer": None}
cap = {}
layers = m.model.layers if hasattr(m.model, "layers") else m.model.language_model.layers
for i, lyr in enumerate(layers):
    if hasattr(lyr, "linear_attn"):
        lyr.linear_attn.register_forward_pre_hook(lambda mod, a, i=i: cur.__setitem__("layer", i))
orig = mq.torch_chunk_gated_delta_rule
def patched(query, key, value, g, beta, **kw):
    if cur["layer"] in LAYERS:
        cap[cur["layer"]] = [x[0].detach().double().numpy().copy() for x in (query, key, value, g, beta)]
    return orig(query, key, value, g=g, beta=beta, **kw)
mq.torch_chunk_gated_delta_rule = patched
base = m.model
base(torch.tensor([seq]), use_cache=False)
del m, base
print("captured layers", sorted(cap), flush=True)

def l2n(x):
    return x / np.sqrt((x * x).sum(-1, keepdims=True) + 1e-6)

def code_scale(absmax, maxc, headroom):
    return max(absmax * headroom, 1e-30) / maxc

def run(q, k, v, g, beta, mode, sv, su):
    # q, k: [T, H, dk] (repeated to v heads), v: [T, H, dv], g, beta: [T, H]
    Tn, H, dk = k.shape
    dv = v.shape[-1]
    qn = l2n(q); kn = l2n(k)
    qs = 1.0 / np.sqrt(dk)
    S = np.zeros((H, dk, dv))
    out = np.zeros((Tn, H, dv))
    fw = mode.get("fw", 0); fv = mode.get("fv", 0)
    for t in range(Tn):
        dec = np.exp(g[t])
        b = beta[t]
        kt, qt, vt = kn[t], qn[t], v[t]
        if mode["int"]:
            kt = np.round(kt * 32768) / 32768
            qt = np.round(qt * 32768) / 32768
            dec = np.round(dec * 2**24) / 2**24
            b = np.round(b * 2**24) / 2**24
            gv = sv / 2**fv
            vt = np.round(vt / gv) * gv
        S = S * dec[:, None, None]
        w = np.einsum("hkv,hk->hv", S, kt)
        if mode["int"]:
            gw = sv / 2**fw
            w = np.round(w / gw) * gw
            d = np.round((vt - w) * b[:, None] / gw) * gw
            d = np.round(d / su) * su
        else:
            d = (vt - w) * b[:, None]
        S = S + kt[:, :, None] * d[:, None, :]
        out[t] = np.einsum("hkv,hk->hv", S, qt) * qs
    return out

def win(a, r, lo, hi):
    return np.linalg.norm(a[lo:hi] - r[lo:hi]) / np.linalg.norm(r[lo:hi])

E, Lw = (64, 192), (T - 128, T)
for L in sorted(cap):
    q, k, v, g, beta = cap[L]
    sv = code_scale(stats[f"L{L}.gdn.v"]["absmax"], 32767, 2.0)
    su = code_scale(stats[f"L{L}.gdn.core.delta"]["absmax"], 2**24 - 1, 2.0)
    ref = run(q, k, v, g, beta, {"int": False}, sv, su)
    print(f"L{L}: v absmax {stats[f'L{L}.gdn.v']['absmax']:.3g} (grid {sv:.3g}), delta grid {su:.3g}; |o| rms {np.sqrt((ref**2).mean()):.3g}", flush=True)
    for name, mode in [("lowering today (v, w, RSR on v's grid)", {"int": True}),
                       ("w and RSR 8 bits finer", {"int": True, "fw": 8}),
                       ("w, RSR and v 8 bits finer", {"int": True, "fw": 8, "fv": 8})]:
        o = run(q, k, v, g, beta, mode, sv, su)
        e, l = win(o, ref, *E), win(o, ref, *Lw)
        print(f"   {name:42s} rel err {E[0]}-{E[1]}: {e:.3e}   {Lw[0]}-{Lw[1]}: {l:.3e}   (x{l/e:.2f})", flush=True)
