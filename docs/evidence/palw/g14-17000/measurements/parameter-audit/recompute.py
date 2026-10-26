#!/usr/bin/env python3
"""Source-pinned arithmetic review; not empirical detection or arming evidence."""
import argparse
from datetime import datetime, timezone
from fractions import Fraction
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys


def command(*args):
    return subprocess.check_output(args, text=True).strip()


def tail(n, p, k):
    return sum(math.comb(n, i) * p**i * (1-p)**(n-i) for i in range(k, n+1))


def candidate(m, k, cap):
    false = tail(m, .25, k)
    # Independently check floating arithmetic with exact rational inputs.
    assert abs(false - float(tail(m, Fraction(1, 4), k))) < 1e-15
    a90, a85 = tail(m, .675, k), tail(m, .6375, k)
    return dict(m=m, k=k, claim_draws_per_epoch=cap, positions_per_epoch=m*cap,
                false_per_claim=false, expected_false_per_365_epochs=false*cap*365,
                honest_response_90=a90, honest_response_85=a85,
                criteria_met=false*cap*365 <= .001 and a90 >= .99 and a85 >= .95)


def minimal(cap):
    for m in range(1, 151):
        for k in range(m//2+1, m+1):
            if (tail(m, .25, k)*cap*365 <= .001 and tail(m, .675, k) >= .99
                    and tail(m, .6375, k) >= .95):
                return dict(m=m, k=k)
    raise AssertionError("no candidate in bounded search")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-ref", required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    source = command("git", "rev-parse", args.source_ref)
    paths = ["consensus/core/src/palw_security_certification_v1.rs",
             "consensus/core/src/palw_security_certification_fold_v1.rs",
             "consensus/core/src/palw_g14_values_v1.rs",
             "docs/design/palw/integration/g14-17000/lane-values-report.md"]
    sources = {p: command("git", "show", f"{source}:{p}") + "\n" for p in paths}
    evidence = []
    for path, body in sources.items():
        evidence.append(dict(path=path, sha256=hashlib.sha256(body.encode()).hexdigest(),
                             counter_lines=[dict(line=i, text=line.strip())
                                            for i, line in enumerate(body.splitlines(), 1)
                                            if "network.draws" in line]))
    policy = sources[paths[0]]
    def constant(name):
        match = re.search(r"pub const " + name + r": [^=]+ = ([\d_]+);", policy)
        assert match, name
        return int(match.group(1).replace("_", ""))
    m = constant("PALW_CERT_M_DECIDED_V1")
    k = constant("PALW_CERT_K_DECIDED_V1")
    cap = constant("PALW_CERT_DRAWS_PER_EPOCH_DESIGN_V1")
    increments = sources[paths[1]].count("network.draws = network.draws.saturating_add(1);")
    assert increments == 2, "re-review chain/native and kernel counter semantics before reusing this audit"
    assert (m, k, cap) == (75, 41, 4096), "parameters changed; re-review assumptions"
    alternatives = [candidate(m, k, cap), candidate(m, k, 54), candidate(m, k, 68),
                    candidate(88, 49, cap), candidate(89, 50, cap), candidate(90, 50, cap)]
    assert [r["criteria_met"] for r in alternatives] == [False, True, True, False, False, True]
    minima = {str(c): minimal(c) for c in (54, 4096)}
    assert minima == {"54": dict(m=75, k=41), "4096": dict(m=90, k=50)}
    epoch_daa, seconds_per_daa = 720, 184  # nominal testnet epoch and historical conversion, not a future bound
    ingress, rho, weight, cap_sessions = 1_100_000, 3125, 13, 42
    packet, fee_per_byte = 34_000, 50
    cost = ingress + rho*336*weight + 8*packet*fee_per_byte
    output = dict(
        classification="SOURCE_REVIEW_AND_CONDITIONAL_ARITHMETIC_NOT_MEASUREMENT",
        arming_evidence=False, source_commit=source, source_files=evidence,
        environment=dict(utc=datetime.now(timezone.utc).isoformat(), command=sys.argv,
                         hardware=command("sysctl", "hw.model", "hw.memsize", "hw.ncpu"),
                         uptime=command("uptime"), load=os.getloadavg(), python=sys.version,
                         python_sha256=hashlib.sha256(Path(sys.executable).read_bytes()).hexdigest(),
                         script_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest()),
        assumptions=dict(adversarial_stake_fraction=.25, epoch_daa_assumed=epoch_daa,
                         independent_stake_weighted_position_draws=True,
                         honest_response_definition="responding honest stake mass (1-f)*a, not merely average count of replying operators",
                         reference_horizon="365 epochs; not an unconditional wall-clock year bound",
                         empirical_source_independence_established=False),
        certification=dict(counter_increment=1, counter_sites=increments, counter_unit="claim draw attempt",
                           candidates=alternatives, minimal_seats_under_all_three_criteria=minima,
                           max_cap_at_75_41=math.floor(.001/(tail(75,.25,41)*365)),
                           proposed_cap=54, proposed_status="recommendation for ACT; product policy unchanged",
                           at_historical_184_seconds_per_daa=dict(
                               epoch_hours=epoch_daa*seconds_per_daa/3600,
                               cert_claims_per_wall_day=54*86400/(epoch_daa*seconds_per_daa),
                               court_originals_per_wall_day=86400/(24*seconds_per_daa))),
        carrier=dict(packet_bytes=packet, residual_bytes_per_daa=125000-111949,
                     bytes_divided_by_residual_daa=math.ceil(packet/(125000-111949)),
                     atomic_packets_fit_per_block=math.floor((125000-111949)/packet),
                     counterexample="Repeated residual 13051 cannot carry one atomic 34000-byte packet, regardless of accumulated unused bytes.",
                     reservation_caveat="A real reserved-priority implementation or legal fragmentation may remove this example; it must be demonstrated, not inferred from byte division.",
                     recommended_delay_status="4 DAA is an estimate only; insufficient for an attributable default bound"),
        economics=dict(rho_sompi=rho, weight=weight, capacity=cap_sessions,
                       full_10000_daa_jam_sompi=rho*weight*cap_sessions*10000,
                       honest_modeled_cost_sompi=cost, reporter_share_sompi=147_000_000,
                       unmeasured_cpu_io_and_other_cost_headroom_sompi=147_000_000-cost,
                       economic_profitability_measured=False),
        checks="exact rational tail cross-check and bounded exhaustive minimal-seat search passed")
    args.out.write_text(json.dumps(output, indent=2) + "\n")
    print(json.dumps({"source_commit": source, "current_criteria_met": alternatives[0]["criteria_met"],
                      "proposed": alternatives[1], "minimal_seats": minima}))


if __name__ == "__main__":
    main()
