#!/usr/bin/env python3
"""**The header fetch of sampled repositories** (RFC-0002 §II.10.2): metadata and headers only, by exact byte ranges, resumable.

    fetch.py --snapshot DIR --sample DIR/sample/sample.jsonl [--workers 6] [--api-share 0.8] [--resolver-share 0.6] [--limit N]

Each line of the sample is `{"id", "listing": ListingV1, "plan": PlanV1, ...}` (the sampler's output). For each repository, in its own
directory `DIR/repos/<hh>/<id>/`:

1. `GET /api/models/<id>/revision/<sha>?blobs=true` — the inventory at the pinned commit: every file's size, git blob id and LFS
   SHA-256 (saved as received, `info.json.gz`).
2. The plan's metadata files, whole (`f/<path>`): the configuration, the safetensors index, `model_index.json` and the component
   configurations, `adapter_config.json`. A file larger than 16 MiB is refused by name, never read.
3. Safetensors headers: the 8-byte length, then exactly that many bytes of JSON — the shards the index names (or the plan's files).
   A header longer than 64 MiB is `header_too_large` and is not read.
4. GGUF headers: the magic, the metadata and the tensor infos, read by ranges that never pass the header's end: each request asks
   only for bytes the structure parsed so far proves exist (every remaining key, array element and tensor info has a minimum size), so
   no byte of the data section is ever fetched. Stored at its length with the large arrays' contents zeroed (`h/<path>.hdr.gz`),
   with the SHA-256 of the bytes as fetched.
5. `listing.json` and `fetch.json` (`misaka.palw.hf-census-fetch.v1`): one item per read, its status, size, hash and stored copy.

A repository is written to a temporary directory and renamed when complete, so a killed run resumes at the first repository without
a `fetch.json`. Failures are results: a 404, a gated file, a timeout after retries are recorded in `fetch.json`, and the census counts
them as failures in D_all.
"""

from __future__ import annotations

import argparse
import concurrent.futures as cf
import datetime as dt
import gzip
import hashlib
import json
import os
import shutil
import struct
import sys
import threading
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import hfc_http  # noqa: E402
from hfc_http import FetchError  # noqa: E402

FETCH_SCHEMA = "misaka.palw.hf-census-fetch.v1"
GGUF_ZERO_ARRAYS_OVER = 1024


def repo_dir(root: Path, repo: str) -> Path:
    hh = hashlib.sha256(repo.encode()).hexdigest()[:2]
    return root / hh / repo.replace("/", "__")


def now() -> str:
    return dt.datetime.now(dt.UTC).isoformat()


# ---- a byte source over one remote file: the resolver once, then exact ranges ---------------------------------------------------------
class Remote:
    def __init__(self, client: hfc_http.Client, repo: str, rev: str, path: str):
        self.c, self.repo, self.rev, self.path = client, repo, rev, path
        self.location: str | None = None
        self.size: int | None = None
        self.first: bytes | None = None
        self.requests = 0

    def open(self, first_len: int) -> bytes:
        """Ask the resolver for bytes [0, first_len): a 302 gives the CDN location (and the size); a 206 is a non-LFS file served
        directly. Returns the first bytes."""
        url = f"{hfc_http.ENDPOINT}/{self.repo}/resolve/{self.rev}/{hfc_http.quote_path(self.path)}"
        # Streamed: a resolver that answered a weight file with its whole body (a 200) is cut off unread.
        for attempt in range(6):
            self.c.resolvers.acquire()
            self.requests += 1
            try:
                with self.c.http.stream("GET", url, headers={"Range": f"bytes=0-{first_len - 1}"}) as r:
                    self.c.resolvers.observe(r.headers)
                    if r.status_code in (301, 302, 307, 308) and r.headers.get("location"):
                        self.location = r.headers["location"]
                        if r.headers.get("x-linked-size"):
                            self.size = int(r.headers["x-linked-size"])
                        break
                    if r.status_code == 206:
                        cr = r.headers.get("content-range", "")
                        if "/" in cr and cr.rsplit("/", 1)[1].isdigit():
                            self.size = int(cr.rsplit("/", 1)[1])
                        body = bytearray()
                        for chunk in r.iter_bytes():
                            body += chunk
                            if len(body) > first_len:
                                raise FetchError("range_oversized", f"{self.path}: more than {first_len} bytes", 206)
                        if len(body) != first_len:
                            raise FetchError("short_range", f"{self.path}: asked {first_len}, got {len(body)}", 206)
                        self.location = url
                        return bytes(body)
                    if r.status_code == 200:
                        raise FetchError("range_ignored", f"{self.path}: the server ignored Range: not read", 200)
                    if r.status_code == 429 or r.status_code >= 500:
                        time.sleep(min(120, 2 * 2**attempt))
                        continue
                    r.read()
                    raise hfc_http.classify(r.status_code, self.path, r)
            except (hfc_http.httpx.TimeoutException, hfc_http.httpx.TransportError):
                time.sleep(min(120, 2 * 2**attempt))
                continue
        else:
            raise FetchError("timeout", f"{self.path}: the resolver did not answer")
        return self.range(0, first_len)

    def range(self, start: int, end: int) -> bytes:
        assert self.location
        if self.size is not None and end > self.size:
            raise FetchError("header_invalid", f"{self.path}: a header that runs past the file ({end} > {self.size})")
        self.requests += 1
        return self.c.cdn_range(self.location, start, end)


