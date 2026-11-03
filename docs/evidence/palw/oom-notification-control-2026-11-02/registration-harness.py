#!/usr/bin/env python3
"""Compile/run the actual std-only registration state, with no dependencies."""
from pathlib import Path
import hashlib
import subprocess
import tempfile
root = Path(__file__).resolve().parents[4]
source = root / 'notify/src/registration_state.rs'
print('registration_source_sha256=' + hashlib.sha256(source.read_bytes()).hexdigest(), flush=True)
with tempfile.TemporaryDirectory(prefix='misaka-registration-control-') as work:
    work = Path(work)
    runner, binary = work / 'test.rs', work / 'test'
    runner.write_text('#[path = ' + repr(str(source)).replace("'", '"') + '] mod registration_state;\n')
    subprocess.run(['rustc', '--edition', '2024', '--test', str(runner), '-o', str(binary)], check=True)
    subprocess.run([str(binary), '--nocapture'], check=True)
