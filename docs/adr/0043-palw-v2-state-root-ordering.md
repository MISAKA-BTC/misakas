# ADR-0043 — PALW V2 state-root hash ordering (no challenge↔commitment cycle)

- **Status:** Accepted (2026-08-20)
- **Relates to:** ADR-0042 Decision 5 (which required this record before any header commits a
  state root: "the ADR that lands it must spell out a hash ordering with no
  challenge↔commitment cycle — don't implement by vibes"), Decision 3 (the attempt transcript),
  Decision 9 (the comparator), Decision 11 (the ruleset fingerprint).
- **Lands with:** PR-03 (`consensus/core/src/palw_state_v2.rs`). The header FIELD that carries
  the root arrives with the V2 header format in PR-10; this ADR fixes what the bytes will mean
  so nothing about them is decided by the PR that wires them.

## 1. What a V2 header commits, and why there is no cycle

A V2 header at block `B` commits

```
B.palw_state_root  =  state_root( PalwChainStateV2 after applying B's SELECTED PARENT )
```

that is, the **parent-side** state: the result of applying the selected-parent chain up to and
including `parent(B)`, and nothing of `B` itself. `B`'s own attempt and accepted objects are
applied by [`apply_palw_transition_v2`] to *produce* the state that `B`'s children commit.

The cycle analysis, term by term:

```
challenge(B)   = H(network_domain ‖ pre_pow_hash(B) ‖ timestamp ‖ nonce ‖ class ‖ bond)   (ADR-0042 D3a)
root(B)        = H(challenge ‖ class ‖ bond ‖ trace_root ‖ output_root ‖ pwu)
attempt_id(B)  = H(canonical unsigned attempt)
state_root(B)  = f(state after parent(B))
```

- `challenge(B)` takes **no state root** as input, so the attempt does not depend on any value
  the attempt itself could move.
- `state_root(B)` is fully determined before `B`'s attempt exists (it is a function of the
  parent chain), so the header may carry both the attempt and the state root with neither
  hashing the other's output.
- The claim that `B`'s attempt creates enters the state committed by `B`'s **children** — the
  same parent-side pattern as the UTXO commitment, and the reason a verifier can check
  `B.palw_state_root` before ever validating `B`'s own PALW content.

Rejected alternative, for the record: committing the *post-B* state ("this block's own effects
included") would make the header's committed root depend on the attempt, whose challenge is
domain-separated over header fields adjacent to that commitment — auditing that no path closes
into a cycle then has to be redone every time either transcript grows a field. Parent-side
commitment makes the acyclicity structural rather than reviewed-per-change.

## 2. The root derivation, frozen

Implemented in `PalwChainStateV2::state_root` (domain
`misaka-palw/state-v2/state-root/v1`, keyed BLAKE2b-512):

```
H( version_le
 ‖ root("bonds")  ‖ root("reserved_exposure") ‖ root("classes") ‖ root("class_targets")
 ‖ root("capabilities") ‖ root("claims") ‖ root("panels") ‖ root("court_sessions")
 ‖ root("epoch_counters")
 ‖ safe_weight_le(16) ‖ bounded_immature_le(16)
 ‖ safe_frontier_blue_score_le(8) ‖ safe_frontier(64)
 ‖ last_point_tag(1) [‖ borsh(last_point)] )
```

Chain-block identity throughout the state — `PalwBlockContextV2::block`, `safe_frontier`, a
claim's `accepted_block`, the state-book keys — is the codebase's `BlockHash` (**`Hash64`**,
flipped in PR-9.5e / ADR-0008). This is also why `PalwCandidateOrderV1::candidate` is a
`Hash64`: the comparator's tie-break IS the candidate's block hash, no widening layer between
them.

```
```

Each collection root (domain `misaka-palw/state-v2/collection/v1`) is

```
H( len(label) ‖ label ‖ count_le(8) ‖ ( len(key) ‖ borsh(key) ‖ len(record) ‖ borsh(record) )* )
```

over the collection's key order — `BTreeMap` order, which for every key type in the state is a
comparison of fixed-width byte arrays and integers, identical on every ISA. Length prefixes keep
adjacent entries from bleeding into one another; the label keeps two same-shaped collections
from colliding.

**Change rule:** adding, removing, or reordering a field or collection — or changing any
record's Borsh shape — is a consensus change and takes a new version constant AND new domain
strings. There is no "compatible" evolution of a hash preimage.

## 3. What the root deliberately does not cover

- **Parameters** (β, windows, epoch length): per-network constants inside the atomic bundle,
  committed by the ruleset fingerprint (ADR-0042 Decision 11). Re-hashing them per block would
  say nothing the fingerprint has not already said.
- **Indices** (`deadlines`, `unresolved`, `open_courts_by_claim`): derivable caches. They are
  rebuilt on every load and cross-checked against primary data; hashing them would freeze an
  implementation detail into consensus.

## 4. The carriage, and the None-root rule

`PalwStateCarriageV2` (domain `misaka-palw/state-v2/carriage/v1`) is the Borsh snapshot of the
primary data that the pruning proof carries (ADR-0042 Decision 5), digested as
`H(len ‖ borsh(carriage))`. Loading rebuilds all indices and refuses a snapshot whose derivable
facts disagree with its primary data.

Self-consistency is necessary, not sufficient: a claim's `reserved` / `immature_contribution`
snapshots are the accounting basis by design, so a tamper that adjusts `pwu` and nothing
derivable from it is *coherent*. What catches it is the committed root. Therefore:
**`into_state(…, expected_root = None)` is legal only for a node's own trusted disk; a
peer-supplied carriage MUST be loaded against the root the chain committed.** PR-08's IBD wiring
inherits this rule as written.

## 5. The frontier, since the comparator orders by it first

`safe_frontier` advances to `(blue_score, block)` of the chain point being applied exactly when,
at the end of that application, **no unresolved claim exists** — and holds still otherwise. It
is monotone by construction and observed lazily at block boundaries. This is the first key of
`compare_palw_candidates_v1` (ADR-0042 Decision 9): a private fork can pile up attempts, but its
claims cannot mature, so from the fork point its frontier never moves again.

## 6. Number hygiene

This is ADR-0043; 0042 is the last committed. Ties with a concurrent 0043 resolve per ADR-0036
Decision 5 (this file's content stays, the later writer renumbers).
