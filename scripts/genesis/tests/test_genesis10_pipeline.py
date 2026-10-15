"""Offline tests of the genesis pipeline: registry, manifests, model cards, disk plans, state merging, catalog fill, dry runs.

    python3 -m unittest discover -s scripts/genesis/tests -p 'test_*.py'

No network, no download, no conversion (every `run` here is `--dry-run`).
"""

from __future__ import annotations

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))

import genesis10_pipeline as gp  # noqa: E402


class Registry(unittest.TestCase):
    def setUp(self):
        self.reg = gp.load_registry()

    def test_the_set_is_eight_with_pins_and_lfs_ids(self):
        ids = [m["id"] for m in self.reg["models"]]
        self.assertEqual(ids, [1, 2, 3, 4, 5, 6, 8, 10])
        for m in self.reg["models"]:
            self.assertEqual(len(m["source_revision"]), 40, m["id"])
            self.assertTrue(m["target_repo"].startswith("Misakachain/") and m["target_repo"].endswith("-PALW"))
            weights = [f for f in m["files"] if f["kind"] == "weight"]
            self.assertTrue(weights, m["id"])
            for f in weights:
                self.assertEqual(len(f["lfs_sha256"] or ""), 64, f["path"])
        self.assertTrue(gp.model_by_id(self.reg, 10)["stem"].startswith("Llama"))
        self.assertFalse(gp.model_by_id(self.reg, 10)["e2e_adjudicable"])
        six = gp.model_by_id(self.reg, 6)
        self.assertEqual(gp.variant_file(six, None), "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-bf16.gguf")
        self.assertIn("mmproj-model-bf16.gguf", [f["path"] for f in six["files"]])
        self.assertNotIn("mmproj-model-bf16.gguf", [f["path"] for f in gp.needed_files(six, None)], "pinned, not converted")

    def test_every_gdn_hybrid_converts_with_the_fix_of_the_9b_fit_refusal(self):
        # The past refusal of Huihui-Qwen3.5-9B (HF_FIT_OUT_OF_TOLERANCE, KL 0.268) was the GDN core's 16-bit grid; the fix is
        # `gdn_core_wide` (KL 0.0039 on the full model). The fixtures of the refusals stay; every genesis GDN conversion sets it.
        gdn = [m for m in self.reg["models"] if m["family"].startswith("qwen35-gdn-hybrid") and m["converter"] in ("tir", "tir-gguf")]
        self.assertEqual(sorted(m["id"] for m in gdn), [2, 3, 6])
        for m in gdn:
            self.assertTrue(m["convert_options"].get("gdn_core_wide"), m["id"])
        nine = HERE.parents[1] / "misaka/palw/tir/lower/tests/fixtures/vlm-generic/huihui-qwen3.5-9b/fixture.json"
        self.assertEqual(json.loads(nine.read_text())["revision"], gp.model_by_id(self.reg, 3)["source_revision"])


class Cards(unittest.TestCase):
    def setUp(self):
        self.reg = gp.load_registry()

    def test_llama_card_and_manifest_carry_the_license_terms(self):
        m = gp.model_by_id(self.reg, 10)
        man = gp.manifest_for(m, None)
        self.assertTrue(man["license"]["built_with_llama"])
        self.assertTrue(man["license"]["name_begins_with_llama"])
        self.assertIn("Built with Llama", man["attribution"])
        card = gp.model_card(m, None, None)
        self.assertIn("**Built with Llama.**", card)
        self.assertIn(gp.LLAMA_NOTICE, card)
        self.assertIn("license_name: llama3.3", card)
        self.assertIn("Not end-to-end adjudicable", card)

    def test_apache_cards_name_base_derivation_author_revision_and_the_palw_form(self):
        for mid in (1, 2, 4, 5, 6, 8):
            m = gp.model_by_id(self.reg, mid)
            card = gp.model_card(m, None, {"root": "ab" * 64, "index_file": "x.palwart", "index_sha256": "cd" * 32,
                                          "weights": {}, "segments": [], "data_bytes": 0})
            for needle in (m["base_model"], m["derivation_author"], m["source_revision"], "PALW-converted form", "license: apache-2.0"):
                self.assertIn(needle, card, f"#{mid}: {needle}")
        one = gp.manifest_for(gp.model_by_id(self.reg, 1), None)
        self.assertFalse(one["conversion"]["requantised"])
        self.assertEqual(one["conversion"]["legacy_artifact"]["sha256"],
                         "894e6914294e0b225a559365914a5e4509ffcd6ac884ca77f7e0f020c4595438")


