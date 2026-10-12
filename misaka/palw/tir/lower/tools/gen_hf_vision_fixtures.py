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
import math
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
    "vit": dict(cfg=("ViTConfig", dict(hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4,
                                       image_size=28, patch_size=7, hidden_act="gelu", layer_norm_eps=1e-12,
                                       qkv_bias=True)),
                model="ViTModel", size=(28, 28), mean=HALF, std=HALF, layout="nchw"),
    "resnet": dict(cfg=("ResNetConfig", dict(num_channels=3, embedding_size=8, hidden_sizes=[8, 16], depths=[1, 1], layer_type="basic",
                                             hidden_act="relu", downsample_in_first_stage=False)),
                   model="ResNetModel", size=(32, 32), mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225], layout="nchw"),
    "resnet_bottleneck": dict(cfg=("ResNetConfig", dict(num_channels=3, embedding_size=8, hidden_sizes=[16, 32], depths=[2, 1],
                                                        layer_type="bottleneck", hidden_act="relu", downsample_in_first_stage=False)),
                              model="ResNetModel", size=(32, 32), mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225], layout="nchw"),
    # Deep enough that the program needs layer blocks: the carry between them changes shape (the feature map halves each stage).
    "resnet_deep": dict(cfg=("ResNetConfig", dict(num_channels=3, embedding_size=8, hidden_sizes=[8, 8, 16, 16], depths=[2, 2, 2, 2],
                                                  layer_type="basic", hidden_act="relu", downsample_in_first_stage=False)),
                        model="ResNetModel", size=(64, 64), mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225], layout="nchw"),
    # Deeper still: its program has TWO layer blocks, so the carry between them is read and written by blocks that share
    # no site names — the case a network with one layer block cannot show.
    "resnet_deeper": dict(cfg=("ResNetConfig", dict(num_channels=3, embedding_size=8, hidden_sizes=[8, 8, 8, 8], depths=[4, 4, 4, 4],
                                                    layer_type="basic", hidden_act="relu", downsample_in_first_stage=False)),
                          model="ResNetModel", size=(64, 64), mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225], layout="nchw"),
    # ConvNeXt: a patch convolution and channel LayerNorm, stages of depthwise/pointwise blocks with a layer scale, a 2x2 stride-2
    # downsampling between stages, the pooled vector through the final LayerNorm. The layer scale is set live (see `make`).
    "convnext": dict(cfg=("ConvNextConfig", dict(num_channels=3, patch_size=4, num_stages=4, hidden_sizes=[8, 8, 16, 16], depths=[1, 2, 2, 1],
                                                 hidden_act="gelu", layer_norm_eps=1e-12, layer_scale_init_value=1e-6, image_size=64)),
                     model="ConvNextModel", size=(64, 64), mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225], layout="nchw"),
    # Deep enough that the program needs layer blocks (the carry between them is a padded flat activation).
    "convnext_deep": dict(cfg=("ConvNextConfig", dict(num_channels=3, patch_size=4, num_stages=3, hidden_sizes=[8, 8, 16], depths=[4, 6, 4],
                                                      hidden_act="gelu", layer_norm_eps=1e-12, layer_scale_init_value=1e-6, image_size=64)),
                          model="ConvNextModel", size=(64, 64), mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225], layout="nchw"),
    # MobileNetV2 at depth multiplier 1/4 on a 60x60 image: the maps are 30, 15, 8, 4 and 2 wide, so TensorFlow "SAME" padding
    # is asymmetric at some strides and symmetric at others (an odd extent at a stride of 2).
    "mobilenet_v2": dict(cfg=("MobileNetV2Config", dict(num_channels=3, image_size=60, depth_multiplier=0.25, finegrained_output=False,
                                                        tf_padding=True, hidden_act="relu6", layer_norm_eps=1e-3)),
                         model="MobileNetV2Model", size=(60, 60), mean=HALF, std=HALF, layout="nchw"),
    # The same network with every weight row on the lowering's int8 grid (see `snap_rows_to_int8_grid`): no weight-quantisation
    # error, so the integer program's distance from transformers is the lowering's own — the exactness check of a deep network.
    "mobilenet_v2_grid": dict(cfg=("MobileNetV2Config", dict(num_channels=3, image_size=60, depth_multiplier=0.25, finegrained_output=False,
                                                             tf_padding=True, hidden_act="relu6", layer_norm_eps=1e-3)),
                              model="MobileNetV2Model", size=(60, 60), mean=HALF, std=HALF, layout="nchw"),
    "convnext_grid": dict(cfg=("ConvNextConfig", dict(num_channels=3, patch_size=4, num_stages=4, hidden_sizes=[8, 8, 16, 16], depths=[1, 2, 2, 1],
                                                      hidden_act="gelu", layer_norm_eps=1e-12, layer_scale_init_value=1e-6, image_size=64)),
                          model="ConvNextModel", size=(64, 64), mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225], layout="nchw"),
    # The symmetric-padding variant (tf_padding = false) with an expansion-free first layer (first_layer_is_expansion = false).
    "mobilenet_v2_sym": dict(cfg=("MobileNetV2Config", dict(num_channels=3, image_size=64, depth_multiplier=0.25, finegrained_output=False,
                                                            tf_padding=False, first_layer_is_expansion=False, hidden_act="relu6",
                                                            layer_norm_eps=1e-3)),
                             model="MobileNetV2Model", size=(64, 64), mean=HALF, std=HALF, layout="nchw"),
    "mobilenet_v1": dict(cfg=("MobileNetV1Config", dict(num_channels=3, image_size=60, depth_multiplier=0.25, tf_padding=True,
                                                        hidden_act="relu6", layer_norm_eps=1e-3)),
                         model="MobileNetV1Model", size=(60, 60), mean=HALF, std=HALF, layout="nchw"),
    # The image classifiers (`…ForImageClassification`, HFX 2026-10-10): the same towers with their classifier head, 5 labels.
    "vit_cls": dict(cfg=("ViTConfig", dict(hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4,
                                           image_size=28, patch_size=7, hidden_act="gelu", layer_norm_eps=1e-12,
                                           qkv_bias=True, num_labels=5)),
                    model="ViTForImageClassification", size=(28, 28), mean=HALF, std=HALF, layout="nchw"),
    "resnet_cls": dict(cfg=("ResNetConfig", dict(num_channels=3, embedding_size=8, hidden_sizes=[8, 16], depths=[1, 1], layer_type="basic",
                                                 hidden_act="relu", downsample_in_first_stage=False, num_labels=5)),
                       model="ResNetForImageClassification", size=(32, 32), mean=[0.485, 0.456, 0.406], std=[0.229, 0.224, 0.225],
                       layout="nchw"),
    "convnext_cls": dict(cfg=("ConvNextConfig", dict(num_channels=3, patch_size=4, num_stages=4, hidden_sizes=[8, 8, 16, 16],
                                                     depths=[1, 2, 2, 1], hidden_act="gelu", layer_norm_eps=1e-12,
                                                     layer_scale_init_value=1e-6, image_size=64, num_labels=5)),
                         model="ConvNextForImageClassification", size=(64, 64), mean=[0.485, 0.456, 0.406],
                         std=[0.229, 0.224, 0.225], layout="nchw"),
    "mobilenet_v2_cls": dict(cfg=("MobileNetV2Config", dict(num_channels=3, image_size=60, depth_multiplier=0.25, finegrained_output=False,
                                                            tf_padding=True, hidden_act="relu6", layer_norm_eps=1e-3, num_labels=5)),
                             model="MobileNetV2ForImageClassification", size=(60, 60), mean=HALF, std=HALF, layout="nchw"),
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
    # Whole VLMs for the text stage (RFC-0003 §II.2.1): tower + merger + the M-RoPE language model.
    "qwen2_vl": dict(cfg=("Qwen2VLConfig", None), model="Qwen2VLForConditionalGeneration", size=(28, 28), mean=CLIP_MEAN,
                     std=CLIP_STD, layout="qwen2vl"),
    "qwen2_5_vl": dict(cfg=("Qwen2_5_VLConfig", None), model="Qwen2_5_VLForConditionalGeneration", size=(56, 56),
                       mean=CLIP_MEAN, std=CLIP_STD, layout="qwen2vl"),
    # Qwen3.5: the tower with its learned position table resampled to the grid, the hybrid (Gated DeltaNet + attention)
    # language model with interleaved M-RoPE.
    "qwen3_5": dict(cfg=("Qwen3_5Config", None), model="Qwen3_5ForConditionalGeneration", size=(48, 48), mean=HALF,
                    std=HALF, layout="qwen2vl"),
}
NEW_TOKENS = 8


