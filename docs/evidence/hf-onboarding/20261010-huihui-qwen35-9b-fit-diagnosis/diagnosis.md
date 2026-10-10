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

**F6 — the hypothesis "a few outlier tokens set the static scale" is wrong; the per-token test refutes it.** The per-position values of the layer-0 sites
over 96 positions (`palw-tir-fidelity --site-windows 0..96 --dump-sites ... --dump-out`; `per-token/token-magnitude.out`): the token's rms at
`L0.gdn.core` and `L0.gdn.normed` is nearly constant (0.035-0.071 and 0.062-0.068; the largest only at position 0), and the relative error does not
fall as 1 / magnitude (log-log slope -0.25 at the core). Magnitude across tokens is not the variable.

**F7 — the variable is the magnitude across HEADS, and the cause is the grid of the gated-delta core.** `L0.gdn.core` is `[32 heads x 128]` and is delivered on ONE
scale per tensor; the gated RMS norm after it is per head (`groups = 32`). The 32 heads' rms differ by a factor of 32,500 within the layer (p10 2.6e-4, median
2.9e-2, p90 0.10; `per-token/head-range.out`). The quantisation noise of the tensor is about the same absolute size in every head (median 8e-5), so a quiet
head sits on one or two code steps (relative core error 0.055 in the quietest quarter, 0.002 in the loudest) and the per-head norm lifts each head to unit scale:
the relative error at `gdn.normed` is **0.996 in the quietest quarter of (token, head) cells, 0.223, 0.041, 0.030** in the quarters above. Ten of the 32 heads
(1, 11, 12, 13, 20, 21, 24, 25, 30, 31) have a median normed error of 0.36-1.00; they carry 1.0 % of the normed energy and **91 % of its squared error**
(`per-token/bad-heads.out`). That is the 0.101 at `L0.gdn.normed`, and the x4.5 growth to `L0.norm.ffn` is the next norm lifting the residual that the bad heads'
noise reached through `out_proj`.

**F8 — the integer norm is exact; the whole error is the grid of its input.** Applying an EXACT float gated norm to the integer core (the dumped codes, decoded)
gives a relative error against the float `gdn.normed` of 0.0831 (energy weighted); the integer program's own `gdn.normed` is 0.0830 and agrees head by head
(`per-token/exact-norm-on-int-core.out`). So neither the integer RMS norm, its eps nor the gate table is at fault: the core's resolution is.

**F9 — the lever, measured.** The core is already an `i32` value on the wire (`gdn_step_q36`'s output); only its scale key is the 16-bit-grid one. Delivering it at the
wide rail's key (`wide_key`: the calibrated absmax at 32-bit resolution with `WIDE_RECURRENT_HEADROOM` = 256, i.e. 21 bits of range) changes only the constants
materialised for the core's output scale and the norm's eps - the node graph is identical (`tests/gdn_core_wide.rs` asserts program bytes equal, tensors different).
Measured against the Hugging Face float32 logits, same 32 positions (`gdn-core-wide-fit.txt`):

| model / depth | default grid | core on the wide grid |
|---|---|---|
| Huihui-9B, 4 layers, headroom16 2.0 | KL 0.0810, corr 0.971, top-1 0.719 | **KL 0.0013, corr 0.99955, top-1 0.938** |
| Huihui-9B, 4 layers, headroom16 1.0 | KL 0.0495, corr 0.982, top-1 0.812 | KL 0.0083, corr 0.9971, top-1 0.844 |
| Qwen3.5-0.8B, 4 layers, headroom16 2.0 | KL 0.0132, corr 0.99766, top-1 0.844 | KL 0.0045, corr 0.99918, top-1 0.906 |
| **Huihui-9B, all 32 layers, 8k-calibration statistics, headroom16 2.0** | **KL 0.26754, slope 0.9451, corr 0.95069, top-1 0.688 (the refused build, reproduced)** | **KL 0.00388, slope 1.0001, corr 0.99933, top-1 0.969** |

The full-depth default row reproduces the refused real build to every printed digit (KL 0.26754, corr 0.95069, top-1 0.688) in 95 s, so the fidelity tool is a faithful
probe of the pack builder's fit. With the lever the 9B is inside the default tolerance (slope [0.97, 1.03], corr >= 0.99, top-1 >= 0.8, KL <= 0.1) with margin on every
number, at the default headroom16 2.0 (the headroom16 = 1.0 of F3 is no longer wanted: with the finer core the clipping side dominates).

## 3. What this does and does not show

Shown: the refusal is a quantisation-quality property of the lowering for this checkpoint, localised to the grid of the gated-delta core feeding a per-head norm
(F6-F8); a lowering option that gives that tensor a finer grid takes the real 32-layer model from KL 0.268 to 0.0039 on the pack builder's own fit measure (F9); the same
option helps the registered-class model Qwen3.5-0.8B at four layers (KL 0.0132 -> 0.0045).

Not shown: that the same holds for unseen text (the fit is 32 positions of two README sequences, as for every model here); anything about any other layer kind (the
ten bad heads were found in layer 0; the option moves every gated-delta layer's core, and the full-depth number is what counts); that the option is the best possible
lever (a per-head scale, or a finer grid for the gated norm's gate, were not tried).

## 4. What changed in the tree (this branch)

* `LowerOpts::gdn_core_wide` (default `false`; with it off every program and artifact is byte for byte as before: `tests/gdn_core_wide.rs` pins the program and the tensors
  against the default, and `palw-class pack verify --rebuild --strict` of the registered-class Qwen3.5-0.8B pack gives the same result on the integration binary and on this
  one). The node graph with it ON is the same program (same bytes); the materialised constants and tensors differ, so the artifact, and its root, are a different class.
* The runtime pack pins it (`profile.gdn_core_wide`, absent when false; the calibration identity hashes under `...calibration-id.v2` ONLY when it is set, v1 otherwise), so
  `pack verify --rebuild` lowers the same way; `palw-class pack build --gdn-core-wide`; `palw-tir-fidelity --gdn-core-wide`.
* `palw-class pack build` accepts pinned statistics together with the calibration sequences they were measured on (`--stats-in` + `--calib`): the statistics are read, the
  sequences give a recurrent model's artifact its `calibrated_context` (the combination `verify --rebuild` already used), so the 8k calibration (5 h) need not be repeated.
* Four-layer prefix, the whole pipeline: `pack build --gdn-core-wide --stats-in --calib` -> fit KL 0.00129 -> `pack verify --rebuild --strict` VERIFIED
  (`pipe-hui-L4-verify.json`).

## 5. Routing

Not armed and not registered. Registrations are immutable (ADR-0175): a class built with the option is a NEW class (a different artifact root) of the same checkpoint,
never a replacement of one already registered. The lowering owner (lane A) should review the option before it is a documented registrant choice; for the 9B the real 8k
build with the option is `../20261011-huihui-qwen35-9b-r2`.
