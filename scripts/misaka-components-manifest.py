#!/usr/bin/env python3
"""**A manifest row is a pointer with a digest, and nothing is trusted from a pointer alone.**

ADR-0096 Decision 10: every component a person needs — the node, the CLI, the family workers,
the gateway, the rail, the class artifacts — comes from ONE manifest (`components.json`, schema
`misaka/components/v1`) that both repositories publish and both check. This script is the node
repository's writer of its half and the checker of any manifest; `docs/components-manifest.md`
is the schema it enforces. It is standard-library Python 3 only, because it runs inside the
release workflow on three runners and on an operator's machine, and a dependency it had to fetch
would be one more thing nobody verified.

Three modes:

    # write: hash the binaries a release job produced, refuse any file whose name it cannot
    # classify, merge artifact rows from a file, validate, write sorted by id
    python3 scripts/misaka-components-manifest.py \
        --release <tag> --network <name> --platform <triple> \
        --url-base https://github.com/<owner>/<repo>/releases/download/<tag> \
        --out components-<triple>.json [--archive bin/<release>.zip] [--artifacts artifacts.json] \
        bin/kaspad bin/misaka ...

    # check: every row's file in <dir> hashes to its sha256 and has its size; exit non-zero
    # naming the first mismatch
    python3 scripts/misaka-components-manifest.py --check components-<triple>.json <dir>

    # validate the structure of a manifest someone else wrote (the Studio's half, a hand edit)
    python3 scripts/misaka-components-manifest.py --validate components.json

    # prove the writer and the checker agree, from temp files, with negative controls
    python3 scripts/misaka-components-manifest.py --self-test

Why `--archive`: the deploy workflow uploads one zip per platform, not loose binaries. A row whose
`url` named `<base>/kaspad` would be a pointer at nothing. With `--archive` the row's `url` is the
zip the release actually publishes, `member` is the path inside it, `archive_sha256`/`archive_size`
let a downloader verify the transport, and `sha256`/`size` stay the component's OWN bytes — the
writer opens the zip and refuses if the member's bytes are not the file it was handed.

Exit 0: written / verified / valid. Exit 1: a mismatch or a schema violation, named. Exit 2: usage,
a file this writer refuses by name, an unreadable input.
"""

import argparse
import hashlib
import json
import os
import re
import sys
import tempfile
import zipfile

SCHEMA = "misaka/components/v1"

KINDS = ("node", "cli", "worker", "gateway", "rail", "engine", "artifact", "tokenizer-table", "runtime", "shell")
# Rows of these kinds are files of no platform: `platform` MUST be `any`. Every other kind MUST
# name a Rust target triple — a binary for no platform is a row nobody can install.
PLATFORM_ANY_KINDS = frozenset(("artifact", "tokenizer-table"))

TOP_REQUIRED = frozenset(("schema", "release", "network", "components"))
TOP_OPTIONAL = frozenset(("node_manifest",))
ROW_REQUIRED = frozenset(("id", "kind", "version", "platform", "url", "sha256", "size", "requires"))
ROW_OPTIONAL = frozenset(
    (
        "member",
        "archive_sha256",
        "archive_size",
        "class_id",
        "artifact_root",
        "tokenizer_commitment",
        "model_id",
        "convert_command",
        "notes",
    )
)
KIND_REQUIRED = {
    "artifact": frozenset(("class_id", "artifact_root")),
    "tokenizer-table": frozenset(("tokenizer_commitment",)),
}

ID_RE = re.compile(r"^[a-z0-9][a-z0-9.-]*$")
HEX64_RE = re.compile(r"^[0-9a-f]{64}$")
HEX128_RE = re.compile(r"^[0-9a-f]{128}$")
# aarch64-apple-darwin, x86_64-unknown-linux-musl, x86_64-pc-windows-msvc, ...
TRIPLE_RE = re.compile(r"^[a-z0-9_]+-[a-z0-9_.-]+$")
URL_SCHEMES = ("https://", "hf://")