def build_config(name, spec):
    cls, kw = spec["cfg"]
    if name in ("qwen2_vl", "qwen2_5_vl"):
        q25 = name == "qwen2_5_vl"
        text = dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4,
                    num_key_value_heads=2, max_position_embeddings=128, rms_norm_eps=1e-6, tie_word_embeddings=False,
                    rope_parameters={"rope_type": "default", "rope_theta": 10000.0, "mrope_section": [2, 1, 1]})
        vis = CONFIGS["qwen2_5_vl_vision" if q25 else "qwen2_vl_vision"]["cfg"][1] | {"out_hidden_size" if q25 else "hidden_size": 32}
        cfg = getattr(transformers, cls)(text_config=text, vision_config=vis, image_token_id=63, vision_start_token_id=62,
                                         vision_end_token_id=61, video_token_id=60, tie_word_embeddings=False)
        return cfg
    if name == "qwen3_5":
        text = dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4,
                    num_key_value_heads=2, head_dim=16, max_position_embeddings=128, rms_norm_eps=1e-6,
                    tie_word_embeddings=False, layer_types=["linear_attention", "full_attention"],
                    linear_num_key_heads=2, linear_num_value_heads=4, linear_key_head_dim=8, linear_value_head_dim=8,
                    linear_conv_kernel_dim=4, partial_rotary_factor=0.5,
                    rope_parameters={"rope_type": "default", "rope_theta": 10000.0, "partial_rotary_factor": 0.5,
                                     "mrope_section": [2, 1, 1], "mrope_interleaved": True})
        vis = dict(depth=2, hidden_size=32, hidden_act="gelu_pytorch_tanh", intermediate_size=64, num_heads=4,
                   in_channels=3, patch_size=8, spatial_merge_size=2, temporal_patch_size=2, out_hidden_size=32,
                   num_position_embeddings=36)
        return transformers.Qwen3_5Config(text_config=text, vision_config=vis, image_token_id=63, vision_start_token_id=62,
                                          vision_end_token_id=61, video_token_id=60, tie_word_embeddings=False)
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


