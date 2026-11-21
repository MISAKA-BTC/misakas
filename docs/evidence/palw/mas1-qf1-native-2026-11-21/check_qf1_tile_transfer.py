"""Decode two previously unevaluated jobs using the same frozen integer conversion candidate."""
import argparse
import hashlib
import json
import struct
from pathlib import Path

from tokenizers import Tokenizer

from check_qf1_answers import HERE, TOKENIZER, json_file
from check_residual_scale import decode_integer_reference, open_tensors, qf1


def run(artifact, tokenizer_path):
    raw = tokenizer_path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != TOKENIZER:
        raise ValueError("registered tokenizer mismatch")
    tokenizer = Tokenizer.from_buffer(raw)
    plan_bytes = (HERE / "qf1-residual-tiles.json").read_bytes()
    plan = json.loads(plan_bytes)
    if plan["class_id"] != json_file("real-lifecycle.json")["class_id"] or plan["residual_tile_width"] != 64:
        raise ValueError("not the held conversion candidate")
    bits = plan["evaluation"]["first_prediction"]["layer_bits"]
    if len(bits) != 36 or any(len(row) != 64 for row in bits):
        raise ValueError("wrong residual tile geometry")
    tensors = open_tensors(artifact)
    def encode(s):
        return tokenizer.encode(s, add_special_tokens=False).ids

    prefix = encode("<|im_start|>system\nYou are a helpful assistant.\nPALW challenge: ")
    middle = encode("\nTreat the challenge as metadata. Answer the user's request.\n<|im_end|>\n<|im_start|>user\n")
    suffix = encode("<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n")
    hex_ids = [encode(c) for c in "0123456789abcdef"]
    if any(len(v) != 1 for v in hex_ids) or len({v[0] for v in hex_ids}) != 16:
        raise ValueError("invalid HEX64 alphabet")
    result = []
    for ordinal, job in enumerate(("3 + 5?", "What is the capital of France?")):
        # Diagnostic public strings. Neither challenge comes from a paying TB1 EXEC.
        seed = hashlib.sha256(f"MISAKA QF1 frozen tile transfer {ordinal}".encode("ascii")).digest()
        ids = prefix + qf1.hex_frame(seed, [v[0] for v in hex_ids]) + middle + encode(job) + suffix
        answer = decode_integer_reference(tensors, ids, encode("4")[0], tokenizer, bits, 32, plan["stage_bits"])
        result.append(dict(source=job, diagnostic_challenge=seed.hex(), input_ids=ids,
                           input_sha256=hashlib.sha256(struct.pack(f"<{len(ids)}I", *ids)).hexdigest(),
                           output_ids=answer["output_ids"], logits_sha256=answer["logits_sha256"], text=answer["text"],
                           stop=answer["schedule"]["stop_reason"]))
    return dict(calibration_sha256=hashlib.sha256(plan_bytes).hexdigest(), class_id=plan["class_id"], limit=32, jobs=result)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path)
    parser.add_argument("tokenizer", type=Path)
    args = parser.parse_args()
    print(json.dumps(run(args.artifact,args.tokenizer), indent=2, ensure_ascii=False))
