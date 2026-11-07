#!/usr/bin/env python3
"""Check that the RFC06 native wire/inventory generators are idempotent and that the committed outputs are their output.

Runs scripts/drills/gen_rfc06_native_wire.py (which also runs gen_rfc06_native_inventory.py) twice and compares every file
it owns byte for byte across the two runs; then reports any owned file that differs from the git index (i.e. a committed
output that is not the generator's own output). Executes no compilation. Exit status: 0 = idempotent and committed,
1 = the second run changed a file, 2 = committed outputs differ from the generator output.
"""
import hashlib
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CORE = ROOT / 'consensus/core/src/palw_native_resource_v1'
OWNED = sorted(
    [p for p in CORE.glob('*.rs') if p.name in {
        'generated.rs', 'kernel_ledger.rs', 'ledger_rows_generated.rs', 'wire_generated.rs', 'wire_budget.rs',
        'wire_kernel_aux.rs', 'wire_jobs.rs'}]
    + list((CORE / 'private').glob('*.rs'))
    + [ROOT / 'misaka/palw/kernel/src/native_inventory_ledger_v1.rs', ROOT / 'misaka/palw/kernel/src/flat_quota11/stage14/source26.rs']
)


def digest():
    return {p: hashlib.sha256(p.read_bytes()).hexdigest() for p in OWNED if p.exists()}


def run():
    subprocess.run([sys.executable, str(ROOT / 'scripts/drills/gen_rfc06_native_wire.py')], cwd=ROOT, check=True,
                   stdout=subprocess.DEVNULL)


def main():
    run()
    first = digest()
    run()
    second = digest()
    changed = [str(p.relative_to(ROOT)) for p in OWNED if first.get(p) != second.get(p)]
    if changed:
        print('NOT IDEMPOTENT:', *changed, sep='\n  ')
        return 1
    dirty = subprocess.run(['git', 'diff', '--name-only', '--', *[str(p.relative_to(ROOT)) for p in OWNED]], cwd=ROOT,
                           capture_output=True, text=True, check=True).stdout.split()
    untracked = subprocess.run(['git', 'ls-files', '--others', '--exclude-standard', '--', str(CORE.relative_to(ROOT))],
                               cwd=ROOT, capture_output=True, text=True, check=True).stdout.split()
    if dirty or untracked:
        print('COMMITTED OUTPUT DIFFERS FROM GENERATOR OUTPUT:', *(dirty + untracked), sep='\n  ')
        return 2
    print('idempotent; committed outputs are the generator output')
    return 0


if __name__ == '__main__':
    sys.exit(main())
