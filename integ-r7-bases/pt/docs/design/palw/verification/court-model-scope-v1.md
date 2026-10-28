# `palw_court_model_scope_v1` — no court route compels model bytes (register D2)

**Status:** design (2026-10-12). The fence exists, dormant and refused when armed (`validate_palw_2026_10_12_ruling_fences_v1`).
**Ruling:** user, 2026-10-12 ([register](../integration/mainnet-spec-register-2026-10-12.md) §3): close, with a dormant restriction fence, the
testnet-12 routes that compel model bytes (ADR-0177 D2, RFC-0014 §16.4, [DA16 §7.5](da16-transport-and-provider-court.md)).
Seat readiness is **out of scope** (user, 2026-10-12): it is the Panel's possession proof for seating (N1/N2), not a court demand.

## Scope

| Unit (`palw_court_scope_v1`) | Today on t12 | Past the fence |
| --- | --- | --- |
| `HeldStepLeaf` — the held `StepLeaf` answer's `artifact_openings` | the producer owes the rows; the one-move verdict reads them | **already built** behind `palw_legacy_held_da_v2` (LG14-B): the answer owes its claim half only (`check_execution_step_committed_half_v1`), rows in the answer are refused, the accuser supplies them in its one-move filing. The scope fence requires LG14-B at or below it. |
| `AttnRootClaimOperands` — `CourtAttnRootClaimed{,Anchored,Held}.operand_openings` | the producer's root claim carries the site's registered quantization rows; `palw_attn_dispute_site_v3` derives the site from them | the root claim carries **no** openings (refused if non-empty); the operands come from the challenger (below) |
| `TirGenRootClaimParams` — `CourtTirRootClaimed` / `CourtGenRootClaimed` finalize `params` | the producer's root claim carries the parameter leaves the disputed tile reads | the root claim names parameter **coordinates** only; the challenger supplies the leaves |
| `SeatReadinessLeaves` | seats publish ~16 plaintext leaves | unchanged (out of scope) |

## Design of the second half (root claims)

The dissection phase is opened from a site derived from the operands (attention) or checked against them (IR / generative
finalize). Taking the operands out of the producer's move needs a party that holds the model to supply them before the phase opens.

**Recommended:** a challenger move between the root claim and the phase.
1. Past the fence, the producer's root claim (`CourtAttnRootClaimed*`, `CourtTirRootClaimed`, `CourtGenRootClaimed`) carries an empty
   `operand_openings` / no param leaves; a non-empty one is refused by name. It records the root claim and starts the challenger's
   turn (the existing `turn_deadline_daa`).
2. The session's challenger files a new court move, `CourtRootOperandsSupplied { session_id, openings, signature }` (a new inner kind;
   A-2: below the fence it rides unjudged as int-12 reads it). The openings are verified against the class's REGISTERED roots
   (`palw_verify_verifier_model_operand_v1` / `PalwProvenOperandsV1::from_openings_v1` against the claim's proven artifact root), then
   the phase opens exactly as today with those operands.
3. If the challenger does not supply them by its turn deadline, the session ends as the **challenger's default**: the accusation lapses,
   the producer is not charged, and the claim's other ends resume (the same outcome as an accuser that stops answering a dissection).

**Why the challenger:** under ADR-0177 D7, G14 holds only for a verifier that holds the registered model. Such a verifier can supply
the rows; one that cannot was never able to finish the dissection. Silence by the producer is unaffected: a producer that does not file
its root claim still defaults as today.

**Alternatives considered:** (a) let *any* bond supply the operands — widens grief surface for nothing, the challenger is the party that
needs them; (b) open the phase without operands and have the terminal read them — the attention site cannot be derived without them,
so the phase could not be opened.

## Work items

- consensus-core: the new move kind + A-2 row; the root-claim arms refuse openings past the fence; the session waits in a
  "root claimed, operands pending" state; the phase opens on supply; timeout → challenger default. Inventory: the two kinds become
  `VerifierOwnCopy` past the fence (`palw_court_unit_scope_v1` gains a ruleset input or a second table).
- kaspad: the accuser's filer supplies the operands from its own copy after the producer's root claim; the producer's filer stops
  attaching them past the fence.
- tests: REQ-D2-1 (a root claim with openings past the fence is refused), REQ-D2-2 (verifier-supplied operands open the phase and the
  dissection reaches the same verdicts as today, honest and lying), REQ-D2-3 (challenger silence → challenger default, producer
  uncharged), REQ-D2-4 (below the fence: byte-identical int-12 behaviour), REQ-D2-5 (the scope inventory past the fence compels no
  model bytes on any route — the existing inventory test, extended), and a real-node E2E per dissection family.
