"""The census's one HTTP client (RFC-0002 §II.10.2): metadata and headers only, anonymous, polite.

Rules this client enforces, so no caller can break them by accident:

* **Anonymous.** No `Authorization` header is ever sent; `trust_env=False`, so neither a `.netrc` nor a proxy variable nor a
  stored Hugging Face token is read. (A token exists on this machine; the census does not use it.)
* **Metadata and headers only.** `get_api` reads the Hub API (`/api/...`); `get_small` reads a whole file only when the Hub says
  it is small (`MAX_SMALL_FILE`), never an LFS weight; `get_range` reads an explicit byte range and refuses a range that is not
  bounded by the caller. Nothing here downloads a weight file: the safetensors and GGUF readers ask for the 8-byte length, the
  JSON header, or the GGUF metadata and tensor infos, by exact ranges.
* **Rate limits.** The Hub answers with `ratelimit: "<bucket>";r=<remaining>;t=<seconds to reset>` and
  `ratelimit-policy: "fixed window";"<bucket>";q=<quota>;w=<window>`. Each bucket (`api`, `resolvers`) has a minimum spacing
  between requests (a fraction of the anonymous quota) and a floor: when the remaining count falls under it, the client sleeps
  until the window resets. A 429 sleeps for `Retry-After` (or the window); a 5xx or a timeout backs off exponentially.
* **A polite User-Agent** that names the tool and what it does.
"""

from __future__ import annotations

import random
import re
import threading
import time
from dataclasses import dataclass, field

import httpx

USER_AGENT = "misaka-palw-hf-census/0.1 (header-only metadata census; RFC-0002 II.10; no weight downloads)"
ENDPOINT = "https://huggingface.co"
# A file read whole must be at most this large (configs, indices, tokenizer metadata). Anything larger is refused by name.
MAX_SMALL_FILE = 16 << 20
# The most a header read may fetch (the user's cap): beyond it the header is refused as HEADER_TOO_LARGE.
MAX_HEADER_BYTES = 64 << 20

_RL = re.compile(r'"(?P<b>[^"]+)"\s*;\s*r=(?P<r>\d+)\s*;\s*t=(?P<t>\d+)')
_POL = re.compile(r'"(?P<b>[^"]+)"\s*;\s*q=(?P<q>\d+)\s*;\s*w=(?P<w>\d+)')


class FetchError(Exception):
    """A fetch that did not succeed: `kind` is a stable name (`http_404`, `http_401`, `gated`, `timeout`, `too_large`, ...)."""

    def __init__(self, kind: str, detail: str = "", status: int | None = None):
        super().__init__(f"{kind}: {detail}" if detail else kind)
        self.kind = kind
        self.detail = detail
        self.status = status


@dataclass
class Bucket:
    name: str
    min_interval: float  # seconds between two requests of this bucket (all threads)
    floor: int  # sleep until the reset when fewer than this many remain
    lock: threading.Lock = field(default_factory=threading.Lock)
    next_at: float = 0.0
    remaining: int | None = None
    reset_at: float = 0.0
    quota: int | None = None
    window: int | None = None
    requests: int = 0
    throttled_s: float = 0.0

    def acquire(self) -> None:
        with self.lock:
            now = time.monotonic()
            wait = max(0.0, self.next_at - now)
            if self.remaining is not None and self.remaining < self.floor and self.reset_at > now:
                wait = max(wait, self.reset_at - now + 1.0)
            if wait > 0:
                self.throttled_s += wait
                time.sleep(wait)
            self.next_at = time.monotonic() + self.min_interval
            self.requests += 1

    def observe(self, headers: httpx.Headers) -> None:
        rl = headers.get("ratelimit")
        pol = headers.get("ratelimit-policy")
        with self.lock:
            if rl and (m := _RL.search(rl)) and m.group("b") == self.name:
                self.remaining = int(m.group("r"))
                self.reset_at = time.monotonic() + int(m.group("t"))
            if pol and (m := _POL.search(pol)) and m.group("b") == self.name:
                self.quota = int(m.group("q"))
                self.window = int(m.group("w"))

    def hold_until_reset(self, seconds: float) -> None:
        with self.lock:
            self.next_at = max(self.next_at, time.monotonic() + seconds)


