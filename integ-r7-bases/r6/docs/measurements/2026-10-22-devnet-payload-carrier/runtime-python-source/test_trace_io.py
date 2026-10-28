"""Synthetic compressed records only; no node activity or measured throughput."""
import json
import tempfile
import unittest
from pathlib import Path
from audit import audit,audit_paths
from trace_io import iter_rows,open_trace_writer
from test_audit import observe,send,trace
class TraceIO(unittest.TestCase):
    def test_gzip_record_roundtrip_and_exclusive_creation(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'observer.jsonl.gz'
            with open_trace_writer(path) as out:
                for n in range(200):json.dump(dict(ev='hypothetical',n=n,repeated='a'*4000),out);out.write('\n');out.flush()
            records=iter_rows(path)
            self.assertEqual(sum(r['n'] for r in records),sum(range(200)))
            self.assertLess(path.stat().st_size,200*4000)
            with self.assertRaises(FileExistsError):open_trace_writer(path)
    def test_observer_is_walked_once_and_gzip_paths_match_list_semantics(self):
        class Once:
            def __init__(self):self.calls=0
            def __iter__(self):
                self.calls+=1
                if self.calls>1:raise AssertionError('second observation pass')
                yield from observe()
        once=Once();expected=audit(trace(send()),once)
        self.assertEqual(once.calls,1);self.assertTrue(expected['prerequisites_pass'])
        with tempfile.TemporaryDirectory() as directory:
            sender=Path(directory)/'sender.jsonl';observer=Path(directory)/'observer.jsonl.gz'
            sender.write_text(''.join(json.dumps(r)+'\n' for r in trace(send())))
            with open_trace_writer(observer) as out:
                for r in observe():json.dump(r,out);out.write('\n')
            self.assertEqual(audit_paths(sender,observer),expected)
    def test_truncated_gzip_and_bad_json_are_explicit_audit_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            sender=Path(directory)/'sender.jsonl';observer=Path(directory)/'observer.jsonl.gz'
            sender.write_text(''.join(json.dumps(r)+'\n' for r in trace(send())))
            with open_trace_writer(observer) as out:
                for r in observe():json.dump(r,out);out.write('\n')
            data=observer.read_bytes();observer.write_bytes(data[:-5])
            result=audit_paths(sender,observer)
            self.assertEqual(result['audit_status'],'incomplete_or_invalid_evidence')
            self.assertIsNone(result['selected_chain_accepted_attempt_ids'])
            observer.unlink()
            with open_trace_writer(observer) as out:out.write('{malformed JSON}\n')
            self.assertEqual(audit_paths(sender,observer)['audit_status'],'incomplete_or_invalid_evidence')
if __name__=='__main__':unittest.main()
