import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from rfc12_owned_resources import OwnedResourceSampler, ResourceLimitExceeded, check_completed_limits, read_limits


def bounds():
    # Oracle values, not a release recommendation.
    return dict(schemaVersion=1, kind='rfc12-construction-resource-bounds', maximumOwnedProcessRssKiB=512 * 1024,
                maximumOwnedDatabaseLogicalBytes=1024, maximumProbeMs=100, maximumFactorySeconds=30, samplePeriodMs=1000)


class OwnedResources(unittest.TestCase):
    def test_predeclared_schema_rejects_missing_and_boolean_limits(self):
        with tempfile.TemporaryDirectory() as folder:
            file = Path(folder) / 'limits.json'
            valid = bounds()
            file.write_text(json.dumps(valid))
            self.assertEqual(read_limits(file), valid)
            for invalid in ({**valid, 'maximumProbeMs': True}, {**valid, 'schemaVersion': True},
                            {key: value for key, value in valid.items() if key != 'maximumFactorySeconds'}):
                file.write_text(json.dumps(invalid))
                with self.assertRaises(ValueError):
                    read_limits(file)

    def test_real_owned_child_rss_survives_replacement(self):
        children = []
        sampler = OwnedResourceSampler({}, bounds())
        try:
            for _ in range(2):
                child = subprocess.Popen([sys.executable, '-c',
                    "import sys; kept=bytearray(16*1024*1024); print('ready', flush=True); sys.stdin.read(1)"],
                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
                children.append(child)
                self.assertEqual(child.stdout.readline().strip(), 'ready')
                sampler.register('A', child)
                sampler.sample_now()
                sampler.check()
                if len(children) == 1:
                    child.kill()
                    child.wait(timeout=5)
            result = sampler.stop()
            self.assertEqual(set(result['ownedChildren']), {'A'})
            row = result['ownedChildren']['A']
            self.assertEqual(row['launches'], 2)
            self.assertEqual(row['lastPid'], children[-1].pid)
            self.assertGreater(row['sampledPeakRssKiB'], 1)
            self.assertGreaterEqual(row['samples'], 2)
            self.assertFalse(result['osHardQuotaApplied'])
            self.assertFalse(result['completePeakRssMeasured'])
        finally:
            sampler.stop()
            for child in children:
                if child.poll() is None:
                    child.kill()
                child.wait(timeout=5)
                child.stdin.close()
                child.stdout.close()

    def test_predeclared_small_bound_refuses_actual_owned_child(self):
        limits = bounds()
        limits['maximumOwnedProcessRssKiB'] = 1  # negative bound, fixed before launch
        sampler = OwnedResourceSampler({}, limits)
        child = subprocess.Popen([sys.executable, '-c', "import sys; print('ready', flush=True); sys.stdin.read(1)"],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        try:
            self.assertEqual(child.stdout.readline().strip(), 'ready')
            sampler.register('MANIFEST', child)
            sampler.sample_now()
            with self.assertRaises(ResourceLimitExceeded):
                sampler.check()
            with self.assertRaises(TypeError):
                sampler.limits['maximumOwnedProcessRssKiB'] = 1024
        finally:
            sampler.stop()
            if child.poll() is None:
                child.kill()
            child.wait(timeout=5)
            child.stdin.close()
            child.stdout.close()

    def test_factory_latency_database_and_missing_samples_do_not_pass(self):
        valid = dict(factorySeconds=1, probeLatenciesMs=[10], ownedDatabaseLogicalBytes={role: 100 for role in ('A', 'B', 'JOIN')},
                     sampledOwnedResources=dict(violation=None, ownedChildren={role: {'samples': 1} for role in ('MANIFEST', 'A', 'B', 'JOIN')}))
        check_completed_limits(bounds(), valid)
        invalids = [dict(valid, factorySeconds=31), dict(valid, probeLatenciesMs=[101]),
                    dict(valid, ownedDatabaseLogicalBytes={'A': 1025, 'B': 100, 'JOIN': 100}),
                    dict(valid, ownedDatabaseLogicalBytes={'A': 100}),
                    dict(valid, sampledOwnedResources=dict(violation=None, ownedChildren={'A': {'samples': 1}}))]
        for result in invalids:
            with self.assertRaises(ResourceLimitExceeded):
                check_completed_limits(bounds(), result)


if __name__ == '__main__':
    unittest.main()