# The only names this writer classifies. A file not in this table is refused BY NAME rather than
# written under a guessed kind: the release also builds rothschild, kaspa-wallet, the PQ tools and
# the stratum bridge, and none of them is a component the Studio spawns.
KIND_BY_NAME = (
    (re.compile(r"^kaspad$"), "node"),
    (re.compile(r"^misaka$"), "cli"),
    (re.compile(r"^palw-[a-z0-9]+-fp-worker$"), "worker"),
    (re.compile(r"^misaka-palw-gateway$"), "gateway"),
    (re.compile(r"^misaka-palw-fp-rail$"), "rail"),
)

CHUNK = 1 << 20


class Refusal(Exception):
    """An input this writer will not turn into a row. Exit 2, named."""


def _basename(name):
    # Zip members may carry either separator (PowerShell's Compress-Archive has written
    # backslashes); a basename must not depend on which.
    return re.split(r"[\\/]", name)[-1]


def component_id_of(path, platform):
    """The row id a file name denotes: the basename without `.exe` and without a `-<platform>` tail."""
    name = _basename(path)
    if name.lower().endswith(".exe"):
        name = name[:-4]
    tail = "-" + platform
    if platform != "any" and name.endswith(tail) and len(name) > len(tail):
        name = name[: -len(tail)]
    return name


def kind_of(component_id):
    for pattern, kind in KIND_BY_NAME:
        if pattern.match(component_id):
            return kind
    return None


def sha256_of_file(path):
    h = hashlib.sha256()
    size = 0
    with open(path, "rb") as f:
        while True:
            block = f.read(CHUNK)
            if not block:
                break
            h.update(block)
            size += len(block)
    return h.hexdigest(), size


def sha256_of_member(zf, member):
    h = hashlib.sha256()
    size = 0
    with zf.open(member) as f:
        while True:
            block = f.read(CHUNK)
            if not block:
                break
            h.update(block)
            size += len(block)
    return h.hexdigest(), size


# --------------------------------------------------------------------------------------------
# validation — the schema in docs/components-manifest.md, checked by hand, one message per fault
# --------------------------------------------------------------------------------------------


def _is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def validate_row(row, where, ids, has_node_manifest):
    """Every fault in one row, each naming `where` (e.g. `components[3]` or `artifacts.json[0]`)."""
    errs = []
    if not isinstance(row, dict):
        return ["%s: a row must be an object" % where]
    keys = set(row)
    missing = sorted(ROW_REQUIRED - keys)
    if missing:
        errs.append("%s: missing required key(s) %s" % (where, ", ".join(missing)))
    unknown = sorted(keys - ROW_REQUIRED - ROW_OPTIONAL)
    if unknown:
        errs.append("%s: unknown key(s) %s (a new key is a schema change, not a row)" % (where, ", ".join(unknown)))
    if missing:
        return errs

    rid = row["id"]
    tag = "%s (%s)" % (where, rid if isinstance(rid, str) else "?")
    if not isinstance(rid, str) or not ID_RE.match(rid):
        errs.append("%s: id must match %s" % (tag, ID_RE.pattern))
    kind = row["kind"]
    if kind not in KINDS:
        errs.append("%s: kind %r is not one of %s" % (tag, kind, "|".join(KINDS)))
    if not isinstance(row["version"], str) or not row["version"]:
        errs.append("%s: version must be a non-empty string" % tag)
    platform = row["platform"]
    if not isinstance(platform, str):
        errs.append("%s: platform must be a string" % tag)
    elif kind in PLATFORM_ANY_KINDS:
        if platform != "any":
            errs.append("%s: kind %s is a file of no platform; platform must be `any`, not %r" % (tag, kind, platform))
    elif platform == "any" or not TRIPLE_RE.match(platform):
        errs.append("%s: platform must be a Rust target triple, not %r" % (tag, platform))
    url = row["url"]
    if not isinstance(url, str) or not url.startswith(URL_SCHEMES) or url.endswith("/"):
        errs.append("%s: url must start with one of %s and name a file" % (tag, ", ".join(URL_SCHEMES)))
    if not isinstance(row["sha256"], str) or not HEX64_RE.match(row["sha256"]):
        errs.append("%s: sha256 must be 64 lowercase hex characters" % tag)
    if not _is_int(row["size"]) or row["size"] < 0:
        errs.append("%s: size must be a non-negative integer" % tag)
    reqs = row["requires"]
    if not isinstance(reqs, list) or not all(isinstance(r, str) for r in reqs):
        errs.append("%s: requires must be a list of ids" % tag)
    else:
        for r in reqs:
            if r == rid:
                errs.append("%s: requires itself" % tag)
            elif r not in ids and not has_node_manifest:
                errs.append("%s: requires %r, which is not a row of this manifest (and no node_manifest is named)" % (tag, r))
    for key in KIND_REQUIRED.get(kind, ()):
        if key not in row:
            errs.append("%s: kind %s requires %s" % (tag, kind, key))
    for key in ("class_id", "artifact_root", "tokenizer_commitment"):
        if key in row and (not isinstance(row[key], str) or not HEX128_RE.match(row[key])):
            errs.append("%s: %s must be 128 lowercase hex characters" % (tag, key))
    if "member" in row:
        if not isinstance(row["member"], str) or not row["member"]:
            errs.append("%s: member must be a non-empty path inside the archive" % tag)
        for key in ("archive_sha256", "archive_size"):
            if key not in row:
                errs.append("%s: member names an archive, so %s is required" % (tag, key))
        if "archive_sha256" in row and (not isinstance(row["archive_sha256"], str) or not HEX64_RE.match(row["archive_sha256"])):
            errs.append("%s: archive_sha256 must be 64 lowercase hex characters" % tag)
        if "archive_size" in row and (not _is_int(row["archive_size"]) or row["archive_size"] < 0):
            errs.append("%s: archive_size must be a non-negative integer" % tag)
    else:
        for key in ("archive_sha256", "archive_size"):
            if key in row:
                errs.append("%s: %s without member" % (tag, key))
    for key in ("model_id", "convert_command", "notes"):
        if key in row and not isinstance(row[key], str):
            errs.append("%s: %s must be a string" % (tag, key))
    return errs


