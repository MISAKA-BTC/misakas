#!/usr/bin/env python3
"""Attach human-readable text to jobs-export.json.

QWEN25-A16 decodes exactly (HF tokenizer.json). QWEN36 decodes via the tokenizer embedded in
the source GGUF (llama.cpp vocab): token strings are the byte-level BPE surface forms, joined
and de-byte-mapped — exact for ordinary text, and the anchor-derived prompt is random token
ids so its text is EXPECTED to read as gibberish; showing it is the point (the input is a
deterministic lottery draw, not prose)."""
import json, sys

EXPORT = "/root/palw-class/jobs-export.json"
OUT    = "/root/palw-class/llm-jobs.json"
Q25_TOK = "/root/palw-class/qwen25-src/tokenizer.json"
Q36_GGUF = "/root/palw-class/Qwen3.6-abliterated-35b-Claude-4.7-Q4_K_M.gguf"

# byte-level BPE inverse map (GPT-2 style), for GGUF surface forms
def bytes_map():
    bs = list(range(33,127)) + list(range(161,173)) + list(range(174,256))
    cs = bs[:]
    n = 0
    for b in range(256):
        if b not in bs:
            bs.append(b); cs.append(256+n); n += 1
    return {chr(c): b for b, c in zip(bs, cs)}
BMAP = bytes_map()
def surface_to_text(pieces):
    buf = bytearray()
    for p in pieces:
        for ch in p:
            b = BMAP.get(ch)
            if b is None:
                buf.extend(ch.encode("utf-8"))
            else:
                buf.append(b)
    return buf.decode("utf-8", errors="replace")

_q25 = None
def q25_decode(ids):
    global _q25
    if _q25 is None:
        from tokenizers import Tokenizer
        _q25 = Tokenizer.from_file(Q25_TOK)
    return _q25.decode(ids, skip_special_tokens=False)

_q36 = None
def q36_tokens():
    global _q36
    if _q36 is None:
        from gguf import GGUFReader
        r = GGUFReader(Q36_GGUF)
        f = r.get_field("tokenizer.ggml.tokens")
        _q36 = [bytes(f.parts[i]).decode("utf-8", errors="replace") for i in f.data]
    return _q36
def q36_decode(ids):
    toks = q36_tokens()
    return surface_to_text([toks[i] if 0 <= i < len(toks) else f"<id:{i}>" for i in ids])

d = json.load(open(EXPORT))
for row in d["rows"]:
    dec = q36_decode if row["class"] == "QWEN36" else q25_decode
    try:
        row["prompt_text"] = dec([int(x) for x in row["prompt_ids"]])
    except Exception as e:
        row["prompt_text"] = None
    # The answer is not in the export any more and must not be invented here: the chain carries
    # output_root, the ids reach the claim's drawn seats, and a demand answered on chain is the
    # only thing that publishes one (ADR-0062). No key at all, so a stale page cannot read a null
    # as "still loading".

# A free prompt is the author's: the export carries no ids for it and this step adds no text.

# Which block carried each free-prompt commitment (fp_blocks.py's cache, keyed by claim). These rows
# come from the executor's outbox, which knows the txid the rail submitted and nothing about where
# it landed, and the page's own sweep decodes only the 0x4b lifecycle band while an FP commitment
# rides 0x4a — so without this the Block and Age columns printed "—" for the one lane whose words a
# person actually chose.
try:
    _fp_blocks = (json.load(open("/root/palw-class/fp-blocks.json")) or {}).get("claims") or {}
except Exception:
    _fp_blocks = {}
for r in (d.get('fp_rows') or []):
    hit = _fp_blocks.get(r.get('claim'))
    if hit:
        r['block'] = hit.get('block')
        r['ts'] = hit.get('ts')

json.dump(d, open(OUT, "w"), ensure_ascii=False)
print("textified", len(d["rows"]), "rows ->", OUT)
