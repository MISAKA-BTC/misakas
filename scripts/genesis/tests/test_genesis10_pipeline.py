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


class UploadGateTest(unittest.TestCase):
    """User decision 2026-10-18: a genesis upload needs a MEASURED fit within tolerance, short and long."""

    def test_unmeasured_or_out_of_tolerance_fits_never_upload(self):
        good_long = {"measured": True, "within": True}
        self.assertIsNone(gp.upload_refusal({"measured": True, "within": True, "long": good_long}))
        for fit in (None, {}, {"measured": False, "reason": "source evicted"},
                    {"measured": True, "within": False, "long": good_long},
                    {"measured": True, "within": True},
                    {"measured": True, "within": True, "long": {"measured": False, "reason": "--no-fit-long"}},
                    {"measured": True, "within": True, "long": {}},
                    {"measured": True, "within": True, "long": {"measured": True, "within": False}}):
            self.assertIsNotNone(gp.upload_refusal(fit), fit)


if __name__ == "__main__":
    unittest.main()


class A16Windows(unittest.TestCase):
    def setUp(self):
        self.tol = gp.load_registry()["calibration_texts"]["fit_tolerance"]
        self.digest = "ab" * 64
        self.report = {
            "sequence_tokens": 32768, "a16_artifact_digest": self.digest,
            "calibration": {"schema": "misaka-palw/a16-source-calibration/v1", "artifact_digest": self.digest,
                            "n_ctx": 32768, "logit_scale": 0.01},
            "windows": [{"window_start": start, "window_len": 64, "positions": 64,
                         "top1_agree": 60, "kl": {"mean": 0.002, "max": 0.004}}
                        for start in gp.a16_window_starts(32768, 32)]}

    def test_all_32_absolute_windows_use_existing_tolerance_and_keep_scope_explicit(self):
        fit = gp.a16_window_fit(self.report, self.digest, 32768, self.tol)
        self.assertIsNone(gp.upload_refusal(fit))
        self.assertEqual(fit["long"]["sequence_tokens"], 32768)
        self.assertEqual(fit["long"]["positions"], 2048)
        self.assertEqual(len(fit["long"]["windows"]), 32)
        self.assertEqual(fit["long"]["windows"][-1]["window_start"], 32704)
        self.assertIn("whole prefix", fit["long"]["measurement_scope"])

    def test_bad_late_window_is_not_hidden_by_good_aggregate(self):
        self.report["windows"][-1]["kl"] = {"mean": 0.2, "max": 0.3}
        self.assertIsNotNone(gp.upload_refusal(gp.a16_window_fit(self.report, self.digest, 32768, self.tol)))
        self.report["windows"][-1]["kl"] = {"mean": 0.002, "max": 0.004}
        self.report["windows"][-1]["top1_agree"] = 10
        self.assertIsNotNone(gp.upload_refusal(gp.a16_window_fit(self.report, self.digest, 32768, self.tol)))

    def test_incomplete_foreign_and_nonfinite_measurements_are_refused(self):
        import copy
        for change in [lambda r: r["windows"].pop(),
                       lambda r: r.update(a16_artifact_digest="cd" * 64),
                       lambda r: r["calibration"].update(artifact_digest="cd" * 64),
                       lambda r: r["calibration"].update(logit_scale=0),
                       lambda r: r["calibration"].update(logit_scale=True),
                       lambda r: r["windows"][0].update(top1_agree=True),
                       lambda r: r["windows"][0].update(positions=63),
                       lambda r: r["windows"][0]["kl"].update(mean=float("nan")),
                       lambda r: r["windows"][0]["kl"].update(max=float("inf"))]:
            report = copy.deepcopy(self.report)
            change(report)
            with self.assertRaises(ValueError):
                gp.a16_window_fit(report, self.digest, 32768, self.tol)

    def test_late_drift_cannot_pass_by_zero_early_kl(self):
        self.report["windows"][0]["kl"] = {"mean": 0.0, "max": 0.0}
        self.report["windows"][-1]["kl"] = {"mean": 0.02, "max": 0.03}
        self.assertFalse(gp.a16_window_fit(self.report, self.digest, 32768, self.tol)["within"])


