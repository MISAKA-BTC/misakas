"""The G14 driver's operation wait must fail promptly on a live but stalled chain."""
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import g14_collusion_drill as D  # noqa: E402


class StalledChainWaitTests(unittest.TestCase):
    def test_wait_raises_when_live_chain_daa_does_not_advance(self):
        drill = D.Drill.__new__(D.Drill)
        drill.nodes = {}
        drill.args = type('Args', (), {'stall_s': 3})()
        drill.daa_quiet = lambda: 16
        start = [0.0]

        def monotonic():
            start[0] += 1.0
            return start[0]

        with patch.object(D.time, 'monotonic', side_effect=monotonic), patch.object(D.time, 'sleep'):
            with self.assertRaisesRegex(D.Blocked, 'DAA did not move for 3 s \(DAA 16'):
                drill.wait('stalled template', lambda: False, timeout_s=60, every=1)


if __name__ == '__main__':
    unittest.main()