def validate_manifest(m):
    """Every fault in a manifest, as strings. Empty means valid."""
    errs = []
    if not isinstance(m, dict):
        return ["the manifest must be a JSON object"]
    keys = set(m)
    missing = sorted(TOP_REQUIRED - keys)
    if missing:
        errs.append("missing top-level key(s) %s" % ", ".join(missing))
    unknown = sorted(keys - TOP_REQUIRED - TOP_OPTIONAL)
    if unknown:
        errs.append("unknown top-level key(s) %s" % ", ".join(unknown))
    if missing:
        return errs
    if m["schema"] != SCHEMA:
        errs.append("schema is %r, this checker knows %r" % (m["schema"], SCHEMA))
    for key in ("release", "network"):
        if not isinstance(m[key], str) or not m[key]:
            errs.append("%s must be a non-empty string" % key)
    has_node_manifest = "node_manifest" in m
    if has_node_manifest:
        nm = m["node_manifest"]
        if not isinstance(nm, dict) or set(nm) != {"url", "sha256"}:
            errs.append("node_manifest must be exactly {url, sha256}")
        else:
            if not isinstance(nm["url"], str) or not nm["url"].startswith("https://"):
                errs.append("node_manifest.url must be an https:// URL")
            if not isinstance(nm["sha256"], str) or not HEX64_RE.match(nm["sha256"]):
                errs.append("node_manifest.sha256 must be 64 lowercase hex characters")
    rows = m["components"]
    if not isinstance(rows, list) or not rows:
        errs.append("components must be a non-empty list")
        return errs
    ids = [r.get("id") for r in rows if isinstance(r, dict)]
    id_set = set(i for i in ids if isinstance(i, str))
    for i, row in enumerate(rows):
        errs.extend(validate_row(row, "components[%d]" % i, id_set, has_node_manifest))
    seen = set()
    for i in ids:
        if i in seen:
            errs.append("id %r appears twice" % i)
        seen.add(i)
    str_ids = [i for i in ids if isinstance(i, str)]
    if str_ids != sorted(str_ids):
        errs.append("components are not sorted by id (the canonical form is sorted; a reordered manifest is a different file with the same meaning)")
    return errs


# --------------------------------------------------------------------------------------------
# write
# --------------------------------------------------------------------------------------------


