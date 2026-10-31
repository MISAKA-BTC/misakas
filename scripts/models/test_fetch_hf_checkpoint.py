"""Check source pins, byte ceilings, and corrupt downloads without the Hub."""
import hashlib
import io
import unittest
import fetch_hf_checkpoint as hf

REV = 'a' * 40


def file(name, data, lfs=False):
    entry = {'rfilename': name, 'size': len(data),
             'blobId': hashlib.sha1(f'blob {len(data)}\0'.encode()+data).hexdigest()}
    if lfs:
        entry['lfs'] = {'sha256': hashlib.sha256(data).hexdigest(), 'size': len(data)}
    return entry


class SourceFetchTests(unittest.TestCase):
    def info(self):
        return {'sha': REV, 'siblings': [file('config.json', b'{}'),
                                       file('model.safetensors', b'weights', True),
                                       file('modeling_custom.py', b'never execute'),
                                       file('pytorch_model.bin', b'never unpickle')]}

    def test_only_pinned_data_files_are_selected(self):
        plan = hf.plan_files(self.info(), REV, 100)
        self.assertEqual([f['path'] for f in plan], ['config.json', 'model.safetensors'])
        for item, body in zip(plan, [b'{}', b'weights']):
            out = io.BytesIO()
            self.assertEqual(hf.stream_file(io.BytesIO(body), out, item), hashlib.sha256(body).hexdigest())
            self.assertEqual(out.getvalue(), body)

    def test_mutable_or_wrong_revision_is_refused(self):
        for rev in ['main', 'b'*40]:
            with self.assertRaisesRegex(ValueError, 'commit SHA'):
                hf.plan_files(self.info(), rev, 100)

    def test_entire_download_budget_is_checked_before_transfer(self):
        with self.assertRaisesRegex(ValueError, 'ceiling'):
            hf.plan_files(self.info(), REV, 8)

    def test_unsafe_and_duplicate_paths_are_refused(self):
        for name in ['../config.json', '/config.json', 'a/../config.json', 'a\\config.json']:
            info = self.info()
            info['siblings'].append(file(name, b'{}'))
            with self.assertRaisesRegex(ValueError, 'unsafe'):
                hf.plan_files(info, REV, 100)
        info = self.info()
        info['siblings'].append(info['siblings'][0])
        with self.assertRaisesRegex(ValueError, 'duplicate'):
            hf.plan_files(info, REV, 100)

    def test_missing_weight_identity_is_refused(self):
        info = self.info()
        del info['siblings'][1]['lfs']
        del info['siblings'][1]['blobId']
        with self.assertRaisesRegex(ValueError, 'identity'):
            hf.plan_files(info, REV, 100)

    def test_corrupt_truncated_and_oversized_sources_are_refused(self):
        for item in hf.plan_files(self.info(), REV, 100):
            original = b'{}' if item['path'] == 'config.json' else b'weights'
            for body in [original[:-1], original+b'x', b'x'*len(original)]:
                with self.assertRaises(ValueError):
                    hf.stream_file(io.BytesIO(body), io.BytesIO(), item)


if __name__ == '__main__':
    unittest.main()
