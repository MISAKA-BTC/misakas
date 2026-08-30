# PALW mainnet audit, 2026-08-30 — the union build

Scope: everything the 2026-08-30 train adds over `acc430a2` (68 files, +4,593 lines) — the 10B
premine cap (ADR-0059), the liveness doctrine (ADR-0060), zero-seat genesis and right-sized
collateral (ADR-0061), and the bond-economics pass (VLT floors, escrow backing, the panel
redraw). Six adversarial passes, each given one dimension and told to substantiate or drop every
claim. Question asked of every finding: **would this hurt on a mainnet carrying real value?**

Every finding below was re-derived by hand before it was acted on; the numbers are measured, not
quoted.

## Outcome

| | found | fixed here | shipped disabled | left open |
|---|---:|---:|---:|---:|
| CRITICAL | 6 | 4 | 2 | 0 |
| HIGH | 5 | 4 | 0 | 1 |
| MEDIUM | 9 | 4 | — | 5 |

Two features are **switched off rather than shipped**: the heartbeat lane and the finality
inactivity leak. Both are ADR-0060's, both were mine, and in both cases the mechanism is sound
while the implementation cannot be fed what it needs. Turning them off is the finding, not a
workaround for it.

---

## The two that ship OFF

### The heartbeat lane (ADR-0060 D1/D2) — `PALW_HEARTBEAT_LANE_ENABLED = false`

Four structural defects, any one of which is disqualifying:

1. **It can price the bonded lane off its own chain, permanently.** Heartbeat headers carry the
   lane's own 2²⁴-hard `bits`, and those rows sit in the GLOBAL difficulty window. A V2 network's
   ambient target is `MAX_DIFFICULTY_TARGET` (work **2**) because the class lottery is its
   throttle. Measured over the shipped 264-row window: 255 bonded + 9 heartbeat rows still demands
   work 2 — but **0 bonded + 263 heartbeat rows demands 33,554,432**. After a bonded outage longer
   than the window (~14 h at the ramped cadence) a returning producer would need ~33M inferences
   per block, so no bonded block can re-enter the window and the average never re-mixes. The fixed
   point is a heartbeat-only chain recoverable only by re-mint — the self-feeding refusal the
   doctrine exists to abolish, reintroduced by its own remedy, firing in exactly the regime §4
   designs for.
2. **ε is not small against a V2 block.** Decision 1.2 argued a bonded block (~10⁶ work) dwarfs
   ε = 1. On a V2 preset `calc_work(0x207fffff) = 2`, so a heartbeat is worth **half** a bonded
   block. At `ghostdag_k = 1` a bondless attacker mining two siblings per layer accrues 2 units
   per 120 s against the honest chain's 2 — parity, for about 280 kH/s.
3. **The slot rule bounds the chain, not the DAG.** Sibling heartbeats share one POV, so they
   share one admissible timestamp and one `expected_bits`, and nothing bounds their width. The
   retarget cannot rise above the floor either: the slot rule guarantees `measured ≥ expected`,
   which the clamp turns back into the floor. Unbounded valid blocks at a permanently fixed price.
4. **The evidence walk terminates on row count, not depth**, so an archival node and a pruned node
   can compute different `expected_bits` for the same block and reject each other — a partition
   along the `--archival` flag.

Fixing 1 needs the lane's price to leave `header.bits` (the way the receipt lane's ticket already
does); 2 needs a work basis that is not the shared blue-work scale; 3 needs DAG-wide evidence;
4 needs a depth bound tied to `pruning_depth`. That is a redesign, and it must not ride a re-mint
as a surprise. The code, rules and tests stay in the tree — the integration test skips loudly —
so the redesign starts from something measured.

**Two real defects were found and fixed on the way in** and remain fixed: the walk was a remote
amplifier (PoW is checked against the header's *declared* bits, so a trivial target bought a full
chain walk for two hashes — now an O(1) floor gate runs first, and the caps are 8/2,000 instead of
32/30,000), and nothing refused a stuffed `palw_state_root` on a lane that does not commit one
(hash-invisible bytes = block-identity malleability; now `UncommittedPalwStateRoot` at the door).

### The finality inactivity leak (ADR-0060 D4) — `inactivity_leak_daa: u64::MAX` on every preset

**The implemented rule is not the intended one, by four orders of magnitude.**
`last_attestation_daa_by_validator` can only report anchors drawn from `epoch_anchor_daa`, and
that map spans `stake_score_window_blue_score` — 1500 blue score (~150 s at 10 bps) on mainnet
against a declared 7 days. So a validator absent from a two-minute window, holding a bond older
than the constant, was leaked at once; and a validator that attested at the oldest anchor could
never be leaked however long it had really been silent.

Consequence, independently derived by two passes: wired into `stake_score_since_ancestor`, whose
evidence is one BRANCH's segment, the leak let a candidate branch write its own denominator. A
2-of-12 attacker branch scored `f = 1.0` and earned full credit per epoch where before it scored
0 and could never satisfy the reorg gate's stake dimension — the §8.1 quorum-intersection
prohibition, exactly.

Three changes: dormant on every preset; the branch comparison passes `none()` **structurally**, so
a future activation cannot inherit the inversion; and the constant is no longer misfiled as an
activation fence (below). Correct activation needs PERSISTED per-validator last-attestation state,
not a windowed walk. `the_leak_cannot_be_fed_the_evidence_its_meaning_needs` pins the constraint.

---

## Fixed in place

