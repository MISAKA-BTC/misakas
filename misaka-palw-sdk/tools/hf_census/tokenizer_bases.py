#!/usr/bin/env python3
"""**The tokenizer a repository without one may bind from its pinned base** (HFX 2026-10-10; `TOKENIZER_MISSING`, RFC-0011 §18).

    tokenizer_bases.py --snapshot DIR [--listing dall.listing-v2.jsonl.gz] --out DIR/hfx/tokenizer-bases.jsonl
                       [--dirs FILE] [--config-cache DIR [--fetch-configs]]

A fine-tune that ships no tokenizer file registers by committing to the bytes of its base's tokenizer file (the class commits to ONE
tokenizer file; which algorithm it describes is the file's own business). This tool reads only the snapshot's saved listing — no Hub
request — and writes, for every repository that has NO tokenizer file at its root and names exactly one base (or a PEFT
`base_model_name_or_path`) found in the snapshot, ungated, whose root has a tokenizer file and whose configuration declares a
`vocab_size`, one line `{"repo", "base", "vocab_size"}`. `palw-class census gates --tokenizer-bases FILE` offers it; the preflight
accepts it only when the model's own configuration declares the SAME vocabulary size (every id the tokenizer can emit then indexes a
row of the embedding table). Nothing here is a verdict: a base whose tokenizer does not match the fine-tune's training is the
registrant's to choose against.

**The vocabulary comes from the base's `config.json`**, which the listing's configuration summary does not carry: it is read from
`--config-cache DIR/<owner>__<name>.json`, and with `--fetch-configs` the missing ones are fetched (metadata only: the Hub API's
inventory at the base's pinned sha for the file's size, then the one `config.json`, through `hfc_http`, anonymous and rate-limited —
no weight is touched). `--dirs FILE` restricts the repositories to the fetched ones (one repository directory per line, whose
`listing.json` names it): a census samples, and the sample's bases are what is fetched.

The tokenizer-file names are `misaka_palw_tir_lower::artifact::TOKENIZER_FILES_V1` (+ `*.tiktoken`); keep them equal.
"""
import argparse
import gzip
import json
import sys
from pathlib import Path

TOKENIZER_FILES = {"tokenizer.json", "tokenizer.model", "spiece.model", "sentencepiece.bpe.model", "sentencepiece.model",
                   "tekken.json", "vocab.json", "vocab.txt"}


def has_tokenizer(siblings):
    for s in siblings or []:
        if "/" in s:
            continue
        if s in TOKENIZER_FILES or s.endswith(".tiktoken"):
            return True
    return False


def vocab_of(config):
    if not isinstance(config, dict):
        return None
    v = config.get("vocab_size")
    if isinstance(v, int) and v > 0:
        return v
    t = config.get("text_config")
    if isinstance(t, dict) and isinstance(t.get("vocab_size"), int) and t["vocab_size"] > 0:
        return t["vocab_size"]
    return None


def repos_of_dirs(path):
    """The repository ids of the fetched directories listed in `path` (each holds the `listing.json` it was fetched from)."""
    out = set()
    for line in open(path):
        d = Path(line.strip())
        if not line.strip() or line.strip() == "END":
            continue
        try:
            out.add(json.load(open(d / "listing.json"))["id"])
        except (OSError, ValueError, KeyError):
            continue
    return out


def base_config(cache, fetcher, base, sha):
    """The base's configuration: the cache's, else (with a fetcher) fetched and cached; `None` if unavailable."""
    f = Path(cache) / (base.replace("/", "__") + ".json") if cache else None
    if f is not None and f.exists():
        try:
            return json.load(open(f))
        except ValueError:
            return None
    if fetcher is None or f is None:
        return None
    try:
        r = fetcher.get_api(f"https://huggingface.co/api/models/{base}/revision/{sha}?blobs=true")
        if r.status_code != 200:
            return None
        inv = {s["rfilename"]: int(s.get("size") or (s.get("lfs") or {}).get("size") or 0) for s in r.json().get("siblings") or []}
        if "config.json" not in inv:
            return None
        body = fetcher.get_small(base, sha, "config.json", inv["config.json"])
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_bytes(body)
        return json.loads(body)
    except Exception:  # noqa: BLE001 - a failed fetch is "no offer", never a verdict
        return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--listing", default="dall.listing-v2.jsonl.gz")
    ap.add_argument("--out", required=True)
    ap.add_argument("--dirs", help="restrict to the repositories of these fetched directories")
    ap.add_argument("--config-cache", help="DIR of <owner>__<name>.json base configurations")
    ap.add_argument("--fetch-configs", action="store_true", help="fetch the missing base configurations (metadata only)")
    a = ap.parse_args()
    only = repos_of_dirs(a.dirs) if a.dirs else None
    fetcher = None
    if a.fetch_configs:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import hfc_http  # noqa: E402

        fetcher = hfc_http.Client(api_share=0.3, resolver_share=0.3)
    path = Path(a.snapshot).expanduser() / a.listing
    # Pass 1: every repository's (has a root tokenizer, declared vocabulary).
    index = {}
    with gzip.open(path, "rt") as fh:
        for line in fh:
            r = json.loads(line)
            index[r["id"]] = (has_tokenizer(r.get("siblings")), vocab_of(r.get("config")), bool(r.get("gated")), bool(r.get("disabled")))
    # Pass 2: the repositories without a tokenizer that name one resolvable base.
    out = Path(a.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    n_no_tok = n_offer = 0
    with gzip.open(path, "rt") as fh, open(out, "w") as fo:
        for line in fh:
            r = json.loads(line)
            if has_tokenizer(r.get("siblings")) or (only is not None and r["id"] not in only):
                continue
            cfg = r.get("config") or {}
            if not isinstance(cfg, dict) or not (cfg.get("architectures") or cfg.get("model_type")):
                continue
            n_no_tok += 1
            peft = (cfg.get("peft") or {}).get("base_model_name_or_path") if isinstance(cfg.get("peft"), dict) else None
            ids = r.get("base_ids") or []
            base = peft or (ids[0] if len(ids) == 1 else None)
            if not base:
                continue
            res = next((b for b in (r.get("base_resolved") or []) if b.get("id") == base or b.get("renamed_from") == base), None)
            if not res or not res.get("found") or res.get("gated") or res.get("disabled"):
                continue
            tok, vocab, gated, disabled = index.get(res["id"], (False, None, True, True))
            if not tok or gated or disabled:
                continue
            if vocab is None:
                vocab = vocab_of(base_config(a.config_cache, fetcher, res["id"], res.get("sha")))
            if vocab is None:
                continue
            fo.write(json.dumps({"repo": r["id"], "base": res["id"], "vocab_size": vocab}) + "\n")
            n_offer += 1
    print(json.dumps({"schema": "misaka.palw.hf-census-tokenizer-bases.v1", "listing": a.listing, "model_repos_without_tokenizer": n_no_tok,
                      "offered": n_offer}))


if __name__ == "__main__":
    sys.exit(main())
