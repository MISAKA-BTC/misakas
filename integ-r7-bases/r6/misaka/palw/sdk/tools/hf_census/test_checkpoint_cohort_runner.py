"""Scope and artifact regressions for the new runner; execution is deferred."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest

import acceptance_cohort as ac
import checkpoint_cohort_runner as runner


class FullScopeGuards(unittest.TestCase):
    def setup_result(self, root):
        payload = root / "artifact.bin"
        payload.write_bytes(b"authentic input fixture")
        member = {"id": "m", "source_declared_context_max": 32768,
                  "required_task_components": ["text", "vision", "video"]}
        scope = {"repository": "publisher/model", "revision": "a" * 40, "context_max": 32768}
        result = {"schema": runner.RESULT_SCHEMA, "status": "PASS", "stage": "fidelity", "member_id": "m",
                  "run_plan_sha256": "b" * 64, "scope": scope, "actual_context_max": 32768,
                  "full_advertised_task": True, "completed_task_components": ["text", "vision", "video"],
                  "artifacts": [{"path": "artifact.bin", "sha256": ac.digest(payload)}],
                  "metrics": {"teacher_forced_top1":1.0,"mean_kl":0,"logit_correlation":1.0,
                              "generated_token_agreement":1.0,"exact_generated_job_fraction":1.0,
                              "source_task_success":1.0,"integer_task_success":1.0,
                              "teacher_forced_positions":32768,"generated_tokens":32,"complete_jobs":3}}
        path = root / "result.json"
        path.write_text(json.dumps(result))
        return path, member, scope, result

    def test_short_context_missing_video_and_modified_file_remain_failures(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path, member, scope, result = self.setup_result(root)
            inspect = lambda: runner.inspect_result(path, root, member, scope, "fidelity", "b" * 64)[0]
            self.assertTrue(inspect())
            result["actual_context_max"] = 512
            path.write_text(json.dumps(result))
            self.assertFalse(inspect())
            result["actual_context_max"] = 32768
            result["completed_task_components"] = ["text", "vision"]
            path.write_text(json.dumps(result))
            self.assertFalse(inspect())
            result["completed_task_components"].append("video")
            path.write_text(json.dumps(result))
            (root / "artifact.bin").write_bytes(b"a different claim")
            self.assertFalse(inspect())

    def test_producer_supplied_chain_audit_cannot_replace_separate_audit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            path, member, scope, result = self.setup_result(root)
            result["stage"] = "final"
            fake = root / "audit.json"
            fake.write_text('{"final_authenticated":true}')
            result["independent_native_audit"] = {"path": fake.name, "sha256": ac.digest(fake)}
            path.write_text(json.dumps(result))
            good, reason = runner.inspect_result(path, root, member, scope, "final", "b" * 64)
            self.assertFalse(good)
            self.assertEqual(reason, "MISSING_INDEPENDENT_NATIVE_AUDIT")

    def test_source_pins_do_not_accept_missing_weights_or_partial_payload(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            payload = b"whole model bytes"
            member = {"source_directory": tmp, "source_files": [{"path": "weights.bin", "bytes": len(payload),
                      "sha256": hashlib.sha256(payload).hexdigest()}]}
            self.assertFalse(runner.source_pins_ok(member))
            (root / "weights.bin").write_bytes(payload[:-1])
            self.assertFalse(runner.source_pins_ok(member))
            (root / "weights.bin").write_bytes(payload)
            self.assertTrue(runner.source_pins_ok(member))

    def test_plan_cannot_substitute_population_or_lower_target(self):
        cohort = {"schema": ac.SCHEMA, "design": "SRS_WITHOUT_REPLACEMENT", "N": 100, "n": 1,
                  "target_share": .9, "required_stages": list(ac.STAGES), "members": [{"id": "one"}]}
        plan = {"schema": runner.SCHEMA, "cohort_sha256": "c" * 64, "target_share": .9,
                "required_stages": list(ac.STAGES), "execution_policy": "full-original-context-and-all-advertised-components;no-substitution;missing-fails",
                "members": [{"id": "one", "stages": {}}]}
        runner.validate_plan(cohort, plan, "c" * 64)
        plan["target_share"] = .5
        with self.assertRaises(ValueError):
            runner.validate_plan(cohort, plan, "c" * 64)
        plan["target_share"] = .9
        plan["members"] = [{"id": "other", "stages": {}}]
        with self.assertRaises(ValueError):
            runner.validate_plan(cohort, plan, "c" * 64)

    def test_original_source_resolution_cannot_replace_unknown_or_changed_revision(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            metadata = root / "config.json"
            metadata.write_text('{"max_position_embeddings":32768}')
            resolution = root / "resolution.json"
            member = {"id":"m", "repository":"publisher/model", "revision":"a"*40, "advertised_task":"text-generation"}
            document = {"schema":"misaka.rfc02.original-source-resolution.v1", "repository":member["repository"],
                        "revision":member["revision"], "advertised_task":"text-generation", "source_declared_context_max":32768,
                        "required_task_components":["text"], "source_directory":tmp,
                        "source_files":[{"path":"config.json","bytes":metadata.stat().st_size,"sha256":ac.digest(metadata)}],
                        "publisher_metadata_files":[{"path":"config.json","sha256":ac.digest(metadata)}]}
            resolution.write_text(json.dumps(document))
            entry = {"original_source_resolution":{"path":"resolution.json","sha256":ac.digest(resolution)}}
            actual, _ = runner.resolve_original_member(member,entry,root)
            self.assertEqual(actual["source_declared_context_max"],32768)
            unknown, evidence = runner.resolve_original_member({**member,"revision":None},entry,root)
            self.assertNotIn("source_declared_context_max",unknown)
            self.assertIsNone(evidence)
            with self.assertRaises(ValueError):
                runner.resolve_original_member({**member,"revision":"b"*40},entry,root)

    def test_audit_pair_requires_two_exporters_and_exact_native_utxo_anchor(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            pair = root / "pair.json"
            pair.write_text(json.dumps({"schema":runner.AUDIT_PAIR_SCHEMA,"exports":[]}))
            good, reason = runner.inspect_audit_pair(pair,root,{},"redemption","a"*64)
            self.assertFalse(good)
            self.assertEqual(reason,"TWO_INDEPENDENT_EXPORTS_REQUIRED")
            auditors = [{"operator_id":"same", "node_database":"same"}] * 2
            with self.assertRaises(ValueError):
                runner.validate_auditors(auditors,{"network":"devnet","genesis":"b"*128})


if __name__ == "__main__":
    unittest.main()
