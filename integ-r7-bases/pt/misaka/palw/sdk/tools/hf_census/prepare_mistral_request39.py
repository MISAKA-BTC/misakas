#!/usr/bin/env python3
"""Pinned original Mistral multimodal frontend -> canonical native39 job inputs.

This is an actual fixed-library adapter, not an external-command callback. It
executes the publisher's tokenizer/normalizer/tool template and image encoder.
No inference/Final/eligibility is asserted. The new integer processor41 remains
subject to independent float/OpenCV fidelity measurements.
"""
from __future__ import annotations
import argparse
import copy
import hashlib
import importlib.metadata
import json
import pathlib
import sys

SCHEMA = "misaka.palw.mistral-original-request-adapter.v39"
PINNED_MODULES = {
    "tokens/tokenizers/mistral.py": "c45f7ad29023cb68cc92fb2176d059303c642678d51caac04b546f2a7ccc35b3",
    "tokens/tokenizers/instruct.py": "bd404ce28d8e2bb0db521f4952e49d991f5ad1bb2c5b7337e590707d29ba4ea1",
    "tokens/tokenizers/multimodal.py": "a565c6bbb55070534478ac01edfde6cc5364a2b5b70b35a2fa18746450190ecf",
    "tokens/tokenizers/tekken.py": "d9417a854e900f3dcaca435a69141c85b1839e29c7a0553402b2c839f949ebeb",
    "tokens/tokenizers/base.py": "91417e64cfe3917b434568d4a4eabdee5653b7917900888b8be286f6132669d7",
    "protocol/instruct/request.py": "b0c337364c5732c03edaeeed893194c2e37672def55ba0c4222ec1caf2990e38",
    "protocol/instruct/messages.py": "bacde23120607beee03f592b5583a1495eb5fd621b4fb0394921e0c11a08cfac",
}

def fail(message: str):
    raise ValueError(message)


def sha(path: pathlib.Path, maximum: int) -> str:
    size = path.stat().st_size
    if size > maximum:
        fail("MISTRAL39_FILE_EXTENT")
    digest = hashlib.sha256()
    total = 0
    with path.open("rb") as handle:
        while block := handle.read(1 << 20):
            total += len(block)
            if total > maximum:
                fail("MISTRAL39_FILE_GREW")
            digest.update(block)
    return digest.hexdigest()


def pinned(path: pathlib.Path, expected: str, maximum: int) -> bytes:
    if len(expected) != 64 or sha(path, maximum) != expected:
        fail("MISTRAL39_INDEPENDENT_FILE_SHA_DIFFERS: " + str(path))
    data = path.read_bytes()
    if len(data) > maximum or hashlib.sha256(data).hexdigest() != expected:
        fail("MISTRAL39_FILE_CHANGED")
    return data


