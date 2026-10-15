"""Launch policy regressions; execute only in the consolidated validation phase."""
import copy
import json
from pathlib import Path
import unittest

import acceptance_cohort as ac
import initial_model_launch_v1 as launch


def target():
    path = Path(__file__).resolve().parents[5] / "docs/design/palw/tir/launch/initial-seven-models-v1.json"
    return json.loads(path.read_text())


class SevenModelPolicy(unittest.TestCase):
    def test_moe_cannot_replace_dense32_and_mistral_cannot_drop_vision(self):
        original = target()
        launch.validate_target(original)
        altered = copy.deepcopy(original)
        altered["models"] = [m for m in altered["models"] if m["id"] != "dense-32b"]
        with self.assertRaises(ValueError):
            launch.validate_target(altered)
        altered = copy.deepcopy(original)
        next(m for m in altered["models"] if m["id"] == "dense-24b")["required_components"] = ["decoder"]
        with self.assertRaises(ValueError):
            launch.validate_target(altered)

    def test_launch_is_all_required_and_missing_native_identity_fails(self):
        doc = target()
        records, observations = [], []
        for index, model in enumerate(doc["models"]):
            pin = {k: f"{index + 1:0128x}" for k in
                   ("native_class", "native_source", "program_root", "inventory_root", "real_parameter_root")}
            model["complete_artifact_pin"] = pin
            records.append({"id": model["id"], "final_capable": True, "first_blocker": "PASS",
                            "stages": {s: "PASS" for s in ac.STAGES}})
            observations.append({"id": model["id"], "launch_native_anchor": pin})
        records[-1]["final_capable"] = False
        records[-1]["first_blocker"] = "MOE_REFERENCE_UNVERIFIED"
        result = launch.launch_outcome(doc, records, observations, True)
        self.assertEqual(result["passed_models"], 6)
        self.assertFalse(result["all_seven_evidence_passed"])
        records[-1]["final_capable"] = True
        observations[-1].pop("launch_native_anchor")
        result = launch.launch_outcome(doc, records, observations, True)
        self.assertFalse(result["all_seven_evidence_passed"])
        self.assertEqual(result["models"][-1]["blocking"], "NATIVE_ARTIFACT_IDENTITY_UNPROVEN")
        self.assertFalse(result["consensus_activation_performed"])
        self.assertFalse(result["A6_or_A8_proven_by_launch_set"])


if __name__ == "__main__":
    unittest.main()
