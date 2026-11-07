#!/usr/bin/env python3
"""RFC v2 capacity arithmetic and a conservative reservation toy model.

NOT a consensus implementation, network benchmark, or proof of actual inclusion.
Inputs are complete charged resource bounds, not payload-only estimates.
All durations are integer DAA. The toy scheduler reserves a complete lane and
serializes wire, CPU, IO and fixed waits conservatively. Production scheduling
must account for phase dependencies, service gaps, responder/verifier resources,
notices, adversarial alternatives, and control traffic as specified by the RFC.
"""
from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import heapq
import json
from pathlib import Path
from typing import Any

U64_MAX = (1 << 64) - 1
BASE_DOOR = 3_200


def natural(value: int, name: str, *, positive: bool = False) -> int:
    if isinstance(value, bool) or not isinstance(value, int):
        raise ValueError(f"{name} must be an integer")
    if value < (1 if positive else 0) or value > U64_MAX:
        raise ValueError(f"{name} is outside its permitted u64 range")
    return value


def checked_add(a: int, b: int) -> int:
    natural(a, "a"); natural(b, "b")
    return natural(a + b, "sum")


def ceil_div(a: int, b: int) -> int:
    natural(a, "numerator"); natural(b, "denominator", positive=True)
    return a // b + int(a % b != 0)


def wire_lower_bound(count: int, bytes_per_case: int, court_rate: int) -> int:
    """Only a necessary aggregate capacity bound; never a completion bound."""
    natural(count, "count"); natural(bytes_per_case, "bytes_per_case")
    total = natural(count * bytes_per_case, "total_bytes")
    return ceil_div(total, court_rate)


def required_rate(count: int, bytes_per_case: int, window: int,
                  other_daa: int = 0) -> int:
    natural(window, "window", positive=True); natural(other_daa, "other_daa")
    if other_daa >= window:
        raise ValueError("no transfer budget remains")
    natural(count, "count"); natural(bytes_per_case, "bytes_per_case")
    return ceil_div(natural(count * bytes_per_case, "total_bytes"), window - other_daa)


def wire_only_case_limit(court_rate: int, window: int, bytes_per_case: int) -> int:
    natural(court_rate, "court_rate", positive=True)
    natural(window, "window"); natural(bytes_per_case, "bytes_per_case", positive=True)
    return natural(court_rate * window, "capacity") // bytes_per_case


