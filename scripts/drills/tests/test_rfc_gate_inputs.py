"""Source controls for the gate UNKNOWN cap; no campaign is executed."""
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc_gate_inputs as inputs
import rfc_integration_campaign as campaign


class GateInputCap(unittest.TestCase):
    def test_every_gate_input_names_a_real_gate_and_input(self):
        for gate, needed in inputs.GATE_INPUTS.items():
            self.assertIn(gate, campaign.GATES)
            for name in needed:
                self.assertIn(name, inputs.UNAVAILABLE)

    def test_pass_is_capped_to_unknown_but_fail_is_kept(self):
        gates = {g: dict(status='PASS', evidence=[]) for g in campaign.GATES}
        gates['rfc12-market']['status'] = 'FAIL'
        capped = inputs.cap_gates(gates)
        self.assertEqual(capped['rfc12-market']['status'], 'FAIL')
        self.assertEqual(capped['rfc06-model-boundaries-cost']['status'], 'UNKNOWN')
        self.assertEqual(capped['rfc06-model-boundaries-cost']['missingExternalInputs'][0]['cls'], 'EXTERNAL')
        self.assertEqual(capped['rfc07-income-cost']['missingExternalInputs'][0]['cls'], 'POLICY')
        self.assertEqual(capped['rfc06-routing-court']['status'], 'PASS')

    def test_manifest_cannot_supply_unavailable_inputs(self):
        with tempfile.TemporaryDirectory() as t:
            p = Path(t) / 'm.json'
            p.write_text(json.dumps(dict(schemaVersion=1, inputs=[dict(id='k3-formal-inputs', sha256='00' * 32)])))
            with self.assertRaises(inputs.InputRefused):
                inputs.load_input_manifest(p)
            p.write_text(json.dumps(dict(schemaVersion=1, inputs=[])))
            self.assertEqual(inputs.load_input_manifest(p)['inputs'], [])

    def test_ledger_keeps_failures_ignores_torn_tail_and_marks_unreached_unknown(self):
        lines = [json.dumps(dict(id='a', status='FAIL')), json.dumps(dict(id='b', status='PASS')), '{"id": "c", "sta']
        rows = {r['id']: r for r in inputs.aggregate_ledger(lines, ['a', 'b', 'c', 'd'])}
        self.assertEqual([rows[k]['status'] for k in 'abcd'], ['FAIL', 'PASS', 'UNKNOWN', 'UNKNOWN'])

    def test_later_pass_cannot_erase_an_earlier_fail(self):
        fail, ok = json.dumps(dict(id='a', status='FAIL')), json.dumps(dict(id='a', status='PASS'))
        for lines in ([fail, '{"id": "x', ok], [fail, ok]):
            with self.assertRaises(inputs.LedgerRefused):
                inputs.aggregate_ledger(lines, ['a'])

    def test_only_the_tail_may_be_torn_and_ids_stay_inside_the_plan(self):
        with self.assertRaises(inputs.LedgerRefused):
            inputs.aggregate_ledger(['{"id": "a', json.dumps(dict(id='b', status='PASS'))], ['a', 'b'])
        with self.assertRaises(inputs.LedgerRefused):
            inputs.aggregate_ledger([json.dumps(dict(id='z', status='PASS'))], ['a'])
        with self.assertRaises(inputs.LedgerRefused):
            inputs.aggregate_ledger(['[1, 2]', ''], ['a'])

    def test_aggregate_root_must_be_frozen_for_the_current_campaign(self):
        import hashlib
        freeze = dict(campaignId='c1', canonicalPlanSha256='11' * 32, candidate=dict(sha256='22' * 32))
        def root_with(stored, once_patch=None):
            t = Path(tempfile.mkdtemp())
            raw = json.dumps(stored).encode()
            (t / 'campaign-freeze.json').write_bytes(raw)
            once = dict(campaignId=stored['campaignId'], freezeSha256=hashlib.sha256(raw).hexdigest(),
                candidateSha256=stored['candidate']['sha256'])
            once.update(once_patch or {})
            (t / 'campaign-once.json').write_text(json.dumps(once))
            return t
        self.assertEqual(inputs.verify_aggregate_root(root_with(freeze), freeze)['campaignId'], 'c1')
        for other in (dict(freeze, campaignId='c2'), dict(freeze, canonicalPlanSha256='33' * 32),
                      dict(freeze, candidate=dict(sha256='44' * 32))):
            with self.assertRaises(inputs.LedgerRefused):
                inputs.verify_aggregate_root(root_with(other), freeze)
        with self.assertRaises(inputs.LedgerRefused):
            inputs.verify_aggregate_root(root_with(freeze, dict(freezeSha256='00' * 32)), freeze)
        with self.assertRaises(inputs.LedgerRefused):
            inputs.verify_aggregate_root(Path(tempfile.mkdtemp()), freeze)


if __name__ == '__main__':
    unittest.main()