def load_artifact_rows(path):
    try:
        with open(path, "r", encoding="utf-8") as f:
            data = json.load(f)
    except (OSError, ValueError) as e:
        raise Refusal("cannot read artifacts file %s: %s" % (path, e))
    if isinstance(data, dict) and "components" in data:
        data = data["components"]
    if not isinstance(data, list):
        raise Refusal("%s: expected a list of rows or {\"components\": [...]}" % path)
    return data


def build_manifest(release, network, platform, url_base, files, archive=None, artifacts_path=None):
    if not url_base.startswith("https://"):
        raise Refusal("--url-base must be an https:// URL, got %r" % url_base)
    url_base = url_base.rstrip("/")
    if platform == "any" or not TRIPLE_RE.match(platform):
        raise Refusal("--platform must be a Rust target triple, got %r" % platform)
    if not files:
        raise Refusal("no files given: a manifest with no rows names nothing")

    zf = None
    members = {}
    archive_digest = None
    if archive is not None:
        try:
            zf = zipfile.ZipFile(archive)
        except (OSError, zipfile.BadZipFile) as e:
            raise Refusal("cannot open archive %s: %s" % (archive, e))
        for info in zf.infolist():
            if info.is_dir():
                continue
            base = _basename(info.filename)
            if base in members:
                raise Refusal("archive %s holds %r twice (%s and %s); a member must be unique by basename" % (archive, base, members[base], info.filename))
            members[base] = info.filename
        archive_digest = sha256_of_file(archive)

    rows = []
    for path in files:
        if not os.path.isfile(path):
            raise Refusal("not a file: %s" % path)
        cid = component_id_of(path, platform)
        kind = kind_of(cid)
        if kind is None:
            raise Refusal(
                "refused by name: %s (id %r) is not a component this writer classifies; the table is %s"
                % (path, cid, ", ".join(p.pattern for p, _ in KIND_BY_NAME))
            )
        digest, size = sha256_of_file(path)
        row = {
            "id": cid,
            "kind": kind,
            "version": release,
            "platform": platform,
            "sha256": digest,
            "size": size,
            "requires": [],
        }
        if zf is None:
            row["url"] = url_base + "/" + _basename(path)
        else:
            base = _basename(path)
            if base not in members:
                raise Refusal("archive %s has no member named %r, so a row for %s would point at nothing" % (archive, base, path))
            member = members[base]
            mdigest, msize = sha256_of_member(zf, member)
            if (mdigest, msize) != (digest, size):
                raise Refusal(
                    "archive member %s is not the file %s (member sha256 %s/%d bytes, file %s/%d bytes)"
                    % (member, path, mdigest, msize, digest, size)
                )
            row["url"] = url_base + "/" + _basename(archive)
            row["member"] = member
            row["archive_sha256"] = archive_digest[0]
            row["archive_size"] = archive_digest[1]
        rows.append(row)
    if zf is not None:
        zf.close()

    if artifacts_path is not None:
        extra = load_artifact_rows(artifacts_path)
        known = set(r["id"] for r in rows) | set(r.get("id") for r in extra if isinstance(r, dict))
        errs = []
        for i, row in enumerate(extra):
            errs.extend(validate_row(row, "%s[%d]" % (artifacts_path, i), known, False))
        if errs:
            raise Refusal("artifact rows refused:\n  " + "\n  ".join(errs))
        rows.extend(extra)

    rows.sort(key=lambda r: r["id"])
    manifest = {"schema": SCHEMA, "release": release, "network": network, "components": rows}
    errs = validate_manifest(manifest)
    if errs:
        raise Refusal("the manifest this writer built does not validate:\n  " + "\n  ".join(errs))
    return manifest


def dump_manifest(manifest):
    # Canonical form: sorted keys, two-space indent, one trailing newline. A second writer (the
    # Studio's release) that follows this form reproduces the same bytes for the same rows.
    return json.dumps(manifest, indent=2, sort_keys=True) + "\n"


# --------------------------------------------------------------------------------------------
# check
# --------------------------------------------------------------------------------------------


