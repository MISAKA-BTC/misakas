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
  upload     `hf upload` to Misakachain/<Name>-PALW: index, segments, README model card, manifest,
             license/notice files (segments one commit each, so a large model goes in parts)
  verify     the remote SHA-256 / git blob of every uploaded file against the local record
  cleanup    delete the local download and output (only after `verify`)

and appends the model's record to `<work>/genesis10_results.json`.

Usage:
    python3 scripts/genesis/genesis10_pipeline.py run --model 2 [--stream] [--dry-run]
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
STEPS = ["preflight", "download", "convert", "record", "upload", "verify", "cleanup"]
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

def estimate(m: dict, variant: str | None, stream: bool, segment_gb: float) -> dict:
    vf = variant_file(m, variant)
    weights = [f for f in m["files"] if f["kind"] == "weight" and (vf is None or f["path"] == vf)]
    total_in = sum(f["bytes"] for f in weights)
    biggest = max((f["bytes"] for f in weights), default=0)
    if m["converter"] == "a16-legacy-wrap":
        out = m["legacy_artifact"]["bytes"]
        total_in = out  # the published legacy file is the input
        biggest = out
    elif m["source_format"] == "gguf" and vf and "iq3" in vf.lower():
        out = int(54.66e9 / 2 * 1.08)  # stored integer codes are one byte a weight in the TIR section (~27B params)
    else:
        out = int(total_in / 2 * 1.08)  # bf16 in → int8 codes (+ i16 embeddings, scales)
    seg = int(segment_gb * GB)
    if stream:
        # Sharded safetensors: two resident shards (`--shard-keep 2`) + one segment. A GGUF is read over HTTPS ranges and
        # never stored (`--gguf-url`): one segment and the small files.
        if m["source_format"] == "gguf":
            peak_in = 0
        else:
            peak_in = biggest if len(weights) <= 1 else min(total_in, 2 * biggest)
        peak = peak_in + seg + 2 * GB
    else:
        peak = total_in + out + 2 * GB
    return {"input_bytes": total_in, "biggest_input_file": biggest, "output_bytes_est": out, "peak_disk_bytes_est": peak,
            "stream": stream, "segment_bytes": seg}


# ---------------------------------------------------------------------------------------------- manifest and card

def manifest_for(m: dict, variant: str | None) -> dict:
    vf = variant_file(m, variant)
    src_files = [{"path": f["path"], "bytes": f["bytes"], "sha256": f["lfs_sha256"], "git_blob": f["git_blob"]}
                 for f in m["files"] if f["kind"] == "weight" and (vf is None or f["path"] == vf)]
    lic = {"id": m["license"]}
    if m["license"] == "llama3.3":
        lic.update({"name": "Llama 3.3 Community License Agreement", "notice": LLAMA_NOTICE, "acceptable_use_policy": LLAMA_AUP,
                    "built_with_llama": True, "name_begins_with_llama": m["stem"].startswith("Llama")})
    conv = {"converter": m["converter"], "options": m["convert_options"]}
    if m["converter"] == "a16-legacy-wrap":
        la = m["legacy_artifact"]
        conv.update({"legacy_artifact": {k: la[k] for k in ("repo", "revision", "path", "bytes", "sha256", "artifact_digest", "converter")},
                     "requantised": False})
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
    }


def attribution_line(m: dict) -> str:
    s = f"{m['source_repo']}@{m['source_revision']} ({m['derivation']} by {m['derivation_author']}) of {m['base_model']} by {m['base_author']}"
    if m["license"] == "llama3.3":
        s = "Built with Llama. " + s
    return s