class Plans(unittest.TestCase):
    def test_streaming_bounds_the_disk(self):
        reg = gp.load_registry()
        six = gp.estimate(gp.model_by_id(reg, 6), None, True, 2.147)
        self.assertLess(six["peak_disk_bytes_est"], 6 * gp.GB, "the streamed GGUF is never stored")
        ten = gp.estimate(gp.model_by_id(reg, 10), None, True, 2.147)
        self.assertLess(ten["peak_disk_bytes_est"], 16 * gp.GB)
        self.assertGreater(gp.estimate(gp.model_by_id(reg, 10), None, False, 2.147)["peak_disk_bytes_est"], 200 * gp.GB)


class StateMerge(unittest.TestCase):
    def test_the_hooks_uploads_survive_a_later_mark(self):
        reg = gp.load_registry()
        m = gp.model_by_id(reg, 6)
        with tempfile.TemporaryDirectory() as d:
            work = Path(d)
            mine = gp.State(work, m)
            mine.mark("download", {})
            hook = gp.State(work, m)
            hook.data.setdefault("uploaded_segments", {})["s.00000.palwseg"] = {"sha256": "00" * 32}
            hook.save()
            mine.mark("convert", {})
            again = gp.State(work, m)
            self.assertIn("s.00000.palwseg", again.data.get("uploaded_segments", {}))
            self.assertEqual(sorted(again.data["steps"]), ["convert", "download"])


class DryRuns(unittest.TestCase):
    def test_every_model_dry_runs_offline(self):
        with tempfile.TemporaryDirectory() as d:
            for mid in (1, 2, 3, 4, 5, 6, 10):
                for stream in ([], ["--stream"]):
                    buf = io.StringIO()
                    with contextlib.redirect_stdout(buf):
                        rc = gp.main(["run", "--model", str(mid), "--dry-run", "--work", d, "--hf", "/nonexistent/hf", *stream])
                    self.assertEqual(rc, 0, f"#{mid} {stream}: {buf.getvalue()[-2000:]}")
                    out = buf.getvalue()
                    self.assertNotIn("HF_TOKEN=", out)
                    if mid == 6 and stream:
                        self.assertIn("--gguf-sha256 fcddc1ff6efa4932cf4a7eb18087be39527b740cdca0810bbd3a717312d61d53", out)
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = gp.main(["run", "--model", "8", "--dry-run", "--work", d, "--hf", "/nonexistent/hf"])
            self.assertEqual(rc, 1, "#8 is blocked until a PALWQ361 converter exists")


class Catalog(unittest.TestCase):
    def test_edition0_needs_a_root_and_a_class_for_every_listed_model(self):
        with tempfile.TemporaryDirectory() as d:
            work = Path(d)
            ids = [1, 2, 4, 5, 6, 8, 10]
            gp.append_result(work, {"id": 1, "root": "11" * 64})
            classes = work / "classes.json"
            classes.write_text(json.dumps({str(i): f"{i:02x}" * 64 for i in ids}))
            out = work / "cat.json"
            with contextlib.redirect_stderr(io.StringIO()):
                rc = gp.main(["catalog", "--work", d, "--classes", str(classes), "--fence-daa", "100", "--out", str(out)])
            self.assertEqual(rc, 2, "only #1 has a root")
            for i in ids:
                gp.append_result(work, {"id": i, "root": f"{i + 16:02x}" * 64})
            rc = gp.main(["catalog", "--work", d, "--classes", str(classes), "--fence-daa", "100", "--out", str(out)])
            self.assertEqual(rc, 0)
            cat = json.loads(out.read_text())
            self.assertEqual(len(cat["editions"][0]["models"]), 7, "the tentative #3 is left out unless asked")


if __name__ == "__main__":
    unittest.main()