def snap_rows_to_int8_grid(w):
    """Every output row of `w` onto the per-row int8 grid the lowering's weight quantiser uses: a power-of-two step, integer
    codes in [-127, 127] with one at exactly +-127 (so the quantiser recovers the same step and the same codes), bf16-exact.
    A network on this grid has no weight-quantisation error: what the integer program differs by is the lowering's own."""
    rows = w.reshape(w.shape[0], -1).clone()
    for i in range(rows.shape[0]):
        r = rows[i]
        amax = float(r.abs().max())
        if amax == 0.0:
            continue
        step = 2.0 ** math.ceil(math.log2(amax / 127.0))
        codes = torch.clamp(torch.round(r / step), -127, 127)
        j = int(torch.argmax(r.abs()))
        codes[j] = 127.0 if r[j] > 0 else -127.0
        rows[i] = codes * step
    return rows.reshape(w.shape)


def randomise_cnn(model, seed, grid=False):
    """He-scaled weights (std sqrt(2/fan_in): the signal survives fifty layers of a MobileNet), norm gains 1 +- 0.1, biases and
    batch-norm statistics +- 0.1, ConvNeXt's layer scales ~ 0.5 (their initial 1e-6 would make every block a no-op); then
    everything rounded to bfloat16."""
    g = torch.Generator().manual_seed(seed + 1)
    sd_names = set(model.state_dict().keys())
    with torch.no_grad():
        for name, p in model.named_parameters():
            if not p.is_floating_point():
                continue
            if "layer_scale" in name:
                p.copy_(0.5 + 0.1 * torch.randn(p.shape, generator=g))
            elif p.ndim >= 2:
                fan_in = p[0].numel()
                p.copy_(torch.randn(p.shape, generator=g) * math.sqrt(2.0 / fan_in))
                if grid:
                    p.copy_(snap_rows_to_int8_grid(p))
            else:
                p.add_(torch.randn(p.shape, generator=g) * 0.1)
            p.copy_(p.to(torch.bfloat16).to(torch.float32))
        for name, b in model.named_buffers():
            if name in sd_names and b.is_floating_point():
                b.add_(torch.randn(b.shape, generator=g) * 0.1)
                b.copy_(b.to(torch.bfloat16).to(torch.float32))