def model_card(m: dict, variant: str | None, record: dict | None) -> str:
    man = manifest_for(m, variant)
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
        "model's quality or safety. The source model is an uncensored variant (refusal behaviour reduced upstream).",
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
        f"- converter: `{m['converter']}`; options: `{json.dumps(m['convert_options'], sort_keys=True)}`",
    ]
    if m["converter"] == "a16-legacy-wrap":
        la = m["legacy_artifact"]
        lines.append(f"- wraps the published legacy A16 file `{la['path']}` (SHA-256 `{la['sha256']}`, artifact digest `{la['artifact_digest'][:16]}…`) "
                     f"without re-quantising; built by `{la['converter']}`")
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
        if m["converter"] == "palwq36":
            problems.append("BLOCKED: no safetensors→PALWQ361 converter exists yet (the legacy qwen36-convert reads GGUF only); "
                            "see docs/design/palw/genesis8-status.md")
        needed = {"a16-legacy-wrap": ["palwart"], "tir": ["palw-tir-convert", "palwart"], "tir-gguf": ["palw-tir-convert", "palwart"],
                  "palwq36": []}[m["converter"]]
        for t in needed:
            if not self.a.dry_run and not Path(self.tool(t)).is_file():
                problems.append(f"binary {self.tool(t)} is missing (cargo build --release -p … --bin {t})")
        if not self.a.dry_run and not Path(self.hf).is_file():
            problems.append(f"hf CLI not found at {self.hf}")
        # Authentication for the upload: the exit status only; the token is never read or shown.
        if self.a.stream and self.a.no_upload and m["converter"] != "a16-legacy-wrap":
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

    # -- download ----------------------------------------------------------------------------------

    def download(self, m: dict, st: State) -> None:
        d = model_dirs(self.work, m)
        for k in ("src", "stage"):
            if not self.a.dry_run:
                d[k].mkdir(parents=True, exist_ok=True)
        env = anon_env()
        if m["converter"] == "a16-legacy-wrap":
            la = m["legacy_artifact"]
            p = d["legacy"] / la["path"]
            if self.a.local_legacy and not self.a.dry_run:
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
        weights = not self.a.stream
        if m["converter"] == "a16-legacy-wrap":
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

    def write_manifest(self, m: dict) -> Path:
        d = model_dirs(self.work, m)
        p = d["base"] / "manifest.json"
        if not self.a.dry_run:
            d["base"].mkdir(parents=True, exist_ok=True)
            p.write_text(json.dumps(manifest_for(m, self.a.gguf_variant), indent=1, ensure_ascii=False) + "\n")
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

    def calibration(self, m: dict, src: Path, calib_dir: Path) -> Path:
        c = m["convert_options"]["calib"]
        out = calib_dir / "calib-tokens.json"
        if not self.a.dry_run:
            calib_dir.mkdir(parents=True, exist_ok=True)
        if out.exists():
            return out
        tok = src / "tokenizer.json"
        argv = [self.a.tokenize_python, "-I", str(ROOT / "misaka/palw/tir/lower/tools/tokenize_docs.py"), "--tokenizer", str(tok),
                "--len", str(c["len"]), "--chunks", str(c["chunks"]), "--out", str(out)] + [str(ROOT / d) for d in c["docs"]]
        env = anon_env()
        env["HF_HUB_OFFLINE"] = "1"
        self.r.run(argv, env=env)
        if not self.a.dry_run:
            seqs = json.loads(out.read_text()).get("sequences", [])
            if not seqs or any(len(s) != c["len"] for s in seqs):
                raise SystemExit(f"{out}: calibration needs sequences of {c['len']} tokens, got {[len(s) for s in seqs]}")
        return out

    def convert(self, m: dict, st: State) -> None:
        d = model_dirs(self.work, m)
        manifest = self.write_manifest(m)
        if not self.a.dry_run:
            d["out"].mkdir(parents=True, exist_ok=True)
            if any(d["out"].glob(f"{m['stem']}.*")) and not st.done("convert"):
                raise SystemExit(f"{d['out']}: a previous conversion left files; remove them (the converter never overwrites) and rerun")
        report = d["base"] / "convert-report.json"
        seg = ["--segment-mib", str(self.a.segment_mib)]
        if m["converter"] == "a16-legacy-wrap":
            la = m["legacy_artifact"]
            argv = [self.tool("palwart"), "pack", "--from", str(d["legacy"] / la["path"]), "--source-dir", str(d["src"]),
                    "--out-dir", str(d["out"]), "--stem", m["stem"], "--family", m["family"], "--manifest", str(manifest), *seg]
            for lf in self.license_files(m, d["src"]):
                argv += ["--license-file", lf]
            argv += self.hook_args(m, "--on-segment", "--on-segment-arg", "--delete-after-hook")
            argv += ["--json"]
        elif m["converter"] in ("tir", "tir-gguf"):
            calib = self.calibration(m, d["src"], d["calib"])
            stats = d["calib"] / "calib-stats.json"
            o = m["convert_options"]
            model_path = d["src"]
            if m["converter"] == "tir-gguf":
                model_path = d["src"] / variant_file(m, self.a.gguf_variant)
                self.prepare_gguf_side_files(m, d)
            argv = [self.tool("palw-tir-convert"), str(model_path), "--out", str(d["base"] / "unused.palwtir"), "--calib", str(calib),
                    "--context", str(o["context"]), "--stats-out", str(stats), "--json",
                    "--palwart-out-dir", str(d["out"]), "--palwart-stem", m["stem"], "--palwart-manifest", str(manifest),
                    "--palwart-family", m["family"], "--palwart-segment-mib", str(self.a.segment_mib),
                    "--palwart-source-dir", str(d["src"]), "--palwart-calibration-file", str(calib), "--palwart-calibration-file", str(stats)]
            if o.get("gdn_core_wide"):
                argv.append("--gdn-core-wide")
            if m["converter"] == "tir-gguf":
                argv += ["--tokenizer", str(d["src"] / "tokenizer.json")]
                if not self.a.local_source and (self.a.stream or not model_path.exists()):
                    # End to end: every tensor by HTTPS range at the pinned revision, the whole-file SHA-256 computed over the
                    # streamed bytes in order and compared with the LFS id before the index is written.
                    gw = self.gguf_weight(m)
                    argv += ["--gguf-url", self.gguf_url(m), "--gguf-bytes", str(gw["bytes"]), "--gguf-sha256", gw["lfs_sha256"],
                             "--gguf-window-mib", "64", "--gguf-cache-windows", "8"]
            if self.a.calib_lazy_mib:
                # Layer-by-layer calibration with large params bound lazily (rows read when an op asks): bounded RAM.
                argv += ["--calib-lazy-mib", str(self.a.calib_lazy_mib), "--calib-lazy-cache-mib", "512"]
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

    # -- upload ------------------------------------------------------------------------------------

    def stage_files(self, m: dict, rec: dict) -> Path:
        d = model_dirs(self.work, m)
        stage = d["stage"]
        stage.mkdir(parents=True, exist_ok=True)
        (stage / "README.md").write_text(model_card(m, self.a.gguf_variant, rec))
        (stage / "manifest.json").write_text(json.dumps(manifest_for(m, self.a.gguf_variant), indent=1, ensure_ascii=False) + "\n")
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
        if self.a.dry_run:
            self.ensure_repo(m)
            self.r.run([self.hf, "upload", m["target_repo"], "<stage dir>", ".", "--commit-message", f"genesis8 #{m['id']}: PALWART3 index and card"])
            return
        self.ensure_repo(m)
        d = model_dirs(self.work, m)
        st.merge_disk()  # the segments the converter's hook uploaded
        for s in rec.get("segments", []):
            p = d["out"] / s["file"]
            if p.exists():
                self.upload_file(m, p, s["file"], s["sha256"])
            elif s["file"] not in (st.data.get("uploaded_segments") or {}):
                raise SystemExit(f"#{m['id']}: segment {s['file']} is neither local nor recorded as uploaded")
        stage = self.stage_files(m, rec)
        self.r.run([self.hf, "upload", m["target_repo"], str(stage), ".", "--exclude", "source-card/*",
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
            r = remote.get(s["file"])
            if not r or r.get("sha256") != s["sha256"] or r.get("size") != s["bytes"]:
                problems.append(f"{s['file']}: remote {r}")
        def same(r: dict | None, sha: str, blob: str) -> bool:
            # An LFS file is named by its SHA-256, a small one by its git blob id.
            return bool(r) and (r.get("sha256") == sha if r.get("sha256") else r.get("blob") == blob)

        ri = remote.get(rec["index_file"])
        if not same(ri, rec["index_sha256"], rec["index_blob"]):
            problems.append(f"{rec['index_file']}: remote {ri}")
        stage = model_dirs(self.work, m)["stage"]
        for n in ("README.md", "manifest.json", "LICENSE"):
            r = remote.get(n)
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
    if (remote.get(path.name) or {}).get("sha256") != sha:
        subprocess.run([hf, "upload", m["target_repo"], str(path), path.name, "--commit-message", f"genesis8 #{m['id']}: segment {path.name}"],
                       check=True, env=upload_env())
        remote = remote_files(m["target_repo"], "main", hf)
        r = remote.get(path.name) or {}
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
    r.add_argument("--calib-lazy-mib", type=int, default=64,
                   help="row-streamed calibration: params of at least this many MiB are read when an op asks (0: off)")
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

    cd = sub.add_parser("card", help="write a model's README model card")
    cd.add_argument("--model", type=int, required=True)
    cd.add_argument("--out", required=True)
    cd.add_argument("--record", help="the palwart-record.json of a conversion")
    cd.add_argument("--gguf-variant")

    for name in ("upload-segment", "fetch-shard"):
        h = sub.add_parser(name, help="(called by the converter)")
        common(h)
        h.add_argument("--model", type=int, required=True)

    s = sub.add_parser("status")
    common(s)

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
        Path(a.out).write_text(json.dumps(manifest_for(model_by_id(reg, a.model), a.gguf_variant), indent=1, ensure_ascii=False) + "\n")
        return 0
    if a.cmd == "card":
        rec = json.loads(Path(a.record).read_text()) if a.record else None
        Path(a.out).write_text(model_card(model_by_id(reg, a.model), a.gguf_variant, rec))
        return 0
    if a.cmd == "upload-segment":
        return hook_upload_segment(a)
    if a.cmd == "fetch-shard":
        return hook_fetch_shard(a)
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
