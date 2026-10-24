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


    def local_orchestrator(self, reply=None, registry_edit=None):
        """Scripted RPC/gateway observations; the actual R-8 orchestration and verdicts run.

        This unit seam launches no node or LLM and supplies no campaign evidence.
        """
        spec = json.loads(json.dumps(self.spec))
        spec['catalog_root_after'] = 'cc'
        plan = {'model_add': spec, 'switch_daa': 10,
                'nodes': {r: {'wrpc': '127.0.0.1:%d' % (100 + i)} for i, r in enumerate(('A', 'B'))},
                'local_llm': {'prompt': 'fixed prompt', 'gateways': [
                    {'class_id': 'aa', 'url': 'http://127.0.0.1:9010'},
                    {'class_id': 'bb', 'url': 'http://127.0.0.1:9011'}]}}
        instance = drill.Drill(plan, self.d)
        phase, calls = {'value': 'before'}, []
        instance.scenario_rolling = mock.Mock(return_value=['A', 'B'])
        instance.advance = mock.Mock(side_effect=lambda *_: phase.update(value='after'))
        instance.daa = mock.Mock(side_effect=lambda _role: 1 if phase['value'] == 'before' else 20)
        instance.wait = lambda predicate, _label: predicate()

        def registry(role):
            after = phase['value'] == 'after'
            reading = ({'aa': ('11', 1000), 'bb': ('22', 1000 if after else 300)}, 1 if after else 0, 'cc' if after else 'dd')
            return registry_edit(phase['value'], role, reading) if registry_edit else reading

        def gateway(g, _prompt, _max_tokens):
            cid, when = g['class_id'], phase['value']
            calls.append((when, cid))
            chat = {'choices': [{'message': {'content': 'real-response-shape'}}], 'misaka': {'class_id': cid}}
            if reply:
                chat = reply(when, cid, chat)
            return drill.check_gateway_answer(cid, {'class_id': cid}, {'data': [{'id': 'misaka'}]}, chat)

        instance.registry_reading = mock.Mock(side_effect=registry)
        instance.ask_gateway = mock.Mock(side_effect=gateway)
        return instance, calls

    def test_rfc0019_local_llm_runs_old_and_new_both_sides_and_records_all_registries(self):
        instance, calls = self.local_orchestrator()
        instance.scenario_local_llm()
        self.assertEqual(calls, [('before', 'aa'), ('before', 'bb'), ('after', 'aa'), ('after', 'bb')])
        self.assertEqual([r['class_id'] for r in instance.report['local-llm']['before']], ['aa', 'bb'])
        self.assertEqual(set(instance.report['local-llm']['registries_after']), {'A', 'B'})
        self.assertEqual(instance.registry_reading.call_count, 4)

    def test_rfc0019_new_gateway_pre_switch_failure_or_refusal_cannot_pass_post_switch(self):
        for failure in ('offline', 'refused', 'empty', 'fallback'):
            with self.subTest(failure=failure):
                def reply(when, cid, chat):
                    if (when, cid) != ('before', 'bb'):
                        return chat
                    if failure == 'offline':
                        raise OSError('new gateway offline before H')
                    if failure == 'refused':
                        return {'error': {'message': 'inference refused'}}
                    if failure == 'empty':
                        return {'choices': [{'message': {'content': ' '}}]}
                    return dict(chat, misaka={'class_id': 'aa'})
                instance, calls = self.local_orchestrator(reply=reply)
                with self.assertRaises((OSError, AssertionError)):
                    instance.scenario_local_llm()
                self.assertEqual(calls, [('before', 'aa'), ('before', 'bb')])
                instance.advance.assert_not_called()
                self.assertNotIn('local-llm', instance.report)

    def test_rfc0019_missing_new_pre_switch_observation_cannot_be_recorded(self):
        instance, _ = self.local_orchestrator()
        instance.ask_gateway.side_effect = lambda g, *_: None if g['class_id'] == 'bb' else {'class_id': 'aa'}
        with self.assertRaisesRegex(AssertionError, 'pinned id bb before'):
            instance.scenario_local_llm()
        instance.advance.assert_not_called()
        self.assertNotIn('local-llm', instance.report)

    def test_rfc0019_local_llm_reuses_registry_root_tier_edition_and_role_checks(self):
        for failure in ('before_disagreement', 'after_disagreement', 'wrong_root', 'wrong_tier', 'wrong_edition', 'wrong_catalog'):
            with self.subTest(failure=failure):
                def edit(when, role, reading):
                    pairs, edition, root = reading
                    if failure == 'before_disagreement' and when == 'before' and role == 'B':
                        pairs['bb'] = ('22', 1000)
                    if when == 'after':
                        if failure == 'after_disagreement' and role == 'B':
                            pairs['bb'] = ('99', 1000)
                        if failure == 'wrong_root':
                            pairs['aa'] = ('99', 1000)
                        if failure == 'wrong_tier':
                            pairs['bb'] = ('22', 300)
                        if failure == 'wrong_edition':
                            edition = 0
                        if failure == 'wrong_catalog':
                            root = 'ee'
                    return pairs, edition, root
                instance, _ = self.local_orchestrator(registry_edit=edit)
                with self.assertRaises(AssertionError):
                    instance.scenario_local_llm()
                self.assertNotIn('local-llm', instance.report)


    def test_rfc0019_gateway_observation_requires_actual_chain_phase_on_both_sides(self):
        for phase in ('before', 'after'):
            with self.subTest(phase=phase):
                instance, _ = self.local_orchestrator()
                original = instance.ask_gateway.side_effect

                def crossed(g, prompt, maximum):
                    row = original(g, prompt, maximum)
                    # Simulate external chain progress during the pre-H HTTP call,
                    # or a rollback below H during a post-H observation.
                    if g['class_id'] == 'bb' and (phase == 'before' or instance.advance.called):
                        instance.daa.side_effect = None
                        instance.daa.return_value = 10 if phase == 'before' else 9
                    return row

                instance.ask_gateway.side_effect = crossed
                with self.assertRaisesRegex(AssertionError, 'outside the actual switch phase'):
                    instance.scenario_local_llm()
                if phase == 'before':
                    instance.advance.assert_not_called()
                self.assertNotIn('local-llm', instance.report)


if __name__ == '__main__':
    unittest.main()
