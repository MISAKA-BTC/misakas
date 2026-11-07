# Fixture court verifier measurement — 2026-10-22

Status: **completed fixture selection; G14 incomplete**. The ignored harness is ported from `g14/door-design` commit `2a6ea1a4ff934635978144a84f8ec96351d2bdb8` to the current APIs. [Reuse audit](p2-branch-reuse-audit.md) records why the old designated-answer/FIFO/door-fit implementation is not silently adopted.

The five operations are honest cone close, forged-last-stage cone close, root admission, one numerical dissection round and honest bottom verification. Inputs are existing small fixture classes; each process uses its explicitly named operation. The timed verifier checks the expected native outcome. The default test remains ignored; the explicit harness iteration bound is a test allocation limit, not a consensus parameter.

`Instant` reports **wall elapsed** inside the verifier closure. Setup, execution/evidence construction, mutable phase clones and output sorting are outside that timer. Each operation has a fresh-process N=0 setup-only run and N=1,000 run under `/usr/bin/time -l`; user/sys CPU and maximum RSS include setup and test harness overhead. They are not verifier-only CPU/RSS, and no difference of two processes is asserted as an exact per-operation cost. N=0 does not construct the N-dependent phase clones.

Cases cycle when N exceeds the available distinct cases. Percentiles describe this fixture run, not independent samples of the network or an upper bound for every model/branch. `exemplar_body_bytes` serializes the first verifier body, not a maximum whole proof, signed court transport, carrier transaction, network transfer or retention footprint. The run does not establish a price schedule, physical service profile, saturation capacity, actual Panel profit, heartbeat rate or real-model door <=3,120.

The harness validates selector and iteration bounds before setup and uses checked forged-value perturbations. Exact commands, preserved source and copied executables belong to the separately named repair verification record; the initial geometry fixture failure remains preserved in its original record.


## Recorded results

All five N=1,000 operation runs produced their 1,000 expected outcomes; five separate N=0 setup runs also completed. These are benchmark invocations, not ten additional unique consensus tests. [Exact commands](fixture-verifier-commands.json), [source/build/pinned executables](numerical-preflight-repair-verification.json), [environment](fixture-verifier-environment.json) and [raw-derived summary](fixture-verifier-summary.json) preserve the scope.

| Fixture operation | Distinct cases | Median verifier wall (µs) | p99 verifier wall (µs) | N=1,000 process user+sys CPU (s), including setup |
| --- | ---: | ---: | ---: | ---: |
| `close_cone_acquit` | 984 | 350.042 | 3247.167 | 38.84 |
| `close_cone_forged_last_stage` | 984 | 350.958 | 3233.250 | 39.07 |
| `dissect_bottom_honest` | 96 | 1118.584 | 1151.125 | 44.02 |
| `dissect_round_apply` | 64 | 0.292 | 1.250 | 32.03 |
| `root_claim_admit` | 96 | 1370.875 | 1422.625 | 27.14 |

The process CPU column includes roughly tens of seconds of fixture/evidence setup per run; it must not be read as verifier CPU or as real protocol preparation cost. For example, honest close's N=0 process used 38.33 CPU seconds, while its N=1,000 process used 38.84 CPU seconds and recorded 0.544 seconds inside verifier wall timers. Independent process differences are noisy; no per-verifier CPU price is inferred. Process peak RSS and all setup baselines remain in the summary/raw logs rather than being presented as per-operation memory.

No model artifact, daemon signature/UTXO admission, paid commonfrontier, transfer, retention or saturation was measured by this harness. The earlier imported external-verification logs and capital simulations retain their distinct scope.
