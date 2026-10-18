#!/usr/bin/env python3
"""Emit a NEW RFC06 source pin; never refresh old pins or run Rust tests.

The source walk follows the existing A2U declaration manifest grammar. Its output
is an unexecuted source-generation input, not a codec/compatibility acceptance.
"""
from pathlib import Path
import re
import argparse

ROOT = Path(__file__).resolve().parents[2]
DIRS = [
    ("consensus/core/src", "consensus/core/src"), ("crypto/hashes/src", "crypto/hashes/src"), ("math/src", "math/src"),
    *[(f"misaka-palw-{name}/src", f"misaka/palw/{path}/src") for name, path in [
        ("tir", "tir/core"), ("tir-exec", "tir/exec"), ("tir-lower", "tir/lower"), ("tir-sketch", "tir/sketch"),
        ("gen", "gen"), ("panel", "panel"), ("challenge", "challenge"), ("kernel", "kernel")]],
]

def group_end(text, at):
    opened = text[at]
    closed = {"{": "}", "(": ")", "[": "]", "<": ">"}[opened]
    depth, i = 0, at
    while i < len(text):
        if text[i] == '"' and opened != "<":
            i += 1
            while i < len(text) and text[i] != '"':
                i += 2 if text[i] == "\\" else 1
        elif text[i] == "'" and opened != "<":
            close = None
            if text[i + 1:i + 2] == "\\":
                found = text.find("'", i + 3)
                if found >= 0: close = found
            elif text[i + 2:i + 3] == "'": close = i + 2
            if close is not None: i = close
        elif text[i] == opened: depth += 1
        elif text[i] == closed and not (closed == ">" and text[i - 1:i] == "-"):
            depth -= 1
            if depth == 0: return i + 1
        i += 1
    return len(text)

def strip_comments(text):
    out, i = [], 0
    while i < len(text):
        if text.startswith("//", i):
            end = text.find("\n", i)
            i = len(text) if end < 0 else end
        elif text.startswith("/*", i):
            end = text.find("*/", i + 2)
            i = len(text) if end < 0 else end + 2
        elif text[i] == '"':
            start = i
            i += 1
            while i < len(text) and text[i] != '"': i += 2 if text[i] == "\\" else 1
            i += i < len(text)
            out.append(text[start:i])
        elif text.startswith("'\"'", i): out.append("'\"'"); i += 3
        else: out.append(text[i]); i += 1
    return "".join(out)

def strip_tests(text):
    marker, i, out = "#[cfg(test)]", 0, []
    while (at := text.find(marker, i)) >= 0:
        out.append(text[i:at])
        j, depth = at + len(marker), 0
        while j < len(text):
            char = text[j]
            if char in "([": depth += 1
            elif char in ")]": depth = max(0, depth - 1)
            elif char == ";" and depth == 0: j += 1; break
            elif char == "{" and depth == 0: j = group_end(text, j); break
            elif char == '"':
                j += 1
                while j < len(text) and text[j] != '"': j += 2 if text[j] == "\\" else 1
            j += 1
        i = j
    out.append(text[i:])
    return "".join(out)

def fnv(text):
    value = 0xcbf29ce484222325
    for byte in text.encode(): value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)
    return value

