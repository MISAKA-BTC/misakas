#!/usr/bin/env python3
"""Genesis models → one PALWART3 `.palwart` each → Hugging Face, one model at a time.

The genesis set (user decisions 2026-10-15; ids keep the original 1..10 numbering, 7 and 9 dropped):
`scripts/genesis/genesis8_models.json`. Per model the pipeline runs these steps, each recorded in
`<work>/state/<id>.json` so a rerun resumes after the last completed step:

  preflight  free disk against the plan, `hf auth whoami` (exit status only), license/gating, binaries
  download   only the needed files, `hf download --include …` at the PINNED revision, SHA-256 checked
             against the registry's LFS ids (anonymous: HF_HUB_DISABLE_IMPLICIT_TOKEN=1)
  convert    to PALWART3 with the repository's converters (palw-tir-convert --palwart-*, or
             `palwart pack` around the published legacy A16 file for #1); --stream fetches shards on
             first read (`fetch-shard`) and uploads each finished segment (`upload-segment`) then deletes it
  record     container root, index SHA-256, every segment's bytes and SHA-256, the inner roots
  fit        the quality guard: `palw-tir-fidelity --exec` on the statistics the conversion used, held-out text, mean KL
             <= 0.1 and top-1 >= 0.8 against the float reference, else the index is not uploaded (measured where the source
             is on disk and the integer program fits in RAM; otherwise recorded as not measured, with the reason)
  upload     `hf upload` to Misakachain/<Name>-PALW: index, segments, README model card, manifest,
             license/notice files (segments one commit each, so a large model goes in parts)
  verify     the remote SHA-256 / git blob of every uploaded file against the local record
  cleanup    delete the local download and output (only after `verify`)

and appends the model's record to `<work>/genesis10_results.json`.

Served context (user decision 2026-10-16): every genesis model is served at 32,768 tokens (`--context`, default the registry's
32768). Calibration: the converter measures each site's activation range on the float reference over pinned texts of this
repository (git blobs at one commit, `calibration_texts` in the registry). A recurrent (Gated DeltaNet) class is calibrated on
one sequence as long as the context it serves (freeze-v1 §5.2): #2 #3 #6 calibrate on one 32,768-token sequence; dense classes
use four 512-token texts. #1 is re-converted with the A16 converter at --n-ctx 32768. #8's published runtime serves 512 and is
blocked until re-converted. The token file and the statistics travel in the artifact; the manifest records the texts,
tokenizer, lengths, policy and the token file's SHA-256. `--calib-stats-in FILE` uses statistics measured elsewhere (a
calibration-only machine) on the same token file.

Usage:
    python3 scripts/genesis/genesis10_pipeline.py run --model 2 [--stream] [--dry-run] [--context 32768]
    python3 scripts/genesis/genesis10_pipeline.py run --all --keep-going
    python3 scripts/genesis/genesis10_pipeline.py plan                 # disk plan and commands, nothing run
    python3 scripts/genesis/genesis10_pipeline.py manifest --model 2 --out m.json
    python3 scripts/genesis/genesis10_pipeline.py card --model 2 --out README.md
    python3 scripts/genesis/genesis10_pipeline.py status

Rules: the Hugging Face token is never read, printed or passed on a command line — downloads of the
public sources run anonymously, uploads use whatever `hf auth login` stored. Every source file is
untrusted DATA: it is hashed, parsed by the converters' bounded readers, never executed.
Nothing here claims PASS: a completed run is "converted, uploaded, remote hashes equal".
"""

from __future__ import annotations

import argparse
import datetime as _dt
import hashlib
import json
import os
import platform
import shlex
import shutil
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
REGISTRY = HERE / "genesis8_models.json"
APACHE = HERE / "licenses" / "Apache-2.0.txt"
DEFAULT_WORK = Path(os.environ.get("GENESIS_WORK", str(Path.home() / "Downloads" / "genesis-models" / "work")))
DEFAULT_HF = "/Users/wata/Downloads/hf-upload-venv/bin/hf"
DEFAULT_TOKENIZE_PY = "/Users/wata/Downloads/MISAKA-wt-b/tir-venv/bin/python"
HF_API = "https://huggingface.co/api/models"
HF_RESOLVE = "https://huggingface.co"
STEPS = ["preflight", "download", "convert", "record", "fit", "upload", "verify", "cleanup"]
# Every genesis model's served context (user decision 2026-10-16; registry: GENESIS_CONTEXT). A recurrent class is calibrated
# on one sequence of this length.
GENESIS_CONTEXT = 32768
TOKENIZE_DOCS = "misaka/palw/tir/lower/tools/tokenize_docs.py"
GB = 1_000_000_000
LLAMA_NOTICE = "Llama 3.3 is licensed under the Llama 3.3 Community License, Copyright © Meta Platforms, Inc. All Rights Reserved."
LLAMA_AUP = "https://www.llama.com/llama3_3/use-policy"


# ---------------------------------------------------------------------------------------------- utils

def now() -> str:
    return _dt.datetime.now(_dt.timezone.utc).isoformat(timespec="seconds")


def log(msg: str) -> None:
    print(f"[genesis] {msg}", flush=True)


def sha256_file(p: Path) -> str:
    h = hashlib.sha256()
    with open(p, "rb") as f:
        while True:
            b = f.read(8 << 20)
            if not b:
                break
            h.update(b)
    return h.hexdigest()


def git_blob_sha1(p: Path) -> str:
    data = p.read_bytes()
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def load_registry() -> dict:
    return json.loads(REGISTRY.read_text())


def model_by_id(reg: dict, mid: int) -> dict:
    for m in reg["models"]:
        if m["id"] == mid:
            return m
    raise SystemExit(f"no genesis model #{mid} (have {[m['id'] for m in reg['models']]})")


def anon_env() -> dict:
    """The environment for anonymous Hub reads: the stored token is not used (it may be expired)."""
    env = dict(os.environ)
    env["HF_HUB_DISABLE_IMPLICIT_TOKEN"] = "1"
    env.pop("HF_TOKEN", None)
    env.pop("HUGGING_FACE_HUB_TOKEN", None)
    return env


def upload_env() -> dict:
    """The environment for Hub writes: the stored login (`hf auth login`) is used; it is never read or printed here. The
    converter runs anonymously (`anon_env`), so a hook it starts restores the default."""
    env = dict(os.environ)
    env.pop("HF_HUB_DISABLE_IMPLICIT_TOKEN", None)
    return env


class Runner:
    def __init__(self, dry: bool):
        self.dry = dry

    def run(self, argv: list[str], env: dict | None = None, capture: bool = False, check: bool = True) -> subprocess.CompletedProcess:
        log("$ " + " ".join(shlex.quote(a) for a in argv))
        if self.dry:
            return subprocess.CompletedProcess(argv, 0, "", "")
        return subprocess.run(argv, env=env, check=check, text=True, capture_output=capture)


# ---------------------------------------------------------------------------------------------- state

class State:
    def __init__(self, work: Path, model: dict):
        self.path = work / "state" / f"{model['id']:02d}-{model['slug']}.json"
        self.data = json.loads(self.path.read_text()) if self.path.exists() else {"id": model["id"], "slug": model["slug"], "steps": {}}

    def done(self, step: str) -> bool:
        return step in self.data["steps"]

    def mark(self, step: str, info: dict | None = None, dry: bool = False) -> None:
        if dry:
            return
        self.merge_disk()
        self.data["steps"][step] = {"at": now(), **(info or {})}
        self.save()

    def merge_disk(self) -> None:
        """Keep what another process (the converter's segment hook) recorded since this one read the file."""
        if not self.path.exists():
            return
        disk = json.loads(self.path.read_text())
        for k, v in disk.items():
            if k == "steps":
                self.data["steps"] = {**v, **self.data.get("steps", {})}
            elif k == "uploaded_segments":
                self.data[k] = {**v, **self.data.get(k, {})}
            elif k not in self.data:
                self.data[k] = v

    def save(self) -> None:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        tmp = self.path.with_suffix(".tmp")
        tmp.write_text(json.dumps(self.data, indent=1) + "\n")
        tmp.replace(self.path)


# ---------------------------------------------------------------------------------------------- paths

def model_dirs(work: Path, m: dict) -> dict:
    base = work / f"{m['id']:02d}-{m['slug']}"
    return {"base": base, "src": base / "src", "out": base / "out", "stage": base / "stage", "legacy": base / "legacy", "calib": base / "calib"}


def variant_file(m: dict, variant: str | None) -> str | None:
    if m["source_format"] != "gguf":
        return None
    v = variant or m["convert_options"].get("gguf_variant", "bf16")
    f = (m.get("gguf_variants") or {}).get(v)
    if not f:
        raise SystemExit(f"#{m['id']}: GGUF variant `{v}` is not one of {sorted(m.get('gguf_variants') or {})}")
    return f


def needed_files(m: dict, variant: str | None, weights: bool = True) -> list[dict]:
    """The registry files this conversion reads (metadata always; weights unless streamed)."""
    vf = variant_file(m, variant)
    out = []
    for f in m["files"]:
        if f["kind"] == "metadata":
            if f["path"] == "README.md":
                continue  # the publisher's card is attribution, fetched into the stage separately
            out.append(f)
        elif weights and (vf is None or f["path"] == vf):
            out.append(f)
    return out


# ---------------------------------------------------------------------------------------------- plan

def is_legacy_wrap(m: dict) -> bool:
    """A model whose published legacy file is carried byte for byte (#1 A16, #8 PALWQ361): no re-conversion."""
    return m["converter"].endswith("legacy-wrap")


def repo_path(m: dict, name: str) -> str:
    """Where a file goes in the target repository: under `upload_prefix` for an existing repository (an addition, never a
    replacement of its own files), at the root of a new one."""
    return f"{m.get('upload_prefix', '')}{name}"


def estimate(m: dict, variant: str | None, stream: bool, segment_gb: float) -> dict:
    vf = variant_file(m, variant)
    weights = [f for f in m["files"] if f["kind"] == "weight" and (vf is None or f["path"] == vf)]
    total_in = sum(f["bytes"] for f in weights)
    biggest = max((f["bytes"] for f in weights), default=0)
    if is_legacy_wrap(m):
        out = m["legacy_artifact"]["bytes"]
        total_in = out  # the published legacy file is the input
        biggest = out
    elif is_a16(m):
        out = int(total_in / 2 * 1.17)  # the A16 file: int8 codes, i16 embedding rows, the 32K rotary table
    elif is_palwq36(m):
        out = int(total_in * 0.98)  # int8 codes + a power-of-two exponent per 32 + the header (~0.75 GB of parameter rows)
    elif m["source_format"] == "gguf" and vf and "iq3" in vf.lower():
        out = int(54.66e9 / 2 * 1.08)  # stored integer codes are one byte a weight in the TIR section (~27B params)
    else:
        out = int(total_in / 2 * 1.08)  # bf16 in → int8 codes (+ i16 embeddings, scales)
    seg = int(segment_gb * GB)
    if stream:
        # Sharded safetensors: two resident shards (`--shard-keep 2`) + one segment. A GGUF, or a published legacy file, is read
        # over HTTPS and never stored (`--gguf-url`, `palwart pack --from-url`): one segment and the small files.
        if is_palwq36(m):
            peak_in = 1 * GB  # the measuring record (parameter rows) beside the segment in flight
        elif m["source_format"] == "gguf" or is_legacy_wrap(m):
            peak_in = 0
        elif is_a16(m):
            peak_in = total_in + out  # the checkpoint is read whole and the A16 file is written before it is packed
        else:
            peak_in = biggest if len(weights) <= 1 else min(total_in, 2 * biggest)
        peak = peak_in + seg + 2 * GB
    elif is_palwq36(m):
        peak = total_in + seg + 3 * GB  # a local GGUF; the artifact still goes through the pipe, segment by segment
    else:
        peak = total_in + out + 2 * GB + (out if is_a16(m) else 0)
    return {"input_bytes": total_in, "biggest_input_file": biggest, "output_bytes_est": out, "peak_disk_bytes_est": peak,
            "stream": stream, "segment_bytes": seg}


# ---------------------------------------------------------------------------------------------- manifest and card

