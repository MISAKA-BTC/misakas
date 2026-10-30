import contextlib
import hashlib
import io
import pathlib
import re
import tempfile
import subprocess
import unittest
import gen_t12_18400_records as G

ROOT=pathlib.Path(__file__).resolve().parents[2]

class RebindTests(unittest.TestCase):
    def output(self):
        out=io.StringIO()
        with contextlib.redirect_stdout(out),contextlib.redirect_stderr(io.StringIO()):
            G.main()
        return out.getvalue()

    def test_reproducible_height_only_rebind_preserves_historical_records_and_runs(self):
        historical=ROOT/'consensus/core/src/palw_t12_17000_records_v1.rs'
        old=historical.read_bytes()
        runs={p:hashlib.sha256(p.read_bytes()).hexdigest() for p in G.HISTORICAL_RUNS_DIR.glob('*.json')}
        first=self.output()
        self.assertEqual(first,self.output())
        self.assertEqual(first,(ROOT/'consensus/core/src/palw_t12_18400_records_v1.rs').read_text())
        heights=re.findall(r'activation_daa: (\d+)',first)
        self.assertTrue(heights)
        self.assertEqual(set(heights),{'18400'})
        self.assertIn('NOT fresh 18,400-candidate runs',first)
        self.assertEqual([re.findall(r'0x[0-9a-f]+',v) for v in re.findall(r'source_commit:\s*(\[[^\]]*\])',first)], [re.findall(r'0x[0-9a-f]+',v) for v in re.findall(r'source_commit:\s*(\[[^\]]*\])',old.decode())])
        self.assertEqual(re.findall(r'kind: ([^,]+),',first),re.findall(r'kind: ([^,]+),',old.decode()))
        self.assertEqual(historical.read_bytes(),old)
        self.assertEqual(runs,{p:hashlib.sha256(p.read_bytes()).hexdigest() for p in runs})

    def test_actual_deploy_kit_refuses_unpinned_18400_template_before_any_deploy(self):
        kit=ROOT/'contrib/t12-deploy-kit'
        with tempfile.TemporaryDirectory() as tmp:
            d=pathlib.Path(tmp)
            for name in ['lib.sh','pinner-lib.sh']:
                (d/name).write_bytes((kit/name).read_bytes())
            (d/'fleet.env').write_bytes((kit/'fleet.env.example').read_bytes())
            self.assertIn('G14_FLAG_DAY_DAA=18400',(d/'fleet.env').read_text())
            proc=subprocess.run(['bash','-c','source "$1/lib.sh"; require_release','guard',str(d)],capture_output=True,text=True)
            self.assertNotEqual(proc.returncode,0)
            self.assertIn('EXPECT_FP is still a placeholder',proc.stderr)
            self.assertIn('ABORT:',proc.stderr)

if __name__=='__main__':
    unittest.main()
