#!/usr/bin/env python3
"""Generate the reduced SD3 / MMDiT fixture and its float reference (RFC-0003 §6, activation step 6).

The fixture is a *diffusers 0.40* text-to-image pipeline at a size a unit test can lower, run in
integers and hold to a float run: tiny seeded random configs for the three components, the
`FlowMatchEulerDiscreteScheduler`'s own sigma table, and a reference that is the pipeline's own
denoise loop fed the INTEGER run's noise. Nothing is read from a hub (`HF_HUB_OFFLINE=1`; no
`from_pretrained` on a hub id) and no tokenizer exists: the harness feeds prompt ids, and the
pipeline's `prompt_embeds` path is the reference's.

    tests/fixtures/hf-diff/sd3_tiny/transformer/{config.json,diffusion_pytorch_model.safetensors}  (BF16)
    tests/fixtures/hf-diff/sd3_tiny/text/{config.json,model.safetensors}                           (BF16)
    tests/fixtures/hf-diff/sd3_tiny/vae/{config.json,diffusion_pytorch_model.safetensors}          (BF16)
    tests/fixtures/hf-diff/sd3_tiny/scheduler/scheduler_config.json, sigmas.json

The host's watchdog kills a Python process over 12 GB, so the discipline is the one the fixtures
before this follow, tightened for diffusers: **parameter counts are computed on the meta device first**
(`counts`), **one model per subprocess** (`build <component>` is one process: a driver runs the three
in turn), and `reference` loads only what it runs, in float32, one component at a time where it can.

Usage (every command with `HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1`, from the crate directory):

    python tools/gen_diffusers_sd3_fixture.py counts
    python tools/gen_diffusers_sd3_fixture.py build transformer
    python tools/gen_diffusers_sd3_fixture.py build text
    python tools/gen_diffusers_sd3_fixture.py build vae
    python tools/gen_diffusers_sd3_fixture.py scheduler
    python tools/gen_diffusers_sd3_fixture.py reference <request.json> <out.json>

`reference`'s request (the integer run's inputs, written by the Rust harness):

    {"ids": [..the padded template: bos, prompt ids, eos, pad.. (the class's length)..], "steps": 4,
     "noise": [... 4*8*8 floats, C-major (c, h, w): the Q24 Gaussian words / 2^24 ...],
     "sigmas": [... steps+1 floats ... the integer run's table, to check against the scheduler's ...]}

and its output carries, per step, the latent after the step and the model's velocity, the text
encoder's `prompt_embeds` (final-norm rows) and pooled vector, and the final image as
u8 HWC after the pipeline's own post-processing.
"""

import json
import math
import os
import sys

import numpy as np
import torch

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from gen_hf_fixtures import randomise  # noqa: E402

CRATE = os.path.dirname(HERE)
FIX = os.path.join(CRATE, "tests", "fixtures", "hf-diff", "sd3_tiny")

# --- the reduced configuration (RFC-0003 §6, "The fixture") ----------------------------------------
LATENT_C, LATENT_HW, PATCH = 4, 8, 2
HEADS, HEAD_DIM = 4, 16
INNER = HEADS * HEAD_DIM  # 64
TEXT_HIDDEN, TEXT_VOCAB, TEXT_LAYERS = 32, 64, 2
EOS_ID = 63
STEPS_OFFERED = [2, 4]
SHIFT = 3.0
NUM_TRAIN_TIMESTEPS = 1000

TRANSFORMER = dict(
    sample_size=LATENT_HW,
    patch_size=PATCH,
    in_channels=LATENT_C,
    out_channels=LATENT_C,
    num_layers=2,
    attention_head_dim=HEAD_DIM,
    num_attention_heads=HEADS,
    joint_attention_dim=TEXT_HIDDEN,
    caption_projection_dim=INNER,
    pooled_projection_dim=TEXT_HIDDEN,
    # A table larger than the grid, so the position table is cropped from its centre as the real model's is.
    pos_embed_max_size=8,
    dual_attention_layers=(),
    qk_norm=None,
)
TEXT = dict(
    vocab_size=TEXT_VOCAB,
    hidden_size=TEXT_HIDDEN,
    intermediate_size=64,
    num_hidden_layers=TEXT_LAYERS,
    num_attention_heads=4,
    max_position_embeddings=16,
    projection_dim=TEXT_HIDDEN,
    hidden_act="quick_gelu",
    layer_norm_eps=1e-5,
    eos_token_id=EOS_ID,
    bos_token_id=62,
    pad_token_id=EOS_ID,
)
VAE = dict(
    in_channels=3,
    out_channels=3,
    down_block_types=("DownEncoderBlock2D", "DownEncoderBlock2D"),
    up_block_types=("UpDecoderBlock2D", "UpDecoderBlock2D"),
    block_out_channels=(8, 16),
    layers_per_block=1,
    latent_channels=LATENT_C,
    norm_num_groups=4,
    sample_size=2 * LATENT_HW,
    scaling_factor=1.5305,
    shift_factor=0.0609,
    use_quant_conv=False,
    use_post_quant_conv=False,
    force_upcast=False,
)
IMAGE_HW = 2 * LATENT_HW  # the decoder doubles the latent once (two blocks, one upsample)

