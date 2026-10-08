#!/usr/bin/env python3
"""Fixtures for `tests/torch_bin.rs`: PyTorch checkpoints written by `torch.save` (the files the pickle reader must read) and
their expectations. Run with the repository's venv (torch + safetensors):

    tir-venv/bin/python misaka-palw-tir-lower/tools/gen_torch_bin_fixtures.py

Writes, under tests/fixtures/torch/:
  flat.bin           an OrderedDict of tensors: f32, f16, bf16, i64, a tied tensor (one storage, two names), a storage-offset view
  flat.json          the expectation: name -> {dtype, shape, values} (values as the float/int list)
  state_dict.bin     nn.Module.state_dict() — carries `_metadata` (the BUILD the reader accepts and drops)
  proto4.bin         the same state dict pickled with protocol 4 (STACK_GLOBAL, MEMOIZE, FRAME)
  legacy.bin         torch.save(..., _use_new_zipfile_serialization=False): a refusal, not a read
  strided.bin        a transposed (non-contiguous) tensor: a refusal by name
  nested.bin         {"model": {...}} a training checkpoint: a refusal by name
  hf-bin/llama/      tests/fixtures/hf/llama re-saved as pytorch_model.bin (+ config.json)
  hf-bin/llama-sharded/   the same, in two shards with pytorch_model.bin.index.json
"""
import collections, json, os, shutil, sys
import torch
from safetensors.torch import load_file

HERE = os.path.dirname(os.path.abspath(__file__))
FIX = os.path.join(HERE, "..", "tests", "fixtures")
OUT = os.path.join(FIX, "torch")
os.makedirs(OUT, exist_ok=True)

sd = collections.OrderedDict()
sd["embed.weight"] = torch.arange(12, dtype=torch.float32).reshape(3, 4) / 7
sd["layer.0.w"] = torch.arange(8, dtype=torch.float16).reshape(2, 4)
sd["layer.0.b"] = torch.arange(4, dtype=torch.bfloat16) * 3
sd["tied"] = sd["embed.weight"]
sd["ids"] = torch.arange(5, dtype=torch.int64) - 2
sd["view"] = sd["embed.weight"][1:]
sd["scalar"] = torch.tensor(2.5, dtype=torch.float32)
torch.save(sd, os.path.join(OUT, "flat.bin"))
exp = {}
for k, v in sd.items():
    dt = {torch.float32: "F32", torch.float16: "F16", torch.bfloat16: "BF16", torch.int64: "I64"}[v.dtype]
    exp[k] = {"dtype": dt, "shape": list(v.shape), "values": v.float().flatten().tolist() if v.dtype != torch.int64 else v.flatten().tolist()}
json.dump(exp, open(os.path.join(OUT, "flat.json"), "w"), indent=1)

torch.manual_seed(0)
m = torch.nn.Sequential(torch.nn.Linear(2, 3), torch.nn.LayerNorm(3))
torch.save(m.state_dict(), os.path.join(OUT, "state_dict.bin"))
torch.save(m.state_dict(), os.path.join(OUT, "proto4.bin"), pickle_protocol=4)
torch.save(m.state_dict(), os.path.join(OUT, "legacy.bin"), _use_new_zipfile_serialization=False)
torch.save({"w": torch.arange(6, dtype=torch.float32).reshape(2, 3).t()}, os.path.join(OUT, "strided.bin"))
torch.save({"model": {"w": torch.zeros(2)}, "step": 7}, os.path.join(OUT, "nested.bin"))
json.dump({k: {"shape": list(v.shape), "values": v.flatten().tolist()} for k, v in m.state_dict().items()}, open(os.path.join(OUT, "state_dict.json"), "w"))

# HF checkpoint re-saved as pytorch_model.bin
src = os.path.join(FIX, "hf", "llama")
w = load_file(os.path.join(src, "model.safetensors"))
for name, shards in (("llama", 1), ("llama-sharded", 2)):
    d = os.path.join(FIX, "hf-bin", name)
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(d)
    shutil.copy(os.path.join(src, "config.json"), os.path.join(d, "config.json"))
    keys = sorted(w)
    if shards == 1:
        torch.save(collections.OrderedDict((k, w[k].contiguous()) for k in keys), os.path.join(d, "pytorch_model.bin"))
    else:
        weight_map, total = {}, 0
        half = len(keys) // 2
        for i, part in enumerate((keys[:half], keys[half:])):
            fn = f"pytorch_model-{i + 1:05d}-of-00002.bin"
            torch.save(collections.OrderedDict((k, w[k].contiguous()) for k in part), os.path.join(d, fn))
            for k in part:
                weight_map[k] = fn
                total += w[k].numel() * w[k].element_size()
        json.dump({"metadata": {"total_size": total}, "weight_map": weight_map}, open(os.path.join(d, "pytorch_model.bin.index.json"), "w"), indent=1)
print("ok", sorted(os.listdir(OUT)))
