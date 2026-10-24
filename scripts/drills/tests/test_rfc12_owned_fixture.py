"""Real owned-process lifecycle controls; never RFC12 node/campaign acceptance."""
import importlib.util
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from rfc12_owned_fixture import start_fixture, stop_fixture

spec = importlib.util.spec_from_file_location('rfc12_owned_fixture_drill',
    Path(__file__).resolve().parents[1] / 'misaka-rfc12-multiprocess.py')
drill = importlib.util.module_from_spec(spec)
spec.loader.exec_module(drill)


@unittest.skipUnless(os.name == 'posix', 'owned process groups require POSIX')
class OwnedFixture(unittest.TestCase):
    def check_descendant_cleanup(self, leader_exits):
        code = '''import os, time
pid = os.fork()
if pid == 0:
    print('descendant-ready', flush=True)
    time.sleep(60)
else:
    %s
''' % ('os._exit(0)' if leader_exits else 'time.sleep(60)')
        child = start_fixture([sys.executable, '-c', code], subprocess.PIPE, subprocess.DEVNULL)
        try:
            self.assertTrue(select.select([child.stdout], [], [], 5)[0], 'descendant failed to start')
            self.assertEqual(child.stdout.readline(), 'descendant-ready\n')
            if not leader_exits:
                self.assertEqual(os.getpgid(child.pid), child.pid)
            if leader_exits:
                self.assertEqual(child.wait(timeout=5), 0)
            stop_fixture(child)
            # Both leader and descendant held this writer. EOF demonstrates that
            # cleanup did not merely reap the leader while leaving its child alive.
            self.assertTrue(select.select([child.stdout], [], [], 5)[0], 'descendant retained the output descriptor')
            self.assertEqual(child.stdout.read(), '')
        finally:
            stop_fixture(child)
            child.stdout.close()

    def test_live_fixture_and_its_descendant_are_stopped(self):
        self.check_descendant_cleanup(False)

    def test_exited_leader_does_not_hide_a_live_descendant(self):
        self.check_descendant_cleanup(True)

    def test_only_the_fixture_group_is_signalled(self):
        peer = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(60)'])
        child = start_fixture([sys.executable, '-c', 'import time; time.sleep(60)'], subprocess.DEVNULL, subprocess.DEVNULL)
        try:
            stop_fixture(child)
            self.assertIsNone(peer.poll())
        finally:
            stop_fixture(child)
            peer.kill()
            peer.wait(timeout=5)

    def instance(self, root, refuse_registration=False):
        class Sampler:
            child = None
            def check(self):
                pass
            def register(self, name, child):
                self.child = child
                if refuse_registration:
                    raise RuntimeError('sampler registration refused')
        instance = drill.MeasuredDrill.__new__(drill.MeasuredDrill)
        instance.root = Path(root)
        instance.plan = dict(command_timeout_s=5)
        instance.deadline = time.monotonic() + 10
        instance.limits = dict(maximumFixtureOutputBytes=1024)
        instance.sampler = Sampler()
        instance.command_count, instance.command_observations, instance.command_outputs = 0, [], []
        return instance

    def test_registration_failure_reaps_the_already_launched_fixture(self):
        with tempfile.TemporaryDirectory() as folder:
            instance = self.instance(folder, refuse_registration=True)
            with self.assertRaisesRegex(RuntimeError, 'registration refused'):
                instance.command([sys.executable, '-c', 'import time; time.sleep(60)'])
            self.assertIsNotNone(instance.sampler.child.poll())
            self.assertTrue(instance.command_observations[0]['cleanupSucceeded'])
            self.assertEqual(instance.command_observations[0]['cleanupScope'], 'owned-posix-process-group')

    def test_deadline_failure_preserves_output_and_reaps_the_fixture(self):
        with tempfile.TemporaryDirectory() as folder:
            instance = self.instance(folder)
            with self.assertRaisesRegex(drill.Blocked, 'deadline'):
                instance.command([sys.executable, '-c', "import time; print('started', flush=True); time.sleep(60)"], timeout=.3)
            self.assertIsNotNone(instance.sampler.child.poll())
            row = instance.command_observations[0]
            self.assertTrue(row['cleanupSucceeded'])
            self.assertEqual(Path(row['stdout']).read_text(), 'started\n')

    def test_finite_success_and_nonzero_exit_both_record_cleanup(self):
        with tempfile.TemporaryDirectory() as folder:
            instance = self.instance(folder)
            self.assertEqual(instance.command([sys.executable, '-c', 'pass']), '')
            with self.assertRaisesRegex(drill.Blocked, 'exited 7'):
                instance.command([sys.executable, '-c', 'raise SystemExit(7)'])
            self.assertEqual([row['returnCode'] for row in instance.command_observations], [0, 7])
            self.assertTrue(all(row['cleanupSucceeded'] for row in instance.command_observations))


if __name__ == '__main__':
    unittest.main()
