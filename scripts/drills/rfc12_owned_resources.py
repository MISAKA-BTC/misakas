"""Bounded observations of only the private driver's current child processes.

RSS limits below are sampled acceptance limits, not OS memory quotas. No approved
release threshold is supplied by this module. The caller fixes any limits before
starting its manifest factory or nodes.
"""
import json
from pathlib import Path
import subprocess
import threading
from types import MappingProxyType


class ResourceLimitExceeded(RuntimeError):
    pass


def read_limits(path):
    if path is None:
        return None
    raw = Path(path).read_bytes()
    limits = json.loads(raw)
    required = {'schemaVersion', 'kind', 'maximumOwnedProcessRssKiB',
                'maximumOwnedDatabaseLogicalBytes', 'maximumProbeMs',
                'maximumFactorySeconds', 'samplePeriodMs'}
    if set(limits) != required or type(limits['schemaVersion']) is not int or limits['schemaVersion'] != 1 or limits['kind'] != 'rfc12-construction-resource-bounds':
        raise ValueError('resource limits need the exact version-1 construction schema')
    for key in required - {'schemaVersion', 'kind'}:
        if type(limits[key]) is not int or limits[key] <= 0:
            raise ValueError(key + ' must be a positive integer fixed before the run')
    if not 100 <= limits['samplePeriodMs'] <= 5000:
        raise ValueError('samplePeriodMs must be between 100 and 5000')
    return limits


class OwnedResourceSampler:
    def __init__(self, processes, limits=None):
        self.processes = processes
        self.limits = MappingProxyType(limits.copy()) if limits else None
        self.period = (limits or {}).get('samplePeriodMs', 1000) / 1000
        self.lock, self.closed = threading.Lock(), threading.Event()
        self.rows, self.failures, self.violation = {}, 0, None
        self.thread = threading.Thread(target=self._run, daemon=True)
        self.thread.start()

    def _sample(self):
        # The driver may replace a role's Popen on cold restart. Snapshot under
        # the registration lock; poll before and after ps to reject exited PIDs.
        with self.lock:
            supplied = self.processes.copy()
        current = {role: child for role, child in supplied.items() if child.poll() is None}
        if not current:
            return
        pids = ','.join(str(child.pid) for child in current.values())
        try:
            answer = subprocess.run(['ps', '-o', 'pid=,rss=', '-p', pids], capture_output=True,
                                    text=True, timeout=5, check=True).stdout
            readings = {int(parts[0]): int(parts[1]) for line in answer.splitlines()
                        if len(parts := line.split()) == 2}
        except (ValueError, OSError, subprocess.SubprocessError):
            # A sampled child may have exited during ps. A still-live child whose
            # RSS cannot be read cannot supply successful bounded-resource evidence.
            if any(child.poll() is None for child in current.values()):
                with self.lock:
                    self.failures += 1
                    if self.limits:
                        self.violation = 'RSS observation failed for an owned live child'
            return
        with self.lock:
            for role, child in current.items():
                if child.poll() is not None:
                    continue
                rss = readings.get(child.pid)
                if rss is None or rss <= 0:
                    self.failures += 1
                    if self.limits:
                        self.violation = 'RSS observation unavailable for owned live child ' + role
                    continue
                row = self.rows.setdefault(role, dict(sampledPeakRssKiB=0, lastRssKiB=0, samples=0, launches=0, lastPid=None))
                if row['lastPid'] != child.pid:
                    row['launches'] += 1
                    row['lastPid'] = child.pid
                row.update(sampledPeakRssKiB=max(row['sampledPeakRssKiB'], rss), lastRssKiB=rss,
                           samples=row['samples'] + 1)
                if self.limits and rss > self.limits['maximumOwnedProcessRssKiB']:
                    self.violation = '%s sampled RSS %d KiB exceeds fixed bound %d KiB' % (
                        role, rss, self.limits['maximumOwnedProcessRssKiB'])

    def _run(self):
        while not self.closed.is_set():
            self._sample()
            self.closed.wait(self.period)

    def register(self, role, child):
        with self.lock:
            self.processes[role] = child

    def sample_now(self):
        self._sample()

    def check(self):
        with self.lock:
            if self.violation:
                raise ResourceLimitExceeded(self.violation)

    def stop(self):
        self.closed.set()
        self.thread.join(timeout=6)
        with self.lock:
            if self.thread.is_alive():
                self.failures += 1
                if self.limits:
                    self.violation = 'owned RSS sampler did not stop within its bounded ps timeout'
            return dict(samplePeriodMs=round(self.period * 1000), ownedChildren={key: value.copy() for key, value in self.rows.items()},
                        observationFailures=self.failures, violation=self.violation,
                        osHardQuotaApplied=False, completePeakRssMeasured=False)


def check_completed_limits(limits, result):
    """Check recorded factory/read/database costs without inventing any limits."""
    if not limits:
        return
    if result['factorySeconds'] > limits['maximumFactorySeconds']:
        raise ResourceLimitExceeded('factory elapsed time exceeded the predeclared bound')
    if not result['probeLatenciesMs'] or max(result['probeLatenciesMs']) > limits['maximumProbeMs']:
        raise ResourceLimitExceeded('public RPC probe latency exceeded the predeclared bound')
    if set(result['ownedDatabaseLogicalBytes']) != {'A', 'B', 'JOIN'}:
        raise ResourceLimitExceeded('owned database measurements are incomplete')
    if any(size > limits['maximumOwnedDatabaseLogicalBytes'] for size in result['ownedDatabaseLogicalBytes'].values()):
        raise ResourceLimitExceeded('owned database logical bytes exceeded the predeclared bound')
    if result['sampledOwnedResources']['violation']:
        raise ResourceLimitExceeded(result['sampledOwnedResources']['violation'])
    for role in ('MANIFEST', 'A', 'B', 'JOIN'):
        if result['sampledOwnedResources']['ownedChildren'].get(role, {}).get('samples', 0) == 0:
            raise ResourceLimitExceeded('no owned RSS observation for ' + role)
