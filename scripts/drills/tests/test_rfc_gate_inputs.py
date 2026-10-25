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


if __name__ == '__main__':
    unittest.main()
