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
            self.assertTrue(m["target_repo"].startswith("Misakachain/") and ("-PALW" in m["target_repo"]), m["id"])
            weights = [f for f in m["files"] if f["kind"] == "weight"]
            self.assertTrue(weights, m["id"])
            for f in weights:
                self.assertEqual(len(f["lfs_sha256"] or ""), 64, f["path"])
        self.assertTrue(gp.model_by_id(self.reg, 10)["stem"].startswith("Llama"))
        self.assertFalse(gp.model_by_id(self.reg, 10)["e2e_adjudicable"])
        six = gp.model_by_id(self.reg, 6)
        self.assertEqual(gp.variant_file(six, None), "Huihui-Qwen3.8-27B-Coder390-EfficientThink-abliterated-bf16.gguf")
        self.assertNotIn("mmproj-model-bf16.gguf", [f["path"] for f in six["files"]], "out of scope: never fetched")
        self.assertIn("mmproj-model-bf16.gguf", six["excluded_files"])
        for m in self.reg["models"]:
            self.assertEqual(m["scope"], "text-generation")
            for f in m["files"]:
                self.assertNotIn(f["path"], ("preprocessor_config.json", "video_preprocessor_config.json", "processor_config.json"))
        eight = gp.model_by_id(self.reg, 8)
        self.assertEqual(eight["target_repo"], "Misakachain/Huihui-Qwen3.6-35B-A3B-abliterated-PALW", "a NEW repository")
        self.assertEqual(eight["source_repo"], "huihui-ai/Huihui-Qwen3.6-35B-A3B-abliterated-MTP-GGUF")
        self.assertEqual(eight["source_revision"], "acd5abe0c5ebd064f3863ca7d4bba2d924ec6544")
        q8 = next(f for f in eight["files"] if f["path"] == gp.variant_file(eight, None))
        self.assertEqual((q8["path"], q8["bytes"]), ("Huihui-Qwen3.6-35B-A3B-abliterated-ggml-model-Q8_0.gguf", 37801096416))
        self.assertEqual(len(q8["lfs_sha256"]), 64)
        self.assertIn("mmproj-model-f16.gguf", eight["excluded_files"], "text only: never fetched")
        self.assertNotIn("upload_prefix", eight)
        self.assertTrue(gp.is_palwq36(eight) and not gp.is_legacy_wrap(eight))
        self.assertEqual(eight["history_artifact"]["repo"], "Misakachain/Qwen3.6-35B-A3B-PALW-runtime", "the old runtime is history")

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
            for needle in (m["base_model"], m["derivation_author"], m["source_revision"], "PALW-converted form", "license: apache-2.0",
                           "Scope: text generation only"):
                self.assertIn(needle, card, f"#{mid}: {needle}")
            self.assertEqual(gp.manifest_for(m, None)["scope"]["kind"], "text-generation")
        eight_card = gp.model_card(gp.model_by_id(self.reg, 8), None, None)
        self.assertNotIn("official-derived", eight_card)
        self.assertIn("not an integer-exact lowering of Q8_0", eight_card)
        self.assertTrue(all(gp.manifest_for(m, None)["uncensored"] for m in self.reg["models"]), "every entry is uncensored")
        one = gp.manifest_for(gp.model_by_id(self.reg, 1), None)
        self.assertEqual(one["served_context"], 32768, "#1 is re-converted at 32K")
        self.assertIn("--a16 --n-ctx 32768", one["conversion"]["a16"]["tool"])
        self.assertEqual(one["conversion"]["history_artifact"]["sha256"],
                         "894e6914294e0b225a559365914a5e4509ffcd6ac884ca77f7e0f020c4595438", "the 8k file stays as history")
        eight = gp.manifest_for(gp.model_by_id(self.reg, 8), None)
        self.assertEqual(eight["served_context"], 32768, "#8 rebuilt at 32K")
        self.assertIn("--context 32768", eight["conversion"]["palwq36"]["tool"])


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
            for mid in (1, 2, 3, 4, 5, 6, 8, 10):
                for stream in ([], ["--stream"]):
                    buf = io.StringIO()
                    with contextlib.redirect_stdout(buf):
                        rc = gp.main(["run", "--model", str(mid), "--dry-run", "--work", d, "--hf", "/nonexistent/hf", *stream])
                    self.assertEqual(rc, 0, f"#{mid} {stream}: {buf.getvalue()[-2000:]}")
                    out = buf.getvalue()
                    self.assertNotIn("HF_TOKEN=", out)
                    if mid == 6 and stream:
                        self.assertIn("--gguf-sha256 fcddc1ff6efa4932cf4a7eb18087be39527b740cdca0810bbd3a717312d61d53", out)
                    if mid == 1:
                        self.assertIn("--a16 --n-ctx 32768", out)
                        self.assertIn("model.safetensors", out, "the A16 converter reads the checkpoint")
                    self.assertNotIn("BLOCKED", out)
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = gp.main(["run", "--model", "8", "--dry-run", "--stream", "--work", d, "--hf", "/nonexistent/hf", "--redo"])
            self.assertEqual(rc, 0, buf.getvalue()[-2000:])
            out = buf.getvalue()
            eight = gp.model_by_id(gp.load_registry(), 8)
            q8 = next(f for f in eight["files"] if f["path"] == gp.variant_file(eight, None))
            # Streamed both ends: the measuring pass checks the GGUF against its LFS id; the output pass writes into a pipe.
            self.assertIn("--url-sha256 " + q8["lfs_sha256"], out)
            self.assertIn("--reference-only --measured-out", out)
            self.assertIn("--url-windows-in", out)
            self.assertIn("--measured-in", out)
            self.assertIn("--from-pipe", out)
            self.assertIn("--context 32768", out)
            self.assertNotIn("mmproj", out.split("[genesis] #8 convert")[0].split("download")[-1], "mmproj is never fetched")
            self.assertIn("Misakachain/Huihui-Qwen3.6-35B-A3B-abliterated-PALW", out)