# ---- safetensors -----------------------------------------------------------------------------------------------------------------------
def st_header(client, repo, rev, path) -> tuple[bytes, int | None, int]:
    rm = Remote(client, repo, rev, path)
    head = rm.open(8)
    n = struct.unpack("<Q", head)[0]
    if n == 0:
        raise FetchError("header_invalid", f"{path}: a header length of 0")
    if 8 + n > hfc_http.MAX_HEADER_BYTES:
        raise FetchError("header_too_large", f"{path}: a header of {n} bytes is over the {hfc_http.MAX_HEADER_BYTES} cap")
    if rm.size is not None and 8 + n > rm.size:
        raise FetchError("header_invalid", f"{path}: a header of {n} bytes in a file of {rm.size}")
    body = rm.range(8, 8 + n)
    try:
        v = json.loads(body)
        if not isinstance(v, dict):
            raise ValueError("not an object")
    except Exception as e:  # noqa: BLE001
        raise FetchError("header_invalid", f"{path}: the header is not a JSON object: {e}")
    return head + body, rm.size, rm.requests


# ---- GGUF ------------------------------------------------------------------------------------------------------------------------------
_SCALAR = {0: 1, 1: 1, 2: 2, 3: 2, 4: 4, 5: 4, 6: 4, 7: 1, 10: 8, 11: 8, 12: 8}


def _elem_min(t: int) -> int:
    if t in _SCALAR:
        return _SCALAR[t]
    if t == 8:
        return 8
    if t == 9:
        return 12
    raise FetchError("header_invalid", f"GGUF value type {t}")


