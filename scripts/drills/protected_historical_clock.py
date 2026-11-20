"""Owned-process clock barrier for isolated protected-v3 integration.

The caller supplies the exact exported genesis and explicit node arguments.
Each requested step waits for every owned node's clock acknowledgement. This
is observation-time coordination only: the caller must separately read native
chain agreement, registrations and claim transitions before requesting a step.
No DAA, accepted row, block, funding or service capacity is synthesized here.
"""
import json
import math
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time


HEARTBEAT_INTERVAL_MS = 120_000


class ClockBarrierError(RuntimeError):
    pass


class ProtectedHistoricalClock:
    def __init__(self, directory, genesis, genesis_ms, initial_ms):
        directory = Path(directory)
        if not directory.is_absolute() or not re.fullmatch(r"[0-9a-f]{128}", genesis):
            raise ValueError("an absolute fresh directory and exact Hash64 genesis are required")
        if (type(genesis_ms) is not int or type(initial_ms) is not int
                or not 0 <= genesis_ms <= initial_ms <= time.time_ns() // 1_000_000):
            raise ValueError("initial time must be between genesis and real wall time")
        directory.mkdir(mode=0o700)
        self.directory = directory.resolve()
        self.path = self.directory / 'clock.json'
        self.genesis, self.current = genesis, initial_ms
        self.peers = {}
        self.sealed = self.broken = self.closed = False
        self._write(initial_ms)

    def _write(self, timestamp):
        body = json.dumps(dict(version=1, genesis=self.genesis, timestampMs=timestamp)).encode()
        fd, name = tempfile.mkstemp(prefix='.clock-', dir=self.directory)
        try:
            with os.fdopen(fd, 'wb') as stream:
                stream.write(body)
                stream.flush()
                os.fsync(stream.fileno())
            os.replace(name, self.path)
        finally:
            if os.path.exists(name):
                os.unlink(name)

    def start_peer(self, name, executable, arguments, *, heartbeat_permit=None):
        """Launch and retain the actual Popen handle; never attach by PID alone.

        The native entry point independently checks exact private policy,
        loopback listeners, owned appdir, real PoW and isolated-clock scope.
        """
        if self.closed or self.broken or self.sealed:
            raise ClockBarrierError('the peer set is closed')
        if not re.fullmatch(r'[A-Za-z0-9_-]{1,40}', name) or name in self.peers or len(self.peers) >= 8:
            raise ValueError('one to eight uniquely named peers are supported')
        executable = Path(executable)
        if not executable.is_absolute() or not executable.is_file():
            raise ValueError('node executable must be an absolute file')
        forbidden = ('--private-probe-profile', '--private-probe-clock-file',
                     '--private-probe-export-bootstrap', '--print-private-probe-profile', '--help', '-h')
        if any(arg.split('=', 1)[0] in forbidden for arg in arguments):
            raise ValueError('node mode, protected profile and clock are owned by the controller')
        argv = [str(executable), '--private-probe-profile=protected-v3',
                '--private-probe-clock-file=' + str(self.path), *arguments]
        path = self.directory / (name + '.log')
        env = {**os.environ, 'KASPA_TEST_CLOCK_ISOLATED': '1'}
        env.pop('MISAKA_PRIVATE_HEARTBEAT_PERMIT', None)
        if heartbeat_permit is not None:
            heartbeat_permit = Path(heartbeat_permit)
            if heartbeat_permit.parent != self.directory:
                raise ValueError('heartbeat permit must belong to this clock')
            env['MISAKA_PRIVATE_HEARTBEAT_PERMIT'] = str(heartbeat_permit)
        with path.open('xb') as output:
            child = subprocess.Popen(argv, stdin=subprocess.DEVNULL, stdout=output,
                                     stderr=subprocess.STDOUT,
                                     env=env)
        self.peers[name] = dict(child=child, reader=path.open('rb'), tail=b'', ack=None)
        return child

    def _alive(self):
        for name, peer in self.peers.items():
            code = peer['child'].poll()
            if code is not None:
                raise ClockBarrierError(f'owned peer {name} exited with {code}')

    def _wait(self, target, startup, timeout):
        if not isinstance(timeout, (int, float)) or not math.isfinite(timeout) or not 0 < timeout <= 60:
            raise ValueError('barrier timeout must be finite and between zero and 60 seconds')
        if startup:
            pattern = re.compile(rb'^\[test-clock\] isolated simulated wall clock starts at (\d+); monotonic timers remain real$')
        else:
            pattern = re.compile(rb'^\[private-historical-clock\] requested_ms=(\d+) observed_ms=(\d+)$')
        deadline = time.monotonic() + timeout
        try:
            while True:
                self._alive()
                for name, peer in self.peers.items():
                    data = peer['tail'] + peer['reader'].read(64 * 1024)
                    lines = data.split(b'\n')
                    peer['tail'] = lines.pop()
                    if len(peer['tail']) > 64 * 1024:
                        raise ClockBarrierError(f'owned peer {name} emitted an oversized log line')
                    for line in lines:
                        if b'[private-historical-clock] update refused:' in line:
                            raise ClockBarrierError(f'owned peer {name} refused the clock command')
                        match = pattern.fullmatch(line.rstrip(b'\r'))
                        if match and int(match[1]) == target:
                            if not startup and int(match[2]) < target:
                                raise ClockBarrierError(f'owned peer {name} did not observe the requested time')
                            peer['ack'] = target
                if all(peer['ack'] == target for peer in self.peers.values()):
                    self._alive()
                    return dict(requestedMs=target, peers={name: peer['child'].pid for name, peer in self.peers.items()},
                                clockAcknowledged=True, chainAgreementVerified=False)
                if time.monotonic() >= deadline:
                    missing = [name for name, peer in self.peers.items() if peer['ack'] != target]
                    raise ClockBarrierError('clock acknowledgement timed out: ' + ', '.join(missing))
                time.sleep(0.02)
        except Exception:
            # A partially observed update cannot be retried as a new step.
            # Retain the command and logs, and force an explicit fresh run.
            self.broken = True
            raise

    def seal(self, timeout=30):
        if self.closed or self.broken or self.sealed or not self.peers:
            raise ClockBarrierError('a fresh nonempty live peer set is required')
        result = self._wait(self.current, True, timeout)
        self.sealed = True
        return result

    def advance_to(self, timestamp, timeout=30):
        if self.closed or self.broken or not self.sealed:
            raise ClockBarrierError('every owned peer must acknowledge startup before stepping')
        if (type(timestamp) is not int or not self.current < timestamp <= self.current + HEARTBEAT_INTERVAL_MS
                or timestamp > time.time_ns() // 1_000_000):
            raise ValueError('advance must be positive, at most one heartbeat interval, and not in the future')
        # Validate timeout before the command is published.
        if not isinstance(timeout, (int, float)) or not math.isfinite(timeout) or not 0 < timeout <= 60:
            raise ValueError('barrier timeout must be finite and between zero and 60 seconds')
        try:
            self._alive()
            self._write(timestamp)
            result = self._wait(timestamp, False, timeout)
        except Exception:
            self.broken = True
            raise
        self.current = timestamp
        return result

    def close(self):
        if self.closed:
            return
        self.closed = True
        for peer in self.peers.values():
            if peer['child'].poll() is None:
                peer['child'].terminate()
        for peer in self.peers.values():
            try:
                peer['child'].wait(timeout=5)
            except subprocess.TimeoutExpired:
                peer['child'].kill()
                peer['child'].wait(timeout=5)
            finally:
                peer['reader'].close()

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
