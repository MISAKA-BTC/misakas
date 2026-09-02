# ADR-0035: The public PALW testnet is testnet-11, continued — and it pins its determinism class at the door

- Status: Accepted (implemented 2026-08-17; operator items listed in §6)
- Track A gate 5. Evidence base: gates 1–4 —
  `docs/palw-algo4-forgery-audit-2026-08-16.md` (gate 1),
  `docs/palw-algo4-crosshost-determinism-2026-08-16.md` (gate 2),
  `docs/palw-algo4-difficulty-economics-2026-08-16.md` (gate 3),
  `docs/palw-testnet11-soak-2026-08-16.md` (gate 4, running).

> **Superseded in part (index reconciliation, 2026-09-02).** Decision 1 — "the chain the fleet is
> soaking IS the public chain; no fresh re-genesis at announce" — held for Relaunch 1 only.
> [ADR-0042](0042-palw-mainnet-candidate-ruleset.md) (§"The two-network split") made testnet-11
> the PALW-RC and ruled that a public RC that changes any consensus rule is re-genesised as
> RC(n+1), never continued; testnet-11 has since been re-minted as Relaunch 2 through 5e
> (`docs/testnet11-relaunch2-genesis.md`, `docs/testnet11-remint-2026-08-29.md`,
> `docs/testnet11-regenesis-2026-08-30.md`, `docs/testnet11-relaunch5-runbook.md`,
> `docs/testnet11-relaunch5e-runbook.md`). The algo-4 `LegacyTn11` lane this ADR launched is not
> running anywhere: `--netsuffix=11` resolves to `palw_rc_shipped_params` (`ConsensusV2`), and the
> old `TESTNET11_PARAMS` is kept only so a node asked for it by name gets a coherent answer.
> Decision 2 (class admission pinned in code, checkable by machine) stands and is the ancestor of
> ADR-0042 Decision 11's fingerprint and ADR-0067's registration index. Map: [`README.md`](README.md).

## 1. Context

Two competing deployment plans have coexisted since the PALW work began:

- **Plan A (superseded by this ADR): t10 PALW re-genesis** —
  `docs/testnet10-palw-rollout-runbook.md`, the "-palw" re-genesis of the live
  testnet-10. Written before the Ollama (algo 5) forgery was found and before t10
  grew its present roles.
- **Plan B: a separate PALW net.** `TESTNET11_PARAMS` (staging preset, commit
  f9daebf) exists and is live on the fleet as the gate-4 soak: 120 s blocks, PALW-4
  (worker/full-logits flavor) from genesis, hash lane off, genesis bits `0x200ccccc`.

Facts that decide between them:

1. t10 today is the **hash-PoW + PoS-finality + EVM** experiment: bonded validators
   (A/B/C, 20k MSK each), the misakastake.com/MTP infrastructure, a public entry
   (A's 26211 socket bridge) — and its own unresolved wedge (DNS finality anchor
   stale). Re-genesising it for PALW destroys a working experiment to fix nothing.
2. The soak chain already demonstrates the public-testnet shape end to end: real
   inference at real difficulty, three miners on two CPU vendors, cross-host replay
   acceptance, and a launch window that behaved exactly as gate 3 predicted — the
   pre-scaled genesis bits landed **within ~10 % of converged difficulty** (first
   adjustment moved bits from `0x200ccccc` only to the `0x200c–0x200d` range): no
   trivial-bits emission burst, no stall.
3. Gate 2 measured the determinism-class boundary exactly: across 61 seeds the
   x86-64 CPU class agrees byte-for-byte on 305/305 tag fields, while Metal agrees
   on decode length 61/61, output text 47/61 and **gemm trace 0/61**. One network
   must be one class, and membership must be checkable by machine.

## 2. Decision 1 — the public PALW testnet is testnet-11, the *current chain*

The chain the fleet is soaking IS the public chain. No fresh re-genesis at
announce.

- The multi-day real-inference history is the strongest launch artifact we have:
  every header in it is a replayed pinned inference, publicly re-verifiable by any
  in-class node from genesis (from-genesis IBD is the gate-4 join test).
