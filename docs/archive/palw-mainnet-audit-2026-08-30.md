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

### The heartbeat lane (ADR-0060 D1/D2) — withdrawn here, redesigned by ADR-0066

> **Update 2026-08-31.** The switch this heading names no longer exists. ADR-0066 landed the
> redesign: the lane has its own algorithm id (8), its price is a network constant substituted in
> `StateLayer0::new` rather than `header.bits`, the evidence walk is deleted, and the lane is armed
> by `Params::palw_heartbeat` — a top-level fence, not a `const bool`. **F1 and F3b (below) are
> closed as arithmetic; F4 is closed by deletion. F3a is open and F2 is staged as ADR-0066
> Decision 3.** The findings below stand as the record of why.


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

### The finality inactivity leak (ADR-0060 D4) — withdrawn here, re-fenced by ADR-0066

> **Update 2026-08-31.** The leak is armed by `Params::palw_inactivity_leak`, a top-level fence
> carrying `t_leak_daa`. `DnsParams.inactivity_leak_daa` is retired at `u64::MAX` permanently and
> pinned there by a test — the CRITICAL below is closed at its cause rather than by a warning.
> The committed per-validator table (ADR-0066 Decision 4's other half) is NOT landed; the leak
> ships dormant, so nothing depends on it.


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

---

## Addendum, same day — a CRITICAL this audit did not look for

Found while answering a *design* question (ADR-0064, trustless recovery from a total stop). It is
not a consequence of that proposal; it is a property of the shipped tree. Every link was read in
the code, not inferred.

### A bond costs 0.004 MSK, lasts forever, and is enough to seat your own panels on a private fork

`palw_fork_choice` states its safety argument as: *a fork nobody could see collects no receipts, so
it has no `safe_frontier`.* That argument is false today for anyone who has ever registered one
bond.

1. **Post-genesis registration gates on the collateral floor and nothing else.**
   `min_collateral_sompi` is **400,000 sompi = 0.004 MSK**, and the collateral is refundable, so
   the real price of a permanent bond is a transaction fee.
2. **A bond never leaves the registry.** `grep -rn "write_bond([^,]*, *None"` returns **nothing** —
   every writer passes `Some(..)`. Retirement moves the status to `Retiring`; it does not remove
   the record. The right a bond confers is therefore permanent.
3. **`registered_daa` has no readers.** It is written at registration and read by **no consensus
   gate anywhere in the tree**. There is no maturity period, no soak, no "this bond was too young
   at this DAA" test — so a bond is as usable on a fork rooted before it was funded as on the
   honest chain.
4. **`PalwBondStatusV2` is `Active | Retiring`** — Active from the block that registers it.
5. **Seat tickets are `H(anchor ‖ claim ‖ bond)`** with both the anchor and the claim id
   influenced by the party constructing the fork.

**The attack.** Fork from any point at or after your bond's registration. Inside your fork's own
blocks, carry sybil `BondRegistered` objects — they fold at the accepting block under today's rules,
with no change required. Seat panels drawn from your own bonds, self-license the receipts, and grow
`safe_frontier` on a branch no honest node ever saw. The frontier is supposed to be the thing that
cannot be manufactured privately, and it is the input the deep-reorg comparator trusts.

**Why it is worse than a normal sybil bound.** The floor is not merely low, it is *retroactive and
permanent*: one 0.004 MSK registration made at any time in the network's life is a standing option
to run this at any later date, and nothing expires it.

### What closing it needs (written as ADR-0065; two of the four have landed — see the note after)

* **Seat maturity.** A bond may be drawn for a panel only if `daa - registered_daa ≥ maturity`.
  This is the rule `registered_daa` was evidently recorded for; the field already exists, so this
  costs no new state.
* **Frontier provenance.** A `safe_frontier` advance should require receipts whose panels were
  drawn from bonds that were mature **relative to the fork point**, not merely relative to the
  branch's own tip — otherwise the attacker simply roots the fork later.
* **Bond removal.** `write_bond(key, None)` having no callers means the registry only grows.
  Whether that is a leak or a deliberate append-only choice is undocumented; it must be decided,
  because "permanent" is doing load-bearing work in the attack.
* **Re-price or rate-limit registration.** 0.004 MSK is not a Sybil cost. Note that raising it
  alone does **not** close the attack — points 2 and 3 are what make one purchase permanent and
  retroactive.

It is a **mainnet blocker** and it is live on testnet-11 now.

> **Where these four ended up, and two of them did not survive contact with the code.**
>
> * **Seat maturity — LANDED, dormant** (ADR-0065 D1), behind a top-level fence. Measured from the
>   claim's own anchor rather than the binding block, so the panel stays a pure function of the
>   claim, and `validate_palw_v2` refuses a fence armed before its own window has elapsed — the
>   shipped genesis has exactly `seat_count + 1` bonds and the draw excludes the executor, so
>   arming it early would have starved every panel on the network at once.
> * **Frontier provenance — UNIMPLEMENTABLE as stated.** `safe_frontier` is written in a pure
>   single-chain fold whose result is hashed into `state_root`; a value depending on a fork point
>   would depend on which competing branch a node holds, so two nodes would compute different roots
>   for the same block. And the "otherwise the attacker simply roots the fork later" above is
>   wrong: `registered_daa` and the comparison DAA are both branch-local, so the attacker's cost is
>   to EXTEND the fork by the window — which is exactly the cost maturity was meant to impose, and
>   does. ADR-0065 D2 restates it as a comparison-site rule (which cannot cover IBD) plus an anchor
>   that needs new state.
> * **Bond removal — DECIDED: append-only, deliberately.** A retirement is a status transition, and
>   the record is the only thing that proves the withdrawal delay elapsed. A `Retiring` bond already
>   takes no seats, so "permanent" means the registration is, not the powers.
> * **Re-pricing — unchanged**, and now the only lever left on the one residual D2 named: a bond
>   pre-registered on the honest chain and held as a standing option is mature at the fork point,
>   so no provenance rule catches it.
>
> **Not a re-mint.** Both landed rules are top-level fences left `None` on every preset, so no
> shipped fingerprint moves and either can be armed by rolling deploy. The bundle placement this
> section assumed is the thing that would have partitioned testnet-11 on deploy day.

---

## Second addendum — the live chain is convicting its own honest producer

Measured on testnet-11 at sink ≈ 730, hours after the regenesis. Not a design question: this is
happening now.

### The numbers

Since the regenesis, the lifecycle objects carried in blocks tally:

| event | count |
|---|---|
| `PanelBound` | 7,587 (≈1,265 claims × 6) |
| `ReceiptLicensed` | 825 |
| `ProducerDefaulted` | **443** |
| lifecycle object **dropped** (`receipt set does not carry a quorum`) | **875** |

**35 % of every claim the chain has ever created ended in `ProducerDefaulted`,** and in the last
hour the rate was 66 defaults against 73 licences — ~47 %. Each default voids the claim: the node
logs `voided claims holding 275628448680 sompi of escrow`, i.e. **≈2,756 MSK destroyed per event**.
`final_claims` is 0, which is correct at this height (the lattice needs ≈5,400 DAA), but the claims
are not maturing toward Final — they are being convicted.

### The mechanism, read from both sides

Panel receipts carry one of two verdicts and **`Unavailable` is not a null vote — it is a positive
conviction.** From `palw_panel_v2`'s own header: *the two quorums license OPPOSITE transitions* — a
`Valid` quorum licenses `ReceiptLicensed`, an `Unavailable` quorum licenses `ProducerDefaulted`.

The shipped panel is `seat_count = 5`, `quorum = 3`.

**Host C runs exactly three seats.** And its three seats file `Unavailable` *together*, because
availability fails **per claim, not per seat**: when the trace for a claim cannot be fetched, every
remote seat sees the same nothing. In the current window C files 9/40, 8/41 and 10/41 `Unavailable`
— the same ~22 % of claims — while the producer's co-located panel on ibm files **55 Valid and 0
Unavailable** over the same window, because it reads the trace off its own disk.

So three seats on one host are **exactly quorum**, they vote as one because their failure is shared,
and they convict a producer that is serving correctly to itself. The 875 drops are the near misses:
3 Valid against 2 Unavailable reaches neither quorum, the object is dropped, "the block stands", and
the claim hangs until it times out.

### Two distinct defects, and neither is an operator error

1. **The panel draws seats, not operators.** `min_active_validators` was fixed to dedup by
   `validator_pubkey_hash`, but `derive_panel_v2` still draws per *bond*. One operator holding three
   bonds holds quorum. Combined with the CRITICAL above — bonds cost 0.004 MSK, never expire and
   never leave the registry — **quorum is purchasable for roughly a cent.** That is the same root as
   the sybil-frontier finding, reached from the liveness side instead of the safety side.

   > **Wrong as written, and the conclusion survives anyway — see the correction below and ADR-0065
   > D3 (withdrawn).** `derive_panel_v2` does *not* draw per bond: `palw_panel_v2.rs:216-228` skips
   > any bond whose `operator_id` is already seated, and `palw_state_v2.rs:4632` refuses a second
   > bond for a key already bonded. Host C's three seats are three bonds, three operator keys and
   > three bond keys — legitimate under both rules. The dedup was there the whole time; what it
   > cannot do is tell two identities from two people, because `operator_id` is a hash of a key the
   > registrant picks freely. So the sentence to keep is the last one — quorum is purchasable — and
   > the reason is a free identity namespace, not a missing dedup. Deduping harder was the proposed
   > remedy and it would have shipped a gate that enforces nothing.

2. **`Unavailable` is trusted as evidence when it is really an absence of evidence.** A seat that
   cannot fetch reports the same verdict as a seat that asked an evasive producer. The court has no
   way to tell "the producer withheld" from "I could not reach it", so unreliable transport is
   indistinguishable from fraud, and the honest producer pays. Trace serving to remote seats is
   evidently the weak link — the producer's own panel never files `Unavailable` while remote ones do.

### What this blocks

`min_slash_permille_of_escrow` is 0 on the shipped bundle, so today a default destroys escrow but
does not slash the bond. **The audit's "fix before switching it on" note about admission item 9 now
has a second, larger reason: turning slashing on over this default rate would slash honest bonds at
roughly one claim in three.** Do not enable it, and do not treat `ProducerDefaulted` counts as a
fraud signal, until availability and the seat-diversity rule are fixed.

> **Wrong, and it is the most consequential error in this addendum — the slash is not gated and is
> happening now.** `min_slash_permille_of_escrow` is read in exactly one place,
> `palw_admission_v2.rs:418`, where it is admission item 9's *collateral-backs-the-escrow* check.
> It has nothing to do with what a default costs. The `ProducerDefaulted` arm
> (`palw_state_v2.rs:5140-5155`) calls `void_and_slash(…, ProducerWithholding)`, and
> `void_and_slash` (`:3155-3164`) is `void_claim` **followed by
> `slash_bond(claim.bond, claim.reserved)`** — `claim.reserved` being `pwu × slash_value_per_pwu`,
> the figure the exposure ceiling is denominated in. The arm's own comment says so: *"this void
> takes the stake, unlike the two timeouts."*
>
> So every one of the 443 defaults debited the producer's bond, on top of voiding the escrow. The
> contrast that makes it unambiguous is the `ReceiptTimeout` sweep (`:4570`), which calls plain
> `void_claim` and slashes nobody — the two paths were written to differ in exactly this, and the
> addendum read the wrong one as the live one.
>
> Two things follow. **The harm is larger than recorded**: a relay loss does not merely destroy a
> claim's escrow, it takes collateral from the producer that served it correctly. And **the honest
> seats are being charged too** — `slash_dissenting_seats(…, true)` on the licensing arm (`:4975`)
> takes `claim.reserved` (capped at `min_collateral_sompi`) from every seat that filed
> `Unavailable` on a claim the panel went on to license, which under transport loss is the
> un-fed honest seat. `Incapable` is exempt by one `match` arm (`:3182`); `Withheld` is not.
>
> **Operational consequence, and it should be measured before anything else:** read the producer
> bond's `collateral` and `slashed` on testnet-11. The bond is capped-debited per event, so a long
> enough run of false defaults drives it under `min_collateral_sompi`, at which point
> `palw_bond_may_take_work_v2` (`:965`) stops it taking work at all — the chain would then be
> refusing its own only producer for an offence it did not commit.

Remedies belong with ADR-0065: draw seats from distinct operators (the bond-maturity work has to
touch the same draw), and either make `Unavailable` require positive proof of a refusal rather than
a failed fetch, or require the seats reporting it to be provably distinct from one another.

### Correction to the second addendum — the artifact gap was real, and it was not the cause

The addendum above named the cause as a seat-side configuration gap: the producer loaded three class
artifacts and host C's three seats loaded two. That gap was real and has been closed — the missing
`qwen25-coder-a16.palwart` was deployed to C, all three seat units now load the same three artifacts
the producer does, and the load is confirmed in each seat's log.

**It did not fix the convictions.** Measured on a clean window with all three seats up and fully
configured:

| panel | Valid | Unavailable |
|---|---|---|
| ibm (co-located with the producer) | 3 | **0** |
| C seat2 | 28 | 16 |
| C seat3 | 156 | 64 |
| C seat4 | 108 | 57 |

Still ~30 % `Unavailable` from every remote seat, and the chain still recorded 3 `ProducerDefaulted`
against 5 `ReceiptLicensed`. So the fleet fix stands on its own merits and the diagnosis it was based
on was wrong.

**The real cause is the material transport.** A seat verifies a claim from *material* it holds; when
it holds none it issues a gossip **pull** (`request_palw_material`, rate-limited to one attempt per
25 DAA), waits out half the receipt window, and then signs `Unavailable`. The producer's own panel
never needs the pull, which is exactly why it is at 0 %. Roughly a third of pulls evidently never
deliver, and **neither side logs a request, a hit, or a miss** — grepping host C for any
gossip/material/pull line over the whole post-restart window returns nothing.

So the conviction rate is a measurement of relay loss, wearing a fraud verdict's clothes.

**This strengthens ADR-0065 D4 rather than replacing it.** The remedy is not "ship the artifacts" —
that is now done and the convictions continue. It is that `Unavailable` must require positive
evidence of a refusal, because the seat cannot distinguish *the producer withheld* from *the network
did not deliver*, and under the current rule the second is punished as the first. Two additions the
measurement earns:

* **The transport must be observable before it can be trusted.** A pull that is neither logged when
  sent, when answered, nor when it times out cannot be operated, and its loss rate can only be
  inferred from convictions — which is how a third of the chain's claims were lost unnoticed.
* **Half the receipt window is one rate-limited retry.** With one pull per 25 DAA, a seat gets very
  few attempts before it must accuse. That is a tuning question only after the transport is
  observable; until then the retry budget is being spent blind.