def source_types():
    types, files = {}, []
    for key, directory in DIRS:
        base = ROOT / directory
        for path in base.rglob("*.rs"):
            if path.name == "tests.rs" or {"tests", "benches"} & set(path.relative_to(base).parts): continue
            files.append((key + "/" + path.relative_to(base).as_posix(), path))
    for relative, path in sorted(files):
        text = strip_tests(strip_comments(path.read_text()))
        def insert(name, declaration):
            key, suffix = relative + "::" + name, 2
            while key in types:
                key = relative + "::" + name + "#" + str(suffix); suffix += 1
            squashed = re.sub(r"\s", "", declaration)
            types[key] = (fnv(squashed), squashed)
        i = 0
        while (at := text.find("#[derive(", i)) >= 0:
            end = group_end(text, at + 1)
            i, borsh, attrs, start = end, "Borsh" in text[at:end], "", at
            while text[:start].rstrip().endswith("]"):
                before = text[:start].rstrip()
                closing, depth, opening = len(before) - 1, 0, None
                for k in range(closing, -1, -1):
                    if text[k] == "]": depth += 1
                    elif text[k] == "[":
                        depth -= 1
                        if depth <= 0: opening = k; break
                if opening is None or opening == 0 or text[opening - 1] != "#": break
                attr = text[opening - 1:closing + 1]
                if attr.startswith(("#[borsh", "#[repr")): attrs = attr + attrs
                start = opening - 1
            j = end
            while True:
                while j < len(text) and text[j].isspace(): j += 1
                if text[j:j + 1] != "#": break
                end = group_end(text, j + 1)
                attr = text[j:end]
                if attr.startswith(("#[borsh", "#[repr")): attrs += attr
                j = end
            if text.startswith("pub(", j): j = group_end(text, j + 3)
            elif text.startswith("pub ", j): j += 4
            while j < len(text) and text[j].isspace(): j += 1
            kind = "struct" if text.startswith("struct ", j) and borsh else "enum" if text.startswith("enum ", j) and (borsh or "#[repr(u" in attrs or "#[repr(i" in attrs) else None
            if kind is None: continue
            name_at = j + len(kind) + 1
            match = re.match(r"[A-Za-z_0-9]+", text[name_at:])
            if match is None: continue
            name = match[0]
            k = name_at + len(name)
            while k < len(text) and text[k] not in "{(;": k = group_end(text, k) if text[k] == "<" else k + 1
            if k >= len(text): end = len(text)
            elif text[k] == ";": end = k + 1
            elif text[k] == "(":
                end = group_end(text, k)
                semi = text.find(";", end)
                if semi >= 0: end = semi + 1
            else: end = group_end(text, k)
            insert(name, attrs + text[j:end]); i = end
        for match in re.finditer(r"BorshDeserialize for ([A-Za-z_0-9]+)", text):
            opening = text.find("{", match.end())
            if opening < 0: continue
            start = text.rfind("impl", 0, match.start())
            insert("impl BorshDeserialize for " + match[1], text[start:group_end(text, opening)])
    return types

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reader-router', action='store_true', help='emit only the additive tag244 declaration pin')
    args = parser.parse_args()
    name, tag = ('ReaderRouterDeclaredV2', 244) if args.reader_router else ('GdnStepAccusedV2', 243)
    types = source_types()
    key = "consensus/core/src/palw_state_v2.rs::PalwConsensusObjectV2"
    declaration = types.pop(key)[1]
    start = declaration.index(name + "{")
    end = group_end(declaration, start + len(name))
    comma = declaration.find(",", end)
    segment = declaration[start:comma]
    if not segment.endswith(f"={tag}"): raise RuntimeError(f"tag{tag} declaration not frozen")
    by_name = {}
    for key, (_, value) in sorted(types.items()):
        name = key.rsplit("::", 1)[-1].removeprefix("impl BorshDeserialize for ").split("#")[0]
        by_name.setdefault(name, []).append((key, value))
    seen = set(re.findall(r"[A-Za-z_0-9]+", segment))
    queue = list(seen)
    while queue:
        for _, value in by_name.get(queue.pop(), []):
            for name in re.findall(r"[A-Za-z_0-9]+", value):
                if name in by_name and name not in seen: seen.add(name); queue.append(name)
    pin = segment
    for name in sorted(seen):
        for key, _ in by_name.get(name, []): pin += f"|{key}={types[key][0]:016x}"
    print(f"({tag}, 0x{fnv(pin):016x}), // additive RFC06 carrier v2; source only")

if __name__ == "__main__": main()