class Client:
    """One client per process. Thread-safe: the buckets are shared, each thread may hold its own connection pool slot."""

    def __init__(self, api_share: float = 0.5, resolver_share: float = 0.5, cdn_interval: float = 0.05):
        # The anonymous quotas measured on 2026-10-03: api 500 / 300 s, resolvers 3,000 / 300 s.
        self.api = Bucket("api", min_interval=300.0 / (500 * api_share), floor=40)
        self.resolvers = Bucket("resolvers", min_interval=300.0 / (3000 * resolver_share), floor=200)
        self.cdn = Bucket("cdn", min_interval=cdn_interval, floor=0)
        self.http = httpx.Client(
            headers={"User-Agent": USER_AGENT, "Accept-Encoding": "gzip"},
            timeout=httpx.Timeout(60.0, connect=20.0),
            follow_redirects=False,
            trust_env=False,
            limits=httpx.Limits(max_connections=16, max_keepalive_connections=8),
        )
        self.bytes_in = 0
        self._lock = threading.Lock()

    def close(self) -> None:
        self.http.close()

    # ---- the one request loop ------------------------------------------------------------------------------------------------
    def _request(self, bucket: Bucket, url: str, headers: dict | None = None, attempts: int = 6) -> httpx.Response:
        delay = 2.0
        last: Exception | None = None
        for i in range(attempts):
            bucket.acquire()
            try:
                r = self.http.get(url, headers=headers or {})
            except (httpx.TimeoutException, httpx.TransportError) as e:
                last = e
                time.sleep(delay + random.random())
                delay = min(delay * 2, 120)
                continue
            bucket.observe(r.headers)
            with self._lock:
                self.bytes_in += len(r.content)
            if r.status_code == 429:
                ra = r.headers.get("retry-after")
                wait = float(ra) if ra and ra.isdigit() else float(bucket.window or 300) / 2
                bucket.hold_until_reset(wait)
                time.sleep(wait)
                continue
            if r.status_code >= 500:
                last = FetchError(f"http_{r.status_code}", url, r.status_code)
                time.sleep(delay + random.random())
                delay = min(delay * 2, 120)
                continue
            return r
        if isinstance(last, FetchError):
            raise last
        raise FetchError("timeout", f"{url}: {last}")

    # ---- the API --------------------------------------------------------------------------------------------------------------
    def get_api(self, url: str) -> httpx.Response:
        """A Hub API GET (`/api/...`): the response, whatever its status below 500."""
        if not url.startswith(ENDPOINT + "/api/"):
            raise ValueError(f"not a Hub API URL: {url}")
        return self._request(self.api, url)

    # ---- files ----------------------------------------------------------------------------------------------------------------
    def resolve(self, repo: str, revision: str, path: str) -> tuple[int, httpx.Response]:
        """`/resolve/<sha>/<path>` without following the redirect: (status, response). A small (non-LFS) file answers 200 with
        its body; an LFS file answers 302 with `Location` (the CDN) and `X-Linked-Size`."""
        url = f"{ENDPOINT}/{repo}/resolve/{revision}/{quote_path(path)}"
        r = self._request(self.resolvers, url)
        return r.status_code, r

    def get_small(self, repo: str, revision: str, path: str, known_size: int, max_bytes: int = MAX_SMALL_FILE) -> bytes:
        """A small metadata file, whole, whose size the inventory states (`known_size`, at most `max_bytes`). The resolver answers
        200 with the body, or redirects: to the CDN for an LFS file (`X-Linked-Size`), or to `/api/resolve-cache/...` for a git file.
        Either way exactly `known_size` bytes are read by one bounded range."""
        if known_size > max_bytes:
            raise FetchError("too_large", f"{path}: {known_size} bytes", None)
        status, r = self.resolve(repo, revision, path)
        if status == 200:
            if len(r.content) > max_bytes:
                raise FetchError("too_large", f"{path}: {len(r.content)} bytes", 200)
            return r.content
        if status in (301, 302, 307, 308) and r.headers.get("location"):
            size = int(r.headers.get("x-linked-size") or known_size)
            if size > max_bytes:
                raise FetchError("too_large", f"{path}: {size} bytes", status)
            if size == 0:
                return b""
            return self.cdn_range(r.headers["location"], 0, size)
        raise classify(status, path, r)

    def cdn_range(self, location: str, start: int, end: int) -> bytes:
        """Bytes [start, end) of an object the resolver redirected to. The range is explicit and bounded, and the response is
        **streamed**: a server that ignores `Range` (a 200) or announces more bytes than were asked is cut off before its body is
        read, so a weight file can never be downloaded whole by accident."""
        if end <= start:
            return b""
        if end - start > MAX_HEADER_BYTES:
            raise FetchError("too_large", f"a range of {end - start} bytes is over the header cap")
        url = location if location.startswith("http") else ENDPOINT + location
        bucket = self.resolvers if url.startswith(ENDPOINT) else self.cdn
        want = end - start
        delay = 2.0
        for _ in range(6):
            bucket.acquire()
            try:
                with self.http.stream("GET", url, headers={"Range": f"bytes={start}-{end - 1}"}) as r:
                    bucket.observe(r.headers)
                    if r.status_code == 206:
                        cl = r.headers.get("content-length")
                        if cl is not None and int(cl) > want:
                            raise FetchError("range_oversized", f"asked {want} bytes, the server announces {cl}", 206)
                        body = bytearray()
                        for chunk in r.iter_bytes():
                            body += chunk
                            if len(body) > want:
                                raise FetchError("range_oversized", f"asked {want} bytes, the server sent more", 206)
                        with self._lock:
                            self.bytes_in += len(body)
                        if len(body) != want:
                            raise FetchError("short_range", f"asked {want} bytes, got {len(body)}", 206)
                        return bytes(body)
                    if r.status_code == 200:
                        raise FetchError("range_ignored", "the server ignored Range (a whole-file download): not read", 200)
                    if r.status_code == 416:
                        raise FetchError("range_unsatisfiable", f"bytes {start}-{end - 1}", 416)
                    if r.status_code == 429 or r.status_code >= 500:
                        retry = r.status_code
                    else:
                        r.read()
                        raise classify(r.status_code, url, r)
            except (httpx.TimeoutException, httpx.TransportError):
                retry = "timeout"
            time.sleep(delay)
            delay = min(delay * 2, 120)
        raise FetchError("timeout" if retry == "timeout" else f"http_{retry}", url)


def classify(status: int, what: str, r: httpx.Response | None = None) -> FetchError:
    code = (r.headers.get("x-error-code") if r is not None else None) or ""
    msg = (r.headers.get("x-error-message") if r is not None else None) or ""
    if status == 401 or status == 403:
        if code == "GatedRepo" or "gated" in msg.lower() or "access to model" in msg.lower():
            return FetchError("gated", f"{what}: {msg}", status)
        return FetchError(f"http_{status}", f"{what}: {code} {msg}".strip(), status)
    if status == 404:
        return FetchError("http_404", f"{what}: {code} {msg}".strip(), status)
    return FetchError(f"http_{status}", f"{what}: {code} {msg}".strip(), status)


def quote_path(p: str) -> str:
    from urllib.parse import quote

    return quote(p, safe="/")
