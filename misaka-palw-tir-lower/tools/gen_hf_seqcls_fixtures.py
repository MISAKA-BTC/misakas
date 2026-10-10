#!/usr/bin/env python3
"""Hugging Face reference fixtures for SEQUENCE CLASSIFIERS, rerankers and reward models (`…ForSequenceClassification`).

Each tiny model is built with random weights, re-randomised and rounded to bfloat16 exactly as `gen_hf_fixtures.py` does, saved,
reloaded fresh in f32 with eager attention and run on fixed token sequences:

    tests/fixtures/hf-cls/<name>/config.json
    tests/fixtures/hf-cls/<name>/model.safetensors   (BF16)
    tests/fixtures/hf-cls/<name>/outputs.json        {"sequences": [{"tokens", "logits"}], "kind": "decoder" | "encoder:<pad>:<lmax>"}

A decoder reads the LAST token of the prompt (transformers picks the last non-pad token; no pad id occurs inside these prompts); an
encoder is run over a padded template with an attention mask and reads row 0 (`[CLS]` / `<s>`).

Usage:  HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 tir-venv/bin/python tools/gen_hf_seqcls_fixtures.py [name ...]
"""
import json, os, sys
import torch

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from gen_hf_fixtures import randomise  # noqa: E402
import transformers  # noqa: E402

CRATE = os.path.dirname(HERE)
FIX = os.path.join(CRATE, "tests", "fixtures", "hf-cls")
V = 64
DEC = dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4, num_key_value_heads=2,
           max_position_embeddings=64, pad_token_id=0)
DSEQ = [[5, 17, 33, 8, 21], [40, 2, 29, 11, 50, 7, 21, 9]]
ENC = dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4)

# name -> (config class, model class, kwargs, sequences, kind)
CONFIGS = {
    "llama_cls": ("LlamaConfig", "LlamaForSequenceClassification", dict(DEC, num_labels=3, tie_word_embeddings=False), DSEQ, "decoder"),
    "llama_reward": ("LlamaConfig", "LlamaForSequenceClassification", dict(DEC, num_labels=1, tie_word_embeddings=True), DSEQ, "decoder"),
    "qwen2_cls": ("Qwen2Config", "Qwen2ForSequenceClassification", dict(DEC, num_labels=2), DSEQ, "decoder"),
    "qwen3_rerank": ("Qwen3Config", "Qwen3ForSequenceClassification", dict(DEC, num_labels=1, head_dim=8), DSEQ, "decoder"),
    "mistral_cls": ("MistralConfig", "MistralForSequenceClassification", dict(DEC, num_labels=2), DSEQ, "decoder"),
    "gemma_cls": ("GemmaConfig", "GemmaForSequenceClassification", dict(DEC, num_labels=2, head_dim=8), DSEQ, "decoder"),
    "gemma2_cls": ("Gemma2Config", "Gemma2ForSequenceClassification", dict(DEC, num_labels=2, head_dim=8, query_pre_attn_scalar=8), DSEQ, "decoder"),
    "phi3_cls": ("Phi3Config", "Phi3ForSequenceClassification", dict(DEC, num_labels=2), DSEQ, "decoder"),
    "mixtral_cls": ("MixtralConfig", "MixtralForSequenceClassification", dict(DEC, num_labels=2, num_local_experts=4, num_experts_per_tok=2), DSEQ, "decoder"),
    "qwen3_moe_cls": ("Qwen3MoeConfig", "Qwen3MoeForSequenceClassification", dict(DEC, num_labels=2, head_dim=8, num_experts=4, num_experts_per_tok=2, moe_intermediate_size=32, decoder_sparse_step=1, mlp_only_layers=[]), DSEQ, "decoder"),
    "olmo2_cls": ("Olmo2Config", "Olmo2ForSequenceClassification", dict(DEC, num_labels=2), DSEQ, "decoder"),
    "gpt2_cls": ("GPT2Config", "GPT2ForSequenceClassification", dict(vocab_size=V, n_embd=32, n_layer=2, n_head=4, n_positions=64, num_labels=2, pad_token_id=0), DSEQ, "decoder"),
    "opt_cls": ("OPTConfig", "OPTForSequenceClassification", dict(vocab_size=V, hidden_size=32, ffn_dim=64, num_hidden_layers=2, num_attention_heads=4, max_position_embeddings=64, num_labels=2, pad_token_id=0, word_embed_proj_dim=32), DSEQ, "decoder"),
    # Encoders. [CLS] 2, [SEP] 3 (BERT family); <s> 0, </s> 2, <pad> 1 (RoBERTa family).
    "bert_cls": ("BertConfig", "BertForSequenceClassification", dict(ENC, max_position_embeddings=32, type_vocab_size=2, hidden_act="gelu", layer_norm_eps=1e-12, pad_token_id=0, num_labels=3), [[2, 11, 25, 7, 3], [2, 40, 9, 17, 33, 21, 8, 3]], "encoder:0:12"),
    "bert_rerank": ("BertConfig", "BertForSequenceClassification", dict(ENC, max_position_embeddings=32, type_vocab_size=2, hidden_act="gelu", layer_norm_eps=1e-12, pad_token_id=0, num_labels=1), [[2, 11, 25, 3, 7, 9, 3], [2, 40, 9, 3, 33, 21, 8, 5, 3]], "encoder:0:12"),
    "roberta_cls": ("RobertaConfig", "RobertaForSequenceClassification", dict(ENC, max_position_embeddings=34, type_vocab_size=1, hidden_act="gelu", layer_norm_eps=1e-5, pad_token_id=1, bos_token_id=0, eos_token_id=2, num_labels=2), [[0, 11, 25, 7, 2], [0, 40, 9, 17, 33, 21, 8, 2]], "encoder:1:12"),
    "xlmr_rerank": ("XLMRobertaConfig", "XLMRobertaForSequenceClassification", dict(ENC, max_position_embeddings=34, type_vocab_size=1, hidden_act="gelu", layer_norm_eps=1e-5, pad_token_id=1, bos_token_id=0, eos_token_id=2, num_labels=1), [[0, 11, 25, 7, 2], [0, 40, 9, 17, 33, 21, 8, 2]], "encoder:1:12"),
    # HFX 2026-10-10: ALBERT (a pooler `dense` + tanh, shared layers), DeBERTa-v2 (ContextPooler: `dense` + the pooler activation) and
    # CamemBERT (RoBERTa's classification head under the `roberta.` prefix).
    "albert_cls": ("AlbertConfig", "AlbertForSequenceClassification", dict(ENC, embedding_size=16, max_position_embeddings=32, type_vocab_size=2, hidden_act="gelu_new", layer_norm_eps=1e-12, pad_token_id=0, num_hidden_groups=1, inner_group_num=1, num_labels=3), [[2, 11, 25, 7, 3], [2, 40, 9, 17, 33, 21, 8, 3]], "encoder:0:12"),
    "deberta_v2_cls": ("DebertaV2Config", "DebertaV2ForSequenceClassification", dict(ENC, max_position_embeddings=32, type_vocab_size=0, hidden_act="gelu", layer_norm_eps=1e-7, pad_token_id=0, relative_attention=True, position_buckets=8, max_relative_positions=-1, pos_att_type=["p2c", "c2p"], share_att_key=True, norm_rel_ebd="layer_norm", position_biased_input=False, pooler_hidden_size=32, pooler_hidden_act="gelu", pooler_dropout=0.0, legacy=True, num_labels=3), [[1, 11, 25, 7, 2], [1, 40, 9, 17, 33, 21, 8, 2]], "encoder:0:12"),
    "camembert_cls": ("CamembertConfig", "CamembertForSequenceClassification", dict(ENC, max_position_embeddings=34, type_vocab_size=1, hidden_act="gelu", layer_norm_eps=1e-5, pad_token_id=1, bos_token_id=0, eos_token_id=2, num_labels=2), [[0, 11, 25, 7, 2], [0, 40, 9, 17, 33, 21, 8, 2]], "encoder:1:12"),
    "distilbert_cls": ("DistilBertConfig", "DistilBertForSequenceClassification", dict(vocab_size=V, dim=32, hidden_dim=64, n_layers=2, n_heads=4, max_position_embeddings=32, activation="gelu", pad_token_id=0, num_labels=2), [[2, 11, 25, 7, 3], [2, 40, 9, 17, 33, 21, 8, 3]], "encoder:0:12"),
}


