#!/usr/bin/env python3
"""Write header-only safetensors fixtures for the genesis models (metadata only, no weight byte).

For every model in `genesis8_models.json`, each weight shard is fetched by two bounded HTTP range
reads at the pinned revision: its 8-byte header length and its JSON header. The fixture file is that
length plus the header, cut before the data region (the same `misaka.cov-p1p2.header-fixture.v1`
layout as `misaka/palw/tir/lower/tests/fixtures/vlm-generic/huihui-qwen3.5-9b`, which
`source::read_safetensors_header` reads leniently). Small metadata files (config, index,
preprocessor/processor config) are copied whole when the registry lists them below 1 MiB.

Rules:
* Anonymous. No `Authorization` header is sent and no Hugging Face token is read or printed.
* Metadata only. A header read is refused above 64 MiB; nothing reads a tensor's bytes.
* Idempotent. A fixture whose `fixture.json` already names the pinned revision and whose shard
  header hashes match is left alone; `--force` rewrites it.

Usage:
    python3 scripts/genesis/genesis_header_fixtures.py [--model N ...] [--out DIR] [--dry-run]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
REGISTRY = HERE / "genesis8_models.json"
DEFAULT_OUT = ROOT / "misaka/palw/tir/lower/tests/fixtures/genesis8"
ENDPOINT = "https://huggingface.co"
USER_AGENT = "misaka-genesis8-header-fixtures/1 (safetensors headers and small configs only; no weight downloads)"
MAX_HEADER = 64 << 20
MAX_SMALL = 1 << 20
SMALL = ("config.json", "model.safetensors.index.json", "preprocessor_config.json", "processor_config.json",
         "generation_config.json")
# The 9B's fixture already exists at the same pinned revision; reuse it instead of a second copy.
EXISTING = {
    "huihui-ai/Huihui-Qwen3.5-9B-abliterated": ROOT / "misaka/palw/tir/lower/tests/fixtures/vlm-generic/huihui-qwen3.5-9b",
}


class _NoAuthRedirect(urllib.request.HTTPRedirectHandler):
    """Follow the Hub's CDN redirect but never carry an Authorization header across it."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: D401
        new = super().redirect_request(req, fp, code, msg, headers, newurl)
        if new is not None:
            new.headers.pop("Authorization", None)
            new.unredirected_hdrs.pop("Authorization", None)
        return new


_OPENER = urllib.request.build_opener(_NoAuthRedirect(), urllib.request.ProxyHandler({}))


def _get(url: str, rng: str | None = None, limit: int = MAX_SMALL) -> bytes:
    headers = {"User-Agent": USER_AGENT}
    if rng:
        headers["Range"] = f"bytes={rng}"
    for attempt in range(5):
        try:
            with _OPENER.open(urllib.request.Request(url, headers=headers), timeout=60) as resp:
                data = resp.read(limit + 1)
            if len(data) > limit:
                raise ValueError(f"{url}: more than {limit} bytes")
            return data
        except urllib.error.HTTPError as e:
            if e.code in (429, 500, 502, 503, 504) and attempt < 4:
                time.sleep(2 ** attempt * 3)
                continue
            raise
    raise RuntimeError("unreachable")


def header_bytes(repo: str, rev: str, path: str) -> bytes:
    url = f"{ENDPOINT}/{repo}/resolve/{rev}/{path}"
    n = struct.unpack("<Q", _get(url, "0-7", 8)[:8])[0]
    if n <= 0 or n > MAX_HEADER:
        raise ValueError(f"{repo}/{path}: header length {n} outside (0, {MAX_HEADER}]")
    hdr = _get(url, f"8-{8 + n - 1}", n)
    json.loads(hdr)  # must parse
    return struct.pack("<Q", n) + hdr


def fixture_ok(dest: Path, model: dict) -> bool:
    fx = dest / "fixture.json"
    if not fx.is_file():
        return False
    try:
        d = json.loads(fx.read_text())
    except ValueError:
        return False
    if d.get("revision") != model["source_revision"]:
        return False
    for name, rec in d.get("shards", {}).items():
        p = dest / name
        if not p.is_file() or hashlib.sha256(p.read_bytes()).hexdigest() != rec.get("header_sha256"):
            return False
    return True


