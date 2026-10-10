#!/usr/bin/env python3
"""Hugging Face reference fixtures for the NLU TASK HEADS over a bidirectional encoder (HFX, 2026-10-08): token classification
(`…ForTokenClassification`), extractive question answering (`…ForQuestionAnswering`) and masked language modelling
(`…ForMaskedLM`, the fill-mask task).

Each tiny model is built with random weights, re-randomised and rounded to bfloat16 exactly as `gen_hf_fixtures.py` does, saved,
reloaded fresh in f32 with eager attention and run on fixed padded token sequences with an attention mask:

    tests/fixtures/hf-heads/<name>/config.json
    tests/fixtures/hf-heads/<name>/model.safetensors   (BF16)
    tests/fixtures/hf-heads/<name>/outputs.json        {"kind": "<head>:<pad>:<lmax>", "sequences": [...]}

    token  : "logits" is [count, labels] (the real rows only; pad rows are the class's own business)
    qa     : "logits" is [count, 2] — start logits in column 0, end logits in column 1 (transformers' `qa_outputs` split)
    mlm    : "mask_pos" is a row index; "logits" is that row's [vocab] logits

Every model here is run with token_type_ids all zero (the BERT embedding's type row 0, what the encoder lowering reads); a BERT-type
pair class's segment ids are recorded beside it (`logits_segments`, segment 1 after the first separator) for the pair-segment rule.

Usage:  HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 tir-venv/bin/python -I tools/gen_hf_heads_fixtures.py [name ...]
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import torch  # noqa: E402
import transformers  # noqa: E402
from gen_hf_fixtures import randomise  # noqa: E402

CRATE = os.path.dirname(HERE)
FIX = os.path.join(CRATE, "tests", "fixtures", "hf-heads")
V = 64
ENC = dict(vocab_size=V, hidden_size=32, intermediate_size=64, num_hidden_layers=2, num_attention_heads=4)
BERT = dict(ENC, max_position_embeddings=32, type_vocab_size=2, hidden_act="gelu", layer_norm_eps=1e-12, pad_token_id=0)
ROBERTA = dict(ENC, max_position_embeddings=34, type_vocab_size=1, hidden_act="gelu", layer_norm_eps=1e-5, pad_token_id=1, bos_token_id=0,
               eos_token_id=2)
DISTIL = dict(vocab_size=V, dim=32, hidden_dim=64, n_layers=2, n_heads=4, max_position_embeddings=32, activation="gelu", pad_token_id=0)
# [CLS] 2, [SEP] 3 (BERT family); <s> 0, </s> 2, <pad> 1 (RoBERTa family). A pair is `a [SEP] b` / `a </s></s> b`.
BSEQ = [[2, 11, 25, 7, 3], [2, 40, 9, 17, 3, 33, 21, 8, 3]]
RSEQ = [[0, 11, 25, 7, 2], [0, 40, 9, 17, 2, 2, 33, 21, 8, 2]]
ALBERT = dict(ENC, embedding_size=16, max_position_embeddings=32, type_vocab_size=2, hidden_act="gelu_new", layer_norm_eps=1e-12,
              pad_token_id=0, num_hidden_groups=1, inner_group_num=1)
DEBERTA = dict(ENC, max_position_embeddings=32, type_vocab_size=0, hidden_act="gelu", layer_norm_eps=1e-7, pad_token_id=0,
               relative_attention=True, position_buckets=8, max_relative_positions=-1, pos_att_type=["p2c", "c2p"], share_att_key=True,
               norm_rel_ebd="layer_norm", position_biased_input=False, pooler_hidden_size=32, pooler_hidden_act="gelu", legacy=True)
DSEQ2 = [[1, 11, 25, 7, 2], [1, 40, 9, 17, 2, 33, 21, 8, 2]]
MASK_B = 4  # an id used as the mask token in the MLM fixtures (any id: the class reads the row at a position)

# name -> (config class, model class, kwargs, sequences, head, pad, lmax, separator id for the pair-segment record)
CONFIGS = {
    "bert_tokcls": ("BertConfig", "BertForTokenClassification", dict(BERT, num_labels=5), BSEQ, "token", 0, 12, 3),
    "roberta_tokcls": ("RobertaConfig", "RobertaForTokenClassification", dict(ROBERTA, num_labels=4), RSEQ, "token", 1, 12, None),
    "distilbert_tokcls": ("DistilBertConfig", "DistilBertForTokenClassification", dict(DISTIL, num_labels=3), BSEQ, "token", 0, 12, None),
    "bert_qa": ("BertConfig", "BertForQuestionAnswering", dict(BERT), BSEQ, "qa", 0, 12, 3),
    "roberta_qa": ("RobertaConfig", "RobertaForQuestionAnswering", dict(ROBERTA), RSEQ, "qa", 1, 12, None),
    "distilbert_qa": ("DistilBertConfig", "DistilBertForQuestionAnswering", dict(DISTIL), BSEQ, "qa", 0, 12, None),
    # HFX 2026-10-10: ALBERT and DeBERTa-v2 task heads (ALBERT: [CLS] 2, [SEP] 3; DeBERTa-v2: [CLS] 1, [SEP] 2).
    "albert_tokcls": ("AlbertConfig", "AlbertForTokenClassification", dict(ALBERT, num_labels=4), BSEQ, "token", 0, 12, None),
    "albert_qa": ("AlbertConfig", "AlbertForQuestionAnswering", dict(ALBERT), BSEQ, "qa", 0, 12, 3),
    "deberta_v2_tokcls": ("DebertaV2Config", "DebertaV2ForTokenClassification", dict(DEBERTA, num_labels=4), DSEQ2, "token", 0, 12, None),
    "deberta_v2_qa": ("DebertaV2Config", "DebertaV2ForQuestionAnswering", dict(DEBERTA), DSEQ2, "qa", 0, 12, None),
    "bert_mlm": ("BertConfig", "BertForMaskedLM", dict(BERT), [[2, 11, MASK_B, 7, 3], [2, 40, 9, 17, 33, MASK_B, 8, 3]], "mlm", 0, 12, None),
    "roberta_mlm": ("RobertaConfig", "RobertaForMaskedLM", dict(ROBERTA), [[0, 11, MASK_B, 7, 2], [0, 40, 9, 17, 33, MASK_B, 8, 2]], "mlm", 1,
                    12, None),
    "distilbert_mlm": ("DistilBertConfig", "DistilBertForMaskedLM", dict(DISTIL), [[2, 11, MASK_B, 7, 3], [2, 40, 9, 17, 33, MASK_B, 8, 3]],
                       "mlm", 0, 12, None),
}


def rows_of(out, head):
    if head == "qa":
        return torch.stack([out.start_logits[0], out.end_logits[0]], dim=-1)
    return out.logits[0]


def make(name, seed=11):
    cfgc, modc, kw, seqs, head, pad, lmax, sep = CONFIGS[name]
    cfg = getattr(transformers, cfgc)(**kw)
    cfg._attn_implementation = "eager"
    torch.manual_seed(seed)
    model = getattr(transformers, modc)(cfg)
    hidden = getattr(cfg, "hidden_size", None) or getattr(cfg, "dim", None)
    randomise(model, hidden, seed)
    d = os.path.join(FIX, name)
    os.makedirs(d, exist_ok=True)
    model.save_pretrained(d, safe_serialization=True)
    fresh = getattr(transformers, modc).from_pretrained(d, torch_dtype=torch.float32, attn_implementation="eager").eval()
    out = []
    with torch.no_grad():
        for s in seqs:
            n = len(s)
            ids = torch.tensor([s + [pad] * (lmax - n)])
            mask = torch.tensor([[1] * n + [0] * (lmax - n)])
            kw2 = {"input_ids": ids, "attention_mask": mask}
            if "token_type_ids" in fresh.forward.__code__.co_varnames:
                kw2["token_type_ids"] = torch.zeros_like(ids)
            rows = rows_of(fresh(**kw2), head)
            rec = {"tokens": s, "padded": ids[0].tolist(), "count": n}
            if head == "mlm":
                pos = s.index(MASK_B)
                rec["mask_pos"] = pos
                rec["logits"] = rows[pos].tolist()
            else:
                rec["logits"] = rows[:n].tolist()
                if sep is not None and s.count(sep) >= 2:
                    first = s.index(sep)
                    tt = torch.tensor([[0 if i <= first else 1 for i in range(lmax)]])
                    kw2["token_type_ids"] = tt
                    rec["token_type_ids"] = tt[0].tolist()
                    rec["logits_segments"] = rows_of(fresh(**kw2), head)[:n].tolist()
            out.append(rec)
    with open(os.path.join(d, "outputs.json"), "w") as f:
        json.dump({"kind": f"{head}:{pad}:{lmax}", "sequences": out, "transformers": transformers.__version__, "torch": torch.__version__,
                   "seed": seed, "weights": "bfloat16-exact (rounded before the forward)", "attn_implementation": "eager",
                   "tensors": sorted(fresh.state_dict().keys())}, f)
    for extra in ("generation_config.json",):
        p = os.path.join(d, extra)
        if os.path.exists(p):
            os.remove(p)
    return max(abs(x) for r in out for x in (r["logits"] if head == "mlm" else [v for row in r["logits"] for v in row]))


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1 (fixtures are built from local configs only)")
    names = sys.argv[1:] or list(CONFIGS)
    bad = 0
    for n in names:
        try:
            print(f"ok   {n:18s} max|logit| {make(n):.3f}")
        except Exception as e:  # noqa: BLE001
            bad += 1
            print(f"FAIL {n:18s} {type(e).__name__}: {str(e)[:300]}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
