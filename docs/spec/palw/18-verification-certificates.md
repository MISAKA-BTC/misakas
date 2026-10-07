# PALW spec — 18. Verification certificates: vertices, licence by tally, equivocation, `Held` leaves, security parameters, the witness manifest, the audit mesh, capped onboarding

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](../../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **Normative.** This chapter specifies RFC-0007 Parts I and III (`docs/rfc/0007-palw-verification-certificates-and-algebraic-checks.md`,
> branch `rfc7/vertex`):
> - the **verification vertex**: one signed statement per seat per round of every verdict the seat reached;
> - **licence by tally**: the fold counts a vertex's verdict leaves and licenses a claim when its panel's `Valid` leaves reach quorum;
> - **equivocation**: two vertices of one `(seat, round)` with different roots, slashed without a court;
> - **`Held` leaves** and the DA certificate;
> - the **security parameters** (`m` per interval, the escape probability of a one-point lie, the slash a sampled class needs, what a seat
>   does when its own check fails).
>
> - the **witness manifest** (§18.15): a class's witness profile pins how many trace chunks an attempt commits, so `Unavailable` can name a
>   witness chunk (`palw_witness_manifest_v1`);
> - the **audit mesh** (§18.16): post-commit audit draws over all bonded seats, `Audited` leaves, audit pay, planted traps and their
>   settlement (`palw_audit_mesh_v1`);
> - **staged (capped) onboarding** (§18.17): the `Capped` lifecycle state, the weight cap, provisional rewards and the re-verification
>   window (`palw_capped_onboarding_v1`).
>
> **Activation on testnet-12: DAA 5,300 (the int-12 flag day; RFC-0006's `palw_tir_shard_v1` and RFC-0007's four fences arm with the rest of the list).** It applies past the dormant fence `palw_verification_vertex_v1` (Parts I and III) and past each later fence for its part. Below the fence nothing here is read, the vertex tables are empty,
> and every root, carriage and delta is byte-identical to a build without it. Part II's checker itself
> is seat-local node software (§18.15.3, §18.15.5): it binds no consensus object.
>
> Status: **implemented** under the dormant fence (consensus-core `palw_vertex_v1`, the fold's child `palw_vertex_fold_v1`; node
> `kaspad/src/palw_panel/vertex.rs`). The lead's decisions of 2026-10-03 on RFC open questions 1–4 are applied (§18.0).

Contents:
- §18.0 decisions, tags and numbers;
- §18.1 objects;
- §18.2 signing;
- §18.3 admissibility;
- §18.4 the tally and the licence;
- §18.5 the path rule (migration);
- §18.6 equivocation;
- §18.7 state, root, carriage, delta;
- §18.8 `Held` leaves and the DA certificate;
- §18.9 security parameters (Part III);
- §18.10 the fence and the drill;
- §18.11 the node (informative);
- §18.12 rules PALW-VC-1…7;
- §18.13 carriage, measured;
- §18.14 tests;
- §18.15 the witness manifest and the seat-local checker (Part II);
- §18.16 the audit mesh and its traps (Part IV.1);
- §18.17 staged onboarding (Part IV.2);
- §18.18 the three fences and their drills;
- §18.19 rules PALW-AV-1…4, PALW-AM-1…4;
- §18.20 the decisions of 2026-10-03 on RFC open questions 5 to 10;
- §18.21 tests of this part.

---

## 18.0 Decisions, tags and numbers

| Decision (the lead, 2026-10-03) | Where it lands |
| --- | --- |
| `round_daa` = 1 DAA; one vertex per seat per round; a per-vertex leaf cap; no mid-round signing in v1 | `PALW_VERTEX_ROUND_DAA_V1` = 1; `PALW_VERTEX_MAX_LEAVES_V1` = 1,024; `PALW_VERTEX_MAX_LEAF_BYTES_V1` = 80,000 (§18.1, §18.3) |
| `Compact` references ship in v1 under the same fence; `Full` is also accepted | `PalwClaimRefV1` (§18.1) |
| Equivocation: a new act in the ADR-0152 per-act slash table, 100 ‰ of the bond, plus locks, plus ejection | §18.6 |
| Migration: receipts are accepted past the fence until every claim bound before it is licensed or void | §18.5 |

| Space | Allocation |
| --- | --- |
| Object tags (`PalwConsensusObjectV2`, spec 17 §17.0: the next free) | **100** `VerificationVertexV1`; **101** `VertexEquivocationV1`; **102** `TrapCommittedV1`; **103** `TrapRevealedV1` (Part IV.1) |
| Delta entry (the next free after 100, `ClassCourtWindow`; spec 17 §17.0 holds 101–103 free) | **101** `PalwDeltaEntryV2::VertexRow { table, key, old, new }`; table ids 1 rounds, 2 tallies, 3 held, **4 witness profiles, 5 audit rows, 6 trap rows, 7 capped claims** (Parts II and IV ride the same entry) |
| Carriage tail (after `0xE0`, the court windows') | **`0xE4`** the vertex tables and, in the same struct, the mesh's four |
| Root block | `vertex/v1`, after `improvement-eval/v1`: the collection roots `vertex_rounds`, `vertex_tallies`, `vertex_held`; hashed only when any table holds a row. **`mesh/v1`** after it: `mesh_witness`, `mesh_audits`, `mesh_traps`, `mesh_capped`; hashed only when any of the four holds a row |
| Signing context | `misaka-palw/verification-vertex/mldsa87/v1` |
| Hash domains | `misaka-palw/verification-vertex-message/v1`, `-leaf/v1`, `-node/v1`, `misaka-palw/vertex-equivocation-key/v1`; `misaka-palw/mesh/trap-commitment/v1`, `.../trap-committed-message/v1`, `.../trap-revealed-message/v1`, `.../audit-draw/v1`, `.../trap-slot/v1`, `.../trace-manifest/v2`; signing contexts `misaka-palw/mesh/trap-committed/mldsa87/v1`, `.../trap-revealed/mldsa87/v1` |

Object tags 100–103 are this lane's range (the lead assigned 100–109 to RFC-0007; lanes S and U hold 91–94). Delta 101 and tail `0xE4` are the lane's own (RFC-0006 took delta 102–103 and tail `0xE1` at the int-12 integration).

---

## 18.1 Objects

```text
PalwVerificationVertexV1 {
  version:     u16 = 1,
  seat_bond:   PalwBondKeyV2,
  round:       u64,                   // signed_daa / PALW_VERTEX_ROUND_DAA_V1
  signed_daa:  u64,
  leaves:      Vec<PalwVertexLeafV1>, // strictly ascending by sort key; 1..=1,024 leaves; at most 80,000 bytes
  leaves_root: Hash64,                // palw_vertex_root_v1 over leaf hashes
  signature:   Vec<u8>,               // ML-DSA-87, 4,627 bytes
}

PalwVertexLeafV1 (borsh discriminant = tag) =
  | Verdict { claim: PalwClaimRefV1, verdict: PalwReceiptVerdictV2 }                          // 0
  | Held    { claim: PalwClaimRefV1, object: u8, first: u32, last: u32, digest: Hash64 }      // 1
  | Audited { claim: PalwClaimRefV1, leaf: u64, result: u8 }                                  // 2 (Part IV.1, §18.16; `result` 0 match, 1 mismatch)

PalwClaimRefV1 (borsh discriminant) = Full(Hash64) = 0 | Compact { bound_daa: u32, id_prefix: [u8; 16] } = 1
```

- **A `Verdict` leaf is today's receipt minus its signature and minus its mask.** Its verdicts are `PalwReceiptVerdictV2`'s, unchanged
  (`Valid`, `Unavailable { chunk_index, requested_daa }`, `Incapable`, `Sampled`). A `Valid` attests exactly the seat's assigned segment
  mask (`palw_segment_assignment_v2(panel.anchor, claim, seats).mask_of(index)`), which is the only mask
  `validate_receipt_coverage_v2` takes of a `Valid`; so the leaf does not carry it. Leaf sizes: 67 bytes with a `Full` reference, 23 with
  a `Compact` one (`Unavailable` adds 12) — the RFC's 66 and 22 plus the verdict's tag byte.
- **A `Compact` reference** names the claim whose panel bound at `bound_daa` and whose id begins with `id_prefix`. It resolves to a claim
  only when exactly one panel bound at that DAA has that prefix; an unknown or ambiguous reference names nothing and its leaf is ignored,
  so a seat can only lose by using one wrongly. A DAA past 32 bits cannot be compacted (`PalwClaimRefV1::compact_of` returns `None`).
- **The sort key** of a leaf is `tag ‖ borsh(claim ref)` followed, for `Held`, by `object ‖ first ‖ last` (big-endian) and, for `Audited`, by
  `leaf`. A vertex's leaves are strictly ascending by it: a verdict is named once per claim reference, a leaf has one place.
- **An equivocation** is

  ```text
  PalwVertexEquivocationV1 { a: PalwVertexHeaderV1, b: PalwVertexHeaderV1, a_leaves: Vec<PalwVertexLeafV1>, b_leaves: Vec<PalwVertexLeafV1> }
  PalwVertexHeaderV1 { version, seat_bond, round, signed_daa, leaf_count: u32, leaves_root, signature }
  ```

  Each header is a vertex without its leaves (≈ 4.9 KB). A side's leaves ride when the filer has them, so the fold can forfeit the seat's
  locks on the claims they name (§18.6); a side with no leaves names none.

---

## 18.2 Signing

```text
vertex_message_v1 = keyed BLAKE2b-512("misaka-palw/verification-vertex-message/v1",
                      network_domain ‖ borsh(seat_bond) ‖ le64(round) ‖ le64(signed_daa) ‖ le32(|leaves|) ‖ leaves_root)
leaf_hash(leaf)   = keyed BLAKE2b-512("misaka-palw/verification-vertex-leaf/v1", borsh(leaf))
node(l, r)        = keyed BLAKE2b-512("misaka-palw/verification-vertex-node/v1", l ‖ r)
leaves_root       = binary Merkle root over leaf_hash in order, the odd last node of a level promoted unchanged; none for no leaf
context           = "misaka-palw/verification-vertex/mldsa87/v1"
```

`network_domain` is `palw_network_domain_v2_for(network id, genesis hash)`, as every PALW signature's. The message binds the count and the
round, so a relayer can neither truncate a vertex nor move it to another round. The four domains are new and are in the uniqueness test every
PALW family runs; a vertex signature can be neither a receipt's (`PALW_RECEIPT_V2/V3_MLDSA87_CONTEXT`) nor a window root's nor another court
move's. They are not in testnet-12's committed context set (which sits inside the genesis ruleset id); like the batch licence's they are
covered by the Some-only fence that gates every object signed under them.

---

## 18.3 Admissibility

`palw_vertex_admissible_v1(state, vertex, daa)` — one function, called by the acceptance walk and by the fold:

1. **Shape (PALW-VC-1).** `version` = 1; `round` = `signed_daa / round_daa`; at least one leaf and at most 1,024; leaves at most 80,000
   bytes; strictly ascending sort keys; an `Audited` leaf's `result` is 0 or 1 (`AuditResultUnknown`), and an `Audited` leaf rides only past `palw_audit_mesh_v1` (`AuditedLeafNotArmed`, by the acceptance gate and the fold); a `Held` leaf names
   object 0 (capture), 1 (witness) or 2 (trace manifest) and a range `first ≤ last`; `leaves_root` recomputes; the signature is 4,627 bytes.
2. **Clock.** `signed_daa ≤ daa` of the carrying block, and `daa − signed_daa ≤ PALW_VERTEX_MAX_CARRY_DAA_V1` (240): a vertex that waited
   longer is refused, so the chain need only remember a `(seat, round)` for a bounded time.
3. **Registry.** The seat bond is registered.
4. **One vertex per seat per round.** No row for `(round, seat)` exists (a vertex or a conviction).

Then the acceptance walk verifies the one signature (PALW-VC-2): the bond's registered ML-DSA-87 key, over `vertex_message_v1`, under the
vertex context. Every other condition is the fold's, per leaf (§18.4), and a leaf that does not count is **ignored, never refused**: a
vertex is the seat's whole round, and one wrong leaf must not cost it the rest.

A refused vertex is dropped by the acceptance walk with the block standing (it is a payload a block may carry; an older build skips it);
below the fence the walk drops it by name before any slot or budget is charged, and the fold refuses it as the second lock.

---

## 18.4 The tally and the licence

For each `Verdict` leaf of an accepted vertex, in order, the fold resolves the claim (§18.1) and asks
`palw_vertex_leaf_fate_v1(state, params, daa, seat, signed_daa, claim, verdict)`. A leaf **counts** when all of these hold
(PALW-VC-3: they are the conditions `validate_receipt_coverage_v2` applies to a receipt today, plus the path rule §18.5):

- the claim has a panel and a claim record, and its panel bound at or after the fence (§18.5);
- the claim's class does not license by shard parts;
- the vertex's seat holds a seat on the claim's panel;
- `signed_daa` is at or after the panel's bind, at or before the claim's receipt deadline (`palw_claim_receipt_deadline_v1`) and not after
  the carrying block;
- `Incapable` is not pleaded on the liveness floor; `Sampled` needs `palw_rcore_plus`;
- the claim is `PanelBound` and this is the seat's **first counted verdict** for it (PALW-VC-4: a verdict stands — a later leaf of the same
  seat is ignored, whatever it says); **or** the claim is `ReceiptLicensed` past `palw_rcore_plus`, the leaf is a `Valid` or `Sampled`, the
  seat is on the claim's duty row uncredited and holds no lock on it, and the receipt window is open (the leaf is a supplementary receipt,
  ADR-0124's door).

A counted leaf on a `PanelBound` claim joins the claim's **tally** (`vertex_tallies`: the seat, the verdict, the vertex's `signed_daa`, in
counting order). When the tally holds the colluding quorum (`PALW_PANEL_COLLUDING_QUORUM_V1`, three) of `Valid`s, the fold applies the
licence — **the licence is the tally; no licence object is carried**:

1. The counted leaves are expanded, in the panel's seat order, into the `Vec<PalwSeatReceiptV3>` a `ReceiptLicensedV2` of the same seats
   would carry (`palw_vertex_receipts_of_v1`): empty signatures, a `Valid`'s mask the seat's assigned one, other verdicts' mask none.
2. If the claim is outsider-judged and the outsider has not answered `Valid`, nothing happens (the refusal a carried licence meets at
   acceptance; the tally waits).
3. The set is fed to the same fold arm a `ReceiptLicensedV2` folds through (`apply_receipt_licensed_v2`), so the licence, its door record
   (`Coverage`), its recount, its locks, its credit and its escrow are the receipt path's, record for record. On R-core+ the coverage door
   needs every segment attested twice, which is the full seat and the four partial seats of a five-seat panel: the tally licenses when
   the fifth `Valid` lands, exactly as the receipt path does. A tally the arm declines (not backed, the door does not hold) stays, and the
   next counted leaf tries again.

A claim's tally is dropped in the same write that moves the claim out of `PanelBound` — licensed, redrawn to `Provisional`, voided —
(`write_claim`, the one site all of those pass). `Unavailable` leaves abstain (`palw_unavailable_abstains` is a prerequisite): they are
counted as the seat's answer and decide nothing; a claim that gathers no quorum redraws once and voids at `ReceiptTimeout`, as it does.