def make(name, seed=11):
    cfgc, modc, kw, seqs, kind = CONFIGS[name]
    cfg = getattr(transformers, cfgc)(**kw)
    cfg._attn_implementation = "eager"
    torch.manual_seed(seed)
    model = getattr(transformers, modc)(cfg)
    hidden = getattr(cfg, "hidden_size", None) or getattr(cfg, "dim", None) or getattr(cfg, "n_embd")
    randomise(model, hidden, seed)
    d = os.path.join(FIX, name)
    os.makedirs(d, exist_ok=True)
    model.save_pretrained(d, safe_serialization=True)
    fresh = getattr(transformers, modc).from_pretrained(d, torch_dtype=torch.float32, attn_implementation="eager").eval()
    out = []
    with torch.no_grad():
        for s in seqs:
            if kind == "decoder":
                lg = fresh(input_ids=torch.tensor([s])).logits[0]
                rec = {"tokens": s, "logits": lg.tolist()}
            else:
                _, pad, lmax = kind.split(":")
                pad, lmax = int(pad), int(lmax)
                ids = torch.tensor([s + [pad] * (lmax - len(s))])
                mask = torch.tensor([[1] * len(s) + [0] * (lmax - len(s))])
                lg = fresh(input_ids=ids, attention_mask=mask).logits[0]
                rec = {"tokens": s, "padded": ids[0].tolist(), "count": len(s), "logits": lg.tolist()}
            out.append(rec)
    with open(os.path.join(d, "outputs.json"), "w") as f:
        json.dump({"kind": kind, "sequences": out, "transformers": transformers.__version__, "torch": torch.__version__, "seed": seed,
                   "weights": "bfloat16-exact (rounded before the forward)", "attn_implementation": "eager",
                   "tensors": sorted(fresh.state_dict().keys())}, f)
    for extra in ("generation_config.json",):
        p = os.path.join(d, extra)
        if os.path.exists(p):
            os.remove(p)
    return max(abs(x) for r in out for x in r["logits"])


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1 (fixtures are built from local configs only)")
    names = sys.argv[1:] or list(CONFIGS)
    bad = 0
    for n in names:
        try:
            print(f"ok   {n:18s} max|logit| {make(n):.3f}")
        except Exception as e:
            bad += 1
            print(f"FAIL {n:18s} {type(e).__name__}: {str(e)[:300]}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
