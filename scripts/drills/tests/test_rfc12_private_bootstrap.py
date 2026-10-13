"""Driver refusal/owned-child controls, never real native or P2P acceptance."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
import time
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location('private_bootstrap_driver', SCRIPTS / 'misaka-rfc12-private-bootstrap.py')
driver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(driver)


class DriverControls(unittest.TestCase):
    def test_later_complete_refusal_preserves_inclusion_and_actual_prior_fees_without_pass(self):
        class FakeDrill:
            plan = dict(nodes={'A': dict(borsh='owned:1')})

            def start(self, role):
                pass

            def join_link(self, role, target):
                pass

            def wait(self, predicate, label):
                return predicate()

        included = {role: dict(sink='observed', onboarding=dict(found=True)) for role in ('A', 'B', 'JOIN')}
        preview = dict(dry_run=True, total_fee_sompi=12)
        sent = dict(ok=True, submitted=True, carriers=[dict(fee_sompi=5), dict(fee_sompi=7)])
        result = dict(status='FAIL', nativeRegistrationBindingMeasured=False, coldReopenMeasured=False,
            completeCheckMeasured=False)

        def later_refusal(_drill, _sampler, _cli, _salt, _fixture, _bootstrap, budget, *args):
            driver.complete.cap_and_charge(dict(carriers=[dict(fee_sompi=13)]), budget)
            raise driver.transport.Blocked('complete verdict refused after a funded carrier')

        bootstrap = dict(envelopeValidFromDaa=0, envelopeValidUntilDaa=0,
            fundingKeyFile='owned.key', signedRegistration='registration', artifactBinding='binding')
        with patch.object(driver, 'ready', return_value=True), \
                patch.object(driver, 'rpc_read', return_value=dict(virtualDaaScore=0)), \
                patch.object(driver, 'command', side_effect=[preview, sent]), \
                patch.object(driver, 'agreement', return_value=included), \
                patch.object(driver.complete, 'run', side_effect=later_refusal):
            with self.assertRaisesRegex(driver.transport.Blocked, 'complete verdict refused'):
                driver.run(FakeDrill(), None, Path('/owned/cli'), '74' * 32, Path('/owned/fixture'),
                    bootstrap, 100, True, 100, 1,
                    lambda evidence, cold: driver.record_observed_progress(result, evidence, cold))
        self.assertEqual(result['status'], 'FAIL')
        self.assertTrue(result['nativeRegistrationBindingMeasured'])
        self.assertFalse(result['coldReopenMeasured'])
        self.assertFalse(result['completeCheckMeasured'])
        self.assertEqual(result['evidence']['included'], included)
        self.assertEqual(result['evidence']['carrierBudget']['spent'], 25)
        self.assertIsNone(result['evidence']['completeCheck'])

    def test_registration_window_refuses_actual_daa_zero_before_carrier_spend(self):
        original = dict(envelopeValidFromDaa=1, envelopeValidUntilDaa=32)
        with self.assertRaisesRegex(driver.transport.Blocked, 'excludes actual DAA 0'):
            driver.registration_window(original, 0)
        corrected = dict(envelopeValidFromDaa=0, envelopeValidUntilDaa=0)
        self.assertEqual(driver.registration_window(corrected, 0)['observedDaa'], 0)
        with self.assertRaisesRegex(driver.transport.Blocked, 'excludes actual DAA 1'):
            driver.registration_window(corrected, 1)

    def test_complete_cost_cap_and_moving_private_pins_refuse_before_submission(self):
        bootstrap = dict(bindingReservationSompi='100', completeCheckFeeSompi='1',
            commitment='fixture.borsh', statementRoot='root', commitmentScope='scope.json')
        with self.assertRaisesRegex(ValueError, 'explicit'):
            driver.complete.costs_before_nodes(bootstrap, None, 1)
        with self.assertRaisesRegex(ValueError, 'no node was started'):
            driver.complete.costs_before_nodes(bootstrap, 99, 1)
        driver.complete.costs_before_nodes(bootstrap, 100, 1)
        rows = {role: dict(sink='known-block', virtualDaaScore=45,
            onboarding=dict(ledgerRoot='ledger', auxRoot='aux')) for role in ('A', 'B', 'JOIN')}
        self.assertEqual(driver.complete.pins_from_owned_agreement(rows)['daa'], 45)
        rows['JOIN']['virtualDaaScore'] = 46
        with self.assertRaisesRegex(AssertionError, 'DAA disagree'):
            driver.complete.pins_from_owned_agreement(rows)

    def test_cumulative_carrier_cap_refuses_second_batch_and_overflow(self):
        budget = dict(maximum=100, spent=0)
        driver.complete.cap_and_charge(dict(carriers=[dict(fee_sompi=40), dict(fee_sompi=20)]), budget)
        self.assertEqual(budget['spent'], 60)
        with self.assertRaisesRegex(AssertionError, 'remaining total workflow cap'):
            driver.complete.cap_and_charge(dict(carriers=[dict(fee_sompi=41)]), budget)
        self.assertEqual(budget['spent'], 60)
        with self.assertRaisesRegex(AssertionError, 'remaining total workflow cap'):
            driver.complete.cap_and_charge(dict(carriers=[dict(fee_sompi=2**63), dict(fee_sompi=2**63)]), dict(maximum=2**64-1, spent=0))

    def test_expired_total_deadline_starts_no_actual_child(self):
        sampler = driver.OwnedResourceSampler({}, None)
        try:
            log = type('Log', (), {'workflow_deadline': time.monotonic() - 1})()
            with patch.object(driver.subprocess, 'Popen') as popen:
                with self.assertRaisesRegex(driver.transport.Blocked, 'deadline expired before'):
                    driver.command(log, sampler, 'NEVER_START', [sys.executable, '-c', 'raise RuntimeError()'], 5)
                popen.assert_not_called()
        finally:
            sampler.stop()

    def test_wrong_private_factory_is_refused_before_onboarding_read(self):
        calls = []

        def rpc(_drill, _role, method, payload=None):
            calls.append(method)
            return {'consensusParamsId': 'other-profile', 'genesisHash': 'salted-genesis'}

        with patch.object(driver, 'rpc_read', rpc):
            with self.assertRaisesRegex(AssertionError, 'factory ID mismatch'):
                driver.onboarding(None, 'A', {'consensusParamsId': 'complete-profile', 'genesis': 'salted-genesis'})
        self.assertEqual(calls, ['getPalwNodeStatus'])

    def test_timeouting_command_kills_only_its_actual_child(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            pidfile = root / 'owned.pid'
            sampler = driver.OwnedResourceSampler({}, None)
            code = 'import os,time;from pathlib import Path;Path(%r).write_text(str(os.getpid()));time.sleep(60)' % str(pidfile)
            log = type('Log', (), {'root': root})()
            try:
                with self.assertRaises(subprocess.TimeoutExpired):
                    driver.command(log, sampler, 'OWNED_TIMEOUT', [sys.executable, '-c', code], .5)
                self.assertTrue(pidfile.is_file(), 'actual OS child did not start')
                with self.assertRaises(ProcessLookupError):
                    os.kill(int(pidfile.read_text()), 0)
            finally:
                sampler.stop()

    def test_actual_failed_command_preserves_diagnostic_and_refuses_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            sampler = driver.OwnedResourceSampler({}, None)
            root = Path(directory)
            try:
                with self.assertRaisesRegex(driver.transport.Blocked, 'refused actual native admission'):
                    driver.command(type('Log', (), {'root': root})(), sampler, 'REFUSAL',
                        [sys.executable, '-c', 'import sys;print("refused actual native admission",file=sys.stderr);sys.exit(2)'], 5)
                self.assertIn('refused actual native admission', (root / 'REFUSAL.stderr').read_text())
            finally:
                sampler.stop()


if __name__ == '__main__':
    unittest.main()
