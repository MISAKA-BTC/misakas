#!/usr/bin/env bash
# Build actual A5 node adapters while A0 retains workspace registration ownership.
set -euo pipefail
task_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
task_cargo_command="${POSITIVE_ACCOUNTING_NODE_COMMAND:-test}"
case "$task_cargo_command" in
    test|clippy|check) ;;
    *) echo "POSITIVE_ACCOUNTING_NODE_COMMAND must be test, clippy or check." >&2; exit 1 ;;
esac
task_python="${PYTHON3:-python3}"
if ! "$task_python" -c 'import tomllib' >/dev/null 2>&1; then
    for task_candidate in python3.14 python3.13 python3.12 python3.11; do
        if command -v "$task_candidate" >/dev/null 2>&1 && "$task_candidate" -c 'import tomllib' >/dev/null 2>&1; then
            task_python="$task_candidate"
            break
        fi
    done
fi
if ! "$task_python" -c 'import tomllib' >/dev/null 2>&1; then
    echo "Python 3.11+ (tomllib) is required to preserve the actual node dependency manifest." >&2
    exit 1
fi
task_scratch="$(mktemp -d "${TMPDIR:-/tmp}/misaka-positive-accounting-node.XXXXXX")"
trap 'rm -rf "$task_scratch"' EXIT
"$task_python" - "$task_root" "$task_scratch" <<'PY'
import json
import pathlib
import shutil
import sys
import tomllib

root, scratch = map(pathlib.Path, sys.argv[1:])
workspace = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']
source_roots = {
    'misaka-palw-positive-accounting': root / 'misaka/palw/positive-accounting',
    'misaka-palw-positive-accounting-node': root / 'misaka/palw/positive-accounting-node',
}
shadow_roots = {name: scratch / name for name in source_roots}

def value(obj):
    if isinstance(obj, bool):
        return 'true' if obj else 'false'
    if isinstance(obj, str):
        return json.dumps(obj)
    if isinstance(obj, (int, float)):
        return str(obj)
    if isinstance(obj, list):
        return '[' + ', '.join(value(item) for item in obj) + ']'
    if isinstance(obj, dict):
        return '{ ' + ', '.join(json.dumps(k) + ' = ' + value(v) for k, v in obj.items()) + ' }'
    raise TypeError(type(obj))

def dependency(name, spec, source):
    if isinstance(spec, str):
        return spec
    spec = dict(spec)
    if spec.pop('workspace', False):
        base = workspace['dependencies'][name]
        base = {'version': base} if isinstance(base, str) else dict(base)
        inherited_features = base.get('features', [])
        local_features = spec.get('features', [])
        base.update(spec)
        if inherited_features or local_features:
            base['features'] = list(dict.fromkeys(inherited_features + local_features))
        spec = base
        source = root
    package = spec.get('package', name)
    if package in shadow_roots:
        spec['path'] = str(shadow_roots[package])
    elif 'path' in spec:
        spec['path'] = str((source / spec['path']).resolve())
    return spec

for name, source in source_roots.items():
    original = tomllib.loads((source / 'Cargo.toml').read_text())
    target = shadow_roots[name]
    target.mkdir()
    package = {}
    for field, spec in original['package'].items():
        package[field] = workspace['package'][field] if isinstance(spec, dict) and spec.get('workspace') else spec
    lines = ['[package]'] + [f'{field} = {value(spec)}' for field, spec in package.items()]
    lib = dict(original.get('lib', {}))
    lib['path'] = str(source / lib.get('path', 'src/lib.rs'))
    lines += ['', '[lib]'] + [f'{field} = {value(spec)}' for field, spec in lib.items()]
    for section in ['dependencies', 'dev-dependencies', 'build-dependencies']:
        if section not in original:
            continue
        lines += ['', f'[{section}]']
        lines += [f'{dep} = {value(dependency(dep, spec, source))}' for dep, spec in original[section].items()]
    if 'features' in original:
        lines += ['', '[features]'] + [f'{feature} = {value(spec)}' for feature, spec in original['features'].items()]
    for platform, sections in original.get('target', {}).items():
        for section, deps in sections.items():
            lines += ['', f'[target.{json.dumps(platform)}.{section}]']
            lines += [f'{dep} = {value(dependency(dep, spec, source))}' for dep, spec in deps.items()]
    for test in sorted((source / 'tests').glob('*.rs')):
        lines += ['', '[[test]]', f'name = {value(test.stem)}', f'path = {value(str(test))}']
    (target / 'Cargo.toml').write_text('\n'.join(lines) + '\n')

(scratch / 'Cargo.toml').write_text('[workspace]\nresolver = "2"\nmembers = ' + value(list(source_roots)) + '\n')
shutil.copyfile(root / 'Cargo.lock', scratch / 'Cargo.lock')
PY
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${TMPDIR:-/tmp}/misaka-positive-accounting-node-target}"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
cargo "$task_cargo_command" --offline --manifest-path "$task_scratch/Cargo.toml" -p misaka-palw-positive-accounting-node "$@"
