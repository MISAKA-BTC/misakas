"""Regression checks against scope laundering and denominator substitution."""
import copy
import gzip
import json
from pathlib import Path
import tempfile
import unittest

import acceptance_cohort as ac


class Acceptance(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        (self.root / "chain.json").write_text('{}\n')
        self.evidence = {"path": "chain.json", "sha256": ac.digest(self.root / "chain.json")}
        self.receipt = self.native_receipt("qualified.json", qualified=True)
        self.doc = {"schema": ac.SCHEMA, "population": "D_all", "design": "SRS_WITHOUT_REPLACEMENT",
                    "N": 10000, "n": 32, "target_share": .90, "required_stages": list(ac.STAGES),
                    "frame_complete": True, "members": [{"id": f"o/{i}", "repository": f"o/{i}",
                    "revision": "a" * 40, "advertised_task": "text-generation"} for i in range(32)]}
        self.cohort = self.root / "cohort.json"
        self.observations = self.root / "rows.jsonl"
        self.panel = self.root / "panel.json"
        self.windows = {"minimum_independent_operators": 3, "receipt_window_daa": 100, "windows": [
            {"start_daa": start, "end_daa": start + 100, "independent_operators": 3,
             "intended_claims": 10, "issued_claims": 10, "finalized_claims": 10,
             "queue_start": 0, "queue_end": 0, "unaccounted_drops": 0, "evidence": self.evidence}
            for start in (0, 100)]}

    def native_receipt(self, name, qualified, spend=True):
        doc = {"schema": ac.NATIVE_AUDIT_SCHEMA, "job": "ab" * 64, "pinned_headers_and_original_rows_verified": True,
               "qualified_native_profile": qualified, "calibration_receipts_are_not_A6_or_A8_evidence": not qualified,
               "final_receipt_class": "qualified-final" if qualified else "measurement-final",
               "utxo_redemption_verified": spend,
               "coinbase_delivery": {"real_coinbase_inclusion_and_original_queue_consumption_verified": True,
                                     "accepted_spend": {"accepted_spend_of_original_outpoint_verified": True} if spend else None}}
        (self.root / name).write_text(json.dumps(doc))
        return {"path": name, "sha256": ac.digest(self.root / name)}

    def save(self):
        self.cohort.write_text(json.dumps(self.doc))
        self.panel.write_text(json.dumps(self.windows))
        rows = []
        for m in self.doc["members"]:
            scope = {k: "bound" for k in ac.SCOPE}
            scope.update(repository=m["repository"], revision=m["revision"], complete_task=m["advertised_task"] or "text-generation", context_max=32768)
            rows.append({"id": m["id"], "cohort_sha256": ac.digest(self.cohort), "scope": scope,
                         "source_declared_context_max": 32768, "real_size": True, "full_advertised_task": True,
                         "active_shipping_kernel": True, "public_chain": True,
                         "stages": {s: {"status": "PASS", "scope": copy.deepcopy(scope),
                                        "evidence": self.receipt if s in ("final", "redemption") else self.evidence}
                                    for s in ac.STAGES}})
        self.observations.write_text("".join(json.dumps(r) + "\n" for r in rows))
        return rows

    def result(self):
        return ac.report(self.cohort, self.observations, self.root, self.panel)

    def test_all_scoped_evidence_and_two_windows(self):
        self.save()
        r = self.result()
        self.assertTrue(r["A6_satisfied"])
        self.assertFalse(r["A8_satisfied"])
        self.assertGreaterEqual(r["rates"]["final_capable"]["lb95"], .90)

    def test_measurement_final_and_unspent_delivery_do_not_count(self):
        rows = self.save()
        rows[0]["stages"]["final"]["evidence"] = self.native_receipt("calibration.json", qualified=False)
        rows[1]["stages"]["redemption"]["evidence"] = self.native_receipt("unspent.json", qualified=True, spend=False)
        rows[2]["stages"]["final"]["evidence"] = self.evidence
        self.observations.write_text("".join(json.dumps(r) + "\n" for r in rows))
        r = self.result()
        self.assertEqual(r["rates"]["final_capable"]["successes"], 29)
        self.assertEqual(r["residual_table"].get("MEASUREMENT_FINAL_ONLY"), 1)
        self.assertEqual(r["residual_table"].get("REDEMPTION_NOT_SPEND_VERIFIED"), 1)
        self.assertEqual(r["residual_table"].get("UNAUTHENTICATED_NATIVE_RECEIPT"), 1)

    def test_missing_rows_are_failure_not_smaller_denominator(self):
        rows = self.save()
        self.observations.write_text(json.dumps(rows[0]) + "\n")
        r = self.result()
        self.assertEqual(r["rates"]["final_capable"]["sample_denominator"], 32)
        self.assertEqual(r["missing_observations_count_as_failures"], 31)
        self.assertFalse(r["A6_satisfied"])

    def test_stage_hash_and_scope_cannot_be_reused(self):
        rows = self.save()
        rows[0]["stages"]["final"]["scope"]["context_max"] = 128
        rows[1]["stages"]["fidelity"]["evidence"] = {"path": "chain.json", "sha256": "0" * 64}
        self.observations.write_text("".join(json.dumps(r) + "\n" for r in rows))
        r = self.result()
        self.assertEqual(r["rates"]["final_capable"]["successes"], 30)

    def test_reduced_context_and_inactive_kernels_do_not_count(self):
        rows = self.save()
        rows[0]["scope"]["context_max"] = 128
        rows[1]["active_shipping_kernel"] = False
        rows[2]["full_advertised_task"] = False
        self.observations.write_text("".join(json.dumps(r) + "\n" for r in rows))
        self.assertEqual(self.result()["rates"]["final_capable"]["successes"], 29)

    def test_rolling_overlap_stalled_rate_and_queue_growth(self):
        self.save()
        for patch in ({"start_daa": 50}, {"finalized_claims": 9}, {"queue_end": 1}, {"unaccounted_drops": 1}):
            windows = copy.deepcopy(self.windows)
            windows["windows"][1].update(patch)
            self.assertFalse(ac.panel_ok(windows, self.root))

    def test_engineering_panel_never_becomes_population_coverage(self):
        self.doc["design"] = "ENGINEERING_PANEL"
        self.save()
        self.assertFalse(self.result()["A6_satisfied"])

    def test_a8_requires_complete_files_and_catalogues(self):
        self.doc["population"] = "L_files"
        self.save()
        self.assertFalse(self.result()["A8_satisfied"])
        self.doc.update(frame_eligibility="complete_public_weights_and_full_task_metadata",
                        digest_deduplication_complete=True, catalogues=["huggingface", "ollama"])
        self.save()
        self.assertTrue(self.result()["A8_satisfied"])

    def test_duplicate_or_replaced_members_are_rejected(self):
        rows = self.save()
        for row in (rows[0], {**rows[0], "id": "replacement"}):
            self.observations.write_text("".join(json.dumps(r) + "\n" for r in rows + [row]))
            with self.assertRaises(ValueError):
                self.result()

    def test_a6_original_confidence_and_current_share_are_distinct(self):
        rows = self.save()
        for i in range(3):
            rows[i]["active_shipping_kernel"] = False
        self.observations.write_text("".join(json.dumps(r) + "\n" for r in rows))
        result = self.result()
        self.assertTrue(result["A6_satisfied"])
        self.assertGreaterEqual(result["rates"]["final_capable"]["share"], .90)
        self.assertLess(result["rates"]["final_capable"]["lb95"], .90)
        rows[3]["active_shipping_kernel"] = False
        self.observations.write_text("".join(json.dumps(r) + "\n" for r in rows))
        self.assertFalse(self.result()["A6_satisfied"])

    def test_freeze_keeps_inaccessible_and_unknown_rows(self):
        snapshot = self.root / "snapshot"
        snapshot.mkdir()
        listing = [
            {"id": "open/model", "sha": "a" * 40, "pipeline_tag": "text-generation", "siblings": ["model.safetensors"]},
            {"id": "gated/model", "sha": "b" * 40, "gated": True, "siblings": []},
            {"id": "unknown/model", "sha": "c" * 40, "pipeline_tag": None, "disabled": True},
        ]
        with gzip.open(snapshot / "dall.listing-v2.jsonl.gz", "wt") as f:
            f.write("".join(json.dumps(r) + "\n" for r in listing))
        (snapshot / "dall.summary-v2.json").write_text(json.dumps({"repos": 3}))
        (snapshot / "MANIFEST.json").write_text("{}")
        frozen = self.root / "freeze.json"
        ac.freeze_hub(snapshot, frozen, 3, "predeclared")
        design = json.loads(frozen.read_text())
        self.assertEqual({m["id"] for m in design["members"]}, {r["id"] for r in listing})
        self.assertEqual(design["inclusion_probability"], 1)
        (snapshot / "dall.summary-v2.json").write_text(json.dumps({"repos": 4}))
        with self.assertRaises(ValueError):
            ac.freeze_hub(snapshot, self.root / "truncated.json", 3, "predeclared")

    def test_untagged_listing_requires_pinned_full_task_resolution(self):
        self.doc["members"][0]["advertised_task"] = None
        rows = self.save()
        self.assertEqual(self.result()["rates"]["final_capable"]["successes"], 31)
        rows[0]["pinned_source_task_resolution_evidence"] = self.evidence
        self.observations.write_text("".join(json.dumps(r) + "\n" for r in rows))
        self.assertEqual(self.result()["rates"]["final_capable"]["successes"], 32)


if __name__ == "__main__":
    unittest.main()
