"""Driver refusal/owned-child controls, never real native or P2P acceptance."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location('private_bootstrap_driver', SCRIPTS / 'misaka-rfc12-private-bootstrap.py')
driver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(driver)


class DriverControls(unittest.TestCase):
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
