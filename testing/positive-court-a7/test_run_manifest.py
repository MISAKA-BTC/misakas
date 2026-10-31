import tempfile
from pathlib import Path
import unittest
from run import final_identity_errors, required_target_errors, sha, source_snapshot, test_counts


class EvidenceIdentity(unittest.TestCase):
    def test_run_output_is_excluded_without_hiding_new_source_outside_it(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory)
            output=root/'testing/positive-court-a7/fresh-output'
            output.mkdir(parents=True)
            before=source_snapshot(root,[output])
            (output/'run.json').write_text('{"result":"partial"}')
            self.assertEqual(before,source_snapshot(root,[output]))
            (output.parent/'new.rs').write_text('concurrent source')
            self.assertNotEqual(before,source_snapshot(root,[output]))

    def test_one_passing_target_cannot_hide_an_ignored_or_missing_required_target(self):
        data = '''     Running tests/independent.rs (target/debug/deps/independent-abc)
test result: ok. 12 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out;
     Running tests/pipeline.rs (target/debug/deps/pipeline-abc)
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out;
'''
        command = ['cargo', 'test', '--test', 'independent', '--test', 'pipeline']
        counts = test_counts(data)
        self.assertEqual(required_target_errors(command, counts), ['pipeline'])
        self.assertEqual(required_target_errors(command, counts[:1]), ['pipeline'])
        data = data.replace('0 passed; 0 failed; 4 ignored', '4 passed; 0 failed; 0 ignored')
        self.assertEqual(required_target_errors(command, test_counts(data)), [])

    def test_final_identity_rejects_changed_source_set_lock_binary_and_head(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root/'testing/positive-court-a7/fixture.rs'
            source.parent.mkdir(parents=True)
            source.write_text('original')
            lock = root/'Cargo.lock'
            lock.write_text('locked')
            binary = root/'checker'
            binary.write_bytes(b'checked binary')
            record = {'commit':'abc', 'source_sha256':source_snapshot(root),
                      'cargo_lock_sha256':{'Cargo.lock':sha(lock)},
                      'a7_binary':{'path':str(binary),'sha256':sha(binary)}}
            self.assertEqual(final_identity_errors(root, record, 'abc'), [])
            source.with_name('added.rs').write_text('concurrent new source')
            self.assertIn('SOURCE_SET_OR_BYTES_CHANGED',final_identity_errors(root, record, 'abc'))
            source.with_name('added.rs').unlink()
            source.write_text('concurrent edit')
            lock.write_text('different lock')
            binary.write_bytes(b'other binary')
            errors = final_identity_errors(root, record, 'def')
            self.assertIn('HEAD_CHANGED', errors)
            self.assertIn('SOURCE_SET_OR_BYTES_CHANGED', errors)
            self.assertIn('LOCK_CHANGED:Cargo.lock', errors)
            self.assertIn('BINARY_CHANGED_OR_MISSING', errors)


if __name__ == '__main__':
    unittest.main()