class GgufReader:
    """Parses a GGUF header while fetching only bytes the parse proves exist."""

    def __init__(self, rm: Remote, first: bytes):
        self.rm = rm
        self.buf = bytearray(first)
        self.pos = 0
        self.zero: list[tuple[int, int]] = []

    def need(self, k: int, tail: int) -> None:
        end = self.pos + k
        if end <= len(self.buf):
            return
        upto = end + tail
        if upto > hfc_http.MAX_HEADER_BYTES:
            if end > hfc_http.MAX_HEADER_BYTES:
                raise FetchError("header_too_large", f"{self.rm.path}: a GGUF header past {hfc_http.MAX_HEADER_BYTES} bytes")
            upto = hfc_http.MAX_HEADER_BYTES
        if self.rm.size is not None:
            if end > self.rm.size:
                raise FetchError("header_invalid", f"{self.rm.path}: the GGUF header runs past the file")
            upto = min(upto, self.rm.size)
        self.buf += self.rm.range(len(self.buf), upto)

    def u32(self, tail):
        self.need(4, tail)
        v = struct.unpack_from("<I", self.buf, self.pos)[0]
        self.pos += 4
        return v

    def u64(self, tail):
        self.need(8, tail)
        v = struct.unpack_from("<Q", self.buf, self.pos)[0]
        self.pos += 8
        return v

    def string(self, tail, zero=False):
        n = self.u64(tail)
        if n > (64 << 20):
            raise FetchError("header_invalid", f"a GGUF string of {n} bytes")
        self.need(n, tail)
        s = bytes(self.buf[self.pos : self.pos + n])
        if zero and n:
            self.zero.append((self.pos, self.pos + n))
        self.pos += n
        return s

    def value(self, t: int, tail: int, depth: int = 0):
        if t in _SCALAR:
            k = _SCALAR[t]
            self.need(k, tail)
            raw = bytes(self.buf[self.pos : self.pos + k])
            self.pos += k
            return raw
        if t == 8:
            return self.string(tail)
        if t == 9:
            if depth > 2:
                raise FetchError("header_invalid", "GGUF arrays nested too deep")
            et = self.u32(tail + 8)
            n = self.u64(tail)
            if n > 100_000_000:
                raise FetchError("header_invalid", f"a GGUF array of {n} elements")
            em = _elem_min(et)
            big = n > GGUF_ZERO_ARRAYS_OVER
            if et in _SCALAR:
                k = _SCALAR[et] * n
                self.need(k, tail)
                if big:
                    self.zero.append((self.pos, self.pos + k))
                self.pos += k
                return None
            for j in range(n):
                rest = (n - j - 1) * em + tail
                if et == 8:
                    self.string(rest, zero=big)
                else:
                    sub_t = self.u32(rest + 8)
                    self.value_array_body(sub_t, rest, depth + 1)
            return None
        raise FetchError("header_invalid", f"GGUF value type {t}")

    def value_array_body(self, et, tail, depth):
        # A nested array: its element type was read; its count and elements follow.
        n = self.u64(tail)
        em = _elem_min(et)
        for j in range(n):
            self.value(et, (n - j - 1) * em + tail, depth)

    def parse(self) -> dict:
        self.need(24, 0)
        if bytes(self.buf[:4]) != b"GGUF":
            raise FetchError("header_invalid", f"{self.rm.path}: not a GGUF file")
        self.pos = 4
        version = self.u32(0)
        if version not in (2, 3):
            raise FetchError("header_invalid", f"GGUF version {version}")
        n_t = self.u64(8)
        n_kv = self.u64(0)
        if n_t > 1_000_000 or n_kv > 1_000_000:
            raise FetchError("header_invalid", f"GGUF: {n_t} tensors, {n_kv} keys")
        # The least a remaining part can take: a key/value pair 13 bytes (length 8, an empty key, type 4, a 1-byte value), a tensor
        # info 32 (length 8, an empty name, dims 4, one dimension 8, type 4, offset 8). Asking for no more than these never reads past
        # the header.
        tail_t = n_t * 32
        arch = None
        for i in range(n_kv):
            tail = (n_kv - i - 1) * 13 + tail_t
            key = self.string(tail + 5)
            t = self.u32(tail + 1)
            v = self.value(t, tail)
            if key == b"general.architecture" and isinstance(v, (bytes, bytearray)):
                arch = v.decode("utf-8", "replace")
        for i in range(n_t):
            tail = (n_t - i - 1) * 32
            self.string(tail + 24)
            nd = self.u32(tail + 20)
            if nd == 0 or nd > 8:
                raise FetchError("header_invalid", f"a GGUF tensor of {nd} dimensions")
            for d in range(nd):
                self.u64(tail + 12 + 8 * (nd - d - 1))
            self.u32(tail + 8)
            self.u64(tail)
        return {"version": version, "tensors": n_t, "kv": n_kv, "header_len": self.pos, "architecture": arch}


def gguf_header(client, repo, rev, path):
    rm = Remote(client, repo, rev, path)
    first = rm.open(24)
    g = GgufReader(rm, first)
    info = g.parse()
    raw = bytes(g.buf[: info["header_len"]])
    comp = bytearray(raw)
    for a, b in g.zero:
        comp[a:b] = bytes(b - a)
    return raw, bytes(comp), rm.size, rm.requests, info