class Calibration(unittest.TestCase):
    def setUp(self):
        self.reg = gp.load_registry()

    def test_every_genesis_model_serves_32k_and_a_recurrent_class_calibrates_on_one_32k_sequence(self):
        self.assertEqual(self.reg["served_context"], gp.GENESIS_CONTEXT)
        self.assertEqual(gp.GENESIS_CONTEXT, 32768)
        for mid in (2, 3, 6):
            m = gp.model_by_id(self.reg, mid)
            self.assertEqual(m["convert_options"]["calib"]["kind"], "recurrent")
            o = gp.effective_options(m, None)
            self.assertEqual((o["calib"]["len"], o["context"]), (32768, 32768))
            o = gp.effective_options(m, 8192)
            self.assertEqual((o["calib"]["len"], o["context"]), (8192, 8192), "freeze-v1 §5.2: the served context is the length")
            self.assertEqual(m["convert_options"]["calib"]["len"], 32768, "the registry is not modified")
            with self.assertRaises(SystemExit):
                gp.effective_options(m, 16)
        for mid in (4, 5, 10):
            m = gp.model_by_id(self.reg, mid)
            o = gp.effective_options(m, None)
            self.assertEqual((o["calib"]["kind"], o["calib"]["len"], o["context"]), ("dense", 512, 32768))
        one = gp.model_by_id(self.reg, 1)
        self.assertTrue(gp.is_a16(one) and not gp.is_legacy_wrap(one))
        self.assertEqual(gp.effective_options(one, None)["n_ctx"], 32768)
        for m in self.reg["models"]:
            self.assertEqual(m["served_context"], 32768, m["id"])
            self.assertNotIn("blocked", m, m["id"])

    def test_the_manifest_pins_the_texts_tokenizer_lengths_and_policy(self):
        texts = self.reg["calibration_texts"]
        self.assertEqual(len(texts["git_commit"]), 40)
        for path, blob in texts["blobs"].items():
            self.assertTrue((gp.ROOT / path).exists(), path)
            self.assertEqual(len(blob), 40, path)
        for m in self.reg["models"]:
            man = gp.manifest_for(m, None, 4096, None, texts)
            cal = man["conversion"].get("calibration")
            if gp.is_legacy_wrap(m) or gp.is_a16(m) or gp.is_palwq36(m):
                self.assertIsNone(cal, f"#{m['id']}: no TIR calibration (a wrapped legacy file, or the A16 converter's own)")
                continue
            self.assertEqual(cal["tokens_file"], "calib-tokens.json")
            self.assertEqual(cal["stats_file"], "calib-stats.json")
            for doc in cal["documents"]:
                self.assertEqual(doc["git_blob"], texts["blobs"][doc["path"]])
                self.assertEqual(doc["git_commit"], texts["git_commit"])
            self.assertEqual(cal["policy"]["gdn_core_wide"], bool(m["convert_options"].get("gdn_core_wide")))
            if cal["kind"] == "recurrent":
                self.assertEqual((cal["positions_per_sequence"], cal["served_context"], cal["calibrated_context"]), (4096, 4096, 4096))
                self.assertEqual(man["conversion"]["options"]["context"], 4096)
            if m.get("tokenizer_source"):
                self.assertEqual(cal["tokenizer"]["repo"], m["tokenizer_source"]["repo"])
        card = gp.model_card(gp.model_by_id(self.reg, 2), None, None, None, None)
        self.assertIn("served context: 32,768 tokens", card)
        measured = {"tokens_sha256": "ab" * 32, "sequences": 1}
        man = gp.manifest_for(gp.model_by_id(self.reg, 2), None, 2048, measured, texts)
        self.assertEqual(man["conversion"]["calibration"]["tokens_sha256"], "ab" * 32)

    def test_the_fit_guard_runs_where_it_can_and_says_why_not_elsewhere(self):
        with tempfile.TemporaryDirectory() as d:
            ap = ["run", "--model", "2", "--dry-run", "--work", d, "--hf", "/nonexistent/hf"]
            self.assertIn("fit", gp.STEPS)
            self.assertLess(gp.STEPS.index("fit"), gp.STEPS.index("upload"), "the guard decides before the index is uploaded")
            import argparse

            a = argparse.Namespace(work=d, hf="/nonexistent/hf", dry_run=True, bin_dir=d, no_fit=False, fit_max_gb=20.0,
                                   local_source=None, gguf_variant=None)
            p = gp.Pipeline(a)
            self.assertEqual(p.fit_plan(gp.model_by_id(p.reg, 2))[0], True)
            self.assertIn("PALWQ361 at 32K is not measurable", p.fit_plan(gp.model_by_id(p.reg, 8))[1])
            self.assertIn("streamed", p.fit_plan(gp.model_by_id(p.reg, 6))[1])
            self.assertIn("--fit-max-gb", p.fit_plan(gp.model_by_id(p.reg, 10))[1])
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = gp.main(ap)
            self.assertEqual(rc, 0)
            out = buf.getvalue()
            self.assertIn("palw-tir-fidelity", out)
            self.assertIn("--stats-in", out)
            self.assertIn("--context 32768", out)
            self.assertIn("--drift-windows 32", out, "the long-context check")
            self.assertIn("--len 32768 --chunks 2", out, "the held-out next 32K tokens")
            self.assertNotIn("--calib-lazy-mib", out, "row-streamed calibration is off by default")
            self.assertIn("cat-file blob " + p.reg["calibration_texts"]["blobs"]["docs/rfc/0002-palw-tensor-ir.md"], out)
            self.assertNotIn("--ram-accounting", out, "accounting is opt-in")
            # RFC-0020: the RAM accounting files and the opt-in carry spill reach the converter and both fit checks.
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = gp.main(ap + ["--ram-accounting", "--spill-carries-dir", d, "--spill-carries-budget-mib", "512", "--redo"])
            self.assertEqual(rc, 0)
            out = buf.getvalue()
            for step in ("convert", "fit", "fit-long"):
                self.assertIn(f"ram-{step}.jsonl", out)
            self.assertEqual(out.count("--spill-carries-budget-mib 512"), 3)

    def test_a_measured_fit_out_of_tolerance_blocks_the_upload(self):
        with tempfile.TemporaryDirectory() as d:
            import argparse

            a = argparse.Namespace(work=d, hf="/nonexistent/hf", dry_run=False, bin_dir=d, no_upload=False, private=False)
            p = gp.Pipeline(a)
            m = gp.model_by_id(p.reg, 2)
            st = gp.State(Path(d), m)
            st.data["fit"] = {"measured": True, "within": False, "kl_mean": 0.27, "top1_agreement": 0.69}
            with self.assertRaises(SystemExit):
                p.upload(m, st)


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