def environment(path: pathlib.Path, expected: str) -> dict:
    profile = json.loads(pinned(path, expected, 16 << 20))
    if profile.get("schema") != "misaka.palw.original-python-environment.v39":
        fail("MISTRAL39_OWNING_ENVIRONMENT_SCHEMA")
    if profile.get("python_version") != sys.version or profile.get("python_sha256") != sha(pathlib.Path(sys.executable).resolve(), 128 << 20):
        fail("MISTRAL39_ORIGINAL_PYTHON_BINARY_DIFFERS")
    packages = profile.get("packages")
    if not isinstance(packages, dict) or not 6 <= len(packages) <= 256:
        fail("MISTRAL39_PINNED_PACKAGE_SET")
    required = {"mistral-common", "numpy", "pillow", "tiktoken", "pydantic"}
    normalized = {name.lower().replace("_", "-"): row for name, row in packages.items()}
    if not required <= normalized.keys() or not ({"opencv-python", "opencv-python-headless"} & normalized.keys()):
        fail("MISTRAL39_ORIGINAL_NUMPY_CV2_PIL_DEPENDENCIES_REQUIRED")
    # Every file the selected distributions declare must be independently pinned;
    # a version string alone does not authenticate an imported implementation.
    count = 0
    for name, row in packages.items():
        distribution = importlib.metadata.distribution(name)
        if row.get("version") != distribution.version:
            fail("MISTRAL39_DEPENDENCY_VERSION_DIFFERS: " + name)
        expected_files = row.get("files")
        if not isinstance(expected_files, dict) or not expected_files:
            fail("MISTRAL39_DEPENDENCY_FULL_FILE_PINS_REQUIRED")
        actual = {str(file) for file in distribution.files or () if not str(file).endswith((".pyc", ".pyo", "/RECORD"))}
        if actual != set(expected_files):
            fail("MISTRAL39_DEPENDENCY_FILE_COVERAGE: " + name)
        for relative in sorted(actual):
            count += 1
            if count > 200000:
                fail("MISTRAL39_DEPENDENCY_FILE_COUNT")
            file = pathlib.Path(distribution.locate_file(relative)).resolve()
            if sha(file, 512 << 20) != expected_files[relative]:
                fail("MISTRAL39_DEPENDENCY_FILE_SHA: " + relative)
    if importlib.metadata.version("mistral-common") != "1.6.2":
        fail("MISTRAL39_UNSUPPORTED_UNPINNED_TEMPLATE_REVISION")
    root = pathlib.Path(importlib.metadata.distribution("mistral-common").locate_file("mistral_common")).resolve()
    for relative, digest in PINNED_MODULES.items():
        if sha(root / relative, 1 << 20) != digest:
            fail("MISTRAL39_PUBLISHER_V162_TEMPLATE_SOURCE_DIFFERS: " + relative)
    return profile


