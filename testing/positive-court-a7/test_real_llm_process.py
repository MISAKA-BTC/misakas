"""Pure/mock driver guards; no model binary or Rust/build/cache access."""
import argparse
from contextlib import contextmanager
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
import runpy
from unittest.mock import patch

def run_guards(driver):
    previous = sys.path[:]
    try:
        sys.path.insert(0, str(driver.parent))
        spec = importlib.util.spec_from_file_location('real_llm_process_under_test', driver)
        m = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(m)
    finally:
        sys.path[:] = previous
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
    failure = dict(good, status='FAILED_OR_RESOURCE_LIMIT', phase='quote', cpu_execution_started=False,
                   error='RESOURCE_LIMIT: logical upper exceeds explicit cap',
                   quote={'external_retained_bytes': 1024, 'simultaneous_operation_upper': 5000},
                   caps=dict(model['caps'], max_requested_bytes=4000))
    diagnostic = m.runner_diagnostic(failure)
    check('resource_failure_details_are_preserved', diagnostic['classification'] == 'RESOURCE_LIMIT_REPORTED' and
          diagnostic['reported_error'] == failure['error'] and diagnostic['reported_phase'] == 'quote' and
          diagnostic['reported_cpu_execution_started'] is False)
    check('logical_quote_excess_is_not_rss', diagnostic['logical_operation_exceeds_reported_cap'] is True and
          diagnostic['logical_operation_excess_bytes'] == 1000)
    check('resource_failure_stays_rejected', 'RUNNER_REPORTED_RESOURCE_LIMIT' in m.result_errors(failure, model, 'quote-only'))
    check('error_cannot_hide_behind_success_status', 'RUNNER_REPORTED_ERROR' in
          m.result_errors(dict(good, error='failure with a success status'), model, 'quote-only'))
    check('missing_error_is_not_invented_as_resource_limit',
          m.runner_diagnostic(dict(good, status='FAILED_OR_RESOURCE_LIMIT'))['classification'] == 'NO_ERROR_REPORTED')
    check('boolean_is_not_a_byte_bound', m.runner_diagnostic(dict(failure, quote={'simultaneous_operation_upper': True}))
          ['logical_operation_excess_bytes'] is None)


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

    return {'status': 'PASS_PURE_MOCK_GUARDS_ONLY', 'passed': len(passed),
                      'passed_names': passed, 'model_processes_executed': 0,
                      'rust_builds_executed': 0, 'resource_data': 'synthetic parser fixture, not a measurement'}


class DriverGuards(unittest.TestCase):
    def test_original_driver_guards(self):
        report = run_guards(Path(__file__).with_name('real_llm_process.py'))
        self.assertEqual(report['status'], 'PASS_PURE_MOCK_GUARDS_ONLY')
        self.assertEqual(report['passed'], 18)
        self.assertEqual(len(set(report['passed_names'])), 18)
        self.assertEqual(report['model_processes_executed'], 0)
        self.assertEqual(report['rust_builds_executed'], 0)

    def test_import_defers_arguments_driver_loading_and_guard_execution(self):
        # unittest discovery imports this file with its own argv. Importing it
        # must neither parse those arguments nor start the guard fixtures.
        with patch.object(sys, 'argv', ['unittest', 'discover', '-v']), \
             patch.object(argparse.ArgumentParser, 'parse_args', side_effect=AssertionError('parsed during import')), \
             patch.object(tempfile, 'TemporaryDirectory', side_effect=AssertionError('fixture ran during import')):
            namespace = runpy.run_path(str(Path(__file__).resolve()))
        self.assertTrue(callable(namespace['run_guards']))
        self.assertNotIn('m', namespace)
        self.assertNotIn('passed', namespace)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--driver', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(run_guards(args.driver.resolve()), indent=2))


if __name__ == '__main__':
    sys.dont_write_bytecode = True
    main()
