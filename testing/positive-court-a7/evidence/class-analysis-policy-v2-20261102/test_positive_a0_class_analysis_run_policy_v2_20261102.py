"""Mock-only policy tests. No model subprocess, Cargo, build or target read."""
import copy
from contextlib import contextmanager
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

WRAPPER = Path('/tmp/positive-a0-class-analysis-run-policy-v2-20261102.py')
spec = importlib.util.spec_from_file_location('positive_a0_policy_v2', WRAPPER)
policy = importlib.util.module_from_spec(spec)
with patch.object(subprocess, 'Popen', side_effect=AssertionError('import spawned a process')), \
     patch.object(subprocess, 'check_output', side_effect=AssertionError('import inspected Root')):
    spec.loader.exec_module(policy)


def sample_item(slug='pythia-70m'):
    row = next(x for x in policy.MODELS if x[0] == slug)
    return dict(zip(['slug', 'model', 'revision', 'original_max_context'], row))


def sample_report(item):
    return dict(schema='misaka.a7.real-llm-class-analysis.v1', mode='class-only-analysis',
                scope='CLASS_ONLY_DATA_ANALYSIS', status='CLASS_ONLY_ANALYSIS_COMPLETE', phase='complete',
                model=item['model'], revision=item['revision'], original_max_context=item['original_max_context'],
                cpu_executed=False, native_v2_admission_executed=False, credit_executed=False,
                independent_expected_executed=False, registered_or_rooted_state_installed=False,
                protocol_params_modified=False, economic_or_authentication_fields_synthesized=False)


class Child:
    pid = 999991

    def __init__(self, polls=None, on_poll=None):
        self.polls = list(polls or [0])
        self.returncode = None
        self.on_poll = on_poll

    def poll(self):
        value = self.polls.pop(0) if self.polls else 0
        if self.on_poll:
            self.on_poll(self, value)
        if value is not None:
            self.returncode = value
        return value

    def wait(self):
        if self.returncode is None:
            self.returncode = 0
        return self.returncode


class Fixture:
    def __init__(self, directory, state, child=None):
        self.directory = directory
        self.state = state
        self.child = child or Child()
        self.spawns = 0
        self.hashes = 0
        self.stops = 0
        self.clock = 0.0
        self.identity_values = [True]
        self.on_identity = None
        self.on_rss = None
        self.close_cancel = False
        self.enter_cancel = False
        self.zombies = []
        self.live = []
        self.cleanup_failure = False
        self.disk_values = [policy.DISK_STOP_BYTES * 2]

    def identity(self):
        if self.on_identity:
            self.on_identity()
        return self.identity_values.pop(0) if len(self.identity_values) > 1 else self.identity_values[0]

    @contextmanager
    def streams(self, directory):
        if self.enter_cancel:
            self.state['signal'] = 15
        yield None, None
        if self.close_cancel:
            self.state['signal'] = 15

    def popen(self, *args, **kwargs):
        self.spawns += 1
        return self.child

    def stop(self, child):
        self.stops += 1
        if self.cleanup_failure:
            raise RuntimeError('mock group remains active')
        child.returncode = 0
        return {'group': child.pid, 'live_descendants': [], 'zombies': list(self.zombies)}

    def rss(self, group):
        return self.on_rss() if self.on_rss else 0

    def disk(self):
        return self.disk_values.pop(0) if len(self.disk_values) > 1 else self.disk_values[0]

    def hash_raw(self, directory):
        self.hashes += 1
        return {'MOCK_NOT_PHYSICAL_RESOURCE_DATA': {}}

    def hooks(self):
        return SimpleNamespace(popen=self.popen, monotonic=lambda: self.clock, sleep=lambda _: None,
                               streams=self.streams, active=lambda _: self.live, members=lambda _: self.zombies,
                               stop_group=self.stop, sample_rss=self.rss, disk_free=self.disk,
                               identity_check=self.identity,
                               parse_resource=lambda _: {'status': 'RECORDED_MOCK_FIXTURE_ONLY'}, hash_raw=self.hash_raw)


class PolicyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='a0-policy-v2-mocks-')
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.state = {'signal': None}
        self.item = sample_item()
        (self.directory / 'analysis.json').write_text(json.dumps(sample_report(self.item)))

    def run_mock(self, fixture):
        return policy.run_case(self.item, self.directory, ['NEVER_EXECUTED'], {}, self.state, fixture.hooks())

    def test_normal_zero_and_strict_report_can_complete(self):
        fixture = Fixture(self.directory, self.state)
        result = self.run_mock(fixture)
        self.assertTrue(policy.complete(result))
        self.assertEqual((fixture.spawns, fixture.hashes), (1, 1))

    def test_rss_stop_then_natural_zero_does_not_complete(self):
        fixture = Fixture(self.directory, self.state, Child([None, 0]))
        def rss():
            fixture.child.returncode = 0
            return policy.RSS_STOP_BYTES + 1
        fixture.on_rss = rss
        result = self.run_mock(fixture)
        self.assertEqual(result['exit_code'], 0)
        self.assertEqual(result['stop_reason'], 'POLLED_RSS_STOP')
        self.assertFalse(result['analysis_complete'])
        self.assertEqual(fixture.hashes, 1)  # quiet failed raw is retained

    def test_disk_stop_then_normal_zero_does_not_complete(self):
        fixture = Fixture(self.directory, self.state, Child([None, 0]))
        fixture.disk_values = [policy.DISK_STOP_BYTES * 2, 1]
        result = self.run_mock(fixture)
        self.assertEqual(result['stop_reason'], 'DISK_STOP')
        self.assertFalse(result['analysis_complete'])

    def test_timeout_then_normal_zero_does_not_complete(self):
        fixture = Fixture(self.directory, self.state)
        fixture.child = Child([None], lambda *_: setattr(fixture, 'clock', 181.0))
        result = self.run_mock(fixture)
        self.assertEqual(result['stop_reason'], 'TIMEOUT')
        self.assertFalse(result['analysis_complete'])

    def test_cancel_during_successful_poll_is_preserved(self):
        fixture = Fixture(self.directory, self.state, Child([0], lambda *_: self.state.update(signal=15)))
        result = self.run_mock(fixture)
        self.assertEqual(result['stop_reason'], 'CANCELLED')
        self.assertFalse(result['analysis_complete'])

    def test_cancel_during_stream_context_exit_is_preserved(self):
        fixture = Fixture(self.directory, self.state)
        fixture.close_cancel = True
        result = self.run_mock(fixture)
        self.assertEqual(result['stop_reason'], 'CANCELLED')
        self.assertFalse(result['analysis_complete'])

    def test_cancel_beats_simultaneous_rss_and_prevents_next_case(self):
        fixture = Fixture(self.directory, self.state, Child([None, 0]))
        def rss():
            self.state['signal'] = 15
            return policy.RSS_STOP_BYTES + 1
        fixture.on_rss = rss
        specs = [sample_item(row[0]) for row in policy.MODELS]
        result, finished = policy.run_sequence(specs, lambda _: self.run_mock(fixture), self.state, lambda: True, lambda _: None)
        self.assertEqual(len(result), 1)
        self.assertFalse(finished)
        self.assertEqual(fixture.spawns, 1)
        self.assertEqual(result[0]['stop_reason'], 'CANCELLED')
        self.assertIn('POLLED_RSS_STOP', result[0]['observed_stop_reasons'])

    def test_pre_spawn_cancel_and_cancel_during_identity_never_spawn(self):
        fixture = Fixture(self.directory, self.state)
        fixture.enter_cancel = True
        result = self.run_mock(fixture)
        self.assertEqual(fixture.spawns, 0)
        self.assertEqual(result['stop_reason'], 'CANCELLED')
        self.state['signal'] = None
        def identity():
            self.state['signal'] = 15
            return True
        results, finished = policy.run_sequence([self.item], lambda _: self.fail('spawn after cancel'), self.state, identity, lambda _: None)
        self.assertEqual(results, [])
        self.assertFalse(finished)

    def test_cancel_always_beats_disk_rss_latched_reasons(self):
        observations = []
        self.state['signal'] = 15
        reason = policy.latch_reason('DISK_STOP', 'POLLED_RSS_STOP', self.state, observations)
        self.assertEqual(reason, 'CANCELLED')
        self.assertEqual(policy.latch_reason(reason, 'TIMEOUT', {'signal': None}, observations), 'CANCELLED')

        # Cancellation also wins if final identity/progress bookkeeping raises.
        self.assertEqual(policy.failure_status(OSError('mock final source read failure'), {'signal': 15}), 'CANCELLED')
        self.assertEqual(policy.failure_status(KeyboardInterrupt(), {'signal': None}), 'CANCELLED')
        self.assertEqual(policy.failure_status(OSError('mock read failure'), {'signal': None}), 'PREFLIGHT_OR_WRAPPER_ERROR')

    def test_zombie_or_live_cleanup_forbids_hash_and_complete(self):
        for key in ['zombies', 'live']:
            with self.subTest(key=key):
                fixture = Fixture(self.directory, self.state)
                if key == 'zombies':
                    fixture.zombies = [(999992, 'Z')]
                else:
                    fixture.live = [999992]
                    fixture.cleanup_failure = True
                result = self.run_mock(fixture)
                self.assertFalse(result['raw_finalized'])
                self.assertFalse(result['analysis_complete'])
                self.assertEqual(fixture.hashes, 0)

    def test_post_run_identity_failure_preserves_raw_but_not_complete(self):
        fixture = Fixture(self.directory, self.state)
        fixture.identity_values = [True, False]
        result = self.run_mock(fixture)
        self.assertTrue(result['raw_finalized'])
        self.assertEqual(fixture.hashes, 1)
        self.assertFalse(result['source_and_binary_stable'])
        self.assertFalse(result['analysis_complete'])

    def test_wrong_schema_scope_flags_and_generic_error_cannot_complete(self):
        for changes in [{'schema': 'wrong'}, {'scope': 'wrong'}, {'cpu_executed': True},
                        {'native_v2_admission_executed': True}, {'credit_executed': True},
                        {'status': 'ERROR'}, {'mode': 'run'}, {'error': 'generic only'}, {'error': 'hidden failure'}]:
            with self.subTest(changes=changes):
                report = {'error': 'generic only'} if changes == {'error': 'generic only'} else dict(sample_report(self.item), **changes)
                self.assertTrue(policy.report_errors(report, self.item))

    def complete_items(self):
        rows = []
        for spec in policy.MODELS:
            item = sample_item(spec[0])
            item.update(stop_reason=None, cancel_seen=False, exit_code=0, leader_reaped=True,
                        cleanup={'group': 999991, 'live_descendants': [], 'zombies': []},
                        raw_finalized=True, report_valid=True, source_and_binary_stable=True)
            rows.append(item)
        return rows

    def test_final_identity_partial_duplicate_and_wrong_model_never_all(self):
        rows = self.complete_items()
        specs = [sample_item(row[0]) for row in policy.MODELS]
        good = {'source_stable': True, 'binary_stable': True, 'head_stable': True}
        self.assertEqual(policy.aggregate(rows, specs, True, good, self.state), 'CLASS_DATA_COMPLETE_ALL')
        for key in good:
            self.assertNotEqual(policy.aggregate(rows, specs, True, dict(good, **{key: False}), self.state), 'CLASS_DATA_COMPLETE_ALL')
        for bad in [rows[:-1], [rows[0], rows[0], rows[2]], [dict(rows[0], model='wrong'), *rows[1:]]]:
            self.assertNotEqual(policy.aggregate(bad, specs, True, good, self.state), 'CLASS_DATA_COMPLETE_ALL')
        self.assertNotEqual(policy.aggregate(rows, specs, False, good, self.state), 'CLASS_DATA_COMPLETE_ALL')
        self.assertNotEqual(policy.aggregate(rows[:1], specs[:1], True, good, self.state), 'CLASS_DATA_COMPLETE_ALL')
        self.assertNotEqual(policy.aggregate([], [], True, good, self.state), 'CLASS_DATA_COMPLETE_ALL')

    def test_late_outer_context_cancel_prevents_final_all(self):
        rows = self.complete_items()
        @contextmanager
        def context():
            yield self.state
            self.state['signal'] = 15
        with context() as state:
            pass
        result = policy.aggregate(rows, [sample_item(row[0]) for row in policy.MODELS], True,
                                  {'source_stable': True, 'binary_stable': True, 'head_stable': True}, state)
        self.assertEqual(result, 'CANCELLED')

    def test_progress_keeps_running_until_for_completion(self):
        rows = self.complete_items()
        rows[0]['exit_code'] = 2
        progress = []
        state = {'status': 'RUNNING'}
        def update(results):
            progress.append((state['status'], len(results)))
        results, finished = policy.run_sequence(rows, lambda row: row, self.state, lambda: True, update)
        self.assertTrue(finished)
        self.assertEqual(progress, [('RUNNING', 1), ('RUNNING', 2), ('RUNNING', 3)])
        self.assertEqual(policy.aggregate(results, rows, finished,
                         {'source_stable': True, 'binary_stable': True, 'head_stable': True}, self.state), 'CLASS_DATA_INCOMPLETE')

    def test_existing_actual_snapshot_zero_two_two_reclassifies_unchanged(self):
        # Read existing JSON only. This invokes no binary/source/target/model path.
        manifest = policy.ORIGINAL_OUT / 'manifest.json'
        original = json.loads(manifest.read_text())
        items = []
        for old in original['results']:
            report = json.loads((policy.ORIGINAL_OUT / old['slug'] / 'analysis.json').read_text())
            item = dict(old, cancel_seen=False, leader_reaped=type(old['exit_code']) is int,
                        raw_finalized=policy.clean_group(old['cleanup']), report_valid=not policy.report_errors(report, old))
            items.append(item)
        self.assertEqual([row['exit_code'] for row in items], [0, 2, 2])
        self.assertEqual([policy.complete(row) for row in items], [True, False, False])
        result = policy.aggregate(items, [sample_item(row[0]) for row in policy.MODELS], True,
                                  {'source_stable': original['source_stable'], 'binary_stable': True, 'head_stable': True}, self.state)
        self.assertEqual(result, 'CLASS_DATA_INCOMPLETE')


if __name__ == '__main__':
    unittest.main(verbosity=2)
