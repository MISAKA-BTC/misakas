# The 6,001 release: what was gated, what was measured, and the verdict

**Release candidate.** The bundle audited as `05169552` plus the fixes that audit and its own
re-audits produced. Fingerprint `3d150afd18d2367a1ed0de65d0b1c12cfe06cd8a61478354ecab27e6283855e1`;
schedule `1150, 1900, 2150, 2400, 3500, 4000, 6000, 6001, 6100, 6201, 6900`.

This is the third report on this bundle and the only one that decides. The first
([the pre-arming audit](palw-audit-2026-09-18-6001.md)) found two Critical and six High and said
NOT SAFE. The second ([the DAA-clock audit](palw-daa-clock-audit-2026-09-18.md)) is why ADR-0138
exists. This one checks the gates the operator set on the frozen candidate and answers one question:
does arming at 6,001 introduce a new Critical or High?

## 1. What changed after the candidate was frozen, and why each change was allowed

The freeze forbade new features. Five changes landed; none is one.

| commit | what | why it is not a feature |
|---|---|---|
| `ccde5885` | the heartbeat re-entered the DAA clock; the four release gates | fixes a freeze the candidate would have shipped |
| `4006bfee` | the DNS leak's evidence window covers the DAA span it is decided over | fixes a leak that could never fire |
| `d8efa139` | a V2 possession proof is no wider than the challenge names | the diff audit's one new High |
| `38f977e2` | a heartbeat ticks only where nothing `bits` priced was merged | fixes a double-count the previous fix created |
| `e31b7357` | the 390 M ceiling measured, ADR-0139 §3a | a measurement and an `#[ignore]`d bench; no rule moves |

Two more are drill and documentation only: `867cb476` and `f5c77d07` give the drill a hash lane and a
heartbeat lane, `1c4cb36d` and `17d22232` bring the operator pages and the ADR in line with the rule
that actually shipped. No consensus rule, constant or fence moves in any of the four.

## 2. The three holes this bundle's own fixes opened, and how each was found

Every one was found by re-auditing a fix rather than the bundle. This is the pattern worth naming:
a fix that closes a real hole is the most likely place for the next one, because it is the code
nobody has yet read adversarially.

* **The DAA freeze.** The first cut of ADR-0138 exempted every lane `bits` does not price, the
  heartbeat included. A chain whose hash lane stops still beats, so its score would have frozen
  while blocks kept coming — every fence, deadline, retention and leak window with it. Found live:
  the registry drill sat at virtual DAA 20 with 183 blocks accepted.
* **The heartbeat double-count.** Counting the heartbeat unconditionally is the opposite error and
  was live, not hypothetical. `heartbeat_interval_ms` falls back to the 120-second recovery interval
  whenever the selected parent is not a PALW-v2 block, and testnet-11's selected chain is 300 out of
  300 algo-3 blocks, so every parent takes that branch. testnet-11 runs a heartbeat producer on
  `.113` today. The shipped rule is per mergeset: a heartbeat gives back one exemption only where
  nothing priced was merged.
* **The DNS leak's units.** ADR-0128 decides the leak in DAA and walked its evidence in blue. Once
  the DAA clock slowed, the blue-bounded walk reached back fewer DAA than the leak is decided over,
  and no bond would ever have been leaked.

## 3. The operator's gates

**ADR-0138 re-audited against every DAA increment site.** `internal_calc_daa_score` is the only
arithmetic that produces a score, and both callers — the template's
`calc_daa_score_and_mergeset_non_daa_blocks` and validation's `block_daa_window` → `calc_daa_score` —
end in it, so a header claiming otherwise is refused by `check_difficulty_and_daa_score`. The
exemption is arithmetic on the score alone and not a `mergeset_non_daa` membership, so the coinbase
still pays exempt blocks and the PALW fold still folds their claims.

**The difficulty retarget and the DAA clock now count the same blocks.** `calculate_difficulty_bits`
has excluded the heartbeat, receipt and attempt lanes since ADR-0083 and ADR-0132 S. Until ADR-0138
the DAA score counted all three. That disagreement between the two clocks IS the window shrink the
clock audit measured; past 6,001 both count only what `bits` prices. A heartbeat header carries the
global `bits` and its own constant target is applied at validation, so an unpriced row never pollutes
the average target either.

**What is still counted in blue, and what that means.** Finality (360), merge depth (30), pruning
(~900) and the DNS attestation epoch (100) are blue-score windows. The model lane still paces blue,
so their wall-clock length is about half what the anchor cadence alone would give. None is a
chain-split risk, finality moves in the safe direction, and the numbers are in §9 of the clock audit.
This is named here because "we fixed the DAA, so the time problems are gone" is the wrong conclusion
to draw from ADR-0138, and it is the one most easily drawn.

**The registry fence on the consensus side.** A class registered before the fence has no entry in
`genesis_works`. At activation `step_model_registry` writes it an explicit inert row: `Registered`,
zero work, admission zero, so its permille returns to the room at that very boundary. The row is the
same on every node because "the build cannot describe this class" is a fact about the build, not
about the host. On the node side a registration is held until the fence is scheduled (`05169552`).
Neither path leaves a class in limbo.

**ADR-0133 §11.3's units.** `verification_window_spans` is in spans and `window_receipt()` is in DAA;
the comparison multiplies by `span_daa` before comparing. Pinned at the boundary by
`adr0133_the_window_fits_the_deadline_in_daa_at_deadline_minus_one_deadline_and_plus_one`, and the
lifecycle step is pinned separately: a class whose window does not fit is HELD from every state and
stays HELD.

**ADR-0139's burst.** The rule is pinned by unit tests — one budget per distinct round, two permits
of one round buy one, a missing round buys none, the ceiling reached exactly at 120 rounds and
saturating beyond. What those do not say is what a block AT the ceiling costs, so it was measured.

## 4. The 390 M ceiling, measured

`o13_bench_ceiling_block_apply_and_reorg`, 2026-09-18, one chain block filled to 389,991,000 gas with
18,571 transfers to fresh addresses — the most transactions the ceiling admits and the worst case for
state growth. n = 30.

| phase | min | p50 | p95 | max |
|---|---|---|---|---|
| execute the 390 M gas | 2,166 ms | 2,205 ms | 3,635 ms | 3,680 ms |
| commit (`state_root` + snapshot) | 13.5 ms | 14.3 ms | 25.4 ms | 28.2 ms |
| **apply** | **2,180 ms** | **2,219 ms** | **3,653 ms** | **3,701 ms** |
| reseed from the 18,572-account post-state | 1.3 ms | 1.6 ms | 2.5 ms | 6.4 ms |

The p95 and max columns come from a host that was compiling at the same time, so the gap from p50 is
this workload's load sensitivity, 1.7×, not a tail of the rule. Against a 120-second anchor slot a
ceiling block costs 1.8 % quiet and 3.1 % loaded. Thirty of them — the whole merge depth — re-execute
in about 67 seconds, inside one slot. The block carries 1.97 MB and leaves 1.71 MB of new state,
which is 1.23 GB a day at one ceiling block a slot: ADR-0139 §3's estimate, measured rather than
multiplied. State growth remains the binding cost and is the line to re-measure before the budget
moves.

## 5. The drill

_(filled from the run's own logs; see §6)_

## 6. Verdict

_(pending the drill)_
