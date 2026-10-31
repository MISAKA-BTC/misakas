import hashlib
import json
import tempfile
from pathlib import Path
import unittest
from run import (
    REQUIRED_TARGETS, build_inputs, final_identity_errors, required_target_errors, sha, source_snapshot,
    suite_commands, suite_requirement_errors, test_counts,
)


# Independent literal inventory: never construct expected names from the runner
# policy being tested, or a missing requirement would also disappear here.
PIPELINE_NAMES = [
    'bound_a2_rejects_parameters_that_do_not_open_to_the_fixed_registration',
    'bound_a2_foreign_class_job_and_prompt_never_reach_the_signer',
    'committed_a2_a3_a1_and_a5_connect_with_independent_expected_tokens',
    'checked_math_does_not_replace_gate_c_funding_maturity_or_current_fraud_state',
    'a2_signed_wrong_output_is_checked_mismatch_and_never_private_credit',
    'missing_last_a3_frame_cannot_produce_a5_terminal_authority',
    'paid_success_has_only_existing_worker_payment_and_settlement_after_real_checked_execution',
    'omitted_existing_entitlement_port_denies_even_after_real_checked_execution_and_all_prerequisites',
]
FENCE_NAMES = [
    'checks::positive2_rejects_unsupported_ids_zero_unbounded_overflow_and_inconsistent_caps',
    'checks::positive2_ruleset_binds_only_the_fired_fork_and_current_policy',
    'checks::positive2_raw_tag_is_opaque_before_decode_signature_resources_or_funding',
    'checks::positive2_policy_root_matches_independent_python_fixed_schema_vector',
    'checks::positive2_never_collapses_the_whole_profile_but_raw_diagnostics_commit_it',
    'checks::positive2_all_profile_fields_are_committed',
    'checks::positive2_scheduled_profile_preserves_pre_fork_peer_identity_and_is_not_a_public_permit',
    'checks::positive2_released_presets_are_absent_and_none_writes_no_fingerprint_field',
]


def cargo_output(target, passed_names=(), ignored_names=(), *, passed=None, failed=0):
    lines = [f'     Running tests/{target}.rs (target/debug/deps/{target}-abc)',
             f'running {len(passed_names) + len(ignored_names) + failed} tests']
    lines += [f'test {name} ... ok' for name in passed_names]
    lines += [f'test {name} ... ignored' for name in ignored_names]
    count = len(passed_names) if passed is None else passed
    outcome = 'FAILED' if failed else 'ok'
    lines.append(f'test result: {outcome}. {count} passed; {failed} failed; '
                 f'{len(ignored_names)} ignored; 0 measured; 0 filtered out;')
    return '\n'.join(lines) + '\n'


def target_command(*targets):
    return ['cargo', 'test', *[arg for target in targets for arg in ['--test', target]]]


def result_entry(target, names, *, argv=None, exit_code=0):
    return {'argv': target_command(target) if argv is None else argv,
            'exit_code': exit_code, 'test_counts': test_counts(cargo_output(target, names))}


