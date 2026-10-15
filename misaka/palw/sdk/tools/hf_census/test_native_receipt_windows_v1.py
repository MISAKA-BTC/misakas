"""A6 regression sources; execute only in the coordinated validation phase."""
from copy import deepcopy
from pathlib import Path
import tempfile
import unittest
import acceptance_cohort as ac
import checkpoint_cohort_runner as runner
from native_receipt_window_driver_v1 import validate_contract


def frozen_contract():
    root = "1" * 128
    fields = ("class", "source", "inventory", "real_parameter", "consent", "maximum_contract", "maximum_scope",
              "maximum_resource", "queue_plan", "queue_policy", "source_plan", "physical_service", "requester_operator")
    windows = []
    for w in range(2):
        start = 100 + w * 100
        jobs = []
        for b in range(2):
            j = {k: root for k in fields}
            j.update(member="m", job=f"{1+w*2+b:0128x}", funding=None,
                     producer_operator=f"{10+b:0128x}", nonce="0" * 128,
                     issuance_not_before=start+b*20, issuance_deadline=start+(b+1)*20)
            jobs.append(j)
        windows.append({"start_daa": start, "end_daa": start+40, "issuance_bin_daa": 20,
                        "minimum_commits_per_bin": 1, "maximum_final_lag_daa": 20, "intended": jobs})
    return {"schema": "misaka.rfc02.native-receipt-window-contract.v1", "cohort_sha256": "a" * 64,
            "original_model_roots": {"m": root}, "receipt_window_daa": 40,
            "minimum_independent_operators": 2, "windows": windows}


class ReceiptWindowGuards(unittest.TestCase):
    def test_future_admissions_and_original_zero_nonce_are_valid_intent(self):
        validate_contract(frozen_contract())

    def test_one_burst_cannot_replace_frozen_sustained_issuance(self):
        contract = frozen_contract()
        for j in contract["windows"][0]["intended"]:
            j["issuance_not_before"], j["issuance_deadline"] = 100, 120
        with self.assertRaisesRegex(ValueError, "cadence bin"):
            validate_contract(contract)

    def test_source_substitution_and_reused_job_remain_denominator_failures(self):
        for change in ("inventory", "job"):
            contract = frozen_contract()
            j = contract["windows"][1]["intended"][0]
            j[change] = "2" * 128 if change == "inventory" else contract["windows"][0]["intended"][0]["job"]
            with self.assertRaises(ValueError):
                validate_contract(contract)

    def test_no_unbounded_final_stall_can_be_called_a_receipt_window(self):
        contract = frozen_contract()
        contract["windows"][0]["maximum_final_lag_daa"] = 10000
        with self.assertRaises(ValueError):
            validate_contract(contract)

    def test_two_bool_only_exporters_cannot_mint_receipt_credit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            artifact = root / "opaque.bin"
            artifact.write_bytes(b"not a native proof")
            exports = []
            for i in range(2):
                doc = {"schema": runner.CHAIN_SCHEMA, "stage": "receipt-windows", "run_plan_sha256": "b" * 64,
                       "operator_id": f"operator-{i}", "node_database": f"db-{i}", "genesis": "g", "network": "devnet",
                       "anchor_from_validated_consensus_database": True, "canonical_blocks_verified": True,
                       "qualified_native_profile": True, "isolated_calibration": False,
                       "intended_claims_and_queue_accounting_verified": True,
                       "chain_objects": [{"path": artifact.name, "sha256": ac.digest(artifact)}]}
                p = root / f"audit-{i}.json"
                ac.write(p, doc)
                exports.append({"path": p.name, "sha256": ac.digest(p), **{k: doc[k] for k in ("operator_id", "node_database", "genesis", "network")}})
            pair = root / "pair.json"
            ac.write(pair, {"schema": runner.AUDIT_PAIR_SCHEMA, "exports": exports})
            with self.assertRaisesRegex(ValueError, "PROOF_REPORT_MISSING"):
                runner.inspect_receipt_audit_pair(pair, root, frozen_contract(), "b" * 64)


if __name__ == "__main__":
    unittest.main()