def image_geometry(height: int, width: int, metadata: dict) -> tuple[int, int, list[int]]:
    maximum = metadata["max_image_size"]
    patch = metadata["patch_size"]
    merge = metadata["spatial_merge_size"]
    if not 0 < height <= 1 << 20 or not 0 < width <= 1 << 20 or height * width * 3 * 16 > 2 << 30:
        fail("MISTRAL39_ORIGINAL_RASTER_EXTENT")
    ratio = max(height / maximum, width / maximum)
    if ratio > 1:
        width, height = round(width / ratio), round(height / ratio)
    columns = (width - 1) // (patch * merge) + 1
    rows = (height - 1) // (patch * merge) + 1
    if not columns or not rows:
        fail("MISTRAL39_PUBLISHER_ZERO_IMAGE_AXIS")
    tokens = []
    for row in range(rows):
        tokens.extend([metadata["image_token_id"]] * columns)
        tokens.append(metadata["image_end_token_id"] if row + 1 == rows else metadata["image_break_token_id"])
    return rows * patch * merge, columns * patch * merge, tokens


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("request", "tokenizer", "params", "generation-config", "image-bindings", "environment"):
        parser.add_argument("--" + name, type=pathlib.Path, required=True)
        parser.add_argument("--" + name + "-sha256", required=True)
    parser.add_argument("--requested-new-tokens", type=int, required=True)
    parser.add_argument("--out", type=pathlib.Path, required=True)
    args = parser.parse_args()
    if args.out.exists():
        fail("MISTRAL39_FRESH_REQUEST_OUTPUT_REQUIRED")
    environment(args.environment, args.environment_sha256)
    raw = pinned(args.request, args.request_sha256, 16 << 20)
    request = json.loads(raw)
    tokenizer_raw = pinned(args.tokenizer, args.tokenizer_sha256, 64 << 20)
    tokenizer_json = json.loads(tokenizer_raw)
    params = json.loads(pinned(args.params, args.params_sha256, 4 << 20))
    generation = json.loads(pinned(args.generation_config, args.generation_config_sha256, 4 << 20))
    bindings = json.loads(pinned(args.image_bindings, args.image_bindings_sha256, 4 << 20))
    if tokenizer_json["config"]["version"] != "v11" or params["vision_encoder"]["mm_projector_id"] != "patch_merge":
        fail("MISTRAL39_UNSUPPORTED_TOKENIZER_OR_PROCESSOR_REVISION")
    metadata = params["vision_encoder"]
    mm = tokenizer_json["multimodal"]
    if (mm["image_patch_size"], mm["max_image_size"], mm["spatial_merge_size"]) != (metadata["patch_size"], metadata["max_image_size"], metadata["spatial_merge_size"]):
        fail("MISTRAL39_TOKENIZER_ORIGINAL_PROCESSOR_DIFFERS")
    specials = {item["token_str"]: item["rank"] for item in tokenizer_json["special_tokens"]}
    if (specials["[IMG]"], specials["[IMG_BREAK]"], specials["[IMG_END]"]) != (metadata["image_token_id"], metadata["image_break_token_id"], metadata["image_end_token_id"]):
        fail("MISTRAL39_ORIGINAL_IMAGE_SPECIAL_IDS_DIFFER")
    if not isinstance(bindings, list) or len(bindings) > 10:
        fail("MISTRAL39_ORIGINAL_IMAGE_BINDING_COUNT")
    if request.get("truncate_for_context_length"):
        fail("MISTRAL39_SILENT_TASK_TRUNCATION_FORBIDDEN")
    # No arbitrary URL/file path is evaluated by the original library. Every
    # image reference is replaced with the independently pinned local image.
    from PIL import Image
    import numpy as np
    from mistral_common.tokens.tokenizers.multimodal import _convert_to_rgb
    from mistral_common.tokens.tokenizers.mistral import MistralTokenizer
    from mistral_common.protocol.instruct.request import ChatCompletionRequest
    copied = copy.deepcopy(request)
    rasters = []
    image_rows = []
    seen = set()
    for binding in bindings:
        key = (binding["message"], binding["chunk"])
        if key in seen:
            fail("MISTRAL39_DUPLICATE_ORIGINAL_IMAGE_COORDINATE")
        seen.add(key)
        image_path = pathlib.Path(binding["path"])
        pinned(image_path, binding["sha256"], 256 << 20)
        image = Image.open(image_path)
        if image.width * image.height * 3 * 16 > 2 << 30:
            fail("MISTRAL39_DECOMPRESSED_IMAGE_BOUND")
        image.load()
        rgb = _convert_to_rgb(image)
        array = np.asarray(rgb, dtype=np.uint8)
        original_height, original_width = array.shape[:2]
        resized_height, resized_width, tokens = image_geometry(original_height, original_width, metadata)
        chunk = copied["messages"][key[0]]["content"][key[1]]
        if chunk.get("type") not in ("image", "image_url"):
            fail("MISTRAL39_ORIGINAL_IMAGE_COORDINATE_TYPE")
        copied["messages"][key[0]]["content"][key[1]] = {"type": "image", "image": image}
        rasters.append(array)
        image_rows.append({"message": key[0], "chunk": key[1], "original": [original_height, original_width], "resized": [resized_height, resized_width], "image_tokens": tokens, "source_sha256": binding["sha256"]})
    if list(seen) and [(b["message"], b["chunk"]) for b in bindings] != sorted(seen):
        fail("MISTRAL39_IMAGE_BINDING_ORDER_DIFFERS_FROM_ORIGINAL_REQUEST")
    for m, message in enumerate(copied["messages"]):
        if isinstance(message.get("content"), list):
            for c, chunk in enumerate(message["content"]):
                if chunk.get("type") in ("image", "image_url") and (m, c) not in seen:
                    fail("MISTRAL39_UNPINNED_ORIGINAL_IMAGE_OR_URL")
    parsed = ChatCompletionRequest.model_validate(copied)
    tokenizer = MistralTokenizer.from_file(str(args.tokenizer))
    encoded = tokenizer.encode_chat_completion(parsed)
    ids = list(encoded.tokens)
    if not ids or any(not isinstance(token, int) or not 0 <= token < tokenizer_json["config"]["default_vocab_size"] for token in ids):
        fail("MISTRAL39_ORIGINAL_TOKEN_IDS_OUTSIDE_SOURCE_VOCABULARY")
    if not 0 < args.requested_new_tokens or len(ids) + args.requested_new_tokens - 1 > params["max_position_embeddings"]:
        fail("MISTRAL39_ORIGINAL_REQUEST_EXCEEDS_FULL_CONTEXT")
    if len(encoded.images) != len(image_rows):
        fail("MISTRAL39_ORIGINAL_IMAGE_OUTPUT_COVERAGE")
    cursor = 0
    spans = []
    special_ids = set((metadata["image_token_id"], metadata["image_break_token_id"], metadata["image_end_token_id"]))
    for row in image_rows:
        while cursor < len(ids) and ids[cursor] not in special_ids:
            cursor += 1
        tokens = row["image_tokens"]
        if ids[cursor:cursor + len(tokens)] != tokens:
            fail("MISTRAL39_ORIGINAL_IMAGE_TOKEN_SPAN_DIFFERS")
        spans.append({"start": cursor, "tokens": len(tokens)})
        cursor += len(tokens)
    if any(token in special_ids for token in ids[cursor:]):
        fail("MISTRAL39_EXTRA_ORIGINAL_IMAGE_TOKEN_SPAN")
    eos = generation.get("eos_token_id")
    eos = [eos] if isinstance(eos, int) else eos
    if not isinstance(eos, list) or not eos or len(eos) != len(set(eos)) or any(not isinstance(t, int) or not 0 <= t < tokenizer_json["config"]["default_vocab_size"] for t in eos):
        fail("MISTRAL39_ORIGINAL_EOS_POLICY_PIN_REQUIRED")
    args.out.mkdir()
    for i, (raster, source_image, row) in enumerate(zip(rasters, encoded.images, image_rows)):
        if list(source_image.shape) != [3, *row["resized"]]:
            fail("MISTRAL39_ORIGINAL_FLOAT_IMAGE_GEOMETRY_DIFFERS")
        np.save(args.out / f"image-{i}.original-float.npy", np.asarray(source_image), allow_pickle=False)
        (args.out / f"image-{i}.rgb8").write_bytes(raster.tobytes(order="C"))
        (args.out / f"image-{i}.input-i16le").write_bytes(raster.astype("<i2").tobytes(order="C"))
        row.update({"native_input_file": f"image-{i}.input-i16le", "reference_float_file": f"image-{i}.original-float.npy", "reference_float_sha256": sha(args.out / f"image-{i}.original-float.npy", 256 << 20), "native_input_sha256": sha(args.out / f"image-{i}.input-i16le", 256 << 20)})
    # Recheck the immutable source inputs after every original library call.
    for name in ("request", "tokenizer", "params", "generation_config", "image_bindings", "environment"):
        pinned(getattr(args, name), getattr(args, name + "_sha256"), 64 << 20)
    for binding in bindings:
        pinned(pathlib.Path(binding["path"]), binding["sha256"], 256 << 20)
    result = {"schema": SCHEMA, "source_pins": {name: getattr(args, name + "_sha256") for name in ("request", "tokenizer", "params", "generation_config", "image_bindings", "environment")}, "template": "mistral-common==1.6.2/Tekken-v11", "prompt": ids, "requested_new_tokens": args.requested_new_tokens, "eos": sorted(eos), "maximum_positions": len(ids) + args.requested_new_tokens - 1, "original_context_max": params["max_position_embeddings"], "images": image_rows, "image_token_spans": spans, "source_fidelity": "NOT_RUN", "native_final": "NOT_RUN", "a6_eligible": False, "a8_eligible": False}
    (args.out / "request39.json").write_text(json.dumps(result, sort_keys=True, indent=2) + "\n")
    return 0

if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (ValueError, KeyError, OSError, TypeError, IndexError) as error:
        print("MISTRAL39_REFUSED: " + str(error), file=sys.stderr)
        raise SystemExit(2)