def make(name):
    spec = CONFIGS[name]
    seed = sum(ord(ch) for ch in name)
    torch.manual_seed(seed)
    cfg = build_config(name, spec)
    cls = model_class(spec["model"])
    model = cls(cfg)
    model.eval()
    hidden = getattr(cfg, "hidden_size", None) if name != "llava" else cfg.text_config.hidden_size
    if name.startswith("qwen2_vl") and name.endswith("_vision"):
        hidden = cfg.embed_dim
    if name.startswith("resnet"):  # (resnet_cls too)
        hidden = 72  # about a 3x3 convolution's fan-in over 8 channels: the weights' scale keeps activations O(1)
    if name in ("qwen2_vl", "qwen2_5_vl", "qwen3_5"):
        hidden = cfg.text_config.hidden_size
    if name.startswith(("convnext", "mobilenet")):
        randomise_cnn(model, seed, grid=name.endswith("_grid"))
    else:
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
            vcfg = cfg.vision_config if name in ("llava", "qwen2_vl", "qwen2_5_vl", "qwen3_5") else cfg
            pv, grid = pixel_values(img, spec, vcfg)
            out = {}
            if name == "clip_vision":
                o = fresh(pixel_values=pv)
                out = {"image_embeds": o.image_embeds[0].tolist(), "last_hidden_state": o.last_hidden_state[0].tolist()}
            elif name == "siglip_vision":
                o = fresh(pixel_values=pv)
                out = {"pooler_output": o.pooler_output[0].tolist(), "last_hidden_state": o.last_hidden_state[0].tolist()}
            elif name.endswith("_cls"):
                o = fresh(pixel_values=pv)
                out = {"logits": o.logits[0].tolist()}
            elif name.startswith(("resnet", "convnext", "mobilenet")):
                o = fresh(pixel_values=pv)
                fm = o.last_hidden_state[0]  # [C, H, W] -> rows [H*W, C]
                out = {"last_hidden_state": fm.permute(1, 2, 0).reshape(-1, fm.shape[0]).tolist(), "pooler_output": o.pooler_output[0].flatten().tolist()}
            elif name == "vit":
                o = fresh(pixel_values=pv)
                out = {"last_hidden_state": o.last_hidden_state[0].tolist(), "cls": o.last_hidden_state[0, 0].tolist(),
                       "pooler_output": o.pooler_output[0].tolist()}
            elif name in ("qwen2_vl_vision", "qwen2_5_vl_vision"):
                o = fresh(pv, grid_thw=grid)
                out = {"merged": o.pooler_output.tolist(), "last_hidden_state": o.last_hidden_state.tolist()}
            elif name in ("qwen2_vl", "qwen2_5_vl", "qwen3_5"):
                vc = cfg.vision_config
                n_img = (grid[0, 1] * grid[0, 2] // (vc.spatial_merge_size ** 2)).item()
                ids = [5, 62] + [63] * n_img + [61, 7, 9]
                t = torch.tensor([ids])
                mm = (t == 63).int()
                # The special ids (image, vision start/end, video) are never generated: a stream holds the prompt's image rows only.
                gen = fresh.generate(input_ids=t, pixel_values=pv, image_grid_thw=grid, mm_token_type_ids=mm,
                                     max_new_tokens=NEW_TOKENS, do_sample=False,
                                     bad_words_ids=[[60], [61], [62], [63]] if name == "qwen3_5" else None)[0, len(ids):].tolist()
                stream = torch.tensor([ids + gen[:-1]])
                # The processor's mm_token_type_ids: 1 on the prompt's image rows only.
                smm = torch.zeros_like(stream, dtype=torch.int32)
                smm[0, :len(ids)] = mm[0]
                o = fresh(input_ids=stream, pixel_values=pv, image_grid_thw=grid, mm_token_type_ids=smm)
                pos, deltas = fresh.model.get_rope_index(stream, mm_token_type_ids=smm, image_grid_thw=grid)
                rows = fresh.model.get_image_features(pv, grid)
                rows = rows.pooler_output if hasattr(rows, "pooler_output") else rows
                rows = torch.cat(list(rows), 0) if isinstance(rows, (list, tuple)) else rows
                out = {"input_ids": ids, "generated": gen, "logits": o.logits[0].tolist(), "image_rows": rows.tolist(),
                       "mrope_positions": pos[:, 0].tolist(), "grid": grid[0].tolist()}
            elif name == "llava":
                n_img = (h // cfg.vision_config.patch_size) * (w // cfg.vision_config.patch_size)
                ids = [1, 5] + [63] * n_img + [7, 9, 11, 13]
                o = fresh(input_ids=torch.tensor([ids]), pixel_values=pv)
                vt = fresh.model.vision_tower(pv, output_hidden_states=True)
                feats = fresh.model.multi_modal_projector(vt.hidden_states[-2][:, 1:])
                gen = fresh.generate(input_ids=torch.tensor([ids]), pixel_values=pv, max_new_tokens=NEW_TOKENS,
                                     do_sample=False)[0, len(ids):].tolist()
                stream = torch.tensor([ids + gen[:-1]])
                so = fresh(input_ids=stream, pixel_values=pv)
                out = {"input_ids": ids, "image_start": 2, "logits": o.logits[0].tolist(), "image_rows": feats[0].tolist(),
                       "generated": gen, "stream_logits": so.logits[0].tolist()}
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