class Palwq36Windows(unittest.TestCase):
    """RFC-0002/0013: #8 is measured by qwen36-window-compare only from a real reference file; no fake pass."""

    def _pipeline(self, d, **kw):
        import argparse

        base = dict(work=d, hf="/nonexistent/hf", dry_run=True, bin_dir=d, no_fit=False, no_fit_long=False, fit_max_gb=20.0,
                    local_source=None, gguf_variant=None, context=None)
        base.update(kw)
        return gp.Pipeline(argparse.Namespace(**base))

    def test_palwq36_fit_plan_refuses_until_real_files_exist(self):
        with tempfile.TemporaryDirectory() as d:
            eight = gp.model_by_id(gp.load_registry(), 8)
            ok, why = self._pipeline(d).fit_plan(eight)
            self.assertFalse(ok)
            self.assertIn("--palwq36-reference-windows not given", why)
            ok, why = self._pipeline(d, palwq36_artifact=f"{d}/none.palwq36", palwq36_reference_windows=f"{d}/none.bin").fit_plan(eight)
            self.assertFalse(ok)
            self.assertIn("none.bin", why)
            art, ref = Path(d) / "a.palwq36", Path(d) / "r.bin"
            art.write_bytes(b"x")
            ref.write_bytes(b"x")
            self.assertEqual(self._pipeline(d, palwq36_artifact=str(art), palwq36_reference_windows=str(ref)).fit_plan(eight), (True, ""))
            self.assertFalse(self._pipeline(d, no_fit=True, palwq36_artifact=str(art), palwq36_reference_windows=str(ref)).fit_plan(eight)[0])

    def test_palwq36_dry_run_fit_invokes_the_window_tool(self):
        with tempfile.TemporaryDirectory() as d:
            art, ref = Path(d) / "a.palwq36", Path(d) / "r.bin"
            art.write_bytes(b"x")
            ref.write_bytes(b"x")
            p = self._pipeline(d, palwq36_artifact=str(art), palwq36_reference_windows=str(ref), palwq36_logit_scale=0.25)
            p.tokens_file = lambda spec, tok, out: {"tokens_sha256": "0" * 64}
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
                p.fit_palwq36(gp.model_by_id(p.reg, 8), None)
            out = buf.getvalue()
            self.assertIn("qwen36-window-compare", out)
            self.assertIn("--reference-windows " + str(ref), out)
            self.assertIn("--logit-scale 0.25", out)

    def _report(self, sha, context=32768, scale=0.25, kl=0.01):
        starts = gp.a16_window_starts(context, 32)
        return {"tool": "qwen36-window-compare", "sequence_tokens": context, "tokens_sha256": sha, "logit_scale": scale,
                "reference": {"schema": "misaka-palw/qwen36-reference-windows/v1", "tokens_sha256": sha},
                "windows": [{"window_start": s, "window_len": 64, "positions": 64, "top1_agree": 60,
                             "kl": {"mean": kl, "max": kl * 2}} for s in starts]}

    def test_palwq36_reader_scores_a_whole_measurement(self):
        tol = gp.load_registry()["calibration_texts"]["fit_tolerance"]
        fit = gp.palwq36_window_fit(self._report("ab"), "ab", 32768, tol)
        self.assertTrue(fit["measured"] and fit["within"])
        self.assertFalse(gp.palwq36_window_fit(self._report("ab", kl=0.5), "ab", 32768, tol)["within"])

    def test_palwq36_reader_refuses_missing_scale_foreign_sequence_or_windows(self):
        tol = gp.load_registry()["calibration_texts"]["fit_tolerance"]
        with self.assertRaises(ValueError):
            gp.palwq36_window_fit(self._report("ab", scale=None), "ab", 32768, tol)
        with self.assertRaises(ValueError):
            gp.palwq36_window_fit(self._report("ab"), "cd", 32768, tol)
        r = self._report("ab")
        r["windows"] = r["windows"][:-1]
        with self.assertRaises(ValueError):
            gp.palwq36_window_fit(r, "ab", 32768, tol)
        r = self._report("ab")
        del r["windows"][3]["kl"]
        with self.assertRaises(ValueError):
            gp.palwq36_window_fit(r, "ab", 32768, tol)