# ---- an adapter's base ---------------------------------------------------------------------------------------------------------------
def fetch_base(client, tmp: Path, repo: str, rev: str, item) -> dict:
    """The base of an adapter at its pinned commit: the inventory, `config.json`, the safetensors index and the shard headers (or
    `model.safetensors`'s), stored under `base/`. Returns the base record for `fetch.json`."""
    rec = {"repo": repo, "revision": rev, "info": {}, "inventory": []}
    try:
        r = client.get_api(f"{hfc_http.ENDPOINT}/api/models/{repo}/revision/{rev}?blobs=true")
        if r.status_code != 200:
            e = hfc_http.classify(r.status_code, repo, r)
            rec["info"] = {"status": "error", "error": e.kind, "detail": e.detail}
            return rec
        mi = r.json()
        rec["info"] = {"status": "ok", "sha": mi.get("sha"), "gated": mi.get("gated", False)}
        for s in mi.get("siblings") or []:
            lfs = s.get("lfs") or {}
            rec["inventory"].append({"path": s.get("rfilename"), "size": int(s.get("size") or lfs.get("size") or 0), "lfs_sha256": lfs.get("sha256"), "blob_id": s.get("blobId")})
    except FetchError as e:
        rec["info"] = {"status": "error", "error": e.kind, "detail": e.detail}
        return rec
    inv = {f["path"]: f for f in rec["inventory"]}
    files = [p for p in ("config.json", "model.safetensors.index.json") if p in inv]
    for p in files:
        try:
            body = client.get_small(repo, rev, p, inv[p]["size"])
            dst = tmp / "f" / "base" / p
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(body)
            item("base/" + p, "file", status="ok", store=f"f/base/{p}", bytes=len(body), sha256=hashlib.sha256(body).hexdigest(), file_size=inv[p]["size"])
        except FetchError as e:
            item("base/" + p, "file", status="error", error=e.kind, detail=e.detail[:300])
    shards = []
    if "model.safetensors.index.json" in inv and (tmp / "f" / "base" / "model.safetensors.index.json").exists():
        try:
            wm = json.loads((tmp / "f" / "base" / "model.safetensors.index.json").read_bytes()).get("weight_map") or {}
            shards = sorted(set(v for v in wm.values() if isinstance(v, str) and "/" not in v))
        except ValueError:
            shards = []
    elif "model.safetensors" in inv:
        shards = ["model.safetensors"]
    for p in shards[:256]:
        if p not in inv:
            continue
        try:
            hb, size, nreq = st_header(client, repo, rev, p)
            name = f"h/base/{p}.hdr.gz"
            dst = tmp / name
            dst.parent.mkdir(parents=True, exist_ok=True)
            dst.write_bytes(gzip.compress(hb, compresslevel=6))
            item("base/" + p, "st_header", status="ok", store=name, bytes=len(hb), sha256=hashlib.sha256(hb).hexdigest(), file_size=inv[p]["size"], header_len=len(hb))
        except FetchError as e:
            item("base/" + p, "st_header", status="error", error=e.kind, detail=e.detail[:300], file_size=inv[p]["size"])
    return rec


