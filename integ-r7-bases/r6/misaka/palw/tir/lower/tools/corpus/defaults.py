#!/usr/bin/env python3
"""Print a transformers config class's defaults (what an adapter's `config.defaults` must carry).

    HF_HUB_OFFLINE=1 python defaults.py MODEL_TYPE [key ...]     # all keys, or the named ones

`save_pretrained` writes only the values that differ from the class defaults, so a published
`config.json` omits most keys; an adapter has to know the class's own defaults to read it.
"""
import json
import os
import sys

os.environ.setdefault("HF_HUB_OFFLINE", "1")
from transformers.models.auto.configuration_auto import CONFIG_MAPPING  # noqa: E402

SKIP = {"architectures", "torch_dtype", "dtype", "transformers_version", "output_hidden_states", "output_attentions", "return_dict",
        "use_cache", "is_encoder_decoder", "tie_encoder_decoder", "chunk_size_feed_forward", "cross_attention_hidden_size",
        "add_cross_attention", "is_decoder", "id2label", "label2id", "problem_type", "tokenizer_class", "prefix", "sep_token_id",
        "decoder_start_token_id", "task_specific_params", "finetuning_task", "tf_legacy_loss", "pruned_heads", "_name_or_path"}


def main():
    mt, keys = sys.argv[1], sys.argv[2:]
    d = CONFIG_MAPPING[mt]().to_dict()
    out = {k: v for k, v in d.items() if (k in keys if keys else k not in SKIP)}
    print(json.dumps(out, default=str))


if __name__ == "__main__":
    main()