def _candidates(row, directory):
    if "member" in row:
        member = row["member"].replace("\\", "/")
        return [os.path.join(directory, *member.split("/")), os.path.join(directory, _basename(member))]
    return [os.path.join(directory, _basename(row["url"]))]


def check_manifest(manifest_path, directory):
    """(ok, lines). Stops at the FIRST mismatch and names it; rows of platform `any` whose file
    is not in the directory are skipped and listed, never counted as verified."""
    lines = []
    try:
        with open(manifest_path, "r", encoding="utf-8") as f:
            m = json.load(f)
    except (OSError, ValueError) as e:
        return False, ["cannot read manifest %s: %s" % (manifest_path, e)]
    errs = validate_manifest(m)
    if errs:
        return False, ["%s does not validate:" % manifest_path] + ["  " + e for e in errs]
    if not os.path.isdir(directory):
        return False, ["not a directory: %s" % directory]
    verified = 0
    skipped = []
    for row in m["components"]:
        found = None
        for cand in _candidates(row, directory):
            if os.path.isfile(cand):
                found = cand
                break
        if found is None:
            if row["platform"] == "any":
                skipped.append(row["id"])
                continue
            lines.append("MISMATCH %s: no file for it in %s (looked for %s)" % (row["id"], directory, ", ".join(_candidates(row, directory))))
            return False, lines
        digest, size = sha256_of_file(found)
        if digest != row["sha256"] or size != row["size"]:
            lines.append(
                "MISMATCH %s: %s is sha256 %s (%d bytes); the manifest says %s (%d bytes)"
                % (row["id"], found, digest, size, row["sha256"], row["size"])
            )
            return False, lines
        lines.append("ok %s: %s" % (row["id"], found))
        verified += 1
    if skipped:
        lines.append("skipped (platform any, not in %s): %s" % (directory, ", ".join(skipped)))
    if verified == 0:
        lines.append("CHECK FAILED: nothing verified — every row was skipped, which proves nothing")
        return False, lines
    lines.append("CHECK OK: %d verified, %d skipped" % (verified, len(skipped)))
    return True, lines


# --------------------------------------------------------------------------------------------
# self-test — the writer and the checker agree, and every refusal fires
# --------------------------------------------------------------------------------------------


