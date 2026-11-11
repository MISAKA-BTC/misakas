"""Check source pins, byte ceilings, and corrupt downloads without the Hub."""
import hashlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import fetch_hf_checkpoint as hf

REV = 'a' * 40
REPO = 'owner/repository'


class Response(io.BytesIO):
    def __init__(self, url, body):
        super().__init__(body)
        self.url = url

    def geturl(self):
        return self.url


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

    def mock_hub(self, info, *, metadata_url=None):
        def urlopen(url, *, timeout):
            if '/api/models/' in url:
                return Response(metadata_url or url, json.dumps(info).encode())
            self.assertIn(f'https://huggingface.co/{REPO}/resolve/{REV}/', url)
            data = {'config.json': b'{}', 'model.safetensors': b'weights'}
            return Response(url, data[url.rsplit('/', 1)[-1]])

        return patch.object(hf.urllib.request, 'urlopen', side_effect=urlopen)

    def test_public_ungated_requires_exact_metadata_before_file_transfer(self):
        public = dict(self.info(), id=REPO, private=False, gated=False)
        variants = [dict(public, id='other/repository'), dict(public, sha='b' * 40)]
        for field in ['private', 'gated']:
            variants.append({key: value for key, value in public.items() if key != field})
            variants.extend(dict(public, **{field: value}) for value in [None, 'false', 0, True, 'auto', 'manual'])

        with tempfile.TemporaryDirectory() as root:
            out = Path(root) / 'checkpoint'
            for info in variants:
                with self.subTest(info=info), self.mock_hub(info) as opened:
                    with self.assertRaises(ValueError):
                        hf.fetch(REPO, REV, out, 100, require_public_ungated=True)
                    self.assertEqual(opened.call_count, 1)
                    self.assertFalse(out.exists())
            with self.mock_hub(public, metadata_url='https://example.com/hub') as opened:
                with self.assertRaisesRegex(ValueError, 'repository and revision URL'):
                    hf.fetch(REPO, REV, out, 100, require_public_ungated=True)
                self.assertEqual(opened.call_count, 1)
                self.assertFalse(out.exists())
            # Old callers can continue to use metadata lacking access flags.
            with self.mock_hub(self.info()):
                self.assertTrue(hf.fetch(REPO, REV, out, 100)['complete'])

    def test_exact_manifest_checks_file_plan_and_sha256_before_completion(self):
        public = dict(self.info(), id=REPO, private=False, gated=False)
        with tempfile.TemporaryDirectory() as root:
            reference = Path(root) / 'reference'
            with self.mock_hub(public):
                manifest = hf.fetch(REPO, REV, reference, 100, require_public_ungated=True)
            expected = Path(root) / 'expected.json'
            out = Path(root) / 'checkpoint'
            config, weight = manifest['files']
            mismatches = [
                dict(manifest, complete=False), dict(manifest, repository='other/repository'),
                dict(manifest, revision='b' * 40), dict(manifest, files=[config]),
                dict(manifest, files=[config, weight, weight]),
                dict(manifest, files=[config, weight, dict(weight, path='other.safetensors')]),
                dict(manifest, files=[dict(config, bytes=3), weight]),
                dict(manifest, files=[dict(config, bytes=True), weight]),
                dict(manifest, files=[dict(config, sha256='not hex'), weight]),
                dict(manifest, files=[config, dict(weight, sha256='f' * 64)]),
            ]
            for mismatch in mismatches:
                expected.write_text(json.dumps(mismatch))
                with self.subTest(mismatch=mismatch), self.mock_hub(public) as opened:
                    with self.assertRaises(ValueError):
                        hf.fetch(REPO, REV, out, 100, require_public_ungated=True, expected_source_manifest=expected)
                    self.assertEqual(opened.call_count, 1)
                    self.assertFalse(out.exists())

            # The Git blob can match the Hub while failing the independent SHA-256 pin.
            expected.write_text(json.dumps(dict(manifest, files=[dict(config, sha256='f' * 64), weight])))
            with self.mock_hub(public) as opened:
                with self.assertRaisesRegex(ValueError, 'expected source SHA-256 mismatch: config.json'):
                    hf.fetch(REPO, REV, out, 100, require_public_ungated=True, expected_source_manifest=expected)
                self.assertEqual(opened.call_count, 2)
            self.assertFalse(json.loads((out / 'source-manifest.json').read_text())['complete'])
            self.assertFalse((out / 'config.json').exists())
            self.assertFalse((out / 'config.json.partial').exists())

            expected.write_text(json.dumps(dict(manifest, files=[weight, config])))
            fresh = Path(root) / 'fresh'
            with self.mock_hub(public) as opened:
                with self.assertRaisesRegex(ValueError, 'ceiling'):
                    hf.fetch(REPO, REV, fresh, 8, require_public_ungated=True, expected_source_manifest=expected)
                self.assertEqual(opened.call_count, 1)
                self.assertFalse(fresh.exists())
            with self.mock_hub(public):
                self.assertEqual(hf.fetch(REPO, REV, fresh, 9, require_public_ungated=True,
                                          expected_source_manifest=expected), manifest)


if __name__ == '__main__':
    unittest.main()
