"""Counterexample controls over the actual RFC12 coverage consumer; not a campaign run."""
import copy
from pathlib import Path
import re
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rfc12_acceptance_judgement as J
from rfc06_exec_origin_coverage import NATIVE_HEADER_CAP_BYTES as CAP, validate_census


def census(eligible=3, unavailable=0, oversized=0):
    return dict(cap_bytes=CAP, eligible=eligible, within_cap=eligible - unavailable - oversized,
        max_bytes_seen=CAP + 1 if oversized else (100 if eligible > unavailable else 0),
        oversized_total=oversized, unavailable_total=unavailable,
        oversized=[[format(i, '0128x'), 'bb' * 64, CAP + 1] for i in range(min(oversized, 256))],
        unavailable=[[format(i + oversized, '0128x'), 'cc' * 64] for i in range(min(unavailable, 256))])


def row(role, value):
    generation = dict(sink='aa' * 64, virtualDaaScore=100, importEpoch=7)
    envelope = dict(schemaVersion=2, kind=J.COVERAGE_KIND, consensusParamsId='22' * 32,
        generation=generation, census=value, error=None, activationApproval=False)
    return dict(source='node-rpc', role=role, dag=dict(sink=generation['sink'], virtualDaaScore=100),
        node=dict(importSessionAvailable=True, importSessionEpoch=7, pruningImportStable=True,
            importSessionPoint='', importSessionPhase='idle'), value=dict(coverage=envelope))


class NativeCoverage(unittest.TestCase):
    def test_pinned_cap_matches_native_constants_without_silent_drift(self):
        root = Path(__file__).resolve().parents[3]
        state = (root / 'consensus/core/src/palw_state_v2.rs').read_text()
        work = (root / 'consensus/core/src/palw_work_slice_v2.rs').read_text()
        def constant(name):
            match = re.search(r'pub const ' + name + r': [a-z0-9]+ = ([0-9_]+);', state)
            self.assertIsNotNone(match, 'native constant grammar changed: ' + name)
            return int(match[1].replace('_', ''))
        self.assertRegex(state, r'pub const PALW_OBJECT_CHUNK_SLOT_MASS: u64 = '
            r'PALW_OBJECT_CHUNK_MAX_BYTES as u64 \* PALW_OBJECT_CHUNK_MAX_COUNT as u64;')
        origin = re.search(r'pub const PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1: usize = '
            r'crate::palw_state_v2::PALW_OBJECT_CHUNK_SLOT_MASS as usize - ([0-9_]+);', work)
        self.assertIsNotNone(origin, 'native origin cap formula changed; review required')
        actual = constant('PALW_OBJECT_CHUNK_MAX_BYTES') * constant('PALW_OBJECT_CHUNK_MAX_COUNT') - int(origin[1].replace('_', ''))
        self.assertEqual(CAP, actual, 'native carriage rule drift requires an explicit reviewed tooling update')

    def test_matching_arbitrary_caps_cannot_supply_coverage_evidence(self):
        for invalid in (0, 4096, CAP - 1, CAP + 1, 2**64 - 1):
            archive, pruned = census(), census()
            archive['cap_bytes'] = pruned['cap_bytes'] = invalid
            with self.subTest(cap=invalid), self.assertRaisesRegex(J.NotObservable, 'pinned native V1 rule'):
                J.archive_coverage(row('B', archive), row('A', pruned), '22' * 32)

    def test_same_generation_pruned_absence_is_reported_and_not_global_absence(self):
        facts = J.archive_coverage(row('B', census()), row('A', census(unavailable=3)), '22' * 32)
        self.assertEqual(facts['archive']['within_cap'], 3)
        self.assertEqual(facts['prunedReportedNotJudged']['unavailable_total'], 3)

    def test_empty_history_does_not_exercise_the_historical_header_consumer(self):
        empty = census(eligible=0)
        validate_census(empty)  # valid observation, insufficient acceptance exercise
        with self.assertRaisesRegex(J.NotObservable, 'no eligible legacy'):
            J.archive_coverage(row('B', empty), row('A', empty), '22' * 32)
        with self.assertRaisesRegex(J.Violation, 'different eligible claims'):
            J.archive_coverage(row('B', empty), row('A', census()), '22' * 32)

    def test_counter_partition_cannot_hide_headers_behind_zero_failure_totals(self):
        for field in ('eligible', 'within_cap', 'oversized_total', 'unavailable_total'):
            bad = census()
            bad[field] += 1
            with self.subTest(field=field), self.assertRaises(J.NotObservable):
                J.archive_coverage(row('B', bad), row('A', census()), '22' * 32)

    def test_every_native_counter_is_an_exact_u64(self):
        for field in ('cap_bytes', 'eligible', 'within_cap', 'max_bytes_seen', 'oversized_total', 'unavailable_total'):
            for invalid in (True, 0.0, -1, 2**64, '3'):
                bad = census()
                bad[field] = invalid
                with self.subTest(field=field, invalid=invalid), self.assertRaises(ValueError):
                    validate_census(bad)

    def test_lists_follow_the_native_writer_cap_without_losing_complete_totals(self):
        value = census(eligible=300, unavailable=300)
        validate_census(value)
        self.assertEqual(len(value['unavailable']), 256)
        for mutation in (lambda v: v['unavailable'].pop(),
                         lambda v: v['unavailable'].append(['dd' * 64, 'ee' * 64]),
                         lambda v: v['unavailable'][0].pop(),
                         lambda v: v['unavailable'][0].__setitem__(0, 'not-a-native-hash'),
                         lambda v: v['unavailable'].__setitem__(1, v['unavailable'][0].copy())):
            bad = copy.deepcopy(value)
            mutation(bad)
            with self.assertRaises(ValueError):
                validate_census(bad)

    def test_maximum_and_named_sizes_cannot_contradict_the_carriage_cap(self):
        boundary = census()
        boundary['max_bytes_seen'] = CAP
        J.archive_coverage(row('B', boundary), row('A', boundary), '22' * 32)
        for field, value in (('cap_bytes', 0), ('max_bytes_seen', CAP + 1)):
            bad = census()
            bad[field] = value
            with self.assertRaises(ValueError):
                validate_census(bad)
        value = census(oversized=1)
        validate_census(value)
        for size in (True, -1, CAP, CAP + 2, 2**64):
            bad = copy.deepcopy(value)
            bad['oversized'][0][2] = size
            with self.assertRaises(ValueError):
                validate_census(bad)
        bad = census(eligible=3, unavailable=3)
        bad['max_bytes_seen'] = 1
        with self.assertRaises(ValueError):
            validate_census(bad)

    def test_one_claim_cannot_be_both_oversized_and_unavailable(self):
        bad = census(oversized=1, unavailable=1)
        bad['unavailable'][0][0] = bad['oversized'][0][0]
        with self.assertRaises(ValueError):
            validate_census(bad)

    def test_malformed_pruned_census_is_not_silently_reported_as_valid(self):
        bad = census(unavailable=3)
        bad['unavailable'] = []
        with self.assertRaises(J.NotObservable):
            J.archive_coverage(row('B', census()), row('A', bad), '22' * 32)
        mismatched = census()
        mismatched['cap_bytes'] *= 2
        with self.assertRaisesRegex(J.NotObservable, 'pinned native V1 rule'):
            J.archive_coverage(row('B', census()), row('A', mismatched), '22' * 32)


if __name__ == '__main__':
    unittest.main()
