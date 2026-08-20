# PALW free-prompt fleet drill — run of 2026-08-20

ADR-0044 FP-09. What was actually run, what it measured, and what it did **not** reach.

```bash
python3 scripts/misaka-palw-fp-fleet-drill.py \
  target/release/misaka-palw-gateway \
  target/release/misaka-palw-fp-rail \
  target/release/palw-worker \
  ~/Downloads/misaka-palw-runtime/models/Qwen3.5-2B-Q4_K_M.gguf
```

Model: the pinned `Qwen3.5-2B-Q4_K_M.gguf`. Host: Apple M-series, macOS.

## Result

| # | Step | Evidence it produces |
|---|------|----------------------|
| 1 | worker | the executor is deterministic; every malformed job shape is fail-closed |
| 2 | gateway | one real inference yields an answer **and** its commitment inputs, from one run |
| 3 | rail | that artifact becomes a signed subnetwork-`0x4a` transaction; a foreign key, an edited outbox, a late retention deadline and underfunding are each refused |
| 4 | **seam** | the transaction the rail just built is read by the **consensus** extractor and accepted by the state machine |
| 5 | chain | the V2 wiring: state walk, admission, fork-choice authority, pruning-carriage round trip |
| 6 | price | the CU weight calibration, re-measured |

All six PASS.

### Why step 4 is the one that matters

Nothing else in the tree crosses that line. The sidecar tests end at *"the bytes were written"*;
the consensus tests build their own fixtures. Two halves that never carry one side's real output
into the other's real reader agree only by construction. Step 4 takes the real transaction and
runs `palw_fp_objects_from_accepted_txs_v3` — the same extractor a block runs over its accepted
transactions — then `apply_palw_transition_v3`, and checks the claim that comes out:

```
inference: 'A Merkle tree is a data structure…'   cu=3093
transaction: 15556 bytes, cu=3093 -> 3 quanta / 300 pwu
consensus:  claim 6d0c4842deb616b2… accepted — 3 quanta, 300 pwu, immature contribution 0
```

The claim identity, the quanta and the pwu are the same on both sides, and the immature
contribution is **0** — a commitment does not pump live weight. The drill stages the commitment's
own bond as a genesis registration and says so in its output; what is under test is the boundary,
not which bond a devnet happens to register.

## Measured parameters

`scripts/misaka-palw-fp-cu-calibrate.py` fits `T(p, d) = a + b·p + c·d` over a 24-run grid.

| Backend | fixed | per prompt token (`b`) | per decode token (`c`) | `c/b` |
|---|---|---|---|---|
| CPU (the registered class) | 3.31 s | 3.35 ms | 26.81 ms | **8.0** |
| Metal (not a valid class) | 2.78 s | 0.92 ms | 10.95 ms | 11.9 |

The pricing rule is one-sided: prefill becomes a grinding lever exactly when
`prefill_weight / b > decode_weight / c`, so the table must satisfy
`CU_DECODE_WEIGHT / CU_PREFILL_WEIGHT ≥ c/b` **for every producer's hardware** — the largest
`c/b` anyone can bring is what binds, not the average.

**Verdict: 1 : 64 stands.** It clears the class's own 8.0 with ~5× headroom and the fastest
backend measured with ~5×. The headroom is not slack: prefill parallelises across cores while
decode is memory-bandwidth bound, so a wide server CPU inside the same class has a materially
higher `c/b` than the laptop this ran on, and that node is the one that would find the lever.
Moving toward exact pricing is a consensus change and needs this re-run on the widest CPU in the
class.

Quantum size, against real jobs (`QUANTUM_CU = 1000`):

```
p= 33  d= 24  ->  cu = 1_569  ->  1 quantum    (a short question)
p=100  d=128  ->  cu = 8_292  ->  8 quanta     (a paragraph of answer)
p=360  d=128  ->  cu = 8_552  ->  8 quanta
```

A handful of draws rather than one all-or-nothing ticket, and never so many that one job
dominates a window.

`WORST_CASE_COURT` is **still declared, not measured** — deliberately named as such in the source
rather than presented beside the numbers that now are. Measuring it needs an end-to-end court: a
real refutation opened against a real receipt, bisected to a step, adjudicated. That is the
constant a multi-node drill replaces first.

## Two defects the drill found

1. **The rail could not build a transaction at all.** It passed a stated placeholder as the
   bundle's `class_catalog_root`; the construction gate that checks the root against the class
   list landed later (`d10e23b7`, Unit C), and nothing re-ran the rail until this drill. Fixed by
   using the derived-root constructor. The rail smoke had been green at `2c264313` and was red
   from `d10e23b7` onward — **a real-model script that nothing runs in CI is a test that stops
   being true silently.**
2. **A correct refusal was reported as a crash.** On a Metal-backed build the worker's clean
   `exit(1)` runs a ggml atexit destructor that asserts, so the gateway reported
   `signal: 6 (SIGABRT)` and the worker's actual reason was lost. The refusal itself was intact
   (no result frame is written either way); what was broken was the diagnosis. The gateway now
   keeps the worker's last `rejected:` line and reports it. Not a consensus issue — the
   registered class is CPU — but it is what an operator would have had to debug blind.

## What this drill does not reach

The claim it produces is **Provisional**. Carrying it to `Final` needs the panel to certify it,
which needs the overlay rounds running on more than one node — and only a `Final` claim can be
spent by a receipt block. So the covered arc is

```
inference -> artifact -> signed tx -> extractor -> state machine -> Provisional claim
```

and the remaining arc is

```
Provisional -> PanelBound -> ReceiptLicensed -> Final -> receipt-spend block wins a ticket
```

Each of those needs a multi-node fleet, which is the next drill and not this one. Presenting the
first arc as a full lap would be the kind of claim this document exists to avoid.

## Preset

`consensus/core/src/palw_fp_preset_v3.rs` installs the ruleset onto a base `Params`. It is a
function, not a `const`, because a `ConsensusV2` bundle is not const-constructible and its genesis
bond's public key is a network artifact some operator holds the secret half of.

Installing moves `consensus_params_id`, so a node that installed it and one that did not cannot
handshake — an accidental half-rollout is a refused connection, not a partition. It refuses a base
that already runs PALW in any form, and refuses one carrying a legacy V1 knob, rather than
silently clearing it: two weighing rules over one chain is the shape of P0-5.

**It is installed on no shipped network**, and a test asserts that all five presets remain
dormant.
