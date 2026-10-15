#!/usr/bin/env python3
"""Bounded GGUF metadata reader (header, key/value metadata, tensor infos) — never a tensor byte.

GGUF metadata is UNTRUSTED input: every count and length is checked against a cap before it sizes
anything, and parsing stops at the tensor-info table's end. The source is a local file or an
anonymous HTTP range reader (`https://huggingface.co/<repo>/resolve/<rev>/<file>`), read in bounded
windows; no Authorization header is sent and no Hugging Face token is read.

    python3 scripts/genesis/gguf_meta.py --url https://huggingface.co/<repo>/resolve/<rev>/<file>.gguf --out meta.json
    python3 scripts/genesis/gguf_meta.py --file model.gguf --keys-only

The JSON lists every key with its type and (for arrays) its length and first values, and every
tensor's name, ggml type, shape and offset. Large arrays (the tokenizer's vocabulary) are summarised
by length and SHA-256 of their canonical JSON, never embedded whole.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import struct
import sys
import urllib.error
import urllib.request

MAGIC = b"GGUF"
MAX_META_BYTES = 256 << 20  # the KV + tensor-info region a reader accepts
MAX_KV = 1 << 16
MAX_TENSORS = 1 << 20
MAX_STRING = 64 << 20
MAX_ARRAY = 1 << 24
MAX_DIMS = 8
WINDOW = 8 << 20
ARRAY_PREVIEW = 8

# GGUF value types.
T_U8, T_I8, T_U16, T_I16, T_U32, T_I32, T_F32, T_BOOL, T_STR, T_ARR, T_U64, T_I64, T_F64 = range(13)
SCALAR = {T_U8: "<B", T_I8: "<b", T_U16: "<H", T_I16: "<h", T_U32: "<I", T_I32: "<i", T_F32: "<f", T_BOOL: "<?",
          T_U64: "<Q", T_I64: "<q", T_F64: "<d"}
TYPE_NAME = {T_U8: "u8", T_I8: "i8", T_U16: "u16", T_I16: "i16", T_U32: "u32", T_I32: "i32", T_F32: "f32", T_BOOL: "bool",
             T_STR: "string", T_ARR: "array", T_U64: "u64", T_I64: "i64", T_F64: "f64"}
# ggml tensor types (subset named; others reported by number).
GGML_TYPE = {0: "F32", 1: "F16", 2: "Q4_0", 3: "Q4_1", 6: "Q5_0", 7: "Q5_1", 8: "Q8_0", 9: "Q8_1", 10: "Q2_K", 11: "Q3_K",
             12: "Q4_K", 13: "Q5_K", 14: "Q6_K", 15: "Q8_K", 16: "IQ2_XXS", 17: "IQ2_XS", 18: "IQ3_XXS", 19: "IQ1_S",
             20: "IQ4_NL", 21: "IQ3_S", 22: "IQ2_S", 23: "IQ4_XS", 24: "I8", 25: "I16", 26: "I32", 27: "I64", 28: "F64",
             29: "IQ1_M", 30: "BF16", 34: "TQ1_0", 35: "TQ2_0", 39: "MXFP4"}


class GgufError(Exception):
    pass


class Source:
    """Bounded random access: a local file or an anonymous HTTP range reader."""

    def __init__(self, path: str | None = None, url: str | None = None):
        self.path, self.url = path, url
        self.f = open(path, "rb") if path else None
        self.cache_at, self.cache = 0, b""
        self.fetched = 0
        if url:
            self.opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))

    def _fetch(self, at: int, n: int) -> bytes:
        if self.f is not None:
            self.f.seek(at)
            return self.f.read(n)
        req = urllib.request.Request(self.url, headers={"User-Agent": "misaka-genesis8-gguf-meta/1 (metadata only)",
                                                         "Range": f"bytes={at}-{at + n - 1}"})
        with self.opener.open(req, timeout=120) as r:
            data = r.read(n + 1)
        if len(data) > n:
            raise GgufError("the server ignored the range request")
        self.fetched += len(data)
        return data

    def read(self, at: int, n: int) -> bytes:
        if at + n > MAX_META_BYTES:
            raise GgufError(f"metadata region past {MAX_META_BYTES} bytes")
        if not (self.cache_at <= at and at + n <= self.cache_at + len(self.cache)):
            self.cache_at = at
            self.cache = self._fetch(at, max(n, WINDOW))
        s = at - self.cache_at
        out = self.cache[s:s + n]
        if len(out) != n:
            raise GgufError("the file ends inside its metadata")
        return out


class Reader:
    def __init__(self, src: Source):
        self.src, self.at = src, 0

    def take(self, n: int) -> bytes:
        b = self.src.read(self.at, n)
        self.at += n
        return b

    def scalar(self, t: int):
        fmt = SCALAR[t]
        return struct.unpack(fmt, self.take(struct.calcsize(fmt)))[0]

    def string(self) -> str:
        n = self.scalar(T_U64)
        if n > MAX_STRING:
            raise GgufError(f"a string of {n} bytes")
        return self.take(n).decode("utf-8", errors="replace")

    def value(self, t: int):
        if t in SCALAR:
            return self.scalar(t)
        if t == T_STR:
            return self.string()
        if t == T_ARR:
            et = self.scalar(T_U32)
            n = self.scalar(T_U64)
            if n > MAX_ARRAY:
                raise GgufError(f"an array of {n} elements")
            if et == T_ARR:
                raise GgufError("nested arrays are refused")
            if et not in SCALAR and et != T_STR:
                raise GgufError(f"array element type {et}")
            items = [self.value(et) for _ in range(n)]
            return {"array_of": TYPE_NAME.get(et, str(et)), "items": items}
        raise GgufError(f"value type {t}")


def parse_full(src: Source, keep_arrays: set[str] = frozenset(), keep_strings: set[str] = frozenset()) -> dict:
    """`parse` plus the full values of the named arrays and strings (each still bounded by the caps):
    `{"summary": <parse result>, "arrays": {key: [...]}, "strings": {key: str}}`."""
    kept: dict = {}
    summary = parse(src, keep=kept, keep_keys=set(keep_arrays) | set(keep_strings))
    return {"summary": summary,
            "arrays": {k: v for k, v in kept.items() if k in keep_arrays and isinstance(v, list)},
            "strings": {k: v for k, v in kept.items() if k in keep_strings and isinstance(v, str)}}


def parse(src: Source, keep: dict | None = None, keep_keys: set[str] = frozenset()) -> dict:
    r = Reader(src)
    if r.take(4) != MAGIC:
        raise GgufError("not a GGUF file")
    version = r.scalar(T_U32)
    if version not in (2, 3):
        raise GgufError(f"GGUF version {version}")
    n_tensors = r.scalar(T_U64)
    n_kv = r.scalar(T_U64)
    if n_tensors > MAX_TENSORS or n_kv > MAX_KV:
        raise GgufError(f"{n_tensors} tensors / {n_kv} keys exceed the caps")
    meta = {}
    for _ in range(n_kv):
        k = r.string()
        if k in meta:
            raise GgufError(f"duplicate key {k}")
        t = r.scalar(T_U32)
        v = r.value(t)
        meta[k] = (t, v)
        if keep is not None and k in keep_keys:
            keep[k] = v["items"] if isinstance(v, dict) else v
    alignment = 32
    if "general.alignment" in meta:
        alignment = int(meta["general.alignment"][1])
        if alignment <= 0 or alignment & (alignment - 1) or alignment > 1 << 20:
            raise GgufError(f"alignment {alignment}")
    tensors = []
    for _ in range(n_tensors):
        name = r.string()
        nd = r.scalar(T_U32)
        if nd > MAX_DIMS:
            raise GgufError(f"{name}: {nd} dims")
        dims = [r.scalar(T_U64) for _ in range(nd)]
        ty = r.scalar(T_U32)
        off = r.scalar(T_U64)
        tensors.append({"name": name, "type": GGML_TYPE.get(ty, str(ty)), "type_id": ty, "shape": dims, "offset": off})
    data_start = (r.at + alignment - 1) // alignment * alignment
    out_meta = {}
    for k, (t, v) in meta.items():
        if isinstance(v, dict):
            items = v["items"]
            blob = json.dumps(items, ensure_ascii=False, separators=(",", ":")).encode()
            out_meta[k] = {"type": "array", "of": v["array_of"], "len": len(items), "head": items[:ARRAY_PREVIEW],
                           "sha256": hashlib.sha256(blob).hexdigest()}
        else:
            out_meta[k] = {"type": TYPE_NAME.get(t, str(t)), "value": v}
    return {"version": version, "n_tensors": n_tensors, "n_kv": n_kv, "alignment": alignment, "metadata_bytes": r.at,
            "data_start": data_start, "metadata": out_meta, "tensors": tensors}


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--file")
    g.add_argument("--url")
    ap.add_argument("--out")
    ap.add_argument("--keys-only", action="store_true")
    a = ap.parse_args(argv)
    src = Source(path=a.file, url=a.url)
    try:
        d = parse(src)
    except (GgufError, urllib.error.URLError, struct.error) as e:
        print(f"gguf_meta: {e}", file=sys.stderr)
        return 2
    d["fetched_bytes"] = src.fetched
    if a.out:
        with open(a.out, "w") as f:
            json.dump(d, f, indent=1, ensure_ascii=False)
    if a.keys_only:
        for k, v in d["metadata"].items():
            print(k, v.get("value", f"[{v.get('of')} x {v.get('len')}]"))
    else:
        types = {}
        for t in d["tensors"]:
            types[t["type"]] = types.get(t["type"], 0) + 1
        print(json.dumps({"version": d["version"], "n_tensors": d["n_tensors"], "n_kv": d["n_kv"],
                          "metadata_bytes": d["metadata_bytes"], "tensor_types": types}, indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