class EvidenceIdentity(unittest.TestCase):
    def test_run_output_is_excluded_without_hiding_new_source_outside_it(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            output=root/'testing/positive-court-a7/fresh-output'
            output.mkdir(parents=True)
            before=source_snapshot(root,[output])
            (output/'run.json').write_text('{"result":"partial"}')
            self.assertEqual(before,source_snapshot(root,[output]))
            (output.parent/'new.rs').write_text('concurrent source')
            self.assertNotEqual(before,source_snapshot(root,[output]))

    def test_one_passing_target_cannot_hide_an_ignored_or_missing_required_target(self):
        independent = cargo_output('independent', ['independent_ok'], passed=12)
        data = independent + cargo_output('pipeline', ignored_names=PIPELINE_NAMES)
        command = target_command('independent', 'pipeline')
        counts = test_counts(data)
        self.assertEqual(required_target_errors(command, counts), ['pipeline'])
        self.assertEqual(required_target_errors(command, counts[:1]), ['pipeline'])
        data = independent + cargo_output('pipeline', PIPELINE_NAMES)
        self.assertEqual(required_target_errors(command, test_counts(data)), [])

    def test_required_policy_names_are_literal_complete_and_manifest_serializable(self):
        expected = {'pipeline': PIPELINE_NAMES, 'positive_court_fence_v2': FENCE_NAMES}
        self.assertEqual(set(REQUIRED_TARGETS), set(expected))
        for target, names in expected.items():
            with self.subTest(target=target):
                self.assertEqual(REQUIRED_TARGETS[target]['min_passed'], 8)
                self.assertEqual(set(REQUIRED_TARGETS[target]['passed_names']), set(names))
                self.assertEqual(len(REQUIRED_TARGETS[target]['passed_names']), 8)
        self.assertEqual(json.loads(json.dumps(REQUIRED_TARGETS)), REQUIRED_TARGETS)

    def test_each_required_target_rejects_empty_ignored_missing_or_failed_results(self):
        for target, names in [('pipeline', PIPELINE_NAMES), ('positive_court_fence_v2', FENCE_NAMES)]:
            command = target_command(target)
            for data in ['', cargo_output(target), cargo_output(target, ignored_names=names),
                         cargo_output(target, names, passed=7),
                         cargo_output(target, names, failed=1)]:
                with self.subTest(target=target, data=data):
                    self.assertEqual(required_target_errors(command, test_counts(data)), [target])

    def test_other_target_and_one_private_visitor_cannot_replace_eight_fence_cases(self):
        command = target_command('pipeline', 'positive_court_fence_v2')
        other = cargo_output('independent', ['other_pass'], passed=100)
        pipeline = cargo_output('pipeline', PIPELINE_NAMES)
        visitor = cargo_output('positive_court_fence_v2', ['checks::private_fork_visitor_diagnostic'])
        self.assertEqual(required_target_errors(command, test_counts(other + pipeline)), ['positive_court_fence_v2'])
        self.assertEqual(required_target_errors(command, test_counts(other + pipeline + visitor)), ['positive_court_fence_v2'])
        self.assertEqual(required_target_errors(command, test_counts(cargo_output('positive_court_fence_v2', FENCE_NAMES))), ['pipeline'])

    def test_eight_passes_cannot_replace_one_missing_or_ignored_required_name(self):
        for target, names in [('pipeline', PIPELINE_NAMES), ('positive_court_fence_v2', FENCE_NAMES)]:
            for omitted in names:
                passed = [name for name in names if name != omitted] + ['unrelated_extra_case']
                for ignored in [[], [omitted]]:
                    with self.subTest(target=target, omitted=omitted, ignored=bool(ignored)):
                        data = cargo_output(target, passed, ignored)
                        counts = test_counts(data)
                        self.assertEqual(counts[0]['passed'], 8)
                        self.assertNotIn(omitted, counts[0]['passed_names'])
                        self.assertEqual(required_target_errors(target_command(target), counts), [target])

    def test_required_names_cannot_be_combined_across_two_passing_summaries(self):
        for target, names in [('pipeline', PIPELINE_NAMES), ('positive_court_fence_v2', FENCE_NAMES)]:
            with self.subTest(target=target):
                extras = [f'other_{i}' for i in range(4)]
                data = cargo_output(target, names[:4] + extras) + cargo_output(target, names[4:] + extras)
                counts = test_counts(data)
                self.assertEqual([row['passed'] for row in counts], [8, 8])
                self.assertEqual(set(counts[0]['passed_names']), set(names[:4] + extras))
                self.assertEqual(set(counts[1]['passed_names']), set(names[4:] + extras))
                self.assertEqual(required_target_errors(target_command(target), counts), [target])

    def test_running_headers_and_summaries_reset_passed_name_state(self):
        first = cargo_output('pipeline', PIPELINE_NAMES[:4])
        # A second harness summary without a new target header cannot inherit the
        # earlier successes, even if it reports eight aggregate passes.
        second = ''.join(f'test {name} ... ok\n' for name in PIPELINE_NAMES[4:])
        second += 'test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n'
        counts = test_counts(first + second)
        self.assertEqual(set(counts[1]['passed_names']), set(PIPELINE_NAMES[4:]))
        self.assertEqual(required_target_errors(target_command('pipeline'), counts), ['pipeline'])
        for header in ['     Running unittests src/lib.rs (target/debug/deps/library-abc)', '   Doc-tests library']:
            with self.subTest(header=header):
                counts = test_counts(first + header + '\n' + second)
                self.assertIsNone(counts[1]['target'])
                self.assertEqual(required_target_errors(target_command('pipeline'), counts), ['pipeline'])
        # A new harness also resets names when the preceding harness did not
        # supply a summary at all (for example an interrupted target).
        interrupted = first[:first.index('test result:')]
        counts = test_counts(interrupted + 'running 8 tests\n' + second)
        self.assertEqual(set(counts[0]['passed_names']), set(PIPELINE_NAMES[4:]))
        self.assertEqual(required_target_errors(target_command('pipeline'), counts), ['pipeline'])

    def test_required_names_with_extra_passing_cases_are_allowed(self):
        for target, names in [('pipeline', PIPELINE_NAMES), ('positive_court_fence_v2', FENCE_NAMES)]:
            with self.subTest(target=target):
                counts = test_counts(cargo_output(target, names + ['future_additional_case']))
                self.assertEqual(counts[0]['passed'], 9)
                self.assertEqual(required_target_errors(target_command(target), counts), [])

    def test_suite_uses_the_dedicated_fence_target_without_a_library_filter(self):
        commands = suite_commands('python-for-test')
        self.assertEqual(len(commands), 12)
        self.assertEqual(len(suite_commands('python-for-test', independent_only=True)), 3)
        fence = [command for command in commands if 'positive_court_fence_v2' in command]
        self.assertEqual(len(fence), 1)
        command = fence[0]
        self.assertIn('--test', command)
        self.assertEqual(command[command.index('--test') + 1], 'positive_court_fence_v2')
        self.assertNotIn('--lib', command)
        self.assertNotIn('config::params::positive_court_v2::tests::', command)
        self.assertNotIn('--no-run', command)
        self.assertIn('--offline', command)
        self.assertIn('--locked', command)
        self.assertFalse(any('--lib' in command and any('positive_court_v2' in part for part in command)
                             for command in commands))
        self.assertFalse(any('positive_court_fence_v2' in command
                             for command in suite_commands('python-for-test', independent_only=True)))

    def test_suite_cannot_count_no_run_entries_even_with_forged_complete_results(self):
        for target, names, other_target, other_names in [
            ('pipeline', PIPELINE_NAMES, 'positive_court_fence_v2', FENCE_NAMES),
            ('positive_court_fence_v2', FENCE_NAMES, 'pipeline', PIPELINE_NAMES),
        ]:
            with self.subTest(target=target):
                build_only = result_entry(target, names, argv=target_command(target) + ['--no-run'])
                entries = [build_only, result_entry(other_target, other_names)]
                self.assertEqual(suite_requirement_errors(entries), [target])

    def test_suite_missing_pipeline_execution_cannot_borrow_fence_or_other_target_success(self):
        entries = [result_entry('positive_court_fence_v2', FENCE_NAMES),
                   result_entry('independent', ['independent_ok'])]
        self.assertEqual(suite_requirement_errors(entries), ['pipeline'])
        self.assertEqual(suite_requirement_errors([]), ['pipeline', 'positive_court_fence_v2'])
        # Even apparently complete pipeline counts must originate from a
        # command which actually selected that target before Cargo's separator.
        for argv in [target_command('independent'), ['cargo', 'test', '--', '--test', 'pipeline']]:
            with self.subTest(argv=argv):
                self.assertEqual(suite_requirement_errors(entries + [result_entry('pipeline', PIPELINE_NAMES, argv=argv)]),
                                 ['pipeline'])

    def test_suite_ignores_failed_or_invalid_entries_despite_complete_success_lines(self):
        fence = result_entry('positive_court_fence_v2', FENCE_NAMES)
        for code in [1, 'INTERRUPTED']:
            with self.subTest(exit_code=code):
                entry = result_entry('pipeline', PIPELINE_NAMES, exit_code=code)
                self.assertEqual(suite_requirement_errors([entry, fence]), ['pipeline'])
        invalid = result_entry('pipeline', PIPELINE_NAMES)
        invalid['validation_error'] = 'NO_EXECUTED_TESTS'
        self.assertEqual(suite_requirement_errors([invalid, fence]), ['pipeline'])

    def test_suite_cannot_merge_partial_success_names_from_separate_entries(self):
        for target, names, other_target, other_names in [
            ('pipeline', PIPELINE_NAMES, 'positive_court_fence_v2', FENCE_NAMES),
            ('positive_court_fence_v2', FENCE_NAMES, 'pipeline', PIPELINE_NAMES),
        ]:
            with self.subTest(target=target):
                extras = [f'extra_{i}' for i in range(4)]
                entries = [result_entry(target, names[:4] + extras),
                           result_entry(target, names[4:] + extras),
                           result_entry(other_target, other_names)]
                self.assertEqual(suite_requirement_errors(entries), [target])

    def test_suite_accepts_both_executed_targets_and_diagnostic_requires_only_pipeline(self):
        pipeline = result_entry('pipeline', PIPELINE_NAMES)
        fence = result_entry('positive_court_fence_v2', FENCE_NAMES)
        self.assertEqual(suite_requirement_errors([pipeline, fence]), [])
        self.assertEqual(suite_requirement_errors([pipeline]), ['positive_court_fence_v2'])
        self.assertEqual(suite_requirement_errors([pipeline], independent_only=True), [])
        self.assertEqual(suite_requirement_errors([fence], independent_only=True), ['pipeline'])
        no_run = result_entry('pipeline', PIPELINE_NAMES, argv=target_command('pipeline') + ['--no-run'])
        self.assertEqual(suite_requirement_errors([no_run], independent_only=True), ['pipeline'])

    def test_build_inputs_respects_existing_absolute_relative_and_default_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache = root/'existing-cache'
            cache.mkdir()
            for value in [str(cache), 'existing-cache']:
                with self.subTest(value=value):
                    inputs = build_inputs(root, {'CARGO_TARGET_DIR': value})
                    self.assertEqual(inputs['build_env']['CARGO_TARGET_DIR'], str(cache.resolve()))
                    self.assertIsNone(inputs['native_archive'])
                    self.assertFalse((root/'target').exists())
            (root/'target').mkdir()
            self.assertEqual(build_inputs(root, {})['build_env']['CARGO_TARGET_DIR'], str((root/'target').resolve()))

    def test_missing_target_or_archive_is_rejected_without_creating_directories(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache = root/'existing-cache'
            cache.mkdir()
            file_target = root/'not-a-directory'
            file_target.write_bytes(b'file')
            cases = [{}, {'CARGO_TARGET_DIR': ''}, {'CARGO_TARGET_DIR': 'missing/target'},
                     {'CARGO_TARGET_DIR': str(file_target)},
                     {'CARGO_TARGET_DIR': str(cache), 'ROCKSDB_LIB_DIR': ''},
                     {'CARGO_TARGET_DIR': str(cache), 'ROCKSDB_LIB_DIR': 'missing/native'},
                     {'CARGO_TARGET_DIR': str(cache), 'ROCKSDB_LIB_DIR': str(cache)}]
            before = set(root.rglob('*'))
            for environ in cases:
                with self.subTest(environ=environ), self.assertRaises(ValueError):
                    build_inputs(root, environ)
                self.assertEqual(set(root.rglob('*')), before)

    def test_native_archive_metadata_pins_hash_raw_environment_and_resolved_build_path(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'target').mkdir()
            native = root/'native'
            native.mkdir()
            archive = native/'librocksdb.a'
            archive.write_bytes(b'private synthetic archive bytes')
            for static in [None, '', '1']:
                environ = {'ROCKSDB_LIB_DIR': 'native'}
                if static is not None:
                    environ['ROCKSDB_STATIC'] = static
                with self.subTest(static=static):
                    inputs = build_inputs(root, environ)
                    self.assertEqual(inputs['native_archive'], {
                        'path': str(archive.resolve()),
                        'sha256': hashlib.sha256(b'private synthetic archive bytes').hexdigest(),
                        'ROCKSDB_LIB_DIR': 'native', 'ROCKSDB_STATIC': static,
                    })
                    self.assertEqual(inputs['build_env']['ROCKSDB_LIB_DIR'], str(native.resolve()))
                    if static is None:
                        self.assertNotIn('ROCKSDB_STATIC', inputs['build_env'])
                    else:
                        self.assertEqual(inputs['build_env']['ROCKSDB_STATIC'], static)

    def test_final_native_identity_rejects_archive_change_removal_and_environment_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'target').mkdir()
            native = root/'native'
            native.mkdir()
            archive = native/'librocksdb.a'
            archive.write_bytes(b'original archive')
            binary = root/'checker'
            binary.write_bytes(b'checker')
            environ = {'ROCKSDB_LIB_DIR': 'native', 'ROCKSDB_STATIC': '1'}
            record = {'commit': 'abc', 'source_sha256': source_snapshot(root), 'cargo_lock_sha256': {},
                      'a7_binary': {'path': str(binary), 'sha256': sha(binary)},
                      'native_archive': build_inputs(root, environ)['native_archive']}
            self.assertEqual(final_identity_errors(root, record, 'abc', environ=environ), [])
            archive.write_bytes(b'changed archive')
            self.assertIn('NATIVE_ARCHIVE_CHANGED_OR_MISSING', final_identity_errors(root, record, 'abc', environ=environ))
            archive.unlink()
            self.assertIn('NATIVE_ARCHIVE_CHANGED_OR_MISSING', final_identity_errors(root, record, 'abc', environ=environ))
            archive.write_bytes(b'original archive')
            for changed in [{}, {**environ, 'ROCKSDB_STATIC': ''}, {**environ, 'ROCKSDB_LIB_DIR': str(native)}]:
                with self.subTest(environ=changed):
                    self.assertIn('NATIVE_ARCHIVE_ENV_CHANGED', final_identity_errors(root, record, 'abc', environ=changed))

    def test_final_native_identity_rejects_symlink_retarget_with_unchanged_environment_and_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root/'target').mkdir()
            for name in ['first', 'second']:
                (root/name).mkdir()
                (root/name/'librocksdb.a').write_bytes(b'identical synthetic archive')
            link = root/'native'
            link.symlink_to(root/'first', target_is_directory=True)
            environ = {'ROCKSDB_LIB_DIR': 'native'}
            inputs = build_inputs(root, environ)
            self.assertEqual(inputs['build_env']['ROCKSDB_LIB_DIR'], str((root/'first').resolve()))
            binary = root/'checker'
            binary.write_bytes(b'checker')
            record = {'commit': 'abc', 'source_sha256': source_snapshot(root), 'cargo_lock_sha256': {},
                      'a7_binary': {'path': str(binary), 'sha256': sha(binary)}, 'native_archive': inputs['native_archive']}
            self.assertEqual(final_identity_errors(root, record, 'abc', environ=environ), [])
            link.unlink()
            link.symlink_to(root/'second', target_is_directory=True)
            errors = final_identity_errors(root, record, 'abc', environ=environ)
            self.assertIn('NATIVE_ARCHIVE_PATH_CHANGED', errors)
            self.assertNotIn('NATIVE_ARCHIVE_ENV_CHANGED', errors)
            self.assertNotIn('NATIVE_ARCHIVE_CHANGED_OR_MISSING', errors)

    def test_final_identity_rejects_changed_source_set_lock_binary_and_head(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root/'testing/positive-court-a7/fixture.rs'
            source.parent.mkdir(parents=True)
            source.write_text('original')
            lock = root/'Cargo.lock'
            lock.write_text('locked')
            binary = root/'checker'
            binary.write_bytes(b'checked binary')
            record = {'commit':'abc', 'source_sha256':source_snapshot(root),
                      'cargo_lock_sha256':{'Cargo.lock':sha(lock)},
                      'a7_binary':{'path':str(binary),'sha256':sha(binary)}}
            self.assertEqual(final_identity_errors(root, record, 'abc'), [])
            source.with_name('added.rs').write_text('concurrent new source')
            self.assertIn('SOURCE_SET_OR_BYTES_CHANGED',final_identity_errors(root, record, 'abc'))
            source.with_name('added.rs').unlink()
            source.write_text('concurrent edit')
            lock.write_text('different lock')
            binary.write_bytes(b'other binary')
            errors = final_identity_errors(root, record, 'def')
            self.assertIn('HEAD_CHANGED', errors)
            self.assertIn('SOURCE_SET_OR_BYTES_CHANGED', errors)
            self.assertIn('LOCK_CHANGED:Cargo.lock', errors)
            self.assertIn('BINARY_CHANGED_OR_MISSING', errors)


if __name__ == '__main__':
    unittest.main()
