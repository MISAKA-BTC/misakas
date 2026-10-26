"""Checks for evidence classification; no timings or measurement PASS are produced."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("court_batch", Path(__file__).with_name("run_court_batch.py"))
batch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(batch)


class ClassificationTests(unittest.TestCase):
    def run_batch(self, *, allow=False, load=8.0, raw_status="UNDER_LOAD", child_exit=0):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder) / "result"
            args = ["batch", "--build-record", "unused.json", "--out", str(out),
                    "--case", "mldsa87_verify", "--max-wait-seconds", "0"]
            if allow:
                args.append("--allow-under-load")

            def child(command, **kwargs):
                child_out = Path(command[command.index("--out") + 1])
                child_out.mkdir()
                (child_out / "raw.json").write_text(json.dumps({"status": raw_status}))
                return subprocess.CompletedProcess(command, child_exit, "", "")

            with patch.object(batch.sys, "argv", args), patch.object(batch.os, "getloadavg", return_value=(load, load, load)), \
                    patch.object(batch.subprocess, "run", side_effect=child) as process:
                code = batch.main()
            return code, json.loads((out / "batch.json").read_text()), process.call_count

    def test_high_load_without_opt_in_runs_nothing(self):
        code, result, calls = self.run_batch()
        self.assertEqual((code, result["status"], calls), (75, "NOT_RUN_LOAD", 0))

    def test_successful_loaded_child_never_becomes_quiet_pass(self):
        code, result, calls = self.run_batch(allow=True)
        self.assertEqual((code, result["status"], calls), (0, "UNDER_LOAD_FIXTURE_ONLY", 1))
        self.assertFalse(result["arming_evidence"])

    def test_child_failure_stops_batch(self):
        code, result, calls = self.run_batch(allow=True, child_exit=1, raw_status="FAILED")
        self.assertEqual((code, result["status"], calls), (1, "PARTIAL", 1))

    def test_quiet_fixture_is_still_not_arming_evidence(self):
        code, result, calls = self.run_batch(load=2.0, raw_status="PASS")
        self.assertEqual((code, result["status"], calls), (0, "PASS_FIXTURE_ONLY", 1))
        self.assertFalse(result["arming_evidence"])


if __name__ == "__main__":
    unittest.main()
