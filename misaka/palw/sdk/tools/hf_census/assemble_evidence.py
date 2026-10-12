#!/usr/bin/env python3
"""Assemble the machine-readable evidence of the dated coverage report from `coverage_report.py`'s output (no network).

    assemble_evidence.py --coverage DIR/p4/report/coverage.json --update2 DIR/report-v3/report.json --snapshot DIR \
        --binary BIN --out docs/rfc/evidence/0011-hf-census-coverage-2026-10-08.json

Writes the file `docs/rfc/evidence/0011-census-coverage-check.mjs` checks. The old evidence (`0011-existing-coverage-audit.json`) is not
touched. The blocker families and the generic change that would close each are `FAMILIES` below: a code (and, where the argument
decides it, an argument prefix) → a label and the closing feature; nothing here reads a model name.
"""
import argparse
import hashlib
import json
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from buckets import CLOSABLE  # noqa: E402

# (code, argument prefix or None) -> (family, the generic feature or profile that would close it)
FAMILIES = [
    ("MODALITY_PROFILE_MISSING", ("text-classification", "token-classification", "zero-shot-classification", "text-ranking", "multiple-choice", "question-answering", "table-question-answering", "fill-mask"),
     "NLU heads (classification / token / QA / fill-mask)",
     "a canonical job profile for encoder heads: label / span / token logits as the committed output, the label set in the class (the frontend already lowers a sequence classifier as an Embedding-profile class); a `tasks.rs` row per head + one RFC-0003 profile"),
    ("MODALITY_PROFILE_MISSING", ("image-classification", "object-detection", "image-segmentation", "depth-estimation", "zero-shot-image-classification", "zero-shot-object-detection", "mask-generation", "keypoint-detection", "video-classification", "image-to-image", "unconditional-image-generation", "image-to-3d", "text-to-3d", "image-text-to-image"),
     "vision heads (classification / detection / segmentation / dense)",
     "the CNN / ViT route (`read_cnn`, `read_vision`) exists as stage programs: needs RFC-0003 Image-input job profiles for class labels, boxes and masks (JobImage is bound) and their output kinds"),
    ("MODALITY_PROFILE_MISSING", ("automatic-speech-recognition", "text-to-speech", "text-to-audio", "audio-classification", "audio-to-audio", "voice-activity-detection"),
     "audio tasks (ASR / TTS / audio classification)",
     "a `JobAudio` binding (FR-23: feature frames) and an audio output kind: the Whisper / wav2vec2 stage programs lower, no protocol job supplies frames"),
    ("MODALITY_PROFILE_MISSING", ("text-to-video", "image-to-video", "video-to-video", "reinforcement-learning", "robotics", "tabular-classification", "tabular-regression", "time-series-forecasting", "graph-ml", "other"),
     "RL / robotics / tabular / time series / video / graph",
     "no importer for these frameworks (SB3 zips, sklearn, torch pickles) and no canonical job: a per-family profile each; the least generic of the closable blockers"),
    ("PARTIAL_TASK_ONLY", None,
     "vision-chat / any-to-any (the text stage lowers, the other stage is not credited)",
     "tower adapters for the wrappers in the census (Qwen3.5, Gemma 3/4, Qwen3-VL, LLaVA-NeXT, Florence-2, Mistral3, Mllama) + the generative close sizing for a tower (`palw_gen_range_twin_v1` / tile-class sizing) + arming `palw_gen_v1`'s Text profile with image slots; Qwen2/2.5-VL at 196 px already admits under the dormant fences"),
    ("TASK_UNKNOWN", None, "a model class with no declared task (`pipeline_tag` absent)",
     "task inference from the remaining head classes and from `architectures` (inference-v3 reads causal LM, T5, vision-chat, sentence encoders, adapter bases); a head-class → task table row per family"),
    ("NOT_RUN_NEEDS_PICKLE_DIRECTORY", None, "PyTorch `pytorch_model.bin` (zip directory + pickle not read)",
     "a bounded range read of the zip central directory and `data.pkl` (no weights, no pickle execution): outside the 10-03 network policy (headers of safetensors/GGUF only) — a policy decision, then `weights::torchzip` already reads it"),
    ("FORMAT_UNSUPPORTED", None, "weights only as onnx / tensorflow / flax / pytorch-adapter / split GGUF",
     "an ONNX initializer reader and a TF/Flax checkpoint reader (headers by range), a split-GGUF reader, an `adapter_model.bin` reader"),
    ("ADAPTER_UNCHECKED", None, "an adapter with no PEFT configuration / not composed",
     "compose the remaining adapter forms (diffusers LoRA `pytorch_lora_weights`, IA3/prefix adapters) with their pinned base"),
    ("ADAPTER_REFUSED", None, "a LoRA the attach refuses (GGUF LoRA, rank/target forms)", "GGUF LoRA composition and the remaining PEFT target forms in `lora::attach`"),
    ("FEATURE_C", ("GEN_UNET_SKIP_V1",), "diffusers UNet denoisers (skip connections over convolutions)",
     "a UNet route: concatenating skip connections + spatial transformers lowered as stage programs (`GEN_UNET_SKIP_V1`), with the Image-profile job already in RFC-0003"),
    ("FEATURE_C", None, "an architecture the generic lowerer has no adapter or feature for", "per-architecture data adapters (`an adapter for X`) and the missing features of the registry (`POS_ROPE_AXES_V1`, …)"),
    ("ARCH_REFUSED", ("gguf-namespace",), "GGUF files carrying a foreign metadata namespace (`mradermacher.*`, uploader bookkeeping)",
     "a data registry of inert provenance namespaces for GGUF metadata (quantizer / uploader keys change no math; a namespace that declares a transform, like `prism.hadamard`, stays refused)"),
    ("ARCH_REFUSED", ("gguf-arch", "gguf-meta", "gguf-rope", "gguf-missing"), "GGUF architectures without an HF mapping",
     "GGUF → HF configuration mappings per architecture (bert, t5, qwen35moe, gemma4, qwen2vl, …), each verified on real tensors"),
    ("ARCH_REFUSED", None, "an architecture the frontend refuses (no adapter claims it, an unmodelled key)", "data adapters for the remaining encoder–decoders (M2M100, IndicTrans, Parler) and decoder families"),
    ("CUSTOM_CODE_UNMODELLED", None, "`trust_remote_code` architectures", "a data adapter for each remote-code family that is a combination of known features (never repository code)"),
    ("TOKENIZER_MISSING", None, "no tokenizer file beside the checkpoint", "bind the tokenizer of the pinned base (vocab size checked)"),
    ("CONFIG_KEY_UNREAD", None, "configuration keys with no rule", "read each key where it changes the math, inert where it does not"),
    ("QUANT_DESCRIPTOR_MISSING", None, "quantisation schemes with no descriptor", "descriptors (data files) for the remaining schemes: hqq, auto-round, ggml types, config/unknown"),
    ("QUANT_REFUSED", None, "quantisation the descriptor refuses (CT-FP8 channel, GPTQ/AWQ forms)", "the refused forms of the described schemes"),
    ("NOT_RUN_NEEDS_TENSOR_DATA", None, "a GGUF whose mapping reads a small tensor's data", "read ≤ 4 KiB of tensor data (policy) — ROPE_FREQ_FACTORS_V1 already removes `rope_freqs`"),
    ("NOT_RUN_NEEDS_WEIGHTS", None, "a diffusers route that lowers from weights", "a shape-only diffusers lowering (no calibration on weights) or a weights-depth census"),
    ("NOT_RUN_PIPELINE_ADMISSION", None, "an embedding / image / encoder-alone class: the pipeline admission cannot be asked from headers",
     "a shape-only Embedding- and Image-profile class declaration (the encoder-decoder's `preflight::pipeline` is the model)"),
    ("CLOSE_TOO_LARGE", None, "close sizing past the generative cap (T5 / Marian encoder stage)",
     "tile-class (range) close sizing for a pipeline stage's inputs and per-layer cross K/V commit points: consensus, behind a dormant fence"),
    ("ADMISSION_EXCEEDS", None, "a position past `max_position_macs` / `max_job_step_leaves` (BART-large encoder, ≥100B decoders)", "RFC-0006 layer × position cells: lower one encoder pass across positions"),
    ("COURT_BUDGET", None, "court work past the ceiling (≥100B)", "RFC-0006 layer × position cells"),
    ("SEAT_MEMORY", None, "no fleet seat tier holds the class", "layer-sharded seats / a larger tier: not a registration blocker"),
]


