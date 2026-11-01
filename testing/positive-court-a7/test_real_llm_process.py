"""Pure/mock driver guards; no model binary or Rust/build/cache access."""
import argparse
from contextlib import contextmanager
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch

sys.dont_write_bytecode = True
parser = argparse.ArgumentParser()
parser.add_argument('--driver', type=Path, required=True)
args = parser.parse_args()
driver = args.driver.resolve()
sys.path.insert(0, str(driver.parent))
spec = importlib.util.spec_from_file_location('real_llm_process_under_test', driver)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
model = {'model': 'EleutherAI/pythia-70m', 'class_id': 'class', 'max_context': 2048,
         'artifact_sha256': 'a' * 64, 'prompt_sha256': 'b' * 64, 'prompt_tokens': 16,
         'decode_tokens': 1, 'caps': {key: 1 for key in m.CAPS}, 'external_retained_bytes': 1024}
good = {'schema': 'misaka.a7.real-llm-repro.v1', 'mode': 'quote-only', 'status': 'QUOTE_ONLY',
        'model': model['model'], 'class_id': 'class', 'original_max_context': 2048,
        'scope': 'SELF_CONSISTENCY_ONLY', 'native_v2_admission_executed': False,
        'credit_executed': False, 'independent_expected_executed': False,
        'artifact_sha256': model['artifact_sha256'], 'prompt_ids_sha256': model['prompt_sha256'],
        'prompt_tokens': 16, 'declared_prefill_tokens': 16, 'exact_decode_tokens': 1,
        'caps': model['caps'], 'quote': {'external_retained_bytes': 1024}}
passed = []


def check(name, condition):
    if not condition:
        raise AssertionError(name)
    passed.append(name)


check('exit_zero_error_json_is_not_success', m.result_errors({'error': 'failed'}, model, 'quote-only'))
check('complete_quote_contract_accepted', not m.result_errors(good, model, 'quote-only'))
check('original_context_cannot_shrink', m.result_errors(dict(good, original_max_context=1024), model, 'quote-only'))
check('decode_count_must_match', m.result_errors(dict(good, exact_decode_tokens=2), model, 'quote-only'))
check('prompt_hash_must_match', m.result_errors(dict(good, prompt_ids_sha256='c' * 64), model, 'quote-only'))
check('caps_must_match', m.result_errors(dict(good, caps=dict(model['caps'], max_work=2)), model, 'quote-only'))
check('external_owner_allowance_must_match', m.result_errors(dict(good, quote={'external_retained_bytes': 1}), model, 'quote-only'))
run = dict(good, mode='run', status='LOCAL_ROUNDTRIP_PASS', roundtrip_pass=True)
check('complete_run_contract_accepted', not m.result_errors(run, model, 'run'))
check('failed_roundtrip_cannot_pass', m.result_errors(dict(run, roundtrip_pass=False), model, 'run'))


@contextmanager
def late_signal():
    state = {'signal': None}
    yield state
    state['signal'] = 15


class Child:
    pid = 999999
    returncode = 0

    def wait(self, timeout=None):
        return 0


with tempfile.TemporaryDirectory(prefix='a7-driver-pure-mock-') as temporary:
    directory = Path(temporary)
    log = directory / 'time-l.log'
    log.write_text(' 1.25 real 0.90 user 0.05 sys\n 1048576 maximum resident set size\n'
                   ' 3 block input operations\n 4 block output operations\n')
    data = m.parse_darwin_time(log)
    check('darwin_rss_bytes_and_io_operations', data['peak_rss_bytes'] == 1048576 and
          data['block_input_operations'] == 3 and data['status'] == 'RECORDED')
    interrupted = False
    with patch.object(m, 'cancellation_signals', late_signal), \
         patch.object(m.subprocess, 'Popen', return_value=Child()), \
         patch.object(m, 'active', return_value=[]), patch.object(m, 'members', return_value=[]):
        try:
            m.run_fresh(['NEVER_EXECUTED'], directory, {}, 1)
        except m.ProcessInterrupted as error:
            interrupted = error.signum == 15
    check('late_cancel_after_context_exit_is_not_lost', interrupted)
    log.write_text('{"value":NaN}')
    rejected = False
    try:
        m.read_json(log)
    except ValueError:
        rejected = True
    check('nonfinite_json_is_refused', rejected)

print(json.dumps({'status': 'PASS_PURE_MOCK_GUARDS_ONLY', 'passed': len(passed),
                  'passed_names': passed, 'model_processes_executed': 0,
                  'rust_builds_executed': 0, 'resource_data': 'synthetic parser fixture, not a measurement'}, indent=2))