COMPONENTS = {
    "transformer": ("diffusion_pytorch_model.safetensors", lambda: _transformer(), TRANSFORMER),
    "text": ("model.safetensors", lambda: _text(), TEXT),
    "vae": ("diffusion_pytorch_model.safetensors", lambda: _vae(), VAE),
}


def _transformer():
    from diffusers import SD3Transformer2DModel

    return SD3Transformer2DModel(**TRANSFORMER)


def _text():
    import transformers

    return transformers.CLIPTextModelWithProjection(transformers.CLIPTextConfig(**TEXT))


def _vae():
    from diffusers import AutoencoderKL

    return AutoencoderKL(**VAE)


def seed_of(name):
    return sum(ord(ch) for ch in "sd3_tiny/" + name)


def count_params(name):
    """Parameter and buffer element counts, on the meta device: nothing is allocated."""
    with torch.device("meta"):
        model = COMPONENTS[name][1]()
    params = sum(p.numel() for p in model.parameters())
    buffers = sum(b.numel() for b in model.buffers())
    return {"params": params, "buffers": buffers, "bytes_bf16": 2 * (params + buffers)}


def cmd_counts():
    out = {}
    for name in COMPONENTS:
        out[name] = count_params(name)
    out["total_params"] = sum(v["params"] for v in out.values() if isinstance(v, dict))
    print(json.dumps(out, indent=2))


def randomise_diffusers(model, seed):
    """`randomise` for a model whose 2-D+ weights are convs and linears alike: O(1)-scaled, fan-in normalised.

    `gen_hf_fixtures.randomise` scales every matrix by the model's hidden width, which is right for a
    transformer and wrong for a convolution (its fan-in is `in_channels · k²`); a norm's gain and
    shift keep their initial values with the same ±0.1 noise the other fixtures give them.
    """
    g = torch.Generator().manual_seed(seed + 1)
    sd_names = set(model.state_dict().keys())
    with torch.no_grad():
        for name, p in model.named_parameters():
            if not p.is_floating_point():
                continue
            if p.ndim >= 2:
                fan_in = int(np.prod(p.shape[1:]))
                p.copy_(torch.randn(p.shape, generator=g) / math.sqrt(max(fan_in, 1)))
            else:
                p.add_(torch.randn(p.shape, generator=g) * 0.1)
            p.copy_(p.to(torch.bfloat16).to(torch.float32))
        for name, b in model.named_buffers():
            if name in sd_names and b.is_floating_point():
                b.add_(torch.randn(b.shape, generator=g) * 0.1)
                b.copy_(b.to(torch.bfloat16).to(torch.float32))


def cmd_build(name):
    if name not in COMPONENTS:
        sys.exit(f"unknown component {name}: one of {sorted(COMPONENTS)}")
    counts = count_params(name)
    print(f"{name}: {counts['params']:,} parameters, {counts['buffers']:,} buffer elements (meta device)")
    if counts["params"] > 50_000_000:
        sys.exit(f"{name} is {counts['params']:,} parameters: this is a tiny-config generator, refusing")
    seed = seed_of(name)
    torch.manual_seed(seed)
    model = COMPONENTS[name][1]()
    model.eval()
    if name == "text":
        randomise(model, TEXT_HIDDEN, seed)
    else:
        randomise_diffusers(model, seed)
    d = os.path.join(FIX, name)
    os.makedirs(d, exist_ok=True)
    model.to(torch.bfloat16)
    model.save_pretrained(d)
    print(f"wrote {d}")


def cmd_scheduler():
    from diffusers import FlowMatchEulerDiscreteScheduler

    d = os.path.join(FIX, "scheduler")
    os.makedirs(d, exist_ok=True)
    sched = FlowMatchEulerDiscreteScheduler(num_train_timesteps=NUM_TRAIN_TIMESTEPS, shift=SHIFT)
    sched.save_pretrained(d)
    tables = {}
    for steps in STEPS_OFFERED:
        s = FlowMatchEulerDiscreteScheduler(num_train_timesteps=NUM_TRAIN_TIMESTEPS, shift=SHIFT)
        s.set_timesteps(steps)
        tables[str(steps)] = {
            "sigmas": [float(x) for x in s.sigmas.to(torch.float64).tolist()],
            "timesteps": [float(x) for x in s.timesteps.to(torch.float64).tolist()],
        }
    with open(os.path.join(d, "sigmas.json"), "w") as f:
        json.dump({"shift": SHIFT, "num_train_timesteps": NUM_TRAIN_TIMESTEPS, "steps": tables}, f, indent=1)
    print(f"wrote {d}")


def load(name, dtype=torch.float32):
    d = os.path.join(FIX, name)
    if name == "transformer":
        from diffusers import SD3Transformer2DModel

        m = SD3Transformer2DModel.from_pretrained(d, torch_dtype=dtype, local_files_only=True)
    elif name == "vae":
        from diffusers import AutoencoderKL

        m = AutoencoderKL.from_pretrained(d, torch_dtype=dtype, local_files_only=True)
    else:
        import transformers

        m = transformers.CLIPTextModelWithProjection.from_pretrained(d, dtype=dtype, attn_implementation="eager")
    m.eval()
    return m


