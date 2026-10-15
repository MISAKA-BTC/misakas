"""Driver bookkeeping controls without any child process; never a node, P2P or acceptance result."""
import importlib.util
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location('acceptance_driver_under_test', SCRIPTS / 'misaka-rfc12-acceptance.py')
driver = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(driver)
J = driver.J

IDENTITY = dict(genesis='11' * 64, consensusParamsId='22' * 32, retirementActivationDaa=1, retirementPolicyId='33' * 64)


def bare(network=None):
    run = driver.AcceptanceRun.__new__(driver.AcceptanceRun)
    argv = ['--palw-produce', '--palw-producer-key=/owned/k', '--private-probe-kernel-paid-work=/owned/a',
            '--private-probe-profile=paid-v4', '--private-probe-salt=74', '--private-probe-native-exchange=127.0.0.1:1',
            '--private-probe-kernel-paid-check=/owned/c', '--appdir={A_APPDIR}', '--nodnsseed']
    run.drill = SimpleNamespace(plan=dict(nodes=dict(A=dict(argv=argv))),
                                argv=lambda template: [item.format_map({'A_APPDIR': '/owned/A/appdir'}) for item in template])
    run.plan = dict(network=network or dict(kind='private-profile', profile='paid-v4'), identity=IDENTITY,
                    evm=dict(consumerCheckpoints=[]), crash=dict(points=[]))
    run.findings, run.phases, run.flags, run.stopped, run.armed, run.crash_records = [], [], set(), None, [], {}
    run.write_evidence = lambda: None
    return run


class DriverControls(unittest.TestCase):
    def test_flag_replacement_never_touches_a_longer_neighbour(self):
        run = bare()
        argv = run.drill.plan['nodes']['A']['argv']
        run.set_flag('A', '--palw-produce', None)
        self.assertNotIn('--palw-produce', argv)
        self.assertIn('--palw-producer-key=/owned/k', argv)
        run.set_flag('A', '--private-probe-kernel-paid-work=', '--private-probe-kernel-paid-work=/owned/b')
        self.assertEqual([item for item in argv if item.startswith('--private-probe-kernel-paid-work=')],
                         ['--private-probe-kernel-paid-work=/owned/b'], 'a repeated cycle replaces, never stacks, the spec')

    def test_wrong_profile_launch_keeps_only_profile_and_salt_private_options(self):
        argv = bare().mismatch_argv('A', 'paid-v3')
        self.assertIn('--private-probe-profile=paid-v3', argv)
        self.assertIn('--private-probe-salt=74', argv)
        self.assertIn('--appdir=/owned/A/appdir', argv)
        self.assertFalse([item for item in argv if item.startswith('--private-probe-') and
                          not item.startswith(('--private-probe-profile=', '--private-probe-salt='))])

    def test_phase_outcomes_map_to_verdicts_and_stop_only_on_lost_processes(self):
        run = bare()
        called = []
        run.phase('needs-work', (2,), ('workSettled',), lambda: called.append(1))
        self.assertEqual((called, run.findings[-1]['verdict']), ([], J.NOT_RUN))

        def violation():
            raise J.Violation('roots differ')
        run.phase('fork', (1, 4), (), violation)
        self.assertEqual([item['verdict'] for item in run.findings[-2:]], [J.FAIL, J.FAIL])
        self.assertIsNone(run.stopped)

        def defect():
            raise KeyError('field')
        run.phase('defect', (3,), (), defect)
        self.assertEqual(run.findings[-1]['verdict'], J.BLOCKED)
        self.assertIn('harness error', run.findings[-1]['reason'])

        def lost():
            raise driver.ProcessExited('A exited 9 unexpectedly')
        run.phase('lost', (2,), (), lost)
        self.assertEqual(run.findings[-1]['verdict'], J.BLOCKED)
        self.assertIsNotNone(run.stopped)
        run.phase('after', (5,), (), lambda: called.append(2))
        self.assertEqual((called, run.findings[-1]['verdict']), ([], J.NOT_RUN))
        self.assertNotIn('PASS', {item['verdict'] for item in run.findings})

    def test_a_preset_is_blocked_before_any_binary_or_process(self):
        run = bare(dict(kind='preset', name='testnet-12'))
        run.phase('identity', (1, 7, 9), (), run.phase_identity)
        self.assertEqual({item['verdict'] for item in run.findings}, {J.BLOCKED})
        self.assertIn('preset', run.stopped)
        self.assertNotIn('identity', run.flags)


if __name__ == '__main__':
    unittest.main()