- Nothing spendable predates the public: soak miners run `--allow-burn`, so every
  pre-announce coinbase is provably unspendable. There is no insider accumulation
  window to explain away.
- The genesis marker already names the chain honestly (`misaka-testnet` / TN11 /
  Relaunch 1). A "clean" re-genesis would only re-run the launch window we already
  measured, and discard the measurement.
- **t10 is untouched** and keeps its own lane (its wedge is its own workstream).
  `docs/testnet10-palw-rollout-runbook.md` is superseded as a deployment plan;
  its operational content (fingerprint methodology, fleet mechanics) remains valid
  reference.

## 3. Decision 2 — class admission is pinned in code, not in a runbook

`algo_id = 5` (Ollama) had a boot-time calibration (`POW_L1_PALW_OLLAMA_CALIBRATION_V1`);
`algo_id = 4` had none — a drifted worker runtime would start happily and silently
fork itself off the network, the exact failure mode the Ollama lesson catalogued.
Closed by this ADR:

- `POW_L1_PALW_PROBE_SEED_V1` — the audited "uniform/u0" probe seed
  (`BLAKE2b-256("palw-audit-2026-08-16/uniform-0/0")`).
- `POW_L1_PALW_WORKER_CALIBRATION_TN11_V1` — the 200-byte tag that seed MUST
  produce, measured byte-identical on all four fleet hosts (Broadwell + 3× EPYC,
  two vendors, four kernels; gate-2 canonical digest `311d7eab…`).
- `palw_worker_calibration_v1(network_id)` — the per-network pin table.
  **testnet-11 pins the x86-64 CPU class; devnet deliberately pins nothing**
  (a dev net is where new classes are born); nets where algo 4 is inert never
  reach the check. Every future public PALW net must add its row before its
  activation flips.
- Enforcement is double: **eagerly** in the kaspad startup rail (one probe
  inference before any peer is dialed, with an actionable message) and **lazily,
  memoized, in `palw_l1_tag`** — the path every consumer (node validation, miner,
  pruning-proof replay) must pass through, so no harness can skip it. The fixture
  tag family is refused outright on class-pinned nets (defense in depth behind the
  existing devnet-only rail).
- These constants are **admission-side, not consensus fields**: nothing entered
  `Params`, the fingerprints did not move (662/662 consensus-core tests, pins
  unchanged) — same design position as `dns_seeders` and the Ollama pins.

Functional verification, both directions:

- an Apple-Silicon/Metal worker attempting testnet-11 is **refused at startup**
  (exit 1) — and the refusal message itself renders the class split: expected vs
  got differ **only in the gemm_trace_root bytes** (output commitment, schedule,
  token counts identical) — gates 1–2's central finding, now visible to any
  operator who tries;
- devnet boots and mines unchanged (no pin, no probe, no new messages).

## 4. Decision 3 — the participation model is honest about who can join

At launch, validating/mining testnet-11 requires being in the pinned class:

- the published **worker binary** (CPU profile
  `release/cpu-only/single-variant/no-native/no-lto/no-blas/no-openmp/threads-4/gpu-off/static/v1`,
  sha `2bd857f8…`) and **GGUF** (`Qwen3.5-2B-Q4_K_M.gguf`, sha `aaf42c8b…`,
  1_280_835_840 B);
- x86-64 hosts. The class is *measured* on Broadwell + EPYC across four kernels;
  the single-variant/no-native build is designed to keep one code path on any
  x86-64, and the calibration gate turns "designed" into "checked at your door".
- **Apple Silicon / arm cannot join at launch** (measured out of class, 0/61 gemm
  agreement). This is stated, not hidden. An arm-class port of the worker (or a
  multi-class committee design) is future work and would arrive as its own pinned
  class + ADR addendum, not as a silent widening.
- The audit harness (`scripts/misaka-palw-forgery-audit.py`) doubles as a
  self-service class check: run it, diff the jsonl against the published fleet
  values — the same methodology that produced the pin.

