import os
from pathlib import Path
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
        return result

    def test_normal_exit(self):
        self.assertEqual(self.run_case("print('done')")["exit_code"], 0)

    def test_timeout_kills_term_ignoring_parent_and_grandchild(self):
        child = "import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); exec('while True:\\n print(123,flush=True); time.sleep(.02)')"
        code = f"import signal,subprocess,sys,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); subprocess.Popen([sys.executable,'-u','-c',{child!r}]); time.sleep(60)"
        self.assertEqual(self.run_case(code, timeout=0.5)["exit_code"], "TIMEOUT")

    def test_successful_parent_must_not_leave_a_log_writer(self):
        child = "import time; exec('while True:\\n print(123,flush=True); time.sleep(.02)')"
        code = f"import subprocess,sys; subprocess.Popen([sys.executable,'-u','-c',{child!r}])"
        self.assertEqual(self.run_case(code)["exit_code"], "DESCENDANT_LEAK")


if __name__ == "__main__":
    unittest.main()