---

## 18.5 The path rule (migration)

One predicate, `palw_vertex_claim_licenses_by_tally_v1(state, params, daa, claim)`: the fence is active at `daa` **and** the claim's panel
bound at or after the fence's height. Everything about migration follows from it (the lead's decision of 2026-10-03, RFC question 4):

- A claim whose panel bound **at or after** the fence licenses by tally. A receipt-path licence object for it — `ReceiptLicensed`,
  `ReceiptLicensedV2`, `OptimisticLicensed`, `ProducerDefaulted` — is refused by name by the fold ("licenses by tally") and dropped by the
  acceptance walk; a `ReceiptLicensedBatchV1` entry for it is inert, as an entry for a claim that is gone.
- A claim whose panel bound **before** the fence licenses on the receipt path, as it always did, until it is licensed or void: receipts are
  accepted past the fence for exactly those claims, so nothing is stranded. A leaf naming it counts for nothing.
- The two paths never count one seat on one claim twice. A claim redrawn after the fence binds a new panel and takes the path its new bind
  height says.
- The consensus assemblers (`palw_v2_*_assemble`) offer nothing for a claim that licenses by tally.

---

## 18.6 Equivocation

`VertexEquivocationV1` is admissible (`palw_vertex_equivocation_admissible_v1`) when:

- both headers are version 1 with the round of their `signed_daa` and a 4,627-byte signature;
- they name the same seat, the same round, and **different** `leaves_root`;
- each side's carried leaves, if any, number the header's `leaf_count`, are well-formed (§18.3) and root to the header's root;
- the round is still provable: `daa − signed_daa ≤ PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1` (1,200) for both;
- the seat bond is registered; and the pair is not already convicted.

The acceptance walk additionally verifies **both signatures** under the seat's registered key (PALW-VC-5): two signatures over one round
are the whole proof, and no court is needed. The fold then, in this order:

1. resolves the claims named by either side's `Verdict` and `Held` leaves;
2. charges the seat `PALW_VERTEX_EQUIVOCATION_PENALTY_PERMILLE_V1` = **100 ‰ of its collateral**, plus **every lock** it holds on a named claim
   (each lock removed and its amount added to the debit);
3. slashes the total from the bond (`slash_bond`: clamped at the collateral, burned, recorded in `slashed`);
4. **ejects** the bond: an `Active` bond becomes `Retiring { since_daa, settled_at_since }` — it backs its existing claims to resolution
   and takes no new work (the retirement path of ADR-0042 Decision 6 item 9); and
