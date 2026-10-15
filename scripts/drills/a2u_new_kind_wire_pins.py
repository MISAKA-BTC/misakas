#!/usr/bin/env python3
"""Recompute PALW_A2_NEW_KIND_WIRE_V1 (the A-2 wire pins of post-int-12 lifecycle kinds) WITHOUT building.

A line-for-line port of the manifest reader in consensus/core/src/palw_lifecycle_objects_v2.rs (`a2u` test module):
`strip_comments`, `group_end`, `strip_cfg_test_items`, `wire_types_of`, `object_variant_decls` (with
`native_fp_metadata_wire_equivalent`), `item_of`, `reachable_from`, `fnv64` and `new_kind_wire_digests`.
It reads source text only and executes no Rust. The authority stays the Rust test
`every_landed_kind_is_pinned_to_its_wire_form`: if this port and the test ever disagree, the test wins and this port is wrong.

Usage: python3 -I scripts/drills/a2u_new_kind_wire_pins.py [--check]
Prints `(tag, 0x....),` for every row of PALW_LIFECYCLE_NEW_KINDS_V1 in table order, then compares with the pinned table.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
WIRE_DIRS = [
    ("consensus/core/src", "consensus/core/src"),
    ("crypto/hashes/src", "crypto/hashes/src"),
    ("math/src", "math/src"),
    ("misaka-palw-tir/src", "misaka/palw/tir/core/src"),
    ("misaka-palw-tir-exec/src", "misaka/palw/tir/exec/src"),
    ("misaka-palw-tir-lower/src", "misaka/palw/tir/lower/src"),
    ("misaka-palw-tir-sketch/src", "misaka/palw/tir/sketch/src"),
    ("misaka-palw-gen/src", "misaka/palw/gen/src"),
    ("misaka-palw-panel/src", "misaka/palw/panel/src"),
    ("misaka-palw-challenge/src", "misaka/palw/challenge/src"),
    ("misaka-palw-kernel/src", "misaka/palw/kernel/src"),
]
LIFECYCLE = ROOT / "consensus/core/src/palw_lifecycle_objects_v2.rs"


def fnv64(data: bytes) -> int:
    h = 0xCBF29CE484222325
    for x in data:
        h ^= x
        h = (h * 0x00000100000001B3) & 0xFFFFFFFFFFFFFFFF
    return h


def strip_comments(b: bytes) -> bytes:
    out = bytearray()
    i, n = 0, len(b)
    while i < n:
        if b.startswith(b"//", i):
            while i < n and b[i] != 0x0A:
                i += 1
        elif b.startswith(b"/*", i):
            i += 2
            while i < n and not b.startswith(b"*/", i):
                i += 1
            i += 2
        elif b[i] == 0x22:  # "
            out.append(b[i])
            i += 1
            while i < n and b[i] != 0x22:
                if b[i] == 0x5C:  # backslash
                    out.append(b[i])
                    i += 1
                if i < n:
                    out.append(b[i])
                    i += 1
            if i < n:
                out.append(b[i])
                i += 1
        elif b.startswith(b"'\"'", i):
            out.extend(b"'\"'")
            i += 3
        else:
            out.append(b[i])
            i += 1
    # Rust: String::from_utf8_lossy(&out) — the cuts are at ASCII bytes, so the bytes stay valid UTF-8.
    return bytes(out).decode("utf-8", "replace").encode("utf-8")


def group_end(b: bytes, open_: int) -> int:
    pairs = {0x7B: (0x7B, 0x7D), 0x28: (0x28, 0x29), 0x5B: (0x5B, 0x5D)}
    o, c = pairs.get(b[open_], (0x3C, 0x3E))
    depth = 0
    i, n = open_, len(b)
    while i < n:
        if b[i] == 0x22 and o != 0x3C:
            i += 1
            while i < n and b[i] != 0x22:
                i += 2 if b[i] == 0x5C else 1
        elif b[i] == 0x27 and o != 0x3C:
            close = None
            if i + 1 < n and b[i + 1] == 0x5C:
                rest = b[i + 3:] if i + 3 <= n else b""
                k = rest.find(b"'")
                if k >= 0:
                    close = i + 3 + k
            elif i + 2 < n and b[i + 2] == 0x27:
                close = i + 2
            if close is not None:
                i = close
        elif b[i] == o:
            depth += 1
        elif b[i] == c and not (c == 0x3E and i > 0 and b[i - 1] == 0x2D):
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def strip_cfg_test_items(b: bytes) -> bytes:
    attr = b"#[cfg(test)]"
    out = bytearray()
    i, n = 0, len(b)
    while True:
        at = b.find(attr, i)
        if at < 0:
            break
        out.extend(b[i:at])
        j = at + len(attr)
        depth = 0
        while True:
            if j >= n:
                end = n
                break
            x = b[j]
            if x in (0x28, 0x5B):
                depth += 1
            elif x in (0x29, 0x5D):
                depth = max(0, depth - 1)
            elif x == 0x3B and depth == 0:
                end = j + 1
                break
            elif x == 0x7B and depth == 0:
                end = group_end(b, j)
                break
            elif x == 0x22:
                j += 1
                while j < n and b[j] != 0x22:
                    j += 2 if b[j] == 0x5C else 1
            j += 1
        i = end
    out.extend(b[i:])
    return bytes(out)


def squash(s: bytes) -> str:
    return "".join(ch for ch in s.decode("utf-8", "replace") if not ch.isspace())


def ident_prefix(b: bytes, at: int) -> str:
    j = at
    while j < len(b) and (chr(b[j]).isalnum() and b[j] < 128 or b[j] == 0x5F):
        j += 1
    return b[at:j].decode()


def walk(d: Path, out: list):
    try:
        entries = list(d.iterdir())
    except OSError:
        return
    for p in entries:
        if p.is_dir():
            if p.name not in ("tests", "benches"):
                walk(p, out)
        elif p.suffix == ".rs" and p.name != "tests.rs":
            out.append(p)


def wire_types_of(root: Path) -> dict:
    files = []
    for key, d in WIRE_DIRS:
        base = root / d if (root / d).is_dir() else root / key
        found = []
        walk(base, found)
        for f in found:
            tail = f.relative_to(base).as_posix()
            files.append((f"{key}/{tail}", str(f)))
    files.sort()
    out = {}

    def insert(rel, name, decl):
        key = f"{rel}::{name}"
        n = 2
        while key in out:
            key = f"{rel}::{name}#{n}"
            n += 1
        out[key] = (fnv64(decl.encode("utf-8")), decl)

    for rel, file in files:
        raw = Path(file).read_bytes()
        text = strip_cfg_test_items(strip_comments(raw))
        b = text
        n = len(b)
        i = 0
        while True:
            at = b.find(b"#[derive(", i)
            if at < 0:
                break
            attr_end = group_end(b, at + 1)
            i = attr_end
            borsh = b"Borsh" in b[at:attr_end]
            attrs = b""
            start = at
            while True:
                before = b[:start].rstrip()
                if not before.endswith(b"]"):
                    break
                close = len(before) - 1
                depth = 0
                k = close + 1
                open_ = None
                while True:
                    if k == 0:
                        open_ = None
                        break
                    k -= 1
                    if b[k] == 0x5D:
                        depth += 1
                    elif b[k] == 0x5B:
                        depth -= 1
                        if depth <= 0:
                            open_ = k
                            break
                if open_ is not None and open_ > 0 and b[open_ - 1] == 0x23:
                    a = b[open_ - 1:close + 1]
                    if a.startswith(b"#[borsh") or a.startswith(b"#[repr"):
                        attrs = a + attrs
                    start = open_ - 1
                else:
                    break
            j = attr_end
            while True:
                while j < n and b[j] in b" \t\n\r\x0b\x0c":
                    j += 1
                if j < n and b[j] == 0x23:
                    end = group_end(b, j + 1)
                    a = b[j:end]
                    if a.startswith(b"#[borsh") or a.startswith(b"#[repr"):
                        attrs = attrs + a
                    j = end
                else:
                    break
            if b.startswith(b"pub(", j):
                j = group_end(b, j + 3)
            elif b.startswith(b"pub ", j):
                j += 4
            while j < n and b[j] in b" \t\n\r\x0b\x0c":
                j += 1
            if b.startswith(b"struct ", j) and borsh:
                kind = "struct"
            elif b.startswith(b"enum ", j) and (borsh or b"#[repr(u" in attrs or b"#[repr(i" in attrs):
                kind = "enum"
            else:
                continue
            name_at = j + len(kind) + 1
            name = ident_prefix(b, name_at)
            k = name_at + len(name)
            while k < n and b[k] not in (0x7B, 0x28, 0x3B):
                if b[k] == 0x3C:
                    k = group_end(b, k)
                else:
                    k += 1
            if k >= n:
                end = n
            elif b[k] == 0x3B:
                end = k + 1
            elif b[k] == 0x28:
                close = group_end(b, k)
                s = b.find(b";", close)
                end = close + (s - close + 1 if s >= 0 else 0)
            else:
                end = group_end(b, k)
            insert(rel, name, squash(attrs + b[j:end]))
            i = end
        i = 0
        needle = b"BorshDeserialize for "
        while True:
            at = b.find(needle, i)
            if at < 0:
                break
            name_at = at + len(needle)
            name = ident_prefix(b, name_at)
            open_ = b.find(b"{", name_at)
            if open_ < 0:
                open_ = n - 1
            end = group_end(b, open_)
            start = b.rfind(b"impl", 0, at)
            if start < 0:
                start = at
            insert(rel, f"impl BorshDeserialize for {name}", squash(b[start:end]))
            i = end
    return out


FP_FIELD = "#[borsh(skip)]public_job:Option<Box<crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>>,"


def object_variant_decls(types: dict) -> dict:
    _, decl = types["consensus/core/src/palw_state_v2.rs::PalwConsensusObjectV2"]
    b = decl.encode("utf-8")
    open_ = decl.find("PalwConsensusObjectV2{") + len("PalwConsensusObjectV2")
    close = group_end(b, open_) - 1
    out = {}
    depth, start = 0, open_ + 1
    for i in range(open_ + 1, close + 1):
        x = b[i]
        if x in (0x7B, 0x28, 0x5B, 0x3C) and i < close:
            depth += 1
        elif x in (0x7D, 0x29, 0x5D, 0x3E) and depth > 0:
            depth -= 1
        if (x == 0x2C and depth == 0) or i == close:
            segment = b[start:i]
            while segment.startswith(b"#["):
                segment = segment[group_end(segment, 1):]
            name = ident_prefix(segment, 0)
            if name:
                seg = segment.decode("utf-8")
                if name == "FreePromptCommitted":
                    assert seg.count("public_job:") == 1 and seg.count(FP_FIELD) == 1, "FreePromptCommitted field shape"
                    seg = seg.replace(FP_FIELD, "", 1)
                out[name] = seg
            start = i + 1
    return out


def item_of(key: str) -> str:
    item = key.rsplit("::", 1)[-1]
    if item.startswith("impl BorshDeserialize for "):
        item = item[len("impl BorshDeserialize for "):]
    return item.split("#", 1)[0]


def split_idents(s: str):
    return [t for t in re.split(r"[^A-Za-z0-9_]", s) if t]


def reachable_from(types: dict, roots) -> list:
    by_name = {}
    for key in sorted(types):
        by_name.setdefault(item_of(key), []).append(types[key][1])
    seen = set(roots)
    queue = sorted(seen)
    while queue:
        name = queue.pop()
        for decl in by_name.get(name, []):
            for ident in split_idents(decl):
                if ident in by_name and ident not in seen:
                    seen.add(ident)
                    queue.append(ident)
    return sorted(seen)


def new_kind_table():
    src = LIFECYCLE.read_text()
    body = src[src.index("pub const PALW_LIFECYCLE_NEW_KINDS_V1"):]
    body = body[:body.index("];")]
    return [(int(t), n) for t, n in re.findall(r'\(\s*(\d+)\s*,\s*"(\w+)"\s*,\s*PalwLifecycleKindFenceV1::\w+\s*\)', body)]


def pinned_table():
    src = LIFECYCLE.read_text()
    body = src[src.index("const PALW_A2_NEW_KIND_WIRE_V1"):]
    body = body[:body.index("];")]
    body = re.sub(r"//[^\n]*", "", body)
    return [(int(t), int(h, 16)) for t, h in re.findall(r"\(\s*(\d+)\s*,\s*0x([0-9a-fA-F]+)\s*\)", body)]


def main():
    types = wire_types_of(ROOT)
    variants = object_variant_decls(types)
    del types["consensus/core/src/palw_state_v2.rs::PalwConsensusObjectV2"]
    keys = sorted(types)
    now = []
    for tag, name in new_kind_table():
        segment = variants[name]
        text = segment
        for item in reachable_from(types, split_idents(segment)):
            for key in keys:
                if item_of(key) == item:
                    text += f"|{key}={types[key][0]:016x}"
        now.append((tag, fnv64(text.encode("utf-8"))))
    pinned = pinned_table()
    for tag, digest in now:
        print(f"    ({tag}, 0x{digest:016x}),")
    if "--check" in sys.argv:
        mism = [(t, d, p) for (t, d), (_, p) in zip(now, pinned) if d != p]
        print(f"# rows {len(now)} pinned {len(pinned)}; differing: {[(t, hex(d), hex(p)) for t, d, p in mism]}", file=sys.stderr)


if __name__ == "__main__":
    main()