def is_palwq36(m: dict) -> bool:
    """#8: rebuilt from a `qwen35moe` GGUF with the legacy PALWQ361 converter (`qwen36-convert --context N`), streamed both ends:
    a measuring pass, then an output pass writing into a pipe that `palwart pack --from-pipe` cuts into segments."""
    return m["converter"] == "palwq36"


def is_a16(m: dict) -> bool:
    """#1: converted with the legacy base0 A16 converter (`qwen25-convert --a16 --n-ctx N`), then carried in the container."""
    return m["converter"] == "a16"


def effective_options(m: dict, context: int | None) -> dict:
    """The conversion options as run, at the served context `context` (default: the registry's, 32,768 for every genesis model).
    A recurrent (Gated DeltaNet) program is calibrated on one sequence of that many tokens (freeze-v1 §5.2: the converter refuses
    a context longer than the calibration); a dense program keeps its four short texts and declares the context; the A16
    converter takes it as its width (`--n-ctx`, part of the artifact digest). A published legacy file keeps the context baked
    into it."""
    o = json.loads(json.dumps(m.get("convert_options") or {}))
    if is_legacy_wrap(m):
        return o
    if "context" in o or is_a16(m):  # (the PALWQ361 rebuild's `context` is its rotary width, baked into the file)
        n = int(context or o.get("context") or o.get("n_ctx"))
        if n < 64:
            raise SystemExit(f"--context {n}: give at least 64")
        if is_a16(m):
            o["n_ctx"] = n
        else:
            o["context"] = n
            c = o.get("calib")
            if c and c.get("kind") == "recurrent":
                c["len"] = n
    return o


def served_context(m: dict, opts: dict) -> int | None:
    """The context the class serves: the declared one, or (a wrapped legacy file) the one baked into it."""
    if is_legacy_wrap(m):
        return m.get("served_context")
    return opts.get("context") or opts.get("n_ctx")


def calibration_record(m: dict, opts: dict, texts: dict | None, measured: dict | None = None) -> dict | None:
    """What the manifest says about the calibration: enough to rebuild the identical token file and statistics."""
    c = opts.get("calib")
    if not c:
        return None
    texts = texts or {}
    ts = m.get("tokenizer_source") or {}
    rec = {
        "kind": c["kind"],
        "tokens_file": "calib-tokens.json",
        "stats_file": "calib-stats.json",
        "carried_in": "the calibration section of the .palwart, both files byte for byte",
        "positions_per_sequence": c["len"],
        "chunks_per_document": c["chunks"],
        "documents": [{"path": d, "git_commit": texts.get("git_commit"), "git_blob": (texts.get("blobs") or {}).get(d)} for d in c["docs"]],
        "tokenizer": {"file": "tokenizer.json", "repo": ts.get("repo", m["source_repo"]), "revision": ts.get("revision", m["source_revision"])},
        "tokenization": f"{TOKENIZE_DOCS}: the document's text with add_special_tokens=False and no chat template, its first "
                        "`chunks_per_document` chunks of `positions_per_sequence` tokens; source paths normalised (canonical JSON)",
        "served_context": opts.get("context"),
        "calibrated_context": c["len"] if c["kind"] == "recurrent" else None,
        "length_rule": "freeze-v1 §5.2: a recurrent program is calibrated on one sequence at least as long as the context it is "
                       "served at" if c["kind"] == "recurrent" else "not recurrent: the rule does not apply",
        "policy": {"headroom16": 2.0, "headroom32": 4.0, "headroom_resid": 4.0, "linear_weight_bits": 8,
                   "gdn_core_wide": bool(opts.get("gdn_core_wide"))},
        "float_reference": "layer-major (every position through one layer before the next), f64 sums in index order; the threaded "
                           "kernels are bit-identical to the one-thread kernels, so the statistics do not depend on the core count",
        "reproduce": "same platform as measured_on: rerun with this manifest (pinned texts, fixed token file, fixed summation order). "
                     "Elsewhere: rebuild from the carried statistics (palw-tir-convert --stats-in calib-stats.json, the same "
                     "--context, flags, manifest and calibration files); exp/tanh come from the platform's libm, so re-measured "
                     "statistics may differ in the last bits off this platform",
    }
    if measured:
        rec.update(measured)
    return rec


def manifest_for(m: dict, variant: str | None, context: int | None = None, calib: dict | None = None,
                 texts: dict | None = None) -> dict:
    vf = variant_file(m, variant)
    src_files = [{"path": f["path"], "bytes": f["bytes"], "sha256": f["lfs_sha256"], "git_blob": f["git_blob"]}
                 for f in m["files"] if f["kind"] == "weight" and (vf is None or f["path"] == vf)]
    lic = {"id": m["license"]}
    if m["license"] == "llama3.3":
        lic.update({"name": "Llama 3.3 Community License Agreement", "notice": LLAMA_NOTICE, "acceptable_use_policy": LLAMA_AUP,
                    "built_with_llama": True, "name_begins_with_llama": m["stem"].startswith("Llama")})
    opts = effective_options(m, context)
    conv = {"converter": m["converter"], "options": opts}
    if opts.get("calib") and not is_legacy_wrap(m):
        conv["calibration"] = calibration_record(m, opts, texts if texts is not None else load_registry().get("calibration_texts"), calib)
    if is_legacy_wrap(m):
        la = m["legacy_artifact"]
        conv.update({"legacy_artifact": {k: la[k] for k in ("repo", "revision", "path", "bytes", "sha256", "artifact_digest", "xet_hash",
                                                             "converter") if k in la},
                     "requantised": False, "reconverted": False})
    if is_palwq36(m):
        conv["palwq36"] = {
            "tool": f"qwen36-convert --url <pinned resolve URL> --url-bytes N --url-sha256 <LFS id> --context {opts['context']}: a "
                    "measuring pass (--reference-only --measured-out), then an output pass (--measured-in --out FIFO) into "
                    "palwart pack --from-pipe",
            "calibration": "the converter's own 8-token prompt, with the analytic horizon correction of the Gated DeltaNet state "
                           "exponents to the served context (qwen36_calibrate::horizon_growth_bits)",
            "quantisation": "the stored Q8_0 blocks are dequantized and re-quantized per output channel with a power-of-two exponent "
                            "per 32 weights (PALWQ361): not an integer-exact lowering of Q8_0",
            "mtp": "the trailing multi-token-prediction block (nextn_predict_layers) is not a layer of the artifact",
            "input_check": "whole-file SHA-256 over the range-read bytes in order against the LFS id; every window's SHA-256 checked "
                           "again when it is read again, in either pass",
            "output_check": "the output pass refuses any tensor whose codes differ from the measuring pass's (BLAKE2b) and any "
                            "measured parameter that differs from the header's",
            **(calib or {}),
        }
    if m.get("history_artifact") and not is_a16(m):
        conv["history_artifact"] = dict(m["history_artifact"])
    if is_a16(m):
        conv["a16"] = {"tool": f"qwen25-convert <source> --a16 --n-ctx {opts['n_ctx']} --out {opts['a16_file']}",
                       "calibration": "the converter's own 57-token prompt (misaka/palw/base0/src/bin/qwen25-convert.rs), fixed in source",
                       **(calib or {})}
        if m.get("history_artifact"):
            conv["history_artifact"] = {**m["history_artifact"], "note": "the 8k A16 file published 2026-10-15, kept as history"}
    if vf:
        conv["gguf_variant_file"] = vf
    return {
        "schema": "misaka.genesis8.palwart-manifest.v1",
        "genesis_id": m["id"],
        "name": m["stem"],
        "target_repo": m["target_repo"],
        "source": {"repo": m["source_repo"], "revision": m["source_revision"], "format": m["source_format"], "files": src_files},
        "base_model": {"repo": m["base_model"], "author": m["base_author"]},
        "derivation": {"kind": m["derivation"], "author": m["derivation_author"]},
        "architecture": m["architecture"],
        "family": m["family"],
        "license": lic,
        "attribution": attribution_line(m),
        "conversion": conv,
        "adjudication": {"end_to_end_adjudicable": m["e2e_adjudicable"],
                         "method": "sampled deterministic re-execution, public localization, deterministic court (PRINCIPLES §8: no cryptographic proof of computation)"},
        "tentative": m.get("tentative", False),
        "served_context": served_context(m, opts),
        "uncensored": m.get("uncensored", True),
        "scope": {"kind": "text-generation", "inputs": "token ids", "outputs": "next-token logits",
                  "out_of_scope_components": m.get("out_of_scope", []),
                  "rule": "vision towers, projectors (mmproj), image/audio/video embedders and multi-token-prediction heads are not "
                          "computed, not covered, not admitted as claims and not rewarded"},
    }


def attribution_line(m: dict) -> str:
    s = f"{m['source_repo']}@{m['source_revision']} ({m['derivation']} by {m['derivation_author']}) of {m['base_model']} by {m['base_author']}"
    if m["license"] == "llama3.3":
        s = "Built with Llama. " + s
    return s


