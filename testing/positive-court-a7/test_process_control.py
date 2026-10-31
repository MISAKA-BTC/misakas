import os
import json
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from process_control import active, run_group


class ProcessGroups(unittest.TestCase):
    def run_case(self, code, timeout=2):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        path = Path(directory.name)
        log = path / "raw.log"
        result = run_group([sys.executable, "-u", "-c", code], cwd=path, env=os.environ.copy(), log=log, timeout=timeout)
        self.assertEqual(active(result["cleanup"]["group"]), [])
        before = log.read_bytes()
        time.sleep(0.15)
        self.assertEqual(log.read_bytes(), before)
        result['captured_output'] = before
        return result

    def test_normal_exit(self):
        self.assertEqual(self.run_case("print('done')")["exit_code"], 0)

    def test_timeout_kills_term_ignoring_parent_and_grandchild(self):
        child = "import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); exec('while True:\\n print(123,flush=True); time.sleep(.02)')"
        code = f"import signal,subprocess,sys,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); subprocess.Popen([sys.executable,'-u','-c',{child!r}]); time.sleep(60)"
        result = self.run_case(code, timeout=1)
        self.assertEqual(result["exit_code"], "TIMEOUT")
        self.assertIn(b'123', result['captured_output'])

    def test_successful_parent_must_not_leave_a_log_writer(self):
        child = "import time; exec('while True:\\n print(123,flush=True); time.sleep(.02)')"
        code = f"import subprocess,sys; subprocess.Popen([sys.executable,'-u','-c',{child!r}])"
        result = self.run_case(code)
        self.assertEqual(result["exit_code"], "DESCENDANT_LEAK")

    def interruption_case(self, signum, repeat=False):
        """Use real controller signals, not a mocked wait/cleanup function."""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)
            log = path / "raw.log"
            child = "import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); print('CHILD_READY',flush=True); exec('while True:\\n print(123,flush=True); time.sleep(.02)')"
            job = f"import os,signal,subprocess,sys,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); print('GROUP',os.getpid(),flush=True); subprocess.Popen([sys.executable,'-u','-c',{child!r}]); time.sleep(60)"
            module_path = str(Path(__file__).resolve().parent)
            controller = f"""
import json, os, signal, sys
from pathlib import Path
sys.path.insert(0, {module_path!r})
from process_control import ProcessInterrupted, run_group
previous = signal.getsignal(signal.SIGTERM)
try:
    run_group([sys.executable, '-u', '-c', {job!r}], cwd=Path.cwd(), env=os.environ.copy(), log=Path({str(log)!r}), timeout=30)
except ProcessInterrupted as error:
    print(json.dumps({{'signal': error.signum, 'cleanup': error.cleanup,
                      'handler_restored': signal.getsignal(signal.SIGTERM) == previous}}), flush=True)
else:
    raise SystemExit('controller incorrectly completed without cancellation')
"""
            process = subprocess.Popen([sys.executable, "-u", "-c", controller], cwd=path,
                                       stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            group = None
            try:
                until = time.monotonic() + 8
                while time.monotonic() < until:
                    data = log.read_bytes() if log.exists() else b""
                    if b"CHILD_READY" in data:
                        group = int(next(line.split()[1] for line in data.splitlines() if line.startswith(b"GROUP ")))
                        break
                    if process.poll() is not None:
                        self.fail(f"controller exited before child readiness: {process.communicate()}")
                    time.sleep(.02)
                self.assertIsNotNone(group, "grandchild must actually start before interruption")
                process.send_signal(signum)
                if repeat:
                    # The job ignores TERM, leaving the first cancellation in
                    # the TERM grace period when the second signal is sent.
                    time.sleep(.15)
                    process.send_signal(signum)
                stdout, stderr = process.communicate(timeout=8)
                self.assertEqual(process.returncode, 0, (stdout, stderr))
                result = json.loads(stdout)
                self.assertEqual(result["signal"], signum)
                self.assertTrue(result["handler_restored"])
                self.assertEqual(result["cleanup"]["live_descendants"], [])
                self.assertEqual(active(group), [])
                before = log.read_bytes()
                time.sleep(.15)
                self.assertEqual(log.read_bytes(), before)
            finally:
                # A failing regression must not itself leak the hostile job.
                if group is not None:
                    try:
                        os.killpg(group, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                if process.poll() is None:
                    process.kill()
                process.communicate(timeout=2)

    def test_sigterm_cancels_the_group_and_restores_handlers(self):
        self.interruption_case(signal.SIGTERM)

    def test_repeated_sigint_cannot_interrupt_group_cleanup(self):
        self.interruption_case(signal.SIGINT, repeat=True)


if __name__ == "__main__":
    unittest.main()
