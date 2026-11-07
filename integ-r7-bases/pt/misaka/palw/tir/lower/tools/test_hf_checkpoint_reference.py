"""Adversarial preallocation checks; no Torch or model data is loaded."""
import importlib.util
import json
from pathlib import Path
import struct
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("source_reference", Path(__file__).with_name("hf_checkpoint_reference.py"))
reference = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reference)


class PreallocationChecks(unittest.TestCase):
    def test_exact_job_tokens_and_context_are_required(self):
        seq = {"sequences": [[1, 2], [3]]}
        self.assertEqual(reference.bounded_sequences(seq, 4, 2, 3, 12), [[1, 2], [3]])
        for invalid in ({"sequences": [[4]]}, {"sequences": [[True]]}, {"sequences": [[]]},
                        {"sequences": [[0, 1, 2]]}, {"sequences": [[-1]]}):
            with self.assertRaises(ValueError):
                reference.bounded_sequences(invalid, 4, 2, 3, 12)
        with self.assertRaises(ValueError):
            reference.bounded_sequences(seq, 4, 2, 2, 12)
        with self.assertRaises(ValueError):
            reference.bounded_sequences(seq, 4, 2, 3, 11)

    def test_source_shards_and_hashes_are_checked_independently(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "config.json").write_text("{}")
            (root / "model.safetensors").write_bytes(b"pinned tensor bytes")
            files = [{"path": p.name, "bytes": p.stat().st_size,
                      "sha256": reference.sha256_file(p)} for p in root.iterdir()]
            manifest = {"repository": "author/model", "revision": "1" * 40, "source_files": files}
            pins = reference.source_pins(root, manifest, 1000)
            reference.verify_pins(root, pins)
            (root / "model.safetensors").write_bytes(b"changed tensor data")
            with self.assertRaises(ValueError):
                reference.verify_pins(root, pins)
            for bad in ([files[0], files[0]], [{**files[0], "path": "../model.safetensors"}],
                        [{**files[0], "path": "/absolute"}], [{**files[0], "bytes": True}]):
                with self.assertRaises(ValueError):
                    reference.source_pins(root, {**manifest, "source_files": bad}, 1000)

    def test_header_and_float32_expansion_are_capped_without_data_allocation(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            header = json.dumps({"weight": {"dtype": "BF16", "shape": [4], "data_offsets": [0, 8]}}).encode()
            path = root / "model.safetensors"
            path.write_bytes(struct.pack("<Q", len(header)) + header + bytes(8))
            pins = [{"path": path.name, "bytes": path.stat().st_size}]
            self.assertEqual(reference.model_bytes(root, pins, 16), 16)
            with self.assertRaises(ValueError):
                reference.model_bytes(root, pins, 15)
            path.write_bytes(struct.pack("<Q", (1 << 63)))
            with self.assertRaises(ValueError):
                reference.model_bytes(root, [{"path": path.name, "bytes": path.stat().st_size}], 16)

    def test_duplicate_source_files_cannot_hide_an_unpinned_shard(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            (root / "config.json").write_text("{}")
            (root / "model.safetensors").write_bytes(b"first")
            (root / "hidden.safetensors").write_bytes(b"second")
            files = [{"path": name, "bytes": (root / name).stat().st_size,
                      "sha256": reference.sha256_file(root / name)} for name in ("config.json", "model.safetensors")]
            with self.assertRaises(ValueError):
                reference.source_pins(root, {"repository": "author/model", "revision": "a" * 40,
                                            "source_files": files}, 1000)


if __name__ == "__main__":
    unittest.main()
