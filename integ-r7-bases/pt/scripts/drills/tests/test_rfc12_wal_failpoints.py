"""Exact-WAL protocol controls; no process or acceptance claim is made here."""
import importlib.util
from pathlib import Path
import re
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
import rfc12_acceptance_plan as P
import rfc12_acceptance_judgement as J
spec = importlib.util.spec_from_file_location('rfc12_wal_driver_controls', SCRIPTS / 'misaka-rfc12-acceptance.py')
D = importlib.util.module_from_spec(spec)
spec.loader.exec_module(D)


class WalFailpoints(unittest.TestCase):
    def test_plan_stages_match_the_real_rust_checkpoint_labels(self):
        source = (SCRIPTS.parents[1] / 'consensus/src/pipeline/virtual_processor/processor.rs').read_text()
        actual = set(re.findall(r'pruning_import_crash_checkpoint\("([^"]+)"\)', source))
        self.assertEqual(set(P.IMPORT_WAL_STAGES), actual)
        for stage in P.IMPORT_WAL_STAGES:
            self.assertEqual(P._trigger(dict(kind='import-wal', stage=stage), 'trigger', 'crashJoin', 'freshJoin')['stage'], stage)

    def test_unknown_stage_wrong_role_or_phase_cannot_arm_a_process(self):
        for stage, role, phase in [('bad', 'crashJoin', 'freshJoin'),
                                   ('utxo-before-clear', 'producer', 'freshJoin'),
                                   ('utxo-before-clear', 'crashJoin', 'coldReopen')]:
            with self.subTest(stage=stage, role=role, phase=phase), self.assertRaises(P.PlanError):
                P._trigger(dict(kind='import-wal', stage=stage), 'trigger', role, phase)

    def test_exact_wal_plan_cannot_lose_its_rendezvous_to_a_coarse_kill(self):
        import copy
        from test_rfc12_acceptance_plan import valid_plan, raw
        plan = valid_plan()
        point = copy.deepcopy(plan['crash']['points'][1])
        point.update(name='exact-wal', trigger=dict(kind='import-wal', stage='utxo-before-clear'))
        plan['crash']['points'].append(point)
        with self.assertRaisesRegex(P.PlanError, 'coarse import phase'):
            P.parse_plan(raw(plan))
        plan['crash']['points'] = [point]
        P.parse_plan(raw(plan))

    def test_only_the_exact_owned_ready_marker_triggers_a_kill(self):
        run = D.AcceptanceRun.__new__(D.AcceptanceRun)
        with tempfile.TemporaryDirectory() as root:
            run.root = Path(root)
            ready = run.root / 'CRASHJOIN/import-wal/ready'
            ready.parent.mkdir(parents=True)
            point = dict(trigger=dict(kind='import-wal', stage='utxo-before-clear'))
            self.assertIsNone(run.trigger(point, 'CRASHJOIN'))
            ready.write_text('utxo-after-clear')
            self.assertIsNone(run.trigger(point, 'CRASHJOIN'))
            ready.write_text('utxo-before-clear')
            result = run.trigger(point, 'CRASHJOIN')
            self.assertEqual(result['source'], 'owned-file')
            self.assertEqual(result['stage'], point['trigger']['stage'])

    def test_recovered_import_must_match_the_requested_stage_and_be_stable(self):
        point = dict(name='exact', role='crashJoin', trigger=dict(kind='import-wal', stage='utxo-before-clear'))
        after = dict(importSessionAvailable=True, importSessionEpoch=1, importSessionPoint='',
                     importSessionPhase='idle', pruningImportStable=True)
        record = dict(killed=True, recovered=True,
                      trigger=dict(source='owned-file', stage='utxo-before-clear'),
                      before=dict(walStage='utxo-before-clear'), after=after,
                      afterRows=[dict(source='node-rpc')])
        J.crash_point(point, record)
        record['trigger']['stage'] = 'utxo-after-clear'
        with self.assertRaises(J.Violation): J.crash_point(point, record)
        record['trigger']['stage'] = 'utxo-before-clear'
        after['pruningImportStable'] = False
        with self.assertRaises(J.Violation): J.crash_point(point, record)


if __name__ == '__main__': unittest.main()
