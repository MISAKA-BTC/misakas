#!/usr/bin/env python3
"""Generate Hugging Face reference fixtures for the vision lowering (RFC-0003 Part II.4).

Each tiny vision tower below is built with random weights, re-randomised and rounded to bfloat16
exactly as `gen_hf_fixtures.py` does, saved, reloaded fresh in f32 with eager attention, and run
on canonical images: u8 HWC RGB at the tower's input size, seeded. The images reach HF the way a
processor with resizing off would give them: `pixel_values = (u8 / 255 − mean) / std`, in the
layout each model reads. For Qwen2-VL and Qwen2.5-VL that layout is the processor's own `patchify`,
reproduced below from `image_processing_qwen2_vl.py`.

    tests/fixtures/hf-vis/<name>/config.json, model.safetensors (BF16)
    tests/fixtures/hf-vis/<name>/outputs.json   {"images": [{"hwc": [...], "outputs": {...}}], ...}

The LLaVA fixture is a whole VLM: its record also carries `input_ids` (with the image placeholder
tokens) and the logits of `LlavaForConditionalGeneration`.

Usage:
    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 python tools/gen_hf_vision_fixtures.py [name ...]
"""

import json
import os
import sys

import numpy as np
import torch

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from gen_hf_fixtures import randomise  # noqa: E402

import transformers  # noqa: E402

CRATE = os.path.dirname(HERE)
FIX = os.path.join(CRATE, "tests", "fixtures", "hf-vis")
CLIP_MEAN, CLIP_STD = [0.48145466, 0.4578275, 0.40821073], [0.26862954, 0.26130258, 0.27577711]
HALF = [0.5, 0.5, 0.5]
V = 64

TINY_CLIP = dict(hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4, image_size=28,
                 patch_size=7, projection_dim=24, hidden_act="quick_gelu", layer_norm_eps=1e-5)

CONFIGS = {
    "clip_vision": dict(cfg=("CLIPVisionConfig", TINY_CLIP), model="CLIPVisionModelWithProjection", size=(28, 28),
                        mean=CLIP_MEAN, std=CLIP_STD, layout="nchw"),
    "siglip_vision": dict(cfg=("SiglipVisionConfig", dict(hidden_size=32, intermediate_size=64, num_hidden_layers=2,
                                                          num_attention_heads=4, image_size=28, patch_size=7,
                                                          hidden_act="gelu_pytorch_tanh", layer_norm_eps=1e-6,
                                                          vision_use_head=True)),
                          model="SiglipVisionModel", size=(28, 28), mean=HALF, std=HALF, layout="nchw"),
    "qwen2_vl_vision": dict(cfg=("Qwen2VLVisionConfig", dict(depth=2, embed_dim=32, hidden_size=48, hidden_act="quick_gelu",
                                                             mlp_ratio=2, num_heads=4, in_channels=3, patch_size=7,
                                                             spatial_merge_size=2, temporal_patch_size=2)),
                            model="qwen2_vl.modeling_qwen2_vl.Qwen2VisionTransformerPretrainedModel", size=(28, 28),
                            mean=CLIP_MEAN, std=CLIP_STD, layout="qwen2vl"),
    "qwen2_5_vl_vision": dict(cfg=("Qwen2_5_VLVisionConfig", dict(depth=2, hidden_size=32, intermediate_size=64, num_heads=4,
                                                                  out_hidden_size=48, patch_size=7, spatial_merge_size=2,
                                                                  temporal_patch_size=2, window_size=28,
                                                                  fullatt_block_indexes=[1], hidden_act="silu")),
                              model="qwen2_5_vl.modeling_qwen2_5_vl.Qwen2_5_VisionTransformerPretrainedModel",
                              size=(56, 56), mean=CLIP_MEAN, std=CLIP_STD, layout="qwen2vl"),
    "llava": dict(cfg=("LlavaConfig", None), model="LlavaForConditionalGeneration", size=(28, 28), mean=CLIP_MEAN,
                  std=CLIP_STD, layout="nchw"),
}


def build_config(name, spec):
    cls, kw = spec["cfg"]
    if name == "llava":
        vis = transformers.CLIPVisionConfig(**TINY_CLIP)
        txt = transformers.LlamaConfig(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2,
                                       num_attention_heads=4, num_key_value_heads=2, max_position_embeddings=64,
                                       tie_word_embeddings=False)
        return transformers.LlavaConfig(vision_config=vis, text_config=txt, image_token_id=63, vision_feature_layer=-2,
                                        vision_feature_select_strategy="default", projector_hidden_act="gelu",
                                        multimodal_projector_bias=True)
    return getattr(transformers, cls)(**kw)