def model_card(m: dict, variant: str | None, record: dict | None, manifest: dict | None = None,
               context: int | None = None) -> str:
    man = manifest or manifest_for(m, variant, context)
    lic_yaml = "license: apache-2.0" if m["license"] == "apache-2.0" else "license: other\nlicense_name: llama3.3\nlicense_link: LICENSE"
    base = m["source_repo"]
    tags = ["palw", "misaka", "palwart", m["family"]] + (["abliterated", "uncensored"] if "abliterat" in m["derivation"] else ["uncensored"])
    if m["license"] == "llama3.3":
        tags.append("llama")
    lines = ["---", lic_yaml, "base_model:", f"- {base}", "tags:"] + [f"- {t}" for t in tags] + ["pipeline_tag: text-generation", "---", ""]
    lines.append(f"# {m['stem']}")
    lines.append("")
    if m["license"] == "llama3.3":
        lines += ["**Built with Llama.** This repository contains a derivative of Meta's Llama 3.3. "
                  f"{LLAMA_NOTICE} Use is subject to the Llama 3.3 Community License (`LICENSE`) and the "
                  f"[Llama 3.3 Acceptable Use Policy]({LLAMA_AUP}) (`USE_POLICY.md`).", ""]
    lines += [
        "This is the **PALW-converted form** of an existing model for the MISAKA chain: one `PALWART3` `.palwart` artifact "
        "(an index file plus segment files). It is **not** a newly trained or fine-tuned model and makes no claim about the "
        "model's quality or safety. "
        + "The source model is an uncensored variant (refusal behaviour reduced upstream).",
        "",
        "## Scope: text generation only",
        "",
        "Token ids in, next-token logits out. Vision towers and projectors (mmproj), image/audio/video embedders and processors, and "
        "multi-token-prediction heads are out of scope: not computed, not covered, not admitted as claims, not rewarded. The "
        "artifact refuses them by name (`check_text_scope_v3`)."
        + (f" Out of scope in this checkpoint: {'; '.join(m['out_of_scope'])}." if m.get("out_of_scope") else ""),
        "",
        "## Provenance",
        "",
        "| | |",
        "|---|---|",
        f"| Base model | [{m['base_model']}](https://huggingface.co/{m['base_model']}) — {m['base_author']} |",
        f"| Derivation | {m['derivation']} — {m['derivation_author']} |",
        f"| Source repository | [{m['source_repo']}](https://huggingface.co/{m['source_repo']}) |",
        f"| Source revision (pinned) | `{m['source_revision']}` |",
        f"| Architecture | `{m['architecture']}` ({m['family']}) |",
        f"| License | {m['license']} |",
    ]
    if m.get("derivation_note"):
        lines += ["", m["derivation_note"]]
    lines += ["", "Source weight files (publisher LFS SHA-256, checked before conversion):", ""]
    for f in man["source"]["files"]:
        lines.append(f"- `{f['path']}` — {f['bytes']:,} bytes — `{f['sha256']}`")
    lines += ["", "## The artifact", ""]
    if record:
        lines += [
            f"- `.palwart` root (identity): `{record.get('root', '?')}`",
            f"- index `{record.get('index_file', '?')}` SHA-256 `{record.get('index_sha256', '?')}`",
            f"- weights section: {record.get('weights', {}).get('section', '?')}, inner root ({record.get('weights', {}).get('inner_root_scheme', '?')}) "
            f"`{record.get('weights', {}).get('inner_root', '?')}`",
            f"- segments: {len(record.get('segments', []))}, {record.get('data_bytes', 0):,} bytes",
        ]
        for s in record.get("segments", []):
            lines.append(f"  - `{s['file']}` — {s['bytes']:,} bytes — SHA-256 `{s['sha256']}`")
    else:
        lines.append("(filled in after conversion)")
    lines += [
        "",
        "`PALWART3` holds a version header, a canonical manifest (`manifest.json` here is a readable copy), the program record "
        "(TIR program and kernel id, or the A16 / PALWQ361 engine), the per-architecture weight section, the tokenizer, the chat "
        "template, the source configuration, the license texts and the calibration inputs. Legacy formats (`.palwart` A16, "
        "`.palwtir`, `.palwq36`) are carried unmodified as sections and can be re-extracted byte for byte.",
        "",
        "Check it (no node needed): `palwart verify <stem>.palwart` (segments beside the index), `palwart extract-legacy`.",
        "",
        "## Conversion",
        "",
        f"- converter: `{m['converter']}`; options: `{json.dumps(man['conversion']['options'], sort_keys=True)}`",
    ]
    if man.get("served_context"):
        lines.append(f"- served context: {man['served_context']:,} tokens")
    a16 = man["conversion"].get("a16")
    if a16:
        lines.append(f"- converted with `{a16['tool']}` (A16; calibration: {a16['calibration']})"
                     + (f"; artifact digest `{a16['artifact_digest'][:16]}…`" if a16.get("artifact_digest") else ""))
        if man["conversion"].get("history_artifact"):
            h = man["conversion"]["history_artifact"]
            lines.append(f"- history: the 8k A16 file `{h['path']}` (SHA-256 `{h['sha256']}`) published at `{h['revision'][:12]}` stays in this "
                         "repository; it is not this artifact")
    q36 = man["conversion"].get("palwq36")
    if q36:
        lines.append(f"- converted with `{q36['tool']}`; {q36['quantisation']}; calibration: {q36['calibration']}")
        h = man["conversion"].get("history_artifact")
        if h:
            lines.append(f"- history: `{h['repo']}@{h['revision'][:12]}` `{h['path']}` ({h['note']}) — not this artifact")
    cal = man["conversion"].get("calibration")
    if cal:
        docs = ", ".join(f"`{d['path']}` (git blob `{str(d.get('git_blob'))[:12]}`)" for d in cal["documents"])
        lines.append(f"- calibration: {cal.get('sequences', '?')} sequence(s) of {cal['positions_per_sequence']} tokens from {docs} at "
                     f"`{str(cal['documents'][0].get('git_commit'))[:12]}`, tokenised by `{cal['tokenizer']['repo']}@"
                     f"{cal['tokenizer']['revision'][:12]}` tokenizer.json; token file SHA-256 `{cal.get('tokens_sha256', '(written by the run)')}`; "
                     "the token file and the measured statistics are carried in the artifact")
        if cal["kind"] == "recurrent":
            lines.append(f"- A Gated DeltaNet class is calibrated on one sequence as long as the context it serves (freeze-v1 §5.2): "
                         f"here one {cal['served_context']:,}-token sequence; a longer context is outside this class")
    fit = (record or {}).get("fit")
    if fit and fit.get("measured"):
        lines.append(f"- fit against the float reference (held-out text, {fit.get('positions', '?')} positions): mean KL "
                     f"{fit['kl_mean']:.5f}, top-1 {fit['top1_agreement']:.3f} (tolerance: KL <= {fit['tolerance']['kl_mean_max']}, "
                     f"top-1 >= {fit['tolerance']['top1_min']})")
        lg = fit.get("long") or {}
        if lg.get("measured"):
            lines.append(f"- long-context fit (held-out {lg.get('positions', '?'):,} positions, every one compared): mean KL "
                         f"{lg['kl_mean']:.5f}, top-1 {lg['top1_agreement']:.3f}, worst window mean KL {lg['window_kl_mean_max']:.5f}, "
                         f"late/early {lg.get('drift_ratio')}")
        elif lg:
            lines.append(f"- long-context fit: not measured ({lg.get('reason')})")
    elif fit:
        lines.append(f"- fit against the float reference: not measured on the conversion host ({fit.get('reason')})")
    if is_legacy_wrap(m):
        la = m["legacy_artifact"]
        digest = f", artifact digest `{la['artifact_digest'][:16]}…`" if la.get("artifact_digest") else ""
        lines.append(f"- wraps the published legacy file `{la['repo']}@{la['revision'][:12]}` `{la['path']}` (SHA-256 `{la['sha256']}`{digest}) "
                     f"byte for byte, without re-quantising or re-converting; built by {la['converter']}")
    lines += [
        "",
        "## Verification on MISAKA",
        "",
        ("End-to-end adjudicable: claims over this model are checked by sampled deterministic re-execution, localized with public "
         "evidence and decided by the deterministic court." if m["e2e_adjudicable"] else
         "**Not end-to-end adjudicable** (user decision 2026-10-15): the artifact is published for completeness; it earns no "
         "consensus credit until its claims can be adjudicated."),
        "MISAKA never uses cryptographic proofs of model computation (PRINCIPLES §8). Hashes and Merkle roots here authenticate "
        "bytes only.",
        "",
        "## License and attribution",
        "",
    ]
    if m["license"] == "apache-2.0":
        lines.append("Apache License 2.0 — the full text is in `LICENSE`. " + attribution_line(m) + ".")
    else:
        lines.append(f"Llama 3.3 Community License — `LICENSE` (copied from the source), `USE_POLICY.md`, `NOTICE`. {LLAMA_NOTICE} "
                     + attribution_line(m) + ".")
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------------------------- remote checks

def remote_files(repo: str, revision: str = "main", hf: str | None = None) -> dict:
    """{path: {size, sha256 (LFS) or None, blob}} of a repository, anonymously; falls back to huggingface_hub."""
    url = f"{HF_API}/{repo}/revision/{revision}?blobs=true"
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "misaka-genesis8-pipeline/1"})
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
        with opener.open(req, timeout=60) as r:
            d = json.load(r)
        return {s["rfilename"]: {"size": s.get("size"), "sha256": (s.get("lfs") or {}).get("sha256"), "blob": s.get("blobId")}
                for s in d.get("siblings", [])}
    except urllib.error.HTTPError as e:
        if e.code not in (401, 403, 404) or not hf:
            raise
    # A private repository: ask through the hf environment's huggingface_hub (it uses the stored login; never printed).
    py = str(Path(hf).parent / "python")
    code = ("import json,sys\nfrom huggingface_hub import HfApi\n"
            "info=HfApi().model_info(sys.argv[1],revision=sys.argv[2],files_metadata=True)\n"
            "print(json.dumps({s.rfilename:{'size':s.size,'sha256':(s.lfs.sha256 if s.lfs else None),'blob':s.blob_id} for s in info.siblings}))")
    out = subprocess.run([py, "-I", "-c", code, repo, revision], check=True, text=True, capture_output=True, env=upload_env()).stdout
    return json.loads(out)


# ---------------------------------------------------------------------------------------------- steps

