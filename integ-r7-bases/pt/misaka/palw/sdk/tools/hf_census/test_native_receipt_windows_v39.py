"""Owning39 receipt contract: both windows keep every model and every bin.
Unrun test source for the consolidated validation phase."""
import copy
import unittest

from native_receipt_window_driver_v39 import validate_contract


def h(n: int) -> str:
    return f"{n:02x}" * 64


def master(n: int, member: str, start: int) -> dict:
    return {
        "member": member, "master": h(n), "sequence": 0, "requester_operator": h(90),
        "complete_source": h(n + 1), "manifest": h(n + 2), "pipeline": h(n + 3), "request": h(n + 4),
        "class_set": h(n + 5), "profile_set": h(n + 6), "program_set": h(n + 7), "inventory_set": h(12),
        "real_parameter_set": h(n + 8), "maximum_scope_set": h(n + 9), "maximum_resource_set": h(n + 10),
        "components": [{"stage": 0, "requester_operator": h(90), "producer_operator": h(91), "job_if_already_known": None}],
        "issuance_not_before": start, "issuance_deadline": start + 20,
    }


def contract() -> dict:
    windows = []
    for w in range(2):
        start = 100 + w * 100
        windows.append({"start_daa": start, "end_daa": start + 40, "issuance_bin_daa": 20, "minimum_commits_per_bin": 1,
                        "maximum_final_lag_daa": 20,
                        "intended": [master(20 * w + 1, "original", start), master(20 * w + 50, "original", start + 20)]})
    return {"schema": "misaka.rfc02.native-receipt-window-contract.v39", "cohort_sha256": "a" * 64,
            "receipt_window_daa": 40, "minimum_independent_operators": 2,
            "original_model_inventory_sets": {"original": h(12)},
            "original_native_policy": h(1), "original_final_policy": h(2), "original_queue_policy": h(3),
            "original_physical_service": h(4), "windows": windows}


class ContractV39(unittest.TestCase):
    def test_original_contract_is_accepted(self):
        validate_contract(contract())

    def test_a_window_cannot_drop_a_model_or_a_bin(self):
        c = contract()
        c["windows"][1]["intended"] = c["windows"][1]["intended"][:1]
        with self.assertRaises(ValueError):
            validate_contract(c)
        c = contract()
        c["original_model_inventory_sets"]["second"] = h(13)
        with self.assertRaises(ValueError):
            validate_contract(c)

    def test_a_known_job_cannot_repeat_and_masters_cannot_repeat(self):
        c = contract()
        c["windows"][1]["intended"][0]["master"] = c["windows"][0]["intended"][0]["master"]
        with self.assertRaises(ValueError):
            validate_contract(c)
        c = copy.deepcopy(contract())
        for w in c["windows"]:
            w["intended"][0]["components"][0]["job_if_already_known"] = h(77)
        with self.assertRaises(ValueError):
            validate_contract(c)


if __name__ == "__main__":
    unittest.main()
