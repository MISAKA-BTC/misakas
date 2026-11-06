#!/usr/bin/env bash
# Standalone A5 verification without editing A0-owned workspace registration.
set -euo pipefail
task_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
task_contract="${POSITIVE_CONTRACT_DIR:-$task_root/misaka/palw/positive-contract}"
if [[ ! -f "$task_contract/Cargo.toml" ]]; then
    echo "A0 positive-contract is required; import the common-contract commit first." >&2
    exit 1
fi
task_scratch="$(mktemp -d "${TMPDIR:-/tmp}/misaka-positive-accounting.XXXXXX")"
trap 'rm -rf "$task_scratch"' EXIT
python3 - "$task_root" "$task_contract" "$task_scratch" <<'PY'
import json
import pathlib
import sys
import re
root, contract, scratch = map(pathlib.Path, sys.argv[1:])
workspace_package = (root / 'Cargo.toml').read_text().split('[workspace.package]', 1)[1].split('\n[', 1)[0]
package = dict(re.findall(r'^([\w-]+)\s*=\s*"([^"]+)"\s*$', workspace_package, re.M))
manifest = '\n'.join([
    '[package]', 'name = "misaka-palw-positive-accounting"',
    f'version = {json.dumps(package["version"])}',
    f'edition = {json.dumps(package["edition"])}',
    f'rust-version = {json.dumps(package["rust-version"])}',
    '[workspace]', '[lib]',
    f'path = {json.dumps(str(root / "misaka/palw/positive-accounting/src/lib.rs"))}',
    '[dependencies]',
    f'misaka-palw-positive-contract = {{ path = {json.dumps(str(contract))} }}',
    '',
])
for test in sorted((root / 'misaka/palw/positive-accounting/tests').glob('*.rs')):
    manifest += '\n'.join(['', '[[test]]', f'name = {json.dumps(test.stem)}',
                           f'path = {json.dumps(str(test))}', ''])
(scratch / 'Cargo.toml').write_text(manifest)
import shutil
shutil.copyfile(root / 'Cargo.lock', scratch / 'Cargo.lock')
PY
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${TMPDIR:-/tmp}/misaka-positive-accounting-a5-target}"
export CARGO_INCREMENTAL=0
cargo test --offline --manifest-path "$task_scratch/Cargo.toml" "$@"
