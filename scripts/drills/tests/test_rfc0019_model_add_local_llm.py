"""RFC-0019 §11 R-7 / R-8: argument and plan wiring of the `model-add` and `local-llm` scenarios (no node is started).

    python3 -m unittest discover -s scripts/drills/tests -p 'test_rfc0019_*.py'
"""
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import rfc0019_rolling_upgrade_drill as drill  # noqa: E402


def entry(cid, root, path, before, after):
    return {'class_id': cid, 'artifact_root': root, 'artifact_path': path, 'tier_permille_before': before, 'tier_permille_after': after}


class Wiring(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        d = Path(self.tmp.name)
        (d / 'old.palwart').write_bytes(b'old')
        (d / 'new.palwart').write_bytes(b'new')
        self.spec = {'edition_after': 1,
                     'old': [entry('aa', '11', str(d / 'old.palwart'), 1000, 1000)],
                     'new': [entry('bb', '22', str(d / 'new.palwart'), 300, 1000)]}
        self.d = d

    def tearDown(self):
        self.tmp.cleanup()

    def test_rfc0019_new_scenarios_are_choices_and_dispatch(self):
        self.assertIn('model-add', drill.SCENARIOS)
        self.assertIn('local-llm', drill.SCENARIOS)
        self.assertTrue(hasattr(drill.Drill, 'scenario_model_add'))
        self.assertTrue(hasattr(drill.Drill, 'scenario_local_llm'))

    def test_rfc0019_model_add_without_section_is_blocked_by_name(self):
        plan = {'nodes': {'A': {'old_argv': ['x'], 'new_argv': ['y'], 'wrpc': '127.0.0.1:1', 'p2p': '127.0.0.1:2'}}, 'switch_daa': 10}
        plan_path, out = self.d / 'plan.json', self.d / 'out.json'
        plan_path.write_text(json.dumps(plan))
        with mock.patch('builtins.print'):
            rc = drill.main(['model-add', '--plan', str(plan_path), '--out', str(out)])
        self.assertEqual(rc, 1)
        result = json.loads(out.read_text())
        self.assertEqual(result['status'], 'BLOCKED')
        self.assertIn('model_add', result['reason'])

    def test_rfc0019_model_add_spec_refuses_missing_keys_and_reuse(self):
        self.assertIs(drill.model_add_spec({'model_add': self.spec}), self.spec)
        bad = json.loads(json.dumps(self.spec))
        del bad['new'][0]['artifact_root']
        with self.assertRaisesRegex(drill.Blocked, 'artifact_root'):
            drill.model_add_spec({'model_add': bad})
        reuse = json.loads(json.dumps(self.spec))
        reuse['new'][0]['class_id'] = 'aa'
        with self.assertRaisesRegex(drill.Blocked, 'reuses'):
            drill.model_add_spec({'model_add': reuse})

    def test_rfc0019_absent_artifact_fails_loudly(self):
        drill.check_artifacts(self.spec)
        gone = json.loads(json.dumps(self.spec))
        gone['new'][0]['artifact_path'] = str(self.d / 'missing.palwart')
        with self.assertRaisesRegex(drill.Blocked, 'absent: .*missing.palwart'):
            drill.check_artifacts(gone)
        pinned = json.loads(json.dumps(self.spec))
        pinned['old'][0]['artifact_sha256'] = '00' * 32
        with self.assertRaisesRegex(drill.Blocked, 'sha256'):
            drill.check_artifacts(pinned)

    def test_rfc0019_model_add_verdict(self):
        before = {'aa': ('11', 1000)}
        after = {'aa': ('11', 1000), 'bb': ('22', 1000)}
        drill.check_model_add(before, after, self.spec, 1)
        with self.assertRaisesRegex(AssertionError, 'root changed'):
            drill.check_model_add(before, {'aa': ('99', 1000), 'bb': ('22', 1000)}, self.spec, 1)
        with self.assertRaisesRegex(AssertionError, 'absent after'):
            drill.check_model_add(before, {'aa': ('11', 1000)}, self.spec, 1)
        with self.assertRaisesRegex(AssertionError, 'edition'):
            drill.check_model_add(before, after, self.spec, 0)
        with self.assertRaisesRegex(AssertionError, 'below the switch'):
            drill.check_model_add({'aa': ('11', 1000), 'bb': ('22', 1000)}, after, self.spec, 1)
        with self.assertRaisesRegex(AssertionError, 'two class ids'):
            drill.check_model_add(before, dict(after, cc=('22', 300)), self.spec, 1)

    def test_rfc0019_local_llm_spec_and_answer(self):
        with self.assertRaisesRegex(drill.Blocked, 'local_llm'):
            drill.local_llm_spec({})
        with self.assertRaisesRegex(drill.Blocked, 'loopback'):
            drill.local_llm_spec({'local_llm': {'prompt': 'hi', 'gateways': [{'class_id': 'aa', 'url': 'http://example.com'}]}})
        health = {'class_id': 'aa', 'runtime_manifest_hash': 'f0'}
        models = {'data': [{'id': 'misaka'}]}
        chat = {'choices': [{'message': {'content': 'hello'}}], 'misaka': {'class_id': 'aa'}}
        row = drill.check_gateway_answer('aa', health, models, chat)
        self.assertEqual(row['chars'], 5)
        with self.assertRaisesRegex(AssertionError, 'names class'):
            drill.check_gateway_answer('bb', health, models, chat)
        with self.assertRaisesRegex(AssertionError, 'empty answer'):
            drill.check_gateway_answer('aa', health, models, {'choices': [{'message': {'content': ' '}}]})
        with self.assertRaisesRegex(AssertionError, 'fallback'):
            drill.check_gateway_answer('aa', health, models, dict(chat, misaka={'class_id': 'bb'}))


if __name__ == '__main__':
    unittest.main()