**CRITICAL — a redrawn claim crashed every node one block later.** `rebuild_deadline_index_v2`
kept dating the bind phase from `accepted_daa` while `expected_deadline` and
`assert_deadline_consistency` had moved to `bind_base_daa()`. `into_state` runs rebuild-then-assert,
so the tip stopped loading (`CarriageInconsistent`) and the virtual processor `.expect`s it — a
crash a datadir wipe cannot fix, because re-sync re-derives it. The delta path skips the assert, so
it would instead have carried a stale deadline and voided the claim while an inline node kept it
live: the same defect wearing the reorg-divergence costume.

**CRITICAL — the redraw could never bind a second panel, so the remedy was inert.** Three sites
still derived the anchor and the bind deadline from `accepted_daa`; the second panel's anchor
therefore sat before the slot `validate_panel_bound_v2` expected (`AnchorMismatch`), and the
deadline had lapsed by construction on every shipped bundle. Every revived claim voided anyway,
its escrow burned, and silence stayed the seats' winning play.

**CRITICAL — `min_active_validators` counted BONDS, not validators.** One key registering twelve
50M bonds satisfied both the 12-seat floor and the 600M stake floor and flipped DNS to `Active`
alone, holding 100 % of the voting weight — audit H-11's refusal verbatim, and it silently
nullified the 3 → 12 re-pricing whose whole argument is that corrupting a quorum costs `ceil(2n/3)`
SEPARATE bonds. Every other counter in the file dedups; this one did not.

**CRITICAL — `inactivity_leak_daa` was registered as an activation fence.** It is a DURATION.
`consensus_identity_id` normalises fences to `0 or u64::MAX`, so 6,048,000, 5,040, 1 and "disabled"
all collapsed to one identity: two builds disagreeing about the finality denominator would have
peered normally and diverged at the first confirmation. The file states the correct classification
two entries above the mistake.

**HIGH — the bond withdrawal delay did not count the redraw.** The pruning-horizon check was
updated when the redraw landed; the invariant that decides money — "a bond cannot commit fraud and
leave before it is provable" — was not. Shipped values computed 5,700 against a 6,000 delay and
booted, while the true redrawn liability is **6,900**: a producer could retire, wait out the delay,
and take its collateral back 901 DAA before a redrawn claim's fraud stopped being provable.
Formula corrected and `WITHDRAWAL_DELAY` raised to 7,500.

**HIGH — `MAX_CLAIM_EXPOSURE_DAA` did not count the redraw either**, under-funding every derived
bond by 20 % against the lifetime it actually reserves — the block-600 wedge, re-armed and masked
only by the redraw being inert. Now 7,200, and the guard test that "proved" the old value was
tautological (`X == <its own definition>`); it now walks the state machine's arms and demands the
constant cover the longest path.

**HIGH — a zero-seat genesis silently dropped the entire ConsensusV2 bundle.** ADR-0061 retired the
panel gate, but `palw_rc_shipped_params` still keyed the whole ruleset off the bond card: emptying
the registry to mint the zero-seat genesis the ADR describes returned a **hash-only chain** with a
10B premine and nothing to spend it on, and a fleet of identical binaries fingerprints identically
and peers happily, so the one warning that fires is false exactly when it matters. The fallback is
now keyed on the artifact root — the one input code cannot mint.

**MEDIUM — ADR-0061's "≈3× collateral margin" did not exist.** Every runtime exposure ceiling reads
the *declared* collateral, which was pinned to the derived structural minimum; the surplus sitting
in the 10,000 MSK outpoint bought exactly one extra concurrent claim. The declaration is now
`max(derived, held)`, so the money that is really there is the money consensus prices.

---

## Left open, with reasons

* **`GENESIS_BOND_COLLATERAL_SOMPI` is now a ceiling on the dearest registrable genesis class**
  (10,000 MSK ⇒ `pwu_per_inference ≤ ~8.3M`; Qwen3.6 is 2.69M, a 3.1× headroom) and exceeding it
  panics inside a `From` impl — the boot path for every binary. Documented rather than changed:
  raising the carve is a supply decision, and the ADR-0059 cap is the operator's.
* **`MAX_SOMPI` is a transaction-validity rule outside the fingerprint.** Pre-existing class;
  practically unreachable (it bounds a transaction moving the entire supply).
* **`--features devnet-prealloc` disables the M-07 divergent-genesis guard on any network**,
  including mainnet. Off by default; a build-time footgun, not a runtime one.
* **`required_stake_depth` demands ≈86.7 % inclusion**, i.e. ≥11 of 12 validators attesting in
  essentially every epoch — calibrated for the 3-seat floor and not re-derived for 12. A finality
  *liveness* tuning question, and the overlay is dormant.
* **Admission item 9 prices merged attempts against an escrow they never hold** (merged work is
  applied with `escrows_reward: false`). Dormant while `min_slash_permille_of_escrow` is 0; fix
  before switching it on. Quantified: only 0 is satisfiable on the shipped bundle —
  `slash_value_per_pwu` must rise ~1,838× for 1‰, ~183,747× for the documented 100‰.

## Verification

`kaspa-consensus-core` 1,373 lib + 9 tests-target, `kaspa-consensus` 252 — read from the output,
not from exit codes. All four preset fingerprints re-derived once from the fixed tree
(testnet-11 `f3bf86b4…`); the testnet-11 genesis hash is unchanged at `d2789338…` because none of
the remediation touched the premine.