# ---- one repository ---------------------------------------------------------------------------------------------------------------------
def fetch_repo(client: hfc_http.Client, root: Path, rec: dict) -> dict:
    l = rec["listing"]
    plan = rec["plan"]
    repo, rev = l["id"], l["sha"]
    final = repo_dir(root, repo)
    if (final / "fetch.json").exists():
        return {"repo": repo, "skipped": True}
    tmp = final.with_name(final.name + f".tmp{os.getpid()}")
    shutil.rmtree(tmp, ignore_errors=True)
    (tmp / "f").mkdir(parents=True)
    (tmp / "h").mkdir(parents=True)
    out = {"schema": FETCH_SCHEMA, "repo": repo, "revision": rev, "started_at": now(), "info": {}, "inventory": [], "items": []}
    req0 = client.bytes_in
    # 1. the inventory at the pinned commit
    try:
        r = client.get_api(f"{hfc_http.ENDPOINT}/api/models/{repo}/revision/{rev}?blobs=true")
        if r.status_code in (301, 302, 307, 308):
            out["info"] = {"status": "error", "error": "renamed", "detail": r.headers.get("location", "")}
        elif r.status_code != 200:
            e = hfc_http.classify(r.status_code, repo, r)
            out["info"] = {"status": "error", "error": e.kind, "detail": e.detail}
        else:
            (tmp / "info.json.gz").write_bytes(gzip.compress(r.content))
            mi = r.json()
            out["info"] = {"status": "ok", "sha": mi.get("sha"), "gated": mi.get("gated", False)}
            for s in mi.get("siblings") or []:
                lfs = s.get("lfs") or {}
                out["inventory"].append(
                    {"path": s.get("rfilename"), "size": int(s.get("size") or lfs.get("size") or 0), "lfs_sha256": lfs.get("sha256"), "blob_id": s.get("blobId")}
                )
    except FetchError as e:
        out["info"] = {"status": "error", "error": e.kind, "detail": e.detail}
    inv = {f["path"]: f for f in out["inventory"]}

    def item(path, kind, **kw):
        it = {"path": path, "kind": kind, **kw}
        out["items"].append(it)
        return it

    if out["info"].get("status") == "ok":
        # 2. metadata files
        files = list(plan.get("files") or [])
        for p in files:
            if p not in inv:
                item(p, "file", status="error", error="http_404", detail="not in the inventory at the pinned commit")
                continue
            if inv[p]["size"] > hfc_http.MAX_SMALL_FILE:
                item(p, "file", status="error", error="too_large", detail=f"{inv[p]['size']} bytes", file_size=inv[p]["size"])
                continue
            try:
                b = client.get_small(repo, rev, p, inv[p]["size"])
                dst = tmp / "f" / p
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(b)
                item(p, "file", status="ok", store=f"f/{p}", bytes=len(b), sha256=hashlib.sha256(b).hexdigest(), file_size=inv[p]["size"])
            except FetchError as e:
                item(p, "file", status="error", error=e.kind, detail=e.detail[:300])
        # 3. safetensors headers
        st = list(plan.get("st_headers") or [])
        ix = plan.get("st_from_index")
        if ix:
            d = ix.rsplit("/", 1)[0] if "/" in ix else ""
            try:
                wm = json.loads((tmp / "f" / ix).read_bytes()).get("weight_map") or {}
                shards = sorted(set(v for v in wm.values() if isinstance(v, str)))
                st = [f"{d}/{s}" if d else s for s in shards]
            except Exception:  # noqa: BLE001 — the census records the index's own failure
                st = []
        for p in st[:256]:
            if p not in inv:
                continue  # the census's WEIGHTS_INCOMPLETE: named and absent
            try:
                hb, size, nreq = st_header(client, repo, rev, p)
                name = f"h/{p}.hdr.gz"
                dst = tmp / name
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(gzip.compress(hb, compresslevel=6))
                item(p, "st_header", status="ok", store=name, bytes=len(hb), sha256=hashlib.sha256(hb).hexdigest(), file_size=inv[p]["size"], header_len=len(hb))
            except FetchError as e:
                item(p, "st_header", status="error", error=e.kind, detail=e.detail[:300], file_size=inv[p]["size"])
        if len(st) > 256:
            item("(shards)", "st_header", status="error", error="too_many_shards", detail=f"{len(st)} shards; the census reads at most 256")
        # 4. GGUF headers
        for p in plan.get("gguf_headers") or []:
            if p not in inv:
                item(p, "gguf_header", status="error", error="http_404", detail="not in the inventory")
                continue
            try:
                raw, comp, size, nreq, info = gguf_header(client, repo, rev, p)
                name = f"h/{p}.hdr.gz"
                dst = tmp / name
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_bytes(gzip.compress(comp, compresslevel=6))
                item(
                    p,
                    "gguf_header",
                    status="ok",
                    store=name,
                    bytes=len(raw),
                    sha256=hashlib.sha256(raw).hexdigest(),
                    file_size=inv[p]["size"],
                    header_len=info["header_len"],
                    detail=f"v{info['version']} {info['tensors']} tensors {info['kv']} keys {nreq} requests",
                )
            except FetchError as e:
                item(p, "gguf_header", status="error", error=e.kind, detail=e.detail[:300], file_size=inv[p]["size"])
        # 5. An adapter's base, at the commit the snapshot pinned: its inventory, configuration, index and shard headers (under base/).
        b = plan.get("base")
        if b:
            out["base"] = fetch_base(client, tmp, b["repo"], b["sha"], item)
    out["finished_at"] = now()
    out["bytes_in"] = client.bytes_in - req0
    (tmp / "listing.json").write_text(json.dumps(l, separators=(",", ":")))
    (tmp / "fetch.json").write_text(json.dumps(out, indent=0))
    final.parent.mkdir(parents=True, exist_ok=True)
    shutil.rmtree(final, ignore_errors=True)
    os.replace(tmp, final)
    return {"repo": repo, "info": out["info"].get("status"), "items": len(out["items"]), "errors": sum(1 for i in out["items"] if i["status"] != "ok")}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--snapshot", required=True)
    ap.add_argument("--sample", required=True)
    ap.add_argument("--workers", type=int, default=6)
    ap.add_argument("--api-share", type=float, default=0.8)
    ap.add_argument("--resolver-share", type=float, default=0.6)
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--disk-floor-gb", type=float, default=15.0)
    ap.add_argument("--budget-gb", type=float, default=5.0, help="stop when the store exceeds this")
    a = ap.parse_args()
    snap = Path(a.snapshot).expanduser()
    root = snap / "repos"
    root.mkdir(exist_ok=True)
    recs = [json.loads(x) for x in open(a.sample)]
    todo = [r for r in recs if not (repo_dir(root, r["listing"]["id"]) / "fetch.json").exists()]
    if a.limit:
        todo = todo[: a.limit]
    print(f"{len(recs)} in the sample, {len(todo)} to fetch", flush=True)
    client = hfc_http.Client(api_share=a.api_share, resolver_share=a.resolver_share)
    log = open(snap / "fetch.log.jsonl", "a")
    lock = threading.Lock()
    stop = threading.Event()
    done = 0
    t0 = time.time()

    def store_bytes() -> int:
        tot = 0
        for dp, _, fs in os.walk(root):
            for f in fs:
                try:
                    tot += os.lstat(os.path.join(dp, f)).st_blocks * 512
                except OSError:
                    pass
        return tot

    def work(r):
        if stop.is_set():
            return None
        try:
            return fetch_repo(client, root, r)
        except Exception as e:  # noqa: BLE001 — a bug is logged and the run continues; the repository stays unfetched
            return {"repo": r["listing"]["id"], "crash": f"{type(e).__name__}: {e}"}

    try:
        with cf.ThreadPoolExecutor(max_workers=a.workers) as ex:
            for res in ex.map(work, todo):
                if res is None:
                    continue
                with lock:
                    done += 1
                    log.write(json.dumps({"at": now(), **res}) + "\n")
                    log.flush()
                    if done % 50 == 0:
                        st = os.statvfs(str(snap))
                        free = st.f_bavail * st.f_frsize / 2**30
                        used = store_bytes() / 2**30
                        el = time.time() - t0
                        print(
                            f"{dt.datetime.now().strftime('%H:%M:%S')} {done}/{len(todo)} {done / el * 3600:.0f}/h "
                            f"api={client.api.requests} res={client.resolvers.requests} cdn={client.cdn.requests} "
                            f"in={client.bytes_in / 2**20:.0f}MiB store={used:.2f}GiB free={free:.0f}GiB",
                            flush=True,
                        )
                        if free < a.disk_floor_gb or used > a.budget_gb:
                            print(f"stopping: free {free:.1f} GiB, store {used:.2f} GiB", flush=True)
                            stop.set()
    finally:
        log.close()
        client.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