def family_of(code: str, arg: str):
    for c, args, label, fix in FAMILIES:
        if c != code:
            continue
        if args is None or any(arg.startswith(a) for a in args):
            return label, fix
    return f"{code} (unmapped family)", "(no family row: add one)"


def sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--coverage", required=True)
    ap.add_argument("--update2", required=True)
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--binary", required=True)
    ap.add_argument("--tree", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--registration", help="JSON of the registration columns (hand-kept, cited)")
    ap.add_argument("--top", type=int, default=10)
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    cov = json.load(open(a.coverage))
    old = json.load(open(a.update2))
    man = json.load(open(snap / "MANIFEST.json"))
    rulesets = {}
    for rs, r in cov["rulesets"].items():
        fam = defaultdict(lambda: {"repos": 0.0, "codes": defaultdict(float), "fix": "", "units": 0})
        for k in r["by_key"]:
            if k["bucket"] not in CLOSABLE:
                continue
            label, fix = family_of(k["code"], k["arg"])
            f = fam[(k["bucket"], label)]
            f["repos"] += k["repos_est"]
            f["units"] += k.get("sampled_units", 0)
            f["fix"] = fix
            f["codes"][f"{k['code']}({k['arg']})" if k["arg"] else k["code"]] += k["repos_est"]
        top = sorted(fam.items(), key=lambda x: -x[1]["repos"])
        rulesets[rs] = {
            **{k: v for k, v in r.items() if k not in ("by_key", "codes", "buckets", "variant", "cell_unmeasured")},
            "buckets": r["buckets"],
            "codes_top": r["codes"][:60],
            "top_software_closable_blockers": [
                {
                    "rank": i + 1,
                    "bucket": b[0],
                    "family": b[1],
                    "repos_est": round(v["repos"], 1),
                    "share_d_all": v["repos"] / cov["d_all"],
                    "sampled_units_behind_the_estimate": v["units"],
                    "leading_codes": [[c, round(n, 1)] for c, n in sorted(v["codes"].items(), key=lambda x: -x[1])[:4]],
                    "generic_change_that_would_close_it": v["fix"],
                }
                for i, (b, v) in enumerate(top[: a.top])
            ],
            "estimator_note": r["variant"],
            "cell_unmeasured": r["cell_unmeasured"],
        }
    reg = json.load(open(a.registration)) if a.registration else {}
    # update 2, by code, as published
    u2 = {f"{b['gate']}/{b['code']}": round(b["repos_est"], 1) for b in old["buckets"]}
    ev = {
        "schema": "misaka.rfc11.hf-census-coverage.v1",
        "meaning": "Re-measurement of the Hugging Face census snapshot on the tree named below at two rulesets, with the blockers in the Model Onboarding buckets and two denominators. A header/shape census: nothing here is a registration, and the registered column is evidence-bound.",
        "snapshot": {"id": man["snapshot"], "t0_utc": man["t0_utc"], "d_all": cov["d_all"], "manifest_sha256": sha(snap / "MANIFEST.json"), "sample_design_sha256": sha(snap / "sample" / "design.json")},
        "tree": a.tree,
        "binary_sha256": sha(a.binary),
        "tools_sha256": {n: sha(Path(__file__).resolve().parent / n) for n in ("coverage_report.py", "buckets.py", "estimate.py", "report.py", "compact_rows.py", "shadow_dirs.py", "render_coverage.py", "assemble_evidence.py")},
        "estimator": "hf-census-v1 §4/§4b: exact counts over the baseline-decided repositories by the current listing verdict; Horvitz–Thompson over the stratified header sample (n = 1,773); the three baseline bucket cohorts (n = 150 each) post-stratified by the exact current-listing verdict of their frames; one-sided 95% Korn–Graubard lower bounds; the plain cohort expansion of update 2 is reported beside it (`plain_method`). Shape-depth judgment of every sampled repository; no repository without a row is dropped (nonresponse = UNTESTED).",
        "denominator_b_rule": "D_b = D_all minus every repository whose primary blocker is external: MISSING_WEIGHTS (no weight file, an incomplete shard set, an unreachable repository, an unreadable header), GATED (gated or disabled), ADAPTER_BASE_MISSING (an adapter whose base is absent, ambiguous, not in the snapshot or gated), NO_MODEL_TASK (no declared task and no configuration naming a model class, or no readable configuration). The listing-decided part is exact from the snapshot; the header-decided part is estimated from the sample.",
        "buckets": {"external": ["MISSING_WEIGHTS", "GATED", "ADAPTER_BASE_MISSING", "NO_MODEL_TASK"], "software_closable": list(CLOSABLE)},
        "rulesets": rulesets,
        "update2_by_code_as_published": u2,
        "update2_headline": {"d_all_shape_ready": old["technical"]["d_all"]["shape_ready"], "d_files_shape_ready": old["technical"]["d_files"]["shape_ready"]},
        "registration_columns": reg,
        "acceptance_bar": {
            "statement": "RFC-0011 §7/§10: one-sided 95% lower bound of registered_full_task / D_all >= 0.90 over all public repositories",
            "numerator": "registered_full_task_active_or_final",
            "met": False,
        },
        "network": {"requests": 0, "note": "offline: the snapshot's saved headers only; no Hub request was made"},
    }
    Path(a.out).write_text(json.dumps(ev, indent=1) + "\n")
    print(f"wrote {a.out} ({Path(a.out).stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
