#!/usr/bin/env python3
"""Run the actual local readiness/publication decisions and tests, without Cargo.
Hash64 only is replaced with a tiny local value type. Consensus API/session,
bootstrap loading and service scheduling are not exercised by this harness.
"""
from pathlib import Path
import hashlib
import subprocess
import tempfile
root = Path(__file__).resolve().parents[4]
production = (root / 'kaspad/src/palw_producer.rs').read_text()
tests = (root / 'kaspad/src/palw_producer_context_tests.rs').read_text()
def function(text, name):
    start = text.index('fn ' + name + '(')
    body = text.index('{', start)
    depth, end = 1, body + 1
    while depth:
        depth += (text[end] == '{') - (text[end] == '}')
        end += 1
    return text[start:end]
source = '''#[derive(Clone, Copy, PartialEq, Eq)]
struct Hash64(u64);
impl Hash64 { fn from_u64_word(n:u64)->Self { Self(n) } }
'''
for name in ['palw_producer_view_guard_v1', 'palw_producer_epoch_guard_v1']:
    actual = function(production, name)
    print(name + '_sha256=' + hashlib.sha256(actual.encode()).hexdigest(), flush=True)
    source += actual + '\n'
source += '#[cfg(test)] mod tests {\n' + tests + '\n}'
with tempfile.TemporaryDirectory(prefix='misaka-producer-readiness-') as work:
    work = Path(work)
    src, binary = work / 'readiness.rs', work / 'readiness'
    src.write_text(source)
    subprocess.run(['rustc', '--edition', '2024', '--test', str(src), '-o', str(binary)], check=True)
    subprocess.run([str(binary), '--nocapture'], check=True)
