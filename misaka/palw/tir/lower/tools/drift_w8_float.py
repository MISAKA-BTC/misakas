# Usage (offline, one heavy run at a time): python tools/drift_w8_float.py <checkpoint> <drift.json> <positions> "all;x_proj;all-but:x_proj,dt_proj"  (freeze-v1 §5.2)
# The drift of the float model with per-row quantised weights, one variant per fresh load (no
# second copy of the weights in memory). Variant syntax: "all" (every weight at 8 bits),
# "a,b" (only params whose name contains a or b at 8 bits), "all-but:a,b" (8 bits except those,
# which get 16). Gathered tables and a tied head are always 16 bits.
import json, sys, gc, torch
from transformers import AutoModelForCausalLM
ck, drift = sys.argv[1], sys.argv[2]
T = int(sys.argv[3])
variants = sys.argv[4].split(";")
seq = json.load(open(drift))["sequences"][0][:T]
torch.set_grad_enabled(False)
def load():
    m = AutoModelForCausalLM.from_pretrained(ck, dtype=torch.float32)
    m.eval()
    return m
def q_rows(W, bits):
    lim = 2 ** (bits - 1) - 1
    R = W.reshape(W.shape[0], -1)
    s = R.abs().amax(-1, keepdim=True).clamp_min(1e-12) / lim
    return ((R / s).round().clamp(-lim, lim) * s).reshape(W.shape)
def hidden(m):
    base = m.model if hasattr(m, "model") else m.backbone
    out = base(torch.tensor([seq]), use_cache=False)
    return out.last_hidden_state[0] if hasattr(out, "last_hidden_state") else out[0][0]
def logits_of(h, W):
    return torch.cat([h[i:i + 128] @ W.T for i in range(0, h.shape[0], 128)])
m = load()
ref_h = hidden(m)
ref_W = m.get_output_embeddings().weight.detach().clone()
del m; gc.collect()
windows = [(64, 192), (T - 128, T)]
ref_lp = [torch.log_softmax(logits_of(ref_h[lo:hi], ref_W), -1) for lo, hi in windows]
table = lambda n: "embed" in n or "lm_head" in n
for v in variants:
    m = load()
    but = v.startswith("all-but:")
    names = v.split(":", 1)[1].split(",") if but else v.split(",")
    for n, p in m.named_parameters():
        if p.dim() < 2:
            continue
        if table(n):
            p.copy_(q_rows(p.detach(), 16))
        elif but:
            p.copy_(q_rows(p.detach(), 16 if any(t in n for t in names) else 8))
        elif v == "all" or any(t in n for t in names):
            p.copy_(q_rows(p.detach(), 8))
    h = hidden(m)
    W = m.get_output_embeddings().weight.detach()
    kl = []
    for (lo, hi), lf in zip(windows, ref_lp):
        lq = torch.log_softmax(logits_of(h[lo:hi], W), -1)
        kl.append((lf.exp() * (lf - lq)).sum(-1).mean().item())
    print(f"W8 on [{v}]: KL 64-192 {kl[0]:.5f}  {T-128}-{T} {kl[1]:.5f}  (x{kl[1]/kl[0]:.2f})", flush=True)
    del m, h, W; gc.collect()
