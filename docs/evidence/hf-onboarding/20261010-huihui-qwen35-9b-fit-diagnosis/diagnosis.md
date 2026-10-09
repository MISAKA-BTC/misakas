# Huihui-Qwen3.5-9B (8k): why the fit gate refused the real build — diagnosis (H1, 2026-10-10)

Status vocabulary: this is a **measurement and a localisation**. It is not a fix; nothing here is verified, armable or registered.
Registration level of the model stays **L1** (the 8k artifact exists only as a refused build); the L2 gate is `pack verify --strict
--rebuild` VERIFIED with the default tolerance (slope in [0.97, 1.03], corr >= 0.99, top-1 >= 0.8, mean KL <= 0.1), and that
tolerance is not loosened to pass.

## 0. The real build (record: `../20261008-huihui-qwen35-9b-r1/failures.json`, last entry)

One calibration sequence of 8,192 positions (916 sites, 18,360 s), conversion of 2,640 tensors / 9,649.2 MiB (artifact b4494df7fe2e562c),
streamed conformance, then `slope 0.9563, corr 0.95069, top-1 0.688, KL 0.26754` against the float32 Hugging Face reference: the pack
builder refuses to write the pack. The predecessor's WIP recorded this as an infrastructure failure from the `time(1)` footer; it is a
fit refusal and is now recorded as such (`CONFORMANCE_FAILED / HF_FIT_OUT_OF_TOLERANCE`).

## 1. Method: a four-layer prefix of the real checkpoint, one calibration sequence of 512, the same 32 evaluation positions

`tests/hf-onboarding/make_prefix_checkpoint.py` writes the first N decoder layers (embedding, final norm and head kept) of the pinned
checkpoint; the fit is measured on the same two 16-token README sequences as the real build, against the same streamed HF reference.
Two probes: `palw-class pack build --stats-in` (12 min per run, the pack builder's own fit) and `palw-tir-fidelity --exec` (2 min per run;
this lane added `--logits-out`, `--headroom32`, `--headroom-resid` to it; the dumps are compared by `tests/hf-onboarding/kl_compare.py`).
Scripts: `diag-knob.sh`, `fid-sweep.sh` (this directory).

## 2. Findings

**F1 — the lowering's float program is the checkpoint's.** The float reference of the lowering against the HF logits over the 32
positions: slope 1.0000, corr 1.00000, top-1 1.000, KL 0.00000, max |d| < 0.0005. So the 9B's loss is integer quantisation, not a
semantic mismatch of the Qwen3.5 gated-delta / attention programs (that route is what the 0.8B, registered at L4, also runs).

**F2 — W8 per-row weight error does not separate the two models.** Relative per-row int8 error of every decoder weight tensor of
layers 0-3: median 0.0092 (0.8B) vs 0.0100 (9B); the worst tensors are the same kinds (`in_proj_b`, `down_proj`) at 0.010-0.019 in both.

**F3 — the A16 activation headroom is a real lever, and the default is not the optimum for the 9B.** Four-layer prefix, same
statistics, `headroom16` swept (`headroom16-sweep-9b-L4.log`): 0.25 KL 1.198, 0.5 KL 0.563, 0.75 KL 0.130, **1.0 KL 0.049 (top-1 0.812,
corr 0.982)**, 1.5 KL 0.078, **2.0 (default) KL 0.081 (top-1 0.719, corr 0.971)**; the pack builder agrees (1.0: slope 0.9892, corr
0.98242, top-1 0.812, KL 0.04946; 4.0: corr 0.95111, KL 0.14141). Below 1.0 the calibrated peaks clip; above it the bulk loses
resolution. Even at the optimum the four-layer prefix is below the tolerance on correlation (0.982 < 0.99) and at the top-1 edge, so
tuning this one knob should not be expected to make the 32-layer model pass: under the default policy the correlation already falls from 0.971 at 4 layers to 0.951 at 32 (the real build).

**F4 — the first large error is in layer 0, not accumulated.** Per-site relative error (`||int - float|| / ||float||`, positions 0-16 of
the first sequence; files `sites-*.json`):

| site | 0.8B (headroom 2.0) | 9B (headroom 1.0) |
|---|---|---|
| `pre.embed` | 0.0001 | 0.0001 |
| `L0.gdn.core` | 0.0064 | 0.0040 |
| `L0.gdn.normed` | 0.029 | **0.101** |
| `L0.resid.mix` | 0.049 | 0.072 |
| `L0.norm.ffn` | 0.055 | **0.324** |
| `L0.mlp.hidden` | 0.071 | **0.340** |

`L0.norm.mix` (the input norm of the same layer) is 0.0003 in the 9B; the error appears after the gated-delta output is added to the
residual and is multiplied about 4.5x by the RMS norm that feeds the MLP (in the 0.8B that step is x1.1). Per position
(`sites-9b-L4-h1.0-per-position.json`) it is uniform, not a first-token effect: `L0.norm.ffn` 0.17-0.51 at all 16 positions, and the
logit error is not concentrated at positions 0 and 16 either (KL 0.019 and 0.010 there; KL 0.0518 over the rest).

**F5 — the 9B's sites have a wider dynamic range.** Calibration absmax / rms per site, layers 0-3: median 30, p90 229 (9B) against
median 18, p90 73 (0.8B); over the 9B's whole 8k statistics median 30, p90 164. Worst: `L6.mlp.hidden` 1,100, `L2.gdn.normed` 785.
About 10 % of the 9B's sites (88 of 884) are first-token-dominated (pos0 absmax > 1.5 x the rest; median x1.9, max x8.2), so a
first-token-only clip would reach a minority of the sites.

## 3. What this does and does not show

Shown: the refusal is a quantisation-quality property of the lowering for this checkpoint, reproducible at four layers in 2 minutes;
the optimum `headroom16` cuts the four-layer KL by 40 % against the default; the error enters at layer 0 where the gated-delta branch meets the
residual and the following RMS norm.

Not shown: which operation in that path is responsible (an ablation that forces one site to exact float is the next test and lives in
the lowering lane); whether `headroom16 = 1.0` at 32 layers is any better than the default (not measured: a full-depth fit costs about
2 h and the four-layer result already says it cannot reach the tolerance); any effect on a registered model (none was touched).

## 4. Routing

`CONFORMANCE_FAILED / HF_FIT_OUT_OF_TOLERANCE`, owner A (lowering quality) / H1 (calibration policy). Candidate levers for the Lead's
decision (none is armed or implemented here): (a) a pack-recorded `headroom16 = 1.0` profile for the 9B (a policy value the pack already
pins and `verify --rebuild` honours; clipping risk on unseen prompts is the cost); (b) an ablation lane on `L0.gdn.normed -> resid -> norm.ffn`;
(c) wider (i32) sites on that path, which changes the program and therefore any NEW registration's root, never a registered one (ADR-0175).
