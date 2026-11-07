"""Arithmetic and deterministic toy state tests, NOT protocol/network tests."""
import copy
import json
import unittest
from capacity_planner import (BASE_DOOR, U64_MAX, LaneScheduler, Resources, ceil_div,
                              checked_add, conservative_duration, deadline, natural,
                              required_rate, wire_lower_bound, wire_only_case_limit)

class CapacityTests(unittest.TestCase):
    def test_448_kib_1000(self):
        self.assertEqual(wire_lower_bound(1000, 448*1024, 25000), 18351)
    def test_448_kib_10000(self):
        self.assertEqual(wire_lower_bound(10000, 448*1024, 25000), 183501)
    def test_64_kib_10000(self):
        self.assertEqual(wire_lower_bound(10000, 64*1024, 25000), 26215)
    def test_32_kib_10000(self):
        self.assertEqual(wire_lower_bound(10000, 32*1024, 25000), 13108)
    def test_low_bandwidth(self):
        self.assertEqual(wire_lower_bound(10000, 64*1024, 2500), 262144)
    def test_required_bandwidth(self):
        self.assertEqual(required_rate(10000, 64*1024, 3200), 204800)
    def test_wire_only_limit(self):
        self.assertEqual(wire_only_case_limit(25000, 3200, 64*1024), 1220)
    def test_ceil(self):
        self.assertEqual([ceil_div(x, 3) for x in (0,1,3,4)], [0,1,1,2])
    def test_bad_rates(self):
        for x in (0,-1,True):
            with self.assertRaises(ValueError): wire_lower_bound(1,1,x)
    def test_bad_numeric_types(self):
        for x in (True,1.5,"12",None,-1,U64_MAX+1):
            with self.assertRaises(ValueError): natural(x,"x")
    def test_checked_overflow(self):
        with self.assertRaises(ValueError): checked_add(U64_MAX,1)
    def test_no_remaining_budget(self):
        with self.assertRaises(ValueError): required_rate(1000,10,3200,3200)
    def test_total_overflow(self):
        with self.assertRaises(ValueError): wire_lower_bound(U64_MAX,2,1)
    def test_exclusive_boundary(self):
        self.assertEqual(deadline(100,3299,100), (3300,0))
        self.assertEqual(deadline(100,3300,100), (3301,1))
    def test_deadline_cap(self):
        with self.assertRaises(ValueError): deadline(0,4000,10)
    def test_terminal_before_accept(self):
        with self.assertRaises(ValueError): deadline(100,99,10)
    def test_resource_serialization(self):
        self.assertEqual(conservative_duration(Resources(21,31,41), Resources(10,10,10),5),18)
    def test_lanes_do_not_create_capacity(self):
        r=Resources(25,35,45).split(3)
        self.assertTrue(r.wire*3 <= 25 and r.cpu*3 <= 35 and r.io*3 <= 45)
    def test_too_many_lanes(self):
        with self.assertRaises(ValueError): Resources(1,1,1).split(2)

class SchedulerTests(unittest.TestCase):
    def make(self,cap=100000): return LaneScheduler(Resources(100,100,100),2,cap)
    def test_later_arrival_cannot_change_previous(self):
        s=self.make(); a=s.reserve(0,Resources(100,100,100),10)
        frozen=copy.deepcopy(a)
        for i in range(100): s.reserve(i,Resources(100,100,100),10)
        self.assertEqual(a,frozen)
    def test_atomic_failed_admission(self):
        s=self.make(0); before=s.snapshot()
        with self.assertRaises(ValueError): s.reserve(0,Resources(1000000,1000000,1000000),1000)
        self.assertEqual(s.snapshot(),before)
    def test_canonical_order(self):
        s=self.make(); s.reserve(5,Resources(1,1,1),0)
        with self.assertRaises(ValueError): s.reserve(4,Resources(1,1,1),0)
    def test_snapshot_replay(self):
        s=self.make()
        for i in range(20): s.reserve(i,Resources(100,100,100),10)
        restored=LaneScheduler.restore(json.loads(json.dumps(s.snapshot())))
        self.assertEqual(s.reserve(20,Resources(10,10,10),1), restored.reserve(20,Resources(10,10,10),1))
        self.assertEqual(s.snapshot(),restored.snapshot())
    def test_corrupt_snapshot(self):
        s=self.make().snapshot(); s['calendar']=[[1,0],[1,0]]
        with self.assertRaises(ValueError): LaneScheduler.restore(s)
    def test_10000_finite_burst(self):
        s=LaneScheduler(Resources(25000,1000000,1048576),1024,100000)
        jobs=[s.reserve(0,Resources(32768,100000,32768),1000) for _ in range(10000)]
        self.assertTrue(all(j.finish_exclusive <= j.collect_until for j in jobs))
        self.assertEqual(jobs[999].extension,0)
        self.assertGreater(jobs[-1].extension,0)
        self.assertEqual(jobs[-1].sequence,9999)
    def test_same_lane_never_overlaps(self):
        s=self.make(); ends={}
        for i in range(100):
            j=s.reserve(i//3,Resources(100+i,50,60),10)
            self.assertGreaterEqual(j.start,ends.get(j.lane,0)); ends[j.lane]=j.finish_exclusive
    def test_zero_payload_does_not_freeze_clock(self):
        s=self.make(); j=s.reserve(0,Resources(0,0,0),10)
        self.assertEqual(j.finish_exclusive-j.start,11)

if __name__=='__main__': unittest.main(verbosity=2)
