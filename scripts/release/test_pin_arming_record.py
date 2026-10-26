import json
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import pin_arming_record as P  # noqa: E402

C = 'a' * 40
G = '1' * 64


class PinTests(unittest.TestCase):
    def setUp(self):
        self.d = pathlib.Path(tempfile.mkdtemp())

    def run_json(self, name, **body):
        p = self.d / name
        p.write_text(json.dumps(dict({'exitCode': 0, 'passed': True, 'sourceCommit': C}, **body)))
        return str(p)

    def review(self):
        p = self.d / 'review.sig'
        p.write_bytes(b'review')
        return str(p)

    def test_pins_real_results(self):
        rec = P.build_record('kernel-route', 'ProbabilisticConstraintsV1', G, 17_000, C,
                             [('suite', self.run_json('s.json')), ('drill', self.run_json('d.json', run='x'))], self.review())
        self.assertEqual(len(rec['evidence']), 2)
        self.assertIn('activation_daa: 17000', P.render_rust(rec))

    def test_refuses_estimates_failures_docs_and_other_commits(self):
        bad = [self.run_json('e.json', estimate=True), self.run_json('f.json', exitCode=1),
               self.run_json('c.json', sourceCommit='b' * 40)]
        md = self.d / 'r.md'
        md.write_text('{}')
        bad.append(str(md))
        for path in bad:
            with self.assertRaises(P.PinError):
                P.build_record('g14', 'X', G, 17_000, C, [('suite', path)], self.review())

    def test_refuses_duplicate_bytes_and_review_as_evidence(self):
        s = self.run_json('s.json')
        with self.assertRaises(P.PinError):
            P.build_record('g14', 'X', G, 17_000, C, [('a', s), ('b', s)], self.review())
        with self.assertRaises(P.PinError):
            P.build_record('g14', 'X', G, 17_000, C, [('a', s)], s)


if __name__ == '__main__':
    unittest.main()