def encode_prompt(ids):
    """The text component's contribution: `prompt_embeds` and the pooled `text_embeds`, from the harness's ids.

    `ids` is the class's padded template (`bos ‖ prompt ‖ eos ‖ pad…`, to the class's length). `prompt_embeds` is
    the encoder's `last_hidden_state` (final-norm rows) — NOT the penultimate hidden states SD3's own pipeline takes
    from its CLIPs: the first fixture's integer text stage is the HF frontend's `Rows` program, whose output is the
    final-norm row (a truncated-depth `Rows` program is a later refinement; the denoiser reads whatever tensor the
    pipeline hands it, and both runs hand it the same). The pooled vector is `text_embeds`, read at the FIRST `eos`
    (transformers' pooling), which is the last position of the integer pooled stage's unpadded template."""
    model = load("text")
    x = torch.tensor([ids], dtype=torch.long)
    with torch.no_grad():
        out = model(input_ids=x)
    prompt_embeds = out.last_hidden_state  # [1, L, hidden]
    pooled = out.text_embeds  # [1, projection]
    del model
    return prompt_embeds, pooled


def cmd_reference(request_path, out_path):
    from diffusers import FlowMatchEulerDiscreteScheduler

    with open(request_path) as f:
        req = json.load(f)
    ids, steps = req["ids"], int(req["steps"])
    if steps not in STEPS_OFFERED:
        sys.exit(f"steps {steps} is not one of the offered {STEPS_OFFERED}")
    prompt_embeds, pooled = encode_prompt(ids)

    sched = FlowMatchEulerDiscreteScheduler(num_train_timesteps=NUM_TRAIN_TIMESTEPS, shift=SHIFT)
    sched.set_timesteps(steps)
    sigmas = [float(x) for x in sched.sigmas.to(torch.float64).tolist()]
    if "sigmas" in req:
        worst = max(abs(a - b) for a, b in zip(sigmas, req["sigmas"]))
        print(f"sigma table: the scheduler's vs the request's, worst difference {worst:.3e}")

    noise = torch.tensor(req["noise"], dtype=torch.float32).reshape(1, LATENT_C, LATENT_HW, LATENT_HW)
    latents = noise.clone()
    transformer = load("transformer")
    steps_out = []
    with torch.no_grad():
        for i, t in enumerate(sched.timesteps):
            timestep = t.expand(latents.shape[0])
            velocity = transformer(
                hidden_states=latents,
                timestep=timestep,
                encoder_hidden_states=prompt_embeds,
                pooled_projections=pooled,
                return_dict=False,
            )[0]
            latents = sched.step(velocity, t, latents, return_dict=False)[0]
            steps_out.append(
                {
                    "step": i,
                    "sigma": sigmas[i],
                    "timestep": float(t),
                    "velocity": velocity.flatten().tolist(),
                    "latent": latents.flatten().tolist(),
                }
            )
    del transformer

    vae = load("vae")
    with torch.no_grad():
        z = latents / vae.config.scaling_factor + vae.config.shift_factor
        decoded = vae.decode(z, return_dict=False)[0]  # [1, 3, H, W] in [-1, 1]
    # The pipeline's VaeImageProcessor.postprocess(output_type="np") then `* 255` rounded: denormalise, clamp, round.
    img = (decoded / 2 + 0.5).clamp(0, 1)[0].permute(1, 2, 0)  # HWC in [0, 1]
    image_u8 = torch.round(img * 255).to(torch.uint8)
    record = {
        "ids": ids,
        "steps": steps,
        "sigmas": sigmas,
        "prompt_embeds": prompt_embeds.flatten().tolist(),
        "prompt_embeds_shape": list(prompt_embeds.shape),
        "pooled": pooled.flatten().tolist(),
        "initial_latent": noise.flatten().tolist(),
        "steps_detail": steps_out,
        "decoded": decoded.flatten().tolist(),
        "image_hwc_u8": image_u8.flatten().tolist(),
        "image_shape": [IMAGE_HW, IMAGE_HW, 3],
        "scaling_factor": float(vae.config.scaling_factor),
        "shift_factor": float(vae.config.shift_factor),
    }
    with open(out_path, "w") as f:
        json.dump(record, f)
    print(f"wrote {out_path}")


def main(argv):
    if len(argv) < 2:
        sys.exit(__doc__)
    cmd = argv[1]
    if cmd == "counts":
        cmd_counts()
    elif cmd == "build" and len(argv) == 3:
        cmd_build(argv[2])
    elif cmd == "scheduler":
        cmd_scheduler()
    elif cmd == "reference" and len(argv) == 4:
        cmd_reference(argv[2], argv[3])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    os.environ.setdefault("HF_HUB_OFFLINE", "1")
    os.environ.setdefault("TRANSFORMERS_OFFLINE", "1")
    main(sys.argv)
