#!/usr/bin/env python3
"""Fetch a pinned safetensors checkpoint for the RFC-v2-0003 onboarding path.

Only data files are downloaded. No model Python or pickle code is executed.
The byte ceiling is checked against the complete plan before any weight transfer.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import urllib.parse
import urllib.request

METADATA = {'config.json', 'generation_config.json', 'tokenizer.json', 'tokenizer_config.json',
            'special_tokens_map.json', 'added_tokens.json', 'vocab.json', 'vocab.txt', 'merges.txt',
            'tokenizer.model', 'sentencepiece.bpe.model', 'README.md', 'LICENSE', 'LICENSE.txt'}
MAX_METADATA = 16 << 20


def safe_name(name: str) -> str:
    p = PurePosixPath(name)
    if not name or p.is_absolute() or '..' in p.parts or '\\' in name or str(p) != name:
        raise ValueError(f'unsafe checkpoint path: {name!r}')
    return name


def plan_files(info: dict, revision: str, ceiling: int) -> list[dict]:
    if not re.fullmatch(r'[0-9a-f]{40}', revision) or info.get('sha') != revision:
        raise ValueError('source must match an explicit immutable 40-character commit SHA')
    files = []
    for f in info.get('siblings', []):
        name = safe_name(f['rfilename'])
        if name not in METADATA and not name.endswith(('.safetensors', '.safetensors.index.json')):
            continue
        if type(f.get('size')) is not int or f['size'] < 0:
            raise ValueError(f'missing source byte length: {name}')
        if not name.endswith('.safetensors') and f['size'] > MAX_METADATA:
            raise ValueError(f'metadata exceeds {MAX_METADATA} bytes: {name}')
        lfs = f.get('lfs') or {}
        sha256, blob = lfs.get('sha256'), f.get('blobId')
        if sha256:
            if not re.fullmatch(r'[0-9a-f]{64}', sha256) or lfs.get('size') != f['size']:
                raise ValueError(f'invalid LFS identity: {name}')
        elif not isinstance(blob, str) or not re.fullmatch(r'[0-9a-f]{40}', blob):
            raise ValueError(f'missing git blob identity: {name}')
        files.append({'path': name, 'bytes': f['size'], 'lfs_sha256': sha256, 'git_blob_sha1': blob})
    if len({f['path'] for f in files}) != len(files):
        raise ValueError('duplicate checkpoint paths')
    if not any(f['path'] == 'config.json' for f in files) or not any(f['path'].endswith('.safetensors') for f in files):
        raise ValueError('checkpoint requires config.json and safetensors weights')
    total = sum(f['bytes'] for f in files)
    if total > ceiling:
        raise ValueError(f'checkpoint needs {total} bytes, exceeding the {ceiling}-byte ceiling')
    return sorted(files, key=lambda f: (f['path'].endswith('.safetensors'), f['path']))


def stream_file(response, output, item: dict) -> str:
    sha = hashlib.sha256()
    blob = hashlib.sha1(f"blob {item['bytes']}\0".encode())
    count = 0
    while chunk := response.read(min(1 << 20, item['bytes'] - count + 1)):
        count += len(chunk)
        if count > item['bytes']:
            raise ValueError(f"source exceeds declared length: {item['path']}")
        sha.update(chunk)
        blob.update(chunk)
        output.write(chunk)
    if count != item['bytes']:
        raise ValueError(f"truncated source: {item['path']}")
    if item['lfs_sha256']:
        if sha.hexdigest() != item['lfs_sha256']:
            raise ValueError(f"LFS SHA-256 mismatch: {item['path']}")
    elif blob.hexdigest() != item['git_blob_sha1']:
        raise ValueError(f"git blob identity mismatch: {item['path']}")
    return sha.hexdigest()


def fetch(repo: str, revision: str, out: Path, ceiling: int) -> dict:
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repo):
        raise ValueError('expected an owner/repository identifier')
    if not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise ValueError('revision must be a full immutable commit SHA')
    if out.exists():
        raise ValueError('output must be a fresh directory')
    api = f'https://huggingface.co/api/models/{repo}/revision/{revision}?blobs=true'
    with urllib.request.urlopen(api, timeout=60) as response:
        raw = response.read(MAX_METADATA + 1)
    if len(raw) > MAX_METADATA:
        raise ValueError('repository metadata exceeds ceiling')
    info = json.loads(raw)
    files = plan_files(info, revision, ceiling)
    out.mkdir(parents=True)
    (out/'hub-info.json').write_bytes(raw)
    manifest = {'schema': 'misaka.hf-checkpoint-source.v1', 'repository': repo,
                'revision': revision, 'api': api, 'complete': False, 'files': files}
    manifest_path = out/'source-manifest.json'
    manifest_path.write_text(json.dumps(manifest, indent=2)+'\n')
    for item in files:
        dest = out/item['path']
        dest.parent.mkdir(parents=True, exist_ok=True)
        temporary = dest.with_name(dest.name+'.partial')
        url = f"https://huggingface.co/{repo}/resolve/{revision}/" + urllib.parse.quote(item['path'], safe='/')
        print(f"fetch {item['path']} ({item['bytes']} bytes)", flush=True)
        try:
            with urllib.request.urlopen(url, timeout=120) as response, temporary.open('xb') as output:
                item['sha256'] = stream_file(response, output, item)
            temporary.rename(dest)
        except Exception:
            temporary.unlink(missing_ok=True)
            raise
    manifest['complete'] = True
    manifest_path.write_text(json.dumps(manifest, indent=2)+'\n')
    return manifest


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo', required=True)
    p.add_argument('--revision', required=True)
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--max-bytes', type=int, default=2 << 30)
    args = p.parse_args()
    try:
        m = fetch(args.repo, args.revision, args.out, args.max_bytes)
    except (ValueError, OSError) as error:
        p.exit(1, f'checkpoint fetch refused: {error}\n')
    print(f"verified {len(m['files'])} files at {m['repository']}@{m['revision']}")


if __name__ == '__main__':
    main()
