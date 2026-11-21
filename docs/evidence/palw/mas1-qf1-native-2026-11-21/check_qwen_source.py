"""Check the upstream BF16 geometry used for RFC-0066 lowering, by immutable revision."""
import hashlib
import json
import math
from pathlib import Path

from huggingface_hub import HfApi, hf_hub_download


MODEL = "Qwen/Qwen3-8B"
REVISION = "b968826d9c46dd6066d109eabc6255188de91218"


def expected_shapes():
    projection = {
        "self_attn.q_proj": [4096, 4096], "self_attn.k_proj": [1024, 4096],
        "self_attn.v_proj": [1024, 4096], "self_attn.o_proj": [4096, 4096],
        "self_attn.q_norm": [128], "self_attn.k_norm": [128],
        "input_layernorm": [4096], "post_attention_layernorm": [4096],
        "mlp.gate_proj": [12288, 4096], "mlp.up_proj": [12288, 4096], "mlp.down_proj": [4096, 12288],
    }
    shapes = {f"model.layers.{layer}.{name}.weight": shape for layer in range(36) for name, shape in projection.items()}
    shapes.update({"model.embed_tokens.weight": [151936, 4096], "model.norm.weight": [4096], "lm_head.weight": [151936, 4096]})
    return shapes


def check():
    api = HfApi(token=False)
    repo = api.model_info(MODEL, revision=REVISION, files_metadata=True)
    if repo.sha != REVISION:
        raise ValueError("upstream revision mismatch")
    files = {item.rfilename: item for item in repo.siblings}
    digests = {}

    def read(name):
        source = Path(hf_hub_download(MODEL, name, revision=REVISION, token=False)).read_bytes()
        digest = hashlib.sha256(source).hexdigest()
        entry = files[name]
        git_id = hashlib.sha1(f"blob {len(source)}\0".encode() + source).hexdigest()
        if len(source) != entry.size or (digest != entry.lfs.sha256 if entry.lfs else git_id != entry.blob_id):
            raise ValueError(f"{name} differs from the pinned file")
        digests[name] = digest
        return source

    config = json.loads(read("config.json"))
    required = {
        "architectures": ["Qwen3ForCausalLM"], "model_type": "qwen3", "torch_dtype": "bfloat16", "hidden_size": 4096,
        "intermediate_size": 12288, "num_hidden_layers": 36, "num_attention_heads": 32, "num_key_value_heads": 8,
        "head_dim": 128, "vocab_size": 151936, "hidden_act": "silu", "attention_bias": False, "attention_dropout": 0.0,
        "tie_word_embeddings": False, "rope_scaling": None, "rope_theta": 1000000, "rms_norm_eps": 1e-6,
        "use_sliding_window": False, "sliding_window": None,
    }
    for field, value in required.items():
        if field not in config or config[field] != value:
            raise ValueError(f"upstream config differs at {field}")
    if config["max_position_embeddings"] < 8192:
        raise ValueError("source position limit below QF1")
    index = json.loads(read("model.safetensors.index.json"))
    metadata = api.get_safetensors_metadata(MODEL, revision=REVISION)
    expected = expected_shapes()
    if set(expected) != set(index["weight_map"]) or metadata.weight_map != index["weight_map"]:
        raise ValueError("tensor names differ from the QF1 source geometry")
    seen = set()
    inventory = []
    for shard, file in sorted(metadata.files_metadata.items()):
        entry = files[shard]
        cursor = 0
        for name, tensor in sorted(file.tensors.items(), key=lambda pair: pair[1].data_offsets[0]):
            if name in seen or index["weight_map"].get(name) != shard or tensor.dtype != "BF16" or tensor.shape != expected[name]:
                raise ValueError(f"tensor descriptor differs at {name}")
            seen.add(name)
            end = cursor + 2 * math.prod(expected[name])
            if list(tensor.data_offsets) != [cursor, end]:
                raise ValueError(f"tensor byte extent differs at {name}")
            cursor = end
        # The hub supplies the complete object's LFS digest. Only its header was read.
        inventory.append(dict(file=shard, bytes=entry.size, upstream_sha256=entry.lfs.sha256,
                              tensor_data_bytes=cursor, tensors=len(file.tensors)))
    total = sum(item["tensor_data_bytes"] for item in inventory)
    if seen != set(expected) or total != index["metadata"]["total_size"]:
        raise ValueError("incomplete shard header inventory")
    return dict(model=MODEL, revision=REVISION, source_files_sha256=digests, tensor_count=len(seen),
                bf16_parameters=total // 2, source_shards=inventory, weight_payloads_verified=False)


if __name__ == "__main__":
    print(json.dumps(check(), indent=2))