def write_gguf_fixture(model: dict, out: Path, force: bool, dry: bool) -> str:
    """A GGUF source: its parsed metadata (keys, tensor infos; large arrays by length and SHA-256) per variant."""
    if str(HERE) not in sys.path:
        sys.path.insert(0, str(HERE))  # the sibling module, also under `python3 -I`
    import gguf_meta  # noqa: PLC0415

    repo, rev = model["source_repo"], model["source_revision"]
    dest = out / model["slug"]
    variants = model.get("gguf_variants") or {}
    want = {k: v for k, v in variants.items() if k in ("bf16", "iq3_s1")}
    if dry:
        return f"would write GGUF metadata of {sorted(want)} to {dest.relative_to(ROOT)}"
    dest.mkdir(parents=True, exist_ok=True)
    done = []
    for key, fname in sorted(want.items()):
        p = dest / f"gguf-meta-{key}.json"
        if p.exists() and not force:
            done.append(f"{key} present")
            continue
        src = gguf_meta.Source(url=f"{ENDPOINT}/{repo}/resolve/{rev}/{fname}")
        d = gguf_meta.parse(src)
        d["file"] = fname
        d["repo"], d["revision"], d["weights_read"] = repo, rev, False
        p.write_text(json.dumps(d, indent=1, ensure_ascii=False) + "\n")
        done.append(f"{key} ({src.fetched} metadata bytes read)")
    return f"{dest.relative_to(ROOT)}: " + ", ".join(done)


def write_fixture(model: dict, out: Path, force: bool, dry: bool) -> str:
    repo, rev = model["source_repo"], model["source_revision"]
    if model.get("source_format") == "gguf":
        return write_gguf_fixture(model, out, force, dry)
    if model.get("source_format") == "palwq361":
        return "skip: a published legacy runtime file, carried whole (no safetensors header to fixture)"
    if repo in EXISTING and fixture_ok(EXISTING[repo], model):
        return f"reuse {EXISTING[repo].relative_to(ROOT)}"
    dest = out / model["slug"]
    if not force and fixture_ok(dest, model):
        return f"present {dest.relative_to(ROOT)}"
    weights = [f for f in model["files"] if f["kind"] == "weight"]
    smalls = [f for f in model["files"] if f["path"] in SMALL and (f["bytes"] or 0) <= MAX_SMALL]
    if dry:
        return f"would write {len(weights)} shard headers + {len(smalls)} small files to {dest.relative_to(ROOT)}"
    dest.mkdir(parents=True, exist_ok=True)
    shards = {}
    for f in weights:
        b = header_bytes(repo, rev, f["path"])
        (dest / f["path"]).write_bytes(b)
        shards[f["path"]] = {"header_sha256": hashlib.sha256(b).hexdigest(), "file_bytes": f["bytes"],
                             "lfs_sha256": f["lfs_sha256"]}
    for f in smalls:
        data = _get(f"{ENDPOINT}/{repo}/resolve/{rev}/{f['path']}")
        (dest / f["path"]).write_bytes(data)
    fixture = {
        "schema": "misaka.cov-p1p2.header-fixture.v1",
        "repo": repo,
        "revision": rev,
        "license": model["license"],
        "weights_read": False,
        "shards": shards,
        "total_file_bytes": sum(f["bytes"] for f in weights),
        "note": "each shard file is its 8-byte length and JSON header only, cut after the header "
                "(source::read_safetensors_header is lenient about the data region); fetched by HTTP range "
                "from the pinned revision by scripts/genesis/genesis_header_fixtures.py",
    }
    (dest / "fixture.json").write_text(json.dumps(fixture, indent=1) + "\n")
    return f"wrote {dest.relative_to(ROOT)} ({len(shards)} shard headers)"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--model", type=int, action="append", help="registry id (repeatable); default: all")
    ap.add_argument("--out", type=Path, default=DEFAULT_OUT)
    ap.add_argument("--force", action="store_true")
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args(argv)
    reg = json.loads(REGISTRY.read_text())
    for m in reg["models"]:
        if a.model and m["id"] not in a.model:
            continue
        print(f"#{m['id']} {m['source_repo']}@{m['source_revision'][:12]}: {write_fixture(m, a.out, a.force, a.dry_run)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