def model_class(path):
    if "." not in path:
        return getattr(transformers, path)
    mod, cls = path.rsplit(".", 1)
    import importlib
    return getattr(importlib.import_module(f"transformers.models.{mod}"), cls)


def pixel_values(img, spec, cfg):
    x = (img.astype(np.float64) / 255.0 - np.array(spec["mean"])) / np.array(spec["std"])  # HWC
    x = torch.tensor(x, dtype=torch.float32).permute(2, 0, 1).contiguous()  # CHW
    if spec["layout"] == "nchw":
        return x.unsqueeze(0), None
    # Qwen2-VL's patchify (image_processing_qwen2_vl.py), batch 1.
    p, m, tp = cfg.patch_size, cfg.spatial_merge_size, cfg.temporal_patch_size
    c, h, w = x.shape
    gh, gw = h // p, w // p
    pt = x.reshape(1, c, gh // m, m, p, gw // m, m, p).permute(0, 2, 5, 3, 6, 1, 4, 7)
    flat = pt.unsqueeze(6).expand(-1, -1, -1, -1, -1, -1, tp, -1, -1).reshape(gh * gw, c * tp * p * p)
    return flat, torch.tensor([[1, gh, gw]])


def make(name):
    spec = CONFIGS[name]
    seed = sum(ord(ch) for ch in name)
    torch.manual_seed(seed)
    cfg = build_config(name, spec)
    cls = model_class(spec["model"])
    model = cls(cfg)
    model.eval()
    hidden = getattr(cfg, "hidden_size", None) if name != "llava" else cfg.text_config.hidden_size
    if name.startswith("qwen2_vl"):
        hidden = cfg.embed_dim
    randomise(model, hidden, seed)
    d = os.path.join(FIX, name)
    os.makedirs(d, exist_ok=True)
    model.to(torch.bfloat16)
    model.save_pretrained(d)
    fresh = cls.from_pretrained(d, dtype=torch.float32, attn_implementation="eager")
    fresh.eval()
    rng = np.random.default_rng(seed)
    h, w = spec["size"]
    recs = []
    with torch.no_grad():
        for _ in range(2):
            img = rng.integers(0, 256, size=(h, w, 3), dtype=np.uint8)
            pv, grid = pixel_values(img, spec, cfg if name != "llava" else cfg.vision_config)
            out = {}
            if name == "clip_vision":
                o = fresh(pixel_values=pv)
                out = {"image_embeds": o.image_embeds[0].tolist(), "last_hidden_state": o.last_hidden_state[0].tolist()}
            elif name == "siglip_vision":
                o = fresh(pixel_values=pv)
                out = {"pooler_output": o.pooler_output[0].tolist(), "last_hidden_state": o.last_hidden_state[0].tolist()}
            elif name.startswith("qwen2"):
                o = fresh(pv, grid_thw=grid)
                out = {"merged": o.pooler_output.tolist(), "last_hidden_state": o.last_hidden_state.tolist()}
            elif name == "llava":
                n_img = (h // cfg.vision_config.patch_size) * (w // cfg.vision_config.patch_size)
                ids = [1, 5] + [63] * n_img + [7, 9, 11, 13]
                o = fresh(input_ids=torch.tensor([ids]), pixel_values=pv)
                vt = fresh.model.vision_tower(pv, output_hidden_states=True)
                feats = fresh.model.multi_modal_projector(vt.hidden_states[-2][:, 1:])
                out = {"input_ids": ids, "image_start": 2, "logits": o.logits[0].tolist(), "image_rows": feats[0].tolist()}
            recs.append({"hwc": img.reshape(-1).tolist(), "outputs": out})
    meta = {"images": recs, "size": [h, w], "mean": spec["mean"], "std": spec["std"],
            "transformers": transformers.__version__, "torch": torch.__version__, "seed": seed,
            "weights": "bfloat16-exact (rounded before the forward)", "attn_implementation": "eager"}
    with open(os.path.join(d, "outputs.json"), "w") as f:
        json.dump(meta, f)
    return list(recs[0]["outputs"].keys())


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1 (fixtures are built from local configs only)")
    names = sys.argv[1:] or list(CONFIGS)
    bad = 0
    for n in names:
        try:
            k = make(n)
            print(f"ok   {n:22s} {k}")
        except Exception as e:  # report and continue
            import traceback
            traceback.print_exc()
            bad += 1
            print(f"FAIL {n:22s} {type(e).__name__}: {str(e)[:300]}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
