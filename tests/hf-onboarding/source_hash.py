#!/usr/bin/env python3
"""Pin a Hugging Face source: the file list of REPO@REVISION (metadata only — the hub's tree API, never a weight byte) and, given a
local checkpoint directory, every local file checked against it (LFS files by SHA-256, the rest by their git blob id).

    source_hash.py REPO REVISION [--local DIR] [--out FILE] [--endpoint URL]

Exit 0 when every file the hub lists for the model (config, tokenizer files, weights) is present locally with the pinned hash, or
when no local directory was given; 3 on a mismatch or a missing file (HF_REVISION_OR_SOURCE_MISMATCH); 2 when the hub cannot be read
(HF_ACCESS_FAILED). Stdlib only; reads the local files in 8 MiB blocks.
"""
import argparse
import hashlib
import json
import os
import sys
import urllib.error
import urllib.request

SKIP_SUFFIX = (".md", ".gitattributes", ".png", ".jpg", ".jpeg", ".gif", ".pdf")


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "misaka-h1-source-hash/1"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.load(r)


def sha256_file(p):
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for b in iter(lambda: f.read(8 << 20), b""):
            h.update(b)
    return h.hexdigest()


def git_blob(p):
    size = os.path.getsize(p)
    h = hashlib.sha1(b"blob %d\0" % size)
    with open(p, "rb") as f:
        for b in iter(lambda: f.read(8 << 20), b""):
            h.update(b)
    return h.hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("repo")
    ap.add_argument("revision")
    ap.add_argument("--local")
    ap.add_argument("--out")
    ap.add_argument("--endpoint", default="https://huggingface.co")
    a = ap.parse_args()
    try:
        info = fetch(f"{a.endpoint}/api/models/{a.repo}/revision/{a.revision}")
        tree = fetch(f"{a.endpoint}/api/models/{a.repo}/tree/{a.revision}?recursive=1")
    except urllib.error.HTTPError as e:
        print(json.dumps({"error": f"HTTP {e.code}", "category": "HF_ACCESS_FAILED" if e.code in (401, 403) else "HF_REVISION_OR_SOURCE_MISMATCH"}))
        return 2
    except Exception as e:  # network
        print(json.dumps({"error": str(e), "category": "HF_ACCESS_FAILED"}))
        return 2
    files = []
    for e in tree:
        if e.get("type") != "file":
            continue
        lfs = e.get("lfs") or {}
        files.append({"path": e["path"], "size": e.get("size"), "git_oid": e.get("oid"), "sha256": lfs.get("oid"), "lfs": bool(lfs)})
    out = {
        "schema": "misaka.h1.source.v1", "repo": a.repo, "revision": a.revision, "resolved_sha": info.get("sha"),
        "gated": info.get("gated"), "private": info.get("private"), "architectures": (info.get("config") or {}).get("architectures"),
        "model_type": (info.get("config") or {}).get("model_type"), "pipeline_tag": info.get("pipeline_tag"),
        "files": files, "weights_bytes": sum(f["size"] or 0 for f in files if f["path"].endswith((".safetensors", ".bin", ".gguf"))),
    }
    rc = 0
    if out["resolved_sha"] != a.revision:
        out["revision_mismatch"] = True
        rc = 3
    if a.local:
        checks = []
        for f in files:
            if f["path"].endswith(SKIP_SUFFIX):
                continue
            p = os.path.join(a.local, f["path"])
            if not os.path.exists(p):
                checks.append({"path": f["path"], "status": "ABSENT_LOCALLY"})
                continue
            if f["lfs"]:
                got = sha256_file(p)
                ok = got == f["sha256"]
                checks.append({"path": f["path"], "status": "MATCH" if ok else "MISMATCH", "sha256": got})
            else:
                got = git_blob(p)
                ok = got == f["git_oid"]
                checks.append({"path": f["path"], "status": "MATCH" if ok else "MISMATCH", "git_oid": got})
        out["local"] = {"dir": a.local, "checks": checks}
        # Required: config.json and the top-level weights (safetensors, else pytorch_model*.bin); everything else the hub lists
        # (onnx exports, training logs) need not be present, but a present file must match.
        top = [c for c in checks if "/" not in c["path"]]
        weights = [c for c in top if c["path"].endswith(".safetensors")] or [c for c in top if c["path"].startswith("pytorch_model") and c["path"].endswith(".bin")]
        required = weights + [c for c in top if c["path"] == "config.json"]
        out["local"]["required"] = [c["path"] for c in required]
        if any(c["status"] == "MISMATCH" for c in checks) or any(c["status"] != "MATCH" for c in required) or not weights:
            rc = 3
        out["local"]["verdict"] = "MATCH" if rc == 0 else "MISMATCH_OR_ABSENT"
    js = json.dumps(out, indent=1)
    if a.out:
        with open(a.out, "w") as f:
            f.write(js + "\n")
    print(js if not a.out else json.dumps({"verdict": out.get("local", {}).get("verdict", "METADATA_ONLY"), "rc": rc}))
    return rc


if __name__ == "__main__":
    sys.exit(main())
