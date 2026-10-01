#!/usr/bin/env python3
"""Generate Hugging Face reference fixtures for the encoder-decoder lowering (RFC-0003 §II.2.1's text
stage behind an encoder stage).

Each tiny sequence-to-sequence model below is built with random weights, re-randomised and rounded
to bfloat16 exactly as `gen_hf_fixtures.py` does, saved, reloaded fresh in f32 with eager attention,
and run on two seeded source sequences (each ends with the model's EOS, as its tokenizer adds it):

* `generate` greedy (`do_sample=False`, one beam) for 8 new ids, under an explicit
  `GenerationConfig` so no model-config default (forced BOS/EOS, n-gram blocking) applies, and
  with no EOS stop (an EOS id is fed back like any other);
* the decoder's logits over the stream `[decoder_start] + generated[:-1]` (teacher-forced), and over
  a random stream `[decoder_start] + 7 random ids` (a model this small often repeats one id);
* the encoder's last hidden state;
* T5 only: `_relative_position_bucket` over the distances the fixtures reach, both directions.

    tests/fixtures/hf-encdec/<name>/config.json, model.safetensors (BF16)
    tests/fixtures/hf-encdec/<name>/outputs.json

Usage:
    HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 python tools/gen_hf_encdec_fixtures.py [name ...]
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
FIX = os.path.join(CRATE, "tests", "fixtures", "hf-encdec")
V = 64
NEW_TOKENS = 8
SRC_LENS = (10, 7)

BART_DIMS = dict(vocab_size=V, d_model=32, encoder_layers=2, decoder_layers=2, encoder_attention_heads=4,
                 decoder_attention_heads=4, encoder_ffn_dim=64, decoder_ffn_dim=64, max_position_embeddings=64)

CONFIGS = {
    # Original T5: ReLU FFN, the head tied to the shared table and the decoder output scaled by d^-0.5.
    "t5": dict(cls="T5Config", model="T5ForConditionalGeneration", embed_mul=0.3,
               kw=dict(vocab_size=V, d_model=32, d_kv=8, d_ff=64, num_layers=2, num_decoder_layers=2, num_heads=4,
                       relative_attention_num_buckets=8, relative_attention_max_distance=8, feed_forward_proj="relu",
                       pad_token_id=0, eos_token_id=1, decoder_start_token_id=0)),
    # T5 v1.1 / Flan: gated-GELU FFN, an untied head and no output scaling; d_kv·heads ≠ d_model.
    "t5_gated": dict(cls="T5Config", model="T5ForConditionalGeneration", untie=True, embed_mul=0.1,
                     kw=dict(vocab_size=V, d_model=32, d_kv=6, d_ff=48, num_layers=2, num_decoder_layers=2, num_heads=4,
                             relative_attention_num_buckets=8, relative_attention_max_distance=8,
                             feed_forward_proj="gated-gelu", tie_word_embeddings=False, pad_token_id=0, eos_token_id=1,
                             decoder_start_token_id=0)),
    # BART: post-norm, learned positions (offset 2), layernorm_embedding, final_logits_bias.
    "bart": dict(cls="BartConfig", model="BartForConditionalGeneration", embed_mul=0.3,
                 kw=dict(BART_DIMS, activation_function="gelu", scale_embedding=False, pad_token_id=1, bos_token_id=0,
                         eos_token_id=2, decoder_start_token_id=2, forced_bos_token_id=None, forced_eos_token_id=None)),
    # mBART: pre-norm with final layer norms, √d embedding scale, layernorm_embedding.
    "mbart": dict(cls="MBartConfig", model="MBartForConditionalGeneration", embed_mul=0.1,
                  kw=dict(BART_DIMS, activation_function="gelu", scale_embedding=True, pad_token_id=1, bos_token_id=0,
                          eos_token_id=2, decoder_start_token_id=2, forced_bos_token_id=None, forced_eos_token_id=None)),
    # Marian: post-norm, sinusoidal positions (not saved), √d scale, SiLU.
    "marian": dict(cls="MarianConfig", model="MarianMTModel", embed_mul=0.3,
                   kw=dict(BART_DIMS, activation_function="swish", scale_embedding=True, pad_token_id=63, eos_token_id=0,
                           decoder_start_token_id=63, forced_eos_token_id=None)),
    # Pegasus: pre-norm with final layer norms, sinusoidal positions (saved), √d scale.
    "pegasus": dict(cls="PegasusConfig", model="PegasusForConditionalGeneration", embed_mul=0.5, seed_off=1,
                    kw=dict(BART_DIMS, activation_function="gelu", scale_embedding=True, pad_token_id=0, eos_token_id=1,
                            decoder_start_token_id=0, forced_eos_token_id=None)),
}


OUT_GAIN = 8.0


def out_norm_names(spec, cfg):
    """The norm whose output is the decoder's output (the head's input)."""
    if spec["cls"] == "T5Config":
        return {"decoder.final_layer_norm.weight"}
    if spec["cls"] in ("MBartConfig", "PegasusConfig"):
        return {"model.decoder.layer_norm.weight", "model.decoder.layer_norm.bias"}
    last = cfg.decoder_layers - 1
    return {f"model.decoder.layers.{last}.final_layer_norm.weight", f"model.decoder.layers.{last}.final_layer_norm.bias"}


def sinusoid_restore(model):
    """Put the sinusoidal tables back after `randomise` (they are frozen parameters)."""
    for m in model.modules():
        if hasattr(m, "create_weight") and isinstance(m, torch.nn.Embedding):
            with torch.no_grad():
                m.weight.copy_(m.create_weight())


def make(name):
    spec = CONFIGS[name]
    seed = sum(ord(ch) for ch in name) + spec.get("seed_off", 0)
    torch.manual_seed(seed)
    cfg = getattr(transformers, spec["cls"])(**spec["kw"])
    cls = getattr(transformers, spec["model"])
    model = cls(cfg)
    model.eval()
    randomise(model, cfg.d_model, seed)
    sinusoid_restore(model)
    # A tied table of randn rows maps every token to itself through a network this small (the
    # embedding dominates the residual): scale it so the layers decide the next id.
    with torch.no_grad():
        emb = model.get_input_embeddings().weight
        emb.mul_(spec.get("embed_mul", 1.0))
        emb.copy_(emb.to(torch.bfloat16).to(torch.float32))
        # A tied head over that small table gives nearly uniform logits: scale the decoder's output
        # norm instead (the logits scale with it; the ids the model prefers barely move).
        if not spec.get("untie"):
            for n, prm in model.named_parameters():
                if n in out_norm_names(spec, cfg):
                    prm.mul_(OUT_GAIN)
                    prm.copy_(prm.to(torch.bfloat16).to(torch.float32))
    if spec.get("untie"):
        with torch.no_grad():
            g = torch.Generator().manual_seed(seed + 7)
            w = torch.randn(model.lm_head.weight.shape, generator=g).to(torch.bfloat16).to(torch.float32)
            model.lm_head.weight = torch.nn.Parameter(w)
    d = os.path.join(FIX, name)
    os.makedirs(d, exist_ok=True)
    model.to(torch.bfloat16)
    model.save_pretrained(d)
    fresh = cls.from_pretrained(d, dtype=torch.float32, attn_implementation="eager")
    fresh.eval()
    if spec.get("untie"):
        assert not torch.equal(fresh.lm_head.weight, fresh.get_input_embeddings().weight), "the head is still tied"
    # No EOS stop: every record generates 8 ids (an EOS id is fed back like any other).
    fresh.generation_config.eos_token_id = None
    gc = transformers.GenerationConfig(max_new_tokens=NEW_TOKENS, do_sample=False, num_beams=1,
                                       decoder_start_token_id=cfg.decoder_start_token_id,
                                       eos_token_id=None, pad_token_id=cfg.pad_token_id)
    rng = np.random.default_rng(seed)
    eos = cfg.eos_token_id
    recs = []
    with torch.no_grad():
        for n in SRC_LENS:
            body = [int(t) for t in rng.integers(3, V - 4, size=n - 1)]
            src = body + [eos]
            ids = torch.tensor([src])
            gen = fresh.generate(input_ids=ids, generation_config=gc)[0].tolist()
            assert gen[0] == cfg.decoder_start_token_id, gen
            gen = gen[1:]
            dec = [cfg.decoder_start_token_id] + gen[:-1]
            o = fresh(input_ids=ids, decoder_input_ids=torch.tensor([dec]))
            enc = fresh.get_encoder()(input_ids=ids).last_hidden_state[0]
            # A random decoder stream too: the generations of a model this small repeat ids.
            rnd = [cfg.decoder_start_token_id] + [int(t) for t in rng.integers(0, V, size=NEW_TOKENS - 1)]
            ro = fresh(input_ids=ids, decoder_input_ids=torch.tensor([rnd]))
            recs.append({"input_ids": src, "generated": gen, "decoder_input_ids": dec,
                         "logits": o.logits[0].tolist(), "encoder_hidden": enc.tolist(),
                         "random_decoder_ids": rnd, "random_logits": ro.logits[0].tolist()})
    meta = {"records": recs, "transformers": transformers.__version__, "torch": torch.__version__, "seed": seed,
            "weights": "bfloat16-exact (rounded before the forward)", "attn_implementation": "eager",
            "decoder_start_token_id": cfg.decoder_start_token_id, "eos_token_id": eos}
    if spec["cls"] == "T5Config":
        from transformers.models.t5.modeling_t5 import T5Attention
        nb, md = cfg.relative_attention_num_buckets, cfg.relative_attention_max_distance
        rel = torch.arange(-40, 41)
        meta["buckets_bidirectional"] = {"from": -40, "buckets": T5Attention._relative_position_bucket(
            rel, bidirectional=True, num_buckets=nb, max_distance=md).tolist()}
        # The decoder's relative position is `key − query ≤ 0`.
        meta["buckets_causal"] = {"from": -40, "buckets": T5Attention._relative_position_bucket(
            rel, bidirectional=False, num_buckets=nb, max_distance=md).tolist()}
        meta["scale_decoder_outputs"] = bool(fresh.config.scale_decoder_outputs)
    with open(os.path.join(d, "outputs.json"), "w") as f:
        json.dump(meta, f)
    return [(r["generated"], len(r["input_ids"])) for r in recs]


def main():
    if os.environ.get("HF_HUB_OFFLINE") != "1":
        sys.exit("refusing to run without HF_HUB_OFFLINE=1 (fixtures are built from local configs only)")
    names = sys.argv[1:] or list(CONFIGS)
    bad = 0
    for n in names:
        try:
            k = make(n)
            print(f"ok   {n:10s} {k}")
        except Exception as e:  # report and continue
            import traceback
            traceback.print_exc()
            bad += 1
            print(f"FAIL {n:10s} {type(e).__name__}: {str(e)[:300]}")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