def self_test():
    import random

    rng = random.Random(0x0096)
    checks = 0

    def expect(cond, what):
        nonlocal checks
        checks += 1
        if not cond:
            raise AssertionError("self-test: " + what)

    def expect_refusal(fn, what):
        nonlocal checks
        checks += 1
        try:
            fn()
        except Refusal:
            return
        raise AssertionError("self-test: expected a refusal: " + what)

    with tempfile.TemporaryDirectory() as tmp:
        bindir = os.path.join(tmp, "bin")
        os.mkdir(bindir)
        names = ["kaspad", "misaka", "palw-a16-fp-worker", "misaka-palw-gateway", "misaka-palw-fp-rail"]
        for n in names:
            with open(os.path.join(bindir, n), "wb") as f:
                f.write(bytes(rng.getrandbits(8) for _ in range(rng.randint(1000, 5000))))
        art_bytes = bytes(rng.getrandbits(8) for _ in range(2048))
        art_digest = hashlib.sha256(art_bytes).hexdigest()
        with open(os.path.join(bindir, "qwen25-1.5b-a16.palwart"), "wb") as f:
            f.write(art_bytes)
        artifacts = os.path.join(tmp, "artifacts.json")
        with open(artifacts, "w", encoding="utf-8") as f:
            json.dump(
                [
                    {
                        "id": "qwen25-1.5b-a16",
                        "kind": "artifact",
                        "version": "5f",
                        "platform": "any",
                        "url": "hf://example/repo/palw-runtime/qwen25-1.5b-a16.palwart",
                        "sha256": art_digest,
                        "size": len(art_bytes),
                        "requires": ["palw-a16-fp-worker"],
                        "class_id": "42" * 64,
                        "artifact_root": "1a" * 64,
                    },
                    {
                        "id": "qwen25-tokenizer-table",
                        "kind": "tokenizer-table",
                        "version": "5f",
                        "platform": "any",
                        "url": "https://example.invalid/qwen25-tokenizer-table.bin",
                        "sha256": "00" * 32,
                        "size": 0,
                        "requires": [],
                        "tokenizer_commitment": "7b" * 64,
                    },
                ],
                f,
            )
        files = [os.path.join(bindir, n) for n in names]
        base = "https://example.invalid/releases/download/testnet-main-selftest"

        # 1. plain write, validate, check
        m = build_manifest("testnet-main-selftest", "testnet-11", "aarch64-apple-darwin", base, files, artifacts_path=artifacts)
        expect(validate_manifest(m) == [], "a written manifest validates")
        ids = [r["id"] for r in m["components"]]
        expect(ids == sorted(ids) and len(ids) == 7, "seven rows, sorted by id: %s" % ids)
        kinds = dict((r["id"], r["kind"]) for r in m["components"])
        expect(kinds["kaspad"] == "node" and kinds["misaka"] == "cli", "kaspad is node, misaka is cli")
        expect(kinds["palw-a16-fp-worker"] == "worker" and kinds["misaka-palw-gateway"] == "gateway" and kinds["misaka-palw-fp-rail"] == "rail", "worker/gateway/rail kinds")
        expect(all(r["url"] == base + "/" + r["id"] for r in m["components"] if r["platform"] != "any"), "binary urls are <base>/<filename>")
        out = os.path.join(tmp, "components.json")
        with open(out, "w", encoding="utf-8") as f:
            f.write(dump_manifest(m))
        with open(out, "r", encoding="utf-8") as f:
            expect(json.load(f) == m, "the file round-trips")
        ok, lines = check_manifest(out, bindir)
        expect(ok and lines[-1] == "CHECK OK: 6 verified, 1 skipped", "check passes over the directory: %s" % lines[-1])

        # 2. a corrupted binary is named as the first mismatch
        with open(os.path.join(bindir, "misaka"), "ab") as f:
            f.write(b"\x00")
        ok, lines = check_manifest(out, bindir)
        expect(not ok and lines[-1].startswith("MISMATCH misaka:"), "the corrupted file is named: %s" % lines[-1])
        with open(os.path.join(bindir, "misaka"), "rb+") as f:
            f.seek(-1, os.SEEK_END)
            f.truncate()
        ok, _ = check_manifest(out, bindir)
        expect(ok, "restored, the check passes again")
        os.remove(os.path.join(bindir, "kaspad"))
        ok, lines = check_manifest(out, bindir)
        expect(not ok and lines[-1].startswith("MISMATCH kaspad: no file"), "a missing platform file is a mismatch: %s" % lines[-1])
        with open(os.path.join(bindir, "kaspad"), "wb") as f:
            f.write(b"kaspad")  # a different kaspad: the digest, not the name, is the identity
        ok, lines = check_manifest(out, bindir)
        expect(not ok and lines[-1].startswith("MISMATCH kaspad:"), "a same-named different file is a mismatch")

        # 3. archive mode, both layouts the deploy workflow produces
        zpath = os.path.join(tmp, "rusty-kaspa-selftest-osx.zip")
        with zipfile.ZipFile(zpath, "w", zipfile.ZIP_DEFLATED) as zf:
            for n in ("misaka", "palw-a16-fp-worker"):
                zf.write(os.path.join(bindir, n), "bin/" + n)  # `zip -r x.zip ./bin/*`
        m2 = build_manifest("selftest", "testnet-11", "aarch64-apple-darwin", base, [os.path.join(bindir, "misaka"), os.path.join(bindir, "palw-a16-fp-worker")], archive=zpath)
        zdigest, zsize = sha256_of_file(zpath)
        for r in m2["components"]:
            expect(r["url"] == base + "/rusty-kaspa-selftest-osx.zip" and r["member"] == "bin/" + r["id"], "archive rows point at the zip and name the member")
            expect(r["archive_sha256"] == zdigest and r["archive_size"] == zsize, "archive digest recorded")
        out2 = os.path.join(tmp, "components-osx.json")
        with open(out2, "w", encoding="utf-8") as f:
            f.write(dump_manifest(m2))
        extracted = os.path.join(tmp, "extracted")
        with zipfile.ZipFile(zpath) as zf:
            zf.extractall(extracted)
        ok, lines = check_manifest(out2, extracted)
        expect(ok and lines[-1] == "CHECK OK: 2 verified, 0 skipped", "check resolves members under the extracted root")
        ok, lines = check_manifest(out2, os.path.join(extracted, "bin"))
        expect(ok, "check resolves members by basename when pointed at bin/ itself")
        # Windows: Compress-Archive bin/* puts kaspad.exe at the root, and the id drops the suffix
        wdir = os.path.join(tmp, "win")
        os.mkdir(wdir)
        with open(os.path.join(wdir, "kaspad.exe"), "wb") as f:
            f.write(b"MZ-not-really")
        wzip = os.path.join(tmp, "rusty-kaspa-selftest-win64.zip")
        with zipfile.ZipFile(wzip, "w") as zf:
            zf.write(os.path.join(wdir, "kaspad.exe"), "kaspad.exe")
        m3 = build_manifest("selftest", "testnet-11", "x86_64-pc-windows-msvc", base, [os.path.join(wdir, "kaspad.exe")], archive=wzip)
        expect(m3["components"][0]["id"] == "kaspad" and m3["components"][0]["member"] == "kaspad.exe", "kaspad.exe is the row kaspad, member kaspad.exe")
        expect(component_id_of("kaspad-x86_64-pc-windows-msvc.exe", "x86_64-pc-windows-msvc") == "kaspad", "a platform-suffixed name is still the id")
        # the archive member must BE the file: a stale zip is refused
        with open(os.path.join(wdir, "kaspad.exe"), "ab") as f:
            f.write(b"!")
        expect_refusal(lambda: build_manifest("selftest", "testnet-11", "x86_64-pc-windows-msvc", base, [os.path.join(wdir, "kaspad.exe")], archive=wzip), "a member whose bytes differ from the file")
        expect_refusal(lambda: build_manifest("selftest", "testnet-11", "x86_64-pc-windows-msvc", base, [os.path.join(bindir, "misaka")], archive=wzip), "a file the archive does not hold")

        # 4. refusals by name and by flag
        with open(os.path.join(bindir, "rothschild"), "wb") as f:
            f.write(b"x")
        expect_refusal(lambda: build_manifest("selftest", "testnet-11", "aarch64-apple-darwin", base, [os.path.join(bindir, "rothschild")]), "rothschild is refused by name")
        expect_refusal(lambda: build_manifest("selftest", "testnet-11", "any", base, files), "platform any for binaries")
        expect_refusal(lambda: build_manifest("selftest", "testnet-11", "aarch64-apple-darwin", "http://example.invalid", files), "a non-https url base")
        expect_refusal(lambda: build_manifest("selftest", "testnet-11", "aarch64-apple-darwin", base, []), "no files")

        # 5. the validator's rules, one violation each
        def broken(mutate):
            mm = json.loads(dump_manifest(m))
            mutate(mm)
            return validate_manifest(mm)

        def set_row(mm, rid, key, value):
            for r in mm["components"]:
                if r["id"] == rid:
                    r[key] = value

        expect(broken(lambda mm: set_row(mm, "kaspad", "platform", "any")) != [], "a binary of platform any is refused")
        expect(broken(lambda mm: set_row(mm, "qwen25-1.5b-a16", "platform", "aarch64-apple-darwin")) != [], "an artifact with a triple is refused")
        expect(broken(lambda mm: set_row(mm, "kaspad", "sha265", "00" * 32)) != [], "a misspelled key is refused")
        expect(broken(lambda mm: set_row(mm, "kaspad", "sha256", "zz" * 32)) != [], "a non-hex digest is refused")
        expect(broken(lambda mm: set_row(mm, "kaspad", "requires", ["misaka-studiod"])) != [], "requires of an unknown id is refused")
        expect(broken(lambda mm: (set_row(mm, "kaspad", "requires", ["misaka-studiod"]), mm.__setitem__("node_manifest", {"url": "https://example.invalid/components.json", "sha256": "00" * 32}))) == [], "…unless a node_manifest is named")
        expect(broken(lambda mm: mm["components"].reverse()) != [], "an unsorted manifest is refused")
        expect(broken(lambda mm: mm["components"].append(dict(mm["components"][0]))) != [], "a duplicate id is refused")
        expect(broken(lambda mm: mm.__setitem__("schema", "misaka/components/v2")) != [], "another schema is refused")
        expect(broken(lambda mm: [r.pop("class_id") for r in mm["components"] if r["kind"] == "artifact"]) != [], "an artifact without class_id is refused")
        expect(broken(lambda mm: set_row(mm, "kaspad", "member", "bin/kaspad")) != [], "member without archive digest is refused")
        expect(broken(lambda mm: set_row(mm, "kaspad", "archive_size", 3)) != [], "archive_size without member is refused")
        expect(broken(lambda mm: mm.__setitem__("extra", 1)) != [], "an unknown top-level key is refused")
        expect(broken(lambda mm: mm.__setitem__("node_manifest", {"url": "https://x/", "sha256": "0"})) != [], "a malformed node_manifest is refused")

    print("SELF-TEST OK: %d checks (write, round-trip, check, first-mismatch naming, archive members on both layouts, refusal by name, %d validator rules)" % (checks, 14))
    return 0


