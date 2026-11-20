"""Real child-process coordination tests; these do not exercise native consensus."""
from pathlib import Path
import json
import os
import sys
import tempfile
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from protected_historical_clock import ClockBarrierError, ProtectedHistoricalClock, HEARTBEAT_INTERVAL_MS


PEER = r'''
import json, os, sys, time
path = next(arg.split('=', 1)[1] for arg in sys.argv if arg.startswith('--private-probe-clock-file='))
assert '--private-probe-profile=protected-v3' in sys.argv
assert os.environ['KASPA_TEST_CLOCK_ISOLATED'] == '1'
mode = next((arg.split('=', 1)[1] for arg in sys.argv if arg.startswith('--mode=')), 'normal')
if mode == 'permit':
    assert os.environ['MISAKA_PRIVATE_HEARTBEAT_PERMIT'] == os.path.join(os.path.dirname(path), 'heartbeat-permit.json')
else:
    assert 'MISAKA_PRIVATE_HEARTBEAT_PERMIT' not in os.environ
last = json.load(open(path))['timestampMs']
print(f'[test-clock] isolated simulated wall clock starts at {last}; monotonic timers remain real', flush=True)
while True:
    target = json.load(open(path))['timestampMs']
    if target != last:
        if mode == 'silent':
            last = target
            continue
        if mode == 'refuse':
            print('[private-historical-clock] update refused: test rejection', flush=True)
        else:
            if mode == 'split':
                print(f'[private-historical-clock] requested_ms={target}', end='', flush=True)
                time.sleep(.04)
                print(f' observed_ms={target}', flush=True)
            else:
                print(f'[private-historical-clock] requested_ms={target} observed_ms={target}', flush=True)
        last = target
    time.sleep(.01)
'''


class ClockTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name).resolve()
        self.exe = self.root / 'test-peer'
        self.exe.write_text('#!' + sys.executable + '\n' + PEER)
        self.exe.chmod(0o700)
        self.initial = time.time_ns() // 1_000_000 - 10 * HEARTBEAT_INTERVAL_MS
        self.clock = ProtectedHistoricalClock(self.root / 'run', 'b9' * 64, self.initial - 1, self.initial)

    def tearDown(self):
        self.clock.close()
        self.temp.cleanup()

    def test_all_owned_peers_acknowledge_each_step_and_close(self):
        peers = [self.clock.start_peer('A', self.exe, []),
                 self.clock.start_peer('B', self.exe, ['--mode=split'])]
        result = self.clock.seal(timeout=2)
        self.assertEqual(set(result['peers']), {'A', 'B'})
        self.assertFalse(result['chainAgreementVerified'])
        for slot in (1, 2):
            target = self.initial + slot * HEARTBEAT_INTERVAL_MS
            result = self.clock.advance_to(target, timeout=2)
            self.assertEqual(result['requestedMs'], target)
            self.assertEqual(json.loads(self.clock.path.read_text())['timestampMs'], target)
        self.assertEqual(os.stat(self.clock.path).st_mode & 0o777, 0o600)
        self.clock.close()
        self.assertTrue(all(peer.poll() is not None for peer in peers))

    def test_heartbeat_permit_is_visible_only_to_the_owned_miner(self):
        permit = self.clock.directory / 'heartbeat-permit.json'
        with self.assertRaisesRegex(ValueError, 'belong to this clock'):
            self.clock.start_peer('A', self.exe, [], heartbeat_permit=self.root / permit.name)
        self.clock.start_peer('miner', self.exe, ['--mode=permit'], heartbeat_permit=permit)
        self.clock.start_peer('verifier', self.exe, [])
        self.assertEqual(set(self.clock.seal(timeout=2)['peers']), {'miner', 'verifier'})

    def test_silent_peer_blocks_next_command_even_if_other_peer_acknowledges(self):
        self.clock.start_peer('A', self.exe, [])
        self.clock.start_peer('B', self.exe, ['--mode=silent'])
        self.clock.seal(timeout=2)
        target = self.initial + HEARTBEAT_INTERVAL_MS
        with self.assertRaisesRegex(ClockBarrierError, 'timed out: B'):
            self.clock.advance_to(target, timeout=.15)
        before = self.clock.path.read_bytes()
        with self.assertRaises(ClockBarrierError):
            self.clock.advance_to(target + 1)
        self.assertEqual(before, self.clock.path.read_bytes())
        self.assertEqual(self.clock.current, self.initial)

    def test_dead_peer_cannot_supply_stale_startup_ack_or_advance(self):
        peer = self.clock.start_peer('A', self.exe, [])
        self.clock.seal(timeout=2)
        peer.terminate()
        peer.wait(timeout=2)
        before = self.clock.path.read_bytes()
        with self.assertRaisesRegex(ClockBarrierError, 'exited'):
            self.clock.advance_to(self.initial + 1)
        self.assertEqual(before, self.clock.path.read_bytes())
        self.assertTrue(self.clock.broken)

    def test_native_refusal_stops_barrier(self):
        self.clock.start_peer('A', self.exe, ['--mode=refuse'])
        self.clock.seal(timeout=2)
        with self.assertRaisesRegex(ClockBarrierError, 'refused'):
            self.clock.advance_to(self.initial + 1, timeout=2)
        self.assertTrue(self.clock.broken)

    def test_invalid_steps_and_peer_set_changes_do_not_publish_commands(self):
        self.clock.start_peer('A', self.exe, [])
        with self.assertRaises(ClockBarrierError):
            self.clock.advance_to(self.initial + 1)
        self.clock.seal(timeout=2)
        before = self.clock.path.read_bytes()
        for target in (self.initial, self.initial - 1, self.initial + HEARTBEAT_INTERVAL_MS + 1, True):
            with self.assertRaises(ValueError):
                self.clock.advance_to(target)
        for timeout in (0, -1, float('inf'), float('nan'), 61):
            with self.assertRaises(ValueError):
                self.clock.advance_to(self.initial + 1, timeout)
        with self.assertRaises(ClockBarrierError):
            self.clock.start_peer('B', self.exe, [])
        self.assertEqual(before, self.clock.path.read_bytes())
        self.assertFalse(self.clock.broken)

    def test_existing_directory_and_profile_override_are_rejected(self):
        with self.assertRaises(FileExistsError):
            ProtectedHistoricalClock(self.clock.directory, 'b9' * 64, self.initial - 1, self.initial)
        for arg in ('--private-probe-profile=native-v1', '--private-probe-clock-file=/tmp/other',
                    '--print-private-probe-profile', '--private-probe-export-bootstrap=/tmp/export'):
            with self.assertRaises(ValueError):
                self.clock.start_peer('A', self.exe, [arg])
        self.assertFalse(self.clock.peers)


if __name__ == '__main__':
    unittest.main()