class Pipeline:
    def __init__(self, args: argparse.Namespace):
        self.a = args
        self.reg = load_registry()
        self.work = Path(args.work).expanduser()
        self.r = Runner(args.dry_run)
        self.hf = args.hf or shutil.which("hf") or DEFAULT_HF
        self.bin = Path(args.bin_dir).expanduser()

    def tool(self, name: str) -> str:
        return str(self.bin / name)

    # -- preflight ---------------------------------------------------------------------------------

    def preflight(self, m: dict, st: State) -> None:
        plan = estimate(m, self.a.gguf_variant, self.a.stream, self.a.segment_mib / 1024 * (1 << 30) / GB)
        self.work.mkdir(parents=True, exist_ok=True)
        free = shutil.disk_usage(self.work).free
        log(f"#{m['id']} {m['source_repo']}@{m['source_revision'][:12]}: input {plan['input_bytes'] / GB:.1f} GB, "
            f"output ~{plan['output_bytes_est'] / GB:.1f} GB, peak disk ~{plan['peak_disk_bytes_est'] / GB:.1f} GB "
            f"({'stream' if plan['stream'] else 'whole'}), free {free / GB:.1f} GB")
        problems = []
        if plan["peak_disk_bytes_est"] + self.a.min_free_gb * GB > free:
            problems.append(f"free disk {free / GB:.1f} GB < peak {plan['peak_disk_bytes_est'] / GB:.1f} GB + floor {self.a.min_free_gb} GB"
                            + ("" if self.a.stream else " (try --stream)"))
        if m.get("gated"):
            problems.append("the source is gated: accept its terms on the Hub first")
        if m["license"] not in ("apache-2.0", "llama3.3"):
            problems.append(f"license {m['license']} is not one the genesis set allows")
        if m["license"] == "llama3.3" and not m["stem"].startswith("Llama"):
            problems.append("a Llama 3.3 derivative's name must begin with \"Llama\"")
        if is_legacy_wrap(m) and not self.a.no_upload:
            dedup = self.dedup_check(m)
            st.data["dedup_check"] = dedup
            log(f"#{m['id']} Xet dedup: {dedup['summary']}")
        if m.get("blocked") and not self.a.dry_run:
            problems.append(f"BLOCKED: {m['blocked']}")
        elif m.get("blocked"):
            log(f"#{m['id']} BLOCKED (a real run stops here): {m['blocked']}")
        needed = (["palwart"] if is_legacy_wrap(m) else ["qwen25-convert", "palwart"] if is_a16(m)
                  else ["qwen36-convert", "palwart"] if is_palwq36(m) else ["palw-tir-convert", "palwart"])
        if self.fit_plan(m)[0]:
            needed.append("palw-tir-fidelity")
        for t in needed:
            if not self.a.dry_run and not Path(self.tool(t)).is_file():
                problems.append(f"binary {self.tool(t)} is missing (cargo build --release -p … --bin {t})")
        if not self.a.dry_run and not Path(self.hf).is_file():
            problems.append(f"hf CLI not found at {self.hf}")
        # Authentication for the upload: the exit status only; the token is never read or shown.
        if self.a.stream and self.a.no_upload:
            problems.append("--stream hands every segment to the uploader and deletes it: it cannot run with --no-upload")
        if not self.a.dry_run and not self.a.no_upload:
            p = subprocess.run([self.hf, "auth", "whoami"], text=True, capture_output=True, env=upload_env())
            if p.returncode != 0:
                problems.append("`hf auth whoami` failed: run `hf auth login --force` (uploads need a valid login)")
            else:
                who = (p.stdout.strip().splitlines() or ["?"])[0]
                log(f"hf: logged in ({who[:80]})")
        else:
            self.r.run([self.hf, "auth", "whoami"], check=False)
        if problems:
            for p in problems:
                log(f"PREFLIGHT #{m['id']}: {p}")
            if not self.a.dry_run:
                raise SystemExit(2)
        st.mark("preflight", {"plan": plan, "free_bytes": free}, self.a.dry_run)

    def dedup_check(self, m: dict) -> dict:
        """Whether re-uploading the legacy file's bytes is cheap: the target repository stores files in Xet (content-defined
        chunks, deduplicated per repository), the legacy file is already there as Xet chunks, and the local `hf` uses `hf_xet`.
        The weight entry sits at logical offset 0 of the PALWART3 data, so every segment but the last is an exact byte slice of
        the legacy file: only the chunks that straddle a segment cut (a few per segment) and the small sections are new bytes."""
        la = m["legacy_artifact"]
        out = {"repo": m["target_repo"], "legacy": la["path"]}
        if self.a.dry_run:
            out["summary"] = "not checked in a dry run"
            return out
        try:
            info = json.loads(urllib.request.urlopen(urllib.request.Request(
                f"{HF_API}/{m['target_repo']}?expand[]=xetEnabled", headers={"User-Agent": "misaka-genesis8-pipeline/1"}), timeout=60).read())
            tree = json.loads(urllib.request.urlopen(urllib.request.Request(
                f"{HF_API}/{la['repo']}/tree/{la['revision']}/{la['path'].rsplit('/', 1)[0]}?expand=true",
                headers={"User-Agent": "misaka-genesis8-pipeline/1"}), timeout=60).read())
            entry = next((e for e in tree if e.get("path") == la["path"]), {})
            py = str(Path(self.hf).parent / "python")
            has_xet = subprocess.run([py, "-I", "-c", "import importlib.util as u; print(u.find_spec('hf_xet') is not None)"],
                                     text=True, capture_output=True).stdout.strip() == "True"
            out.update({"xet_enabled": bool(info.get("xetEnabled")), "legacy_xet_hash": entry.get("xetHash"),
                        "legacy_lfs_sha256": (entry.get("lfs") or {}).get("oid"), "hf_xet_installed": has_xet})
            cheap = out["xet_enabled"] and bool(out["legacy_xet_hash"]) and has_xet and out["legacy_lfs_sha256"] == la["sha256"]
            segs = -(-la["bytes"] // (self.a.segment_mib << 20))
            out["segments"] = segs
            out["summary"] = (f"expected cheap: Xet repository, legacy file stored as Xet chunks ({str(out['legacy_xet_hash'])[:12]}…), hf_xet "
                              f"installed; {segs} segments are exact slices of it, so the new bytes are roughly the chunks at "
                              f"{segs - 1} cuts plus the small sections (expectation, not a measurement)") if cheap else (
                              f"NOT expected cheap: xet_enabled={out['xet_enabled']}, legacy_xet_hash={bool(out['legacy_xet_hash'])}, "
                              f"hf_xet_installed={has_xet}: the full {la['bytes'] / GB:.1f} GB would be uploaded")
        except Exception as e:  # noqa: BLE001 — a metadata failure only loses the estimate
            out["summary"] = f"could not check ({e})"
        return out

    # -- download ----------------------------------------------------------------------------------

    def download(self, m: dict, st: State) -> None:
        d = model_dirs(self.work, m)
        for k in ("src", "stage"):
            if not self.a.dry_run:
                d[k].mkdir(parents=True, exist_ok=True)
        env = anon_env()
        if is_legacy_wrap(m):
            la = m["legacy_artifact"]
            p = d["legacy"] / la["path"]
            if self.a.stream and not self.a.local_legacy:
                log(f"{la['path']}: streamed by `palwart pack --from-url` during convert (never stored)")
            elif self.a.local_legacy and not self.a.dry_run:
                lp = Path(self.a.local_legacy).expanduser()
                self.check_file(lp, la["bytes"], la["sha256"])
                p.parent.mkdir(parents=True, exist_ok=True)
                p.unlink(missing_ok=True)
                p.symlink_to(lp.resolve())
            else:
                self.r.run([self.hf, "download", la["repo"], la["path"], "--revision", la["revision"], "--local-dir", str(d["legacy"])], env=env)
                if not self.a.dry_run:
                    self.check_file(p, la["bytes"], la["sha256"])
        # Streamed: sharded safetensors arrive shard by shard during conversion; a GGUF is read over HTTPS ranges, never stored.
        weights = not self.a.stream or is_a16(m)
        if is_legacy_wrap(m):
            weights = False
        if self.a.local_source and m["source_format"] == "gguf":
            weights = True  # the local copy is used in place (checked against its pin, linked)
        files = needed_files(m, self.a.gguf_variant, weights=weights)
        fetch = list(files)
        if self.a.local_source:
            # A copy the user already downloaded at the pinned revision: every file it holds is checked against the pin;
            # weights are linked (never copied, never deleted), metadata copied; what it lacks is downloaded.
            local = Path(self.a.local_source).expanduser()
            fetch = []
            for f in files:
                lp = local / f["path"]
                if not lp.is_file():
                    fetch.append(f)
                    continue
                if not self.a.dry_run:
                    self.check_file(lp, f["bytes"], f["lfs_sha256"], f.get("git_blob"))
                    dst = d["src"] / f["path"]
                    dst.unlink(missing_ok=True)
                    if f["kind"] == "weight":
                        dst.symlink_to(lp.resolve())
                    else:
                        shutil.copyfile(lp, dst)
                else:
                    log(f"would use {lp} (checked against its pin)")
            log(f"local source {local}: {len(files) - len(fetch)} files used, {len(fetch)} to download")
        if fetch:
            argv = [self.hf, "download", m["source_repo"], "--revision", m["source_revision"], "--local-dir", str(d["src"])]
            for f in fetch:
                argv += ["--include", f["path"]]
            self.r.run(argv, env=env)
        if m.get("tokenizer_source"):
            ts = m["tokenizer_source"]
            argv = [self.hf, "download", ts["repo"], *ts["files"], "--revision", ts["revision"], "--local-dir", str(d["src"])]
            self.r.run(argv, env=env)
        # The publisher's own card, kept for attribution (staged, uploaded as SOURCE_README.md).
        self.r.run([self.hf, "download", m["source_repo"], "README.md", "--revision", m["source_revision"], "--local-dir", str(d["stage"] / "source-card")],
                   env=env, check=False)
        if not self.a.dry_run:
            for f in files:
                self.check_file(d["src"] / f["path"], f["bytes"], f["lfs_sha256"], f.get("git_blob"))
        if self.a.stream and m["source_format"] == "safetensors" and m["converter"] in ("tir",):
            self.write_stubs(m, d["src"])
        st.mark("download", {"files": [f["path"] for f in files], "weights_local": weights}, self.a.dry_run)

    def check_file(self, p: Path, size: int | None, sha256: str | None, blob: str | None = None) -> None:
        if not p.is_file():
            raise SystemExit(f"{p}: missing after download")
        if size is not None and p.stat().st_size != size:
            raise SystemExit(f"{p}: {p.stat().st_size} bytes, the registry says {size}")
        if sha256:
            got = sha256_file(p)
            if got != sha256:
                raise SystemExit(f"{p}: SHA-256 {got} is not the pinned LFS id {sha256}")
        elif blob:
            got = git_blob_sha1(p)
            if got != blob:
                raise SystemExit(f"{p}: git blob {got} is not the pinned {blob}")
        log(f"checked {p.name} ({p.stat().st_size:,} bytes)")

    def write_stubs(self, m: dict, src: Path) -> None:
        """Header-only stubs `<shard>.header` (HTTP range, anonymous) and the shard-size file the lazy source needs."""
        if str(HERE) not in sys.path:
            sys.path.insert(0, str(HERE))
        import genesis_header_fixtures as ghf  # noqa: PLC0415

        sizes = {}
        for f in m["files"]:
            if f["kind"] != "weight":
                continue
            sizes[f["path"]] = f["bytes"]
            stub = src / f"{f['path']}.header"
            if self.a.dry_run:
                log(f"would fetch the header stub {stub.name}")
                continue
            if not stub.exists():
                stub.write_bytes(ghf.header_bytes(m["source_repo"], m["source_revision"], f["path"]))
        if not self.a.dry_run:
            (src / "shard-sizes.json").write_text(json.dumps(sizes, indent=1) + "\n")

    # -- convert -----------------------------------------------------------------------------------

    def self_cmd(self, sub: str, m: dict) -> list[str]:
        return [sys.executable, "-I", str(Path(__file__).resolve()), sub, "--model", str(m["id"]), "--work", str(self.work), "--hf", self.hf]

    def hook_args(self, m: dict, flag_prog: str, flag_arg: str, flag_delete: str) -> list[str]:
        if not self.a.stream or self.a.no_upload:
            return []
        cmd = self.self_cmd("upload-segment", m)
        out = [flag_prog, cmd[0]]
        for c in cmd[1:]:
            out += [flag_arg, c]
        return out + [flag_delete]

    def write_manifest(self, m: dict, calib: dict | None = None) -> Path:
        """The manifest the converter embeds (it is part of the root); the upload ships this same file."""
        d = model_dirs(self.work, m)
        p = d["base"] / "manifest.json"
        man = manifest_for(m, self.a.gguf_variant, self.a.context, calib, self.reg.get("calibration_texts"))
        if not self.a.dry_run:
            d["base"].mkdir(parents=True, exist_ok=True)
            p.write_text(json.dumps(man, indent=1, ensure_ascii=False) + "\n")
        return p

    def license_files(self, m: dict, src: Path) -> list[str]:
        if m["license"] == "apache-2.0":
            return [] if (src / "LICENSE").exists() else [str(APACHE)]
        # Llama 3.3: the license and AUP come from the source as data (source/LICENSE, source/USE_POLICY.md); add the notice.
        notice = model_dirs(self.work, m)["stage"] / "NOTICE"
        if not self.a.dry_run:
            notice.parent.mkdir(parents=True, exist_ok=True)
            notice.write_text(LLAMA_NOTICE + "\n")
        return [str(notice)]

    def pinned_text(self, path: str, dest: Path) -> tuple[Path, dict]:
        """A calibration / evaluation text exactly as pinned: the git blob the registry names, extracted and its id checked —
        never the working tree's copy, which later edits move."""
        texts = self.reg.get("calibration_texts") or {}
        blob = (texts.get("blobs") or {}).get(path)
        if not blob:
            raise SystemExit(f"{path}: not a pinned text (registry calibration_texts.blobs)")
        out = dest / path.replace("/", "__")
        meta = {"path": path, "git_commit": texts.get("git_commit"), "git_blob": blob}
        argv = ["git", "-C", str(ROOT), "cat-file", "blob", blob]
        if self.a.dry_run:
            self.r.run(argv)
            return out, meta
        data = subprocess.run(argv, check=True, capture_output=True).stdout
        if hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest() != blob:
            raise SystemExit(f"{path}: git blob {blob} did not verify")
        dest.mkdir(parents=True, exist_ok=True)
        out.write_bytes(data)
        meta["sha256"] = hashlib.sha256(data).hexdigest()
        return out, meta

    def tokens_file(self, spec: dict, tokenizer: Path, out: Path) -> dict:
        """Tokenise the pinned texts of `spec` ({docs, len, chunks}) into `out` in a canonical, machine-independent form (no
        local paths: the file is carried in the artifact, so its bytes are part of the root). Reused when it exists with the
        same texts, lengths and tokenizer; refused when it exists with others. Returns what the manifest records."""
        work = out.parent
        if not self.a.dry_run:
            work.mkdir(parents=True, exist_ok=True)
        docs = [self.pinned_text(d, work / "texts") for d in spec["docs"]]
        tok_sha = None if self.a.dry_run else sha256_file(tokenizer)
        want = {"len": spec["len"], "chunks": spec["chunks"], "skip": spec.get("skip", 0), "tokenizer_sha256": tok_sha,
                "blobs": [meta["git_blob"] for _, meta in docs]}
        if out.exists() and not self.a.dry_run:
            have = json.loads(out.read_text()).get("source", {})
            got = {"len": have.get("len"), "chunks": have.get("chunks"), "skip": have.get("skip", 0),
                   "tokenizer_sha256": (have.get("tokenizer") or {}).get("sha256"), "blobs": [f.get("git_blob") for f in have.get("files", [])]}
            if got != want:
                raise SystemExit(f"{out} was made with other parameters ({got}, now {want}); remove it (and calib-stats.json) to recalibrate")
        else:
            raw = work / (out.stem + ".raw.json")
            argv = [self.a.tokenize_python, "-I", str(ROOT / TOKENIZE_DOCS), "--tokenizer", str(tokenizer),
                    "--len", str(spec["len"]), "--chunks", str(spec["chunks"]), "--out", str(raw)] + [str(p) for p, _ in docs]
            env = anon_env()
            env["HF_HUB_OFFLINE"] = "1"
            self.r.run(argv, env=env)
            if self.a.dry_run:
                return {}
            rawj = json.loads(raw.read_text())
            files = rawj.get("source", {}).get("files", [])
            skip = spec.get("skip", 0)
            if skip and len(docs) != 1:
                raise SystemExit("a `skip` applies to one document's chunks")
            source = {"tool": TOKENIZE_DOCS, "tokenizer": {"file": "tokenizer.json", "sha256": tok_sha}, "len": spec["len"],
                      "chunks": spec["chunks"],
                      "files": [{**meta, "tokens": f.get("tokens"), "chunks": f.get("chunks")} for (_, meta), f in zip(docs, files)]}
            if skip:
                source["skip"] = skip
            canon = {"source": source, "sequences": rawj.get("sequences", [])[skip:]}
            out.write_text(json.dumps(canon, sort_keys=True, separators=(",", ":")) + "\n")
            raw.unlink()
        j = json.loads(out.read_text())
        seqs = j.get("sequences", [])
        if not seqs or any(len(q) != spec["len"] for q in seqs):
            raise SystemExit(f"{out}: needs sequences of {spec['len']} tokens, got {[len(q) for q in seqs]} (is the text long enough?)")
        ver = subprocess.run([self.a.tokenize_python, "-I", "-c", "import tokenizers; print(tokenizers.__version__)"],
                             text=True, capture_output=True).stdout.strip()
        return {"tokens_sha256": sha256_file(out), "sequences": len(seqs), "documents": j["source"]["files"],
                "tokenizers_version": ver or None}

    def calibration(self, m: dict, opts: dict, d: dict) -> tuple[Path, dict]:
        c = opts["calib"]
        out = d["calib"] / "calib-tokens.json"
        info = self.tokens_file(c, d["src"] / "tokenizer.json", out)
        if info:
            tok = dict(calibration_record(m, opts, self.reg.get("calibration_texts"))["tokenizer"])
            tok["sha256"] = sha256_file(d["src"] / "tokenizer.json")
            info["tokenizer"] = tok
            info["measured_on"] = f"{platform.system()}-{platform.machine()}"
        return out, info

    def convert(self, m: dict, st: State) -> None:
        d = model_dirs(self.work, m)
        opts = effective_options(m, self.a.context)
        if not self.a.dry_run:
            d["out"].mkdir(parents=True, exist_ok=True)
            if any(d["out"].glob(f"{m['stem']}.*")) and not st.done("convert"):
                raise SystemExit(f"{d['out']}: a previous conversion left files; remove them (the converter never overwrites) and rerun")
        report = d["base"] / "convert-report.json"
        seg = ["--segment-mib", str(self.a.segment_mib)]
        if is_legacy_wrap(m):
            la = m["legacy_artifact"]
            if self.a.stream and not self.a.local_legacy:
                url = f"{HF_RESOLVE}/{la['repo']}/resolve/{la['revision']}/{la['path']}"
                src = ["--from-url", url, "--from-bytes", str(la["bytes"]), "--from-sha256", la["sha256"], "--from-name", la["path"]]
            else:
                src = ["--from", str(d["legacy"] / la["path"])]
            manifest = self.write_manifest(m)
            argv = [self.tool("palwart"), "pack", *src, "--source-dir", str(d["src"]),
                    "--out-dir", str(d["out"]), "--stem", m["stem"], "--family", m["family"], "--manifest", str(manifest), *seg]
            for lf in self.license_files(m, d["src"]):
                argv += ["--license-file", lf]
            argv += self.hook_args(m, "--on-segment", "--on-segment-arg", "--delete-after-hook")
            argv += ["--json"]
        elif is_a16(m):
            a16 = d["legacy"] / opts["a16_file"]
            info = self.a16_convert(m, opts, d, a16, st)
            manifest = self.write_manifest(m, info)
            argv = [self.tool("palwart"), "pack", "--from", str(a16), "--source-dir", str(d["src"]),
                    "--out-dir", str(d["out"]), "--stem", m["stem"], "--family", m["family"], "--manifest", str(manifest), *seg]
            for lf in self.license_files(m, d["src"]):
                argv += ["--license-file", lf]
            argv += self.hook_args(m, "--on-segment", "--on-segment-arg", "--delete-after-hook")
            argv += ["--json"]
        elif is_palwq36(m):
            return self.palwq36_convert(m, opts, d, st, seg)
        elif m["converter"] in ("tir", "tir-gguf"):
            o = opts
            model_path = d["src"]
            if m["converter"] == "tir-gguf":
                model_path = d["src"] / variant_file(m, self.a.gguf_variant)
                # The calibration tokenizer is checked id by id against the GGUF's vocabulary before it tokenises anything.
                self.prepare_gguf_side_files(m, d)
            calib, cinfo = self.calibration(m, opts, d)
            stats = d["calib"] / "calib-stats.json"
            pinned_stats = None
            if self.a.calib_stats_in:
                # Statistics measured on another machine (`palw-tir-fidelity --calibrate-only --stats-out` on this token file): read,
                # not re-measured; the converter still checks the calibration-length rule on the token file.
                pinned_stats = Path(self.a.calib_stats_in).expanduser()
                if not self.a.dry_run:
                    shutil.copyfile(pinned_stats, stats)
                    cinfo["statistics"] = {"measured": "elsewhere (--calib-stats-in)", "sha256": sha256_file(stats)}
            elif stats.exists() and not self.a.dry_run:
                stats.unlink()  # measured again by this run (`--stats-out`); a leftover of other parameters must not be carried
            manifest = self.write_manifest(m, cinfo)
            log(f"#{m['id']} calibration: {cinfo.get('sequences', '?')} sequence(s) of {o['calib']['len']} tokens, served context "
                f"{o['context']} (token file SHA-256 {cinfo.get('tokens_sha256', '?')})")
            argv = [self.tool("palw-tir-convert"), str(model_path), "--out", str(d["base"] / "unused.palwtir"), "--calib", str(calib),
                    "--context", str(o["context"]), *(["--stats-in", str(stats)] if pinned_stats else ["--stats-out", str(stats)]), "--json",
                    "--palwart-out-dir", str(d["out"]), "--palwart-stem", m["stem"], "--palwart-manifest", str(manifest),
                    "--palwart-family", m["family"], "--palwart-segment-mib", str(self.a.segment_mib),
                    "--palwart-source-dir", str(d["src"]), "--palwart-calibration-file", str(calib), "--palwart-calibration-file", str(stats)]
            if o.get("gdn_core_wide"):
                argv.append("--gdn-core-wide")
            argv += self.ram_args(d, "convert")
            if m["converter"] == "tir-gguf":
                argv += ["--tokenizer", str(d["src"] / "tokenizer.json")]
                if not self.a.local_source and (self.a.stream or not model_path.exists()):
                    # End to end: every tensor by HTTPS range at the pinned revision, the whole-file SHA-256 computed over the
                    # streamed bytes in order and compared with the LFS id before the index is written.
                    gw = self.gguf_weight(m)
                    argv += ["--gguf-url", self.gguf_url(m), "--gguf-bytes", str(gw["bytes"]), "--gguf-sha256", gw["lfs_sha256"],
                             "--gguf-window-mib", "64", "--gguf-cache-windows", "8"]
            if self.a.calib_lazy_mib:
                # Off by default: a lazily bound dense tensor larger than the cache is read again at EVERY position (whole tensor,
                # `ParamStore::view`), which for a streamed source means downloading it again. The layer-major run already holds one
                # layer (or the embedding / head) at a time, which fits 32 GB for every genesis model. Same statistics either way.
                argv += ["--calib-lazy-mib", str(self.a.calib_lazy_mib), "--calib-lazy-cache-mib", str(self.a.calib_lazy_cache_mib)]
            for lf in self.license_files(m, d["src"]):
                argv += ["--palwart-license-file", lf]
            argv += self.hook_args(m, "--palwart-on-segment", "--palwart-on-segment-arg", "--palwart-delete-after-hook")
            if self.a.stream and m["converter"] == "tir":
                fetch = self.self_cmd("fetch-shard", m)
                argv += ["--lazy-shards", "--shard-sizes", str(d["src"] / "shard-sizes.json"), "--shard-keep", "2", "--shard-fetch", fetch[0]]
                for c in fetch[1:]:
                    argv += ["--shard-fetch-arg", c]
        else:
            raise SystemExit(f"#{m['id']}: converter `{m['converter']}` is blocked (see preflight)")
        log(f"converting #{m['id']} (report: {report})")
        if self.a.dry_run:
            self.r.run(argv)
            return
        # The converter's reads are anonymous: no token reaches it (a public source), so none can be printed or sent.
        p = subprocess.run(argv, text=True, stdout=subprocess.PIPE, env=anon_env())
        if p.returncode != 0:
            raise SystemExit(f"#{m['id']}: the converter exited {p.returncode}")
        report.write_text(p.stdout)
        st.mark("convert", {"report": str(report)})

    def palwq36_convert(self, m: dict, opts: dict, d: dict, st: State, seg: list[str]) -> None:
        """#8 at 32K: `qwen36-convert` twice over the same verified bytes, never storing the GGUF or the artifact.
        1. measuring pass (`--reference-only --measured-out`): reads every tensor by range, calibrates, records the measured
           parameter rows, the artifact's length and every tensor's code digest; hashes the GGUF in order and compares it with
           the LFS id before it exits, and records every window's digest (`--url-windows-out`);
        2. output pass (`--measured-in --url-windows-in --out FIFO`): every window it reads must match those digests; it writes
           header (built with the measured rows) and codes strictly in order into a pipe, refusing any tensor whose codes differ
           from the measuring pass's; `palwart pack --from-pipe --from-bytes N` cuts the stream into 2 GiB segments, each handed
           to the uploader and deleted, the index last.
        The bytes are the one-pass file's (`misaka/palw/base0/tests/qwen36_streamed_output.rs`)."""
        self.prepare_gguf_side_files(m, d)  # the tokenizer checked id by id against the GGUF; its chat template and metadata
        manifest = self.write_manifest(m)
        report = d["base"] / "convert-report.json"
        vf = variant_file(m, self.a.gguf_variant)
        if self.a.local_source:
            src_in = ["--gguf", str(d["src"] / vf)]
            first, second = list(src_in), list(src_in)
        else:
            gw = self.gguf_weight(m)
            url = ["--url", self.gguf_url(m), "--url-bytes", str(gw["bytes"]),
                   "--url-window-mib", str(opts.get("window_mib", 64)), "--url-cache-windows", str(opts.get("cache_windows", 24))]
            first = url + ["--url-sha256", gw["lfs_sha256"], "--url-windows-out", str(d["base"] / "q36-windows.json")]
            second = url + ["--url-windows-in", str(d["base"] / "q36-windows.json")]
        measured = d["base"] / "q36-measured.bin"
        ctx = ["--context", str(opts["context"])]
        if not (st.done("q36-measure") and measured.exists()):
            argv = [self.tool("qwen36-convert"), *first, *ctx, "--reference-only", "--measured-out", str(measured),
                    "--out", str(d["base"] / "q36-measure.unused")]
            log(f"#{m['id']} PALWQ361 measuring pass (log: {d['base'] / 'q36-measure.log'})")
            if self.a.dry_run:
                self.r.run(argv)
            else:
                with open(d["base"] / "q36-measure.log", "w") as logf:
                    p = subprocess.run(argv, stdout=logf, stderr=subprocess.STDOUT, env=anon_env())
                if p.returncode != 0:
                    raise SystemExit(f"#{m['id']}: the measuring pass exited {p.returncode} (log: {d['base'] / 'q36-measure.log'})")
                st.mark("q36-measure", {"measured": str(measured)})
        file_bytes = "<from the measuring record>"
        if not self.a.dry_run:
            head = measured.read_bytes()[:16]
            if head[:8] != b"PALWQ36M":
                raise SystemExit(f"{measured}: not a measuring pass's record")
            file_bytes = str(int.from_bytes(head[8:16], "little"))
        fifo = d["base"] / "q36.fifo"
        pack = [self.tool("palwart"), "pack", "--from-pipe", str(fifo), "--from-bytes", file_bytes,
                "--from-name", f"{m['stem']}.palwq36", "--source-dir", str(d["src"]), "--out-dir", str(d["out"]), "--stem", m["stem"],
                "--family", m["family"], "--manifest", str(manifest), *seg]
        for lf in self.license_files(m, d["src"]):
            pack += ["--license-file", lf]
        pack += self.hook_args(m, "--on-segment", "--on-segment-arg", "--delete-after-hook") + ["--json"]
        conv = [self.tool("qwen36-convert"), *second, *ctx, "--measured-in", str(measured), "--out", str(fifo)]
        log(f"#{m['id']} PALWQ361 output pass into {fifo} → palwart pack (report: {report})")
        if self.a.dry_run:
            self.r.run(["mkfifo", str(fifo)])
            self.r.run(pack + ["&"])
            self.r.run(conv)
            return
        fifo.unlink(missing_ok=True)
        os.mkfifo(fifo)
        packer = subprocess.Popen(pack, text=True, stdout=subprocess.PIPE, env=anon_env())
        try:
            with open(d["base"] / "q36-output.log", "w") as logf:
                p = subprocess.run(conv, stdout=logf, stderr=subprocess.STDOUT, env=anon_env())
            if p.returncode != 0:
                packer.kill()
                raise SystemExit(f"#{m['id']}: the output pass exited {p.returncode} (log: {d['base'] / 'q36-output.log'})")
            out, _ = packer.communicate()
        finally:
            if packer.poll() is None:
                packer.kill()
            fifo.unlink(missing_ok=True)
        if packer.returncode != 0:
            raise SystemExit(f"#{m['id']}: palwart pack exited {packer.returncode}")
        report.write_text(out)
        st.mark("convert", {"report": str(report)})

    def ram_args(self, d: dict, step: str) -> list[str]:
        """RFC-0020 §2/§6: the RAM accounting file of a step (`--ram-accounting`: `<model dir>/ram-<step>.jsonl`, live and peak
        bytes by kind and stage plus the resident set) and the opt-in carry spill."""
        argv = []
        if self.a.ram_accounting:
            argv += ["--ram-accounting", str(d["base"] / f"ram-{step}.jsonl")]
        if self.a.spill_carries_dir:
            argv += ["--spill-carries-dir", str(Path(self.a.spill_carries_dir).expanduser()),
                     "--spill-carries-budget-mib", str(self.a.spill_carries_budget_mib)]
        return argv

    def a16_convert(self, m: dict, opts: dict, d: dict, out: Path, st: State) -> dict:
        """#1: `qwen25-convert <src> --a16 --n-ctx N --out FILE` (the legacy base0 A16 converter; the width is part of the
        artifact digest). Reused when a previous run recorded this file. Returns what the manifest records."""
        prev = st.data.get("a16") or {}
        if not self.a.dry_run and out.exists() and prev.get("sha256") and prev.get("n_ctx") == opts["n_ctx"] \
                and sha256_file(out) == prev["sha256"]:
            log(f"#{m['id']} A16 file {out.name} reused ({prev['sha256'][:16]}…)")
            return prev
        argv = [self.tool("qwen25-convert"), str(d["src"]), "--a16", "--n-ctx", str(opts["n_ctx"]), "--out", str(out)]
        if self.a.dry_run:
            self.r.run(argv)
            return {}
        out.parent.mkdir(parents=True, exist_ok=True)
        out.unlink(missing_ok=True)
        log("$ " + " ".join(shlex.quote(x) for x in argv))
        p = subprocess.run(argv, text=True, stdout=subprocess.PIPE, env=anon_env())
        (d["base"] / "qwen25-convert.log").write_text(p.stdout)
        if p.returncode != 0 or not out.exists():
            raise SystemExit(f"#{m['id']}: qwen25-convert exited {p.returncode} (log: {d['base'] / 'qwen25-convert.log'})")
        lines = p.stdout.splitlines()
        pick = lambda key: next((ln.split(key, 1)[1].strip() for ln in lines if key in ln), None)  # noqa: E731
        info = {"n_ctx": opts["n_ctx"], "file": out.name, "bytes": out.stat().st_size, "sha256": sha256_file(out),
                "artifact_digest": (pick("a16 artifact") or "").split()[0] or None,
                "faithful": {k: pick(k) for k in ("a16 top-1 agree", "a16 top-5 contains", "a16 rank corr (100)", "a16 FAITHFUL")}}
        st.data["a16"] = info
        st.save()
        return info

    def gguf_url(self, m: dict) -> str:
        return f"{HF_RESOLVE}/{m['source_repo']}/resolve/{m['source_revision']}/{variant_file(m, self.a.gguf_variant)}"

    def gguf_weight(self, m: dict) -> dict:
        vf = variant_file(m, self.a.gguf_variant)
        return next(f for f in m["files"] if f["path"] == vf)

    def prepare_gguf_side_files(self, m: dict, d: dict) -> None:
        """For a GGUF source: check the calibration tokenizer token by token against the GGUF's own vocabulary, and
        stage the GGUF's chat template and metadata as data files the container carries."""
        if self.a.dry_run:
            log("would check tokenizer.json against tokenizer.ggml.tokens and extract tokenizer.chat_template")
            return
        if str(HERE) not in sys.path:
            sys.path.insert(0, str(HERE))
        import gguf_meta  # noqa: PLC0415

        g = d["src"] / variant_file(m, self.a.gguf_variant)
        if g.exists():
            src = gguf_meta.Source(path=str(g))
        else:
            # The streamed file: its header (metadata and tensor table, ~11 MB) by bounded anonymous range reads.
            src = gguf_meta.Source(url=self.gguf_url(m))
        meta = gguf_meta.parse_full(src, keep_arrays={"tokenizer.ggml.tokens"}, keep_strings={"tokenizer.chat_template"})
        tokens = meta["arrays"].get("tokenizer.ggml.tokens") or []
        tj = json.loads((d["src"] / "tokenizer.json").read_text())
        by_id: dict[int, str] = {i: t for t, i in (tj.get("model", {}).get("vocab") or {}).items()}
        for at in tj.get("added_tokens") or []:
            by_id[at["id"]] = at["content"]
        bad = [i for i, t in enumerate(tokens) if i in by_id and by_id[i] != t]
        missing = [i for i in range(len(tokens)) if i not in by_id]
        if bad or len(by_id) < len(tokens) - len(missing):
            raise SystemExit(f"#{m['id']}: tokenizer.json differs from the GGUF vocabulary at {len(bad)} ids (first {bad[:5]})")
        if missing:
            log(f"note: {len(missing)} GGUF ids have no tokenizer.json entry (reserved/padding ids, e.g. {missing[:3]})")
        tpl = meta["strings"].get("tokenizer.chat_template")
        if tpl and not (d["src"] / "chat_template.jinja").exists():
            (d["src"] / "chat_template.jinja").write_text(tpl)
        (d["src"] / "configuration.json").write_text(json.dumps({"gguf_metadata": meta["summary"]}, indent=1, ensure_ascii=False) + "\n")

    # -- record ------------------------------------------------------------------------------------

    def record(self, m: dict, st: State) -> dict:
        d = model_dirs(self.work, m)
        if self.a.dry_run:
            log(f"would read {d['base'] / 'convert-report.json'} and {d['out'] / (m['stem'] + '.palwart')}")
            return {}
        rep = json.loads((d["base"] / "convert-report.json").read_text())
        pal = rep.get("palwart") or rep  # palw-tir-convert nests it; `palwart pack` is it
        index = d["out"] / f"{m['stem']}.palwart"
        insp = json.loads(subprocess.run([self.tool("palwart"), "inspect", str(index), "--json"], check=True, text=True, capture_output=True).stdout)
        weights = next((s for s in insp["sections"] if s["kind"] in ("base0-a16", "palwq361-int", "tir-inventory", "mxfp4-exact-int")), {})
        rec = {
            "id": m["id"], "name": m["stem"], "target_repo": m["target_repo"],
            "source": f"{m['source_repo']}@{m['source_revision']}",
            "root": insp["root"],
            "index_file": index.name,
            "index_sha256": sha256_file(index),
            "index_blob": git_blob_sha1(index),
            "data_bytes": insp["data_bytes"],
            "segments": insp["segments"],
            "weights": {"section": weights.get("kind"), "family": weights.get("family"), "inner_root_scheme": weights.get("inner_root_scheme"),
                        "inner_root": weights.get("inner_root"), "inner_leaf_count": weights.get("inner_leaf_count")},
            "program": insp["program"],
            "converter_report": {k: pal.get(k) for k in ("root", "tir_inventory_root", "tir_leaves", "kernel_id", "tokenizer_id") if k in pal},
            "e2e_adjudicable": m["e2e_adjudicable"],
            "license": m["license"],
        }
        if pal.get("root") and pal["root"] != insp["root"]:
            raise SystemExit(f"#{m['id']}: the converter reported root {pal['root']}, the index says {insp['root']}")
        st.data["record"] = rec
        st.mark("record", {"root": rec["root"]})
        log(f"#{m['id']} .palwart root {rec['root']}")
        return rec

    # -- fit (the quality guard) -------------------------------------------------------------------------

    def fit_plan(self, m: dict) -> tuple[bool, str]:
        """Whether the fit can be measured here: `palw-tir-fidelity` re-reads the source and holds the integer program whole."""
        if is_legacy_wrap(m):
            return False, "a wrapped legacy file: nothing was converted here"
        if is_a16(m):
            return False, ("the A16 route's check is qwen25-convert's own FAITHFUL report over its 57-token prompt (in the manifest); "
                           "no long-context check exists for the A16 engine")
        if is_palwq36(m):
            return False, ("PALWQ361 at 32K is not measurable on this Mac: no tool compares the legacy engine with the f32 reference "
                           "at windows along a long sequence (qwen36-run --reference scores the converter's 8-token prompt only), "
                           "the f32 reference over 32K positions of the 35B-A3B MoE is another full read of the GGUF and hours of "
                           "compute, and the integer engine needs the ~37 GB artifact on local disk against 32 GB of RAM (the "
                           "streamed run keeps none of it). The measuring pass's log records the reference's next-token check")
        if self.a.no_fit:
            return False, "--no-fit"
        if m["converter"] == "tir-gguf" and not self.a.local_source:
            return False, "the GGUF is streamed and never on disk; measure with palw-tir-fidelity on a host that holds it"
        est = estimate(m, self.a.gguf_variant, False, 1.0)["output_bytes_est"]
        if est > self.a.fit_max_gb * GB:
            return False, f"the integer program (~{est / GB:.0f} GB) exceeds --fit-max-gb {self.a.fit_max_gb:g}"
        return True, ""

    def fit(self, m: dict, st: State) -> None:
        d = model_dirs(self.work, m)
        ok, why = self.fit_plan(m)
        model_path = d["src"] if m["converter"] == "tir" else d["src"] / (variant_file(m, self.a.gguf_variant) or "")
        if ok and not self.a.dry_run:
            vf = variant_file(m, self.a.gguf_variant)
            missing = [f["path"] for f in m["files"] if f["kind"] == "weight" and (vf is None or f["path"] == vf)
                       and not (d["src"] / f["path"]).exists()]
            if missing:
                ok, why = False, f"{len(missing)} source weight file(s) no longer on disk (streamed and evicted), e.g. {missing[0]}"
        if not ok:
            log(f"#{m['id']} fit: not measured — {why}")
            fit = {"measured": False, "reason": why}
            st.data["fit"] = fit
            if st.data.get("record") is not None:
                st.data["record"]["fit"] = fit
            st.mark("fit", {"measured": False}, self.a.dry_run)
            return
        opts = effective_options(m, self.a.context)
        texts = self.reg["calibration_texts"]
        ev = self.tokens_file(texts["evaluation"], d["src"] / "tokenizer.json", d["calib"] / "eval-tokens.json")
        argv = [self.tool("palw-tir-fidelity"), str(model_path), "--calib", str(d["calib"] / "calib-tokens.json"),
                "--stats-in", str(d["calib"] / "calib-stats.json"), "--eval", str(d["calib"] / "eval-tokens.json"), "--exec", "--json"]
        if opts.get("gdn_core_wide"):
            argv.append("--gdn-core-wide")
        argv += self.ram_args(d, "fit")
        if self.a.dry_run:
            self.r.run(argv)
            self.fit_long(m, opts, d, model_path)
            return
        log("$ " + " ".join(shlex.quote(x) for x in argv))
        p = subprocess.run(argv, text=True, stdout=subprocess.PIPE, env=anon_env())
        if p.returncode != 0:
            raise SystemExit(f"#{m['id']}: palw-tir-fidelity exited {p.returncode}")
        (d["base"] / "fit-report.json").write_text(p.stdout)
        res = json.loads(p.stdout)
        met = res.get("metrics") or {}
        tol = texts["fit_tolerance"]
        kl, top1 = met.get("kl_mean"), met.get("top1_agreement")
        within = kl is not None and top1 is not None and kl <= tol["kl_mean_max"] and top1 >= tol["top1_min"]
        fit = {"measured": True, "within": within, "kl_mean": kl, "kl_max": met.get("kl_max"), "top1_agreement": top1,
               "ppl_delta": met.get("ppl_delta"), "positions": met.get("positions"), "tolerance": tol,
               "evaluation_tokens_sha256": ev.get("tokens_sha256"), "program_digest": res.get("program_digest"),
               "seconds": res.get("seconds")}
        st.data["fit"] = fit
        if st.data.get("record") is not None:
            st.data["record"]["fit"] = fit
        log(f"#{m['id']} fit: mean KL {kl}, top-1 {top1} (tolerance KL <= {tol['kl_mean_max']}, top-1 >= {tol['top1_min']})")
        if within:
            fit["long"] = self.fit_long(m, opts, d, model_path)
            within = fit["long"].get("within", True)
            fit["within"] = within
        if not within:
            st.save()
            raise SystemExit(f"#{m['id']}: FIT OUT OF TOLERANCE — the index, manifest and card are not uploaded "
                             f"(reports: {d['base'] / 'fit-report.json'}, {d['base'] / 'fit-long-report.json'})")
        st.mark("fit", {"measured": True, "within": True})

    def fit_long(self, m: dict, opts: dict, d: dict, model_path: Path) -> dict:
        """The long-context check: one held-out sequence of the served length (the next 32K tokens of RFC-0002 after the
        calibration's), every position compared as it is made (`--exec`; no logits row is held), the mean and worst KL of
        `drift_windows` windows along it. Costs one float pass over the sequence (about a calibration) plus the integer run."""
        texts = self.reg["calibration_texts"]
        spec = dict(texts["long_evaluation"])
        spec["len"] = opts["context"]
        if self.a.no_fit_long:
            return {"measured": False, "reason": "--no-fit-long"}
        ev = self.tokens_file(spec, d["src"] / "tokenizer.json", d["calib"] / "eval-long-tokens.json")
        argv = [self.tool("palw-tir-fidelity"), str(model_path), "--calib", str(d["calib"] / "calib-tokens.json"),
                "--stats-in", str(d["calib"] / "calib-stats.json"), "--eval", str(d["calib"] / "eval-long-tokens.json"), "--exec",
                "--drift-windows", str(spec["drift_windows"]), "--json"]
        if opts.get("gdn_core_wide"):
            argv.append("--gdn-core-wide")
        argv += self.ram_args(d, "fit-long")
        if self.a.dry_run:
            self.r.run(argv)
            return {}
        log("$ " + " ".join(shlex.quote(x) for x in argv))
        p = subprocess.run(argv, text=True, stdout=subprocess.PIPE, env=anon_env())
        if p.returncode != 0:
            raise SystemExit(f"#{m['id']}: palw-tir-fidelity (long) exited {p.returncode}")
        (d["base"] / "fit-long-report.json").write_text(p.stdout)
        res = json.loads(p.stdout)
        met, dr = res.get("metrics") or {}, res.get("drift") or {}
        tol = texts["fit_tolerance"]
        wins = dr.get("windows") or []
        worst = max((w["kl_mean"] for w in wins), default=None)
        ratio, late = dr.get("ratio"), dr.get("kl_late")
        drift_ok = ratio is None or late is None or late <= tol["drift_floor_kl"] or (ratio == ratio and ratio <= tol["drift_ratio_max"])
        within = (met.get("kl_mean") is not None and met["kl_mean"] <= tol["kl_mean_max"]
                  and met.get("top1_agreement", 0) >= tol["top1_min"]
                  and worst is not None and worst <= tol["window_kl_mean_max"] and drift_ok)
        out = {"measured": True, "within": within, "positions": met.get("positions"), "kl_mean": met.get("kl_mean"),
               "kl_max": met.get("kl_max"), "top1_agreement": met.get("top1_agreement"), "window_kl_mean_max": worst,
               "drift_ratio": ratio, "kl_early": dr.get("kl_early"), "kl_late": late, "windows": wins,
               "evaluation_tokens_sha256": ev.get("tokens_sha256"), "seconds": res.get("seconds")}
        log(f"#{m['id']} long fit: {met.get('positions')} positions, mean KL {met.get('kl_mean')}, worst window {worst}, "
            f"late/early {ratio} -> {'within' if within else 'OUT OF TOLERANCE'}")
        return out

    # -- upload ------------------------------------------------------------------------------------

    def stage_files(self, m: dict, rec: dict) -> Path:
        d = model_dirs(self.work, m)
        stage = d["stage"]
        stage.mkdir(parents=True, exist_ok=True)
        # The manifest the converter embedded (part of the root), byte for byte; the card reads its calibration from it.
        man_path = d["base"] / "manifest.json"
        if not man_path.exists():
            raise SystemExit(f"#{m['id']}: {man_path} (written by convert) is missing")
        man = json.loads(man_path.read_text())
        (stage / "README.md").write_text(model_card(m, self.a.gguf_variant, rec, man))
        shutil.copyfile(man_path, stage / "manifest.json")
        (stage / "palwart-record.json").write_text(json.dumps(rec, indent=1) + "\n")
        if m["license"] == "apache-2.0":
            src_lic = d["src"] / "LICENSE"
            shutil.copyfile(src_lic if src_lic.exists() else APACHE, stage / "LICENSE")
        else:
            for n in ("LICENSE", "USE_POLICY.md"):
                if (d["src"] / n).exists():
                    shutil.copyfile(d["src"] / n, stage / n)
            (stage / "NOTICE").write_text(LLAMA_NOTICE + "\n")
            if not (stage / "LICENSE").exists() or not (stage / "USE_POLICY.md").exists():
                raise SystemExit(f"#{m['id']}: the Llama 3.3 LICENSE and USE_POLICY.md must ship with the artifact")
        card = stage / "source-card" / "README.md"
        if card.exists():
            shutil.copyfile(card, stage / "SOURCE_README.md")
        shutil.copyfile(d["out"] / rec["index_file"], stage / rec["index_file"])
        return stage

    def ensure_repo(self, m: dict) -> None:
        argv = [self.hf, "repos", "create", m["target_repo"], "--type", "model", "--exist-ok"]
        argv.append("--private" if self.a.private else "--no-private")
        self.r.run(argv, env=upload_env())

    def upload_file(self, m: dict, path: Path, name: str, sha256: str | None) -> None:
        """One file, skipped when the remote already holds the same bytes."""
        try:
            remote = remote_files(m["target_repo"], "main", self.hf) if not self.a.dry_run else {}
        except Exception:  # noqa: BLE001 — a new repository has no listing yet
            remote = {}
        r = remote.get(name)
        if r and ((sha256 and r.get("sha256") == sha256) or (not sha256 and r.get("blob") == git_blob_sha1(path))):
            log(f"{name}: already uploaded")
            return
        self.r.run([self.hf, "upload", m["target_repo"], str(path), name, "--commit-message", f"genesis8 #{m['id']}: {name}"],
                   env=upload_env())

    def upload(self, m: dict, st: State) -> None:
        if self.a.no_upload:
            log("--no-upload: skipped")
            return
        rec = st.data.get("record") or {}
        fit = st.data.get("fit") or {}
        if fit.get("measured") and not fit.get("within"):
            raise SystemExit(f"#{m['id']}: the fit is out of tolerance (KL {fit.get('kl_mean')}, top-1 {fit.get('top1_agreement')}); "
                             "the index is not uploaded")
        if self.a.dry_run:
            self.ensure_repo(m)
            self.r.run([self.hf, "upload", m["target_repo"], "<stage dir>", m.get("upload_prefix", "").rstrip("/") or ".",
                        "--commit-message", f"genesis8 #{m['id']}: PALWART3 index and card"])
            return
        self.ensure_repo(m)
        d = model_dirs(self.work, m)
        st.merge_disk()  # the segments the converter's hook uploaded
        for s in rec.get("segments", []):
            p = d["out"] / s["file"]
            if p.exists():
                self.upload_file(m, p, repo_path(m, s["file"]), s["sha256"])
            elif s["file"] not in (st.data.get("uploaded_segments") or {}):
                raise SystemExit(f"#{m['id']}: segment {s['file']} is neither local nor recorded as uploaded")
        stage = self.stage_files(m, rec)
        self.r.run([self.hf, "upload", m["target_repo"], str(stage), m.get("upload_prefix", "").rstrip("/") or ".", "--exclude", "source-card/*",
                    "--commit-message", f"genesis8 #{m['id']}: PALWART3 index, manifest, model card, license"], env=upload_env())
        st.mark("upload", {"repo": m["target_repo"]})

    # -- verify ------------------------------------------------------------------------------------

    def verify(self, m: dict, st: State) -> None:
        if self.a.no_upload:
            return
        if self.a.dry_run:
            log(f"would compare the remote LFS SHA-256 / git blob of every file in {m['target_repo']} with the record")
            return
        rec = st.data["record"]
        remote = remote_files(m["target_repo"], "main", self.hf)
        problems = []
        for s in rec["segments"]:
            r = remote.get(repo_path(m, s["file"]))
            if not r or r.get("sha256") != s["sha256"] or r.get("size") != s["bytes"]:
                problems.append(f"{s['file']}: remote {r}")
        def same(r: dict | None, sha: str, blob: str) -> bool:
            # An LFS file is named by its SHA-256, a small one by its git blob id.
            return bool(r) and (r.get("sha256") == sha if r.get("sha256") else r.get("blob") == blob)

        ri = remote.get(repo_path(m, rec["index_file"]))
        if not same(ri, rec["index_sha256"], rec["index_blob"]):
            problems.append(f"{rec['index_file']}: remote {ri}")
        stage = model_dirs(self.work, m)["stage"]
        for n in ("README.md", "manifest.json", "LICENSE"):
            r = remote.get(repo_path(m, n))
            if not same(r, sha256_file(stage / n), git_blob_sha1(stage / n)):
                problems.append(f"{n}: remote {r}")
        if problems:
            raise SystemExit(f"#{m['id']}: remote verification failed: {problems}")
        st.mark("verify", {"files": len(rec["segments"]) + 4})
        log(f"#{m['id']}: remote hashes equal the record")

    # -- cleanup -----------------------------------------------------------------------------------

    def cleanup(self, m: dict, st: State) -> None:
        if not st.done("verify") and not self.a.no_upload and not self.a.dry_run:
            raise SystemExit(f"#{m['id']}: refusing to delete before the remote copy is verified")
        d = model_dirs(self.work, m)
        if self.a.keep:
            log("--keep: local files kept")
            return
        for k in ("src", "out", "legacy", "stage"):
            if self.a.dry_run:
                log(f"would delete {d[k]}")
            elif d[k].exists():
                shutil.rmtree(d[k])
        st.mark("cleanup", {}, self.a.dry_run)

    # -- the model loop ----------------------------------------------------------------------------

    def run_model(self, m: dict) -> dict:
        st = State(self.work, m)
        only = set(self.a.steps.split(",")) if self.a.steps else None
        for step in STEPS:
            if only and step not in only:
                continue
            if st.done(step) and not self.a.redo:
                log(f"#{m['id']} {step}: done earlier ({st.data['steps'][step]['at']})")
                continue
            log(f"#{m['id']} {step}")
            getattr(self, step)(m, st)
        rec = dict(st.data.get("record") or {})
        rec.update({"steps": sorted(st.data["steps"]), "at": now()})
        if not self.a.dry_run:
            append_result(self.work, rec)
        return rec


def append_result(work: Path, rec: dict) -> None:
    p = work / "genesis10_results.json"
    data = json.loads(p.read_text()) if p.exists() else {"schema": "misaka.genesis8.results.v1", "models": []}
    data["models"] = [r for r in data["models"] if r.get("id") != rec.get("id")] + [rec]
    data["models"].sort(key=lambda r: r.get("id", 0))
    tmp = p.with_suffix(".tmp")
    tmp.write_text(json.dumps(data, indent=1) + "\n")
    tmp.replace(p)


# ---------------------------------------------------------------------------------------------- hooks

def hook_upload_segment(a: argparse.Namespace) -> int:
    """Called by the converter with every finished segment: upload it, verify the remote SHA-256, record it."""
    reg = load_registry()
    m = model_by_id(reg, a.model)
    path = Path(os.environ["PALWART_SEGMENT_PATH"])
    sha = os.environ["PALWART_SEGMENT_SHA256"]
    size = int(os.environ["PALWART_SEGMENT_BYTES"])
    if path.stat().st_size != size or sha256_file(path) != sha:
        print(f"upload-segment: {path} does not match what the writer reported", file=sys.stderr)
        return 3
    hf = a.hf or DEFAULT_HF
    subprocess.run([hf, "repos", "create", m["target_repo"], "--type", "model", "--exist-ok"], check=False, capture_output=True,
                   env=upload_env())
    try:
        remote = remote_files(m["target_repo"], "main", hf)
    except Exception:  # noqa: BLE001
        remote = {}
    name = repo_path(m, path.name)
    if (remote.get(name) or {}).get("sha256") != sha:
        subprocess.run([hf, "upload", m["target_repo"], str(path), name, "--commit-message", f"genesis8 #{m['id']}: segment {path.name}"],
                       check=True, env=upload_env())
        remote = remote_files(m["target_repo"], "main", hf)
        r = remote.get(name) or {}
        if r.get("sha256") != sha or r.get("size") != size:
            print(f"upload-segment: remote {path.name} is {r}, expected {sha} / {size}", file=sys.stderr)
            return 4
    st = State(Path(a.work).expanduser(), m)
    st.data.setdefault("uploaded_segments", {})[path.name] = {"sha256": sha, "bytes": size, "at": now()}
    st.save()
    return 0


def hook_fetch_shard(a: argparse.Namespace) -> int:
    """Called by the lazy checkpoint for a shard it needs: download it (anonymously) and check its pinned SHA-256."""
    reg = load_registry()
    m = model_by_id(reg, a.model)
    name = os.environ["PALW_SHARD_NAME"]
    dest = Path(os.environ["PALW_SHARD_PATH"])
    f = next((f for f in m["files"] if f["path"] == name and f["kind"] == "weight"), None)
    if f is None:
        print(f"fetch-shard: {name} is not a weight file of #{m['id']}", file=sys.stderr)
        return 3
    hf = a.hf or DEFAULT_HF
    subprocess.run([hf, "download", m["source_repo"], name, "--revision", m["source_revision"], "--local-dir", str(dest.parent)],
                   check=True, env=anon_env())
    if dest.stat().st_size != f["bytes"] or sha256_file(dest) != f["lfs_sha256"]:
        dest.unlink(missing_ok=True)
        print(f"fetch-shard: {name} does not match its pinned SHA-256", file=sys.stderr)
        return 4
    return 0


def write_catalog(a: argparse.Namespace, reg: dict) -> int:
    """Edition 0 of the next network from the PALWART3 roots in the results and the declared class ids. Refuses a model with
    either missing (the Rust twin, `palw_genesis8_edition0_v1`, refuses the same)."""
    tpl = json.loads((ROOT / "docs/design/palw/genesis8/catalog-edition0-template.json").read_text())
    results = json.loads((Path(a.work).expanduser() / "genesis10_results.json").read_text())
    roots = {r["id"]: r.get("root") for r in results.get("models", [])}
    classes = {int(k): v for k, v in json.loads(Path(a.classes).read_text()).items()}
    hexok = lambda h: isinstance(h, str) and len(h) == 128 and all(c in "0123456789abcdef" for c in h) and h != "0" * 128  # noqa: E731
    models, missing = [], []
    for m in tpl["editions"][0]["models"]:
        if m["tentative"] and not a.include_tentative:
            continue
        gid = m["genesis_id"]
        cid, root = classes.get(gid), roots.get(gid)
        if not hexok(cid) or not hexok(root):
            missing.append(gid)
            continue
        models.append({"class_id": cid, "artifact_root": root, "kernel_version": m["kernel_version"],
                       "verification_plan_id": m["verification_plan_id"]})
    if missing:
        print(f"catalog: no class id or PALWART3 root yet for {missing}", file=sys.stderr)
        return 2
    out = {"fence_daa": a.fence_daa, "editions": [{"activation_daa": 0, "models": models}]}
    Path(a.out).write_text(json.dumps(out, indent=1) + "\n")
    log(f"edition 0 with {len(models)} models -> {a.out} (root it with scripts/release/misaka-release-manifest.py)")
    return 0


# ---------------------------------------------------------------------------------------------- CLI

def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)

    def common(p):
        p.add_argument("--work", default=str(DEFAULT_WORK), help=f"work directory (default {DEFAULT_WORK}, or $GENESIS_WORK)")
        p.add_argument("--hf", default=None, help=f"the hf CLI (default: on PATH, else {DEFAULT_HF})")

    r = sub.add_parser("run", help="run the steps for one or more models")
    common(r)
    g = r.add_mutually_exclusive_group(required=True)
    g.add_argument("--model", type=int, action="append", help="genesis id (repeatable): 1 2 3 4 5 6 8 10")
    g.add_argument("--all", action="store_true")
    r.add_argument("--dry-run", action="store_true", help="print the plan and every command; run nothing")
    r.add_argument("--stream", action="store_true", help="shard in → segment out → upload → delete (bounded disk)")
    r.add_argument("--steps", help=f"only these steps, comma-separated ({','.join(STEPS)})")
    r.add_argument("--redo", action="store_true", help="rerun steps already recorded as done")
    r.add_argument("--keep", action="store_true", help="keep local files after verification")
    r.add_argument("--keep-going", action="store_true", help="continue with the next model after a failure")
    r.add_argument("--no-upload", action="store_true", help="convert and record only")
    r.add_argument("--private", action="store_true", help="create the target repositories private")
    r.add_argument("--gguf-variant", help="#6: bf16 (default), iq3_s1, iq3_s2 or iq3_s3")
    r.add_argument("--segment-mib", type=int, default=2048)
    r.add_argument("--min-free-gb", type=float, default=30.0,
                   help="free-disk floor kept for others (the validation agent stops below 30 GB)")
    r.add_argument("--context", "--calib-positions", dest="context", type=int, default=None,
                   help=f"the served context (default: the registry's, {GENESIS_CONTEXT} for every genesis model). A recurrent (GDN) "
                        "class is calibrated on one sequence of this length (freeze-v1 §5.2); #1's A16 width")
    r.add_argument("--calib-stats-in", help="calibration statistics measured elsewhere on this run's token file "
                                            "(palw-tir-fidelity --calibrate-only --stats-out): read, not re-measured")
    r.add_argument("--no-fit-long", action="store_true", help="skip the long-context fit check (recorded as not measured)")
    r.add_argument("--ram-accounting", action="store_true",
                   help="RFC-0020 §2: the converter and the fit checks write live/peak bytes by kind and stage and the resident set "
                        "to <model dir>/ram-<step>.jsonl")
    r.add_argument("--spill-carries-dir", help="RFC-0020 §6 (opt-in): spill the float reference's carries between layers here")
    r.add_argument("--spill-carries-budget-mib", type=int, default=0,
                   help="spill a sequence's carries only when they would pass this many MiB (0: always, when a dir is given)")
    r.add_argument("--calib-lazy-mib", type=int, default=0,
                   help="row-streamed calibration (default off): params of at least this many MiB are read when an op asks. "
                        "A dense tensor larger than --calib-lazy-cache-mib is then read again at every position")
    r.add_argument("--calib-lazy-cache-mib", type=int, default=512)
    r.add_argument("--no-fit", action="store_true", help="skip the fit guard (recorded as not measured)")
    r.add_argument("--fit-max-gb", type=float, default=20.0,
                   help="measure the fit only when the integer program (held whole by palw-tir-fidelity) is at most this size")
    r.add_argument("--local-source", help="a copy of the source already downloaded at the pinned revision (one --model only); "
                                          "its files are checked against the pins and linked, never modified or deleted")
    r.add_argument("--local-legacy", help="#1: the legacy A16 .palwart already on disk (checked against its pinned SHA-256)")
    r.add_argument("--bin-dir", default=str(ROOT / "target" / "release"))
    r.add_argument("--tokenize-python", default=DEFAULT_TOKENIZE_PY, help="a Python with the `tokenizers` package (offline)")

    p = sub.add_parser("plan", help="the disk plan of every model (nothing run)")
    p.add_argument("--stream", action="store_true")
    p.add_argument("--segment-mib", type=int, default=2048)
    p.add_argument("--gguf-variant")

    mf = sub.add_parser("manifest", help="write a model's manifest")
    mf.add_argument("--model", type=int, required=True)
    mf.add_argument("--out", required=True)
    mf.add_argument("--gguf-variant")
    mf.add_argument("--context", "--calib-positions", dest="context", type=int, default=None)

    cd = sub.add_parser("card", help="write a model's README model card")
    cd.add_argument("--model", type=int, required=True)
    cd.add_argument("--out", required=True)
    cd.add_argument("--record", help="the palwart-record.json of a conversion")
    cd.add_argument("--gguf-variant")
    cd.add_argument("--context", "--calib-positions", dest="context", type=int, default=None)

    for name in ("upload-segment", "fetch-shard"):
        h = sub.add_parser(name, help="(called by the converter)")
        common(h)
        h.add_argument("--model", type=int, required=True)

    s = sub.add_parser("status")
    common(s)

    dd = sub.add_parser("dedup-check", help="whether re-uploading a wrapped legacy file is cheap (Xet chunk dedup); metadata only")
    common(dd)
    dd.add_argument("--model", type=int, required=True)
    dd.add_argument("--segment-mib", type=int, default=2048)

    cg = sub.add_parser("catalog", help="fill the RFC-0019 edition-0 template from the recorded roots and the declared class ids")
    common(cg)
    cg.add_argument("--classes", required=True, help="JSON {genesis id: class id (128 hex)} from the class declarations")
    cg.add_argument("--fence-daa", type=int, required=True, help="the tier fence of the next network")
    cg.add_argument("--include-tentative", action="store_true", help="list #3 (awaiting the user's decision)")
    cg.add_argument("--out", required=True, help="input for scripts/release/misaka-release-manifest.py write")

    a = ap.parse_args(argv)
    reg = load_registry()
    if a.cmd == "plan":
        rows = []
        for m in reg["models"]:
            e = estimate(m, a.gguf_variant, a.stream, a.segment_mib / 1024 * (1 << 30) / GB)
            rows.append((m["id"], m["source_repo"], m["converter"], e))
            print(f"#{m['id']:>2} {m['source_repo']:<72} {m['converter']:<16} in {e['input_bytes'] / GB:6.1f} GB  "
                  f"out ~{e['output_bytes_est'] / GB:5.1f} GB  peak ~{e['peak_disk_bytes_est'] / GB:6.1f} GB")
        return 0
    if a.cmd == "manifest":
        man = manifest_for(model_by_id(reg, a.model), a.gguf_variant, a.context, None, reg.get("calibration_texts"))
        Path(a.out).write_text(json.dumps(man, indent=1, ensure_ascii=False) + "\n")
        return 0
    if a.cmd == "card":
        rec = json.loads(Path(a.record).read_text()) if a.record else None
        Path(a.out).write_text(model_card(model_by_id(reg, a.model), a.gguf_variant, rec, None, a.context))
        return 0
    if a.cmd == "upload-segment":
        return hook_upload_segment(a)
    if a.cmd == "fetch-shard":
        return hook_fetch_shard(a)
    if a.cmd == "dedup-check":
        m = model_by_id(reg, a.model)
        if not is_legacy_wrap(m):
            print(f"#{a.model} is converted, not wrapped: its segments are new bytes", file=sys.stderr)
            return 2
        a.dry_run = False
        a.bin_dir = str(ROOT / "target" / "release")
        print(json.dumps(Pipeline(a).dedup_check(m), indent=1))
        return 0
    if a.cmd == "catalog":
        return write_catalog(a, reg)
    if a.cmd == "status":
        work = Path(a.work).expanduser()
        for m in reg["models"]:
            st = State(work, m)
            print(f"#{m['id']:>2} {m['stem']:<64} steps: {', '.join(s for s in STEPS if st.done(s)) or '-'}")
        return 0
    pipe = Pipeline(a)
    ids = [m["id"] for m in reg["models"]] if a.all else a.model
    if (a.local_source or a.local_legacy) and len(ids) != 1:
        raise SystemExit("--local-source / --local-legacy name one model's files: give exactly one --model")
    failed = []
    for mid in ids:
        m = model_by_id(reg, mid)
        try:
            pipe.run_model(m)
        except SystemExit as e:
            log(f"#{mid} stopped: {e}")
            failed.append(mid)
            if not a.keep_going:
                return 1
        except subprocess.CalledProcessError as e:
            log(f"#{mid} stopped: {e}")
            failed.append(mid)
            if not a.keep_going:
                return 1
    if failed:
        log(f"failed: {failed}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