class Palwq36EstimateR7(unittest.TestCase):
    """R7 (user decision 2026-10-22, 目安で可): the PALWQ361 logit scale and reference window by ESTIMATE — recorded as one,
    checked against the windows' own spread, a prefix reference held to a tightened bar, never written as measured."""

    S = gp.PALWQ361_ESTIMATED_LOGIT_SCALE

    def _pipeline(self, d, **kw):
        import argparse

        base = dict(work=d, hf="/nonexistent/hf", dry_run=True, bin_dir=d, no_fit=False, no_fit_long=False, fit_max_gb=20.0,
                    local_source=None, gguf_variant=None, context=None)
        base.update(kw)
        return gp.Pipeline(argparse.Namespace(**base))

    def _report(self, sha, n=2048, scale=None, kl=0.01, spread_log2=0.02):
        scale = self.S if scale is None else scale
        starts = gp.a16_window_starts(n, 32)
        return {"tool": "qwen36-window-compare", "sequence_tokens": n, "tokens_sha256": sha, "logit_scale": scale,
                "reference": {"schema": "misaka-palw/qwen36-reference-windows/v1", "tokens_sha256": sha},
                "windows": [{"window_start": s, "window_len": 64, "positions": 64, "top1_agree": 60,
                             "logit_scale_spread": scale * 2 ** spread_log2, "kl": {"mean": kl, "max": kl * 2}} for s in starts]}

    def test_the_register_value_is_two_to_the_minus_nine_and_the_prefix_windows_tile_it(self):
        self.assertEqual(self.S, 0.001953125)
        self.assertEqual(gp.a16_window_starts(gp.PALWQ361_REFERENCE_PREFIX_DEFAULT, 32), list(range(0, 2048, 64)))

    def test_the_prefix_bar_is_never_looser_than_the_a16_bar(self):
        tol = gp.load_registry()["calibration_texts"]["fit_tolerance"]
        bar = gp.palwq36_estimate_tolerance(tol)
        for k in ("kl_mean_max", "window_kl_mean_max", "drift_floor_kl", "drift_ratio_max"):
            self.assertLessEqual(bar[k], tol[k], k)
        self.assertEqual(bar["kl_mean_max"], tol["kl_mean_max"] * gp.PALWQ361_ESTIMATE_KL_MARGIN)
        self.assertGreaterEqual(bar["top1_min"], tol["top1_min"])

    def test_an_estimate_fit_is_recorded_as_an_estimate_and_never_as_measured(self):
        tol = gp.load_registry()["calibration_texts"]["fit_tolerance"]
        fit = gp.palwq36_estimate_fit(self._report("ab"), "ab", 2048, tol, 32, self.S, "prefix", 32768)
        self.assertFalse(fit["measured"])
        self.assertTrue(fit["estimated"] and fit["within"] and fit["estimate"]["scale_consistent"])
        self.assertFalse(fit["long"]["measured"])
        self.assertTrue(fit["long"]["estimated"])
        est = fit["estimate"]
        self.assertEqual((est["status"], est["approval"], est["class"]), (gp.R7_STATUS, gp.R7_APPROVAL, "PALWQ361"))
        self.assertEqual((est["reference_scope"], est["reference_sequence_tokens"], est["served_context"]), ("prefix", 2048, 32768))
        self.assertIn("ESTIMATE", fit["measurement_scope"])
        self.assertIsNone(gp.upload_refusal(fit))

    def test_a_scale_a_power_of_two_off_the_windows_spread_is_refused(self):
        tol = gp.load_registry()["calibration_texts"]["fit_tolerance"]
        for off in (1.0, -1.0, 0.3):
            fit = gp.palwq36_estimate_fit(self._report("ab", spread_log2=off), "ab", 2048, tol, 32, self.S, "prefix", 32768)
            self.assertFalse(fit["within"], off)
            self.assertFalse(fit["estimate"]["scale_consistent"], off)
            self.assertIn("disagrees with the windows' own spread", gp.upload_refusal(fit))

    def test_the_prefix_is_held_to_the_tightened_bar_and_a_full_reference_to_the_a16_bar(self):
        tol = gp.load_registry()["calibration_texts"]["fit_tolerance"]
        kl = 0.07  # inside the A16 bar (0.1), outside the prefix bar (0.05)
        self.assertTrue(gp.palwq36_window_fit(self._report("ab", n=32768, kl=kl), "ab", 32768, tol)["within"])
        prefix = gp.palwq36_estimate_fit(self._report("ab", kl=kl), "ab", 2048, tol, 32, self.S, "prefix", 32768)
        self.assertFalse(prefix["within"])
        self.assertIsNotNone(gp.upload_refusal(prefix))
        full = gp.palwq36_estimate_fit(self._report("ab", n=32768, kl=kl), "ab", 32768, tol, 32, self.S, "full", 32768)
        self.assertTrue(full["within"])
        self.assertFalse(full["measured"])

    def test_the_estimate_reader_refuses_what_it_cannot_check(self):
        tol = gp.load_registry()["calibration_texts"]["fit_tolerance"]
        with self.assertRaises(ValueError):  # the run used another scale than the estimate
            gp.palwq36_estimate_fit(self._report("ab", scale=self.S * 2), "ab", 2048, tol, 32, self.S, "prefix", 32768)
        r = self._report("ab")
        del r["windows"][5]["logit_scale_spread"]
        with self.assertRaises(ValueError):  # a tool that predates the spread
            gp.palwq36_estimate_fit(r, "ab", 2048, tol, 32, self.S, "prefix", 32768)
        with self.assertRaises(ValueError):  # a foreign sequence
            gp.palwq36_estimate_fit(self._report("ab"), "cd", 2048, tol, 32, self.S, "prefix", 32768)
        with self.assertRaises(ValueError):
            gp.palwq36_estimate_fit(self._report("ab"), "ab", 2048, tol, 32, float("nan"), "prefix", 32768)
        with self.assertRaises(ValueError):
            gp.palwq36_estimate_fit(self._report("ab"), "ab", 2048, tol, 32, self.S, "sampled", 32768)

    def test_the_upload_gate_takes_only_an_approved_palwq36_estimate(self):
        tol = gp.load_registry()["calibration_texts"]["fit_tolerance"]
        fit = gp.palwq36_estimate_fit(self._report("ab"), "ab", 2048, tol, 32, self.S, "prefix", 32768)
        for field, value in (("approval", "someone else"), ("class", "A16"), ("status", "measured")):
            forged = json.loads(json.dumps(fit))
            forged["estimate"][field] = value
            self.assertIn("outside the R7 approval", gp.upload_refusal(forged), field)
        forged = json.loads(json.dumps(fit))
        forged["long"]["within"] = False
        self.assertIsNotNone(gp.upload_refusal(forged))
        # The pre-existing gate is unchanged for everything that is neither measured nor an R7 estimate.
        self.assertIn("not measured", gp.upload_refusal({"measured": False, "reason": "x"}))

    def test_reference_windows_are_cut_from_the_converters_logit_rows(self):
        n, vocab = 6, 3
        rows = [[float(10 * p + v) for v in range(vocab)] for p in range(n)]
        with tempfile.TemporaryDirectory() as d:
            logits, out = Path(d) / "l.bin", Path(d) / "w.bin"

            def write(rs, width=vocab):
                b = bytearray(len(rs).to_bytes(8, "little"))
                for r in rs:
                    b += width.to_bytes(8, "little") + b"".join(__import__("struct").pack("<f", v) for v in r)
                logits.write_bytes(bytes(b))

            write(rows)
            header = gp.write_reference_windows_from_logits(logits, out, "ab", n, [0, 4], 2, producer="test")
            self.assertEqual(header["vocab"], vocab)
            data = out.read_bytes()
            hlen = int.from_bytes(data[:4], "little")
            h = json.loads(data[4:4 + hlen])
            self.assertEqual((h["schema"], h["tokens_sha256"], h["sequence_tokens"], h["window_starts"], h["window_len"]),
                             ("misaka-palw/qwen36-reference-windows/v1", "ab", n, [0, 4], 2))
            import struct
            body = struct.unpack(f"<{4 * vocab}f", data[4 + hlen:])
            self.assertEqual(list(body), rows[0] + rows[1] + rows[4] + rows[5])
            with self.assertRaises(ValueError):  # the converter ran over another length
                gp.write_reference_windows_from_logits(logits, out, "ab", n + 1, [0, 4], 2)
            with self.assertRaises(ValueError):  # a window past the sequence
                gp.write_reference_windows_from_logits(logits, out, "ab", n, [0, 5], 2)
            logits.write_bytes(logits.read_bytes()[:-4])
            with self.assertRaises(ValueError):  # truncated
                gp.write_reference_windows_from_logits(logits, out, "ab", n, [0, 4], 2)

    def test_fit_plan_in_estimate_mode(self):
        with tempfile.TemporaryDirectory() as d:
            eight = gp.model_by_id(gp.load_registry(), 8)
            art = Path(d) / "a.palwq36"
            ok, why = self._pipeline(d, palwq36_estimated_scale=self.S).fit_plan(eight)
            self.assertFalse(ok)
            self.assertIn("--palwq36-artifact not given", why)
            art.write_bytes(b"x")
            self.assertEqual(self._pipeline(d, palwq36_estimated_scale=self.S, palwq36_artifact=str(art)).fit_plan(eight), (True, ""))
            ok, why = self._pipeline(d, palwq36_estimated_scale=self.S, palwq36_logit_scale=0.25, palwq36_artifact=str(art)).fit_plan(eight)
            self.assertFalse(ok)
            self.assertIn("not both", why)
            for bad in (64, 8192):
                ok, why = self._pipeline(d, palwq36_estimated_scale=self.S, palwq36_artifact=str(art),
                                         palwq36_reference_prefix=bad).fit_plan(eight)
                self.assertFalse(ok, bad)
            ok, _ = self._pipeline(d, palwq36_estimated_scale=self.S, palwq36_artifact=str(art),
                                   palwq36_reference_windows=f"{d}/none.bin").fit_plan(eight)
            self.assertFalse(ok)
            # Without the estimate flag the measured path is unchanged: no reference windows, no fit.
            self.assertFalse(self._pipeline(d, palwq36_artifact=str(art)).fit_plan(eight)[0])

    def test_dry_run_computes_a_prefix_reference_and_compares_at_the_estimate(self):
        with tempfile.TemporaryDirectory() as d:
            art = Path(d) / "a.palwq36"
            art.write_bytes(b"x")
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(buf):
                rc = gp.main(["run", "--model", "8", "--dry-run", "--work", d, "--hf", "/nonexistent/hf",
                              "--palwq36-artifact", str(art), "--palwq36-estimated-scale"])
            out = buf.getvalue()
            self.assertEqual(rc, 0, out[-2000:])
            self.assertIn("R7 ESTIMATE mode", out)
            self.assertIn("--reference-only --prompt", out)
            self.assertIn("--reference-logits", out)
            self.assertIn("--logit-scale 0.001953125", out)
            self.assertIn("q36-prefix2048-reference-windows.bin", out)