5. writes the `(round, seat)` row with `convicted = true` (keeping the first accepted vertex's root and DAA if a row exists).

A convicted `(round, seat)` is convicted once, and no vertex of that round lands afterwards (rule 4 of §18.3). Counted leaves of the
equivocator stand (PALW-VC-4). The penalty is a new act in the ADR-0152 per-act table beside `Eq` (the executor's equivocation, `min(C₀,
3·G_eq)`): this one is a flat 100 ‰ because a vertex's damage is not a claim's gain.

Rows are kept `PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1` past their round and swept oldest-first, at most `PALW_VERTEX_SWEEP_PER_BLOCK_V1` = 256 a block
(`sweep_vertex_rounds_v1`, run beside the other sweeps and mirrored in the pre-object base).

---

## 18.7 State, root, carriage, delta

`PalwChainStateV2.vertex: PalwVertexStateV1` holds three tables:

| Table | Key → row | Written by | Dropped when |
| --- | --- | --- | --- |
| `rounds` | `(round, seat)` → `{ leaves_root, accepted_daa, convicted }` (round-first, so the sweep takes the oldest) | a vertex; an equivocation | the sweep, `PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1` past the round |
| `tallies` | claim → `{ counted: Vec<{ seat, verdict, signed_daa }> }` | a counted `PanelBound` leaf | the claim leaves `PanelBound` |
| `held` | claim → `Vec<{ seat, object, first, last, digest, signed_daa, charged }>` (at most 64 a claim) | a `Held` leaf | the claim is terminal |

- **Root.** When any table is non-empty, after the `improvement-eval/v1` block and before `bounded_immature`: the label `vertex/v1` and the
  three collection roots in the order above. An empty `vertex` hashes nothing, so every dormant chain commits the root it always did.
- **Carriage.** One tail `0xE4` carrying the struct, written only when non-empty and last in the carriage; a reader that meets it twice
  ignores the second.
- **Delta.** Entry 101, `VertexRow { table, key, old, new }`: the key and the rows as their borsh bytes, `table` 1/2/3. Every write goes
  through the one writer (`write_vertex_row`), so a delta reproduces the fold and reverts to the parent exactly.
- **Consistency** (`assert_vertex_consistency_v1`, run by every carriage load): a tally belongs to a `PanelBound` claim with a panel, names
  only seats of it, once each, and is non-empty; `held` rows belong to live claims with a panel, between 1 and 64.

---

## 18.8 `Held` leaves and the DA certificate

A `Held { claim, object, first, last, digest }` leaf attests "I hold chunks `first..=last` of `object` of claim `claim`, whose digest is
`digest`, and I will serve them until the claim's challenge window closes."

- **Recorded** (PALW-VC-6) once per `(seat, object, first, last)` on a **live** claim whose panel the seat sits on, up to 64 rows a claim; a
  second attestation of the same range with another digest is ignored (the first stands). A leaf for a claim that is gone, terminal or
  unresolved is ignored.
- **A DA certificate** (`palw_vertex_da_certificate_v1`) is `PALW_VERTEX_DA_QUORUM_V1` = 3 distinct seats' equal `Held` leaves (same object, range
  and digest). It is what a capped-onboarding producer must show (Part IV.2), what an audit may fetch from (Part IV.1) and what an
  ADR-0111 request may name an attester by.
- **The `Held` exposure.** When the data-availability court concludes a claim's data was **not** served (`da_default_v1`), the producer is
  charged as ever and then **every attester of the claim not yet charged** loses `PALW_VERTEX_HELD_EXPOSURE_PERMILLE_V1` = 5 ‰ of its collateral
  (once; the rows are marked `charged`). The attesters are read before the charge, because the charge's void drops the rows.

`Held` leaves are voluntary and are paid from the claim's DA fee share; nothing in the licence path depends on them.

---

## 18.9 Security parameters (Part III)

The consensus fold reads none of these to decide a block; they are the one place the producer and the verifier of a rule read the same
number, and the RFC's tables are tests over them (`part_iii_reproduces_the_rfc_tables`).

| Item | Value |
| --- | --- |
| Model | 1-of-N with the exact court; never a cluster majority. Weight comes from bonded seat eligibility, post-commit random assignment, permissionless challenge and random audits |
| `m`: independent checkers of one interval | full replay or algebraic, all five seats, duty to `Final`: `m` = 5; full, licensed at quorum, the rest stop: 3; `k` of `N` intervals sampled by each seat: `5·k/N` expected |
| Escape of a one-point lie at adversary share `f` | `f^m`; sampled: `(f + (1 − f)(1 − k/N))^5`. At `f` = 0.1 / 0.2 / 0.33: full-to-`Final` 0.001 % / 0.03 % / 0.4 %; quorum-only 0.1 % / 0.8 % / 3.6 %; `k` = 4 of `N` = 299: 94.1 % / 94.8 % / 95.6 % (the catch at `f` = 0: 6.51 %) |
| Slash a rational producer cannot profit past | `gain / P(detect)`: about 17 × the gain at a 6 % catch; once the gain at certainty (`palw_vertex_required_slash_multiple_ppm_v1`, `palw_vertex_slash_deters_v1`) |
| Duty | runs to `Final`, not to the licence (`PALW_VERTEX_DUTY_RUNS_TO_FINAL_V1`): seats keep checking after quorum so `m` stays the panel's size, and a later finding opens the court inside the licence-to-`Final` window |
| Escalation (`palw_vertex_escalation_v1`) | a check that passed files `Valid` into the vertex; one that **failed** files no `Valid` and falls back to the exact reference evaluation and the (unchanged) court; one that could not run abstains, never `Valid` |
| Equivocation | §18.6 |
| Audits, challenges | permissionless challenge stays (any bonded node may re-run a licensed claim and open the court); the global audit mesh is Part IV.1's own fence |

Weight-bearing work needs full-coverage holders (RFC-0006 shards, or Part II sketching seats): sampled intervals are a sensor, and collateral
for a sampled class is priced at the multiple above.

---

## 18.10 The fence and the drill

`Params::palw_verification_vertex_v1: Option<ForkActivation>` — a bare height, **`None` on every shipped preset** (testnet-12, testnet-11,
testnet-10, mainnet, devnet, simnet). It is written in the four places every fence is: the `Params` field; `for_each_fence`; Some-only
writes in `consensus_params_id` and `consensus_schedule_id`; the `never()` collapse in
`normalize_values_a_scheduled_fence_drags_with_it`. It is mirrored on the V2 bundle's state params (`vertex_from_daa`,
`sync_palw_verification_vertex_v1`), and `validate_palw_v2` (`validate_palw_verification_vertex_v1`) refuses:

- a ruleset whose mirror disagrees with the fence;
- arming on a ruleset that is not `ConsensusV2`;
- arming without each of `palw_verification_v2`, `palw_rcore_plus`, `palw_unavailable_abstains`, `palw_panel_economy` and
  `palw_objective_offence` in force at or below its height, **by name**.

A drill moves it on a salted chain with `--palw-drill-vertex-at=H` (`palw_drill_vertex_at_v1`; the marker line `vertex_at=`, the manifest key
`vertex_at`): it moves nothing else, and the height must be one no other fence uses (the fork id hashes sorted, de-duplicated heights).
Nothing arms the fence on testnet-12 here; it joins a flag-day list when the lead decides one.

---

## 18.11 The node (informative)

Nothing in this section is a rule: the fold judges whatever arrives, and below the fence every line of it is dormant.

- **A seat speaks in vertices** past the fence, for claims whose panel bound at or after it: the duty loop's verdict becomes a leaf of the round's
  vertex instead of a receipt. A claim bound before the fence keeps the receipt path to its licence or its void.
- **One signature per round, never two.** The seal persists the round (`palw-vertex-round` in the state dir) before the vertex leaves the
  node, and a restart refuses to seal a round at or below it; a seat that cannot persist a round seals nothing. (Two vertices of a round are
  an equivocation, §18.6; two nodes sharing one bond would commit it, which is the operator's.)
- **A vertex rides once**, as an ordinary lifecycle carrier in the panel's own slot (`OwnReceipts`), relayed as any transaction and taken by any
  block template. It is re-sent after the replan interval until the seat's row appears at the tip, and after `PALW_VERTEX_MAX_CARRY_DAA_V1` its
  leaves go back into a later round's vertex. There is no collector, no per-(claim, bond) receipt pool for a post-fence claim, and no
  licence to order.
- **References** are compact unless `--palw-vertex-full-refs`.
- **Status.** `getPalwNodeStatus`'s `verification` line carries `vertex_fence`, `vertex_pending`, `vertex_sealed_waiting`, `vertex_sealed`,
  `vertex_sent`, `vertex_landed`, `vertex_expired`, `vertex_leaves`, `vertex_last_round`, and the tip's `vertex_tip_rounds`,
  `vertex_tip_tallies`, `vertex_tip_held`; `ConsensusApi::palw_v2_vertex_status_v1(seat, from_round)` reads a seat's own rows.

---

## 18.12 Rules

- **PALW-VC-1 (one vertex per seat-round).** Past `palw_verification_vertex_v1`, the fold MUST accept at most one `VerificationVertexV1` per
  `(seat_bond, round)`, the first in accepted order, and MUST refuse a vertex whose leaves are not strictly ascending, whose count or size
  exceeds the caps, whose `leaves_root` does not recompute, or that is older than `PALW_VERTEX_MAX_CARRY_DAA_V1`.
- **PALW-VC-2 (the signature).** A vertex MUST verify under its seat bond's registered ML-DSA-87 key, over `vertex_message_v1`, with context
  `misaka-palw/verification-vertex/mldsa87/v1`.
- **PALW-VC-3 (the tally).** A `Verdict` leaf MUST count toward its claim only under the conditions of §18.4. A claim whose counted `Valid`
  leaves reach the quorum MUST transition as `ReceiptLicensedV2` of the same seats would, in the block that completes the tally; no licence
  object is carried.
- **PALW-VC-4 (a verdict stands).** A seat's first counted verdict for a claim MUST NOT be replaced by a later leaf of the same seat.
- **PALW-VC-5 (equivocation).** Two vertices of one `(seat_bond, round)` with different roots, both validly signed, MUST be accepted as
  `VertexEquivocationV1` evidence. The fold MUST slash 100 ‰, forfeit the named locks, eject the bond, and convict the pair once.
- **PALW-VC-6 (DA attestations).** A `Held` leaf MUST bind its signer to serve the named chunks until the claim's challenge window closes;
  an attester of a claim whose data the court concludes was not served MUST be charged its `Held` exposure once.
- **PALW-VC-7 (the path rule).** A claim whose panel bound at or after the fence MUST license by tally and MUST refuse a receipt-path licence
  object by name; a claim whose panel bound before it MUST keep the receipt path until licensed or void and MUST NOT count a leaf.

---

## 18.13 Carriage, measured

**Sizes** (borsh, consensus-core `palw_vertex_v1`): a vertex header with its signature is 4,808 bytes; a `Verdict` leaf 67 bytes with a `Full`
reference and 23 with a `Compact` one (`Unavailable` adds 12); the lifecycle payload adds 3 bytes. A five-receipt coverage licence is 125,768
transient mass (≈ 24 KB of payload; ADR-0160 V-T2).

**On a real testnet-12 chain through real blocks** (`t12_capacity_verify::rfc7_vertex_carriage_per_licence`, debug build, testnet-12's params,
real ML-DSA-87 by the registered keys, 100 DAA, one planted 1M bond beside the eight cards, the capacity fences armed; every round the eight seats
sign one vertex over the claims bound and unanswered; evidence in `~/Downloads/MISAKA-wt-b/lanes/evidence/rfc7-vertex/carriage-measure.jsonl`):

| run | claims licensed by tally | vertices | payload bytes per licence | transient mass per licence | five-receipt licence |
| --- | --- | --- | --- | --- | --- |
| compact references, ≈ 5 claims a DAA | 425 | 461 | 5,329 | 53,821 | 125,768 |
| whole ids, ≈ 5 claims a DAA | 425 | 460 | 5,579 | 54,749 | 125,768 |
| compact references, 60 asked a DAA (the room admits ≈ 5) | 434 | 422 | 4,791 | 48,301 | 125,768 |

At this issuance a round holds a few claims, so the eight headers dominate (≈ 4.9 KB each): **2.3 – 2.6 × less transient mass per licence than the
receipt path, ≈ 4.5 × less payload**, with licence latency unchanged (accept → licence 22 DAA at p50, p90 and max, the bind's own delay).

**What the headers amortise to.** A claim sits on five of the eight seats, so a licence takes five leaves; per licence, payload ≈ `5 × leaf + 8 × 4,808 / N`
for `N` claims licensed in a round:

| claims a DAA (`round_daa` = 1) | ×1 (5.3) | ×10 (53) | ×100 (530) | ×1000 (5,300) |
| --- | --- | --- | --- | --- |
| receipts, bytes a licence | 24,000 | 24,000 | 24,000 | 24,000 |
| vertices, compact: bytes a licence | 7,300 | 840 | 190 | 120 |
| vertices, whole ids: bytes a licence | 7,500 | 1,060 | 410 | 340 |
| vertices, compact: carriage a DAA | 38.5 KB | 44.5 KB | 100 KB | 636 KB |

**The leaf cap bounds licensing, not carriage.** A seat signs one vertex a round and a vertex holds at most 1,024 leaves, so the chain licenses at most
`seats × 1,024 / 5` claims a DAA at `round_daa` = 1 — 1,638 with testnet-12's eight seats, 40,960 with the 200 the RFC assumes; a seat whose round holds more
leaves carries the excess into its next round (the node's book does exactly that, `a_round_past_the_leaf_cap_carries_the_rest_to_the_next`). So ×1000 (5,300 a
DAA) needs more than eight seats or a longer round; ×100 (530) fits eight.

**The drill's row** (RFC-0007 activation table): licences by receipts below the fence and by tally above it, the cross-fence claim on the old path,
the equivocating seat slashed, and the carriage per licence — `t12_capacity_verify::rfc7_vertex_crosses_its_fence_on_a_real_chain_and_licenses_by_tally`
(real blocks, in process; fence at DAA 70: one vertex dropped by name below it, four claims bound below it of which one licenses by receipts below and
one licenses by receipts **after** the fence, six bound at or after it all licensed by eight vertices, a leaf naming a pre-fence claim counting for
nothing, the receipt path refused by name for a post-fence claim and by the assemblers, and the seat that signed a round twice slashed 100 ‰ — 93,906,321,001,040 → 84,491,676,088,216
sompi — and ejected), and the multi-node drill of §18.10 (`audit-vertex/dv.sh`).

## 18.14 Tests

- consensus-core `palw_vertex_v1::tests` (shape by name, the message's fields, the root, compact references, Part III's tables, the domain
  registry) and `palw_state_v2::tests::vertex_fold_v1` (the licence is the tally and equals the receipt path's record; a verdict stands;
  leaves that do not count are ignored; hostile vertices refused by name; a claim bound across the fence licenses on the old path and refuses
  a receipt for a post-fence claim; equivocation slashed, locks forfeited, bond ejected, convicted once, hostile evidence refused;
  `Held` leaves, the certificate and the attester charge; compact references; tallies leave with their claim and rounds are swept, with
  reorg round trips; a dormant chain roots and carries as before; the tail is `0xE4`).
- node `palw_panel::vertex::tests` (one vertex a round and never two across a restart, a vertex is carried until it lands and an expired one is
  said again, the leaf cap, compact references, the state file).

---

## 18.15 The witness manifest and the seat-local checker (Part II)

**18.15.1 What the chain does, and does not.** The algebraic checker (RFC-0007 §II.3) is seat-local node software: a seat may recompute a claim,
check it by Freivalds against its own secret sketches, or both, and the chain reads none of it. What the chain adds, behind
`palw_witness_manifest_v1`, is the **producer's duty to serve the witness** (the output of every `MatMul` the canonical serving set names) and a
way for a seat to say *which chunk* went unserved.

**18.15.2 The witness profile** (`PalwWitnessProfileRowV1`, `mesh.witness`, keyed by class). A class registered at or past the fence records, at its
registration, a profile read off its program by the **canonical serving set** (open question 10, settled: pinned in the class profile):

- `elements_per_position` — `misaka_palw_tir::dataflow::canonical_witness_elements_per_position_v1(program, max_context)`: the elements of every
  `MatMul` output the dataflow analysis serves under the default policy — a weight product of contraction at least 64, a `P·V` from a history of
  1,024 — over every occurrence of the schedule (`pre` once, each layer block once per layer, `post` once), at history length `max_context`;
- `bytes = elements_per_position × max_context × 8` (an `i64` accumulator per element), checked arithmetic, `None` on overflow;
- `chunks = ⌈bytes / 1 MiB⌉`, at least 1, at most `PALW_WITNESS_MAX_CHUNKS_V1` = 4,096.

A program that serves nothing records no profile and keeps the v1 manifest. The analysis lives in `misaka-palw-tir` (`dataflow`, moved from the
sketch crate) so that the chain and the checker read the one set.

**18.15.3 The pin.** Past the fence an attempt of a class **with** a profile commits exactly `1 + chunks` trace chunks (`trace_chunk_count`) under

```text
trace_manifest_root_v2 = keyed BLAKE2b-512("misaka-palw/mesh/trace-manifest/v2", trace_root ‖ le32(chunk_count))
```

and an attempt of a class **without** one is unchanged (count 1, the v1 root). Chunk 0 is the trace; chunks `1..` are the witness. The stateless
half (`palw_witness_manifest_shape_ok_v1`, asked at the header stage and by the composed admission) takes a count of 1 under the v1 root or
`2..=1 + 4,096` under the v2 root; the stateful half (`check_palw_attempt_witness_pin_v1`) demands the count the class's profile owes. **The
manifest root is a function of the trace root and the count alone: it carries no witness root.** A witness root the chain cannot recompute would
be a free field in the priced bytes — a free draw on both lotteries (ADR-0072 Decision 8); the chunk digests are the producer's served manifest,
and what a seat trusts is the algebra, not them. `Unavailable { chunk_index, requested_daa }` already requires `chunk_index < trace_chunk_count`
(PALW-FS-13), so under the fence it may name a witness chunk; its meaning is ADR-0065 Decision 4's, unchanged (it abstains: the claim redraws and
voids at `ReceiptTimeout`, nobody is slashed for silence).

**18.15.4 The window, not the pay** (open question 5, settled). A class's verification window is derived from `verification_ccu`; past the fence it
is derived from `verification_ccu + palw_witness_ccu_v1(profile.bytes)` where a served byte costs 320 MAC-eq (a 100 Mbit/s link, 12,500 bytes a
millisecond, against the registry's reference of 4,000,000 MAC-eq a millisecond) — `palw_class_verify_ccu_v1`, read by the class's compute
deadline (`claim_verify_daa_v1`), the measured-row gate and the free-prompt cap. **Seat pay reads the registry row's `verification_ccu`, which no
witness term touches.** A sketching seat's receipt earns what a replay's earns (open question 6): the verdict is the same verdict, and nothing in
the fold reads how a seat checked.

**18.15.5 The node.** Behind `--palw-sketch-check` (off by default; a node without it never opens the sketch state):

- **The secret** is 32 bytes drawn from the operating system once and kept in `palw-sketch-secret` in the state dir (mode 0600), never serialised
  elsewhere, never logged. Every sketch vector derives from it per class and epoch (7,200 DAA); a sketch is as secret as the secret.
- **The mirror check** runs beside every full replay of an IR class the seat holds: the seat produces the claim's witness itself on the typed
  backend (`tir_witness_capture_v1`), checks it (`tir_mirror_check_v1`: Freivalds for every served `MatMul`, exact recomputation for the rest, the
  derived committed rows against the served ones) and compares the verdict with the replay's (`tir_mirror_agreement_v1`). A refusal of the seat's
  own honest witness is a **false reject**, a checker's acceptance of a served witness of a claim the replay does not reproduce a **false accept**;
  both are logged at error level and counted (`sketch_mirror_false_rejects`, `sketch_mirror_false_accepts` in the `verification` status line). No
  seat should rely on the checker alone before the mirror has run clean over real claims.
- **Per-row history sketches** (`TirHistorySketchV1`, open question 9, settled: built): `S_V[h] = Σ_d σ[d]·V_h[d]` once per appended row and the
  running `R[d] = Σ_h ρ[h]·K_h[d]`, so a `P·V` check reads `d + H` numbers and a `Q·Kᵀ` check likewise, a row costs `O(d)`, and a sliding window
  evicts exactly. Sound for a weight sketch's reason: uniform secret vectors over verified rows (`tests/history.rs` observes `1/p` at a toy prime).
- **The producer** keeps a claim's witness as an obligation (`{claim}.witness`, `{claim}.witness.chunks`): the canonical image
  (`codec::encode`), cut into exactly the pinned number of chunks, each with a digest bound to its index; a producer that cannot make it does not
  publish the block. `palw_witness_chunk_v1` serves a chunk only while it matches its digest. The transport that carries a chunk to a seat is the
  interval lane's off-chain path (RFC-0007 §II.7); this chapter pins the count and the naming, not the wire.
- **A failed check is not a verdict** (PALW-AV-4): the seat files no `Valid` and escalates through the exact court, unchanged (§II.8).
- **Row-block sketches** (§II.8, node-local, not consensus): each weight is sketched in `B = ceil(|W| / F)` free-axis blocks, `F` = 2 MiB (`TIR_BLOCK_FETCH_CAP_BYTES_V1`); the blocks sum entrywise to the whole-site sketch the routine check uses (`tests/soundness.rs`). After a failed check each block is checked with the vector masked to it, and the failure names the failing blocks (`TirCheckFailureV1::blocks`), so the seat fetches at most `F` bytes of weight per failing block (inventory openings from the producer or a holder, then ADR-0111's bounded on-chain request) or hands the interval to a holder, and the unchanged court tries the named leaf. A weight within `F` has one block and stores nothing extra.

---

## 18.16 The audit mesh and its traps (Part IV.1)

**18.16.1 The draw.** At a claim's **acceptance** past `palw_audit_mesh_v1` (so a claim the panel never licenses is still audited — the mesh is a
sensor, not a licence), a claim of an IR class (`tir_classes` row) with a non-zero audit pay draws [`PALW_AUDITS_PER_CLAIM_V1`] = **2** auditors
from **all bonded seats**: every Active bond registered before the claim, other than its producer, with free collateral for the penalty
(`collateral − reserved_exposure − registration_exposure`), weighted by stake (whole BILI, capped as the panel's stake draw caps it), one seat per
operator. The race is the panel's (`−ln u / w`, `palw_draw_key_cmp_v1`) over tickets `H(seed ‖ bond)` under a seed bound to the claim id, its
carrying block, its execution root and the draw's DAA. Each winner draws its **leaf ticket** (a `u64`; the auditor audits committed tile
`ticket % tiles`, the chain does not know the tile count). **The hook cannot fail**: it runs after the claim is written, where a refusal would
strand the write, so what it cannot do it leaves undone — an unaudited claim is a sensor that missed, never a refused block. At most
`PALW_AUDIT_MAX_OPEN_V1` = 20,000 rows are open; a claim accepted past it is not audited.

**18.16.2 The row** (`PalwAuditRowV1`, keyed by claim): the draw DAA, `audit_end = draw + 240`, `row_end = audit_end + 240`, the pay (4 ‰ of the
claim's escrowed reward) and penalty (10 × the pay) snapshotted, and per auditor the ticket, the reservation and the outcome. **Each auditor
reserves the penalty in `reserved_exposure`** for the row's life, so the withdrawal rule and every free-collateral reader see it; the consistency
check re-derives the reservations from the rows. The sweep (before every block's objects, mirrored in the pre-object base) retires up to 256 rows a
block past `row_end`, releasing their reservations.

**18.16.3 Audited leaves and pay.** An `Audited { claim, leaf, result }` leaf of an accepted vertex (a `Compact` reference names the DAA the audit was
drawn at) counts when its seat is a drawn auditor of the claim, `leaf` is the drawn ticket, `signed_daa` is in `[drawn, audit_end]`, and it is the seat's
first answer; `result` is 0 (match) or 1 (mismatch), anything else is refused by name at shape. The audit is paid `pay` out of the **panel reserve**
(clamped at it), queued as the seat's panel payout. **Silence is never a match** (PALW-AM-2): an unanswered assignment earns nothing and is charged
nothing. An audit leaf below the fence is refused by name by the acceptance gate and the fold.

**18.16.4 Traps** (open question 7). A bonded **setter** drawn by the slot lottery (`palw_trap_slot_drawn_v1`: a uniform ticket over `(bond, ⌊DAA /
100⌋)` against `PALW_TRAP_RATE_BP_V1` = 100 of 10,000, i.e. 1 %) carries `TrapCommittedV1 { setter_bond, commitment, signature }` (tag 102) with
`commitment = H(claim ‖ fault leaf ‖ tiles ‖ salt)`, reserving a 100 BILI deposit (one trap open per setter, at most 1,024 open). It then produces a claim of
its own with a fault planted in committed tile `fault_leaf` of `tiles` (at most 64), and after the audit window carries `TrapRevealedV1 { setter_bond,
claim, fault_leaf, tiles, salt, signature }` (tag 103), admissible only for **the setter's own audited claim**, after `audit_end` and no later than
`row_end`. The fold: releases the deposit and spends the commitment; for each drawn auditor whose ticket landed on the planted tile
(`ticket % tiles == fault_leaf`) — **slashes `penalty` from one that attested a match, pays the bounty (5 audit pays, from the reserve) to one that
attested the mismatch**; marks the row settled (a second reveal of the claim changes nothing); and **voids the trap claim without slashing its
setter** (`ReceiptTimeout`: no conviction, no probe note). A commitment never revealed within `2 × (audit + reveal)` = 960 DAA forfeits the deposit.

---

## 18.17 Staged onboarding (Part IV.2)

**18.17.1 The state.** `PalwModelLifecycleV1::Capped { since_daa }` is **appended last** in the enum (index 7, after `Candidate`: the 2026-09-10
failure). `admits_claims` is true; `admission_permille` is `PALW_CAPPED_ADMISSION_PERMILLE_V1` = 20; a capped class never counts toward
`panel_drawable`.

**18.17.2 Entry.** A `Prefetching` class steps to `Capped` at a span boundary when, past `palw_capped_onboarding_v1`: its ready seats do not fill a
panel (`ready_seats < seat_count`); a **DA certificate** stands for it — `q` = 3 distinct seats' equal `Held` leaves (object 0, the capture of its
probe job) naming the **class id**, recorded by the fold in `vertex.held` under the class while it is `Prefetching`, from any Active bond (§18.8);
and the capped classes' shares together with its own stay within **`w_cap` = 1 % (10 ‰)** of the share table. The step takes the boundary's DAA as
`since_daa`.

**18.17.3 Capped claims.** The class's admission gate is not the panel room (it has no panel) but the **weight cap**: a claim is refused once the open
capped claims' `immature_contribution` exceeds `w_cap` of `bounded_immature` (the cap is exceeded by at most the one claim that crossed it) or the capped
table (20,000) is full. A claim accepted while its class is `Capped` writes a **capped row** and **owes no bind deadline** while the class is capped
(`palw_provisional_bind_deadline_v1` returns `u64::MAX`; the NCP retry re-anchors it at every slot as for any class that cannot seat a panel). Audits
are drawn for it as for any IR claim — the mesh is its sensor.

**18.17.4 Provisional rewards and the re-verification window.** A capped claim reaches a panel only once the class's holders are seated, and its
reward is paid, like every claim's, only at `Final`; until then the escrow is withheld and never minted for a claim that is voided — that is the
reward being **provisional**. When the class leaves `Capped` (ready seats at least `required_ready_seats` and a panel's worth), each capped claim
still waiting gets a **window** of `PALW_CAPPED_REVERIFY_WINDOW_DAA_V1` = 2,400 DAA from that boundary (its deadline is re-armed from the one
function): a claim the holders bind a panel to inside it leaves the capped row and is licensed and finalized by the ordinary receipt/tally path —
**every capped claim is re-verified by the class's holders** (open question 8, settled), a forged one gets no `Valid`, times out, and is voided with
its reward forfeited (or, if a holder opens the exact court, convicted and slashed as any claim is); a claim that never binds is voided at the window's
end like any claim that never bound. Already-accepted blocks are not undone: the cap bounds by how much.

---

## 18.18 The three fences and their drills

| fence | arms | prerequisites (in force at or below it, by name) | drill flag |
| --- | --- | --- | --- |
| `palw_witness_manifest_v1` | §18.15 | `palw_tir_v1`, `palw_unavailable_abstains` | `--palw-drill-witness-at=H` |
| `palw_audit_mesh_v1` | §18.16 | `palw_verification_vertex_v1`, `palw_tir_v1`, `palw_panel_economy` | `--palw-drill-audit-mesh-at=H` |
| `palw_capped_onboarding_v1` | §18.17 | `palw_audit_mesh_v1`, `palw_admission_independence`, `palw_registry_resilience` | `--palw-drill-capped-at=H` |

Each is a bare height in the four places (the `Params` field; `for_each_fence`; the Some-only writes in both ids; the `never()` collapse), mirrored on
the V2 bundle (`witness_manifest_from_daa`, `audit_mesh_from_daa`, `capped_from_daa`; `sync_palw_*`), **`None` on every shipped preset**, in no testnet-12
flag-day list, and refused by `validate_palw_v2` by name without its prerequisites or with its mirror unsynced (`consensus/core/tests/palw_mesh_fences.rs`).
The drill flags work only with `--palw-drill-genesis-salt`; each moves nothing else; heights must be ones no other fence uses.

---

## 18.19 Rules

- **PALW-AV-1 (the witness)** *(node)*. A producer of a class with a witness profile MUST keep the canonical witness of a claim, in the pinned number of
  chunks, for the claim's retention, and SHOULD serve a chunk on request during the receipt window.
- **PALW-AV-2 (secrecy)** *(node)*. A seat MUST NOT disclose its sketch secret, its keys or its sketches.
- **PALW-AV-3 (moduli)** *(node)*. A seat checking a node algebraically MUST use moduli whose product exceeds the span of the node's refined proven
  interval and MUST refuse a served value outside it.
- **PALW-AV-4 (failure is not a verdict)** *(node)*. A seat whose check fails MUST NOT file `Valid`; it escalates by §II.8.
- **PALW-WM-1 (the pin).** Past `palw_witness_manifest_v1`, admission MUST refuse an attempt whose `trace_chunk_count` is not `1 + chunks` of its class's
  profile (1 for a class with none) or whose manifest root is not the v1 (count 1) or v2 (count > 1) derivation.
- **PALW-AM-1 (audit assignment).** Past `palw_audit_mesh_v1`, a claim of an IR class MUST be assigned its auditors at acceptance from all bonded seats by
  stake, after the claim commits, and each auditor MUST reserve the trap penalty.
- **PALW-AM-2 (positive attestation).** Only an `Audited` leaf counts as an audit. Absence MUST NOT count as a match.
- **PALW-AM-3 (traps).** A `TrapRevealed` that opens a prior `TrapCommitted` of the setter's own audited claim MUST void the trap claim without slashing the
  setter, MUST slash every auditor whose `Audited` leaf attested a match on the planted tile, and MUST pay those who attested the mismatch.
- **PALW-AM-4 (capped mode).** Past `palw_capped_onboarding_v1`, a class in `Capped` MUST admit at most `capped_admission_permille`; all capped claims together
  MUST NOT hold more than `w_cap` of the immature weight (past it the next claim is refused); a capped claim MUST owe no bind deadline while its class is
  `Capped`, MUST get its re-verification window when the class leaves it, and MUST reach `Final` (and be paid) only through a panel.

---

## 18.20 The decisions of 2026-10-03 on RFC open questions 5 to 10

| # | Question | Decision (the lead) | Where |
| --- | --- | --- | --- |
| 5 | Witness bytes in the class profile | They enter the verification-window derivation, **not** seat pay | §18.15.4 |
| 6 | Seat pay for algebraic checks | A sketching seat's receipt earns what a replay's earns | §18.15.4 |
| 7 | Audit-mesh parameters | `trap_rate` 1 %; `trap_penalty` a reservation equal to 10 × the audit pay; 2 audits per claim; audit pay 4 ‰ of the claim's escrowed reward per attested audit, from the panel reserve; trap bounty 5 audit pays; trap deposit 100 BILI | §18.16 |
| 8 | `w_cap` and re-verification | `w_cap` = 1 % (10 ‰); `capped_admission_permille` = 20; re-verify **all** capped claims on testnet-12 | §18.17 |
| 9 | Per-row history sketches | Build them | §18.15.5, `misaka-palw-tir-sketch::history` |
| 10 | The canonical serving set | Pin it in the class profile | §18.15.2 |

Where this chapter fixed what the RFC left room for: audit pay has no on-chain measure of "openings fetched", so it is a fixed share of the claim's reward;
the trap slot lottery is a hash lottery over `(bond, slot)` rather than the anchor beacon (a trap setter learns nothing by predicting its own slot); the
witness root is **not** a chain field (§18.15.3); and the mesh's licence role for capped claims is realised as the holders' ordinary path (§18.17.4) rather
than a second licensing arm, because every licensing door the chain has reads a panel's receipts and the rcore ledger beside them.

---

## 18.21 Tests of this part

- consensus-core `tests/palw_mesh_v1.rs` (the v2 manifest root and its pins, the canonical serving set read off a program, the window term, the audit draw's
  determinism, stake weighting and one-seat-per-operator, pay/penalty/bounty, the trap commitment's binding and its slot lottery near 1 %, the domain
  registry), `tests/palw_mesh_fences.rs` (dormancy on every preset, the four places, prerequisites by name, mirrors, the drill movers),
  `palw_state_v2::tests::mesh_fold_v1` (the draw at acceptance, `Audited` leaves counted and paid, silence not a match, traps settled, hostile trap moves
  refused by name, the sweep, the witness pin and the window term), `palw_state_v2::tests::adr0135::capped_onboarding_v1` (entry on a certificate, the
  capped claim waiting, the window and the void, the weight cap, and the holders' ordinary path to `Final`).
- `misaka-palw-tir-sketch` `tests/history.rs` (per-row history sketches), `tests/mirror.rs` (the witness codec and chunks, the mirror and its agreement table).
- node `palw_panel::mesh::tests` (the audit leaf, the trap book), `palw_panel::sketch::tests` (the secret, the retained witness and its chunks).
