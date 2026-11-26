"""Arming records must cover every command, including multi-command suites."""
import unittest
import pathlib
import subprocess
import sys
import tempfile
from run_arming_evidence import run_outcome


def summary(passed, failed=0):
    return f'test result: {"FAILED" if failed else "ok"}. {passed} passed; {failed} failed; 0 ignored\n'


class OutcomeTests(unittest.TestCase):
    def test_historical_evidence_directory_is_not_an_output(self):
        script = pathlib.Path(__file__).with_name('run_arming_evidence.py')
        historical = script.parent / 'arming-runs-17400'
        with tempfile.TemporaryDirectory() as tmp:
            alias = pathlib.Path(tmp) / 'evidence'
            alias.symlink_to(historical, target_is_directory=True)
            for target in (historical, alias / 'nested'):
                result = subprocess.run([sys.executable, str(script), '--out', str(target), '--no-warm'],
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 2)
                self.assertIn('historical 17,400 evidence is immutable', result.stderr)
                self.assertFalse((historical / 'nested').exists())

    def test_empty_part_never_borrows_another_parts_success(self):
        self.assertFalse(run_outcome([(0, summary(5)), (0, summary(0))])[3])
        self.assertFalse(run_outcome([])[3])

    def test_unfinished_or_crashed_part_cannot_pin(self):
        self.assertFalse(run_outcome([(0, summary(5)), (0, 'running 1 test')])[3])
        self.assertFalse(run_outcome([(101, summary(5))])[3])
        self.assertFalse(run_outcome([(0, summary(1, 1))])[3])

    def test_nested_subprocess_summaries_are_not_double_counted(self):
        self.assertEqual(run_outcome([(0, summary(1) + summary(1) + summary(5)),
                                      (0, summary(3))]), (0, 8, 0, True))


if __name__ == '__main__':
    unittest.main()