# --------------------------------------------------------------------------------------------
# main
# --------------------------------------------------------------------------------------------


def main(argv):
    ap = argparse.ArgumentParser(
        prog="misaka-components-manifest.py",
        description="write, validate or check a misaka/components/v1 manifest (docs/components-manifest.md)",
    )
    ap.add_argument("--release", help="release tag; becomes every binary row's version")
    ap.add_argument("--network", help="the network this release is cut for, e.g. testnet-11")
    ap.add_argument("--platform", help="Rust target triple of the binaries, e.g. aarch64-apple-darwin")
    ap.add_argument("--url-base", help="where the release publishes its assets; a row's url is <base>/<filename>")
    ap.add_argument("--out", help="where to write the manifest")
    ap.add_argument("--archive", help="the zip the release publishes; rows then point at it and name their member")
    ap.add_argument("--artifacts", help="a JSON file of artifact / tokenizer-table rows to merge verbatim")
    ap.add_argument("--check", nargs=2, metavar=("MANIFEST", "DIR"), help="verify every row's file in DIR")
    ap.add_argument("--validate", metavar="MANIFEST", help="validate a manifest's structure only")
    ap.add_argument("--self-test", action="store_true", help="build a manifest from temp files and check it")
    ap.add_argument("files", nargs="*", help="binaries to hash (write mode)")
    args = ap.parse_args(argv)

    if args.self_test:
        try:
            return self_test()
        except AssertionError as e:
            print(str(e), file=sys.stderr)
            return 1

    if args.check:
        ok, lines = check_manifest(args.check[0], args.check[1])
        for line in lines:
            print(line)
        return 0 if ok else 1

    if args.validate:
        try:
            with open(args.validate, "r", encoding="utf-8") as f:
                m = json.load(f)
        except (OSError, ValueError) as e:
            print("cannot read %s: %s" % (args.validate, e), file=sys.stderr)
            return 2
        errs = validate_manifest(m)
        for e in errs:
            print(e)
        print("VALID %s: %d rows" % (args.validate, len(m["components"])) if not errs else "INVALID %s: %d fault(s)" % (args.validate, len(errs)))
        return 0 if not errs else 1

    for flag, value in (("--release", args.release), ("--network", args.network), ("--platform", args.platform), ("--url-base", args.url_base), ("--out", args.out)):
        if not value:
            ap.error("%s is required to write a manifest" % flag)
    try:
        m = build_manifest(args.release, args.network, args.platform, args.url_base, args.files, archive=args.archive, artifacts_path=args.artifacts)
    except Refusal as e:
        print("refused: %s" % e, file=sys.stderr)
        return 2
    text = dump_manifest(m)
    try:
        with open(args.out, "w", encoding="utf-8") as f:
            f.write(text)
    except OSError as e:
        print("cannot write %s: %s" % (args.out, e), file=sys.stderr)
        return 2
    digest = hashlib.sha256(text.encode("utf-8")).hexdigest()
    print("wrote %s: %d row(s) [%s] sha256 %s" % (args.out, len(m["components"]), ", ".join(r["id"] for r in m["components"]), digest))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
