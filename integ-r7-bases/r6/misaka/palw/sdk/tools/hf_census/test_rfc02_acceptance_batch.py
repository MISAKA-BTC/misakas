"""Premeasurement orchestration regressions. Run in the final verification batch."""
import json
from pathlib import Path
import tempfile
import unittest
import acceptance_cohort as ac
import rfc02_acceptance_batch as batch


class FrozenBatch(unittest.TestCase):
    def test_unknown_members_are_retained_in_prepared_plan(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cohort = {"schema": ac.SCHEMA, "population": "D_all", "design": "SRS_WITHOUT_REPLACEMENT",
                      "N": 3117871, "n": 2, "target_share": .90, "required_stages": list(ac.STAGES),
                      "members": [{"id": "known", "repository": "known", "revision": "a" * 40},
                                  {"id": "unknown", "repository": "unknown", "revision": None}]}
            c, commands, out = root / "cohort.json", root / "commands.json", root / "plan.json"
            ac.write(c, cohort)
            ac.write(commands, {"members": {"known": {"stages": {}}}})
            result = batch.make_plan(c, commands, out, "devnet", "b" * 128)
            plan = json.loads(out.read_text())
            self.assertEqual(result["members"], 2)
            self.assertEqual([m["id"] for m in plan["members"]], ["known", "unknown"])
            self.assertEqual(plan["members"][1]["stages"], {})
            self.assertTrue(result["premeasurement_commit_required"])
            self.assertFalse(result["measurement_or_eligibility"])

    def test_engineering_panel_cannot_fill_a8_or_a6(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cohort = {"schema": ac.SCHEMA, "population": "D_all", "design": "ENGINEERING_PANEL",
                      "N": 1, "n": 1, "target_share": .90, "required_stages": list(ac.STAGES),
                      "members": [{"id": "m", "repository": "m", "revision": None}]}
            c, commands, plan = root / "c.json", root / "commands.json", root / "plan.json"
            ac.write(c, cohort)
            ac.write(commands, {"members": {}})
            batch.make_plan(c, commands, plan, "devnet", "b" * 128)
            entry = {"name": "engineering", "cohort": c.name, "cohort_sha256": ac.digest(c),
                     "plan": plan.name, "committed_plan_sha256": ac.digest(plan)}
            for gate in ("A6", "A8"):
                document = {"schema": batch.SCHEMA, "target_share": .90,
                            "failure_policy": "all-frozen-members;unknown-and-nonresponse-fail;no-substitution",
                            "cohorts": [{**entry, "completion_gate": gate}]}
                with self.assertRaises(ValueError):
                    batch.validate(document, root)


if __name__ == "__main__":
    unittest.main()
