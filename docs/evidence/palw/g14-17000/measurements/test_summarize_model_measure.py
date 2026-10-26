"""Reject incomplete/inconsistent measurement evidence even with updated log hashes."""
import hashlib
import json
from pathlib import Path
import shutil
import tempfile
import unittest

from summarize_model_measure import summarize

ROOT = Path(__file__).parent / 'M-COST' / 'raw'


class ModelEvidenceValidation(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.folder = Path(self.temp.name)
        self.source = ROOT / 'dolphin-long-n8192-2026-10-26'
        for name in ['raw.json', 'stdout.log', 'stderr.log']:
            shutil.copy2(self.source / name, self.folder / name)
        self.raw = json.loads((self.folder / 'raw.json').read_text())
        self.lines = (self.folder / 'stdout.log').read_text().splitlines()

    def save(self):
        log = self.folder / 'stdout.log'
        log.write_text('\n'.join(self.lines) + '\n')
        self.raw['stdout_sha256'] = hashlib.sha256(log.read_bytes()).hexdigest()
        (self.folder / 'raw.json').write_text(json.dumps(self.raw))

    def replace_record(self, prefix, row):
        index = next(i for i, line in enumerate(self.lines) if prefix in line)
        self.lines[index] = self.lines[index].split(prefix, 1)[0] + prefix + json.dumps(row)

    def test_finished_8k_preserves_existing_summary(self):
        expected = json.loads((self.source / 'summary.json').read_text())
        self.assertEqual(summarize(self.folder), expected)

    def test_zero_position_setup_control(self):
        self.assertEqual(summarize(ROOT / 'dolphin-preflight-n0-2026-10-26')['positions'], 0)

    def test_collector_result_must_match_stdout(self):
        self.raw['result']['positions_executed'] -= 16
        self.save()
        with self.assertRaisesRegex(AssertionError, 'result record mismatch'):
            summarize(self.folder)

    def test_final_digest_must_match_last_chunk(self):
        self.raw['result']['last_logits_sha256'] = '0' * 64
        self.replace_record('G14_MODEL_RESULT ', self.raw['result'])
        self.save()
        with self.assertRaisesRegex(AssertionError, 'final logits mismatch'):
            summarize(self.folder)

    def test_final_cache_must_match_last_chunk(self):
        self.raw['result']['cache_resident_bytes'] += 1
        self.replace_record('G14_MODEL_RESULT ', self.raw['result'])
        self.save()
        with self.assertRaisesRegex(AssertionError, 'final cache mismatch'):
            summarize(self.folder)

    def test_missing_chunk_is_not_completed_work(self):
        index = next(i for i, line in enumerate(self.lines) if line.startswith('G14_MODEL_CHUNK '))
        del self.lines[index]
        self.save()
        with self.assertRaises(AssertionError):
            summarize(self.folder)

    def test_requested_scope_must_match_executed_scope(self):
        self.raw['env']['G14_A16_POSITIONS'] = '32768'
        self.save()
        with self.assertRaises(AssertionError):
            summarize(self.folder)

    def test_wrong_chunk_width_is_rejected(self):
        index = next(i for i, line in enumerate(self.lines) if line.startswith('G14_MODEL_CHUNK '))
        row = json.loads(self.lines[index].split(' ', 1)[1])
        row['end'] += 1
        self.lines[index] = 'G14_MODEL_CHUNK ' + json.dumps(row)
        self.save()
        with self.assertRaises(AssertionError):
            summarize(self.folder)


if __name__ == '__main__':
    unittest.main()
