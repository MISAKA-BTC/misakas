#!/usr/bin/env python3
"""**Ollama's library as a local-LLM listing** (RFC-0002 §II.12.1): metadata only.

    ollama.py --snapshot DIR [--interval 1.0]

Network policy (the user's, 2026-10-03): the library and listing pages and the registry **manifests** only — never a model blob or
layer, not even a byte range of one. Everything fetched is saved as received (`DIR/ollama/…`, gzip) with its URL and time, so the
units are reproducible offline.

1. `https://ollama.com/library` — the model names.
2. `https://ollama.com/library/<model>/tags` — every tag with its short digest, size, context window and input modalities.
3. One registry manifest per checkpoint unit (`https://registry.ollama.ai/v2/library/<model>/manifests/<tag>`): the model layer's full
   digest and size and the other layer types (a `projector` layer is a vision model's; an `adapter` layer an adapter's).

**Units.** A checkpoint unit is `<model>:<stem>` where the stem is the tag without its quantisation suffix (`8b-instruct-q4_K_M` →
`8b-instruct`); tags with the same short digest are one file (an alias such as `8b` or `latest` collapses onto it by digest); the
quantisations of one stem are the unit's format stratum, not extra units. Output: `DIR/ollama/units.jsonl`.
"""

from __future__ import annotations

import argparse
import datetime as dt
import gzip
import hashlib
import html
import json
import re
import sys
import time
from pathlib import Path

import httpx

UA = "misaka-palw-hf-census/0.1 (metadata-only local-model census; RFC-0002 II.12; no blob downloads)"
LIB = "https://ollama.com/library"
REG = "https://registry.ollama.ai/v2/library"
QUANT = re.compile(r"-(q\d[\w]*|iq\d[\w]*|fp16|fp32|bf16|f16|f32|mxfp4|nvfp4|int4|int8|fp8)$", re.I)


class Polite:
    def __init__(self, interval: float):
        self.c = httpx.Client(headers={"User-Agent": UA}, timeout=60, follow_redirects=True, trust_env=False)
        self.interval = interval
        self.last = 0.0
        self.requests = 0

    def get(self, url: str, accept: str | None = None) -> httpx.Response:
        for attempt in range(6):
            wait = self.last + self.interval - time.monotonic()
            if wait > 0:
                time.sleep(wait)
            self.last = time.monotonic()
            self.requests += 1
            try:
                r = self.c.get(url, headers={"Accept": accept} if accept else {})
            except (httpx.TimeoutException, httpx.TransportError):
                time.sleep(2**attempt)
                continue
            if r.status_code == 429 or r.status_code >= 500:
                time.sleep(min(120, 5 * 2**attempt))
                continue
            return r
        raise RuntimeError(f"{url}: no answer after retries")


def save(root: Path, name: str, url: str, r: httpx.Response) -> None:
    p = root / name
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(gzip.compress(r.content))
    with open(root / "fetch.log.jsonl", "a") as f:
        f.write(json.dumps({"url": url, "status": r.status_code, "bytes": len(r.content), "sha256": hashlib.sha256(r.content).hexdigest(), "at": dt.datetime.now(dt.UTC).isoformat(), "store": name}) + "\n")


