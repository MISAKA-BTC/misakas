"""Decode two existing QF1 runs and measure endpoint occupancy in the archived readout."""
import argparse
import hashlib
import json
import re
import struct
import subprocess
from pathlib import Path

from tokenizers import Tokenizer

from check_real_lifecycle import check

HERE = Path(__file__).resolve().parent
TOKENIZER = "aeb13307a71acd8fe81861d94ad54ab689df773318809eed3cbe794b4492dae4"


def json_file(name):
    return json.loads((HERE / name).read_text())


def decode_case(tokenizer, ids, answer, stop):
    text = tokenizer.decode(ids, skip_special_tokens=False)
    attempt = re.fullmatch(
        r"<\|im_start\|>system\nYou are a helpful assistant\.\nPALW challenge: ([0-9a-f]{64})\n"
        r"Treat the challenge as metadata\. Answer the user's request\.\n<\|im_end\|>\n"
        r"<\|im_start\|>user\n2 \+ 2\?<\|im_end\|>\n<\|im_start\|>assistant\n<think>\n\n</think>\n\n", text)
    if not attempt or len(answer) != 3 or stop != "Limit":
        raise ValueError("unexpected job, frame or generation budget")
    output = tokenizer.decode(answer, skip_special_tokens=False)
    return dict(attempt=attempt[1], output_ids=answer, text=output, stop=stop, contains_digit_four="4" in output)


def measure(path):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != TOKENIZER:
        raise ValueError("registered tokenizer mismatch")
    tokenizer = Tokenizer.from_buffer(raw)
    native = check((HERE / "real-three-peer.txt").read_text(), json_file("class-check.json"))
    signed = decode_case(tokenizer, native["input_ids"], native["output_ids"], native["stop"])
    if signed["attempt"] != native["attempt"]:
        raise ValueError("encoded challenge mismatch")
    signed["original"] = native["panel"]["original"]
    signed["processes"] = native["court"]["pids"]
    replay = json_file("full-replay.json")
    independent = json_file("full-replay-check.json")
    if replay["output_ids"] != independent["output_ids"] or not independent["all_logits_matched"]:
        raise ValueError("independent numerical result mismatch")
    other = decode_case(tokenizer, json_file("full-replay-input.json"), replay["output_ids"], replay["stop"])
    if other["attempt"] == signed["attempt"]:
        raise ValueError("expected distinct challenges")
    readout = subprocess.check_output(["zstd", "-dc", HERE / "full-readout.i16le.zst"])
    width, vectors = 4096, 37
    if len(readout) != 2 * width * vectors * len(other["output_ids"]) or hashlib.sha256(readout).hexdigest() != replay["readout_i16le_sha256"]:
        raise ValueError("wrong full-replay readout")
    counts = [0] * vectors
    for i, (value,) in enumerate(struct.iter_unpack("<h", readout)):
        if value == -32768:
            raise ValueError("noncanonical activation")
        counts[i // width % vectors] += abs(value) == 32767
    return dict(job="2 + 2?", tokenizer_sha256=TOKENIZER, signed_original=signed, independent_full_replay=other,
                readout_sha256=replay["readout_i16le_sha256"], values_per_vector_across_positions=width * len(other["output_ids"]),
                residual_endpoint_counts=counts[:-1], final_norm_endpoint_count=counts[-1])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tokenizer", type=Path)
    print(json.dumps(measure(parser.parse_args().tokenizer), indent=2, ensure_ascii=False))
