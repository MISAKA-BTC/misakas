"""Launch policy regressions; execute only in the consolidated validation phase."""
import copy
import json
from pathlib import Path
import unittest
import tempfile

import acceptance_cohort as ac
import initial_model_launch_v1 as launch


def target():
    path = Path(__file__).resolve().parents[5] / "docs/design/palw/tir/launch/initial-seven-models-v1.json"
    return json.loads(path.read_text())


class SevenModelPolicy(unittest.TestCase):
    def test_export_flags_without_linked_offline_proof_are_not_an_identity(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            audit = {"complete_model_identity": {"native_class": "a" * 128},
                     "qualified_native_profile": True, "final_authenticated": True}
            self.assertIsNone(launch.proof_derived_identity(audit, root))
            report = {"schema": ac.NATIVE_AUDIT_SCHEMA, "namespace": 29,
                      "pinned_headers_and_original_rows_verified": True,
                      "qualified_native_profile": True, "final_receipt_class": "qualified-final",
                      "utxo_redemption_verified": True, "local_qualified_validation_network": True,
                      **{k: "a" * 128 for k in ("class", "source", "job", "program_root", "inventory", "real_parameter_root")}}
            path = root / "local-qualified.json"
            ac.write(path, report)
            evidence = {"path": path.name, "sha256": ac.digest(path)}
            audit.update(complete_model_identity_evidence=evidence, chain_objects=[evidence],
                         validated_anchor={k: report[k] for k in ("class", "source", "job")})
            self.assertIsNone(launch.proof_derived_identity(audit, root), "local devnet validation cannot grant shipping initial rights")

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
            if model["id"] == "dense-24b":
                pin = {"namespace": 32, **{k: f"{index + 1:0128x}" for k in launch.MASTER_PIN_FIELDS}}
                model["native_master_pin"] = pin
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

    def test_multimodal_identity_cannot_be_replaced_by_a_token_only_final(self):
        model = next(m for m in target()["models"] if m["id"] == "dense-24b")
        model["complete_artifact_pin"] = {k: "a" * 128 for k in
            ("native_class", "native_source", "program_root", "inventory_root", "real_parameter_root")}
        self.assertIsNone(launch.expected_identity(model))
        model["native_master_pin"] = {"namespace": 32, **{k: "b" * 128 for k in launch.MASTER_PIN_FIELDS}}
        self.assertEqual(launch.expected_identity(model), model["native_master_pin"])
        model["native_master_pin"]["namespace"] = 28
        self.assertIsNone(launch.expected_identity(model))

    def test_master_identity_requires_each_original_component_redemption(self):
        # These are policy fixtures, not proof-derived Final evidence. The
        # actual runner independently invokes the native proof verifier first.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binding = {"namespace": 32, **{k: "a" * 128 for k in launch.MASTER_PIN_FIELDS}}
            mapping = {"class": "class", "source": "source", "job": "job", "program_root": "program",
                       "inventory": "inventory", "real_parameter_root": "real_parameter", "original_funding": "original_funding",
                       "initial_consent": "initial_consent", "original_maximum_scope": "maximum_scope",
                       "original_maximum_resource": "maximum_resource", "claim": "claim", "final_book": "final_book",
                       "single_credit": "single_credit", "requester_operator": "requester_operator", "producer_operator": "producer_operator"}
            component = {k: f"{index + 1:0128x}" for index, k in enumerate(mapping.values())}
            component.update(stage=0, reward=123)
            report = {"schema": "misaka.palw.pinned-native-master-execution-audit.v32", "namespace": 32,
                      "network": "testnet-999", "genesis": "b" * 128, "pinned_headers_and_original_rows_verified": True,
                      "all_original_component_finals_and_credits_verified": True, "qualified_native_profile": True,
                      "final_receipt_class": "qualified-final", "local_qualified_validation_network": False,
                      "master_mints_extra_credit": False, "complete_model_binding": binding, "components": [component]}
            receipt = {"schema": ac.NATIVE_AUDIT_SCHEMA, "namespace": 28, "network": report["network"], "genesis": report["genesis"],
                       "pinned_headers_and_original_rows_verified": True, "qualified_native_profile": True,
                       "final_receipt_class": "qualified-final", "local_qualified_validation_network": False,
                       "utxo_redemption_verified": True, "producer_credit": component["reward"],
                       "coinbase_delivery": {"real_coinbase_inclusion_and_original_queue_consumption_verified": True,
                           "accepted_spend": {"accepted_spend_of_original_outpoint_verified": True}},
                       **{key: component[value] for key, value in mapping.items()}}
            master_file, receipt_file = root / "master.json", root / "component.json"
            ac.write(master_file, report)
            ac.write(receipt_file, receipt)
            master_ref = {"path": master_file.name, "sha256": ac.digest(master_file)}
            receipt_ref = {"path": receipt_file.name, "sha256": ac.digest(receipt_file)}
            audit = {"complete_model_identity_evidence": master_ref, "complete_model_binding_evidence": master_ref,
                     "complete_model_binding": binding, "chain_objects": [master_ref, receipt_ref]}
            self.assertIsNone(launch.proof_derived_identity(audit, root), "aggregate flags cannot replace redemption")
            audit["component_redemption_evidence"] = [receipt_ref]
            self.assertEqual(launch.proof_derived_identity(audit, root), binding)
            receipt["original_funding"] = "f" * 128
            ac.write(receipt_file, receipt)
            receipt_ref["sha256"] = ac.digest(receipt_file)
            self.assertIsNone(launch.proof_derived_identity(audit, root), "a different funded job cannot donate a credit")

    def test_both_launch_windows_must_exercise_each_original_complete_model(self):
        models = launch.validate_target(target())
        roots, intended, masters = {}, [], []
        for index, (key, model) in enumerate(models.items()):
            value = f"{index + 1:0128x}"
            if key == "dense-24b":
                identity = {"namespace": 32, **{field: value for field in launch.MASTER_PIN_FIELDS}}
                model["native_master_pin"] = identity
                roots[key] = identity["complete_inventory"]
                masters.append({"member": key, **{field: identity[field] for field in
                    ("complete_source", "manifest", "complete_inventory", "program_set", "real_parameter_set")}})
            else:
                model["complete_artifact_pin"] = {field: value for field in
                    ("native_class", "native_source", "inventory_root", "program_root", "real_parameter_root")}
                roots[key] = value
                intended.append({"member": key, **{field: value for field in ("class", "source", "inventory", "real_parameter")}})
        contract = {"original_model_roots": roots, "windows": [
            {"intended": copy.deepcopy(intended), "masters": copy.deepcopy(masters)} for _ in range(2)]}
        launch.validate_launch_receipt_scope(models, contract)
        removed = contract["windows"][1]["intended"].pop()
        with self.assertRaises(ValueError):
            launch.validate_launch_receipt_scope(models, contract)
        contract["windows"][1]["intended"].append(removed)
        contract["windows"][0]["masters"][0]["real_parameter_set"] = "f" * 128
        with self.assertRaises(ValueError):
            launch.validate_launch_receipt_scope(models, contract)


if __name__ == "__main__":
    unittest.main()
