from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import TestCase
from unittest.mock import patch

import t12_repin as repin


class ReleaseDocumentPins(TestCase):
    def test_release_pin_and_readme_values_can_be_checked_and_rewritten(self):
        pins = [pin for pin in repin.registry() if pin.file in {repin.RELEASE_PINS, repin.README}]
        original = Path(repin.REPO)
        with TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name in {pin.file for pin in pins}:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((original / name).read_bytes())

            with patch.object(repin, "REPO", tmp):
                rows = repin.compare(pins, {}, {})
                self.assertEqual(len(rows), 14)
                self.assertEqual({row.status for row in rows}, {"label", "nocompute"})

                # Both the full release pins and shortened README values must see a change.
                fingerprint = "a" * 64
                schedule_id = "b" * 64
                computed = {"from.testnet-12.params_id": fingerprint, "from.testnet-12.schedule_id": schedule_id}
                rows = repin.compare(pins, computed, {})
                drift = [row for row in rows if row.status == "drift"]
                self.assertEqual({row.pin.key for row in drift},
                                 {"checklist.EXPECT_FP", "checklist.schedule_id", "readme.fp", "readme.schedule_id"})
                for row in drift:
                    row.decision = "move"
                self.assertEqual(set(repin.apply(drift, "test fixture", "01234567")), {repin.README, repin.RELEASE_PINS})
                by_key = {row.pin.key: row for row in repin.compare(pins, computed, {})}
                self.assertTrue(all(by_key[row.pin.key].status == "ok" for row in drift))
                self.assertEqual(by_key["readme.fp"].pinned, fingerprint[:16] + "…")