def parse_tags(model: str, page: str) -> list[dict]:
    """The tags of a model's tags page: name, short digest, size, context, inputs."""
    out = {}
    text = page
    for m in re.finditer(r'href="/library/' + re.escape(model) + r':([^"]+)"', text):
        tag = html.unescape(m.group(1))
        if tag in out:
            continue
        window = re.sub(r"\s+", " ", re.sub(r"<[^>]+>", " ", text[m.end() : m.end() + 2500]))
        dg = re.search(r"\b([0-9a-f]{12})\b", window)
        size = re.search(r"([\d.]+)\s*(KB|MB|GB|TB)\b", window)
        ctx = re.search(r"([\d.]+)\s*(K|M)?\s*context window", window)
        inputs = re.search(r"((?:Text|Image|Audio|Video)(?:,\s*(?:Text|Image|Audio|Video))*)\s+input", window)
        out[tag] = {
            "tag": tag,
            "digest12": dg.group(1) if dg else None,
            "size": f"{size.group(1)}{size.group(2)}" if size else None,
            "context": (ctx.group(1) + (ctx.group(2) or "")) if ctx else None,
            "inputs": inputs.group(1) if inputs else None,
        }
    return list(out.values())


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--interval", type=float, default=1.0)
    ap.add_argument("--limit", type=int, default=0)
    a = ap.parse_args()
    root = Path(a.snapshot).expanduser() / "ollama"
    root.mkdir(parents=True, exist_ok=True)
    p = Polite(a.interval)
    r = p.get(LIB)
    save(root, "library.html.gz", LIB, r)
    models = sorted(set(html.unescape(x) for x in re.findall(r'href="/library/([^"/:?#]+)"', r.text)))
    if a.limit:
        models = models[: a.limit]
    print(f"{len(models)} models", flush=True)
    units = []
    for i, model in enumerate(models):
        url = f"{LIB}/{model}/tags"
        tp = root / "tags" / f"{model}.html.gz"
        if tp.exists():
            page = gzip.decompress(tp.read_bytes()).decode("utf-8", "replace")
        else:
            r = p.get(url)
            save(root, f"tags/{model}.html.gz", url, r)
            page = r.text if r.status_code == 200 else ""
        tags = parse_tags(model, page)
        # Units: the stem without its quantisation; aliases collapse onto a quantised tag by digest.
        by_digest = {}
        for t in tags:
            by_digest.setdefault(t["digest12"], []).append(t)
        stems = {}
        for dg, ts in by_digest.items():
            quant = [t for t in ts if QUANT.search(t["tag"])]
            rep = quant[0] if quant else ts[0]
            m = QUANT.search(rep["tag"])
            stem = rep["tag"][: m.start()] if m else rep["tag"]
            fmt = m.group(1).lower() if m else "default"
            u = stems.setdefault(stem, {"unit": f"{model}:{stem}", "model": model, "stem": stem, "files": []})
            u["files"].append({"digest12": dg, "format": fmt, "tags": sorted(t["tag"] for t in ts), "size": rep["size"], "context": rep["context"], "inputs": rep["inputs"]})
        for stem, u in sorted(stems.items()):
            # One manifest per unit: the alias-carrying file (the default the library serves), else the first.
            pick = next((f for f in u["files"] if any(not QUANT.search(t) for t in f["tags"])), u["files"][0])
            tag = next((t for t in pick["tags"] if not QUANT.search(t)), pick["tags"][0])
            mp = root / "manifests" / model / f"{tag}.json.gz"
            murl = f"{REG}/{model}/manifests/{tag}"
            if mp.exists():
                man = json.loads(gzip.decompress(mp.read_bytes()))
            else:
                r = p.get(murl, accept="application/vnd.docker.distribution.manifest.v2+json")
                save(root, f"manifests/{model}/{tag}.json.gz", murl, r)
                man = r.json() if r.status_code == 200 else {"error": r.status_code}
            layers = man.get("layers") or []
            u["manifest_tag"] = tag
            u["manifest_status"] = "ok" if layers else f"error {man.get('error')}"
            u["layers"] = [{"type": l.get("mediaType", "").rsplit(".", 1)[-1], "digest": l.get("digest"), "size": l.get("size")} for l in layers]
            u["model_digest"] = next((l["digest"] for l in layers if l.get("mediaType", "").endswith(".model")), None)
            u["formats"] = sorted({f["format"] for f in u["files"]})
            units.append(u)
        if (i + 1) % 20 == 0:
            print(f"{i + 1}/{len(models)} models, {len(units)} units, {p.requests} requests", flush=True)
    with open(root / "units.jsonl", "w") as f:
        for u in units:
            f.write(json.dumps(u) + "\n")
    print(json.dumps({"models": len(models), "units": len(units), "requests": p.requests}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
