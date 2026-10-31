"""Failure/status contracts for the onboarding driver; no network or model execution."""
import json
from pathlib import Path
import sys
import tempfile
import unittest
import onboard_hf_checkpoint as onboarding


class OnboardingTests(unittest.TestCase):
    def test_context_is_source_declared_without_guessing(self):
        self.assertEqual(onboarding.declared_context({'max_position_embeddings': 32768}), 32768)
        for config in [{}, {'n_positions': 0}, {'n_positions': True},
                       {'n_positions': 64, 'max_position_embeddings': 128}]:
            with self.assertRaises(ValueError):
                onboarding.declared_context(config)

    def test_registration_requires_all_stages_and_exact_scope(self):
        pack = {'declared': [{'class_id': 'class', 'max_context': 2048}], 'result': {'inventory_root': 'root'}}
        good = {'schema': 'misaka.hf-registration-local-acceptance.v1', 'class_id': 'class',
                'artifact_root': 'root', 'max_context': 2048, 'public_network': False,
                'mempool_accepted': True, 'registration_mined': True,
                'persisted_registry_read': True, 'independent_node_replay': True}
        onboarding.native_result_matches(pack, good)
        for key, value in [('class_id', 'other'), ('artifact_root', 'other'), ('max_context', 64),
                           ('registration_mined', False), ('independent_node_replay', False), ('public_network', True)]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                onboarding.native_result_matches(pack, {**good, key: value})

    def test_nonzero_exit_is_retained_and_never_registration_success(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = onboarding.Run(Path(tmp)/'run', 10)
            with self.assertRaisesRegex(ValueError, 'exit 7'):
                run.stage('failed', [sys.executable, '-c', 'print("reason"); raise SystemExit(7)'])
            state = json.loads((run.root/'run.json').read_text())
            self.assertEqual(state['stages'][0]['status'], 'FAILED')
            self.assertEqual(state['stages'][0]['exit_code'], 7)
            self.assertEqual(state['registration'], 'NOT_RUN')
            self.assertIn('reason', (run.root/'failed.stdout').read_text())

    def test_timeout_stops_process_and_records_failed_stage(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = onboarding.Run(Path(tmp)/'run', .05)
            with self.assertRaisesRegex(ValueError, 'timeout'):
                run.stage('timeout', [sys.executable, '-c', 'import time; time.sleep(120)'])
            self.assertEqual(json.loads((run.root/'run.json').read_text())['stages'][0]['status'], 'FAILED')

    def test_range_error_does_not_classify_a_checkpoint_as_untrained(self):
        message = 'NOT_LOWERABLE(LOGITS_Q24_V1: the calibrated logits reach 1082.2 natural-log units)'
        result = onboarding.failure_diagnostic(message)
        self.assertEqual(result['category'], 'OUTPUT_REPRESENTATION_RANGE')
        self.assertEqual(result['observed_absmax'], 1082.2)
        self.assertFalse(result['training_status_inferred'])
        self.assertIsNone(onboarding.failure_diagnostic('checkpoint HTTP 404'))

    def test_wide_range_error_names_the_wide_representation(self):
        result = onboarding.failure_diagnostic('LOGITS_I64_Q24_V2: the calibrated logits reach 600000000000.0 natural-log units')
        self.assertEqual(result['profile'], 'LOGITS_I64_Q24_V2')
        self.assertEqual(result['dtype'], 'i64')
        self.assertFalse(result['training_status_inferred'])

    def test_output_cannot_reuse_stale_acceptance_results(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(FileExistsError):
                onboarding.Run(Path(tmp), 10)


if __name__ == '__main__':
    unittest.main()