## 5. Economics at launch (facts an announcement must carry)

- Full schedule: **4445.62 MSK/block** (rate-preserving 120 s table,
  `YEAR1_PER_BLOCK_TWO_MINUTE`). Until validators bond, the chain mints the
  ADR-0018 §F worker BASE share only — **62 %, 2756.28 MSK/block to the miner**;
  the validator 30 % is don't-minted and the §D inclusion 8 % follows its own
  pool path (gate-3 measured this to the sompi on 118 consecutive coinbases).
- The fixed-difficulty launch window is exactly `min_difficulty_window_size = 150`
  blocks and, with the pre-scaled genesis bits, runs at roughly target cadence
  (soak-measured ~148 s), so there is no burst-emission window to disclaim.

## 6. Operator items (deliberately NOT decided in code)

1. **Discovery**: TN11 seeder records (e.g. `n11-seed*.misakascan.com`) and which
   hosts answer them; whether public nodes default to the soak port (37711) or the
   suffix default. Ports are node-local config, not consensus.
2. **Public entry**: whether A's socket-bridge pattern (t10's 26211 precedent) is
   replicated for TN11, and on which host — C cannot take inbound (ufw
   default-deny), ibm currently centers the star.
3. **Validator bonding** on TN11 (lifts emission from 62 % toward full; reuses the
   t10 bond tooling) — its own runbook when scheduled.
4. **B (95.111.236.186)**: disk 100 % full; `/root/palw` (15 G) is the cleanup
   candidate. B joins the fleet only after that operator decision.
5. **Launch criteria** before announcing (gate 6's entry checklist):
   ≥ 48 h clean soak; a fresh node's from-genesis IBD join over the public path;
   the calibration-gated binary deployed fleet-wide (next natural rebuild — the
   running soak binaries ARE the measured class; the gate exists for new joiners);
   node-operator doc published with the shas and the audit-harness check.

### Status of the gate-6 checklist, measured 2026-08-18

| item | state |
|---|---|
| ≥ 48 h clean soak | 44.7 h, 0 panics — met on schedule, nothing to do |
| a fresh node's from-genesis IBD join | **IBD half PASSED: 7 h 45 m** for a ~1,300-block chain. The *participation* half failed — see below |
| calibration-gated binary deployed fleet-wide | **NOT met.** The fleet kaspad is from 2026-08-16 13:05 and predates both the remote-panic fix and the switch-counter fix |
| node-operator doc | **published:** `docs/testnet11-node-operator.md` |
| discovery (item 1) | **NOT met.** `n11-seed*.misakascan.com` do not resolve |

**The join measured a cost this ADR did not carry, and an announcement must.** 7 h 45 m for 1.5 days
of history, sustained at ~163 headers/hour against a chain producing ~40/hour. The cost grows with
chain age until the pruning proof caps it at roughly 35–50 h (ADR-0041's per-header numbers). Budget
**1.5–2 days for a first sync on a mature chain**.

**And the join found the blocker.** The node completed its IBD and was then permanently
`quarantined`: it reached the chain-switch cap of 5 without ever having switched, because every
refused candidate advanced the counter that refused it (`switched chains 384 times`), and
`--clear-quarantine` could not recover it — the count it preserved was what re-quarantined the node
seconds later. Fixed in `fix(ibd): a refused chain switch no longer feeds the counter that refused
it`; it fires by construction on a PALW network, where a node that verifies slower than its chain
grows is permanently behind and so sees every peer as verified-better forever.

So the remaining launch work is **operational, not consensus**: rebuild and redeploy the fleet
binary with both fixes, then settle discovery. Nothing on this list now needs a code decision.

## 7. Consequences

- Track A's remaining work is now exactly gate 6: the operator items above plus
  the coordinated announce. No consensus changes remain on the critical path.
- Track B (verified-compute credits, VLT overlay activation on TN11) stays fully
  decoupled, as the original two-track decision required.
- The class-pin table is the template for any future public PALW network,
  including a mainnet decision — which this ADR explicitly does NOT make.
