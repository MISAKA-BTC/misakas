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
SOURCE_MANIFEST_SCHEMA = 'misaka.hf-checkpoint-source.v1'


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


def expected_file_hashes(path: Path, repo: str, revision: str, plan: list[dict]) -> dict[str, str]:
    with path.open('rb') as source:
        raw = source.read(MAX_METADATA + 1)
    if len(raw) > MAX_METADATA:
        raise ValueError('expected source manifest exceeds ceiling')
    manifest = json.loads(raw)
    if (not isinstance(manifest, dict) or manifest.get('schema') != SOURCE_MANIFEST_SCHEMA
            or manifest.get('complete') is not True):
        raise ValueError('expected a complete checkpoint source manifest')
    if manifest.get('repository') != repo or manifest.get('revision') != revision:
        raise ValueError('expected source manifest repository or revision mismatch')
    if not isinstance(manifest.get('files'), list):
        raise ValueError('expected source manifest files must be a list')

    planned = {item['path']: item for item in plan}
    hashes = {}
    for item in manifest['files']:
        if not isinstance(item, dict) or not isinstance(item.get('path'), str):
            raise ValueError('invalid expected source file path')
        name = safe_name(item['path'])
        if name in hashes or name not in planned:
            raise ValueError(f'expected source manifest file set mismatch: {name}')
        if type(item.get('bytes')) is not int or item['bytes'] != planned[name]['bytes']:
            raise ValueError(f'expected source size mismatch: {name}')
        digest = item.get('sha256')
        if not isinstance(digest, str) or not re.fullmatch(r'[0-9a-f]{64}', digest):
            raise ValueError(f'invalid expected source SHA-256: {name}')
        advertised = planned[name]['lfs_sha256']
        if advertised is not None and digest != advertised:
            raise ValueError(f'expected source SHA-256 mismatch with Hub LFS metadata: {name}')
        hashes[name] = digest
    if hashes.keys() != planned.keys():
        raise ValueError('expected source manifest file set mismatch')
    return hashes


def fetch(repo: str, revision: str, out: Path, ceiling: int, *, require_public_ungated: bool = False,
          expected_source_manifest: Path | None = None) -> dict:
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repo):
        raise ValueError('expected an owner/repository identifier')
    if not re.fullmatch(r'[0-9a-f]{40}', revision):
        raise ValueError('revision must be a full immutable commit SHA')
    if out.exists():
        raise ValueError('output must be a fresh directory')
    api = f'https://huggingface.co/api/models/{repo}/revision/{revision}?blobs=true'
    with urllib.request.urlopen(api, timeout=60) as response:
        if require_public_ungated and response.geturl() != api:
            raise ValueError('public Hub metadata must come from the requested repository and revision URL')
        raw = response.read(MAX_METADATA + 1)
    if len(raw) > MAX_METADATA:
        raise ValueError('repository metadata exceeds ceiling')
    info = json.loads(raw)
    if require_public_ungated and (not isinstance(info, dict) or info.get('id') != repo
                                   or info.get('private') is not False or info.get('gated') is not False):
        raise ValueError('Hub metadata must identify the requested repository and explicitly report private=false and gated=false')
    files = plan_files(info, revision, ceiling)
    expected_hashes = (expected_file_hashes(expected_source_manifest, repo, revision, files)
                       if expected_source_manifest is not None else None)
    out.mkdir(parents=True)
    (out/'hub-info.json').write_bytes(raw)
    manifest = {'schema': SOURCE_MANIFEST_SCHEMA, 'repository': repo,
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
                digest = stream_file(response, output, item)
            if expected_hashes is not None and digest != expected_hashes[item['path']]:
                raise ValueError(f"expected source SHA-256 mismatch: {item['path']}")
            temporary.rename(dest)
            item['sha256'] = digest
        except Exception:
            temporary.unlink(missing_ok=True)
            raise
    manifest['complete'] = True
    complete_path = manifest_path.with_suffix('.json.partial')
    complete_path.write_text(json.dumps(manifest, indent=2)+'\n')
    complete_path.replace(manifest_path)
    return manifest


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--repo', required=True)
    p.add_argument('--revision', required=True)
    p.add_argument('--out', required=True, type=Path)
    p.add_argument('--max-bytes', type=int, default=2 << 30)
    p.add_argument('--require-public-ungated', action='store_true',
                   help='require explicit private=false and gated=false for the pinned repository')
    p.add_argument('--expected-source-manifest', type=Path,
                   help='require exact repository, commit, file paths, sizes and SHA-256 from a complete source manifest')
    args = p.parse_args()
    try:
        m = fetch(args.repo, args.revision, args.out, args.max_bytes,
                  require_public_ungated=args.require_public_ungated, expected_source_manifest=args.expected_source_manifest)
    except (ValueError, OSError) as error:
        p.exit(1, f'checkpoint fetch refused: {error}\n')
    print(f"verified {len(m['files'])} files at {m['repository']}@{m['revision']}")


if __name__ == '__main__':
    main()
