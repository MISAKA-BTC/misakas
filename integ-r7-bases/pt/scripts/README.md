# scripts/

`ci-gates.sh` is the front door: `bash scripts/ci-gates.sh [--group fast]` runs the same gates CI runs.
Everything else is grouped by purpose.

| Directory | Purpose |
| --- | --- |
| [`ci/`](ci/) | CI guards and repository checks (`ci-gates-selftest.sh`, toolchain pin, PQ guard, doc/tree/repin checks, CI summary) |
| [`release/`](release/) | Release metadata, smoke test, components manifest, the testnet-12 re-pin (`t12-repin.sh`, `t12_repin.py`) |
| [`verify/`](verify/) | Independent verification: derived-artifact conformance and third-party checks, forgery audits, class probes, consensus-vector generators |
| [`drills/`](drills/) | Devnet drills, end-to-end runs and smoke tests (PALW lanes, RFC-0001/0002/0012 drills, studio economy), the shared wRPC client `misaka_wrpc_json.py`, and their unit tests (`drills/tests/`) |
| [`ops/`](ops/) | Node operation: liveness probes and watchers, web UI start/redeploy, memory census, fleet relaunch |
| [`sim/`](sim/) | Economic and DAA simulations |

Scripts find the repository root from their own location, so run them from anywhere. The longer drill harnesses
live in [`tools/drills/`](../tools/drills/).