@dataclass(frozen=True)
class Resources:
    wire: int
    cpu: int
    io: int

    def validate(self, *, positive: bool = False) -> None:
        for name, value in asdict(self).items():
            natural(value, name, positive=positive)

    def split(self, lanes: int) -> 'Resources':
        natural(lanes, "lanes", positive=True)
        result = Resources(self.wire // lanes, self.cpu // lanes, self.io // lanes)
        result.validate(positive=True)
        return result


def conservative_duration(work: Resources, rate: Resources, fixed_daa: int) -> int:
    """Serial resource service toy, plus a final exclusive-boundary tick."""
    work.validate(); rate.validate(positive=True); natural(fixed_daa, "fixed_daa")
    duration = checked_add(fixed_daa, 1)
    for field in ("wire", "cpu", "io"):
        duration = checked_add(duration, ceil_div(getattr(work, field), getattr(rate, field)))
    return duration


def deadline(accepted: int, terminal_latest: int, max_extension: int,
             base: int = BASE_DOOR) -> tuple[int, int]:
    """Return exclusive collect_until and extension from original acceptance."""
    natural(accepted, "accepted"); natural(terminal_latest, "terminal_latest")
    natural(max_extension, "max_extension"); natural(base, "base", positive=True)
    if terminal_latest < accepted:
        raise ValueError("terminal precedes acceptance")
    normal = checked_add(accepted, base)
    collect = max(normal, checked_add(terminal_latest, 1))
    extension = collect - normal
    if extension > max_extension:
        raise ValueError("extension cap exceeded: do not admit an unfunded new promise")
    return collect, extension


@dataclass(frozen=True)
class Reservation:
    sequence: int
    lane: int
    accepted: int
    start: int
    finish_exclusive: int
    collect_until: int
    extension: int


class LaneScheduler:
    """Toy immutable FIFO reservations; later arrivals cannot delay earlier ones.

    Input order must already be a consensus-defined total order. No local clock,
    mempool observation or model/owner address grants extra capacity here.
    """
    def __init__(self, capacity: Resources, lanes: int, max_extension: int):
        capacity.validate(positive=True); natural(lanes, "lanes", positive=True)
        natural(max_extension, "max_extension")
        self.capacity = capacity
        self.lanes = lanes
        self.rate = capacity.split(lanes)
        self.max_extension = max_extension
        self._heap = [(0, lane) for lane in range(lanes)]
        heapq.heapify(self._heap)
        self.sequence = 0
        self.last_accepted = 0

    def reserve(self, accepted: int, work: Resources, fixed_daa: int) -> Reservation:
        natural(accepted, "accepted")
        if accepted < self.last_accepted:
            raise ValueError("arrival order is not canonical")
        next_free, lane = self._heap[0]
        start = max(accepted, next_free)
        finish = checked_add(start, conservative_duration(work, self.rate, fixed_daa))
        collect, extension = deadline(accepted, finish - 1, self.max_extension)
        next_sequence = checked_add(self.sequence, 1)
        # State changes only after every admission check has succeeded.
        reservation = Reservation(self.sequence, lane, accepted, start, finish, collect, extension)
        heapq.heapreplace(self._heap, (finish, lane))
        self.sequence = next_sequence
        self.last_accepted = accepted
        return reservation

    def snapshot(self) -> dict[str, Any]:
        return {"version": 1, "capacity": asdict(self.capacity), "lanes": self.lanes,
                "max_extension": self.max_extension, "sequence": self.sequence,
                "last_accepted": self.last_accepted, "calendar": sorted(self._heap)}

    @classmethod
    def restore(cls, obj: dict[str, Any]) -> 'LaneScheduler':
        if obj.get("version") != 1:
            raise ValueError("unsupported snapshot version")
        result = cls(Resources(**obj["capacity"]), obj["lanes"], obj["max_extension"])
        calendar = obj["calendar"]
        if not isinstance(calendar, list) or len(calendar) != result.lanes:
            raise ValueError("invalid calendar size")
        entries = []
        for row in calendar:
            if not isinstance(row, (list, tuple)) or len(row) != 2:
                raise ValueError("invalid calendar row")
            end, lane = row
            natural(end, "end"); natural(lane, "lane")
            entries.append((end, lane))
        if sorted(lane for _, lane in entries) != list(range(result.lanes)):
            raise ValueError("missing or duplicate lane")
        result._heap = entries; heapq.heapify(result._heap)
        result.sequence = natural(obj["sequence"], "sequence")
        result.last_accepted = natural(obj["last_accepted"], "last_accepted")
        return result


def report() -> dict[str, Any]:
    table = []
    for size in (32 * 1024, 64 * 1024, 448 * 1024):
        for count in (100, 1000, 10000):
            table.append({"count": count, "KiB_per_court": size // 1024,
                          "total_bytes": count * size,
                          "wire_lower_bound_DAA_at_25000": wire_lower_bound(count, size, 25000),
                          "wire_lower_bound_DAA_at_2500": wire_lower_bound(count, size, 2500),
                          "required_B_per_DAA_for_3200_wire_only": required_rate(count, size, 3200)})
    # Synthetic parameters chosen only to exercise ordering, congestion and restore.
    params = {"capacity": Resources(25000, 1000000, 1048576), "lanes": 1024,
              "work": Resources(32768, 100000, 32768), "fixed_daa": 1000,
              "max_extension": 100000}
    scheduler = LaneScheduler(params["capacity"], params["lanes"], params["max_extension"])
    reservations = [scheduler.reserve(0, params["work"], params["fixed_daa"]) for _ in range(10000)]
    return {"kind": "ARITHMETIC_AND_TOY_ONLY_NOT_NODE_E2E", "base_door": BASE_DOOR,
            "wire_table": table,
            "wire_only_case_limit_3200_at_25000": {str(size): wire_only_case_limit(25000, 3200, size * 1024) for size in (32,64,448)},
            "synthetic_scheduler": {"parameters": {**params, "capacity": asdict(params["capacity"]), "work": asdict(params["work"])},
                                    "count": len(reservations), "first": asdict(reservations[0]),
                                    "case_1000": asdict(reservations[999]), "last": asdict(reservations[-1]),
                                    "missed_reserved_deadlines": sum(r.finish_exclusive > r.collect_until for r in reservations)}}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, help="Write arithmetic and synthetic report JSON")
    args = parser.parse_args()
    data = json.dumps(report(), ensure_ascii=False, indent=2) + "\n"
    if args.out:
        args.out.write_text(data, encoding="utf-8")
    else:
        print(data, end="")


if __name__ == "__main__":
    main()
